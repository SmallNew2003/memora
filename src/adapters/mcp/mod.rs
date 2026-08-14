//! MCP adapter：把 RMCP transport 映射为 application query。
//!
//! 约束（design D2 / spec mcp-runtime-health）：
//! - 该模块 MUST NOT 直接打开数据库或执行 SQL。
//! - 所有 protocol 输出走 stdout，tracing / 诊断走 stderr。
//! - 注册 7 个 tool：`memora_status`（只读健康）+ 6 个 L1 业务 tool。
//!
//! 实现要点（design D12）：自定义 server metadata ⇒ 不使用
//! `#[tool_router(server_handler)]` 快捷形态，而显式标注 `#[tool_router]` +
//! `#[tool_handler] impl ServerHandler for ...`，并覆盖 `get_info` 暴露
//! 我们自己的 `Implementation` / `ServerCapabilities`。

pub mod memory_tools;

use std::sync::Arc;

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo},
    schemars,
    service::ServiceExt,
    tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use serde::Deserialize;

use crate::adapters::mcp::memory_tools::{
    ObserveParams, RecentObservationsParams, RecentSessionsParams, SearchParams, SessionEndParams,
    SessionStartParams,
};
use crate::adapters::sqlite::SqliteHealthRepository;
use crate::application::ports::{
    MemoryRepository, ObserveInput, SearchKind, SessionEndInput, SessionStartInput,
};
use crate::application::{HealthService, MemoryService};
use crate::domain::{
    resolve_operation_mode, Observation, OperationMode, RuntimeStatus, Session, SessionId,
};

/// `memora_status` tool 的入参：当前固定为空对象，预留未来扩展。
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct StatusParams {}

/// MCP server 实现：持有 `HealthService` 与 `MemoryService` 引用，通过
/// `ToolRouter` 注册 7 个 tool。
///
/// 泛型参数 `R` 让测试可以注入 fake `MemoryRepository`；当前唯一 production
/// 实例是 `MemoraServer<SqliteMemoryRepository>`。
#[derive(Clone)]
pub struct MemoraServer<R: MemoryRepository + 'static> {
    health: Arc<HealthService<SqliteHealthRepository>>,
    memory: Arc<MemoryService<R>>,
    archive_after_seconds: u64,
}

impl<R: MemoryRepository + 'static> MemoraServer<R> {
    pub fn new(
        health: Arc<HealthService<SqliteHealthRepository>>,
        memory: Arc<MemoryService<R>>,
        archive_after_seconds: u64,
    ) -> Self {
        Self {
            health,
            memory,
            archive_after_seconds,
        }
    }

    /// 直接返回 `RuntimeStatus`，供集成测试断言使用。
    pub fn status(&self) -> RuntimeStatus {
        self.health.status()
    }
}

