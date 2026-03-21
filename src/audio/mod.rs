//! 语音识别（ASR）与语音合成（TTS）的 **trait 与类型占位**。
//!
//! # 当前行为
//!
//! [`crate::create_transcription_provider`] 与 [`crate::create_speech_provider`] 会立即返回 [`Error::Unsupported`]，**不发起任何 HTTP**。`capability` 分别为 `"transcription"` 与 `"speech"`。
//!
//! 启用 `audio` feature 仅为了引入上述类型，便于下游依赖预先编写接口；对接具体厂商时需新增实现并改工厂分支（往往还涉及 multipart、流式响应等，与现有 JSON POST 客户端不同）。

use async_trait::async_trait;

use crate::config::ProviderConfig;
use crate::error::{Error, Result};

/// 输入/输出音频容器格式提示（具体字节布局由后续厂商实现约定）。
#[derive(Debug, Clone, Copy)]
pub enum AudioFormat {
    Wav,
    Mp3,
    Ogg,
    Flac,
}

#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    /// 将音频字节转为文本；尚无库内实现。
    async fn transcribe(&self, audio: &[u8], format: AudioFormat) -> Result<String>;
}

#[async_trait]
pub trait SpeechProvider: Send + Sync {
    /// 将文本合成为音频字节；尚无库内实现。
    async fn synthesize(&self, text: &str, voice: &str) -> Result<Vec<u8>>;
}

pub(crate) fn create_transcription(
    config: &ProviderConfig,
) -> Result<Box<dyn TranscriptionProvider>> {
    Err(Error::Unsupported {
        provider: config.provider.to_string(),
        capability: "transcription",
    })
}

pub(crate) fn create_speech(config: &ProviderConfig) -> Result<Box<dyn SpeechProvider>> {
    Err(Error::Unsupported {
        provider: config.provider.to_string(),
        capability: "speech",
    })
}
