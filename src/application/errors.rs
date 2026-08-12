//! Application 业务错误定义。
//!
//! 三条业务错误码（spec l1-search-retrieval "响应字段稳定性"）：
//! - `SessionNotFound`：session_id 在 sessions 表中不存在。
//! - `InvalidInput`：入参违反契约（空字符串、超长、limit 上限违反、kind 非法等）。
//! - `Storage`：底层 SQLite 故障；message MUST NOT 暴露绝对路径或原始内容。
//!
//! MemoryError MUST NOT 携带绝对数据库路径、未脱敏的 observation / summary 内容、
//! 本地文件系统信息；这些约束由 to_mcp_error 的 message 构造严格保证。

use rmcp::ErrorData as McpError;

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("storage error")]
    Storage(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl MemoryError {
    /// 业务码 → MCP error code。
    ///
    /// 复用 RMCP 内置的 INVALID_PARAMS / RESOURCE_NOT_FOUND / INTERNAL_ERROR；
    /// 同时携带自定义 code 字段以便调用方按业务类型分发。
    pub fn to_mcp_error(&self) -> McpError {
        match self {
            MemoryError::SessionNotFound(id) => McpError::resource_not_found(
                format!("session not found: {id}"),
                Some(serde_json::json!({ "code": "SESSION_NOT_FOUND" })),
            ),
            MemoryError::InvalidInput(msg) => McpError::invalid_params(
                msg.clone(),
                Some(serde_json::json!({ "code": "INVALID_INPUT" })),
            ),
            MemoryError::Storage(err) => {
                // message 仅暴露高层语义，绝不泄露 underlying 错误的内部路径或
                // 原始 payload。`%err` 走 Display，rusqlite / anyhow 都自带
                // sanitize，但仍走 fallback 防御。
                tracing::warn!(error = %err, "storage error");
                McpError::internal_error(
                    "storage error",
                    Some(serde_json::json!({ "code": "STORAGE_ERROR" })),
                )
            }
        }
    }
}

impl From<rusqlite::Error> for MemoryError {
    fn from(err: rusqlite::Error) -> Self {
        MemoryError::Storage(Box::new(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_errors_expose_stable_business_codes() {
        let cases = [
            (
                MemoryError::SessionNotFound("missing".to_string()),
                "SESSION_NOT_FOUND",
            ),
            (
                MemoryError::InvalidInput("bad input".to_string()),
                "INVALID_INPUT",
            ),
            (
                MemoryError::Storage(Box::new(std::io::Error::other("disk failure"))),
                "STORAGE_ERROR",
            ),
        ];

        for (error, expected) in cases {
            let mcp = error.to_mcp_error();
            assert_eq!(
                mcp.data
                    .as_ref()
                    .and_then(|data| data.get("code"))
                    .and_then(|code| code.as_str()),
                Some(expected)
            );
        }
    }
}
