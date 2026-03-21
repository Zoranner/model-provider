# 文档

本目录包含 model-provider 的完整技术文档。

## 面向用户

| 文档 | 说明 |
|:---|:---|
| [API 参考](api-reference.md) | Rust trait、工厂函数、配置类型、错误处理 |
| [HTTP 端点](http-endpoints.md) | 各厂商的请求/响应格式、路径、鉴权方式 |

## 面向贡献者

| 文档 | 说明 |
|:---|:---|
| [设计准则](design-guidelines.md) | 库的架构原则、边界、演进约定 |
| [贡献指南](contributing.md) | 如何参与开发、测试、发版流程 |

## 快速定位

**我想……**

- 了解有哪些 trait 和方法 → [API 参考](api-reference.md)
- 查看某个厂商的 HTTP 细节 → [HTTP 端点](http-endpoints.md)
- 接入新厂商 → [贡献指南](contributing.md) + [设计准则](design-guidelines.md)
- 理解错误类型 → [API 参考 - 错误处理](api-reference.md#错误处理)
- 本地生成文档 → `cargo doc --all-features --no-deps --open`
