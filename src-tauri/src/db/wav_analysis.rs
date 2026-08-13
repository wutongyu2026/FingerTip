//! v0.4: WAV 字节流 → 振幅分析
//!
//! 历史：
//!   - v0.3.4: wav_encoder 把 Music 渲染成 WAV，并手工合成 64 桶 amplitudes（与波形无关）。
//!   - v0.4:   模型产出的 WAV 是真实音频，需要从 WAV 解析出 64 桶 RMS 包络 ——
//!             T8 ModelMusicAdapter 调 `analyze_wav(bytes)` 填进 Music.amplitudes。
//!             amplitudes 契约不变（长度 64，值 ∈ [0.0, 1.0]）。
//!
//! 设计要点：
//!   - 严格只支持 16-bit PCM（mono / stereo），其它格式（IEEE float、24-bit 等）报错
//!   - 跳过 fmt 与 data 之间的非 fmt/RIFF chunk（LIST/JUNK/INFO 等），容忍格式变体
//!   - 64 桶 RMS 包络：每个 sample 取绝对值归一化（除以 i16::MAX），分桶求 RMS
//!   - 整段静音 → 全 0 桶；duration_ms 由 samples 数量与 sample_rate 算出

use anyhow::{anyhow, Result};

/// 振幅分析结果
///
/// `duration_ms`：音频时长（毫秒）
/// `amplitudes`：长度为 64 的 RMS 包络，每桶值 ∈ [0.0, 1.0]（已按 i16::MAX 归一化）
pub struct WavAnalysis {
    pub duration_ms: u64,
    pub amplitudes: Vec<f32>,
}

/// 振幅桶数（契约固定）
const BUCKETS: usize = 64;

