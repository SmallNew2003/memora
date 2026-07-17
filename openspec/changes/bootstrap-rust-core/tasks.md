## 1. Crate 与工具链初始化

- [ ] 1.1 清点既有 OpenSpec 文档与构建残留，保留文档和 target；将没有 manifest 的历史 `Cargo.lock` 视为 superseded generated output，禁止沿用其 RMCP 0.7 依赖
- [ ] 1.2 初始化单一 `memora` 二进制 crate，添加精确固定的 `rust-toolchain.toml`（含 rustfmt、clippy）、`Cargo.toml` 与按 manifest 重新生成的锁文件
- [ ] 1.3 记录并配置经官方示例验证的精确 RMCP 版本、server/stdio feature 和 MCP protocol version，以及 Tokio、serde、bundled rusqlite、tracing 和测试依赖
- [ ] 1.4 添加质量命令或任务入口：执行 `cargo fmt --check`，并以 `--locked` 执行 `cargo clippy -- -D warnings` 与 `cargo test`
- [ ] 1.5 用最小 RMCP stdio service 编译验证已选版本与 feature，禁止从旧 RMCP 0.7 导入路径复制实现

## 2. 模块边界与应用骨架

- [ ] 2.1 建立 `domain`、`application`、`adapters::sqlite`、`adapters::mcp`、`config`、`app` 和 `main` 模块树，并最小化对外可见性
- [ ] 2.2 定义 application health query 及其 repository port，使其不依赖 RMCP、Tokio、rusqlite 或文件系统
- [ ] 2.3 在 composition root 组装配置、SQLite adapter、application service 与 MCP adapter，确保 main 不承载业务逻辑
- [ ] 2.4 配置 tracing 仅写 stderr，并添加回归测试或进程级验证，确保 stdio stdout 不含非协议日志
- [ ] 2.5 让每个 SQLite repository 操作在 `spawn_blocking` 中使用短生命周期独立连接，禁止连接、statement、transaction 或锁跨 `.await` 持有

## 3. SQLite 启动与迁移

- [ ] 3.1 实现数据库路径解析：优先 `MEMORA_DB_PATH`，否则创建平台本地 memora 数据目录；测试强制使用临时路径
- [ ] 3.2 实现 bundled SQLite 打开策略，验证 FTS5 能力，启用 foreign keys 和有限 busy timeout，并为不可写路径返回不泄漏业务数据的错误
- [ ] 3.3 实现包含版本与 SHA-256 原始 UTF-8 字节校验和的 `schema_migrations`，验证记录为当前 binary 迁移的连续前缀，并以 `BEGIN IMMEDIATE` 事务应用缺失迁移
- [ ] 3.4 为 `SQLITE_BUSY` 实现 100ms/300ms/900ms 的最多三次退避重试，并测试首次初始化、顺序升级、校验和漂移、未来未知版本、并发启动迁移和失败迁移回滚，确认超出预算返回可重试错误且不会启动 MCP service
- [ ] 3.5 保持本变更无 sqlite-vec、embedding、FTS 业务表和 memory/session 业务 schema

## 4. MCP 健康垂直切片

- [ ] 4.1 基于已验证 RMCP API 实现仅支持 stdio 的 server 生命周期，启动前完成配置和迁移
- [ ] 4.2 注册只读 `memora_status` tool，并通过 application health query 返回版本、schema version、数据库健康状态与 stdio transport 标识
- [ ] 4.3 确保 `memora_status` 不创建 session、observation、summary 或其他业务记录，也不暴露绝对数据库路径或记忆内容
- [ ] 4.4 编写端到端 MCP contract test，覆盖临时数据库迁移、`initialize -> notifications/initialized -> tools/list -> tools/call(memora_status)`、五字段响应及类型、重复调用无副作用和 stdout 每行合法 JSON-RPC

## 5. 验证与后续衔接

- [ ] 5.1 在干净工作目录上运行格式化、Clippy、单元测试和 MCP 集成测试并全部通过
- [ ] 5.2 记录本地 stdio 启动、`MEMORA_DB_PATH` 覆盖和质量命令的开发者说明
- [ ] 5.3 更新项目状态说明与开发者文档，记录 stdio 启动、`MEMORA_DB_PATH` 覆盖、质量命令、锁定 RMCP/MCP protocol 版本和本变更只提供 crate/迁移基础
- [ ] 5.4 确认后续 `add-agent-memory-capability-profiles` 的任务 1.1 仅有 crate/迁移前置部分由本变更提供；session/observation 基础存储仍必须由独立 L1 实现变更完成后才能勾选
- [ ] 5.5 运行 `openspec validate bootstrap-rust-core --strict`，确认提案制品与规格格式有效
