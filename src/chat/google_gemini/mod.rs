//! **Google Gemini**（Generative Language API）：`POST …/models/{model}:generateContent` / `streamGenerateContent`，`key` query 鉴权。
//!
//! 多轮、`systemInstruction`、`tools`（`functionDeclarations`）与 `functionCall` / `functionResponse` 映射见实现与 [Gemini API](https://ai.google.dev/api/rest)。

use async_trait::async_trait;
use futures::future::ready;
use futures::StreamExt;
use serde_json::{json, Value};
use std::time::Duration;

use crate::client::HttpClient;
use crate::config::ProviderConfig;
use crate::error::{Error, Result};
use crate::sse::SseEvent;

use super::{
    ChatChunk, ChatMessage, ChatProvider, ChatRequest, ChatResponse, ChatStream, FinishReason,
    FunctionCallResult, Role, ToolCall, ToolCallDelta, ToolChoice,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_TEMPERATURE: f32 = 0.2;

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

    fn build_request_json(&self, request: &ChatRequest) -> Result<Value> {
        let (system_instruction, contents) = gemini_contents_from_chat(request)?;
        let mut generation_config = json!({
            "temperature": request.temperature.unwrap_or(DEFAULT_TEMPERATURE),
        });
        if let Some(m) = request.max_tokens {
            generation_config["maxOutputTokens"] = json!(m);
        }
        if let Some(p) = request.top_p {
            generation_config["topP"] = json!(p);
        }
        let mut body = json!({
            "contents": contents,
            "generationConfig": generation_config,
        });
        if let Some(si) = system_instruction {
            body["systemInstruction"] = si;
        }
        if let Some(tools) = request.tools.as_ref().filter(|t| !t.is_empty()) {
            let decls: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.function.name,
                        "description": t.function.description,
                        "parameters": t.function.parameters.clone(),
                    })
                })
                .collect();
            body["tools"] = json!([{ "functionDeclarations": decls }]);
            if let Some(tc) = &request.tool_choice {
                body["toolConfig"] = json!({
                    "functionCallingConfig": gemini_function_calling_config(tc)
                });
            }
        }
        Ok(body)
    }
}

fn gemini_function_calling_config(tc: &ToolChoice) -> Value {
    match tc {
        ToolChoice::None => json!({ "mode": "NONE" }),
        ToolChoice::Auto => json!({ "mode": "AUTO" }),
        ToolChoice::Required => json!({ "mode": "ANY" }),
        ToolChoice::Tool(name) => json!({
            "mode": "ANY",
            "allowedFunctionNames": [name]
        }),
    }
}

fn gemini_contents_from_chat(request: &ChatRequest) -> Result<(Option<Value>, Vec<Value>)> {
    let mut system_parts: Vec<String> = Vec::new();
    let mut contents: Vec<Value> = Vec::new();
    for m in &request.messages {
        match m.role {
            Role::System => {
                if let Some(c) = &m.content {
                    if !c.is_empty() {
                        system_parts.push(c.clone());
                    }
                }
            }
            Role::User => {
                let text = m
                    .content
                    .clone()
                    .ok_or(Error::MissingField("user.content"))?;
                contents.push(json!({
                    "role": "user",
                    "parts": [{ "text": text }]
                }));
            }
            Role::Assistant => {
                let parts = gemini_model_parts(m)?;
                contents.push(json!({
                    "role": "model",
                    "parts": parts
                }));
            }
            Role::Tool => {
                let name = m
                    .name
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .ok_or(Error::MissingField("tool.name"))?;
                let response = tool_message_to_gemini_response(m)?;
                contents.push(json!({
                    "role": "user",
                    "parts": [{
                        "functionResponse": {
                            "name": name,
                            "response": response
                        }
                    }]
                }));
            }
        }
    }
    let system_instruction = if system_parts.is_empty() {
        None
    } else {
        Some(json!({
            "parts": [{ "text": system_parts.join("\n\n") }]
        }))
    };
    Ok((system_instruction, contents))
}

fn gemini_model_parts(m: &ChatMessage) -> Result<Vec<Value>> {
    let mut parts: Vec<Value> = Vec::new();
    if let Some(t) = &m.content {
        if !t.is_empty() {
            parts.push(json!({ "text": t }));
        }
    }
    if let Some(calls) = &m.tool_calls {
        for c in calls {
            let args: Value =
                serde_json::from_str(&c.function.arguments).unwrap_or_else(|_| json!({}));
            parts.push(json!({
                "functionCall": {
                    "name": c.function.name,
                    "args": args
                }
            }));
        }
    }
    if parts.is_empty() {
        parts.push(json!({ "text": "" }));
    }
    Ok(parts)
}

fn tool_message_to_gemini_response(m: &ChatMessage) -> Result<Value> {
    let raw = m.content.as_deref().unwrap_or("{}");
    serde_json::from_str(raw).or_else(|_| Ok(json!({ "result": raw })))
}

