//! 云端客户端：MiniMax（chat 编排器 + music + image）。
//!
//! v0.4「编排器 LLM → 专有模型」改造的云端兑底接入层。
//! 本模块只封装对云端 API 的 HTTP 调用，不接 UI/命令，不关心路由决策
//! （路由在 T2 的 `config::route_capability`）。
//!
//! 云端契约（全部 MiniMax —— 让用户一个 key 配全链路 LLM+图像+音乐；
//! 以 minimax-music-gen skill 与真实 API 实测为准）：
//!   - MiniMax chat   → POST /v1/chat/completions（Bearer key + `response_format.json_schema`，
//!                     编排器 LLM 用；OpenAI 兼容端点不支持 json_object，报 2013）
//!   - MiniMax music  → POST /v1/music_generation（Bearer key，music-3.0 + is_instrumental，响应 `data.audio` 为 hex 音频）
//!   - MiniMax image  → POST /v1/image_generation（Bearer key，实返 `data.image_base64` 为 base64 JPEG → 转码 PNG）

// `base64::Engine` trait 提供 `decode` 方法，`generate_image` 的 production 路径
// (`base64::engine::general_purpose::STANDARD.decode(b64)`) 需要它在作用域内。
use base64::Engine as _;
// MiniMax 图像实返 **JPEG**（`data.image_base64`，非文档猜的 PNG/base64），下游
// art_png_path 契约要 PNG → 用 image crate 解码 JPEG + Cursor 写入 PNG。
use image;
use std::io::Cursor;

/// MiniMax 编排器 chat 端点路径（OpenAI 兼容，`/v1/chat/completions`）。
const MINIMAX_CHAT_PATH: &str = "/v1/chat/completions";
/// MiniMax json_schema 模式下必需的 schema name（实测缺 name 直接报错）。
const MINIMAX_ORCHESTRATOR_SCHEMA_NAME: &str = "orchestrator_output";
/// MiniMax 音乐端点路径（`/v1/music_generation`，按 minimax-music-gen skill）。
const MINIMAX_MUSIC_PATH: &str = "/v1/music_generation";
/// MiniMax 图像端点路径（`/v1/image_generation`，按 MiniMax 官方 API）。
const MINIMAX_IMAGE_PATH: &str = "/v1/image_generation";

/// 编排器 chat 响应容错：M3 等推理模型会先吐 `<think>...</think>`，且 json_schema
/// 模式下仍可能用 markdown code fence（```json ... ```）包 JSON。解析前统一剥掉
/// 这两类噪音，让 M3 / M2.x / Text-01 任意 MiniMax 模型都能进编排器。
///
/// v0.8.1: 剥完 think 后为空 → 返 "{}"（让调用方拿到明确 JSON 而非 EOF 晦涩错）。
/// 空说明 M3 思考块耗尽 max_tokens、JSON 未输出即被截断 —— 这是「响应被截断」而非
/// 「响应格式错」。返回 "{}" 后上游 parse 会报"缺必填字段"，比 serde 的
/// `EOF while parsing a value at line 1 column 0` 可诊断得多。
fn strip_llm_json_noise(content: &str) -> &str {
    let mut s = content.trim();
    // 剥掉 <think>...</think>（多段时取首 <think> 到末 </think> 整段删除）
    while let (Some(start), Some(end)) = (s.find("<think>"), s.rfind("</think>")) {
        if end > start {
            s = s[end + "</think>".len()..].trim_start();
        } else {
            break;
        }
    }
    // v0.8.1: 截断在思考中途（有 <think> 无闭合 </think>）→ 整段视为噪音（后面也不会有 JSON）
    if s.contains("<think>") && !s.contains("</think>") {
        log::warn!(
            "编排器响应含未闭合 <think>（疑似 max_tokens 截断在思考中）—— 按空响应处理"
        );
        return "{}";
    }
    // 剥掉 markdown code fence：开头 ```json / ```，截到末尾 ```（首尾各至少一个）
    for f in ["```json", "```"] {
        if s.starts_with(f) {
            if let Some(close) = s.rfind("```") {
                s = s[f.len()..close].trim();
            }
            break;
        }
    }
    if s.is_empty() {
        log::warn!(
            "编排器响应剥 think 后为空（content 长度 {}，前 200 字符: {}）—— 疑似 M3 思考耗尽 max_tokens、JSON 未输出",
            content.len(),
            &content[..content.len().min(200)]
        );
        return "{}";
    }
    s
}

