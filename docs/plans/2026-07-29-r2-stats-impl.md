# R2 Stats + theme_word 简化 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 把 4 个核心行为特征指标（密集度/平稳度/流畅度/活跃度）和 WASD/Space/Enter 等键位 3 类汇总（game_keys/text_keys/modifier_keys）落地到 FingerTip 主线；同步把 theme_word 从 `"X · N"` 简化为只保留最高频字母。

**Architecture:** 在 `summary::Aggregator` 现算 4 指标 + 3 分类，写进 `DailyStats` 内存结构；`SummaryRepo::upsert_stats` 把 5 列持久化到 `daily_summary`（兼容老库用 `ALTER TABLE ADD COLUMN`）；`get_today_summary` Command 透传给前端；Today.vue 第 2 行 4→6 卡 + 新增键位分类水平条；History.vue day card 加 4 圆点（颜色按阈值）。

**Tech Stack:**
- Rust: `serde`, `serde_json`, `rusqlite`（已有）
- TS: 不加新依赖；复用 `keyCodeToGlyph` / `formatDateCN`
- 测试: `cargo test` + `vitest run` + `pnpm test:e2e`

**项目约定:** TDD 严格 Red→Green→Refactor，每个 Task 一个 commit。文件路径用绝对路径前缀 `E:/一人公司/技术部工作区/小玩具/FingerTip/`。

---

## 索引

| Task | 主题 | commit |
|---|---|---|
| Task 1 | `extract_theme_word` 去掉数字（只保留高频字母） | `refactor(theme)` |
| Task 2 | `KeyClassSummary` + `Aggregator::classify_keys` | `feat(aggregator)` |
| Task 3 | `Aggregator::compute_metrics`（4 指标一次性算齐） | `feat(aggregator)` |
| Task 4 | `DailyStats` 扩 5 字段 + JSON round-trip 测试 | `refactor(stats)` |
| Task 5 | `migrations.rs` 加 5 列 + 老库 ALTER 兼容 | `feat(db)` |
| Task 6 | `DailySummaryRow` 扩 5 字段 + `upsert`/`upsert_stats`/`upsert_mood` 同步 + tests | `refactor(summary_repo)` |
| Task 7 | `commands.rs::get_today_summary` 透传新 5 字段 | `chore(commands)` |
| Task 8 | Today.vue 第 2 行 4→6 卡 + 键位分类水平条 | `feat(ui)` |
| Task 9 | History.vue day card 加 4 圆点（按阈值变色） | `feat(ui)` |
| Task 10 | 端到端验证（typecheck + vitest + cargo test + Playwright） | `chore(release)` |

---

## Task 1: `extract_theme_word` 去掉数字

**Files:**
- Modify: `E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri/src/summary/theme.rs`

**Step 1: 改写测试期望值**

改 `theme.rs` 现有 7 个测试中的 4 个：

```rust
// 第 70 行附近：top_key_with_count_returns_readable_summary
// 旧: assert_eq!(word, "h · 28");
// 新:
assert_eq!(word, "h");

// 第 81 行附近：filters_non_printable_keys
// 旧: assert_eq!(word, "a · 8");
// 新:
assert_eq!(word, "a");

// 第 111 行附近：tie_breaks_by_smaller_key_code
// 旧: assert_eq!(word, "a · 10");
// 新:
assert_eq!(word, "a");

// 第 125 行附近：high_freq_ina_no_longer_returns_ina_concat
// 旧: assert_eq!(word, "I · 303");
// 新:
assert_eq!(word, "I");
```

**Step 2: 跑测试确认 RED**

Run: `cd src-tauri && cargo test summary::theme`
Expected: 4 测试失败（assert_eq! 左 "h · 28" 等 vs 右 "h" 不匹配）

**Step 3: 改实现**

替换 `theme.rs::extract_theme_word`（约第 26-50 行）：

```rust
pub fn extract_theme_word(counts: &HashMap<u32, usize>) -> String {
    if counts.is_empty() {
        return String::new();
    }

    // 取最高频的单个 ASCII 字符（v0.3.5: 不再输出数字）
    let mut best_key: Option<u32> = None;
    let mut best_count: usize = 0;

    for (&k, &v) in counts {
        if !is_printable_ascii(k) {
            continue;
        }
        if best_key.is_none() || v > best_count || (v == best_count && k < best_key.unwrap()) {
            best_key = Some(k);
            best_count = v;
        }
    }

    match best_key {
        // v0.3.5: 只返字母本身，丢弃 ascii_total（卡片 hero 不再带数字）
        Some(k) => (k as u8 as char).to_string(),
        None => String::new(),
    }
}
```

**Step 4: 跑测试确认 GREEN**

Run: `cd src-tauri && cargo test summary::theme`
Expected: 7 测试全过

**Step 5: Commit**

```bash
git add src-tauri/src/summary/theme.rs
git commit -m "refactor(theme): extract_theme_word 去数字，只保留最高频字母"
```

---

## Task 2: `KeyClassSummary` + `Aggregator::classify_keys`

**Files:**
- Create: `E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri/src/summary/key_class.rs`
- Modify: `E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri/src/summary/mod.rs`

**Step 1: 写失败测试**

