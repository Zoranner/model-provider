//! 语音识别与语音合成（骨架，待接入具体厂商）

use async_trait::async_trait;

use crate::config::ProviderConfig;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy)]
pub enum AudioFormat {
    Wav,
    Mp3,
    Ogg,
    Flac,
}

#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    async fn transcribe(&self, audio: &[u8], format: AudioFormat) -> Result<String>;
}

#[async_trait]
pub trait SpeechProvider: Send + Sync {
    async fn synthesize(&self, text: &str, voice: &str) -> Result<Vec<u8>>;
}

pub(crate) fn create_transcription(config: &ProviderConfig) -> Result<Box<dyn TranscriptionProvider>> {
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
