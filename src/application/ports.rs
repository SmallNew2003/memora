//! Application port —— `MemoryRepository` trait 与输入 DTO。
//!
//! 该模块定义 application 与 persistence adapter 之间的全部边界。
//! 返回值全部是 domain 值对象；入参是 application 层的 DTO（避免
//! adapter 直接耦合 MCP 参数结构）。

use crate::domain::{Observation, SearchHit, Session, SessionId};

use super::errors::MemoryError;

/// session_start 的入参。server 拥有主键与时间戳生成权（design D2）。
#[derive(Debug, Clone)]
pub struct SessionStartInput {
    pub name: String,
}

/// session_end 的入参。`summary == None` 时仅更新 `ended_at`。
#[derive(Debug, Clone)]
pub struct SessionEndInput {
    pub session_id: SessionId,
    pub summary: Option<String>,
}

/// observe 的入参。`idempotency_key == None` 视为一次性写入。
#[derive(Debug, Clone)]
pub struct ObserveInput {
    pub session_id: SessionId,
    pub content: String,
    pub tool_name: Option<String>,
    pub idempotency_key: Option<String>,
}

/// `kind` 过滤：`Both` = observation + summary 都返回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Observation,
    Summary,
    Both,
}

impl SearchKind {
    pub fn from_wire(value: Option<&str>) -> Result<Self, MemoryError> {
        match value.unwrap_or("both") {
            "observation" => Ok(SearchKind::Observation),
            "summary" => Ok(SearchKind::Summary),
            "both" => Ok(SearchKind::Both),
            other => Err(MemoryError::InvalidInput(format!(
                "kind must be observation|summary|both, got {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchInput {
    pub query: String,
    pub session_id: Option<SessionId>,
    pub kind: SearchKind,
    pub limit: u32,
}

/// 持久化层 port —— 6 个方法对应 6 个 L1 MCP tool。
///
/// 约束：
/// - 该 trait MUST NOT 暴露 rusqlite 句柄或 SQL 字符串；
/// - 所有 SQL 由 adapter 内部 prepared statement 承担；
/// - adapter 负责 SELECT-then-INSERT 实现 `idempotency_key` 幂等。
pub trait MemoryRepository: Send + Sync + 'static {
    fn start_session(&self, input: SessionStartInput) -> Result<Session, MemoryError>;
    fn end_session(&self, input: SessionEndInput) -> Result<Session, MemoryError>;
    fn observe(&self, input: ObserveInput) -> Result<Observation, MemoryError>;
    fn recent_observations(
        &self,
        session_id: Option<&SessionId>,
        limit: u32,
    ) -> Result<Vec<Observation>, MemoryError>;
    fn recent_sessions(&self, limit: u32) -> Result<Vec<Session>, MemoryError>;
    fn search(&self, input: SearchInput) -> Result<Vec<SearchHit>, MemoryError>;
}
