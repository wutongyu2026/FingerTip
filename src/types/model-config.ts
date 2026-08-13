// v0.4 T14: 与 src-tauri/src/model/config.rs 的 FingertipConfig 对齐
//
// Rust 端用 snake_case 序列化；TS 侧做同样命名约定，转换语义由调用方负责。
//
// 注意：Rust 的 `LlmConfig.local_gguf` 是 `Vec<String>`（多 GGUF 路径），
// TS 表单层为用户体验用「逗号分隔字符串」承载，调用 `FingertipConfigDefault`
// 时 split / join —— 不直接对接 Rust（避免表单处理多行 input 的复杂度）。
// wire 形态（FingertipConfigWire）是 invoke 入参形状；与表单态只差 `local_gguf`。

/// 能力路由三态：本地优先 / 仅本地 / 仅云端。
/// 与 Rust `CapabilityMode` 一一对应（snake_case 序列化）。
export type CapabilityMode = 'local_first' | 'cloud_only' | 'local_only'

/** 本地引擎（Python FingerTip-Engine）开关 + 地址。 */
export interface EngineConfig {
  enabled: boolean
  /** 默认 `http://127.0.0.1:8765` —— 与 Rust EngineConfig::default 对齐 */
  base_url: string
}

/** LLM 编排器配置（表单态：`local_gguf` 是逗号分隔字符串）。 */
export interface LlmConfig {
  mode: CapabilityMode
  /** 表单层语义：逗号分隔多路径（如 "/a.gguf, /b.gguf"）。 */
  local_gguf: string
  cloud_base: string
  cloud_key: string
  cloud_model: string
}

/** 图像生成配置。 */
export interface ImageConfig {
  mode: CapabilityMode
  local_model_path: string
  cloud_base: string
  cloud_key: string
  cloud_model: string
}

/** 音频/TTS 生成配置。 */
export interface AudioConfig {
  mode: CapabilityMode
  /** 命名遵循 Rust 字段名（不动 audio 用 minimax_ 前缀以保持与 Rust 对齐） */
  minimax_base: string
  minimax_key: string
  minimax_model: string
}

/** 模型生成总配置（表单态）。 */
export interface FingertipConfig {
  engine: EngineConfig
  llm: LlmConfig
  image: ImageConfig
  audio: AudioConfig
}

/** LLM wire 形态：`local_gguf` 是数组（与 Rust `Vec<String>` 对齐）。 */
export interface LlmConfigWire {
  mode: CapabilityMode
  local_gguf: string[]
  cloud_base: string
  cloud_key: string
  cloud_model: string
}

/** FingertipConfig 的 wire 形态（invoke 入参）。 */
export interface FingertipConfigWire {
  engine: EngineConfig
  llm: LlmConfigWire
  image: ImageConfig
  audio: AudioConfig
}

/** MiniMax 云端 API 默认基址（与 Rust config.rs `MINIMAX_API_BASE` 对齐，防 placeholder 陷阱）。 */
export const MINIMAX_API_BASE = 'https://api.minimaxi.com'
/** 编排器 LLM 默认模型（实测 /v1/models 在列）。 */
export const LLM_CLOUD_MODEL_DEFAULT = 'MiniMax-M3'
/** 图像默认模型（实测 /v1/image_generation 有效）。 */
export const IMAGE_CLOUD_MODEL_DEFAULT = 'image-01'
/** 音乐默认模型（实测 /v1/music_generation 有效；Music-3.0-free 不存在）。 */
export const AUDIO_CLOUD_MODEL_DEFAULT = 'music-3.0'

/**
 * 构造默认值 —— 字段与 Rust `FingertipConfig::default()` 一一对应。
 * 表单首次进入未拿到后端配置时填这里（base/model 预填真实值，用户只需补 key）。
 */
export function FingertipConfigDefault(): FingertipConfig {
  return {
    engine: { enabled: false, base_url: 'http://127.0.0.1:8765' },
    llm: { mode: 'local_first', local_gguf: '', cloud_base: MINIMAX_API_BASE, cloud_key: '', cloud_model: LLM_CLOUD_MODEL_DEFAULT },
    image: { mode: 'local_first', local_model_path: '', cloud_base: MINIMAX_API_BASE, cloud_key: '', cloud_model: IMAGE_CLOUD_MODEL_DEFAULT },
    audio: { mode: 'local_first', minimax_base: MINIMAX_API_BASE, minimax_key: '', minimax_model: AUDIO_CLOUD_MODEL_DEFAULT },
  }
}

/**
 * 表单层（逗号字符串）→ 数组（Rust 序列化产物）。
 * 空字符串 → 空数组（避免后端出现 `[""]` 这种「有一条空路径」的脏数据）。
 */
export function llmLocalGgufToArray(s: string): string[] {
  return s
    .split(',')
    .map((x) => x.trim())
    .filter((x) => x.length > 0)
}

/**
 * 数组（Rust 序列化产物）→ 表单层（逗号字符串）。
 * 缺失 / 异常类型时返空字符串（前端展示态稳定）。
 */
export function llmLocalGgufFromArray(arr: unknown): string {
  if (!Array.isArray(arr)) return ''
  return arr.filter((x): x is string => typeof x === 'string').join(', ')
}

/**
 * 表单态 → wire 态（invoke 入参）。
 * 唯一差异：把 `llm.local_gguf` 字符串按逗号拆成数组。
 */
export function toWire(cfg: FingertipConfig): FingertipConfigWire {
  return {
    ...cfg,
    llm: { ...cfg.llm, local_gguf: llmLocalGgufToArray(cfg.llm.local_gguf) },
  }
}

/**
 * 后端 JSON → 表单态（mount 时回填）。
 * 后端数组 → 表单「逗号字符串」。
 */
export function fromWire(raw: FingertipConfigWire): FingertipConfig {
  return {
    ...raw,
    llm: { ...raw.llm, local_gguf: llmLocalGgufFromArray(raw.llm.local_gguf) },
  }
}