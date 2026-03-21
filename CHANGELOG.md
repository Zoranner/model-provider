# Changelog

所有用户可见的变更都记录在此文档。

格式基于 [Keep a Changelog](https://keepachangelog.com/)，版本号遵循 [SemVer](https://semver.org/)。

---

## [Unreleased]

### 新增

- **Google Gemini Chat** — 新增 `google` feature 和 `Provider::Google`。实现 `generateContent` 端点（API Key 作为 query 参数 `key`）。若 HTTP 200 但 `candidates` 为空，返回含 `promptFeedback` 摘要的解析错误。
- **Google Gemini Embed** — 实现 `embedContent`（单条）和 `batchEmbedContents`（批量）。
- **Anthropic Chat** — 新增 `anthropic` feature 和 `Provider::Anthropic`。实现 Messages 兼容端点（`x-api-key` + `anthropic-version` 头），支持官方及兼容网关。
- **GitHub Actions CI** — 推送 `v*` 标签时自动运行 fmt、clippy、test，通过后发布到 crates.io。

### 变更

- **Rerank 错误语义** — `create_rerank_provider` 对 `OpenAI`/`Ollama` 现返回 `Unsupported` 而非 `ProviderDisabled`，以区分「厂商不支持」与「feature 未启用」。
- **Image 错误语义** — `create_image_provider` 对未启用 feature 的 `OpenAI`/`Aliyun` 现返回 `ProviderDisabled`，行为与 Rerank 一致。
- **工厂穷尽检查** — 去掉 `#[allow(unreachable_patterns)]`，用 cfg 互斥分支保证 match 穷尽。

---

## 发布版本

（暂无）
