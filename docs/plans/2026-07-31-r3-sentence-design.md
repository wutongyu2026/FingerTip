# R3 Top5 按键→英文句子 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 从 daily_summary 的 Top 5 按键 + theme_word，本地算法生成一句英文短句；通过 Tauri Command 暴露给前端调用。

**Architecture:** 纯本地算法（不调外部 LLM；R3 v0.3.8 阶段；AI 接口留作未来扩展）。算法核心：每个 key_code 查本地词库（首选"首字母匹配"，不满足时回退"含该字母"）；5 top keys + 1 theme word → 拼接成自然短句。Tauri Command `generate_sentence(date)` 读 daily_summary，调算法，返 Sentence JSON。前端 Artworks.vue 在音乐/画作之后渲染句子块。

**Tech Stack:**
- Rust: `serde`, `serde_json`, `rusqlite`（已有）
- TS: 不加新依赖；复用 `store.generationResult` + `invoke`
- 测试: `cargo test` + `pnpm test` + `pnpm typecheck` + Playwright e2e

**项目约定:** TDD 严格 Red→Green→Refactor，每个 Task 一个 commit。绝对路径前缀 `E:/一人公司/技术部工作区/小玩具/FingerTip/`。

---

## 索引

| Task | 主题 | commit |
|---|---|---|
| Task 1 | sentence 模块（词库 + 算法 + WordSpec/Sentence struct + tests） | `feat(sentence)` |
| Task 2 | commands.rs 新 Command `generate_sentence`（透传 + tests） | `feat(commands)` |
| Task 3 | Artworks.vue 加句子面板（fetch + render） | `feat(artworks)` |
| Task 4 | e2e + 端到端验证 + 合并 dev → main + tag v0.3.8 | `test(e2e)` + `merge` |

---

## Task 1: sentence 模块

**Files:**
- Create: `src-tauri/src/generate/sentence.rs`
- Modify: `src-tauri/src/generate/mod.rs`（注册 mod）

### Step 1: 写失败测试

在 `sentence.rs` 末尾 `#[cfg(test)] mod tests`：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top5_keys_to_sentence_produces_5_words() {
        let top_keys = vec![(65, 50), (66, 30), (67, 20), (68, 15), (69, 10)]; // A B C D E
        let s = top5_keys_to_sentence(&top_keys, "hello");
        assert_eq!(s.words.len(), 6, "5 keys + 1 theme word");
        assert!(!s.text.is_empty(), "sentence text 不能空");
        // 首字母匹配规则：每个 key 对应的 word 必须以该字母开头
        let chars: Vec<char> = "ABCDE".chars().collect();
        for (i, ws) in s.words.iter().take(5).enumerate() {
            let expected = chars[i].to_ascii_lowercase();
            let word_lower = ws.word.to_lowercase();
            assert!(
                word_lower.starts_with(expected) || word_lower.contains(expected),
                "word '{}' 必须以 '{}' 开头或包含 '{}'",
                word_lower, expected, expected
            );
        }
    }

    #[test]
    fn top5_keys_to_sentence_includes_theme_word() {
        let top_keys = vec![(65, 10), (66, 8), (67, 6), (68, 4), (69, 2)];
        let s = top5_keys_to_sentence(&top_keys, "hello");
        let theme_word = s.words.iter().find(|w| w.key_code == 0).expect("theme word 存在");
        assert_eq!(theme_word.word, "hello");
    }

    #[test]
    fn top5_keys_to_sentence_handles_digits() {
        let top_keys = vec![(48, 10), (49, 8), (50, 6), (51, 4), (52, 2)]; // 0 1 2 3 4
        let s = top5_keys_to_sentence(&top_keys, "");
        assert_eq!(s.words.len(), 5);
        // digits 用 Zero/One/Two... 词
        for (i, expected) in ["Zero", "One", "Two", "Three", "Four"].iter().enumerate() {
            assert_eq!(s.words[i].word, *expected, "key {} should map to {}", i, expected);
        }
    }

    #[test]
    fn top5_keys_to_sentence_fallback_contains_letter() {
        // 假设未来词库扩展，验证 fallback 含字母规则
        // 现在词库完备，所有字母都首字母匹配；这条保留给未来
        let top_keys = vec![(87, 10), (65, 8), (83, 6), (68, 4), (32, 2)]; // W A S D Space
        let s = top5_keys_to_sentence(&top_keys, "");
        assert_eq!(s.words.len(), 5);
        // Space → "Space"（非字母键命名）
        let space_word = s.words.iter().find(|w| w.key_code == 32).expect("Space word");
        assert!(space_word.word.contains("Space") || space_word.word == "Space");
    }

    #[test]
    fn top5_keys_to_sentence_empty_input_returns_empty() {
        let top_keys: Vec<(u32, usize)> = vec![];
        let s = top5_keys_to_sentence(&top_keys, "");
        assert_eq!(s.words.len(), 0);
        assert_eq!(s.text, "");
    }

    #[test]
    fn top5_keys_to_sentence_caps_first_letter_of_text() {
        let top_keys = vec![(65, 10), (66, 8)];
        let s = top5_keys_to_sentence(&top_keys, "");
        // text 应以大写字母开头（自然句子）
        assert!(s.text.chars().next().unwrap().is_ascii_uppercase() || s.text.is_empty());
    }
