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
use crate::application::ports::{MemoryRepository, SessionStartInput, SessionStartOutput};
use crate::domain::{resolve_operation_mode, OperationMode, Session};

/// `name` 长度上限：与 `idempotency_key` 同档（256 字符 ASCII 可打印），避免
/// 客户端滥用会话名撑爆数据库。
const NAME_MAX: usize = 256;

pub fn execute<R: MemoryRepository>(
    repo: &R,
    input: SessionStartInput,
    archive_after_seconds: u64,
) -> Result<SessionStartOutput, MemoryError> {
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

    // 2.4: 会话恢复 —— 同 (project_id, external_session_ref) 且未归档的 session 直接复用。
    if let (Some(pid), Some(esr)) = (&input.project_id, &input.external_session_ref) {
        if let Some(existing) = repo.find_active_session_by_project_and_ref(Some(pid), Some(esr))? {
            if !is_expired(&existing.last_active_at, archive_after_seconds) {
                return Ok(SessionStartOutput {
                    session: existing,
                    recovered: true,
                });
            }
            // 过期会话：归档后创建新 session。
            let now = now_utc_iso();
            repo.archive_session(&existing.id, &now)?;
            return create_new_session(repo, input, archive_after_seconds);
        }
    }

    // 解析 capability profile 并创建新 session。
    create_new_session(repo, input, archive_after_seconds)
}

fn create_new_session<R: MemoryRepository>(
    repo: &R,
    input: SessionStartInput,
    _archive_after_seconds: u64,
) -> Result<SessionStartOutput, MemoryError> {
    let (operation_mode, capabilities_json) = match input.client_capabilities.as_ref() {
        None => (OperationMode::StatelessManual, None),
        Some(caps) => {
            let (mode, _fallback_reason) = resolve_operation_mode(caps);
            let json =
                serde_json::to_string(caps).map_err(|e| MemoryError::Storage(Box::new(e)))?;
            (mode, Some(json))
        }
    };

    let session = repo.start_session(SessionStartInput {
        name: input.name,
        agent_id: input.agent_id,
        project_id: input.project_id,
        external_session_ref: input.external_session_ref,
        client_capabilities: input.client_capabilities,
        operation_mode,
        capabilities_json,
    })?;

    Ok(SessionStartOutput {
        session,
        recovered: false,
    })
}

/// 查找所有同 (project_id, external_session_ref) 的 session（不限 archived_at）。
#[allow(dead_code)]
fn find_all_by_project_and_ref<R: MemoryRepository>(
    repo: &R,
    project_id: &str,
    external_session_ref: &str,
) -> Result<Vec<Session>, MemoryError> {
    // 通过 recent_sessions 获取所有 session，然后过滤。
    // 简单实现：使用 find_active_session 的变体，不限制 archived_at。
    // 这里我们借助 repository 的 find_active_session_by_project_and_ref 已经筛选了
    // archived_at IS NULL。对于归档检查，我们需要一个不限制 archived_at 的查询。
    // 简化：直接读取所有 sessions 并过滤（项目规模小，可行）。
    let all = repo.recent_sessions(1000)?;
    Ok(all
        .into_iter()
        .filter(|s| {
            s.project_id.as_deref() == Some(project_id)
                && s.external_session_ref.as_deref() == Some(external_session_ref)
        })
        .collect())
}

/// 判断 last_active_at 是否超出 archive_after_seconds。
fn is_expired(last_active_at: &str, archive_after_seconds: u64) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // 解析 ISO-8601: "2026-08-12T00:00:00.000Z"
    let parsed = parse_iso_secs(last_active_at);
    match parsed {
        Some(secs) => now.saturating_sub(secs) > archive_after_seconds,
        None => false,
    }
}

fn parse_iso_secs(s: &str) -> Option<u64> {
    // 简单解析 "YYYY-MM-DDTHH:MM:SS.mmmZ" 的前 19 个字符
    if s.len() < 19 {
        return None;
    }
    let year: i32 = s[0..4].parse().ok()?;
    let month: u32 = s[5..7].parse().ok()?;
    let day: u32 = s[8..10].parse().ok()?;
    let hour: u32 = s[11..13].parse().ok()?;
    let min: u32 = s[14..16].parse().ok()?;
    let sec: u32 = s[17..19].parse().ok()?;
    // 简化：计算从 1970-01-01 起的天数
    let days = days_from_civil(year, month, day);
    let total_secs = days as u64 * 86400 + hour as u64 * 3600 + min as u64 * 60 + sec as u64;
    Some(total_secs)
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i32 {
    let y = y - (m <= 2) as i32;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m <= 2 { m + 9 } else { m - 3 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i32 - 719468
}

fn now_utc_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let secs = (nanos / 1_000_000_000) as u64;
    let millis = ((nanos % 1_000_000_000) / 1_000_000) as u32;
    let (year, month, day, hour, min, sec) = unix_secs_to_ymdhms(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}.{millis:03}Z")
}

fn unix_secs_to_ymdhms(mut secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let second = (secs % 60) as u32;
    secs /= 60;
    let minute = (secs % 60) as u32;
    secs /= 60;
    let hour = (secs % 24) as u32;
    let mut days = (secs / 24) as i64;
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = (y + (m <= 2) as i64) as i32;
    (y, m, d, hour, minute, second)
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

        fn find_session(&self, _: &SessionId) -> Result<Option<Session>, MemoryError> {
            unreachable!()
        }

        fn find_by_session_idempotency_and_hash(
            &self,
            _: &SessionId,
            _: &str,
            _: &str,
        ) -> Result<Option<ObservationId>, MemoryError> {
            unreachable!()
        }

        fn find_active_session_by_project_and_ref(
            &self,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<Option<Session>, MemoryError> {
            Ok(None)
        }

        fn archive_session(&self, _: &SessionId, _: &str) -> Result<(), MemoryError> {
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
        let output = execute(&repo, input("x", None), 2592000).expect("start");
        let session = output.session;
        assert_eq!(session.operation_mode, OperationMode::StatelessManual);
        assert!(
            session.capabilities_json.is_none(),
            "absent caps ⇒ capabilities_json IS NULL"
        );

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
        let output = execute(
            &repo,
            input(
                "x",
                Some(ClientCapabilities {
                    session_lifecycle: Some(SESSION_LIFECYCLE_HOOK_TAG.to_string()),
                    ..ClientCapabilities::default()
                }),
            ),
            2592000,
        )
        .expect("start");
        let session = output.session;
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
        let output = execute(
            &repo,
            input(
                "x",
                Some(ClientCapabilities {
                    tool_capture: Some(true),
                    context_injection: Some(true),
                    ..ClientCapabilities::default()
                }),
            ),
            2592000,
        )
        .expect("start");
        let session = output.session;
        assert_eq!(session.operation_mode, OperationMode::StatelessManual);
        assert!(session.capabilities_json.is_some());
    }

    #[allow(dead_code)]
    fn _silence_observationid(_: ObservationId) {}
    #[allow(dead_code)]
    fn _silence_summaryid(_: SummaryId) {}
}
