//! FingertipConfig：模型生成的持久化配置数据模型。
//!
//! 存放位置：`app_data_dir/fingertip-config.json`（App 启动时 load_config，
//! Settings 保存时 save_config）。`CapabilityMode` 三态（本地优先/仅云端/仅本地）
//! 是后续「编排器 LLM → 专有模型」路由的核心。

use serde::{Deserialize, Serialize};
use std::path::Path;

/// 能力路由三态模式。
///
/// 这是后续整个「编排器 → 专有模型」路由的核心选择：
/// - `LocalFirst`：优先走本地引擎；本地不可用/失败时回退云端
/// - `CloudOnly`：只走云端（本地引擎未安装、或用户显式只信云端 API）
/// - `LocalOnly`：只走本地（隐私场景，绝不上传数据到云端）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityMode { LocalFirst, CloudOnly, LocalOnly }

/// 单次能力路由的决策结果。
///
/// 三态语义（与 `CapabilityMode` 一一对应但不相同）：
/// - `Local`：走本地引擎（本地可用，且模式允许本地）
/// - `Cloud`：走云端 API（本地不可用/模式禁本地时回退，或仅云端模式）
/// - `Unavailable(reason)`：按当前模式两端都不可用，`reason` 为面向用户的中文说明
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision { Local, Cloud, Unavailable(String) }

/// 纯函数：按「模式 + 本/云可用性」决定能力走哪条路。
///
/// 不读任何外部状态、不产生副作用，输入 `(mode, local_ok, cloud_ok)` 即输出决策，
/// 供后续客户端（T3/T4）与本地/云端适配器（T8/T9）消费。
/// `cap` 是能力名（"llm"/"image"/"audio"），仅用于拼装 Unavailable 的提示文案。
pub fn route_capability(mode: CapabilityMode, local_ok: bool, cloud_ok: bool, cap: &str) -> RouteDecision {
    match mode {
        CapabilityMode::LocalFirst => {
            if local_ok { RouteDecision::Local }
            else if cloud_ok { RouteDecision::Cloud }
            else { RouteDecision::Unavailable(format!("{} 不可用：本地引擎未就绪且云端未配置", cap)) }
        }
        CapabilityMode::CloudOnly => {
            if cloud_ok { RouteDecision::Cloud }
            else { RouteDecision::Unavailable(format!("{} 不可用：云端未配置（仅云端模式）", cap)) }
        }
        CapabilityMode::LocalOnly => {
            if local_ok { RouteDecision::Local }
            else { RouteDecision::Unavailable(format!("{} 不可用：本地引擎未就绪（仅本地模式）", cap)) }
        }
    }
}

/// MiniMax 云端 API 默认基址。
///
/// 默认配置里预填真实值（而非空串 + Settings 占位符）—— 防止「placeholder 陷阱」：
/// 表单灰色提示 `https://api.minimaxi.com` 看着像已填，实际保存的是空串，导致
/// `cloud_*_ok` 判定失败、仅云端模式直接路由不可用（v0.4.1 实测踩坑）。
pub const MINIMAX_API_BASE: &str = "https://api.minimaxi.com";
/// 编排器 LLM 默认模型（MiniMax-M3，实测 /v1/models 在列且 json_schema 可用，
/// 解析端已容错其推理噪音；旧 MiniMax-Text-01 已不在官方清单）。
pub const LLM_CLOUD_MODEL_DEFAULT: &str = "MiniMax-M3";
/// 图像默认模型（image-01，实测 /v1/image_generation 有效）。
pub const IMAGE_CLOUD_MODEL_DEFAULT: &str = "image-01";
/// 音乐默认模型（music-3.0，实测 /v1/music_generation 有效；
/// Music-3.0-free 实测不存在，报 2013 invalid model）。
pub const AUDIO_CLOUD_MODEL_DEFAULT: &str = "music-3.0";

/// 本地可选插件引擎（Python 服务）连接配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EngineConfig { pub enabled: bool, pub base_url: String }
impl Default for EngineConfig {
    fn default() -> Self { Self { enabled: false, base_url: "http://127.0.0.1:8765".into() } }
}

