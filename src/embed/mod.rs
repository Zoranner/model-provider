//! 文本向量嵌入：非流式 JSON，默认 HTTP 超时约 30 秒（可用 [`ProviderConfig::timeout`] 覆盖）。
//!
//! # HTTP 与厂商分支
//!
//! 请求地址为 `POST {base_url}/embeddings`，`base_url` 会先 `trim_end_matches('/')` 再拼接路径。
//!
//! **`OpenAI` / `Aliyun` / `Ollama`**（启用对应厂商 feature 与 `embed`）：OpenAI 兼容请求体，含 `model`、`input`（字符串数组）、**`dimensions`**（等于配置中的 [`ProviderConfig::dimension`]，序列化进 JSON）。成功时解析 `data[].embedding`。
//!
//! **`Zhipu`**：路径仍为 `…/embeddings`，请求体仅 `model` 与 `input`，**不发送 `dimensions` 字段**；配置中的 `dimension` 仍必填，用于 [`EmbedProvider::dimension`] 返回值，且须与模型实际输出维数一致。未启用 `zhipu` feature 时选择智谱会得到 [`Error::ProviderDisabled`]。
//!
//! # 文本预处理
//!
//! [`EmbedProvider::encode`] 与 [`EmbedProvider::encode_batch`] 在组请求前会对每条文本做首尾空白裁剪与连续空白折叠。
//!
//! # 鉴权
//!
//! 与其它模态相同：`Authorization: Bearer {api_key}` 的 JSON POST。

mod openai_compat;
#[cfg(feature = "zhipu")]
mod zhipu;

use async_trait::async_trait;
use std::time::Duration;

use crate::client::HttpClient;
use crate::config::Provider;
use crate::config::ProviderConfig;
use crate::error::{Error, Result};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[async_trait]
pub trait EmbedProvider: Send + Sync {
    /// 单条文本嵌入；内部先规范化空白再调用 [`encode_batch`](Self::encode_batch)。
    async fn encode(&self, text: &str) -> Result<Vec<f32>>;
    /// 批量嵌入；顺序与 `texts` 一致，长度与 [`dimension`](Self::dimension) 由配置与上游决定。
    async fn encode_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    /// 配置维度（智谱请求体虽不含 `dimensions` 字段，仍须与模型输出一致）。
    fn dimension(&self) -> usize;
}

fn http_client(config: &ProviderConfig) -> Result<HttpClient> {
    HttpClient::new(config.timeout.unwrap_or(DEFAULT_TIMEOUT))
}

pub(crate) fn create(config: &ProviderConfig) -> Result<Box<dyn EmbedProvider>> {
    let dimension = config.dimension.ok_or(Error::MissingConfig("dimension"))?;

    #[allow(unreachable_patterns)]
    match config.provider {
        #[cfg(all(feature = "openai", feature = "embed"))]
        Provider::OpenAI => Ok(Box::new(openai_compat::OpenaiCompatEmbed::new(
            config,
            dimension,
            http_client(config)?,
        ))),
        #[cfg(all(feature = "aliyun", feature = "embed"))]
        Provider::Aliyun => Ok(Box::new(openai_compat::OpenaiCompatEmbed::new(
            config,
            dimension,
            http_client(config)?,
        ))),
        #[cfg(all(feature = "ollama", feature = "embed"))]
        Provider::Ollama => Ok(Box::new(openai_compat::OpenaiCompatEmbed::new(
            config,
            dimension,
            http_client(config)?,
        ))),
        #[cfg(all(feature = "zhipu", feature = "embed"))]
        Provider::Zhipu => Ok(Box::new(zhipu::ZhipuEmbed::new(
            config,
            dimension,
            http_client(config)?,
        ))),
        #[cfg(all(feature = "embed", not(feature = "zhipu")))]
        Provider::Zhipu => Err(Error::ProviderDisabled("zhipu".to_string())),
        p => Err(Error::ProviderDisabled(p.to_string())),
    }
}
