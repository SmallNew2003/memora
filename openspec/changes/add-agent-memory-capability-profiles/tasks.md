## 1. 前置条件与数据模型

- [ ] 1.1 确认 Cargo workspace 与 Phase 1 的 session/observation 基础存储已落地；未满足时先完成其独立变更，不在本变更中重复初始化项目
  - **涉及文件**：`Cargo.toml`、`src/migrations/v002__l1_memory.sql`、`src/domain/session.rs`、`src/domain/observation.rs`、`src/migrations/mod.rs`
  - **涉及测试**：`tests/l1_session_memory.rs`（若不存在则属阻塞，需先合 PR #2 `feat/l1-session-memory`）
  - **AC**：① `git rev-parse --abbrev-ref HEAD` 在执行 `git diff main` 时仍能列出 `src/domain/session.rs` 等 L1 文件；② `cargo test` 能跑通 L1 测试集；③ 本地 `~/.local/share/memora/memora.db` 启动后 `memora_status` 的 `schema_version` 字段返回 2
- [ ] 1.2 定义并校验 `ClientCapabilities` 与 `OperationMode` 类型，为缺失能力声明实现 `stateless-manual` 保守默认值
  - **涉及文件**：`src/domain/capability.rs`（新增）、`src/domain/mod.rs`
  - **涉及测试**：`src/domain/capability.rs` 内嵌 `#[cfg(test)] mod tests`
  - **AC**：① 五个 capability 字段 (`native_memory`、`session_lifecycle`、`tool_capture`、`context_injection`、`max_context_tokens`) 都实现 `Deserialize + Serialize + schemars::JsonSchema`；② `OperationMode` 枚举仅含三个变体 `NativeOpaque` / `StatelessHooked` / `StatelessManual`；③ 单元测试覆盖 `ClientCapabilities::default()` 返回 `stateless-manual` 路径并断言 `resolve_operation_mode(&ClientCapabilities::default()) == OperationMode::StatelessManual`
- [ ] 1.3 为 session 持久化 `agent_id`、可选 `external_session_ref`、客户端能力快照、运行模式、活跃时间和归档状态
  - **涉及文件**：`src/migrations/v003__capability_profile_session.sql`（新增）、`src/adapters/sqlite/memory_repository.rs`、`src/domain/session.rs`、`src/application/ports.rs`、`src/application/use_cases/start_session.rs`、`src/application/use_cases/end_session.rs`
  - **涉及测试**：`tests/capability_profiles_session.rs`（新增）
  - **AC**：① `v003` 迁移为 `sessions` 表追加 `capabilities_json TEXT`、`operation_mode TEXT`、`last_active_at TEXT`、`archived_at TEXT`；② `SessionStartInput` 接受可选 `client_capabilities` 与 `external_session_ref`；③ `start_session` 在未提供能力时把 `operation_mode` 持久化为 `"stateless-manual"`；④ 单元测试断言：`start_session(name="x", capabilities=None)` 写入的行 `operation_mode="stateless-manual"` 且 `capabilities_json IS NULL`
- [ ] 1.4 为记忆记录持久化 scope、kind、origin、project、内容哈希、authority、来源引用、过期与替代关系元数据，并为查询字段建立索引
  - **涉及文件**：`src/migrations/v003__capability_profile_session.sql`（同一迁移内 ALTER）、`src/domain/observation.rs`、`src/domain/summary.rs`、`src/application/use_cases/observe.rs`、`src/application/use_cases/search.rs`、`src/adapters/sqlite/memory_repository.rs`
  - **涉及测试**：`tests/capability_profiles_provenance.rs`（新增）
  - **AC**：① `observations` / `summaries` 表追加 `scope`、`kind`、`origin`、`project_id`、`content_hash`、`authority`、`source_refs_json`、`expires_at`、`supersedes_id`、`fact_key` 字段；② 为 `(origin)`、`(scope, project_id)`、`(fact_key)` 建索引；③ `observe` 在 `tool_result` 路径写入 `origin="tool_result"` 并计算 SHA-256 `content_hash`；④ 单元测试断言：跨 session 同 `fact_key` 不同值的两条记录各自保留 `conflict_state` 字段可读
