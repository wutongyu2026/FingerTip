# FingerTip 生成层设计文档

| 项目 | FingerTip |
|---|---|
| 模块 | 生成层（音乐 + 数字画作，算法实现） |
| 日期 | 2026-07-18 |
| 状态 | v1.0 — 经头脑风暴确认，转入实施规划阶段 |
| 上游 | 键盘行为数据 + 心情词 + 主题词 + 风格选项 |
| 输出 | 1-2 分钟 .wav + 一张 .png（位置：~/FingerTip/outputs/{date}/）|

---

## 一、项目背景与目标

### 1.1 背景
FingerTip 已完成键盘监听 → SQLite 记录 → 日聚合 → UI 展示（Today/Artworks/Settings 等）。**生成层** 是把"键盘节奏"翻译为**音频 + 画作**的最后一步。已在 brainstorming 阶段确认：

- 已选 **方案 C**：Rust 算参数 + 前端 Tone.js / Canvas 渲染
- **AI 可选**：现有 `MusicAdapter / ArtAdapter` trait 已预留，首版算法实现不依赖 AI
- **触发**：点 Recalculate 按钮一次性生成

### 1.2 目标
1. 用户按完一天键 → 点 Recalculate → 几秒内生成 1-2 分钟音频 + 一张画作
2. **自动跳到 Artworks 页面**（无新逻辑：router.push）
3. 算法基于 5 维键盘行为数据，**确定性种子 + 受心情词调制的随机扰动**
4. 隐私优先：所有数据已在本地；算法不会发送任何按键序列到外部

### 1.3 非目标（YAGNI）
- ❌ AI 模型接入（trait 已预留，未来再加）
- ❌ 歌词生成 / MIDI 输入输出 / 实时动画 / 24fps 视频
- ❌ 多用户协同 / 离线 LLM
- ❌ 自动 BPM 推断（首版算法公式）

---

## 二、需求摘要（已通过头脑风暴澄清）

### 2.1 已决定的设计参数
| 维度 | 决策 |
|---|---|
| 音乐长度 | **1-2 分钟**（中间档） |
| 音色来源 | **Tone.js**（高级合成框架，~150KB） |
| 图片形态 | **Canvas 2D 静态 PNG** |
| 触发时机 | **Recalculate 时一次性生成** |
| AI 可选接口 | 现有 trait 切换（**什么都不动**） |
| 4 风格映射 | **预设参数集**（A minor pent / D dorian / C minor harm / F major） |
| UI 集成 | Recalculate 完成后 **`router.push('/artworks')` 自动跳转**（无新逻辑）|

### 2.2 输入数据（已有）
```
input:
  total_keys:        u32           // 今日总按键
  hourly[24]:        [u32; 24]     // 每小时按键数
  theme_word:        String         // 高频键位主题词
  mood_word:         String         // 用户心情词
  style:             String         // ambient / jazz / cinematic / lo-fi
  key_sequence:      Vec<KeyEvent>  // 今日按键序列（按时间排序）
```

---

## 三、架构总览（方案 C）

```
┌─────────────────────────────────────────────────────────────────┐
│  前端 (Vue 3 + TypeScript)                                       │
│  ┌─────────────────┐  ┌────────────────────┐                   │
│  │ Recalculate 按钮 │  │ 生成结果面板        │                   │
│  └─────────────────┘  │ ▶ 播放 / 导出 wav   │                   │
│        │                │ 🖼 画作预览 / 下载  │                   │
│        ▼                └────────────────────┘                   │
│  invoke('generate_now')                                          │
│        │ router.push('/artworks')                                │
└────────┼───────────────────────────────────────────────────────┘
         ▼ Tauri IPC（JSON 参数，非原始事件）
┌──────────────────────────────────────────────────────────────────┐
│  Rust 后端                                                          │
│  ┌────────────────┐    ┌─────────────────────┐                  │
│  │ summary JSON    │──▶│ GenerationEngine    │                  │
│  │ + style + mood   │    │ • 5 维 → 音符列表    │                  │
│  └────────────────┘    │ • 5 维 → 像素列表    │                  │
│                         │ • 风格预设查表       │                  │
│                         └─────────────────────┘                  │
│                                                                    │
│  MusicAdapter / ArtAdapter trait（预留 AI，**首版不调用**）      │
└──────────────────────────────────────────────────────────────────┘
```

