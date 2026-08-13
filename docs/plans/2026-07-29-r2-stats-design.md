# R2 行为特征 + 键位分类 — 设计文档

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 把用户截图里"核心特征保留四类"（密集度 / 平稳度 / 流畅度 / 活跃度）落地到 FingerTip 主线，并把 WASD/Space/Enter/Backspace/Delete 等"分类汇总"成 3 类键集合。

**Architecture:** 在 `summary::Aggregator` 现算 4 指标 + 3 分类，写进 `DailyStats` 内存结构；`SummaryRepo::upsert_stats` 把这 5 列持久化到 `daily_summary` 表（兼容老库用 `ALTER TABLE ADD COLUMN`）；`get_today_summary` Command 透传给前端；Today.vue / History.vue 渲染。

**Tech Stack:**
- Rust: `serde`, `serde_json`, `rusqlite`（已有）；不加新依赖
- TS: 不加新依赖；复用 `keyCodeToGlyph` / `formatDateCN`
- 测试: `cargo test` + `vitest run` + `pnpm test:e2e`

**项目约定:** TDD 严格 Red→Green→Refactor，每个 Task 一个 commit。文件路径用绝对路径前缀 `E:/一人公司/技术部工作区/小玩具/FingerTip/`。

---

## 1. 公式定义（截图原文复刻）

### 1.1 核心特征四类

| 指标 | 原始数据 | 公式 | 阈值 |
|---|---|---|---|
| **密集度 (dynsity)** | 总按键数 / 有效输入时长（小时） | `total_keys / active_hours_count` | > 800 键/小时 = **快**；< 800 = **慢** |
| **平稳度 (stabilit)** | 每小时按键数变化 | 变异系数 = `stddev(hourly) / mean(hourly)` | <= 0.8 = **平稳**；> 0.8 = **跳跃** |
| **流畅度 (fluency)** | 超过 2 秒的间隔比例 | 长停顿比例 = `pauses_over_2s_count / total_intervals_count` | < 10% = **流畅**；> 10% = **停顿** |
| **活跃度 (activity)** | 总按键数和活跃时长 | 活跃小时数 = `count(hourly[i] > 0)` | > 4 小时 = **活跃**；< 4 = **不活跃** |

### 1.2 边界值处理

| 场景 | 处理 |
|---|---|
| `active_hours_count == 0` | `intensity = 0.0`（数学上除零） |
| `events.len() < 2` | `fluency = 0.0`（无间隔可算） |
| `mean(hourly) == 0` | `steadiness = 0.0`（变异系数分母为零） |
| `total_keys == 0` | 4 个指标全部 0.0（不写有意义数值） |

### 1.3 键位分类（3 类汇总）

| 类别 | key_code 范围 / 值 | 说明 |
|---|---|---|
| **game_keys**（游戏键） | `W(87) A(65) S(83) D(68) Space(32) Enter(13)` | 主要在游戏中使用 |
| **text_keys**（文本键） | `A-Z (65-90) + 0-9 (48-57)`，但 A/S/D/W 已被 game_keys 归类 | 文本输入 |
| **modifier_keys**（功能键） | `Backspace(8) Delete(46) Shift(16) Ctrl(17) Alt(18) Meta(91)` | 编辑 / 修饰 |

**注意：** A/S/D/W (65/83/68/87) 既属于 A-Z 字母范围也属于 WASD 游戏键，**优先归 game_keys**（防止双重计数）。text_keys 严格 `65-90` + `48-57` 减去已归 game_keys 的 65/83/68/87。

---

## 2. 数据契约

### 2.1 `summary::stats::DailyStats`（扩字段）

```rust
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
    pub intensity: f64,
    pub steadiness: f64,
    pub fluency: f64,
    pub activity_hours: i32,
    pub key_class_json: String,  // 序列化的 KeyClassSummary
}
```

