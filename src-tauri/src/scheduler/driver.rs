//! 每日聚合调度驱动
//!
//! 验证意图：每天 00:05 触发昨日事件 → Aggregator → SummaryRepo.upsert。
//! 拆成两个函数：
//! 1. `summarize_date(&conn, date)` —— 纯函数，可单测
//! 2. `spawn_loop(conn)` —— tokio 后台循环
use chrono::TimeZone;

use crate::db::summary_repo::SummaryRepo;
use crate::summary::aggregator::Aggregator;
use crate::summary::theme::extract_theme_word;
use chrono::NaiveDate;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 把指定日期（YYYY-MM-DD）的 key_events 聚合后写入 daily_summary。
///
/// 验证意图：调度的核心动作可独立测试，不依赖 tokio 时钟。
///
/// v0.3.1: **不再写入 mood_word** —— mood 由 `set_mood` / `SummaryRepo::upsert_mood`
/// 单点管理。scheduler 60s tick 用 `upsert_stats`（不覆盖 mood_word），
/// 否则用户提交心情后 60s 内会被重置为 NULL（v0.3.0 缺陷）。
///
/// 返回 Ok(Some(date)) 表示已写入（当日有事件）；
/// 返回 Ok(None) 表示该日无事件（不写 daily_summary）。
pub fn summarize_date(
    conn: &Connection,
    date: &str,
) -> anyhow::Result<Option<String>> {
    // 1. 读当日事件
    //   从整张 key_events 读所有，然后过滤 timestamp 落在 date 当天
    //   （Phase 后续：扩展 EventRepo 接受日期 filter）
    let all = conn.prepare(
        "SELECT key_code, timestamp_ms, session_id, modifiers FROM key_events WHERE timestamp_ms >= ? AND timestamp_ms < ?",
    )?;
    let (start_ms, end_ms) = date_range_ms(date)?;
    let mut stmt = all;
    let events_iter = stmt.query_map(
        rusqlite::params![start_ms, end_ms],
        |row| {
            Ok(crate::hook::event::KeyEvent {
                key_code: row.get(0)?,
                timestamp_ms: row.get(1)?,
                session_id: row.get(2)?,
                modifiers: row.get(3)?,
            })
        },
    )?;
    let events: Vec<_> = events_iter.collect::<Result<Vec<_>, _>>()?;

    if events.is_empty() {
        return Ok(None);
    }

    // 2. 聚合
    let stats = Aggregator::aggregate(date.to_string(), &events);

    // 3. 主题词
    let counts = Aggregator::count_by_key(&events);
    let theme_word = extract_theme_word(&counts);

    // 4. 写 daily_summary（不动 mood_word —— 已被 set_mood 接管）
    SummaryRepo::new(conn).upsert_stats(&stats, &theme_word)?;

    Ok(Some(date.to_string()))
}

/// 把某个日期（YYYY-MM-DD）转成 (start_ms, end_ms) = 该日 00:00 / 次日 00:00 的 UTC 毫秒。
pub fn date_range_ms(date: &str) -> anyhow::Result<(i64, i64)> {
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
    // 把 date 当**本地日历日**解读（用户视角的"今天"）
    //   —— 跨时区不会丢失数据（UTC+8 凌晨按的键仍归在本地今天）
    let ndt = d.and_hms_opt(0, 0, 0).ok_or_else(|| anyhow::anyhow!("invalid date"))?;
    let start_local = chrono::Local
        .from_local_datetime(&ndt)
        .earliest()
        .ok_or_else(|| anyhow::anyhow!("invalid local datetime"))?;
    let next_d = d.succ_opt().ok_or_else(|| anyhow::anyhow!("invalid date"))?;
    let end_ndt = next_d.and_hms_opt(0, 0, 0).ok_or_else(|| anyhow::anyhow!("invalid date"))?;
    let end_local = chrono::Local
        .from_local_datetime(&end_ndt)
        .earliest()
        .ok_or_else(|| anyhow::anyhow!("invalid local datetime"))?;
    // DateTime<Local>::timestamp_millis() 返回 UTC 毫秒（与 DateTime<Utc> 数值一致）
    Ok((start_local.timestamp_millis(), end_local.timestamp_millis()))
}

/// 启动 tokio 后台循环：每分钟 tick 一次，把"今日"跑聚合（增量）。
///
/// 改用"今日"而非"昨日"：让用户今天按完键，今天就能在 UI 看到 summary。
/// 代价：每 60 秒会重新聚合今日（成本廉价，数据量小）。
///
/// v0.3.1: 不再需要 mood_source 参数 —— mood_word 由 `set_mood` 单独管，
/// scheduler 用 `upsert_stats` 不会动 mood_word。参数保留兼容旧调用方（已 deprecated）。
pub fn spawn_loop(
    conn: Arc<Mutex<Connection>>,
    _mood_source: Arc<Mutex<Option<String>>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;

            let today: String = chrono::Local::now()
                .date_naive()
                .format("%Y-%m-%d")
                .to_string();
            let res = match conn.lock() {
                Ok(c) => summarize_date(&c, &today),
                Err(e) => Err(anyhow::anyhow!("conn lock poisoned: {}", e)),
            };
            if let Err(e) = res {
                log::error!("summarize {} failed: {:?}", today, e);
            }
        }
    })
}

