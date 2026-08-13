# FingerTip 设计文档

| 项目 | FingerTip |
|---|---|
| 类型 | 娱乐向桌面应用 |
| 日期 | 2026-07-16 |
| 状态 | 已通过 brainstorming 阶段，进入实施规划 |
| 平台 | Windows 优先 + macOS 架构预留 |
| 框架 | Tauri（Rust 后端 + Webview 前端） |

---

## 一、项目背景与目标

### 1.1 一句话定义
FingerTip 是一个后台静默运行的桌面应用，**记录用户敲击键盘的行为数据**，在次日产出**当日键盘总结 + AI 音乐 + 数字画作**，让用户以娱乐方式"看见"自己一天的输入节奏。

### 1.2 核心价值主张
- **娱乐性**：把日复一日的键盘敲击变成可消费的音乐与画作
- **自我观察**：用户从未有机会量化"自己一天都在打什么字、按多快、什么时候最忙"
- **隐私优先**：原始数据永远不出本机；AI 只接收抽象统计参数

### 1.3 目标用户与分发
- 首版：个人使用 / 加密狗模式（无需账号、无应用商店）
- 后续：视用户反馈决定是否公开发布

---

## 二、需求摘要（已通过头脑风暴澄清）

### 2.1 输入
- **键盘行为数据（5 维）**：
  1. 按键顺序与各按键使用次数
  2. 敲击速度、间隔和连续输入时长
  3. 停顿、删除、修改与重复输入
  4. 不同时段的输入频率变化
  5. （预留：按键 modifier 组合）
- **每日心情词**：用户主动提交（一词描述当天）
- **偏好风格**：用户在设置页选择（音乐风格 / 画作风格）

### 2.2 输出
- **键盘总结**：按键次数、占比、时段分布
- **主题词**：从高频键位算法提取的"词"
- **AI 音乐**（首版纯音乐）：MP3 文件
- **数字画作**（静态 PNG/JPG，抽象表现主义风格）
- **后续拓展**：带歌词音乐（需显式开启 + 隐私确认）

### 2.3 关键决策记录

| 维度 | 决策 | 理由 |
|---|---|---|
| 数据存储 | 纯本地（SQLite） | 用户隐私 + 离线可用 |
| AI 调用 | 混合模式（云 API + 本地模型可选） | 用户在设置里切换，灵活 |
| 云 API 选型 | MiniMax 多模态 | 与既有 skill 生态对齐 |
| 首版平台 | Windows 优先 | rdev Hook 在 Windows 实现成熟 |
| 运行形态 | 后台常驻 + 系统托盘 | 贴合"全天记录"初衷 |
| 画作呈现 | 静态 PNG/JPG | 复杂度适中，便于分享 |
| 画作风格 | 抽象表现主义 | 贴近情绪表达 |
| 音乐形态 | 首版仅纯音乐 | 降低首版风险 |
| 前端栈 | Vue 3 + TS + Vite + Naive UI | 中文社区活跃、模板语法直观 |
| 数据存储 | SQLite + 本地 JSON | 工业级本地存储事实标准 |
| AI 架构 | Adapter 抽象层 | 让"混合模式"自然落地 |
| 键盘 Hook | Rust `rdev` crate | 跨平台 Hook 标准 |

---

## 三、架构总览

### 3.1 分层

```
┌─────────────────────────────────────────────────────────┐
│  表现层（Vue 3 + Naive UI，Webview）                     │
│  - 主窗口：当日总结 / 心情提交 / 生成按钮                │
│  - 设置页：偏好风格、AI Key、本地/云模式切换             │
│  - 托盘菜单：快速查看当日统计                            │
└─────────────────────────────────────────────────────────┘
                       ↕ Tauri IPC（事件 + 命令）
┌─────────────────────────────────────────────────────────┐
│  应用层（Rust，Tauri 后端）                              │
│  ┌────────────┐ ┌────────────┐ ┌──────────────────────┐ │
│  │ Hook       │ │ Summary    │ │ Generate             │ │
│  │ Listener   │→│ Aggregator │→│ Orchestrator         │ │
│  └────────────┘ └────────────┘ └──────────────────────┘ │
│       ↓                ↓                ↓                │
│  原始事件流        统计聚合           生成管线            │
└─────────────────────────────────────────────────────────┘
                       ↕ Adapter 接口
┌─────────────────────────────────────────────────────────┐
│  数据层                                                  │
│  - SQLite：键盘事件 / 日总结 / 心情词 / 设置             │
│  - 本地 JSON：用户偏好 / API Key 加密                    │
│  - 生成产物：~/FingerTip/outputs/{date}/                 │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│  AI 抽象层（横切关注点）                                 │
│  - MusicAdapter（trait）：MiniMaxCloud / LocalModel       │
│  - ArtAdapter（trait）：MiniMaxCloud / LocalSD            │
│  - 切换由用户在设置里选定                                │
└─────────────────────────────────────────────────────────┘
```

### 3.2 核心组件清单