```rust
// 新文件 src-tauri/src/summary/key_class.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct KeyClassSummary {
    pub game_keys: i64,
    pub text_keys: i64,
    pub modifier_keys: i64,
}

#[cfg(test)]
mod tests {
    // ...

    #[test]
    fn classify_keys_game_only() {
        use crate::hook::event::KeyEvent;
        use super::KeyClassSummary;
        // 写测试调用 Aggregator::classify_keys（稍后在 mod.rs 引用）
        let events = vec![
            KeyEvent::now(87, "w".into(), 0), // W
            KeyEvent::now(65, "a".into(), 0), // A
            KeyEvent::now(83, "s".into(), 0), // S
            KeyEvent::now(68, "d".into(), 0), // D
            KeyEvent::now(32, "space".into(), 0),
            KeyEvent::now(13, "enter".into(), 0),
        ];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s, KeyClassSummary { game_keys: 6, text_keys: 0, modifier_keys: 0 });
    }

    #[test]
    fn classify_keys_text_only() {
        use crate::hook::event::KeyEvent;
        use super::KeyClassSummary;
        let events = vec![
            KeyEvent::now(66, "b".into(), 0), // B
            KeyEvent::now(67, "c".into(), 0), // C
            KeyEvent::now(49, "1".into(), 0), // 1
        ];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s, KeyClassSummary { game_keys: 0, text_keys: 3, modifier_keys: 0 });
    }

    #[test]
    fn classify_keys_modifier_only() {
        use crate::hook::event::KeyEvent;
        use super::KeyClassSummary;
        let events = vec![
            KeyEvent::now(8, "bs".into(), 0),   // Backspace
            KeyEvent::now(46, "del".into(), 0),  // Delete
            KeyEvent::now(16, "shift".into(), 0),
            KeyEvent::now(17, "ctrl".into(), 0),
            KeyEvent::now(18, "alt".into(), 0),
        ];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s, KeyClassSummary { game_keys: 0, text_keys: 0, modifier_keys: 5 });
    }

    #[test]
    fn classify_keys_wins_over_text() {
        // 关键：WASD 即使是 ASCII 字母也归 game_keys，不双计到 text_keys
        use crate::hook::event::KeyEvent;
        use super::KeyClassSummary;
        let events = vec![
            KeyEvent::now(87, "w".into(), 0),  // W = ASCII 字母 + game_key
            KeyEvent::now(65, "a".into(), 0),  // A = ASCII 字母 + game_key
            KeyEvent::now(83, "s".into(), 0),  // S = ASCII 字母 + game_key
            KeyEvent::now(68, "d".into(), 0),  // D = ASCII 字母 + game_key
            KeyEvent::now(66, "b".into(), 0),  // B = 纯 text_key
        ];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s, KeyClassSummary { game_keys: 4, text_keys: 1, modifier_keys: 0 });
    }

    #[test]
    fn classify_keys_empty() {
        use crate::hook::event::KeyEvent;
        let events: Vec<KeyEvent> = vec![];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s.game_keys + s.text_keys + s.modifier_keys, 0);
    }

    #[test]
    fn classify_keys_ignores_unknown_codes() {
        // F1(112)、方向键(37-40) 等其它键不归入任何类别
        use crate::hook::event::KeyEvent;
        use super::KeyClassSummary;
        let events = vec![
            KeyEvent::now(112, "f1".into(), 0), // F1
            KeyEvent::now(37, "left".into(), 0),
            KeyEvent::now(38, "up".into(), 0),
        ];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s, KeyClassSummary::default());
    }
}
```

**Step 2: 跑测试确认 RED**

Run: `cd src-tauri && cargo test summary::key_class`
Expected: 编译失败 "function `classify_keys` not found in `Aggregator`"

**Step 3: 注册 mod + 加 classify_keys 实现**

`mod.rs` 加 `pub mod key_class;`：

```rust
pub mod aggregator;
pub mod key_class;
pub mod scheduler;
pub mod stats;
pub mod theme;
```

`aggregator.rs` 加 classify_keys（约第 105 行末尾，aggregate 函数前）：

```rust
use crate::summary::key_class::KeyClassSummary;

impl Aggregator {
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
}
```

**Step 4: 跑测试确认 GREEN**

Run: `cd src-tauri && cargo test summary::key_class`
Expected: 6 测试全过

**Step 5: Commit**

```bash
git add src-tauri/src/summary/key_class.rs src-tauri/src/summary/mod.rs src-tauri/src/summary/aggregator.rs
git commit -m "feat(aggregator): KeyClassSummary + classify_keys（3 类键汇总）"
```

---

## Task 3: `Aggregator::compute_metrics`（4 指标）

**Files:**
- Modify: `E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri/src/summary/aggregator.rs`

**Step 1: 写失败测试**

在 `aggregator.rs` 现有 `#[cfg(test)] mod tests` 末尾追加：

```rust
    // ====== v0.3.5: 4 个核心指标 ======

    #[test]
    fn compute_metrics_intensity_with_active_hours() {
        // 100 keys / 5 hours → intensity = 20
        let hourly = [0; 24];
        let mut hourly = hourly;
        for i in 0..5 { hourly[i] = 20; } // 5 hours × 20 keys = 100
        let events: Vec<KeyEvent> = (0..100).map(|i| ev((i % 26 + 65) as u32)).collect();
        let (intensity, _, _, _) = Aggregator::compute_metrics(&events, &hourly);
        assert!((intensity - 20.0).abs() < 0.01, "intensity = {}", intensity);
    }

    #[test]
    fn compute_metrics_intensity_zero_hours_returns_zero() {
        let hourly = [0; 24];
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
            e.timestamp_ms = i as i64 * 100; // 100ms 间隔
            events.push(e);
        }
        // 在 5 个位置插入 > 2s 间隔
        for &i in &[5, 10, 15, 20, 25] {
            events[i].timestamp_ms += 3000;
        }
        let (_, _, fluency, _) = Aggregator::compute_metrics(&events, &[0; 24]);
        let expected = 5.0 / 29.0;
        assert!((fluency - expected).abs() < 0.01, "fluency = {}, expected {}", fluency, expected);
    }

    #[test]
    fn compute_metrics_fluency_zero_intervals_returns_zero() {
        // 1 event → 无间隔
        let events = vec![ev(65)];
        let (_, _, fluency, _) = Aggregator::compute_metrics(&events, &[0; 24]);
        assert_eq!(fluency, 0.0);
    }

    #[test]
    fn compute_metrics_activity_hours_counts_non_zero_buckets() {
        let mut hourly = [0usize; 24];
        hourly[3] = 5;  // 3:00
        hourly[7] = 10; // 7:00
        hourly[20] = 2; // 20:00
        let events: Vec<KeyEvent> = vec![];
        let (_, _, _, activity) = Aggregator::compute_metrics(&events, &hourly);
        assert_eq!(activity, 3);
    }
```

