# rust-runtime-foundation

## Purpose

定义 memora Rust 运行时的基础工程契约：可重复构建的单二进制 crate、固定的
toolchain 与质量命令、单向模块依赖方向、配置与协议日志隔离，以及历史锁文件的
处理规则。

## Requirements

### Requirement: 可重复构建的单二进制运行时
系统 MUST 提供一个可通过精确固定的 stable Rust toolchain 构建的 `memora` 二进制
crate，并提交 `Cargo.toml`、`Cargo.lock` 和 `rust-toolchain.toml`。toolchain 文件
MUST 固定 Rust release，并启用 `rustfmt` 和 `clippy` 组件。项目 MUST 提供格式化
检查，以及使用 `--locked` 的 Clippy（将 warning 视为错误）和测试命令。

#### Scenario: 干净检出构建运行时
- **WHEN** 开发者在干净检出上安装声明的 Rust toolchain 并执行项目记录的质量命令
- **THEN** `cargo fmt --check`、`cargo clippy --locked -- -D warnings` 和 `cargo test --locked` 均成功完成

### Requirement: 模块依赖方向保持单向
运行时 MUST 以 `domain`、`application`、`adapters::sqlite`、`adapters::mcp`、
`config` 和 `app` 组织。domain 与 application MUST NOT 直接依赖 RMCP、Tokio、
rusqlite、sqlite-vec 或文件系统 API；application MUST 通过 port trait 访问持久化。

#### Scenario: MCP handler 处理 application query
- **WHEN** MCP adapter 需要读取运行时健康状态
- **THEN** adapter 调用 application query 而不是直接打开数据库或执行 SQL

### Requirement: 配置与协议日志隔离
运行时 MUST 支持 `MEMORA_DB_PATH` 作为数据库路径覆盖；没有覆盖时 MUST 解析一个
平台本地的默认应用数据目录。stdio MCP 运行期间，JSON-RPC 协议输出 MUST 仅写入
stdout，tracing 和诊断输出 MUST 仅写入 stderr。

#### Scenario: 启用 tracing 的 stdio server
- **WHEN** 运行时以 stdio transport 启动且 tracing 被启用
- **THEN** stdout 只包含 MCP 协议消息，诊断日志不会使客户端解析 stdout 失败

#### Scenario: 使用临时数据库运行测试
- **WHEN** 集成测试设置 `MEMORA_DB_PATH` 为临时文件
- **THEN** 运行时只使用该临时文件而不会创建或修改默认用户数据目录

### Requirement: 旧锁文件不得决定新运行时依赖
工作区中没有对应 Cargo manifest 的历史 `Cargo.lock` MUST 被视为生成残留。初始化时，
系统 MUST 以新的 manifest 和验证过的 RMCP 版本重新生成锁文件；实现文档 MUST 记录
所选 RMCP 版本、feature 与 MCP protocol version。

#### Scenario: 旧锁文件包含过期 RMCP
- **WHEN** 初始化前存在锁定旧 RMCP 版本的 orphaned `Cargo.lock`
- **THEN** 新 crate 的锁文件由当前 manifest 解析生成，且 MCP health contract test 使用该锁定版本通过