| 组件 | 职责 | 关键依赖 |
|---|---|---|
| `HookListener` | 全局键盘监听，按去抖/合并后写入事件流 | `rdev`, `tauri::AppHandle` |
| `EventBuffer` | 内存环形缓冲 + 5 分钟刷盘到 SQLite | `tokio`, `rusqlite` |
| `SummaryAggregator` | 每日 0:05 触发，统计 5 维数据并产出"键位主题词" | `rusqlite` |
| `GenerateOrchestrator` | 接收心情词 + 偏好 + 摘要，调度 AI 抽象层 | `MusicAdapter`, `ArtAdapter` |
| `PrivacyVault` | API Key 加密存储（OS Keyring via `keyring` crate） | `keyring` |
| `TrayManager` | 系统托盘图标 + 右键菜单 | `tauri::tray` |
| `Frontend` | Vue 3 SPA，5 个页面：今日总结 / 历史 / 设置 / 关于 / 提交心情 | `naive-ui`, `pinia` |

---

## 四、数据流

### 4.1 端到端流程

```
[用户敲键盘]
    ↓
[HookListener 监听] ← rdev 抓取 key_down 事件
    ↓ (去抖：合并 < 50ms 重复键)
[EventBuffer 内存队列]
    ↓ (每 5 分钟 / 队列满 1000 条 / 应用退出时)
[SQLite: key_events 表]
    - key_code, timestamp_ms, session_id, duration_ms, modifiers
    ↓
[每日 00:05 触发 SummaryAggregator]
    - 按键次数 (COUNT GROUP BY key_code)
    - 占比 (COUNT / TOTAL)
    - 敲击速度 (events_per_minute)
    - 停顿 (gap > 2s 计数)
    - 删除/修改 (backspace, delete, ctrl+z 计数)
    - 时段分布 (按小时分桶)
    ↓
[SQLite: daily_summary 表]
    - date, total_keys, top_keys_json, theme_word, mood_word, ...
    ↓
[用户在主窗口提交心情词 + 选择生成]
    ↓
[GenerateOrchestrator.orchestrate(date, mood)]
    ↓ 构造 prompt 输入
    1. 主题词提取：top_keys 取最高频 3-5 个键
       → 算法：按键→字母映射 + 中文词库（首版用内置 100 词词典）
    2. 音乐 prompt：{心情词} + {偏好风格} + {主题词描述} + 节奏提示
       → MusicAdapter.generate_music(prompt) → MP3 文件路径
    3. 画作 prompt：抽象表现主义模板 + 5 维数据映射描述
       → ArtAdapter.generate_art(prompt) → PNG 文件路径
    ↓
[输出目录：~/FingerTip/outputs/YYYY-MM-DD/]
    ├── summary.json
    ├── theme_word.txt
    ├── music.mp3
    └── artwork.png
    ↓
[主窗口展示 + 允许保存/分享]
```

### 4.2 关键数据契约

```rust
struct KeyEvent {
    id: i64,                  // SQLite 自增
    key_code: u32,            // rdev 虚拟键码
    timestamp_ms: i64,        // Unix 毫秒
    session_id: String,       // 应用启动 UUID
    duration_ms: Option<i32>, // 按下→松开间隔（rdev 不直接给，预留字段）
    modifiers: u8,            // 位掩码：Shift/Ctrl/Alt/Meta
}
```

### 4.3 数据保留策略
- **原始事件**：保留 30 天，超期聚合到日总结后删除（保护隐私 + 控制库大小）
- **日总结**：永久保留
- **生成产物**：永久保留（用户主动删除除外）

---

## 五、错误处理

### 5.1 错误分类与处理

| 错误类别 | 例子 | 处理策略 | 用户感知 |
|---|---|---|---|
| **键盘监听失败** | rdev hook 安装失败（无权限） | 启动时检测，提示用户授权 | 红色横幅："未获键盘权限，请重启并允许" |
| **SQLite 写入失败** | 磁盘满 / 锁竞争 | 写入重试 3 次，失败则事件降级到本地 JSON 文件兜底 | 不打断记录，但托盘图标变黄 |
| **AI 生成失败** | 网络超时 / Key 无效 / 配额耗尽 | 自动降级：云端失败 → 试本地模型；本地无 → 用占位图 + 默认音频 | 主窗口顶部"生成失败"提示 + 失败原因 |
| **API Key 缺失** | 首次启动未配置 Key | 拦截 AI 调用，提示用户去设置页配置 | 主窗口"AI 服务未配置"占位卡 |
| **每日总结失败** | 聚合过程中崩溃 | 下次启动时检测未完成日期，重新执行 | 静默重试，最多重试 3 次 |
| **生成产物写入失败** | 输出目录无写权限 | 提示用户选择新目录 | 模态对话框 |

### 5.2 关键设计原则

