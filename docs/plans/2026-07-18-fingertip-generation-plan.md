# FingerTip 生成层 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 键盘行为 → 算法 → 1-2 分钟音频（Tone.js）+ 一张 PNG（Canvas）。Recalculate 一站式触发，自动跳转 Artworks。

**Architecture:** Rust 算 MusicParams / PixelParams（5 维 → 规则映射 + 4 风格预设），前端 Tone.js / Canvas 渲染。AI trait 保留不动。

**Tech Stack:**
- Rust: `serde`, `serde_json`, rusqlite（已有）；`anyhow`
- TS: `tone` (~150KB, 懒加载在 Artworks.vue)
- 测试: `cargo test` + `vitest run`

**项目约定:** TDD 严格 Red→Green→Refactor，每个 Task 一个 commit。文件路径用绝对路径前缀 `E:/一人公司/技术部工作区/小玩具/FingerTip/`。

---

## Phase A — 数据结构（Task 1-2）

### Task 1: MusicParams + MusicNote

**Files:**
- Create: `src-tauri/src/generation/music_params.rs`
- Create: `src-tauri/src/generation/mod.rs`

**Step 1: 写失败测试（RED）**

```rust
// src-tauri/src/generation/music_params.rs (顶部 #[cfg(test)] 块内)
use super::*;
use serde_json;

#[test]
fn music_params_round_trip() {
    let note = MusicNote { beat: 0.0, duration_beats: 1.0, pitch_midi: 60, velocity: 0.8 };
    let params = MusicParams {
        bpm: 90.0, key_root: "D".into(), scale: Scale::Dorian,
        notes: vec![note.clone()], seed: 42,
    };
    let json = serde_json::to_string(&params).unwrap();
    let back: MusicParams = serde_json::from_str(&json).unwrap();
    assert_eq!(back.bpm, 90.0);
    assert_eq!(back.scale, Scale::Dorian);
    assert_eq!(back.notes.len(), 1);
    assert_eq!(back.notes[0].pitch_midi, 60);
    assert_eq!(back.seed, 42);
}

#[test]
fn empty_music_params_serializes() {
    let p = MusicParams { bpm: 0.0, key_root: "A".into(), scale: Scale::MinorPentatonic, notes: vec![], seed: 0 };
    let j = serde_json::to_string(&p).unwrap();
    assert!(j.contains("\"notes\":[]"));
}
```

**Step 2:** `cd src-tauri && cargo test generation::music_params` → 编译失败（模块不存在）= RED

**Step 3: 写最简实现（GREEN）**

```rust
// src-tauri/src/generation/music_params.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Scale {
    MinorPentatonic, Dorian, HarmonicMinor, Major,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MusicNote {
    pub beat: f32,        // 在音乐中的拍位（0.0 = 起点）
    pub duration_beats: f32,
    pub pitch_midi: u8,   // MIDI note number (60 = middle C)
    pub velocity: f32,    // 0.0-1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicParams {
    pub bpm: f32,
    pub key_root: String,  // "A", "D", "C", "F" 等
    pub scale: Scale,
    pub notes: Vec<MusicNote>,
    pub seed: u64,         // for reproducibility
}

#[cfg(test)] mod tests { use super::*; /* 测试代码见 Step 1 */ }
```

```rust
// src-tauri/src/generation/mod.rs
pub mod music_params;
```

**Step 4:** `cargo test generation::music_params` → PASS
**Step 5:** `git add src-tauri/src/generation/ && git commit -m "feat(gen): MusicParams + MusicNote + Scale (Phase A Task 1)"`

---

### Task 2: PixelParams + Pixel

**Files:**
- Create: `src-tauri/src/generation/pixel_params.rs`
- Modify: `src-tauri/src/generation/mod.rs`（加 `pub mod pixel_params;`）

**Step 1: 失败测试**

```rust
// 在 pixel_params.rs 顶部
use super::*;
use serde_json;

#[test]
fn pixel_params_round_trip() {
    let pixel = Pixel { x: 0.5, y: 0.5, hue: 30.0, saturation: 0.8, lightness: 0.5, size: 12.0 };
    let params = PixelParams {
        width: 1920, height: 1080, background: "#FAF8F4".into(),
        palette_seed: 42, style_base: "abstract".into(),
        pixels: vec![pixel.clone()],
    };
    let j = serde_json::to_string(&params).unwrap();
    let back: PixelParams = serde_json::from_str(&j).unwrap();
    assert_eq!(back.width, 1920);
    assert_eq!(back.pixels[0].x, 0.5);
    assert_eq!(back.pixels[0].hue, 30.0);
}

#[test]
fn pixel_coordinates_normalized() {
    // 验证意图：x/y ∈ [0.0, 1.0] 是归一化坐标
    // 渲染时乘 width/height 得到实际像素
    let p = Pixel { x: 0.0, y: 1.0, hue: 0.0, saturation: 0.0, lightness: 0.0, size: 1.0 };
    assert!(p.x >= 0.0 && p.x <= 1.0);
    assert!(p.y >= 0.0 && p.y <= 1.0);
}
```

