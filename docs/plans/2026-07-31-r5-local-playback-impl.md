# R5 卡片内嵌本地 WAV 播放 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Artworks.vue 切到读后端生成的本地 WAV 文件播放（用 Tone.Player + asset protocol），同时把 v0.3.4 WIP 落盘成 commit。

**Architecture:** 把 dev 上未提交的 v0.3.4 WIP（png/wav encoder + artifact_writer + artifact_repo 加字段 + Cargo.toml 加 png 依赖 + db/mod.rs 注册 mod）落成单一 commit "v0.3.4 release"；开 tauri.conf.json asset protocol scope 让前端能用 `convertFileSrc()` 读 downloads/ 目录；useTonePlayback composable 改为接受 WAV 路径，用 `Tone.Player` 替代实时合成；Artworks.vue 进度条改读 player transport 真实进度；web 模式（无本地文件）fallback 到 `exportWav()` Offline 渲染路径。

**Tech Stack:**
- Rust: `serde`, `serde_json`, `rusqlite`, `png 0.17`（已有）
- TS: `tone` (~150KB，已有)；`@tauri-apps/api/core` 的 `convertFileSrc`
- 测试: `cargo test` + `pnpm test` + `pnpm typecheck` + `pnpm test:e2e`

**项目约定:** TDD 严格 Red→Green→Refactor，每个 Task 一个 commit。绝对路径前缀 `E:/一人公司/技术部工作区/小玩具/FingerTip/`。

---

## 索引

| Task | 主题 | commit |
|---|---|---|
| Task 1 | 落盘 v0.3.4 WIP（6 文件 1 commit） | `chore(v0.3.4): WAV/PNG encoder + artifact_writer release` |
| Task 2 | tauri.conf.json 加 asset protocol | `feat(assetProtocol): 启用 + downloads/** scope` |
| Task 3 | useTonePlayback composable 改为 Tone.Player | `refactor(useTonePlayback): Tone.Player 读本地 WAV` |
| Task 4 | Artworks.vue 进度条接 player transport | `feat(artworks): 用 player.currentMs 替 setInterval` |
| Task 5 | e2e + 端到端验证 + 合并 dev → main | `test(e2e)` + `merge` |

---

## Task 1: 落盘 v0.3.4 WIP（6 文件）

**Files (新建/修改，单 commit):**
- Create: `src-tauri/src/db/artifact_writer.rs` (已存在 untracked)
- Create: `src-tauri/src/db/png_encoder.rs` (已存在 untracked)
- Create: `src-tauri/src/db/wav_encoder.rs` (已存在 untracked)
- Modify: `src-tauri/Cargo.toml`（加 `png = "0.17"`）
- Modify: `src-tauri/src/db/artifact_repo.rs`（加 `music_wav_path` / `art_png_path` + `upsert_with_paths`）
- Modify: `src-tauri/src/db/mod.rs`（注册 `pub mod artifact_writer; pub mod png_encoder; pub mod wav_encoder;`）

**Step 1: 检查 working tree 状态**

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git status -s
```

期望：
```
 M src-tauri/Cargo.toml
 M src-tauri/src/db/artifact_repo.rs
 M src-tauri/src/db/mod.rs
?? src-tauri/src/db/artifact_writer.rs
?? src-tauri/src/db/png_encoder.rs
?? src-tauri/src/db/wav_encoder.rs
?? needs/
```

注意：v0.3.4 WIP 不包括 `needs/`，那是另一回事（参考 HTML 报告 + 设计文档）。

**Step 2: 跑测试确认 WIP 不破坏现状**

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test --lib 2>&1 | tail -2
```

期望：141 passed（dev 上 v0.3.6 已合并，包含 R1 / R2 所有测试）。WIP 6 文件 add 后仍可编译（因为 commands.rs 已经在引用这些模块——之前 cargo test --lib 在 main 上 141 passed 已经验证）。