/// MiniMax 编排器 chat 客户端（OpenAI 兼容端点 + json_schema response_format）。
///
/// `base_url` 形如 `https://api.minimaxi.com`（不带尾部斜杠）。
/// `key` 为 MiniMax API key（Bearer 头传递）。`model` 形如 `"MiniMax-M3"`。
///
/// v0.4.1：编排器（LLM）云端兑底改 MiniMax —— 让用户一个 MiniMax key 配全链路
/// （LLM + 图像 + 音乐），不再依赖 OpenAI。
///
/// 契约（已用真实 API 实测确认）：
///   - MiniMax 的 OpenAI 兼容端点**不支持** `response_format.json_object`
///     （报 2013 unknown type），必须用
///     `{"type":"json_schema","json_schema":{"name":..., "schema":{...}}}`
///   - json_schema 模式**必须**带 `name`（缺 name 报错），schema 约束三字段
///   - 响应 `choices[0].message.content` 是**包了一层 schema name 的 JSON 字符串**
///     （`{"orchestrator_output":{三字段}}`），本客户端 unwrap 返回内层三字段对象
pub struct MiniMaxChatClient {
    base_url: String,
    key: String,
    model: String,
    http: reqwest::Client,
}

impl MiniMaxChatClient {
    /// 新建 MiniMax chat 客户端。
    ///
    /// 不在 `new` 里强制非空 —— 配置层（T1）已经校验，这里再 assert 反而冗余。
    pub fn new(
        base_url: impl Into<String>,
        key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("reqwest::Client 构建失败（TLS 后端缺失）");
        Self {
            base_url: base_url.into(),
            key: key.into(),
            model: model.into(),
            http,
        }
    }

    /// 调 MiniMax 编排器 chat（json_schema response_format），返回 unwrap 后的三字段 JSON。
    ///
    /// 请求体：`model` / `messages` / `response_format.type="json_schema"`（带 `name` +
    /// 三字段 schema）/ `max_tokens=2048`。响应先走 `check_minimax_base_resp` 业务码检查，
    /// 再取 `choices[0].message.content` 二次解析为 JSON，最后 unwrap 掉 schema name
    /// 包装层（顶层只有 1 个 key 且其 value 是 object → 返回内层）。
    ///
    /// v0.4.1: `max_tokens` 从 500 提到 2048 —— M3 是推理模型，真实编排 prompt 下
    /// `<think>` 思考块可占 ~1500+ token，500 会被思考耗尽、JSON 未输出即被截断
    /// （实测：500→只出 think 无 JSON；2048→think+JSON 完整可解析）。
    /// 注释里 77 行的旧值描述已废弃。
    /// `system` 为系统提示，`user` 为本次请求内容。
    pub async fn chat_json(
        &self,
        system: &str,
        user: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": MINIMAX_ORCHESTRATOR_SCHEMA_NAME,
                    "schema": {
                        "type": "object",
                        "properties": {
                            "music_description": {"type": "string"},
                            "image_description": {"type": "string"},
                            "sentence": {"type": "string"},
                            "english_sentence": {"type": "string"},
                            "theme_explanation": {"type": "string"},
                            "funny_summary": {"type": "string"},
                        },
                        "required": ["music_description", "image_description", "sentence"],
                    },
                },
            },
            "max_tokens": 4096,
        });
        log::info!("MiniMax chat 请求 → {}  model={}", format!("{}{}", self.base_url, MINIMAX_CHAT_PATH), self.model);
        let resp = self
            .http
            .post(format!("{}{}", self.base_url, MINIMAX_CHAT_PATH))
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("MiniMax chat 请求失败（网络/超时）: {}", e))?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("MiniMax chat 非 2xx: {}", e))?;
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("MiniMax chat 响应解析失败: {}", e))?;
        check_minimax_base_resp(&v)?;
        let content = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("MiniMax chat 响应缺 content"))?;
        log::info!("MiniMax chat 响应: content 前 120 字符 = {}", &content[..content.len().min(120)]);
        let parsed: serde_json::Value = serde_json::from_str(strip_llm_json_noise(content))
            .map_err(|e| anyhow::anyhow!("MiniMax chat 返回非 JSON: {}", e))?;
        // json_schema 会把三字段包在 `name`（orchestrator_output）一层：顶层只有 1 个
        // key 且其 value 是 object → unwrap 内层（编排器 parse 对两种形态都健壮，这里
        // 提前 unwrap 让 OpenAI 直出三字段 / MiniMax 包一层的外在表现一致）。
        if let Some(inner) = parsed.as_object().and_then(|o| {
            if o.len() == 1 {
                o.values().next()
            } else {
                None
            }
        }) {
            if inner.is_object() {
                return Ok(inner.clone());
            }
        }
        Ok(parsed)
    }
}

