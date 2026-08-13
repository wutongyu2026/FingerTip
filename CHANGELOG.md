# Changelog

FingerTip 的版本变更日志。本文档遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 规范。

## [0.8.3] - 2026-08-13

**三个体验修复**：点 × 直接退出进程 → 最小化到托盘；关于页标题块被推到窗口最右；分享卡片二维码太密太小扫不了。

### Fixed

- **点 × 最小化到托盘**：`on_window_event` 拦截 `CloseRequested` → `prevent_close` + `hide`，托盘 tooltip 提示「仍在后台运行 · 单击托盘图标恢复」；托盘「退出」菜单仍走 `app.exit(0)`（`ExitRequested` 不经过 `CloseRequested`），不受拦截
- **关于页标题在最右面**：Tauri 默认窗口正好 1100px，触发 tokens.css `@media (max-width: 1100px)` 把 `.ft-page-header` 切纵向堆叠；About.vue 为横向布局写的 `align-items: flex-end` 在纵向下变成「整块文字右对齐」。补同断点媒体查询恢复左对齐（版本徽章跟到文字下方）
- **二维码扫不了**：旧实现固定 120px + Lanczos 非整数缩放出糊边；且 qrcode image renderer 默认 8px/模块 + 自带 4 模块静区，被误当纯网格导致尺寸公式错误。改为模块 ≥3px 整数放大（黑白边界锐利）+ 规范 4 模块静区，尺寸随 payload 密度自适应（~800 字符分享链接 ≈ 300px 大码），句子/统计区按实际码宽让位
- 海报输出唯一文件名（原固定 `fingertip-card.png`，并发渲染会互相覆盖）

### Tests

- 新增端到端「扫码」验证：真实长度分享链接渲染海报 → 按布局公式抠出 QR 区域 → rqrr 解码断言内容一致（等价于手机扫码成功）
- 验收：cargo test --lib **223** 通过 / About 页 playwright 无 console error、1100px 下标题块回到 x=20 左对齐

## [0.8.2] - 2026-08-13

**修「生成不出图片」—— 分享卡片前端读旧结构**：用户反馈"生成二维码和海报在哪？生成不出图片"。根因是 v0.7 后端 `upload_and_generate_qr` 已从 `{url, qr_png_base64}` 升级为 `{local_path, audio_ok, share_url}`（海报卡片 PNG 本地路径），但前端 Artworks.vue 还在读 `qr_png_base64` / `url`（undefined）→ 图裂、链接空。

### Fixed

- **前端 QrArtifact 接口对齐后端新结构**：`{local_path, audio_ok, share_url}`；海报卡片用 `convertFileSrc(local_path)` 显示 16:9 预览（替代旧的 base64 二维码）
- **`onGenerateQr` 传 `english_sentence`**：后端完整版海报需要它（重新生成过句子时透传最新值）
- **新增 `onCopyShareLink`（复制分享链接）+ `onSaveCard`（保存卡片 PNG）** + 打开分享页按钮
- **`upload_and_generate_qr` 后端升级完整版**（对齐同学）：从 daily_summary 算 top1 键 / 频率 / 总按键 / 活跃小时 / hourly，替代旧的统计全 0；生成后用系统浏览器打开卡片预览
- 新增 `get_hourly_impl` + `key_display_name` helper

### Changed

- UI 文案「分享二维码」→「分享今日作品 / 生成卡片图片 / 海报 + 扫码直达分享页」
- e2e 测试断言同步更新

### Tests

- 验收：cargo test --lib **222** 通过 / vue-tsc 0 错 / vitest **87** 通过 / e2e **18** 通过

## [0.8.1] - 2026-08-13

**修「生成失败：编排器重试后仍失败」**：用户实测 v0.8.0 生成报 `MiniMax chat 返回非 JSON: EOF while parsing a value at line 1 column 0`。

### Root Cause（第一性原理）

日志两次响应都以 `<think>` 开头，错误 `EOF while parsing a value at line 1 column 0` = **serde_json 解析空字符串**。即：M3 思考块被 `max_tokens=2048` 截断，**JSON 还没输出就没了**；`strip_llm_json_noise` 剥掉 think 后得到空串 → 晦涩 EOF 错误。

为什么 v0.4.1（500→2048）解决了、v0.8.0 又复发？因为 v0.8 把编排器从 4 字段升到 **6 字段契约** + prompt 加了特殊键统计 —— M3 思考块变大，2048 token 又被思考耗尽。

非 token plan（套餐配额）问题：日志显示请求成功、响应正常返回，是本机 `max_tokens` 单次输出上限不够。

### Fixed

- **`max_tokens` 2048 → 4096**（M3 思考 + 6 字段 JSON 留足空间）
- **MiniMax json_schema 补 3 字段**：schema 只约束 music_description/image_description/sentence 三必填，v0.8 的 english_sentence/theme_explanation/funny_summary 会丢 → 补齐 6 字段 schema
- **`strip_llm_json_noise` 容错**：剥 think 后为空 / 含未闭合 `<think>`（截断在思考中）→ 返 `"{}"` 并打 warn 日志，让编排器报「缺必填字段」（可诊断）而非 serde 晦涩 EOF
- **Engine mock 升级 6 字段**：本地 mock 链路也能看到 english_sentence / theme_explanation / funny_summary

### Tests

- 新增：`strip_noise_returns_empty_object_when_only_think_block_truncated`（闭合 think 无 JSON / 未闭合 think 截断两种情况）
- 验收：cargo test --lib **222** 通过 / engine pytest **11** 通过

## [0.8.0] - 2026-08-13

**整合同学完整 v0.7 管线（时间窗口 + 6 字段编排器）**：盘点发现之前 v0.7 只移植了「海报渲染 + 落地页」，数据层 / 时间窗口 / 编排器 6 字段 / 前端 regenerate 按钮没接。本次把同学 v0.7 完整管线补齐 —— 用户实测反馈"生成作品的时间段选择"缺失即由此而来。

### Added

- **自定义时间窗口（核心）**：
  - SubmitMood.vue 加「选择时间窗口」区块：两个 datetime-local（从/到）+ ↺ 恢复默认 48h
  - store 加 `timeRangeStartMs` / `timeRangeEndMs`（SubmitMood 选完存，Artworks regenerate 复用）
  - `generate_now` 接受 `start_ms` / `end_ms`（Option<i64>）—— 指定时按窗口查 events + **现场重算 theme_word**（`determine_theme_from_behavior`）
  - 后端 `format_time_range_label`：窗口 → "06:00–08:00"（Local 时区），写入 artifacts.time_range_label 透传海报/落地页
- **48h 窗口查询**：`EventRepo::list_by_date_48h` + `list_by_timerange`（昨天 00:00 → 明天 00:00，覆盖跨零点通宵）
- **编排器 6 字段契约**：`OrchestrationContext` 加 6 特殊键计数（backspace/delete/enter/space/wasd/total）+ `OrchestratorResult` 加 `english_sentence` / `theme_explanation`；system prompt 升级到 6 字段；parse 向后兼容旧 3/4 字段
- **Aggregator 特殊键计数**：`count_special_keys` → `SpecialKeyCounts`
- **theme.rs 行为驱动主题词**：`determine_theme_from_behavior`（5 级优先级：REWRITE/BREAK/PAUSE/CONTROL + 16 种四维组合）+ `infer_mood_from_behavior`
- **前端 regenerate 按钮**：Artworks.vue 加「不满意？换一句」+ 画作/音乐 🔄 按钮，三个 `onRegenerate*` 传 startMs/endMs；`regenerate_sentence` 用编排器新产出覆盖 4 字段；`regenerate_art/music` 透传窗口参数

### Changed

