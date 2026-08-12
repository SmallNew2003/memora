//! 6 个 L1 MCP tool —— session_start / session_end / observe /
//! recent_observations / recent_sessions / search 的入参结构。
//!
//! 约束（spec l1-search-retrieval "响应字段稳定性"）：
//! - 所有 tool 仅调用 application `MemoryService`，不直接访问 SQLite。
//! - 错误响应统一走 `MemoryError::to_mcp_error`，message 绝不含绝对路径或
//!   未脱敏的 observation / summary 内容。
//! - tracing 走 stderr：debug 级别打 tool name + 入参摘要，warn / error 级别
//!   打错误。绝不写 stdout（spec rust-runtime-foundation "配置与协议日志隔离"）。

use rmcp::schemars;
use serde::Deserialize;

// ── 入参结构（每个 tool 对应一个），由 schemars 自动生成 JSON Schema ──

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionStartParams {
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionEndParams {
    pub session_id: String,
    /// 可选 summary 字符串；不传则仅更新 ended_at（spec l1-session-memory
    /// "summaries 手动写入边界"）。
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ObserveParams {
    pub session_id: String,
    pub content: String,
    pub tool_name: Option<String>,
    /// 可选幂等键；同一 (session_id, key) 重复提交返回首次行（spec
    /// l1-session-memory "observe 幂等性"）。
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RecentObservationsParams {
    /// 可选；不提供则跨 session 返回。
    pub session_id: Option<String>,
    /// 可选；默认 20，上限 100（spec "recent_observations 时间序接口"）。
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RecentSessionsParams {
    /// 可选；默认 20，上限 100。
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    pub query: String,
    pub session_id: Option<String>,
    /// "observation" | "summary" | "both"，默认 "both"。
    pub kind: Option<String>,
    pub limit: Option<u32>,
}
