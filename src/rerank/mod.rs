//! 重排序

mod aliyun;
mod zhipu;

use async_trait::async_trait;
use std::time::Duration;

use crate::client::HttpClient;
use crate::config::Provider;
use crate::config::ProviderConfig;
use crate::error::{Error, Result};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct RerankItem {
    pub index: usize,
    pub score: f64,
}

#[async_trait]
pub trait RerankProvider: Send + Sync {
    async fn rerank(
        &self,
        query: &str,
        documents: &[&str],
        top_n: Option<usize>,
    ) -> Result<Vec<RerankItem>>;
}

fn http_client(config: &ProviderConfig) -> Result<HttpClient> {
    HttpClient::new(config.timeout.unwrap_or(DEFAULT_TIMEOUT))
}

pub(crate) fn create(config: &ProviderConfig) -> Result<Box<dyn RerankProvider>> {
    match config.provider {
        #[cfg(all(feature = "aliyun", feature = "rerank"))]
        Provider::Aliyun => Ok(Box::new(aliyun::AliyunRerank::new(
            config,
            http_client(config)?,
        ))),
        #[cfg(all(feature = "zhipu", feature = "rerank"))]
        Provider::Zhipu => Ok(Box::new(zhipu::ZhipuRerank::new(
            config,
            http_client(config)?,
        ))),
        p => Err(Error::ProviderDisabled(p.to_string())),
    }
}
