use super::*;
use crate::chat::{ChatChunk, ToolChoice, ToolDefinition};
use crate::config::Provider;
use crate::error::Error;
use futures::StreamExt;
use wiremock::matchers::{method, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_config(server: &MockServer) -> ProviderConfig {
    ProviderConfig::new(
        Provider::Google,
        "AIza-test",
        format!("{}/v1beta", server.uri()),
        "gemini-2.0-flash",
    )
}

#[tokio::test]
async fn generate_content_success_returns_text() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/v1beta/models/[^/]+(:|%3A)generateContent"))
        .and(query_param("key", "AIza-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{ "text": "hi there" }],
                    "role": "model"
                }
            }]
        })))
        .mount(&server)
        .await;

    let chat = GoogleGeminiChat::new(&test_config(&server)).unwrap();
    let reply = chat.chat("hello").await.unwrap();
    assert_eq!(reply, "hi there");
}

#[tokio::test]
async fn complete_multi_turn_and_system_instruction() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/v1beta/models/[^/]+(:|%3A)generateContent"))
        .and(query_param("key", "AIza-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [{
                "content": { "parts": [{ "text": "ok" }] }
            }]
        })))
        .mount(&server)
        .await;

    let chat = GoogleGeminiChat::new(&test_config(&server)).unwrap();
    let req = ChatRequest {
        messages: vec![
            ChatMessage::system("Be brief."),
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
async fn complete_errors_when_tool_message_missing_function_name() {
    let server = MockServer::start().await;
    let chat = GoogleGeminiChat::new(&test_config(&server)).unwrap();
    let req = ChatRequest {
        messages: vec![
            ChatMessage::user("hi"),
            ChatMessage::tool("call_1", "{}"),
        ],
        ..Default::default()
    };
    let err = chat.complete(&req).await.unwrap_err();
    match err {
        Error::MissingField(name) => assert_eq!(name, "tool.name"),
        other => panic!("expected MissingField(tool.name), got {:?}", other),
    }
}

#[tokio::test]
async fn complete_with_tools_tool_choice_and_sampling_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/v1beta/models/[^/]+(:|%3A)generateContent"))
        .and(query_param("key", "AIza-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [{
                "content": { "parts": [{ "text": "ok" }] }
            }]
        })))
        .mount(&server)
        .await;

    let chat = GoogleGeminiChat::new(&test_config(&server)).unwrap();
    let req = ChatRequest {
        messages: vec![ChatMessage::user("hi")],
        tools: Some(vec![ToolDefinition::function(
            "get_weather",
            serde_json::json!({ "type": "object", "properties": {} }),
        )]),
        tool_choice: Some(ToolChoice::Tool("get_weather".into())),
        temperature: Some(0.7),
        max_tokens: Some(512),
        top_p: Some(0.85),
    };
    let r = chat.complete(&req).await.unwrap();
    assert_eq!(r.content.as_deref(), Some("ok"));
}

#[tokio::test]
async fn complete_parses_function_call() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/v1beta/models/[^/]+(:|%3A)generateContent"))
        .and(query_param("key", "AIza-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": "get_weather",
                            "args": { "city": "NYC" }
                        }
                    }]
                },
                "finishReason": "STOP"
            }]
        })))
        .mount(&server)
        .await;

    let chat = GoogleGeminiChat::new(&test_config(&server)).unwrap();
    let r = chat.complete(&ChatRequest::single_user("w")).await.unwrap();
    let tc = r.tool_calls.as_ref().unwrap();
    assert_eq!(tc[0].function.name, "get_weather");
    assert!(tc[0].function.arguments.contains("NYC"));
    assert_eq!(r.finish_reason, Some(FinishReason::ToolCalls));
}

#[tokio::test]
async fn generate_content_base_url_trailing_slash_normalized() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/v1beta/models/[^/]+(:|%3A)generateContent"))
        .and(query_param("key", "AIza-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{ "text": "ok" }]
                }
            }]
        })))
        .mount(&server)
        .await;

    let mut cfg = test_config(&server);
    cfg.base_url = format!("{}/v1beta/", server.uri());
    let chat = GoogleGeminiChat::new(&cfg).unwrap();
    assert_eq!(chat.chat("x").await.unwrap(), "ok");
}