/// 把任意 `Serialize` 值封装为 `CallToolResult`，统一 success 路径。
fn success<T: serde::Serialize>(value: &T) -> Result<CallToolResult, McpError> {
    let json = serde_json::to_value(value)
        .map_err(|err| McpError::internal_error(format!("serialize failed: {err}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::json(json)?]))
}

/// 在 Tokio blocking pool 中执行同步 repository 操作，并统一转换业务错误。
async fn blocking_memory<T, F>(tool: &'static str, operation: F) -> Result<T, McpError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, crate::application::MemoryError> + Send + 'static,
{
    match tokio::task::spawn_blocking(operation).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => {
            tracing::warn!(tool, error = %err, "failed");
            Err(err.to_mcp_error())
        }
        Err(err) => {
            tracing::error!(tool, error = %err, "blocking repository task failed");
            Err(McpError::internal_error(
                "storage operation failed",
                Some(serde_json::json!({ "code": "STORAGE_ERROR" })),
            ))
        }
    }
}

/// `tool_router` 宏生成 `MemoraServer::tool_router()` 并实现 `ToolRouter<MemoraServer>`。
///
/// 所有 7 个 tool 都在同一个 impl block 内，`tool_router` 才能聚合它们的路由。
#[tool_router]
impl<R: MemoryRepository + 'static> MemoraServer<R> {
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

    /// 启动新会话；server 拥有主键与时间戳生成权（design D2）。
    #[tool(
        name = "session_start",
        description = "Start a new L1 session and return its server-generated id"
    )]
    async fn session_start(
        &self,
        Parameters(params): Parameters<SessionStartParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::debug!(
            tool = "session_start",
            name_len = params.name.len(),
            has_caps = params.client_capabilities.is_some(),
            "called"
        );

        let (operation_mode, fallback_reason) = match &params.client_capabilities {
            Some(caps) => {
                let (mode, reason) = resolve_operation_mode(caps);
                (mode, reason)
            }
            None => (OperationMode::StatelessManual, None),
        };

        let memory = Arc::clone(&self.memory);
        let archive_after_seconds = self.archive_after_seconds;
        let session_output = blocking_memory("session_start", move || {
            memory.start_session(
                SessionStartInput {
                    name: params.name,
                    agent_id: None,
                    project_id: None,
                    external_session_ref: params.external_session_ref,
                    client_capabilities: params.client_capabilities,
                    operation_mode,
                    capabilities_json: None,
                },
                archive_after_seconds,
            )
        })
        .await?;
        let mut payload = serde_json::json!({
            "session_id": session_output.session.id.0,
            "name": session_output.session.name,
            "created_at": session_output.session.created_at,
            "operation_mode": operation_mode.as_wire_str(),
            "fallback_reason": null,
            "recovered": session_output.recovered,
        });
        if let Some(reason) = fallback_reason {
            payload["fallback_reason"] = serde_json::json!(reason.as_wire_str());
        }
        success(&payload)
    }

    /// 结束会话并（可选）记录 summary。
    #[tool(
        name = "session_end",
        description = "End an L1 session; optionally persist a manual summary"
    )]
    async fn session_end(
        &self,
        Parameters(params): Parameters<SessionEndParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::debug!(tool = "session_end", session_id = %params.session_id, has_summary = params.summary.is_some(), "called");
        let sid = SessionId(params.session_id.clone());
        let memory = Arc::clone(&self.memory);
        let session = blocking_memory("session_end", move || {
            memory.end_session(SessionEndInput {
                session_id: sid,
                summary: params.summary,
                summary_content_hash: None,
                summary_kind: None,
                summary_authority: None,
                summary_origin: None,
                summary_scope: None,
                summary_source_refs_json: None,
            })
        })
        .await?;
        let payload = serde_json::json!({
            "session_id": session.id.0,
            "ended_at": session.ended_at,
            "summary": session.summary,
            "operation_mode": session.operation_mode.as_wire_str(),
        });
        success(&payload)
    }

    /// 写入一条 observation；可选幂等键。
    #[tool(
        name = "observe",
        description = "Record an observation in the current session; supports idempotency_key"
    )]
    async fn observe(
        &self,
        Parameters(params): Parameters<ObserveParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::debug!(
            tool = "observe",
            session_id = %params.session_id,
            content_len = params.content.len(),
            has_idem = params.idempotency_key.is_some(),
            "called"
        );
        let session_id = SessionId(params.session_id.clone());
        let memory = Arc::clone(&self.memory);
        let (obs, session): (Observation, Option<Session>) =
            blocking_memory("observe", move || {
                let obs = memory.observe(ObserveInput {
                    session_id: session_id.clone(),
                    content: params.content,
                    tool_name: params.tool_name,
                    idempotency_key: params.idempotency_key,
                    content_hash: None,
                    origin: None,
                    project_id: None,
                    fact_key: None,
                    scope: None,
                    kind: None,
                    authority: None,
                    source_refs: None,
                    source_refs_json: None,
                    expires_at: None,
                    supersedes_id: None,
                })?;
                let session = memory.find_session(&session_id)?;
                Ok((obs, session))
            })
            .await?;
        let operation_mode = session
            .as_ref()
            .map(|s| s.operation_mode)
            .unwrap_or(OperationMode::StatelessManual);
        let fallback_reason = if operation_mode == OperationMode::StatelessManual
            && session
                .as_ref()
                .and_then(|s| s.capabilities_json.as_deref())
                .is_none()
        {
            Some("tool_capture_unavailable")
        } else {
            None
        };
        let mut payload = serde_json::json!({
            "observation_id": obs.id.0,
            "session_id": obs.session_id.0,
            "content": obs.content,
            "tool_name": obs.tool_name,
            "created_at": obs.created_at,
            "operation_mode": operation_mode.as_wire_str(),
            "fallback_reason": null,
        });
        if let Some(reason) = fallback_reason {
            payload["fallback_reason"] = serde_json::json!(reason);
        }
        success(&payload)
    }

    /// 按 created_at DESC, id DESC 取最近 observation。
    #[tool(
        name = "recent_observations",
        description = "List recent observations optionally scoped to a session"
    )]
    async fn recent_observations(
        &self,
        Parameters(params): Parameters<RecentObservationsParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::debug!(
            tool = "recent_observations",
            session_id = ?params.session_id.as_deref(),
            limit = ?params.limit,
            "called"
        );
        let memory = Arc::clone(&self.memory);
        let rows = blocking_memory("recent_observations", move || {
            let session_id = params.session_id.map(SessionId);
            memory.recent_observations(session_id.as_ref(), params.limit)
        })
        .await?;
        let items: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|obs| {
                serde_json::json!({
                    "id": obs.id.0,
                    "session_id": obs.session_id.0,
                    "content": obs.content,
                    "tool_name": obs.tool_name,
                    "created_at": obs.created_at,
                })
            })
            .collect();
        let payload = serde_json::json!({
            "total": items.len(),
            "results": items,
        });
        success(&payload)
    }

    /// 按 created_at DESC, id DESC 取最近 session。
    #[tool(name = "recent_sessions", description = "List recent sessions")]
    async fn recent_sessions(
        &self,
        Parameters(params): Parameters<RecentSessionsParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::debug!(tool = "recent_sessions", limit = ?params.limit, "called");
        let memory = Arc::clone(&self.memory);
        let rows = blocking_memory("recent_sessions", move || {
            memory.recent_sessions(params.limit)
        })
        .await?;
        let items: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id.0,
                    "name": s.name,
                    "created_at": s.created_at,
                    "ended_at": s.ended_at,
                    "summary": s.summary,
                })
            })
            .collect();
        let payload = serde_json::json!({
            "total": items.len(),
            "results": items,
        });
        success(&payload)
    }

    /// FTS5 BM25 全文检索。
    #[tool(
        name = "search",
        description = "Full-text search over observations and summaries using BM25"
    )]
    async fn search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::debug!(
            tool = "search",
            query_len = params.query.len(),
            session_id = ?params.session_id.as_deref(),
            kind = ?params.kind,
            limit = ?params.limit,
            "called"
        );
        let kind = match SearchKind::from_wire(params.kind.as_deref()) {
            Ok(k) => k,
            Err(err) => {
                tracing::warn!(tool = "search", error = %err, "invalid kind");
                return Err(err.to_mcp_error());
            }
        };
        let search_sid = params.session_id.map(|s| SessionId(s.clone()));
        let memory = Arc::clone(&self.memory);
        let (hits, session): (Vec<crate::domain::SearchHit>, Option<Session>) =
            blocking_memory("search", move || {
                let hits = memory.search(params.query, search_sid.clone(), kind, params.limit)?;
                let session = match &search_sid {
                    Some(sid) => memory.find_session(sid)?,
                    None => None,
                };
                Ok((hits, session))
            })
            .await?;
        let operation_mode = session
            .as_ref()
            .map(|s| s.operation_mode)
            .unwrap_or(OperationMode::StatelessManual);
        let fallback_reason = if operation_mode == OperationMode::StatelessManual
            && session
                .as_ref()
                .and_then(|s| s.capabilities_json.as_deref())
                .is_none()
        {
            Some("context_injection_unavailable")
        } else {
            None
        };
        let items: Vec<serde_json::Value> = hits
            .into_iter()
            .map(|h| {
                let mut item = serde_json::json!({
                    "kind": h.kind,
                    "id": h.id,
                    "session_id": h.session_id.0,
                    "content": h.content,
                    "created_at": h.created_at,
                    "score": h.score,
                });
                if h.kind == "observation" {
                    item.as_object_mut()
                        .expect("search hit payload is an object")
                        .insert("tool_name".to_string(), serde_json::json!(h.tool_name));
                }
                item
            })
            .collect();
        let mut payload = serde_json::json!({
            "total": items.len(),
            "results": items,
            "operation_mode": operation_mode.as_wire_str(),
            "fallback_reason": null,
        });
        if let Some(reason) = fallback_reason {
            payload["fallback_reason"] = serde_json::json!(reason);
        }
        success(&payload)
    }
}

/// 自定义 `ServerHandler`：覆盖默认 `get_info`，声明我们自己的 metadata。
#[tool_handler]
impl<R: MemoryRepository + 'static> ServerHandler for MemoraServer<R> {
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
                "memora is a local-first multi-layer memory MCP server. Exposes memora_status and six L1 tools: session_start, session_end, observe, recent_observations, recent_sessions, search.",
            )
    }
}

/// 启动 stdio MCP service：阻塞直到客户端关闭 stdin 或进程被终止。
///
/// 该函数由 `main` 在 composition root 之后调用。
pub async fn serve_stdio<R: MemoryRepository + 'static>(
    server: MemoraServer<R>,
) -> Result<(), McpError> {
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
