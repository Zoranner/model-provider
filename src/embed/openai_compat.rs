//! OpenAI 兼容 Embeddings（阿里云 / OpenAI / Ollama）

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::client::HttpClient;
use crate::config::ProviderConfig;
use super::EmbedProvider;
use crate::error::Result;
use crate::util::normalize_for_embedding;

#[derive(Debug, Serialize)]
struct OpenaiEmbedRequest {
    model: String,
    input: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct OpenaiEmbedResponse {
    data: Vec<OpenaiEmbedData>,
}

#[derive(Debug, Deserialize)]
struct OpenaiEmbedData {
    embedding: Vec<f32>,
}

pub(crate) struct OpenaiCompatEmbed {
    client: HttpClient,
    api_key: String,
    model: String,
    base_url: String,
    dimension: usize,
}

impl OpenaiCompatEmbed {
    pub fn new(config: &ProviderConfig, dimension: usize, client: HttpClient) -> Self {
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
impl EmbedProvider for OpenaiCompatEmbed {
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

        let request = OpenaiEmbedRequest {
            model: self.model.clone(),
            input: normalized,
            dimensions: Some(self.dimension),
        };

        let url = format!("{}/embeddings", self.base_url.trim_end_matches('/'));

        let embed_response: OpenaiEmbedResponse = self
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
