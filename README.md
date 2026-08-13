# FingerTip

> 后台记录键盘行为，次日产出 AI 音乐与数字画作

一个娱乐向的桌面应用，让你以新的方式"看见"自己的输入节奏——通过后台监听键盘敲击，次日生成专属于今天的 AI 音乐与抽象表现主义数字画作。

**核心价值**：娱乐性、自我观察、隐私优先。

---

## ✨ 核心特性

- 🎹 **键盘指纹**：5 维数据采集（按键顺序、敲击速度、停顿/删除/重复、时段分布、修改模式）
- 🎵 **AI 音乐**：结合心情词 + 偏好风格 + 主题词，生成当日专属音乐（首版纯音乐）
- 🎨 **数字画作**：抽象表现主义风格，把按键行为映射为色彩与形状
- 🛡️ **隐私优先**：原始数据不出本机；AI 仅接收抽象参数
- 🖥️ **后台常驻**：系统托盘常驻，不打扰日常使用

---

## 🚀 快速开始

### 前置要求

- **Node.js** 18+ + **pnpm** 9+
- **Rust** 1.77+ (edition 2021)
- **Windows 10/11** + **Visual Studio 2022 Build Tools** (C++ workload)
- **macOS**: Xcode Command Line Tools（架构预留，未实现）
- **Linux**: webkit2gtk-4.1（架构预留，未实现）

### 开发模式

```bash
pnpm install
cd src-tauri
cargo install tauri-cli --version "^2.0"
cd ..
pnpm tauri dev
```

应用窗口会弹出，可看到欢迎界面。

### 构建发布版

```bash
pnpm tauri build
```

产物路径：

- `src-tauri/target/release/bundle/msi/FingerTip_0.1.0_x64_en-US.msi`（Windows MSI）
- `src-tauri/target/release/bundle/nsis/FingerTip_0.1.0_x64-setup.exe`（Windows NSIS）

---

## 🧪 测试

### Rust 单元 + 集成测试

```bash
cd src-tauri
cargo test --workspace          # 全部（含 tauri_config / integration_hook_to_db）
cargo test --release --test perf # 性能基线（10 万事件聚合 < 5 秒）
```

### 前端测试

```bash
pnpm test --run    # Vitest 单元 + 路由
pnpm build         # 类型检查 + 构建
```

### Playwright E2E

```bash
pnpm exec playwright install    # 首次安装浏览器
pnpm exec playwright test       # 跑 E2E（自动启动 vite dev）
```

> E2E 测试在 Web 环境下跑：Tauri Command 调用会失败属预期（占位错误提示）。
> 完整 Tauri E2E 需 Tauri Driver，未在首版范围。

---

## 📁 文档

| 文档 | 路径 |
|---|---|
| 设计文档 | [`docs/plans/2026-07-16-fingertip-design.md`](docs/plans/2026-07-16-fingertip-design.md) |
| 精益画布（Jeff Gothelf v2） | [`docs/specs/lean-ux.md`](docs/specs/lean-ux.md) |
| 实施计划 | [`docs/plans/2026-07-16-fingertip-implementation.md`](docs/plans/2026-07-16-fingertip-implementation.md) |

---

## 🏗️ 架构概览

```
┌─────────────────────────────────────────────┐
│  表现层（Vue 3 + Naive UI）                 │
│  5 路由：Today / SubmitMood / History /      │
│         Settings / About                    │
└─────────────────────────────────────────────┘
                ↕ Tauri IPC
┌─────────────────────────────────────────────┐
│  应用层（Rust）                             │
│  HookListener → EventBuffer → Aggregator    │
│                                       ↓     │
│                          GenerateOrchestrator│
└─────────────────────────────────────────────┘
                ↕ Adapter 抽象
┌─────────────────────────────────────────────┐
│  数据层 + AI 抽象层                         │
│  - SQLite（key_events + daily_summary）     │
│  - MusicAdapter / ArtAdapter trait          │
│  - MiniMaxCloudAdapter stub                 │
└─────────────────────────────────────────────┘
```

---

## 🎯 首版验收清单

- ✅ pnpm build（前端 vue-tsc + vite build）
- ✅ cargo build（Rust 后端）
- ✅ cargo test --workspace（44 个测试全绿）
- ✅ pnpm test --run（5 个前端测试全绿）
- ✅ pnpm tauri dev（GUI 启动验证）
- ⚠️ pnpm tauri build（MSI/NSIS 打包 — 用户手动跑）
- ⚠️ 7 天自用实验（精益画布 Box 8 验证价值假设）

---

## 🛣️ 路线图

### 首版已完成（v0.1.0）
- 后台键盘监听 + 5 维聚合
- AI 音乐生成（MiniMax Cloud stub）
- AI 数字画作生成（MiniMax Cloud stub）
- 系统托盘常驻

### 后续规划
1. macOS / Linux 原生打包
2. 带歌词音乐（独立隐私评估后启用）
3. 历史可视化（热力图、趋势线）
4. 自定义 prompt 模板
5. 本地模型接入（MusicGen small / SD-Turbo）

---

## 🔐 隐私说明

- **原始事件**：仅存本机 SQLite（30 天后清理）
- **AI 调用**：只传抽象参数（心情词 + 风格 + 主题词 + 5 维统计 JSON），**不传原始键位序列**
- **API Key**：存于 OS Keyring（Windows Credential Manager / macOS Keychain）

---

## 📜 License

个人项目，未指定开源协议。