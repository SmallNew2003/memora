//! session_end use case —— 幂等更新 `ended_at` 与 `summary`。
//!
//! 边界（spec l1-session-memory "summaries 手动写入边界"）：
//! - 不传 `summary` MUST 仅更新 `ended_at`，不写 summaries 行。
//! - 多次调用只更新 `ended_at` 与 `summary`；不新增 session 行。
//! - `session_id` 不存在 MUST 返回 SessionNotFound。

use crate::application::errors::MemoryError;
use crate::application::ports::{MemoryRepository, SessionEndInput};
use crate::domain::Session;

/// `summary` 长度上限：与 content 同档（64 KiB），避免单行过大撑爆 FTS5 索引。
const SUMMARY_MAX: usize = 64 * 1024;

pub fn execute<R: MemoryRepository>(
    repo: &R,
    input: SessionEndInput,
) -> Result<Session, MemoryError> {
    if let Some(s) = input.summary.as_deref() {
        if s.len() > SUMMARY_MAX {
            return Err(MemoryError::InvalidInput(format!(
                "summary must be <= {SUMMARY_MAX} chars, got {}",
                s.len()
            )));
        }
    }
    repo.end_session(input)
}
