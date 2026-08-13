use serde::{Deserialize, Serialize};

/// 日聚合统计的数据契约。
///
/// 验证意图：daily_summary 表的内容结构，方便序列化与跨层传递。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStats {
    pub date: String,           // YYYY-MM-DD
    pub total_keys: usize,
    pub top_keys: Vec<(u32, usize)>,
    pub percentages: Vec<(u32, f64)>,
    pub pauses: usize,
    pub deletes: usize,
    pub repeats: usize,
    pub hourly: [usize; 24],
    // ====== v0.3.5 新增 5 字段 ======
    /// 密集度 dynsity：total_keys / active_hours（> 800 键/小时为快）
    pub intensity: f64,
    /// 平稳度 stabilit：变异系数 stddev/mean（<= 0.8 为平稳）
    pub steadiness: f64,
    /// 流畅度 fluency：pauses > 2s / total_intervals（< 10% 为流畅）
    pub fluency: f64,
    /// 活跃度 activity：active_hours_count（> 4 为活跃）
    pub activity_hours: i32,
    /// 3 类键汇总 JSON（KeyClassSummary 序列化）
    pub key_class_json: String,
    // ====== v0.3.6 新增 1 字段 ======
    /// 首活时间 first_active_ms：今日首次按键 UTC 毫秒（0 = 无事件）
    /// UI 端按 store.timezoneOffsetMinutes 偏移后显示 HH:mm
    pub first_active_ms: i64,
}

impl DailyStats {
    pub fn empty(date: String) -> Self {
        Self {
            date,
            total_keys: 0,
            top_keys: vec![],
            percentages: vec![],
            pauses: 0,
            deletes: 0,
            repeats: 0,
            hourly: [0; 24],
            // v0.3.5
            intensity: 0.0,
            steadiness: 0.0,
            fluency: 0.0,
            activity_hours: 0,
            key_class_json: "{}".into(),
            // v0.3.6
            first_active_ms: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_stats_round_trip_with_new_fields() {
        let stats = DailyStats {
            date: "2026-07-29".into(),
            total_keys: 100,
            top_keys: vec![(65, 50), (66, 30)],
            percentages: vec![(65, 50.0), (66, 30.0)],
            pauses: 5,
            deletes: 3,
            repeats: 10,
            hourly: [10; 24],
            intensity: 20.0,
            steadiness: 0.5,
            fluency: 0.05,
            activity_hours: 5,
            key_class_json: r#"{"game_keys":10,"text_keys":80,"modifier_keys":10}"#.into(),
            first_active_ms: 0,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: DailyStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back.intensity, 20.0);
        assert_eq!(back.steadiness, 0.5);
        assert_eq!(back.fluency, 0.05);
        assert_eq!(back.activity_hours, 5);
        assert!(back.key_class_json.contains("game_keys"));
    }

    #[test]
    fn daily_stats_empty_has_zero_new_fields() {
        let s = DailyStats::empty("2026-07-29".into());
        assert_eq!(s.intensity, 0.0);
        assert_eq!(s.steadiness, 0.0);
        assert_eq!(s.fluency, 0.0);
        assert_eq!(s.activity_hours, 0);
        assert_eq!(s.key_class_json, "{}");
    }

    #[test]
    fn daily_stats_round_trip_includes_first_active_ms() {
        let s = DailyStats {
            date: "2026-07-29".into(),
            total_keys: 10,
            top_keys: vec![],
            percentages: vec![],
            pauses: 0,
            deletes: 0,
            repeats: 0,
            hourly: [0; 24],
            intensity: 0.0,
            steadiness: 0.0,
            fluency: 0.0,
            activity_hours: 0,
            key_class_json: "{}".into(),
            first_active_ms: 1_753_401_600_123,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: DailyStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back.first_active_ms, 1_753_401_600_123);
    }
}