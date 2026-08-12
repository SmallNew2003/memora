//! Observation 值对象 —— 单条观察记录。
//!
//! 设计要点：
//! - 主键 `ObservationId` 由 server 用 UUIDv4 生成，调用方不传入。
//! - `idempotency_key` 是 Agent 在 Hook 重试场景下的唯一可控幂等手段；
//!   同一 `(session_id, idempotency_key)` 重复提交必须返回首次行（见 spec
//!   "observe 幂等性"）。
//! - `content_hash` 是 v002 预留列；v003（capability profile 1.4）起在 use case
//!   层用 SHA-256 计算并写入。
//! - `scope` / `kind` / `origin` / `authority` / `source_refs_json` /
//!   `expires_at` / `supersedes_id` / `fact_key` / `project_id` 是 v003 新增列，
//!   由 use case 层负责填默认值（缺失时回填）。

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

/// 单条 observation。`tool_name` 标识触发该观察的 MCP tool 名（可空）。
///
/// v003 字段语义：
/// - `content_hash`：use case 层入口 SHA-256 写入，hex 小写 64 字符；
/// - `scope`：作用域（`session` / `project` / `user`），use case 默认 `session`；
/// - `kind`：类型（`observation` / `summary`），use case 默认 `observation`；
/// - `origin`：来源（`user` / `tool_result` / `memora_recall` ...），use case 默认 `user`；
/// - `authority`：权威等级，use case 默认 `l1_observation`（summary 用 `l1_summary`）；
/// - `source_refs_json`：引用来源列表 JSON 数组字符串；
/// - `expires_at`：过期时间（ISO-8601）；
/// - `supersedes_id`：被取代 observation 的 id；
/// - `fact_key`：事实去重 key（同 session 唯一）；
/// - `project_id`：v002 预留，v003 起回写。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub id: ObservationId,
    pub session_id: SessionId,
    pub content: String,
    pub tool_name: Option<String>,
    pub created_at: String,

    // v002 已有列（v003 起回读）
    /// SHA-256(content) hex 小写 64 字符；use case 层入口计算。
    pub content_hash: Option<String>,

    // v003 新增列
    pub scope: Option<String>,
    pub kind: Option<String>,
    pub origin: Option<String>,
    pub project_id: Option<String>,
    pub authority: Option<String>,
    /// JSON 数组字符串（`serde_json::to_string(&Vec<String>)`）。
    pub source_refs_json: Option<String>,
    pub expires_at: Option<String>,
    pub supersedes_id: Option<String>,
    pub fact_key: Option<String>,
}
