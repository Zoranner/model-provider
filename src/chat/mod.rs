//! 对话补全：非流式 JSON 与 **SSE 流式**（`chat_stream`）。
//!
//! # [`ChatProvider::chat`]
//!
//! `prompt` 为本轮**唯一用户文本**：OpenAI 兼容分支将其作为单条 `role: user` 消息发送；Anthropic、Google 的请求 JSON 形状见各自实现（Google 单轮示例不发送可选字段 `role`）。均不含 system、多轮 history；若需要须另行扩展 API 或直连接口。
//!
//! **`OpenAI` / `Aliyun` / `Ollama` / `Zhipu`**：OpenAI 兼容 `POST {base_url}/chat/completions`，非流式；流式时请求体 `stream: true`，响应 `text/event-stream`，`data:` 行为 OpenAI Chat Completions chunk JSON。
//!
//! **`Anthropic`**（`anthropic` + `chat`）：**Anthropic Messages 兼容**实现，见源码 `anthropic_compat.rs`。流式时 `stream: true`，SSE 事件见 [Anthropic streaming](https://docs.anthropic.com/en/api/messages-streaming)。
//!
//! **`Google`**（`google` + `chat`）：非流式为 `generateContent`；流式为 `streamGenerateContent`，SSE `data:` 为 `GenerateContentResponse` 片段 JSON。
//!
//! # [`ChatProvider::chat_stream`]
//!
//! 返回 [`ChatStream`]：每项为 [`ChatChunk`]（文本增量与可选 [`FinishReason`]`）。具体字段映射见各实现与 `docs/http-endpoints.md`。
//!
//! # URL 与鉴权（OpenAI 兼容分支）
//!
//! 请求地址为 `{base_url}/chat/completions`，其中 `base_url` 来自 [`ProviderConfig`]，会先对 `base_url` 做 `trim_end_matches('/')` 再拼接路径段。
//!
//! 鉴权为 `Authorization: Bearer {api_key}`。**空字符串密钥仍会原样放入请求头**；网关是否接受由上游决定（例如部分本地 Ollama 部署不校验 Bearer）。

mod types;

#[cfg(feature = "anthropic")]
mod anthropic_compat;
#[cfg(feature = "google")]
mod google_gemini;
mod openai_compat;

#[cfg(feature = "anthropic")]
use anthropic_compat::AnthropicCompatChat;
#[cfg(feature = "anthropic")]
pub use anthropic_compat::ANTHROPIC_VERSION;
use async_trait::async_trait;
#[cfg(feature = "google")]
use google_gemini::GoogleGeminiChat;
use openai_compat::OpenaiCompatChat;
pub use types::{ChatChunk, FinishReason};

use std::pin::Pin;

use futures::Stream;

use crate::config::Provider;
use crate::config::ProviderConfig;
use crate::error::Result;

/// 流式对话：每项为 [`Result<ChatChunk>`](crate::error::Result)。
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatChunk>> + Send>>;

#[async_trait]
pub trait ChatProvider: Send + Sync {
    /// 单轮用户消息补全；语义与模块级文档一致。
    async fn chat(&self, prompt: &str) -> Result<String>;

    /// 单轮流式补全；SSE 解析失败或上游错误时流中会出现 `Err`。
    async fn chat_stream(&self, prompt: &str) -> Result<ChatStream>;
}

pub(crate) fn create(config: &ProviderConfig) -> Result<Box<dyn ChatProvider>> {
    match config.provider {
        #[cfg(feature = "openai")]
        Provider::OpenAI => Ok(Box::new(OpenaiCompatChat::new(config)?)),
        #[cfg(not(feature = "openai"))]
        Provider::OpenAI => Err(crate::error::Error::ProviderDisabled("openai".to_string())),

        #[cfg(feature = "aliyun")]
        Provider::Aliyun => Ok(Box::new(OpenaiCompatChat::new(config)?)),
        #[cfg(not(feature = "aliyun"))]
        Provider::Aliyun => Err(crate::error::Error::ProviderDisabled("aliyun".to_string())),

        #[cfg(feature = "anthropic")]
        Provider::Anthropic => Ok(Box::new(AnthropicCompatChat::new(config)?)),
        #[cfg(not(feature = "anthropic"))]
        Provider::Anthropic => Err(crate::error::Error::ProviderDisabled(
            "anthropic".to_string(),
        )),

        #[cfg(feature = "google")]
        Provider::Google => Ok(Box::new(GoogleGeminiChat::new(config)?)),
        #[cfg(not(feature = "google"))]
        Provider::Google => Err(crate::error::Error::ProviderDisabled("google".to_string())),

        #[cfg(feature = "ollama")]
        Provider::Ollama => Ok(Box::new(OpenaiCompatChat::new(config)?)),
        #[cfg(not(feature = "ollama"))]
        Provider::Ollama => Err(crate::error::Error::ProviderDisabled("ollama".to_string())),

        #[cfg(feature = "zhipu")]
        Provider::Zhipu => Ok(Box::new(OpenaiCompatChat::new(config)?)),
        #[cfg(not(feature = "zhipu"))]
        Provider::Zhipu => Err(crate::error::Error::ProviderDisabled("zhipu".to_string())),
    }
}