- [ ] 1.5 设计并实施兼容迁移，确保已有 session 与 observation 在新字段缺失时可按保守默认值读取
  - **涉及文件**：`src/migrations/v003__capability_profile_session.sql`、`src/adapters/sqlite/memory_repository.rs`、`src/application/ports.rs`
  - **涉及测试**：`src/adapters/sqlite/memory_repository.rs` 内嵌测试 + `tests/capability_profiles_migration.rs`
  - **AC**：① 旧 v002 数据库升级到 v003 后所有原行 `scope='session'`、`kind='observation'`、`origin='user'`、`authority='l1_observation'`、`content_hash=NULL`（触发回填逻辑填值）；② `recent_observations` 行为与 L1 现状一致（不加 scope 过滤）；③ 升级路径测试：用预先准备的 v002 schema 快照 db 启动后断言新查询不丢旧数据

## 2. MCP 会话与能力协商

- [ ] 2.1 扩展 `session_start` 以接受可选 client capabilities 和 external session ref，并在响应中返回 operation mode 及 fallback reason
  - **涉及文件**：`src/adapters/mcp/memory_tools.rs`、`src/adapters/mcp/mod.rs`、`src/application/use_cases/start_session.rs`、`src/domain/capability.rs`
  - **涉及测试**：`tests/mcp_contract.rs` 扩展 + `tests/capability_profiles_session.rs`
  - **AC**：① `SessionStartParams` 增加 `client_capabilities: Option<ClientCapabilities>` 与 `external_session_ref: Option<String>` 字段；② 响应 JSON 包含 `operation_mode` 与可选 `fallback_reason`；③ `mcp_contract.rs` 新增断言：未传 capabilities 时响应 `operation_mode="stateless-manual"` 且 `fallback_reason IS NULL`；传 `session_lifecycle="hook"` 时响应 `operation_mode="stateless-hooked"`
- [ ] 2.2 实现按能力组合选择 `native-opaque`、`stateless-hooked` 与 `stateless-manual` 的纯逻辑，禁止依赖客户端产品名称
  - **涉及文件**：`src/domain/capability.rs`（新增 `resolve_operation_mode` 函数）、`src/application/use_cases/start_session.rs`
  - **涉及测试**：`src/domain/capability.rs` 内嵌表格驱动测试
  - **AC**：① `resolve_operation_mode` 是纯函数，无 IO、无全局状态；② 表格驱动测试覆盖 4 类客户端：未声明 / `native_memory=opaque` / `lifecycle=hook` / 完全手动；③ 代码中 grep `client_name|product_name|agent_product` 必须 0 命中（CI grep 断言）
- [ ] 2.3 为 `observe` 实现 idempotency key 验证与重复调用返回既有记录的行为
  - **涉及文件**：`src/application/use_cases/observe.rs`、`src/application/ports.rs`、`src/adapters/sqlite/memory_repository.rs`
  - **涉及测试**：`tests/l1_session_memory.rs` 现有 idempotency 用例需仍通过 + `tests/capability_profiles_provenance.rs` 新增
  - **AC**：① 已有 `idempotency_key` 唯一索引保留；② 重复提交同 `(session_id, key)` 返回首次 `id` 与 `created_at`；③ 新增断言：`origin="memora_recall"` 重复内容（含原 `source_refs`）被识别为去重，新行不入库
- [ ] 2.4 实现相同项目与 external session ref 的会话恢复，并为无法恢复的长期未活动会话实现归档路径
  - **涉及文件**：`src/application/use_cases/start_session.rs`、`src/application/use_cases/end_session.rs`、`src/adapters/sqlite/memory_repository.rs`、`src/config/mod.rs`（新增 `archive_after_seconds` 默认 30 天）
  - **涉及测试**：`tests/capability_profiles_recovery.rs`（新增）
  - **AC**：① 同 `(project_id, external_session_ref)` 二次 `session_start` 返回原未结束 `session_id`，且不创建新行；② `session_id` 不匹配但 `external_session_ref` 匹配且 `last_active_at` 超 30 天则把原行 `archived_at` 置当前 UTC，响应标记 `recovered=false`；③ 单元测试断言：跨 project 同 `external_session_ref` 不会触发恢复（必须 project 也匹配）
