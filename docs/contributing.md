# 贡献指南

欢迎参与 model-provider 的开发。

## 开发环境

```bash
# 克隆仓库
git clone https://github.com/your-repo/model-provider.git
cd model-provider

# 运行测试（全 feature）
cargo test --all-features

# 代码检查
cargo fmt --check
cargo clippy --all-features -- -D warnings

# 生成本地文档
cargo doc --all-features --no-deps --open
```

---

## 接入新厂商

### 步骤

1. **添加 feature** — 在 `Cargo.toml` 的 `[features]` 中添加厂商 feature
2. **扩展枚举** — 在 `src/config.rs` 的 `Provider` 枚举中添加新变体
3. **实现工厂** — 在各模态的工厂函数中添加分支
4. **编写实现** — 优先复用 OpenAI 兼容路径，不兼容则创建独立文件
5. **添加测试** — 用 wiremock 覆盖成功、错误、异常响应
6. **更新文档** — 同步 README 矩阵、HTTP 文档、API 参考

### 文件清单

```
src/
├── config.rs          # Provider 枚举 + FromStr
├── chat/
│   ├── mod.rs         # 工厂函数
│   ├── openai_compat.rs
│   └── {vendor}.rs    # 新厂商（如不兼容）
├── embed/
│   └── ...
└── ...
```

### 错误语义

- 未启用厂商 feature → `ProviderDisabled`
- 厂商不支持该能力 → `Unsupported`

---

## 测试规范

### HTTP 测试

使用 wiremock 固定响应：

```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

#[tokio::test]
async fn test_chat_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "Hello"}}]
        })))
        .mount(&server)
        .await;

    // 测试代码...
}
```

### 覆盖场景

- ✅ 成功响应
- ✅ 业务错误（HTTP 200 但内容表示失败）
- ✅ HTTP 非 2xx
- ✅ 非 JSON 响应
- ✅ 缺字段

---

## 发版流程

### 发版前检查

```bash
# 1. 代码格式
cargo fmt --check

# 2. 静态检查
cargo clippy --all-features -- -D warnings

# 3. 测试
cargo test --all-features

# 4. 文档
cargo doc --all-features --no-deps

# 5. 打包检查（可选）
cargo package
```

### 更新版本

1. 更新 `Cargo.toml` 中的 `version`
2. 归档 `CHANGELOG.md` 中的 Unreleased 条目到具体版本
3. 创建 git 标签：`git tag v0.x.x`
4. 推送标签：`git push origin v0.x.x`

### CI 自动发布

推送 `v*` 标签后，GitHub Actions 自动执行：
- `cargo fmt --check`
- `cargo clippy --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo publish`（需配置 `CARGO_ACCESS_TOKEN`）

---

## 文档同步

变更以下内容时，需同步更新对应文档：

| 变更内容 | 需更新文档 |
|:---|:---|
| 新增/修改 trait | `docs/api-reference.md` + rustdoc |
| 新增/修改 HTTP 端点 | `docs/http-endpoints.md` |
| 新增厂商或能力 | `README.md` 矩阵 + `docs/api-reference.md` |
| 修改错误语义 | `docs/api-reference.md` + `docs/design-guidelines.md` |
| 用户可见行为 | `CHANGELOG.md` |

---

## 代码风格

- 遵循标准 Rust 风格（`cargo fmt`）
- 避免 `#[allow(unreachable_patterns)]`，用 cfg 互斥分支保证穷尽
- 公开 API 必须有 rustdoc 注释
- 错误信息要包含足够的排查上下文

---

## 厂商接入优先级

| 优先级 | 厂商 | 说明 |
|:---|:---|:---|
| P0 | OpenAI、Anthropic、Google | 国际主流 |
| P1 | 阿里云、智谱、MiniMax、Kimi | 国内主流 |
| P2 | OpenRouter、New API | 聚合网关 |
| P3 | Bedrock、Azure、xAI 等 | 其他平台 |

多数 OpenAI 兼容厂商只需文档说明 `base_url` 配置，无需新增代码。
