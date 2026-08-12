//! 集成测试 —— sessions v003 扩展列（task 1.3）。
//!
//! 覆盖：
//! 1. `start_session(name)` 不带 capability 声明 → `operation_mode = stateless-manual`
//!    且 `capabilities_json IS NULL`；
//! 2. `start_session(name, ClientCapabilities { session_lifecycle = "hook" })` →
//!    `operation_mode = stateless-hooked` 且 `capabilities_json` 写入非 NULL；
//! 3. 回读 `Session` 时，v003 新增的 4 列 + v002 预留的 3 列都能拿到；
//! 4. 同 `(project_id, external_session_ref)` 两次 start_session 验证 L1 行为未被破。
//!
//! 该测试独立文件，避免污染 `memory_repository::tests` 模块。

use memora::adapters::sqlite::SqliteMemoryRepository;
use memora::application::ports::{
    MemoryRepository, ObserveInput, SessionEndInput, SessionStartInput,
};
use memora::domain::{ClientCapabilities, OperationMode, SessionId, SESSION_LIFECYCLE_HOOK_TAG};

/// 测试 helper：构造一个 `operation_mode = stateless-manual` / 不带 capability 声明的
/// `SessionStartInput`（与单元测试 helper 同步）。
fn minimal_session_start(name: &str) -> SessionStartInput {
    SessionStartInput {
        name: name.to_string(),
        agent_id: None,
        project_id: None,
        external_session_ref: None,
        client_capabilities: None,
        operation_mode: OperationMode::StatelessManual,
        capabilities_json: None,
    }
}

/// 测试 helper：模拟「已经过 use case 填默认值」的状态（与 `observe::execute`
/// 一致）。v003 起 schema 上 `scope / kind / origin / authority /
/// source_refs_json` 是 NOT NULL DEFAULT；helper 直接填默认值让 adapter
/// 不依赖 SQLite DEFAULT 兜底，保证测试确定性。
fn minimal_observe(session_id: SessionId, content: &str) -> ObserveInput {
    ObserveInput {
        session_id,
        content: content.to_string(),
        tool_name: None,
        idempotency_key: None,
        content_hash: None,
        origin: Some("user".to_string()),
        project_id: None,
        fact_key: None,
        scope: Some("session".to_string()),
        kind: Some("observation".to_string()),
        authority: Some("l1_observation".to_string()),
        source_refs: None,
        source_refs_json: Some("[]".to_string()),
        expires_at: None,
        supersedes_id: None,
    }
}

/// 返回 `(TempDir, repo)` —— 测试函数必须持有 `_dir` 以保证 db 路径在测试期间存在。
fn repo() -> (tempfile::TempDir, SqliteMemoryRepository) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("memora.db");
    memora::adapters::sqlite::open_and_migrate(&db).expect("migrate");
    let repository = SqliteMemoryRepository::bootstrap(db);
    (dir, repository)
}

#[test]
fn absent_capabilities_yields_stateless_manual_and_null_json() {
    let (_dir, r) = repo();
    let session = r.start_session(minimal_session_start("x")).expect("start");
    assert_eq!(session.name, "x");
    assert_eq!(session.operation_mode, OperationMode::StatelessManual);
    assert!(
        session.capabilities_json.is_none(),
        "absent caps ⇒ capabilities_json IS NULL"
    );
    assert!(
        session.archived_at.is_none(),
        "archived_at must be NULL on create"
    );
    assert!(
        session.last_active_at.ends_with('Z'),
        "last_active_at must be ISO-8601 UTC"
    );
}

