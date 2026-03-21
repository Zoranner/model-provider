//! 多厂商 AI API 客户端（按 Cargo feature 裁剪厂商与模态）。
//!
//! 0.2 起：`ProviderConfig` 使用 [`Provider`] 枚举；`LlmProvider` 更名为 [`ChatProvider`]，
//! `create_llm_provider` 更名为 [`create_chat_provider`]；错误类型为 [`Error`]（不再使用 `anyhow`）。

mod client;
mod config;
mod error;
mod util;

#[cfg(feature = "chat")]
mod chat;
#[cfg(feature = "embed")]
mod embed;
#[cfg(feature = "rerank")]
mod rerank;
#[cfg(feature = "image")]
mod image;
#[cfg(feature = "audio")]
mod audio;

pub use config::{Provider, ProviderConfig};
pub use error::{Error, Result};

#[cfg(feature = "chat")]
pub use chat::ChatProvider;
#[cfg(feature = "embed")]
pub use embed::EmbedProvider;
#[cfg(feature = "rerank")]
pub use rerank::{RerankItem, RerankProvider};
#[cfg(feature = "image")]
pub use image::{ImageOutput, ImageProvider, ImageSize};
#[cfg(feature = "audio")]
pub use audio::{AudioFormat, SpeechProvider, TranscriptionProvider};

/// 创建 Chat Provider（OpenAI 兼容 `POST .../chat/completions`）
#[cfg(feature = "chat")]
pub fn create_chat_provider(config: &ProviderConfig) -> Result<Box<dyn ChatProvider>> {
    chat::create(config)
}

/// 创建 Embedding Provider
#[cfg(feature = "embed")]
pub fn create_embed_provider(config: &ProviderConfig) -> Result<Box<dyn EmbedProvider>> {
    embed::create(config)
}

/// 创建 Rerank Provider（当前支持阿里云、智谱）
#[cfg(feature = "rerank")]
pub fn create_rerank_provider(config: &ProviderConfig) -> Result<Box<dyn RerankProvider>> {
    rerank::create(config)
}

/// 创建图像生成 Provider（尚未实现具体厂商）
#[cfg(feature = "image")]
pub fn create_image_provider(config: &ProviderConfig) -> Result<Box<dyn ImageProvider>> {
    image::create(config)
}

/// 创建语音识别 Provider（尚未实现具体厂商）
#[cfg(feature = "audio")]
pub fn create_transcription_provider(
    config: &ProviderConfig,
) -> Result<Box<dyn TranscriptionProvider>> {
    audio::create_transcription(config)
}

/// 创建语音合成 Provider（尚未实现具体厂商）
#[cfg(feature = "audio")]
pub fn create_speech_provider(config: &ProviderConfig) -> Result<Box<dyn SpeechProvider>> {
    audio::create_speech(config)
}