- `generate_now_impl` 签名加 `time_range_label: &str`；`GenerateNowOutcome` 加 `english_sentence` / `theme_explanation` / `time_range_label` 三字段
- `upsert_artifact_outcome` 加 3 参数（english_sentence / theme_explanation / time_range_label）
- `regenerate_sentence` 接受 start_ms/end_ms + 现场重算 theme + 行为推断 mood
- 默认时间窗口 = 48h（不传 start/end 时）

### Tests

- 新增：`list_by_date_48h_includes_prev_and_next_day_boundary` / `list_by_timerange_returns_only_slice`
- 验收：cargo test --lib **221** 通过 / vue-tsc 0 错 / vitest **87** 通过 / e2e **18** 通过 / vite build 通过

## [0.7.4] - 2026-08-13

**海报「容器全宽 + 渐变反转」**：v0.7.3 后用户反馈两个问题 ——（1）"是不是反了？怎么先透明后不透明了"——实际是圆角 + 渐变叠加造成的视错觉，期望渐变方向是"上面淡下面重"；（2）"下面一整块包括左右两边"——容器左右各 32px padding 应该去掉，铺满卡片宽度。

### Changed

- **容器渐变反转**：top_alpha 0.85 → **0.00**（顶部透明，画作透出）/ bottom_alpha 0.00 → **0.92**（底部不透明，数据清晰）
- **容器全宽**：component_left 32 → **0**，component_w (CARD_W-64) → **CARD_W**（铺满卡片左右两边）
- **圆角缩小**：24 → **16**（减少圆角对视线的分散，让"两端被圆角切走"的视错觉消失）
- **stats y 位置**：495 → **620**（从容器顶不透明区移到下半部不透明区，保证数据可读）
- **QR y 位置**：485 → **595**（同上）
- **脚注位置**：y=685 不变（已在底部最不透明处）

### Tests

- 验收：cargo test --lib **219** 通过
- 海报 PNG 像素抽样确认：容器顶 (255,171,187) 与画作 (254,170,184) 一致 = 透明；容器底 (253,244,245) 85% 白 = 不透明；左右边缘 x=56/x=1335 都填到卡片边

## [0.7.3] - 2026-08-13

**海报「数据+QR 组件容器」**：用户反馈"显示数据和二维码的组件那边，越接近下面透明度越高"——把底部数据+QR 收进一个圆角矩形容器，垂直渐变（顶 85% 白 → 底 0% 透明），让容器底边自然融入画作。

### Added

- **数据+QR 组件容器**（v0.7.3 核心）：
  - 圆角矩形（radius 24，独立于卡片 RADIUS 32，让组件有"独立 UI 元素"感）
  - 范围：x=32~1248, y=460~720（卡片内坐标）
  - **垂直渐变**：top_alpha=0.85 → bottom_alpha=0.00 —— 数据区（顶部）清晰可读，底部边缘自然 fade 进画作，无生硬边界
  - 卡片外圆角裁剪（`in_card` check）：避免容器底部圆角与卡片圆角冲突

### Changed

- **stats y 位置**：cy(590) → cy(495)（上移到容器内 73% 不透明区域，数据更清晰）
- **QR y 位置**：cy(536) → cy(485)（容器内顶 78% 不透明区域，扫码识别率最高）
- **脚注 y 位置**：cy(668) → cy(685)（下移到容器底部接近完全透明处，靠 halo 兜底可读）
- **删 v0.7.1 的分割线**：容器本身就是视觉分隔，dot line 不再需要

### Tests

- 验收：cargo test --lib **219** 通过；海报 PNG 像素抽样确认容器渐变 87%白 → 8%白 平滑 fade

## [0.7.2] - 2026-08-13

**海报「art 铺满全卡」**：用户实测反馈 v0.7.1 海报"中间是空白"——实为渐变覆层过强把画作糊掉。改成画作贯穿整张卡 + 文字靠 halo 保证可读。

### Fixed

- **渐变覆层降级**：覆盖范围 35%-80% 60%白 → 75%-100% 25%白。整张画作 = 全卡背景；只有底部 25% 有极淡白雾托住脚注
- **新增 `draw_with_shadow` 软白影**：4 方向 1px 偏移画 27% 白色 halo，主文字覆盖后 halo 沿字符边缘露出，对抗彩色画作上深色文字的可读性
- **句子 / 统计标签 / 统计数值 / 圆点 / 描述 / QR 标签 / 脚注 全上 halo**：文字层在任意画作上都能看清
- **句子 y 位置**：420 → 380（移到画作下半区，halo 足够支撑）
- **删除重复代码 bug**：`generate_card_png` 内"句子块"被画了两次（v0.7.1 botched merge 残留），浪费 CPU 且让 ITERATION 描述混乱

### Changed（测试图）

- **测试图去掉白方块**：256×256 红色 + 64×64 白色中心（之前为"让画作有视觉内容"，但白色中心误导用户以为中间是空的）→ 改成 256×256 暖橙→粉→淡紫渐变（模拟真 AI 画作）

### Tests

- 验收：cargo test --lib **219** 通过

## [0.7.1] - 2026-08-13

**海报双重组件重做**：v0.7.0 移植后，海报还是同学原版「左侧画作 + 右侧面板竖排」单层布局。按你的设计意图（16:9 横版 + 上下分割：视觉焦点 / 数据焦点），重做排版：

### Changed

- **删右面板**（同学原版 `right_w=280` 白色半透明面板）
- **句子层移到上半视觉焦点底部**（y=420，从画作底色上读）
- **统计层改为下半「数据焦点」**：4 张 stat 卡等宽横排（每张占 25% 宽）
- **QR 缩到 120px 放右下角**，腾出空间给横排 stat
- **活跃度圆点**：竖排 → 横排（更紧凑）

### Tests

- 验收：cargo test --lib **218** 通过 / vitest **87** 通过 / e2e **18** 通过 / pnpm build 通过

## [0.7.0] - 2026-08-13

**v0.7 大改造：海报分享管线 + 16:9 双重组件重做**。从同学项目移植完整 `upload.rs`（1005 行 → 本地），并按你的设计意图把海报从 9:16 / 单张堆叠改为 **16:9 横版 + 双重组件**（视觉焦点 + 数据焦点）。

### Added

- **完整 `upload.rs` 海报分享管线**（1005 行）：
  - `QrArtifact` 结构（local_path / audio_ok / share_url）
  - `SharePayload` 短名编码 13 字段（v/w/p/s/e/t/m/d/k/f/n/a/r/u）→ 落地页 base64 解码
  - `SharePageData` 15 字段（wav/png/sentence/english/theme/mood/date/top1/freq/total/act/hourly/range/dl/funny）
  - `default_download_url()` / `landing_page_url()`（环境变量 fallback）
  - `create_share(&data)` 一站式：上传 WAV + PNG 到 uguu.se + 合成卡片 PNG（spawn_blocking）
  - `generate_card_png(&data, qr_url)` 核心 PNG 渲染（imageproc 绘图 + ab_glyph 字体）
  - 字体发现：CN 宋体/CN Sans + EN Serif/EN Bold（多平台路径 fallback）
  - QR 码生成：内置 qrcode crate（不依赖外部 API）
  - 圆角矩形 + 阴影绘制（纯 imageproc）

### Changed

- **CARD_W: 1080 → 1280**（16:9 横版比例）
- **CARD_H: 720**（保持）
- **QR_SIZE: 200 → 120**（低调不抢眼，扫码成功率仍够）

### Fixed

- **`upload_and_generate_qr` 命令适配**：从旧的 `upload_music_and_qr` 改为新 `create_share` API（一站式）
- **依赖**：`Cargo.toml` 加 `imageproc = "0.25"` + `ab_glyph = "0.2"`（同学项目移植所需）

### Tests

- 验收：cargo test --lib **218** 通过 / vitest **87** 通过 / e2e **18** 通过 / pnpm build 通过

## [0.6.4] - 2026-08-13