**Step 2:** 编译失败 = RED
**Step 3: 实现 PixelParams + Pixel**（结构：x/y 归一化坐标 0-1；hue 0-360；saturation/lightness 0-1）
**Step 4:** PASS → commit `"feat(gen): PixelParams + Pixel (Phase A Task 2)"`

---

## Phase B — 风格预设（Task 3）

### Task 3: StylePreset 4 套常量

**Files:**
- Create: `src-tauri/src/generation/style_presets.rs`
- Modify: `src-tauri/src/generation/mod.rs`（加 `pub mod style_presets;`）

**Step 1: 失败测试**

```rust
use super::*;
use crate::generation::music_params::Scale;

#[test]
fn preset_for_ambient_returns_known_config() {
    let p = preset_for("ambient").unwrap();
    assert_eq!(p.bpm_min, 60.0);
    assert_eq!(p.bpm_max, 80.0);
    assert_eq!(p.scale, Scale::MinorPentatonic);
    assert_eq!(p.key_root, "A");
    assert_eq!(p.palette_style, "abstract");
}

#[test]
fn preset_for_jazz_different_bpm_range() {
    let p = preset_for("jazz").unwrap();
    assert_eq!(p.bpm_min, 90.0);
    assert_eq!(p.bpm_max, 130.0);
    assert_eq!(p.scale, Scale::Dorian);
}

#[test]
fn preset_for_unknown_returns_ambient_default() {
    let p = preset_for("unknown-style-xyz");
    assert!(p.is_some(), "未知风格应 fallback 而非 None");
    assert_eq!(p.unwrap().scale, Scale::MinorPentatonic);
}

#[test]
fn preset_for_all_4_styles_distinct() {
    let styles = ["ambient", "jazz", "cinematic", "lo-fi"];
    let mut scales = std::collections::HashSet::new();
    for s in styles { scales.insert(format!("{:?}", preset_for(s).unwrap().scale)); }
    assert_eq!(scales.len(), 4, "4 风格应有 4 个不同 Scale");
}

#[test]
fn cinematic_uses_harmonic_minor() {
    let p = preset_for("cinematic").unwrap();
    assert_eq!(p.scale, Scale::HarmonicMinor);
}

#[test]
fn lofi_uses_major() {
    let p = preset_for("lo-fi").unwrap();
    assert_eq!(p.scale, Scale::Major);
}
```

**Step 2:** 编译失败 = RED
**Step 3: 实现 StylePreset + preset_for()**（用 match on &str，4 套配置见设计文档；unknown fallback 到 ambient）
**Step 4:** PASS → commit `"feat(gen): StylePreset 4 套预设 + preset_for (Phase B)"`

---

## Phase C — 核心映射（Task 4-10）

### Task 4: map_keys_to_music(empty) → 空序列

**Files:** Create `src-tauri/src/generation/mapper.rs` + mod 声明

**Step 1: 测试** `fn map_keys_to_music_empty_input_returns_no_notes() { ... }` 用空 Vec<KeyEvent>
**Step 3: 实现** `pub fn map_keys_to_music(...) -> Vec<MusicNote> { if events.is_empty() { return vec![]; } ... }` 写空 vec 即可
**Step 5:** commit `"feat(gen): map_keys_to_music stub (empty input)"`

### Task 5: map_keys_to_music(simple_sequence) → 音符

**Step 1: 测试** 模拟 5 个 KeyEvent（key_code 65, 66, 67, 68, 69，间隔 200ms），断言输出 5 个 MusicNote 且 pitch_midi 落在 C major 调式内
**Step 3: 实现** Scale 度数映射（min 5 个音）：MinorPentatonic = [0,3,5,7,10]、Dorian = [0,2,3,5,7,9,10]、Major = [0,2,4,5,7,9,11]、HarmonicMinor = [0,2,3,5,7,8,11] 半音偏移
**Step 5:** commit `"feat(gen): map_keys_to_music scale mapping"`

### Task 6: bpm_from_total_keys 公式

**Step 1: 测试** `fn bpm_from_total_keys(0) == 72`（默认） / `(100) >= 78` / `(1000) <= 132`（含 clamp 到 60-120）
**Step 3: 实现** `pub fn bpm_from_total_keys(total: u32) -> f32 { (60.0 + (total as f32 + 1.0).log2() * 12.0).clamp(60.0, 120.0) }`；默认值在 0 时返回 72
**Step 5:** commit `"feat(gen): bpm_from_total_keys 公式"`

