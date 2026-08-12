## Context

memora 仓库已完成 bootstrap-rust-core：单二进制 stdio MCP server、SHA-256 校验
的迁移框架、bundled SQLite + FTS5 编译验证、tracing 走 stderr 的日志隔离。
`openspec/specs/` 已沉淀 `mcp-runtime-health` / `rust-runtime-foundation` /
`sqlite-schema-bootstrap` 三个 spec。本变更在这些已确立契约之上叠加 L1 业务
能力。

约束：

- 本变更沿用 bootstrap-rust-core 的依赖方向（`main -> app -> adapters ->
  application -> domain`），domain 与 application MUST NOT 依赖 rusqlite、RMCP
  或文件系统。
- 本变更不引入新 Cargo 依赖。
- 所有新增 MCP tool 必须遵循"tracing 走 stderr、stdout 仅承载 JSON-RPC"的协议
  日志隔离约定（spec `rust-runtime-foundation`）。
- 本变更是 `add-agent-memory-capability-profiles` 的前置依赖；变更产物
  schema 必须能被该后续变更在不破坏既有数据的前提下扩展。

## Goals / Non-Goals

**Goals：**

- 让 Agent 通过 MCP tool 在单次或多次会话中记录观察（observation）和手动摘要
  （summary），并在需要时按全文关键词检索或按时间序拉取最近记录。
- 让 `sessions` / `observations` / `summaries` 三张业务表及其 FTS5 索引在
  `0002` 迁移中一次性建立，并复用 `sqlite-schema-bootstrap` 的 SHA-256 校验
  链路。
- 让 `observe` 在同一 session 内接受调用方生成的 `idempotency_key`，重复提交
  返回既有的 observation 而不是新行；为后续 capability profiles 变更的 Hook
  重试场景提供基础。
- 让 L1 检索（`search` / `recent_observations` / `recent_sessions`）的响应字段
  与排序规则**在后续 capability profiles 变更叠加时不发生不兼容变化**。
- schema 设计为后续 capability profiles 变更预留 `agent_id` / `project_id` /
  `external_session_ref` 等可选字段扩展点，但本变更**不实现**这些字段的语义
  与索引。

**Non-Goals：**

- 不引入 sqlite-vec、不引入 embedding、不调用任何 LLM；AI 压缩与向量检索属于
  Phase 2。
- 不实现 scope / origin / authority / provenance / handoff / `promote` / 客户端
  capability 协商；这些由 `add-agent-memory-capability-profiles` 独立变更
  交付。
- 不实现 OpenSpec 制品桥接；由 `bridge-openspec-as-l2-source` 独立交付。
- 不实现程序性 Skill；由 `add-procedural-memory-skill-compiler` 独立交付。
- 不实现 `memora-proposal.md` 第 5 章 Phase 3+ 的 L2 项目记忆、L3 用户记忆、
  L4 LLM WIKI。
- 不实现自动 session 生命周期管理（无人调用 `session_end` 时不强制归档）；
  `agent_memory_capability_profiles` 后续变更会负责"未结束会话可恢复"的语义。
- 不引入新的 Cargo 依赖。

## Decisions

### D1：业务表与索引遵循 `memora-proposal.md` 第 4.2 节，但补齐 FTS5 触发器与约束

**选择**：`0002_l1_memory.sql` 建立三张业务表 + 两个 FTS5 虚拟表 + 三对
INSERT/UPDATE/DELETE 触发器：

```sql
CREATE TABLE sessions (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at        TEXT,
    summary         TEXT,
    agent_id        TEXT,            -- 预留字段，本变更不读取、不索引
    project_id      TEXT,            -- 预留字段，本变更不读取、不索引
    external_session_ref TEXT        -- 预留字段，本变更不读取、不索引
);

CREATE TABLE observations (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES sessions(id),
    content         TEXT NOT NULL,
    tool_name       TEXT,
    idempotency_key TEXT,             -- 同 (session_id, key) 重复写入返回既有行
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    content_hash    TEXT              -- sha256(content || tool_name)，预留以便
                                       -- 后续 capability profiles 变更做回声去重
);

CREATE UNIQUE INDEX idx_observations_session_idem
    ON observations(session_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE TABLE summaries (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES sessions(id),
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    content_hash    TEXT
);

CREATE VIRTUAL TABLE observations_fts USING fts5(
    id, session_id, content, tool_name,
    content='observations', content_rowid='rowid'
);
CREATE VIRTUAL TABLE summaries_fts USING fts5(
    id, session_id, content,
    content='summaries', content_rowid='rowid'
);
-- 配套的 ai / ad / au 触发器保持 observations_fts / summaries_fts 与基表一致
```

**理由**：完全对齐 `memora-proposal.md` 第 4.2 节既定形状，避免后续 Phase 2 改造
时再做 DDL 迁移；预留字段以 NULL 形式存在，不引入破坏性迁移、不被本变更的
查询路径读取。

**替代方案考虑**：

- *本期只建 sessions / observations 表，summaries 留 Phase 2*：会让 Phase 2 再
  做一次 DDL 迁移，违反 `sqlite-schema-bootstrap` "新增迁移构成连续前缀"约束
  的精神；本期一次性建立三表成本极低。
