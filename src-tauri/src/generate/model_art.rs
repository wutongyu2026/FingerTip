//! v0.4: ModelArtAdapter —— 「吃编排器描述 → 调图像客户端 → 透传 PNG」的真生成路径。
//!
//! 历史：
//!   - v0.3.x: `LocalArtAdapter` 用 `PixelSpec` 序列本地合成 PNG（占位）。
//!   - v0.4:   `ModelArtAdapter` 拿上游 T5 编排器产出的 `ArtPrompt.description`
//!             喂给 `ImageClient`（本地 `EngineClient` / 云端 `MiniMaxImageClient`），
//!             透传返回的 PNG 字节。Art 结构本身不再含像素数据。
//!
//! 设计取舍：
//!   - 严格只吃 `description`，缺失即明确报错（避免静默退化为本地合成）。
//!   - PNG 字节由适配器直接透传 —— 不重解码/不重编码，避免图像质量损耗和契约膨胀。
//!     generate_now 命令层拿字节后统一写盘。
//!   - `model` 字段透传 adapter 构造时的模型标识（"sd-cpp" / "MiniMax-image" 等），
//!     让 artifacts 落盘后可追溯。
//!
//! factory 接线（`build_model_art_adapter`）由 T11 在 generate_now 里实现。
//! 本模块只暴露 `ModelArtAdapter` 与构造器；旧 `build_art_adapter` 的本地路径
//! 在 T11 整体清理时删除。

use std::sync::Arc;

use crate::generate::{Art, ArtAdapter, ArtOutcome, ArtPrompt};
use crate::model::ImageClient;

/// v0.4: 吃 `ArtPrompt.description` 调 `ImageClient` 出 PNG 字节，再透传给调用方的适配器。
///
/// `image`：模型侧图像客户端（本地 `EngineClient` / 云端 `MiniMaxImageClient`，都实现了 `ImageClient`）。
/// `model`：模型标识（写到 `Art.model`，如 `"sd-cpp"` / `"MiniMax-image"`）。
pub struct ModelArtAdapter {
    image: Arc<dyn ImageClient>,
    model: String,
}

impl ModelArtAdapter {
    /// 新建模型艺术适配器。
    ///
    /// `model` 是要写入 `Art.model` 的标识字符串（用于前端/落盘追溯生成模型）。
    /// `image` 是模型侧图像客户端，本地引擎或云端 MiniMax 都实现了 `ImageClient`。
    pub fn new(image: Arc<dyn ImageClient>, model: impl Into<String>) -> Self {
        Self {
            image: Arc::clone(&image),
            model: model.into(),
        }
    }
}

#[async_trait::async_trait]
impl ArtAdapter for ModelArtAdapter {
    fn name(&self) -> &'static str {
        "model"
    }

    async fn generate(&self, prompt: &ArtPrompt) -> anyhow::Result<ArtOutcome> {
        // 1. 编排器必须产出 description；缺失即上层契约违反 —— 不静默退化
        let description = prompt
            .description
            .as_deref()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "ArtAdapter: prompt.description 缺失（编排器未产出，无法调模型）"
                )
            })?;

        // 2. 调 ImageClient 生成图片字节流（本地引擎或云端 MiniMax 返回 PNG 字节）
        let png = self
            .image
            .generate_image(description)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "ModelArtAdapter: ImageClient({}) 生成失败: {}",
                    self.model,
                    e
                )
            })?;

        // 3. 拼装 ArtOutcome —— Art 元数据 + 原始 PNG 字节（不重编码，由调用方落盘）
        Ok(ArtOutcome {
            art: Art {
                theme_word: prompt.theme_word.clone(),
                mood: prompt.mood.clone(),
                description: description.to_string(),
                model: self.model.clone(),
            },
            png,
        })
    }
}

