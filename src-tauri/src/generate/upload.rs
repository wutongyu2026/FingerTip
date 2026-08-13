//! 生成分享卡片 PNG 长图
//!
//! v0.7 重设计：全宽画作 + 优雅排版 + 圆角阴影 + 宋体排版。
//!
//! 不依赖任何外部服务，图片直接通过微信/QQ 转发。

use ab_glyph::{FontRef, PxScale};
use base64::Engine as _;
use image::{GenericImage, ImageBuffer, Rgba, RgbaImage};
use imageproc::drawing;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrArtifact {
    pub local_path: String,
    /// 音频是否已上传（true = 二维码是音乐直链，false = 降级为文本）
    pub audio_ok: bool,
    /// 扫码直达的分享页 URL（GitHub Pages 落地页 + 作品数据片段）
    pub share_url: String,
}

/// 编码进分享页 URL 片段的作品数据（字段用短名，压低二维码体积）。
#[derive(Debug, Clone, Serialize)]
struct SharePayload {
    v: u8,
    w: String,
    p: String,
    s: String,
    e: String,
    t: String,
    m: String,
    d: String,
    k: String,
    f: f64,
    n: usize,
    a: i32,
    r: String,
    /// v0.9: 搞笑按键总结文案
    u: String,
}

#[derive(Debug, Clone)]
pub struct SharePageData {
    pub wav_path: PathBuf,
    pub png_path: PathBuf,
    pub sentence: String,
    pub english_sentence: String,
    pub theme_word: String,
    pub mood: Option<String>,
    pub date: String,
    pub top1_key: String,
    pub frequency_per_min: f64,
    pub total_keys: usize,
    pub activity_hours: i32,
    pub hourly: [usize; 24],
    /// 时间范围描述（如 "06:00–08:00"），为空则只显示日期
    pub time_range_label: String,
    /// 桌面安装包下载直链（GitHub Releases 等），为空则不显示下载区
    pub download_url: String,
    /// v0.9: 搞笑按键总结文案（编排器产出，2 句话、40-80 字）
    pub funny_summary: String,
}

/// 默认安装包下载链接（GitHub Releases 永久直链）。
///
/// 仓库地址后续可能更换，优先读环境变量 `FINGERTIP_DOWNLOAD_URL`，
/// 未设置时回退到当前实验仓库。
pub fn default_download_url() -> String {
    std::env::var("FINGERTIP_DOWNLOAD_URL").unwrap_or_else(|_| {
        "https://github.com/wutongyu2026/FingerTip/releases/latest".to_string()
    })
}

/// 扫码落地页永久地址（GitHub Pages 托管 docs/landing.html）。
///
/// 仓库地址后续可能更换，优先读环境变量 `FINGERTIP_LANDING_PAGE_URL`，
/// 未设置时回退到当前实验仓库。
pub fn landing_page_url() -> String {
    std::env::var("FINGERTIP_LANDING_PAGE_URL").unwrap_or_else(|_| {
        "https://wutongyu2026.github.io/FingerTip/landing.html".to_string()
    })
}

// ═══════════════════════════════════════════════════════════
// 颜色常量
// ═══════════════════════════════════════════════════════════
const BG_PAGE:      Rgba<u8> = Rgba([0xEC, 0xEA, 0xE5, 0xFF]);  // 卡片外背景
const CARD_WHITE:   Rgba<u8> = Rgba([0xFF, 0xFF, 0xFD, 0xFF]);  // 卡片底色（仅用于圆角裁剪 + 渐变叠加）
const TEXT_PRI:     Rgba<u8> = Rgba([0x1A, 0x1A, 0x16, 0xFF]);  // 主文字（更深，在图片上可读）
const TEXT_SEC:     Rgba<u8> = Rgba([0x70, 0x6E, 0x68, 0xFF]);  // 次要文字
const DIVIDER:      Rgba<u8> = Rgba([0xE0, 0xDE, 0xD8, 0xFF]);  // 分割线
const WARM:         Rgba<u8> = Rgba([0xD6, 0x7B, 0x4F, 0xFF]);  // 强调色
const GREEN:        Rgba<u8> = Rgba([0x4C, 0xAF, 0x50, 0xFF]);  // 活跃绿
const WHITE:        Rgba<u8> = Rgba([0xFF, 0xFF, 0xFF, 0xFF]);

// ═══════════════════════════════════════════════════════════
// 排版常量
// ═══════════════════════════════════════════════════════════
const CARD_W: u32 = 1280;
const CARD_H: u32 = 720;
const RADIUS: f32 = 32.0;            // 卡片圆角半径
const SHADOW_DX: i32 = 8;           // 阴影水平偏移
const SHADOW_DY: i32 = 18;          // 阴影垂直偏移
const SHADOW_SIGMA: f32 = 22.0;     // 阴影模糊 σ
const BLUR_PAD: i32 = 56;           // 为阴影预留的边距
const PAD: i32 = 64;                // 卡片内水平边距
const QR_SIZE: u32 = 120;           // v0.7.0: 200→120（低调不抢眼，扫码成功率仍够）

// ═══════════════════════════════════════════════════════════
// 字体发现
// ═══════════════════════════════════════════════════════════

/// 中文宋体（句子用）
fn find_cn_serif() -> PathBuf {
    for c in &[
        "C:/Windows/Fonts/STSONG.TTF",
        "/System/Library/Fonts/PingFang.ttc",
    ] {
        if std::path::Path::new(c).exists() { return PathBuf::from(c); }
    }
    find_cn_sans()
}

/// 中文无衬线（标签/数值用）
fn find_cn_sans() -> PathBuf {
    for c in &[
        "C:/Windows/Fonts/NotoSansSC-VF.ttf",
        "C:/Windows/Fonts/Deng.ttf",
        "/System/Library/Fonts/PingFang.ttc",
    ] {
        if std::path::Path::new(c).exists() { return PathBuf::from(c); }
    }
    PathBuf::from("C:/Windows/Fonts/times.ttf")
}

/// 英文衬线（英文句子用）
fn find_en_serif() -> PathBuf {
    for c in &[
        "C:/Windows/Fonts/georgia.ttf",
        "C:/Windows/Fonts/times.ttf",
    ] {
        if std::path::Path::new(c).exists() { return PathBuf::from(c); }
    }
    find_cn_sans()
}

