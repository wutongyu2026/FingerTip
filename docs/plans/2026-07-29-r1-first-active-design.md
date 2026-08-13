# R1 首活时间 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 把每日首活时间（首次按键 UTC 毫秒）落地到 daily_summary 表，Today.vue hero 显示"11:23"格式。

**Architecture:** 在 `Aggregator::aggregate` 现算 `first_active_ms = events.iter().map(|e| e.timestamp_ms).min().unwrap_or(0)`，加进 `DailyStats` 内存结构；`SummaryRepo` 三 INSERT/SELECT 同步新列；migrations ALTER 兼容老库（沿用 R2 模式）；Today.vue hero 加 1 个 meta row 显示"HH:mm"。

**Tech Stack:**
- Rust: `serde`, `serde_json`, `rusqlite`（已有）
- TS: 不加新依赖；复用 `store.timezoneOffsetMinutes` 时区偏移
- 测试: `cargo test` + `pnpm typecheck` + Playwright e2e

**项目约定:** TDD 严格 Red→Green→Refactor，每个 Task 一个 commit。绝对路径前缀 `E:/一人公司/技术部工作区/小玩具/FingerTip/`。

---

## 索引

| Task | 主题 | commit |
|---|---|---|
| Task 1 | aggregator + stats（加 first_active_ms 字段 + tests） | `feat(aggregator)` |
| Task 2 | migrations ALTER（+1 列 + 兼容测试 + 改 11→12 断言） | `feat(db)` |
| Task 3 | SummaryRepo 同步（11 → 12 列 + 2 tests） | `refactor(summary_repo)` |
| Task 4 | commands 透传测试 | `chore(commands)` |
| Task 5 | Today.vue hero 加首活时间 meta row | `feat(ui)` |
| Task 6 | e2e + 端到端验证 + 合并 dev → main | `test(e2e)` + `merge` |

---

## Task 1: aggregator + stats 加 first_active_ms

**Files:**
- Modify: `src-tauri/src/summary/stats.rs`
- Modify: `src-tauri/src/summary/aggregator.rs`

### Step 1: 改 DailyStats struct

`stats.rs` 现有 struct 末尾加：

```rust
    // ====== v0.3.6 新增 1 字段 ======
    /// 首活时间 first_active_ms：今日首次按键 UTC 毫秒（0 = 无事件）
    /// UI 端按 store.timezoneOffsetMinutes 偏移后显示 HH:mm
    pub first_active_ms: i64,
```

`stats.rs::DailyStats::empty()` 末尾加：

```rust
            // v0.3.6
            first_active_ms: 0,
```

`stats.rs::tests` 加 round-trip 测试：

```rust
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
```

### Step 2: 跑 RED

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test summary::stats
```

期望：编译失败（`DailyStats` 缺 `first_active_ms` 字段，`aggregate()` 字面量构造错）。

### Step 3: aggregator.rs aggregate() 加 1 行 + 构造列表加字段

`aggregator.rs::aggregate()` 函数体（`pub fn aggregate(date: String, events: &[KeyEvent]) -> DailyStats`）找到 `let (pauses, deletes, repeats) = Aggregator::count_meta(events, 2000);` 行后，加：

```rust
    // v0.3.6: 首活时间（events 为空时返 0 sentinel）
    let first_active_ms = events.iter().map(|e| e.timestamp_ms).min().unwrap_or(0);
```

在 `DailyStats { ... }` 构造字面量末尾加：

```rust
        first_active_ms,
