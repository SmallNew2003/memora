//! session_end use case —— 幂等更新 `ended_at` 与 `summary`。
//!
//! 边界（spec l1-session-memory "summaries 手动写入边界"）：
//! - 不传 `summary` MUST 仅更新 `ended_at`，不写 summaries 行。
//! - 多次调用只更新 `ended_at` 与 `summary`；不新增 session 行。
//! - `session_id` 不存在 MUST 返回 SessionNotFound。
//!
//! v003 落地（capability profile 1.4）：当 caller 提供 summary 时，
//! use case 在本层计算 SHA-256(content) 并填默认 provenance 字段
//! （origin='user' / scope='session' / kind='summary' / authority='l1_summary'
//! / source_refs_json='[]'），把 pre-computed 值塞进 SessionEndInput 透传给
//! adapter。adapter 只负责落库，不在 SQL 路径计算。

use sha2::{Digest, Sha256};

use crate::application::errors::MemoryError;
use crate::application::ports::{MemoryRepository, SessionEndInput};
use crate::domain::Session;

/// `summary` 长度上限：与 content 同档（64 KiB），避免单行过大撑爆 FTS5 索引。
const SUMMARY_MAX: usize = 64 * 1024;

const DEFAULT_ORIGIN: &str = "user";
const DEFAULT_SCOPE: &str = "session";
const DEFAULT_KIND_SUMMARY: &str = "summary";
const DEFAULT_AUTHORITY_SUMMARY: &str = "l1_summary";

pub fn execute<R: MemoryRepository>(
    repo: &R,
    input: SessionEndInput,
) -> Result<Session, MemoryError> {
    let (
        summary,
        summary_hash,
        summary_kind,
        summary_authority,
        summary_origin,
        summary_scope,
        summary_source_refs_json,
    ) = match input.summary.as_deref() {
        None => (None, None, None, None, None, None, None),
        Some(content) => {
            if content.len() > SUMMARY_MAX {
                return Err(MemoryError::InvalidInput(format!(
                    "summary must be <= {SUMMARY_MAX} chars, got {}",
                    content.len()
                )));
            }
            let hash = sha256_hex(content.as_bytes());
            (
                Some(content.to_string()),
                Some(hash),
                Some(DEFAULT_KIND_SUMMARY.to_string()),
                Some(DEFAULT_AUTHORITY_SUMMARY.to_string()),
                Some(DEFAULT_ORIGIN.to_string()),
                Some(DEFAULT_SCOPE.to_string()),
                Some("[]".to_string()),
            )
        }
    };

    repo.end_session(SessionEndInput {
        session_id: input.session_id,
        summary,
        summary_content_hash: summary_hash,
        summary_kind,
        summary_authority,
        summary_origin,
        summary_scope,
        summary_source_refs_json,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{
        MemoryRepository, ObserveInput, SearchInput, SessionStartInput,
    };
    use crate::domain::{Observation, SearchHit, Session, SessionId, SummaryId};

    struct CapturingRepo {
        last_input: std::sync::Mutex<Option<SessionEndInput>>,
    }

    impl CapturingRepo {
        fn new() -> Self {
            Self {
                last_input: std::sync::Mutex::new(None),
            }
        }
    }

    impl MemoryRepository for CapturingRepo {
        fn start_session(&self, _: SessionStartInput) -> Result<Session, MemoryError> {
            unreachable!()
        }

        fn end_session(&self, input: SessionEndInput) -> Result<Session, MemoryError> {
            *self.last_input.lock().unwrap() = Some(input.clone());
            Ok(Session {
                id: SessionId(input.session_id.0),
                name: String::new(),
                created_at: String::new(),
                ended_at: Some("2026-08-12T00:00:00.000Z".to_string()),
                summary: input.summary.clone(),
                agent_id: None,
                project_id: None,
                external_session_ref: None,
                capabilities_json: None,
                operation_mode: crate::domain::OperationMode::StatelessManual,
                last_active_at: "2026-08-12T00:00:00.000Z".to_string(),
                archived_at: None,
            })
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

    #[test]
    fn summary_sha256_and_defaults_are_computed() {
        let repo = CapturingRepo::new();
        execute(
            &repo,
            SessionEndInput {
                session_id: SessionId("sid".to_string()),
                summary: Some("wrap-up text".to_string()),
                summary_content_hash: None,
                summary_kind: None,
                summary_authority: None,
                summary_origin: None,
                summary_scope: None,
                summary_source_refs_json: None,
            },
        )
        .expect("end");
        let captured = repo.last_input.lock().unwrap().clone().expect("captured");
        let hash = captured.summary_content_hash.expect("hash computed");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        // SHA-256("wrap-up text") 锁定具体值（独立校验命令：
        // `echo -n "wrap-up text" | shasum -a 256`）。
        assert_eq!(
            hash,
            "c7864f349574bfa049c92aefc0e7fa1b3c84ae5b66da726f3be8983921b484bf"
        );
        assert_eq!(captured.summary_kind.as_deref(), Some("summary"));
        assert_eq!(captured.summary_authority.as_deref(), Some("l1_summary"));
        assert_eq!(captured.summary_origin.as_deref(), Some("user"));
        assert_eq!(captured.summary_scope.as_deref(), Some("session"));
        assert_eq!(captured.summary_source_refs_json.as_deref(), Some("[]"));
    }

    #[test]
    fn no_summary_means_no_hash_and_no_defaults() {
        let repo = CapturingRepo::new();
        execute(
            &repo,
            SessionEndInput {
                session_id: SessionId("sid".to_string()),
                summary: None,
                summary_content_hash: None,
                summary_kind: None,
                summary_authority: None,
                summary_origin: None,
                summary_scope: None,
                summary_source_refs_json: None,
            },
        )
        .expect("end");
        let captured = repo.last_input.lock().unwrap().clone().expect("captured");
        assert!(captured.summary_content_hash.is_none());
        assert!(captured.summary_kind.is_none());
        assert!(captured.summary_authority.is_none());
        assert!(captured.summary_origin.is_none());
        assert!(captured.summary_scope.is_none());
        assert!(captured.summary_source_refs_json.is_none());
    }

    #[test]
    fn summary_rejects_overlong_content() {
        let repo = CapturingRepo::new();
        let big = "x".repeat(SUMMARY_MAX + 1);
        assert!(matches!(
            execute(
                &repo,
                SessionEndInput {
                    session_id: SessionId("sid".to_string()),
                    summary: Some(big),
                    summary_content_hash: None,
                    summary_kind: None,
                    summary_authority: None,
                    summary_origin: None,
                    summary_scope: None,
                    summary_source_refs_json: None,
                }
            ),
            Err(MemoryError::InvalidInput(_))
        ));
    }

    #[allow(dead_code)]
    fn _silence_summaryid(_: SummaryId) {}
}
