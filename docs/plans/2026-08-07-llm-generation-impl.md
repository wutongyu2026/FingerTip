# LLM/模型生成架构 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 去掉本地确定性音乐/图像生成算法，改为「编排器 LLM → 专有模型」链路：App 通过能力路由（本地 FingerTip-Engine 优先 → 云端兑底）完成音乐/图像/句子生成，模型收敛到可选插件引擎。

**Architecture:** App 侧新增 `model/` 层（配置、三态路由、引擎 HTTP 客户端、云端客户端、编排器）；`Music`/`Art` 结构改为携带描述+文件分析结果；`generate_now` 先调编排器得到三条描述，再分别调音乐/图像适配器；旧 local 算法与手写 wav/png 编码器删除。独立 Python `engine/` 服务（可选插件）提供 /v1/health、/v1/chat、/v1/images、/v1/audio。

**Tech Stack:** Rust（reqwest、httpmock 测试、现有 adapter trait）；Python（engine/：FastAPI + uvicorn + pytest，可选 llama-cpp-python / sd-cpp / Step-Audio）；MiniMax 音乐 API、OpenAI 兼容文生图 API（云端兑底）。

**参考设计:** `docs/plans/2026-08-07-llm-generation-design.md`（决策 D1-D11 在此，实现以它为准）。

---
**执行环境注意：** 本仓库惯例在 `dev` 分支直接实施 + 每任务子 agent 提交（参考记忆 `r2-minor-followups.md`：新 Tauri Command 必须同时改 `commands.rs` + `lib.rs` 两处；`Cargo.lock` 不 commit）。

---

### Task 1: 模型配置模块（FingertipConfig + 读写）

**Files:**
- Create: `src-tauri/src/model/mod.rs`
- Create: `src-tauri/src/model/config.rs`
- Modify: `src-tauri/src/lib.rs`（注册 `mod model;`）
- Test: `src-tauri/src/model/config.rs`（内嵌 `#[cfg(test)]`）

**Step 1: 写失败测试**（config.rs 内嵌）

```rust
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
    }

    #[test]
    fn config_round_trip_save_load() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("fingertip-config.json");
        let c = FingertipConfig { engine: EngineConfig { enabled: true, base_url: "http://127.0.0.1:9000".into() }, ..Default::default() };
        save_config(&path, &c).unwrap();
        let loaded = load_config(&path);
        assert_eq!(loaded.engine.base_url, "http://127.0.0.1:9000");
    }

    #[test]
    fn load_config_returns_default_on_corrupt_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{not json").unwrap();
        let c = load_config(&path);
        assert_eq!(c.engine.base_url, "http://127.0.0.1:8765");
    }
}
```

**Step 2: 跑测试确认失败** — `cargo test -p fingertip`（在 `src-tauri/` 下）→ 编译失败（FingertipConfig 不存在）

**Step 3: 最小实现**

```rust
// src-tauri/src/model/config.rs
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityMode { LocalFirst, CloudOnly, LocalOnly }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EngineConfig { pub enabled: bool, pub base_url: String }
impl Default for EngineConfig {
    fn default() -> Self { Self { enabled: false, base_url: "http://127.0.0.1:8765".into() } }
}

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
    fn default() -> Self { Self { mode: CapabilityMode::LocalFirst, local_gguf: vec![], cloud_base: String::new(), cloud_key: String::new(), cloud_model: String::new() } }
}

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
    fn default() -> Self { Self { mode: CapabilityMode::LocalFirst, local_model_path: String::new(), cloud_base: String::new(), cloud_key: String::new(), cloud_model: String::new() } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioConfig {
    pub mode: CapabilityMode,
    pub minimax_base: String,
    pub minimax_key: String,
    pub minimax_model: String,
}
impl Default for AudioConfig {
    fn default() -> Self { Self { mode: CapabilityMode::LocalFirst, minimax_base: String::new(), minimax_key: String::new(), minimax_model: String::new() } }
}

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

pub fn save_config(path: &Path, cfg: &FingertipConfig) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(cfg)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load_config(path: &Path) -> FingertipConfig {
    std::fs::read_to_string(path).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
```

