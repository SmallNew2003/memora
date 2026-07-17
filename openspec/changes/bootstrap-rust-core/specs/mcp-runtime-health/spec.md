## ADDED Requirements

### Requirement: stdio MCP 生命周期
memora MUST 使用经当前稳定 RMCP server API 验证的 stdio transport 启动 MCP service。
成功启动前，系统 MUST 完成配置解析和数据库迁移；启动失败时 MUST 向 stderr 报告
可操作错误并不得发送伪造的 MCP success 响应。实现文档 MUST 记录经测试的 RMCP
版本、feature 和 MCP protocol version。

#### Scenario: 数据库准备完成后启动
- **WHEN** 配置有效且数据库迁移成功
- **THEN** server 在客户端 `initialize` 与 `notifications/initialized` 后响应 `tools/list`，其中包含已注册的 `memora_status`

#### Scenario: 数据库准备失败
- **WHEN** 数据库路径不可写或迁移失败
- **THEN** server 在 MCP 初始化前终止，并在 stderr 报告不泄漏记忆内容的错误

### Requirement: 只读运行时状态工具
server MUST 注册 `memora_status` tool。调用该 tool MUST 返回一个 JSON 对象，且五个
字段均为必需字段：`status`（字符串，初始值为 `healthy`）、`runtime_version`（字符串）、
`schema_version`（非负整数）、`database`（字符串，初始值为 `healthy`）和
`transport`（固定字符串 `stdio`）。MUST NOT 创建 session、observation、summary 或
其他业务记录，且 MUST NOT 返回绝对数据库路径或记忆内容。

#### Scenario: 客户端查询状态
- **WHEN** 已初始化的 MCP 客户端调用 `memora_status`
- **THEN** 系统返回 `{status: "healthy", runtime_version: <string>, schema_version: <non-negative integer>, database: "healthy", transport: "stdio"}`，并满足定义的字段类型

#### Scenario: 重复状态查询
- **WHEN** 同一客户端多次调用 `memora_status`
- **THEN** 系统返回一致的五字段健康对象，且业务记录数量不会因为查询而增加

### Requirement: MCP 健康路径具有端到端测试
项目 MUST 包含一个测试，覆盖从已迁移的临时 SQLite 数据库、application health query、
MCP tool 注册到 `memora_status` 响应的完整路径。测试 MUST 使用当前锁定 RMCP API，
按 `initialize -> notifications/initialized -> tools/list -> tools/call(memora_status)`
执行，并断言 tool schema、响应结构和 stdout 的每一行均为有效 JSON-RPC；诊断输出
只能出现在 stderr。

#### Scenario: 依赖升级后的契约验证
- **WHEN** RMCP 或相关 transport 依赖升级
- **THEN** MCP 健康路径测试验证 server 仍可注册并响应 `memora_status`，否则升级不得合并
