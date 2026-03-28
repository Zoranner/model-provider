//! **Anthropic Messages API** 兼容：`POST …/messages`（非流式与 `stream: true`）。
//!
//! 请求头：`x-api-key`、`anthropic-version`（当前实现为 `2023-06-01`）。详见模块级 rustdoc（历史说明）。

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

/// Anthropic Messages 兼容实现使用的 `anthropic-version` 请求头取值。
pub(crate) const ANTHROPIC_VERSION: &str = "2023-06-01";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_MAX_TOKENS: u32 = 4096;
const DEFAULT_TEMPERATURE: f32 = 0.2;

#[derive(Debug, Serialize)]
struct MessagesBody {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "String::is_empty")]
    system: String,
    messages: Vec<AnthropicApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct AnthropicApiMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<Value>),
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    input_schema: Value,
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

    fn build_body(&self, request: &ChatRequest, stream: bool) -> Result<MessagesBody> {
        let (system, messages) = build_anthropic_messages(&request.messages)?;
        let tools: Option<Vec<AnthropicTool>> = request
            .tools
            .as_ref()
            .filter(|t| !t.is_empty())
            .map(|t| t.iter().map(tool_to_anthropic).collect());
        let tool_choice = if tools.is_some() {
            request.tool_choice.as_ref().and_then(anthropic_tool_choice_json)
        } else {
            None
        };
        let max_tokens = request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
        let temperature = request.temperature.or(Some(DEFAULT_TEMPERATURE));
        Ok(MessagesBody {
            model: self.model.clone(),
            max_tokens,
            system,
            messages,
            tools,
            tool_choice,
            temperature,
            stream: stream.then_some(true),
        })
    }
}

fn tool_to_anthropic(t: &ToolDefinition) -> AnthropicTool {
    AnthropicTool {
        name: t.function.name.clone(),
        description: t.function.description.clone(),
        input_schema: t.function.parameters.clone(),
    }
}

/// Anthropic 仅支持 `auto` / `any` / `tool`；`ToolChoice::None` 不传 `tool_choice`（与默认行为一致）。
fn anthropic_tool_choice_json(c: &ToolChoice) -> Option<Value> {
    match c {
        ToolChoice::None => None,
        ToolChoice::Auto => Some(json!({ "type": "auto" })),
        ToolChoice::Required => Some(json!({ "type": "any" })),
        ToolChoice::Tool(name) => Some(json!({ "type": "tool", "name": name })),
    }
}

fn build_anthropic_messages(msgs: &[ChatMessage]) -> Result<(String, Vec<AnthropicApiMessage>)> {
    let mut system_parts: Vec<String> = Vec::new();
    let mut out: Vec<AnthropicApiMessage> = Vec::new();
    for m in msgs {
        match m.role {
            Role::System => {
                if let Some(c) = &m.content {
                    if !c.is_empty() {
                        system_parts.push(c.clone());
                    }
                }
            }
            Role::User => {
                out.push(AnthropicApiMessage {
                    role: "user".to_string(),
                    content: user_content(m)?,
                });
            }
            Role::Assistant => {
                out.push(AnthropicApiMessage {
                    role: "assistant".to_string(),
                    content: assistant_content(m)?,
                });
            }
            Role::Tool => {
                let tool_use_id = m
                    .tool_call_id
                    .clone()
                    .ok_or(Error::MissingField("tool.tool_call_id"))?;
                let content = m.content.clone().unwrap_or_default();
                let block = json!({
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": content,
                });
                out.push(AnthropicApiMessage {
                    role: "user".to_string(),
                    content: AnthropicContent::Blocks(vec![block]),
                });
            }
        }
    }
    Ok((system_parts.join("\n\n"), out))
}

fn user_content(m: &ChatMessage) -> Result<AnthropicContent> {
    let text = m
        .content
        .clone()
        .ok_or(Error::MissingField("user.content"))?;
    Ok(AnthropicContent::Text(text))
}

fn assistant_content(m: &ChatMessage) -> Result<AnthropicContent> {
    let has_tools = m.tool_calls.as_ref().is_some_and(|t| !t.is_empty());
    if !has_tools {
        return Ok(AnthropicContent::Text(m.content.clone().unwrap_or_default()));
    }
    let mut blocks: Vec<Value> = Vec::new();
    if let Some(t) = &m.content {
        if !t.is_empty() {
            blocks.push(json!({ "type": "text", "text": t }));
        }
    }
    if let Some(calls) = &m.tool_calls {
        for c in calls {
            let input: Value = serde_json::from_str(&c.function.arguments).unwrap_or(json!({}));
            blocks.push(json!({
                "type": "tool_use",
                "id": c.id,
                "name": c.function.name,
                "input": input,
            }));
        }
    }
    if blocks.is_empty() {
        Ok(AnthropicContent::Text(String::new()))
    } else {
        Ok(AnthropicContent::Blocks(blocks))
    }
}

