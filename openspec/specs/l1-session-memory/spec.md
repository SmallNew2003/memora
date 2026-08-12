# l1-session-memory Specification

## Purpose
为 memora L1 临时记忆层定义业务 schema 与写入边界：在 `0001` 迁移之上以 `0002_l1_memory` 一次性建立 `sessions` / `observations` / `summaries` 三张业务表及 `observations_fts` / `summaries_fts` 两个 FTS5 虚拟表，并固化 `idempotency_key` 写入语义与 `summaries` 手动写入边界。`schema_version` 在该迁移落地后由 1 递增到 2，作为后续能力（capability profiles、procedural skill compiler、L2 桥接）叠加业务列与 tool 的稳定基底。本 spec 不引入 sqlite-vec / embedding / AI 压缩，也不在写入路径读取 `agent_id` / `project_id` / `external_session_ref` / `content_hash` 这些为后续变更预留的可空扩展列。
## Requirements
### Requirement: L1 业务表与迁移版本

memora binary 内嵌的迁移集合 MUST 在 `0001`（bootstrap-rust-core 引入）之上新增
`0002_l1_memory`，该迁移 MUST 在单一 `BEGIN IMMEDIATE` 事务内建立 sessions /
observations / summaries 三张业务表与 observations_fts / summaries_fts 两个 FTS5
虚拟表，并配套 INSERT / UPDATE / DELETE 触发器以保持 FTS5 与基表一致。
`schema_migrations` 表的迁移版本在 `0002` 应用成功后 MUST 反映为最高版本号 2，
且 `mcp-runtime-health` 中的 `schema_version` 字段 MUST 返回 2。

#### Scenario: 全新启动应用全部迁移
- **WHEN** memora 在不存在数据库文件的默认配置下启动
- **THEN** 系统创建本地数据库、按版本顺序应用 0001 与 0002，建立 sessions /
      observations / summaries 三张业务表与两个 FTS5 虚拟表，且
      `schema_migrations` 包含版本 1 与版本 2 两行

#### Scenario: 旧 schema_version=1 数据库升级
- **WHEN** memora 在仅含 0001 迁移的数据库上启动
- **THEN** 系统应用 0002 迁移，三张业务表与两个 FTS5 索引被建立，
      `schema_version` 从 1 升级到 2，已有数据不受影响

#### Scenario: 迁移校验和漂移
- **WHEN** 已应用 0002 迁移的校验和与当前 binary 内嵌值不匹配
- **THEN** 系统以明确错误中止启动，不应用后续迁移、不注册 MCP tool

#### Scenario: 旧 binary 打开 schema_version=2 数据库
- **WHEN** 数据库记录的迁移最高版本高于当前 binary 内嵌的最高迁移版本
- **THEN** 系统以数据库版本过新的错误中止启动

### Requirement: 三张业务表必填列与可选扩展列

`sessions` / `observations` / `summaries` 表 MUST 至少包含以下列：

- `id TEXT PRIMARY KEY`（除 sessions 外的表 id 由 server 生成 UUIDv4）；
- `created_at TEXT NOT NULL DEFAULT (datetime('now'))`，时间戳以 UTC 秒级
  字符串持久化；
- `sessions` MUST 包含 `name TEXT NOT NULL`、`ended_at TEXT`（可空）、
  `summary TEXT`（可空）；
- `observations` MUST 包含 `session_id TEXT NOT NULL REFERENCES sessions(id)`、
  `content TEXT NOT NULL`、`tool_name TEXT`（可空）；
- `summaries` MUST 包含 `session_id TEXT NOT NULL REFERENCES sessions(id)`、
  `content TEXT NOT NULL`。

业务表 MUST 包含以下可选扩展列，**本变更 MUST NOT 读取、索引或校验其语义**：

- `sessions.agent_id` / `sessions.project_id` / `sessions.external_session_ref`；
- `observations.idempotency_key`（同 `(session_id, idempotency_key)` 唯一，见
  Requirement "observe 幂等性"）；
