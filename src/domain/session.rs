//! Session 值对象 —— L1 会话层。
//!
//! 仅承载字段与最小约束；server 拥有主键与时间戳生成权（详见 design D2）。
//! Domain 不引入 serde 派生以外的 IO。

use serde::{Deserialize, Serialize};

/// Session 主键的语义化包装。避免 stringly-typed 调用方错误地把
/// `observation_id` / `summary_id` 当作 session_id 传入。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 单次会话的领域值对象。`ended_at` / `summary` 在 `session_end` 之前为 None。
///
/// 预留字段 `agent_id` / `project_id` / `external_session_ref` 本变更 MUST NOT
/// 读取、不索引、不校验；保留给后续 capability profiles 变更叠加。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    /// UTC 秒级字符串（ISO-8601）。由 SQLite `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` 写入。
    pub created_at: String,
    /// session_end 之前为 None。
    pub ended_at: Option<String>,
    /// session_end 之前为 None；不传 summary 的 session_end 也会保留 None。
    pub summary: Option<String>,
}