**Artworks 句子卡片：编排器 4 字段透传展示**。编排器已产出 sentence / english_sentence / theme_explanation / funny_summary，落地页（landing.html）已通过 `d.e`/`d.u` 字段消费。本地 Artworks 之前只展示 sentence + funny_summary；现在补齐 english_sentence（英文句子 italic 副行）+ theme_explanation（主题词解释，作为 eyebrow 后缀）。

### Added

- **`englishSentence` / `themeExplanation` / `themeWordDisplay` 计算属性**（Artworks.vue script）
- **中英分行渲染**：中文句为主、手写体英文句 italic 副行（`.ft-sentence-text-en`）
- **主题词解释**在 eyebrow 后作为补充说明（`.ft-theme-explanation`，左边竖线 + italic）

### Why not full port

- 同学 Artworks.vue 整页有 v0.7 regenerate_* 重新生成按钮 + 主题词变更检测等大改造；本任务只 port **展示层**，不动交互层（避免冲掉本地 v0.4.2 暖橙主题 + 居中容器）
- 编排器实际尚未产出 english_sentence / theme_explanation 字段（OrchestratorResult 当前只有 funny_summary）；等编排器升级后，前端无需再改

### Tests

- 验收：vue-tsc 0 错 / vitest 87 通过

## [0.6.3] - 2026-08-13

**CI 自动化（share-enhancement.md ④）**：tag push → 跨平台 Tauri 构建 → GitHub Pages 部署 landing.html → release 草稿。无需手动协调 3 个平台。

### Added

- **`.github/workflows/release-sync.yml`**：3 个 job —— build-tauri (windows/linux/macos 矩阵) + deploy-pages (landing.html → GitHub Pages) + release-summary
- **`docs/plans/2026-08-13-release-pipeline-setup.md`**：仓库管理员一次性设置指引（Pages Source 选 GitHub Actions + 环境保护 + Secrets 说明）

### 触发流程

- `git tag v0.7.0 && git push origin v0.7.0` → 自动出 MSI/NSIS/DEB/AppImage/DMG + Pages 部署
- 手动 `Run workflow` 也支持（测试 / 紧急 release）

### 已知限制

- macOS 构建需要 Apple Developer 证书（当前 ad-hoc）
- 国内 runner 拉 crates.io 慢（已加 cargo cache，后续可加镜像）

## [0.6.2] - 2026-08-13

**Engine Python 反向移植**：从同学项目移植 `_do_image` 按请求 size 生成对应尺寸 PNG + `_parse_image_size` 解析辅助函数。本地 mock_image_png 之前固定 1x1，对 Rust 端 /v1/images/generations 传 size=512x256 无响应（实际只返 1x1 PNG）。

### Added

- **`_parse_image_size(size: str) -> tuple[int, int]`**：解析 "WxH" 字符串，非法值回退 1024x1024
- **`_do_image(prompt, size)`**：按解析后尺寸生成对应 PNG
- seed 字段（`zlib.crc32(prompt)`）预留——本地 mock 不消费，但保留给未来真实后端接入用

### Tests

- 新增：`test_image_with_custom_size_returns_png_of_that_size`（解析 IHDR 校验 512x256 真生效）
- 新增：`test_image_with_garbage_size_falls_back_to_default`（非法 size 回退 1024x1024）
- Engine pytest **11** 通过

## [0.6.1] - 2026-08-12

**landing.html 双按钮下载**：`.cta` 拆为 `.cta-row` 容器，GitHub 直链（d.l 或 hostname fallback）+ 可选国内镜像（d.c）。无 payload 静态页也走同一包装函数。Rust 端 upload.rs 加 l/c 字段后自动联动。

## [0.6.0] - 2026-08-12

**v0.5.0 之上加 3 条 regenerate_* 命令**：复用已有 sentence + description + Music.model + Art.model，仅重跑模型生成产物（PNG / WAV / sentence）。不引入 v0.7 的 list_by_date_48h / OrchestrationContext 6 字段特殊键计数（避免连锁依赖），移植面小、回归风险低。

### Added

- **`regenerate_sentence(date, mood, style)`**：调编排器重生成 sentence + funny_summary，写库保留 music/art/wav/png
- **`regenerate_music(date)`**：复用已有 music.description + theme_word + style，调 AudioAdapter 重写 wav + 跑 wav_analysis 解析新 amplitudes/duration_ms
- **`regenerate_art(date)`**：复用已有 art.description + theme_word，调 ImageAdapter 重写 png
- 三个命令统一注册到 `lib.rs::run()` 的 invoke_handler

### Notes

- 复用已有字段而非重新调编排器拿新 description —— 因为 regenerate_* 的核心需求是"换一张图/换一段音乐"，sentence / descriptions 应保持稳定（避免用户感知上的"主题跳变"）
- v0.7 完整版（list_by_date_48h + OrchestrationContext 6 字段）延后到 P2.5

### Tests

- 验收：cargo test --lib **221** 通过 / vitest **87** 通过 / cargo build --lib 通过

## [0.5.0] - 2026-08-12

**新功能 + DB schema 升级**：v0.4.3 之上补齐「键盘 Hook 状态条」+「AI 键盘诊断（funny_summary）」两条最小可用功能。**有 DB schema 升级**（artifacts 加 4 列，老库自动 ALTER），是无破坏性扩展。

### Added

- **v0.5 hook_status**：Rust 端 `crate::HOOK_RUNNING` atomic 全局标志，Hook 启动成功时 store(true)。前端 `get_hook_status` 命令读取；App.vue 右上角状态条绿点（已启动）/ 灰点（未启动），web 模式不渲染
- **v0.6.0 AI 键盘诊断（funny_summary）**：编排器 system prompt 新增第 4 字段 `funny_summary`（70-135 字段子风搞笑总结），解析时兼容缺省。落到 artifacts 表新列 + Artworks.vue `ft-funny-section`（橙色渐变 + 左边框，对齐落地页 `.funny-card`）

### Changed

- **DB schema 升级**：`artifacts` 表新增 4 列（`english_sentence` / `theme_explanation` / `time_range_label` / `funny_summary`），老库通过 `pragma_table_info` 检测自动 ALTER。当前版本写库时只用 `funny_summary`，其余 3 列占位留给后续 v0.6/v0.7 P2/P2.5 移植
- **编排契约升级**：从 3 字段 → 4 字段；`parse_orchestrator_json` 向后兼容（LLM 漏产 `funny_summary` 时默认空字符串，前端 v-if 兜底不渲染）
- **`GenerateNowOutcome` 加字段**：`funny_summary: Option<String>`；`upsert_artifact_outcome` 一站式 wrapper（含 funny_summary）
- **前端类型**：`GenerateNowResult` 加 4 个 Option 字段（funny_summary / english_sentence / theme_explanation / time_range_label），全部向后兼容

### Tests

- 新增：`artifacts_table_has_v060_4_new_columns` / `old_artifacts_gets_v060_columns_via_alter` / `old_artifacts_with_v060_columns_already_present_skips_alter`
- 新增：`upsert_with_full_round_trips_v060_fields` / `upsert_with_full_optional_fields_write_null`
- 新增：`parse_orchestrator_json_extracts_funny_summary` / `parse_orchestrator_json_unwraps_single_wrapper_key_with_funny`
- 新增：`get_hook_status_reflects_hook_running_atomic`
- 验收：cargo test --lib **221** 通过 / vitest **87** 通过 / e2e **18** 通过 / pnpm build 通过

## [0.4.3] - 2026-08-12

**UI 修复 + 同学项目 baseline 合并**：在 v0.4.1 之上做了一轮网页端实测驱动的 UI 修复 + 并入同学项目 3 个无冲突独立文件。**无 Rust 后端改动，无 DB schema 变更**——纯前端调整 + 文档/配置补全，可作为后续 v0.5/v0.6 大改造的干净基线。

### Fixed（v0.4.2 UI 修复，本版本合并）