**Step 2: 跑测试确认 RED**

Run: `cd src-tauri && cargo test summary::aggregator`
Expected: 7 测试编译失败 "function `compute_metrics` not found"

**Step 3: 实现 compute_metrics**

在 `aggregator.rs` `aggregate` 函数（约第 88 行）前面加：

```rust
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
        let fluency = if total_intervals == 0 { 0.0 } else { 
            pauses_over_2s as f64 / total_intervals as f64 
        };
        
        (intensity, steadiness, fluency, active_hours as i32)
    }
```

**Step 4: 跑测试确认 GREEN**

Run: `cd src-tauri && cargo test summary::aggregator`
Expected: 全部测试通过（旧 + 新）

**Step 5: Commit**

```bash
git add src-tauri/src/summary/aggregator.rs
git commit -m "feat(aggregator): compute_metrics（4 指标密集度/平稳度/流畅度/活跃度）"
```

---

## Task 4: `DailyStats` 扩 5 字段

**Files:**
- Modify: `E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri/src/summary/stats.rs`
- Modify: `E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri/src/summary/aggregator.rs`（让 aggregate() 计算并填入新字段）

**Step 1: 改 DailyStats struct**

`stats.rs`：

```rust
use serde::{Deserialize, Serialize};

/// 日聚合统计的数据契约。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStats {
    pub date: String,
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
        }
    }
}
```

**Step 2: 让 `aggregate()` 调用 compute_metrics + classify_keys**

`aggregator.rs::aggregate`（约第 88 行）改：

```rust
    pub fn aggregate(date: String, events: &[KeyEvent]) -> DailyStats {
        let counts = Aggregator::count_by_key(events);
        let pcts = Aggregator::percentages(&counts);
        let top = Aggregator::top_n(&counts, 5);
        let hourly = Aggregator::hourly_buckets(events);
        let (pauses, deletes, repeats) = Aggregator::count_meta(events, 2000);
        
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
        }
    }
```

**Step 3: 加 JSON round-trip 测试**

`stats.rs` 加：

```rust
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
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: DailyStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back.intensity, 20.0);
        assert_eq!(back.steadiness, 0.5);
        assert_eq!(back.fluency, 0.05);
        assert_eq!(back.activity_hours, 5);
        assert!(back.key_class_json.contains("game_keys"));
    }
}
```

**Step 4: 跑测试确认 GREEN**

Run: `cd src-tauri && cargo test summary::stats && cargo test summary::aggregator`
Expected: 全部测试通过（`aggregate_produces_complete_daily_stats` 等旧测试也会过——只是 `DailyStats` 多 5 字段，assert 只检查 date/total_keys/deletes 等不被破坏）

**Step 5: Commit**

```bash
git add src-tauri/src/summary/stats.rs src-tauri/src/summary/aggregator.rs
git commit -m "refactor(stats): DailyStats 扩 5 字段 + aggregate() 联动 compute_metrics/classify_keys"
```

---

## Task 5: `migrations.rs` 加 5 列 + 老库 ALTER 兼容

**Files:**
- Modify: `E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri/src/db/migrations.rs`

**Step 1: 写失败测试（fresh DB + 5 列齐全）**

`migrations.rs` 加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }
    
    fn count_columns(conn: &Connection, table: &str) -> i64 {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{}')", table),
            [],
            |r| r.get(0),
        ).unwrap()
    }
    
    #[test]
    fn fresh_db_has_all_5_new_columns() {
        // 验证意图：v0.3.5 新库一次性建出 5 列
        let conn = fresh_db();
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('daily_summary')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        for name in &["intensity", "steadiness", "fluency", "activity_hours", "key_class_json"] {
            assert!(cols.iter().any(|c| c == name), "missing column: {}", name);
        }
    }
    
    #[test]
    fn old_db_gets_columns_via_alter() {
        // 验证意图：模拟老库（3 列 daily_summary）→ run_migrations 后 ALTER 出 5 列
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE daily_summary (
                date TEXT PRIMARY KEY,
                total_keys INTEGER NOT NULL,
                top_keys_json TEXT NOT NULL,
                theme_word TEXT NOT NULL,
                mood_word TEXT,
                created_at INTEGER NOT NULL
            );"
        ).unwrap();
        
        // 确认是 6 列（老 schema）
        assert_eq!(count_columns(&conn, "daily_summary"), 6);
        
        // 跑 run_migrations → 应自动 ALTER 加 5 列
        run_migrations(&conn).unwrap();
        
        // 变成 11 列
        assert_eq!(count_columns(&conn, "daily_summary"), 11);
        
        // 5 个新列都存在
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('daily_summary')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        for name in &["intensity", "steadiness", "fluency", "activity_hours", "key_class_json"] {
            assert!(cols.iter().any(|c| c == name), "ALTER failed to add: {}", name);
        }
    }
    
    #[test]
    fn old_db_existing_row_keeps_old_data_with_defaults() {
        // 验证意图：老库已有行不会被 ALTER 删；新列默认值生效
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE daily_summary (
                date TEXT PRIMARY KEY,
                total_keys INTEGER NOT NULL,
                top_keys_json TEXT NOT NULL,
                theme_word TEXT NOT NULL,
                mood_word TEXT,
                created_at INTEGER NOT NULL
            );
            INSERT INTO daily_summary VALUES ('2026-07-28', 100, '[]', 'hello', 'happy', 1000);"
        ).unwrap();
        
        run_migrations(&conn).unwrap();
        
        // 老行还在，新列有默认值
        let row: (i64, f64, String) = conn.query_row(
            "SELECT total_keys, intensity, theme_word FROM daily_summary WHERE date = '2026-07-28'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(row.0, 100);
        assert_eq!(row.1, 0.0);
        assert_eq!(row.2, "hello");
    }
}
```

**Step 2: 跑测试确认 RED**

Run: `cd src-tauri && cargo test db::migrations`
Expected: `fresh_db_has_all_5_new_columns` 失败（fresh db 没有新列）

**Step 3: 改 run_migrations**

```rust
use rusqlite::Connection;

