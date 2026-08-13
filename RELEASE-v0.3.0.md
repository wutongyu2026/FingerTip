# FingerTip v0.3.0

## 🎯 主题：Zero-Mock 真本地生成

v0.3.0 是相对 v0.2.x 的 **major 升级**。彻底清除 21-finding audit 中的 8 个必修 + 5 个重要，让"作品页是真的作品" + "心情真的影响生成" + "下载走原生 Save As"。

## 📊 Diff vs v0.2.5

| 维度 | 变化 |
|---|---|
| Rust 测试 | 111 → 130+ passed |
| 前端测试 | 68 → 77 passed |
| UI 渲染 | 占位 div → 真 canvas / 真波形 / 真时长 |
| 生成层 | stub 算法（哈希 seed） → mood/theme 真驱动 |
| 下载 | 浏览器 `<a download>` → Tauri 原生 Save As |
| mood 持久化 | 永远 NULL → 真写入 SQLite daily_summary |

## 🔥 主要修复

### 1. 后端抽象层重构

清理 v0.2.x 的 `MinimaxCloudAdapter` 全套死代码（placeholder API key + 只 log 不联网 + 返回 orphan 路径），引入 `MusicAdapter` / `ArtAdapter` async trait 抽象。

新的 factory 模式：

```rust
pub fn build_music_adapter() -> Box<dyn MusicAdapter> {
    if std::env::var("FINGERTIP_USE_CLOUD").is_ok_and(|v| v == "1") {
        Box::new(cloud::music::CloudMusicAdapter)
    } else {
        Box::new(local::music::LocalMusicAdapter)
    }
}
```

本地默认，Cloud 占位（v0.4 真接入）。

### 2. LocalMusic / LocalArt 真生成

- **music.amplitudes** 真值（`mood` 决定 BPM，`theme_word` 字符 hash 决定 amplitude 缩放）
- **art.pixels** 真值（`mood` 决定 HSV hue，`theme_word` 决定每像素偏移 + x/y 散布）
- 数学上保证：相同 input → 相同 output（deterministic）

### 3. 作品页占位 UI 全删

| 元素 | v0.2.x | v0.3.0 |
|---|---|---|
| 画作占位 | `<div>abstract / hello</div>` 永远显示 | `<canvas>` 真绘 64 像素 |
| 波形 bar | `4 + (i * 7 % 18)px` 硬编码公式 | `4 + amp * 36px`，amp ∈ [0, 1] |
| 时长 | `0:00 / 0:14 / 0:32` 写死 | `music.duration_ms` 真值 |

### 4. mood 持久化真链路

```
SubmitMood.vue 提交
  → invoke('set_mood', { date, mood })     ← 新 Tauri command
  → SummaryRepo::upsert_mood(date, mood)   ← 真写 daily_summary.mood_word
  → invoke('generate_now', { date, mood, style })
  → Music + Art 真生成
  → 写入 store, 跳转 /artworks
```

### 5. 下载走 Tauri 原生 Save As

| 变化点 | 详情 |
|---|---|
| 后端 deps | `tauri-plugin-dialog` + `tauri-plugin-fs` 加 Cargo.toml / package.json |
| 前端 util | `src/utils/download.ts` 提供 `downloadBlob(filename, blob, ext, mime)` + `ensureDefaultDir()` |
| Store | `downloadDir` 字段 + localStorage 持久化 |
| Settings UI | "下载输出目录" card：readonly 显示当前目录 + "浏览…" picker + "重置默认" |
| 首次启动 | App.vue onMounted 自动 `ensureDefaultDir()` → `%APPDATA%\com.fingertip.app\downloads\` |
| Capabilities | `dialog:default` + `fs:allow-write-file` + `fs:scope` allow list |

## 🔍 验证

| 检查项 | 结果 |
|---|---|
| `cargo test --lib` | **130+ passed** |
| `pnpm test` | **77 passed** (含 9 个新增 download.ts 单测) |
| `pnpm typecheck` | **0 error** |
| `pnpm tauri build` | NSIS + MSI 双 bundle (首次正式出包 in v0.2.5 + 重新生成于 v0.3.0) |

## 📜 Commit 链

```
[RELEASE-v0.3.0] docs + 版本同步
[0ec4fde] feat(download): Stage 5 Batch B — Settings picker + Artworks wires + 单测
[6393c40] feat(download): Stage 5 Batch A — download infra (Tauri plugin + store + util + startup)
[9f7b014] test(e2e): Stage 4 Batch 4 — E2E-A SubmitMood + E2E-B 真渲染 spec
[b1cf21b] feat(wiring): Stage 4 Batch 3 — set_mood invoke + Store 接通新 contract
[d834a39] chore(ui): Stage 4 Batch 2 — 删 Settings 死 radio + 简化 Today dataSource
[b8ef834] feat(artworks): Stage 4 Batch 1 — 真渲染: 画作 canvas + 波形读 amplitudes + 时长读 duration_ms
[19099e6] feat(generate/local): Stage 3 — LocalMusicAdapter/LocalArtAdapter 真生成 + e2e 集成
[d29814f] test(db): Stage 2 Task 2.3 — daily_summary.mood_word schema 验证
[b33663e] feat(commands): Stage 2 Task 2.2 - set_mood Tauri command
[506e732] feat(db): Stage 2 Task 2.1 — SummaryRepo::upsert_mood + EventRepo::list_by_date
[7eaeaa0] chore(generate): Stage 1 review fixes
[4095407] refactor(generate): Stage 1 后端 trait 重构 + 删 stub (merged)
[1ed8883] chore(deps): add async-trait for MusicAdapter/ArtAdapter trait
```

## 🚀 如何验证自启 + 真渲染

1. 装 `FingerTip_0.3.0_x64-setup.exe`（**先卸载 v0.2.x**，避免注册表残留指向旧 debug exe）
2. 安装时勾「开机自启」
3. **新功能**：点作品页下载按钮 → 弹原生 Save As → 默认目录是 `%APPDATA%\com.fingertip.app\downloads\`，文件名 `FingerTip-music-20260725-143210.wav`
4. 按键 5 分钟，提交心情"开心/Jazz" → 跳到 artworks，看到真实画作 + 真波形 32-40s + 心情标签
5. 重启电脑 → 看托盘图标是否出现（FingerTip 静默后台）

## ⚠️ 已知未修（v0.4 候选）

- `useTonePlayback.MusicParams` 与新 `Music` 不兼容（Stage 4 Batch 1 spec gap #2 留）
- `useCanvasRender.PixelParams` HSL 归一化坐标与新 `Art.pixels` 绝对 RGBA 不兼容（Stage 4 Batch 1 spec gap #3 留）
- v0.4: 真实云端 AI 厂商接入（musicgen / suno 等）