- **作品页硬编码 mock 数据清除**：删除「Abstract in orange / Ambient pulse / Today's ambient / focused + hello」4 处假数据。画作/音乐卡标题改读真实 `art/music.theme_word`，音乐副标题读 `style · 时长 · 由心情 X + 主题词 Y 驱动`；无产物时显示 ▢/♪ 引导式空态，不再渲染带假标题的播放器
- **画作下载按钮补样式**：`.ft-art-download` 此前未定义样式会裸渲染，补 36px 圆形 + 浮层右上角 + hover 暖橙强调
- **今日页 label 拼写错误**：`密集度 dynsity` → `density`、`平稳度 stabilit` → `stability`
- **今日页 hero 空态诚实化**：状态标签动态（加载中 / 等待数据 / 已聚合）；无主题词时 em dash 弱化 + 提示行「今日主题词会在按键中自然浮现」
- **关于页版本过期**：`v0.3.0` 写死 → 改从 `package.json` 动态读取（杜绝再次过期）；新增手写体大标题 + vX.Y.Z 徽章 + 三卡（做什么/隐私/技术）
- **作品页头部标题字号统一**：删除内联覆盖 `font-size: 28px`，回归全局 36px

### Changed（v0.4.2 美化）

- **Naive UI 主色统一为暖橙 `#D67B4F`**：通过 `NConfigProvider :theme-overrides` 把 primary 改为设计系统 accent；激活导航、primary 按钮不再突兀显示 Naive 默认蓝
- **宽屏内容居中**：`app-shell` 加 `max-width: 1160px; margin: 0 auto`，`#app` 背景色与 `--bg-base` 对齐（避免两侧灰与暖白不一致）

### Added（同学项目合并）

- **`docs/landing.html`**（同学 v0.9 落地页）：扫码分享落地页 — 主题词 + 句子 + 统计 + 音频播放 + funny_summary。**注：v0.6.0 将按 16:9 双重组件重构此页**
- **`docs/plans/2026-08-12-troubleshooting-notes.md`**：Troubleshooting 通用方法（网络 / 锁文件 / 编码）
- **`pnpm-workspace.yaml`**：`allowBuilds: { esbuild: false, vue-demi: false }`，避免 pnpm 9.x build script 警告

### Tests

- e2e 同步更新：`pages-render.spec.ts` About 版本断言改用类选择器（手写标题 + 徽章）；`v0.3.5-r2-stats.spec.ts` 拼写改 density/stability；`v0.3.7-local-playback.spec.ts` 无数据时断言「空态 + 控件不渲染」（替代旧「控件存在」）
- 验收：vue-tsc 0 错 / vitest 87 通过 / e2e 18 通过 / `pnpm build` 通过

## [0.4.1] - 2026-08-08

**云端一键全通版**：v0.4 的「编排器 LLM + 专有模型」架构全面落地 MiniMax 单 key 全链路（编排器 + 图像 + 音乐），并修掉一批让「点击生成无反应」的根因级问题。实测通过：M3 编排 → music-3.0 / music-3.0-free 音乐 → image-01 图像，全链路可用。

### Added

- **MiniMax 单 key 全链路**：编排器 LLM 云端兑底改 MiniMax（`MiniMax-M3`，OpenAI 兼容 `/v1/chat/completions` + `json_schema` response_format）；图像走 `image-01`；音乐走 `music-3.0` / `music-3.0-free`。一个 MiniMax key 配全链路，不再依赖 OpenAI（`OpenAiClient` 删除）
- **M3 推理模型兼容**：`strip_llm_json_noise` 剥 `<think>` 推理 token 与 markdown code fence；编排器 `max_tokens` 500→2048（M3 思考块会耗尽 500 token 导致 JSON 截断）
- **日志基础设施**：初始化 `env_logger`（此前 `log::*` 宏无后端全静默）；`generate_now` 全链路逐步日志 + 编排/音乐/图像耗时计时
- **默认配置预填云端字段**：`cloud_base` / `cloud_model` 默认 `https://api.minimaxi.com` + M3 / image-01 / music-3.0，杜绝 Settings 占位符陷阱（灰色提示看着像已填、实际保存空串）
- **Settings 保存前校验**：`仅云端` 区块 base/key/model 缺失即红字拦截，不静默存坏配置

### Fixed

- **「点击生成无反应」根因修复**：① 磁盘配置 base/model 为空 → `cloud_*_ok` 判定失败 → 云端路由 Unavailable；② M3 思考耗尽 `max_tokens` → JSON 截断 → 编排器两次解析失败。两者均已实测修复
- **音乐免费版模型名**：确认为 `music-3.0-free`（**小写**，官方文档在列；大写 `Music-3.0-free` 报 2013 invalid model）
- **音乐生成超时** 300s→600s（实测 music-3.0 约 149s / free 约 104s）

### 已知事项

- 本地引擎仍为可选（mock 默认）；云端全链路需有效 MiniMax key
- 音乐生成单次约 2~5 分钟（真实模型代价），编排器 M3 会输出思考过程（已被容错剥除）

## [0.4.0] - 2026-08-07

**大改造发版**：彻底移除 v0.3 的本地确定性音乐/图像生成算法，引入「编排器 LLM + 模型」架构——App 通过三态能力路由（`LocalOnly` / `LocalFirst` / `CloudOnly`）选择本地 FingerTip-Engine（Python 微服务，mock 默认、real 可选升级）或云端 OpenAI/MiniMax 完成音乐/图像/句子生成。句子改由编排器在生成时一次性产出（不再事后拼凑 top5 keys），图改 PNG 字节流（不再写 PixelSpec 序列），WAV 文件回灌经 wav_analysis 反推 64 桶振幅 + 时长。

### Added

#### model 层（编排器 + 三态路由 + trait 抽象）

- **`FingertipConfig` 配置模块** (`src-tauri/src/model/config.rs`)：JSON 序列化/反序列化配置，含 engine `base_url` / `mock_mode` 与 LLM/image/audio 三套 `provider` + `model` + `api_key_env` + `base_url`。AppState 持有 `config: RwLock<FingertipConfig>` + `config_path`，从 `%APPDATA%\com.fingertip.app\model_config.json` 启动时加载、不存在则写默认
- **三态路由决策纯函数** (`src-tauri/src/model/mod.rs`)：`route_engine_provider(provider)` → `LocalOnly` / `LocalFirst` / `CloudOnly`；`EngineClient` + `LocalClient` / `CloudClient` 三 trait 拆开握手
- **`JsonChatClient` + `AudioClient` trait**：抽象 LLM chat 与音频请求，编排器与适配器仅依赖 trait
- **`EngineClient`（健康检查 + OpenAI 兼容 chat/images/audio）**：跑本地 Python 服务失败时错误携带 `endpoint` + `status` 用于 UI 提示
- **`Orchestrator`** (`src-tauri/src/model/orchestrator.rs`)：拼 system prompt（`{ date, mood, style, summary_stats, theme_word, recent_artifacts }`）+ JSON 解析重试 + 字段校验（sentence ≤200 字 / amplitude_count=64 / 等），失败带 attempt 次数字段
- **云端客户端** (`src-tauri/src/model/cloud.rs`)：OpenAI `/v1/chat/completions` + `/v1/images/generations`，MiniMax `/v1/t2a_v2` 音频 + multimodal API
- **`build_clients` 三态路由工厂** (`src-tauri/src/generate/mod.rs`)：根据 FingertipConfig 决定走本地 Engine + 模拟，云端 OpenAI + MiniMax，或双端优先（先试本地，失败回退云端）

#### engine 层（独立 Python 微服务）

- **`engine/`** Python FastAPI 服务，独立 deliverable：
  - `engine/app.py`：FastAPI 入口 + `/health` `/v1/chat/completions` `/v1/images/generations` `/v1/audio/speech` 四个端点（OpenAI 兼容），mock 默认（确定性、可离线、无 API Key）
  - `engine/mock_backends.py`：假 chat 拼音乐 description + 句子；假图像返 PNG；假音频返合成正弦波 WAV
  - `engine/tests/test_app.py`：9 个 pytest 用例
  - `engine/requirements.txt` / `engine/README.md`：可选安装提示（`pip install -r engine/requirements.txt` + `python engine/app.py`）