### Task 7: pauses_emit_rests

**Step 1: 测试** 模拟键序列中含 gap > 2s → 在休息符位置插入 MusicNote { duration_beats: 1.0, pitch_midi: 0 (rest) }
**Step 3: 实现** 遍历事件，按 timestamp_ms 算间隔；> 2000ms 时累加当前 beat + 1
**Step 5:** commit `"feat(gen): pauses emit 1-beat rests"`

### Task 8: style_preset_application

**Step 1: 测试** map_keys_to_music 用 "jazz" style 时 bpm 落在 [90, 130] 区间，scale 是 Dorian
**Step 3: 实现** 在 map_keys_to_music 里调 preset_for(style)，用 bpm_min/bpm_max 算具体 bpm
**Step 5:** commit `"feat(gen): style preset applied in mapper"`

### Task 9: map_keys_to_pixels(basic)

**Step 1: 测试** 10 个事件（key_code 65-74）→ 10 个 Pixel，hue 在 [0, 360) 区间内，x/y 在 [0, 1] 归一化
**Step 3: 实现** 映射：x = (i % 24) / 24.0（径向散开）、hue = (key_code - 65) * 360 / 26 % 360
**Step 5:** commit `"feat(gen): map_keys_to_pixels basic mapping"`

### Task 10: pixel_density_from_frequency

**Step 1: 测试** key_code 65 (高频 100 次) vs key_code 75 (低频 5 次) → 前者 size > 后者
**Step 3: 实现** Pixel.size = base_size * sqrt(frequency / max_freq)
**Step 5:** commit `"feat(gen): pixel density from frequency"`

---

---

## Phase D — 编排（Task 11）

### Task 11: GenerationEngine::compute 完整管线

**Files:** Create `src-tauri/src/generation/engine.rs` + mod 声明

**Step 1: 失败测试**

```rust
use super::*;
use crate::generation::music_params::Scale;
use crate::hook::event::KeyEvent;

fn ev(code: u32, ts: i64) -> KeyEvent {
    KeyEvent { key_code: code, timestamp_ms: ts, session_id: "s".into(), modifiers: 0 }
}

#[test]
fn engine_compute_returns_both_params() {
    let events: Vec<KeyEvent> = (0..50).enumerate()
        .map(|(i, n)| ev(65 + (n % 26) as u32, 1_700_000_000_000 + (i as i64) * 200))
        .collect();
    let (music, pixels) = GenerationEngine::compute(
        "calm", "ambient", "hello", &events
    );
    assert!(!music.notes.is_empty());
    assert!(!pixels.pixels.is_empty());
    assert!(music.bpm > 0.0);
}

#[test]
fn engine_compute_empty_input_returns_ambient_default() {
    let (music, pixels) = GenerationEngine::compute("calm", "ambient", "hello", &[]);
    assert_eq!(music.notes.len(), 0);
    assert_eq!(music.bpm, 72.0); // fallback default
    assert_eq!(music.scale, Scale::MinorPentatonic);
    assert_eq!(pixels.style_base, "abstract");
}
```

**Step 3: 实现** struct GenerationEngine + `pub fn compute(mood, style, theme, events) -> (MusicParams, PixelParams)` 调 mapper::map_keys_to_music / map_keys_to_pixels + preset_for
**Step 5:** commit `"feat(gen): GenerationEngine.compute (Phase D)"`

---

## Phase E — Tauri Command（Task 12-13）

### Task 12: generate_now Command + 测试

**Files:** Modify `src-tauri/src/commands.rs`

**Step 1: 失败测试** 测 `pub fn generate_now_impl(conn, date) -> Result<(MusicParams, PixelParams), String>`（纯函数版，不依赖 State 便于单测）
**Step 3: 实现** 读 daily_summary + key_events，调 GenerationEngine::compute
**Step 5:** commit `"feat(commands): generate_now impl (TDD pure fn)"`

### Task 13: lib.rs 注册新 command

**Files:** Modify `src-tauri/src/lib.rs`（generate_handler! 列表加 `commands::generate_now`）
**Step 1:** 实际无新测试（行为由 Task 12 覆盖）；改 `[tauri::command]` wrapper `pub fn generate_now(state: State<AppState>, date: String, mood: Option<String>) -> Result<String, String>`
**Step 2:** `cd src-tauri && cargo build` → 通过
**Step 5:** commit `"feat(commands): register generate_now (Phase E done)"`

---

## Phase F — 前端合成层（Task 14-15）

### Task 14: useTonePlayback composable

**Files:** Create `src/composables/useTonePlayback.ts`

