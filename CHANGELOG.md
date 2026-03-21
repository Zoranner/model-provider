# Changelog

## Unreleased

### Added

- GitHub Actions：推送 `v*` 标签时运行 `fmt`、`clippy`（全 feature）、`test`（全 feature），通过后发布至 crates.io（需配置 `CARGO_ACCESS_TOKEN`）；工具链使用 `dtolnay/rust-toolchain@stable`（替代已归档的 `actions-rs/toolchain`）。

### Changed

- **Rerank**：`create_rerank_provider` 对 `OpenAI` / `Ollama` 现返回 `Error::Unsupported`（`capability: "rerank"`），而不再返回 `Error::ProviderDisabled`，以区分「未启用厂商 feature」与「该厂商在本模态无实现」。未启用 `aliyun` / `zhipu` feature 时仍选阿里云 / 智谱的，仍为 `ProviderDisabled`（行为未变）。若依赖旧错误变体区分 OpenAI/Ollama 重排序，请改为匹配 `Unsupported`。

- **Image**：`create_image_provider` 在启用 `image` 但未启用 `openai` / `aliyun` 时，对 `OpenAI` / `Aliyun` 现返回 `ProviderDisabled`（与重排序、设计准则一致）；此前会落入 `Unsupported`。`Ollama` / `Zhipu` 仍为 `Unsupported`（`capability: "image"`）。

- **Chat / Embed / Image 工厂**：去掉 `#[allow(unreachable_patterns)]`，用与 `rerank` 相同的 `cfg` 互斥分支保证 `match` 穷尽，便于在全 feature 下依赖编译器检查。
