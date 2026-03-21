//! 库级错误类型。
//!
//! # `ProviderDisabled` 与 `Unsupported`
//!
//! **`ProviderDisabled`**：当前编译配置下该厂商或模态未启用（Cargo feature 组合不满足），或例如启了 `embed` 但未启 `zhipu` 时仍选择智谱，工厂在 `match` 中落到禁用分支。
//!
//! **`Unsupported`**：对应模态的工厂已编译，但该厂商在该能力上**没有实现**（例如未支持文生图的厂商在 `create_image_provider` 中），或占位能力（如 `audio` 工厂尚未接任何远端）。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("unknown provider name: {0}")]
    UnknownProvider(String),

    /// 厂商或模态未在 Cargo feature 中启用，或该组合在工厂 `match` 中不可用。
    #[error("provider `{0}` is not enabled (Cargo feature or modality)")]
    ProviderDisabled(String),

    /// 该厂商在此模态下无实现，或能力仍为占位（如语音工厂）。
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