/// 数字加粗体
fn find_en_bold() -> PathBuf {
    for c in &[
        "C:/Windows/Fonts/georgiab.ttf",
        "C:/Windows/Fonts/georgia.ttf",
    ] {
        if std::path::Path::new(c).exists() { return PathBuf::from(c); }
    }
    find_cn_sans()
}

// ═══════════════════════════════════════════════════════════
// 绘制工具
// ═══════════════════════════════════════════════════════════

/// 判断像素是否在圆角矩形内
fn in_rounded(x: i32, y: i32, rx: i32, ry: i32, w: i32, h: i32, r: f32) -> bool {
    // 中心矩形区
    let ri = r.ceil() as i32;
    if x >= rx + ri && x < rx + w - ri && y >= ry && y < ry + h {
        return true;
    }
    if x >= rx && x < rx + w && y >= ry + ri && y < ry + h - ri {
        return true;
    }
    // 四角
    let corners: [(i32, i32); 4] = [
        (rx + ri - 1, ry + ri - 1),
        (rx + w - ri, ry + ri - 1),
        (rx + ri - 1, ry + h - ri),
        (rx + w - ri, ry + h - ri),
    ];
    let zone = ri;
    for (cx, cy) in corners {
        let in_x = x >= cx - zone && x <= cx + zone;
        let in_y = y >= cy - zone && y <= cy + zone;
        if in_x && in_y {
            let dx = (x - cx) as f32;
            let dy = (y - cy) as f32;
            if (dx * dx + dy * dy) <= (r * r) { return true; }
        }
    }
    false
}

/// 绘制带抗锯齿的圆角矩形（填充）
fn fill_rounded_rect(
    canvas: &mut RgbaImage,
    rx: i32, ry: i32, w: i32, h: i32, r: f32,
    color: Rgba<u8>,
) {
    let soft = 1.2f32;
    let ri = r.ceil() as i32;

    for py in ry..ry + h {
        for px in rx..rx + w {
            // 快速路径：中心区直接填色
            let in_center_x = px >= rx + ri && px < rx + w - ri;
            let in_center_y = py >= ry + ri && py < ry + h - ri;
            if in_center_x || in_center_y {
                canvas.put_pixel(px as u32, py as u32, color);
                continue;
            }
            // 边角区：计算距离
            let in_corner = in_rounded(px, py, rx, ry, w, h, r);
            if in_corner {
                canvas.put_pixel(px as u32, py as u32, color);
            } else {
                // 抗锯齿：检查是否在软边缘内
                if in_rounded(px, py, rx, ry, w, h, r + soft) {
                    let bg = canvas.get_pixel(px as u32, py as u32);
                    let alpha = 0.35;
                    let a = (color[3] as f32 * (1.0 - alpha)) as u8 + (bg[3] as f32 * alpha) as u8;
                    canvas.put_pixel(px as u32, py as u32, Rgba([
                        ((color[0] as f32 * (1.0 - alpha) + bg[0] as f32 * alpha) as u8),
                        ((color[1] as f32 * (1.0 - alpha) + bg[1] as f32 * alpha) as u8),
                        ((color[2] as f32 * (1.0 - alpha) + bg[2] as f32 * alpha) as u8),
                        a,
                    ]));
                }
            }
        }
    }
}

/// 给画布绘制投影（在 card 区域下）
fn draw_shadow(canvas: &mut RgbaImage, card_x: i32, card_y: i32) {
    use image::imageops;
    let w = CARD_W as i32;
    let h = CARD_H as i32;

    // 创建独立阴影层
    let mut shadow = ImageBuffer::from_pixel(
        (w + BLUR_PAD * 2) as u32,
        (h + BLUR_PAD * 2) as u32,
        Rgba([0, 0, 0, 0]),
    );
    let sx = BLUR_PAD + SHADOW_DX;
    let sy = BLUR_PAD + SHADOW_DY;
    fill_rounded_rect(&mut shadow, sx, sy, w, h, RADIUS, Rgba([0, 0, 0, 60]));

    // 高斯模糊
    let blurred = imageops::blur(&shadow, SHADOW_SIGMA);

    // 合成到主画布
    for (x, y, p) in blurred.enumerate_pixels() {
        if p[3] > 0 {
            let cx = card_x + x as i32 - BLUR_PAD;
            let cy = card_y + y as i32 - BLUR_PAD;
            if cx >= 0 && cy >= 0 && (cx as u32) < canvas.width() && (cy as u32) < canvas.height() {
                let bg = canvas.get_pixel(cx as u32, cy as u32);
                let a = p[3] as f32 / 255.0;
                canvas.put_pixel(cx as u32, cy as u32, Rgba([
                    ((bg[0] as f32 * (1.0 - a) + p[0] as f32 * a) as u8),
                    ((bg[1] as f32 * (1.0 - a) + p[1] as f32 * a) as u8),
                    ((bg[2] as f32 * (1.0 - a) + p[2] as f32 * a) as u8),
                    255,
                ]));
            }
        }
    }
}

/// 绘制文字，返回推进后的 y
///
/// 已无直接调用（句子 / 统计 / 标签 / 脚注全走带 halo 的 draw_with_shadow），
/// 仅作 API 残留。如需简单绘制可考虑直接调 drawing::draw_text_mut。
#[allow(dead_code)]
fn draw(
    img: &mut RgbaImage,
    font: &FontRef,
    text: &str,
    x: i32,
    y: i32,
    scale: PxScale,
    color: Rgba<u8>,
) -> i32 {
    drawing::draw_text_mut(img, color, x, y, scale, font, text);
    y + (scale.y * 1.15) as i32
}

