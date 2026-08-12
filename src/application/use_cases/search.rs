//! search use case —— 校验 `query` 与 `kind` 后委托 repository。
//!
//! 排序（spec l1-search-retrieval "search 全文检索与 BM25 排序"）：
//! FTS5 MATCH + BM25 升序，分数越小越相关。

use crate::application::errors::MemoryError;
use crate::application::ports::{MemoryRepository, SearchInput, SearchKind};
use crate::application::use_cases::validate_limit;
use crate::domain::{SearchHit, SessionId};

const QUERY_MAX: usize = 1024;

pub fn execute<R: MemoryRepository>(
    repo: &R,
    query: String,
    session_id: Option<SessionId>,
    kind: SearchKind,
    limit: Option<u32>,
) -> Result<Vec<SearchHit>, MemoryError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(MemoryError::InvalidInput(
            "query must not be empty".to_string(),
        ));
    }
    if trimmed.len() > QUERY_MAX {
        return Err(MemoryError::InvalidInput(format!(
            "query must be <= {QUERY_MAX} chars, got {}",
            trimmed.len()
        )));
    }
    let limit = validate_limit(limit)?;
    repo.search(SearchInput {
        query: trimmed.to_string(),
        session_id,
        kind,
        limit,
    })
}
