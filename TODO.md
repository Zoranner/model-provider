# 开发计划

## 近期任务

### 发版准备

- [ ] 确定 semver 版本号（考虑 Rerank/Image 错误语义变更是否破坏性）
- [ ] 归档 CHANGELOG Unreleased 到具体版本
- [ ] `Cargo.toml` 补充元数据：`rust-version`、`repository`、`readme`、`keywords`、`categories`

### 功能补全

- [ ] 添加 `examples/` 目录（各厂商 + 各模态的示例）
- [ ] Anthropic embed（按需）
- [ ] Google / Anthropic image（按需）

### 可选优化

- [ ] PR / push 时运行 CI（当前仅 `v*` 标签触发）
- [ ] 升级 `actions/cache` 到 v4

---

## 厂商规划

| 优先级 | 厂商 | 状态 | 说明 |
|:---|:---|:---|:---|
| P0 | OpenAI | ✅ 已接入 | 兼容基准 |
| P0 | Anthropic | ✅ Chat 已接 | Embed/Rerank/Image Unsupported |
| P0 | Google Gemini | ✅ Chat/Embed 已接 | Rerank/Image Unsupported |
| P1 | 阿里云 | ✅ 已接入 | 全模态 |
| P1 | 智谱 | ✅ 已接入 | Chat/Embed/Rerank |
| P1 | Ollama | ✅ 已接入 | 本地 OpenAI 兼容 |
| P1 | MiniMax | 未接入 | 以官方文档为准 |
| P1 | Kimi | 未接入 | 以官方文档为准 |
| P2 | OpenRouter | 未接入 | 多为 OpenAI 兼容 |
| P2 | New API | 未接入 | 多为 OpenAI 兼容 |
| P3 | DeepSeek | 未接入 | 多为 OpenAI 兼容 + 自定义 base_url |
| P3 | Azure OpenAI | 未接入 | OpenAI 兼容 + 自定义 base_url |
| P3 | 硅基流动 | 未接入 | 多为 OpenAI 兼容 |
| P3 | 火山引擎 | 未接入 | 以官方文档为准 |
| P3 | Bedrock / xAI / Groq 等 | 未接入 | 按需评估 |

**注意**：多数 OpenAI 兼容厂商无需新增代码，只需文档说明 `base_url` 配置方式。

---

## 能力规划

| 能力 | 当前状态 | 说明 |
|:---|:---|:---|
| Chat | ✅ 已实现 | 全厂商 |
| Embed | ✅ 已实现 | Anthropic 除外 |
| Rerank | ✅ 已实现 | 仅阿里云、智谱 |
| Image | ✅ 已实现 | 仅 OpenAI、阿里云 |
| Audio | 占位 | Trait 已定义，无实现 |

### 非目标（当前不承诺）

- 自动重试与 429 退避
- 共享 `reqwest::Client` 连接池
- 流式 chat
- 语音 multipart
- 按厂商维护模型白名单

---

## 发版清单

每次发版前检查：

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-features -- -D warnings`
- [ ] `cargo test --all-features`
- [ ] `cargo doc --all-features --no-deps`
- [ ] CHANGELOG 已更新
- [ ] README 能力矩阵与实现一致
