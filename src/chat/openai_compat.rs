//! OpenAI 兼容对话：`POST …/chat/completions`（非流式与 `stream: true` 流式）。

use async_trait::async_trait;
use futures::future::ready;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

use crate::client::HttpClient;
use crate::config::ProviderConfig;
use crate::error::{Error, Result};
use crate::sse::SseEvent;

use super::{ChatChunk, ChatProvider, ChatStream, FinishReason};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Serialize)]
struct OpenaiChatRequest {
    model: String,
    messages: Vec<OpenaiChatMessage>,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct OpenaiChatStreamRequest {
    model: String,
    messages: Vec<OpenaiChatMessage>,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenaiChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenaiChatResponse {
    choices: Vec<OpenaiChatChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenaiChatChoice {
    message: OpenaiChatMessage,
}

pub(crate) struct OpenaiCompatChat {
    client: HttpClient,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenaiCompatChat {
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
}

#[async_trait]
impl ChatProvider for OpenaiCompatChat {
    async fn chat(&self, prompt: &str) -> Result<String> {
        let request = OpenaiChatRequest {
            model: self.model.clone(),
            messages: vec![OpenaiChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            temperature: 0.2,
        };

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let chat_response: OpenaiChatResponse = self
            .client
            .post_bearer_json(&url, &self.api_key, &request, |s| s)
            .await?;

        chat_response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or(Error::MissingField("choices[0].message"))
    }

    async fn chat_stream(&self, prompt: &str) -> Result<ChatStream> {
        let request = OpenaiChatStreamRequest {
            model: self.model.clone(),
            messages: vec![OpenaiChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            temperature: 0.2,
            stream: true,
        };
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let sse = self
            .client
            .post_bearer_sse(&url, &self.api_key, &request, |s| s)
            .await?;
        Ok(Box::pin(
            sse.filter_map(|item| ready(openai_sse_item_to_chunk(item))),
        ))
    }
}

fn openai_sse_item_to_chunk(item: Result<SseEvent>) -> Option<Result<ChatChunk>> {
    match item {
        Err(e) => Some(Err(e)),
        Ok(ev) => openai_parse_sse_event(ev),
    }
}

fn openai_parse_sse_event(ev: SseEvent) -> Option<Result<ChatChunk>> {
    let data = ev.data.trim();
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    let v: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => return Some(Err(Error::Parse(e.to_string()))),
    };
    let choices = v.get("choices").and_then(|c| c.as_array())?;
    let ch = choices.first()?;
    let delta_text = ch
        .get("delta")
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str())
        .map(str::to_string);
    let finish_reason = ch
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .and_then(map_openai_finish_reason);
    if delta_text.is_none() && finish_reason.is_none() {
        return None;
    }
    Some(Ok(ChatChunk {
        delta: delta_text,
        finish_reason,
    }))
}

fn map_openai_finish_reason(s: &str) -> Option<FinishReason> {
    match s {
        "stop" | "end_turn" => Some(FinishReason::Stop),
        "length" => Some(FinishReason::Length),
        "content_filter" => Some(FinishReason::ContentFilter),
        "tool_calls" => Some(FinishReason::ToolCalls),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Provider;
    use futures::StreamExt;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(server: &MockServer) -> ProviderConfig {
        ProviderConfig::new(
            Provider::OpenAI,
            "test-key",
            server.uri().to_string(),
            "gpt-4o-mini",
        )
    }

    #[tokio::test]
    async fn chat_success_returns_assistant_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer test-key"))
            .and(body_json(serde_json::json!({
                "model": "gpt-4o-mini",
                "messages": [{ "role": "user", "content": "hello" }],
                "temperature": 0.2,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "role": "assistant", "content": "hi there" }
                }]
            })))
            .mount(&server)
            .await;

        let chat = OpenaiCompatChat::new(&test_config(&server)).unwrap();
        let reply = chat.chat("hello").await.unwrap();
        assert_eq!(reply, "hi there");
    }

    #[tokio::test]
    async fn chat_base_url_trailing_slash_normalized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "role": "assistant", "content": "ok" }
                }]
            })))
            .mount(&server)
            .await;

        let mut cfg = test_config(&server);
        cfg.base_url = format!("{}/", server.uri());
        let chat = OpenaiCompatChat::new(&cfg).unwrap();
        assert_eq!(chat.chat("x").await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn chat_empty_choices_yields_missing_field() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": []
            })))
            .mount(&server)
            .await;

        let chat = OpenaiCompatChat::new(&test_config(&server)).unwrap();
        let err = chat.chat("x").await.unwrap_err();
        match err {
            Error::MissingField(name) => assert_eq!(name, "choices[0].message"),
            other => panic!("expected MissingField, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn chat_non_success_maps_to_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("invalid key"))
            .mount(&server)
            .await;

        let chat = OpenaiCompatChat::new(&test_config(&server)).unwrap();
        let err = chat.chat("x").await.unwrap_err();
        match err {
            Error::Api { status, message } => {
                assert_eq!(status, 401);
                assert_eq!(message, "invalid key");
            }
            other => panic!("expected Api, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn chat_success_body_not_json_yields_parse() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let chat = OpenaiCompatChat::new(&test_config(&server)).unwrap();
        let err = chat.chat("x").await.unwrap_err();
        match err {
            Error::Parse(_) => {}
            other => panic!("expected Parse, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn chat_stream_yields_deltas_and_finish() {
        let server = MockServer::start().await;
        let sse_body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"he\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer test-key"))
            .and(body_json(serde_json::json!({
                "model": "gpt-4o-mini",
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

        let chat = OpenaiCompatChat::new(&test_config(&server)).unwrap();
        let mut stream = chat.chat_stream("hello").await.unwrap();
        let mut chunks = Vec::new();
        while let Some(item) = stream.next().await {
            chunks.push(item.unwrap());
        }
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].delta.as_deref(), Some("he"));
        assert!(chunks[0].finish_reason.is_none());
        assert_eq!(chunks[1].delta, None);
        assert_eq!(
            chunks[1].finish_reason,
            Some(crate::chat::FinishReason::Stop)
        );
    }

    #[tokio::test]
    async fn chat_stream_http_error_before_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate"))
            .mount(&server)
            .await;

        let chat = OpenaiCompatChat::new(&test_config(&server)).unwrap();
        let err = match chat.chat_stream("x").await {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        match err {
            Error::Api { status, message } => {
                assert_eq!(status, 429);
                assert_eq!(message, "rate");
            }
            other => panic!("expected Api, got {:?}", other),
        }
    }
}
