//! EngineClient：FingerTip-Engine（本地可选插件，Python 服务）的 HTTP 客户端。
//!
//! v0.4「编排器 LLM → 专有模型」改造的本地引擎接入层。
//! 本模块只封装对引擎的 HTTP 调用（health / chat / images / audio），
//! 不接 UI/命令，不关心路由决策（路由在 T2 的 `config::route_capability`）。
//!
//! 引擎契约（T10 实现，OpenAI 兼容 + Step-Audio 自定义端点）：
//!   - GET  /v1/health            → `{"llm":bool,"image":bool,"audio":bool}`（三态路由探测用）
//!   - POST /v1/chat/completions   → OpenAI 兼容，带 `response_format.json_object` 强制 JSON
//!   - POST /v1/images/generations → OpenAI 兼容，`response_format:"b64_json"` 返回 base64
//!   - POST /v1/audio              → Step-Audio 自定义端点，返回原始字节

use serde::Deserialize;

use super::{AudioClient, ImageClient, JsonChat};

/// chat 端点（/v1/chat/completions）使用的模型名 —— T10 引擎必须接受这个值
const ENGINE_CHAT_MODEL: &str = "fingertip-llm";
/// image 端点（/v1/images/generations）使用的模型名 —— T10 引擎必须接受这个值
const ENGINE_IMAGE_MODEL: &str = "fingertip-image";

/// 引擎能力探测结果（`GET /v1/health` 响应体）。
///
/// 三态路由（`config::route_capability`）用它判断本地引擎是否就绪：
/// `llm` = LLM 推理可用，`image` = 图像生成可用，`audio` = 音频生成可用。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EngineHealth {
    pub llm: bool,
    pub image: bool,
    pub audio: bool,
}

/// FingerTip-Engine 的 HTTP 客户端。
///
/// 只封装对引擎的调用，不持有配置（`base_url` 由调用方注入，来源为
/// `FingertipConfig::engine.base_url`）。所有方法返回 `anyhow::Result`，
/// 网络/HTTP/解析错误都带上下文抛出（「失败要大声」）。
#[derive(Clone)]
pub struct EngineClient {
    base_url: String,
    http: reqwest::Client,
}

impl EngineClient {
    /// 新建客户端。`base_url` 形如 `http://127.0.0.1:8765`（不带尾部斜杠）。
    ///
    /// 超时策略：引擎是可选插件，挂了要快速失败 → `connect_timeout(3s)`；
    /// 但生成类调用可长（推理/出图常 30s+）→ `timeout(300s)` 兜底防挂死。
    /// `Client::builder().build()` 实际只在 TLS 后端配置错误时失败（本客户端
    /// 用 rustls-tls，构建期不可能走到），故用 `expect` 保持 `new() -> Self`。
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("reqwest::Client 构建失败（TLS 后端缺失）");
        Self { base_url: base_url.into(), http }
    }

    /// 探测引擎能力（`GET /v1/health`）。
    ///
    /// 三态路由的 `local_ok` 输入：任一能力位为 true 即认为本地引擎活着，
    /// 具体能力位由编排器按能力名分别读取。
    pub async fn health(&self) -> anyhow::Result<EngineHealth> {
        let resp = self.http.get(format!("{}/v1/health", self.base_url))
            .send().await
            .map_err(|e| anyhow::anyhow!("health 请求失败（引擎未就绪?）: {}", e))?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("health 非 2xx: {}", e))?;
        resp.json().await.map_err(|e| anyhow::anyhow!("health 响应解析失败: {}", e))
    }

    /// 调 LLM 聊天补全（OpenAI 兼容 + `response_format.json_object`），返回解析后的 JSON。
    ///
    /// 契约：请求带 `response_format: {"type":"json_object"}` 强制引擎输出 JSON；
    /// 响应取 `choices[0].message.content` 字符串再二次解析为 `serde_json::Value`。
    /// 编排器（T5）拿这个 Value 读 `music_description` / `image_description` / `sentence`。
    pub async fn chat_json(&self, system: &str, user: &str) -> anyhow::Result<serde_json::Value> {
        let body = serde_json::json!({
            "model": ENGINE_CHAT_MODEL,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "response_format": {"type": "json_object"},
        });
        let resp = self.http.post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body).send().await
            .map_err(|e| anyhow::anyhow!("chat 请求失败（网络/超时）: {}", e))?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("chat 非 2xx: {}", e))?;
        let v: serde_json::Value = resp.json().await
            .map_err(|e| anyhow::anyhow!("chat 响应解析失败: {}", e))?;
        let content = v["choices"][0]["message"]["content"].as_str()
            .ok_or_else(|| anyhow::anyhow!("chat 响应缺 content"))?;
        serde_json::from_str(content).map_err(|e| anyhow::anyhow!("chat 返回非 JSON: {}", e))
    }

    /// 调引擎生成音频（`POST /v1/audio`，Step-Audio 自定义端点），返回 WAV 原始字节。
    pub async fn generate_audio(&self, text: &str) -> anyhow::Result<Vec<u8>> {
        let resp = self.http.post(format!("{}/v1/audio", self.base_url))
            .json(&serde_json::json!({"text": text})).send().await
            .map_err(|e| anyhow::anyhow!("audio 请求失败（网络/超时）: {}", e))?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("audio 非 2xx: {}", e))?;
        let bytes = resp.bytes().await.map_err(|e| anyhow::anyhow!("audio 响应读取失败: {}", e))?;
        Ok(bytes.to_vec())
    }

    /// 调引擎生成图像（`POST /v1/images/generations`，OpenAI 兼容），返回 PNG 原始字节。
    ///
    /// 契约：请求带 `response_format:"b64_json"`，响应取 `data[0].b64_json`
    /// 再 base64 解码为图片字节。
    pub async fn generate_image(&self, prompt: &str) -> anyhow::Result<Vec<u8>> {
        let body = serde_json::json!({
            "model": ENGINE_IMAGE_MODEL,
            "prompt": prompt,
            "size": "1024x1024",
            "response_format": "b64_json",
        });
        let resp = self.http.post(format!("{}/v1/images/generations", self.base_url))
            .json(&body).send().await
            .map_err(|e| anyhow::anyhow!("image 请求失败（网络/超时）: {}", e))?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("image 非 2xx: {}", e))?;
        let v: serde_json::Value = resp.json().await
            .map_err(|e| anyhow::anyhow!("image 响应解析失败: {}", e))?;
        let b64 = v["data"][0]["b64_json"].as_str()
            .ok_or_else(|| anyhow::anyhow!("image 响应缺 b64_json"))?;
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.decode(b64)
            .map_err(|e| anyhow::anyhow!("image b64 解码失败: {}", e))
    }
}