/// MiniMax 音乐生成客户端（`POST /v1/music_generation`）。
///
/// `base_url` 形如 `https://api.MiniMax.chat`（不带尾部斜杠）。
/// `key` 为 MiniMax API key（Bearer 头传递）。`model` 形如 `"music-3.0"`
/// （music-01 已不可用，当前可用模型是 music-3.0）。
pub struct MiniMaxMusicClient {
    base_url: String,
    key: String,
    model: String,
    http: reqwest::Client,
}

impl MiniMaxMusicClient {
    /// 新建 MiniMax 音乐客户端。
    pub fn new(
        base_url: impl Into<String>,
        key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        // v0.4.1: 超时从 300s 提到 600s —— 实测 music-3.0 生成约 149s，免费版
        // （music-3.0-free）可能更慢（免费队列），300s 可能掐断正常生成。
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .expect("reqwest::Client 构建失败（TLS 后端缺失）");
        Self {
            base_url: base_url.into(),
            key: key.into(),
            model: model.into(),
            http,
        }
    }

    /// 按文本描述生成一段音乐，返回音频原始字节（WAV）。
    ///
    /// 契约（minimax-music-gen skill，已用真实 API 实测修正）：
    ///   - 请求体: `{ "model": "music-3.0", "prompt", "is_instrumental": true,
    ///     "audio_setting": { "sample_rate": 44100, "bitrate": 256000, "format": "wav" } }`
    ///     （music-01 已不可用报 "invalid model"；`is_instrumental:true` 纯器乐，缺则强制
    ///     填词；WAV 必须写在 `audio_setting.format` —— 顶层 `audio_format` 是 music-02
    ///     专属，会 2013 报错）
    ///   - 鉴权: `Authorization: Bearer <key>`
    ///   - 响应体: `{ "data": { "audio": "<hex 编码的音频字节>" } }`
    ///   - 网络/HTTP/解析错误都带中文上下文抛出
    pub async fn generate_audio(&self, text: &str) -> anyhow::Result<Vec<u8>> {
        let body = serde_json::json!({
            "model": self.model,
            "prompt": text,
            "is_instrumental": true,
            "audio_setting": {"sample_rate": 44100, "bitrate": 256000, "format": "wav"},
        });
        log::info!("MiniMax music 请求 → {}  model={}（生成耗时可能 30s+）", format!("{}{}", self.base_url, MINIMAX_MUSIC_PATH), self.model);
        let resp = self
            .http
            .post(format!("{}{}", self.base_url, MINIMAX_MUSIC_PATH))
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("MiniMax music 请求失败（网络/超时）: {}", e))?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("MiniMax music 非 2xx: {}", e))?;
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("MiniMax music 响应解析失败: {}", e))?;
        check_minimax_base_resp(&v)?;
        let hex_audio = v["data"]["audio"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("MiniMax music 响应缺 audio"))?;
        log::info!("MiniMax music 响应: audio hex 长度 = {} 字符", hex_audio.len());
        decode_hex(hex_audio).map_err(|e| anyhow::anyhow!("MiniMax music hex 解码失败: {}", e))
    }
}

