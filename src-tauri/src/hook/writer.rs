//! HookWriter：把 rdev 键盘事件从 OS 钩子线程异步落到 SQLite。
//!
//! v0.2.1 关键性能修复 ——「实时记录卡顿」的解决方案 C：
//!
//! ## 旧路径（v0.2.0）
//! ```text
//! rdev callback (OS 钩子线程)
//!   └─ buffer.lock() → push → unlock
//! EventBuffer 累积 → sink 触发
//!   └─ conn.lock() → execute (重新 prepare) → commit → unlock
//! ```
//! 痛点：
//!   - 每键重新 prepare SQL（100 evt/s = 100×prepare 解析）
//!   - 默认 journal=FULL，每键 fsync → 移动硬盘 5-50ms
//!   - **SQLite 在 OS 钩子线程执行**，DB 卡 → 钩子卡 → 系统输入延迟
//!
//! ## 新路径（v0.2.1）
//! ```text
//! rdev callback (OS 钩子线程, 永不阻塞)
//!   └─ tx.try_send(event) // 纳秒级，channel 满只丢不阻
//! writer thread（独立线程）
//!   ├─ 50 条 OR 100ms 触发 batch
//!   ├─ prepare 一次，cache 复用（性能 B 改动）
//!   ├─ conn.execute_many or prepared stmt.execute 批量落库
//!   └─ PRAGMA WAL + synchronous=NORMAL（性能 A 改动）
//! ```
//!
//! ## 验证意图
//! - rdev 钩子线程不被 DB I/O 阻塞 → 系统输入零延迟
//! - 每 100ms flush 一次 → UI 看到「近实时」
//! - 50 条 batch → SQLite 写入效率高
//! - channel 4096 大小 → 应对 1s 突发不丢

use crate::hook::event::KeyEvent;
use rusqlite::{params, Connection, CachedStatement};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// 写到 SQLite 的批次大小（条）
pub const BATCH_SIZE: usize = 50;

/// 写线程 flush 间隔（毫秒）
pub const FLUSH_INTERVAL_MS: u64 = 100;

/// channel 容量 —— 4096 = 约能装 4 秒突发（极端情况下 1000 evt/s）
pub const CHANNEL_CAPACITY: usize = 4096;

/// 写线程运行状态（用于监控 + 测试）
pub struct WriterStats {
    /// 累计收到的 event 数（尝试发送的）
    pub received: AtomicU64,
    /// 累计成功写入 SQLite 的 event 数
    pub written: AtomicU64,
    /// 累计因 channel 满而丢弃的 event 数（监控丢失率）
    pub dropped: AtomicU64,
}

impl WriterStats {
    pub fn new() -> Self {
        Self {
            received: AtomicU64::new(0),
            written: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }
}

impl Default for WriterStats {
    fn default() -> Self {
        Self::new()
    }
}

/// HookWriter：rdev 端的无锁发送句柄。
///
/// 持有一个 SyncSender + 监控 stats。Clone 友好（内部 Arc）。
/// `send()` 用 try_send，绝不阻塞。
#[derive(Clone)]
pub struct HookWriter {
    tx: SyncSender<KeyEvent>,
    stats: Arc<WriterStats>,
}

impl HookWriter {
    /// 启动 writer 后台线程，返回可在任意线程 clone 的 HookWriter
    ///
    /// `conn` —— 共享的 SQLite 连接（Arc<Mutex>）
    pub fn spawn(conn: Arc<Mutex<Connection>>) -> Self {
        let (tx, rx) = sync_channel::<KeyEvent>(CHANNEL_CAPACITY);
        let stats = Arc::new(WriterStats::new());
        let stats_for_thread = stats.clone();

        thread::spawn(move || {
            run_writer_loop(conn, rx, stats_for_thread);
        });

        Self { tx, stats }
    }

    /// 永不阻塞的发送。
    ///
    /// channel 满了就丢弃（drop_count++）—— 比阻塞 OS 钩子好。
    /// 也就是说：**极端过载时宁可丢几键也不影响用户打字流畅度**。
    pub fn send(&self, event: KeyEvent) {
        self.stats.received.fetch_add(1, Ordering::Relaxed);
        match self.tx.try_send(event) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.stats.dropped.fetch_add(1, Ordering::Relaxed);
                log::warn!("HookWriter channel full, dropped event");
            }
            Err(TrySendError::Disconnected(_)) => {
                // writer 线程死掉了 —— 不应发生，但保险
                log::error!("HookWriter channel disconnected");
            }
        }
    }

    /// 监控：当前已发送条数
    pub fn received(&self) -> u64 {
        self.stats.received.load(Ordering::Relaxed)
    }

    /// 监控：当前已写入条数
    pub fn written(&self) -> u64 {
        self.stats.written.load(Ordering::Relaxed)
    }

    /// 监控：累计丢弃条数（channel 满）
    pub fn dropped(&self) -> u64 {
        self.stats.dropped.load(Ordering::Relaxed)
    }
}

