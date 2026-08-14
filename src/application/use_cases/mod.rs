//! Use case modules —— 每个 tool 对应一个文件。
//!
//! 每个文件暴露 `pub fn execute<R: MemoryRepository>(repo: &R, ...) -> Result<_, MemoryError>`，
//! 负责入参校验 + 委托给 repository。`MemoryService` 仅做薄壳封装，方便在
//! application / MCP adapter 共享同一份校验路径。

pub mod end_session;
pub mod observe;
pub mod recent_observations;
pub mod recent_sessions;
pub mod search;
pub mod start_session;

use super::errors::MemoryError;
use super::ports::{MemoryRepository, SearchKind, SessionStartOutput};
use crate::application::ports::{ObserveInput, SessionEndInput, SessionStartInput};
use crate::domain::{Observation, SearchHit, Session, SessionId};

/// 薄壳 service —— 直接转发到各 use case 模块。
///
/// 该结构体存在的目的是：
/// 1. 让 MCP adapter 持有一个 `Arc<MemoryService<R>>`，与既有 `HealthService`
///    形态一致；
/// 2. 共享 use case 之间的任何未来横切关注点（如全局 limit 上限、调用日志）；
/// 3. 单元测试可以通过 `MemoryService::new(fake_repo)` 注入 fake repository。
pub struct MemoryService<R: MemoryRepository> {
    repo: R,
}

impl<R: MemoryRepository> MemoryService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub fn start_session(
        &self,
        input: SessionStartInput,
        archive_after_seconds: u64,
    ) -> Result<SessionStartOutput, MemoryError> {
        start_session::execute(&self.repo, input, archive_after_seconds)
    }

    pub fn end_session(&self, input: SessionEndInput) -> Result<Session, MemoryError> {
        end_session::execute(&self.repo, input)
    }

    pub fn observe(&self, input: ObserveInput) -> Result<Observation, MemoryError> {
        observe::execute(&self.repo, input)
    }

    pub fn recent_observations(
        &self,
        session_id: Option<&SessionId>,
        limit: Option<u32>,
    ) -> Result<Vec<Observation>, MemoryError> {
        recent_observations::execute(&self.repo, session_id, limit)
    }

    pub fn recent_sessions(&self, limit: Option<u32>) -> Result<Vec<Session>, MemoryError> {
        recent_sessions::execute(&self.repo, limit)
    }

    pub fn search(
        &self,
        query: String,
        session_id: Option<SessionId>,
        kind: SearchKind,
        limit: Option<u32>,
    ) -> Result<Vec<SearchHit>, MemoryError> {
        search::execute(&self.repo, query, session_id, kind, limit)
    }

    pub fn find_session(&self, session_id: &SessionId) -> Result<Option<Session>, MemoryError> {
        self.repo.find_session(session_id)
    }
}

/// `limit` 上限：超过该值以 `InvalidInput` 拒绝，而不是悄悄截断
/// （spec l1-search-retrieval "超限拒绝" / "recent_observations 时间序接口" /
/// "recent_sessions 时间序接口"）。
pub(crate) const LIMIT_MAX: u32 = 100;
/// `limit` 默认值。
pub(crate) const LIMIT_DEFAULT: u32 = 20;

/// 校验并归一化 `limit`：0 / 缺失 → default；> max → InvalidInput。
pub(crate) fn validate_limit(limit: Option<u32>) -> Result<u32, MemoryError> {
    match limit {
        None => Ok(LIMIT_DEFAULT),
        Some(0) => Ok(LIMIT_DEFAULT),
        Some(n) if n > LIMIT_MAX => Err(MemoryError::InvalidInput(format!(
            "limit must be <= {LIMIT_MAX}, got {n}"
        ))),
        Some(n) => Ok(n),
    }
}