```

### Step 2: 跑 RED

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test generate::sentence
```

期望：编译失败 "module `sentence` not found in `generate`"。

### Step 3: 注册 mod + 实现 sentence.rs

**3a. `src-tauri/src/generate/mod.rs` 加 `pub mod sentence;`**：

在 `pub mod local;` 后加：
```rust
pub mod sentence;
```

**3b. 整个 `src-tauri/src/generate/sentence.rs`**：

```rust
//! v0.3.8: Top 5 按键 + theme_word → 英文短句（纯本地算法）
//!
//! 验证意图：从 daily_summary.top_keys + theme_word 生成一句自然英文短句。
//! 规则（needs/ 修改意见.md 第 3 条）：
//!   - 每个按键对应一个英文单词
//!   - 首字母匹配：单词必须以按键对应字母开头
//!   - 首字母匹配失败时回退：单词包含该字母（不限制位置）
//!   - 单词组织成一句英文句子
//!
//! 设计：纯本地查表（不调外部 LLM；AI 接口预留为 v0.4+ 扩展）

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WordSpec {
    /// v0.3.8: 0 表示 theme_word（非按键）；其余为 key_code
    pub key_code: u32,
    pub word: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sentence {
    pub date: String,
    pub words: Vec<WordSpec>,
    /// 自然句子（首字母大写 + 末尾句号）
    pub text: String,
    pub created_at: i64,
}

/// 字母 → 英文单词（首选；首字母匹配）
fn letter_word(c: char) -> &'static str {
    match c {
        'a' | 'A' => "Apple",
        'b' | 'B' => "Banana",
        'c' | 'C' => "Cat",
        'd' | 'D' => "Dog",
        'e' | 'E' => "Eagle",
        'f' | 'F' => "Fox",
        'g' | 'G' => "Garden",
        'h' | 'H' => "Hello",
        'i' | 'I' => "Island",
        'j' | 'J' => "Joy",
        'k' | 'K' => "Kite",
        'l' | 'L' => "Light",
        'm' | 'M' => "Moon",
        'n' | 'N' => "Night",
        'o' | 'O' => "Ocean",
        'p' | 'P' => "Pear",
        'q' | 'Q' => "Quiet",
        'r' | 'R' => "River",
        's' | 'S' => "Star",
        't' | 'T' => "Tree",
        'u' | 'U' => "Universe",
        'v' | 'V' => "Violet",
        'w' | 'W' => "Wonder",
        'x' | 'X' => "Xylophone",
        'y' | 'Y' => "Yellow",
        'z' | 'Z' => "Zebra",
        _ => "Key",
    }
}

/// 数字键 → 英文单词
fn digit_word(c: char) -> &'static str {
    match c {
        '0' => "Zero",
        '1' => "One",
        '2' => "Two",
        '3' => "Three",
        '4' => "Four",
        '5' => "Five",
        '6' => "Six",
        '7' => "Seven",
        '8' => "Eight",
        '9' => "Nine",
        _ => "Number",
    }
}

/// 把 key_code 翻译成单个英文单词
/// 首字母匹配规则：key 是字母时，word 以该字母开头（满足）；不满足时回退"含该字母"
/// 非字母键（Space / Backspace / F1-F12）→ 用键名
fn word_for_key(key_code: u32) -> String {
    match key_code {
        // ASCII letters 65-90
        65..=90 => {
            let c = key_code as u8 as char;
            letter_word(c).to_string()
        }
        // digits 48-57
        48..=57 => digit_word(key_code as u8 as char).to_string(),
        // 特殊键：命名
        32 => "Space".into(),
        13 => "Enter".into(),
        8 => "Backspace".into(),
        46 => "Delete".into(),
        16 => "Shift".into(),
        17 => "Control".into(),
        18 => "Alt".into(),
        9 => "Tab".into(),
        27 => "Escape".into(),
        // 其它键（F1-F12 等） → 占位
        _ => format!("Key{}", key_code),
    }
}

/// 校验单词是否符合 "首字母匹配 OR 含字母" 规则
fn word_matches_key(word: &str, key_code: u32) -> bool {
    // 字母键
    if (65..=90).contains(&key_code) {
        let c = (key_code as u8 as char).to_ascii_lowercase();
        let word_lower = word.to_lowercase();
        return word_lower.starts_with(c) || word_lower.contains(c);
    }
    // 数字键
    if (48..=57).contains(&key_code) {
        let word_lower = word.to_lowercase();
        // 数字单词都唯一，无需 fallback
        return true;
    }
    // 非字母键：词名匹配即过
    true
}

/// 从 Top 5 按键 + theme_word 生成短句
pub fn top5_keys_to_sentence(
    top_keys: &[(u32, usize)],
    theme_word: &str,
) -> Sentence {
    let mut words: Vec<WordSpec> = Vec::new();

    // Top 5 按键
    for (code, _count) in top_keys.iter().take(5) {
        let word = word_for_key(*code);
        // 校验：极端情况（字母键的 word 不含该字母）时回退到基础形式
        let final_word = if word_matches_key(&word, *code) {
            word
        } else {
            // fallback：用 "Letter" 形式（含该字母）
            format!("{} Letter", (code as u8 as char).to_ascii_uppercase())
        };
        words.push(WordSpec { key_code: *code, word: final_word });
    }

    // theme_word 作为第 6 个"虚拟按键"
    if !theme_word.is_empty() {
        words.push(WordSpec { key_code: 0, word: theme_word.to_string() });
    }

    // 拼接 sentence：首字母大写 + 末尾句号
    let text = if words.is_empty() {
        String::new()
    } else {
        let joined: Vec<String> = words.iter().map(|w| w.word.to_lowercase()).collect();
        let mut s = joined.join(" ");
        // 首字母大写
        if let Some(first) = s.chars().next() {
            s = first.to_ascii_uppercase().to_string() + &s[1..];
        }
        // 末尾句号
        s.push('.');
        s
    };

    Sentence {
        date: String::new(), // 由 caller 填充
        words,
        text,
        created_at: chrono::Utc::now().timestamp_millis(),
    }
}

#[cfg(test)]
mod tests {
    // tests in Step 1 above
}
```

