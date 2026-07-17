## Context

memora 处于预实现阶段，工作区没有 Cargo manifest 或源文件。`target/` 构建残留表明
曾有 `main`、`engine`、`mcp` 和 `storage` 模块的试验，但其 RMCP 0.7 导入与服务
生命周期 API 不匹配，且源文件不再存在。本变更建立新的、可编译的起点，而不是恢复
不可验证的构建残留。

后续已规划的变化会增加 L1 会话记忆、客户端能力协商和 L2 OpenSpec 桥接。它们需要
稳定的 SQLite 迁移边界与 MCP transport 边界，但本变更不提前实现这些业务能力。

约束：
- 交付仍是 Local-First、单二进制、stdio MCP server；不引入常驻服务或网络依赖。
- 当前官方 RMCP 文档使用的稳定 server API 必须在实现时重新验证；不得复制旧
  `Cargo.lock` 的 RMCP 0.7 调用方式。
- SQLite 数据库只在显式运行服务器时初始化；测试必须使用临时数据库，不写默认
  用户数据目录。

## Goals / Non-Goals

**Goals:**
- 提供可重复构建、格式化、静态检查和测试的 Rust 二进制基础。
- 用模块边界隔离领域规则、应用用例、SQLite 持久化和 MCP 协议代码。
- 用版本化迁移安全地创建本地数据库，并提供端到端可验证的 MCP 健康状态工具。
- 为未来扩展预留 adapter 位置，但不在 crate 边界过早冻结公共 API。

**Non-Goals:**
- 实现 session、observation、摘要、搜索、FTS、embedding 或 sqlite-vec 虚拟表。
- 提供 HTTP/SSE transport、认证、多用户或网络同步。
- 实现客户端 Hook、OpenCode adapter 或原生记忆能力协商。
- 从 target 构建残留逆向恢复旧实现。
- 在初始版本拆分 Cargo workspace 或发布多个 crate。

## Decisions

### D1：单 crate 的模块化单体，而非一开始拆分 workspace

**选择**：初始化一个 `memora` 二进制 crate，模块为 `domain`、`application`、
`adapters::sqlite`、`adapters::mcp`、`config` 和 `app`。`main` 仅解析启动参数并
调用 composition root。依赖方向固定为：

```
main -> app -> adapters -> application -> domain
                    sqlite ----^       ^
```

`application` 定义 repository port；SQLite adapter 实现该 port；domain 与
application 不得依赖 RMCP、rusqlite、Tokio 或文件系统。

**理由**：单二进制的 Phase 1 需要低摩擦迭代；模块边界已足以阻止协议和 SQL 渗透到
领域规则。未来出现独立客户端 adapter 或可复用 library 的真实需求时，再以编译器
保障的 crate 边界拆分。

**替代方案考虑**：
- *四到五个 workspace crate*：边界更强，但在没有稳定公共 API 时增加循环依赖、
  feature 管理和发布负担。
- *只有 main/engine/mcp/storage 四个大模块*：容易让 MCP handler 直接编排 SQL，
  重演旧试验的耦合。

### D2：MCP transport 是 adapter，stdio 是唯一启用的首个 transport

**选择**：`adapters::mcp` 只负责将 RMCP 参数映射为 application command/query，
不得包含 SQL 或领域决策。首个 binary 仅支持 stdio；结构化协议输出只能写 stdout，
tracing 和诊断只能写 stderr。

**理由**：stdio 是跨 Coding Agent 的最小通用面，也能最快验证当前 RMCP server API。
stdout 污染会破坏 JSON-RPC，因此日志隔离是运行时正确性而非风格问题。

**替代方案考虑**：
- *同时实现 HTTP/SSE*：扩大测试矩阵，当前没有产品需求。
- *在 handler 中直接使用 SQLite connection*：短期更快，但会妨碍未来测试和
  transport 扩展。

### D3：使用经验证的 RMCP API，并以精确工具链和锁文件保证可重现构建

**选择**：实现前根据官方 RMCP Rust SDK 的 stable server 示例选择 RMCP 精确版本、
feature 和 MCP protocol version，并将它们记录在开发者说明中。`rust-toolchain.toml`
固定精确 Rust release，并包含 `rustfmt` 与 `clippy` 组件；`Cargo.lock` 是唯一可用
的依赖解析结果，所有解析依赖的质量门禁使用 `--locked`，不解析依赖的格式检查直接
运行。初始化改动必须包含一个真实 RMCP service
的编译和集成测试，而非只验证类型可导入。

**理由**：旧残留已证明 RMCP API 不能从过期示例拼接。服务启动与 tool registration
测试比依赖版本文字更能发现生命周期不兼容。

### D4：嵌入式、并发安全的 SQLite 迁移与显式数据库路径

**选择**：数据库路径优先读取 `MEMORA_DB_PATH`；缺失时解析到平台本地应用数据目录
下的 `memora/memora.db`。迁移以版本号和校验和记录在 `schema_migrations` 表中，
在 `BEGIN IMMEDIATE` 事务中按顺序应用。校验和为迁移嵌入 UTF-8 原始字节的 SHA-256，
不做换行或空白标准化。启动时先验证已记录迁移是当前 binary 迁移集合的连续前缀，
拒绝校验和漂移、版本缺口和高于当前 binary 的未知版本；然后启用 SQLite foreign
keys 并设置有限 busy timeout。遇到 `SQLITE_BUSY` 时，迁移以 100ms、300ms、900ms
的退避最多重试三次；仍未获得锁时返回可重试的启动错误。测试显式传入临时路径。

**理由**：显式覆盖便于测试和备份，默认路径保持零配置。迁移表同时提供已应用版本和
内容漂移的可诊断性；事务避免半迁移数据库对未来 Agent 造成隐性损坏。