/// 解析 WAV 字节流，返回 64 桶振幅包络与时长。
///
/// 支持格式：
///   - 16-bit PCM（audio_format = 1）
///   - mono / stereo（stereo 取左右平均）
///   - fmt 块大小 16 / 18 / 40（额外字段会被跳过）
///   - fmt 与 data 间允许 LIST/JUNK/INFO 等任意 chunk
///
/// 不支持时报错：
///   - 非 RIFF 容器
///   - 找不到 "fmt " / "data" 块
///   - audio_format != 1（非 PCM）
///   - bits_per_sample != 16
///   - channels == 0
///   - sample_rate == 0
///   - 字节流过早截断
pub fn analyze_wav(bytes: &[u8]) -> Result<WavAnalysis> {
    // 1. RIFF 头
    if bytes.len() < 12 {
        return Err(anyhow!("WAV 字节流过短（< 12 字节），不是合法 RIFF 头"));
    }
    if &bytes[0..4] != b"RIFF" {
        return Err(anyhow!("WAV 缺少 RIFF 标识：前 4 字节应为 \"RIFF\""));
    }
    if &bytes[8..12] != b"WAVE" {
        return Err(anyhow!("WAV 缺少 WAVE 标识：byte 8..12 应为 \"WAVE\""));
    }

    // 2. 遍历子块，提取 fmt + data
    let mut cursor: usize = 12;
    let mut audio_format: Option<u16> = None;
    let mut channels: Option<u16> = None;
    let mut sample_rate: Option<u32> = None;
    let mut bits_per_sample: Option<u16> = None;
    let mut data_offset: Option<usize> = None;
    let mut data_size: Option<u32> = None;

    while cursor + 8 <= bytes.len() {
        let chunk_id = &bytes[cursor..cursor + 4];
        let chunk_size = u32::from_le_bytes([
            bytes[cursor + 4],
            bytes[cursor + 5],
            bytes[cursor + 6],
            bytes[cursor + 7],
        ]) as usize;
        let chunk_body_start = cursor + 8;

        if chunk_id == b"fmt " {
            // fmt 块最小 16 字节
            if chunk_size < 16 || chunk_body_start + 16 > bytes.len() {
                return Err(anyhow!("fmt 块大小不足（< 16 字节）"));
            }
            let fmt = &bytes[chunk_body_start..chunk_body_start + chunk_size.min(bytes.len() - chunk_body_start)];
            let af = u16::from_le_bytes([fmt[0], fmt[1]]);
            let ch = u16::from_le_bytes([fmt[2], fmt[3]]);
            let sr = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
            // bytes 8..10 是 byte_rate，10..12 是 block_align（这里不直接用）
            let bps = u16::from_le_bytes([fmt[14], fmt[15]]);
            audio_format = Some(af);
            channels = Some(ch);
            sample_rate = Some(sr);
            bits_per_sample = Some(bps);
        } else if chunk_id == b"data" {
            data_offset = Some(chunk_body_start);
            data_size = Some(chunk_size as u32);
            break;
        }
        // 其它 chunk（LIST/JUNK/INFO/fact 等）跳过
        // RIFF chunk 数据按 2 字节对齐，所以下一步位置需要向上对齐
        let next = chunk_body_start + chunk_size;
        let aligned = next + (next % 2);
        cursor = aligned;
    }

    let audio_format = audio_format.ok_or_else(|| anyhow!("WAV 缺少 fmt 块"))?;
    let channels = channels.ok_or_else(|| anyhow!("WAV fmt 块缺 channels"))?;
    let sample_rate = sample_rate.ok_or_else(|| anyhow!("WAV fmt 块缺 sample_rate"))?;
    let bits_per_sample = bits_per_sample.ok_or_else(|| anyhow!("WAV fmt 块缺 bits_per_sample"))?;
    let data_offset = data_offset.ok_or_else(|| anyhow!("WAV 找不到 data 块"))?;
    let data_size = data_size.ok_or_else(|| anyhow!("WAV 缺少 data 块大小"))?;

    if audio_format != 1 {
        return Err(anyhow!(
            "WAV audio_format = {}（非 PCM=1），暂不支持 IEEE float / ADPCM / 其它压缩格式",
            audio_format
        ));
    }
    if bits_per_sample != 16 {
        return Err(anyhow!(
            "WAV bits_per_sample = {}（非 16-bit），暂不支持 24-bit / 8-bit / 32-bit",
            bits_per_sample
        ));
    }
    if channels == 0 {
        return Err(anyhow!("WAV channels = 0（非法）"));
    }
    if sample_rate == 0 {
        return Err(anyhow!("WAV sample_rate = 0（非法）"));
    }

    // 3. 解 samples
    let block_align = (channels as u32) * (bits_per_sample as u32) / 8;
    if block_align == 0 {
        return Err(anyhow!("WAV block_align = 0（非法）"));
    }
    let data_end = data_offset + data_size as usize;
    if data_end > bytes.len() {
        return Err(anyhow!(
            "WAV data 越界：声明 {} 字节，实际剩余 {} 字节",
            data_size,
            bytes.len() - data_offset
        ));
    }
    let samples_total = data_size as u64 / block_align as u64;
    let duration_ms = samples_total * 1000 / sample_rate as u64;

    // 4. 64 桶 RMS 计算
    let amplitudes = compute_buckets(
        &bytes[data_offset..data_end],
        channels as usize,
        samples_total as usize,
    );

    Ok(WavAnalysis {
        duration_ms,
        amplitudes,
    })
}

/// 提取一帧的「各声道 |sample| 均值」并归一化到 [0.0, 1.0]
///
/// 用 |sample|（unsigned_abs）而非带符号样本，避免 stereo 反相信号
/// 被相加抵消（L=+X / R=-X → 求和=0 → 误判为静音）。
///
/// mono：|sample| / i16::MAX
/// stereo：左右 |sample| 的算术平均 / i16::MAX
fn frame_abs_mean(data: &[u8], frame_idx: usize, channels: usize) -> Option<f32> {
    if channels == 0 {
        return None;
    }
    let frame_offset = frame_idx * channels * 2;
    let mut acc = 0.0f64;
    for ch in 0..channels {
        let byte_pos = frame_offset + ch * 2;
        if byte_pos + 2 > data.len() {
            return None;
        }
        let s = i16::from_le_bytes([data[byte_pos], data[byte_pos + 1]]);
        acc += s.unsigned_abs() as f64 / i16::MAX as f64;
    }
    Some((acc / channels as f64) as f32)
}