/// 数据库迁移入口。
///
/// 验证意图：建表语句集中管理，所有表一次性建立，避免漏建。
/// v0.3.5: 兼容老库（无 ALTER 机制）—— 启动时检查列名，缺则 ADD COLUMN。
pub fn run_migrations(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS key_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            key_code INTEGER NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            session_id TEXT NOT NULL,
            modifiers INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_key_events_ts ON key_events(timestamp_ms);
        CREATE INDEX IF NOT EXISTS idx_key_events_session ON key_events(session_id);

        CREATE TABLE IF NOT EXISTS daily_summary (
            date TEXT PRIMARY KEY,
            total_keys INTEGER NOT NULL,
            top_keys_json TEXT NOT NULL,
            theme_word TEXT NOT NULL,
            mood_word TEXT,
            -- v0.3.5 新增
            intensity REAL NOT NULL DEFAULT 0.0,
            steadiness REAL NOT NULL DEFAULT 0.0,
            fluency REAL NOT NULL DEFAULT 0.0,
            activity_hours INTEGER NOT NULL DEFAULT 0,
            key_class_json TEXT NOT NULL DEFAULT '{}',
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS artifacts (
            date TEXT PRIMARY KEY,
            music_json TEXT NOT NULL,
            art_json TEXT NOT NULL,
            music_wav_path TEXT,
            art_png_path TEXT,
            created_at INTEGER NOT NULL
        );
        ",
    )?;
    
    // v0.3.5: 兼容老库（无 migration 版本号机制）
    // 检查 intensity 列是否存在；不存在则 ADD COLUMN
    let has_intensity: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('daily_summary') WHERE name='intensity'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);
    
    if !has_intensity {
        log::warn!("daily_summary 缺 5 个新列（v0.3.5）—— 自动 ALTER TABLE ADD COLUMN");
        conn.execute_batch(
            "ALTER TABLE daily_summary ADD COLUMN intensity REAL NOT NULL DEFAULT 0.0;
             ALTER TABLE daily_summary ADD COLUMN steadiness REAL NOT NULL DEFAULT 0.0;
             ALTER TABLE daily_summary ADD COLUMN fluency REAL NOT NULL DEFAULT 0.0;
             ALTER TABLE daily_summary ADD COLUMN activity_hours INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE daily_summary ADD COLUMN key_class_json TEXT NOT NULL DEFAULT '{}';"
        )?;
    }
    
    Ok(())
}
```

**Step 4: 跑测试确认 GREEN**

Run: `cd src-tauri && cargo test db::migrations`
Expected: 3 测试全过

**Step 5: Commit**

```bash
git add src-tauri/src/db/migrations.rs
git commit -m "feat(db): daily_summary 加 5 列 + 老库 ALTER 兼容"
```

---

## Task 6: `DailySummaryRow` 扩 5 字段 + `SummaryRepo` 同步

**Files:**
- Modify: `E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri/src/db/summary_repo.rs`

**Step 1: 改 DailySummaryRow + 3 个 upsert 函数**

```rust
use crate::summary::stats::DailyStats;
use rusqlite::{params, Connection, OptionalExtension};

pub struct SummaryRepo<'a> {
    conn: &'a Connection,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DailySummaryRow {
    pub date: String,
    pub total_keys: i64,
    pub top_keys_json: String,
    pub theme_word: String,
    pub mood_word: Option<String>,
    // v0.3.5 新增
    pub intensity: f64,
    pub steadiness: f64,
    pub fluency: f64,
    pub activity_hours: i32,
    pub key_class_json: String,
    pub created_at: i64,
}

