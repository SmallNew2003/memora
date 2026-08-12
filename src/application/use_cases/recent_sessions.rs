//! recent_sessions use case —— 校验 `limit` 后委托 repository。
//!
//! 排序（spec l1-search-retrieval "recent_sessions 时间序接口"）：
//! `(created_at DESC, id DESC)` 稳定排序，由 adapter 用索引保证。

use crate::application::errors::MemoryError;
use crate::application::ports::MemoryRepository;
use crate::application::use_cases::validate_limit;
use crate::domain::Session;

pub fn execute<R: MemoryRepository>(
    repo: &R,
    limit: Option<u32>,
) -> Result<Vec<Session>, MemoryError> {
    let limit = validate_limit(limit)?;
    repo.recent_sessions(limit)
}