/// LLM 编排器配置。
///
/// `local_gguf` 是**多路径列表**：编排器可加载多个 GGUF 备用（如通用 + 音乐专用），
/// 与 `ImageConfig.local_model_path`（单路径，SD1.5 只有一种本地产物）不对称是有意为之。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LlmConfig {
    pub mode: CapabilityMode,
    pub local_gguf: Vec<String>,      // 多 GGUF 路径可选
    pub cloud_base: String,
    pub cloud_key: String,
    pub cloud_model: String,
}
impl Default for LlmConfig {
    fn default() -> Self { Self { mode: CapabilityMode::LocalFirst, local_gguf: vec![], cloud_base: MINIMAX_API_BASE.into(), cloud_key: String::new(), cloud_model: LLM_CLOUD_MODEL_DEFAULT.into() } }
}

/// 图像生成配置。
///
/// `local_model_path` 是**单路径**：本地产物固定为 SD1.5 的 GGUF 一个模型，
/// 与 `LlmConfig.local_gguf`（多路径列表）不对称是有意为之。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ImageConfig {
    pub mode: CapabilityMode,
    pub local_model_path: String,     // 默认 SD1.5 GGUF 路径
    pub cloud_base: String,
    pub cloud_key: String,
    pub cloud_model: String,
}
impl Default for ImageConfig {
    fn default() -> Self { Self { mode: CapabilityMode::LocalFirst, local_model_path: String::new(), cloud_base: MINIMAX_API_BASE.into(), cloud_key: String::new(), cloud_model: IMAGE_CLOUD_MODEL_DEFAULT.into() } }
}

/// 音频/TTS 生成配置。
///
/// 默认 `LocalFirst`：音频确实有本地后端（Step-Audio 经本地引擎），
/// 只是本地路径/开关字段不在此处（后续 registry/engine 模块再落），故默认按本地优先路由。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioConfig {
    pub mode: CapabilityMode,
    pub minimax_base: String,
    pub minimax_key: String,
    pub minimax_model: String,
}
impl Default for AudioConfig {
    fn default() -> Self { Self { mode: CapabilityMode::LocalFirst, minimax_base: MINIMAX_API_BASE.into(), minimax_key: String::new(), minimax_model: AUDIO_CLOUD_MODEL_DEFAULT.into() } }
}

/// 模型生成总配置（引擎 / LLM / 图像 / 音频）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FingertipConfig {
    pub engine: EngineConfig,
    pub llm: LlmConfig,
    pub image: ImageConfig,
    pub audio: AudioConfig,
}
impl Default for FingertipConfig {
    fn default() -> Self { Self { engine: EngineConfig::default(), llm: LlmConfig::default(), image: ImageConfig::default(), audio: AudioConfig::default() } }
}

/// 保存配置到磁盘。
///
/// 行为：
///   - 自动 `create_dir_all` 创建父目录（对齐 `db::init_at` 先例）
///   - **原子写**：先写 `{name}.json.tmp` 临时文件并 `sync_all` 落盘，
///     再 `std::fs::rename` 原子替换，避免中途崩溃把配置写坏
///   - 任何一步失败向调用方抛 `anyhow::Error`（不静默吞错）
pub fn save_config(path: &Path, cfg: &FingertipConfig) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = path.with_extension("json.tmp");
    let mut f = std::fs::File::create(&tmp)?;
    serde_json::to_writer_pretty(&mut f, cfg)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 从磁盘加载配置；失败时静默回退默认值。
