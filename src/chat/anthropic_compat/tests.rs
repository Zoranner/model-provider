use super::*;
use crate::chat::{ChatChunk, FinishReason, ToolChoice, ToolDefinition};
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
async fn messages_system_top_level_and_multi_turn() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_json(serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 4096,
            "system": "Be brief.\n\nSecond line.",
            "messages": [
                { "role": "user", "content": "hi" },
                { "role": "assistant", "content": "hey" },
                { "role": "user", "content": "bye" }
            ],
            "temperature": 0.2,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "text", "text": "ok" }],
            "stop_reason": "end_turn"
        })))
        .mount(&server)
        .await;

    let chat = AnthropicCompatChat::new(&test_config(&server)).unwrap();
    let req = ChatRequest {
        messages: vec![
            ChatMessage::system("Be brief."),
            ChatMessage::system("Second line."),
            ChatMessage::user("hi"),
            ChatMessage::assistant("hey"),
            ChatMessage::user("bye"),
        ],
        ..Default::default()
    };
    let r = chat.complete(&req).await.unwrap();
    assert_eq!(r.content.as_deref(), Some("ok"));
}

#[tokio::test]
async fn complete_serializes_tool_choice_and_sampling() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_json(serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 512,
            "messages": [{ "role": "user", "content": "hi" }],
            "tools": [{
                "name": "get_weather",
                "input_schema": { "type": "object", "properties": {} }
            }],
            "tool_choice": { "type": "tool", "name": "get_weather" },
            "temperature": 0.7,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "text", "text": "ok" }],
            "stop_reason": "end_turn"
        })))
        .mount(&server)
        .await;

    let chat = AnthropicCompatChat::new(&test_config(&server)).unwrap();
    let req = ChatRequest {
        messages: vec![ChatMessage::user("hi")],
        tools: Some(vec![ToolDefinition::function(
            "get_weather",
            serde_json::json!({ "type": "object", "properties": {} }),
        )]),
        tool_choice: Some(ToolChoice::Tool("get_weather".into())),
        temperature: Some(0.7),
        max_tokens: Some(512),
        top_p: None,
    };
    let r = chat.complete(&req).await.unwrap();
    assert_eq!(r.content.as_deref(), Some("ok"));
}

#[tokio::test]
async fn complete_omits_tool_choice_when_none() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_json(serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 4096,
            "messages": [{ "role": "user", "content": "hi" }],
            "tools": [{
                "name": "get_weather",
                "input_schema": { "type": "object", "properties": {} }
            }],
            "temperature": 0.2,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "text", "text": "ok" }],
            "stop_reason": "end_turn"
        })))
        .mount(&server)
        .await;

    let chat = AnthropicCompatChat::new(&test_config(&server)).unwrap();
    let req = ChatRequest {
        messages: vec![ChatMessage::user("hi")],
        tools: Some(vec![ToolDefinition::function(
            "get_weather",
            serde_json::json!({ "type": "object", "properties": {} }),
        )]),
        tool_choice: Some(ToolChoice::None),
        ..Default::default()
    };
    let r = chat.complete(&req).await.unwrap();
    assert_eq!(r.content.as_deref(), Some("ok"));
}

#[tokio::test]
async fn messages_non_stream_tool_use_in_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{
                "type": "tool_use",
                "id": "tu_1",
                "name": "get_weather",
                "input": { "city": "NYC" }
            }],
            "stop_reason": "tool_use"
        })))
        .mount(&server)
        .await;

    let chat = AnthropicCompatChat::new(&test_config(&server)).unwrap();
    let r = chat.complete(&ChatRequest::single_user("w")).await.unwrap();
    assert!(r.content.is_none());
    let tc = r.tool_calls.as_ref().unwrap();
    assert_eq!(tc[0].id, "tu_1");
    assert_eq!(tc[0].function.name, "get_weather");
    assert!(tc[0].function.arguments.contains("NYC"));
    assert_eq!(r.finish_reason, Some(FinishReason::ToolCalls));
}

#[tokio::test]
async fn messages_stream_tool_use_deltas() {
    let server = MockServer::start().await;
    let sse_body = concat!(
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"fn\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"a\\\":1}\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"}}\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_json(serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 4096,
            "messages": [{ "role": "user", "content": "x" }],
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
    let mut stream = chat.complete_stream(&ChatRequest::single_user("x")).await.unwrap();
    let mut chunks = Vec::new();
    while let Some(item) = stream.next().await {
        chunks.push(item.unwrap());
    }
    assert_eq!(chunks.len(), 3);
    assert!(chunks[0].tool_call_deltas.is_some());
    assert_eq!(
        chunks[0].tool_call_deltas.as_ref().unwrap()[0].id.as_deref(),
        Some("tu_1")
    );
    assert_eq!(
        chunks[1].tool_call_deltas.as_ref().unwrap()[0]
            .function_arguments
            .as_deref(),
        Some("{\"a\":1}")
    );
    assert_eq!(chunks[2].finish_reason, Some(FinishReason::ToolCalls));
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
        Error::MissingField(name) => assert_eq!(name, "response content"),
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
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
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
