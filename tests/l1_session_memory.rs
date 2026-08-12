//! L1 session memory 端到端 MCP contract test。
//!
//! 覆盖 spec l1-session-memory 与 l1-search-retrieval 的关键场景：
//! - 会话生命周期：start → observe × N → end → recent_*
//! - observe idempotency_key 重复提交 → 返回首次行
//! - BM25 全文检索与 kind 过滤
//! - 错误码到 MCP 响应的映射（session_not_found / invalid_params）
//! - limit 上限拒绝（> 100）
//! - `schema_version == 2`

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const TEST_IO_TIMEOUT: Duration = Duration::from_secs(10);

struct TempDirGuard(#[allow(dead_code)] tempfile::TempDir);

fn temp_db_path() -> (TempDirGuard, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("memora.db");
    (TempDirGuard(dir), path)
}

fn spawn_memora(db_path: &std::path::Path) -> std::process::Child {
    let bin = env!("CARGO_BIN_EXE_memora");
    Command::new(bin)
        .env("MEMORA_DB_PATH", db_path)
        .env("MEMORA_LOG", "warn")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn memora")
}

type SharedStdout = Arc<Mutex<BufReader<std::process::ChildStdout>>>;

fn shared_stdout(child: &mut std::process::Child) -> SharedStdout {
    let stdout = child.stdout.take().expect("stdout pipe");
    Arc::new(Mutex::new(BufReader::new(stdout)))
}

fn read_line(stdout: SharedStdout) -> String {
    let (tx, rx) = mpsc::channel();
    let stdout_clone = Arc::clone(&stdout);
    std::thread::spawn(move || {
        let mut guard = stdout_clone.lock().expect("stdout mutex poisoned");
        let mut line = String::new();
        match guard.read_line(&mut line) {
            Ok(_) => {
                let _ = tx.send(Ok(line));
            }
            Err(err) => {
                let _ = tx.send(Err(format!("read failed: {err}")));
            }
        }
    });

    let start = Instant::now();
    let remaining = || TEST_IO_TIMEOUT.saturating_sub(start.elapsed());
    match rx.recv_timeout(remaining()) {
        Ok(Ok(line)) => line,
        Ok(Err(msg)) => panic!("{msg}"),
        Err(mpsc::RecvTimeoutError::Timeout) => panic!(
            "read_line timed out after {TEST_IO_TIMEOUT:?}: memora is unresponsive on stdout"
        ),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("read_line: reader thread disconnected unexpectedly")
        }
    }
}

/// 启动 memora、initialize、notifications/initialized，返回 (child, stdin, stdout)。
/// 调用方负责在测试结束后 drop stdin 让 binary 自然退出。
struct McpHandle {
    stdin: std::process::ChildStdin,
    stdout: SharedStdout,
    child: std::process::Child,
    _guard: TempDirGuard,
}

fn boot_mcp() -> McpHandle {
    let (guard, db) = temp_db_path();
    let mut child = spawn_memora(&db);
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = shared_stdout(&mut child);

    // 1. initialize
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "l1-test", "version": "0.0.0" }
        }
    });
    writeln!(stdin, "{init}").expect("write initialize");
    stdin.flush().expect("flush");
    let line = read_line(Arc::clone(&stdout));
    let v: serde_json::Value = serde_json::from_str(&line).expect("initialize valid JSON");
    assert_eq!(v.get("jsonrpc").and_then(|x| x.as_str()), Some("2.0"));

    // 2. notifications/initialized
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {}
    });
    writeln!(stdin, "{initialized}").expect("write initialized");
    stdin.flush().expect("flush");

    McpHandle {
        stdin,
        stdout,
        child,
        _guard: guard,
    }
}

/// 通过 `tools/call` 发起调用，返回 `(content_text, error_or_null)`。
fn call_tool(
    handle: &mut McpHandle,
    name: &str,
    args: serde_json::Value,
    id: u64,
) -> (String, Option<serde_json::Value>) {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": args }
    });
    writeln!(handle.stdin, "{req}").expect("write tools/call");
    handle.stdin.flush().expect("flush");
    let line = read_line(Arc::clone(&handle.stdout));
    let v: serde_json::Value = serde_json::from_str(&line).expect("call response is valid JSON");
    assert_eq!(v.get("jsonrpc").and_then(|x| x.as_str()), Some("2.0"));
    if let Some(err) = v.get("error") {
        return (String::new(), Some(err.clone()));
    }
    let text = v
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .expect("call result text")
        .to_string();
    (text, None)
}

