//! 端到端集成测试：HookWriter → SQLite
//!
//! 验证意图：跨两个模块的事件流必须无丢失、无重复、字段保真。
//! 这是 FingerTip 数据采集链路的命脉——一旦这里有 bug，键盘指纹的真实性就崩塌。
//!
//! v0.3.1 重写：v0.1 EventBuffer + HookListener trait 链路已废弃，
//! 现链路是 HookWriter::send → bounded channel → 后台 writer 线程 batch flush → SQLite。
//! HookWriter 自身的 4 个 unit test（clone / flood / persist / no-panic）覆盖细节，
//! 此处仅测端到端：不丢、不重、字段保真。

use fingertip_lib::db::{event_repo::EventRepo, migrations::run_migrations};
use fingertip_lib::hook::{event::KeyEvent, writer::HookWriter};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// rusqlite::Connection 不是 Sync，必须用 Mutex 包装后才能在闭包间共享
type SharedConn = Arc<Mutex<Connection>>;

fn fresh_db() -> SharedConn {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    Arc::new(Mutex::new(conn))
}

#[test]
fn end_to_end_events_persist_to_sqlite() {
    // 验证意图：从 HookWriter::send 到 SQLite 的链路完整，事件不丢、字段不串
    let shared = fresh_db();
    let writer = HookWriter::spawn(shared.clone());

    // send 5 个事件（都打到同一 session，便于 list_by_session 验证）
    for i in 0..5u32 {
        writer.send(KeyEvent::now(i, "integ-session".into(), 0));
    }

    // 等 writer 线程消费 + flush
    std::thread::sleep(Duration::from_millis(300));

    // 验证 SQLite 持久化了 5 个
    let stored = EventRepo::new(&shared.lock().unwrap())
        .list_by_session("integ-session")
        .unwrap();
    assert_eq!(stored.len(), 5);

    // 验证 key_code 顺序保真（不丢、不乱）
    let codes: Vec<u32> = stored.iter().map(|e| e.key_code).collect();
    assert_eq!(codes, vec![0, 1, 2, 3, 4], "key_code 顺序必须严格保真");

    // 验证 writer 自身统计也一致
    assert_eq!(writer.received(), 5, "received 计数 = 5");
    assert_eq!(writer.written(), 5, "written 计数 = 5");
    assert_eq!(writer.dropped(), 0, "正常流量下不该有 drop");
}

#[test]
fn high_volume_persistence_under_5_seconds() {
    // 验证意图：1000 个事件批量灌入，writer 必须在合理时间内全部落库
    //   （类似 perf.rs 10w 事件 < 5s 的轻量级版本）
    let shared = fresh_db();
    let writer = HookWriter::spawn(shared.clone());

    for i in 0..1000u32 {
        writer.send(KeyEvent::now(65 + (i % 26) as u32, "bulk".into(), 0));
    }

    // 等 writer 消费
    std::thread::sleep(Duration::from_millis(500));

    // 1000 条应全部落库（capacity 4096 远大于 1000，无 drop）
    let count = EventRepo::new(&shared.lock().unwrap())
        .count()
        .unwrap();
    assert_eq!(count, 1000, "1000 events 全部落库");
    assert_eq!(writer.received(), 1000);
    assert_eq!(writer.dropped(), 0, "1000 << 4096 capacity，无 drop");
}