- 默认运行 mock 后端——App 在无 Python 环境 / 无 API Key 时仍可用；用户配置切换到 real mode 走 OpenAI/MiniMax 真实 API

#### 数据结构 & 前端契约

- **Music 改元数据契约** (`src-tauri/src/generate/mod.rs`)：删 `notes: Vec<NoteSpec>`，加 `description: String`（编排器产出）+ `model: String`（生成所用模型 id）。V0.4 起的 Music 不再携带音符序列
- **Art 改元数据契约**：删 `pixels: Vec<PixelSpec>`、`width`/`height`，加 `description: String` + `model: String`。唯一物理载体：PNG 文件流（`art_png_path`）
- **`artifacts` 表加 `sentence` 列**（迁移 + upsert_with_sentence），`Artifact` 结构加 `sentence: String`（编排器生成时一次性写入，不再事后生成）
- **`generate_now` 返回 JSON 补 `art_png_base64`**（PNG 字节 base64 字符串 + 文件路径），前端可直接 `<img :src=...>` 渲染 PNG（绕过 base64 → 数据 URL → canvas 路径）
- **Artworks 改文件渲染**：原 `<canvas>` 用 64 像素渲染，改成 `<img :src="art_png_src">` 渲染 art_png_path（不再手工合成像素）
- **Artworks 句子字段**：读 `artifacts.sentence`（编排器一次性产出）替代原 top5_keys 拼凑
- **generate_sentence 改读 artifacts**：命令实现直接从 artifact_repo 读 sentence，不存在报明确错误（`artifact_has_no_sentence`）
- **Settings 模型接入表单**：`get_model_config` / `set_model_config` 命令 + Settings.vue 模型接入 section 含 toggle + provider/model/api_key + 测试连接按钮（Invoke engine /v1/chat，UI 显示成功/失败）

### Removed

- `src-tauri/src/generate/local/music.rs` + `local/art.rs` — v0.3 本地确定性算法（NoteSpec 序列 → WAV，PixelSpec 序列 → PNG）
- `src-tauri/src/generate/local/mod.rs` + `cloud/music.rs` + `cloud/art.rs` + `cloud/mod.rs` — v0.3 Cloud 占位 adapter
- `src-tauri/src/db/wav_encoder.rs` + `png_encoder.rs` — v0.3 手工合成 WAV/PNG
- `src-tauri/src/generate/sentence.rs` + `top5_keys_to_sentence` — v0.3 top-5 拼凑法（被编排器一次性产出取代）
- `FINGERTIP_USE_CLOUD` 环境变量 + `is_cloud_enabled()` 函数（被 FingertipConfig JSON 取代）
- 前端 `src/composables/useCanvasRender.ts` — Artworks 不再 canvas 渲染（PNG 直渲）
- `Music.notes` / `Art.pixels` / `Art.width` / `Art.height` 字段（PNG 是唯一载体）
- `Art.palette_seed` 字段（v0.3.0 已删，此处清理残留注释）
- v0.3 旧 `Local*Adapter` / `Cloud*Adapter` 工厂分支，env var 解析代码（`build_music_adapter` / `build_art_adapter` 改返 `ModelMusicAdapter` / `ModelArtAdapter`）

### Changed

- **架构**：单一 trait 抽象 → 多层 trait（`MusicAdapter`/`ArtAdapter` + `JsonChatClient`/`AudioClient` + `EngineClient`/本地/云端三面）以支持三态路由与可插拔
- **资源管理**：`Music`/`Art` 不再携带完整内容，PNG/WAV 文件落盘 + DB 元数据 + 编排器描述三者分离
- **句子生成时机**：从事后（用户点"生成句子"按钮时由 top-5 拼凑）改为事前（编排器在音乐/图像生成时一次性产出，存 DB）

### Fixed

- **T6 onDownloadArt 走错路径**：v0.3.10 `onDownloadArt` 仍走 `Art.pixels` 转 PNG 的旧逻辑，v0.4 改为 `downloadBlob(art_png_path, ...)` 读文件直传
- **T7 wav_analysis 真缺陷**：
  - 短帧 ≤ 4 bytes RIFF 数据被跳过的空洞 bug 修复（边缘帧路径加 `data_len > 0` 守卫）
  - 立体声反相音频（左右声道互为反相）原 RMS 恒为 0（互相抵消）的 bug 修复——改用单声道 `mixdown` 后再算 RMS
- **T11 集成**：`generate_now` 编排器失败 / adapter 失败错误路径补全（明确错误码而非 panic），超时从 30s 调到 60s 适配真实云端延迟

### Privacy

- **`src-tauri/src/privacy/` 模块**（v0.3.1 预留）正式接入：KeyringVault 存 API Key + PrivacyVault trait；Settings「模型接入」表单提交时校验 api_key_env 非空

### Tests

- **cargo lib 201 passed**（含 T14 + 3 配置路由测试）；集成 11（capabilities 3 + integration_hook_to_db 2 + perf 2 + setup 2 + tauri_config 2）；总计 **212 通过，0 警告**
- **vitest 87 passed**（含 T13/T14 Artworks + Settings 模型接入：4 个新测试）
- **engine pytest 9 passed**（T10 mock_backends + app.py）
- **vue-tsc --noEmit EXIT 0**
- **pnpm test:e2e 18 passed**（web 环境 invoke 必败的几个用例断言"错误可见但 UI 不崩"——v0.4 改 generate_now 数据流后结构断言仍绿）

### Docs

- `docs/plans/2026-08-07-llm-generation-design.md`：v0.4 架构设计（编排器 → 模型适配器 → 文件落盘 → wav_analysis；本地优先 / 云端兑底 三态路由）
- `docs/plans/2026-08-07-llm-generation-impl.md`：15-task 实施计划（T1-T15）+ 每 task 的 TDD 红绿循环记录

## [0.3.10] - 2026-08-03

桌面实测驱动的修复版：修通「下载 / 播放 / 二维码」三条链路，并移除冗余 UI。

### Fixed

- **音乐下载无效（v0.3.7 遗留）**：`onDownloadMusic` 仍调已 deprecated 的 `player.exportWav()`。改为新增 `downloadWavFromPath(wavPath, filename)`（readFile→Blob→Save As），Artworks 调之
- **下载在桌面被 capability 拒**：`capabilities/default.json` 补 `fs:allow-read-file`；新增 `tests/capabilities.rs` 3 个防回归测试（web 测不到，只能锁配置）
- **music_wav_path 到不了前端**：后端 `generate_now` 返回 JSON 补 `music_wav_path`/`art_png_path`（`augment_json_with_paths`）；前端 SubmitMood/Today/History 改 spread 不再丢字段；`GenerateNowResult` 类型补字段
- **播放进度卡 0:00**：先改 `Tone.now()` 计时（Player 无 `transport` 属性），再补 `await Tone.start()` 解锁挂起的 AudioContext（autoplay 策略）
- **二维码上传失败**：tmpfiles.org 被 GFW/SNI 封锁、0x0.st 上传停用，均经真实网络探测确认；换 **uguu.se**（实测上传+直链下载可用）。响应解析改 JSON `{success, files[0].url}`，所有失败分支带原文报错
- **下载失败静默**（code-review 🔴1）：新增 `downloadError` 可见提示（`role=alert`），失败/缺产物都在 UI 显示；顺手去 `as any`（code-review 🟡3）+ 修 `toBe(v,msg)` 类型错

### Changed

