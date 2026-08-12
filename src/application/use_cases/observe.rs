//! observe use case —— 校验 `content` 与 `idempotency_key` 后委托 repository。
//!
//! 幂等性（spec "observe 幂等性"）：repository 在 adapter 层
//! SELECT-then-INSERT；本 use case 只校验入参。

use crate::application::errors::MemoryError;
use crate::application::ports::{MemoryRepository, ObserveInput};
use crate::domain::Observation;

const CONTENT_MAX: usize = 64 * 1024;
/// `idempotency_key` 长度上限（spec l1-session-memory "observe 幂等性"）：
/// 1-256 字符、ASCII 可打印。
const IDEMPOTENCY_KEY_MAX: usize = 256;

pub fn execute<R: MemoryRepository>(
    repo: &R,
    input: ObserveInput,
) -> Result<Observation, MemoryError> {
    if input.content.is_empty() {
        return Err(MemoryError::InvalidInput(
            "content must not be empty".to_string(),
        ));
    }
    if input.content.len() > CONTENT_MAX {
        return Err(MemoryError::InvalidInput(format!(
            "content must be <= {CONTENT_MAX} chars, got {}",
            input.content.len()
        )));
    }
    if let Some(key) = input.idempotency_key.as_deref() {
        if key.is_empty() {
            return Err(MemoryError::InvalidInput(
                "idempotency_key must not be empty when present".to_string(),
            ));
        }
        if key.len() > IDEMPOTENCY_KEY_MAX {
            return Err(MemoryError::InvalidInput(format!(
                "idempotency_key must be <= {IDEMPOTENCY_KEY_MAX} chars, got {}",
                key.len()
            )));
        }
        if !key.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
            return Err(MemoryError::InvalidInput(
                "idempotency_key must contain only printable ASCII characters".to_string(),
            ));
        }
    }
    repo.observe(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{SearchInput, SessionEndInput, SessionStartInput};
    use crate::domain::{SearchHit, Session, SessionId};

    struct UnreachableRepo;

    impl MemoryRepository for UnreachableRepo {
        fn start_session(&self, _: SessionStartInput) -> Result<Session, MemoryError> {
            unreachable!()
        }

        fn end_session(&self, _: SessionEndInput) -> Result<Session, MemoryError> {
            unreachable!()
        }

        fn observe(&self, _: ObserveInput) -> Result<Observation, MemoryError> {
            unreachable!()
        }

        fn recent_observations(
            &self,
            _: Option<&SessionId>,
            _: u32,
        ) -> Result<Vec<Observation>, MemoryError> {
            unreachable!()
        }

        fn recent_sessions(&self, _: u32) -> Result<Vec<Session>, MemoryError> {
            unreachable!()
        }

        fn search(&self, _: SearchInput) -> Result<Vec<SearchHit>, MemoryError> {
            unreachable!()
        }
    }

    fn input(key: &str) -> ObserveInput {
        ObserveInput {
            session_id: SessionId("session".to_string()),
            content: "content".to_string(),
            tool_name: None,
            idempotency_key: Some(key.to_string()),
        }
    }

    #[test]
    fn idempotency_key_rejects_ascii_control_characters() {
        for key in ["line\nbreak", "nul\0byte", "delete\u{7f}"] {
            assert!(matches!(
                execute(&UnreachableRepo, input(key)),
                Err(MemoryError::InvalidInput(_))
            ));
        }
    }
}
