//! v0.4 T11: ModelMusicAdapter —— 「吃编排器描述 → 调音频客户端 → 透传 WAV 字节」的真正生成路径。
//!
//! 历史：
//!   - v0.3.x: `LocalMusicAdapter` 用 NoteSpec 序列本地合成 WAV（仍是占位，模型产物）。
//!   - v0.4 (T8/T9):  `ModelMusicAdapter` 拿上游 T5 编排器产出的 `MusicPrompt.description`
//!             喂给 `AudioClient`（本地 `EngineClient` / 云端 `MiniMaxMusicClient`），
//!             内部做 wav_analysis 出振幅。
//!   - v0.4 (T11+): MusicAdapter 改返 `MusicOutcome { music, wav }`：adapter **不再做
//!             wav_analysis**，只产 wav 字节；振幅分析在 `generate_now_impl` 层统一做。
//!             这样职责更清晰（adapter = 模型产物；analysis = 下游消费的事）。
//!
//! 设计取舍：
//!   - 严格只吃 `description`，没有就明确报错（避免静默退化为本地合成）。
//!   - `model` 字段透传 adapter 构造时的模型标识（"step-audio" / "MiniMax-music" 等），
//!     让 artifacts 落盘后可追溯。
//!   - `bpm = 0`：v0.4 不再从 events 估算（保留字段给未来 BPM 检测）。
//!   - 不持有 `Music` 的本地缓存：每次 `generate` 即时调模型拿 WAV 字节。
//!
//! factory 接线（`build_model_music_adapter`）由 `generate_now_impl` 在 T11 完成。

use std::sync::Arc;

use crate::generate::{Music, MusicAdapter, MusicOutcome, MusicPrompt, AMPLITUDE_SAMPLE_COUNT};
use crate::model::AudioClient;

/// v0.4 T11: 吃 `MusicPrompt.description` 调 AudioClient 出 WAV 字节并透传的适配器。
///
/// `audio`：模型侧音频客户端（本地 `EngineClient` / 云端 `MiniMaxMusicClient`，
///          都实现了 `AudioClient`）。
/// `model`：模型标识（写到 `Music.model`，如 `"step-audio"` / `"MiniMax-music"`）。
pub struct ModelMusicAdapter {
    audio: Arc<dyn AudioClient>,
    model: String,
}

impl ModelMusicAdapter {
    /// 新建模型音乐适配器。
    ///
    /// `model` 是要写入 `Music.model` 的标识字符串（用于前端/落盘追溯生成模型）。
    /// `audio` 是模型侧音频客户端，本地引擎或云端都实现了 `AudioClient`。
    pub fn new(audio: Arc<dyn AudioClient>, model: impl Into<String>) -> Self {
        Self {
            audio: Arc::clone(&audio),
            model: model.into(),
        }
    }
}

