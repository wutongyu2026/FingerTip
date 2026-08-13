//! model 层：LLM/模型生成架构的配置数据模型与生成客户端。
//!
//! v0.4「编排器 LLM → 专有模型」改造的 App 侧数据层。
//! 本模块只建配置数据模型与客户端（本地引擎 / 云端 OpenAI / MiniMax），不接 UI/命令。

pub mod cloud;
pub mod config;
pub mod engine;
pub mod orchestrator;

/// JSON chat 抽象：编排器依赖它，本地引擎与云端 OpenAI 都实现。
///
/// v0.4 三态路由的 LLM 兑底层：T5 编排器拿 `&dyn JsonChat` 调用，
/// 具体走本地引擎还是云端 OpenAI 由路由决策决定。
#[async_trait::async_trait]
pub trait JsonChat: Send + Sync {
    /// 调 LLM 聊天补全并返回解析后的 JSON（契约：模型输出 `json_object`）。
    ///
    /// `system` 为系统提示（角色设定/输出格式约束），`user` 为本次请求内容。
    /// 返回的 `Value` 由编排器按能力名读取 `music_description` / `image_description` / `sentence`。
    async fn chat_json(&self, system: &str, user: &str) -> anyhow::Result<serde_json::Value>;
}

/// 音频生成抽象：本地引擎与 MiniMax 都实现，供音乐适配器路由。
///
/// v0.4 三态路由的音频兑底层：T8 音乐适配器拿 `&dyn AudioClient` 调用。
#[async_trait::async_trait]
pub trait AudioClient: Send + Sync {
    /// 按文本描述生成一段音频，返回音频原始字节（本地引擎为 WAV，MiniMax 为 MP3）。
    async fn generate_audio(&self, text: &str) -> anyhow::Result<Vec<u8>>;
}

/// 图像生成抽象：本地引擎（`/v1/images/generations`）与云端 MiniMax 都实现，供艺术适配器路由。
///
/// v0.4 三态路由的图像兑底层：T9 艺术适配器拿 `&dyn ImageClient` 调用。
/// 与 `AudioClient` 的契约对齐 —— 输入 prompt、输出 PNG 字节流（不做语义解释，
/// 字节解析/重编码由调用方按需处理）。
#[async_trait::async_trait]
pub trait ImageClient: Send + Sync {
    /// 按文本描述生成一张图片，返回 PNG 原始字节。
    async fn generate_image(&self, prompt: &str) -> anyhow::Result<Vec<u8>>;
}
