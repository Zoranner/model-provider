//! **Anthropic Messages 兼容**对话：实现与 [Anthropic Messages API](https://docs.anthropic.com/en/api/messages) 一致的 HTTP 契约（路径、请求头、JSON 形状与成功体解析），而非仅绑定官方域名。
//!
//! 因此凡暴露**同一兼容面**的网关均可接入：除 `api.anthropic.com` 外，常见还有各类 **Coding Plan**、聚合代理、自建转发等（通常同样为 `POST …/messages`，`x-api-key` + `anthropic-version`）。将 [`ProviderConfig::base_url`] 设为该网关提供的根 URL（多含 `/v1` 前缀）即可；若某网关改动路径或头名，则超出本兼容层，需另开实现或 fork。
//!
//! 请求头：`x-api-key`（值为 [`ProviderConfig::api_key`]）、`anthropic-version`（[`ANTHROPIC_VERSION`]，与官方当前 Messages 版本头一致）、`Content-Type: application/json`。**不使用** `Authorization: Bearer`。
//!
//! `base_url` 示例：`https://api.anthropic.com/v1`；实现会 `trim_end_matches('/')` 后拼接 `/messages`。

use async_trait::async_trait;
use futures::future::ready;
use futures::StreamExt;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

use crate::client::HttpClient;
use crate::config::ProviderConfig;
use crate::error::{Error, Result};
use crate::sse::SseEvent;

use super::{ChatChunk, ChatProvider, ChatStream, FinishReason};

/// Anthropic Messages 兼容实现使用的 `anthropic-version` 请求头取值（与官方文档对齐；上游变更时需同步本常量）。
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_MAX_TOKENS: u32 = 4096;

#[derive(Debug, Serialize)]
struct MessagesRequest<'a> {
    model: String,
    max_tokens: u32,
    messages: Vec<MessageParam<'a>>,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct MessagesStreamRequest<'a> {
    model: String,
    max_tokens: u32,
    messages: Vec<MessageParam<'a>>,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct MessageParam<'a> {
    role: &'a str,
    content: &'a str,
}

pub(crate) struct AnthropicCompatChat {
    client: HttpClient,
    api_key: String,
    model: String,
    base_url: String,
}

impl AnthropicCompatChat {
    pub fn new(config: &ProviderConfig) -> Result<Self> {
        let timeout = config.timeout.unwrap_or(DEFAULT_TIMEOUT);
        let client = HttpClient::new(timeout)?;
        Ok(Self {
            client,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            base_url: config.base_url.clone(),
        })
    }

    fn extract_assistant_text(content: &[Value]) -> Result<String> {
        let mut parts = Vec::new();
        for block in content {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                    parts.push(t);
                }
            }
        }
        if parts.is_empty() {
            return Err(Error::MissingField("content[text]"));
        }
        Ok(parts.join(""))
    }
}

#[async_trait]
impl ChatProvider for AnthropicCompatChat {
    async fn chat(&self, prompt: &str) -> Result<String> {
        let request = MessagesRequest {
            model: self.model.clone(),
            max_tokens: DEFAULT_MAX_TOKENS,
            messages: vec![MessageParam {
                role: "user",
                content: prompt,
            }],
            temperature: 0.2,
        };

        let url = format!("{}/messages", self.base_url.trim_end_matches('/'));

        let headers = [
            ("x-api-key", self.api_key.as_str()),
            ("anthropic-version", ANTHROPIC_VERSION),
        ];

        let body: Value = self
            .client
            .post_json_with_headers(&url, &headers, &request, |s| s)
            .await?;

        let content = body
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or(Error::MissingField("content"))?;

        Self::extract_assistant_text(content)
    }

    async fn chat_stream(&self, prompt: &str) -> Result<ChatStream> {
        let request = MessagesStreamRequest {
            model: self.model.clone(),
            max_tokens: DEFAULT_MAX_TOKENS,
            messages: vec![MessageParam {
                role: "user",
                content: prompt,
            }],
            temperature: 0.2,
            stream: true,
        };
        let url = format!("{}/messages", self.base_url.trim_end_matches('/'));
        let headers = [
            ("x-api-key", self.api_key.as_str()),
            ("anthropic-version", ANTHROPIC_VERSION),
        ];
        let sse = self
            .client
            .post_json_with_headers_sse(&url, &headers, &request, |s| s)
            .await?;
        Ok(Box::pin(sse.filter_map(|item| {
            ready(anthropic_sse_item_to_chunk(item))
        })))
    }
}

fn anthropic_sse_item_to_chunk(item: Result<SseEvent>) -> Option<Result<ChatChunk>> {
    match item {
        Err(e) => Some(Err(e)),
        Ok(ev) => anthropic_parse_sse_event(ev),
    }
}