#[async_trait::async_trait]
impl MusicAdapter for ModelMusicAdapter {
    fn name(&self) -> &'static str {
        "model"
    }

    async fn generate(&self, prompt: &MusicPrompt) -> anyhow::Result<MusicOutcome> {
        // 1. 编排器必须产出 description；缺失即上层契约违反 —— 不静默退化
        let description = prompt
            .description
            .as_deref()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "MusicAdapter: prompt.description 缺失（编排器未产出，无法调模型）"
                )
            })?;

        // 2. 调 AudioClient 生成音频字节流（本地 WAV / 云端 MP3；契约是字节流）
        let wav_bytes = self
            .audio
            .generate_audio(description)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "ModelMusicAdapter: AudioClient({}) 生成失败: {}",
                    self.model,
                    e
                )
            })?;

        // 3. 拼装 Music —— amplitudes 留空（64 桶占位），duration_ms 也先 0；
        //    generate_now_impl 拿 wav 字节后会调 wav_analysis 一次性填这两个字段。
        let music = Music {
            bpm: 0,
            duration_ms: 0,
            amplitudes: vec![0.0; AMPLITUDE_SAMPLE_COUNT],
            mood: prompt.mood.clone(),
            style: prompt.style.clone(),
            theme_word: prompt.theme_word.clone(),
            description: description.to_string(),
            model: self.model.clone(),
        };
        Ok(MusicOutcome { music, wav: wav_bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::MusicPrompt;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// 构造合法 PCM16 mono WAV（与 wav_analysis 测试同款，简化版）
    fn make_sine_wav(duration_ms: u32, sample_rate: u32, freq: f32, amp: f32) -> Vec<u8> {
        let n = (duration_ms as usize) * sample_rate as usize / 1000;
        let mut samples = Vec::with_capacity(n * 2);
        for i in 0..n {
            let t = i as f32 / sample_rate as f32;
            let v = (2.0 * std::f32::consts::PI * freq * t).sin() * amp;
            let s = (v * i16::MAX as f32) as i16;
            samples.extend_from_slice(&s.to_le_bytes());
        }
        let data_size = (n * 2) as u32;
        let mut buf = Vec::with_capacity(44 + data_size as usize);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(36 + data_size).to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&16u16.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.extend_from_slice(&samples);
        buf
    }

    /// Mock AudioClient：可记录收到的 description + 返指定 wav_bytes
    struct MockAudio {
        wav: Vec<u8>,
        received: Mutex<Vec<String>>,
    }
    #[async_trait]
    impl AudioClient for MockAudio {
        async fn generate_audio(&self, text: &str) -> anyhow::Result<Vec<u8>> {
            self.received.lock().unwrap().push(text.to_string());
            Ok(self.wav.clone())
        }
    }

    fn sample_prompt() -> MusicPrompt {
        MusicPrompt {
            events: vec![],
            mood: Some("calm".into()),
            style: "ambient".into(),
            theme_word: "rain".into(),
            description: Some("calm piano with rain ambience".into()),
        }
    }

    #[tokio::test]
    async fn model_music_uses_prompt_description_and_returns_wav_bytes() {
        // T11: adapter 只产 wav 字节；振幅分析在 generate_now_impl 层做。
        // 这里只断言 wav 字节透传 + Music 元数据正确 + amplitudes 是空占位。
        let wav = make_sine_wav(500, 44100, 440.0, 0.5);
        let mock = Arc::new(MockAudio {
            wav: wav.clone(),
            received: Mutex::new(vec![]),
        });
        let adapter = ModelMusicAdapter::new(mock.clone() as Arc<dyn AudioClient>, "step-audio");
        let outcome = adapter.generate(&sample_prompt()).await.unwrap();
        assert_eq!(outcome.music.description, "calm piano with rain ambience");
        assert_eq!(outcome.music.model, "step-audio");
        assert_eq!(outcome.music.bpm, 0);
        // amplitudes 是空占位（不在 adapter 内分析）
        assert_eq!(outcome.music.amplitudes.len(), 64);
        assert!(outcome.music.amplitudes.iter().all(|&v| v == 0.0));
        assert_eq!(outcome.music.duration_ms, 0);
        // wav 字节透传 === mock 提供的字节
        assert_eq!(outcome.wav, wav);
        // WAV 头校验
        assert_eq!(&outcome.wav[0..4], b"RIFF");
        assert_eq!(&outcome.wav[8..12], b"WAVE");
        // Mock 收到了 description
        assert_eq!(
            *mock.received.lock().unwrap(),
            vec!["calm piano with rain ambience".to_string()]
        );
    }

    #[tokio::test]
    async fn model_music_errors_when_description_missing() {
        let wav = make_sine_wav(100, 44100, 440.0, 0.5);
        let mock = Arc::new(MockAudio {
            wav,
            received: Mutex::new(vec![]),
        });
        let adapter = ModelMusicAdapter::new(mock as Arc<dyn AudioClient>, "step-audio");
        let mut p = sample_prompt();
        p.description = None;
        let err = adapter.generate(&p).await.unwrap_err();
        assert!(
            err.to_string().contains("description") || err.to_string().contains("编排器"),
            "错误信息应说明 description 缺失原因，实际: {}",
            err
        );
    }

    #[tokio::test]
    async fn model_music_propagates_audio_client_errors() {
        struct FailingAudio;
        #[async_trait]
        impl AudioClient for FailingAudio {
            async fn generate_audio(&self, _text: &str) -> anyhow::Result<Vec<u8>> {
                anyhow::bail!("MiniMax 上传失败")
            }
        }
        let adapter = ModelMusicAdapter::new(
            Arc::new(FailingAudio) as Arc<dyn AudioClient>,
            "MiniMax-music",
        );
        let err = adapter.generate(&sample_prompt()).await.unwrap_err();
        assert!(
            err.to_string().contains("MiniMax"),
            "错误信息应含上游原因，实际: {}",
            err
        );
    }

    #[tokio::test]
    async fn model_music_does_not_validate_wav_bytes_adapter_passes_through() {
        // T11: adapter 不做 wav_analysis —— 即便客户端返非法 WAV 也透传出去。
        // 真正的 wav 解析失败由 generate_now_impl 层 wav_analysis 上报。
        struct RawAudio;
        #[async_trait]
        impl AudioClient for RawAudio {
            async fn generate_audio(&self, _text: &str) -> anyhow::Result<Vec<u8>> {
                Ok(b"NOT a valid wav".to_vec())
            }
        }
        let adapter = ModelMusicAdapter::new(
            Arc::new(RawAudio) as Arc<dyn AudioClient>,
            "step-audio",
        );
        let outcome = adapter.generate(&sample_prompt()).await.unwrap();
        // adapter 不报错，把字节透传给调用方
        assert_eq!(outcome.wav, b"NOT a valid wav".to_vec());
    }

    #[tokio::test]
    async fn model_music_constructor_takes_model_name() {
        // 同步测试：构造不需要 .await
        use std::sync::Arc;
        struct EmptyAudio;
        #[async_trait]
        impl AudioClient for EmptyAudio {
            async fn generate_audio(&self, _: &str) -> anyhow::Result<Vec<u8>> {
                Ok(vec![])
            }
        }
        let a = ModelMusicAdapter::new(Arc::new(EmptyAudio) as Arc<dyn AudioClient>, "MiniMax-music");
        assert_eq!(a.model, "MiniMax-music");
        assert_eq!(a.name(), "model");
    }
}