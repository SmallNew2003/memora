//! 集成测试 —— v002 → v003 兼容迁移回填（task 1.5）。
//!
//! 覆盖：
//! 1. 在临时 DB 跑 v001 + v002 迁移、插入「L1 老格式」observation 与 summary，
//!    然后通过 memora 启动路径跑 v003 upgrade；
//! 2. 升级后老 observation 行的 v003 新列被回填：
//!    `scope='session' / kind='observation' / origin='user' /
//!     authority='l1_observation' / content_hash=NULL`；
//!    老 session 行的 v003 新列被回填：`operation_mode='stateless-manual' /
//!    capabilities_json=NULL`；
//! 3. 升级后 `recent_observations` 仍能读取老数据，行为与 L1 一致（不加
//!    scope 过滤）；
//! 4. summary 行的回填：`kind='summary' / authority='l1_summary'`。
//!
//! 该测试独立文件，避免污染 `memory_repository::tests` 模块。

use memora::adapters::sqlite::SqliteMemoryRepository;
use memora::application::ports::{MemoryRepository, ObserveInput, SessionStartInput};
use memora::domain::{OperationMode, SessionId};

const V001_SQL: &str = include_str!("../src/migrations/v001__schema_migrations.sql");
const V002_SQL: &str = include_str!("../src/migrations/v002__l1_memory.sql");

/// 在临时 DB 里手动跑 v001 + v002 迁移，模拟「v002 老用户升级」起点。
/// 关键：必须在 `schema_migrations` 里写入 v1 + v2 的正确 checksum，
/// 否则 memora::migrations::apply_pending 看到 schema 已创建但 checksum
/// 缺失/不匹配会拒绝启动 / 重复执行。
fn bring_up_to_v2(db_path: &std::path::Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(db_path).expect("open v2 db");
    conn.execute_batch(V001_SQL).expect("apply v001");
    conn.execute_batch(V002_SQL).expect("apply v002");
    // 写入 v1 + v2 的真实 checksum（与 binary 内嵌迁移字节一致），让
    // apply_pending 把它们视为「已应用」并只跑 v003。
    let v1_checksum = memora::migrations::checksum(V001_SQL);
    let v2_checksum = memora::migrations::checksum(V002_SQL);
    conn.execute(
        "INSERT INTO schema_migrations (version, checksum) VALUES (1, ?1)",
        rusqlite::params![v1_checksum],
    )
    .expect("record v1 checksum");
    conn.execute(
        "INSERT INTO schema_migrations (version, checksum) VALUES (2, ?1)",
        rusqlite::params![v2_checksum],
    )
    .expect("record v2 checksum");
    conn
}

/// 测试 helper：构造 `ObserveInput` 时已经填 v003 默认值（与 use case
/// 行为一致；helper 模拟 use case 已经填好的状态）。
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

