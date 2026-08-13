# FingerTip 迭代日志

记录大型版本迭代的概括与重要小版本更新（详细变更见 `CHANGELOG.md`）。

## v0.8.3（2026-08-13）三个体验修复：托盘最小化 / 关于页右推 / 二维码扫不了

- 用户反馈三连：①点 × 直接退出进程（想最小化到托盘）②关于页「关于/FingerTip/副标题」为什么在最右面、一开始就有吗 ③分享卡片二维码太小太密扫不了
- ①`on_window_event` 拦 `CloseRequested` → prevent_close + hide，托盘 tooltip 提示仍在后台；托盘「退出」走 `app.exit(0)` 不受影响
- ②根因：窗口正好 1100px 触发全局媒体查询纵向堆叠，About 页横向布局的 `align-items: flex-end` 纵向下变成右对齐整块文字（从首个提交起就有）。补媒体查询恢复左对齐
- ③根因：固定 120px + Lanczos 糊边 + qrcode renderer 默认 8px/模块自带静区被误当纯网格。改为 ≥3px/模块整数放大 + 4 模块静区，尺寸随 payload 自适应（长链接 ≈300px），句子/统计区让位；海报输出唯一文件名防并发覆盖
- 验收：cargo 223 全过（新增 rqrr 端到端「扫码」测试，抠海报 QR 解码断言内容一致）；About 页 playwright 验证 1100px 左对齐无报错

## v0.8.2（2026-08-13）修「分享卡片生成不出图片」

- 用户反馈"生成二维码和海报在哪？生成不出图片"
- 根因：v0.7 后端 `upload_and_generate_qr` 返回 `{local_path, audio_ok, share_url}`（海报卡片 PNG 本地路径），前端还在读旧的 `qr_png_base64`/`url` → 图裂
- 修：前端接口对齐新结构 + `convertFileSrc(local_path)` 显示 16:9 海报预览 + 复制链接/保存卡片按钮
- 后端 `upload_and_generate_qr` 升级完整版（真实统计 top1/频率/总按键/活跃 + 系统浏览器打开卡片）
- 验收：cargo 222 / vue-tsc 0 错 / vitest 87 / e2e 18 全过

## v0.8.1（2026-08-13）修「生成失败：编排器重试后仍失败」

- 用户实测报 `MiniMax chat 返回非 JSON: EOF while parsing a value at line 1 column 0`
- 根因：v0.8 编排器 4→6 字段契约让 M3 思考块变大，max_tokens=2048 被思考耗尽，JSON 未输出即截断
- 修：max_tokens 2048→4096 + json_schema 补齐 6 字段 + strip 剥空返 "{}" 可诊断（含 warn 日志）
- Engine mock 同步升级 6 字段
- 验收：cargo 222 / engine pytest 11 全过

## v0.8.0（2026-08-13）整合同学完整 v0.7 管线（时间窗口 + 6 字段编排器）

用户实测反馈"生成作品的时间段选择"缺失。盘点发现：之前 v0.7 只移植了「海报渲染 + 落地页」，数据层 / 时间窗口 / 编排器 6 字段 / 前端 regenerate 按钮都没接。

- **时间窗口**：SubmitMood 加 datetime-local 从/到 + ↺（默认 48h）；store 存 timeRangeStartMs/EndMs；generate_now 接受 start_ms/end_ms → 按窗口查 events + 现场重算 theme_word
- **48h 窗口查询**：EventRepo::list_by_date_48h + list_by_timerange
- **编排器 6 字段**：OrchestrationContext 加 6 特殊键计数 + OrchestratorResult 加 english_sentence/theme_explanation
- **行为驱动主题词**：theme.rs 加 determine_theme_from_behavior（5 级优先级 + 16 组合）+ infer_mood_from_behavior
- **前端 regenerate**：Artworks 加「换一句」+ 画作/音乐 🔄 按钮，regenerate_* 传窗口参数
- 验收：cargo 221 / vue-tsc 0 错 / vitest 87 / e2e 18 / build 全过

## v0.7.4（2026-08-13）海报「容器全宽 + 渐变反转」

- 用户反馈"是不是反了？先透明后不透明"——期望"上面淡下面重"
- 渐变反转：top_alpha 0.85→0.00（顶透明，画作透出）/ bottom 0.00→0.92（底不透明，数据清晰）
- 用户反馈"下面一整块包括左右两边"——容器去 padding 全宽
- 数据位置下移：stats y 495→620, QR y 485→595（落到容器下半部不透明区）
- 圆角 24→16（消除"两端被圆角切走"的视错觉）
- 验收：cargo 219 全过；像素抽样 容器顶 (255,171,187)=画作色（透明），容器底 (253,244,245)=85%白

## v0.7.3（2026-08-13）海报「数据+QR 组件容器」

- 用户新需求：把数据+QR 收进圆角矩形容器，垂直渐变（顶 85% 白 → 底 0% 透明）
- 容器 y=460~720, radius 24，独立于卡片 RADIUS 32
- stats / QR / 脚注 全部移到容器内（垂直位置重排）
- 删 v0.7.1 分割线（容器本身就是分隔）
- 验收：cargo 219 全过；像素抽样确认 87%白→8%白 平滑 fade

## v0.7.2（2026-08-13）海报「art 铺满全卡」