**Step 4: 跑测试确认通过** — `cargo test -p fingertip` → 全过

**Step 5: 提交** — `git add src-tauri/src/model/ src-tauri/src/lib.rs && git commit -m "feat(model): FingertipConfig 配置模块（引擎/LLM/图像/音频）"`

---

### Task 2: 三态路由纯函数

**Files:**
- Modify: `src-tauri/src/model/config.rs`（追加 `route_capability`）
- Test: 同上内嵌

**Step 1: 写失败测试**

```rust
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
    assert_eq!(route_capability(CapabilityMode::CloudOnly, true, false, "llm"), RouteDecision::Unavailable(_));
    assert_eq!(route_capability(CapabilityMode::CloudOnly, true, true, "llm"), RouteDecision::Cloud);
}
#[test]
fn route_local_only_never_uses_cloud() {
    assert_eq!(route_capability(CapabilityMode::LocalOnly, false, true, "image"), RouteDecision::Unavailable(_));
}
```

**Step 2: 跑测试确认失败** — `cargo test -p fingertip` → 编译失败（RouteDecision 不存在）

**Step 3: 最小实现**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision { Local, Cloud, Unavailable(String) }

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
```

**Step 4/5: 测试通过 + 提交** — `cargo test -p fingertip` → `git commit -m "feat(model): 三态路由决策纯函数"`

---

### Task 3: 引擎 HTTP 客户端（EngineClient）

**Files:**
- Create: `src-tauri/src/model/engine.rs`
- Modify: `src-tauri/src/model/mod.rs`（`pub mod engine;`）
- Test: `src-tauri/src/model/engine.rs` 内嵌（用 `httpmock`，加到 dev-dependencies）

**Step 1: Cargo.toml 加 httpmock**（dev-dependencies）：

```toml
httpmock = "0.7"
```

**Step 2: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[tokio::test]
    async fn health_reports_capabilities() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v1/health");
            then.status(200).json_body(r#"{"llm":true,"image":false,"audio":true}"#);
        });
        let c = EngineClient::new(server.base_url());
        let h = c.health().await.unwrap();
        assert!(h.llm && !h.image && h.audio);
    }

    #[tokio::test]
    async fn chat_json_requests_json_object_and_parses() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions")
                .json_body_partial(r#"{"response_format":{"type":"json_object"}}"#);
            then.status(200).json_body(r#"{"choices":[{"message":{"content":"{\"music_description\":\"m\",\"image_description\":\"i\",\"sentence\":\"s\"}"}}]}"#);
        });
        let c = EngineClient::new(server.base_url());
        let v = c.chat_json("system", "user prompt").await.unwrap();
        assert_eq!(v["music_description"], "m");
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
        let b64 = base64_encode_for_test(b"\x89PNG");
        server.mock(|when, then| {
            when.method(POST).path("/v1/images/generations");
            then.status(200).json_body(format!(r#"{{"data":[{{"b64_json":"{}"}}]}}"#, b64));
        });
        let c = EngineClient::new(server.base_url());
        let b = c.generate_image("an orange abstract").await.unwrap();
        assert_eq!(b, b"\x89PNG");
    }
}
```

**Step 3: 跑测试确认失败** — `cargo test -p fingertip engine` → 编译失败

**Step 4: 最小实现**