**Step 1: 失败测试** `src/composables/__tests__/useTonePlayback.spec.ts`（mock Tone.js）验证：
- 给定 MusicParams（5 个 MusicNote），composable 内部把 notes 转成 Tone.js Transport 调度
- 调 play() → 触发 start 回调；调 stop() → 触发 stop
**Step 3: 实现** composable 内部 lazy import `tone`，用 `Tone.Transport` 调度 notes；导出 `play / stop / isPlaying / progress` ref
**Step 4:** `pnpm test --run` → PASS
**Step 5:** commit `"feat(fe): useTonePlayback composable + mock test (Phase F-1)"`

### Task 15: useCanvasRender composable

**Files:** Create `src/composables/useCanvasRender.ts` + `src/composables/__tests__/canvasMock.ts`
**Step 1: 失败测试** 验证：给定 PixelParams（10 个 Pixel），composable 内部按顺序在 OffscreenCanvas 上 fillRect（hsl + alpha），最后导出 PNG dataURL
**Step 3: 实现** OffscreenCanvas 绘制循环（hue/saturation/lightness 来自 Pixel，size 来自 Pixel.size），导出 `canvas.toDataURL('image/png')`
**Step 4:** PASS
**Step 5:** commit `"feat(fe): useCanvasRender composable + canvas mock (Phase F-2)"`

---

## Phase G — UI 集成（Task 16-17）

### Task 16: Today.vue OnRecalculate 扩展

**Files:** Modify `src/views/Today.vue`
**Step 1: 测试** `src/views/__tests__/Today.spec.ts`（已有 router 路由 stub）模拟 Recalculate 点击 → 调 generate_now + 存 store + router.push('/artworks')
**Step 3: 实现** 在现有 onRecalculate() 末尾追加：调 invoke<string>('generate_now', {date, mood: ...}) 解析 → store.params = ... → router.push('/artworks')
**Step 5:** commit `"feat(fe): Today.vue triggers generate_now + auto-navigate (Phase G-1)"`

### Task 17: Artworks.vue 渲染 params

**Files:** Modify `src/views/Artworks.vue`
**Step 1: 测试** 模拟 store 有 MusicParams/PixelParams，组件渲染出 canvas 元素 + play 按钮（mock composables）
**Step 3: 实现** onMounted 从 store 读 params + 调 useTonePlayback.play / useCanvasRender.render；提供 play/stop toggle 按钮 + 渲染 canvas preview
**Step 5:** commit `"feat(fe): Artworks.vue renders params (Phase G-2)"`

---

## Phase H — E2E（Task 18）

### Task 18: Playwright E2E

**Files:** Modify `tests-e2e/daily-flow.spec.ts`（已有）加新测试：`recalculation_triggers_artwork_and_audio`
**Step 1:** 写 spec：点 Recalculate → 等待 URL 跳到 /artworks → 断言 canvas 元素 + play 按钮存在
**Step 2:** 跑 `pnpm playwright test` → 已有 Playwright 配置应该 pick up
**Step 3: 实现** mock Tauri Command 用 `@tauri-apps/api/mocks` 或页面 fixture 注入生成结果（避免真实 AI）
**Step 5:** commit `"test(e2e): Recalculate → Artworks 渲染闭环 (Phase H)"`

---

## 验证清单（实施完后逐项打勾）

- [ ] `cd src-tauri && cargo test --workspace` → 75 + 18 = **93+ tests pass**
- [ ] `pnpm test --run` → 6 + 4 = **10 frontend tests pass**
- [ ] `pnpm build` → OK
- [ ] `pnpm tauri dev` 启动 → 点 Recalculate → 几秒内跳到 Artworks
- [ ] Artworks 显示画作 + 播放器可 play
- [ ] 4 风格按钮（ambient/jazz/cinematic/lo-fi）切换后 Recalculate 产出不同风格

## 风险节点

- **Phase F（前端）首次跑测试时**：vitest 配置可能没有支持 `__tests__` 目录的别名 → 需要在 `vitest.config.ts` 加 `resolve.alias['@tonejs/...']` 或用 `vi.mock`
- **Tone.js 体积**：150KB+ → 必须在 Artworks.vue 才 import（懒加载），不能在全局
- **AI trait 不动**：必须严格不动 MusicAdapter/ArtAdapter trait 的方法签名（即使加新字段，AI 实现可能要同步改）

## Git 工作流

每个 Task 一个 commit（按 commit message 模板）。Phase 间可加 `git tag phase-X-done` 方便回溯。

Plan 终止状态：所有 18 Task 完成后，进入 `superpowers:executing-plans` 执行（建议选项：1=subagent-driven 当前会话  2=parallel session 新 worktree）。
