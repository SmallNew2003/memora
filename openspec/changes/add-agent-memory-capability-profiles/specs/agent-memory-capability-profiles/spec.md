## ADDED Requirements

### Requirement: 客户端能力声明与保守默认值
memora MUST 允许客户端在创建会话时声明 `native_memory`、`session_lifecycle`、
`tool_capture`、`context_injection` 和 `max_context_tokens`。初始支持的值分别为
`absent|opaque`、`manual|hook`、`none|manual|hook`、`manual|startup_hook` 和正整数。
客户端省略声明时，系统 MUST 使用 `stateless-manual` 的保守默认值，且 MUST NOT 假定
生命周期 Hook、工具拦截或自动上下文注入存在。

#### Scenario: 无能力声明的标准 MCP 客户端
- **WHEN** 客户端调用 `session_start` 且未提供 `client_capabilities`
- **THEN** 系统创建会话并返回 `operation_mode: "stateless-manual"`，且不声明任何自动捕获或自动注入已启用

#### Scenario: 声明无原生记忆且具备 Hook 的客户端
- **WHEN** 客户端声明 `native_memory: "absent"`、`session_lifecycle: "hook"` 和 `context_injection: "startup_hook"`
- **THEN** 系统返回 `operation_mode: "stateless-hooked"` 及该模式可调用的上下文准备与检查点能力

### Requirement: 运行模式不依赖客户端产品名称
memora MUST 仅根据声明的能力组合选择 `native-opaque`、`stateless-hooked` 或
`stateless-manual` 运行模式。系统 MUST NOT 将客户端产品名称作为选择记忆语义、
隔离边界或自动化保证的条件。

#### Scenario: 两个不同名称但能力相同的客户端
- **WHEN** 两个客户端提供不同的 `agent_id` 但相同的能力声明
- **THEN** 系统为两者选择相同的 `operation_mode` 和相同的 MCP 行为契约

### Requirement: 降级状态对调用方可见
当声明的能力无法满足自动化路径时，所有相关响应 MUST 包含 `operation_mode`，并在
适用时包含 `fallback_reason`。系统 MUST 提供自动化工具的手动等价调用路径。

#### Scenario: 无会话 Hook 的无状态客户端
- **WHEN** `native_memory` 为 `absent` 且 `session_lifecycle` 为 `manual`
- **THEN** 系统返回 `operation_mode: "stateless-manual"`、`fallback_reason: "session_lifecycle_hook_unavailable"`，并允许客户端显式调用会话、上下文和检查点工具

### Requirement: 原生记忆保持不透明
对于 `native_memory: "opaque"` 的客户端，memora MUST NOT 要求、读取、写入、同步或
推断该客户端的原生记忆内容。memora 返回的记录 MUST 只描述 memora 自己持有的来源。

#### Scenario: 有私有原生记忆的客户端读取上下文
- **WHEN** 声明 `native_memory: "opaque"` 的客户端请求 memora 上下文
- **THEN** 响应只返回 memora 记录及其 provenance，且不声称已与客户端原生记忆合并或同步
