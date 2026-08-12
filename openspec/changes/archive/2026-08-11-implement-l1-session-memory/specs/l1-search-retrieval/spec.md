## ADDED Requirements

### Requirement: search 全文检索与 BM25 排序

`search` MUST 接受 `query: string`（必填）、`session_id?: string`（可选，按
session 过滤）、`kind?: "observation" | "summary" | "both"`（默认 `both`）、
`limit?: number`（默认 20，上限 100，超过上限以错误拒绝）。`search` MUST 在
`observations_fts` 与 `summaries_fts` 上使用 FTS5 MATCH 检索并按 BM25 分数
排序，分数越小表示越相关。`session_id` 过滤 MUST 在 FTS5 结果集上额外叠加，
而不是改写 MATCH 表达式。

#### Scenario: 关键词全文检索
- **WHEN** 客户端调用 `search`，`query` 为包含于某 observation `content` 的
      字符串
- **THEN** 系统返回按 BM25 升序排列的 observation 列表，每条含 `kind`、
      `id`、所属 `session_id`、`content`、`tool_name`、`created_at` 与
      `score`

#### Scenario: 按 session 过滤
- **WHEN** 客户端调用 `search` 同时提供 `query` 与 `session_id`
- **THEN** 系统仅返回与该 session 匹配的结果

#### Scenario: kind 过滤
- **WHEN** 客户端调用 `search` 提供 `kind=summary`
- **THEN** 系统仅返回 summary 行，结果中 `tool_name` 字段不出

#### Scenario: 超限拒绝
- **WHEN** 客户端调用 `search` 提供 `limit > 100`
- **THEN** 系统以 `INVALID_INPUT` 错误拒绝请求，而不是悄悄截断

### Requirement: recent_observations 时间序接口

`recent_observations` MUST 接受 `session_id?: string`（可选；不提供则跨
session 返回）、`limit?: number`（默认 20，上限 100，超过上限以错误拒绝）。
结果 MUST 按 `(created_at DESC, id DESC)` 稳定排序；`session_id` 提供时 MUST
仅返回该 session 的 observation。结果每条 MUST 包含 `id`、所属 `session_id`、
`content`、`tool_name`、`created_at`。

#### Scenario: 限定 session 最近 observation
- **WHEN** 客户端调用 `recent_observations`，提供 `session_id`
- **THEN** 系统返回该 session 的 observation 列表，按 `created_at DESC,
      id DESC` 排序

#### Scenario: 跨 session 最近 observation
- **WHEN** 客户端调用 `recent_observations` 不提供 `session_id`
- **THEN** 系统返回跨 session 的 observation 列表，仍按
      `created_at DESC, id DESC` 排序

#### Scenario: 排序稳定性
- **WHEN** 同一 `created_at` 秒级时间戳下存在多条 observation
- **THEN** 系统按 `id DESC` 兜底排序，两次相同调用返回的顺序完全一致

### Requirement: recent_sessions 时间序接口

`recent_sessions` MUST 接受 `limit?: number`（默认 20，上限 100，超过上限以
错误拒绝）。结果 MUST 按 `(created_at DESC, id DESC)` 稳定排序。每条 MUST
包含 `id`、`name`、`created_at`、`ended_at`（可空，未结束 session 为 `null`）、
`summary`（可空，未调用 `session_end` 或未传 `summary` 时为 `null`）。

#### Scenario: 默认最近会话
- **WHEN** 客户端调用 `recent_sessions` 不提供 `limit`
- **THEN** 系统返回最多 20 条 session，按 `created_at DESC, id DESC` 排序

#### Scenario: 未结束会话字段为 null
- **WHEN** 系统返回最近会话中包含未调用 `session_end` 的 session
- **THEN** 该行的 `ended_at` 与 `summary` 字段 MUST 为 `null`

### Requirement: 响应字段稳定性

本变更引入的所有 MCP tool 响应 MUST 仅包含本 spec 显式声明的字段；新增字段
视为 minor 演进（不破坏既有字段语义即可），但 MUST NOT 改变既有字段类型、
含义或顺序。MCP error code MUST 在 `PARSE_ERROR` / `INVALID_REQUEST` /
`METHOD_NOT_FOUND` / `INTERNAL_ERROR` 等 RMCP 内置码之外使用以下业务错误码：
`SESSION_NOT_FOUND` / `INVALID_INPUT` / `STORAGE_ERROR`。错误响应 message
MUST NOT 包含绝对数据库路径、未脱敏的 observation 或 summary 内容、本地文件
系统信息。

#### Scenario: 响应字段固定
- **WHEN** 客户端按本 spec 文档解析任何 L1 tool 响应
- **THEN** 字段集合与类型与本 spec 一致；额外字段视为将来扩展，旧字段不可
      缺失或换名

#### Scenario: 错误响应脱敏
- **WHEN** 任何 L1 tool 因业务错误返回 error 响应
- **THEN** error message 中 MUST NOT 出现绝对路径、原始 observation / summary
      内容或本地文件系统路径