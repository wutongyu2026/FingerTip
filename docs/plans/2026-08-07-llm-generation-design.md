# LLM/模型生成架构设计（v0.4 起）

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:writing-plans to create the implementation plan after this design is approved.

**Goal:** 去掉本地确定性音乐/图像生成算法，改为「编排器 LLM + 专有模型」链路：App 通过能力路由（本地引擎优先 → 云端兑底）完成音乐/图像/句子生成。

**Architecture:** 新增「编排器」层（llama.cpp/GGUF 本地 LLM 或云端 OpenAI 兼容 chat），把当日数据转成三条描述（音乐/图像/句子）；音乐由 Step-Audio（本地）或 MiniMax（云端）执行，图像由 sd.cpp（本地 SD1.5 GGUF）或云端文生图执行；模型推理全部收敛到一个可选插件「FingerTip-Engine」（独立 Python HTTP 服务），App 只连 HTTP。

**Tech Stack:** Rust（App 侧：reqwest 客户端、现有 adapter trait）；Python（引擎侧：FastAPI + llama-cpp-python + sd-cpp + Step-Audio）；MiniMax 音乐 API（云端兑底）；OpenAI 兼容图像 API（云端兑底）。

---

## 1. 设计决策（已与用户逐题确认）

| # | 决策 | 结论 |
|---|---|---|
| D1 | 部署拓扑 | **混合**：配置驱动，本地引擎优先，云端兑底 |
| D2 | LLM 角色 | **编排器**：写音乐/图像/句子描述，专有模型执行 |
| D3 | 图像路线 | **云端文生图走起 + 本地图像服务留接口**（本地 sd.cpp 默认 SD1.5 GGUF） |
| D4 | 前端契约 | **图改文件渲染**（`<img>`），**波形保留**（amplitudes[64] 由后端从 WAV 分析），播放仍走 Tone.Player |
| D5 | 配置 | **Settings 页 UI + JSON 配置文件**（app_data_dir/fingertip-config.json） |
| D6 | 旧算法 | **彻底删除** LocalMusicAdapter / LocalArtAdapter / wav_encoder / png_encoder |
| D7 | 引擎形态 | **我们自己做一个最小推理引擎**（可选插件，独立 Python 服务，单端口），用现有轮子（llama-cpp-python / sd-cpp / Step-Audio） |
| D8 | LLM 模型 | llama.cpp 框架，**多 GGUF 可选**（用户自己选/微调） |
| D9 | 音乐/图像模型 | 音乐=Step-Audio（云端兑底 MiniMax）；图像=SD1.5 GGUF（架构可配，支持换 Z-Image/FLUX.2 等 sd.cpp 支持的架构） |
| D10 | 音乐云端兑底 | **MiniMax 音乐 API**（base/key/model 进配置） |
| D11 | 图像 4GB 选型 | SD 1.5 Q4/Q5 GGUF（4GB 基线）；SSD-1B fp8 为质量边沿；FLUX.2 klein / Z-Image 需 8GB+，引擎接口不绑定架构 |

## 2. 架构总览

```
┌──────────────────────── FingerTip App (Tauri/Rust) ────────────────────────┐
│  generate_now → 读当日数据(summary+events)                                  │
│     ↓                                                                      │
│  ① 编排器(LLM)   prompt(当日数据) ──► JSON {音乐描述, 图像描述, 句子}         │
│     ↓                    ↓                 ↓                               │
│  ② 音乐适配器        ③ 图像适配器        ④ 句子(并入产出)                     │
│     ↓                    ↓                                                  │
│  ⑤ WAV分析→amplitudes   PNG 文件                                            │
│     ↓                    ↓                                                  │
│  写 downloads/{date}/ + artifacts 表 ──► 前端 Artworks                       │
└────────────────────────────────────────────────────────────────────────────┘
        ▲ 每能力三态路由：本地引擎 → 云端兑底 → 明确报错
        │
┌───────┴─────── FingerTip-Engine（可选插件，独立 Python 服务，单端口）────────┐
│  /v1/health   健康/能力探测 {llm, image, audio}                             │
│  /v1/chat     llama-cpp-python（多 GGUF 可选）        ← LLM 编排             │
│  /v1/images    sd.cpp（默认 SD1.5 GGUF，架构自动识别）  ← 图像（OpenAI 兼容）  │
│  /v1/audio     Step-Audio（单模型）                    ← 音乐                 │
└────────────────────────────────────────────────────────────────────────────┘
```

要点：
- App 侧保留 `MusicAdapter`/`ArtAdapter` trait 抽象，实现换成 model 版；`generate_now` 调用骨架几乎不动
- 新增编排器层：LLM 把当日数据转成三条产物描述
- 引擎 = 可选插件（方案 1，Python 单服务）；App 只连 HTTP，不捆绑推理
- 三态路由：每能力独立判断本地 → 云端 → 报错

## 3. 数据流 + 数据结构变化

**generate_now 新流程**：