/// MiniMax 图像生成客户端（`POST /v1/image_generation`）。
///
/// `base_url` 形如 `https://api.minimaxi.com`（不带尾部斜杠）。
/// `key` 为 MiniMax API key（Bearer 头传递）。`model` 形如 `"image-01"`。
///
/// v0.4.1：图像云端兑底改 MiniMax（OpenAiClient 已删干净，编排器 LLM 也走 MiniMax）。
/// 用 `response_format:"base64"` 模式拿图片字节 —— URL 模式有 24h 过期，不落盘。
/// 实测实返 JPEG（`data.image_base64`），本客户端解码后
/// **转码 PNG** 返回（下游 art_png_path 契约要 PNG）。
pub struct MiniMaxImageClient {
    base_url: String,
    key: String,
    model: String,
    http: reqwest::Client,
}

impl MiniMaxImageClient {
    /// 新建 MiniMax 图像客户端。
    pub fn new(
        base_url: impl Into<String>,
        key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("reqwest::Client 构建失败（TLS 后端缺失）");
        Self {
            base_url: base_url.into(),
            key: key.into(),
            model: model.into(),
            http,
        }
    }

    /// 按文本描述生成一张图片，返回图片原始字节（PNG）。
    ///
    /// 契约（MiniMax 官方 API，已用真实 API 实测修正）：
    ///   - 请求体: `{ "model", "prompt", "aspect_ratio": "1:1",
    ///     "response_format": "base64", "n": 1, "prompt_optimizer": true }`
    ///   - 鉴权: `Authorization: Bearer <key>`
    ///   - 响应体: `{ "id": "...", "data": { "image_base64": ["<base64 JPEG>"] } }`
    ///     —— 实测字段是 `data.image_base64`（数组元素为 base64 编码的 **JPEG**，
    ///     非文档猜的 `base64`/`image_urls`）
    ///   - 优先 `data.image_base64[0]` → base64 解码 → **转码 JPEG → PNG** 返回；
    ///     若缺则退 `data.image_urls[0]`（再 GET 该 URL 拿字节，兜底 —— URL 模式
    ///     24h 过期，同样转码 PNG）
    ///   - 网络/HTTP/解析/base64 解码/转码错误都带中文上下文抛出
    pub async fn generate_image(&self, prompt: &str) -> anyhow::Result<Vec<u8>> {
        let body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "aspect_ratio": "1:1",
            "response_format": "base64",
            "n": 1,
            "prompt_optimizer": true,
        });
        log::info!("MiniMax image 请求 → {}  model={}", format!("{}{}", self.base_url, MINIMAX_IMAGE_PATH), self.model);
        let resp = self
            .http
            .post(format!("{}{}", self.base_url, MINIMAX_IMAGE_PATH))
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("MiniMax image 请求失败（网络/超时）: {}", e))?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("MiniMax image 非 2xx: {}", e))?;
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("MiniMax image 响应解析失败: {}", e))?;
        check_minimax_base_resp(&v)?;

        // 1. 优先 base64 模式：`data.image_base64[0]`（实测字段，数组元素是 base64 JPEG）
        if let Some(b64) = v["data"]["image_base64"][0].as_str() {
            log::info!("MiniMax image 响应: image_base64 长度 = {} 字符", b64.len());
            let jpeg = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| anyhow::anyhow!("MiniMax image base64 解码失败: {}", e))?;
            return image_bytes_to_png(&jpeg);
        }

        // 2. 兜底 url 模式：`data.image_urls[0]` → 再 GET 拿字节 → 也转 PNG
        if let Some(url) = v["data"]["image_urls"][0].as_str() {
            let bytes = self
                .http
                .get(url)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("MiniMax image url 拉取失败（网络/超时）: {}", e))?
                .error_for_status()
                .map_err(|e| anyhow::anyhow!("MiniMax image url 拉取非 2xx: {}", e))?
                .bytes()
                .await
                .map_err(|e| anyhow::anyhow!("MiniMax image url 拉取响应读取失败: {}", e))?;
            return image_bytes_to_png(&bytes);
        }

        anyhow::bail!("MiniMax image 响应缺 image_base64/image_urls")
    }
}