#[test]
fn hook_capabilities_resolve_to_stateless_hooked_with_serialized_json() {
    let (_dir, r) = repo();
    let input = SessionStartInput {
        name: "x".to_string(),
        agent_id: None,
        project_id: None,
        external_session_ref: None,
        client_capabilities: Some(ClientCapabilities {
            session_lifecycle: Some(SESSION_LIFECYCLE_HOOK_TAG.to_string()),
            ..ClientCapabilities::default()
        }),
        // adapter 走裸路径（不走 use case），这里显式给出 use case 应当解析出的值，
        // 验证"wire 字符串 + JSON 序列化"落库正确。
        operation_mode: OperationMode::StatelessHooked,
        capabilities_json: Some(
            serde_json::json!({
                "session_lifecycle": SESSION_LIFECYCLE_HOOK_TAG
            })
            .to_string(),
        ),
    };
    let session = r.start_session(input).expect("start");
    assert_eq!(session.operation_mode, OperationMode::StatelessHooked);
    let json = session
        .capabilities_json
        .as_deref()
        .expect("capabilities_json must be written when caps present");
    assert!(
        json.contains(SESSION_LIFECYCLE_HOOK_TAG),
        "serialized JSON must carry lifecycle hook marker: {json}"
    );
}

#[test]
fn v002_reserved_fields_round_trip_on_session() {
    let (_dir, r) = repo();
    let input = SessionStartInput {
        agent_id: Some("agent-007".to_string()),
        project_id: Some("project-alpha".to_string()),
        external_session_ref: Some("ext-ref-xyz".to_string()),
        ..minimal_session_start("roundtrip")
    };
    let session = r.start_session(input).expect("start");
    assert_eq!(session.agent_id.as_deref(), Some("agent-007"));
    assert_eq!(session.project_id.as_deref(), Some("project-alpha"));
    assert_eq!(session.external_session_ref.as_deref(), Some("ext-ref-xyz"));

    // 通过 recent_sessions 也能拿到 —— SELECT 路径也补回了 v002 3 列。
    let recent = r.recent_sessions(10).expect("recent");
    let same = recent
        .iter()
        .find(|s| s.id == session.id)
        .expect("session in recent");
    assert_eq!(same.agent_id.as_deref(), Some("agent-007"));
    assert_eq!(same.project_id.as_deref(), Some("project-alpha"));
    assert_eq!(same.external_session_ref.as_deref(), Some("ext-ref-xyz"));
}

#[test]
fn existing_l1_behavior_preserved_across_v003_changes() {
    // 同一 `(project_id, external_session_ref)` 多次 start_session：
    // L1 行为是每次都生成新 session_id（不查重）；v003 落地后这一行为必须不变。
    let (_dir, r) = repo();
    let make = || SessionStartInput {
        project_id: Some("proj-shared".to_string()),
        external_session_ref: Some("ext-shared".to_string()),
        ..minimal_session_start("shared")
    };

    let a = r.start_session(make()).expect("first");
    let b = r.start_session(make()).expect("second");
    assert_ne!(a.id, b.id, "v003 must not collapse sessions by project+ref");

    // 同时也能正常 observe / end_session —— 验证 L1 主路径不被破。
    let obs = r
        .observe(minimal_observe(a.id.clone(), "hello"))
        .expect("observe");
    assert_eq!(obs.content, "hello");

    let ended = r
        .end_session(SessionEndInput {
            session_id: b.id.clone(),
            summary: None,
            summary_content_hash: None,
            summary_kind: None,
            summary_authority: None,
            summary_origin: None,
            summary_scope: None,
            summary_source_refs_json: None,
        })
        .expect("end");
    assert!(ended.ended_at.is_some());
}

/// 验证 use case 层（而不是裸 adapter）也按 brief 落地：capability profile
/// 由 use case 解析、`operation_mode` 与 `capabilities_json` 由 use case 决定。
#[test]
fn use_case_resolves_operation_mode_and_serializes_capabilities() {
    let (_dir, r) = repo();
    let session = memora::application::use_cases::start_session::execute(
        &r,
        SessionStartInput {
            name: "via-usecase".to_string(),
            client_capabilities: Some(ClientCapabilities {
                session_lifecycle: Some(SESSION_LIFECYCLE_HOOK_TAG.to_string()),
                ..ClientCapabilities::default()
            }),
            ..minimal_session_start("via-usecase")
        },
    )
    .expect("start via use case");

    assert_eq!(session.operation_mode, OperationMode::StatelessHooked);
    assert!(session.capabilities_json.is_some());
}
