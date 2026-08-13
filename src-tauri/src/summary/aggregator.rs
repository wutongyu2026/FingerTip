//! 按键事件聚合器。
//!
//! 验证意图：原始事件 → 日统计的纯函数映射。
//! - 5 维数据：次数、占比、时段、停顿、删除/重复
//! - 输入：`&[KeyEvent]`
//! - 输出：`DailyStats`

use crate::hook::event::KeyEvent;
use crate::summary::key_class::KeyClassSummary;
use crate::summary::stats::DailyStats;
use chrono::Timelike;
#[cfg(test)]
use chrono::TimeZone;
use std::collections::HashMap;

pub struct Aggregator;

/// v0.6: 6 字段特殊键计数（编排器 prompt 用）。
///
/// 字段命名对齐同学 OrchestrationContext 6 字段，serde 反序列化给
/// `OrchestrationContext::from(events)` 喂数据。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpecialKeyCounts {
    pub backspace_count: usize,
    pub delete_count: usize,
    pub enter_count: usize,
    pub space_count: usize,
    pub wasd_count: usize,
    pub total_events: usize,
}

impl Aggregator {
    /// v0.3.5: 4 个核心行为指标（密集度 / 平稳度 / 流畅度 / 活跃度）
    ///
    /// 输入：events（fluency 用），hourly（已算好的 24 桶）
    /// 输出：(intensity, steadiness, fluency, activity_hours)
    ///
    /// 边界：所有除零返回 0.0
    pub fn compute_metrics(events: &[KeyEvent], hourly: &[usize; 24]) -> (f64, f64, f64, i32) {
        let active_hours = hourly.iter().filter(|&&c| c > 0).count();
        let total_keys = events.len();

        // 密集度 = total_keys / active_hours
        let intensity = if active_hours == 0 {
            0.0
        } else {
            total_keys as f64 / active_hours as f64
        };

        // 平稳度 = stddev / mean（变异系数）
        let hourly_f: Vec<f64> = hourly.iter().map(|&c| c as f64).collect();
        let mean = hourly_f.iter().sum::<f64>() / 24.0;
        let variance = hourly_f.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / 24.0;
        let stddev = variance.sqrt();
        let steadiness = if mean == 0.0 { 0.0 } else { stddev / mean };

        // 流畅度 = pauses > 2s / total_intervals
        let total_intervals = if events.len() < 2 { 0 } else { events.len() - 1 };
        let pauses_over_2s = if events.len() < 2 {
            0
        } else {
            events.windows(2)
                .filter(|w| w[1].timestamp_ms - w[0].timestamp_ms > 2000)
                .count()
        };
        let fluency = if total_intervals == 0 {
            0.0
        } else {
            pauses_over_2s as f64 / total_intervals as f64
        };

        (intensity, steadiness, fluency, active_hours as i32)
    }

    /// v0.3.5: 把按键按"游戏键 / 文本键 / 功能键"3 类汇总
    ///
    /// 优先级：game_keys (WASD + Space + Enter) > modifier_keys (Backspace/Delete/Shift/Ctrl/Alt/Meta) > text_keys (其余 ASCII 字母数字)
    /// 其它键（F1-F12、方向键等）忽略
    pub fn classify_keys(events: &[KeyEvent]) -> KeyClassSummary {
        let mut s = KeyClassSummary::default();
        for e in events {
            let code = e.key_code;
            // 优先级 1：game_keys
            if matches!(code, 87 | 65 | 83 | 68 | 32 | 13) {
                s.game_keys += 1;
                continue;
            }
            // 优先级 2：modifier_keys
            if matches!(code, 8 | 46 | 16 | 17 | 18 | 91) {
                s.modifier_keys += 1;
                continue;
            }
            // 优先级 3：text_keys（A-Z 扣 WASD + 0-9）
            if (48..=57).contains(&code) || matches!(code, 66 | 67 | 69 | 70 | 71..=82 | 84..=86 | 88..=90) {
                s.text_keys += 1;
            }
            // 其它键忽略
        }
        s
    }