- `observations.content_hash`；
- `summaries.content_hash`。

#### Scenario: 业务表结构
- **WHEN** 系统成功应用 0002 迁移
- **THEN** 三张业务表存在上述必填列与可选扩展列，且 `observations` 的
      `session_id` 与 `summaries` 的 `session_id` 都通过外键约束引用
      `sessions.id`

### Requirement: observe 幂等性

`observe` MUST 接受调用方可选传入的 `idempotency_key`（字符串，1-256 字符，
ASCII 可打印）。同 `(session_id, idempotency_key)` 的重复提交 MUST 返回首次
写入的 observation（`id` 与 `created_at` 不变），而不是新增一行。不提供
`idempotency_key` 的写入 MUST 始终产生新行。`idempotency_key` 重复提交
MUST NOT 视作错误。

#### Scenario: 首次 observe 写入
- **WHEN** 客户端在已知 session 内调用 `observe` 并提供 `idempotency_key`
- **THEN** 系统写入一行新 observation 并返回其 `id` 与 `created_at`

#### Scenario: 重复 idempotency_key 提交
- **WHEN** 客户端在相同 session 内用相同 `idempotency_key` 再次调用 `observe`
- **THEN** 系统返回首次写入的 observation，`id` 与 `created_at` 与首次一致，
      数据库行数不增加

#### Scenario: 跨 session 的同名 key
- **WHEN** 客户端在不同 session 内使用相同 `idempotency_key` 调用 `observe`
- **THEN** 系统视为不同写入，分别产生新行

#### Scenario: 缺省 idempotency_key
- **WHEN** 客户端在 `observe` 中不提供 `idempotency_key`
- **THEN** 系统每次都产生新行

### Requirement: summaries 手动写入边界

`summaries` 表本期 MUST 仅由 `session_end` 在调用方传入 `summary` 字符串时
写入；memora MUST NOT 在本变更中调用任何 LLM、不生成 AI 压缩摘要、不引入
embedding。若 `session_end` 不传 `summary`，MUST 仅更新 `ended_at` 而不写入
summaries 行。

#### Scenario: session_end 传入 summary
- **WHEN** 客户端调用 `session_end` 并提供 `summary` 字符串
- **THEN** 系统更新对应 session 的 `ended_at` 与 `summary`，并在
      `summaries` 表新增一行

#### Scenario: session_end 不传 summary
- **WHEN** 客户端调用 `session_end` 不提供 `summary`
- **THEN** 系统仅更新 `ended_at`，`summaries` 表行数不增加

#### Scenario: session_end 幂等
- **WHEN** 客户端对同一 session 多次调用 `session_end`
- **THEN** `ended_at` 更新为最近一次时间，`summaries` 行数与最近一次
      `summary` 内容保持一致，不会产生重复行

### Requirement: 兼容迁移与字段预留

`0002` 迁移 MUST 在 bootstrap-rust-core 留下的 schema_version=1 数据库上幂等
应用；不允许跳过迁移版本号或修改既有 `schema_migrations` 行。预留字段以可空
形式存在，后续 `add-agent-memory-capability-profiles` 等变更可以在不破坏既有
数据的前提下通过新增迁移叠加 scope / origin / authority / handoff 等列。

#### Scenario: 升级路径不破坏既有数据
- **WHEN** memora 在 schema_version=1 的数据库上启动
- **THEN** 系统应用 0002 迁移后，schema_migrations 包含版本 1 与 2 两行，
      既有 0001 引入的表与索引未受影响

#### Scenario: 预留字段不读取
- **WHEN** 本变更内的任何 tool 或查询路径运行
- **THEN** 系统 MUST NOT 基于 `agent_id` / `project_id` /
      `external_session_ref` / `content_hash` 字段做过滤、排序、检索或校验