```rust
// src-tauri/src/model/engine.rs
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EngineHealth { pub llm: bool, pub image: bool, pub audio: bool }

pub struct EngineClient { base_url: String, http: reqwest::Client }

impl EngineClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), http: reqwest::Client::new() }
    }
    pub async fn health(&self) -> anyhow::Result<EngineHealth> {
        let resp = self.http.get(format!("{}/v1/health", self.base_url)).send().await?;
        Ok(resp.json().await?)
    }
    pub async fn chat_json(&self, system: &str, user: &str) -> anyhow::Result<serde_json::Value> {
        let body = serde_json::json!({
            "model": "fingertip-llm",
            "messages": [{"role":"system","content":system},{"role":"user","content":user}],
            "response_format": {"type": "json_object"},
        });
        let resp = self.http.post(format!("{}/v1/chat/completions", self.base_url)).json(&body).send().await?
            .error_for_status()?;
        let v: serde_json::Value = resp.json().await?;
        let content = v["choices"][0]["message"]["content"].as_str()
            .ok_or_else(|| anyhow::anyhow!("chat 响应缺 content"))?;
        serde_json::from_str(content).map_err(|e| anyhow::anyhow!("chat 返回非 JSON: {}", e))
    }
    pub async fn generate_audio(&self, text: &str) -> anyhow::Result<Vec<u8>> {
        let resp = self.http.post(format!("{}/v1/audio", self.base_url))
            .json(&serde_json::json!({"text": text})).send().await?.error_for_status()?;
        Ok(resp.bytes().await?.to_vec())
    }
    pub async fn generate_image(&self, prompt: &str) -> anyhow::Result<Vec<u8>> {
        let body = serde_json::json!({"model":"fingertip-image","prompt":prompt,"size":"1024x1024","response_format":"b64_json"});
        let resp = self.http.post(format!("{}/v1/images/generations", self.base_url)).json(&body).send().await?.error_for_status()?;
        let v: serde_json::Value = resp.json().await?;
        let b64 = v["data"][0]["b64_json"].as_str().ok_or_else(|| anyhow::anyhow!("image 响应缺 b64_json"))?;
        use base64_alphabet();
        Ok(base64_decode(b64))
    }
}
```

> base64 编解码：复用 `generate/upload.rs` 里已有的 `base64_encode`/`base64_decode`（pub(crate) 提升或用 `base64` crate 0.22）。倾向加 `base64 = "0.22"` 依赖，删 upload.rs 手写实现（DRY）。

**Step 5: 提交** — `git commit -m "feat(model): EngineClient（health/chat/images/audio，OpenAI 兼容）"`

---

### Task 4: 云端客户端（OpenAI chat/images + MiniMax audio）

**Files:**
- Create: `src-tauri/src/model/cloud.rs`
- Modify: `src-tauri/src/model/mod.rs`（`pub mod cloud;`）
- Test: 内嵌（httpmock）

**Step 1: 写失败测试**（要点：OpenAI chat 走 `/chat/completions` 带 key 头；images 走 `/images/generations`；MiniMax 严格按 `minimax-music-gen` skill 的 API 形态——实现时先读该 skill）

```rust
#[tokio::test]
async fn openai_chat_uses_bearer_and_parses_json() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions").header("Authorization", "Bearer sk-test");
        then.status(200).json_body(r#"{"choices":[{"message":{"content":"{\"music_description\":\"m\",\"image_description\":\"i\",\"sentence\":\"s\"}"}}]}"#);
    });
    let c = OpenAiClient::new(server.base_url(), "sk-test", "gpt-x");
    let v = c.chat_json("sys", "user").await.unwrap();
    assert_eq!(v["sentence"], "s");
}

#[tokio::test]
async fn openai_image_returns_png_bytes() {
    let server = MockServer::start();
    let b64 = base64_encode_test(b"\x89PNG");
    server.mock(|when, then| {
        when.method(POST).path("/v1/images/generations");
        then.status(200).json_body(format!(r#"{{"data":[{{"b64_json":"{}"}}]}}"#, b64));
    });
    let c = OpenAiClient::new(server.base_url(), "sk-test", "gpt-image");
    let b = c.generate_image("prompt").await.unwrap();
    assert_eq!(b, b"\x89PNG");
}

#[tokio::test]
async fn minimax_audio_returns_wav_bytes() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/music/generate");
        then.status(200).body("RIFF....WAVE");
    });
    let c = MiniMaxMusicClient::new(server.base_url(), "mm-key", "music-model");
    let b = c.generate_audio("calm piano").await.unwrap();
    assert_eq!(&b[0..4], b"RIFF");
}
```