- **移除 Today 页「手动聚合」组件**：scheduler 已每 60s 自动聚合，Recalculate 按钮冗余且混合「聚合+生成+跳转」。6 卡→5 卡；空态 hint 改述「自动聚合」。后端 `trigger_run_summary_now` 保留

### Docs

- `docs/algorithm-explainer.html`：四指标 + 音乐/图像生成算法说明（摘自真实代码）

### Tests

- 新增：`Artworks.download.spec.ts`（3，失败可见性）、`capabilities.rs`（3）、`augment_json_with_paths`、`useTonePlayback` 进度/Tone.start、`parse_upload_response`（uguu JSON）
- 验收：cargo lib 154 + vitest 88 + e2e 18 全绿，vue-tsc 0 错

## [0.3.2] - 2026-07-28

### Added

- **生成产物持久化（v0.3.2 核心交付）**：解决"重启 App 看不到昨日作品"的最大体验债
  - 加 `artifacts` 表（`date PK` + `music_json` + `art_json` + `created_at`），与 `daily_summary` 一对一
  - 新 `db/artifact_repo.rs`：`upsert` / `read_by_date` / `list_recent` + 6 个单测
  - 新 Tauri command `get_artifact(date)`：返回 `{ music, art, date }` JSON，**与 `generate_now` 输出形态一致**，让 Artworks.vue 走同一渲染路径
  - `commands::generate_now` 改造：异步生成 music/art 后**再 lock 写库**（MutexGuard 不跨 await），让每次成功生成自动持久化
  - `generate_now_impl` 签名变化：从 `Result<String>` 改为 `Result<(Music, Art, String)>` —— 调用方拿到 Music/Art 用于写库，JSON 字符串给前端
  - `Music` / `Art` / `NoteSpec` / `PixelSpec` 补 `Deserialize` derive（v0.3.0 只 `Serialize`，v0.3.2 反序列化读库需要）
  - `History.vue` day card 改为可点击：调 `get_artifact(date)` 拉回历史作品 → 存 `store.generationResult` → 跳 `/artworks`。无 artifact 的日期静默不跳
  - 7 个新测试：`artifact_repo` 6 个（round-trip / idempotent / missing / list_recent 倒序+limit+0 边界）+ `get_artifact_round_trip_via_pure_impls` 1 个（端到端：upsert + get_artifact_impl 读回 JSON 字段保真 + 缺失日期返 `"null"`）

### Tests

- `cargo test --workspace`: 100 passed (lib 99 + main 0 + integration 2 + perf 2 + setup 2 + tauri_config 2)
- `pnpm test --run`: 80 passed (vitest)
- `pnpm test:e2e`: 12 passed (playwright web E2E)
- **总计 192 测试全绿，0 警告 0 失败**

## [0.3.1] - 2026-07-28

### Fixed

- **生成层真与键盘行为相关（v0.3.0 核心缺陷）**：`commands::generate_now` 之前传 `vec![]` 占位给 adapter，导致 LocalMusicAdapter / LocalArtAdapter 走空 events 分支，生成的 music / art 与"用户当天敲了哪些键"完全无关。v0.3.1 同步阶段多读 `EventRepo::list_by_date(&date)`，events 真正进 `LocalMusicAdapter.generate`（驱动 notes 数量 + duration_ms + amplitudes 模式）和 `LocalArtAdapter.generate`（驱动像素 x/y 散布）。新增回归测试 `generate_now_impl_uses_real_events_not_placeholder` 验证空 vs 真实 events 路径输出必须不同
- **Scheduler 不再覆盖用户 mood_word**：v0.3.0 scheduler 60s tick 调 `SummaryRepo::upsert(stats, theme, None)` 整体覆盖 mood_word，导致用户 `set_mood` 提交的心情 60s 内被重置为 NULL。v0.3.1 加 `SummaryRepo::upsert_stats`（INSERT ON CONFLICT 路径不更新 mood_word 字段），`summarize_date` 改用此方法，mood_word 由 `set_mood` 单点管理。新增 2 个回归测试：`summarize_writes_theme_word_but_preserves_mood` + `summarize_does_not_clear_mood_on_repeat`（跑 3 次 tick 仍保留 mood）
- **Settings 偏好风格终于生效**：v0.3.0 `Settings.vue` 的 style 选完是装饰（`ref('ambient')` 没接 store），`Today.vue:182` 调 `generate_now` 硬编码 `'ambient'`。v0.3.1 加 `useAppStore.style` + localStorage 持久化（key: `fingertip_style`），`Today.vue` 调 `generate_now` 改用 `store.style`，`SubmitMood.vue` 的 `selectedStyle` 默认值从 `store.style` 初始化，提交时写回 store 让 Settings 同步
- **cargo test doctest 解析失败（v0.3.0 release 漏修）**：`hook/writer.rs` 文件头 `//!` doc comment 里有 `rdev callback (OS 钩子线程, 永不阻塞)` 这种 `identifier (params)` 模式，被 rustdoc 启发式当 Rust 代码解析，导致 `cargo test --workspace` 在 doctest 阶段报 `unknown start of token: \u{ff08}`。修法：把 ASCII 伪代码围栏从 ```` ``` ```` 改成 ```` ```text ````，让 rustdoc 显式标记为 plain text
- **E2E About 页版本号断言硬编码 v0.2.0**：`pages-render.spec.ts:30` 断言 `getByText('FingerTip v0.2.0')`，v0.3.0 release 时 About.vue 改成 `v0.3.0` 但 E2E 没跟着改。改为正则 `/FingerTip v\d+\.\d+\.\d+/` 以后升级不用动测试
- **`unused import: migrations` 警告（commands.rs）**：`init_db` 函数只被测试调、生产代码不用，cargo 报 `unused import: crate::db::migrations::run_migrations`。加 `#[allow(dead_code)]` 标记为公共 API 预留接口
- **hook/writer.rs tests 块 2 个 unused import**：`use crate::db::migrations::migrations;` + `use std::sync::mpsc::sync_channel;` 是 v0.1 EventBuffer 时代残留，删 buffer.rs 时漏了。删除这两个 import 让 cargo 编译 0 warning

### Changed

- **删除 `src-tauri/src/generation/` 死代码**（25KB）：v0.2.x `MinimaxCloudAdapter` 时代的"纯算法映射"路径（`music_params / pixel_params / mapper / engine / style_presets`）被 v0.3 `generate/LocalXxxAdapter` 完全替代，但代码 + 7 个测试留着。删除整个 `generation/` 目录 + `lib.rs:11` 的 `pub mod generation;`。production 路径完全不走这里
- **删除 `src-tauri/src/hook/buffer.rs` + `listener.rs` 死代码**（7KB）：v0.1 `EventBuffer` 容量/时间双触发 + `HookListener` trait 抽象。v0.2.1 `HookWriter` 异步落库取代前者；lib.rs 直接用 `RdevListener` 没走 trait。删除两个文件 + `hook/mod.rs` 移除 `pub mod` + 改写 `rdev_listener.rs` 移除 `impl HookListener` 块
- **重写 `tests/integration_hook_to_db.rs`**：从 `HookListener → EventBuffer → SQLite` 端到端改为 `HookWriter → SQLite` 端到端。保留 2 个测试（end_to_end_events_persist_to_sqlite + high_volume_persistence_under_5_seconds），原 2 个 buffer-centric 测试删除
- **`rdev_listener` 方法改 `pub fn` 直暴露**：删 `HookListener` trait 后，`start()` / `stop()` 不再走 trait，文档从"实现 HookListener trait"改为"v0.3.1 直接暴露 API"
- **`summarize_date` 签名变化**：移除 `mood_word: Option<&str>` 参数（mood 不再由 scheduler 传入）。`spawn_loop` 的 `mood_source` 参数保留为 deprecated（_mood_source 不用，兼容旧调用方）
- **Settings.vue style 选项补 Lo-fi**：与后端 `generation/style_presets` 对齐（之前 3 个缺 lo-fi）

### Removed