/// 计算 64 桶 RMS 包络
///
/// 处理逻辑（反向映射，保证短帧也填满 64 桶）：
///   - 对每个桶反算帧区间：`[start, end)`，其中
///     `start = i * N / 64`，`end = max(start+1, (i+1) * N / 64)`
///   - `end` 强制 ≥ start+1 防「帧数 < 64」时出现空桶
///   - 每帧值 = `frame_abs_mean` → 已用 |sample| 处理 stereo 反相
///   - 桶内 RMS：`sqrt(mean(value²))`，最后 clamp 到 [0.0, 1.0]
fn compute_buckets(data: &[u8], channels: usize, samples_total: usize) -> Vec<f32> {
    let mut buckets = vec![0.0f32; BUCKETS];
    if samples_total == 0 || data.is_empty() || channels == 0 {
        return buckets;
    }

    for bucket_idx in 0..BUCKETS {
        let start_frame = bucket_idx * samples_total / BUCKETS;
        let end_frame = ((bucket_idx + 1) * samples_total / BUCKETS).max(start_frame + 1);
        let end_frame = end_frame.min(samples_total);

        let mut sum_sq = 0.0f64;
        let mut count = 0u32;
        for f in start_frame..end_frame {
            if let Some(v) = frame_abs_mean(data, f, channels) {
                let v = v.clamp(0.0, 1.0);
                sum_sq += (v as f64) * (v as f64);
                count += 1;
            }
        }

        if count > 0 {
            let mean_sq = sum_sq / count as f64;
            buckets[bucket_idx] = (mean_sq.sqrt() as f32).clamp(0.0, 1.0);
        }
    }
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造 1 秒 44.1kHz 16-bit mono 正弦波，振幅 amp
    ///
    /// 这是 T7 测试基础设施：与 wav_encoder 的 make_sine_wav 思路一致，
    /// 保证「自产自测」可验证解析契约。
    fn make_sine_wav(duration_ms: u32, sample_rate: u32, freq_hz: f32, amp: f32) -> Vec<u8> {
        let n = (duration_ms as usize) * sample_rate as usize / 1000;
        let mut samples = Vec::with_capacity(n * 2);
        for i in 0..n {
            let t = i as f32 / sample_rate as f32;
            let v = (2.0 * std::f32::consts::PI * freq_hz * t).sin() * amp;
            let s = (v * i16::MAX as f32) as i16;
            samples.extend_from_slice(&s.to_le_bytes());
        }
        // RIFF header
        let data_size = (n * 2) as u32; // 16-bit mono
        let byte_rate = sample_rate * 2;
        let mut buf = Vec::with_capacity(44 + data_size as usize);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(36 + data_size).to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes()); // fmt size = 16 (PCM)
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&1u16.to_le_bytes()); // 1 channel
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes()); // block align
        buf.extend_from_slice(&16u16.to_le_bytes()); // bits/sample
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.extend_from_slice(&samples);
        buf
    }

    /// 验证意图：标准 sin 波应正确解析出 64 桶，且每桶值 ∈ [0, 1]，
    /// 至少有一些桶有能量（正弦 RMS ≈ amp / √2 ≈ 0.354）
    #[test]
    fn analyze_wav_parses_pcm_and_extracts_64_buckets() {
        let wav = make_sine_wav(1000, 44100, 440.0, 0.5);
        let a = analyze_wav(&wav).expect("标准 PCM WAV 应能解析");
        assert_eq!(a.duration_ms, 1000, "1 秒正弦波 → duration_ms = 1000");
        assert_eq!(a.amplitudes.len(), 64, "amplitudes 长度固定 64");
        assert!(
            a.amplitudes.iter().all(|&v| (0.0..=1.0).contains(&v)),
            "每桶值应在 [0, 1] 区间（已归一化），实际: {:?}",
            a.amplitudes
        );
        // 正弦 RMS ≈ 0.5 / sqrt(2) ≈ 0.354；至少应有一些能量
        assert!(
            a.amplitudes.iter().any(|&v| v > 0.2),
            "正弦波应有能量（至少一桶 > 0.2），实际全桶: {:?}",
            a.amplitudes
        );
    }

    /// 验证意图：整段静音（amp=0）→ 64 桶全 0
    #[test]
    fn analyze_wav_silent_returns_zeros() {
        let wav = make_sine_wav(500, 44100, 0.0, 0.0); // amp=0 静音
        let a = analyze_wav(&wav).expect("静音 WAV 仍合法");
        assert_eq!(a.duration_ms, 500, "500ms 静音 → duration_ms = 500");
        assert!(
            a.amplitudes.iter().all(|&v| v == 0.0),
            "整段静音 → 64 桶应全 0，实际: {:?}",
            a.amplitudes
        );
    }

    /// 验证意图：帧数 < 64 时（恒定振幅）不应出现空洞桶 / 锯齿
    /// ——验证「短时长桶填充」契约真的落实，不是只测了 1000ms 长音频
    #[test]
    fn analyze_wav_short_returns_shorter_buckets_filled() {
        // 44 帧 < 64 桶：反向映射必须保证每桶至少 1 帧
        let sample_rate = 44100u32;
        let n = 44usize;
        let mut samples = Vec::with_capacity(n * 2);
        for _ in 0..n {
            samples.extend_from_slice(&16000i16.to_le_bytes());
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

        let a = analyze_wav(&buf).expect("44 帧恒定振幅应能解析");
        assert_eq!(a.amplitudes.len(), 64, "桶数固定 64");
        // 核心意图：恒定振幅不应被反向映射错位成锯齿 → 每桶都有值、且在 [0.2, 1.0]
        assert!(
            a.amplitudes.iter().all(|&v| v > 0.2),
            "44 帧反向映射应填满 64 桶，不能有空洞（0 值）。实际: {:?}",
            a.amplitudes
        );
        assert!(
            a.amplitudes.iter().all(|&v| v <= 1.0),
            "归一化应在 [0, 1] 区间，实际: {:?}",
            a.amplitudes
        );
    }

    /// 构造 stereo 16-bit PCM 头部（fmt size=16），samples 跟在 data 块后面由调用方拼接
    fn make_stereo_header(sample_rate: u32, n: usize) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        // RIFF size = 4 (WAVE) + 8 (fmt) + 16 (fmt body) + 8 (data hdr) + n*4
        buf.extend_from_slice(&((4 + 8 + 16 + 8 + n * 4) as u32).to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&2u16.to_le_bytes()); // stereo
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&(sample_rate * 4).to_le_bytes());
        buf.extend_from_slice(&4u16.to_le_bytes()); // block align = 4
        buf.extend_from_slice(&16u16.to_le_bytes()); // bits/sample
        buf
    }

    /// 验证意图：满幅反相立体声（L=+20000 / R=-20000）不应被判为静音
    /// ——验证 stereo 反相 bug 真修（带符号求和会得到 0，用 |sample| 才能保留能量）
    #[test]
    fn analyze_wav_stereo_opposite_channels_not_silent() {
        let n = 4410usize; // 100ms @ 44.1k
        let mut samples = Vec::with_capacity(n * 2 * 2);
        for _ in 0..n {
            samples.extend_from_slice(&20000i16.to_le_bytes()); // L+
            samples.extend_from_slice(&(-20000i16).to_le_bytes()); // R-
        }
        let mut buf = make_stereo_header(44100, n);
        let data_size = (n * 2 * 2) as u32;
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.extend_from_slice(&samples);

        let a = analyze_wav(&buf).expect("反相立体声 WAV 应能解析");
        // 反相若被错误按带符号求和 → RMS=0 → 误判静音；
        // 改用 |sample| 后能量被保留 → 至少一桶 > 0.2
        assert!(
            a.amplitudes.iter().any(|&v| v > 0.2),
            "反相立体声不应被判为静音，实际: {:?}",
            a.amplitudes
        );
    }

    /// 验证意图：非 RIFF 头 → 错误（不应崩）
    #[test]
    fn analyze_wav_errors_on_not_riff() {
        let mut bad = b"NOTRIFF".to_vec();
        bad.extend_from_slice(&[0u8; 100]);
        assert!(analyze_wav(&bad).is_err(), "非 RIFF 标识应报错");
    }

    /// 验证意图：合法 RIFF + WAVE + fmt 但无 data 块 → 错误
    #[test]
    fn analyze_wav_errors_on_missing_data_chunk() {
        let mut bad = Vec::new();
        bad.extend_from_slice(b"RIFF");
        bad.extend_from_slice(&36u32.to_le_bytes());
        bad.extend_from_slice(b"WAVE");
        bad.extend_from_slice(b"fmt ");
        bad.extend_from_slice(&16u32.to_le_bytes());
        bad.extend_from_slice(&1u16.to_le_bytes());
        bad.extend_from_slice(&1u16.to_le_bytes());
        bad.extend_from_slice(&44100u32.to_le_bytes());
        bad.extend_from_slice(&88200u32.to_le_bytes());
        bad.extend_from_slice(&2u16.to_le_bytes());
        bad.extend_from_slice(&16u16.to_le_bytes());
        assert!(analyze_wav(&bad).is_err(), "缺 data 块应报错");
    }

    /// 验证意图：audio_format = 3（IEEE float）→ 错误（仅支持 PCM）
    #[test]
    fn analyze_wav_errors_on_non_pcm() {
        let mut bad = Vec::new();
        bad.extend_from_slice(b"RIFF");
        bad.extend_from_slice(&36u32.to_le_bytes());
        bad.extend_from_slice(b"WAVE");
        bad.extend_from_slice(b"fmt ");
        bad.extend_from_slice(&16u32.to_le_bytes());
        bad.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
        bad.extend_from_slice(&1u16.to_le_bytes());
        bad.extend_from_slice(&44100u32.to_le_bytes());
        bad.extend_from_slice(&88200u32.to_le_bytes());
        bad.extend_from_slice(&2u16.to_le_bytes());
        bad.extend_from_slice(&16u16.to_le_bytes());
        bad.extend_from_slice(b"data");
        bad.extend_from_slice(&4u32.to_le_bytes());
        bad.extend_from_slice(&[0u8; 4]);
        assert!(analyze_wav(&bad).is_err(), "非 PCM 格式应报错");
    }

    /// 验证意图：fmt 与 data 间有 LIST/INFO chunk → 应跳过，正确解析
    #[test]
    fn analyze_wav_handles_list_chunk_before_data() {
        let mut wav = Vec::new();
        let sample_rate = 44100u32;
        let n = 100usize;
        let mut samples = Vec::new();
        for i in 0..n {
            let t = i as f32 / sample_rate as f32;
            let v = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
            let s = (v * i16::MAX as f32) as i16;
            samples.extend_from_slice(&s.to_le_bytes());
        }
        let data_size = (n * 2) as u32;
        let total_size = 36 + 16 + 8 + 8 + data_size;
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&total_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        // LIST chunk
        wav.extend_from_slice(b"LIST");
        wav.extend_from_slice(&8u32.to_le_bytes()); // list size = 8 (含 "data\0" 等)
        wav.extend_from_slice(b"INFO");
        wav.extend_from_slice(b"abcd");
        // data chunk
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        wav.extend_from_slice(&samples);
        let a = analyze_wav(&wav).expect("LIST chunk 后应能跳过，正确解析 data");
        assert_eq!(a.amplitudes.len(), 64, "跳过 LIST → 仍 64 桶");
    }
}