**Step 2-4: 失败 → 实现 → 通过**

OpenAiClient 与 MiniMaxMusicClient 都实现共同 trait `JsonChat`（chat 走 OpenAI 兼容；MiniMax 不参与编排，只实现 `generate_audio`）。三者都实现 `AudioClient` trait（`async fn generate_audio(&self, text: &str) -> Result<Vec<u8>>`）——EngineClient 与 MiniMaxMusicClient 都实现，供音乐适配器路由。

```rust
// model/mod.rs
#[async_trait::async_trait]
pub trait JsonChat: Send + Sync {
    async fn chat_json(&self, system: &str, user: &str) -> anyhow::Result<serde_json::Value>;
}
#[async_trait::async_trait]
pub trait AudioClient: Send + Sync {
    async fn generate_audio(&self, text: &str) -> anyhow::Result<Vec<u8>>;
}
```

**Step 5: 提交** — `git commit -m "feat(model): 云端客户端（OpenAI chat/images + MiniMax audio）"`

---

### Task 5: 编排器（context 组装 + prompt + JSON 解析/重试）

**Files:**
- Create: `src-tauri/src/model/orchestrator.rs`
- Modify: `src-tauri/src/model/mod.rs`
- Test: 内嵌

**Step 1: 写失败测试**

```rust
#[test]
fn orchestrator_prompt_contains_daily_signals() {
    let ctx = OrchestrationContext::sample();
    let p = orchestrator_prompt(&ctx);
    assert!(p.contains(&ctx.theme_word));
    assert!(p.contains("intensity"));
}

#[test]
fn parse_orchestrator_json_valid() {
    let j = r#"{"music_description":"calm piano","image_description":"orange abstract","sentence":"A quiet day of focus"}"#;
    let r = parse_orchestrator_json(j).unwrap();
    assert_eq!(r.sentence, "A quiet day of focus");
}

#[test]
fn parse_orchestrator_json_missing_fields_errors() {
    let j = r#"{"music_description":"x"}"#;
    assert!(parse_orchestrator_json(j).is_err());
}

#[tokio::test]
async fn run_orchestrator_retries_once_on_invalid_json() {
    // mock JsonChat：第一次返回非 JSON，第二次返回合法 JSON
    let mut calls = 0;
    let chat = MockChat(Box::new(move || {
        calls += 1;
        if calls == 1 { serde_json::json!({"bad": true}) } else { serde_json::json!({"music_description":"m","image_description":"i","sentence":"s"}) }
    }));
    let r = run_orchestrator(&chat, &OrchestrationContext::sample()).await.unwrap();
    assert_eq!(r.sentence, "s");
    assert_eq!(calls, 2);
}
```

**Step 2-4: 实现**（要点：prompt 把 theme_word/mood/四指标/Top5/hourly 摘要/首活拼进去；音乐描述按「风格/情绪/主题词」三维，兼容 Step-Audio 与 MiniMax；解析失败重试 1 次，再失败返可读错误）：

```rust
pub struct OrchestrationContext {
    pub theme_word: String, pub mood: Option<String>, pub style: String,
    pub intensity: f64, pub steadiness: f64, pub fluency: f64, pub activity_hours: i32,
    pub top_keys: Vec<(u32, usize)>, pub hourly: [usize; 24], pub first_active_ms: i64,
}
pub struct OrchestratorResult { pub music_description: String, pub image_description: String, pub sentence: String }

pub fn parse_orchestrator_json(text: &str) -> anyhow::Result<OrchestratorResult> { /* 校验三字段非空 */ }
pub async fn run_orchestrator(chat: &dyn JsonChat, ctx: &OrchestrationContext) -> anyhow::Result<OrchestratorResult> {
    let p = orchestrator_prompt(ctx);
    for attempt in 0..2 {
        let v = chat.chat_json(SYSTEM, &p).await?;
        if let Ok(r) = parse_orchestrator_json(v) { return Ok(r); }
        if attempt == 1 { anyhow::bail!("编排器输出非法 JSON（重试后仍失败）"); }
    }
    unreachable!()
}
```