- 整个 `src-tauri/src/generation/` 目录（5 个文件 + 7 个测试）
- `src-tauri/src/hook/buffer.rs`（v0.1 残留）
- `src-tauri/src/hook/listener.rs`（v0.1 残留）

### Privacy（v0.4 预留）

- `src-tauri/src/privacy/` 模块（PrivacyVault trait + KeyringVault + InMemoryVault）保留并加 TODO 注释。v0.3 删 MinimaxCloudAdapter 后无 API Key 需求，模块当前未被集成。v0.4 真实云端 AI 接入时使用

### Tests

- `cargo test --workspace`: 101 passed (lib 93 + integration 2 + perf 2 + setup 2 + tauri_config 2)
- `pnpm test --run`: 80 passed (vitest)
- `pnpm test:e2e`: 12 passed (playwright web E2E)
- **总计 193 测试全绿，0 警告 0 失败**

## [0.3.0] - 2026-07-25

### Changed（破坏性变更 - Rust 内部 API）

- **抽象层重构**：删除 `MinimaxCloudAdapter` / `orchestrator.rs` / `music.rs` / `art.rs` 等 v0.2.x 死代码 stub
- 引入 `pub trait MusicAdapter: Send + Sync` + `pub trait ArtAdapter: Send + Sync`（async）
- 新 `pub fn build_music_adapter()` / `pub fn build_art_adapter()`：根据 `FINGERTIP_USE_CLOUD=1` env var 切换 Local（默认）或 Cloud（占位）
- `AppState` 删除 `orchestrator` 字段
- 外部 IPC `generate_now` 输出 contract：新增 `{ "music": Music, "art": Art, "date", "mood", "style" }`，移除旧的孤兒路径字段

### Added

- **`Music` 真生成**（Local 默认路径）
  - `mood` 决定 BPM（calm 75 / happy 130 / energetic 140 / angry 155 / neutral 100）
  - `theme_word` 字符 hash 决定 amplitude 缩放
  - `amplitudes: Vec<f32>` 长度 64、值 ∈ [0, 1]
  - 6 个单元测试覆盖 mood bpm in range / amplitudes 范围 / theme 不同 amplitudes 不同 / 空 events 仍 valid
- **`Art` 真生成**
  - `mood` 决定 HSV 基础 hue（happy 30 / calm 120 / sad 210 / angry 0 / focused 250）
  - `theme_word` 字符决定每像素色相偏移
  - `key_code` + `theme_word` 决定 64 像素的 x/y 散布（绝对像素坐标 ∈ [0, 256) × ∈ [0, 256)）
  - 5 个单元测试
- **mood 持久化链路**：`SummaryRepo::upsert_mood` 真写 `daily_summary.mood_word`，不再永远是 NULL
- **新 Tauri command**：`set_mood(date, mood)` 调 `upsert_mood`，提交心情时同时持久化
- **`EventRepo::list_by_date`** 真查询指定日期的 key events（UTC 半开区间）
- **`tauri-plugin-dialog` + `tauri-plugin-fs`** 接入：下载作品走原生 Save As 对话框
- **`<canvas>` 真渲染**：作品页占位 `abstract` / `hello` 字面 div 删除，改用真 canvas 绘 64 像素
- **波形真数据**：36 波形 bar 高度从硬编码 `i*7%18` 改为读 `music.amplitudes` 真实值
- **时长真数据**：`0:00 / 0:14 / 0:32` 硬编码改为 `music.duration_ms` 真值 + 播放进度
- **`@tauri-apps/plugin-dialog` 在 Settings 新增「下载输出目录」 picker**：首次启动自动 `ensureDefaultDir()` 创建 `%APPDATA%\com.fingertip.app\downloads\`
- **E2E spec 创建**：`tests-e2e/v0.3-mood-and-render.spec.ts`（E2E-A SubmitMood + E2E-B 真渲染）
- **下载单测**：9 个 vitest 测试覆盖 `downloadBlob` Tauri 路径 + user 取消 + web fallback + `ensureDefaultDir`

### Removed

- v0.2.x 所有占位 UI：Settings「AI 接入」radio
- v0.2.x 过渡 state：Today.vue `dataSource = 'placeholder' | 'live'` → 简化 `loading: boolean`
- `Art.palette_seed` 字段（无 reader，YAGNI）
- 旧 `trigger_generate` Tauri command（返回 orphan 路径）
- 旧 `scheduler/driver.rs: list_by_session("")` 死代码
- 旧 `keymap.rs` 双份实现的 script 端（保留 Rust 版本，前端独立 `keycode-glyph.ts`）

### Fixed

- 拖网修复：`FINGERTIP_USE_CLOUD` 现在仅当 value = "1" 时启用（之前 `is_ok` 误启用于 `=0`/`=false`/空串）
- SET-MOOD 链路：用户提交"开心"不再被 lost，`daily_summary.mood_word` 真实写入 SQLite
- 不再让 `std::sync::MutexGuard` 跨 `.await`
- Cloud adapter 错误文案改用稳定信息（不再含源码树路径）

### Tests

- Rust `cargo test --lib`: **130+ passed**（Stage 1-3 共 +30 测试覆盖 Local 真生成 + mood 持久化 + schema）
- 前端 `pnpm test`: **77 passed**（9 个新增 download.ts 单测）
- `pnpm typecheck` (vue-tsc --noEmit): 0 error

### Bundle

- `tauri-plugin-dialog` + `tauri-plugin-fs` 集成：
  - `capabilities/default.json`：`dialog:default` + `fs:allow-write-file` + `fs:scope` allow list（`$APPDATA/$DOCUMENT/$DOWNLOAD/$DESKTOP`）
- `pnpm tauri build` 输出：
  - `FingerTip_0.3.0_x64-setup.exe` (NSIS, ~7.4 MB)
  - `FingerTip_0.3.0_x64_en-US.msi` (WiX, ~13.0 MB)

## [0.2.5] - 2026-07-25

### Fixed
- **🔴 开机自启闪退（核心 panic 修复）**：`src-tauri/src/lib.rs::run()` 原本在顶层用 `std::env::var("FINGERTIP_DATA_DIR").unwrap_or(".")` 解析 DB 路径，回退到 `"."` 当前工作目录。Windows HKCU Run 键启动时进程 cwd = `C:\Windows\System32\`（普通用户无写权限），导致 `db::init_at` 拒绝访问 → `expect("init DB")` panic → 进程闪退无任何日志（Windows `subsystem=windows` 不留 console trace）。修复：DB 初始化整段下沉到 `setup()` 闭包内，用 `app.path().app_data_dir()` 解析到 `com.fingertip.app` bundle identifier 对应的目录（`%APPDATA%\com.fingertip.app\fingertip.db`），保证任意 cwd 都写入合法位置。`AppState` 也从 `Builder::manage(state)` 改为 `app.manage(state)` 在 setup 内注册
- **bundle.identifier 警告**：NSIS 打包时提示 `com.fingertip.app` 以 `.app` 结尾与 macOS application bundle 冲突（Tauri 内部 non-fatal warning，build 仍成功）。暂不改 identifier，文档明示

### Added
- **正式安装包发布**：通过 `pnpm tauri build` 出 NSIS + MSI 双格式安装包
  - `FingerTip_0.2.5_x64-setup.exe` (7.4 MB, NSIS) — 用户首选，推荐用这个
  - `FingerTip_0.2.5_x64_en-US.msi` (13.0 MB, WiX) — 企业级 / SCCM 场景
- DB 自启 init 在 `%APPDATA%\com.fingertip.app\` 自动 `create_dir_all`，无需任何手动配置
- 自动注册 HKCU Run 自启动（安装时可选），卸载时自动清理

### Tests
- Rust `cargo test --lib` **111 passed** —— 修复未引入回归
- `pnpm tauri build` 完整产物输出两份 bundle

### Bundle
- 启动后窗口可见 `visible: true`（默认安装后 UX）；注册表 Run 项仍带 `--silent` 让二次启动后台常驻
- 仅当用户**勾选「开机自启动」**（安装器内嵌 NSIS 自启开关）才会写入注册表

## [0.2.4] - 2026-07-24

### Fixed
- **时区切换首页不变**：`get_today_hourly` / `get_today_key_count` 命令接受 `offset_minutes` 参数，按用户时区算"今日 0:00"边界；前端 `watch(timezoneOffsetMinutes, refresh)` 切时区立刻重拉数据
- **主题词总是 I,N,A**：`extract_theme_word` 把 top 5 ASCII 字符直接拼接（"INA"），改成输出**可读摘要** `{最高频字母} · {总按键数}`（如 "I · 303"）
- **开机自启反馈错觉**：注册表项实际存在但用户感受不到。前端加 Tauri 环境检测（`window.__TAURI_INTERNALS__`）+ 提示文案说明实际注册路径 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\FingerTip`