- 用户实测反馈 v0.7.1 "中间空白"：根因是渐变覆层过强（35-80% / 60% 白）把画作糊掉
- 覆层改成 75-100% / 25% 白，整张画作 = 全卡背景
- 加 `draw_with_shadow` 软白影：句子 / 统计 / 标签 / 脚注 全上 halo，在彩色画作上读得清
- 删 v0.7.1 botched merge 残留的重复句子代码
- 测试图改暖橙→粉→紫渐变（去掉误导用的白方块）
- 验收：cargo 219 全过；海报 PNG 像素抽样确认全卡 art 色域

## v0.7.1（2026-08-13）海报双重组件重做

- 删右面板（同学原版 right_w=280 白色半透面板）
- 句子层移到上半视觉焦点底部（y=420，从画作底色上读）
- 统计层改为下半「数据焦点」：4 张 stat 卡等宽横排
- QR 缩到 120px 放右下角
- 验收：cargo 218 / vitest 87 / e2e 18 / build 全过

## v0.7.0（2026-08-13）海报分享管线 + 16:9 重做

- 完整移植同学 upload.rs（1005 行）：QrArtifact / SharePayload / SharePageData / create_share / generate_card_png
- CARD_W: 1080 → 1280（16:9 横版）；QR_SIZE: 200 → 120（低调）
- commands.rs `upload_and_generate_qr` 适配新 API（create_share）
- Cargo.toml 加 imageproc + ab_glyph 渲染依赖
- 验收：cargo 218 / vitest 87 / e2e 18 / build 全过

## v0.6.4（2026-08-13）Artworks 句子卡片 4 字段展示

- englishSentence（英文 italic 副行）+ themeExplanation（主题词解释后缀）
- 只 port 展示层，不动交互（避开同学 Artworks 整页重设计）
- 验收：vue-tsc 0 错 / vitest 87 通过

## v0.6.3（2026-08-13）CI 自动化

- `.github/workflows/release-sync.yml`：tag push → 跨平台 Tauri 构建 → Pages 部署 landing.html → release 草稿
- `docs/plans/2026-08-13-release-pipeline-setup.md`：仓库管理员一次性设置指引
- 验收：workflow 文件已提交，需手动 push tag 触发首次构建

## v0.6.2（2026-08-13）Engine Python image size 反向移植

- `_do_image(prompt, size)` 按请求尺寸生成 PNG（之前固定 1x1）
- `_parse_image_size` 解析 "WxH"，非法回退 1024x1024
- 验收：engine pytest 11 通过

## v0.6.1（2026-08-12）落地页双按钮下载

- landing.html `.cta-row` 容器：GitHub + 国内镜像
- 验收：vitest 87 / e2e 18 / build 全过

## v0.6.0（2026-08-12）重新生成系列命令

- regenerate_sentence / regenerate_music / regenerate_art 三条 Tauri command
- 复用已有 sentence + descriptions，不重跑编排器拿新 description（避免用户感知主题跳变）
- 验收：cargo 221 / build --lib 通过

## v0.5.0（2026-08-12）hook_status + AI 键盘诊断

- v0.5 hook_status：Rust `HOOK_RUNNING` atomic + 前端状态条（绿/灰点），让 Hook 启动失败可立即可见
- v0.6.0 AI 键盘诊断（funny_summary）：编排器新 4 字段契约；DB artifacts 表加 4 列（老库自动 ALTER）；前端 Artworks.vue `ft-funny-section` 橙色渐变 + 左边框
- 验收：cargo 221 / vitest 87 / e2e 18 / build 全过

## v0.4.3（2026-08-12）UI 修复 + 同学项目 baseline

- 网页端实测驱动的 UI 修复：作品页 mock 数据清除、今日页拼写修正、About 动态版本号、暖橙主色统一、宽屏居中容器
- 并入同学项目 3 个无冲突独立文件：`docs/landing.html` + `docs/plans/2026-08-12-troubleshooting-notes.md` + `pnpm-workspace.yaml`
- 无 Rust 后端改动，无 DB schema 变更 — 纯前端调整 + 文档/配置补全，作为后续 v0.5/v0.6 大改造的干净基线

## v0.4 —— 云端大模型化（编排器 LLM + 专有模型）

**概括**：彻底移除 v0.3 的本地确定性音乐/图像生成算法，改为「编排器 LLM → 专有模型」架构。App 通过三态能力路由（仅本地 / 本地优先 / 仅云端）在本地 FingerTip-Engine（Python 微服务，mock 默认）与云端 MiniMax 之间选择。v0.4.1 起云端全面落地 MiniMax 单 key 全链路。

### v0.4.1（2026-08-08）云端一键全通

- MiniMax 单 key 配全链路：编排器 LLM（`MiniMax-M3`）+ 图像（`image-01`）+ 音乐（`music-3.0` / `music-3.0-free`）
- 修「点击生成无反应」：云端字段默认预填 + Settings 保存校验；M3 推理噪音容错（剥 `<think>` / code fence）+ `max_tokens` 500→2048
- 初始化 `env_logger`，生成链路全日志可观测（此前 `log::*` 全静默）
- 实测通过：M3 编排 → 音乐（约 2~5 分钟）→ 图像，全链路可用

### v0.4.0（2026-08-07）架构大改造

- 移除本地确定性算法，引入「编排器 + 三态路由 + trait 抽象」
- 新增 FingerTip-Engine（Python FastAPI 微服务，mock 默认）
- Music / Art 改元数据契约（`description` + `model`），PNG 文件渲染，句子由编排器一次性产出