/// 绘制带柔和白色 halo 的文字 —— 在彩色画作背景上保证深色文字可读。
///
/// 做法：4 个方向各偏移 1px 画半透明白（70/255 alpha），主文字最后画在原位。
/// 白色 halo 只在主文字覆盖不到的边缘露出，形成自然的外发光。
///
/// 用于：句子 / 统计标签 / 统计数值 / 脚注 —— 全在画作上的文字层。
fn draw_with_shadow(
    img: &mut RgbaImage,
    font: &FontRef,
    text: &str,
    x: i32,
    y: i32,
    scale: PxScale,
    color: Rgba<u8>,
) -> i32 {
    let halo = Rgba([0xFF, 0xFF, 0xFD, 0x70]);  // 半透明白（27% 不透明度）
    drawing::draw_text_mut(img, halo, x - 1, y, scale, font, text);
    drawing::draw_text_mut(img, halo, x + 1, y, scale, font, text);
    drawing::draw_text_mut(img, halo, x, y - 1, scale, font, text);
    drawing::draw_text_mut(img, halo, x, y + 1, scale, font, text);
    drawing::draw_text_mut(img, color, x, y, scale, font, text);
    y + (scale.y * 1.15) as i32
}

/// 测量文字宽度（粗略）
fn measure(text: &str, scale: PxScale) -> i32 {
    let cn_w = scale.y;
    let en_w = scale.y * 0.55;
    let mut w = 0.0f32;
    for ch in text.chars() {
        w += if ch.is_ascii_alphanumeric() || ch == ' ' || ch == '.' || ch == ',' { en_w } else { cn_w };
    }
    w.ceil() as i32
}

/// 字符级换行
fn wrap(text: &str, max_w: u32, scale: PxScale) -> Vec<String> {
    let cn_w = scale.y;
    let en_w = scale.y * 0.55;
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0.0f32;
    for ch in text.chars() {
        let cw = if ch.is_ascii_alphanumeric() || ch == ' ' || ch == ',' || ch == '.' {
            en_w
        } else {
            cn_w
        };
        if cur_w + cw > max_w as f32 && !cur.is_empty() {
            lines.push(cur);
            cur = String::new();
            cur_w = 0.0;
        }
        cur.push(ch);
        cur_w += cw;
    }
    if !cur.is_empty() { lines.push(cur); }
    if lines.is_empty() { lines.push(text.to_string()); }
    lines
}

/// 在指定位置绘制水平居中文字，返回推进后的 y
fn draw_centered(
    img: &mut RgbaImage,
    font: &FontRef,
    text: &str,
    center_x: i32,
    y: i32,
    scale: PxScale,
    color: Rgba<u8>,
) -> i32 {
    let w = measure(text, scale);
    let x = center_x - w / 2;
    draw(img, font, text, x.max(0), y, scale, color)
}

/// 生成二维码 RgbaImage（干净白底，留白边，不搞圆角）
fn make_qr(content: &str, size: u32) -> anyhow::Result<RgbaImage> {
    use image::imageops;
    use qrcode::QrCode;

    let code = QrCode::with_error_correction_level(content.as_bytes(), qrcode::EcLevel::L)?;
    let rendered = code.render::<image::Luma<u8>>().build();
    let pad = 6u32;
    let qs = size.saturating_sub(pad * 2);
    let scaled = imageops::resize(&rendered, qs, qs, imageops::FilterType::Lanczos3);

    let mut qr = ImageBuffer::from_pixel(size, size, WHITE);
    for (x, y, p) in scaled.enumerate_pixels() {
        qr.put_pixel(x + pad, y + pad, Rgba([p.0[0], p.0[0], p.0[0], 255]));
    }
    Ok(qr)
}

// ═══════════════════════════════════════════════════════════
// 上传
// ═══════════════════════════════════════════════════════════

async fn upload_file(bytes: &[u8], name: &str, mime: &str) -> Option<String> {
    if bytes.is_empty() { return None; }

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let b = bytes.to_vec();
    let n = name.to_string();
    let m = mime.to_string();

    for (svc_name, svc_url) in [
        ("tmpfile.link", "https://tmpfile.link/api/upload"),
        ("tmpfiles.org", "https://tmpfiles.org/api/v1/upload"),
    ] {
        let b2 = b.clone();
        let n2 = n.clone();
        let m2 = m.clone();
        let tx2 = tx.clone();
        tokio::spawn(async move {
            match try_upload(svc_url, &b2, &n2, &m2).await {
                Ok(url) => { let _ = tx2.send(Some(url)); }
                Err(e) => log::warn!("上传 {svc_name} 失败: {e}"),
            }
        });
    }
    drop(tx);

    while let Some(result) = rx.recv().await {
        if let Some(url) = result {
            log::info!("上传成功: {}", url);
            return Some(url);
        }
    }
    log::error!("所有上传服务均失败");
    None
}

async fn try_upload(url: &str, bytes: &[u8], name: &str, mime: &str) -> anyhow::Result<String> {
    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name(name.to_string())
        .mime_str(mime)?;
    let form = reqwest::multipart::Form::new().part("file", part);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let resp = client.post(url).multipart(form).send().await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("HTTP {} → {}", status, &body[..body.len().min(200)]);
    }
    let v: serde_json::Value = serde_json::from_str(&body)?;
    if let Some(u) = v.get("downloadLink").and_then(|s| s.as_str()) {
        return Ok(u.to_string());
    }
    if let Some(u) = v.get("data").and_then(|d| d.get("link")).and_then(|s| s.as_str()) {
        return Ok(u.to_string());
    }
    if let Some(u) = v.get("url").and_then(|s| s.as_str()) {
        return Ok(u.to_string());
    }
    anyhow::bail!("无法解析响应: {}", &body[..body.len().min(200)])
}

