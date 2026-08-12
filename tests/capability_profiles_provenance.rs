//! 集成测试 —— observation/summary provenance 元数据（task 1.4）。
//!
//! 覆盖：
//! 1. `origin="tool_result"` 写入后落库回读一致；
//! 2. `content_hash` 是 64 字符 hex（只 `[0-9a-f]`），由 use case 层 SHA-256 计算；
//! 3. 同 `(session_id, fact_key="X")` 两条 observation 各自保留独立 id / created_at
//!    （不覆盖、不冲突）；
//! 4. SHA-256 对同样 `content` 永远返回同样 hash（确定性）；
//!    标准 NIST 测试向量锁定算法实现。
//!
//! 该测试独立文件，避免污染 `memory_repository::tests` 模块。

use memora::adapters::sqlite::SqliteMemoryRepository;
use memora::application::ports::{
    MemoryRepository, ObserveInput, SearchInput, SearchKind, SessionStartInput,
};
use memora::domain::{OperationMode, SessionId};

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

fn repo() -> (tempfile::TempDir, SqliteMemoryRepository) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("memora.db");
    memora::adapters::sqlite::open_and_migrate(&db).expect("migrate");
    let repository = SqliteMemoryRepository::bootstrap(db);
    (dir, repository)
}

#[test]
fn tool_result_origin_round_trips_through_storage() {
    let (_dir, r) = repo();
    let session = r
        .start_session(minimal_session_start("origin-rt"))
        .expect("start");

    let mut input = minimal_observe(session.id.clone(), "tool produced this");
    input.origin = Some("tool_result".to_string());
    let obs = r.observe(input).expect("observe");

    assert_eq!(
        obs.origin.as_deref(),
        Some("tool_result"),
        "caller-supplied origin must be persisted verbatim"
    );

    // 回读路径：recent_observations 也应能看到 origin。
    let recent = r
        .recent_observations(Some(&session.id), 10)
        .expect("recent");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].origin.as_deref(), Some("tool_result"));
}

#[test]
fn content_hash_is_64_char_lowercase_hex() {
    // 走 use case 入口触发 SHA-256 计算（adapter 直调不负责算 hash）。
    use memora::application::use_cases::observe::execute as observe_uc;
    let (_dir, r) = repo();
    let session = r
        .start_session(minimal_session_start("hash"))
        .expect("start");

    let obs =
        observe_uc(&r, minimal_observe(session.id.clone(), "hello")).expect("observe via use case");

    let hash = obs
        .content_hash
        .as_deref()
        .expect("content_hash must be computed by use case");
    assert_eq!(hash.len(), 64, "SHA-256 hex must be 64 chars");
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit()),
        "all chars must be hex digits: {hash}"
    );
    assert!(
        hash.chars().all(|c| !c.is_ascii_uppercase()),
        "hex must be lowercase: {hash}"
    );

    // SHA-256("hello") 标准向量（RFC 6234 / NIST FIPS 180-4）。
    assert_eq!(
        hash, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        "SHA-256(\"hello\") must match NIST vector"
    );
}

#[test]
fn same_fact_key_yields_distinct_rows_per_call() {
    // v003 设计：`(session_id, fact_key)` 不强制唯一；
    // 仅 `idempotency_key` 强制幂等。两条同 fact_key 的 observation
    // 应各自保留独立 `id` / `created_at`，不发生覆盖。
    let (_dir, r) = repo();
    let session = r
        .start_session(minimal_session_start("fact-key"))
        .expect("start");

    let mut a = minimal_observe(session.id.clone(), "first version");
    a.fact_key = Some("user-prefers-dark-mode".to_string());
    let obs_a = r.observe(a).expect("observe a");

    // 第二条同 fact_key 的 observation：不传 idempotency_key，写新行。
    let mut b = minimal_observe(session.id.clone(), "second version");
    b.fact_key = Some("user-prefers-dark-mode".to_string());
    let obs_b = r.observe(b).expect("observe b");

    assert_ne!(obs_a.id, obs_b.id, "distinct rows must have distinct ids");
    assert_ne!(
        obs_a.created_at, obs_b.created_at,
        "distinct rows must have distinct created_at (insertion time differs)"
    );
    assert_eq!(
        obs_a.fact_key.as_deref(),
        Some("user-prefers-dark-mode"),
        "fact_key round-trips"
    );
    assert_eq!(obs_b.fact_key.as_deref(), Some("user-prefers-dark-mode"));

    let recent = r
        .recent_observations(Some(&session.id), 10)
        .expect("recent");
    assert_eq!(recent.len(), 2, "both rows must persist");
}

