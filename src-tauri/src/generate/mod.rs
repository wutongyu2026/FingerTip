//! v0.4: Music/Art 由「模型输出的结构化规格」改为「模型产物的元数据 + 文件路径」。
//!
//! 历史：
//!   - v0.2.x: `MinimaxCloudAdapter` 是 stub 不联网，已在 v0.3 删除
//!   - v0.3+: `LocalMusicAdapter` / `LocalArtAdapter` 是真生成（NoteSpec 序列 + PixelSpec 列表）
//!   - v0.4+: Music/Art 承载「模型 + 描述 + 元数据 + 64 桶振幅」，不再承载规格结构。
//!            真实图片由 PNG 文件承载（前端用 `<img>` 渲染），真实音频由 WAV 文件承载。
//!            MusicPrompt/ArtPrompt 加 `description: Option<String>` —— T8/T9 适配器
//!            消费编排器产出，再喂给模型。
//!
//! T11: 删 `local/` `cloud/` `sentence` 子模块；MusicAdapter 改返 `MusicOutcome`
//! (Music + WAV 字节)。适配器只产 wav 字节，振幅分析在 `generate_now_impl` 层做。

pub mod model_art;
pub mod model_music;
pub mod upload;

use serde::{Deserialize, Serialize};

/// 1 个 Music 包含多少采样点给波形 bar 用（值域 [0.0, 1.0]）
pub const AMPLITUDE_SAMPLE_COUNT: usize = 64;

/// v0.4: Music 产出元数据 + 64 桶振幅 + 描述/模型标识。
///
/// 设计取舍：
///   - 模型出音频（WAV 文件落盘），不再有 NoteSpec 序列 —— notes 字段删除
///   - 振幅数据从 WAV 分析得到（T7 wav_analysis 模块），64 桶给前端波形 bar
///   - description = 编排器产出的「这段音乐是什么」（T11 写入 artifacts）
///   - model = 实际生成模型（"local" / "step-audio" / "MiniMax-music" 等）
///   - bpm 设为 0：v0.4 不再从 events 估算（保留字段给未来）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Music {
    pub bpm: u32,
    pub duration_ms: u64,
    pub amplitudes: Vec<f32>,
    pub mood: Option<String>,
    pub style: String,
    pub theme_word: String,
    /// v0.4: 编排器产出的音乐描述（自然语言）
    pub description: String,
    /// v0.4: 模型标识（"local" / "step-audio" / "MiniMax-music" 等）
    pub model: String,
}

/// v0.4: MusicPrompt 给 MusicAdapter 的输入。
///
/// description = 上游 T5 编排器从 daily_summary 推出的自然语言描述；
/// 本地 LocalMusicAdapter 不消费，T8 ModelMusicAdapter 喂给模型。
#[derive(Debug, Clone)]
pub struct MusicPrompt {
    pub events: Vec<crate::hook::event::KeyEvent>,
    pub mood: Option<String>,
    pub style: String,
    pub theme_word: String,
    /// v0.4: 编排器产出的描述（None = 旧路径无描述）
    pub description: Option<String>,
}

/// v0.4: Art 产出元数据 + 描述/模型标识。
///
/// 真实图片由 PNG 文件承载（前端用 `<img>` 渲染）；
/// width/height/pixels 字段删除 —— Artworks.vue 渲染 PNG 不再需要像素列表。
/// Artworks 契约变更在 T13 处理。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Art {
    pub theme_word: String,
    pub mood: Option<String>,
    /// v0.4: 编排器产出的画面描述（自然语言）
    pub description: String,
    /// v0.4: 模型标识（"local" / "dall-e-3" / "minimax-image" 等）
    pub model: String,
}

/// v0.4: ArtPrompt 给 ArtAdapter 的输入。
///
/// description = 上游 T5 编排器从 daily_summary 推出的自然语言描述；
/// 本地 LocalArtAdapter 不消费，T9 ModelArtAdapter 喂给模型。
#[derive(Debug, Clone)]
pub struct ArtPrompt {
    pub events: Vec<crate::hook::event::KeyEvent>,
    pub mood: Option<String>,
    pub style: String,
    pub theme_word: String,
    /// v0.4: 编排器产出的描述（None = 旧路径无描述）
    pub description: Option<String>,
}

/// v0.4: MusicAdapter 产出形态 —— Music 元数据 + 原始 WAV 字节。
///
/// 设计动机（T11）：
///   - Music 不再含 wav bytes，只有元数据 + 振幅（由 wav_analysis 在 impl 层填）。
///   - adapter 只负责产出 wav 字节（模型产物 / 本地编码产物）；分析是下游消费的事。
///   - 这样 `generate_now_impl` 拿到 wav 字节后可统一「落盘 + wav_analysis」二次分析，
///     而不依赖某个具体 adapter 内部已分析过。
#[derive(Debug)]
pub struct MusicOutcome {
    pub music: Music,
    pub wav: Vec<u8>,
}

