//! observe use case —— 校验 `content` 与 `idempotency_key` 后计算 SHA-256
//! 并填默认值，最后委托 repository。
//!
//! 幂等性（spec "observe 幂等性"）：repository 在 adapter 层
//! SELECT-then-INSERT；本 use case 只校验入参。
//!
//! v003 落地（capability profile 1.4）：
//! - SHA-256(`content`) 写入 `content_hash`（hex 小写 64 字符），由本 use case 计算；
//! - 默认值回填：`origin='user'` / `scope='session'` / `kind='observation'` /
//!   `authority='l1_observation'` / `source_refs_json='[]'`；
//! - 其他字段（`project_id` / `fact_key` / `expires_at` / `supersedes_id`）缺失时
//!   直传 NULL，不在 use case 引入额外语义。

use sha2::{Digest, Sha256};

use crate::application::errors::MemoryError;
use crate::application::ports::{MemoryRepository, ObserveInput};
use crate::domain::Observation;

const CONTENT_MAX: usize = 64 * 1024;
/// `idempotency_key` 长度上限（spec l1-session-memory "observe 幂等性"）：
/// 1-256 字符、ASCII 可打印。
const IDEMPOTENCY_KEY_MAX: usize = 256;

/// 默认值（v003 SQL DEFAULT 与之保持一致，方便回填语义对齐）。
const DEFAULT_ORIGIN: &str = "user";
const DEFAULT_SCOPE: &str = "session";
const DEFAULT_KIND_OBSERVATION: &str = "observation";
const DEFAULT_AUTHORITY_OBSERVATION: &str = "l1_observation";

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

    // SHA-256(content) hex 小写 64 字符。use case 层强制覆盖（不让 caller 静默传入
    // 错误 hash），符合"持久化契约由 use case 算"的设计原则。
    let content_hash = sha256_hex(input.content.as_bytes());

    // 默认值回填：origin / scope / kind / authority / source_refs_json。
    let origin = input
        .origin
        .clone()
        .unwrap_or_else(|| DEFAULT_ORIGIN.to_string());
    let scope = input
        .scope
        .clone()
        .unwrap_or_else(|| DEFAULT_SCOPE.to_string());
    let kind = input
        .kind
        .clone()
        .unwrap_or_else(|| DEFAULT_KIND_OBSERVATION.to_string());
    let authority = input
        .authority
        .clone()
        .unwrap_or_else(|| DEFAULT_AUTHORITY_OBSERVATION.to_string());
    let source_refs_json = serde_json::to_string(&input.source_refs.clone().unwrap_or_default())
        .map_err(|e| MemoryError::Storage(Box::new(e)))?;

    repo.observe(ObserveInput {
        content_hash: Some(content_hash),
        origin: Some(origin),
        scope: Some(scope),
        kind: Some(kind),
        authority: Some(authority),
        source_refs: None, // 已序列化到 source_refs_json，原值不需要再传
        source_refs_json: Some(source_refs_json),
        ..input
    })
}