**Step 5: 提交**

---

### Task 6: Music/Art 结构变更 + artifacts sentence 列

**Files:**
- Modify: `src-tauri/src/generate/mod.rs`（Music/Art 结构）
- Modify: `src-tauri/src/db/migrations.rs`（artifacts 加 sentence 列）
- Modify: `src-tauri/src/db/artifact_repo.rs`（upsert_with_sentence / 读回 sentence）
- Modify: `src-tauri/src/commands.rs`（get_artifact 返回 sentence）
- Test: artifact_repo / commands 内嵌

**Step 1: 写失败测试**（要点：artifacts 表有 sentence 列；upsert 后 get_artifact 返回 sentence）

```rust
#[test]
fn artifacts_table_has_sentence_column() {
    let conn = init_in_memory().unwrap();
    let has = conn.prepare("SELECT 1 FROM pragma_table_info('artifacts') WHERE name='sentence'")
        .unwrap().exists([]).unwrap();
    assert!(has);
}
```

**Step 2-4: 实现**

```rust
// Music 结构（删 notes，加 description/model）
pub struct Music {
    pub bpm: u32,               // 0：不再估计
    pub duration_ms: u64,
    pub amplitudes: Vec<f32>,   // WAV 分析 [0,1]
    pub mood: Option<String>,
    pub style: String,
    pub theme_word: String,
    pub description: String,
    pub model: String,
}
// Art 结构（删 pixels/width/height，加 description/model）
pub struct Art {
    pub theme_word: String,
    pub mood: Option<String>,
    pub description: String,
    pub model: String,
}
```

migrations：`ALTER TABLE artifacts ADD COLUMN sentence TEXT`（带 pragma_table_info 存在性检查，沿用现有模式）。artifact_repo：`upsert` 改 `upsert_with_sentence(conn, date, &music, &art, Option<&str> sentence, wav, png)`。commands `get_artifact_impl` 返回 `"sentence": row.sentence`。

> 注意：Music 删 notes 会破坏现有测试（`generate_now_e2e_...`、`get_artifact_round_trip...` 等断言 notes/pixels）。本 Task 同步更新这些测试为新结构（notes→空、pixels→文件）。

**Step 5: 提交**

---

### Task 7: WAV 振幅分析（wav_analysis）

