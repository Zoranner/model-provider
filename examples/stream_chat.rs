//! 流式对话示例：从环境变量读取密钥，打印增量文本与可选 tool 增量。
//!
//! ```bash
//! set OPENAI_API_KEY=sk-...
//! cargo run --example stream_chat --features openai,chat
//! ```

use futures::StreamExt;
use model_provider::{create_chat_provider, ChatRequest, Provider, ProviderConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPENAI_API_KEY")?;
    let cfg = ProviderConfig::new(
        Provider::OpenAI,
        api_key,
        "https://api.openai.com/v1",
        "gpt-4o-mini",
    );
    let chat = create_chat_provider(&cfg)?;
    let prompt = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "用一句话介绍 Rust".to_string());

    let mut stream = chat
        .complete_stream(&ChatRequest::single_user(prompt))
        .await?;
    while let Some(item) = stream.next().await {
        let chunk = item?;
        if let Some(t) = chunk.delta {
            print!("{t}");
        }
        if let Some(td) = chunk.tool_call_deltas {
            eprintln!("\n[tool_call_deltas: {td:?}]");
        }
        if let Some(r) = chunk.finish_reason {
            eprintln!("\n[finish: {r:?}]");
        }
    }
    println!();
    Ok(())
}