### Added
- `tz_today_range_ms(offset_minutes)` 后端纯函数 —— 按用户时区算"今日 0:00" UTC 毫秒范围
- 6 个新 theme 单元测试（覆盖 INA 高频、单字符、纯修饰键、同 count 排序等边界）
- Settings.vue 加 `<n-alert type="warning">` 提示 + `isTauri` runtime 检测（web 模式下 toggle disabled）

## [0.2.3] - 2026-07-23

### Fixed
- **作品页 UI 间距 hotfix**：用户反馈画作块右半空白 + 音乐块元素挤，v0.2.2 的间距调整不充分
  - 画作块：grid `1.55fr : 1fr` → `1fr : 1fr` 等宽 + 画布 `width:100% height:320px` 撑满卡片
  - 音乐块：拆 3 段（info / waveform-row / controls），波形独立占行 36 条
  - 控件段加时长显示（current / total）
  - 下载按钮改圆角胶囊（border-radius 100px + 字号 13 + weight 500 + 图标/文字分 span）
  - 播放/暂停图标动态切换（▶ / ⏸）
  - 整体 padding `sp-5 → sp-6`、`sp-6 → sp-8` 增加呼吸空间

## [0.2.2] - 2026-07-23

### Added
- **自定义时区功能**：设置页时区下拉（UTC-12:00 ~ UTC+14:00，27 档），全部日期显示（Today / Artworks / History）按用户 offset 计算
- `src/utils/timezone.ts` —— `epochToLocal / todayStrInTz / formatDateCN / buildTimezoneOptions / detectLocalOffsetMinutes / loadStoredOffset / saveStoredOffset`
- `src/utils/keyCodeGlyph.ts` —— VK code → 人可读字符（控制键用 Unicode 符号 ⌫ ⇥ ↵ ⎵ ⇧ ⌃ ⌥ ⌘ ←↑↓→ F1-F12；防御性处理负数/NaN/Infinity/小数）
- `useAppStore.timezoneOffsetMinutes` + 自动持久化到 localStorage
- 设置页新增「自动检测」按钮（一键设到本机时区）

### Changed
- **节奏指纹（Top 5 按键）**：用 `keyCodeToGlyph` 替换 ASCII-32-126 兜底，所有真实按键渲染人可读符号
- **音乐 UI 间距**：ft-art-block padding sp-4 → sp-5；ft-music-meta 长文本 2 行 ellipsis；播放器 padding 舒展（sp-3 → sp-5）；波形高度 28 → 32 + 左侧分隔线
- **日期显示**：硬编码 "2026 年 7 月 17 日" → 从 `store.generationResult.date` 读取（Today 同样切换）

### Fixed
- 节奏指纹上一排问号问题（keycode < 32 → "?"，现在映射成 Unicode 符号）
- Space (32) 显示为真空格让用户以为没渲染 → 改为 "⎵"

### Tests
- `src/utils/__tests__/keyCodeGlyph.spec.ts` —— 35 个 test cases 覆盖全部真实按键 + 防御性
- `src/utils/__tests__/timezone.spec.ts` —— 12 个 test cases 覆盖 offset / persistence / 边界 / clamp
- 前端 68/68 全过（4 文件），typecheck 0 error

## [0.2.1] - 2026-07-23

### Changed
- **Artworks 页布局**：画作与音乐改为并排 grid（1.55fr : 1fr），整体不再溢出 1100×760 视口
- **画作 canvas**：aspect-ratio 21/9 → 16/9 + 固定 height 180px，让高度受控不依赖宽度
- **音乐播放器**：拆分为两段式（顶：标题+副标题+波形；底：播放+下载按钮），播放按钮从 52×52 缩到 36×36
- **键写入路径（核心性能修复）**：rdev 钩子线程改为 try_send 到 bounded channel，独立的 HookWriter 后台线程负责 batch flush（50 条 OR 100ms）
- **Music 卡片 padding**：sp-5 sp-6 → sp-3 sp-5（紧凑化）

### Added
- `src-tauri/src/hook/writer.rs` —— HookWriter 异步落库器，owns `Arc<Mutex<Connection>>` + 一个 mpsc::SyncSender
  - WriterStats 暴露 `received / written / dropped` 三个原子计数器（监控丢失率）
  - 4 个新单元测试覆盖：clone & send / flood 不阻 / 持久化总数对齐 / 不 panic
- **SQLite PRAGMA 性能优化**：
  - `journal_mode=WAL`：写不阻塞读 + 移动硬盘友好
  - `synchronous=NORMAL`：崩溃丢 ≤1s（用户已接受）vs 每键 fsync
  - `temp_store=MEMORY`、`cache_size=-20000`、`mmap_size=256MB`
- `prepare_cached` —— writer 线程缓存 prepared statement，避免每键重新 prepare
- 4 个 channel 常量（`BATCH_SIZE=50`、`FLUSH_INTERVAL_MS=100`、`CHANNEL_CAPACITY=4096`）便于调优

### Fixed
- **rdev 钩子线程不再被 SQLite 阻塞** —— `try_send` 永不阻塞，channel 满时丢弃（不卡 OS 输入）

### Changed
- **记录机制**：键写入语义从「EventBuffer 5 + 1s timer + insert_many 批次」改为**每按一键即时 INSERT**
  - 回归即时可见性优先（用户明确选择接受此取舍）
  - 已知影响：USB 移动硬盘场景下可能再次出现卡顿
- **开发服务器端口**：固定 `1420`（`strictPort: true`，端口占用即报错）配合 Playwright 截图调优
- **历史页**：去除 mock 数据，接入真实 `daily_summary` 列表
- **心情页**：去除 `trigger_generate` mock 路径文案，接入 `generate_now` 真实生成 → 自动跳转 Artworks

### Added
- 心情页真作品生成：用户提交心情 + 风格 → 立刻驱动 `generate_now` → 跳转到 Artworks 看到 music/pixels 渲染 + 可下载
- 历史页真记录展示：从 SQLite 读最近 N 天 `daily_summary`（`list_summaries` command）
- Playwright E2E 测试覆盖三条核心流程：SubmitMood → Artworks / 今日按键 → History / 下载按钮
- CHANGELOG.md（本文档）

## [0.1.0] - 2026-07-19

### Added
- rdev 键盘 Hook 后台捕获按键 → SQLite 持久化（`fingertip.db`）
- 21 个 Rust 模块 + 101 个单元测试
- Vue 3 + TypeScript + Vite + Naive UI + Pinia + Vue Router 前端
- 生成层（纯算法，无 AI 调用）：键盘 5 维 → MusicParams / PixelParams
- Tauri Command：hook 启停、today summary、key count、hourly distribution、generate_now、autostart toggle
- 系统托盘 + Show/Hide + Quit
- 开机自启动 + 静默后台模式（`tauri-plugin-autostart`）
- 作品下载：导出 wav / png
- Linux/Windows/macOS 跨平台支持