### 数据流（点 Recalculate 那一刻）

```
1. 前端 invoke('generate_now', { date, mood, style })
2. Rust 端：
   a) 读 daily_summary（total_keys / theme_word / mood_word / hourly）
   b) 读今日 key_sequence（按 timestamp_ms 排序）
   c) GenerationEngine.compute(...) → (MusicParams, PixelParams)
   d) 序列化为 JSON 返给前端
3. 前端 store 缓存两份 params
4. router.push('/artworks') 自动跳转
5. Artworks.vue onMounted：从 store 读 params
   - MusicPlayback（Tone.js）按 MusicParams 渲染 1-2 分钟 → play/stop + 下载 wav
   - ArtRender（OffscreenCanvas）绘 PixelParams → PNG 预览 + 下载
```

---

## 四、5 维 → 音乐 映射规则

| 键盘维度 | 音乐参数 | 映射规则 |
|---|---|---|
| **按键顺序**（序列） | **音符序列** | 第 N 个按键 → 音阶第 (N % scale_len) 音 |
| **按键速度 / 间隔** | **音符长短** | 间隔 < 100ms → 八分；100-300ms → 四分；> 300ms → 附点二分 |
| **停顿**（gap > 2s） | **小节分隔 + 1 拍休止** | 每次大停顿加 1 拍休止 |
| **按键总次数 / 密度** | **BPM**（间接） | `BPM = 60 + min(60, log2(total_keys) * 12)` |
| **时段分布**（hourly） | **段落结构 / 力度** | 高峰小时段 → pad + 大力度；凌晨 → minimal + 三和弦 |
| **删除 / 重复** | **装饰音 vs 主旋律** | Backspace → 装饰；普通键 → 主旋律 |
| **主旋律终止** | **收尾渐弱** | 序列末尾自动 4 拍渐弱（algorithm jitter ±10ms）|

### 4.1 4 风格预设

| Style | Scale | 默认音色 (Tone.js) | BPM 区间 | 颜色调 |
|---|---|---|---|---|
| **ambient** | A minor pentatonic | `Tone.PolySynth`（pad） | 60-80 | 暖橙 #D67B4F + 互补蓝 |
| **jazz** | D dorian | `Tone.FMSynth`（electric piano） | 90-130 | 蓝紫 #4F4FB1 + 米白 |
| **cinematic** | C minor harmonic | `Tone.Sampler`（弦乐采样） | 70-100 | 深红 + 金 |
| **lo-fi** | F major | `Tone.MonoSynth`（rhodes） | 80-100 | 暖黄棕 + 暖灰 |

---

## 五、5 维 → 图像 映射规则

| 键盘维度 | 图像参数 |
|---|---|
| **按键次数**（每个键） | **颜色饱和度**（按得多 → 饱和度更高）|
| **按键位置**（A-Z + 数字） | **颜色 hue**（A-S → 0-120°，D-F → 120-240°，G-L → 240-360°，数字 → 加灰白）|
| **按键顺序**（序列） | **空间分布**：径向扫开（开头 → 圆心，向外扩散）|
| **按键频率** | **形状密度**：高频键 → 多圆点；低频 → 稀疏散布 |
| **时段分布**（hourly） | **画面层次**（半透明叠加）|
| **停顿** | **空白留白区** |
| **主题词** | **调色板种子**：`hue_seed = (theme_word.len() as f32 * 12.0) % 360` |
| **4 风格** | **几何基底**：ambient=径向圈、jazz=自由、cinematic=对称镜面、lo-fi=碎片拼贴 |

---

## 六、组件清单

### 6.1 Rust 端（6 个新模块）

