# model-provider

**一个配置，切换多家 AI 云服务。**

面向 Rust 的多厂商 AI 客户端，用统一的 trait 和工厂模式调用 OpenAI、Anthropic、Google Gemini、阿里云、Ollama、智谱等平台。换厂商只需改配置，不必重写调用代码。

[文档](docs/README.md) · [API 参考](docs/api-reference.md) · [HTTP 端点](docs/http-endpoints.md)

## 为什么选它

- **统一接口** — `ChatProvider`、`EmbedProvider` 等 trait 屏蔽厂商差异，你的业务代码不用关心底层是哪家 API
- **灵活切换** — 同一套代码，改个 `ProviderConfig` 就能从 OpenAI 切到阿里云或本地 Ollama
- **按需编译** — 厂商和模态都是 Cargo feature，只用你需要的，不拉多余的依赖
- **清晰的错误** — `ProviderDisabled`（没启用 feature）vs `Unsupported`（厂商不支持该能力）vs `Api`（远端返回错误），排查一目了然
- **OpenAI 兼容优先** — 多数厂商走兼容路径，减少适配层厚度；不兼容的（如 Anthropic Messages、Gemini）单独实现并文档化
- **流式支持** — Chat 支持非流式与 SSE 流式（`chat_stream`），统一 `ChatChunk` / `FinishReason` 抽象

## 快速开始

### 添加依赖

```toml
[dependencies]
model-provider = { version = "0.2", features = ["openai", "chat", "embed"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

默认已包含 `openai` + `chat` + `embed`。需要其他厂商或能力时调整 feature：

```toml
model-provider = { version = "0.2", features = ["aliyun", "chat", "embed", "rerank"] }
```

### 最小示例

```rust
use model_provider::{
    create_chat_provider, create_embed_provider, Provider, ProviderConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 一份配置，指定厂商、密钥、网关、模型
    let mut cfg = ProviderConfig::new(
        Provider::OpenAI,
        std::env::var("OPENAI_API_KEY")?,
        "https://api.openai.com/v1",
        "gpt-4o-mini",
    );
    cfg.dimension = Some(1536); // embed 必填

    // 对话
    let chat = create_chat_provider(&cfg)?;
    let reply = chat.chat("用一句话介绍 Rust").await?;
    println!("{reply}");

    // 向量
    let embed = create_embed_provider(&cfg)?;
    let vec = embed.encode("hello world").await?;
    println!("向量维度: {}", vec.len());

    Ok(())
}
```

### 流式对话

```rust
use futures::StreamExt;
use model_provider::{create_chat_provider, Provider, ProviderConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = ProviderConfig::new(
        Provider::OpenAI,
        std::env::var("OPENAI_API_KEY")?,
        "https://api.openai.com/v1",
        "gpt-4o-mini",
    );

    let chat = create_chat_provider(&cfg)?;
    let mut stream = chat.chat_stream("讲一个笑话").await?;

    while let Some(item) = stream.next().await {
        let chunk = item?;
        if let Some(text) = chunk.delta {
            print!("{text}");
        }
        if let Some(reason) = chunk.finish_reason {
            eprintln!("\n[结束: {reason:?}]");
        }
    }
    println!();
    Ok(())
}
```

### 换个厂商

把 `Provider::OpenAI` 改成 `Provider::Aliyun`，`base_url` 换成阿里云网关，其他代码不动：

```rust
let cfg = ProviderConfig::new(
    Provider::Aliyun,
    std::env::var("DASHSCOPE_API_KEY")?,
    "https://dashscope.aliyuncs.com/compatible-mode/v1",
    "qwen-turbo",
);
```

## 支持的厂商与能力

| 厂商 | Chat | Embed | Rerank | Image |
|:---|:---:|:---:|:---:|:---:|
| OpenAI | ✅ | ✅ | — | ✅ |
| Anthropic | ✅ | — | — | — |
| Google Gemini | ✅ | ✅ | — | — |
| 阿里云 DashScope | ✅ | ✅ | ✅ | ✅ |
| Ollama | ✅ | ✅ | — | — |
| 智谱 | ✅ | ✅ | ✅ | — |

**Chat** 同时提供非流式（`chat`）与流式（`chat_stream`，SSE）。示例：`examples/stream_chat.rs`。

## 配置参考

`ProviderConfig` 包含以下字段：

| 字段 | 说明 | 必填 |
|:---|:---|:---:|
| `provider` | 厂商枚举值 | ✅ |
| `api_key` | API 密钥 | ✅ |
| `base_url` | API 网关地址 | ✅ |
| `model` | 模型名称（原样透传，不校验） | ✅ |
| `dimension` | 向量维度（embed 必填） | embed 时 ✅ |
| `timeout` | 请求超时（覆盖默认值） | — |

### 默认网关与鉴权

#### Chat

| 厂商 | `provider` | 默认 `base_url` | 鉴权方式 |
|:---|:---|:---|:---|
| OpenAI | `OpenAI` | `https://api.openai.com/v1` | `Authorization: Bearer` |
| 阿里云 | `Aliyun` | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `Authorization: Bearer` |
| Ollama | `Ollama` | `http://127.0.0.1:11434/v1` | Bearer（本地可空） |
| 智谱 | `Zhipu` | `https://open.bigmodel.cn/api/paas/v4` | `Authorization: Bearer` |
| Anthropic | `Anthropic` | `https://api.anthropic.com/v1` | `x-api-key` + `anthropic-version` |
| Google | `Google` | `https://generativelanguage.googleapis.com/v1beta` | URL query `key=` |

#### Embed

| 厂商 | `provider` | 默认 `base_url` | 备注 |
|:---|:---|:---|:---|
| OpenAI | `OpenAI` | `https://api.openai.com/v1` | 请求含 `dimensions` |
| 阿里云 | `Aliyun` | `https://dashscope.aliyuncs.com/compatible-mode/v1` | OpenAI 兼容 |
| Ollama | `Ollama` | `http://127.0.0.1:11434/v1` | OpenAI 兼容 |
| 智谱 | `Zhipu` | `https://open.bigmodel.cn/api/paas/v4` | 无 `dimensions` 字段 |
| Google | `Google` | `https://generativelanguage.googleapis.com/v1beta` | `outputDimensionality` |

#### Rerank

| 厂商 | `provider` | 默认 `base_url` | HTTP 路径 |
|:---|:---|:---|:---|
| 阿里云 | `Aliyun` | `https://dashscope.aliyuncs.com/api/v1` | `POST …/reranks` |
| 智谱 | `Zhipu` | `https://open.bigmodel.cn/api/paas/v4` | `POST …/rerank` |

#### Image

| 厂商 | `provider` | 默认 `base_url` | HTTP 路径 |
|:---|:---|:---|:---|
| OpenAI | `OpenAI` | `https://api.openai.com/v1` | `POST …/images/generations` |
| 阿里云 | `Aliyun` | `https://dashscope.aliyuncs.com/api/v1` | `POST …/services/aigc/multimodal-generation/generation` |

完整的 HTTP 字段说明见 [HTTP 端点](docs/http-endpoints.md)。

## 更多链接

- [API 参考](docs/api-reference.md) — Rust trait、工厂函数、错误处理
- [HTTP 端点](docs/http-endpoints.md) — 各厂商的请求/响应格式
- [设计准则](docs/design-guidelines.md) — 库的架构原则
- [贡献指南](docs/contributing.md) — 参与开发

## 许可证

[MIT](LICENSE)
