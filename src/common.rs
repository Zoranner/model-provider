//! 共享类型和 OpenAI 兼容格式实现

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::ProviderConfig;
use crate::traits::{EmbedProvider, LlmProvider};

/// 文本规范化
pub(crate) fn normalize_for_embedding(text: &str) -> String {
    let text = text.trim();
    let re = regex::Regex::new(r"\s+").unwrap();
    re.replace_all(text, " ").to_string()
}

/// OpenAI 兼容格式 Embed（供阿里云、OpenAI、Ollama 使用）
pub struct OpenaiCompatibleEmbed {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
    dimension: usize,
}

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

impl OpenaiCompatibleEmbed {
    pub fn new(config: &ProviderConfig, dimension: usize) -> Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Self {
            client,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            base_url: config.base_url.clone(),
            dimension,
        })
    }
}

#[async_trait]
impl EmbedProvider for OpenaiCompatibleEmbed {
    async fn encode(&self, text: &str) -> Result<Vec<f32>> {
        let normalized = normalize_for_embedding(text);
        let embeddings = self.encode_batch(&[&normalized]).await?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    async fn encode_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let normalized: Vec<String> = texts.iter().map(|t| normalize_for_embedding(t)).collect();

        let request = OpenaiEmbedRequest {
            model: self.model.clone(),
            input: normalized,
            dimensions: Some(self.dimension),
        };

        let url = format!("{}/embeddings", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            tracing::error!("Embed API error ({}): {}", status, error_text);
            anyhow::bail!("Embed API error ({}): {}", status, error_text);
        }

        let embed_response: OpenaiEmbedResponse = response.json().await?;

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

/// OpenAI 兼容格式 LLM（供阿里云、OpenAI、Ollama、智谱使用）
pub struct OpenaiCompatibleLlm {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct OpenaiChatRequest {
    model: String,
    messages: Vec<OpenaiChatMessage>,
    temperature: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenaiChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenaiChatResponse {
    choices: Vec<OpenaiChatChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenaiChatChoice {
    message: OpenaiChatMessage,
}

impl OpenaiCompatibleLlm {
    pub fn new(config: &ProviderConfig) -> Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(60)).build()?;
        Ok(Self {
            client,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            base_url: config.base_url.clone(),
        })
    }
}

#[async_trait]
impl LlmProvider for OpenaiCompatibleLlm {
    async fn chat(&self, prompt: &str) -> Result<String> {
        let request = OpenaiChatRequest {
            model: self.model.clone(),
            messages: vec![OpenaiChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            temperature: 0.2,
        };

        let url = format!("{}/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            tracing::error!("LLM API error ({}): {}", status, error_text);
            anyhow::bail!("LLM API error ({}): {}", status, error_text);
        }

        let chat_response: OpenaiChatResponse = response.json().await?;

        chat_response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| anyhow::anyhow!("LLM response has no choices"))
    }
}
