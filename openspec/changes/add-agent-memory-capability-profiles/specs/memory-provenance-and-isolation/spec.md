## ADDED Requirements

### Requirement: 记忆记录携带来源与范围元数据
每条 memora 记录 MUST 持久化 `scope`、`kind`、`origin`、`agent_id`、`project_id`、
`content_hash`、`authority` 和 `created_at`。系统 MUST 支持可选的
`external_session_ref`、`source_refs`、`expires_at`、`supersedes` 和 `fact_key`。
`origin` MUST 至少区分 `user`、`tool_result`、`agent_summary` 和 `memora_recall`。

#### Scenario: 记录可审计的工具结果
- **WHEN** Agent 将带有文件路径 source ref 的工具结果写入 session
- **THEN** 系统存储该记录的 `origin: "tool_result"`、agent、项目、内容哈希和 source ref，并在检索响应中返回这些元数据

### Requirement: L1 默认隔离且 L2 显式提升
`scope: "session"` 的 L1 记录 MUST 只对所属 session 可见，除非被任务相关的上下文
准备作为受预算约束的候选选中。系统 MUST NOT 因为两个 Agent 位于同一项目而自动共享
L1 记录。记录只有经显式 `promote` 操作并提供提升原因后，才能成为
`scope: "project"` 的 L2 记录。

#### Scenario: 两个 Agent 在同一项目中工作
- **WHEN** Agent A 写入 L1 观察，Agent B 创建同一项目的新 session
- **THEN** Agent B 的默认上下文不包含该 L1 观察，除非存在已提升的项目记录或符合 handoff 选择规则的受预算结果

#### Scenario: 显式提升会话决策
- **WHEN** Agent 调用 `promote` 并提供 L1 记录标识和提升原因
- **THEN** 系统创建或更新关联的 L2 项目记录，并保留原始记录标识与来源作为 provenance

### Requirement: 召回内容不得形成记忆回声
对于 `origin: "memora_recall"` 的写入，系统 MUST 使用原始记录标识或内容哈希检测
重复。相同 session 内的重复召回写入 MUST 返回已有记录或拒绝写入，而不得创建新的
独立记忆。

#### Scenario: Agent 将刚召回的内容再次观察
- **WHEN** Agent 以 `origin: "memora_recall"` 写入与已召回记录相同的内容和来源标识
- **THEN** 系统返回去重结果，且不会增加新的可检索记忆条目

### Requirement: 权威与冲突状态必须可见
检索和上下文准备响应 MUST 为每条记录返回 `authority`、`origin` 和
`conflict_state`。系统 MUST 将当前用户指令、版本化项目规格、可验证工具事实、
Agent 摘要、旧 L1 观察和召回回填内容按此顺序排序。相同 `fact_key` 出现不同值时，
系统 MUST 标记冲突而不得静默覆盖或删除较低权威记录。

#### Scenario: 项目规格与旧会话观察冲突
- **WHEN** 项目规格记录与旧 L1 观察拥有相同 `fact_key` 但不同值
- **THEN** 响应将项目规格排在前面，并将两条记录的 `conflict_state` 标记为冲突及返回各自 provenance