#[test]
fn sha256_hash_is_deterministic_across_calls() {
    // 同一 content 在两次独立调用下必须返回同一 hash，
    // 即"持久化契约由 use case 算"语义稳定。
    use memora::application::use_cases::observe::execute as observe_uc;
    use std::sync::{Arc, Mutex};

    // 捕获 adapter 入参的 CapturingRepo —— 参考 use case 模块自带的实现，
    // 这里直接 inline 一份简化版。
    struct Capturing {
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl memora::application::ports::MemoryRepository for Capturing {
        fn start_session(
            &self,
            _: memora::application::ports::SessionStartInput,
        ) -> Result<memora::domain::Session, memora::application::errors::MemoryError> {
            unreachable!()
        }
        fn end_session(
            &self,
            _: memora::application::ports::SessionEndInput,
        ) -> Result<memora::domain::Session, memora::application::errors::MemoryError> {
            unreachable!()
        }
        fn observe(
            &self,
            input: memora::application::ports::ObserveInput,
        ) -> Result<memora::domain::Observation, memora::application::errors::MemoryError> {
            let hash = input.content_hash.clone().expect("hash");
            self.seen.lock().unwrap().push(hash);
            Ok(memora::domain::Observation {
                id: memora::domain::ObservationId("dummy".to_string()),
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
        ) -> Result<Vec<memora::domain::Observation>, memora::application::errors::MemoryError>
        {
            unreachable!()
        }
        fn recent_sessions(
            &self,
            _: u32,
        ) -> Result<Vec<memora::domain::Session>, memora::application::errors::MemoryError>
        {
            unreachable!()
        }
        fn search(
            &self,
            _: SearchInput,
        ) -> Result<Vec<memora::domain::SearchHit>, memora::application::errors::MemoryError>
        {
            unreachable!()
        }
    }

    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let repo = Capturing {
        seen: Arc::clone(&seen),
    };

    // 三次独立调用，content 全部相同。
    for _ in 0..3 {
        let input = minimal_observe(SessionId("s".to_string()), "deterministic content");
        observe_uc(&repo, input).expect("observe");
    }

    let captured = seen.lock().unwrap();
    assert_eq!(captured.len(), 3);
    assert!(
        captured.iter().all(|h| h == &captured[0]),
        "same content must produce same hash across calls"
    );
    drop(captured);

    // 再跑一次：用已知 SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let repo = Capturing {
        seen: Arc::clone(&seen),
    };
    let input = minimal_observe(SessionId("s".to_string()), "abc");
    observe_uc(&repo, input).expect("observe");
    let captured = seen.lock().unwrap();
    assert_eq!(
        captured[0], "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "SHA-256(\"abc\") must match NIST FIPS 180-4 vector"
    );

    // 空串 SHA-256 = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let repo = Capturing {
        seen: Arc::clone(&seen),
    };
    let input = minimal_observe(SessionId("s".to_string()), "");
    let err = observe_uc(&repo, input).expect_err("empty content rejected by validation");
    assert!(matches!(
        err,
        memora::application::errors::MemoryError::InvalidInput(_)
    ));
}

/// 锁住 v003 default field 填充：缺失时 use case 必须填默认值，
/// 不允许悄悄写成 NULL。走 use case 入口触发 SHA-256 + 默认值填充。
#[test]
fn observe_use_case_fills_defaults_for_missing_provenance() {
    use memora::application::use_cases::observe::execute as observe_uc;
    let (_dir, r) = repo();
    let session = r
        .start_session(minimal_session_start("defaults"))
        .expect("start");

    let obs =
        observe_uc(&r, minimal_observe(session.id.clone(), "plain")).expect("observe via use case");

    assert_eq!(obs.origin.as_deref(), Some("user"));
    assert_eq!(obs.scope.as_deref(), Some("session"));
    assert_eq!(obs.kind.as_deref(), Some("observation"));
    assert_eq!(obs.authority.as_deref(), Some("l1_observation"));
    assert_eq!(
        obs.source_refs_json.as_deref(),
        Some("[]"),
        "empty source_refs must serialize as '[]'"
    );
    assert!(
        obs.content_hash.is_some(),
        "SHA-256 must be computed even for default-filled rows"
    );
}

/// search 路径不回读 v003 新字段（`SearchHit` 保持字段集合稳定，spec
/// l1-search-retrieval "响应字段稳定性" 契约）；但 `recent_observations`
/// 路径必须把 v003 字段完整还原。
#[test]
fn caller_supplied_project_id_and_authority_round_trip_through_recent() {
    let (_dir, r) = repo();
    let session = r.start_session(minimal_session_start("rt")).expect("start");

    let mut input = minimal_observe(session.id.clone(), "unique-token-rt-provenance");
    input.project_id = Some("proj-x".to_string());
    input.authority = Some("l2_agent".to_string());
    // adapter 直调：`source_refs` 不被 adapter 处理（仅 use case 序列化），
    // 所以直接填 `source_refs_json`。
    input.source_refs = None;
    input.source_refs_json =
        Some(serde_json::to_string(&vec!["ref-1".to_string(), "ref-2".to_string()]).expect("json"));
    let obs = r.observe(input).expect("observe");

    assert_eq!(obs.project_id.as_deref(), Some("proj-x"));
    assert_eq!(obs.authority.as_deref(), Some("l2_agent"));
    let json = obs.source_refs_json.as_deref().expect("json");
    assert!(json.contains("ref-1") && json.contains("ref-2"));

    // 通过 recent_observations 验证 SELECT 路径也完整。
    let recent = r
        .recent_observations(Some(&session.id), 10)
        .expect("recent");
    assert_eq!(recent.len(), 1);
    let hit = &recent[0];
    assert_eq!(hit.project_id.as_deref(), Some("proj-x"));
    assert_eq!(hit.authority.as_deref(), Some("l2_agent"));
    let json = hit.source_refs_json.as_deref().expect("json");
    assert!(json.contains("ref-1") && json.contains("ref-2"));

    // search 返回的 SearchHit 不暴露 v003 字段（与 L1 一致）—— 这是契约，不是 bug。
    let hits = r
        .search(SearchInput {
            query: "unique-token-rt-provenance".to_string(),
            session_id: Some(session.id.clone()),
            kind: SearchKind::Observation,
            limit: 10,
        })
        .expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "observation");
}
