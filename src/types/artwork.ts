// v0.4: Music/Art 由「模型输出的结构化规格」改为「模型产物的元数据 + 文件路径」
// 对齐 Rust src-tauri/src/generate/mod.rs

export interface Music {
  bpm: number               // v0.4: 固定 0（不再从 events 估算）
  duration_ms: number
  amplitudes: number[]      // 长度 64, 值 ∈ [0, 1]（v0.4: 从 WAV 文件分析得到）
  mood: string | null
  style: string
  theme_word: string
  description: string       // v0.4: 编排器产出的音乐描述（自然语言）
  model: string             // v0.4: 模型标识（"local" / "step-audio" / "MiniMax-music" 等）
}

export interface Art {
  theme_word: string
  mood: string | null
  description: string       // v0.4: 编排器产出的画面描述（自然语言）
  model: string             // v0.4: 模型标识（"local" / "dall-e-3" / "minimax-image" 等）
}

export interface GenerateNowResult {
  music: Music
  art: Art
  date?: string    // 后端透传（兼容过渡）
  mood?: string    // 后端透传
  style?: string   // 后端透传
  // v0.3.9-fix: generate_now / get_artifact 透传产物绝对路径（Artworks 播放/下载依赖）
  music_wav_path?: string | null
  art_png_path?: string | null
  // v0.4: 编排器产出的一次成型描述（Music/Art 公用），前端 Artworks 挂载时直接读
  sentence?: string | null
  // v0.6.0: 编排器产出的英文句子（用于落地页 + Artworks 中英分行展示）
  english_sentence?: string | null
  // v0.6.0: 主题词解释文案（短句，对今天主题词的含义补充）
  theme_explanation?: string | null
  // v0.6.0: 用户选择的时间窗口标签（如 "06:00–08:00"），无窗口则 null
  time_range_label?: string | null
  // v0.6.0: AI 键盘诊断（funny_summary）—— 2 句话、40-80 字的搞笑总结
  funny_summary?: string | null
}

export const AMPLITUDE_SAMPLE_COUNT = 64