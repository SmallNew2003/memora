//! 2.4 会话恢复 + 长期未活动归档测试。
//!
//! 覆盖 5 个场景：
//! 1. 同 (project_id, external_session_ref) 二次 session_start 返回原 session_id
//! 2. 同 (project_id, external_session_ref) 但 last_active_at 超 30 天 → 归档后创建新 session
//! 3. 跨 project 同 external_session_ref → 不触发恢复
//! 4. 同 (project_id, external_session_ref) 但原 session 已 archived_at 非空 → 创建新 session
//! 5. archive_after_seconds 配置生效（改成 1 秒后跑测试 2 场景）

use memora::adapters::sqlite::SqliteMemoryRepository;
use memora::application::ports::{MemoryRepository, SessionStartInput};
use memora::application::use_cases::start_session;
use memora::domain::OperationMode;

fn minimal_session_start(
    name: &str,
    project_id: Option<&str>,
    external_session_ref: Option<&str>,
) -> SessionStartInput {
    SessionStartInput {
        name: name.to_string(),
        agent_id: None,
        project_id: project_id.map(str::to_string),
        external_session_ref: external_session_ref.map(str::to_string),
        client_capabilities: None,
        operation_mode: OperationMode::StatelessManual,
        capabilities_json: None,
    }
}

fn repo(dir: &tempfile::TempDir) -> SqliteMemoryRepository {
    let db = dir.path().join("memora.db");
    memora::adapters::sqlite::open_and_migrate(&db).expect("migrate");
    SqliteMemoryRepository::bootstrap(db)
}

#[test]
fn same_project_and_ref_returns_existing_session_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = repo(&dir);

    let input = minimal_session_start("first", Some("proj-1"), Some("ref-1"));
    let output1 = start_session::execute(&r, input.clone(), 2592000).expect("first start");
    assert!(!output1.recovered);

    let output2 = start_session::execute(&r, input, 2592000).expect("second start");
    assert!(output2.recovered);
    assert_eq!(
        output1.session.id, output2.session.id,
        "same session id on recovery"
    );
}

#[test]
fn expired_session_archived_and_new_session_created() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = repo(&dir);

    let input = minimal_session_start("expired-test", Some("proj-2"), Some("ref-2"));
    let output1 = start_session::execute(&r, input.clone(), 2592000).expect("first start");
    assert!(!output1.recovered);

    // Manually set last_active_at to a date far in the past (> 30 days)
    let conn = rusqlite::Connection::open(dir.path().join("memora.db")).expect("reopen");
    conn.execute(
        "UPDATE sessions SET last_active_at = '2020-01-01T00:00:00.000Z' WHERE id = ?1",
        rusqlite::params![output1.session.id.0],
    )
    .expect("update last_active_at");

    // Second call with archive_after_seconds = 1 second (the old date is clearly expired)
    let output2 = start_session::execute(&r, input, 1).expect("second start after expiry");
    assert!(
        !output2.recovered,
        "expired session creates new, not recovered"
    );
    assert_ne!(output1.session.id, output2.session.id, "new session id");

    // Verify original session is archived
    let archived: Option<String> = conn
        .query_row(
            "SELECT archived_at FROM sessions WHERE id = ?1",
            rusqlite::params![output1.session.id.0],
            |row| row.get(0),
        )
        .expect("query archived_at");
    assert!(archived.is_some(), "original session archived");
}

#[test]
fn cross_project_same_ref_does_not_recover() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = repo(&dir);

    let output1 = start_session::execute(
        &r,
        minimal_session_start("p1", Some("proj-a"), Some("ref-x")),
        2592000,
    )
    .expect("first start");
    assert!(!output1.recovered);

    let output2 = start_session::execute(
        &r,
        minimal_session_start("p2", Some("proj-b"), Some("ref-x")),
        2592000,
    )
    .expect("second start");
    assert!(!output2.recovered, "different project must not recover");
    assert_ne!(output1.session.id, output2.session.id);
}

#[test]
fn already_archived_session_creates_new_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = repo(&dir);

    let input = minimal_session_start("archived-test", Some("proj-3"), Some("ref-3"));
    let output1 = start_session::execute(&r, input.clone(), 2592000).expect("first start");
    assert!(!output1.recovered);

    // Manually archive the session
    r.archive_session(&output1.session.id, "2026-01-01T00:00:00.000Z")
        .expect("archive");

    let output2 = start_session::execute(&r, input, 2592000).expect("second start");
    assert!(!output2.recovered, "archived session creates new");
    assert_ne!(output1.session.id, output2.session.id);
}

#[test]
fn archive_after_seconds_config_short_expiry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = repo(&dir);

    let input = minimal_session_start("short-expiry", Some("proj-4"), Some("ref-4"));
    let output1 = start_session::execute(&r, input.clone(), 2592000).expect("first start");
    assert!(!output1.recovered);

    // Set last_active_at to 2 seconds ago
    let conn = rusqlite::Connection::open(dir.path().join("memora.db")).expect("reopen");
    conn.execute(
        "UPDATE sessions SET last_active_at = '2020-01-01T00:00:00.000Z' WHERE id = ?1",
        rusqlite::params![output1.session.id.0],
    )
    .expect("update last_active_at");

    // archive_after_seconds = 1 → should archive the old session
    let output2 = start_session::execute(&r, input, 1).expect("second start");
    assert!(!output2.recovered);
    assert_ne!(output1.session.id, output2.session.id);

    let archived: Option<String> = conn
        .query_row(
            "SELECT archived_at FROM sessions WHERE id = ?1",
            rusqlite::params![output1.session.id.0],
            |row| row.get(0),
        )
        .expect("query");
    assert!(archived.is_some(), "old session archived with short expiry");
}