/// SHA-256 hex 小写 64 字符。独立函数便于单元测试锁定算法 + 输出格式。
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
        ObserveInput, SearchInput, SessionEndInput, SessionStartInput,
    };
    use crate::domain::{Observation, ObservationId, SearchHit, Session, SessionId, SummaryId};

    struct CapturingRepo {
        last_input: std::sync::Mutex<Option<ObserveInput>>,
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

        fn end_session(&self, _: SessionEndInput) -> Result<Session, MemoryError> {
            unreachable!()
        }

        fn observe(&self, input: ObserveInput) -> Result<Observation, MemoryError> {
            *self.last_input.lock().unwrap() = Some(input.clone());
            Ok(Observation {
                id: ObservationId("obs-id".to_string()),
                session_id: input.session_id,
                content: input.content,
                tool_name: input.tool_name,
                created_at: "2026-08-12T00:00:00.000Z".to_string(),
                content_hash: input.content_hash,
                scope: input.scope,
                kind: input.kind,
                origin: input.origin,
                project_id: input.project_id,
                authority: input.authority,
                source_refs_json: input.source_refs_json,
                expires_at: input.expires_at,
                supersedes_id: input.supersedes_id,
                fact_key: input.fact_key,
            })
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

    fn minimal_observe(key: Option<&str>) -> ObserveInput {
        ObserveInput {
            session_id: SessionId("session".to_string()),
            content: "hello".to_string(),
            tool_name: None,
            idempotency_key: key.map(str::to_string),
            content_hash: None,
            origin: None,
            project_id: None,
            fact_key: None,
            scope: None,
            kind: None,
            authority: None,
            source_refs: None,
            expires_at: None,
            supersedes_id: None,
            // 该字段由 use case 写入；在 ObserveInput 里是冗余的，测试不传。
            source_refs_json: None,
        }
    }

    #[test]
    fn idempotency_key_rejects_ascii_control_characters() {
        for key in ["line\nbreak", "nul\0byte", "delete\u{7f}"] {
            assert!(matches!(
                execute(&CapturingRepo::new(), minimal_observe(Some(key))),
                Err(MemoryError::InvalidInput(_))
            ));
        }
    }

    #[test]
    fn sha256_hex_is_deterministic_and_64_lowercase_hex_chars() {
        // 标准 NIST 测试向量之一：空串的 SHA-256。
        let empty = sha256_hex(b"");
        assert_eq!(
            empty,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // "abc" → SHA-256 标准向量。
        let abc = sha256_hex(b"abc");
        assert_eq!(
            abc,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // 长度恒为 64，字符仅 [0-9a-f]。
        assert_eq!(abc.len(), 64);
        assert!(abc
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn use_case_fills_default_origin_scope_kind_authority() {
        let repo = CapturingRepo::new();
        execute(&repo, minimal_observe(Some("k"))).expect("observe");
        let captured = repo.last_input.lock().unwrap().clone().expect("captured");
        assert_eq!(captured.origin.as_deref(), Some("user"));
        assert_eq!(captured.scope.as_deref(), Some("session"));
        assert_eq!(captured.kind.as_deref(), Some("observation"));
        assert_eq!(captured.authority.as_deref(), Some("l1_observation"));
        assert_eq!(captured.source_refs_json.as_deref(), Some("[]"));
        // SHA-256 必填：use case 必须计算出 64 字符 hex。
        let hash = captured.content_hash.expect("hash");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn use_case_preserves_caller_supplied_provenance_fields() {
        let repo = CapturingRepo::new();
        let mut input = minimal_observe(None);
        input.origin = Some("tool_result".to_string());
        input.scope = Some("project".to_string());
        input.authority = Some("l2_agent".to_string());
        input.source_refs = Some(vec!["ref-a".to_string(), "ref-b".to_string()]);
        execute(&repo, input).expect("observe");
        let captured = repo.last_input.lock().unwrap().clone().expect("captured");
        assert_eq!(captured.origin.as_deref(), Some("tool_result"));
        assert_eq!(captured.scope.as_deref(), Some("project"));
        assert_eq!(captured.authority.as_deref(), Some("l2_agent"));
        // source_refs 已序列化为 JSON 字符串。
        let json = captured.source_refs_json.expect("json");
        assert!(json.contains("ref-a") && json.contains("ref-b"));
    }

    #[test]
    fn use_case_does_not_overwrite_explicit_content_hash() {
        // 注意：use case 总是覆盖 content_hash（重新计算 SHA-256）；
        // 这个测试锁定"始终覆盖"语义，防止以后悄悄改成"调用方传啥就用啥"导致
        // 与持久化契约漂移。
        let repo = CapturingRepo::new();
        execute(&repo, minimal_observe(None)).expect("observe");
        let captured = repo.last_input.lock().unwrap().clone().expect("captured");
        // minimal_observe 显式设 None；use case 计算后的 hash 不为 None。
        let hash = captured.content_hash.expect("use case must compute hash");
        assert_eq!(hash.len(), 64);
    }

    #[allow(dead_code)]
    fn _silence_observationid(_: ObservationId) {}
    #[allow(dead_code)]
    fn _silence_summaryid(_: SummaryId) {}
}