**Files:**
- Create: `src-tauri/src/db/wav_analysis.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Test: 内嵌

**Step 1: 写失败测试**（构造已知正弦 WAV → 断言 64 桶包络 + 时长）：

```rust
#[test]
fn analyze_wav_parses_pcm_and_buckets() {
    // 构造 1 秒 44.1kHz 正弦 WAV（复用/手写 44 字节头 + samples）
    let wav = make_test_wav(1_000, 44100);   // 1s, 幅度 0.5
    let a = analyze_wav(&wav).unwrap();
    assert_eq!(a.duration_ms, 1000);
    assert_eq!(a.amplitudes.len(), 64);
    assert!(a.amplitudes.iter().all(|&v| (0.0..=1.0).contains(&v)));
    assert!(a.amplitudes.iter().any(|&v| v > 0.1), "正弦应有能量");
}
```

**Step 2-4: 实现**（解析 RIFF/WAVE fmt=PCM16 + data；每桶取 RMS 归一化到 [0,1]；仅支持 16-bit PCM mono/stereo，其它格式报可读错误）：

```rust
pub struct WavAnalysis { pub duration_ms: u64, pub amplitudes: Vec<f32> } // 64 桶
pub fn analyze_wav(bytes: &[u8]) -> anyhow::Result<WavAnalysis> { /* ... */ }
```

**Step 5: 提交**

---

### Task 8: 模型音乐适配器（ModelMusicAdapter）

**Files:**
- Create: `src-tauri/src/generate/model_music.rs`
- Modify: `src-tauri/src/generate/mod.rs`（factory 返回它；MusicPrompt 加 `description: Option<String>`）
- Test: 内嵌（mock AudioClient）

**Step 1: 写失败测试**（Mock AudioClient 返回已知 WAV → 断言 Music 有 amplitudes/duration/description/model）：

```rust
#[tokio::test]
async fn model_music_uses_prompt_description_and_analyzes_wav() {
    let wav = make_sine_wav_for_test(500, 44100);
    let audio = MockAudio(wav.clone());
    let prompt = MusicPrompt { description: Some("calm piano".into()), ..sample_prompt() };
    let m = ModelMusicAdapter::new(audio).generate(&prompt).await.unwrap();
    assert_eq!(m.description, "calm piano");
    assert_eq!(m.duration_ms, 500);
    assert_eq!(m.bpm, 0);
    assert_eq!(m.amplitudes.len(), 64);
}
```

**Step 2-4: 实现**（`generate`：description 缺失报错；调 AudioClient → bytes → `wav_analysis::analyze_wav` → Music；model 字段 = "step-audio" 或 "minimax" 由路由决定传入）：

```rust
pub struct ModelMusicAdapter { audio: Box<dyn AudioClient>, model: String }
pub fn build_model_music_adapter(audio: Box<dyn AudioClient>, model: &str) -> ModelMusicAdapter { ... }
```

**Step 5: 提交**

---

### Task 9: 模型图像适配器（ModelArtAdapter）

**Files:**
- Create: `src-tauri/src/generate/model_art.rs`
- Modify: `src-tauri/src/generate/mod.rs`（ArtPrompt 加 description）
- Test: 内嵌（Mock ImageClient）

**Step 1: 写失败测试**（Mock 返回 PNG 字节 → 断言 Art.description/model 透传）：

```rust
#[tokio::test]
async fn model_art_uses_description_and_passes_bytes() {
    let img = MockImage(b"\x89PNG...".to_vec());
    let prompt = ArtPrompt { description: Some("orange abstract".into()), ..sample_prompt() };
    let a = ModelArtAdapter::new(img).generate(&prompt).await.unwrap();
    assert_eq!(a.description, "orange abstract");
    assert_eq!(a.model, "sd-cpp");
}
```

**Step 2-4: 实现**：图像不再产生 pixels；`generate` 返回 Art 元数据（description/model/theme_word/mood），**PNG 字节由 generate_now 层拿**（适配器可通过返回值带出 bytes，或 generate_now 直接调 ImageClient——倾向：适配器内部持 ImageClient 并暴露 `last_png: Arc<Mutex<Option<Vec<u8>>>>` 或让 generate 返回 `(Art, Vec<u8>)`。**选后者**：`generate` 返回 `(Art, Vec<u8>)` 更显式，但破坏 trait——故 ArtAdapter trait 改为 `async fn generate(&self, prompt) -> Result<ArtOutcome>`，`ArtOutcome { art: Art, png: Vec<u8> }`）。

**Step 5: 提交**

---

### Task 10: FingerTip-Engine（Python 服务，可选插件）

**Files:**
- Create: `engine/app.py`（FastAPI：/v1/health、/v1/chat/completions、/v1/images/generations、/v1/audio）
- Create: `engine/requirements.txt`
- Create: `engine/README.md`（最小引擎包说明 + 模型放哪）
- Create: `engine/tests/test_app.py`（pytest + httpx TestClient）
- Create: `engine/mock_backends.py`（mock LLM/图像/音频，默认启用；真实推理可选 import）

**Step 1: 写失败测试**（pytest，先跑假 app 断言 404，再实现）

```python
def test_health_reports_capabilities():
    from app import app
    from fastapi.testclient import TestClient
    c = TestClient(app)
    r = c.get("/v1/health")
    assert r.status_code == 200
    body = r.json()
    assert set(("llm", "image", "audio")) <= set(body)
