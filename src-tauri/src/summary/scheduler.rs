//! 每日聚合定时调度。
//!
//! 验证意图：每天 00:05 触发一次昨日聚合（设计文档 2.3）。
//! next_run_time 是纯函数：给定当前时间 → 返回下次 00:05。

use chrono::{Duration, NaiveDateTime};

pub fn next_run_time(now: NaiveDateTime) -> NaiveDateTime {
    let today_run = now.date().and_hms_opt(0, 5, 0).unwrap();
    if now < today_run {
        today_run
    } else {
        today_run + Duration::days(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_run_is_next_005_when_past_midnight() {
        // 验证意图：23:00 之后应返回次日 00:05（不会当天重复）
        let now = NaiveDateTime::parse_from_str("2026-07-16 23:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let next = next_run_time(now);
        assert_eq!(next.to_string(), "2026-07-17 00:05:00");
    }

    #[test]
    fn next_run_is_today_005_when_before_midnight_run() {
        // 验证意图：当天 00:05 之前 → 返回当天 00:05
        let now = NaiveDateTime::parse_from_str("2026-07-16 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let next = next_run_time(now);
        assert_eq!(next.to_string(), "2026-07-16 00:05:00");
    }

    #[test]
    fn next_run_skips_to_tomorrow_when_after_run() {
        // 验证意图：00:06 之后 → 跳过当天，等次日
        let now = NaiveDateTime::parse_from_str("2026-07-16 00:06:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let next = next_run_time(now);
        assert_eq!(next.to_string(), "2026-07-17 00:05:00");
    }

    #[test]
    fn next_run_at_exactly_005_is_tomorrow() {
        // 验证意图：00:05 整点 = 当天 run 已发生（避免重复触发），返回次日
        let now = NaiveDateTime::parse_from_str("2026-07-16 00:05:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let next = next_run_time(now);
        assert_eq!(next.to_string(), "2026-07-17 00:05:00");
    }
}