- [ ] 2.5 为所有自动化能力缺失的响应统一返回 operation mode 和可识别的 fallback reason
  - **涉及文件**：`src/adapters/mcp/mod.rs`、`src/adapters/mcp/memory_tools.rs`、`src/application/errors.rs`
  - **涉及测试**：`tests/mcp_contract.rs` 扩展
  - **AC**：① 所有 `observe` / `session_end` / `search` 响应 envelope 增加 `operation_mode` 字段；② `fallback_reason` 在 capability 缺失时取 enum 值（`session_lifecycle_hook_unavailable` / `tool_capture_unavailable` / `context_injection_unavailable`）；③ 表格测试覆盖三个 reason 取值

## 3. 上下文连续性与 handoff

- [ ] 3.1 实现 `prepare_context`，按 project、task 和 token budget 检索并返回带 provenance 的有预算上下文包
  - **涉及文件**：`src/application/use_cases/prepare_context.rs`（新增）、`src/application/ports.rs`、`src/adapters/sqlite/memory_repository.rs`、`src/adapters/mcp/memory_tools.rs`、`src/domain/context.rs`（新增）
  - **涉及测试**：`tests/capability_profiles_context.rs`（新增）
  - **AC**：① 新增 `PrepareContextParams { project_id, task, token_budget, scope_filter }`；② 响应结构包含 `items[]`（每项带 `provenance`）与 `token_estimate`；③ 集成测试：填入 5 条 observation + 2 条 summary + 1 条 L2 fact，`token_budget=200` 时返回总字符数 ≤ 200（粗估 `4 chars ≈ 1 token`）；④ 返回 `operation_mode`
- [ ] 3.2 实现 authority 排序、token 估算和截断原因，确保完整 L1 历史不会作为默认启动上下文返回
  - **涉及文件**：`src/domain/context.rs`、`src/application/use_cases/prepare_context.rs`
  - **涉及测试**：`src/application/use_cases/prepare_context.rs` 内嵌测试
  - **AC**：① 排序顺序固定：当前用户指令 > 版本化项目规格 > 可验证工具事实 > Agent 摘要 > 旧 L1 观察 > 召回回填（spec `memory-provenance-and-isolation` Requirement "权威与冲突状态必须可见"）；② 当候选总 token 超过 budget 时 `truncation_reason` 取 `budget_exceeded_l1_observations_dropped`；③ 单元测试断言：100 条 L1 observation 与 1 条 L2 fact，`token_budget=50` 时只返回 L2 fact
- [ ] 3.3 实现带任务、状态、决策、阻塞项、下一步、文件和过期时间的 `checkpoint` handoff 模型
  - **涉及文件**：`src/domain/handoff.rs`（新增）、`src/application/use_cases/checkpoint.rs`（新增）、`src/migrations/v003__capability_profile_session.sql` 内增加 `handoffs` 表
  - **涉及测试**：`tests/capability_profiles_handoff.rs`（新增）
  - **AC**：① `CheckpointInput` 包含 `task`、`status ∈ {in_progress, blocked, completed}`、`decisions`、`blockers`、`next_steps`、`files`、`expires_at`；② `handoffs` 表含 `id`、`session_id`、`project_id`、`task`、`status`、`expires_at`、`created_at` 字段；③ 集成测试：写入一条 `status=in_progress, expires_at=now+7d` 后 `search_handoff(project_id, include_completed=false)` 能查到
- [ ] 3.4 为 `checkpoint` 实现幂等写入，并让未过期的 `in_progress`/`blocked` handoff 可被后续上下文恢复
  - **涉及文件**：`src/application/use_cases/checkpoint.rs`、`src/adapters/sqlite/memory_repository.rs`、`src/application/use_cases/prepare_context.rs`
  - **涉及测试**：`tests/capability_profiles_handoff.rs`
  - **AC**：① `CheckpointInput` 接受 `idempotency_key: Option<String>`；② 同 `(session_id, idempotency_key)` 重复提交返回首次 `handoff_id`；③ `prepare_context(project_id=…, task=…)` 默认把 `status IN ('in_progress','blocked')` 且 `expires_at > now` 的 handoff 装入 items 数组
