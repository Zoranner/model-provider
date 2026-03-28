# HTTP 端点

本文档描述各厂商的 HTTP 调用细节，供调试和对接网关时参考。

## 通用约定

- `{base_url}` 来自 `ProviderConfig::base_url`
- 实现会自动处理尾部斜杠，避免双斜杠或遗漏
- 默认请求头：`Content-Type: application/json`
- HTTP 非 2xx 时，错误信息尽量从响应体提取
- 默认超时：Chat 60s、Embed 30s、Rerank 60s、Image 120s

---

## Chat 对话

### OpenAI 兼容路径

适用于：OpenAI、阿里云、Ollama、智谱

**非流式**：

```
POST {base_url}/chat/completions
Authorization: Bearer {api_key}
```

**请求体**（由 `ChatRequest` 映射；未指定 `temperature` 时默认 `0.2`）：
```json
{
  "model": "gpt-4o-mini",
  "messages": [{"role": "user", "content": "你好"}],
  "temperature": 0.2
}
```

可含多轮 `messages`（`system` / `user` / `assistant` / `tool`）、`tools`、`tool_choice`、`max_tokens`、`top_p`（与 OpenAI Chat Completions 一致）。

**响应解析**：`choices[0].message` 的 `content` 与 `tool_calls` → `ChatResponse`。

**流式（`complete_stream`）**：

- 请求体在以上基础上增加 `"stream": true`
- 请求头增加 `Accept: text/event-stream`
- 响应为 SSE：`data:` 行为 Chat Completions **chunk** JSON，流结束为 `data: [DONE]`

**chunk 映射**：
- `choices[].delta.content` → `ChatChunk.delta`
- `choices[].delta.tool_calls` → `ChatChunk.tool_call_deltas`（`index` / `id` / `function.name` / `function.arguments` 分片）
- `choices[].finish_reason` → `ChatChunk.finish_reason`（`stop`/`end_turn` → `Stop`，`length` → `Length`，`content_filter` → `ContentFilter`，`tool_calls` → `ToolCalls`）

| 厂商 | 典型 base_url |
|:---|:---|
| OpenAI | `https://api.openai.com/v1` |
| 阿里云 | `https://dashscope.aliyuncs.com/compatible-mode/v1` |
| Ollama | `http://127.0.0.1:11434/v1` |
| 智谱 | `https://open.bigmodel.cn/api/paas/v4` |

### Anthropic Messages

**非流式**：

```
POST {base_url}/messages
x-api-key: {api_key}
anthropic-version: 2023-06-01
```

**请求体**（`ChatRequest`：`system` 角色会合并为顶层 `system` 字符串；`max_tokens` 默认 `4096` 若未设置）：
```json
{
  "model": "claude-3-5-sonnet-20241022",
  "max_tokens": 1024,
  "messages": [{"role": "user", "content": "你好"}],
  "temperature": 0.2
}
```

可含 `tools`（`input_schema`）、`tool_choice`，及带 `tool_use` / `tool_result` 内容块的多轮消息。`ToolChoice::None` 时若仍带 `tools`，请求体中**不**含 `tool_choice` 字段（与默认行为一致）。

**响应解析**：`content` 中 `text` 与 `tool_use` 块 → `ChatResponse.content` / `tool_calls`。

**流式（`complete_stream`）**：

- 请求体增加 `"stream": true`
- 响应为 SSE：事件名如 `content_block_delta`、`content_block_start`（`tool_use`）、`input_json_delta`、`message_delta`、`message_stop` 等
- 流内 `event: error` 映射为 `Error::Api`

**事件映射（摘要）**：
- `text_delta` → `ChatChunk.delta`
- `content_block_start`（tool_use）与 `input_json_delta` → `ChatChunk.tool_call_deltas`
- `message_delta.stop_reason` → `ChatChunk.finish_reason`
- `message_stop`：若 `message_delta` 已产出 `finish_reason`，则不再重复产出 `Stop`；否则映射为 `ChatChunk.finish(Stop)`

**注意**：
- `max_tokens` 来自 `ChatRequest::max_tokens` 或默认 `4096`
- `anthropic-version` 请求头当前实现为 `2023-06-01`
- 兼容遵循相同契约的第三方网关（如部分 Coding Plan）

### Google Gemini

**非流式**：

```
POST {base_url}/models/{model}:generateContent?key={api_key}
```

**请求体**（`ChatRequest`：`contents` 含 `role` + `parts`；`system` → `systemInstruction`；`tools` → `tools[].functionDeclarations`；可选 `toolConfig.functionCallingConfig`）：
```json
{
  "contents": [{"role": "user", "parts": [{"text": "你好"}]}],
  "generationConfig": {"temperature": 0.2}
}
```

**响应解析**：`parts` 中 `text` 与 `functionCall` → `ChatResponse`；存在 `functionCall` 时 `finish_reason` 视为 `ToolCalls`。

**流式（`complete_stream`）**：

```
POST {base_url}/models/{model}:streamGenerateContent?key={api_key}
```

请求体与非流式相同。响应为 SSE：`data:` 为响应片段 JSON。若某包中 `candidates` 为空且含 `promptFeedback`，返回解析错误。