    /// 按 key_code 计数
    pub fn count_by_key(events: &[KeyEvent]) -> HashMap<u32, usize> {
        let mut map = HashMap::new();
        for e in events {
            *map.entry(e.key_code).or_insert(0) += 1;
        }
        map
    }

    /// 计算每个键占总按键的百分比（保留两位小数）
    pub fn percentages(counts: &HashMap<u32, usize>) -> HashMap<u32, f64> {
        let total: usize = counts.values().sum();
        if total == 0 {
            return HashMap::new();
        }
        counts
            .iter()
            .map(|(k, v)| (*k, (*v as f64 / total as f64) * 100.0))
            .collect()
    }

    /// Top N 键（按次数降序）
    pub fn top_n(counts: &HashMap<u32, usize>, n: usize) -> Vec<(u32, usize)> {
        let mut sorted: Vec<(u32, usize)> = counts.iter().map(|(k, v)| (*k, *v)).collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.into_iter().take(n).collect()
    }

    /// 时段分布：按 timestamp_ms 所在小时分桶到 [0..24]
    pub fn hourly_buckets(events: &[KeyEvent]) -> [usize; 24] {
        let mut buckets = [0usize; 24];
        for e in events {
            if let Some(dt) = chrono::DateTime::from_timestamp_millis(e.timestamp_ms) {
                let hour = dt.hour() as usize;
                if hour < 24 {
                    buckets[hour] += 1;
                }
            }
        }
        buckets
    }

    /// 统计 meta 数据：(停顿数, 删除数, 重复数)
    /// - 停顿：相邻事件间隔 > threshold_ms
    /// - 删除：key_code == 8 (Backspace) 或 46 (Delete)
    /// - 重复：相邻事件 key_code 相同
    pub fn count_meta(events: &[KeyEvent], pause_threshold_ms: i64) -> (usize, usize, usize) {
        let mut pauses = 0;
        let mut deletes = 0;
        let mut repeats = 0;
        let mut last: Option<u32> = None;
        for w in events.windows(2) {
            if w[1].timestamp_ms - w[0].timestamp_ms > pause_threshold_ms {
                pauses += 1;
            }
            if w[0].key_code == w[1].key_code {
                repeats += 1;
            }
        }
        for e in events {
            if e.key_code == 8 || e.key_code == 46 {
                deletes += 1;
            }
            let _ = last.take();
            last = Some(e.key_code);
        }
        (pauses, deletes, repeats)
    }

    /// v0.6: 6 字段特殊键计数（编排器 prompt 用，给 LLM 看主题词触发规则）。
    ///
    /// VK codes (Windows 标准):
    ///   Backspace = 8, Delete = 46, Enter = 13, Space = 32, W=87 A=65 S=83 D=68
    pub fn count_special_keys(events: &[KeyEvent]) -> SpecialKeyCounts {
        let mut c = SpecialKeyCounts::default();
        for e in events {
            match e.key_code {
                8 => c.backspace_count += 1,
                46 => c.delete_count += 1,
                13 => c.enter_count += 1,
                32 => c.space_count += 1,
                87 | 65 | 83 | 68 => c.wasd_count += 1,
                _ => {}
            }
        }
        c.total_events = events.len();
        c
    }