```
① 读 summary + events（不变）
② 组装「编排上下文」= 当日数据摘要（theme_word/mood/四指标/Top5/hourly/首活）
③ 调编排器 LLM ──► 一条消息返回 JSON：{ music_description, image_description, sentence }
④ 音乐：description → engine /v1/audio(Step-Audio) 或 MiniMax → WAV
      └─► WAV 分析 → amplitudes[64] + duration_ms（替代被删的手写 wav_encoder）
⑤ 图像：description → engine /v1/images(本地 SD) 或 云端文生图 → PNG
⑥ 写 downloads/{date}/ + artifacts 表（新字段）
⑦ 前端：<img> 渲染 PNG · Tone.Player 播 WAV · 波形用 amplitudes（契约不变）
```

**数据结构变化**：

| 结构 | 变化 |
|---|---|
| `Music` | 删 notes；保留 bpm/duration_ms/amplitudes/mood/style/theme_word；加 description + model |
| `Art` | 删 pixels/width/height；保留 theme_word/mood；加 description + model |
| 新增 `OrchestrationContext` | 编排器输入（当日数据摘要） |
| 新增 `OrchestratorResult` | { music_description, image_description, sentence } |
| 新增 `wav_analysis` 模块 | WAV → 振幅包络[64]/时长（读分析，替代 wav_encoder 的写合成） |
| artifacts 表 | music_json/art_json 存元数据；加 sentence 列（并入产出）；get_artifact 一并返回 |

**已确认的小决策**：
- bpm 保留字段但不设（0），前端不依赖（Artworks 音乐卡不显示 bpm）
- 句子并入 generate_now 产出（artifacts 加 sentence 列），不再 Artworks 挂载时二次调 LLM；`generate_sentence` 命令保留但改为读已存句子

## 4. 引擎契约 + 三态路由 + 配置

**引擎 HTTP 契约**（OpenAI 兼容优先）：

| 端点 | 能力 | 说明 |
|---|---|---|
| `GET /v1/health` | 健康/能力探测 | 返回 `{ llm: bool, image: bool, audio: bool }`，App 据此路由 |
| `POST /v1/chat` | LLM 编排 | OpenAI 兼容 chat completions，JSON 模式输出 `{music_description, image_description, sentence}` |
| `POST /v1/images/generations` | 图像 | sd.cpp 原生 OpenAI 兼容端点（model 传 GGUF 路径/名，架构自动识别） |
| `POST /v1/audio` | 音乐 | 自定义：`{text, seed}` → WAV 字节（Step-Audio） |

**三态路由**（每能力独立，App 启动时 + 每次生成前快速探测 `/v1/health`）：

| 能力 | 本地引擎 | 云端兑底 | 都没配 |
|---|---|---|---|
| LLM 编排 | llama-cpp-python | OpenAI 兼容 chat API（配了 key） | 红字：装引擎或配云端 |
| 图像 | sd.cpp (SD1.5 GGUF) | OpenAI 兼容文生图 API（配了 key） | 红字 |
| 音乐 | Step-Audio | **MiniMax 音乐 API**（base/key/model 进配置） | 红字 |

> 音乐描述要兼容两条路：Step-Audio 吃纯文本描述，MiniMax 吃类似音乐描述。编排器 prompt 里音乐描述按「风格/情绪/主题词」三维通用音乐 prompt 产出。

**配置**（Settings 页「模型接入」区块 + `app_data_dir/fingertip-config.json`）：
- **引擎**：启用开关 + 地址/端口（默认 `http://127.0.0.1:8765`）
- **LLM**：GGUF 模型列表（多可选）+ 云端 base/key/model
- **图像**：本地模型路径（默认 SD1.5 GGUF）+ 云端 base/key/model
- **音频**：引擎启用开关 + MiniMax base/key/model
- **路由**：每能力「本地优先 / 仅云端 / 仅本地」三档

## 5. 错误处理 + 测试策略

**错误处理**（延续「失败要大声」）：
- 每能力路由失败 → 明确红字：`音乐不可用：本地引擎未就绪且云端未配置（MiniMax key 缺失）`，不静默
- 引擎 `/v1/health` 探测失败 → 视为该能力本地不可用，自动落云端
- LLM 编排输出非 JSON / JSON 缺字段 → 重试 1 次，再失败返回可读错误（不 panic）
- 生成超时（LLM 60s / 音频 120s / 图像 60s）→ 超时错误带上下文

**测试策略**：
- 后端单测：编排器 JSON 解析、三态路由决策纯函数、WAV 振幅分析（构造已知波形断言包络）、MiniMax/OpenAI/引擎客户端用 mock HTTP（`mockito`/`httpmock`）
- 前端单测：Settings 模型配置表单、Artworks 渲染 PNG `<img>` + 波形（契约不变处复用现有测试）
- e2e：Settings 提交配置、无引擎时的红字路径
- 引擎本体（Python）独立测试：/v1/health、/v1/chat JSON 模式、/v1/audio 返回合法 WAV

## 6. 本轮范围（明确不做）

- **不做**：引擎的模型训练/微调（用户自己来）；图像本地模型默认装 FLUX.2/Z-Image（8GB+ 档，接口预留但默认 SD1.5）
- **删除**：`generate/local/music.rs`、`generate/local/art.rs`、`db/wav_encoder.rs`、`db/png_encoder.rs`、`FINGERTIP_USE_CLOUD` env 开关（被配置路由取代）
- **保留**：`generate/sentence.rs` 的 prompt 能力（改为读 LLM 产出的句子而非本地拼接）；`upload.rs`（uguu.se 二维码）不变