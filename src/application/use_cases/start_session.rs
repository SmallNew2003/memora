//! session_start use case —— 校验 `name` 后解析 capability profile 并委托 repository。
//!
//! Capability profile 解析（1.3）：
//! - `client_capabilities == None` → 保守路径：`operation_mode = stateless-manual`，
//!   `capabilities_json = NULL`（明确表达"调用方未声明"）。
//! - `Some(caps)` → 调 `resolve_operation_mode(&caps)` 决定 `operation_mode`
//!   字符串，并把 caps 序列化为 JSON 写入 `capabilities_json`。
//!
//! `name` 校验保持 L1 既有的 ASCII / 长度约束，不在 use case 层引入额外语义。

use crate::application::errors::MemoryError;
use crate::application::ports::{MemoryRepository, SessionStartInput};
use crate::domain::{resolve_operation_mode, OperationMode, Session};

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

    // 解析 capability profile：None ⇒ 保守路径 + capabilities_json IS NULL。
    let (operation_mode, capabilities_json) = match input.client_capabilities.as_ref() {
        None => (OperationMode::StatelessManual, None),
        Some(caps) => {
            let mode = resolve_operation_mode(caps);
            let json =
                serde_json::to_string(caps).map_err(|e| MemoryError::Storage(Box::new(e)))?;
            (mode, Some(json))
        }
    };

    repo.start_session(SessionStartInput {
        name: name.to_string(),
        agent_id: input.agent_id,
        project_id: input.project_id,
        external_session_ref: input.external_session_ref,
        client_capabilities: input.client_capabilities,
        operation_mode,
        capabilities_json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{
        ObserveInput, SearchInput, SessionEndInput, SessionStartInput,
    };
    use crate::domain::{
        ClientCapabilities, Observation, ObservationId, SearchHit, Session, SessionId, SummaryId,
        SESSION_LIFECYCLE_HOOK_TAG,
    };

    /// use case 测试用 fake —— 用 `last_input` 锁存最近一次收到的 SessionStartInput，
    /// 同时返回固定 Session；不再做"覆盖回来"的语义修改（语义全在 use case 入参上）。
    struct CapturingRepo {
        last_input: std::sync::Mutex<Option<SessionStartInput>>,
    }

    impl CapturingRepo {
        fn new() -> Self {
            Self {
                last_input: std::sync::Mutex::new(None),
            }
        }
    }

    impl MemoryRepository for CapturingRepo {
        fn start_session(&self, input: SessionStartInput) -> Result<Session, MemoryError> {
            *self.last_input.lock().unwrap() = Some(input.clone());
            Ok(Session {
                id: SessionId("sid".to_string()),
                name: input.name,
                created_at: "2026-08-12T00:00:00.000Z".to_string(),
                ended_at: None,
                summary: None,
                agent_id: input.agent_id,
                project_id: input.project_id,
                external_session_ref: input.external_session_ref,
                capabilities_json: input.capabilities_json.clone(),
                operation_mode: input.operation_mode,
                last_active_at: "2026-08-12T00:00:00.000Z".to_string(),
                archived_at: None,
            })
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

    fn input(name: &str, caps: Option<ClientCapabilities>) -> SessionStartInput {
        SessionStartInput {
            name: name.to_string(),
            agent_id: None,
            project_id: None,
            external_session_ref: None,
            client_capabilities: caps,
            operation_mode: OperationMode::StatelessManual,
            capabilities_json: None,
        }
    }

    #[test]
    fn absent_capabilities_yields_stateless_manual_and_null_json() {
        let repo = CapturingRepo::new();
        let session = execute(&repo, input("x", None)).expect("start");
        assert_eq!(session.operation_mode, OperationMode::StatelessManual);
        assert!(
            session.capabilities_json.is_none(),
            "absent caps ⇒ capabilities_json IS NULL"
        );

        // adapter 收到的入参必须把 `capabilities_json = None` 透传，
        // 不能在 use case 偷偷写入 `{}`。
        let captured = repo.last_input.lock().unwrap().clone().expect("captured");
        assert!(captured.capabilities_json.is_none());
        assert_eq!(captured.operation_mode, OperationMode::StatelessManual);
        assert!(
            captured.client_capabilities.is_none(),
            "use case must forward None to adapter so it can write IS NULL"
        );
    }

    #[test]
    fn hook_capabilities_resolve_to_stateless_hooked_with_serialized_json() {
        let repo = CapturingRepo::new();
        let session = execute(
            &repo,
            input(
                "x",
                Some(ClientCapabilities {
                    session_lifecycle: Some(SESSION_LIFECYCLE_HOOK_TAG.to_string()),
                    ..ClientCapabilities::default()
                }),
            ),
        )
        .expect("start");
        assert_eq!(session.operation_mode, OperationMode::StatelessHooked);
        let json = session.capabilities_json.expect("json written");
        assert!(
            json.contains(SESSION_LIFECYCLE_HOOK_TAG),
            "capabilities_json must carry lifecycle hook marker: {json}"
        );
    }

    #[test]
    fn non_default_capabilities_with_no_hook_or_opaque_falls_back_to_manual() {
        let repo = CapturingRepo::new();
        let session = execute(
            &repo,
            input(
                "x",
                Some(ClientCapabilities {
                    tool_capture: Some(true),
                    context_injection: Some(true),
                    ..ClientCapabilities::default()
                }),
            ),
        )
        .expect("start");
        assert_eq!(session.operation_mode, OperationMode::StatelessManual);
        // 但 caps 仍被序列化（声明了 tool_capture 字段）。
        assert!(session.capabilities_json.is_some());
    }

    #[allow(dead_code)]
    fn _silence_observationid(_: ObservationId) {}
    #[allow(dead_code)]
    fn _silence_summaryid(_: SummaryId) {}
}