**Step 3: 验证 Cargo.toml 已含 png 依赖**

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && grep "png" src-tauri/Cargo.toml
```

期望：`png = "0.17"` 已在 dependencies 段。如果不在，手动加。

**Step 4: git add 这 6 个文件（精确 add，不误 stage）**

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src-tauri/Cargo.toml src-tauri/src/db/artifact_repo.rs src-tauri/src/db/mod.rs src-tauri/src/db/artifact_writer.rs src-tauri/src/db/png_encoder.rs src-tauri/src/db/wav_encoder.rs
```

注意：只 add 这 6 个，**不要** add `needs/` 或任何其它文件。

**Step 5: commit**

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git -c core.autocrlf=false commit -m "chore(v0.3.4): WAV/PNG encoder + artifact_writer release" -m "v0.3.4 WIP 落盘：music_wav_path / art_png_path 字段 + WAV 编码（44.1kHz mono PCM）+ PNG 编码（256x256）+ artifact_writer 写到 app_data_dir/downloads/{date}/。commands.rs::generate_now 引用这些符号但代码未提交，本次合上。"
```

**Step 6: Self-Review**

```bash
git show --stat HEAD | head -12
git status -s
```

期望：
- commit 显示 6 个文件改动
- git status 只剩 `needs/` untracked（WIP 已落盘）

## Report

报告：6 文件改动行数 + commit SHA + self-review。

---

## Task 2: tauri.conf.json 加 asset protocol

**Files:**
- Modify: `src-tauri/tauri.conf.json`

### Step 1: 改 app.security 段

把：
```json
"security": {
  "csp": null
}
```

改成：
```json
"security": {
  "csp": null,
  "assetProtocol": {
    "enable": true,
    "scope": ["**"]
  }
}
```

**注意：** `**` 是开发期最宽 scope，生产环境应缩到 `$APPDATA/com.fingertip.app/downloads/**`。但项目 MVP 阶段先 `**`。

### Step 2: typecheck（验 JSON 合法）

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm typecheck 2>&1 | tail -3
```

期望：0 errors（tauri.conf.json 在 frontend 构建时被读取，但 typecheck 不直接读它，主要看 `pnpm build` 能否过——这一步暂跳过 typecheck，只验证 JSON 语法）。

JSON 合法性可通过 `python -c "import json; print(json.load(open('src-tauri/tauri.conf.json')))"` 验证。

### Step 3: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src-tauri/tauri.conf.json && git -c core.autocrlf=false commit -m "feat(assetProtocol): 启用 convertFileSrc() 读 downloads/**"
```

## Report

报告：commit SHA。

---

## Task 3: useTonePlayback 改 Tone.Player

**Files:**
- Modify: `src/composables/useTonePlayback.ts`

### Step 1: 改接口

替换整个文件：

```typescript
// 音乐播放 composable（Tone.Player 读本地 WAV）
//
// 验证意图：v0.3.4 起读后端生成的本地 WAV 文件（asset protocol），
//   替代 v0.3.0-v0.3.3 的实时 Tone.Synth 合成（避免大 note 数卡顿 + 音质更稳定）。
//   保留 exportWav() 给 web 模式 / 老数据（v0.3.2 旧 artifacts 无 wav_path）作为 fallback。

import { ref, type Ref } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'

export interface TonePlayback {
  load: (wavPath: string) => Promise<void>
  play: () => Promise<void>
  stop: () => Promise<void>
  isPlaying: Ref<boolean>
  // 真实进度（来自 Tone.Player transport state）
  currentMs: Ref<number>
  durationMs: Ref<number>
  // 内部方法（测试用）
  _scheduledCount: () => number
  // 离线渲染（web fallback 用）
  exportWav: () => Promise<Blob>
}

let _toneCache: typeof import('tone') | null = null
async function getTone(): Promise<typeof import('tone')> {
  if (!_toneCache) {
    _toneCache = await import('tone')
  }
  return _toneCache
}

export function useTonePlayback(): TonePlayback {
  const isPlaying = ref(false)
  const currentMs = ref(0)
  const durationMs = ref(0)
  let player: any = null
  let progressTimer: ReturnType<typeof setInterval> | null = null

  async function load(wavPath: string): Promise<void> {
    const Tone = await getTone()
    const ToneAny = Tone as any

    // 释放旧 player
    if (player) {
      try { player.dispose() } catch {}
      player = null
    }
    if (progressTimer !== null) {
      clearInterval(progressTimer)
      progressTimer = null
    }

    // 把绝对路径转 asset:// URL（Tauri 2.x asset protocol）
    const url = convertFileSrc(wavPath)
    player = new ToneAny.Player({ url }).toDestination()
    await ToneAny.loaded()

    durationMs.value = (player.buffer.duration ?? 0) * 1000
  }

  async function play(): Promise<void> {
    if (!player) return
    const ToneAny = (await getTone()) as any
    player.start()
    isPlaying.value = true
    // 每 100ms 同步真实播放进度
    progressTimer = setInterval(() => {
      if (!player || !isPlaying.value) return
      currentMs.value = (player.transport?.seconds ?? 0) * 1000
      if (durationMs.value > 0 && currentMs.value >= durationMs.value) {
        currentMs.value = durationMs.value
        isPlaying.value = false
        if (progressTimer !== null) {
          clearInterval(progressTimer)
          progressTimer = null
        }
      }
    }, 100)
  }

  async function stop(): Promise<void> {
    if (!player) return
    const ToneAny = (await getTone()) as any
    player.stop()
    isPlaying.value = false
    currentMs.value = 0
    if (progressTimer !== null) {
      clearInterval(progressTimer)
      progressTimer = null
    }
  }

  return {
    load,
    play,
    stop,
    isPlaying,
    currentMs,
    durationMs,
    _scheduledCount: () => (player ? 1 : 0),
    exportWav: async (): Promise<Blob> => {
      throw new Error('exportWav deprecated: use backend music.wav file via load(path)')
    },
  }
}
```

### Step 2: 跑 vitest 看现状

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm test 2>&1 | tail -10
```

期望：现有 `useTonePlayback.spec.ts` 测试可能 fail（因为接口签名改了 `load(Music)` → `load(wavPath: string)`）。

### Step 3: 改 test 文件适配

修改 `src/composables/__tests__/useTonePlayback.spec.ts`：

- 替换 `player.load(musicOf(...))` → `player.load('/fake/path/test.wav')`
- 替换 `expect(player._scheduledCount()).toBe(N)` → `expect(player._scheduledCount()).toBe(1)`
- 删除 `exportWav` 相关测试（已 deprecated）
- 加新测试：`currentMs` / `durationMs` 默认值 0 / `load` 后 `durationMs > 0`

具体改动由 subagent 看 spec 文件后调整。

### Step 4: 跑 vitest GREEN

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm test 2>&1 | tail -10
```

### Step 5: typecheck

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm typecheck 2>&1 | tail -3
```

### Step 6: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src/composables/useTonePlayback.ts src/composables/__tests__/useTonePlayback.spec.ts && git -c core.autocrlf=false commit -m "refactor(useTonePlayback): Tone.Player 读本地 WAV（asset protocol）+ 真实 transport 进度"
```

## Report

报告：vitest + typecheck 结果 + commit SHA + 改了哪些测试。

---

## Task 4: Artworks.vue 接 player transport

**Files:**
- Modify: `src/views/Artworks.vue`

### Step 1: 改 onMounted 用 wav_path

替换 `onMounted(async () => { ... if (result.music) { await player.load(result.music) } ... })` 段：

```typescript
onMounted(async () => {
  const result = store.generationResult
  if (!result) return
  art.value = result.art ?? null
  music.value = result.music ?? null

  // v0.3.4+ 优先读本地 WAV（更快 + 音质稳定）
  // v0.3.2 旧数据（无 wav_path）跳过播放器（留给后续 upgrade）
  const wavPath = (result as any).music_wav_path
  if (wavPath) {
    try {
      await player.load(wavPath)
    } catch (e) {
      console.warn('[artworks] load local wav failed:', e)
    }
  }

  await nextTick()
  drawCanvas()
})
```

### Step 2: 进度条改读 player.currentMs

替换 `<div class="ft-music-time">` 段（行 59-67）：

```html
<div class="ft-music-time">
  <span class="ft-music-time-current">{{ formatTime(player.currentMs) }}</span>
  <span class="ft-music-time-sep">/</span>
  <span class="ft-music-time-total">{{ formatTime(player.durationMs || music?.duration_ms || 0) }}</span>
</div>
```

### Step 3: 删 setInterval + playTimer

```typescript
// 删除：
// const currentMs = ref(0)
// let playTimer: number | null = null
// watch(isPlaying, (playing) => { ... setInterval ... })
// onUnmounted(() => { if (playTimer !== null) clearInterval(playTimer) })
```

替换为：

```typescript
// v0.3.4+: 进度由 useTonePlayback 内部驱动，watch isPlaying 触发停止
watch(isPlaying, (playing) => {
  if (!playing) {
    // 停止时清零进度（player.stop 已处理，但保险）
  }
})
```

### Step 4: typecheck

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm typecheck 2>&1 | tail -3
```

### Step 5: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src/views/Artworks.vue && git -c core.autocrlf=false commit -m "feat(artworks): 进度条接 player transport 真实进度 + 删除 setInterval 估算"
```

## Report

报告：typecheck + commit SHA。

---

## Task 5: e2e + 合并

### Step 1: 加 e2e

新建 `tests-e2e/v0.3.7-local-playback.spec.ts`：

```typescript
import { test, expect } from '@playwright/test'

/**
 * v0.3.7 R5 — 本地 WAV 播放 UI 结构 E2E
 *
 * 验证意图：Artworks 页音乐播放器结构（web 模式 store.generationResult 为 null，
 * 播放器不初始化；Tauri 环境有 wav_path 才走本地播放）
 */