/// 把 MiniMax 返回的图片字节解码并转码为 PNG。
///
/// MiniMax 实测实返 **JPEG**（`data.image_base64` 元素，或 url 拉取的字节），而下游
/// art_png_path 契约要 PNG。用 `image` crate 解码输入 → 以 PNG 编码输出，恒返回 PNG。
/// 输入不是可解码图片 → 中文上下文报错。
fn image_bytes_to_png(src: &[u8]) -> anyhow::Result<Vec<u8>> {
    let img = image::load_from_memory(src)
        .map_err(|e| anyhow::anyhow!("MiniMax image 图片解码失败（非 JPEG/PNG？）: {}", e))?;
    let mut png = Vec::new();
    img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| anyhow::anyhow!("MiniMax image PNG 编码失败: {}", e))?;
    Ok(png)
}

/// 解码 hex 字符串为字节数组。
///
/// 用 `from_str_radix` 在字节级别解码，避开 `hex::decode` 的额外依赖（hex crate
/// 还没装，引入只为这一个调用不划算）。失败 → 抛 anyhow 错误，调用方包中文上下文。
fn decode_hex(s: &str) -> anyhow::Result<Vec<u8>> {
    let bytes = s.as_bytes();
    if bytes.len() % 2 != 0 {
        anyhow::bail!("hex 长度为奇数（{} 字节）", bytes.len());
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> anyhow::Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => anyhow::bail!("非法 hex 字符 0x{:02x}", b),
    }
}

/// 解析 MiniMax 响应时先查业务错误码。
///
/// MiniMax 的错误不靠 HTTP 状态码表达，而是 **HTTP 200 + 响应体
/// `{"base_resp":{"status_code":<非0>,"status_msg":"..."}}`**（实测无 key 时
/// music/image 端点都返回 `status_code=1004` "login fail"）。不查会漏过业务失败，
/// 让调用方继续取缺字段 → 报误导性的"缺 audio/缺 base64"。
///
/// 成功时 `base_resp.status_code == 0`（或缺省 base_resp），此处放行。
/// 放在取业务字段之前调用；两 MiniMax 客户端共用（DRY）。
fn check_minimax_base_resp(v: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(code) = v
        .get("base_resp")
        .and_then(|b| b.get("status_code"))
        .and_then(|c| c.as_i64())
    {
        if code != 0 {
            let msg = v
                .get("base_resp")
                .and_then(|b| b.get("status_msg"))
                .and_then(|m| m.as_str())
                .unwrap_or("未知错误");
            anyhow::bail!("MiniMax 业务错误 status_code={}: {}", code, msg);
        }
    }
    Ok(())
}

// ── trait impl：让编排器/音乐适配器能拿 `&dyn JsonChat` / `&dyn AudioClient` ──

use super::{AudioClient, ImageClient, JsonChat};

/// `JsonChat` for `MiniMaxChatClient`：复用现有 `chat_json(system, user)` 方法。
///
/// 签名天然对齐（trait 要求 `&self, system, user`），直接 forward 即可。
#[async_trait::async_trait]
impl JsonChat for MiniMaxChatClient {
    async fn chat_json(&self, system: &str, user: &str) -> anyhow::Result<serde_json::Value> {
        MiniMaxChatClient::chat_json(self, system, user).await
    }
}

/// `AudioClient` for `MiniMaxMusicClient`：复用现有 `generate_audio(text)` 方法。
///
/// 签名天然对齐（trait 要求 `&self, text`），直接 forward 即可。
#[async_trait::async_trait]
impl AudioClient for MiniMaxMusicClient {
    async fn generate_audio(&self, text: &str) -> anyhow::Result<Vec<u8>> {
        MiniMaxMusicClient::generate_audio(self, text).await
    }
}

