//! v0.4 T11: 生成产物文件落盘（接受字节直写版）
//!
//! 历史：
//!   - v0.3.4: wav_encoder/png_encoder 渲染 NoteSpec/PixelSpec → WAV/PNG 文件
//!   - v0.4:   适配器（T8/T9）返 wav/png 字节流，generate_now 命令层原样落盘
//!   - T11:    artifact_writer 完全接受字节直写，不再调用 wav_encoder/png_encoder
//!             —— wav_encoder/png_encoder 模块整体删除。
//!
//! 为什么不引 Tauri plugin-fs：
//!   - plugin-fs 是给前端 invoke 用的；后端直接 std::fs::write 即可
//!   - 减少 capability 配置复杂度

use std::path::{Path, PathBuf};

/// 写产物到 `{base_dir}/downloads/{date}/music.wav` + `art.png`，
/// **完全接受调用方提供的字节流**（不再调 wav_encoder/png_encoder 重渲染）。
///
/// v0.4 T11 设计动机：
///   - wav/png 是适配器（T8/T9）的「模型产物」；任何「写盘前再编码」的方案都会
///     覆盖模型输出、引入无意义的责任。
///   - 调用方拿字节流后统一落盘（write_artifacts_with_bytes）。
///
/// 行为：
///   - 自动 `create_dir_all` 创建 downloads/{date}/
///   - 已有同名文件**覆盖**（重新生成场景）
///   - 失败：目录创建失败 / 文件写失败（IO 错）—— 向上抛
pub fn write_artifacts_with_bytes(
    base_dir: &Path,
    date: &str,
    wav_bytes: &[u8],
    png_bytes: &[u8],
) -> anyhow::Result<(PathBuf, PathBuf)> {
    // 1. 建子目录
    let dir = base_dir.join("downloads").join(date);
    std::fs::create_dir_all(&dir)?;

    // 2. 直接写 wav 字节（透传）
    let wav_path = dir.join("music.wav");
    std::fs::write(&wav_path, wav_bytes)?;

    // 3. 直接写 png 字节（透传）
    let png_path = dir.join("art.png");
    std::fs::write(&png_path, png_bytes)?;

    Ok((wav_path, png_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// 最小合法 WAV 头（44 字节）+ 一点 PCM 数据
    fn minimal_wav() -> Vec<u8> {
        let mut buf = Vec::with_capacity(48);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&40u32.to_le_bytes()); // 文件大小 - 8
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&44100u32.to_le_bytes());
        buf.extend_from_slice(&88200u32.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&16u16.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 4]); // 4 字节 PCM
        buf
    }

    /// 最小合法 PNG 头（8 字节签名）+ 一些字节
    fn minimal_png() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        buf.extend_from_slice(&[0u8; 16]); // 后续字节
        buf
    }

    #[test]
    fn write_artifacts_creates_files_in_downloads_date_dir() {
        // 验证意图：写到 {base}/downloads/{date}/music.wav + art.png
        let tmp = TempDir::new().unwrap();
        let wav = minimal_wav();
        let png = minimal_png();
        let (wav_path, png_path) =
            write_artifacts_with_bytes(tmp.path(), "2026-07-28", &wav, &png).unwrap();

        assert!(wav_path.exists(), "wav 文件应存在：{:?}", wav_path);
        assert!(png_path.exists(), "png 文件应存在：{:?}", png_path);
        // 路径应在 downloads/2026-07-28/ 下
        assert!(wav_path
            .components()
            .any(|c| c.as_os_str() == "downloads"));
        assert!(wav_path
            .components()
            .any(|c| c.as_os_str() == "2026-07-28"));

        // 文件大小 = 传入字节数（透传）
        let wav_size = std::fs::metadata(&wav_path).unwrap().len();
        let png_size = std::fs::metadata(&png_path).unwrap().len();
        assert_eq!(wav_size as usize, wav.len(), "wav 字节透传保真");
        assert_eq!(png_size as usize, png.len(), "png 字节透传保真");
    }

    #[test]
    fn write_artifacts_overwrites_existing_files() {
        // 验证意图：再次写同名文件应覆盖（不 panic "file exists"）
        let tmp = TempDir::new().unwrap();
        write_artifacts_with_bytes(tmp.path(), "2026-07-28", &minimal_wav(), &minimal_png())
            .unwrap();
        let (wav_path, _) =
            write_artifacts_with_bytes(tmp.path(), "2026-07-28", &minimal_wav(), &minimal_png())
                .unwrap();
        // 文件仍是合法 WAV
        let bytes = std::fs::read(&wav_path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
    }

    #[test]
    fn write_artifacts_passes_bytes_through_unchanged() {
        // 验证意图：传入 wav/png 字节流原样落盘（adapter 透传契约）
        let tmp = TempDir::new().unwrap();
        let wav: Vec<u8> = (0..100).map(|i| i as u8).collect();
        let png: Vec<u8> = (100..200).rev().map(|i| i as u8).collect();
        let (wav_path, png_path) =
            write_artifacts_with_bytes(tmp.path(), "2026-08-01", &wav, &png).unwrap();
        assert_eq!(std::fs::read(&wav_path).unwrap(), wav);
        assert_eq!(std::fs::read(&png_path).unwrap(), png);
    }
}