- [ ] 3.5 确保 `completed` handoff 不进入默认恢复上下文，并为归档或过期 handoff 提供显式检索路径
  - **涉及文件**：`src/application/use_cases/prepare_context.rs`、`src/adapters/mcp/memory_tools.rs`（新增 `memora_get_handoff` tool）
  - **涉及测试**：`tests/capability_profiles_handoff.rs`
  - **AC**：① 准备上下文默认排除 `status='completed'` 或 `expires_at <= now` 的 handoff；② 新增 `memora_get_handoff(id)` 工具可显式拉取任一 handoff（含 completed/expired）；③ 集成测试：写入 completed handoff 后 `prepare_context` 不返回，但 `memora_get_handoff(id)` 仍可拉到

## 4. 隔离、提升与冲突可见性

- [ ] 4.1 在检索与上下文准备中默认按 session 隔离 L1，禁止仅因 project 相同而跨 Agent 自动共享 L1 观察
  - **涉及文件**：`src/application/use_cases/recent_observations.rs`、`src/application/use_cases/search.rs`、`src/application/use_cases/prepare_context.rs`、`src/adapters/sqlite/memory_repository.rs`
  - **涉及测试**：`tests/capability_profiles_isolation.rs`（新增）
  - **AC**：① `recent_observations(session_id=None)` 跨 session 查询需 `scope='session'` 才允许（默认行为），`scope='project'` 需显式参数；② `prepare_context` 默认仅当 observation `scope='project'` 且 `authority ≥ 'l2_fact'` 才装入；③ 跨 Agent 测试：Agent A 写 `scope='session'` observation，Agent B `prepare_context` 看不到
- [ ] 4.2 实现 `promote`，要求提升原因并保留原始 L1 record、agent 和 source refs 作为 L2 provenance
  - **涉及文件**：`src/application/use_cases/promote.rs`（新增）、`src/adapters/sqlite/memory_repository.rs`、`src/adapters/mcp/memory_tools.rs`
  - **涉及测试**：`tests/capability_profiles_promote.rs`（新增）
  - **AC**：① `PromoteInput { observation_id, reason }`，`reason` 非空校验；② 提升后新 `scope='project', kind='fact'` 行写入，`source_refs_json` 含原 `observation_id` 与 `agent_id`；③ 原 observation 仍存在且不被删除
- [ ] 4.3 对 `memora_recall` 来源通过原始 record ID 或内容哈希去重，阻止召回内容形成记忆回声
  - **涉及文件**：`src/application/use_cases/observe.rs`、`src/adapters/sqlite/memory_repository.rs`
  - **涉及测试**：`tests/capability_profiles_provenance.rs` 扩展
  - **AC**：① 当 `origin='memora_recall'` 且 `source_refs_json` 含 `observation_id` 时，repository 在 `INSERT` 前 SELECT 相同 id 已存在则返回现有行；② 同 `content_hash` 同 session 内重复 `memora_recall` 也走去重；③ 单元测试断言：两次写入只增加 1 行（去重生效）
- [ ] 4.4 实现相同 fact key 不同值的确定性冲突标记，并在响应中返回 authority、origin、conflict state 与 provenance
  - **涉及文件**：`src/application/use_cases/search.rs`、`src/domain/context.rs`、`src/adapters/sqlite/memory_repository.rs`
  - **涉及测试**：`tests/capability_profiles_conflict.rs`（新增）
  - **AC**：① 同 `fact_key` 不同值记录在 `search` 结果中并列返回且 `conflict_state="conflict"`；② 每项附带 `authority`、`origin`、`provenance` 字段；③ 单元测试：插入 A=`v1, authority=l1_observation`、B=`v2, authority=l2_fact`，搜索结果中 B 在前且两者都标 conflict
- [ ] 4.5 将版本化项目规格置于会话观察之前，但保持低权威冲突记录可审计且不被静默删除
  - **涉及文件**：`src/domain/context.rs`（authority 排序常量）、`src/application/use_cases/prepare_context.rs`、`src/application/use_cases/search.rs`
  - **涉及测试**：`tests/capability_profiles_conflict.rs` 扩展
  - **AC**：① `prepare_context` / `search` 返回顺序与 3.2 排序一致；② 单元测试断言：旧 L1 observation 不会因出现冲突被删除（DELETE 行为由 L1 既有契约保证）；③ authority 顺序常量为 pub 且文档化