### Step 4: 跑 GREEN

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test generate::sentence
```

期望：6 测试全过。

### Step 5: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src-tauri/src/generate/sentence.rs src-tauri/src/generate/mod.rs && git -c core.autocrlf=false commit -m "feat(sentence): Top5 按键 + theme_word → 英文短句（本地算法）"
```

## Report

报告：6 测试结果、commit SHA。

---

## Task 2: commands.rs 新 Command generate_sentence

**Files:**
- Modify: `src-tauri/src/commands.rs`

### Step 1: 在 commands.rs 注册新 command

**a. `invoke_handler!` 块加 `commands::generate_sentence`**：

找到 `tauri::generate_handler![ commands::get_today_summary, ... ]` 块（约行 42-53），加：

```rust
            commands::generate_sentence,
```

**b. 加新 command 函数**：

```rust
/// v0.3.8: 从 daily_summary.top_keys + theme_word 生成英文短句
///
/// 内部：读 daily_summary → 解析 top_keys_json + theme_word → 调本地算法 → 返 Sentence JSON
///
/// 前端调一次拿到 Sentence，可缓存到 store.generationResult（不持久化，纯 UI 用）
#[tauri::command]
pub fn generate_sentence(state: State<'_, AppState>, date: String) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let row = SummaryRepo::new(&conn)
        .read_by_date(&date)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no summary for date {}", date))?;

    // 解析 top_keys_json: `[[65, 50], [66, 30], ...]`
    let top_keys: Vec<(u32, usize)> = serde_json::from_str(&row.top_keys_json)
        .map_err(|e| format!("parse top_keys_json: {}", e))?;

    let mut sentence = crate::generate::sentence::top5_keys_to_sentence(&top_keys, &row.theme_word);
    sentence.date = date;
    let json = serde_json::to_string(&sentence).map_err(|e| e.to_string())?;
    Ok(json)
}
```

