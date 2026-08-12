//! Session 值对象 —— L1 会话层。
//!
//! 仅承载字段与最小约束；server 拥有主键与时间戳生成权（详见 design D2）。
//! Domain 不引入 serde 派生以外的 IO。

use serde::{Deserialize, Serialize};

use super::capability::OperationMode;

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
/// 字段分布：
/// - 业务必填：`id` / `name` / `created_at` / `operation_mode` / `last_active_at`
/// - 业务可空：`ended_at` / `summary` / `archived_at`
/// - v002 预留（已有列、本变更补回读）：`agent_id` / `project_id` / `external_session_ref`
/// - v003 新增（capability profile 1.3 落地）：`capabilities_json` / `operation_mode` / `last_active_at` / `archived_at`
///
/// `capabilities_json` 仅在调用方显式声明 `client_capabilities` 时被填，
/// 旧 session 升级到 v003 时该列保持 NULL（v003 SQL 已 UPDATE 回填）。
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

    // ── v002 预留字段（v003 起开始回读；本变更不引入语义校验） ──
    /// v002 预留；1.3 起由 `SessionStartInput.agent_id` 回写。
    pub agent_id: Option<String>,
    /// v002 预留；1.3 起由 `SessionStartInput.project_id` 回写。
    pub project_id: Option<String>,
    /// v002 预留；1.3 起由 `SessionStartInput.external_session_ref` 回写。
    pub external_session_ref: Option<String>,

    // ── v003 新增字段（capability profile 1.3 落地） ──
    /// 调用方声明的 capability profile 序列化结果。
    /// `None` 表示调用方未声明任何 capability，对应保守的 `stateless-manual` 模式。
    pub capabilities_json: Option<String>,
    /// 由 `resolve_operation_mode` 解析出的运行模式。wire 字符串稳定契约。
    pub operation_mode: OperationMode,
    /// 最近一次观察 / 访问时间戳（UTC ISO-8601）；v003 默认 = `created_at`。
    pub last_active_at: String,
    /// 归档时间戳；`None` 表示未归档。2.4 才会真正消费，本变更只建索引。
    pub archived_at: Option<String>,
}
