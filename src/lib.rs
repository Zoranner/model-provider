//! 多厂商 AI API 客户端（按 Cargo feature 裁剪厂商与模态）。
//!
//! 0.2 起：`ProviderConfig` 使用 [`Provider`] 枚举；`LlmProvider` 更名为 [`ChatProvider`]，
//! `create_llm_provider` 更名为 [`create_chat_provider`]；错误类型为 [`Error`]（不再使用 `anyhow`）。
//!
//! # Feature 与工厂
//!
//! 厂商（如 `openai`）与模态（如 `chat`）需同时在 `Cargo.toml` 中启用，[`create_chat_provider`] 等工厂才会编译进来。
//! 若运行时厂商与 feature 组合不匹配，通常得到 [`Error::ProviderDisabled`]；若该厂商在某一模态下未实现，则见 [`Error::Unsupported`]。
//! 详见 [`Error`] 各变体说明。
//!
//! # 各模态契约摘要
//!
//! **对话（[`ChatProvider`]）**：`OpenAI` / `Aliyun` / `Ollama` / `Zhipu` 为 OpenAI 兼容 `POST {base_url}/chat/completions`；非流式 JSON 与流式 SSE（[`ChatProvider::chat_stream`]，`stream: true`）均支持。每次调用只发送一条 `user` 消息，`temperature` 固定为 `0.2`。**`Anthropic`**（`anthropic` + `chat`）为 **Anthropic Messages 兼容**，`POST {base_url}/messages`（流式同样 `stream: true`，SSE 事件见官方 streaming 文档）。**`Google`**（`google` + `chat`）非流式为 **`generateContent`**；流式为 **`streamGenerateContent`**，API Key 为 query `key`（见 [`chat`]）。
//!
//! **向量（[`EmbedProvider`]）**：`OpenAI` / `Aliyun` / `Ollama` / `Zhipu` 为 `POST {base_url}/embeddings`（非流式 JSON）。**`Google`**（`google` + `embed`）为 Gemini **`embedContent` / `batchEmbedContents`**（query `key`），见 [`embed`]。创建前须在 [`ProviderConfig::dimension`] 中设置维数，否则工厂返回 [`Error::MissingConfig`]。约定与文本预处理见 [`embed`]。
//!
//! **重排序**：启用 `rerank` feature 时，仅 `Aliyun` 与 `Zhipu` 有实现；`OpenAI` / `Ollama` / `Anthropic` / `Google` 会得到 [`Error::Unsupported`]（`capability` 为 `rerank`）。HTTP 路径分别为 `{base_url}/reranks` 与 `{base_url}/rerank`（阿里云为复数 `reranks`）。详见 `rerank` 模块文档（生成文档时需启用该 feature）。
//!
//! **文生图**：启用 `image` feature 时，OpenAI 与阿里云路径见 `image` 模块；未启用对应厂商 feature 时工厂返回 [`Error::ProviderDisabled`]。`Ollama` / `Zhipu` / `Anthropic` / `Google` 无实现，返回 [`Error::Unsupported`]（`capability` 为 `image`）。
//!
//! **音频**：启用 `audio` feature 后，工厂函数 `create_transcription_provider` 与 `create_speech_provider` 仍返回
//! [`Error::Unsupported`]；trait `TranscriptionProvider` 与 `SpeechProvider` 仅占位，尚未对接任何厂商 HTTP。详见 `audio` 模块（启用 `audio` feature 后可在文档中打开）。

mod client;
mod config;
mod error;
mod sse;
mod util;

#[cfg(feature = "audio")]
pub mod audio;
#[cfg(feature = "chat")]
pub mod chat;
#[cfg(feature = "embed")]
pub mod embed;
#[cfg(feature = "image")]
pub mod image;
#[cfg(feature = "rerank")]
pub mod rerank;

pub use config::{Provider, ProviderConfig};
pub use error::{Error, Result};

#[cfg(feature = "audio")]
pub use audio::{AudioFormat, SpeechProvider, TranscriptionProvider};
#[cfg(feature = "chat")]
pub use chat::{ChatChunk, ChatProvider, ChatStream, FinishReason};
#[cfg(feature = "embed")]
pub use embed::EmbedProvider;
#[cfg(feature = "image")]
pub use image::{ImageOutput, ImageProvider, ImageSize};
#[cfg(feature = "rerank")]
pub use rerank::{RerankItem, RerankProvider};

/// 创建 Chat Provider（单轮对话；非流式 [`ChatProvider::chat`] 与流式 [`ChatProvider::chat_stream`]；见 [`chat`]）。
#[cfg(feature = "chat")]
pub fn create_chat_provider(config: &ProviderConfig) -> Result<Box<dyn ChatProvider>> {
    chat::create(config)
}

/// 创建 Embedding Provider（需要 [`ProviderConfig::dimension`]；约定见 [`embed`]）。
#[cfg(feature = "embed")]
pub fn create_embed_provider(config: &ProviderConfig) -> Result<Box<dyn EmbedProvider>> {
    embed::create(config)
}

/// 创建 Rerank Provider（阿里云、智谱；`OpenAI` / `Ollama` 返回 [`Error::Unsupported`]；HTTP 路径见 [`rerank`]）。
#[cfg(feature = "rerank")]
pub fn create_rerank_provider(config: &ProviderConfig) -> Result<Box<dyn RerankProvider>> {
    rerank::create(config)
}

/// 创建图像生成 Provider（端点与 `base_url` 约定见 [`image`]）。
#[cfg(feature = "image")]
pub fn create_image_provider(config: &ProviderConfig) -> Result<Box<dyn ImageProvider>> {
    image::create(config)
}

/// 创建语音识别 Provider。当前始终返回 [`Error::Unsupported`]，未对接厂商（见 [`audio`]）。
#[cfg(feature = "audio")]
pub fn create_transcription_provider(
    config: &ProviderConfig,
) -> Result<Box<dyn TranscriptionProvider>> {
    audio::create_transcription(config)
}

/// 创建语音合成 Provider。当前始终返回 [`Error::Unsupported`]，未对接厂商（见 [`audio`]）。
#[cfg(feature = "audio")]
pub fn create_speech_provider(config: &ProviderConfig) -> Result<Box<dyn SpeechProvider>> {
    audio::create_speech(config)
}
