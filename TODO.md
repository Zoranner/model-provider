# 开发计划

与当前仓库对齐的待办与路线说明。实现细节以 `docs/design-guidelines.md` 为准。

---

## 示例与文档卫生

- [ ] 补充 `examples/`：除 `stream_chat` 外，增加「换厂商同一套代码」等非流式示例（可与 README 快速开始呼应）
- [ ] `docs/contributing.md` 中克隆地址仍为占位 `your-repo`，有公开仓库后改为真实 URL

---

## 功能补全（按需求排期）

- [ ] **Anthropic Embed** — 官方若提供可用 HTTP API 再接入（当前矩阵为「—」）
- [ ] **Google / Anthropic Image** — 视 API 形态评估独立实现或文档化「仅 OpenAI 兼容路径」
- [ ] **Audio** — `TranscriptionProvider` / `SpeechProvider` 仍为占位；实装通常涉及 multipart/流式，需单独设计后再动工厂与准则文档

---

## 厂商接入 backlog

| 优先级 | 厂商 | 状态 | 说明 |
|:---|:---|:---|:---|
| P0 | OpenAI | ✅ 已接入 | 兼容基准 |
| P0 | Anthropic | ✅ Chat 已接 | Embed / Rerank / Image 未接 |
| P0 | Google Gemini | ✅ Chat / Embed 已接 | Rerank / Image 未接 |
| P1 | 阿里云 | ✅ 已接入 | 全模态（chat / embed / rerank / image） |
| P1 | 智谱 | ✅ 已接入 | Chat / Embed / Rerank |
| P1 | Ollama | ✅ 已接入 | 本地 OpenAI 兼容 |
| P1 | MiniMax | 未接入 | 以官方文档为准 |
| P1 | Kimi | 未接入 | 以官方文档为准 |
| P2 | OpenRouter | 未接入 | 多为 OpenAI 兼容 |
| P2 | New API | 未接入 | 多为 OpenAI 兼容 |
| P3 | DeepSeek | 未接入 | OpenAI 兼容 + 自定义 `base_url` |
| P3 | Azure OpenAI | 未接入 | 同上 |
| P3 | 硅基流动 | 未接入 | 多为 OpenAI 兼容 |
| P3 | 火山引擎 | 未接入 | 以官方文档为准 |
| P3 | Bedrock / xAI / Groq 等 | 未接入 | 按需评估 |

多数 OpenAI 兼容厂商无需新增代码，在 README / `http-endpoints` 中说明 `base_url` 与鉴权即可。

---

## 能力一览（与文档矩阵对齐）

| 能力 | 状态 | 说明 |
|:---|:---|:---|
| Chat | ✅ | 已列厂商均支持（含非流式与流式 `chat_stream`） |
| Embed | ✅ | Anthropic 除外 |
| Rerank | ✅ | 仅阿里云、智谱 |
| Image | ✅ | 仅 OpenAI、阿里云 |
| Audio | 占位 | Trait 与工厂已存在，无远端实现 |

---

## 非目标（未写入新 trait 前不承诺）

- 自动重试与 429 退避
- 共享 `reqwest::Client` 连接池策略
- 语音 multipart
- 按厂商维护模型白名单
- 发请求前按模型名本地拦截
