//! MCP adapter：把 RMCP transport 映射为 application query。
//!
//! 约束（design D2 / spec mcp-runtime-health）：
//! - 该模块 MUST NOT 直接打开数据库或执行 SQL。
//! - 所有 protocol 输出走 stdout，tracing / 诊断走 stderr。
//! - 仅注册只读 `memora_status` tool，不创建任何业务记录。
//!
//! 实现要点（design D12）：自定义 server metadata ⇒ 不使用
//! `#[tool_router(server_handler)]` 快捷形态，而显式标注 `#[tool_router]` +
//! `#[tool_handler] impl ServerHandler for ...`，并覆盖 `get_info` 暴露
//! 我们自己的 `Implementation` / `ServerCapabilities`。

use std::sync::Arc;

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo},
    schemars,
    service::ServiceExt,
    tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use serde::Deserialize;

use crate::adapters::sqlite::SqliteHealthRepository;
use crate::application::HealthService;
use crate::domain::RuntimeStatus;

/// `memora_status` tool 的入参：当前固定为空对象，预留未来扩展。
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct StatusParams {}

/// MCP server 实现：持有 `HealthService` 引用，通过 `ToolRouter` 注册 tool。
#[derive(Clone)]
pub struct MemoraServer {
    health: Arc<HealthService<SqliteHealthRepository>>,
}

impl MemoraServer {
    pub fn new(health: Arc<HealthService<SqliteHealthRepository>>) -> Self {
        Self { health }
    }

    /// 直接返回 `RuntimeStatus`，供集成测试断言使用。
    pub fn status(&self) -> RuntimeStatus {
        self.health.status()
    }
}

/// `tool_router` 宏生成 `MemoraServer::tool_router()` 并实现 `ToolRouter<MemoraServer>`。
#[tool_router]
impl MemoraServer {
    /// 只读 `memora_status` tool：返回五字段健康对象。
    /// 该 tool 直接委托给 application health query，不打开数据库、不写业务记录。
    #[tool(
        name = "memora_status",
        description = "Return runtime health snapshot for memora"
    )]
    fn memora_status(
        &self,
        Parameters(_params): Parameters<StatusParams>,
    ) -> Result<CallToolResult, McpError> {
        let status = self.health.status();
        let json = serde_json::to_value(&status).map_err(|err| {
            McpError::internal_error(format!("serialize status failed: {err}"), None)
        })?;
        Ok(CallToolResult::success(vec![ContentBlock::json(json)?]))
    }
}

/// 自定义 `ServerHandler`：覆盖默认 `get_info`，声明我们自己的 metadata。
#[tool_handler]
impl ServerHandler for MemoraServer {
    fn get_info(&self) -> ServerInfo {
        // InitializeResult 是 `#[non_exhaustive]`；必须走 builder。
        // 注意：Implementation::from_build_env() 在 rmcp crate 内部宏展开时，
        // `env!("CARGO_CRATE_NAME")` 解析为 `"rmcp"`（来自 RMCP 编译上下文）。
        // 我们必须显式传入 `"memora"`，避免暴露 SDK crate 名。
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(rmcp::model::ProtocolVersion::LATEST)
            .with_server_info(
                Implementation::new("memora", env!("CARGO_PKG_VERSION"))
                    .with_title("memora")
                    .with_description("Local-first multi-layer memory for AI coding agents"),
            )
            .with_instructions(
                "memora is a local-first multi-layer memory MCP server. Currently exposes a read-only memora_status tool.",
            )
    }
}

/// 启动 stdio MCP service：阻塞直到客户端关闭 stdin 或进程被终止。
///
/// 该函数由 `main` 在 composition root 之后调用。
pub async fn serve_stdio(server: MemoraServer) -> Result<(), McpError> {
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|err| McpError::internal_error(format!("stdio init failed: {err}"), None))?;
    service
        .waiting()
        .await
        .map_err(|err| McpError::internal_error(format!("service join failed: {err}"), None))?;
    Ok(())
}

/// 把 `RuntimeStatus` 序列化为 JSON 字符串，便于集成测试断言。
pub fn status_to_json(status: &RuntimeStatus) -> Result<String, serde_json::Error> {
    serde_json::to_string(status)
}