- *本期就把 scope / origin / authority 字段建好*：与 capability profiles 变更的
  设计契约重叠，违反 D5 的边界原则。

### D2：observ/summary 主键与 `idempotency_key` 都由 server 生成

**选择**：

- `session_start` 不接受调用方传入 `session_id`；server 用 UUIDv4 生成并
  立刻写入 `sessions` 行（`ended_at = NULL`、`summary = NULL`），返回
  `{ session_id, created_at }`。
- `observe` / `session_end` 不接受调用方传入 `observation_id` / `summary_id` /
  `ended_at`；时间戳由 server 用 `datetime('now')` 写入。
- 调用方可在 `observe` 中提供 `idempotency_key`（不提供则视为一次性写入）。
  同 `(session_id, idempotency_key)` 重复提交返回首次写入的 observation 而不
  是新行。
- `session_end` 本身是幂等的：重复调用只更新 `ended_at` 与 `summary`，不新增行。

**理由**：让 server 拥有时间戳与主键生成权，避免客户端时钟漂移、UUID 碰撞或
重复 UUID 导致的隐性 bug；`idempotency_key` 是 Agent 端在 Hook 重试场景下保证
一次写入的**唯一**可控手段。

**替代方案考虑**：

- *允许调用方传入主键*：降低 server 复杂度但增加客户端出错面（重复 UUID 撞
  库、客户端时钟漂移导致 ordering 错乱）。
- *不实现 `idempotency_key`*：违反 `add-agent-memory-capability-profiles` task
  2.3 的明确要求；该变更需要在此之上叠加语义而不是反过来要求我们补实现。

### D3：FTS5 全文检索 + BM25 排序，summary 与 observation 共享接口

**选择**：

- `search` tool 接受 `query`、`session_id?`、`limit?`（默认 20，上限 100）、
  `kind?`（`observation` | `summary` | `both`，默认 `both`）。
- 实现：先用 `observations_fts MATCH ?` 与 `summaries_fts MATCH ?` 取 BM25
  排序的 rowid 集合，再 JOIN 回基表拿完整字段；按 bm25 升序（越相关越靠前）
  取 limit 条。
- 响应字段固定：`{ kind, id, session_id, content, tool_name?, created_at,
  score }`；`score` 是 BM25 分值（数值越小越相关，调用方可直接使用或忽略）。
- `kind=observation` 时 `tool_name` 必出，`kind=summary` 时 `tool_name` 不出。

**理由**：FTS5 是 SQLite 内置能力，无需新依赖；BM25 与 `engram` /
`basic-memory` 等成熟记忆系统使用相同算法，调用方心智模型一致。

**替代方案考虑**：

- *用 LIKE '%query%'*：性能不可控，且无法做相关性排序。
- *本期就把 embedding 一起加上*：违反 Phase 划分；embedding 依赖
  model 选型与 Rust 推理栈，超出本变更范围。

### D4：recent 接口以 `created_at DESC, id DESC` 为稳定排序

**选择**：

- `recent_observations` 接受 `session_id?`（不提供则跨 session 返回）、
  `limit?`（默认 20，上限 100），返回按 `(created_at DESC, id DESC)` 排序
  的 observation 列表。
- `recent_sessions` 接受 `limit?`（默认 20，上限 100），返回按
  `(created_at DESC, id DESC)` 排序的 session 列表（`ended_at` 与 `summary`
  原样返回，未结束 session 的字段为 `null`）。
- 两个接口的排序都基于 `created_at` 的秒级精度 + 主键兜底，确保同一秒内
  的多条记录排序稳定可重现。

**理由**：`created_at DESC` 是 L1 临时记忆最自然的访问模式；`(created_at, id)`
双键排序避免同一秒内多行顺序随机，方便 Agent 做"继续上次工作"。

### D5：本变更为 capability profiles 预留扩展字段但不实现语义

**选择**：`sessions` / `observations` / `summaries` 都包含若干可选元数据
字段（`agent_id` / `project_id` / `external_session_ref` / `content_hash`）。
本变更**不读取、不索引、不校验**这些字段的语义；它们的存在仅为后续 capability
profiles 变更能在不破坏 schema 的前提下叠加 scope / origin / authority 等列。
`content_hash` 在本变更里**也不在写入时自动计算** —— 保留给后续变更按其
provenance 模型使用。

**理由**：让 schema 形态在前置变更阶段就稳定，避免后续 capability profiles 变更
必须再做一次 ALTER TABLE / 重建 FTS5 索引。

**替代方案考虑**：

- *本变更一次性把 capability profiles 变更的所有字段都加进去*：会让本变更承担
  后续变更的责任；后续变更失去独立评审与归档节奏。
- *完全不预留字段，让 capability profiles 变更自行做迁移*：会触发更多迁移版本
  并增加老数据库升级路径复杂度。

### D6：响应字段稳定，错误码固定

**选择**：

- 成功响应是 `{ kind?, id?, session_id?, observation_id?, created_at?,
  tool_name?, content?, results?, summary?, total? }` 之类的 JSON object；
  字段按 tool 各自契约固定，新增字段视为 minor 演进，**不修改既有字段语义**。
