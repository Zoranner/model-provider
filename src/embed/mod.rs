//! 文本嵌入

mod openai_compat;
#[cfg(feature = "zhipu")]
mod zhipu;

use async_trait::async_trait;
use std::time::Duration;

use crate::client::HttpClient;
use crate::config::Provider;
use crate::config::ProviderConfig;
use crate::error::{Error, Result};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[async_trait]
pub trait EmbedProvider: Send + Sync {
    async fn encode(&self, text: &str) -> Result<Vec<f32>>;
    async fn encode_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
}

fn http_client(config: &ProviderConfig) -> Result<HttpClient> {
    HttpClient::new(config.timeout.unwrap_or(DEFAULT_TIMEOUT))
}

pub(crate) fn create(config: &ProviderConfig) -> Result<Box<dyn EmbedProvider>> {
    let dimension = config
        .dimension
        .ok_or(Error::MissingConfig("dimension"))?;

    #[allow(unreachable_patterns)]
    match config.provider {
        #[cfg(all(feature = "openai", feature = "embed"))]
        Provider::OpenAI => Ok(Box::new(openai_compat::OpenaiCompatEmbed::new(
            config,
            dimension,
            http_client(config)?,
        ))),
        #[cfg(all(feature = "aliyun", feature = "embed"))]
        Provider::Aliyun => Ok(Box::new(openai_compat::OpenaiCompatEmbed::new(
            config,
            dimension,
            http_client(config)?,
        ))),
        #[cfg(all(feature = "ollama", feature = "embed"))]
        Provider::Ollama => Ok(Box::new(openai_compat::OpenaiCompatEmbed::new(
            config,
            dimension,
            http_client(config)?,
        ))),
        #[cfg(all(feature = "zhipu", feature = "embed"))]
        Provider::Zhipu => Ok(Box::new(zhipu::ZhipuEmbed::new(
            config,
            dimension,
            http_client(config)?,
        ))),
        #[cfg(all(feature = "embed", not(feature = "zhipu")))]
        Provider::Zhipu => Err(Error::ProviderDisabled("zhipu".to_string())),
        p => Err(Error::ProviderDisabled(p.to_string())),
    }
}