/// Factory：构造 `ModelArtAdapter` 给 T11 在 generate_now 里接线。
///
/// `image` 是 `ImageClient` 多态（本地 `EngineClient` / 云端 `MiniMaxImageClient` 都实现了它）；
/// `model` 写入 `Art.model` 字段。
pub fn build_model_art_adapter(image: Arc<dyn ImageClient>, model: &str) -> ModelArtAdapter {
    ModelArtAdapter::new(image, model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::ArtPrompt;
    use crate::model::ImageClient;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// PNG 头 8 字节 + IHDR + IDAT + IEND 骨架（最小合法 PNG 占位）
    ///
    /// 注意：模型 ImageClient 的契约是「返回 PNG 字节」，不约束必须是合规可渲染 PNG；
    /// 我们这里用最少字节的"PNG 头 + CRC"序列验证透传保真度。
    fn minimal_png() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, // IHDR length
            0x49, 0x48, 0x44, 0x52, // "IHDR"
            0x00, 0x00, 0x00, 0x01, // width=1
            0x00, 0x00, 0x00, 0x01, // height=1
            0x08, 0x06, 0x00, 0x00, 0x00, // bit depth, color type, etc.
            0x1F, 0x15, 0xC4, 0x89, // CRC
            0x00, 0x00, 0x00, 0x0D, // IDAT length
            0x49, 0x44, 0x41, 0x54, // "IDAT"
            0x78, 0x9C, 0x62, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D,
            0xB4, 0x00, 0x00, 0x00, 0x00, // IEND length
            0x49, 0x45, 0x4E, 0x44, // "IEND"
            0xAE, 0x42, 0x60, 0x82, // CRC
        ]
    }

    /// Mock ImageClient：可记录收到的 prompt + 返指定 png 字节
    struct MockImage {
        png: Vec<u8>,
        received: Mutex<Vec<String>>,
    }
    #[async_trait]
    impl ImageClient for MockImage {
        async fn generate_image(&self, prompt: &str) -> anyhow::Result<Vec<u8>> {
            self.received.lock().unwrap().push(prompt.to_string());
            Ok(self.png.clone())
        }
    }

    fn sample_prompt() -> ArtPrompt {
        ArtPrompt {
            events: vec![],
            mood: Some("happy".into()),
            style: "jazz".into(),
            theme_word: "hello".into(),
            description: Some("orange abstract with swirling shapes".into()),
        }
    }

    #[tokio::test]
    async fn model_art_uses_description_and_returns_png_bytes() {
        let png = minimal_png();
        let mock = Arc::new(MockImage {
            png: png.clone(),
            received: Mutex::new(vec![]),
        });
        let adapter = ModelArtAdapter::new(mock.clone() as Arc<dyn ImageClient>, "sd-cpp");
        let out = adapter.generate(&sample_prompt()).await.unwrap();
        // Art 元数据透传
        assert_eq!(out.art.description, "orange abstract with swirling shapes");
        assert_eq!(out.art.model, "sd-cpp");
        assert_eq!(out.art.theme_word, "hello");
        assert_eq!(out.art.mood.as_deref(), Some("happy"));
        // PNG 字节透传（必须 === mock 提供的字节）
        assert_eq!(out.png, png);
        // PNG signature 校验
        assert_eq!(
            out.png[0..8],
            [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
        // Mock 收到的就是 description
        assert_eq!(
            *mock.received.lock().unwrap(),
            vec!["orange abstract with swirling shapes".to_string()]
        );
    }

    #[tokio::test]
    async fn model_art_errors_when_description_missing() {
        let mock = Arc::new(MockImage {
            png: minimal_png(),
            received: Mutex::new(vec![]),
        });
        let adapter = ModelArtAdapter::new(mock as Arc<dyn ImageClient>, "sd-cpp");
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
    async fn model_art_propagates_image_client_errors() {
        // 验证意图：上游 ImageClient 失败时，错误必须带 model 上下文 + 上游原因
        struct FailingImage;
        #[async_trait]
        impl ImageClient for FailingImage {
            async fn generate_image(&self, _: &str) -> anyhow::Result<Vec<u8>> {
                anyhow::bail!("MiniMax rate limit")
            }
        }
        let adapter = ModelArtAdapter::new(
            Arc::new(FailingImage) as Arc<dyn ImageClient>,
            "MiniMax-image",
        );
        let err = adapter.generate(&sample_prompt()).await.unwrap_err();
        // 上游原因透传
        assert!(
            err.to_string().contains("MiniMax"),
            "错误信息应含上游原因，实际: {}",
            err
        );
        // model 上下文带出
        assert!(
            err.to_string().contains("MiniMax-image"),
            "错误信息应包含 model 标识，便于排查路由定位，实际: {}",
            err
        );
    }

    #[tokio::test]
    async fn model_art_constructor_takes_model_name() {
        // 同步测试：构造不需要 .await
        struct EmptyImage;
        #[async_trait]
        impl ImageClient for EmptyImage {
            async fn generate_image(&self, _: &str) -> anyhow::Result<Vec<u8>> {
                Ok(vec![])
            }
        }
        let a = ModelArtAdapter::new(
            Arc::new(EmptyImage) as Arc<dyn ImageClient>,
            "minimax-image",
        );
        assert_eq!(a.model, "minimax-image");
        assert_eq!(a.name(), "model");
    }

    #[test]
    fn build_model_art_adapter_factory_works() {
        struct EmptyImage;
        #[async_trait]
        impl ImageClient for EmptyImage {
            async fn generate_image(&self, _: &str) -> anyhow::Result<Vec<u8>> {
                Ok(vec![])
            }
        }
        let a = build_model_art_adapter(
            Arc::new(EmptyImage) as Arc<dyn ImageClient>,
            "minimax-image",
        );
        assert_eq!(a.model, "minimax-image");
    }
}