#[async_trait::async_trait]
pub trait MusicAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    /// 返回 `MusicOutcome { music, wav }`：Music 元数据（振幅未填，下游分析）+ WAV 字节。
    async fn generate(&self, prompt: &MusicPrompt) -> anyhow::Result<MusicOutcome>;
}

#[async_trait::async_trait]
pub trait ArtAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    /// 返回 (Art, Vec<u8>)：Art 元数据 + 原始 PNG 字节。
    ///
    /// v0.4: Art 不再含 width/height/pixels，真实图片必须由 PNG 字节承载。
    /// 调用方拿到 PNG 字节后直接落盘（`write_artifacts`）。
    async fn generate(&self, prompt: &ArtPrompt) -> anyhow::Result<ArtOutcome>;
}

/// v0.4: ArtAdapter 产出形态 —— Art 元数据 + 原始 PNG 字节。
///
/// 设计动机：
///   - Art 已是「纯元数据」结构（theme_word/mood/description/model），
///     不能再塞下像素数据（前端用 `<img src=...png>` 渲染，不需要像素列表）。
///   - PNG 字节必须由 adapter 透传给 generate_now 命令层落盘 —— 任何
///     「adapter 内部写盘」的方案都会让 generate_now 拿不到字节、也无法
///     拿到 png 路径写 artifacts 表。
#[derive(Debug)]
pub struct ArtOutcome {
    pub art: Art,
    pub png: Vec<u8>,
}

/// v0.4 T11: 构造 ModelMusicAdapter —— `generate_now` 拿 chat + audio client 后接线。
///
/// `audio` 是 `AudioClient` 多态（本地 EngineClient / 云端 MiniMaxMusicClient 都实现了它）；
/// `model` 写入 `Music.model` 字段（用于前端/落盘追溯生成模型）。
pub fn build_model_music_adapter(
    audio: std::sync::Arc<dyn crate::model::AudioClient>,
    model: &str,
) -> model_music::ModelMusicAdapter {
    model_music::ModelMusicAdapter::new(audio, model)
}

/// v0.4 T11: 构造 ModelArtAdapter —— `generate_now` 拿 image client 后接线。
///
/// `image` 是 `ImageClient` 多态（本地 `EngineClient` / 云端 `MiniMaxImageClient` 都实现了它）；
/// `model` 写入 `Art.model` 字段（用于前端/落盘追溯生成模型）。
pub use model_art::build_model_art_adapter;

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证意图：Music/Art 新结构包含 description + model 字段
    #[test]
    fn music_struct_has_description_and_model() {
        let m = Music {
            bpm: 0,
            duration_ms: 1000,
            amplitudes: vec![0.5; AMPLITUDE_SAMPLE_COUNT],
            mood: Some("calm".into()),
            style: "ambient".into(),
            theme_word: "rain".into(),
            description: "calm ambient piece".into(),
            model: "local".into(),
        };
        // JSON 序列化必须含 description + model
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"description\""));
        assert!(json.contains("\"model\""));
        // 必须 NOT 含 notes 字段
        assert!(!json.contains("\"notes\""), "v0.4 删了 Music.notes");
    }

    #[test]
    fn art_struct_has_description_and_model_no_pixels() {
        let a = Art {
            theme_word: "rain".into(),
            mood: Some("calm".into()),
            description: "rainy window".into(),
            model: "local".into(),
        };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"description\""));
        assert!(json.contains("\"model\""));
        assert!(!json.contains("\"pixels\""), "v0.4 删了 Art.pixels");
        assert!(!json.contains("\"width\""), "v0.4 删了 Art.width");
        assert!(!json.contains("\"height\""), "v0.4 删了 Art.height");
    }

    #[test]
    fn music_prompt_has_description_field() {
        // v0.4: MusicPrompt 加 description: Option<String>
        let p = MusicPrompt {
            events: vec![],
            mood: Some("happy".into()),
            style: "jazz".into(),
            theme_word: "x".into(),
            description: Some("happy jazz tune".into()),
        };
        assert_eq!(p.description.as_deref(), Some("happy jazz tune"));
    }

    #[test]
    fn art_prompt_has_description_field() {
        let p = ArtPrompt {
            events: vec![],
            mood: Some("happy".into()),
            style: "jazz".into(),
            theme_word: "x".into(),
            description: Some("bright colors".into()),
        };
        assert_eq!(p.description.as_deref(), Some("bright colors"));
    }
}