```
src-tauri/src/generation/
  ├── mod.rs              # pub mod
  ├── engine.rs           # GenerationEngine::compute(...) → (MusicParams, PixelParams)
  ├── music_params.rs     # struct MusicParams + MusicNote (serde)
  ├── pixel_params.rs     # struct PixelParams + Pixel
  ├── style_presets.rs    # StylePreset + fn preset_for(name)
  └── mapper.rs           # map_keys_to_music / map_keys_to_pixels (纯函数, TDD 核心)
```

### 6.2 前端（2 个 composable + 复用 Artworks.vue）

```
src/composables/
  ├── useTonePlayback.ts    # Tone.js wrapper, 渲染 MusicParams
  └── useCanvasRender.ts     # OffscreenCanvas wrapper, 渲染 PixelParams

(src/views/Artworks.vue 已存在 —— 加 onMounted 读 params + 渲染)
```

### 6.3 输出去向
```
~/FingerTip/outputs/{date}/
  ├── summary.json     # 已有
  ├── theme_word.txt   # 已有
  ├── music.wav        # 新增（Tone.js 录音导出）
  └── artwork.png      # 新增（OffscreenCanvas → PNG）
```

---

## 七、UI 集成（**无新逻辑原则**）

```
[Today.vue]
  Recalculate 按钮 onClick:
    1. invoke('trigger_run_summary_now')  ← 已有
    2. invoke('generate_now', {date, mood, style})  ← 新增
    3. 把两个 params 存到 Pinia store
    4. router.push('/artworks')  ← 自动跳转

[Artworks.vue] 增强 (已有页面)
  onMounted: 从 store 读 params → 启动 MusicPlayback + ArtRender
```

不引入：消息总线、轮询状态、WebSocket 等。**只用 Tauri Command + Pinia + Router 三件套**。

---

## 八、错误处理

| 错误 | 触发 | 谁处理 | UX |
|---|---|---|---|
| 键序 parse 失败 | 时间戳异常 | Rust `parse_sequence` Err | "数据异常，fallback 到 ambient" |
| total_keys = 0 | 今日无输入 | Rust guard clause → 默认 BPM 72 | 静默，仍有音乐 |
| 风格名不识别 | style = "unknown" | Rust `preset_for` → ambient | "未知风格，备用 ambient" |
| Tone.js init 失败 | Safari 旧 | Vue try/catch | "🎵 音频暂不支持" |
| OffscreenCanvas 失败 | Firefox 旧 | Vue fallback main canvas | 卡片占位文字 |
| 参数 JSON 失败 | 后端 schema 变更 | Vue 兜底空音符 | "作品生成中..." |
| Recalculate 失败 | 网络/权限 | Loading + disable 1s | "暂时无法生成" |
| 未来 AI trait panic | 未来 | catch_unwind → 走算法 fallback | 无感知 |

---

## 九、测试策略

```
1. Rust Unit (cargo test):
   - MusicParams / PixelParams serde round-trip
   - StylePreset 4 套常量
   - map_keys_to_music 纯函数 ~10 个测试
     (empty / simple / bpm_from_total / pauses → rests /
      style_preset_application / ...)
   - map_keys_to_pixels 纯函数 ~6 个测试
   - GenerationEngine::compute (full pipeline)
2. Rust Integration:
   - generate_now Command 流程
3. Tauri Smoke (已有 setup_smoke + 加新的):
   - 防 panic 回归
4. Frontend (Vitest):
   - useTonePlayback: mock Tone.js
   - useCanvasRender: mock OffscreenCanvas
   - Artworks.vue: 拿参数 → 渲染 mock
5. E2E (Playwright):
   - 点 Recalculate → 自动跳 Artworks → 看到画作 + 播放器
```

### Mock 策略
- **Tone.js mock**：`vi.mock('@tonejs/...')` 返回 stub 事件回调
- **OffscreenCanvas mock**：手写 `__tests__/canvasMock.ts`，canvas-like API（不依赖 DOM）
- 不做真实音频输出测试（浏览器自动化不可靠）；E2E 只验证 DOM 出画作 + 播放器

---

## 十、性能基线

