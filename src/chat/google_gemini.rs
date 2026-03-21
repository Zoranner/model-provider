//! **Google Gemini**（Generative Language API）单轮对话：`POST …/models/{model}:generateContent`，API Key 经 query 参数 `key` 传递。
//!
//! 参考 [Gemini API 文档](https://ai.google.dev/api/rest/v1beta/models.generateContent)：`base_url` 为含版本前缀的 REST 根，例如 `https://generativelanguage.googleapis.com/v1beta`（实现会在去掉尾部 `/` 后拼接 `/models/{model}:generateContent`）。`model` 取自 [`ProviderConfig::model`]（如 `gemini-2.0-flash`），不在此 crate 内校验名称是否被当前密钥或区域支持。
//!
//! 鉴权：**不使用** `Authorization: Bearer`；`api_key` 作为 **`key`** query 参数附加在 URL 上（与 AI Studio / 多数 REST 示例一致）。**空字符串仍会原样发出**，是否被网关拒绝由上游决定。
//!
//! 请求体与官方 REST 示例一致：`contents` 为单条仅含 `parts`（`text` 为 `prompt`）；[`Content.role`](https://ai.google.dev/api/caching#Content) 在文档中为 **可选**，单轮示例常省略。另含 `generationConfig.temperature`（固定 `0.2`）。若 prompt 被安全策略拦截，官方可能返回 **HTTP 200 且 `candidates` 为空**，此时应查看 `promptFeedback`；本实现会在该情况下返回 [`Error::Parse`] 并带入 `promptFeedback` 摘要。成功时从 `candidates[0].content.parts` 拼接各 `text`；若无文本则 [`Error::MissingField`]。

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
struct GenerateContentRequest<'a> {
    contents: Vec<GeminiContent<'a>>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfig,
}

/// 与官方单轮 curl 示例一致：仅 `parts`，不发送可选字段 `role`（见 [Content](https://ai.google.dev/api/caching#Content)）。
#[derive(Debug, Serialize)]
struct GeminiContent<'a> {
    parts: Vec<GeminiPart<'a>>,
}

#[derive(Debug, Serialize)]
struct GeminiPart<'a> {
    text: &'a str,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    temperature: f32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    prompt_feedback: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiMessageContent,
}

#[derive(Debug, Deserialize)]
struct GeminiMessageContent {
    parts: Vec<GeminiPartOut>,
}

#[derive(Debug, Deserialize)]
struct GeminiPartOut {
    text: Option<String>,
}

pub(crate) struct GoogleGeminiChat {
    client: HttpClient,
    api_key: String,
    model: String,
    base_url: String,
}

impl GoogleGeminiChat {
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

    fn extract_text(response: GenerateContentResponse) -> Result<String> {
        if response.candidates.is_empty() {
            let hint = response
                .prompt_feedback
                .as_ref()
                .map(Value::to_string)
                .unwrap_or_else(|| "empty candidates, no promptFeedback".to_string());
            return Err(Error::Parse(format!(
                "Gemini generateContent returned no candidates (check promptFeedback): {hint}"
            )));
        }

        let candidate = response
            .candidates
            .into_iter()
            .next()
            .ok_or(Error::MissingField("candidates[0]"))?;
        let mut out = String::new();
        for part in candidate.content.parts {
            if let Some(t) = part.text {
                out.push_str(&t);
            }
        }
        if out.is_empty() {
            return Err(Error::MissingField("candidates[0].content.parts[text]"));
        }
        Ok(out)
    }
}

#[async_trait]
impl ChatProvider for GoogleGeminiChat {
    async fn chat(&self, prompt: &str) -> Result<String> {
        let request = GenerateContentRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart { text: prompt }],
            }],
            generation_config: GeminiGenerationConfig { temperature: 0.2 },
        };

        let base = self.base_url.trim_end_matches('/');
        let url = format!("{}/models/{}:generateContent", base, self.model);
        let query = [("key", self.api_key.as_str())];

        let body: GenerateContentResponse = self
            .client
            .post_json_query(&url, &query, &request, |s| s)
            .await?;

        Self::extract_text(body)
    }

    async fn chat_stream(&self, prompt: &str) -> Result<ChatStream> {
        let request = GenerateContentRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart { text: prompt }],
            }],
            generation_config: GeminiGenerationConfig { temperature: 0.2 },
        };
        let base = self.base_url.trim_end_matches('/');
        let url = format!("{}/models/{}:streamGenerateContent", base, self.model);
        let query = [("key", self.api_key.as_str())];
        let sse = self
            .client
            .post_json_query_sse(&url, &query, &request, |s| s)
            .await?;
        Ok(Box::pin(
            sse.filter_map(|item| ready(google_sse_item_to_chunk(item))),
        ))
    }
}

fn google_sse_item_to_chunk(item: Result<SseEvent>) -> Option<Result<ChatChunk>> {
    match item {
        Err(e) => Some(Err(e)),
        Ok(ev) => google_parse_sse_event(ev),
    }
}

fn google_parse_sse_event(ev: SseEvent) -> Option<Result<ChatChunk>> {
    let data = ev.data.trim();
    if data.is_empty() {
        return None;
    }
    let v: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => return Some(Err(Error::Parse(e.to_string()))),
    };

    let candidates = v.get("candidates").and_then(|c| c.as_array())?;

    if candidates.is_empty() {
        if v.get("promptFeedback").is_some() {
            let hint = v
                .get("promptFeedback")
                .map(|p| p.to_string())
                .unwrap_or_default();
            return Some(Err(Error::Parse(format!(
                "Gemini streamGenerateContent returned no candidates: {hint}"
            ))));
        }
        return None;
    }

    let c0 = &candidates[0];
    let mut text = String::new();
    if let Some(parts) = c0
        .get("content")
        .and_then(|x| x.get("parts"))
        .and_then(|p| p.as_array())
    {
        for p in parts {
            if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                text.push_str(t);
            }
        }
    }

    let finish = c0
        .get("finishReason")
        .and_then(|f| f.as_str())
        .and_then(map_gemini_finish_reason);

    if text.is_empty() && finish.is_none() {
        return None;
    }

    let delta = if text.is_empty() { None } else { Some(text) };

    Some(Ok(ChatChunk {
        delta,
        finish_reason: finish,
    }))
}

fn map_gemini_finish_reason(s: &str) -> Option<FinishReason> {
    match s {
        "STOP" | "FINISH_REASON_STOP" => Some(FinishReason::Stop),
        "MAX_TOKENS" | "FINISH_REASON_MAX_TOKENS" => Some(FinishReason::Length),
        "SAFETY" | "RECITATION" | "OTHER" => Some(FinishReason::ContentFilter),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::ChatChunk;
    use crate::config::Provider;
    use futures::StreamExt;
    use wiremock::matchers::{body_json, method, path, query_param};
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
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
            .and(query_param("key", "AIza-test"))
            .and(body_json(serde_json::json!({
                "contents": [{ "parts": [{ "text": "hello" }] }],
                "generationConfig": { "temperature": 0.2 }
            })))
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
    async fn generate_content_base_url_trailing_slash_normalized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
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
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
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
                assert_eq!(name, "candidates[0].content.parts[text]");
            }
            other => panic!("expected MissingField, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn generate_content_empty_candidates_includes_prompt_feedback() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
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
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
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
            .and(path(
                "/v1beta/models/gemini-2.0-flash:streamGenerateContent",
            ))
            .and(query_param("key", "AIza-test"))
            .and(body_json(serde_json::json!({
                "contents": [{ "parts": [{ "text": "hello" }] }],
                "generationConfig": { "temperature": 0.2 }
            })))
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
}