/// `JsonChat` for `EngineClient`：复用现有 `chat_json(system, user)` 方法。
///
/// 签名天然对齐（trait 要求 `&self, system, user`），直接 forward 即可。
/// 这样编排器（T5）能拿 `&dyn JsonChat`，本地引擎与云端 OpenAI 同接口。
#[async_trait::async_trait]
impl JsonChat for EngineClient {
    async fn chat_json(&self, system: &str, user: &str) -> anyhow::Result<serde_json::Value> {
        EngineClient::chat_json(self, system, user).await
    }
}

/// `AudioClient` for `EngineClient`：复用现有 `generate_audio(text)` 方法。
///
/// 签名天然对齐（trait 要求 `&self, text`），直接 forward 即可。
/// 这样音乐适配器（T8）能拿 `&dyn AudioClient`，本地引擎与云端 MiniMax 同接口。
#[async_trait::async_trait]
impl AudioClient for EngineClient {
    async fn generate_audio(&self, text: &str) -> anyhow::Result<Vec<u8>> {
        EngineClient::generate_audio(self, text).await
    }
}

/// `ImageClient` for `EngineClient`：复用现有 `generate_image(prompt)` 方法。
///
/// 签名天然对齐（trait 要求 `&self, prompt`），直接 forward 即可。
/// 这样艺术适配器（T9）能拿 `&dyn ImageClient`，本地引擎与云端 MiniMax 同接口。
#[async_trait::async_trait]
impl ImageClient for EngineClient {
    async fn generate_image(&self, prompt: &str) -> anyhow::Result<Vec<u8>> {
        EngineClient::generate_image(self, prompt).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[tokio::test]
    async fn health_reports_capabilities() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v1/health");
            then.status(200).json_body(serde_json::json!({"llm": true, "image": false, "audio": true}));
        });
        let c = EngineClient::new(server.base_url());
        let h = c.health().await.unwrap();
        assert!(h.llm && !h.image && h.audio);
    }

    #[tokio::test]
    async fn health_errors_on_http_500() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v1/health");
            then.status(500);
        });
        let c = EngineClient::new(server.base_url());
        assert!(c.health().await.is_err());
    }

    #[tokio::test]
    async fn chat_json_requests_json_object_and_parses() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions")
                .json_body_partial(r#"{"response_format":{"type":"json_object"}}"#);
            then.status(200).json_body(serde_json::json!({
                "choices": [{ "message": { "content": "{\"music_description\":\"m\",\"image_description\":\"i\",\"sentence\":\"s\"}" } }]
            }));
        });
        let c = EngineClient::new(server.base_url());
        let v = c.chat_json("system", "user prompt").await.unwrap();
        assert_eq!(v["music_description"], "m");
    }

    #[tokio::test]
    async fn chat_json_errors_when_content_not_json() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(200).json_body(serde_json::json!({
                "choices": [{ "message": { "content": "plain text not json" } }]
            }));
        });
        let c = EngineClient::new(server.base_url());
        let err = c.chat_json("system", "user prompt").await.unwrap_err();
        assert!(err.to_string().contains("非 JSON"));
    }

    #[tokio::test]
    async fn generate_audio_returns_bytes() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/audio");
            then.status(200).body("RIFFfakewav");
        });
        let c = EngineClient::new(server.base_url());
        let b = c.generate_audio("a calm melody").await.unwrap();
        assert_eq!(b, b"RIFFfakewav");
    }

    #[tokio::test]
    async fn generate_image_decodes_b64() {
        let server = MockServer::start();
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(b"\x89PNG");
        server.mock(|when, then| {
            when.method(POST).path("/v1/images/generations");
            then.status(200).json_body(serde_json::json!({"data": [{"b64_json": b64}]}));
        });
        let c = EngineClient::new(server.base_url());
        let b = c.generate_image("an orange abstract").await.unwrap();
        assert_eq!(b, b"\x89PNG");
    }

    #[tokio::test]
    async fn generate_image_errors_when_b64_missing() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/images/generations");
            then.status(200).json_body(serde_json::json!({"data": [{}]}));
        });
        let c = EngineClient::new(server.base_url());
        let err = c.generate_image("an orange abstract").await.unwrap_err();
        assert!(err.to_string().contains("缺 b64_json"));
    }
}
