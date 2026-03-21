//! 阿里云 DashScope Rerank

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::client::HttpClient;
use crate::config::ProviderConfig;
use crate::error::Result;
use crate::rerank::{RerankItem, RerankProvider};

#[derive(Debug, Serialize)]
struct AliyunRerankRequest {
    model: String,
    query: String,
    documents: Vec<String>,
    top_n: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AliyunRerankResponse {
    results: Vec<AliyunRerankResult>,
}

#[derive(Debug, Deserialize)]
struct AliyunRerankResult {
    index: usize,
    relevance_score: f64,
}

#[derive(Debug, Deserialize)]
struct AliyunErrorResponse {
    code: Option<String>,
    message: Option<String>,
}

fn parse_aliyun_error(body: &str) -> String {
    if let Ok(err) = serde_json::from_str::<AliyunErrorResponse>(body) {
        if let (Some(code), Some(message)) = (err.code, err.message) {
            return format!("{}: {}", code, message);
        }
    }
    body.to_string()
}

pub(crate) struct AliyunRerank {
    client: HttpClient,
    api_key: String,
    model: String,
    base_url: String,
}

impl AliyunRerank {
    pub fn new(config: &ProviderConfig, client: HttpClient) -> Self {
        tracing::info!(
            "AliyunRerank: model={}, base_url={}",
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
impl RerankProvider for AliyunRerank {
    async fn rerank(
        &self,
        query: &str,
        documents: &[&str],
        top_n: Option<usize>,
    ) -> Result<Vec<RerankItem>> {
        let request = AliyunRerankRequest {
            model: self.model.clone(),
            query: query.to_string(),
            documents: documents.iter().map(|s| s.to_string()).collect(),
            top_n,
        };

        let url = format!("{}/reranks", self.base_url.trim_end_matches('/'));

        let rerank_response: AliyunRerankResponse = self
            .client
            .post_bearer_json(&url, &self.api_key, &request, |s| parse_aliyun_error(&s))
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