fn parse_gemini_generate_response(v: &Value) -> Result<ChatResponse> {
    let candidates = v
        .get("candidates")
        .and_then(|c| c.as_array())
        .ok_or(Error::MissingField("candidates"))?;
    if candidates.is_empty() {
        let hint = v
            .get("promptFeedback")
            .map(Value::to_string)
            .unwrap_or_else(|| "empty candidates, no promptFeedback".to_string());
        return Err(Error::Parse(format!(
            "Gemini generateContent returned no candidates (check promptFeedback): {hint}"
        )));
    }
    let c0 = &candidates[0];
    let mut text = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    if let Some(parts) = c0
        .get("content")
        .and_then(|x| x.get("parts"))
        .and_then(|p| p.as_array())
    {
        for (i, p) in parts.iter().enumerate() {
            if let Some(t) = p.get("text").and_then(|x| x.as_str()) {
                text.push_str(t);
            }
            if let Some(fc) = p.get("functionCall") {
                let name = fc
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let args = fc.get("args").cloned().unwrap_or(json!({}));
                let arguments = serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string());
                // Gemini 响应无 OpenAI 式 tool_call id；用占位 id 便于在统一类型中承载多轮（下游可自管与 `functionResponse` 的对应关系）。
                tool_calls.push(ToolCall {
                    id: format!("gemini_fc_{i}"),
                    function: FunctionCallResult { name, arguments },
                });
            }
        }
    }
    let finish_reason = if !tool_calls.is_empty() {
        Some(FinishReason::ToolCalls)
    } else {
        c0.get("finishReason")
            .and_then(|f| f.as_str())
            .and_then(map_gemini_finish_reason)
    };
    Ok(ChatResponse {
        content: if text.is_empty() { None } else { Some(text) },
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        finish_reason,
    })
}

#[async_trait]
impl ChatProvider for GoogleGeminiChat {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let body = self.build_request_json(request)?;
        let base = self.base_url.trim_end_matches('/');
        let url = format!("{}/models/{}:generateContent", base, self.model);
        let query = [("key", self.api_key.as_str())];
        let v: Value = self
            .client
            .post_json_query(&url, &query, &body, |s| s)
            .await?;
        parse_gemini_generate_response(&v)
    }

    async fn complete_stream(&self, request: &ChatRequest) -> Result<ChatStream> {
        let body = self.build_request_json(request)?;
        let base = self.base_url.trim_end_matches('/');
        let url = format!("{}/models/{}:streamGenerateContent", base, self.model);
        let query = [("key", self.api_key.as_str())];
        let sse = self
            .client
            .post_json_query_sse(&url, &query, &body, |s| s)
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
        if let Some(pf) = v.get("promptFeedback") {
            let hint = pf.to_string();
            return Some(Err(Error::Parse(format!(
                "Gemini streamGenerateContent returned no candidates: {hint}"
            ))));
        }
        return None;
    }

    let c0 = &candidates[0];
    let mut text = String::new();
    let mut tool_deltas: Vec<ToolCallDelta> = Vec::new();
    if let Some(parts) = c0
        .get("content")
        .and_then(|x| x.get("parts"))
        .and_then(|p| p.as_array())
    {
        for (i, p) in parts.iter().enumerate() {
            if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                text.push_str(t);
            }
            if let Some(fc) = p.get("functionCall") {
                let name = fc
                    .get("name")
                    .and_then(|x| x.as_str())
                    .map(str::to_string);
                let args_str = fc
                    .get("args")
                    .map(|a| serde_json::to_string(a).unwrap_or_else(|_| "{}".to_string()));
                // 流式单条事件常含完整 `functionCall`；`id` 仍为 None（与 OpenAI 流式 index 语义不同，见模块文档）。
                tool_deltas.push(ToolCallDelta {
                    index: i as u32,
                    id: None,
                    function_name: name,
                    function_arguments: args_str,
                });
            }
        }
    }

    let finish = c0
        .get("finishReason")
        .and_then(|f| f.as_str())
        .and_then(map_gemini_finish_reason);

    // 与非流式 `parse_gemini_generate_response` 一致：含 `functionCall` 且本帧已带结束信号时统一为 `ToolCalls`
    //（Gemini 常在此时仍给 `finishReason: STOP`）。
    let finish_reason = if !tool_deltas.is_empty() {
        finish.map(|_| FinishReason::ToolCalls)
    } else {
        finish
    };

    if text.is_empty() && tool_deltas.is_empty() && finish_reason.is_none() {
        return None;
    }

    let delta = if text.is_empty() { None } else { Some(text) };
    let tool_call_deltas = if tool_deltas.is_empty() {
        None
    } else {
        Some(tool_deltas)
    };

    Some(Ok(ChatChunk {
        delta,
        tool_call_deltas,
        finish_reason,
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
mod json_shape_tests {
    use super::*;
    use crate::chat::{ToolChoice, ToolDefinition};
    use crate::config::{Provider, ProviderConfig};

    #[test]
    fn build_request_json_includes_tool_config_for_tool_choice() {
        let cfg = ProviderConfig::new(
            Provider::Google,
            "k",
            "https://example.invalid/v1beta".to_string(),
            "gemini-2.0-flash",
        );
        let chat = GoogleGeminiChat::new(&cfg).unwrap();
        let req = ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            tools: Some(vec![ToolDefinition::function("get_weather", serde_json::json!({}))]),
            tool_choice: Some(ToolChoice::Tool("get_weather".into())),
            temperature: Some(0.7),
            max_tokens: Some(512),
            top_p: None,
        };
        let v = chat.build_request_json(&req).unwrap();
        let t = v["generationConfig"]["temperature"].as_f64().unwrap();
        assert!((t - 0.7f64).abs() < 1e-5);
        assert_eq!(v["generationConfig"]["maxOutputTokens"], 512);
        assert!(v.get("toolConfig").is_some());
        assert_eq!(v["toolConfig"]["functionCallingConfig"]["mode"], "ANY");
        assert_eq!(
            v["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"][0],
            "get_weather"
        );
    }
}

#[cfg(test)]
mod tests;
