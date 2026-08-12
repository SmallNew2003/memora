//! session_start use case —— 校验 `name` 后委托 repository。

use crate::application::errors::MemoryError;
use crate::application::ports::{MemoryRepository, SessionStartInput};
use crate::domain::Session;

/// `name` 长度上限：与 `idempotency_key` 同档（256 字符 ASCII 可打印），避免
/// 客户端滥用会话名撑爆数据库。
const NAME_MAX: usize = 256;

pub fn execute<R: MemoryRepository>(
    repo: &R,
    input: SessionStartInput,
) -> Result<Session, MemoryError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(MemoryError::InvalidInput(
            "name must not be empty".to_string(),
        ));
    }
    if name.len() > NAME_MAX {
        return Err(MemoryError::InvalidInput(format!(
            "name must be <= {NAME_MAX} chars, got {}",
            name.len()
        )));
    }
    if !name.is_ascii() {
        return Err(MemoryError::InvalidInput(
            "name must be ASCII printable".to_string(),
        ));
    }
    repo.start_session(SessionStartInput {
        name: name.to_string(),
    })
}