fn call_tool_ok(
    handle: &mut McpHandle,
    name: &str,
    args: serde_json::Value,
    id: u64,
) -> serde_json::Value {
    let (text, err) = call_tool(handle, name, args, id);
    assert!(err.is_none(), "{name} returned error: {err:?}");
    serde_json::from_str(&text).expect("{name} returned valid JSON text")
}

// ── 测试 ─────────────────────────────────────────────────────────

#[test]
fn memora_status_reports_schema_version_two() {
    let mut h = boot_mcp();
    let status = call_tool_ok(&mut h, "memora_status", serde_json::json!({}), 100);
    assert_eq!(status["status"], "healthy");
    assert_eq!(status["schema_version"], 2);
    drop(h.stdin);
    let _ = h.child.wait();
}

#[test]
fn session_lifecycle_start_observe_end_recent() {
    let mut h = boot_mcp();

    // 1. session_start
    let started = call_tool_ok(
        &mut h,
        "session_start",
        serde_json::json!({ "name": "lifecycle" }),
        101,
    );
    let session_id = started["session_id"]
        .as_str()
        .expect("session_id")
        .to_string();
    assert!(!session_id.is_empty());

    // 2. observe × 3（无 idempotency_key，应产生 3 行）
    for i in 0..3 {
        let args = serde_json::json!({
            "session_id": session_id,
            "content": format!("step-{i}"),
            "tool_name": "Read"
        });
        let obs = call_tool_ok(&mut h, "observe", args, 200 + i);
        assert_eq!(obs["session_id"].as_str(), Some(session_id.as_str()));
    }

    // 3. recent_observations（限定 session）
    let recent = call_tool_ok(
        &mut h,
        "recent_observations",
        serde_json::json!({ "session_id": session_id, "limit": 10 }),
        300,
    );
    assert_eq!(recent["total"].as_u64(), Some(3));
    let results = recent["results"].as_array().expect("results array");
    // 倒序：step-2 在前。
    assert_eq!(results[0]["content"].as_str(), Some("step-2"));

    // 4. session_end with summary
    let ended = call_tool_ok(
        &mut h,
        "session_end",
        serde_json::json!({
            "session_id": session_id,
            "summary": "completed lifecycle"
        }),
        301,
    );
    assert!(ended["ended_at"].is_string());
    assert_eq!(ended["summary"].as_str(), Some("completed lifecycle"));

    // 5. recent_sessions 必须包含该 session
    let sessions = call_tool_ok(
        &mut h,
        "recent_sessions",
        serde_json::json!({ "limit": 5 }),
        302,
    );
    let sessions_arr = sessions["results"].as_array().expect("results");
    let found = sessions_arr
        .iter()
        .any(|s| s["id"].as_str() == Some(session_id.as_str()));
    assert!(found, "recent_sessions must include just-ended session");

    drop(h.stdin);
    let _ = h.child.wait();
}

#[test]
fn observe_with_idempotency_key_returns_existing_row() {
    let mut h = boot_mcp();
    let started = call_tool_ok(
        &mut h,
        "session_start",
        serde_json::json!({ "name": "idem" }),
        401,
    );
    let sid = started["session_id"].as_str().expect("sid").to_string();

    let args_first = serde_json::json!({
        "session_id": sid,
        "content": "first",
        "idempotency_key": "k1"
    });
    let first = call_tool_ok(&mut h, "observe", args_first.clone(), 402);
    let second = call_tool_ok(&mut h, "observe", args_first, 403);

    assert_eq!(
        first["observation_id"], second["observation_id"],
        "idempotency: same observation_id on repeat"
    );
    assert_eq!(first["created_at"], second["created_at"]);

    // recent_observations 应只有 1 行。
    let recent = call_tool_ok(
        &mut h,
        "recent_observations",
        serde_json::json!({ "session_id": sid }),
        404,
    );
    assert_eq!(recent["total"].as_u64(), Some(1));

    drop(h.stdin);
    let _ = h.child.wait();
}