1. **键盘监听永不阻塞**：事件丢失可接受（去抖合并可缓解），但绝不能因为监听逻辑卡住影响用户正常使用电脑
2. **降级优于失败**：每个核心功能都至少有一条降级路径（云→本地、音乐失败→纯画作、画作失败→主题词海报）
3. **隐私敏感路径优先保护**：所有"原始键位事件流"必须在写入磁盘前经过"内容过滤层"（首版默认不过滤，但接口预留，可在设置里加入密码管理器/银行 App 排除规则）
4. **错误上下文日志化**：所有错误写入 `~/FingerTip/logs/app.log`（按日滚动，保留 14 天），便于排查

### 5.3 隐私边界

- 用户在设置里**显式开关**才能启用"歌词模式"（拓展功能），开关打开时必须勾选隐私确认："我理解高频键位组合可能包含个人敏感信息，将被用于歌词生成"
- AI 调用 payload 在写入日志前**脱敏**（只记录字段名，不记录内容）

---

## 六、测试策略

### 6.1 测试分层

| 层级 | 工具 | 覆盖目标 | 关键验证点（验证意图，不止验证行为） |
|---|---|---|---|
| **Rust 单元测试** | `cargo test` | `SummaryAggregator`、`PrivacyVault`、prompt 构建器 | 验证聚合算法正确性（top_keys 排序、theme_word 提取）、API Key 加解密往返 |
| **Rust 集成测试** | `cargo test --test *` | `HookListener` 模拟事件 → SQLite 写入 → 重读一致性 | 验证事件流端到端不丢、不重、顺序正确 |
| **前端组件测试** | Vitest + Vue Test Utils | Summary 卡片、心情词提交、设置页表单 | 验证用户能正确提交心情词——表单校验、Loading 状态、空态 |
| **端到端测试** | Playwright + Tauri Driver | 启动 → 模拟键盘事件 → 生成按钮 → 产物落盘 | 验证用户能完成一天流程——真实交互链路 |
| **手动验收清单** | 文档 | 真实运行一天后查看 summary/music/artwork 是否符合预期 | 情感与艺术生成是 AI 驱动，最终主观质量无法自动化 |

### 6.2 TDD 节奏

每个功能按 **Red → Green → Refactor**：
1. 先写失败测试（描述"为什么这样做"）
2. 实现到测试通过
3. 重构

### 6.3 验证前不宣布完成

任何"完成"声明前必须：
- `cargo test --workspace` 全绿
- `pnpm test` 全绿
- Playwright E2E 关键链路全绿
- 手动运行 ≥ 1 小时，HookListener 日志无 ERROR 级别

### 6.4 性能基线（验收门槛）

| 指标 | 阈值 |
|---|---|
| HookListener CPU 占用 | < 1% （空闲时） |
| HookListener 内存占用 | < 50 MB |
| 每日总结聚合耗时 | < 5 秒（10 万事件） |
| AI 音乐生成 | < 60 秒 |
| AI 画作生成 | < 30 秒 |
| 应用启动时间 | < 2 秒 |

---

## 七、目录结构（首版规划）

```
FingerTip/
├── docs/
│   ├── plans/                          # 设计/计划文档
│   │   └── 2026-07-16-fingertip-design.md  ← 本文件
│   ├── specs/                          # 精益画布 / 项目规格
│   │   └── lean-ux.md
│   └── tasks/                          # 任务拆分（系统/模块/功能）
├── src-tauri/                          # Rust 后端
│   ├── src/
│   │   ├── main.rs
│   │   ├── hook/                       # HookListener + EventBuffer
│   │   ├── summary/                    # SummaryAggregator
│   │   ├── generate/                   # GenerateOrchestrator + AI Adapter
│   │   ├── privacy/                    # PrivacyVault
│   │   ├── tray/                       # TrayManager
│   │   └── db/                         # SQLite 迁移与连接池
│   ├── tests/
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                                # Vue 3 前端
│   ├── views/
│   │   ├── Today.vue                   # 当日总结
│   │   ├── History.vue                 # 历史
│   │   ├── Settings.vue                # 设置
│   │   ├── About.vue
│   │   └── SubmitMood.vue              # 心情提交
│   ├── components/
│   ├── stores/                         # Pinia
│   ├── router/
│   ├── App.vue
│   └── main.ts
├── tests-e2e/                          # Playwright
├── package.json
├── pnpm-lock.yaml
├── vite.config.ts
└── README.md
```

---

## 八、未决事项与拓展（后续规划）

### 8.1 首版不包含（明确边界）
- macOS / Linux 原生打包
- 带歌词音乐（隐私敏感 + 需要独立评估）
- 多用户/账号体系
- 云同步 / 跨设备
- 数据可视化大盘（仅做基础卡片展示）
- 应用商店发布

### 8.2 后续路线（优先级待评估）
1. macOS 实现（已有架构预留）
2. 带歌词音乐（需独立隐私评估 + 用户确认）
3. 历史可视化（热力图、趋势线）
4. 自定义 prompt 模板
5. 本地模型接入（MusicGen small / SD-Turbo）

---

## 九、版本管理

- **本文档版本**：v1.0
- **变更控制**：任何架构级变更需更新本文档并提交 git
- **Git 提交前验收**：文档达到"用户层面可用级别"——即下游实施人员可独立据此搭建工程