```

### Step 4: 改 aggregator 测试（避免编译错）

`aggregator.rs::tests` 里**所有**构造 `DailyStats` 字面量的测试都要加 `first_active_ms: 0` 字段：

- `aggregate_produces_complete_daily_stats`（约 186 行附近）
- 其它如有用到 `DailyStats { ... }` 字面量的测试

`grep -n "DailyStats {" src-tauri/src/summary/aggregator.rs` 找全部位置。

### Step 5: 加首活时间专项测试

`aggregator.rs::tests` 末尾追加：

```rust
    #[test]
    fn aggregate_picks_first_active_ms_from_events() {
        let mut events = vec![];
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
```

### Step 6: 跑 GREEN

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test --lib
```

期望：133（旧）+ 2 新 stats 测试 + 2 新 aggregator 测试 = 137 全过。

### Step 7: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src-tauri/src/summary/stats.rs src-tauri/src/summary/aggregator.rs && git -c core.autocrlf=false commit -m "feat(aggregator): first_active_ms 字段 + 首活时间计算"
```

---

## Task 2: migrations ALTER 加 1 列 + 兼容测试

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`

### Step 1: 改 CREATE TABLE 块

`migrations.rs` 现有 CREATE TABLE `daily_summary` 段（行 24-37）末尾加：

```sql
            -- v0.3.6 新增
            first_active_ms INTEGER NOT NULL DEFAULT 0,
```

### Step 2: 加 ALTER 兼容分支

在现有 `if !has_intensity { ... ALTER 5 列 ... }` 块**之后**加：

```rust
    // v0.3.6: first_active_ms 列
    let has_first_active_ms: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('daily_summary') WHERE name='first_active_ms'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);

    if !has_first_active_ms {
        log::warn!("daily_summary 缺 first_active_ms 列（v0.3.6）—— 自动 ALTER TABLE ADD COLUMN");
        conn.execute_batch(
            "ALTER TABLE daily_summary ADD COLUMN first_active_ms INTEGER NOT NULL DEFAULT 0;"
        )?;
    }
```

### Step 3: 改老库测试断言 + 加新测试

`migrations.rs::tests` 现有 `old_db_gets_columns_via_alter` 测试（约 110-145 行）：

```rust
    // 变成 11 列  → 改成 12 列
    assert_eq!(count_columns(&conn, "daily_summary"), 12);
```

加新测试（追加到 tests 末尾）：

```rust
    #[test]
    fn fresh_db_has_first_active_ms_column() {
        let conn = fresh_db();
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('daily_summary')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert!(cols.iter().any(|c| c == "first_active_ms"), "missing column: first_active_ms");
    }

    #[test]
    fn old_db_with_first_active_ms_already_present_skips_alter() {
        // 验证意图：v0.3.6 已升级的库不应重复 ALTER（避免无意义的 schema 检查开销）
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE daily_summary (
                date TEXT PRIMARY KEY,
                total_keys INTEGER NOT NULL,
                top_keys_json TEXT NOT NULL,
                theme_word TEXT NOT NULL,
                mood_word TEXT,
                intensity REAL NOT NULL DEFAULT 0.0,
                steadiness REAL NOT NULL DEFAULT 0.0,
                fluency REAL NOT NULL DEFAULT 0.0,
                activity_hours INTEGER NOT NULL DEFAULT 0,
                key_class_json TEXT NOT NULL DEFAULT '{}',
                first_active_ms INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );"
        ).unwrap();

        // 跑 run_migrations —— 不应失败，列已存在
        run_migrations(&conn).unwrap();
        assert_eq!(count_columns(&conn, "daily_summary"), 12);
    }
```

### Step 4: 跑 GREEN

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test db::migrations
```

期望：3 旧 + 2 新 = 5 全过。

### Step 5: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src-tauri/src/db/migrations.rs && git -c core.autocrlf=false commit -m "feat(db): daily_summary 加 first_active_ms 列 + 老库 ALTER 兼容"
```

---

## Task 3: SummaryRepo 同步（11 → 12 列）

**Files:**
- Modify: `src-tauri/src/db/summary_repo.rs`

### Step 1: 改 DailySummaryRow + 4 个函数

`DailySummaryRow` struct 末尾加：

```rust
    pub first_active_ms: i64,
```

**4 个函数同步加列：**

1. `upsert` —— SQL INSERT 列表 + params!:
   ```sql
   INSERT OR REPLACE INTO daily_summary
   (date, total_keys, top_keys_json, theme_word, mood_word,
    intensity, steadiness, fluency, activity_hours, key_class_json,
    first_active_ms,
    created_at)
   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
   ```
   params 加 `stats.first_active_ms,`

2. `upsert_stats` —— 同上，ON CONFLICT SET 列表也要加 `first_active_ms = excluded.first_active_ms,`

3. `upsert_mood` —— 默认占位用 `0`：
   ```sql
   INSERT INTO daily_summary
   (date, total_keys, top_keys_json, theme_word, mood_word,
    intensity, steadiness, fluency, activity_hours, key_class_json,
    first_active_ms,
    created_at)
   VALUES (?1, 0, '[]', '', ?2, 0.0, 0.0, 0.0, 0, '{}', 0, ?3)
   ```

4. `read_by_date` + `list_all` + `list_recent` —— SELECT 列表加 `first_active_ms`，row.get(N) 加字段（位置在 `key_class_json` 后、`created_at` 前，所以 index 10）

### Step 2: 加测试

`summary_repo.rs::tests` 末尾追加：

```rust
    #[test]
    fn upsert_stats_persists_first_active_ms() {
        let conn = fresh_db();
        let mut events: Vec<KeyEvent> = vec![];
        for i in 0..10 {
            let mut e = KeyEvent::now((i % 26 + 65) as u32, "s".into(), 0);
            e.timestamp_ms = 1_753_401_600_000 + (i as i64) * 100;
            events.push(e);
        }
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert_stats(&stats, "hello").unwrap();

        let read = SummaryRepo::new(&conn).read_by_date("2026-07-29").unwrap().unwrap();
        assert_eq!(read.first_active_ms, 1_753_401_600_000);
    }

    #[test]
    fn upsert_mood_preserves_first_active_ms() {
        // v0.3.1 P0 #2 回归不破：mood 单点管
        let conn = fresh_db();
        let mut events: Vec<KeyEvent> = vec![];
        for i in 0..10 {
            let mut e = KeyEvent::now((i % 26 + 65) as u32, "s".into(), 0);
            e.timestamp_ms = 1_753_401_600_000 + (i as i64) * 100;
            events.push(e);
        }
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert_stats(&stats, "hello").unwrap();
        SummaryRepo::new(&conn).upsert_mood("2026-07-29", "happy").unwrap();

        let read = SummaryRepo::new(&conn).read_by_date("2026-07-29").unwrap().unwrap();
        assert_eq!(read.first_active_ms, 1_753_401_600_000, "mood upsert 不应覆盖 first_active_ms");
    }
```

### Step 3: 跑 GREEN

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test db::summary_repo
```

期望：11 旧 + 2 新 = 13 全过。

### Step 4: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src-tauri/src/db/summary_repo.rs && git -c core.autocrlf=false commit -m "refactor(summary_repo): DailySummaryRow 扩 first_active_ms + 3 upsert 同步"
```

---

## Task 4: commands 透传测试

**Files:**
- Modify: `src-tauri/src/commands.rs`

### Step 1: 加测试

`commands.rs::tests` 末尾追加：

```rust
    #[test]
    fn get_today_summary_includes_first_active_ms() {
        // 验证意图：get_today_summary 返回 JSON 包含 first_active_ms 字段
        let conn = fresh_db();
        let events = vec![KeyEvent::now(65, "s".into(), 0); 50];
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert(&stats, "hello", Some("happy")).unwrap();

        let row = get_today_summary_impl(&conn, "2026-07-29").unwrap().unwrap();
        let json = serde_json::to_string(&row).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["first_active_ms"].is_number());
    }
```

### Step 2: 跑 GREEN

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test commands::tests::get_today_summary_includes_first_active_ms
```

期望：直接过（auto via serde，impl 无需改）。

### Step 3: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src-tauri/src/commands.rs && git -c core.autocrlf=false commit -m "chore(commands): get_today_summary 透传 first_active_ms（无需新代码）"
```

---

## Task 5: Today.vue hero 加首活时间 meta row

**Files:**
- Modify: `src/views/Today.vue`

### Step 1: 改 DailySummaryRow interface

Today.vue 顶部 interface 末尾加：

```typescript
  // v0.3.6 新增
  first_active_ms: number
```

### Step 2: 改 hero 第 4 个 meta row

Today.vue hero `<dl class="ft-theme-meta-compact">` 段（行 10-23）末尾追加：

```html
        <div class="ft-meta-row">
          <dt>首活时间</dt>
          <dd>{{ firstActiveDisplay || '—' }}</dd>
        </div>
```

### Step 3: 加 firstActiveDisplay computed

Today.vue script 区末尾追加：

```typescript
// v0.3.6: 首活时间显示（HH:mm，按用户时区）
const firstActiveDisplay = computed(() => {
  const ms = summary.value?.first_active_ms
  if (!ms || ms === 0) return ''
  // 与 peakHour 同步：用 timezoneOffsetMinutes 把 UTC ms 移到用户时区
  const shifted = ms + store.timezoneOffsetMinutes * 60_000
  const d = new Date(shifted)
  const hh = String(d.getUTCHours()).padStart(2, '0')
  const mm = String(d.getUTCMinutes()).padStart(2, '0')
  return `${hh}:${mm}`
})
```

### Step 4: typecheck

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm typecheck
```

期望：0 errors

### Step 5: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src/views/Today.vue && git -c core.autocrlf=false commit -m "feat(ui): Today.vue hero 加首活时间 meta row（HH:mm，按用户时区）"
```

---

## Task 6: e2e + 端到端验证 + 合并 dev → main

**Files:**
- Modify: `tests-e2e/v0.3.5-r2-stats.spec.ts`（或新建 spec）

### Step 1: 加 e2e 测试

新建 `tests-e2e/v0.3.6-first-active.spec.ts`：

```typescript
import { test, expect } from '@playwright/test'

/**
 * v0.3.6 R1 首活时间 — UI 结构 E2E
 *
 * 验证意图：Today.vue hero 显示"首活时间"标签 + "HH:mm"占位
 * web 环境 invoke 抛错 → firstActiveDisplay 返空 → 显示"—"
 */
test.describe('FingerTip v0.3.6 — R1 首活时间 UI', () => {
  test('Today 页 hero 显示"首活时间"标签', async ({ page }) => {
    await page.goto('http://localhost:1420/#/')
    await expect(page.getByText('首活时间')).toBeVisible()
  })
})
```

### Step 2: 跑 e2e

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm test:e2e --reporter=line
```

期望：15 测试（14 旧 + 1 新）全过。

### Step 3: 端到端验证

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test --lib
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm test && pnpm typecheck
```

期望：137 lib + 80 vitest + 0 typecheck errors

### Step 4: Commit e2e

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add tests-e2e/v0.3.6-first-active.spec.ts && git -c core.autocrlf=false commit -m "test(e2e): R1 首活时间 UI 结构"
```

### Step 5: 合并 dev → main

⚠️ **关键：** dev 上有 v0.3.4 WIP（3 modified + 3 untracked）。先 stash 再 merge。

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git stash push -u -m "WIP: v0.3.4 png/wav encoders + artifact_writer" && git checkout main && git merge dev --no-ff -m "Merge dev → main: v0.3.6 R1 首活时间（first_active_ms 字段 + hero 显示）" && cargo test --lib 2>&1 | tail -2 && cd .. && pnpm typecheck 2>&1 | tail -3 && git checkout dev && git stash pop
```

期望：
- merge 0 conflict
- main 上 137 lib passed
- typecheck 0 errors
- dev 上 WIP 完整恢复

### Step 6: push + tag（可选）

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git push origin main
git tag -a v0.3.6 -m "v0.3.6: R1 首活时间 + R2 stats + theme_word 去数字"
git push origin v0.3.6
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
| 老库 ALTER 失败 | log warn 但不阻塞启动；前端拿到 0 显示"—" |
| `first_active_ms` 0 sentinel 与真实 0 冲突（极罕见，1970-01-01） | UI 显示"—"过滤 0 |
| 时区显示偏差 | 用与 peakHour 同公式（store.timezoneOffsetMinutes） |

---

## 不做的事（YAGNI）

- ❌ 不做"最后一次按键时间"（last_active_ms）—— spec 没要求
- ❌ 不做"按小时段细分活跃时间"
- ❌ 不做"周末/工作日差异化显示"
- ❌ 不做"首活时间点击下钻详情"

---

## 完成后

发 GitHub PR（合并时 `--no-ff` 保留详细历史）即可。发版本前需 bump `package.json` 和 `Cargo.toml` 的 0.3.0 → 0.3.6。