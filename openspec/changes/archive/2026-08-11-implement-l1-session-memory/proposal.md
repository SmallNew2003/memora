## Why

memora bootstrap-rust-core 已经建立 stdio MCP runtime foundation（crate +
迁移框架 + 唯一的 `memora_status` tool），但还**没有任何 L1 业务能力**：没有
session、observation、summary 概念，没有记忆读写检索接口。所有 in-progress 变更
（`add-agent-memory-capability-profiles`、`bridge-openspec-as-l2-source`、
`add-procedural-memory-skill-compiler`）都依赖这个基础层落地才能推进，其中
`add-agent-memory-capability-profiles` 的 task 1.1 明确写出"未满足 session/observation
基础存储时**先完成其独立变更**，不在本变更中重复初始化项目"。

本变更实现 `openspec/memora-proposal.md` 第 4 章 Phase 1 的全部范围：sessions /
observations / summaries 三张业务表 + 6 个 MCP tool + FTS5 全文检索，让 memora 在
单二进制启动后**真的能记东西、能搜东西**。

## What Changes

- 新增 sessions / observations / summaries 三张业务表；迁移遵循 bootstrap-rust-core
  已建立的 SHA-256 校验机制，新迁移版本 `0002`。
- 增加 6 个 MCP tool：`session_start` / `session_end` / `observe` /
  `recent_observations` / `recent_sessions` / `search`，输入输出契约遵循
  `memora-proposal.md` 第 4.3 节。
- 增加 `observations_fts` / `summaries_fts` 两个 FTS5 虚拟表，支撑 `search` tool
  的全文检索；不引入 sqlite-vec（Phase 2 才启用）。
- `observe` 接受可选 `idempotency_key`：同一 session 内同 key 只写入一次。返回
  既有的 observation 而不是新行，避免 Hook 重试导致重复观察。
- schema 预留若干可选元数据字段（agent_id、project_id、external_session_ref）
  的扩展点，但**不引入** scope / origin / authority / provenance / handoff 字段
  —— 那些由 `add-agent-memory-capability-profiles` 独立变更叠加。
- `summaries` 表本期**只接受手动写入**（通过 `session_end` 接收调用方传入的
  summary 字符串）。不调用任何 LLM、不做 AI 压缩、不引入 embedding —— 这些属于
  Phase 2。
- BREAKING：`memora_status` tool 不变；既有 `schema_version` 由 1 递增到 2。
- BREAKING：不引入新依赖；继续使用 bundled SQLite、rmcp 2.2.0、Tokio 1.40。

## Capabilities

### New Capabilities

- `l1-session-memory`：定义 L1 业务 schema（sessions / observations / summaries
  表）、记录生命周期、`idempotency_key` 写入语义、`summaries` 手动写入边界、
  兼容迁移约束。
- `l1-search-retrieval`：定义 `search` / `recent_observations` / `recent_sessions`
  三个检索接口的查询语义、FTS5 索引形态、查询结果结构、结果字段稳定性。

### Modified Capabilities

无。本变更不修改 `mcp-runtime-health` / `rust-runtime-foundation` /
`sqlite-schema-bootstrap` 三个已归档 spec；`schema_version` 在
`mcp-runtime-health` 的语义下由当前 binary 内嵌的最高迁移版本号决定，新增迁移
后该字段值由 1 变成 2，无需修改 spec 文本。

## Impact

- 新增 Rust 模块：
  - `domain/session` / `domain/observation` / `domain/summary`（纯值对象，不依赖
    rusqlite 或 RMCP）。
  - `application/start_session` / `end_session` / `observe` / `search_recent` /
    `search_fulltext` use case，以及 `ports::MemoryRepository` trait。
  - `adapters/sqlite/memory_repository.rs`（rusqlite 实现）。
  - `adapters/mcp/memory_tools.rs`（RMCP 6 个 tool 注册）。
- 新增 migrations：`0002_l1_memory.sql`（建表 + 索引 + FTS5）。
- 集成测试：新增 `tests/l1_session_memory.rs`（最小闭环：start → observe →
  end → search → recent_*），扩展 `tests/mcp_contract.rs` 断言 tool 列表与各
  tool 错误码。继续使用 `tempfile::TempDir` 隔离数据库路径。
- 不影响 bootstrap-rust-core 的 `memora_status` 与 stdio 配置；tracing 仍走
  stderr，stdout 仅承载 JSON-RPC。
- 后续 `add-agent-memory-capability-profiles` 在本变更之上叠加 scope / origin /
  authority / provenance / handoff / promote 字段与 tool；不重复初始化本变更已
  落地的 schema。
- 后续 `add-procedural-memory-skill-compiler` 把 `observations` / `summaries`
  记录作为 evidence 来源读取，不直接写入；本变更为其预留 `external_session_ref`
  / `content_hash` 字段扩展点。