| 维度 | 阈值 |
|---|---|
| 后端 compute（100 事件）| < 50ms |
| 后端 compute（10k 事件）| < 500ms |
| Tone.js 渲染（1-2 分钟音频）| < 100ms init |
| OffscreenCanvas 渲染 PNG | < 500ms |
| 端到端（按钮 → 看到作品）| < 3 秒 |

---

## 十一、兼容性 / 不动的现有物

| 现有 | 处理 |
|---|---|
| `MusicAdapter / ArtAdapter` trait | **不动**（AI 路径仍预留） |
| `MinimaxCloudAdapter` stub | **不动**（保留） |
| `GenerateOrchestrator` | **不动**（AI 路径仍可用） |
| `trigger_generate` Command | **不动**（AI 路径仍可用） |
| `MusicPrompt / ArtPrompt` | **不动**（与 MusicParams / PixelParams 并存） |
| `daily_summary` 表 | **不动**（数据已有） |
| `trigger_run_summary_now` | **不动**（Recalculate 第一阶段仍调用它） |

**新增**：`MusicParams / PixelParams` 两套新数据结构 + `generate_now` 新 Command + 4 套算法实现 + 前端 2 个 composable。

---

## 十二、风险评估

| 风险 | 概率 | 严重度 | 缓解 |
|---|---|---|---|
| Tone.js 体积 150KB 影响首屏 | 中 | 低 | Artworks.vue 懒加载 |
| Canvas 在 4K 卡 | 低 | 中 | render 前缩到 1920×1080 |
| 算法让音乐显得"机械" | 高 | 中 | 加 jitter（音符时长 ±10ms 随机）+ mood 影响 random |
| 用户期望"每天听同一首" | 低 | 高 | mood/theme 变化让结果自然不同（不追求复现性）|

---

## 十三、TDD 任务切分（写作 plans 阶段会拆开 Task 列表）

```
Phase A - 数据结构（2 Task）
  T1: MusicParams + MusicNote + serde 测试 + 实现
  T2: PixelParams + Pixel + serde 测试 + 实现

Phase B - 风格预设（1 Task）
  T3: StylePreset 4 套常量 + preset_for(name) 测试

Phase C - 核心映射 纯函数（5 Task）
  T4: map_keys_to_music(empty) → 空序列
  T5: map_keys_to_music(simple_sequence) → 音符序列
  T6: map_keys_to_music(bpm_from_total_keys)
  T7: map_keys_to_music(pauses_emit_rests)
  T8: map_keys_to_music(style_preset_application)

  T9: map_keys_to_pixels(empty/simple/density_from_frequency)
  T10: map_keys_to_pixels(color_from_position)

Phase D - 编排（1 Task）
  T11: GenerationEngine::compute (full pipeline)

Phase E - Tauri Command（2 Task）
  T12: generate_now Command + 测试
  T13: lib.rs 注册新 command

Phase F - 前端合成（2 Task）
  T14: useTonePlayback (mock Tone.js)
  T15: useCanvasRender (mock OffscreenCanvas)

Phase G - UI 集成（2 Task）
  T16: Today.vue OnRecalculate 扩展
  T17: Artworks.vue 增强 (从 store 读 params)

Phase H - E2E（1 Task）
  T18: Playwright 验证 Recalculate → Artworks 渲染
```

共 18 个 TDD Task。

---

## 十四、目录变更

新增（不改现有）：
- `src-tauri/src/generation/` (6 个 .rs 文件)
- `src/composables/useTonePlayback.ts`
- `src/composables/useCanvasRender.ts`

变更（仅 .vue）：
- `src/views/Today.vue`：Today.vue 的 onRecalculate 多调用一次 generate_now + store + router.push
- `src/views/Artworks.vue`：onMounted 从 store 读 params + 调 composable

依赖新增：
- `src-tauri/Cargo.toml`：`serde_json`（已有）、`tonic` / `hound`（如 Rust 端要 wav 写出可加；前端 Tone.js 自然处理 wav 导出）
- `package.json`：`tone` (~150KB)

---

## 十五、变更记录
- v1.0 (2026-07-18)：初版，经头脑风暴 5 问 + 4 节设计 全部确认
