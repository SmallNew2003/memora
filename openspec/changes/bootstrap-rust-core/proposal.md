## Why

memora 当前已有设计制品，但没有源代码树或 Cargo manifest。一个已废弃的实验仅能从
构建产物中看出痕迹，且使用了过时的 RMCP API；若不先建立基础，后续功能开发将基于
无法构建、也无法审查的起点。

## What Changes

- 初始化一个单一 Rust 二进制 crate，采用模块化单体边界：domain、application、
  SQLite adapter、MCP adapter、configuration 和 composition root。
- 建立带版本的 SQLite 数据库初始化与健康检查，不引入 Phase 2 的向量检索或
  Phase 3 的项目记忆行为。
- 运行 stdio MCP server，提供一个只读的 `memora_status` 工具，作为从 MCP 请求
  穿过 application 和 storage 的端到端契约检查。
- 锁定精确的 Rust 工具链、经过验证的 RMCP server API 及其 lockfile；增加确定性的
  格式化、lint、单元测试、集成测试和 MCP contract test 入口。解析依赖的命令使用
  `--locked`。
- **BREAKING**：无。这是首次源码实现，不存在已发布的 runtime 或 MCP 调用方。

## Capabilities

### New Capabilities

- `rust-runtime-foundation`：定义可构建的单二进制 runtime、依赖方向、configuration
  和工程质量门禁。
- `sqlite-schema-bootstrap`：定义本地 SQLite 数据库创建、版本化迁移，以及空数据库
  或不兼容数据库的安全启动行为。
- `mcp-runtime-health`：定义 stdio MCP service 生命周期及其只读状态契约，用于端到端
  验证 runtime。

### Modified Capabilities

无。已有 OpenSpec 变更定义未来的 L2 知识和跨客户端记忆契约；本变更只为它们建立
实现基础。

## Impact

- 新增 `Cargo.toml`、`rust-toolchain.toml`、重新生成的 `Cargo.lock`、`src/` 模块树、
  migrations 和测试目录。孤立的 lockfile 被视为已被替代的生成产物，而不是依赖契约。
- 新增当前 RMCP server API、Tokio runtime、基于 serde 的类型、SQLite persistence、
  tracing 和测试支持所需的 Rust 依赖。
- 在显式启动 runtime 时创建本地数据库文件；不增加 network service、HTTP transport、
  embedding model、vector index 或特定客户端 adapter。
- 成为计划中的 session-memory 和 agent-capability-profile 实现变更的前置条件。