/// 上传 WAV + PNG + 生成完整卡片 HTML → 返回卡片页面直链
///
/// v0.8 已弃用：临时文件服务对 HTML 返回 Content-Disposition: attachment，
/// 扫码会变成下载文件而不是打开网页。保留仅作历史参考。
#[allow(dead_code)]
async fn upload_share_page(
    wav_path: &PathBuf,
    png_path: &PathBuf,
    sentence: &str,
    english_sentence: &str,
    theme_word: &str,
    mood: Option<&str>,
    date: &str,
    top1_key: &str,
    frequency_per_min: f64,
    total_keys: usize,
    activity_hours: i32,
    time_range_label: &str,
    download_url: &str,
) -> Option<String> {
    let wav_bytes = tokio::fs::read(wav_path).await.ok()?;
    let png_bytes = tokio::fs::read(png_path).await.ok()?;

    let (wav_url, png_url) = tokio::join!(
        upload_file(&wav_bytes, "music.wav", "audio/wav"),
        upload_file(&png_bytes, "art.png", "image/png"),
    );
    let wav_url = wav_url?;
    let png_url = png_url?;

    let mood_str = mood.unwrap_or("—");
    let en_block = if english_sentence.is_empty() {
        String::new()
    } else {
        format!(r#"<p class="en-sentence">{}</p>"#, escape_html(english_sentence))
    };

    let act_lvl = (activity_hours.min(16) as f32 / 16.0 * 5.0).round() as u8;
    let act_icons: String = (0..5).map(|i| if i < act_lvl { "●" } else { "○" }).collect();
    let total_str = if total_keys < 1000 {
        total_keys.to_string()
    } else {
        format!("{:.1}K", total_keys as f64 / 1000.0)
    };
    let freq_str = format!("{:.0} 键/分钟", frequency_per_min);

    let time_info = if time_range_label.is_empty() {
        date.to_string()
    } else {
        format!("{}  {}", date, time_range_label)
    };

    let download_block = if download_url.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="download-section"><a class="download-btn" href="{}" target="_blank" rel="noopener"><span class="icon">⬇</span>下载 FingerTip 桌面版</a></div>"#,
            download_url
        )
    };

    let html = format!(r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>FingerTip · {date}</title>
<style>
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{
  font-family: "PingFang SC", "Noto Sans SC", -apple-system, sans-serif;
  background: #ECEAE5; display: flex; justify-content: center; align-items: center;
  min-height: 100vh; padding: 24px;
}}
.card {{
  max-width: 560px; width: 100%;
  background: #FFFFFD; border-radius: 22px; overflow: hidden;
  box-shadow: 0 8px 40px rgba(0,0,0,0.10);
}}
.art {{ width: 100%; display: block; background: #E8E5DC; }}
.sentence-area {{
  padding: 28px 36px 10px;
}}
.cn-sentence {{
  font-family: "STSong", "Songti SC", "Noto Serif SC", "Georgia", serif;
  font-size: 22px; font-weight: 600; color: #1F1F1B;
  line-height: 1.8; letter-spacing: 0.04em; margin-bottom: 4px;
}}
.en-sentence {{
  font-family: "Georgia", "Times New Roman", serif;
  font-size: 14px; color: #787670; font-style: italic;
  line-height: 1.6; margin-bottom: 16px;
}}
.stats {{
  display: flex; align-items: center; gap: 10px;
  padding: 16px 36px 18px; flex-wrap: wrap;
}}
.stat-pill {{
  border-radius: 20px;
  padding: 10px 16px; text-align: center; min-width: 72px;
}}
.stat-pill.c1 {{ background: #FEF2EA; }}
.stat-pill.c2 {{ background: #EAF0FE; }}
.stat-pill.c3 {{ background: #ECFAF2; }}
.stat-pill.c4 {{ background: #F4EEFC; }}
.stat-pill .label {{ font-size: 10px; color: #A8A8A0; text-transform: uppercase; letter-spacing: 0.04em; }}
.stat-pill .value {{ font-size: 16px; color: #1A1A16; font-weight: 600; margin-top: 2px; }}
.stat-pill .dots {{ font-size: 16px; letter-spacing: 2px; color: #D67B4F; }}
.footer {{
  border-top: 1px solid #E6E4DE;
  padding: 14px 36px; margin: 0 36px;
  font-size: 11px; color: #A8A8A0;
  text-align: center; letter-spacing: 0.02em;
}}
.audio-bar {{
  padding: 0 24px 20px; margin-top: 4px;
}}
.audio-bar audio {{ width: 100%; height: 36px; border-radius: 18px; }}
.download-section {{
  padding: 20px 36px 8px; text-align: center;
}}
.download-btn {{
  display: inline-flex; align-items: center; gap: 8px;
  padding: 13px 32px; border-radius: 28px;
  background: #1F1F1B; color: #FFFFFD;
  font-size: 15px; font-weight: 600; text-decoration: none;
  letter-spacing: 0.02em; transition: transform 150ms, box-shadow 150ms;
  box-shadow: 0 4px 16px rgba(0,0,0,0.12);
}}
.download-btn:hover {{ transform: translateY(-1px); box-shadow: 0 6px 24px rgba(0,0,0,0.18); }}
.download-btn .icon {{ font-size: 18px; }}
.mobile-warning {{
  margin: 20px 36px 24px; padding: 12px 18px;
  background: #FFF8E1; border: 1px solid #FFE082;
  border-radius: 12px; font-size: 12px; color: #795548;
  line-height: 1.7; text-align: center;
}}
.mobile-warning .warn-icon {{ font-size: 15px; margin-right: 2px; }}
@media (max-width: 480px) {{
  .sentence-area {{ padding: 20px 20px 6px; }}
  .cn-sentence {{ font-size: 18px; }}
  .stats {{ padding: 12px 20px 14px; gap: 6px; }}
  .stat-pill {{ padding: 6px 8px; min-width: 56px; }}
  .stat-pill .value {{ font-size: 13px; }}
  .footer {{ padding: 10px 20px; margin: 0 20px; }}
  .download-section {{ padding: 14px 20px 4px; }}
  .download-btn {{ padding: 10px 24px; font-size: 14px; }}
  .mobile-warning {{ margin: 14px 20px 20px; font-size: 11px; }}
}}
</style>
</head>
<body>
<div class="card">
  <img class="art" src="{png_url}" alt="今日 AI 画作">
  <div class="sentence-area">
    <p class="cn-sentence">{sentence}</p>
    {en_block}
  </div>
  <div class="stats">
    <div class="stat-pill c1"><div class="label">最常用</div><div class="value">{top1_key}</div></div>
    <div class="stat-pill c2"><div class="label">频率</div><div class="value">{freq_str}</div></div>
    <div class="stat-pill c3"><div class="label">总按键</div><div class="value">{total_str}</div></div>
    <div class="stat-pill c4"><div class="label">活跃</div><div class="value dots">{act_icons}</div></div>
  </div>
  <div class="footer">
    FingerTip · {time_info}  ·  主题词 {theme_word}  ·  心情 {mood}
  </div>
  <div class="audio-bar">
    <audio controls src="{wav_url}"></audio>
  </div>
  {download_block}
  <div class="mobile-warning">
    <span class="warn-icon">📱</span> FingerTip 是 <strong>Windows 桌面软件</strong>，安装包需要下载到电脑上安装运行，手机无法使用。
  </div>
</div>
</body>
</html>"#,
        date = date,
        sentence = escape_html(sentence),
        en_block = en_block,
        theme_word = escape_html(theme_word),
        mood = escape_html(mood_str),
        png_url = png_url,
        wav_url = wav_url,
        top1_key = escape_html(top1_key),
        freq_str = freq_str,
        total_str = total_str,
        act_icons = act_icons,
        time_info = time_info,
        download_block = download_block,
    );

    let html_bytes = html.into_bytes();
    upload_file(&html_bytes, "card.html", "text/html; charset=utf-8").await
}

/// 上传 WAV + PNG，返回两个直链。
///
/// v0.8 分享链路：不再上传 HTML 文件（临时文件服务会强制下载），
/// 只把媒体直链编码进 GitHub Pages 分享页 URL 片段，扫码直接打开网页。
async fn upload_share_assets(
    wav_path: &PathBuf,
    png_path: &PathBuf,
) -> (Option<String>, Option<String>) {
    let wav_bytes = match tokio::fs::read(wav_path).await {
        Ok(b) if !b.is_empty() => b,
        _ => return (None, None),
    };
    let png_bytes = match tokio::fs::read(png_path).await {
        Ok(b) if !b.is_empty() => b,
        _ => return (None, None),
    };

    let (wav_url, png_url) = tokio::join!(
        upload_file(&wav_bytes, "music.wav", "audio/wav"),
        upload_file(&png_bytes, "art.png", "image/png"),
    );
    (wav_url, png_url)
}

/// 组装扫码直达的分享页 URL：`landing.html#d=<base64url JSON>`。
fn build_share_url(data: &SharePageData, wav_url: &str, png_url: &str) -> String {
    let payload = SharePayload {
        v: 1,
        w: wav_url.to_string(),
        p: png_url.to_string(),
        s: data.sentence.clone(),
        e: data.english_sentence.clone(),
        t: data.theme_word.clone(),
        m: data.mood.clone().unwrap_or_else(|| "—".into()),
        d: data.date.clone(),
        k: data.top1_key.clone(),
        f: data.frequency_per_min,
        n: data.total_keys,
        a: data.activity_hours,
        r: data.time_range_label.clone(),
        u: data.funny_summary.clone(),
    };
    let json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes());
    format!("{}#d={}", landing_page_url(), encoded)
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&#39;")
}

// ═══════════════════════════════════════════════════════════
// 卡片 PNG 渲染（核心）
// ═══════════════════════════════════════════════════════════
///
/// 画作铺满整卡作背景，底部渐变覆层保证文字清晰可读。
/// 布局：CN/EN 句子居左 → 统计行居左 → QR 右下角 → 信息栏底部。
pub fn generate_card_png(data: &SharePageData, qr_url: &str) -> anyhow::Result<PathBuf> {
    // ── 1. 加载字体 ──
    let cn_serif_bytes = std::fs::read(find_cn_serif())?;
    let cn_serif = FontRef::try_from_slice(&cn_serif_bytes)?;
    let cn_sans_bytes = std::fs::read(find_cn_sans())?;
    let cn_sans = FontRef::try_from_slice(&cn_sans_bytes)?;
    let en_serif_bytes = std::fs::read(find_en_serif())?;
    let en_serif = FontRef::try_from_slice(&en_serif_bytes)?;
    let en_bold_bytes = std::fs::read(find_en_bold())?;
    let en_bold = FontRef::try_from_slice(&en_bold_bytes)?;

    // ── 字号 ──
    let s_cn_sentence = PxScale { x: 38.0, y: 38.0 };
    let s_en_sentence = PxScale { x: 22.0, y: 22.0 };
    let s_footer      = PxScale { x: 15.0, y: 15.0 };
    let s_qr_label    = PxScale { x: 11.0, y: 11.0 };

    // ── 2. 画布 ──
    let canvas_w = CARD_W as i32 + BLUR_PAD * 2;
    let canvas_h = CARD_H as i32 + BLUR_PAD * 2;
    let card_x = BLUR_PAD;
    let card_y = BLUR_PAD;
    let card_w_i = CARD_W as i32;
    let card_h_i = CARD_H as i32;

    let mut canvas = ImageBuffer::from_pixel(canvas_w as u32, canvas_h as u32, BG_PAGE);
    draw_shadow(&mut canvas, card_x, card_y);

    // ── 3. 画作铺满全卡（cover） ──
    let art_img = image::open(&data.png_path)?.into_rgba8();
    let (aw, ah) = art_img.dimensions();
    let scale_x = card_w_i as f32 / aw as f32;
    let scale_y = card_h_i as f32 / ah as f32;
    let scale = scale_x.max(scale_y);
    let draw_w = (aw as f32 * scale).ceil() as u32;
    let draw_h = (ah as f32 * scale).ceil() as u32;
    let ox = (card_w_i - draw_w as i32) / 2;
    let oy = (card_h_i - draw_h as i32) / 2;

    let resized = image::imageops::resize(
        &art_img, draw_w, draw_h,
        image::imageops::FilterType::Lanczos3,
    );
    for (x, y, p) in resized.enumerate_pixels() {
        let px = card_x + ox + x as i32;
        let py = card_y + oy + y as i32;
        if px >= card_x && px < card_x + card_w_i
            && py >= card_y && py < card_y + card_h_i
            && in_rounded(px, py, card_x, card_y, card_w_i, card_h_i, RADIUS)
        {
            canvas.put_pixel(px as u32, py as u32, *p);
        }
    }

    // ── 4. 数据+QR 组件容器：全宽，垂直渐变（顶 0% 透明 → 底 92% 白） ──
    // v0.7.4 用户反馈："是不是反了？先透明后不透明"——我之前把容器顶做成不透明、
    // 底部透明，但用户期望"上面淡下面重"（即 top 透明 → bottom 不透明）。
    // 修正：反转渐变方向。同时全宽（去左右 padding），让"下面一整块"延伸到左右边。
    //
    // 数据位置相应下移到容器下半部（不透明区域），保证可读：
    //   stats y 495→620 / QR y 485→595 / footer 留在 y=685（最不透明处）
    // 范围：x=0 ~ CARD_W（铺满），y=460 ~ y=720（卡片内坐标）
    // 卡片外圆角裁剪（in_card check）：容器底边被卡片圆角自然裁剪。
    let component_left = 0;
    let component_top = 460;
    let component_w = CARD_W as i32;
    let component_h = CARD_H as i32 - component_top;
    let component_radius: f32 = 16.0;
    let component_top_alpha: f32 = 0.00;     // 顶部透明（画作透出）
    let component_bottom_alpha: f32 = 0.92;  // 底部不透明（数据清晰）

    for py in component_top..component_top + component_h {
        let rel_y = (py - component_top) as f32;
        let t = rel_y / component_h as f32;
        let alpha = component_top_alpha * (1.0 - t) + component_bottom_alpha * t;
        if alpha < 0.002 { continue; }
        for px in component_left..component_left + component_w {
            let abs_x = card_x + px;
            let abs_y = card_y + py;
            let in_component = in_rounded(abs_x, abs_y, card_x + component_left, card_y + component_top, component_w, component_h, component_radius);
            let in_card = in_rounded(abs_x, abs_y, card_x, card_y, card_w_i, card_h_i, RADIUS);
            if in_component && in_card {
                let p = canvas.get_pixel(abs_x as u32, abs_y as u32);
                canvas.put_pixel(abs_x as u32, abs_y as u32, Rgba([
                    ((p[0] as f32 * (1.0 - alpha) + CARD_WHITE[0] as f32 * alpha) as u8),
                    ((p[1] as f32 * (1.0 - alpha) + CARD_WHITE[1] as f32 * alpha) as u8),
                    ((p[2] as f32 * (1.0 - alpha) + CARD_WHITE[2] as f32 * alpha) as u8),
                    255,
                ]));
            }
        }
    }

    // 圆角描边
    {
        let edge_color = Rgba([0xD8, 0xD6, 0xD0, 0x60]);
        for py in card_y..card_y + card_h_i {
            for px in card_x..card_x + card_w_i {
                let inside = in_rounded(px, py, card_x, card_y, card_w_i, card_h_i, RADIUS);
                let outside_edge = !inside && in_rounded(px, py, card_x, card_y, card_w_i, card_h_i, RADIUS + 1.0);
                if outside_edge {
                    let bg = canvas.get_pixel(px as u32, py as u32);
                    let a = edge_color[3] as f32 / 255.0;
                    canvas.put_pixel(px as u32, py as u32, Rgba([
                        ((bg[0] as f32 * (1.0 - a) + edge_color[0] as f32 * a) as u8),
                        ((bg[1] as f32 * (1.0 - a) + edge_color[1] as f32 * a) as u8),
                        ((bg[2] as f32 * (1.0 - a) + edge_color[2] as f32 * a) as u8),
                        255,
                    ]));
                }
            }
        }
    }

    let cx = |x: i32| -> i32 { card_x + x };
    let cy = |y: i32| -> i32 { card_y + y };

    // ── v0.7.1: 上半「视觉焦点」= 大画作 + 主题词 + 句子（占满上半） ──
    // 画作已铺满全卡作背景（见 section 3）。文字层放在底部白色渐变覆层上。

    // ── 句子（左侧，居上半视觉焦点 —— y=380 在画作下半区，halo 保证可读） ──
    let content_x = cx(PAD);
    let content_w = (card_w_i - PAD * 2) as u32;
    let mut sy = cy(380);

    let cn_wrapped = wrap(&data.sentence, content_w, s_cn_sentence);
    for line in &cn_wrapped {
        sy = draw_with_shadow(&mut canvas, &cn_serif, line, content_x, sy, s_cn_sentence, TEXT_PRI);
    }

    if !data.english_sentence.is_empty() {
        sy += 6;
        let en_wrapped = wrap(&data.english_sentence, content_w, s_en_sentence);
        for line in &en_wrapped {
            sy = draw_with_shadow(&mut canvas, &en_serif, line, content_x, sy, s_en_sentence, TEXT_SEC);
        }
    }

    // ── v0.7.1: 7. 统计：下半「数据焦点」—— 4 张 stat 卡横排（左侧 70% 宽）──
    let top1_key = &data.top1_key;
    let freq_str = format!("{:.0} /min", data.frequency_per_min);
    let total_str = if data.total_keys < 1000 {
        format!("{} ", data.total_keys)
    } else if data.total_keys < 10000 {
        format!("{:.1}K", data.total_keys as f64 / 1000.0)
    } else {
        format!("{:.1}W", data.total_keys as f64 / 10000.0)
    };
    let act_lvl = (data.activity_hours.min(16) as f32 / 16.0 * 5.0).round() as u8;
    let act_color = match act_lvl { 0..=1 => TEXT_SEC, 2..=3 => WARM, _ => GREEN };

    let s_label = PxScale { x: 16.0, y: 16.0 };
    let s_val   = PxScale { x: 28.0, y: 28.0 };
    let s_sub    = PxScale { x: 12.0, y: 12.0 };

    let stat_labels = ["\u{6700}\u{5E38}\u{7528}\u{952E}", "\u{8F93}\u{5165}\u{9891}\u{7387}", "\u{603B}\u{6309}\u{952E}\u{6570}", "\u{6D3B}\u{8DC3}\u{7A0B}\u{5EA6}"];
    let stat_values = [top1_key.as_str(), freq_str.as_str(), total_str.as_str(), ""];

    // 16:9 横向布局 —— 4 张 stat 卡等宽铺在底部左侧，QR 右侧
    // v0.7.4: 容器渐变反转（顶透明→底不透明），stats 移到 y=620 落入 ~55% 不透明区
    let qr_w = QR_SIZE as i32;
    let qr_gap = 32i32;
    let stat_area_w = (card_w_i - PAD * 2 - qr_w - qr_gap) as u32;
    let stat_col_w = stat_area_w / 4;
    let stat_y = cy(620);

    for i in 0..4 {
        let col_x = cx(PAD) + (i as i32 * stat_col_w as i32);
        // 标签（halo）
        draw_with_shadow(&mut canvas, &cn_sans, stat_labels[i], col_x, stat_y, s_label, WARM);
        // 数值
        if i == 3 {
            // 活跃度用 5 个圆点 + 文字描述（横排）
            let dot_y = stat_y + 28;
            let dot_x = col_x;
            for j in 0..5u8 {
                let c = if j < act_lvl { "\u{25CF}" } else { "\u{25CB}" };
                draw_with_shadow(&mut canvas, &cn_sans, c, dot_x + j as i32 * 18, dot_y, PxScale { x: 12.0, y: 12.0 }, act_color);
            }
            let desc = match act_lvl { 0=>"\u{51E0}\u{4E4E}\u{6CA1}\u{52A8}", 1=>"\u{5C11}\u{8BB8}\u{6D3B}\u{52A8}", 2=>"\u{8F7B}\u{5EA6}\u{6D3B}\u{8DC3}", 3=>"\u{4E2D}\u{5EA6}\u{6D3B}\u{8DC3}", 4=>"\u{9AD8}\u{5EA6}\u{6D3B}\u{8DC3}", _=>"\u{8D85}\u{6D3B}\u{8DC3}" };
            draw_with_shadow(&mut canvas, &cn_sans, desc, dot_x + 5 * 18 + 6, dot_y - 2, s_sub, TEXT_SEC);
        } else {
            draw_with_shadow(&mut canvas, &en_bold, stat_values[i], col_x, stat_y + 28, s_val, act_color);
        }
    }

    // ── v0.7.1: 8. QR 码（右侧，缩到 120px 低调不抢眼） ──
    // v0.7.4: QR 下移到 y=595（容器下半部 ~44%-85% 不透明区，扫码识别稳定）
    let qr_img = make_qr(qr_url, QR_SIZE)?;
    let qx = cx(card_w_i - PAD - qr_w);
    let qy = cy(595);
    canvas.copy_from(&qr_img, qx as u32, qy as u32)?;

    let qr_label_y = qy + QR_SIZE as i32 + 6;
    let qr_center_x = qx + QR_SIZE as i32 / 2;
    // QR 标签在底部右侧（梯度区），加 halo 保证在彩色画作上可读
    let qr_label_w = measure("\u{626B}\u{7801}\u{542C}\u{97F3}\u{4E50}", s_qr_label);
    let qr_label_x = qr_center_x - qr_label_w / 2;
    draw_with_shadow(&mut canvas, &cn_sans, "\u{626B}\u{7801}\u{542C}\u{97F3}\u{4E50}", qr_label_x, qr_label_y, s_qr_label, TEXT_SEC);

    // ── 9. 信息栏（脚注位于容器底部 —— 容器已 fade 到几乎透明，靠 halo 保证可读） ──
    // v0.7.3: 删 v0.7.1 的分割线（容器本身就是视觉分隔），脚注下移到 y=685（更靠容器底部）
    let footer_y = cy(685);

    let mood = data.mood.as_deref().unwrap_or("\u{2014}");
    let time_info = if data.time_range_label.is_empty() {
        data.date.clone()
    } else {
        format!("{}  {}", data.date, data.time_range_label)
    };
    let footer = format!(
        "FingerTip  \u{00B7}  {}  \u{00B7}  \u{4E3B}\u{9898}\u{8BCD} {}  \u{00B7}  \u{5FC3}\u{60C5} {}",
        time_info, data.theme_word, mood
    );
    let fw = measure(&footer, s_footer);
    let fx = card_x + (CARD_W as i32 - fw) / 2;
    draw_with_shadow(&mut canvas, &cn_sans, &footer, fx.max(card_x + PAD), footer_y, s_footer, TEXT_SEC);

    // ── 10. 输出 ──
    let out = std::env::temp_dir().join("fingertip-card.png");
    canvas.save(&out)?;
    Ok(out)
}

// ═══════════════════════════════════════════════════════════
// 分享入口
// ═══════════════════════════════════════════════════════════

pub async fn create_share(data: &SharePageData) -> anyhow::Result<QrArtifact> {
    // 1. 上传 WAV + PNG（HTML 不再上传，避免扫码后强制下载）
    let (wav_url, png_url) = upload_share_assets(&data.wav_path, &data.png_path).await;
    let audio_ok = wav_url.is_some() && png_url.is_some();
    let share_url = match (&wav_url, &png_url) {
        (Some(w), Some(p)) => build_share_url(data, w, p),
        _ => landing_page_url(),
    };

    // 2. 合成卡片 PNG（spawn_blocking 因为 image 操作是 CPU 密集）
    let png = data.png_path.clone();
    let s = data.sentence.clone();
    let es = data.english_sentence.clone();
    let tw = data.theme_word.clone();
    let m = data.mood.clone();
    let d = data.date.clone();
    let top1 = data.top1_key.clone();
    let freq = data.frequency_per_min;
    let total = data.total_keys;
    let act = data.activity_hours;
    let hl = data.hourly;
    let tr = data.time_range_label.clone();
    let dl = data.download_url.clone();
    let fu = data.funny_summary.clone();
    let share_url_for_card = share_url.clone();

    let path = tokio::task::spawn_blocking(move || {
        let sd = SharePageData {
            wav_path: PathBuf::new(),
            png_path: png,
            sentence: s,
            english_sentence: es,
            theme_word: tw,
            mood: m,
            date: d,
            top1_key: top1,
            frequency_per_min: freq,
            total_keys: total,
            activity_hours: act,
            hourly: hl,
            time_range_label: tr,
            download_url: dl,
            funny_summary: fu,
        };
        generate_card_png(&sd, &share_url_for_card)
    }).await??;

    Ok(QrArtifact {
        local_path: path.display().to_string(),
        audio_ok,
        share_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_qr_works() {
        let img = make_qr("FingerTip|2026-08-09|rain", 100).unwrap();
        assert_eq!(img.dimensions(), (100, 100));
    }

    #[test]
    fn build_share_url_embeds_decodable_payload() {
        use base64::Engine as _;

        let data = SharePageData {
            wav_path: PathBuf::from("/tmp/music.wav"),
            png_path: PathBuf::from("/tmp/art.png"),
            sentence: "今天键盘很有节奏，每一次敲击都像在给这一天写下一句悄悄话，安静又完整。".into(),
            english_sentence: "Every keystroke today felt like writing a quiet line of the day, soft, steady and complete. A rhythm only my keyboard knows.".into(),
            theme_word: "FLOW".into(),
            mood: Some("开心".into()),
            date: "2026-08-12".into(),
            top1_key: "S".into(),
            frequency_per_min: 42.0,
            total_keys: 1234,
            activity_hours: 6,
            hourly: [0; 24],
            time_range_label: "09:00-15:00".into(),
            download_url: String::new(),
            funny_summary: "今天键盘敲了1234下，S键独领风骚——编辑还是摸鱼？键盘自己都说不清。".into(),
        };
        let url = build_share_url(&data, "https://x/music.wav", "https://x/art.png");
        assert!(url.starts_with(&landing_page_url()));
        let fragment = url.split_once("#d=").unwrap().1;
        let json = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(fragment)
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(v["w"], "https://x/music.wav");
        assert_eq!(v["p"], "https://x/art.png");
        assert_eq!(v["s"], "今天键盘很有节奏，每一次敲击都像在给这一天写下一句悄悄话，安静又完整。");
        // 长 payload 必须能生成高纠错二维码（分享页扫码可靠性）
        let img = make_qr(&url, 320).unwrap();
        assert_eq!(img.dimensions(), (320, 320));
    }

    /// v0.7.1: 真实渲染 1280×720 海报 PNG（双重组件布局验证）。
    /// 用一张 256×256 纯色 PNG 作"画作"输入 → 调 generate_card_png → 出实际文件。
    /// 验证：CARD_W/H 16:9 比例 + QR_SIZE=120 + 4 张 stat 卡横排 + 句子层 + 渐变覆层都画上。
    #[test]
    fn generate_card_png_renders_16_9_with_real_assets() {
        use image::{ImageBuffer, Rgba};
        // 1) 造一张 256×256 彩色渐变 PNG 模拟真实 AI 画作
        // 暖橙 → 玫瑰粉 → 紫蓝 渐变 + 一些低频噪点（让"画作"有视觉内容，但无白方块误导）
        let art_dir = tempfile::TempDir::new().unwrap();
        let art_path = art_dir.path().join("art.png");
        let mut art_img = ImageBuffer::from_pixel(256u32, 256u32, Rgba([214u8, 123, 79, 255]));
        for y in 0..256u32 {
            for x in 0..256u32 {
                let t = (x + y) as f32 / 512.0;  // 0..1
                let r = ((214.0 * (1.0 - t)) + (210.0 - 70.0 * (y as f32 / 256.0))) as u8;
                let g = ((123.0 * (1.0 - t)) + (90.0 + 40.0 * (x as f32 / 256.0))) as u8;
                let b = ((79.0 * (1.0 - t))  + (160.0 - 30.0 * (1.0 - t))) as u8;
                art_img.put_pixel(x, y, Rgba([r.min(255), g.min(255), b.min(255), 255]));
            }
        }
        art_img.save(&art_path).unwrap();

        let out_dir = tempfile::TempDir::new().unwrap();
        std::env::set_current_dir(&out_dir).ok(); // create_card_png 写到 cwd

        let data = SharePageData {
            wav_path: PathBuf::from("/tmp/music.wav"),
            png_path: art_path.clone(),
            sentence: "今天键盘很有节奏，每一次敲击都像在给这一天写下一句悄悄话。".into(),
            english_sentence: "Every keystroke today felt like a quiet rhythm.".into(),
            theme_word: "FLOW".into(),
            mood: Some("开心".into()),
            date: "2026-08-13".into(),
            top1_key: "S".into(),
            frequency_per_min: 42.0,
            total_keys: 1234,
            activity_hours: 6,
            hourly: [0; 24],
            time_range_label: "09:00-15:00".into(),
            download_url: String::new(),
            funny_summary: "今天键盘敲了1234下，S键独领风骚。".into(),
        };

        let path = generate_card_png(&data, "https://example.com/share#d=abc").unwrap();

        // 1) 文件存在 + 是合法 PNG
        assert!(path.exists(), "海报 PNG 没生成");
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..8], &b"\x89PNG\r\n\x1a\n"[..], "PNG 签名不对");

        // 2) 解码后尺寸：canvas = CARD_W + 2*BLUR_PAD（阴影 pad），卡片内容本身是 16:9
        let img = image::open(&path).unwrap();
        let (w, h) = (img.width(), img.height());
        let expected_w = (CARD_W + (BLUR_PAD as u32) * 2) as u32;
        let expected_h = (CARD_H + (BLUR_PAD as u32) * 2) as u32;
        assert_eq!(w, expected_w, "海报宽度 {} 应为 {}", w, expected_w);
        assert_eq!(h, expected_h, "海报高度 {} 应为 {}", h, expected_h);
        // 卡片本体（裁掉阴影 pad 后的区域）必须是 16:9
        assert_eq!(CARD_W, 1280, "CARD_W 必须 1280（16:9 宽）");
        assert_eq!(CARD_H, 720, "CARD_H 必须 720（16:9 高）");
        let card_ratio = CARD_W as f32 / CARD_H as f32;
        assert!((card_ratio - 16.0 / 9.0).abs() < 0.001, "卡片比例 {} 不是 16:9", card_ratio);

        // 3) 复制海报到 dev 能看见的位置（便于用户目测）
        let dest = std::path::PathBuf::from(r"C:\Users\singsky\AppData\Local\Temp\fingertip-poster-test.png");
        std::fs::copy(&path, &dest).ok();
        eprintln!("[测试海报] 已生成：{} ({}x{} = {}KB)", dest.display(), w, h, bytes.len() / 1024);
    }
}
