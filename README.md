# model-provider

用同一套配置在 Rust 里调用多家云的 **对话**、**向量**、**重排序** API（HTTPS，默认 rustls）。按需打开 Cargo feature，用不到的厂商不会编进产物。

各能力的 Rust API 与 HTTP 约定已整理在 [docs 目录](docs/README.md)（含 [接口一览](docs/interfaces.md)）。维护与扩展本库时的设计约定见 [docs/design-guidelines.md](docs/design-guidelines.md)。

## 🚀 快速接入

在 `Cargo.toml` 里写上依赖和 feature（路径发布时改成你的实际路径或 crates.io 版本号）：

```toml
[dependencies]
model-provider = { version = "0.2", features = ["openai", "chat", "embed"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

默认已包含 `openai` + `chat` + `embed`。要用阿里云 rerank，改成例如 `features = ["aliyun", "chat", "embed", "rerank"]`。

下面示例：填好网关地址和密钥后即可调用（需在 async 运行时里执行）。

```rust
use model_provider::{
    create_chat_provider, create_embed_provider, Provider, ProviderConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = ProviderConfig::new(
        Provider::OpenAI,
        std::env::var("API_KEY")?,
        "https://api.openai.com/v1",
        "gpt-4o-mini",
    );
    cfg.dimension = Some(1536); // embed 必填

    let chat = create_chat_provider(&cfg)?;
    let reply = chat.chat("用一句话介绍 Rust").await?;
    println!("{reply}");

    let emb = create_embed_provider(&cfg)?;
    let v = emb.encode("hello").await?;
    println!("dim = {}", v.len());
    Ok(())
}
```

## 📊 各厂商支持的能力

同一能力在不同云上都是「填 `base_url` + 模型名」，由对方是否提供 OpenAI 兼容接口决定；embed 一般要设置 `dimension`（维数依模型而定）。

| 厂商 | Chat | Embed | Rerank | Image |
|:---:|:---:|:---:|:---:|:---:|
| OpenAI | ✅ | ✅ | ❌ | ✅ |
| 阿里云 | ✅ | ✅ | ✅ | ✅ 🔧 |
| Ollama | ✅ | ✅ | ❌ | ❌ |
| 智谱 | ✅ | ✅ 🔧 | ✅ | ❌ |

图例：🔧 表示「专用请求体或与 OpenAI 路径不一致」。智谱 Embed 等如此；**阿里云文生图**使用 DashScope 原生 `POST .../services/aigc/multimodal-generation/generation`，不是 `compatible-mode/v1` 下的 OpenAI 文生图路径。

图像生成：`openai` + `image` 时，`base_url` 一般为 `https://api.openai.com/v1`，走 `.../images/generations`。`aliyun` + `image` 时，`base_url` 需为 **`https://dashscope.aliyuncs.com/api/v1`**（或新加坡等地域的 `https://dashscope-intl.aliyuncs.com/api/v1`），`model` 填百炼文生图模型名（如 `qwen-image-plus`），与对话用的 `compatible-mode/v1` 网关不同。语音识别与合成仍为占位，可开 `audio` 查看类型。

## ⚙️ 常用 feature 组合

| 你想用 | `features` 示例 |
|:---|:---|
| 只要 OpenAI 对话 + 向量 | 默认不写，或 `["openai", "chat", "embed"]` |
| 全开厂商与能力 | `["full"]` 或 `["all"]` |
| 仅本地 Ollama | `["ollama", "chat", "embed"]` |
| 阿里云 rerank | 在已有 embed/chat 上再加 `aliyun` 与 `rerank` |
| OpenAI 文生图 | `["openai", "image"]`（可与 `chat`、`embed` 等组合） |
| 阿里云文生图（千问图像等） | `["aliyun", "image"]`，`base_url` 见上文 |

厂商 feature（`openai` / `aliyun` / `ollama` / `zhipu`）与模态 feature（`chat` / `embed` / `rerank` / `image` / `audio`）要同时满足才会在对应工厂里可用；配错组合会得到明确的 `Error`，而不是静默失败。

## 📜 许可证

[MIT](LICENSE)
