use super::*;
use crate::chat::{ToolChoice, ToolDefinition};
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
async fn complete_multi_turn_and_system_in_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_json(serde_json::json!({
            "model": "gpt-4o-mini",
            "messages": [
                { "role": "system", "content": "You are helpful." },
                { "role": "user", "content": "hi" },
                { "role": "assistant", "content": "hello" },
                { "role": "user", "content": "bye" }
            ],
            "temperature": 0.2,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": { "role": "assistant", "content": "see you" },
                "finish_reason": "stop"
            }]
        })))
        .mount(&server)
        .await;

    let chat = OpenaiCompatChat::new(&test_config(&server)).unwrap();
    let req = ChatRequest {
        messages: vec![
            ChatMessage::system("You are helpful."),
            ChatMessage::user("hi"),
            ChatMessage::assistant("hello"),
            ChatMessage::user("bye"),
        ],
        ..Default::default()
    };
    let r = chat.complete(&req).await.unwrap();
    assert_eq!(r.content.as_deref(), Some("see you"));
}

#[tokio::test]
async fn complete_serializes_tool_choice_and_sampling() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_json(serde_json::json!({
            "model": "gpt-4o-mini",
            "messages": [{ "role": "user", "content": "hi" }],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "parameters": { "type": "object", "properties": {} }
                }
            }],
            "tool_choice": {
                "type": "function",
                "function": { "name": "get_weather" }
            },
            "temperature": 0.7,
            "max_tokens": 512,
            "top_p": 0.9,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": { "role": "assistant", "content": "ok" }
            }]
        })))
        .mount(&server)
        .await;

    let chat = OpenaiCompatChat::new(&test_config(&server)).unwrap();
    let req = ChatRequest {
        messages: vec![ChatMessage::user("hi")],
        tools: Some(vec![ToolDefinition::function(
            "get_weather",
            serde_json::json!({ "type": "object", "properties": {} }),
        )]),
        tool_choice: Some(ToolChoice::Tool("get_weather".into())),
        temperature: Some(0.7),
        max_tokens: Some(512),
        top_p: Some(0.9),
    };
    let r = chat.complete(&req).await.unwrap();
    assert_eq!(r.content.as_deref(), Some("ok"));
}

#[tokio::test]
async fn complete_returns_tool_calls() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": "get_weather", "arguments": "{\"city\":\"NYC\"}" }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        })))
        .mount(&server)
        .await;

    let chat = OpenaiCompatChat::new(&test_config(&server)).unwrap();
    let r = chat.complete(&ChatRequest::single_user("weather?")).await.unwrap();
    assert!(r.content.is_none());
    let tc = r.tool_calls.as_ref().unwrap();
    assert_eq!(tc.len(), 1);
    assert_eq!(tc[0].id, "call_1");
    assert_eq!(tc[0].function.name, "get_weather");
    assert_eq!(tc[0].function.arguments, "{\"city\":\"NYC\"}");
    assert_eq!(r.finish_reason, Some(FinishReason::ToolCalls));
}

#[tokio::test]
async fn complete_skips_malformed_tool_calls_keeps_valid() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [
                        { "type": "function", "function": { "name": "bad", "arguments": "{}" } },
                        {
                            "id": "call_ok",
                            "type": "function",
                            "function": { "name": "good", "arguments": "{}" }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }]
        })))
        .mount(&server)
        .await;

    let chat = OpenaiCompatChat::new(&test_config(&server)).unwrap();
    let r = chat.complete(&ChatRequest::single_user("x")).await.unwrap();
    let tc = r.tool_calls.as_ref().unwrap();
    assert_eq!(tc.len(), 1);
    assert_eq!(tc[0].id, "call_ok");
    assert_eq!(tc[0].function.name, "good");
}

#[tokio::test]
async fn complete_stream_yields_tool_call_deltas() {
    let server = MockServer::start().await;
    let sse_body = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_x\",\"function\":{\"name\":\"fn\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"a\\\":1}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_json(serde_json::json!({
            "model": "gpt-4o-mini",
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

    let chat = OpenaiCompatChat::new(&test_config(&server)).unwrap();
    let mut stream = chat.complete_stream(&ChatRequest::single_user("x")).await.unwrap();
    let mut chunks = Vec::new();
    while let Some(item) = stream.next().await {
        chunks.push(item.unwrap());
    }
    assert_eq!(chunks.len(), 3);
    assert!(chunks[0].tool_call_deltas.is_some());
    let d0 = &chunks[0].tool_call_deltas.as_ref().unwrap()[0];
    assert_eq!(d0.index, 0);
    assert_eq!(d0.id.as_deref(), Some("call_x"));
    assert_eq!(d0.function_name.as_deref(), Some("fn"));
    let d1 = &chunks[1].tool_call_deltas.as_ref().unwrap()[0];
    assert_eq!(d1.function_arguments.as_deref(), Some("{\"a\":1}"));
    assert_eq!(chunks[2].finish_reason, Some(FinishReason::ToolCalls));
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
        Error::MissingField(name) => assert_eq!(name, "choices[0]"),
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
    assert_eq!(chunks[1].finish_reason, Some(FinishReason::Stop));
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