### Step 2: 加测试

在 `commands.rs::tests` 末尾追加：

```rust
    #[test]
    fn generate_sentence_produces_text_from_summary() {
        let conn = fresh_db();
        let events = vec![KeyEvent::now(65, "s".into(), 0); 50]; // A
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert(&stats, "hello", Some("happy")).unwrap();

        let json = generate_sentence_impl(&conn, "2026-07-29").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["text"].as_str().unwrap().contains("Apple")); // A → Apple
        assert_eq!(parsed["date"], "2026-07-29");
    }

    #[test]
    fn generate_sentence_fails_for_missing_date() {
        let conn = fresh_db();
        let res = generate_sentence_impl(&conn, "2030-01-01");
        assert!(res.is_err());
    }
```

**c. 加 impl 函数（让测试不依赖 State）**：

```rust
pub fn generate_sentence_impl(conn: &Connection, date: &str) -> anyhow::Result<String> {
    let row = SummaryRepo::new(conn)
        .read_by_date(date)?
        .ok_or_else(|| anyhow::anyhow!("no summary for date {}", date))?;
    let top_keys: Vec<(u32, usize)> = serde_json::from_str(&row.top_keys_json)?;
    let mut sentence = crate::generate::sentence::top5_keys_to_sentence(&top_keys, &row.theme_word);
    sentence.date = date.to_string();
    Ok(serde_json::to_string(&sentence)?)
}
```

### Step 3: 跑 GREEN

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test commands::tests::generate_sentence
```

期望：2 测试全过。

### Step 4: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src-tauri/src/commands.rs && git -c core.autocrlf=false commit -m "feat(commands): generate_sentence Tauri Command（前端可调）"
```

## Report

报告：测试结果、commit SHA。

---

## Task 3: Artworks.vue 句子面板

**Files:**
- Modify: `src/views/Artworks.vue`
- Modify: `src/stores/app.ts`（可选：缓存 sentence 到 store）

### Step 1: 改 App.vue 或 Artworks.vue 调用

在 `Artworks.vue` `onMounted` 块（已改 Task 4 R5 后的版本）末尾，**追加 sentence 加载**：

```typescript
  await nextTick()
  drawCanvas()

  // v0.3.8: 加载 sentence（独立 invoke，不依赖 generate_now）
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const json = await invoke<string>('generate_sentence', { date: result.date ?? todayStr() })
    if (json) {
      const parsed = JSON.parse(json)
      sentence.value = parsed
    }
  } catch (e) {
    console.warn('[artworks] generate_sentence failed:', e)
  }
```

**b. 加 sentence ref + 类型**：

```typescript
import type { Art, Music } from '@/types/artwork'

interface Sentence {
  date: string
  words: Array<{ key_code: number; word: string }>
  text: string
  created_at: number
}

const sentence = ref<Sentence | null>(null)
```

### Step 2: 加 template 面板

在 Artworks.vue `<div class="ft-art-grid">` 段后**追加新段落**（v0.3.8 句子展示）：

```html
  <section v-if="sentence" class="ft-sentence-section ft-stagger ft-stagger-4">
    <div class="ft-panel">
      <div class="ft-panel-header">
        <div class="ft-panel-title">今日句子</div>
        <div class="ft-panel-meta">由 Top 5 按键 + 主题词生成</div>
      </div>
      <p class="ft-sentence-text">{{ sentence.text }}</p>
    </div>
  </section>
```

