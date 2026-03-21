//! 智谱 Embedding（请求体不含 `dimensions`）

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::client::HttpClient;
use crate::config::ProviderConfig;
use super::EmbedProvider;
use crate::error::Result;
use crate::util::normalize_for_embedding;

#[derive(Debug, Serialize)]
struct ZhipuEmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ZhipuEmbedResponse {
    data: Vec<ZhipuEmbedData>,
}

#[derive(Debug, Deserialize)]
struct ZhipuEmbedData {
    embedding: Vec<f32>,
}

pub(crate) struct ZhipuEmbed {
    client: HttpClient,
    api_key: String,
    model: String,
    base_url: String,
    dimension: usize,
}

impl ZhipuEmbed {
    pub fn new(config: &ProviderConfig, dimension: usize, client: HttpClient) -> Self {
        tracing::info!(
            "ZhipuEmbed: model={}, dimension={}, base_url={}",
            config.model,
            dimension,
            config.base_url
        );
        Self {
            client,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            base_url: config.base_url.clone(),
            dimension,
        }
    }
}

#[async_trait]
impl EmbedProvider for ZhipuEmbed {
    async fn encode(&self, text: &str) -> Result<Vec<f32>> {
        let normalized = normalize_for_embedding(text);
        let embeddings = self.encode_batch(&[&normalized]).await?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| crate::error::Error::MissingField("embeddings[0]"))
    }

    async fn encode_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let normalized: Vec<String> = texts.iter().map(|t| normalize_for_embedding(t)).collect();

        let request = ZhipuEmbedRequest {
            model: self.model.clone(),
            input: normalized,
        };

        let url = format!("{}/embeddings", self.base_url.trim_end_matches('/'));

        let embed_response: ZhipuEmbedResponse = self
            .client
            .post_bearer_json(&url, &self.api_key, &request, |s| s)
            .await?;

        Ok(embed_response
            .data
            .into_iter()
            .map(|d| d.embedding)
            .collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}
