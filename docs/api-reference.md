# API 参考

本文档描述 model-provider 的 Rust 公共 API。

本地生成完整文档：`cargo doc --all-features --no-deps --open`

## 目录

- [配置](#配置)
- [工厂函数](#工厂函数)
- [能力 Trait](#能力-trait)
- [流式对话](#流式对话)
- [错误处理](#错误处理)
- [能力矩阵](#能力矩阵)

---

## 配置

### ProviderConfig

所有模态共用一份配置：

```rust
let mut cfg = ProviderConfig::new(
    Provider::OpenAI,                              // 厂商
    "sk-xxx".to_string(),                          // API Key
    "https://api.openai.com/v1".to_string(),       // 网关地址
    "gpt-4o-mini".to_string(),                     // 模型名称
);
cfg.dimension = Some(1536);  // embed 必填
cfg.timeout = Some(Duration::from_secs(30));  // 可选
```

| 字段 | 类型 | 说明 |
|:---|:---|:---|
| `provider` | `Provider` | 厂商枚举 |
| `api_key` | `String` | API 密钥 |
| `base_url` | `String` | API 网关地址 |
| `model` | `String` | 模型名称（原样透传） |
| `dimension` | `Option<usize>` | 向量维度（embed 必填） |
| `timeout` | `Option<Duration>` | 请求超时 |

### Provider

厂商枚举，`#[non_exhaustive]`，后续可能扩展：

| 值 | 说明 |
|:---|:---|
| `OpenAI` | OpenAI 官方及兼容网关 |
| `Anthropic` | Anthropic Messages |
| `Google` | Google Gemini |
| `Aliyun` | 阿里云 DashScope |
| `Ollama` | 本地 Ollama |
| `Zhipu` | 智谱 GLM |

支持 `FromStr` 解析字符串（不区分大小写）：

```rust
let provider: Provider = "openai".parse()?;  // OK
let provider: Provider = "Aliyun".parse()?;  // OK
let provider: Provider = "unknown".parse()?; // Err(UnknownProvider)
```

---

## 工厂函数

根据配置创建对应能力的 provider 实例。

### create_chat_provider

```rust
fn create_chat_provider(cfg: &ProviderConfig) -> Result<Box<dyn ChatProvider>>
```

创建对话 provider。需要启用 `chat` feature + 对应厂商 feature。支持非流式（`chat`）与流式（`chat_stream`）两种调用方式。

### create_embed_provider

```rust
fn create_embed_provider(cfg: &ProviderConfig) -> Result<Box<dyn EmbedProvider>>
```

创建向量 provider。需要启用 `embed` feature + 对应厂商 feature。**配置必须设置 `dimension`**。

### create_rerank_provider

```rust
fn create_rerank_provider(cfg: &ProviderConfig) -> Result<Box<dyn RerankProvider>>
```

创建重排序 provider。需要启用 `rerank` feature。仅阿里云和智谱支持。

### create_image_provider

```rust
fn create_image_provider(cfg: &ProviderConfig) -> Result<Box<dyn ImageProvider>>
```

创建文生图 provider。需要启用 `image` feature + 对应厂商 feature。仅 OpenAI 和阿里云支持。

### create_transcription_provider / create_speech_provider

```rust
fn create_transcription_provider(cfg: &ProviderConfig) -> Result<Box<dyn TranscriptionProvider>>
fn create_speech_provider(cfg: &ProviderConfig) -> Result<Box<dyn SpeechProvider>>
```

语音能力占位，当前始终返回 `Unsupported`。

---

## 能力 Trait

### ChatProvider

```rust
#[async_trait]
pub trait ChatProvider: Send + Sync {
    async fn chat(&self, prompt: &str) -> Result<String>;
    async fn chat_stream(&self, prompt: &str) -> Result<ChatStream>;
}
```

- **`chat`**：单轮对话，返回完整文本。
- **`chat_stream`**：单轮流式补全，返回 `ChatStream`。使用 `futures::StreamExt` 驱动。

**实现约定**：
- 单条用户消息
- `temperature` 固定为 `0.2`
- 流式请求体含 `stream: true`（OpenAI 兼容、Anthropic）；Gemini 使用 `streamGenerateContent` 端点

### EmbedProvider

```rust
#[async_trait]
pub trait EmbedProvider: Send + Sync {
    async fn encode(&self, text: &str) -> Result<Vec<f64>>;
    async fn encode_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f64>>>;
    fn dimension(&self) -> usize;
}
```

文本向量化。

**实现约定**：
- 文本会做首尾空白裁剪和连续空白折叠
- 返回向量长度等于配置的 `dimension`
- 批量请求可能合并为单次 HTTP 调用（视厂商而定）

### RerankProvider

```rust
pub struct RerankItem {
    pub index: usize,   // 原文档在输入中的索引
    pub score: f64,     // 相关性分数
}

#[async_trait]
pub trait RerankProvider: Send + Sync {
    async fn rerank(
        &self,
        query: &str,
        documents: &[&str],
        top_n: Option<usize>,
    ) -> Result<Vec<RerankItem>>;
}
```

文档重排序，返回按相关性降序排列的结果。

### ImageProvider

```rust
pub enum ImageSize {
    Square512,      // 512×512
    Square1024,     // 1024×1024
    Landscape,      // 1792×1024
    Portrait,       // 1024×1792
}

pub enum ImageOutput {
    Url(String),        // 图片 URL
    Bytes(Vec<u8>),     // 图片二进制数据
}

#[async_trait]
pub trait ImageProvider: Send + Sync {
    async fn generate(&self, prompt: &str, size: ImageSize) -> Result<ImageOutput>;
}
```

文生图。

### TranscriptionProvider / SpeechProvider

```rust
pub enum AudioFormat {
    Wav, Mp3, Ogg, Flac,
}

#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    async fn transcribe(&self, audio: &[u8], format: AudioFormat) -> Result<String>;
}

#[async_trait]
pub trait SpeechProvider: Send + Sync {
    async fn synthesize(&self, text: &str, voice: &str) -> Result<Vec<u8>>;
}
```

语音能力占位，暂无实现。

---

## 流式对话

### 类型定义

```rust
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatChunk>> + Send>>;

pub struct ChatChunk {
    pub delta: Option<String>,
    pub finish_reason: Option<FinishReason>,
}

pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
}
```

### 使用示例

```rust
use futures::StreamExt;
use model_provider::{create_chat_provider, Provider, ProviderConfig};

let cfg = ProviderConfig::new(
    Provider::OpenAI,
    std::env::var("OPENAI_API_KEY")?,
    "https://api.openai.com/v1",
    "gpt-4o-mini",
);
let chat = create_chat_provider(&cfg)?;

let mut stream = chat.chat_stream("介绍一下 Rust").await?;
while let Some(item) = stream.next().await {
    let chunk = item?;
    if let Some(text) = chunk.delta {
        print!("{text}");
    }
    if let Some(reason) = chunk.finish_reason {
        eprintln!("\n[结束: {:?}]", reason);
    }
}
```

### 各厂商实现差异

| 厂商 | 端点 | SSE 格式 |
|:---|:---|:---|
| OpenAI / 阿里云 / Ollama / 智谱 | `POST …/chat/completions` + `stream: true` | `data: {...}`，结束 `data: [DONE]` |
| Anthropic | `POST …/messages` + `stream: true` | `event: content_block_delta` 等事件类型 |
| Google | `POST …/models/{model}:streamGenerateContent` | `data: {...}` |

---

## 错误处理

### Error 枚举

```rust
pub enum Error {
    UnknownProvider { name: String },
    ProviderDisabled { provider: String, capability: String },
    Unsupported { provider: String, capability: String },
    MissingConfig { field: String },
    Api { status: u16, message: String },
    Http { source: reqwest::Error },
    Parse { message: String },
    MissingField { field: String },
}
```

### 错误类型区分

| 错误 | 含义 | 典型场景 |
|:---|:---|:---|
| `UnknownProvider` | 无法识别的厂商名 | `FromStr` 解析失败 |
| `ProviderDisabled` | 厂商 feature 未启用 | 用了 `Aliyun` 但没开 `aliyun` feature |
| `Unsupported` | 厂商不支持该能力 | `OpenAI` + `rerank` |
| `MissingConfig` | 缺少必要配置 | embed 没设置 `dimension` |
| `Api` | 远端返回错误 | HTTP 非 2xx |
| `Http` | 网络层错误 | 连接超时、DNS 失败 |
| `Parse` | JSON 解析失败 | 响应结构异常 |
| `MissingField` | 响应缺字段 | 预期字段不存在 |

### ProviderDisabled vs Unsupported

这是两个容易混淆的错误：

**ProviderDisabled** — 编译时没启用对应 feature
```rust
// Cargo.toml 只写了 features = ["chat"]
let cfg = ProviderConfig::new(Provider::Aliyun, ...);
create_chat_provider(&cfg)?;  // ProviderDisabled: 没开 aliyun feature
```

**Unsupported** — 厂商本身不支持该能力
```rust
// Cargo.toml 写了 features = ["openai", "rerank"]
let cfg = ProviderConfig::new(Provider::OpenAI, ...);
create_rerank_provider(&cfg)?;  // Unsupported: OpenAI 没有 rerank
```

---

## 能力矩阵

| 能力 | Trait | 工厂函数 | OpenAI | Anthropic | Google | 阿里云 | Ollama | 智谱 |
|:---|:---|:---|:---:|:---:|:---:|:---:|:---:|:---:|
| Chat | `ChatProvider`（含流式） | `create_chat_provider` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Embed | `EmbedProvider` | `create_embed_provider` | ✅ | — | ✅ | ✅ | ✅ | ✅ |
| Rerank | `RerankProvider` | `create_rerank_provider` | — | — | — | ✅ | — | ✅ |
| Image | `ImageProvider` | `create_image_provider` | ✅ | — | — | ✅ | — | — |
| Audio | `TranscriptionProvider` / `SpeechProvider` | `create_*_provider` | — | — | — | — | — | — |

**图例**：✅ 已实现 | — 不支持或未实现

**Chat** 同时提供非流式（`chat`）与流式（`chat_stream`），见[流式对话](#流式对话)。
