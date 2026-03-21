//! 流式对话的统一 chunk 类型。

/// 上游结束生成的原因（OpenAI 兼容 `finish_reason` 的子集；其它厂商在可映射时填入，否则为 `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
}

/// 流式响应中的单个片段：文本增量与可选的结束原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatChunk {
    /// 本轮增量文本；首包或仅含 `finish_reason` 时可能为 `None`。
    pub delta: Option<String>,
    /// 仅在流末尾或上游标明停止原因时出现。
    pub finish_reason: Option<FinishReason>,
}

impl ChatChunk {
    /// 仅文本增量。
    pub fn delta(text: impl Into<String>) -> Self {
        Self {
            delta: Some(text.into()),
            finish_reason: None,
        }
    }

    /// 仅结束原因（常见于 OpenAI 最后一包 `choices` 为空或 `delta` 无 `content`）。
    pub fn finish(reason: FinishReason) -> Self {
        Self {
            delta: None,
            finish_reason: Some(reason),
        }
    }
}
