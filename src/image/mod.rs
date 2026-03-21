//! 文生图（非流式 JSON）。默认 HTTP 超时在各自实现内约为 120 秒，可用 [`ProviderConfig::timeout`] 覆盖。
//!
//! # 厂商与错误
//!
//! - **`OpenAI`**（`openai` + `image`）：`POST {base_url}/images/generations`，OpenAI 兼容；`n` 固定为 `1`，`size` 由 [`ImageSize`] 映射为 `512x512` 等字符串。成功时取 `data[0]` 的 `url` 或 `b64_json`（解码为 [`ImageOutput::Bytes`]）。
//! - **`Aliyun`**（`aliyun` + `image`）：`POST {base_url}/services/aigc/multimodal-generation/generation`。此处 **`base_url` 一般为 DashScope 原生根**（如 `https://dashscope.aliyuncs.com/api/v1`），与对话用的 `compatible-mode/v1` **不是同一路径**。请求体为 DashScope multimodal 格式，尺寸为 `宽*高`（星号）。详见实现文件中的结构体注释。
//!
//! 启用 `image` 但未启用 `openai` / `aliyun` 时，仍选择 `OpenAI` / `Aliyun` 会得到 [`Error::ProviderDisabled`]。**`Ollama`**、**`Zhipu`** 无文生图实现，工厂返回 [`Error::Unsupported`]（`capability` 为 `"image"`）。
//!
//! # 鉴权
//!
//! 与其它模态相同：Bearer + JSON POST。

#[cfg(all(feature = "aliyun", feature = "image"))]
mod aliyun;
#[cfg(all(feature = "openai", feature = "image"))]
mod openai_compat;

use async_trait::async_trait;

use crate::config::Provider;
use crate::config::ProviderConfig;
use crate::error::{Error, Result};

/// 生成尺寸。OpenAI 使用 `宽x高`；阿里云 DashScope 使用 `宽*高`（实现中分别映射）。
#[derive(Debug, Clone, Copy)]
pub enum ImageSize {
    /// 正方形较小边（OpenAI `512x512` / 阿里云 `512*512`）
    Square512,
    /// 正方形 1K（`1024x1024` / `1024*1024`）
    Square1024,
    /// 横版（OpenAI `1792x1024`；阿里云 `1792*1024`）
    Landscape,
    /// 竖版（OpenAI `1024x1792`；阿里云 `1024*1792`）
    Portrait,
}

/// 生成结果：远端 URL，或 PNG 等字节的 `b64_json` 解码结果。
#[derive(Debug, Clone)]
pub enum ImageOutput {
    Url(String),
    Bytes(Vec<u8>),
}

#[async_trait]
pub trait ImageProvider: Send + Sync {
    /// 单次生成一张图；`size` 映射方式见 [`ImageSize`]。
    async fn generate(&self, prompt: &str, size: ImageSize) -> Result<ImageOutput>;
}

pub(crate) fn create(config: &ProviderConfig) -> Result<Box<dyn ImageProvider>> {
    match config.provider {
        #[cfg(feature = "openai")]
        Provider::OpenAI => Ok(Box::new(openai_compat::OpenaiCompatImage::new(config)?)),
        #[cfg(not(feature = "openai"))]
        Provider::OpenAI => Err(Error::ProviderDisabled("openai".to_string())),

        #[cfg(feature = "aliyun")]
        Provider::Aliyun => Ok(Box::new(aliyun::AliyunQwenImage::new(config)?)),
        #[cfg(not(feature = "aliyun"))]
        Provider::Aliyun => Err(Error::ProviderDisabled("aliyun".to_string())),

        Provider::Ollama => Err(Error::Unsupported {
            provider: config.provider.to_string(),
            capability: "image",
        }),
        Provider::Zhipu => Err(Error::Unsupported {
            provider: config.provider.to_string(),
            capability: "image",
        }),
    }
}

#[cfg(test)]
mod factory_tests {
    use super::create;
    use crate::config::{Provider, ProviderConfig};
    use crate::error::Error;

    #[cfg(feature = "ollama")]
    #[test]
    fn ollama_is_unsupported() {
        let cfg = ProviderConfig::new(Provider::Ollama, "k", "http://localhost/v1", "m");
        match create(&cfg) {
            Err(Error::Unsupported {
                provider,
                capability,
            }) => {
                assert_eq!(provider, "ollama");
                assert_eq!(capability, "image");
            }
            Ok(_) => panic!("expected error"),
            Err(e) => panic!("expected Unsupported, got {:?}", e),
        }
    }

    #[cfg(feature = "zhipu")]
    #[test]
    fn zhipu_is_unsupported() {
        let cfg = ProviderConfig::new(Provider::Zhipu, "k", "https://x/v1", "m");
        match create(&cfg) {
            Err(Error::Unsupported {
                provider,
                capability,
            }) => {
                assert_eq!(provider, "zhipu");
                assert_eq!(capability, "image");
            }
            Ok(_) => panic!("expected error"),
            Err(e) => panic!("expected Unsupported, got {:?}", e),
        }
    }

    #[cfg(not(feature = "openai"))]
    #[test]
    fn openai_disabled_without_openai_feature() {
        let cfg = ProviderConfig::new(
            Provider::OpenAI,
            "k",
            "https://api.openai.com/v1",
            "dall-e-3",
        );
        match create(&cfg) {
            Err(Error::ProviderDisabled(s)) => assert_eq!(s, "openai"),
            Ok(_) => panic!("expected error"),
            Err(e) => panic!("expected ProviderDisabled, got {:?}", e),
        }
    }

    #[cfg(not(feature = "aliyun"))]
    #[test]
    fn aliyun_disabled_without_aliyun_feature() {
        let cfg = ProviderConfig::new(
            Provider::Aliyun,
            "k",
            "https://dashscope.aliyuncs.com/api/v1",
            "qwen-image-plus",
        );
        match create(&cfg) {
            Err(Error::ProviderDisabled(s)) => assert_eq!(s, "aliyun"),
            Ok(_) => panic!("expected error"),
            Err(e) => panic!("expected ProviderDisabled, got {:?}", e),
        }
    }
}
