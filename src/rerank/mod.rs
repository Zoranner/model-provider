//! 查询–文档重排序：非流式 JSON，默认 HTTP 超时约 60 秒（可用 [`ProviderConfig::timeout`] 覆盖）。
//!
//! # 支持的厂商
//!
//! 仅 **`Aliyun`** 与 **`Zhipu`**（均须启用 `rerank` 与对应厂商 feature）。其它厂商在工厂阶段返回 [`Error::ProviderDisabled`]。
//!
//! # HTTP 路径（注意阿里云为复数）
//!
//! - **阿里云**：`POST {base_url}/reranks`（路径段为 **`reranks`**）。
//! - **智谱**：`POST {base_url}/rerank`。
//!
//! `base_url` 均会先 `trim_end_matches('/')` 再拼接。请求体含 `model`、`query`、`documents`（字符串数组）、`top_n`（可选）。成功时解析 `results[].index` 与 `relevance_score`，映射为 [`RerankItem::index`] 与 [`RerankItem::score`]。
//!
//! 智谱侧若分数异常，实现会在启动时打日志提示可改用阿里云 Rerank（以 `tracing` 为准）。
//!
//! # 鉴权
//!
//! 与其它模态相同：Bearer + JSON POST。

mod aliyun;
mod zhipu;

use async_trait::async_trait;
use std::time::Duration;

use crate::client::HttpClient;
use crate::config::Provider;
use crate::config::ProviderConfig;
use crate::error::{Error, Result};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// 单条排序结果：在原始 `documents` 切片中的下标与相关度分数。
#[derive(Debug, Clone)]
pub struct RerankItem {
    pub index: usize,
    pub score: f64,
}

#[async_trait]
pub trait RerankProvider: Send + Sync {
    /// `top_n` 为 `None` 时由上游默认行为决定返回条数。
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
