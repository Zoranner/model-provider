//! 库级错误类型

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("unknown provider name: {0}")]
    UnknownProvider(String),

    #[error("provider `{0}` is not enabled (Cargo feature or modality)")]
    ProviderDisabled(String),

    #[error("capability `{capability}` is not supported for provider `{provider}`")]
    Unsupported {
        provider: String,
        capability: &'static str,
    },

    #[error("missing required config: {0}")]
    MissingConfig(&'static str),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error("failed to parse API response: {0}")]
    Parse(String),

    #[error("API response missing expected field: {0}")]
    MissingField(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;
