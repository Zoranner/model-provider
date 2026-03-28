//! OpenAI Chat Completions 兼容：`POST …/chat/completions`（非流式与 `stream: true`）。

use async_trait::async_trait;
use futures::future::ready;
use futures::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;

use crate::client::HttpClient;
use crate::config::ProviderConfig;
use crate::error::{Error, Result};
use crate::sse::SseEvent;

use super::{
    ChatChunk, ChatMessage, ChatProvider, ChatRequest, ChatResponse, ChatStream, FinishReason,
    FunctionCallResult, Role, ToolCall, ToolCallDelta, ToolChoice, ToolDefinition,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_TEMPERATURE: f32 = 0.2;

#[derive(Debug, Serialize)]
struct OpenaiChatCompletionsBody {
    model: String,
    messages: Vec<OpenaiApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenaiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct OpenaiApiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenaiToolCallMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenaiToolCallMessage {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: OpenaiToolCallFunction,
}

#[derive(Debug, Serialize)]
struct OpenaiToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenaiTool {
    #[serde(rename = "type")]
    kind: String,
    function: OpenaiFunctionDefBody,
}

#[derive(Debug, Serialize)]
struct OpenaiFunctionDefBody {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: Value,
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

    fn build_body(&self, request: &ChatRequest, stream: bool) -> OpenaiChatCompletionsBody {
        let messages: Vec<OpenaiApiMessage> = request
            .messages
            .iter()
            .map(chat_message_to_openai)
            .collect();
        let tools: Option<Vec<OpenaiTool>> = request
            .tools
            .as_ref()
            .filter(|t| !t.is_empty())
            .map(|t| t.iter().map(tool_definition_to_openai).collect());
        let tool_choice = if tools.is_some() {
            request.tool_choice.as_ref().map(tool_choice_to_json)
        } else {
            None
        };
        let temperature = request.temperature.or(Some(DEFAULT_TEMPERATURE));
        OpenaiChatCompletionsBody {
            model: self.model.clone(),
            messages,
            tools,
            tool_choice,
            temperature,
            max_tokens: request.max_tokens,
            top_p: request.top_p,
            stream: stream.then_some(true),
        }
    }
}

fn chat_message_to_openai(m: &ChatMessage) -> OpenaiApiMessage {
    let role = match m.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
    .to_string();
    let tool_calls = m.tool_calls.as_ref().map(|calls| {
        calls
            .iter()
            .map(|c| OpenaiToolCallMessage {
                id: c.id.clone(),
                kind: "function".to_string(),
                function: OpenaiToolCallFunction {
                    name: c.function.name.clone(),
                    arguments: c.function.arguments.clone(),
                },
            })
            .collect()
    });
    OpenaiApiMessage {
        role,
        content: m.content.clone(),
        tool_calls,
        tool_call_id: m.tool_call_id.clone(),
        name: m.name.clone(),
    }
}

fn tool_definition_to_openai(t: &ToolDefinition) -> OpenaiTool {
    OpenaiTool {
        kind: "function".to_string(),
        function: OpenaiFunctionDefBody {
            name: t.function.name.clone(),
            description: t.function.description.clone(),
            parameters: t.function.parameters.clone(),
        },
    }
}

fn tool_choice_to_json(c: &ToolChoice) -> Value {
    match c {
        ToolChoice::None => json!("none"),
        ToolChoice::Auto => json!("auto"),
        ToolChoice::Required => json!("required"),
        ToolChoice::Tool(name) => json!({
            "type": "function",
            "function": { "name": name }
        }),
    }
}

#[async_trait]
impl ChatProvider for OpenaiCompatChat {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let body = self.build_body(request, false);
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let v: Value = self
            .client
            .post_bearer_json(&url, &self.api_key, &body, |s| s)
            .await?;
        parse_openai_chat_response(&v)
    }

    async fn complete_stream(&self, request: &ChatRequest) -> Result<ChatStream> {
        let body = self.build_body(request, true);
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let sse = self
            .client
            .post_bearer_sse(&url, &self.api_key, &body, |s| s)
            .await?;
        Ok(Box::pin(
            sse.filter_map(|item| ready(openai_sse_item_to_chunk(item))),
        ))
    }
}

fn parse_openai_chat_response(v: &Value) -> Result<ChatResponse> {
    let choices = v
        .get("choices")
        .and_then(|c| c.as_array())
        .ok_or(Error::MissingField("choices"))?;
    let ch = choices.first().ok_or(Error::MissingField("choices[0]"))?;
    let message = ch.get("message").ok_or(Error::MissingField("choices[0].message"))?;
    let content = message
        .get("content")
        .and_then(|c| c.as_str())
        .map(str::to_string);
    let tool_calls = message.get("tool_calls").and_then(parse_tool_calls_from_json);
    let finish_reason = ch
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .and_then(map_openai_finish_reason);
    Ok(ChatResponse {
        content,
        tool_calls,
        finish_reason,
    })
}

fn parse_tool_calls_from_json(v: &Value) -> Option<Vec<ToolCall>> {
    let arr = v.as_array()?;
    let mut out = Vec::new();
    for item in arr {
        let Some(id) = item.get("id").and_then(|x| x.as_str()) else {
            continue;
        };
        let Some(func) = item.get("function") else {
            continue;
        };
        let Some(name) = func.get("name").and_then(|x| x.as_str()) else {
            continue;
        };
        let arguments = func
            .get("arguments")
            .and_then(|a| a.as_str())
            .unwrap_or("")
            .to_string();
        out.push(ToolCall {
            id: id.to_string(),
            function: FunctionCallResult {
                name: name.to_string(),
                arguments,
            },
        });
    }
    Some(out).filter(|v| !v.is_empty())
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
    let delta = ch.get("delta")?;
    let delta_text = delta
        .get("content")
        .and_then(|c| c.as_str())
        .map(str::to_string);
    let tool_call_deltas = delta.get("tool_calls").and_then(parse_delta_tool_calls);
    let finish_reason = ch
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .and_then(map_openai_finish_reason);
    if delta_text.is_none() && tool_call_deltas.is_none() && finish_reason.is_none() {
        return None;
    }
    Some(Ok(ChatChunk {
        delta: delta_text,
        tool_call_deltas,
        finish_reason,
    }))
}

fn parse_delta_tool_calls(v: &Value) -> Option<Vec<ToolCallDelta>> {
    let arr = v.as_array()?;
    let mut out = Vec::new();
    for item in arr {
        let index = item.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
        let id = item
            .get("id")
            .and_then(|x| x.as_str())
            .map(str::to_string);
        let (fname, fargs) = item
            .get("function")
            .map(|f| {
                (
                    f.get("name")
                        .and_then(|n| n.as_str())
                        .map(str::to_string),
                    f.get("arguments")
                        .and_then(|a| a.as_str())
                        .map(str::to_string),
                )
            })
            .unwrap_or((None, None));
        if id.is_none() && fname.is_none() && fargs.is_none() {
            continue;
        }
        out.push(ToolCallDelta {
            index,
            id,
            function_name: fname,
            function_arguments: fargs,
        });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn map_openai_finish_reason(s: &str) -> Option<FinishReason> {
    match s {
        "stop" => Some(FinishReason::Stop),
        "length" => Some(FinishReason::Length),
        "content_filter" => Some(FinishReason::ContentFilter),
        "tool_calls" => Some(FinishReason::ToolCalls),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