- 错误响应固定为 MCP error code（保留 `PARSE_ERROR` / `INVALID_REQUEST` /
  `METHOD_NOT_FOUND` / `INTERNAL_ERROR` 等 RMCP 内置码）+ 业务错误码
  `SESSION_NOT_FOUND` / `INVALID_INPUT` / `STORAGE_ERROR`，调用方可通过
  `(code, message)` 区分。
- `observe` 的 `idempotency_key` 重复提交**不算错误**，返回首次写入的
  observation。

**理由**：`add-procedural-memory-skill-compiler` 等后续变更需要稳定响应契约来
做 evidence 关联；本变更确立的字段顺序与错误码成为公共 API 的一部分。

### D7：兼容迁移约束（已有数据库升级路径）

**选择**：

- `0002_l1_memory.sql` 必须能在 bootstrap-rust-core 留下的初始数据库上幂等
  执行：从 `schema_version = 1` 直接升级到 `schema_version = 2`。
- 升级路径不修改既有 `schema_migrations` 表的行；只新增 `0002` 行。
- 校验：集成测试在 `tempfile::TempDir` 里创建初始数据库（schema_version = 1），
  启动 memora，断言三表与两个 FTS5 索引都建好且 `schema_migrations` 行连续。

**理由**：沿用 `sqlite-schema-bootstrap` 的迁移契约；本变更不引入迁移版本的
跳跃，不跳过版本号。

## Risks / Trade-offs

- **[风险] `idempotency_key` 范围只覆盖同一 session，跨 session 视为不同 key。**
  → **缓解**：文档明示该语义；后续 capability profiles 变更如需跨 session
  幂等，需要扩展唯一索引但保留旧索引作为前缀。
- **[风险] FTS5 中文分词能力受限（SQLite FTS5 默认 unicode61 分词器按
  Unicode 标准分词，对 CJK 支持差）。** → **缓解**：本变更使用默认分词器；
  若中文检索效果差，后续变更单独引入 `unicode61 remove_diacritics 2
  tokenchars ...` 或 `trigram` 分词器，并在迁移中重建索引。**本变更不预设
  分词器选型**。
- **[风险] `summaries` 表只接受手动写入导致 Phase 1 没有 AI 压缩能力。**
  → **缓解**：明确声明 Phase 2 边界；`session_end` 接受调用方传入的 summary
  字符串，符合 `memora-proposal.md` 第 5 章 Phase 1 "不包含 AI 自动压缩"的
  约束。
- **[风险] 最近接口按 `created_at DESC` 排序，`datetime('now')` 在 SQLite 中是
  UTC 秒级，跨时区不可预期。** → **缓解**：所有时间戳统一 `datetime('now')`，
  以 UTC 字符串形式持久化；调用方解析时自行转换。文档与 response 字段明示
  UTC。
- **[权衡] 不引入 sqlite-vec 意味着 Phase 1 没有语义检索。** → 这是有意的
  Phase 划分；与 `memora-proposal.md` 第 5 章一致。
- **[权衡] `idempotency_key` 在重复提交时必须返回**完全相同**的 observation**
  （包括 `created_at`），而不是新行。→ server 端只需"按 (session_id, key) 唯一
  索引 SELECT 既有行"；存储代价低，行为可预测。

## Migration Plan

1. **本变更落地后**：`openspec/changes/implement-l1-session-memory/` 下有完整
   proposal/design/tasks + 两个 spec，配套 Rust 模块迁移至 `src/`。
2. **实装完成后**：通过 `/opsx:archive-change` 归档本变更，把
   `specs/l1-session-memory/spec.md` 与 `specs/l1-search-retrieval/spec.md`
   同步进 `openspec/specs/`，让它们成为正式能力规格。
3. **后续变更引用**：
   - `add-agent-memory-capability-profiles` 在 `openspec/specs/l1-session-memory`
   之上叠加 scope / origin / authority / handoff / promote 列与 tool。
   - `add-procedural-memory-skill-compiler` 通过 `observations` / `summaries`
   表读取 evidence，不修改 schema。
4. **回滚策略**：本变更若被否决，删除
   `openspec/changes/implement-l1-session-memory/` 整个目录，不影响
   bootstrap-rust-core 的运行能力（现有 `memora_status` 仍工作）。

## Open Questions

1. **`idempotency_key` 的最大长度与字符集限制？** 当前 spec 写 `TEXT`，建议
   限制 1-256 字符、ASCII 可打印。是否在本变更中提前校验？
2. **`session_end` 不传 `summary` 是否合法？** 当前 plan 是合法（仅更新
   `ended_at`）。是否要强制要求 summary 非空？
3. **`recent_sessions` 是否需要包含 `ended_at IS NULL` 的活跃会话过滤选项？**
   当前 plan 是默认包含；后续 capability profiles 变更可叠加 `active_only`
   参数。
4. **FTS5 中文分词器是否本变更内就选型？** 倾向推迟到中文检索需求出现时
   单独提案；当前 spec 写"使用默认分词器"。