#[async_trait]
impl ChatProvider for AnthropicCompatChat {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let body = self.build_body(request, false)?;
        let url = format!("{}/messages", self.base_url.trim_end_matches('/'));
        let headers = [
            ("x-api-key", self.api_key.as_str()),
            ("anthropic-version", ANTHROPIC_VERSION),
        ];
        let v: Value = self
            .client
            .post_json_with_headers(&url, &headers, &body, |s| s)
            .await?;
        parse_anthropic_message_response(&v)
    }

    async fn complete_stream(&self, request: &ChatRequest) -> Result<ChatStream> {
        let body = self.build_body(request, true)?;
        let url = format!("{}/messages", self.base_url.trim_end_matches('/'));
        let headers = [
            ("x-api-key", self.api_key.as_str()),
            ("anthropic-version", ANTHROPIC_VERSION),
        ];
        let sse = self
            .client
            .post_json_with_headers_sse(&url, &headers, &body, |s| s)
            .await?;
        let mut finish_emitted = false;
        Ok(Box::pin(sse.filter_map(move |item| {
            ready(anthropic_stream_item_to_chunk(item, &mut finish_emitted))
        })))
    }
}

fn parse_anthropic_message_response(body: &Value) -> Result<ChatResponse> {
    let content = body
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or(Error::MissingField("content"))?;
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    for block in content {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                    text_parts.push(t.to_string());
                }
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(|x| x.as_str())
                    .ok_or(Error::MissingField("tool_use.id"))?
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|x| x.as_str())
                    .ok_or(Error::MissingField("tool_use.name"))?
                    .to_string();
                let input = block.get("input").cloned().unwrap_or(json!({}));
                let arguments = serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                tool_calls.push(ToolCall {
                    id,
                    function: FunctionCallResult { name, arguments },
                });
            }
            _ => {}
        }
    }
    let text = if text_parts.is_empty() {
        None
    } else {
        Some(text_parts.join(""))
    };
    let tool_calls = if tool_calls.is_empty() {
        None
    } else {
        Some(tool_calls)
    };
    let finish_reason = body
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .and_then(map_anthropic_stop_reason);
    Ok(ChatResponse {
        content: text,
        tool_calls,
        finish_reason,
    })
}

/// 流式解析中间结果：`message_stop` 单独标记，便于与 `message_delta` 中的 `stop_reason` 去重。
enum AnthropicStreamParse {
    Chunk(ChatChunk),
    MessageStopOnly,
}

fn anthropic_stream_item_to_chunk(
    item: Result<SseEvent>,
    finish_emitted: &mut bool,
) -> Option<Result<ChatChunk>> {
    let ev = match item {
        Err(e) => return Some(Err(e)),
        Ok(ev) => ev,
    };
    let inner = match anthropic_parse_sse_event(ev) {
        None => return None,
        Some(Err(e)) => return Some(Err(e)),
        Some(Ok(p)) => p,
    };
    match inner {
        AnthropicStreamParse::Chunk(c) => {
            if c.finish_reason.is_some() {
                *finish_emitted = true;
            }
            Some(Ok(c))
        }
        AnthropicStreamParse::MessageStopOnly => {
            if *finish_emitted {
                None
            } else {
                *finish_emitted = true;
                Some(Ok(ChatChunk::finish(FinishReason::Stop)))
            }
        }
    }
}

fn anthropic_parse_sse_event(ev: SseEvent) -> Option<Result<AnthropicStreamParse>> {
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
        // SSE error 帧通常不带 HTTP 状态码；用 500 表示「流内协议错误」，便于与 `Error::Api` 统一。
        return Some(Err(Error::Api {
            status: 500,
            message: msg.to_string(),
        }));
    }

    let ty = v.get("type").and_then(|t| t.as_str())?;

    match ty {
        "content_block_start" => {
            let block = v.get("content_block")?;
            if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                let id = block
                    .get("id")
                    .and_then(|x| x.as_str())
                    .map(str::to_string);
                let name = block
                    .get("name")
                    .and_then(|x| x.as_str())
                    .map(str::to_string);
                return Some(Ok(AnthropicStreamParse::Chunk(ChatChunk {
                    delta: None,
                    tool_call_deltas: Some(vec![ToolCallDelta {
                        index,
                        id,
                        function_name: name,
                        function_arguments: None,
                    }]),
                    finish_reason: None,
                })));
            }
            None
        }
        "content_block_delta" => {
            let delta = v.get("delta")?;
            if delta.get("type").and_then(|t| t.as_str()) == Some("text_delta") {
                let text = delta.get("text").and_then(|t| t.as_str())?;
                return Some(Ok(AnthropicStreamParse::Chunk(ChatChunk::delta(text))));
            }
            if delta.get("type").and_then(|t| t.as_str()) == Some("input_json_delta") {
                let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                let partial = delta
                    .get("partial_json")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string();
                return Some(Ok(AnthropicStreamParse::Chunk(ChatChunk {
                    delta: None,
                    tool_call_deltas: Some(vec![ToolCallDelta {
                        index,
                        id: None,
                        function_name: None,
                        function_arguments: Some(partial),
                    }]),
                    finish_reason: None,
                })));
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
                    return Some(Ok(AnthropicStreamParse::Chunk(ChatChunk {
                        delta: None,
                        tool_call_deltas: None,
                        finish_reason: Some(fr),
                    })));
                }
            }
            None
        }
        "message_stop" => Some(Ok(AnthropicStreamParse::MessageStopOnly)),
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
mod tests;
