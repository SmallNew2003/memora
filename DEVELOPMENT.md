# memora — 开发者文档

memora 是一个 Local-First 的多层记忆系统 MCP server，本仓库当前实现的是
[bootstrap-rust-core 提案](../openspec/changes/bootstrap-rust-core/) 的产物：
可重复构建、单二进制、stdio MCP runtime foundation。L1 会话记忆、
embedding、向量检索等业务能力必须由独立变更在本变更之上扩展。

## 1. 启动 stdio server

### 1.1 默认启动

```bash
cargo run --bin memora
```

启动后 memora 会在以下平台本地目录创建数据库（design D9）：

| 平台 | 路径 |
|:-----|:-----|
| macOS | `~/Library/Application Support/memora/memora.db` |
| Linux | `~/.local/share/memora/memora.db` |
| Windows | `%LOCALAPPDATA%/memora/memora.db` |

### 1.2 通过 `MEMORA_DB_PATH` 覆盖

```bash
MEMORA_DB_PATH=/tmp/memora-test.db cargo run --bin memora
```

集成测试 (`tests/mcp_contract.rs`) 必须使用临时路径，绝不能写入默认目录。

### 1.3 启用 tracing

tracing 默认级别 `info`；通过 `MEMORA_LOG` 覆盖（与 `tracing_subscriber::EnvFilter`
语法一致）：

```bash
MEMORA_LOG=debug cargo run --bin memora
```

所有 tracing / 诊断输出走 **stderr**；stdout 仅承载 JSON-RPC 协议消息
（spec rust-runtime-foundation "配置与协议日志隔离"）。

## 2. MCP 客户端调用

启动后，memora 注册一个只读 tool `memora_status`。最小客户端示例（stdio 模式）：

```text
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"my-client","version":"0.0.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memora_status","arguments":{}}}
```

`memora_status` 返回的五字段对象（spec mcp-runtime-health）：

```json
{
  "status": "healthy",
  "runtime_version": "0.1.0",
  "schema_version": 1,
  "database": "healthy",
  "transport": "stdio"
}
```

- `status` / `database` 当前只允许 `healthy`；
- `runtime_version` 编译期从 `Cargo.toml` 注入（design D10）；
- `schema_version` 等于当前 binary 内嵌迁移的最高版本号；
- `transport` 固定为 `"stdio"`，本变更不暴露其他 transport。

该 tool 是只读的：不会创建 session、observation、summary 或其他业务记录，
也不会在响应中泄露绝对数据库路径或记忆内容。

## 3. 质量门禁

三道本地门禁（design D11），**所有依赖解析必须使用 `--locked`**：

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

`rust-toolchain.toml` 固定 Rust release `1.94.0` 并启用 `rustfmt` / `clippy` 组件，
保证 CI 与本地一致。

`cargo deny` 与跨平台 CI 推迟到首个 L1 变更落地前的发布准备变更。

## 4. 锁定依赖

| 依赖 | 版本 | 用途 |
|:-----|:-----|:-----|
| `rmcp` | `=2.2.0`（features: `server`, `transport-io`, `macros`） | MCP 框架，锁定 RMCP 官方 `rmcp-v2.2.0` tag 验证过的 server API |
| `tokio` | `1.40` | RMCP 所需的异步运行时 |
| `rusqlite` | `0.32`（features: `bundled`） | bundled SQLite；单二进制不依赖宿主 SQLite |
| `dirs-next` | `2.0` | 跨平台本地数据目录 |
| `sha2` | `0.10` | 迁移 SHA-256 校验和 |

MCP protocol version：当前 RMCP 2.2.0 的 `ProtocolVersion::LATEST` 为 `2025-11-25`。
集成测试以该版本号发起 `initialize` 并断言协商结果。

`Cargo.lock` 由 cargo 按当前 manifest 重新生成，作为唯一可用的依赖解析结果。
启动期本仓库 `Cargo.lock` 不存在（无 manifest）时，初始化流程已删除孤立的
`Cargo.lock` 残留（task 1.1）。

## 5. 模块结构

```
src/
├── main.rs                    # 仅解析 env / 配置 / 启动 composition root
├── lib.rs                     # 公共模块入口
├── app/                       # composition root：装配 + 启动 stdio
├── config/                    # RuntimeConfig + MEMORA_DB_PATH 解析
├── domain/                    # 纯值对象（RuntimeStatus / Transport）
├── application/               # use case + HealthRepository port
├── migrations/                # 版本化迁移 + SHA-256 校验和
└── adapters/
    ├── sqlite/                # rusqlite 实现 HealthRepository
    └── mcp/                   # RMCP 实现 memora_status tool
```

依赖方向（design D1）：

```
main -> app -> adapters -> application -> domain
                    sqlite ----^       ^
```

`domain` 与 `application` MUST NOT 依赖 RMCP、rusqlite、Tokio 或文件系统。

## 6. 与后续变更的边界

本变更只为后续 L1 会话记忆、客户端能力协商与 L2 OpenSpec 桥接变更提供
**crate + 迁移基础**。session / observation / summary 业务 schema 与对应 MCP
工具必须由独立变更引入，不在本变更的范围内。`memora_status` 是当前唯一
tool，不应被视为永久公共 API 模板。