**chunk 解析**：
- `parts[].text` → `ChatChunk.delta`
- `parts[].functionCall`（`name` / `args`）→ `ChatChunk.tool_call_deltas`
- `finishReason` → `ChatChunk.finish_reason`（映射：`STOP` 等 → `Stop` / `Length` / `ContentFilter`）；若同帧含 `functionCall` 且已带结束信号，则与非流式一致，统一为 `ToolCalls`（避免仅映射 `STOP` 与工具调用语义冲突）

**注意**：
- 不使用 Bearer，API Key 作为 query 参数 `key`
- `{model}` 直接嵌入路径，如 `gemini-2.0-flash`
- `Role::Tool` 映射为 `functionResponse` 时，`ChatMessage.name` 须非空，否则在发请求前返回 `MissingField("tool.name")`
- 若 HTTP 200 但 `candidates` 为空（如安全拦截），返回 `Parse` 错误并含 `promptFeedback` 摘要

**典型 base_url**：`https://generativelanguage.googleapis.com/v1beta`

---

## Embed 向量

### OpenAI 兼容路径

适用于：OpenAI、阿里云、Ollama

```
POST {base_url}/embeddings
Authorization: Bearer {api_key}
```

**请求体**：
```json
{
  "model": "text-embedding-3-small",
  "input": ["hello", "world"],
  "dimensions": 1536
}
```

**响应解析**：`data[].embedding`

### 智谱

路径与 OpenAI 兼容，但请求体**不含** `dimensions` 字段：

```json
{
  "model": "embedding-3",
  "input": ["hello", "world"]
}
```

**注意**：`ProviderConfig::dimension` 仍须设置。用于 `EmbedProvider::dimension()` 返回值，且须与模型实际输出一致。

### Google Gemini

```
# 单条
POST {base_url}/models/{model}:embedContent?key={api_key}

# 批量
POST {base_url}/models/{model}:batchEmbedContents?key={api_key}
```

**单条请求体**：
```json
{
  "model": "models/text-embedding-004",
  "content": {"parts": [{"text": "hello"}]},
  "outputDimensionality": 768
}
```

**批量请求体**：
```json
{
  "model": "models/text-embedding-004",
  "requests": [
    {"model": "models/text-embedding-004", "content": {"parts": [{"text": "hello"}]}, "outputDimensionality": 768},
    {"model": "models/text-embedding-004", "content": {"parts": [{"text": "world"}]}, "outputDimensionality": 768}
  ]
}
```

**响应解析**：
- 单条：`embedding.values`
- 批量：`embeddings[].values`

**注意**：
- 不使用 Bearer，API Key 作为 query 参数 `key`
- `model` 字段为资源名格式 `models/{model_id}`
- 若配置的 `model` 已含 `models/` 前缀则不再重复

---

## Rerank 重排序

### 阿里云

```
POST {base_url}/reranks
Authorization: Bearer {api_key}
```

**请求体**：
```json
{
  "model": "gte-rerank",
  "query": "什么是 Rust",
  "documents": ["Rust 是系统编程语言", "Python 是脚本语言"],
  "top_n": 3
}
```

**响应解析**：`results[].index` + `results[].relevance_score`

**典型 base_url**：`https://dashscope.aliyuncs.com/api/v1`（非 compatible-mode）

### 智谱

```
POST {base_url}/rerank
Authorization: Bearer {api_key}
```

请求/响应字段与阿里云一致（`relevance_score`）。

**注意**：路径段为单数 `rerank`，阿里云为复数 `reranks`。

---

## Image 文生图

### OpenAI

```
POST {base_url}/images/generations
Authorization: Bearer {api_key}
```

**请求体**：
```json
{
  "model": "dall-e-3",
  "prompt": "一只可爱的猫",
  "n": 1,
  "size": "1024x1024"
}
```

**响应解析**：
- 若有 `data[0].url` → 返回 `ImageOutput::Url`
- 否则若有 `data[0].b64_json` → base64 解码为 `ImageOutput::Bytes`

**size 映射**：

| ImageSize | OpenAI 字符串 |
|:---|:---|
| `Square512` | `512x512` |
| `Square1024` | `1024x1024` |
| `Landscape` | `1792x1024` |
| `Portrait` | `1024x1792` |

### 阿里云

```
POST {base_url}/services/aigc/multimodal-generation/generation
Authorization: Bearer {api_key}
```

**请求体**：
```json
{
  "model": "wanx-v1",
  "input": {
    "messages": [{"role": "user", "content": [{"text": "一只可爱的猫"}]}]
  },
  "parameters": {
    "size": "1024*1024",
    "prompt_extend": true,
    "watermark": false
  }
}
```

**响应解析**：`output.choices[0].message.content` 中第一项含 `image` 字段的 URL

**size 映射**：

| ImageSize | 阿里云字符串 |
|:---|:---|
| `Square512` | `512*512` |
| `Square1024` | `1024*1024` |
| `Landscape` | `1792*1024` |
| `Portrait` | `1024*1792` |

**注意**：
- base_url 为原生 API 根，如 `https://dashscope.aliyuncs.com/api/v1`
- **不是** `compatible-mode/v1`

---

## Audio 语音

暂无实现。`create_transcription_provider` 和 `create_speech_provider` 直接返回 `Unsupported`。