///
/// **回退语义（API 边界契约）**：
///   - 文件不存在（`NotFound`）→ 静默返默认（首次启动的正常路径，不打扰）
///   - JSON 解析失败 / 其它 IO 错误 → `log::warn!` 后返默认（「失败要大声」，
///     但函数签名不返回 Result，调用方无法处理时仍可拿到可用配置）
pub fn load_config(path: &Path) -> FingertipConfig {
    match std::fs::read_to_string(path) {
        Ok(s) => match serde_json::from_str(&s) {
            Ok(cfg) => cfg,
            Err(e) => {
                log::warn!("fingertip-config.json JSON 解析失败，回退默认配置: {e}");
                FingertipConfig::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => FingertipConfig::default(),
        Err(e) => {
            log::warn!("fingertip-config.json 读取失败，回退默认配置: {e}");
            FingertipConfig::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_engine_base_url_and_local_first_modes() {
        let c = FingertipConfig::default();
        assert_eq!(c.engine.base_url, "http://127.0.0.1:8765");
        assert_eq!(c.llm.mode, CapabilityMode::LocalFirst);
        assert_eq!(c.image.mode, CapabilityMode::LocalFirst);
        assert_eq!(c.audio.mode, CapabilityMode::LocalFirst);
        // v0.4.1: 云端 base/model 默认预填（防 placeholder 陷阱），key 不预填（保密）
        assert_eq!(c.llm.cloud_base, MINIMAX_API_BASE);
        assert_eq!(c.llm.cloud_model, LLM_CLOUD_MODEL_DEFAULT);
        assert_eq!(c.image.cloud_model, IMAGE_CLOUD_MODEL_DEFAULT);
        assert_eq!(c.audio.minimax_base, MINIMAX_API_BASE);
        assert_eq!(c.audio.minimax_model, AUDIO_CLOUD_MODEL_DEFAULT);
        assert!(c.llm.cloud_key.is_empty());
    }

    #[test]
    fn config_round_trip_save_load() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("fingertip-config.json");
        let c = FingertipConfig { engine: EngineConfig { enabled: true, base_url: "http://127.0.0.1:9000".into() }, ..Default::default() };
        save_config(&path, &c).unwrap();
        let loaded = load_config(&path);
        assert_eq!(loaded, c);
    }

    #[test]
    fn load_config_returns_default_on_corrupt_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{not json").unwrap();
        let c = load_config(&path);
        assert_eq!(c.engine.base_url, "http://127.0.0.1:8765");
    }

    #[test]
    fn load_config_returns_default_when_file_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("missing.json");
        let c = load_config(&path);
        assert_eq!(c, FingertipConfig::default());
    }

    #[test]
    fn load_config_partial_json_forward_compatible() {
        // 只写部分字段，其余字段必须经 #[serde(default)] 补默认 —— 前向兼容契约
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("partial.json");
        std::fs::write(&path, r#"{"engine":{"base_url":"x"}}"#).unwrap();
        let c = load_config(&path);
        assert_eq!(c.engine.base_url, "x");
        assert_eq!(c.engine.enabled, false);
        assert_eq!(c.llm, LlmConfig::default());
        assert_eq!(c.image, ImageConfig::default());
        assert_eq!(c.audio, AudioConfig::default());
    }

    #[test]
    fn route_local_first_uses_local_when_available() {
        assert_eq!(route_capability(CapabilityMode::LocalFirst, true, false, "llm"), RouteDecision::Local);
    }

    #[test]
    fn route_local_first_falls_back_to_cloud_when_local_down() {
        assert_eq!(route_capability(CapabilityMode::LocalFirst, false, true, "image"), RouteDecision::Cloud);
    }

    #[test]
    fn route_local_first_unavailable_when_both_down() {
        let d = route_capability(CapabilityMode::LocalFirst, false, false, "audio");
        assert!(matches!(d, RouteDecision::Unavailable(reason) if reason.contains("audio")));
    }

    #[test]
    fn route_cloud_only_never_uses_local() {
        // 注意：此处用 matches! 而非 assert_eq! —— Unavailable 携带 reason 字符串，
        // 断言意图是「结果为 Unavailable（不关心文案）」，`_` 是模式不能在表达式右侧
        assert!(matches!(route_capability(CapabilityMode::CloudOnly, true, false, "llm"), RouteDecision::Unavailable(_)));
        assert_eq!(route_capability(CapabilityMode::CloudOnly, true, true, "llm"), RouteDecision::Cloud);
    }

    #[test]
    fn route_local_only_never_uses_cloud() {
        assert!(matches!(route_capability(CapabilityMode::LocalOnly, false, true, "image"), RouteDecision::Unavailable(_)));
    }

    #[test]
    fn route_local_only_uses_local_when_available() {
        // 本地可用即走本地，即使云端可用也不回退（隐私契约）
        assert_eq!(route_capability(CapabilityMode::LocalOnly, true, true, "llm"), RouteDecision::Local);
        assert_eq!(route_capability(CapabilityMode::LocalOnly, true, false, "llm"), RouteDecision::Local);
    }

    #[test]
    fn route_local_first_prefers_local_when_both_available() {
        // 双端可用 → 仍走本地：LocalFirst 的「优先」语义
        assert_eq!(route_capability(CapabilityMode::LocalFirst, true, true, "llm"), RouteDecision::Local);
    }
}