### 2.2 `KeyClassSummary`（新增 struct，放 `summary/key_class.rs`）

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeyClassSummary {
    pub game_keys: i64,
    pub text_keys: i64,
    pub modifier_keys: i64,
}
```

### 2.3 `db::summary_repo::DailySummaryRow`（扩 5 字段）

```rust
pub struct DailySummaryRow {
    pub date: String,
    pub total_keys: i64,
    pub top_keys_json: String,
    pub theme_word: String,
    pub mood_word: Option<String>,
    // ====== v0.3.5 新增 5 字段 ======
    pub intensity: f64,
    pub steadiness: f64,
    pub fluency: f64,
    pub activity_hours: i32,
    pub key_class_json: String,
    pub created_at: i64,
}
```

---

## 3. DB Schema 迁移

### 3.1 `daily_summary` 表新结构

```sql
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
```

### 3.2 兼容老库（启动期 ALTER TABLE）

```rust
// migrations.rs::run_migrations 末尾追加：
let has_intensity: bool = conn
    .query_row(
        "SELECT COUNT(*) FROM pragma_table_info('daily_summary') WHERE name='intensity'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap_or(false);

if !has_intensity {
    conn.execute_batch("
        ALTER TABLE daily_summary ADD COLUMN intensity REAL NOT NULL DEFAULT 0.0;
        ALTER TABLE daily_summary ADD COLUMN steadiness REAL NOT NULL DEFAULT 0.0;
        ALTER TABLE daily_summary ADD COLUMN fluency REAL NOT NULL DEFAULT 0.0;
        ALTER TABLE daily_summary ADD COLUMN activity_hours INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE daily_summary ADD COLUMN key_class_json TEXT NOT NULL DEFAULT '{}';
    ")?;
}
```

**为什么需要 ALTER：** 当前项目没有 migration 版本号机制，每次启动跑 `run_migrations` 一次性建表。`CREATE TABLE IF NOT EXISTS` 不会给已存在的表加列，老用户升级到 v0.3.5 必须靠 ALTER。

---

## 4. 计算逻辑

### 4.1 `Aggregator::compute_metrics`（新函数）

```rust
impl Aggregator {
    pub fn compute_metrics(events: &[KeyEvent], hourly: &[usize; 24]) -> (f64, f64, f64, i32) {
        let active_hours = hourly.iter().filter(|&&c| c > 0).count();
        let total_keys = events.len();
        
        // intensity
        let intensity = if active_hours == 0 {
            0.0
        } else {
            total_keys as f64 / active_hours as f64
        };
        
        // steadiness = stddev / mean（变异系数）
        let non_zero: Vec<f64> = hourly.iter().map(|&c| c as f64).collect();
        let mean = if non_zero.is_empty() { 0.0 } else { non_zero.iter().sum::<f64>() / 24.0 };
        let variance = if non_zero.is_empty() {
            0.0
        } else {
            non_zero.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / 24.0
        };
        let stddev = variance.sqrt();
        let steadiness = if mean == 0.0 { 0.0 } else { stddev / mean };
        
        // fluency = pauses > 2s / total_intervals
        let total_intervals = if events.len() < 2 { 0 } else { events.len() - 1 };
        let pauses_over_2s = if events.len() < 2 {
            0
        } else {
            events.windows(2)
                .filter(|w| w[1].timestamp_ms - w[0].timestamp_ms > 2000)
                .count()
        };
        let fluency = if total_intervals == 0 { 0.0 } else { pauses_over_2s as f64 / total_intervals as f64 };
        
        (intensity, steadiness, fluency, active_hours as i32)
    }
}
```

### 4.2 `Aggregator::classify_keys`（新函数）

```rust
impl Aggregator {
    pub fn classify_keys(events: &[KeyEvent]) -> KeyClassSummary {
        let mut s = KeyClassSummary::default();
        for e in events {
            match e.key_code {
                // game_keys：WASD + Space + Enter
                87 | 65 | 83 | 68 | 32 | 13 => s.game_keys += 1,
                // modifier_keys：Backspace + Delete + Shift/Ctrl/Alt/Meta
                8 | 46 | 16 | 17 | 18 | 91 => s.modifier_keys += 1,
                // text_keys：A-Z (65-90) + 0-9 (48-57)，扣除 WASD
                48..=57 | 66..=90 | 69..=82 | 84..=90 => s.text_keys += 1,
                _ => {}  // 其它键（F1-F12, 方向键等）忽略
            }
        }
        s
    }
}
```

**注意：** Rust 模式 `48..=57 | 66..=90 | 69..=82 | 84..=90` 是为了**排除 WASD 的 ASCII 字母重叠**——A(65) S(83) D(68) W(87) 已被 game_keys 优先匹配，剩下的 A-Z 字母范围扣掉 65/68/83/87 等于 `66..=90 ∪ 69..=82 ∪ 84..=90`（但 66-90 已含 68/69/82/83/84 范围交集，需要仔细算）。

**简化方案：** 用 `if-else` 链而不是 match 范围，避免重叠歧义：

```rust
pub fn classify_keys(events: &[KeyEvent]) -> KeyClassSummary {
    let mut s = KeyClassSummary::default();
    for e in events {
        let code = e.key_code;
        // 优先级 1：game_keys（WASD + Space + Enter）
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
```

---

## 5. UI 改动

### 5.1 Today.vue（`src/views/Today.vue`）

**第 2 行 stats row**：从 4 卡 → **6 卡**：

```
[密集度 dynsity]   [平稳度 stabilit]
[流畅度 fluency]   [活跃度 activity]
[高峰按键]          [节奏指纹 Top5]（原 Top5 卡片下沉到这一行）
```

**第 3 行**：新增**键位分类水平条**（替换原 Top5 节奏指纹卡片 → 移到第 2 行末尾）。

### 5.2 History.vue（`src/views/History.vue`）

每个 day card 加 4 个小圆点：

```
┌─────────────────────────┐
│ 7 月 16 日               │
│ I · 303                  │
│ happy · 850 keys         │
│ ●  ●  ●  ●               │  ← 4 圆点
│ 快  稳  流  活            │  ← 4 标签（按阈值变色）
└─────────────────────────┘
```

颜色规则（按阈值）：
- intensity > 800 → 暖色，否则灰
- steadiness <= 0.8 → 绿，> 0.8 → 灰
- fluency < 10% → 绿，>= 10% → 灰
- activity_hours > 4 → 绿，否则灰

---

## 6. 测试策略

| 层 | 测试函数 | 覆盖场景 |
|---|---|---|
| `aggregator.rs` | `compute_metrics_intensity_with_active_hours` | 100 keys / 5 hours → intensity = 20 |
| `aggregator.rs` | `compute_metrics_intensity_zero_hours_returns_zero` | 0 events → intensity = 0.0 |
| `aggregator.rs` | `compute_metrics_steadiness_with_uneven_distribution` | uneven hourly → steadiness > 0.8 |
| `aggregator.rs` | `compute_metrics_steadiness_with_even_distribution` | even hourly → steadiness <= 0.8 |
| `aggregator.rs` | `compute_metrics_fluency_with_pauses` | 30 events, 5 pauses > 2s → fluency ~ 0.17 |
| `aggregator.rs` | `compute_metrics_fluency_zero_intervals_returns_zero` | 1 event → fluency = 0.0 |
| `aggregator.rs` | `classify_keys_game_only` | 纯 WASD → game_keys 全占 |
| `aggregator.rs` | `classify_keys_text_only` | 纯字母数字 → text_keys 全占 |
| `aggregator.rs` | `classify_keys_modifier_only` | 纯 Backspace/Delete → modifier_keys 全占 |
| `aggregator.rs` | `classify_keys_wins_over_text` | WASD 即使是 ASCII 字母也归 game_keys，不双计 |
| `stats.rs` | `daily_stats_round_trip_with_new_fields` | JSON 序列化 5 新字段保真 |
| `migrations.rs` | `fresh_db_has_all_5_new_columns` | 新库一次性建出 5 列 |
| `migrations.rs` | `old_db_gets_columns_via_alter` | 模拟老库（3 列 daily_summary）→ ALTER 后 5 列齐全 |
| `summary_repo.rs` | `upsert_stats_persists_5_new_columns` | upsert_stats → read_by_date 含 5 字段 |
| `summary_repo.rs` | `upsert_mood_preserves_new_columns` | upsert_mood 不动 5 字段（mood 单点管） |
| `summary_repo.rs` | `read_by_date_returns_default_for_old_data` | 老库读出 intensity = 0.0 等（默认值） |
| `commands.rs` | `get_today_summary_includes_5_new_columns` | 端到端 invoke |
| **Playwright e2e** | `today-page-shows-4-metrics` | 100 键 → 看到密集度 / 平稳度 / 流畅度 / 活跃度 4 数字 |
| **Playwright e2e** | `today-page-shows-key-class-bar` | 看到 game/text/modifier 3 段比例条 |
| **Playwright e2e** | `history-page-shows-4-color-dots` | History day card 4 小圆点 |

---

## 7. Commit 策略（每个 Task 一个 commit）

```
Task 1: feat(aggregator): compute_metrics + classify_keys (RED→GREEN)
Task 2: refactor(stats): DailyStats 扩 5 字段 + JSON round-trip 测试
Task 3: feat(db): daily_summary + 5 列 + 老库 ALTER 兼容
Task 4: refactor(summary_repo): DailySummaryRow 扩 5 字段 + upsert 同步
Task 5: chore(commands): get_today_summary 透传 5 字段（无需单测，透传）
Task 6: feat(ui): Today.vue 第 2 行 4→6 卡 + 键位分类条 + e2e
Task 7: feat(ui): History.vue day card 加 4 圆点 + e2e
```

每个 commit 完成后跑：`pnpm test` + `pnpm test:e2e` + `cargo test` + `pnpm typecheck`，确保全绿再进下一个。

---

## 8. 验证意图（测试要验"为什么"）

- `compute_metrics_*` 测**阈值边界**（800 / 0.8 / 10% / 4 小时），确保 UI 颜色编码不出错
- `classify_keys_wins_over_text` 测**优先级正确性**（WASD 不双计到 text_keys）
- `old_db_gets_columns_via_alter` 测**用户升级路径**（v0.3.4 老用户升级到 v0.3.5 不丢数据）
- `upsert_mood_preserves_new_columns` 测**v0.3.1 P0 #2 回归不破**（mood 单点管原则）
- Playwright e2e 测**真实用户视角**——4 指标真的在 UI 渲染、分类条比例对

---

## 9. 风险

| 风险 | 缓解 |
|---|---|
| ALTER TABLE 在低 SQLite 版本失败 | `rusqlite = "0.32"` 已 ≥ 3.35，ADD COLUMN 默认值支持 |
| 老库读出新列拿到 0.0 误显示 | 前端判断 `intensity === 0 && total_keys > 0` → 显示"—"而非"0" |
| Rust `match` 范围重叠编译错误 | 用 `if-else` 链避免 match 重叠歧义 |
| Today.vue 第 2 行 6 卡移动端挤压 | `grid-template-columns: repeat(auto-fit, minmax(140px, 1fr))` |
| WASD 优先级测试遗漏 | 显式写测试 `classify_keys_wins_over_text` |

---

## 10. theme_word 改造（v0.3.5 增量，回应 needs/ 修改意见.md 反馈）

**问题：** 当前 `extract_theme_word` 输出 `"X · N"` 格式（N = 按键总数），数字出现在卡片 hero 上对用户无意义。

**决策（2026-07-29 brainstorming 段 1 确认）：** theme_word 只保留最高频字母，**去掉数字**。

### 10.1 新公式

```rust
// summary/theme.rs
pub fn extract_theme_word(counts: &HashMap<u32, usize>) -> String {
    if counts.is_empty() {
        return String::new();
    }
    
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
        Some(k) => (k as u8 as char).to_string(),  // 仅字母，丢弃 ascii_total
        None => String::new(),
    }
}
```

**关键变化：** 删除 `ascii_total` 累加逻辑（原本只用于"是否非空"判断，改后 best_key.is_some() 已能区分）。

### 10.2 测试影响（theme.rs 现有 7 测试，3 个改期望值）

| 原测试 | 旧期望值 | 新期望值 |
|---|---|---|
| `top_key_with_count_returns_readable_summary` | `"h · 28"` | `"h"` |
| `filters_non_printable_keys` | `"a · 8"` | `"a"` |
| `high_freq_ina_no_longer_returns_ina_concat` | `"I · 303"` | `"I"` |
| `tie_breaks_by_smaller_key_code` | `"a · 10"` | `"a"` |
| `empty_counts_returns_empty_string` | `""` | `""`（不变） |
| `only_non_printable_returns_empty` | `""` | `""`（不变） |

### 10.3 下游影响（Music/Art 生成）

`commands.rs::generate_now` 把 `summary.theme_word` 喂给 `MusicPrompt.theme_word` / `ArtPrompt.theme_word`：

- `LocalMusicAdapter::compute_amplitudes` 用 `theme_word.bytes().sum() % 50` 做缩放因子
- 旧："I · 303" → 字符和 = 'I'(73) + ' '(32) + '·'(UTF8 multibyte) + '3'(51) + '0'(48) + '3'(51) → **UTF-8 多字节字符被错误算入**
- 新："I" → 字符和 = 73 → 缩放因子 = `73 % 50 / 100 + 0.75 = 0.98`

**附带好处：** UTF-8 多字节字符不再污染缩放因子，Music 输出**和按键分布更相关**（这是 needs/ 修改意见.md 第 3 条"主题词真影响生成"的隐含意图的一部分）。

### 10.4 老数据兼容

- 接受被覆盖：scheduler 跑过一次后 daily_summary.theme_word 全部重写为新格式
- 不需要 ALTER TABLE 或 schema 迁移——theme_word 本来就是 daily_summary 表的纯文本列
- History.vue 显示老 theme_word 时会看到 `"I · 303"`；新生成后变成 `"I"`（不一致窗口最多 60s）

### 10.5 Commit 拆分

```
Task 2.5: refactor(theme): extract_theme_word 去掉数字 + tests 同步
```

插入到 Task 1（aggregator）和 Task 2（DailyStats 扩字段）之间，作为独立 commit。

---

## 11. 不做的事（YAGNI）

- ❌ 不做"按小时段细分"（如"上午 / 下午 / 晚上"三个时段各自指标）—— 截图只要日级
- ❌ 不做"键分类条点击下钻"—— 当前只要展示，不需要详情页
- ❌ 不做"自适应阈值"（如"密集度阈值随用户基线调整"）—— 固定 800/0.8/10%/4h 足够
- ❌ 不做"指标趋势图"（如"过去 7 天密集度曲线"）—— History day card 一行圆点足够
- ❌ 不做"AI 解读指标"—— R3 阶段才会涉及 LLM