## 5. Adapter 边界与验证

- [ ] 5.1 定义可选 client adapter 接口或配置生成边界，使 Hook、包装器和项目指令模板不改变核心记忆语义
  - **涉及文件**：`src/domain/capability.rs`（adapter port trait）、`src/adapters/mcp/mod.rs`
  - **涉及测试**：`src/domain/capability.rs` 内嵌 + `tests/capability_profiles_adapter.rs`
  - **AC**：① 定义 `trait CapabilityAdapter { fn name(&self) -> &str; fn render_instructions(&self, mode: OperationMode) -> String; }`；② trait 不携带任何 IO；③ 实现空 adapter `ManualInstructionsAdapter`，单元测试断言：输出字符串含 `operation_mode` 字面量与手动调用序列提示
- [ ] 5.2 为没有 Hook 的客户端提供可执行的手动启动、上下文准备、检查点和结束调用说明，不承诺自动捕获
  - **涉及文件**：`src/adapters/manual_instructions.rs`（新增）
  - **涉及测试**：`tests/capability_profiles_adapter.rs`
  - **AC**：① `render_instructions(StatelessManual)` 输出包含 `session_start` / `prepare_context` / `checkpoint` / `session_end` 四个 MCP tool 名；② 输出明确不含"自动捕获"承诺（grep `自动捕获|capture automatically` 中文/英文 0 命中）；③ golden test：固定输入 → 固定输出字符串
- [ ] 5.3 测试无能力声明、无原生记忆带 Hook、无原生记忆手动模式和原生记忆不透明模式的能力协商与降级响应
  - **涉及文件**：`tests/capability_profiles_negotiation.rs`（新增）
  - **涉及测试**：表格驱动 4 场景
  - **AC**：① 4 个独立 `#[test]` 函数，每个发送一组 `SessionStartParams`，断言响应 `operation_mode` 与 `fallback_reason` 字段；② CI `cargo test --test capability_profiles_negotiation` 必跑；③ 4 场景对应 capability-profiles spec `agent-memory-capability-profiles` 的 4 个 Scenario
- [ ] 5.4 测试无状态客户端的新会话恢复、检查点重试、超预算截断、陈旧会话归档和 completed handoff 排除
  - **涉及文件**：`tests/capability_profiles_recovery.rs` + `tests/capability_profiles_handoff.rs` + `tests/capability_profiles_context.rs`
  - **AC**：① 至少 5 个 `#[test]`：`recovery_same_external_ref`、`checkpoint_idempotency`、`prepare_context_truncation`、`stale_session_archive`、`completed_handoff_excluded_from_default_context`；② 每条测试同时断言响应 envelope 包含 `operation_mode`
- [ ] 5.5 测试跨 Agent L1 隔离、显式 L1 到 L2 提升、召回回声去重及 OpenSpec/会话观察冲突排序
  - **涉及文件**：`tests/capability_profiles_isolation.rs` + `tests/capability_profiles_promote.rs` + `tests/capability_profiles_conflict.rs`
  - **AC**：① 至少 4 个 `#[test]`：`cross_agent_l1_isolated`、`promote_preserves_provenance`、`recall_echo_dedup`、`openspec_vs_observation_conflict_ordering`；② 排序断言固定为 OpenSpec 规格在前；③ `promote_preserves_provenance` 断言 L2 行 `source_refs_json` 含 L1 `id` 与 `agent_id`
- [ ] 5.6 运行 Rust 单元测试、集成 MCP 测试和 OpenSpec 严格校验，并记录首个目标客户端 adapter 的实际能力验证结果
  - **涉及文件**：CI（`.github/workflows/*.yml` 已有则扩展）、`openspec/changes/add-agent-memory-capability-profiles/docs/adapter-validation.md`（新增）
  - **AC**：① `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 全绿；② `openspec validate add-agent-memory-capability-profiles --strict` 通过；③ `adapter-validation.md` 至少记录一个 adapter 的 capability 字段实测值（含 grep 命令输出）
