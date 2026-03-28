//! 对话补全：主 API 为 [`ChatProvider::complete`] / [`ChatProvider::complete_stream`]（多轮、`tools`、流式 `tool_calls` 增量）。
//!
//! [`ChatProvider::chat`] / [`ChatProvider::chat_stream`] 为单条 `user` 消息的便捷封装，内部委托到上述方法。
//!
//! **`OpenAI` / `Aliyun` / `Ollama` / `Zhipu`**：OpenAI Chat Completions 兼容 `POST {base_url}/chat/completions`；非流式 JSON；流式时 `stream: true`，`data:` 为 chunk JSON。
//!
//! **`Anthropic`**（`anthropic` + `chat`）：Messages API `POST {base_url}/messages`；流式 SSE 见 [Anthropic streaming](https://docs.anthropic.com/en/api/messages-streaming)。
//!
//! **`Google`**（`google` + `chat`）：`generateContent` / `streamGenerateContent`；`tools` 映射为 Gemini `FunctionDeclaration`。
//!
//! # URL 与鉴权（OpenAI 兼容分支）
//!
//! `{base_url}/chat/completions`，`base_url` 会 `trim_end_matches('/')`。鉴权 `Authorization: Bearer {api_key}`。

mod types;

#[cfg(feature = "anthropic")]
mod anthropic_compat;
#[cfg(feature = "google")]
mod google_gemini;
mod openai_compat;

#[cfg(feature = "anthropic")]
use anthropic_compat::AnthropicCompatChat;
use async_trait::async_trait;
#[cfg(feature = "google")]
use google_gemini::GoogleGeminiChat;
use openai_compat::OpenaiCompatChat;
pub use types::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, FinishReason, FunctionCallResult,
    FunctionDefinition, Role, ToolCall, ToolCallDelta, ToolChoice, ToolDefinition,
};

use std::pin::Pin;

use futures::Stream;

use crate::config::Provider;
use crate::config::ProviderConfig;
use crate::error::{Error, Result};

/// 流式对话：每项为 [`Result<ChatChunk>`](crate::error::Result)。
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatChunk>> + Send>>;

#[async_trait]
pub trait ChatProvider: Send + Sync {
    /// 多轮 / 工具调用等非流式补全。
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse>;

    /// 同上，SSE 流式；chunk 含文本增量与 [`ToolCallDelta`]。
    async fn complete_stream(&self, request: &ChatRequest) -> Result<ChatStream>;

    /// 单条 `user` 消息；仅返回 assistant 文本（无文本则 `MissingField`）。
    async fn chat(&self, prompt: &str) -> Result<String> {
        let resp = self.complete(&ChatRequest::single_user(prompt)).await?;
        resp.content
            .filter(|s| !s.is_empty())
            .ok_or(Error::MissingField("response content"))
    }

    /// 单条 `user` 消息流式。
    async fn chat_stream(&self, prompt: &str) -> Result<ChatStream> {
        self.complete_stream(&ChatRequest::single_user(prompt)).await
    }
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
