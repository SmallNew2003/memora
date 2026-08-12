//! Observation 值对象 —— 单条观察记录。
//!
//! 设计要点：
//! - 主键 `ObservationId` 由 server 用 UUIDv4 生成，调用方不传入。
//! - `idempotency_key` 是 Agent 在 Hook 重试场景下的唯一可控幂等手段；
//!   同一 `(session_id, idempotency_key)` 重复提交必须返回首次行（见 spec
//!   "observe 幂等性"）。
//! - `content_hash` 本变更不在写入时计算（design D5）。

use serde::{Deserialize, Serialize};

use super::session::SessionId;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObservationId(pub String);

impl ObservationId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ObservationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 单条 observation。`tool_name` 标识触发该观察的 MCP tool 名（可空）；
/// `idempotency_key` 与 `content_hash` 详见上注释。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub id: ObservationId,
    pub session_id: SessionId,
    pub content: String,
    pub tool_name: Option<String>,
    pub created_at: String,
}
