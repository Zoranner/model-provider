//! Chat 请求/响应与流式 chunk 类型（OpenAI Chat Completions / Anthropic Messages 语义）。

use serde_json::Value;

/// 消息角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// 单条对话消息（多轮、`tool` 角色、assistant 的 `tool_calls`）。
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: Role,
    /// `user` / `assistant` / `system` 的文本；`tool` 通常为工具输出字符串。
    pub content: Option<String>,
    /// `assistant` 上一轮返回的工具调用（OpenAI 形状）。
    pub tool_calls: Option<Vec<ToolCall>>,
    /// `tool` 消息：对应哪次 `tool_calls[].id`。
    pub tool_call_id: Option<String>,
    /// `tool` 消息可选：函数名。
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Some(text.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Some(text.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    /// 纯文本 `assistant` 消息（无 `tool_calls`）。
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Some(text.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    /// `tool` 角色：上一轮 assistant `tool_calls` 的返回内容（`tool_call_id` 对应 OpenAI `tool_call_id`）。
    /// Gemini 等路径需要函数名：请使用 [`Self::tool_with_name`] 或设置 [`ChatMessage::name`]。
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            name: None,
        }
    }

    /// 同上，并设置 `name`（与 OpenAI `tool` 消息的 `name` 一致；Gemini `functionResponse` 等会用到）。
    pub fn tool_with_name(
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            name: Some(name.into()),
        }
    }
}

/// OpenAI 风格的工具定义（`type: function` + `function` 对象）。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDefinition {
    pub function: FunctionDefinition,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: Option<String>,
    /// JSON Schema 对象（`parameters`）。
    pub parameters: Value,
}

impl FunctionDefinition {
    pub fn new(name: impl Into<String>, parameters: Value) -> Self {
        Self {
            name: name.into(),
            description: None,
            parameters,
        }
    }

    pub fn with_description(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: Some(description.into()),
            parameters,
        }
    }
}

impl ToolDefinition {
    /// OpenAI 形状：`{ "type": "function", "function": { name, parameters } }`。
    pub fn function(name: impl Into<String>, parameters: Value) -> Self {
        Self {
            function: FunctionDefinition::new(name, parameters),
        }
    }

    pub fn function_with_description(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
    ) -> Self {
        Self {
            function: FunctionDefinition::with_description(name, description, parameters),
        }
    }
}

/// 对应 OpenAI `tool_choice`：不传工具时可忽略。
#[derive(Debug, Clone, PartialEq)]
pub enum ToolChoice {
    /// 显式禁用工具。OpenAI 兼容路径映射为请求体中的 `"none"`；Anthropic 在带 `tools` 时不发送 `tool_choice` 字段。
    None,
    Auto,
    Required,
    /// 强制使用指定函数名。
    Tool(String),
}

/// 一次补全请求（非流式 / 流式共用，流式由实现加 `stream: true`）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<ToolChoice>,
    /// `None` → 实现默认 `0.2`（与历史行为一致）。
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
}

impl ChatRequest {
    /// 单条 `user` 消息，其余字段默认。
    pub fn single_user(text: impl Into<String>) -> Self {
        Self {
            messages: vec![ChatMessage::user(text)],
            ..Default::default()
        }
    }
}

/// 非流式补全结果。
#[derive(Debug, Clone, PartialEq)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: Option<FinishReason>,
}

/// 完整工具调用（assistant 消息或非流式响应）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub function: FunctionCallResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCallResult {
    pub name: String,
    pub arguments: String,
}

/// 上游结束生成的原因（OpenAI 兼容 `finish_reason` 的子集；其它厂商在可映射时填入，否则为 `None`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
}

/// 流式响应中的单个片段：文本增量、工具调用增量、可选结束原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatChunk {
    /// 本轮增量文本；首包或仅含工具增量时可能为 `None`。
    pub delta: Option<String>,
    /// OpenAI `delta.tool_calls` 或 Anthropic `input_json_delta` 等映射而来。
    pub tool_call_deltas: Option<Vec<ToolCallDelta>>,
    /// 仅在流末尾或上游标明停止原因时出现。
    pub finish_reason: Option<FinishReason>,
}

/// 流式工具调用增量（按 `index` 合并多条 delta）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallDelta {
    pub index: u32,
    pub id: Option<String>,
    pub function_name: Option<String>,
    pub function_arguments: Option<String>,
}

impl ChatChunk {
    /// 仅文本增量。
    pub fn delta(text: impl Into<String>) -> Self {
        Self {
            delta: Some(text.into()),
            tool_call_deltas: None,
            finish_reason: None,
        }
    }

    /// 仅结束原因（常见于最后一包 `delta` 无 `content`）。
    pub fn finish(reason: FinishReason) -> Self {
        Self {
            delta: None,
            tool_call_deltas: None,
            finish_reason: Some(reason),
        }
    }

    /// 仅工具调用增量。
    pub fn tool_deltas(deltas: Vec<ToolCallDelta>) -> Self {
        Self {
            delta: None,
            tool_call_deltas: Some(deltas),
            finish_reason: None,
        }
    }
}