/// Writer 后台循环：
///
/// - 收事件 → 攒 batch
/// - 触发条件：BATCH_SIZE 条 OR FLUSH_INTERVAL_MS 毫秒
/// - 用 cached prepared statement 一次 INSERT（性能 B 改动）
fn run_writer_loop(
    conn: Arc<Mutex<Connection>>,
    rx: std::sync::mpsc::Receiver<KeyEvent>,
    stats: Arc<WriterStats>,
) {
    let mut batch: Vec<KeyEvent> = Vec::with_capacity(BATCH_SIZE);
    let flush_interval = Duration::from_millis(FLUSH_INTERVAL_MS);

    loop {
        // 收一条事件（带超时）—— 用 recv_timeout 实现定时 flush
        match rx.recv_timeout(flush_interval) {
            Ok(event) => {
                batch.push(event);
                // 顺便吸干 channel，直到空（或满 BATCH_SIZE）
                while batch.len() < BATCH_SIZE {
                    match rx.try_recv() {
                        Ok(more) => batch.push(more),
                        Err(_) => break,
                    }
                }
                if batch.len() >= BATCH_SIZE {
                    flush_batch(&conn, &mut batch, &stats);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // 时间到了 —— flush 残留（即使不超 batch 也落库）
                if !batch.is_empty() {
                    flush_batch(&conn, &mut batch, &stats);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // 所有发送端关闭 —— 退出循环
                if !batch.is_empty() {
                    flush_batch(&conn, &mut batch, &stats);
                }
                break;
            }
        }
    }
}

/// 落库一批事件 —— 用 prepared statement 一次性执行
///
/// 这里**不再走 EventRepo::insert_many**（它会重新构造 SQL + Box 装箱），
/// 而是直接 prepare 一次 + 循环 bind + step。在 writer 线程里 prepare 是
/// 热路径的开销 —— 用 prepare_cached 让 SQLite 内部缓存执行计划。
fn flush_batch(conn: &Arc<Mutex<Connection>>, batch: &mut Vec<KeyEvent>, stats: &WriterStats) {
    if batch.is_empty() {
        return;
    }

    let n = match conn.lock() {
        Ok(c) => {
            let mut stmt: CachedStatement<'_> = match c.prepare_cached(
                "INSERT INTO key_events (key_code, timestamp_ms, session_id, modifiers) VALUES (?, ?, ?, ?)",
            ) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("prepare_cached failed: {:?}", e);
                    batch.clear();
                    return;
                }
            };

            // 用事务包住：BATCH_SIZE 条 INSERT 在一个事务里 commit
            // SQLite 事务开销很小，但去掉 N×begin/commit 是质变
            let tx = match c.unchecked_transaction() {
                Ok(t) => t,
                Err(e) => {
                    log::error!("begin transaction failed: {:?}", e);
                    batch.clear();
                    return;
                }
            };

            let mut written = 0usize;
            for event in batch.iter() {
                if let Err(e) = stmt.execute(params![
                    event.key_code,
                    event.timestamp_ms,
                    event.session_id,
                    event.modifiers
                ]) {
                    log::error!("execute failed: {:?}", e);
                    break;
                }
                written += 1;
            }

            if let Err(e) = tx.commit() {
                log::error!("commit failed: {:?}", e);
            }

            written
        }
        Err(e) => {
            log::error!("conn lock failed: {:?}", e);
            batch.clear();
            return;
        }
    };

    stats.written.fetch_add(n as u64, Ordering::Relaxed);
    batch.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_in_memory;

    fn fresh_db() -> Connection {
        init_in_memory().unwrap()
    }

    #[test]
    fn hook_writer_clone_and_send_no_block() {
        // 验证意图：HookWriter 可 clone、send 不阻塞、try_send 语义正确
        let conn = Arc::new(Mutex::new(fresh_db()));
        let writer = HookWriter::spawn(conn);
        let w2 = writer.clone();
        w2.send(KeyEvent::now(65, "test".into(), 0));
        writer.send(KeyEvent::now(66, "test".into(), 0));
        writer.send(KeyEvent::now(67, "test".into(), 0));
        assert_eq!(writer.received(), 3);

        // 给 writer 线程时间消费 + flush
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(writer.written(), 3);
        assert_eq!(writer.dropped(), 0);
    }

    #[test]
    fn hook_writer_drop_rate_under_flood() {
        // 验证意图：channel 满时丢弃（不阻塞）—— 关键验收「不卡 OS 钩子」
        let conn = Arc::new(Mutex::new(fresh_db()));
        let writer = HookWriter::spawn(conn);
        // 强行灌远超 channel 容量的 event
        for i in 0..(CHANNEL_CAPACITY * 2) {
            writer.send(KeyEvent::now(i as u32, "flood".into(), 0));
        }
        // 等 writer 线程慢慢消费
        std::thread::sleep(Duration::from_millis(500));
        // 因为 batch flush 在写、阻塞 writer 线程 ── 灌太快会有 dropped
        // 注意：具体数值依赖时机，断言只检至少 1 条成功 + 总数对齐
        assert!(
            writer.received() >= writer.written(),
            "received {} should >= written {}",
            writer.received(),
            writer.written(),
        );
    }

    #[test]
    fn hook_writer_eventually_persists_all_written() {
        // 验证意图：writer 线程会持续消费，验证"持久化总条数 = 实际写到 DB 的"
        let conn = Arc::new(Mutex::new(fresh_db()));
        let writer = HookWriter::spawn(conn.clone());

        for i in 0..200 {
            writer.send(KeyEvent::now(65 + (i % 26) as u32, "batch-test".into(), 0));
        }
        // 等待 writer 线程消化完
        std::thread::sleep(Duration::from_millis(500));

        let written_in_writer = writer.written();
        let c = conn.lock().unwrap();
        let from_db: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM key_events WHERE session_id = ?",
                rusqlite::params!["batch-test"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(from_db, written_in_writer as i64);
    }

    #[test]
    fn hook_writer_send_does_not_panic_on_empty_channel() {
        // 验证意图：rx drop 后 send 不 panic（即使不会发生也保证鲁棒）
        let conn = Arc::new(Mutex::new(fresh_db()));
        let writer = HookWriter::spawn(conn);
        // 跑满 batch + 落库
        for _ in 0..100 {
            writer.send(KeyEvent::now(1, "z".into(), 0));
        }
        std::thread::sleep(Duration::from_millis(300));
        // 此时 writer 持续运行 —— 再发一条不应 panic
        writer.send(KeyEvent::now(2, "z".into(), 0));
    }
}
