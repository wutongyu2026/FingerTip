use crate::hook::event::KeyEvent;
use rusqlite::{params, Connection};

/// KeyEvent 仓储：负责 SQLite 读写。
///
/// 验证意图：业务逻辑与 SQL 解耦，方便测试（用 in-memory DB）与未来换数据源。
pub struct EventRepo<'a> {
    conn: &'a Connection,
}

impl<'a> EventRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 插入一个事件
    pub fn insert(&self, event: &KeyEvent) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO key_events (key_code, timestamp_ms, session_id, modifiers) VALUES (?, ?, ?, ?)",
            params![event.key_code, event.timestamp_ms, event.session_id, event.modifiers],
        )?;
        Ok(())
    }

    /// 按 session_id 查询所有事件（用于集成测试与回放）
    pub fn list_by_session(&self, session_id: &str) -> anyhow::Result<Vec<KeyEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT key_code, timestamp_ms, session_id, modifiers FROM key_events WHERE session_id = ? ORDER BY id",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(KeyEvent {
                key_code: row.get(0)?,
                timestamp_ms: row.get(1)?,
                session_id: row.get(2)?,
                modifiers: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 删除早于 cutoff_ms 的事件（30 天过期清理）
    pub fn delete_older_than(&self, cutoff_ms: i64) -> anyhow::Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM key_events WHERE timestamp_ms < ?",
            params![cutoff_ms],
        )?;
        Ok(n)
    }

    /// 当前事件总数（用于监控与测试）
    pub fn count(&self) -> anyhow::Result<i64> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM key_events", [], |row| row.get(0))?;
        Ok(n)
    }

    /// 批量插入多条事件（单事务 + 单 SQL）。
    ///
    /// 验证意图：在 USB 移动硬盘上，N 次单 INSERT ≈ N×5-50ms = 卡顿。
    /// BATCH INSERT ≈ 单次 IO 写入 ≈ 几乎无延迟。
    /// 这是修复「100wpm 输入卡顿」的核心优化。
    pub fn insert_many(&self, events: &[KeyEvent]) -> anyhow::Result<usize> {
        if events.is_empty() {
            return Ok(0);
        }
        let mut sql = String::from(
            "INSERT INTO key_events (key_code, timestamp_ms, session_id, modifiers) VALUES ",
        );
        for i in 0..events.len() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str("(?, ?, ?, ?)");
        }
        // 用 ToSql trait 对象数组让 rusqlite 接受任意类型混合
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(events.len() * 4);
        for e in events {
            params.push(Box::new(e.key_code as i64));
            params.push(Box::new(e.timestamp_ms));
            params.push(Box::new(e.session_id.clone()));
            params.push(Box::new(e.modifiers as i64));
        }
        let params_iter = rusqlite::params_from_iter(params.iter().map(|b| b.as_ref()));
        let n = self.conn.execute(&sql, params_iter)?;
        Ok(n)
    }

    /// 按日期范围查事件：`date` 是 "YYYY-MM-DD"，按 UTC 0:00 起、24h 止半开区间筛选。
    ///
    /// 验证意图：scheduler 跑聚合时需要按日期拉 events；
    /// Stage 3 真生成时也可能要回放当日节奏。
    /// 与 `tz_today_range_ms` 不同：本方法严格按 UTC 切分（与 key_events 存储
    /// 的 timestamp_ms 一致），适合跨时区回放与回测。
    pub fn list_by_date(&self, date: &str) -> anyhow::Result<Vec<KeyEvent>> {
        let (start, end) = parse_date_range_ms(date);
        self.list_by_timerange(start, end)
    }

    /// v0.6: 48 小时窗口查询（昨天 00:00 → 明天 00:00）。
    /// 覆盖跨零点通宵场景：昨天傍晚到今日凌晨的连续数据不会因零点截断而丢失。
    pub fn list_by_date_48h(&self, date: &str) -> anyhow::Result<Vec<KeyEvent>> {
        let (start, end) = parse_date_range_ms_48h(date);
        self.list_by_timerange(start, end)
    }

    /// v0.7: 自定义时间窗口（前端 SubmitMood datetime-local → epoch ms）。
    /// 透传到 SQL，让用户在生成作品时挑想用的活动时段。
    pub fn list_by_timerange(&self, start: i64, end: i64) -> anyhow::Result<Vec<KeyEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT key_code, timestamp_ms, session_id, modifiers FROM key_events
             WHERE timestamp_ms >= ?1 AND timestamp_ms < ?2
             ORDER BY timestamp_ms ASC",
        )?;
        let rows = stmt.query_map(params![start, end], |row| {
            Ok(KeyEvent {
                key_code: row.get(0)?,
                timestamp_ms: row.get(1)?,
                session_id: row.get(2)?,
                modifiers: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

/// 简单 helper（仅本模块用）：把 "YYYY-MM-DD" 转成 [start_ms, end_ms) UTC 半开区间。
///
/// 验证意图：避免在多个方法里重复解析逻辑；与 `commands::tz_today_range_ms`
/// 的"本地时区偏移"语义不同 —— 本函数固定 UTC 切分，因为 key_events 的
/// timestamp_ms 本身就是 UTC ms epoch。
fn parse_date_range_ms(date: &str) -> (i64, i64) {
    parse_date_range_ms_impl(date, 0)
}

/// v0.6: 48h 窗口（昨天 00:00 UTC → 明天 00:00 UTC），覆盖跨零点通宵场景。
fn parse_date_range_ms_48h(date: &str) -> (i64, i64) {
    parse_date_range_ms_impl(date, 1)
}

fn parse_date_range_ms_impl(date: &str, days_before: i32) -> (i64, i64) {
    use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").expect("valid YYYY-MM-DD");
    let start_d = d - Duration::days(days_before as i64);
    let start_dt = NaiveDateTime::new(start_d, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let end_dt = NaiveDateTime::new(d, NaiveTime::from_hms_opt(0, 0, 0).unwrap()) + Duration::days(1);
    (
        start_dt.and_utc().timestamp_millis(),
        end_dt.and_utc().timestamp_millis(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn insert_and_count_round_trip() {
        // 验证意图：插入 3 条事件后能完整读回（不丢字段、顺序正确）
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        repo.insert(&KeyEvent::now(65, "s1".into(), 0)).unwrap();
        repo.insert(&KeyEvent::now(66, "s1".into(), 0)).unwrap();
        repo.insert(&KeyEvent::now(67, "s1".into(), 0)).unwrap();
        assert_eq!(repo.count().unwrap(), 3);
        let all = repo.list_by_session("s1").unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].key_code, 65);
        assert_eq!(all[1].key_code, 66);
        assert_eq!(all[2].key_code, 67);
    }

    #[test]
    fn delete_older_than_prunes_correctly() {
        // 验证意图：30 天清理逻辑只删过期的、不删新的
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        let old = KeyEvent {
            key_code: 1,
            timestamp_ms: 1,
            session_id: "s".into(),
            modifiers: 0,
        };
        let recent = KeyEvent::now(2, "s".into(), 0);
        repo.insert(&old).unwrap();
        repo.insert(&recent).unwrap();
        let pruned = repo
            .delete_older_than(chrono::Utc::now().timestamp_millis() - 1000)
            .unwrap();
        assert_eq!(pruned, 1);
        assert_eq!(repo.list_by_session("s").unwrap().len(), 1);
    }

    #[test]
    fn list_by_session_isolates_sessions() {
        // 验证意图：session 隔离（避免多 session 串数据）
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        repo.insert(&KeyEvent::now(1, "session-A".into(), 0)).unwrap();
        repo.insert(&KeyEvent::now(2, "session-B".into(), 0)).unwrap();
        repo.insert(&KeyEvent::now(3, "session-A".into(), 0)).unwrap();
        assert_eq!(repo.list_by_session("session-A").unwrap().len(), 2);
        assert_eq!(repo.list_by_session("session-B").unwrap().len(), 1);
    }

    #[test]
    fn insert_many_round_trip() {
        // 验证意图：批量插入 N 条后能完整读出（字段不丢）
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        let events: Vec<KeyEvent> = (0..100)
            .map(|i| KeyEvent::now(65 + (i % 26) as u32, "batch".into(), 0))
            .collect();
        let n = repo.insert_many(&events).unwrap();
        assert_eq!(n, 100);
        assert_eq!(repo.list_by_session("batch").unwrap().len(), 100);
    }

    #[test]
    fn insert_many_empty_returns_zero() {
        // 验证意图：空批量不崩（border case：time flush 时 batch 可能空）
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        assert_eq!(repo.insert_many(&[]).unwrap(), 0);
    }

    #[test]
    fn insert_many_preserves_field_values() {
        // 验证意图：批量插入后字段类型/值正确（i32/u32 不丢精度）
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        let events = vec![
            KeyEvent { key_code: 65, timestamp_ms: 1700000000000, session_id: "s".into(), modifiers: 0 },
            KeyEvent { key_code: 32, timestamp_ms: 1700000001000, session_id: "s".into(), modifiers: 1 },
        ];
        repo.insert_many(&events).unwrap();
        let read = repo.list_by_session("s").unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].key_code, 65);
        assert_eq!(read[1].key_code, 32);
    }

    // ---- Stage 2 Task 2.1: list_by_date ----
    // 验证意图：scheduler 要按本地日期范围取事件做聚合 / Stage 3 真生成时回放当日
    // 节奏。`date` 是 "YYYY-MM-DD"，按 UTC 0:00 起、24h 止半开区间筛选。

    #[test]
    fn list_by_date_returns_events_in_range() {
        // 验证意图：10 条同一天的事件全部返回，顺序按 timestamp_ms ASC
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        // 2025-07-25 00:00:00 UTC = 1753401600000ms
        let events: Vec<KeyEvent> = (0..10)
            .map(|i| KeyEvent {
                key_code: 65 + i,
                timestamp_ms: 1_753_401_600_000_i64 + i as i64 * 1000,
                session_id: "t".into(),
                modifiers: 0,
            })
            .collect();
        repo.insert_many(&events).unwrap();

        let listed = repo.list_by_date("2025-07-25").unwrap();
        assert_eq!(listed.len(), 10);
        assert_eq!(listed[0].key_code, 65);
        assert_eq!(listed[9].key_code, 74);
        // 顺序校验
        for w in listed.windows(2) {
            assert!(w[0].timestamp_ms <= w[1].timestamp_ms);
        }
    }

    #[test]
    fn list_by_date_excludes_other_days() {
        // 验证意图：范围 [00:00, 24:00) 半开 — 前一天末尾与后一天开头都应被排除
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        // 2025-07-25 当天 3 条
        let today: Vec<KeyEvent> = (0..3)
            .map(|i| KeyEvent {
                key_code: 65 + i,
                timestamp_ms: 1_753_401_600_000_i64 + i as i64 * 1000,
                session_id: "t".into(),
                modifiers: 0,
            })
            .collect();
        // 2025-07-24 当天 2 条（一天前）
        let yesterday: Vec<KeyEvent> = (0..2)
            .map(|i| KeyEvent {
                key_code: 80 + i,
                timestamp_ms: 1_753_315_200_000_i64 + i as i64 * 1000, // 2025-07-24 UTC 0:00 起
                session_id: "t".into(),
                modifiers: 0,
            })
            .collect();
        repo.insert_many(&today).unwrap();
        repo.insert_many(&yesterday).unwrap();

        let listed = repo.list_by_date("2025-07-25").unwrap();
        assert_eq!(listed.len(), 3, "应只返回当天 3 条，不含前一天的 2 条");
        assert!(listed.iter().all(|e| e.key_code >= 65 && e.key_code < 70));
    }

    /// v0.8: 48h 窗口查询（昨天 00:00 → 明天 00:00）—— 覆盖跨零点通宵场景。
    ///
    /// 验证意图：凌晨工作（昨天 23:00 + 今天 00:30）不应因零点截断而丢失；
    /// 同时窗口外的更早数据（前天）不被捞回。
    #[test]
    fn list_by_date_48h_includes_prev_and_next_day_boundary() {
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        // 时间锚点：2025-07-25 UTC 0:00 = 1_753_401_600_000
        let mid = 1_753_401_600_000_i64;
        // 前天 23:00（48h 窗口之外 → 应排除）: mid - 2天 + 23h
        let day_before_yesterday = mid - 2 * 86_400_000 + 23 * 3_600_000;
        // 昨天 23:00（48h 窗口内 → 应包含）
        let yesterday_late = mid - 3_600_000;
        // 今天 00:30（48h 窗口内 → 应包含）
        let today_early = mid + 30 * 60_000;
        let events = vec![
            KeyEvent { key_code: 1, timestamp_ms: day_before_yesterday, session_id: "s".into(), modifiers: 0 },
            KeyEvent { key_code: 2, timestamp_ms: yesterday_late, session_id: "s".into(), modifiers: 0 },
            KeyEvent { key_code: 3, timestamp_ms: today_early, session_id: "s".into(), modifiers: 0 },
        ];
        repo.insert_many(&events).unwrap();

        let listed = repo.list_by_date_48h("2025-07-25").unwrap();
        assert_eq!(listed.len(), 2, "48h 窗口应含昨天晚 + 今天早，不含前天");
        assert!(listed.iter().all(|e| e.key_code == 2 || e.key_code == 3));
    }

    /// v0.8: 自定义时间窗口查询（前端 datetime-local → epoch ms）透传 SQL。
    ///
    /// 验证意图：用户选 09:00-11:00 → 只返回该区间事件。
    #[test]
    fn list_by_timerange_returns_only_slice() {
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        let base = 1_753_401_600_000_i64;
        let events: Vec<KeyEvent> = (0..5)
            .map(|i| KeyEvent {
                key_code: 70 + i,
                timestamp_ms: base + i as i64 * 3_600_000, // 0,1,2,3,4 点
                session_id: "s".into(),
                modifiers: 0,
            })
            .collect();
        repo.insert_many(&events).unwrap();

        // 窗口 [2:00, 4:00)
        let listed = repo.list_by_timerange(base + 2 * 3_600_000, base + 4 * 3_600_000).unwrap();
        assert_eq!(listed.len(), 2, "应只返回 2 点和 3 点两条");
        assert!(listed.iter().all(|e| (2..=3).contains(&((e.timestamp_ms - base) / 3_600_000))));
    }
}