fn anthropic_parse_sse_event(ev: SseEvent) -> Option<Result<ChatChunk>> {
    let data = ev.data.trim();
    if data.is_empty() {
        return None;
    }
    let v: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => return Some(Err(Error::Parse(e.to_string()))),
    };

    if ev.event.as_deref() == Some("error")
        || v.get("type").and_then(|t| t.as_str()) == Some("error")
    {
        let msg = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("anthropic stream error");
        return Some(Err(Error::Api {
            status: 500,
            message: msg.to_string(),
        }));
    }

    let ty = v.get("type").and_then(|t| t.as_str())?;

    match ty {
        "content_block_delta" => {
            let delta = v.get("delta")?;
            if delta.get("type").and_then(|t| t.as_str()) == Some("text_delta") {
                let text = delta.get("text").and_then(|t| t.as_str())?;
                return Some(Ok(ChatChunk::delta(text)));
            }
            None
        }
        "message_delta" => {
            let stop = v
                .get("delta")
                .and_then(|d| d.get("stop_reason"))
                .and_then(|s| s.as_str());
            if let Some(r) = stop {
                if let Some(fr) = map_anthropic_stop_reason(r) {
                    return Some(Ok(ChatChunk {
                        delta: None,
                        finish_reason: Some(fr),
                    }));
                }
            }
            None
        }
        "message_stop" => Some(Ok(ChatChunk::finish(FinishReason::Stop))),
        _ => None,
    }
}

fn map_anthropic_stop_reason(s: &str) -> Option<FinishReason> {
    match s {
        "end_turn" | "stop_sequence" => Some(FinishReason::Stop),
        "max_tokens" => Some(FinishReason::Length),
        "tool_use" => Some(FinishReason::ToolCalls),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{ChatChunk, FinishReason};
    use crate::config::Provider;
    use futures::StreamExt;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(server: &MockServer) -> ProviderConfig {
        ProviderConfig::new(
            Provider::Anthropic,
            "sk-ant-test",
            format!("{}/v1", server.uri()),
            "claude-sonnet-4-20250514",
        )
    }

    #[tokio::test]
    async fn messages_success_returns_text_block() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-ant-test"))
            .and(header("anthropic-version", ANTHROPIC_VERSION))
            .and(body_json(serde_json::json!({
                "model": "claude-sonnet-4-20250514",
                "max_tokens": 4096,
                "messages": [{ "role": "user", "content": "hello" }],
                "temperature": 0.2,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "msg_1",
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "text", "text": "hi there" }],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": "end_turn"
            })))
            .mount(&server)
            .await;

        let chat = AnthropicCompatChat::new(&test_config(&server)).unwrap();
        let reply = chat.chat("hello").await.unwrap();
        assert_eq!(reply, "hi there");
    }

    #[tokio::test]
    async fn messages_base_url_trailing_slash_normalized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{ "type": "text", "text": "ok" }]
            })))
            .mount(&server)
            .await;

        let mut cfg = test_config(&server);
        cfg.base_url = format!("{}/v1/", server.uri());
        let chat = AnthropicCompatChat::new(&cfg).unwrap();
        assert_eq!(chat.chat("x").await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn messages_empty_text_blocks_yields_missing_field() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{ "type": "tool_use", "id": "t", "name": "x", "input": {} }]
            })))
            .mount(&server)
            .await;

        let chat = AnthropicCompatChat::new(&test_config(&server)).unwrap();
        let err = chat.chat("x").await.unwrap_err();
        match err {
            Error::MissingField(name) => assert_eq!(name, "content[text]"),
            other => panic!("expected MissingField, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn messages_non_success_maps_to_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401).set_body_string("bad key"))
            .mount(&server)
            .await;

        let chat = AnthropicCompatChat::new(&test_config(&server)).unwrap();
        let err = chat.chat("x").await.unwrap_err();
        match err {
            Error::Api { status, message } => {
                assert_eq!(status, 401);
                assert_eq!(message, "bad key");
            }
            other => panic!("expected Api, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn messages_stream_yields_text_delta_and_stop() {
        let server = MockServer::start().await;
        let sse_body = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-ant-test"))
            .and(header("anthropic-version", ANTHROPIC_VERSION))
            .and(body_json(serde_json::json!({
                "model": "claude-sonnet-4-20250514",
                "max_tokens": 4096,
                "messages": [{ "role": "user", "content": "hello" }],
                "temperature": 0.2,
                "stream": true,
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&server)
            .await;

        let chat = AnthropicCompatChat::new(&test_config(&server)).unwrap();
        let mut stream = chat.chat_stream("hello").await.unwrap();
        let mut chunks = Vec::new();
        while let Some(item) = stream.next().await {
            chunks.push(item.unwrap());
        }
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], ChatChunk::delta("Hi"));
        assert_eq!(chunks[1], ChatChunk::finish(FinishReason::Stop));
    }
}