### Step 3: 加 CSS

```css
.ft-sentence-section {
  margin-top: var(--sp-6);
}
.ft-sentence-text {
  font-family: var(--font-hand);
  font-size: 32px;
  line-height: 1.4;
  color: var(--text-primary);
  margin: var(--sp-4) 0;
  padding: var(--sp-4) var(--sp-6);
  background: var(--bg-elevated);
  border-radius: var(--r-md);
  border-left: 3px solid var(--accent-warm);
}
```

### Step 4: typecheck

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm typecheck 2>&1 | tail -3
```

### Step 5: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src/views/Artworks.vue && git -c core.autocrlf=false commit -m "feat(artworks): 句子面板（Top5 按键 + 主题词生成的英文短句）"
```

## Report

报告：typecheck + commit SHA。

---

## Task 4: e2e + 端到端 + 合并 + tag

### Step 1: 加 e2e

新建 `tests-e2e/v0.3.8-sentence.spec.ts`：

```typescript
import { test, expect } from '@playwright/test'

/**
 * v0.3.8 R3 — 句子 UI 结构 E2E
 *
 * 验证意图：Artworks 页"今日句子"面板存在（web 模式 store.generationResult 为 null，
 * sentence 不渲染；UI 结构仍可见）
 */
test.describe('FingerTip v0.3.8 — R3 句子 UI', () => {
  test('Artworks 页"今日句子"标签存在', async ({ page }) => {
    await page.goto('http://localhost:1420/#/artworks')
    await expect(page.getByText('今日句子')).toBeVisible()
  })
})
```

### Step 2: 跑 e2e

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm test:e2e --reporter=line 2>&1 | tail -10
```

期望：17 tests（16 旧 + 1 新）全过。

### Step 3: 端到端

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test --lib 2>&1 | tail -2
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm test 2>&1 | tail -5 && pnpm typecheck 2>&1 | tail -3
```

期望：141+6+2 = 149 lib + 79 vitest + 0 errors

### Step 4: Commit e2e

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add tests-e2e/v0.3.8-sentence.spec.ts && git -c core.autocrlf=false commit -m "test(e2e): R3 句子 UI 结构"
```

### Step 5: 合并 dev → main

dev 上无 WIP。直接 merge：

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git checkout main
git merge dev --no-ff -m "Merge dev → main: v0.3.8 R3 Top5 按键→英文短句（本地算法 + Tauri Command）"
```

merge 后 sanity：

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test --lib 2>&1 | tail -2
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm typecheck 2>&1 | tail -3 && pnpm test 2>&1 | tail -5
```

期望：0 conflict + 全绿

回 dev：

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git checkout dev
```

### Step 6: push + tag

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git push origin main
git tag -a v0.3.8 -m "v0.3.8: R3 Top5 按键 → 英文短句（本地算法，Tauri Command 可调）"
git push origin v0.3.8
```

## Report

报告：merge + push + tag。

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
| 词库太简单（"Apple" / "Banana"）用户觉得不优雅 | 未来 v0.4+ 接入 LLM 时换实现；当前本地版标 MVP |
| theme_word 不在词库（用户输入 "xyz"） | 直接保留原文作为 word（满足"含字母"规则） |
| 5 键全是非字母键（F1-F12） | word_for_key 返回 "Key{N}"；UI 显示"Key112, Key113..." |
| 算法边界 case | 6 个测试覆盖常见场景；CI 跑全 lib 测试 |

---

## 不做的事（YAGNI）

- ❌ 不做 LLM 真实接入（v0.4+ 路线）
- ❌ 不做"句法正确性"语法检查（NLP 库依赖）
- ❌ 不做"单词释义"前端展示
- ❌ 不做"按日期保存 sentence 到 artifacts 表"（sentence 是纯 UI，无需持久化）

---

## 完成后

发 GitHub PR（`--no-ff` 保留详细历史）即可。发版前 bump `package.json` 和 `Cargo.toml` 0.3.0 → 0.3.8。