**替代方案考虑**：
- *只使用 `PRAGMA user_version`*：无法记录迁移校验和，也不利于诊断。
- *首次运行时写入固定 SQL schema*：后续无安全升级路径。

### D5：每个 SQLite 操作使用独立连接并在 blocking 边界执行

**选择**：SQLite adapter 不在 async service state 中共享 `rusqlite::Connection`。
每个 repository 操作在 `tokio::task::spawn_blocking` 内打开、使用并关闭独立连接；
连接、statement 或 transaction MUST NOT 跨 `.await` 持有。启动迁移使用独占的短生命周期
连接，并在其完成前不构造可调用 MCP service。

**理由**：`rusqlite::Connection` 不是 Sync，直接置入 RMCP handler 会破坏 Send/Sync
要求或引入锁跨 await 的死锁风险。初期工作负载低，短连接的可预测性优于连接池。

**替代方案考虑**：
- *Mutex<Connection>*：需要严格的锁生命周期，易在未来 async 代码中阻塞 executor。
- *专用数据库 worker 或连接池*：在单工具 bootstrap 阶段没有足够收益，后续并发需求
  明确后可独立引入。

### D6：健康工具是首个垂直切片

**选择**：启动完成迁移后注册只读 `memora_status`。其 JSON 响应固定为：

```json
{
  "status": "healthy",
  "runtime_version": "0.1.0",
  "schema_version": 1,
  "database": "healthy",
  "transport": "stdio"
}
```

五个字段均为必需字段，`status` 和 `database` 初始只允许 `healthy`；响应不得包含
用户内容、绝对数据库路径或未定义的成功状态。tool 不创建业务会话、不写业务记录。

**理由**：这条细小路径覆盖配置、迁移、storage port、application query、MCP schema
与 transport，而不会抢占 L1 功能的设计空间。它也是用户和客户端排查安装状态的
稳定基础。

### D7：链接 bundled SQLite 并验证 FTS5 能力，但不创建 FTS 业务表

**选择**：初始 SQLite adapter 使用 rusqlite 的 bundled SQLite 构建，启动验证 SQLite
包含 FTS5 编译能力。基础 schema 不创建 FTS 或 memory/session 业务表。

**理由**：单二进制不能依赖宿主 SQLite 的编译选项；FTS5 是 Phase 1 已确定的后续存储
能力，应该在 foundation 阶段消除平台差异。

### D8：Phase 1 不加载 sqlite-vec

**选择**：初始 crate 只引入 SQLite/FTS 所需能力，不创建 vector virtual table，
不加载 sqlite-vec 扩展。向量依赖、加载顺序和 feature flag 留给 embedding 与
向量检索提案决定。

**理由**：当前 Phase 1 不生成 embedding；提前加载向量扩展只增加平台和启动故障面。
SQLite adapter 的隔离确保 Phase 2 能加入该能力而不改变 domain/application API。

## Risks / Trade-offs

- **[风险] 当前 RMCP API 再次发生变动。** → 锁定已验证版本，使用官方 service
  示例的实际编译与 MCP contract test 作为升级门禁。
- **[风险] SQLite 迁移中断或并发启动导致锁竞争。** → 单事务迁移、busy timeout、
  `BEGIN IMMEDIATE` 迁移锁与连续前缀检查；失败即中止服务，不在失败后启动半可用
  MCP server。
- **[风险] RMCP async handler 阻塞 Tokio executor。** → repository 操作在独立连接的
  `spawn_blocking` 边界执行，并测试并发 health 查询不会共享连接状态。
- **[风险] 默认数据库目录权限或系统目录不可写。** → 支持 `MEMORA_DB_PATH` 覆盖，
  并返回不含敏感路径的可操作启动错误。
- **[风险] 把初始模块误当成永久公共 API。** → 维持单 crate 与最小 `pub` 可见性，
  只在 adapter 边界和 application port 暴露必需类型。
- **[权衡] 首个 MCP tool 不提供记忆价值。** → 以小范围换取可验证基础；session
  工具在后续 L1 变更上实现，避免把未定的领域模型固化到脚手架。

## Migration Plan

1. 清点旧构建残留；保留文档和 target，不沿用其中 RMCP 0.7 依赖。以选定 manifest
   重新生成 orphaned `Cargo.lock`，使其成为新 crate 的受控锁文件。
2. 初始化 crate、精确 toolchain、模块骨架、质量命令和测试目录。
3. 建立配置解析、SQLite 打开与顺序迁移；数据库不存在时创建，已有兼容数据库时
   原样复用。
4. 实现 application health query、SQLite adapter 和 stdio `memora_status` tool。
5. 在干净工作目录上运行 fmt、clippy、单元测试和 MCP 集成测试，确认 binary 不会
   向 stdout 输出非协议日志。
6. 后续 L1 变更在这套 foundation 上添加 session schema 与 MCP tools；后续能力协商
   变更在其上添加 metadata，不重建项目结构。

**回滚策略**：本变更上线前没有旧运行时。若 migration 或 RMCP 集成有问题，停止
发布 binary；尚未打开的数据库不会被创建，迁移失败事务会回滚。已创建的空数据库可
保留，后续兼容版本继续使用。

## Open Questions

1. 默认目录是否使用 `dirs-next`，还是由 CLI 强制要求显式 `MEMORA_DB_PATH`？当前
   倾向前者以保持零配置。
2. 初始健康响应的运行时版本是编译期注入，还是从 `Cargo.toml` 读取？倾向编译期
   注入以避免运行时文件依赖。
3. 初始质量门禁是否把 `cargo deny` 与跨平台构建加入 CI，还是先保留给发布前变更？
