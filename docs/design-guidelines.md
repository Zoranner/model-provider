# 设计准则

本文档约定库的架构原则和演进边界。新增能力、接新厂商或修改错误语义时请对照。

若实现与准则不一致，应优先修正实现或更新本文档，避免长期漂移。

---

## 定位与范围

**目标**：在 Rust 中用统一配置调用多家云上的常见 AI HTTP API。

**默认假设**：
- 单次请求，非流式
- 请求与响应主体为 JSON
- TLS 走 rustls（与 reqwest 选型一致）

**超出范围**（不强行塞进现有形态，需单独设计）：
- 流式输出
- multipart 上传
- 长连接
- 重试风暴控制

若需要上述能力，应新增独立 trait 和文档章节，并在本准则中记录。

---

## Feature 与工厂

### 正交组合

厂商维度与模态维度通过 Cargo feature 正交组合：

```
# 只用 OpenAI 对话
features = ["openai", "chat"]

# 用阿里云全部能力
features = ["aliyun", "chat", "embed", "rerank", "image"]
```

### 编译时检查

只有厂商 feature 和模态 feature 同时满足时，对应实现才参与编译。

运行时配置与编译结果不匹配时，应在**工厂阶段**返回明确错误，而非发请求后模糊失败。

### 错误语义

| 错误 | 含义 | 示例 |
|:---|:---|:---|
| `ProviderDisabled` | 未启用厂商或模态 feature | 用了 `Aliyun` 但没开 `aliyun` feature |
| `Unsupported` | 厂商不支持该能力 | `OpenAI` + `rerank` |

**区分原则**：
- 编译配置问题 → `ProviderDisabled`
- 厂商能力问题 → `Unsupported`

---

## 配置与 HTTP 约定

### 配置集中

对外配置集中在 `ProviderConfig`：
- `provider` — 厂商枚举
- `api_key` — API 密钥
- `base_url` — 网关地址
- `model` — 模型名称
- `dimension` — 向量维度（embed 必填）
- `timeout` — 可选超时

### 模型名称透传

`model` 字段**原样进入请求**，库内不做校验：
- 不维护各厂商可用模型清单
- 不做「该厂商是否支持此模型」预检
- 模型是否合法、有权限、已开通，一律以远端响应为准

### URL 拼接

路径拼接统一处理尾部斜杠：
```
base_url = "https://api.openai.com/v1"
path = "/chat/completions"
→ "https://api.openai.com/v1/chat/completions"
```

### 鉴权约定

| 鉴权方式 | 适用厂商 |
|:---|:---|
| `Authorization: Bearer` | OpenAI、阿里云、Ollama、智谱 |
| `x-api-key` + `anthropic-version` | Anthropic |
| URL query `key=` | Google Gemini |

空密钥是否接受由上游网关决定，库内不做本地校验。

---

## 实现分层

### 公开 API

crate 根重导出的稳定面：
- trait：`ChatProvider`、`EmbedProvider` 等
- 类型：`Provider`、`ProviderConfig`、`Error` 等
- 工厂：`create_chat_provider` 等

### 实现细节

以下属于 `pub(crate)`，不保证 semver 稳定：
- HTTP 请求/响应结构体
- 解析逻辑
- 子模块内部类型

### 子模块可见性

`chat`、`embed` 等子模块作为 `pub mod` 暴露，仅用于承载 rustdoc，不鼓励依赖子模块路径编程。

### 厂商实现优先级

1. 优先复用 OpenAI 兼容实现
2. 只有请求体或路径明显不一致时才拆独立文件
3. 非兼容实现需在 HTTP 文档中标注

---

## 错误处理

### 类型设计

使用 `thiserror` 等可枚举形式，避免「任意上下文堆栈」型错误。

### 错误携带信息

| 场景 | 错误类型 | 携带信息 |
|:---|:---|:---|
| HTTP 非 2xx | `Api` | 状态码 + 响应体信息 |
| JSON 结构不符 | `Parse` | 解析失败说明 |
| 响应缺字段 | `MissingField` | 缺失字段名 |

---

## 文档分工

| 文档 | 职责 |
|:---|:---|
| rustdoc | 调用方在 IDE 或 `cargo doc` 中直接看到的契约 |
| `README.md` | 快速入门、能力矩阵、配置示例 |
| `docs/api-reference.md` | Rust API 完整参考 |
| `docs/http-endpoints.md` | HTTP 端点细节 |
| `docs/design-guidelines.md` | 架构原则与边界 |

改行为时至少同步一侧；用户可见行为变了，应同步 `README.md`。

---

## 测试期望

### HTTP 测试

优先用 wiremock 固定响应，覆盖：
- 成功响应体
- 业务错误体
- 非 JSON 异常响应

不为实现细节写脆性过强的全文快照测试，重点断言：
- 状态映射
- 错误变体
- 关键字段解析

### 全 feature 检查

发版前确保全 feature 下通过：
```bash
cargo fmt --check
cargo clippy --all-features -- -D warnings
cargo test --all-features
```

---

## 非目标

下列方向在未写入新 trait 与新准则段落之前，不作为现有 API 的隐含承诺：

- 自动重试与 429 退避
- 共享 `reqwest::Client` 连接池策略
- 流式 chat
- 语音 multipart
- 图像超大载荷分块
- 按厂商维护可用模型白名单
- 发 HTTP 前根据模型名拦截请求

占位模块（如 `audio`）允许长期存在，但工厂与 rustdoc 必须明确「未接远端」。
