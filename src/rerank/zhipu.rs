//! 智谱 Rerank

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::client::HttpClient;
use crate::config::ProviderConfig;
use crate::error::Result;
use crate::rerank::{RerankItem, RerankProvider};

#[derive(Debug, Serialize)]
struct ZhipuRerankRequest {
    model: String,
    query: String,
    documents: Vec<String>,
    top_n: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ZhipuRerankResponse {
    results: Vec<ZhipuRerankResult>,
}

#[derive(Debug, Deserialize)]
struct ZhipuRerankResult {
    index: usize,
    relevance_score: f64,
}

pub(crate) struct ZhipuRerank {
    client: HttpClient,
    api_key: String,
    model: String,
    base_url: String,
}

impl ZhipuRerank {
    pub fn new(config: &ProviderConfig, client: HttpClient) -> Self {
        tracing::warn!(
            "ZhipuRerank: 已知部分环境下分数接近常数，必要时可改用阿里云 Rerank"
        );
        tracing::info!(
            "ZhipuRerank: model={}, base_url={}",
            config.model,
            config.base_url
        );
        Self {
            client,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            base_url: config.base_url.clone(),
        }
    }
}

#[async_trait]
impl RerankProvider for ZhipuRerank {
    async fn rerank(
        &self,
        query: &str,
        documents: &[&str],
        top_n: Option<usize>,
    ) -> Result<Vec<RerankItem>> {
        let request = ZhipuRerankRequest {
            model: self.model.clone(),
            query: query.to_string(),
            documents: documents.iter().map(|s| s.to_string()).collect(),
            top_n,
        };

        let url = format!("{}/rerank", self.base_url.trim_end_matches('/'));

        let rerank_response: ZhipuRerankResponse = self
            .client
            .post_bearer_json(&url, &self.api_key, &request, |s| s)
            .await?;

        Ok(rerank_response
            .results
            .into_iter()
            .map(|r| RerankItem {
                index: r.index,
                score: r.relevance_score,
            })
            .collect())
    }
}