/// `ImageClient` for `MiniMaxImageClient`：复用现有 `generate_image(prompt)` 方法。
///
/// 签名天然对齐（trait 要求 `&self, prompt`），直接 forward 即可。
/// 这样艺术适配器（T9）能拿 `&dyn ImageClient`，本地引擎与云端 MiniMax 同接口。
#[async_trait::async_trait]
impl ImageClient for MiniMaxImageClient {
    async fn generate_image(&self, prompt: &str) -> anyhow::Result<Vec<u8>> {
        MiniMaxImageClient::generate_image(self, prompt).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[tokio::test]
    async fn minimax_chat_uses_json_schema_and_returns_three_fields() {
        // 实测契约：MiniMax OpenAI 兼容端点用 json_schema（非 json_object）+ 必须带 name；
        // 响应 content 是包了一层 name（orchestrator_output）的 JSON → 客户端 unwrap 返回内层。
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path(MINIMAX_CHAT_PATH)
                .header("Authorization", "Bearer mm-key")
                .json_body_partial(
                    r#"{"model":"MiniMax-M3","response_format":{"type":"json_schema"}}"#,
                );
            then.status(200).json_body(serde_json::json!({
                "base_resp":{"status_code":0},
                "choices":[{"message":{"content":"{\"orchestrator_output\":{\"music_description\":\"m\",\"image_description\":\"i\",\"sentence\":\"s\"}}"}}]
            }));
        });
        let c = MiniMaxChatClient::new(server.base_url(), "mm-key", "MiniMax-M3");
        let v = c.chat_json("sys", "user").await.unwrap();
        assert_eq!(v["music_description"], "m");
    }

    #[tokio::test]
    async fn minimax_chat_strips_m3_reasoning_noise() {
        // M3 是推理模型：json_schema 下仍可能先吐 `<think>` 再用 markdown code fence
        // 包 JSON。容错剥掉这两类噪音后必须照常返回三字段（实测契约）。
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path(MINIMAX_CHAT_PATH);
            then.status(200).json_body(serde_json::json!({
                "base_resp":{"status_code":0},
                "choices":[{"message":{"content":
                    "<think>分析用户心情</think>```json\n{\"orchestrator_output\":{\"music_description\":\"m\",\"image_description\":\"i\",\"sentence\":\"s\"}}\n```"
                }}]
            }));
        });
        let c = MiniMaxChatClient::new(server.base_url(), "mm-key", "MiniMax-M3");
        let v = c.chat_json("sys", "user").await.unwrap();
        assert_eq!(v["music_description"], "m");
        assert_eq!(v["sentence"], "s");
    }

    #[tokio::test]
    async fn minimax_chat_strips_think_then_plain_json() {
        // 实测 M3 + max_tokens=2048：`<think>...</think>` 后**直接**跟 JSON（无 markdown
        // fence）—— 编排器真实响应契约。剥掉 think 后必须能解析。
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path(MINIMAX_CHAT_PATH);
            then.status(200).json_body(serde_json::json!({
                "base_resp":{"status_code":0},
                "choices":[{"message":{"content":
                    "<think>Let me analyze the signals...</think>{\"orchestrator_output\":{\"music_description\":\"m\",\"image_description\":\"i\",\"sentence\":\"s\"}}"
                }}]
            }));
        });
        let c = MiniMaxChatClient::new(server.base_url(), "mm-key", "MiniMax-M3");
        let v = c.chat_json("sys", "user").await.unwrap();
        assert_eq!(v["music_description"], "m");
        assert_eq!(v["sentence"], "s");
    }

    #[test]
    fn strip_noise_removes_think_and_code_fence() {
        let raw = "<think>用户想放松</think>```json\n{\"music_description\":\"m\"}\n```";
        assert_eq!(strip_llm_json_noise(raw), "{\"music_description\":\"m\"}");
    }

    #[test]
    fn strip_noise_keeps_pure_json_untouched() {
        let raw = "  {\"music_description\":\"m\"}  ";
        assert_eq!(strip_llm_json_noise(raw), "{\"music_description\":\"m\"}");
    }

    #[test]
    fn strip_noise_removes_multiple_think_blocks() {
        let raw = "<think>a</think><think>b</think>{\"sentence\":\"s\"}";
        assert_eq!(strip_llm_json_noise(raw), "{\"sentence\":\"s\"}");
    }

    #[test]
    fn strip_noise_handles_fence_without_think() {
        let raw = "```json\n{\"image_description\":\"i\"}\n```";
        assert_eq!(strip_llm_json_noise(raw), "{\"image_description\":\"i\"}");
    }

    /// v0.8.1: M3 思考块耗尽 max_tokens → 剥 think 后为空（JSON 未输出即截断）。
    /// 不能返空串（serde 报晦涩 EOF），要返 "{}" 让编排器报「缺必填字段」可诊断。
    #[test]
    fn strip_noise_returns_empty_object_when_only_think_block_truncated() {
        // 闭合 think 但后面无 JSON（JSON 未输出即截断）→ 剥后空 → {}
        let raw_closed = "<think>Let me analyze the signals carefully...</think>";
        assert_eq!(strip_llm_json_noise(raw_closed), "{}", "剥闭合 think 后空应返空对象");
        // 无闭合 <think>（截断在思考中）→ 整段噪音 → {}
        let raw_truncated = "<think>Let me analyze the signals carefully: Theme REWRITE Emotion excited";
        assert_eq!(strip_llm_json_noise(raw_truncated), "{}", "未闭合 think 应按空响应处理");
    }

    #[tokio::test]
    async fn minimax_chat_errors_on_business_error_code() {
        // check_minimax_base_resp 对 chat 同样生效：HTTP 200 + base_resp.status_code 非 0
        // → 不能静默当成功（复用了 music/image 的业务码检查）。
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path(MINIMAX_CHAT_PATH);
            then.status(200).json_body(serde_json::json!({
                "base_resp":{"status_code":1004,"status_msg":"login fail"}
            }));
        });
        let c = MiniMaxChatClient::new(server.base_url(), "", "MiniMax-M3");
        let err = c.chat_json("sys", "user").await.unwrap_err();
        assert!(err.to_string().contains("login fail") || err.to_string().contains("1004"));
    }

    #[tokio::test]
    async fn minimax_image_decodes_image_base64_and_converts_to_png() {
        // 实测契约：字段是 `data.image_base64[0]`（base64 编码的 **JPEG**），客户端
        // 应解码并转码为 PNG 返回（下游 art_png_path 契约要 PNG，不是 JPEG）。
        let server = MockServer::start();
        let img = image::RgbImage::from_pixel(2, 2, image::Rgb([200, 100, 50]));
        let mut jpeg_cursor = Cursor::new(Vec::new());
        img.write_to(&mut jpeg_cursor, image::ImageFormat::Jpeg).unwrap();
        let jpeg = jpeg_cursor.into_inner();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
        server.mock(|when, then| {
            when.method(POST)
                .path(MINIMAX_IMAGE_PATH)
                .header("Authorization", "Bearer mm-key");
            then.status(200).json_body(serde_json::json!({
                "base_resp": {"status_code": 0},
                "data": {"image_base64": [b64]}
            }));
        });
        let c = MiniMaxImageClient::new(server.base_url(), "mm-key", "image-01");
        let png = c.generate_image("orange abstract").await.unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "返回应为 PNG 字节");
    }

    #[tokio::test]
    async fn minimax_image_falls_back_to_url_fetch_and_transcodes() {
        // data.image_urls[0] 兜底 → 再 GET 该 URL 拿 JPEG 字节 → 也转码 PNG
        let server = MockServer::start();
        let img = image::RgbImage::from_pixel(2, 2, image::Rgb([200, 100, 50]));
        let mut jpeg_cursor = Cursor::new(Vec::new());
        img.write_to(&mut jpeg_cursor, image::ImageFormat::Jpeg).unwrap();
        let jpeg = jpeg_cursor.into_inner();
        server.mock(|when, then| {
            when.method(POST).path(MINIMAX_IMAGE_PATH);
            then.status(200).json_body(serde_json::json!({
                "data": {"image_urls": [format!("{}/img.jpg", server.base_url())]}
            }));
        });
        server.mock(|when, then| {
            when.method(GET).path("/img.jpg");
            then.status(200).body(jpeg.clone());
        });
        let c = MiniMaxImageClient::new(server.base_url(), "mm-key", "image-01");
        let png = c.generate_image("x").await.unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "URL 兜底也应返回 PNG");
    }

    #[tokio::test]
    async fn minimax_image_errors_when_both_missing() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path(MINIMAX_IMAGE_PATH);
            then.status(200).json_body(serde_json::json!({"data": {}}));
        });
        let c = MiniMaxImageClient::new(server.base_url(), "mm-key", "image-01");
        assert!(c.generate_image("x").await.is_err());
    }

    #[tokio::test]
    async fn minimax_image_errors_on_http_500() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path(MINIMAX_IMAGE_PATH);
            then.status(500).body("boom");
        });
        let c = MiniMaxImageClient::new(server.base_url(), "mm-key", "image-01");
        assert!(c.generate_image("x").await.is_err());
    }

    #[tokio::test]
    async fn minimax_audio_returns_wav_bytes() {
        let server = MockServer::start();
        // "RIFF....WAVE" → "524946462020202057415645"
        let audio_hex = "524946462020202057415645";
        server.mock(|when, then| {
            when.method(POST)
                .path(MINIMAX_MUSIC_PATH)
                .header("Authorization", "Bearer mm-key")
                // 实测契约：music-3.0 + is_instrumental + audio_setting.format（顶层 audio_format 已废弃）
                .json_body_partial(
                    r#"{"model":"music-3.0","is_instrumental":true,"audio_setting":{"format":"wav"}}"#,
                );
            then.status(200).json_body(serde_json::json!({
                "data": {"audio": audio_hex}
            }));
        });
        let c = MiniMaxMusicClient::new(server.base_url(), "mm-key", "music-3.0");
        let b = c.generate_audio("calm piano").await.unwrap();
        // "RIFF" + "...." + "WAVE" = 12 字节
        assert_eq!(b.len(), 12);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(&b[8..12], b"WAVE");
    }

    #[tokio::test]
    async fn minimax_audio_errors_on_non_hex() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path(MINIMAX_MUSIC_PATH);
            then.status(200).json_body(serde_json::json!({
                "data": {"audio": "ZZZZ"}
            }));
        });
        let c = MiniMaxMusicClient::new(server.base_url(), "mm-key", "music-model");
        let err = c.generate_audio("calm piano").await.unwrap_err();
        assert!(err.to_string().contains("hex 解码失败"));
    }

    #[tokio::test]
    async fn minimax_audio_errors_when_audio_missing() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path(MINIMAX_MUSIC_PATH);
            then.status(200).json_body(serde_json::json!({"data": {}}));
        });
        let c = MiniMaxMusicClient::new(server.base_url(), "mm-key", "music-model");
        let err = c.generate_audio("calm piano").await.unwrap_err();
        assert!(err.to_string().contains("缺 audio"));
    }

    #[tokio::test]
    async fn minimax_audio_errors_on_http_500() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path(MINIMAX_MUSIC_PATH);
            then.status(500);
        });
        let c = MiniMaxMusicClient::new(server.base_url(), "mm-key", "music-model");
        let err = c.generate_audio("calm piano").await.unwrap_err();
        assert!(err.to_string().contains("非 2xx"));
    }

    #[tokio::test]
    async fn minimax_music_errors_on_business_error_code() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/music_generation");
            then.status(200).json_body(serde_json::json!({"base_resp":{"status_code":1004,"status_msg":"login fail"}}));
        });
        let c = MiniMaxMusicClient::new(server.base_url(), "", "music-01");
        let err = c.generate_audio("x").await.unwrap_err();
        assert!(err.to_string().contains("login fail") || err.to_string().contains("1004"));
    }

    #[tokio::test]
    async fn minimax_image_errors_on_business_error_code() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/image_generation");
            then.status(200).json_body(serde_json::json!({"base_resp":{"status_code":1004,"status_msg":"login fail"}}));
        });
        let c = MiniMaxImageClient::new(server.base_url(), "", "image-01");
        let err = c.generate_image("x").await.unwrap_err();
        assert!(err.to_string().contains("login fail") || err.to_string().contains("1004"));
    }
}
