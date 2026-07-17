## ADDED Requirements

### Requirement: 有预算的上下文准备
memora MUST 提供一种上下文准备接口，接受 session、项目、可选任务查询和正整数
`token_budget`。响应 MUST 在估算 token 总量不超过该预算的前提下，按 authority
顺序返回 L2 项目事实、未过期未完成 handoff 和任务相关的 L1 记录，并为每一项提供
来源信息。系统 MUST 返回实际估算量及任何截断原因。

#### Scenario: 无状态客户端开始新任务
- **WHEN** `stateless-manual` 客户端以 `token_budget: 1000` 请求项目任务的上下文包
- **THEN** 系统返回估算量不超过 1000 的上下文包、每项 provenance 和 `operation_mode: "stateless-manual"`

#### Scenario: 上下文候选超过预算
- **WHEN** 检索到的项目事实、handoff 和相关 L1 记录超过请求的 token budget
- **THEN** 系统优先保留 authority 较高的项目事实和未完成 handoff，并返回 `truncation_reason` 而不返回完整 L1 历史

### Requirement: 结构化检查点与 handoff
memora MUST 允许会话在结束前多次写入检查点。检查点 MUST 包含任务、状态
（`in_progress`、`blocked` 或 `completed`）、决策、阻塞项、下一步、可选变更文件和
过期时间。`in_progress` 或 `blocked` 的未过期检查点 MUST 可作为后续上下文准备的
handoff；`completed` 检查点 MUST NOT 作为默认 handoff 返回。

#### Scenario: 中断前记录工作交接
- **WHEN** 无状态客户端在未完成任务上写入 `in_progress` 检查点
- **THEN** 后续同项目的上下文准备可返回该 handoff，包含任务状态、阻塞项和下一步

#### Scenario: 已完成任务的检查点
- **WHEN** 客户端写入状态为 `completed` 的检查点
- **THEN** 默认上下文准备不将该检查点作为未完成 handoff 返回

### Requirement: 检查点与观察写入可幂等重试
`observe` 和 `checkpoint` MUST 接受客户端提供的 `idempotency_key`。同一 session 对
同一工具和 key 的重复调用 MUST 返回原先写入的记录，而不得创建重复记录。

#### Scenario: Hook 重试检查点
- **WHEN** 客户端因网络失败以同一 `idempotency_key` 重试 `checkpoint`
- **THEN** 系统返回首次创建的 checkpoint 标识，且该 session 中只存在一条对应 handoff

### Requirement: 未正常结束会话的恢复与归档
当客户端提供 `external_session_ref` 时，系统 MUST 将同一项目中该 ref 的未结束会话
恢复为原 session。无法通过 ref 恢复且超过配置的无活动期限的会话 MUST 归档为可显式
检索的历史，而 MUST NOT 自动作为新会话上下文注入。

#### Scenario: 客户端重启后恢复会话
- **WHEN** 客户端以同一项目和 `external_session_ref` 再次调用 `session_start`
- **THEN** 系统返回原未结束 session 的标识和恢复状态，而不是创建第二个活跃 session

#### Scenario: 无法恢复的陈旧会话
- **WHEN** 一个未结束 session 超过无活动期限且新客户端未提供匹配的 external session ref
- **THEN** 系统将其归档为历史记录，且默认上下文包不包含该会话的完整观察