/// 公开 summary_date（让前端的 trigger_run_summary_now Command 也可复用）
pub use self::summarize_date as run_summarize_for_date;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::event_repo::EventRepo;
    use crate::db::migrations::run_migrations;
    use crate::hook::event::KeyEvent;
    use chrono::Utc;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn seed_events_for_date(conn: &Connection, date: &str, count: usize) {
        let (start_ms, _) = date_range_ms(date).unwrap();
        let event_repo = EventRepo::new(conn);
        for i in 0..count {
            let mut ev = KeyEvent::now((i % 26 + 65) as u32, "s".into(), 0);
            // 把时间戳设到 date 内（均匀分布在 start_ms + i*1000ms）
            ev.timestamp_ms = start_ms + (i as i64) * 1000;
            event_repo.insert(&ev).unwrap();
        }
    }

    #[test]
    fn summarize_empty_day_returns_none() {
        // 验证意图：没事件那天不写 daily_summary
        let conn = fresh_db();
        let res = summarize_date(&conn, "2026-07-16").unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn summarize_picks_events_within_target_date() {
        // 验证意图：只聚合 date 当天的事件，跨日事件被忽略
        let conn = fresh_db();
        seed_events_for_date(&conn, "2026-07-16", 100);
        // 额外插一条 2026-07-15 的事件（不应该被聚合到 07-16）
        let mut other = KeyEvent::now(65, "other".into(), 0);
        other.timestamp_ms = date_range_ms("2026-07-15").unwrap().0 + 1000;
        EventRepo::new(&conn).insert(&other).unwrap();

        let res = summarize_date(&conn, "2026-07-16").unwrap();
        assert_eq!(res.as_deref(), Some("2026-07-16"));
        let read = SummaryRepo::new(&conn).read_by_date("2026-07-16").unwrap().unwrap();
        assert_eq!(read.total_keys, 100, "只聚合目标日期的 100 条");
    }

    #[test]
    fn summarize_writes_theme_word_but_preserves_mood() {
        // v0.3.1: mood 由 set_mood 单点管 —— summarize 不写 mood
        // 顺序：先 set_mood("开心") → 再 summarize_date → mood 必须仍为"开心"
        let conn = fresh_db();
        SummaryRepo::new(&conn).upsert_mood("2026-07-16", "开心").unwrap();
        seed_events_for_date(&conn, "2026-07-16", 50);

        summarize_date(&conn, "2026-07-16").unwrap();
        let read = SummaryRepo::new(&conn).read_by_date("2026-07-16").unwrap().unwrap();
        assert!(!read.theme_word.is_empty());
        assert_eq!(read.mood_word.as_deref(), Some("开心"),
            "summarize_date 跑后 mood_word 必须保留 —— v0.3.0 bug 修复");
    }

    #[test]
    fn summarize_idempotent_same_day_stats() {
        // v0.3.1: 同一天多次跑 summarize，stats / theme_word 应被覆盖（OR REPLACE），
        // 但 mood_word **不**被覆盖（mood 单点管）。
        let conn = fresh_db();
        seed_events_for_date(&conn, "2026-07-16", 10);
        SummaryRepo::new(&conn).upsert_mood("2026-07-16", "calm").unwrap();

        summarize_date(&conn, "2026-07-16").unwrap();
        // 再加更多 events
        seed_events_for_date(&conn, "2026-07-16", 40);
        summarize_date(&conn, "2026-07-16").unwrap();

        let read = SummaryRepo::new(&conn).read_by_date("2026-07-16").unwrap().unwrap();
        assert_eq!(read.total_keys, 50, "第二次跑应读到 50 events");
        assert_eq!(read.mood_word.as_deref(), Some("calm"),
            "mood 永远保留（set_mood 写入后不被覆盖）");
    }

    #[test]
    fn summarize_does_not_clear_mood_on_repeat() {
        // v0.3.1 P0 #2 核心回归：scheduler 60s tick 不会把用户 mood 改回 None
        // 顺序：
        //   1. 用户 set_mood("happy")
        //   2. scheduler 跑 summarize_date（3 次模拟 3 个 tick）
        //   3. mood 必须仍是 "happy"
        let conn = fresh_db();
        seed_events_for_date(&conn, "2026-07-16", 30);
        SummaryRepo::new(&conn).upsert_mood("2026-07-16", "happy").unwrap();

        for _ in 0..3 {
            summarize_date(&conn, "2026-07-16").unwrap();
        }

        let read = SummaryRepo::new(&conn).read_by_date("2026-07-16").unwrap().unwrap();
        assert_eq!(read.mood_word.as_deref(), Some("happy"),
            "3 次 scheduler tick 后 mood_word 必须仍是 happy（v0.3.0 bug：被覆盖为 NULL）");
    }

    #[test]
    fn date_range_ms_valid_for_today() {
        // 验证意图：今天日期范围合理（start < end）
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let (s, e) = date_range_ms(&today).unwrap();
        assert!(s < e, "start < end");
        assert_eq!(e - s, 86_400_000, "一天 = 86,400,000 ms");
    }
}