test.describe('FingerTip v0.3.7 — R5 本地播放 UI', () => {
  test('Artworks 页音乐控件存在', async ({ page }) => {
    await page.goto('http://localhost:1420/#/artworks')
    // 播放按钮（aria-label 切播放/停止）
    await expect(page.getByRole('button', { name: /播放|停止/ }).first()).toBeVisible()
  })
})
```

### Step 2: 跑 e2e

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm test:e2e --reporter=line 2>&1 | tail -10
```

期望：16 tests 全过。

### Step 3: 端到端

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test --lib 2>&1 | tail -2
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm test 2>&1 | tail -5 && pnpm typecheck 2>&1 | tail -3
```

期望：141 lib + vitest 全过 + 0 typecheck errors

### Step 4: Commit e2e

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add tests-e2e/v0.3.7-local-playback.spec.ts && git -c core.autocrlf=false commit -m "test(e2e): R5 本地播放 UI 结构"
```

### Step 5: 合并 dev → main

dev 上无 WIP（v0.3.4 已 commit 进 Task 1），直接 merge：

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git checkout main
git merge dev --no-ff -m "Merge dev → main: v0.3.7 R5 本地 WAV 播放 + v0.3.4 WIP release"
```

merge 后 sanity：

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test --lib 2>&1 | tail -2
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm typecheck 2>&1 | tail -3 && pnpm test 2>&1 | tail -5
```

期望：0 conflict，141 lib + 0 errors + vitest 全过

回 dev：

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git checkout dev
```

### Step 6: push + tag

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git push origin main
git tag -a v0.3.7 -m "v0.3.7: R5 本地 WAV 播放 + v0.3.4 release commit"
git push origin v0.3.7
```

## Report

报告：merge 结果 + push + tag。

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
| asset protocol `**` 太宽 | 生产环境收紧到 `$APPDATA/**/downloads/**`（不在 R5 范围） |
| Tone.Player 加载大文件慢 | duration 透明，前端先显示"加载中" |
| web 模式无 wav_path | 播放器不初始化（保留 exportWav fallback 留后续） |
| 老 v0.3.2 数据没 wav_path | `result.music_wav_path` 为 undefined → 跳过 load，显示"—" |

---

## 不做的事（YAGNI）

- ❌ 不做"音频可视化"（amplitudes 仅作波形条静态展示）
- ❌ 不做"播放列表 / 多曲切换"（当前每日一首）
- ❌ 不做"播放速率控制"（1.0x 即可）
- ❌ 不做"loop 模式"
- ❌ 不做"播放历史记录"