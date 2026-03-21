//! Provider 与连接配置

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

/// 已支持的厂商（`#[non_exhaustive]` 便于后续扩展）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Provider {
    OpenAI,
    Aliyun,
    Ollama,
    Zhipu,
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Provider::OpenAI => "openai",
            Provider::Aliyun => "aliyun",
            Provider::Ollama => "ollama",
            Provider::Zhipu => "zhipu",
        };
        f.write_str(s)
    }
}

impl FromStr for Provider {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "openai" => Ok(Provider::OpenAI),
            "aliyun" => Ok(Provider::Aliyun),
            "ollama" => Ok(Provider::Ollama),
            "zhipu" => Ok(Provider::Zhipu),
            other => Err(crate::error::Error::UnknownProvider(other.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub provider: Provider,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub dimension: Option<usize>,
    /// 覆盖该次请求使用的 HTTP 超时；`None` 表示由各模态默认值决定
    pub timeout: Option<Duration>,
}

impl ProviderConfig {
    pub fn new(
        provider: Provider,
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            api_key: api_key.into(),
            base_url: base_url.into(),
            model: model.into(),
            dimension: None,
            timeout: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Provider;
    use std::str::FromStr;

    #[test]
    fn provider_from_str_case_insensitive() {
        assert_eq!(Provider::from_str("openai").unwrap(), Provider::OpenAI);
        assert_eq!(Provider::from_str("Aliyun").unwrap(), Provider::Aliyun);
    }

    #[test]
    fn provider_from_str_unknown() {
        assert!(Provider::from_str("unknown").is_err());
    }
}
