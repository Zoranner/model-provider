# 后续工作与维护备忘

本文件跟踪实现缺口、工程化与发版检查。设计约定以 [设计准则](docs/design-guidelines.md) 为准；能力矩阵与接入说明见根目录 [README](README.md)。README 中已标明未支持或需专用契约的能力，若要落地须先评估 HTTP 形态、Cargo feature 组合与错误语义，再落到实现与文档。

## 当前快照

在对应 feature 组合下，对话、向量、重排序、文生图均已对接 HTTP；共享的 `HttpClient::post_bearer_json` 与各厂商分支配合 wiremock 或模块内测试，覆盖了成功体、部分 4xx、解析失败与请求体验证。`docs/` 下接口索引、HTTP 汇总、Rust API 摘要与设计准则已齐；crate 与各子模块 rustdoc 说明了 feature 边界与 `ProviderDisabled` / `Unsupported` 的划分（与 `rerank` 对齐：`chat` / `embed` / `image` 工厂用互斥 `cfg` 保证 `match` 穷尽）。`audio` 仅有 trait 与工厂占位，创建函数始终返回 `Unsupported`，不发起请求。

## 工程与 CI

未对每次 push / PR 跑自动化检查；仅在推送 **`v*`** 版本标签时由 [cargo-publish](.github/workflows/cargo-publish.yml) 执行 `cargo fmt --check`、`cargo clippy --all-targets --all-features -D warnings`、`cargo test --all-features`，通过后 `cargo publish`（仓库 Secrets 需配置 `CARGO_ACCESS_TOKEN`）。工作流使用 `dtolnay/rust-toolchain@stable` 并设 `permissions: contents: read`。本库未提交 `Cargo.lock`，缓存 key 使用 `Cargo.toml` 的 hash。发版前须使 `Cargo.toml` 的 `version` 与标签一致。日常提交可在本地自行执行相同 `cargo` 命令；若将来需要 PR 门禁，再单独加 workflow 即可。

## 发版与 crates.io

`Cargo.toml` 中建议补充 **`rust-version`（MSRV）**，并在 README 或 `docs/rust-api.md` 中写明，便于依赖方与 docs.rs 构建预期一致。发布到 crates.io 前还宜补齐 **`repository`**、按需 **`documentation`** / **`homepage`**，并考虑 `keywords` / `categories` 等元数据。

当前包版本为 **0.2.0**，而 [CHANGELOG](CHANGELOG.md) 仍只有 **Unreleased** 段落。下一版发布时应将已对用户可见的变更归档到具体版本号下，并持续在 Unreleased 中累积新条目；若 0.2.0 已对外发布过，可补写一条 0.2.0 历史区块以免记录断层。

发版前用 `cargo doc --all-features --no-deps` 核对公开 trait、工厂与类型在各 feature 下的可见性，并与 `docs/interfaces.md`、README 矩阵对照。

## 文档与一致性

变更厂商 HTTP 契约、工厂分支或 `Error` 变体时，同步 README 矩阵、`docs/http-api.md`、`docs/rust-api.md` 与设计准则中相关表述；用户可见行为变化必须进 CHANGELOG。

## 示例代码

尚无 `examples/` 目录。可新增最小可运行示例（例如仅 `openai`+`chat`，或 `full` 下多模态各一条），与单测互补，方便接入方复制粘贴与人工联调。

## 测试与质量（按需加深）

现有测试已覆盖各模态主要路径；若某厂商分支与 OpenAI 兼容实现完全复用同一解析逻辑，wiremock 可能已间接覆盖。后续若新增专用请求体或路径，应为该分支补充固定响应用例，避免只在默认 feature 下「碰巧通过」。

源码中仅在少数路径使用 `tracing`（如部分 rerank / embed 日志）；若希望调用方可观测，可在准则或 rustdoc 中简短说明「可选接入 tracing subscriber」，避免依赖方误以为默认有结构化日志输出。

## 能力与架构（中长期）

下列项在设计准则中目前为**非目标**或需单独设计：自动重试、429/5xx 退避、共享或注入 `reqwest::Client`、流式 chat、multipart（语音/大图等）。采纳前须新增 trait / 客户端路径并更新准则，避免与现有「单次 JSON、整包读体」语义混同。

`audio` 可择一对接具体厂商（往往涉及 multipart 与流式），或长期保持占位并在 README / rustdoc 中明确「未接远端」。若对接，需单独评估与各厂商 TTS/ASR API 的契约，并更新能力矩阵与 HTTP 文档。

## 日常维护

矩阵与文档以厂商官方说明为准；接口变更时在 CHANGELOG 中提示调用方核对。新增或调整 `Provider` 枚举时勿忘 `#[non_exhaustive]` 与 `FromStr`、工厂分支、feature 表的全链路更新。