    /// 一站式聚合：输入事件列表 → DailyStats
    pub fn aggregate(date: String, events: &[KeyEvent]) -> DailyStats {
        let counts = Aggregator::count_by_key(events);
        let pcts = Aggregator::percentages(&counts);
        let top = Aggregator::top_n(&counts, 5);
        let hourly = Aggregator::hourly_buckets(events);
        let (pauses, deletes, repeats) = Aggregator::count_meta(events, 2000);

        // v0.3.6: 首活时间（events 为空时返 0 sentinel）
        let first_active_ms = events.iter().map(|e| e.timestamp_ms).min().unwrap_or(0);

        // v0.3.5: 4 指标 + 3 分类
        let (intensity, steadiness, fluency, activity_hours) =
            Aggregator::compute_metrics(events, &hourly);
        let key_class = Aggregator::classify_keys(events);
        let key_class_json = serde_json::to_string(&key_class).unwrap_or_else(|_| "{}".into());

        DailyStats {
            date,
            total_keys: events.len(),
            top_keys: top,
            percentages: pcts.into_iter().collect(),
            pauses,
            deletes,
            repeats,
            hourly,
            intensity,
            steadiness,
            fluency,
            activity_hours,
            key_class_json,
            first_active_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: u32) -> KeyEvent {
        KeyEvent::now(code, "s".into(), 0)
    }

    #[test]
    fn counts_keys_correctly() {
        // 验证意图：按键计数准确反映真实输入
        let events = vec![ev(65), ev(65), ev(66), ev(65), ev(67)];
        let stats = Aggregator::count_by_key(&events);
        assert_eq!(stats.get(&65), Some(&3));
        assert_eq!(stats.get(&66), Some(&1));
        assert_eq!(stats.get(&67), Some(&1));
    }

    #[test]
    fn percentage_sums_to_100() {
        // 验证意图：占比分布加和恒为 100%（数据完整性）
        let events = vec![ev(65), ev(65), ev(66)];
        let pcts = Aggregator::percentages(&Aggregator::count_by_key(&events));
        let sum: f64 = pcts.values().sum();
        assert!((sum - 100.0).abs() < 0.01, "占比和应为 100，实际 {}", sum);
    }

    #[test]
    fn percentage_handles_empty() {
        // 验证意图：空输入不应 panic（边界）
        let pcts = Aggregator::percentages(&HashMap::new());
        assert!(pcts.is_empty());
    }

    #[test]
    fn top_n_returns_sorted_desc() {
        // 验证意图：top_n 按次数降序、限 N
        let mut counts = HashMap::new();
        counts.insert(1u32, 5);
        counts.insert(2u32, 10);
        counts.insert(3u32, 3);
        counts.insert(4u32, 7);
        let top = Aggregator::top_n(&counts, 2);
        assert_eq!(top, vec![(2, 10), (4, 7)]);
    }

    #[test]
    fn hourly_buckets_distributes_correctly() {
        // 验证意图：事件按 timestamp 所在小时分桶
        let mut e1 = ev(65);
        e1.timestamp_ms = chrono::Utc
            .with_ymd_and_hms(2026, 7, 16, 9, 0, 0)
            .unwrap()
            .timestamp_millis();
        let mut e2 = ev(66);
        e2.timestamp_ms = chrono::Utc
            .with_ymd_and_hms(2026, 7, 16, 14, 0, 0)
            .unwrap()
            .timestamp_millis();
        let buckets = Aggregator::hourly_buckets(&[e1, e2]);
        assert_eq!(buckets[9], 1);
        assert_eq!(buckets[14], 1);
        assert_eq!(buckets[3], 0);
    }

    #[test]
    fn count_meta_detects_pauses_deletes_repeats() {
        // 验证意图：停顿 / 删除 / 重复识别准确
        let mut events = vec![ev(65), ev(65)]; // 1 repeat
        events[1].timestamp_ms = events[0].timestamp_ms + 5000; // 5s pause
        events.push(ev(8)); // Backspace → delete
        let (pauses, deletes, repeats) = Aggregator::count_meta(&events, 2000);
        assert_eq!(pauses, 1);
        assert_eq!(deletes, 1);
        assert_eq!(repeats, 1);
    }

    #[test]
    fn aggregate_produces_complete_daily_stats() {
        // 验证意图：一站式聚合输出完整 DailyStats
        let events = vec![ev(65), ev(65), ev(66), ev(67), ev(8)];
        let stats = Aggregator::aggregate("2026-07-16".into(), &events);
        assert_eq!(stats.date, "2026-07-16");
        assert_eq!(stats.total_keys, 5);
        assert!(!stats.top_keys.is_empty());
        assert_eq!(stats.deletes, 1);
    }

    // ====== v0.3.5: 4 个核心指标 ======

    #[test]
    fn compute_metrics_intensity_with_active_hours() {
        // 100 keys / 5 hours → intensity = 20
        let mut hourly = [0usize; 24];
        for i in 0..5 { hourly[i] = 20; }
        let events: Vec<KeyEvent> = (0..100).map(|i| ev((i % 26 + 65) as u32)).collect();
        let (intensity, _, _, _) = Aggregator::compute_metrics(&events, &hourly);
        assert!((intensity - 20.0).abs() < 0.01, "intensity = {}", intensity);
    }

    #[test]
    fn compute_metrics_intensity_zero_hours_returns_zero() {
        let hourly = [0usize; 24];
        let events: Vec<KeyEvent> = vec![];
        let (intensity, _, _, _) = Aggregator::compute_metrics(&events, &hourly);
        assert_eq!(intensity, 0.0);
    }

    #[test]
    fn compute_metrics_steadiness_with_uneven_distribution() {
        // 全部按键集中在 1 小时 → 跳跃
        let mut hourly = [0usize; 24];
        hourly[0] = 100;
        let events: Vec<KeyEvent> = (0..100).map(|i| ev(65)).collect();
        let (_, steadiness, _, _) = Aggregator::compute_metrics(&events, &hourly);
        assert!(steadiness > 0.8, "全部集中 1 小时应跳跃, got {}", steadiness);
    }

    #[test]
    fn compute_metrics_steadiness_with_even_distribution() {
        // 24 小时均匀分布 → 平稳
        let hourly: [usize; 24] = [10; 24];
        let events: Vec<KeyEvent> = (0..240).map(|i| ev(65)).collect();
        let (_, steadiness, _, _) = Aggregator::compute_metrics(&events, &hourly);
        assert!(steadiness < 0.01, "24 小时均匀应接近 0, got {}", steadiness);
    }

    #[test]
    fn compute_metrics_fluency_with_pauses() {
        // 30 events, 5 个 > 2s 间隔 → fluency = 5/29 ≈ 0.17
        let mut events: Vec<KeyEvent> = vec![];
        for i in 0..30 {
            let mut e = ev(65);
            e.timestamp_ms = i as i64 * 100;
            events.push(e);
        }
        for &i in &[5, 10, 15, 20, 25] {
            events[i].timestamp_ms += 3000;
        }
        let (_, _, fluency, _) = Aggregator::compute_metrics(&events, &[0; 24]);
        let expected = 5.0 / 29.0;
        assert!((fluency - expected).abs() < 0.01, "fluency = {}, expected {}", fluency, expected);
    }

    #[test]
    fn compute_metrics_fluency_zero_intervals_returns_zero() {
        let events = vec![ev(65)];
        let (_, _, fluency, _) = Aggregator::compute_metrics(&events, &[0; 24]);
        assert_eq!(fluency, 0.0);
    }

    #[test]
    fn compute_metrics_activity_hours_counts_non_zero_buckets() {
        let mut hourly = [0usize; 24];
        hourly[3] = 5;
        hourly[7] = 10;
        hourly[20] = 2;
        let events: Vec<KeyEvent> = vec![];
        let (_, _, _, activity) = Aggregator::compute_metrics(&events, &hourly);
        assert_eq!(activity, 3);
    }

    // ====== v0.3.6: 首活时间 ======

    #[test]
    fn aggregate_picks_first_active_ms_from_events() {
        let mut events: Vec<KeyEvent> = vec![];
        for i in 0..5 {
            let mut e = ev(65);
            e.timestamp_ms = 1_753_401_600_000 + (i as i64) * 100;
            events.push(e);
        }
        // 打乱顺序确保 min 真的挑最小
        events.reverse();
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        assert_eq!(stats.first_active_ms, 1_753_401_600_000);
    }

    #[test]
    fn aggregate_first_active_ms_zero_for_empty_events() {
        let events: Vec<KeyEvent> = vec![];
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        assert_eq!(stats.first_active_ms, 0);
    }
}