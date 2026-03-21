//! 图像生成（骨架，待接入具体厂商）

use async_trait::async_trait;

use crate::config::ProviderConfig;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy)]
pub enum ImageSize {
    Square512,
    Square1024,
    Landscape,
    Portrait,
}

#[derive(Debug, Clone)]
pub enum ImageOutput {
    Url(String),
    Bytes(Vec<u8>),
}

#[async_trait]
pub trait ImageProvider: Send + Sync {
    async fn generate(&self, prompt: &str, size: ImageSize) -> Result<ImageOutput>;
}

pub(crate) fn create(config: &ProviderConfig) -> Result<Box<dyn ImageProvider>> {
    Err(Error::Unsupported {
        provider: config.provider.to_string(),
        capability: "image",
    })
}