#[test]
fn search_returns_bm25_hits() {
    let mut h = boot_mcp();
    let started = call_tool_ok(
        &mut h,
        "session_start",
        serde_json::json!({ "name": "fts" }),
        501,
    );
    let sid = started["session_id"].as_str().expect("sid").to_string();

    call_tool_ok(
        &mut h,
        "observe",
        serde_json::json!({
            "session_id": sid,
            "content": "unique-token-alpha database migration"
        }),
        502,
    );
    call_tool_ok(
        &mut h,
        "session_end",
        serde_json::json!({
            "session_id": sid,
            "summary": "unique-token-alpha wrap"
        }),
        503,
    );

    // 全文搜索 "unique-token-alpha" 应至少命中 2 条（observation + summary）。
    let hits = call_tool_ok(
        &mut h,
        "search",
        serde_json::json!({ "query": "unique-token-alpha", "limit": 10 }),
        504,
    );
    let results = hits["results"].as_array().expect("results");
    assert!(
        results.len() >= 2,
        "expected at least 2 hits, got {:?}",
        hits
    );

    // kind=summary 过滤只返回 summary。
    let summary_only = call_tool_ok(
        &mut h,
        "search",
        serde_json::json!({
            "query": "unique-token-alpha",
            "kind": "summary",
            "limit": 5
        }),
        505,
    );
    for r in summary_only["results"].as_array().expect("results") {
        assert_eq!(r["kind"].as_str(), Some("summary"));
        assert!(r.get("tool_name").is_none());
    }

    drop(h.stdin);
    let _ = h.child.wait();
}

#[test]
fn search_limit_above_max_returns_invalid_params() {
    let mut h = boot_mcp();
    let (_text, err) = call_tool(
        &mut h,
        "search",
        serde_json::json!({ "query": "x", "limit": 1000 }),
        601,
    );
    // RMCP invalid_params → code = -32602.
    let err = err.expect("error response");
    assert_eq!(err["code"].as_i64(), Some(-32602));
    assert_eq!(err["data"]["code"].as_str(), Some("INVALID_INPUT"));

    drop(h.stdin);
    let _ = h.child.wait();
}

#[test]
fn observe_unknown_session_returns_resource_not_found() {
    let mut h = boot_mcp();
    let (_text, err) = call_tool(
        &mut h,
        "observe",
        serde_json::json!({
            "session_id": "no-such-session",
            "content": "x"
        }),
        701,
    );
    // RMCP resource_not_found → code = -32002.
    let err = err.expect("error response");
    assert_eq!(err["code"].as_i64(), Some(-32002));
    assert_eq!(err["data"]["code"].as_str(), Some("SESSION_NOT_FOUND"));

    drop(h.stdin);
    let _ = h.child.wait();
}

#[test]
fn session_start_rejects_empty_name() {
    let mut h = boot_mcp();
    let (_text, err) = call_tool(
        &mut h,
        "session_start",
        serde_json::json!({ "name": "" }),
        801,
    );
    // 入参校验失败 → invalid_params (-32602).
    let err = err.expect("error response");
    assert_eq!(err["code"].as_i64(), Some(-32602));
    assert_eq!(err["data"]["code"].as_str(), Some("INVALID_INPUT"));

    drop(h.stdin);
    let _ = h.child.wait();
}

#[test]
fn schema_migrations_records_both_versions() {
    // 干净实现：boot_mcp 一次，结束后直接 SELECT schema_migrations。
    let (guard, db) = temp_db_path();
    let mut child = spawn_memora(&db);
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = shared_stdout(&mut child);

    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "x", "version": "0" }
        }
    });
    writeln!(stdin, "{init}").expect("init");
    stdin.flush().expect("flush");
    let _ = read_line(Arc::clone(&stdout));

    drop(stdin);
    let _ = child.wait();

    let conn = rusqlite::Connection::open(&db).expect("reopen");
    let v1: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = 1",
            [],
            |row| row.get(0),
        )
        .expect("v1");
    let v2: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = 2",
            [],
            |row| row.get(0),
        )
        .expect("v2");
    assert_eq!(v1, 1);
    assert_eq!(v2, 1);

    drop(guard);
}
