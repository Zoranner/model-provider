# 接口一览

下表便于从「能力」跳到 Rust 与 HTTP 两端的说明；细节以 [Rust 公共 API](rust-api.md) 与 [HTTP 端点汇总](http-api.md) 为准。

| 能力 | 工厂函数（feature） | 主要 trait | 远端约定摘要 |
|:---|:---|:---|:---|
| 对话 | `create_chat_provider`（`chat`） | `ChatProvider` | `POST …/chat/completions`，OpenAI 兼容 |
| 向量 | `create_embed_provider`（`embed`） | `EmbedProvider` | `POST …/embeddings`；智谱无 `dimensions` 字段 |
| 重排序 | `create_rerank_provider`（`rerank`） | `RerankProvider` | 阿里云 `POST …/reranks`；智谱 `POST …/rerank` |
| 文生图 | `create_image_provider`（`image`） | `ImageProvider` | OpenAI：`…/images/generations`；阿里云：`…/services/aigc/multimodal-generation/generation` |
| 语音识别 | `create_transcription_provider`（`audio`） | `TranscriptionProvider` | 未实现 |
| 语音合成 | `create_speech_provider`（`audio`） | `SpeechProvider` | 未实现 |

厂商由 `Provider` 与 Cargo feature 共同决定；未启用的组合在工厂阶段失败，不会发 HTTP。厂商与能力的矩阵见仓库根目录 `README.md`。