impl<'a> SummaryRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 整体 upsert（含 mood_word）—— 测试 + 一次性初始化用
    pub fn upsert(
        &self,
        stats: &DailyStats,
        theme_word: &str,
        mood_word: Option<&str>,
    ) -> anyhow::Result<()> {
        let top_keys_json = serde_json::to_string(&stats.top_keys)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO daily_summary 
             (date, total_keys, top_keys_json, theme_word, mood_word, 
              intensity, steadiness, fluency, activity_hours, key_class_json, 
              created_at) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                stats.date,
                stats.total_keys as i64,
                top_keys_json,
                theme_word,
                mood_word,
                stats.intensity,
                stats.steadiness,
                stats.fluency,
                stats.activity_hours,
                stats.key_class_json,
                chrono::Utc::now().timestamp_millis(),
            ],
        )?;
        Ok(())
    }

    /// v0.3.1: upsert 但保留已有 mood_word —— scheduler 60s tick 用
    pub fn upsert_stats(
        &self,
        stats: &DailyStats,
        theme_word: &str,
    ) -> anyhow::Result<()> {
        let top_keys_json = serde_json::to_string(&stats.top_keys)?;
        self.conn.execute(
            "INSERT INTO daily_summary 
             (date, total_keys, top_keys_json, theme_word, mood_word, 
              intensity, steadiness, fluency, activity_hours, key_class_json, 
              created_at) 
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(date) DO UPDATE SET
                total_keys = excluded.total_keys,
                top_keys_json = excluded.top_keys_json,
                theme_word = excluded.theme_word,
                intensity = excluded.intensity,
                steadiness = excluded.steadiness,
                fluency = excluded.fluency,
                activity_hours = excluded.activity_hours,
                key_class_json = excluded.key_class_json,
                created_at = excluded.created_at",
            params![
                stats.date,
                stats.total_keys as i64,
                top_keys_json,
                theme_word,
                stats.intensity,
                stats.steadiness,
                stats.fluency,
                stats.activity_hours,
                stats.key_class_json,
                chrono::Utc::now().timestamp_millis(),
            ],
        )?;
        Ok(())
    }

    /// 按日期读
    pub fn read_by_date(&self, date: &str) -> anyhow::Result<Option<DailySummaryRow>> {
        let row = self.conn.query_row(
            "SELECT date, total_keys, top_keys_json, theme_word, mood_word, 
                    intensity, steadiness, fluency, activity_hours, key_class_json, 
                    created_at 
             FROM daily_summary WHERE date = ?",
            params![date],
            |row| {
                Ok(DailySummaryRow {
                    date: row.get(0)?,
                    total_keys: row.get(1)?,
                    top_keys_json: row.get(2)?,
                    theme_word: row.get(3)?,
                    mood_word: row.get(4)?,
                    intensity: row.get(5)?,
                    steadiness: row.get(6)?,
                    fluency: row.get(7)?,
                    activity_hours: row.get(8)?,
                    key_class_json: row.get(9)?,
                    created_at: row.get(10)?,
                })
            },
        ).optional()?;
        Ok(row)
    }

    /// list_all / list_recent 同样改 SELECT 列表
    pub fn list_all(&self) -> anyhow::Result<Vec<DailySummaryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT date, total_keys, top_keys_json, theme_word, mood_word, 
                    intensity, steadiness, fluency, activity_hours, key_class_json, 
                    created_at 
             FROM daily_summary ORDER BY date DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DailySummaryRow {
                date: row.get(0)?,
                total_keys: row.get(1)?,
                top_keys_json: row.get(2)?,
                theme_word: row.get(3)?,
                mood_word: row.get(4)?,
                intensity: row.get(5)?,
                steadiness: row.get(6)?,
                fluency: row.get(7)?,
                activity_hours: row.get(8)?,
                key_class_json: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_recent(&self, limit: usize) -> anyhow::Result<Vec<DailySummaryRow>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT date, total_keys, top_keys_json, theme_word, mood_word, 
                    intensity, steadiness, fluency, activity_hours, key_class_json, 
                    created_at 
             FROM daily_summary ORDER BY date DESC LIMIT ?"
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(DailySummaryRow {
                date: row.get(0)?,
                total_keys: row.get(1)?,
                top_keys_json: row.get(2)?,
                theme_word: row.get(3)?,
                mood_word: row.get(4)?,
                intensity: row.get(5)?,
                steadiness: row.get(6)?,
                fluency: row.get(7)?,
                activity_hours: row.get(8)?,
                key_class_json: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// mood 单点管（不动其它字段）—— v0.3.1 P0 #2 修复
    pub fn upsert_mood(&self, date: &str, mood: &str) -> anyhow::Result<()> {
        let truncated: String = mood.chars().take(64).collect();
        self.conn.execute(
            "INSERT INTO daily_summary 
             (date, total_keys, top_keys_json, theme_word, mood_word, 
              intensity, steadiness, fluency, activity_hours, key_class_json, 
              created_at) 
             VALUES (?1, 0, '[]', '', ?2, 0.0, 0.0, 0.0, 0, '{}', ?3)
             ON CONFLICT(date) DO UPDATE SET mood_word = excluded.mood_word",
            params![date, truncated, chrono::Utc::now().timestamp_millis()],
        )?;
        Ok(())
    }
}
```

**Step 2: 写测试**

`summary_repo.rs` 现有 `#[cfg(test)] mod tests` 末尾追加：

```rust
    #[test]
    fn upsert_stats_persists_5_new_columns() {
        let conn = fresh_db();
        let mut events: Vec<KeyEvent> = vec![];
        for i in 0..50 {
            let mut e = KeyEvent::now((i % 26 + 65) as u32, "s".into(), 0);
            e.timestamp_ms = 1_753_401_600_000 + (i as i64) * 100;
            events.push(e);
        }
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert_stats(&stats, "hello").unwrap();

        let read = SummaryRepo::new(&conn).read_by_date("2026-07-29").unwrap().unwrap();
        assert!(read.intensity > 0.0, "intensity must be > 0, got {}", read.intensity);
        assert!(read.steadiness >= 0.0);
        assert!(read.fluency >= 0.0);
        assert_eq!(read.activity_hours, read.activity_hours); // sanity
        assert!(read.key_class_json.contains("game_keys"));
    }

    #[test]
    fn upsert_mood_preserves_new_columns() {
        // v0.3.1 P0 #2 回归不破：mood 单点管，新指标不被覆盖
        let conn = fresh_db();
        let mut events: Vec<KeyEvent> = vec![];
        for i in 0..30 {
            let mut e = KeyEvent::now((i % 26 + 65) as u32, "s".into(), 0);
            e.timestamp_ms = 1_753_401_600_000 + (i as i64) * 100;
            events.push(e);
        }
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert_stats(&stats, "hello").unwrap();
        SummaryRepo::new(&conn).upsert_mood("2026-07-29", "happy").unwrap();

        let read = SummaryRepo::new(&conn).read_by_date("2026-07-29").unwrap().unwrap();
        assert_eq!(read.mood_word.as_deref(), Some("happy"));
        assert!(read.intensity > 0.0, "新指标必须保留, got {}", read.intensity);
    }

    #[test]
    fn read_by_date_returns_zero_for_old_data_after_alter() {
        // 老库 ALTER 后读老行：5 个新列拿到默认值（0.0 / 0 / "{}"）
        let conn = fresh_db();
        // 模拟老库插入（直接 INSERT 不带新列；它们走 DEFAULT）
        conn.execute(
            "INSERT INTO daily_summary (date, total_keys, top_keys_json, theme_word, mood_word, created_at) 
             VALUES ('2026-07-28', 100, '[]', 'hello', 'happy', 1000)",
            []
        ).unwrap();
        
        let read = SummaryRepo::new(&conn).read_by_date("2026-07-28").unwrap().unwrap();
        assert_eq!(read.intensity, 0.0);
        assert_eq!(read.steadiness, 0.0);
        assert_eq!(read.fluency, 0.0);
        assert_eq!(read.activity_hours, 0);
        assert_eq!(read.key_class_json, "{}");
    }
```

**Step 3: 跑测试确认 GREEN**

Run: `cd src-tauri && cargo test db::summary_repo`
Expected: 全部测试通过（旧的 7 个 + 新的 3 个）

**Step 4: Commit**

```bash
git add src-tauri/src/db/summary_repo.rs
git commit -m "refactor(summary_repo): DailySummaryRow 扩 5 字段 + 3 个 upsert 函数同步"
```

---

## Task 7: `commands.rs::get_today_summary` 透传 5 字段

**Files:**
- Modify: `E:/一人公司/技术部工作区\小玩具\FingerTip/src-tauri/src/commands.rs`

**Step 1: 验证 DailySummaryRow 自动 JSON 序列化**

`DailySummaryRow` 已 derive `Serialize`，新增的 5 字段会自动出现在 JSON 输出里。无需手动改 `get_today_summary_impl` / `get_today_summary` —— 它们只是 `serde_json::to_string(&row)`。

**Step 2: 端到端测试**

`commands.rs::tests` 加：

```rust
    #[test]
    fn get_today_summary_includes_5_new_columns() {
        // 验证意图：get_today_summary 返回 JSON 包含 5 新字段
        let conn = fresh_db();
        let events = vec![KeyEvent::now(65, "s".into(), 0); 50];
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert(&stats, "hello", Some("happy")).unwrap();

        let json = get_today_summary_impl(&conn, "2026-07-29").unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["intensity"].is_number());
        assert!(parsed["steadiness"].is_number());
        assert!(parsed["fluency"].is_number());
        assert!(parsed["activity_hours"].is_number());
        assert!(parsed["key_class_json"].is_string());
    }
```

**Step 3: 跑测试**

Run: `cd src-tauri && cargo test commands::tests::get_today_summary_includes_5_new_columns`
Expected: PASS

**Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "chore(commands): get_today_summary 透传 5 新字段（无需新代码，自动 via serde）"
```

---

## Task 8: Today.vue 第 2 行 4→6 卡 + 键位分类水平条

**Files:**
- Modify: `E:/一人公司/技术部工作区/小玩具/FingerTip/src/views/Today.vue`

**Step 1: 改 DailySummaryRow interface**

`Today.vue` 顶部（约第 118 行）：

```typescript
interface DailySummaryRow {
  date: string
  total_keys: number
  theme_word: string
  mood_word: string | null
  top_keys_json: string
  // v0.3.5 新增
  intensity: number
  steadiness: number
  fluency: number
  activity_hours: number
  key_class_json: string
}
```

**Step 2: 第 2 行 stats-row 改 6 卡**

替换 `<section class="ft-stats-row ft-stagger ft-stagger-2">` 整段（Today.vue 第 28-57 行）：

```html
  <section class="ft-stats-row ft-stats-row--6 ft-stagger ft-stagger-2">
    <div class="ft-stat-mini">
      <div class="ft-stat-mini-label">密集度 dynsity</div>
      <div class="ft-stat-mini-value ft-stat-mini-value--mono">
        {{ summary?.intensity != null ? summary.intensity.toFixed(0) : '—' }}
        <span class="ft-unit">键/h</span>
      </div>
      <div class="ft-stat-mini-delta" :class="{ up: (summary?.intensity ?? 0) >= 800 }">
        <template v-if="(summary?.intensity ?? 0) >= 800">快</template>
        <template v-else-if="(summary?.intensity ?? 0) > 0">慢</template>
        <template v-else>—</template>
      </div>
    </div>
    <div class="ft-stat-mini">
      <div class="ft-stat-mini-label">平稳度 stabilit</div>
      <div class="ft-stat-mini-value ft-stat-mini-value--mono">
        {{ summary?.steadiness != null ? summary.steadiness.toFixed(2) : '—' }}
      </div>
      <div class="ft-stat-mini-delta" :class="{ up: (summary?.steadiness ?? 1) <= 0.8 }">
        <template v-if="(summary?.steadiness ?? 1) <= 0.8">平稳</template>
        <template v-else-if="(summary?.steadiness ?? 0) > 0">跳跃</template>
        <template v-else>—</template>
      </div>
    </div>
    <div class="ft-stat-mini">
      <div class="ft-stat-mini-label">流畅度 fluency</div>
      <div class="ft-stat-mini-value ft-stat-mini-value--mono">
        {{ summary?.fluency != null ? (summary.fluency * 100).toFixed(0) : '—' }}<span class="ft-unit">%</span>
      </div>
      <div class="ft-stat-mini-delta" :class="{ up: (summary?.fluency ?? 1) < 0.10 }">
        <template v-if="(summary?.fluency ?? 1) < 0.10">流畅</template>
        <template v-else-if="(summary?.fluency ?? 0) > 0">停顿</template>
        <template v-else>—</template>
      </div>
    </div>
    <div class="ft-stat-mini">
      <div class="ft-stat-mini-label">活跃度 activity</div>
      <div class="ft-stat-mini-value ft-stat-mini-value--mono">
        {{ summary?.activity_hours ?? '—' }}<span class="ft-unit">h</span>
      </div>
      <div class="ft-stat-mini-delta" :class="{ up: (summary?.activity_hours ?? 0) > 4 }">
        <template v-if="(summary?.activity_hours ?? 0) > 4">活跃</template>
        <template v-else-if="(summary?.activity_hours ?? 0) > 0">不活跃</template>
        <template v-else>—</template>
      </div>
    </div>
    <div class="ft-stat-mini">
      <div class="ft-stat-mini-label">高峰按键</div>
      <div class="ft-stat-mini-value ft-stat-mini-value--mono">{{ peakCount }}</div>
      <div class="ft-stat-mini-delta">最忙小时</div>
    </div>
    <div class="ft-stat-mini">
      <div class="ft-stat-mini-label">手动聚合</div>
      <button class="ft-recalc-btn" :disabled="recalculating" @click="onRecalculate">
        {{ recalculating ? '聚合中…' : 'Recalculate' }}
      </button>
      <div class="ft-stat-mini-delta" :class="{ up: !loading }">
        <template v-if="!loading">已读今日 summary</template>
        <template v-else>按键后立即聚合</template>
      </div>
    </div>
  </section>
```

**Step 3: 第 3 行新增"键位分类"panel**

在第 3 行（24h 热力图 + Top5）之后，**追加一段新 panel**：

```html
  <!-- 第四行：键位分类水平条（v0.3.5 新增） -->
  <section class="ft-key-class-section ft-stagger ft-stagger-4">
    <div class="ft-panel">
      <div class="ft-panel-header">
        <div class="ft-panel-title">键位分类</div>
        <div class="ft-panel-meta">今日按键 · 游戏 / 文本 / 功能</div>
      </div>
      <div class="ft-key-class-bar" v-if="keyClass">
        <div class="ft-key-class-seg" :style="{ width: keyClass.game_ratio + '%' }" :title="`游戏键 ${keyClass.game_keys}`">
          <span class="ft-key-class-label">游戏 {{ keyClass.game_keys }}</span>
        </div>
        <div class="ft-key-class-seg ft-key-class-seg--text" :style="{ width: keyClass.text_ratio + '%' }" :title="`文本键 ${keyClass.text_keys}`">
          <span class="ft-key-class-label">文本 {{ keyClass.text_keys }}</span>
        </div>
        <div class="ft-key-class-seg ft-key-class-seg--mod" :style="{ width: keyClass.modifier_ratio + '%' }" :title="`功能键 ${keyClass.modifier_keys}`">
          <span class="ft-key-class-label">功能 {{ keyClass.modifier_keys }}</span>
        </div>
      </div>
      <div v-else class="ft-empty">
        <div class="ft-empty-text">还没有按键记录</div>
        <div class="ft-empty-hint">点 Recalculate 聚合已捕获的键</div>
      </div>
    </div>
  </section>
```

**Step 4: 改 script 部分（加 keyClass 计算属性）**

`Today.vue` script 区末尾追加：

```typescript
const keyClass = computed(() => {
  const json = summary.value?.key_class_json
  if (!json) return null
  try {
    const parsed = JSON.parse(json) as { game_keys: number; text_keys: number; modifier_keys: number }
    const total = parsed.game_keys + parsed.text_keys + parsed.modifier_keys
    if (total === 0) return null
    return {
      game_keys: parsed.game_keys,
      text_keys: parsed.text_keys,
      modifier_keys: parsed.modifier_keys,
      game_ratio: (parsed.game_keys / total) * 100,
      text_ratio: (parsed.text_keys / total) * 100,
      modifier_ratio: (parsed.modifier_keys / total) * 100,
    }
  } catch {
    return null
  }
})
```

**Step 5: 加 CSS**

`<style scoped>` 末尾追加：

```css
.ft-stats-row--6 {
  grid-template-columns: repeat(6, 1fr);
}
@media (max-width: 1280px) {
  .ft-stats-row--6 {
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  }
}

.ft-key-class-section {
  margin-top: var(--sp-6);
}
.ft-key-class-bar {
  display: flex;
  height: 36px;
  border-radius: var(--r-sm);
  overflow: hidden;
  background: var(--bg-elevated);
}
.ft-key-class-seg {
  background: var(--accent-warm);
  color: #FFFFFF;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  font-weight: 600;
  transition: width 500ms ease;
  overflow: hidden;
  white-space: nowrap;
}
.ft-key-class-seg--text { background: #B87547; }
.ft-key-class-seg--mod { background: #8A8A8A; }
.ft-key-class-label {
  padding: 0 var(--sp-2);
}
```

**Step 6: typecheck**

Run: `cd E:/一人公司/技术部工作区/小玩具/FingerTip && pnpm typecheck`
Expected: 0 errors

**Step 7: 手动验证**

- 启 dev: `pnpm tauri dev`
- 按一些键（含 WASD / 字母 / Backspace）→ 点 Recalculate
- 验证 4 指标数字正确 + 键位分类水平条 3 段比例对

**Step 8: Commit**

```bash
git add src/views/Today.vue
git commit -m "feat(ui): Today.vue 第 2 行 4→6 卡 + 键位分类水平条"
```

---

## Task 9: History.vue day card 加 4 圆点

**Files:**
- Modify: `E:/一人公司/技术部工作区/小玩具/FingerTip/src/views/History.vue`

**Step 1: 改 DailySummaryRow interface + day card 模板**

`History.vue` 顶部：

```typescript
interface DailySummaryRow {
  date: string
  total_keys: number
  theme_word: string
  mood_word: string | null
  top_keys_json: string
  // v0.3.5 新增
  intensity: number
  steadiness: number
  fluency: number
  activity_hours: number
  key_class_json: string
}
```

day card div 替换：

```html
      <div
        v-for="day in days"
        :key="day.date"
        class="ft-day-card"
        @click="goToArtworks(day)"
      >
        <div class="ft-day-date">{{ formatDateCN(day.date) }}</div>
        <div class="ft-day-theme">{{ day.theme_word || '—' }}</div>
        <div class="ft-day-mood">
          {{ day.mood_word || '—' }} · {{ day.total_keys.toLocaleString() }} keys
        </div>
        <div class="ft-day-dots" :title="`强度 ${day.intensity?.toFixed(0)} · 平稳 ${day.steadiness?.toFixed(2)} · 流畅 ${(day.fluency*100).toFixed(0)}% · 活跃 ${day.activity_hours}h`">
          <span class="ft-day-dot" :class="{ 'is-fast': (day.intensity ?? 0) >= 800 }" title="快"></span>
          <span class="ft-day-dot" :class="{ 'is-stable': (day.steadiness ?? 1) <= 0.8 }" title="稳"></span>
          <span class="ft-day-dot" :class="{ 'is-fluent': (day.fluency ?? 1) < 0.10 }" title="流"></span>
          <span class="ft-day-dot" :class="{ 'is-active': (day.activity_hours ?? 0) > 4 }" title="活"></span>
          <span class="ft-day-dot-labels">
            <span>快</span><span>稳</span><span>流</span><span>活</span>
          </span>
        </div>
      </div>
```

**Step 2: 加 CSS**

`<style scoped>` 末尾追加：

```css
.ft-day-dots {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: var(--sp-3);
  padding-top: var(--sp-3);
  border-top: 1px dashed var(--border-subtle);
}
.ft-day-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--border-default);
  transition: background 200ms;
}
.ft-day-dot.is-fast { background: var(--accent-warm); }
.ft-day-dot.is-stable,
.ft-day-dot.is-fluent,
.ft-day-dot.is-active { background: var(--accent-grow); }
.ft-day-dot-labels {
  display: flex;
  gap: 4px;
  margin-left: auto;
  font-size: 10px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
}
.ft-day-dot-labels span {
  min-width: 12px;
  text-align: center;
}
```

**Step 3: typecheck**

Run: `pnpm typecheck`
Expected: 0 errors

**Step 4: Commit**

```bash
git add src/views/History.vue
git commit -m "feat(ui): History.vue day card 加 4 圆点（快/稳/流/活）"
```

---

## Task 10: 端到端验证

**Step 1: 跑全测试**

```bash
cd E:/一人公司/技术部工作区/小玩具/FingerTip
cargo test
pnpm test
pnpm typecheck
```

Expected: 全部通过

**Step 2: 跑 Playwright e2e**

新写两个 e2e spec：

`tests/e2e/today-page-stats.spec.ts`：

```typescript
import { test, expect } from '@playwright/test'

test('today page shows 4 metrics', async ({ page }) => {
  await page.goto('/today')
  // 触发 mock 后端
  await page.evaluate(() => {
    window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || { transformCallback: () => 0, invoke: async (cmd: string, args?: any) => {
      if (cmd === 'get_today_summary') return JSON.stringify({...})
      // ...
    } }
  })
  // 验证 4 卡显示
  await expect(page.locator('text=密集度')).toBeVisible()
  await expect(page.locator('text=平稳度')).toBeVisible()
  await expect(page.locator('text=流畅度')).toBeVisible()
  await expect(page.locator('text=活跃度')).toBeVisible()
})
```

参考现有 `tests/e2e/` 里的 stub 后端模式（v0.3-mock-cleanup）。

**Step 3: 跑 e2e**

```bash
pnpm test:e2e
```

Expected: 新增 2 个 + 旧 12 个 = 14 个全过

**Step 4: 最终 commit**

```bash
git add tests/e2e/
git commit -m "test(e2e): today page 4 metrics + history page 4 dots"
```

---

## 验证清单（每个 Task 完成后必跑）

| 检查 | 命令 |
|---|---|
| Rust 测试 | `cd src-tauri && cargo test` |
| 前端单测 | `pnpm test` |
| 类型检查 | `pnpm typecheck` |
| 端到端 | `pnpm test:e2e` |

---

## 风险 & 回滚

| 风险 | 回滚策略 |
|---|---|
| ALTER TABLE 失败 | log warn 但不阻塞启动；前端拿到默认值 0.0 显示"—" |
| 字段名拼写错（intensity 等） | grep 27 文件引用 → 用 Edit 全替换 |
| Today.vue 6 卡移动端挤压 | CSS `repeat(auto-fit, minmax(140px, 1fr))` 自动堆叠 |
| 老 DB `theme_word` 是 `"I · 303"` | 接受，下次 scheduler 跑就改写（≤60s 一致性窗口） |

---

## 不做的事（YAGNI）

- ❌ 不做"按小时段细分指标"
- ❌ 不做"键分类下钻详情"
- ❌ 不做"自适应阈值"
- ❌ 不做"指标趋势图"
- ❌ 不做"AI 解读"（R3 阶段）

---

## 完成后

发 PR dev → main（合并时 `--no-ff` 保留详细历史），或继续在 dev 上叠 v0.3.5 / v0.3.6 release notes。

发版本：当前 `package.json` 和 `Cargo.toml` 都是 `0.3.0`，发版前需 bump。