#[test]
fn upgrade_path_from_v2_preserves_old_observations() {
    // 场景：v002 老用户在已有 observation / summary 的情况下升级到 v003，
    // 不应丢失任何数据。
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("upgrade.db");

    // ── 阶段 1：v001 + v002 迁移 + 插入老格式数据 ──
    let conn = bring_up_to_v2(&db);

    // 手动建一个老 session（v002 schema 没有 operation_mode 列）。
    let old_session_id = "old-session-id-001";
    conn.execute(
        "INSERT INTO sessions (id, name, created_at, ended_at, summary) \
         VALUES (?1, ?2, ?3, NULL, NULL)",
        rusqlite::params![
            old_session_id,
            "legacy-v2-session",
            "2026-01-15T10:00:00.000Z"
        ],
    )
    .expect("insert old session");

    // 插入老格式 observation：v002 schema 只有 8 列，没有 scope/kind/origin/authority。
    conn.execute(
        "INSERT INTO observations (id, session_id, content, tool_name, idempotency_key, created_at) \
         VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
        rusqlite::params![
            "old-obs-id-001",
            old_session_id,
            "legacy observation content",
            "Bash",
            "2026-01-15T10:01:00.000Z"
        ],
    )
    .expect("insert old observation");

    // 再插入一条带 idempotency_key 的老 observation，用于 L1 兼容路径。
    conn.execute(
        "INSERT INTO observations (id, session_id, content, tool_name, idempotency_key, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            "old-obs-id-002",
            old_session_id,
            "second legacy observation",
            "Read",
            "legacy-idem-key",
            "2026-01-15T10:02:00.000Z"
        ],
    )
    .expect("insert old observation #2");

    // 插入老 summary（v002 schema 没有 v003 字段）。
    conn.execute(
        "INSERT INTO summaries (id, session_id, content, created_at) \
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            "old-sum-id-001",
            old_session_id,
            "legacy summary",
            "2026-01-15T11:00:00.000Z"
        ],
    )
    .expect("insert old summary");

    drop(conn);

    // ── 阶段 2：通过 memora 启动路径跑 v003 upgrade ──
    // open_and_migrate 内部调 apply_pending，会自动跑 v003 并回填。
    memora::adapters::sqlite::open_and_migrate(&db).expect("apply v003");

    // ── 阶段 3：升级后验证回填 + 老数据保留 ──
    let conn = rusqlite::Connection::open(&db).expect("reopen");

    // 老 session 应回填 operation_mode='stateless-manual' + capabilities_json=NULL。
    let (operation_mode, capabilities_json): (String, Option<String>) = conn
        .query_row(
            "SELECT operation_mode, capabilities_json FROM sessions WHERE id = ?1",
            rusqlite::params![old_session_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .expect("read old session");
    assert_eq!(
        operation_mode, "stateless-manual",
        "old session must default to stateless-manual"
    );
    assert!(
        capabilities_json.is_none(),
        "old session capabilities_json must stay NULL (not '{{}}')"
    );

    // 老 observation 应回填 4 个 NOT NULL DEFAULT 字段。
    let (scope, kind, origin, authority, content_hash): (
        String,
        String,
        String,
        String,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT scope, kind, origin, authority, content_hash FROM observations WHERE id = ?1",
            rusqlite::params!["old-obs-id-001"],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .expect("read old observation");
    assert_eq!(
        scope, "session",
        "old observation scope must default to 'session'"
    );
    assert_eq!(
        kind, "observation",
        "old observation kind must default to 'observation'"
    );
    assert_eq!(
        origin, "user",
        "old observation origin must default to 'user'"
    );
    assert_eq!(
        authority, "l1_observation",
        "old observation authority must default to 'l1_observation'"
    );
    assert!(
        content_hash.is_none(),
        "old observation content_hash must stay NULL (no SHA-256 computed for legacy rows)"
    );

    // 老 summary 应回填，但 kind/authority 用 summary 的默认值。
    let (scope_s, kind_s, origin_s, authority_s, content_hash_s): (
        String,
        String,
        String,
        String,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT scope, kind, origin, authority, content_hash FROM summaries WHERE id = ?1",
            rusqlite::params!["old-sum-id-001"],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .expect("read old summary");
    assert_eq!(scope_s, "session");
    assert_eq!(
        kind_s, "summary",
        "old summary kind must default to 'summary'"
    );
    assert_eq!(origin_s, "user");
    assert_eq!(
        authority_s, "l1_summary",
        "old summary authority must default to 'l1_summary'"
    );
    assert!(
        content_hash_s.is_none(),
        "old summary content_hash must stay NULL"
    );

    // schema_migrations 应记录 v1+v2+v3。
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("count migrations");
    assert_eq!(
        count, 3,
        "all three migrations must be recorded after upgrade"
    );

    drop(conn);
}

#[test]
fn recent_observations_unchanged_for_legacy_data_after_upgrade() {
    // 场景：升级后 L1 的 `recent_observations` 路径仍能读老数据，
    // 行为与 L1 完全一致（不加 scope 过滤、不破坏排序）。
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("upgrade-recent.db");

    let conn = bring_up_to_v2(&db);
    let old_session_id = "old-session-recent";
    conn.execute(
        "INSERT INTO sessions (id, name, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![old_session_id, "legacy", "2026-01-15T10:00:00.000Z"],
    )
    .expect("insert session");
    for i in 0..3 {
        conn.execute(
            "INSERT INTO observations (id, session_id, content, tool_name, idempotency_key, created_at) \
             VALUES (?1, ?2, ?3, NULL, NULL, ?4)",
            rusqlite::params![
                format!("old-obs-{i}"),
                old_session_id,
                format!("legacy item {i}"),
                format!("2026-01-15T10:0{i}:00.000Z")
            ],
        )
        .expect("insert old obs");
    }
    drop(conn);

    memora::adapters::sqlite::open_and_migrate(&db).expect("apply v003");

    // 用 SqliteMemoryRepository 走标准 L1 路径。
    let repo = SqliteMemoryRepository::bootstrap(db.clone());
    let sid = SessionId(old_session_id.to_string());
    let recent = repo.recent_observations(Some(&sid), 10).expect("recent");

    assert_eq!(
        recent.len(),
        3,
        "all three legacy observations must remain queryable"
    );
    // 按 created_at DESC 排序：item 2 在前。
    assert_eq!(recent[0].content, "legacy item 2");
    assert_eq!(recent[1].content, "legacy item 1");
    assert_eq!(recent[2].content, "legacy item 0");
    // v003 字段被默认回填。
    for obs in &recent {
        assert_eq!(obs.scope.as_deref(), Some("session"));
        assert_eq!(obs.kind.as_deref(), Some("observation"));
        assert_eq!(obs.origin.as_deref(), Some("user"));
        assert_eq!(obs.authority.as_deref(), Some("l1_observation"));
        assert!(
            obs.content_hash.is_none(),
            "legacy rows must not get synthetic hash"
        );
    }
}

#[test]
fn new_writes_after_upgrade_use_use_case_layer_filled_values() {
    // 场景：升级后新写入的 observation 走 use case，content_hash 应被计算、
    // caller 提供的 origin 等字段应原样保存（与升级后的回填共存）。
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("upgrade-new.db");

    // 直接从 v003 fresh 状态开始（升级路径已被 brief 其他测试覆盖），
    // 这里只验证升级后新写的语义与之前一致。
    let _conn = bring_up_to_v2(&db);
    drop(_conn);
    memora::adapters::sqlite::open_and_migrate(&db).expect("apply v003");

    let repo = SqliteMemoryRepository::bootstrap(db.clone());
    let started = repo
        .start_session(SessionStartInput {
            name: "post-upgrade".to_string(),
            agent_id: None,
            project_id: None,
            external_session_ref: None,
            client_capabilities: None,
            operation_mode: OperationMode::StatelessManual,
            capabilities_json: None,
        })
        .expect("start");
    assert_eq!(started.operation_mode, OperationMode::StatelessManual);
    assert!(started.capabilities_json.is_none());

    // 走 use case 写新 observation，触发 SHA-256 + 默认值填充。
    let obs = memora::application::use_cases::observe::execute(
        &repo,
        minimal_observe(started.id.clone(), "post-upgrade content"),
    )
    .expect("observe via use case");
    assert_eq!(obs.origin.as_deref(), Some("user"));
    assert_eq!(obs.scope.as_deref(), Some("session"));
    assert_eq!(obs.kind.as_deref(), Some("observation"));
    assert_eq!(obs.authority.as_deref(), Some("l1_observation"));
    let hash = obs.content_hash.as_deref().expect("hash computed");
    assert_eq!(hash.len(), 64);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}