```

**Step 2-4: 实现**（mock 模式为默认：/v1/chat 返回固定 JSON、/v1/images 返回 1x1 PNG、/v1/audio 返回合法静音 WAV。真实推理通过 `FINGERTIP_ENGINE_BACKEND=real` + 模型路径 env 启用——`llama-cpp-python` 加载 GGUF、sd-cpp/`sd-server` 子进程、Step-Audio import；任一后端缺失则该能力 health=false，App 自动落云端）：

```python
# engine/app.py 骨架
from fastapi import FastAPI
from pydantic import BaseModel
app = FastAPI()

class ChatRequest(BaseModel):
    model: str; messages: list[dict]; response_format: dict | None = None

@app.get("/v1/health")
def health():
    return {"llm": backends.llm_ok(), "image": backends.image_ok(), "audio": backends.audio_ok()}

@app.post("/v1/chat/completions")
def chat(req: ChatRequest):
    content = backends.chat_json(req.messages)   # mock 或 llama
    return {"choices": [{"message": {"content": content}}]}

@app.post("/v1/images/generations")
def image(req: dict):
    return {"data": [{"b64_json": backends.image(req["prompt"])}]}

@app.post("/v1/audio")
def audio(req: dict):
    return Response(content=backends.audio(req["text"]), media_type="audio/wav")
