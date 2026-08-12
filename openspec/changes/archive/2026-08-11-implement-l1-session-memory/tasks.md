# Tasks: implement-l1-session-memory

> 本变更按顺序落地：先建 migration 与 schema 校验，再建 domain 与 application
> ports，再实现 SQLite adapter，再注册 6 个 MCP tool，最后补集成测试与门禁。
> 任务编号反映建议执行顺序，但不强制串行；除"前置条件"显式声明的依赖外，
> 其他任务可在同一节内并行。

## 1. 迁移与 schema

- [x] 1.1 在 `src/migrations/` 下新增 `m_0002_l1_memory.sql`，定义 sessions /
      observations / summaries 三表 + 配套 FTS5 虚拟表与 INSERT/UPDATE/DELETE
      触发器；预留 `agent_id` / `project_id` / `external_session_ref` /
      `content_hash` / `idempotency_key` 列。本期不计算 `content_hash`。
- [x] 1.2 在 `src/migrations/mod.rs` 注册新迁移版本 `0002`，其 SHA-256 校验和
      由编译期生成的固定常量承担（沿用 bootstrap-rust-core 已有机制）。
- [x] 1.3 单元测试：在临时 SQLite 上手动应用 0001 + 0002，断言三表与两个
      FTS5 虚拟表都存在，`schema_migrations` 行连续，校验和写入正确。
- [x] 1.4 集成测试：构造 `schema_version = 1` 的旧数据库（仅含
      `schema_migrations` 与 0001），启动 memora，断言升级到 schema_version = 2
      且三表已建好；构造版本跳跃（声称有 0002 但 SQL 文件缺失）断言启动失败。

## 2. Domain 层与 application ports

- [x] 2.1 在 `src/domain/` 下新增 `session.rs` / `observation.rs` /
      `summary.rs`，定义纯值对象与必要 newtype；domain MUST NOT 依赖 rusqlite、
      RMCP 或文件系统。
- [x] 2.2 在 `src/application/` 下新增 `ports.rs`，定义 `MemoryRepository`
      trait，覆盖 `start_session` / `end_session` / `observe` /
      `recent_observations` / `recent_sessions` / `search` 六个方法；其返回
      类型全部使用 domain 值对象，不暴露 rusqlite 句柄。
- [x] 2.3 在 `src/application/` 下新增 6 个 use case 模块（每个 tool 对应一个
      文件），封装入参校验、调用 repository、构造返回响应；不允许直接拼装 SQL。
- [x] 2.4 在 `src/application/` 下新增 `errors.rs`，定义 `MemoryError` 枚举
      与到 MCP error code 的映射（`SESSION_NOT_FOUND` / `INVALID_INPUT` /
      `STORAGE_ERROR`）。

## 3. SQLite adapter

- [x] 3.1 在 `src/adapters/sqlite/` 下新增 `memory_repository.rs`，以 rusqlite
      实现 `MemoryRepository`；使用 prepared statement，禁止字符串拼 SQL。
- [x] 3.2 `observe` 实现：在一次事务内先按 `(session_id, idempotency_key)`
      SELECT 既有行；命中则直接返回，未命中则 INSERT 并返回新行。`content_hash`
      本期不计算。
- [x] 3.3 `search` 实现：对 `observations_fts` 与 `summaries_fts` 跑 BM25，按
      `kind` 过滤；返回结构含 `score`（bm25 数值，越小越相关）。
- [x] 3.4 `recent_*` 实现：按 `(created_at DESC, id DESC)` 排序；`limit`
      默认 20、上限 100，超过上限以 error 拒绝而不是悄悄截断。
- [x] 3.5 `session_end` 实现：幂等 UPDATE `ended_at` 与 `summary`；若
      `session_id` 不存在返回 `SESSION_NOT_FOUND`。
- [x] 3.6 单元测试：使用 `tempfile::TempDir` 隔离数据库，覆盖 6 个方法在
      空库 / 单行 / 多行 / 重复 idempotency_key 场景下的行为。

## 4. MCP tool adapter

- [x] 4.1 在 `src/adapters/mcp/` 下新增 `memory_tools.rs`，使用 rmcp `#[tool]`
      宏注册 6 个 tool；输入输出通过 `schemars` 派生 JSON Schema。
- [x] 4.2 6 个 tool 名称固定为 `session_start` / `session_end` / `observe` /
      `recent_observations` / `recent_sessions` / `search`；每个 tool 仅调用
      application ports，不直接访问 SQLite。
- [x] 4.3 错误响应统一包装：把 `MemoryError` 映射到 MCP error code，message
      中**禁止**包含绝对数据库路径、未脱敏的内容或本地文件系统信息。
- [x] 4.4 tracing：所有 tool 调用在 debug 级别打一条"tool called"日志，包含
      tool 名与不含敏感内容的入参摘要；错误在 warn / error 级别打日志。所有
      日志走 stderr（沿用 `rust-runtime-foundation` 约定）。
- [x] 4.5 单元测试：mock `MemoryRepository`，断言每个 tool 的入参校验、
      错误映射与响应结构稳定。

## 5. Composition root 与启动路径

- [x] 5.1 在 `src/app/` 下装配 `SqliteMemoryRepository` 与 MCP tool router，
      沿用 bootstrap-rust-core 的 `main -> app -> adapters` 依赖方向。
- [x] 5.2 `memora_status` 行为保持不变：`runtime_version` 来自 `Cargo.toml`，
      `schema_version` 等于当前 binary 内嵌的最高迁移版本号（升级到 2 后
      该字段返回 `2`），`status` / `database` / `transport` 字段含义不变。
- [x] 5.3 集成测试（smoke）：启动 memora，调用 `memora_status` 断言
      `schema_version == 2`，然后连续调用 6 个 L1 tool 完成
      start → observe × N → end → recent_* → search 最小闭环。

## 6. 集成测试与门禁

- [x] 6.1 新增 `tests/l1_session_memory.rs`，覆盖：会话生命周期、
      `idempotency_key` 重复提交、BM25 排序稳定性、`recent_*` 上限拒绝、
      错误码到 MCP 响应的映射。所有测试**必须**使用 `tempfile::TempDir` 的
      临时数据库，绝不写入默认用户数据目录。
- [x] 6.2 扩展 `tests/mcp_contract.rs`，断言 `tools/list` 返回的 tool 列表
      包含 `memora_status` 与 6 个 L1 tool，且 tool 名称与本 spec 一致。
- [x] 6.3 运行 `cargo fmt --check` / `cargo clippy --locked --all-targets
      -- -D warnings` / `cargo test --locked`，三道门禁全部通过。
- [x] 6.4 运行 `openspec validate implement-l1-session-memory --strict` 通过。

## 7. 归档准备

- [x] 7.1 用 `openspec status --change implement-l1-session-memory` 确认所有
      artifact 都已 `done`。
- [x] 7.2 通过 `/opsx:archive-change` 归档本变更，把
      `specs/l1-session-memory/spec.md` 与 `specs/l1-search-retrieval/spec.md`
      同步进 `openspec/specs/`，让它们成为正式能力规格。