#[tokio::test]
async fn generate_content_empty_text_parts_yields_missing_field() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/v1beta/models/[^/]+(:|%3A)generateContent"))
        .and(query_param("key", "AIza-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{ "text": "" }]
                }
            }]
        })))
        .mount(&server)
        .await;

    let chat = GoogleGeminiChat::new(&test_config(&server)).unwrap();
    let err = chat.chat("x").await.unwrap_err();
    match err {
        Error::MissingField(name) => {
            assert_eq!(name, "response content");
        }
        other => panic!("expected MissingField, got {:?}", other),
    }
}

#[tokio::test]
async fn generate_content_empty_candidates_includes_prompt_feedback() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/v1beta/models/[^/]+(:|%3A)generateContent"))
        .and(query_param("key", "AIza-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [],
            "promptFeedback": { "blockReason": "BLOCK_REASON_UNSPECIFIED" }
        })))
        .mount(&server)
        .await;

    let chat = GoogleGeminiChat::new(&test_config(&server)).unwrap();
    let err = chat.chat("x").await.unwrap_err();
    match err {
        Error::Parse(msg) => {
            assert!(
                msg.contains("no candidates") && msg.contains("blockReason"),
                "unexpected message: {msg}"
            );
        }
        other => panic!("expected Parse, got {:?}", other),
    }
}

#[tokio::test]
async fn generate_content_non_success_maps_to_api_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/v1beta/models/[^/]+(:|%3A)generateContent"))
        .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
        .mount(&server)
        .await;

    let chat = GoogleGeminiChat::new(&test_config(&server)).unwrap();
    let err = chat.chat("x").await.unwrap_err();
    match err {
        Error::Api { status, message } => {
            assert_eq!(status, 403);
            assert_eq!(message, "forbidden");
        }
        other => panic!("expected Api, got {:?}", other),
    }
}

#[tokio::test]
async fn stream_generate_content_yields_text_chunk() {
    let server = MockServer::start().await;
    let sse_body = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hi\"}]}}]}\n\n";
    Mock::given(method("POST"))
        .and(path_regex(r"/v1beta/models/[^/]+(:|%3A)streamGenerateContent"))
        .and(query_param("key", "AIza-test"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&server)
        .await;

    let chat = GoogleGeminiChat::new(&test_config(&server)).unwrap();
    let mut stream = chat.chat_stream("hello").await.unwrap();
    let chunk = stream.next().await.unwrap().unwrap();
    assert_eq!(chunk, ChatChunk::delta("Hi"));
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn stream_maps_finish_reason_tool_calls_when_function_call_with_stop() {
    let server = MockServer::start().await;
    let payload = serde_json::json!({
        "candidates": [{
            "content": { "parts": [{ "functionCall": { "name": "fn", "args": { "a": 1 } } }] },
            "finishReason": "STOP"
        }]
    });
    let sse_body = format!("data: {payload}\n\n");
    Mock::given(method("POST"))
        .and(path_regex(r"/v1beta/models/[^/]+(:|%3A)streamGenerateContent"))
        .and(query_param("key", "AIza-test"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&server)
        .await;

    let chat = GoogleGeminiChat::new(&test_config(&server)).unwrap();
    let mut stream = chat
        .complete_stream(&ChatRequest::single_user("x"))
        .await
        .unwrap();
    let chunk = stream.next().await.unwrap().unwrap();
    assert!(chunk.tool_call_deltas.is_some());
    assert_eq!(chunk.finish_reason, Some(FinishReason::ToolCalls));
}

#[tokio::test]
async fn stream_yields_function_call_delta() {
    let server = MockServer::start().await;
    let payload = serde_json::json!({"candidates":[{"content":{"parts":[{"functionCall":{"name":"fn","args":{"a":1}}}]}}]});
    let sse_body = format!("data: {payload}\n\n");
    Mock::given(method("POST"))
        .and(path_regex(r"/v1beta/models/[^/]+(:|%3A)streamGenerateContent"))
        .and(query_param("key", "AIza-test"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&server)
        .await;

    let chat = GoogleGeminiChat::new(&test_config(&server)).unwrap();
    let mut stream = chat.complete_stream(&ChatRequest::single_user("x")).await.unwrap();
    let chunk = stream.next().await.unwrap().unwrap();
    assert!(chunk.tool_call_deltas.is_some());
    let d = &chunk.tool_call_deltas.as_ref().unwrap()[0];
    assert_eq!(d.function_name.as_deref(), Some("fn"));
    assert!(d.function_arguments.as_ref().unwrap().contains("1"));
}