```

**Step 3: 跑测试** — `cd engine && pip install -r requirements.txt && pytest`

**Step 4: 提交** — `git add engine/ && git commit -m "feat(engine): FingerTip-Engine 最小推理服务（mock 默认，真实后端可选）"`

---

### Task 11: generate_now 接线 + 删除旧代码

**Files:**
- Modify: `src-tauri/src/commands.rs`（generate_now 新流程：编排器 → 音乐/图像适配器 → 写库含 sentence）
- Modify: `src-tauri/src/db/artifact_writer.rs`（改为写 bytes：`write_artifact_bytes(dir, date, wav, png)`）
- Delete: `src-tauri/src/generate/local/`、`src-tauri/src/generate/cloud/`、`src-tauri/src/db/wav_encoder.rs`、`src-tauri/src/db/png_encoder.rs`、`src-tauri/src/generate/sentence.rs`
- Modify: `src-tauri/src/generate/mod.rs`（`build_music_adapter`/`build_art_adapter` 返回 model 版；删 `is_cloud_enabled`/`FINGERTIP_USE_CLOUD`）
- Modify: `src-tauri/src/model/mod.rs`（`OrchestrationContext::from_summary_and_events`）
- Test: commands.rs 内嵌更新（sentence 测试改为读 artifacts；generate_now_impl 测试改新结构）

**Step 1: 写失败测试**（generate_now_impl 编排：mock JsonChat + mock 适配器 → 断言产出含 sentence + description）：

```rust
#[tokio::test]
async fn generate_now_impl_orchestrates_and_produces_sentence() {
    // mock JsonChat 返回固定 JSON；MockMusic/MockArt 适配器
    // 断言 json 含 sentence、music.description、art.description
}
```

**Step 2-4: 实现**（generate_now_impl 签名改为接收 `chat: &dyn JsonChat` + 适配器；流程：ctx → run_orchestrator → 音乐(引擎/MiniMax by route) → 图像(引擎/云端 by route) → 返回 json 含 sentence/paths）：

```rust
pub async fn generate_now_impl(
    date: &str, mood: &str, style: &str, theme_word: &str,
    events: Vec<KeyEvent>, chat: &dyn JsonChat,
    music: &dyn MusicAdapter, art: &dyn ArtAdapter,
) -> anyhow::Result<(Music, Art, String)> {
    let result = run_orchestrator(chat, &ctx_from(theme_word, mood, style, &events)).await?;
    let m = music.generate(&MusicPrompt { description: Some(result.music_description), .. }).await?;
    let a = art.generate(&ArtPrompt { description: Some(result.image_description), .. }).await?;
    let json = serde_json::json!({"music": m, "art": a, "date": date, "mood": mood, "style": style, "sentence": result.sentence});
    Ok((m, a, serde_json::to_string(&json)?))
}
```

**Step 5: 提交**（含删除文件的 commit）

---

### Task 12: generate_sentence 改为读已存句子

**Files:**
- Modify: `src-tauri/src/commands.rs`（generate_sentence 读 artifacts.sentence）
- Test: 更新 generate_sentence 测试

**Step 1: 写失败测试**

```rust
#[test]
fn generate_sentence_reads_stored_sentence() {
    let conn = fresh_db();
    // upsert artifact with sentence "hello world"
    let json = generate_sentence_impl(&conn, "2026-08-08").unwrap();
    assert!(json.contains("hello world"));
}
```

**Step 2-4: 实现**：`generate_sentence_impl` 读 artifacts 表 sentence，无则报错。删除 `top5_keys_to_sentence` 相关。

**Step 5: 提交**

---

### Task 13: 前端 Artworks 改文件渲染

**Files:**
- Modify: `src/views/Artworks.vue`

**Step 1: 写失败测试**（组件测试：mock generationResult 含 art_png_path → 断言渲染 `<img>`）：

```ts
it('画作渲染 art_png_path 的 img', async () => {
  mockStore.generationResult = { ...sample, art_png_path: '/appdata/.../art.png', sentence: 'hi' }
  const wrapper = await mountArtworks()
  const img = wrapper.find('img.ft-art-img')
  expect(img.exists()).toBe(true)
  expect(wrapper.text()).toContain('hi')   // 句子面板用新字段
})
```

**Step 2-4: 实现**：删 canvas/pixels 绘制；`<img :src="convertFileSrc(art_png_path)">`；句子面板读 `result.sentence`；meta 显示 description/model。音乐区不动（amplitudes/duration/Tone.Player）。

**Step 5: 提交**

---

### Task 14: Settings 模型接入表单

**Files:**
- Modify: `src/views/Settings.vue`
- Modify: `src-tauri/src/commands.rs` + `src-tauri/src/lib.rs`（get_model_config / set_model_config 两个新 command，**lib.rs 必须同步注册**）
- Test: 组件测试（表单提交调 set_model_config；vue-tsc 过）

**Step 1: 写失败测试**（组件测试 mock invoke → 断言表单字段渲染 + 保存调 set_model_config）：

```ts
it('模型接入区块渲染引擎地址与模式', async () => {
  const wrapper = mount(Settings)
  expect(wrapper.text()).toContain('模型接入')
  expect(wrapper.find('input[data-test="engine-base-url"]').exists()).toBe(true)
})
```

**Step 2-4: 实现**：Settings 加「模型接入」section：引擎开关+地址、LLM（GGUF 列表 + 云端 key/model + 模式）、图像（本地模型路径 + 云端 + 模式）、音频（引擎 + MiniMax key/model + 模式）。保存调 `set_model_config`。

**Step 5: 提交**

---

### Task 15: 全量验收 + e2e + 合并发版

**Files:**
- Modify: `tests-e2e/`（新增 settings-config.spec.ts：提交配置表单；无引擎红字路径）
- Modify: `CHANGELOG.md`（v0.4.0 段）

**Step 1-3: 验收**
- `cargo test --workspace` 全绿（含新 model 模块单测）
- `npx vitest run` 全绿（Artworks/Settings 组件 + 既有）
- `cd engine && pytest` 全绿
- `pnpm test:e2e` 全绿
- 删除残留引用：全局 grep `wav_encoder|png_encoder|LocalMusic|LocalArt|top5_keys_to_sentence|FINGERTIP_USE_CLOUD` → 0 命中

**Step 4: 提交 + 合并** — CHANGELOG v0.4.0 段；dev 合并 main；tag v0.4.0；push

---

## 关键顺序（依赖）说明

1→2（配置+路由）→3（引擎客户端）→4（云端客户端）→5（编排器）→6（数据结构）→7（WAV 分析）→8→9（适配器）→10（引擎服务，可与 8/9 并行）→11（接线+删除）→12→13→14（前端）→15（验收）。

Task 10 与 8/9 无依赖（引擎是独立 deliverable），可并行。Task 6 的数据结构变化会破坏既有测试，安排在适配器之前完成以留出修测试时间。