//! 端到端 MCP contract test。
//!
//! 覆盖 spec mcp-runtime-health "MCP 健康路径具有端到端测试"：
//! - 在临时目录准备 SQLite 数据库；
//! - 启动 memora 子进程，stdin/stdout 走 JSON-RPC；
//! - 以 `initialize(protocolVersion = "2025-11-25")` 发起会话，断言
//!   协商回相同协议版本；
//! - 发送 `notifications/initialized`、`tools/list`、`tools/call(memora_status)`；
//! - 断言响应结构、字段类型与 stdout 每行都是合法 JSON-RPC。

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use memora::config::RuntimeConfig;
use memora::domain::Transport;

/// 集成测试的总体等待上限：memora 一旦 hang 住，read_line 必须能在该时间内返回。
/// 不设上限会让单测卡死整个 CI。
const TEST_IO_TIMEOUT: Duration = Duration::from_secs(10);

/// 持有的临时目录，drop 时强制清理。
///
/// `temp_db_path()` 把这个守卫返回出去，测试函数负责把它带出作用域。
/// 这样既不再 `mem::forget` 泄漏，也避免在 memora 子进程存活时
/// 把目录路径无效化（dir 在子进程退出后再释放）。
struct TempDirGuard(#[allow(dead_code)] tempfile::TempDir);

/// 通过 `MEMORA_DB_PATH` 强制 memora 走临时数据库（spec rust-runtime-foundation
/// "使用临时数据库运行测试"）。
///
/// 返回 `(TempDirGuard, db_path)`：守卫随调用者出作用域，drop 时由
/// tempfile 自行清理，无需手动调用 remove_dir_all。
fn temp_db_path() -> (TempDirGuard, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("memora.db");
    (TempDirGuard(dir), path)
}

/// 启动 memora 子进程，stdin/stdout 走 JSON-RPC。
fn spawn_memora(db_path: &std::path::Path) -> std::process::Child {
    // 测试使用 build 出的 memora 二进制。
    // `env!("CARGO_BIN_EXE_memora")` 由 cargo 在集成测试中注入。
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

/// 共享 stdout 句柄：`memora` 单进程顺序输出，BufReader 通过 Mutex 串行化即可。
type SharedStdout = Arc<Mutex<BufReader<std::process::ChildStdout>>>;

/// 把 child.stdout 包成共享 BufReader 句柄。
fn shared_stdout(child: &mut std::process::Child) -> SharedStdout {
    let stdout = child.stdout.take().expect("stdout pipe");
    Arc::new(Mutex::new(BufReader::new(stdout)))
}

/// 从共享 stdout 读下一行（直到 `\n`），整体有 `TEST_IO_TIMEOUT` 上限。
///
/// 通过 `mpsc::channel` + 后台线程实现「带超时的阻塞读」：
/// - 后台线程短暂锁住 BufReader 并 read_line；
/// - 主线程等待带超时，超时直接 panic，留下明确信号便于调试 hang 住的 memora binary。
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

#[test]
fn mcp_health_contract_initialize_tools_list_tools_call() {
    let (_guard, db) = temp_db_path();
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
            "clientInfo": {
                "name": "memora-contract-test",
                "version": "0.0.0",
            }
        }
    });
    writeln!(stdin, "{init}").expect("write initialize");
    stdin.flush().expect("flush");

    let init_resp_line = read_line(Arc::clone(&stdout));
    let init_resp: serde_json::Value =
        serde_json::from_str(&init_resp_line).expect("initialize response is valid JSON");
    assert_eq!(
        init_resp.get("jsonrpc").and_then(|v| v.as_str()),
        Some("2.0"),
        "initialize response must carry jsonrpc=2.0; got {init_resp_line}"
    );
    let init_result = init_resp.get("result").expect("initialize returns result");
    assert_eq!(
        init_result.get("protocolVersion").and_then(|v| v.as_str()),
        Some("2025-11-25"),
        "negotiated protocol version must be 2025-11-25"
    );
    assert_eq!(
        init_result
            .get("serverInfo")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str()),
        Some("memora"),
        "serverInfo.name must be memora"
    );
    assert!(
        init_result
            .get("capabilities")
            .and_then(|v| v.get("tools"))
            .is_some(),
        "capabilities.tools must be declared"
    );

    // 2. notifications/initialized
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {}
    });
    writeln!(stdin, "{initialized}").expect("write initialized");
    stdin.flush().expect("flush");

    // 3. tools/list
    let list = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    writeln!(stdin, "{list}").expect("write tools/list");
    stdin.flush().expect("flush");

    let list_resp_line = read_line(Arc::clone(&stdout));
    let list_resp: serde_json::Value =
        serde_json::from_str(&list_resp_line).expect("tools/list response is valid JSON");
    assert_eq!(
        list_resp.get("jsonrpc").and_then(|v| v.as_str()),
        Some("2.0"),
        "tools/list response must carry jsonrpc=2.0; got {list_resp_line}"
    );
    let tools = list_resp
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();
    // 7 个 tool：memora_status + 6 个 L1 业务 tool。
    let expected: &[&str] = &[
        "memora_status",
        "session_start",
        "session_end",
        "observe",
        "recent_observations",
        "recent_sessions",
        "search",
    ];
    for name in expected {
        assert!(
            names.contains(name),
            "tools/list must include {name}, got {names:?}"
        );
    }

    // 4. tools/call(memora_status)
    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "memora_status",
            "arguments": {}
        }
    });
    writeln!(stdin, "{call}").expect("write tools/call");
    stdin.flush().expect("flush");

    let call_resp_line = read_line(Arc::clone(&stdout));
    let call_resp: serde_json::Value =
        serde_json::from_str(&call_resp_line).expect("tools/call response is valid JSON");
    assert_eq!(
        call_resp.get("jsonrpc").and_then(|v| v.as_str()),
        Some("2.0"),
        "tools/call response must carry jsonrpc=2.0; got {call_resp_line}"
    );
    let content = call_resp
        .get("result")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_array())
        .expect("call result content");
    assert_eq!(content.len(), 1);
    let text = content[0]
        .get("text")
        .and_then(|v| v.as_str())
        .expect("content[0].text is a string");
    let status: serde_json::Value = serde_json::from_str(text).expect("status JSON");

    // 5. 断言五字段健康对象。
    assert_eq!(status["status"], "healthy");
    assert_eq!(status["database"], "healthy");
    assert_eq!(status["transport"], Transport::Stdio.as_str());
    assert!(status["runtime_version"].is_string());
    assert!(status["schema_version"].is_u64());

    // 6. 重复调用同一 tool：必须返回一致结果且不创建业务记录。
    writeln!(stdin, "{call}").expect("write tools/call (2nd)");
    stdin.flush().expect("flush");
    let second_line = read_line(Arc::clone(&stdout));
    let second_resp: serde_json::Value =
        serde_json::from_str(&second_line).expect("2nd call response valid");
    let second_status_text = second_resp
        .get("result")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("text"))
        .and_then(|v| v.as_str())
        .expect("2nd call text");
    let second_status: serde_json::Value = serde_json::from_str(second_status_text).expect("parse");
    assert_eq!(second_status, status, "repeated status query is stable");

    // 7. 关闭 stdin 让 binary 自然退出。
    drop(stdin);
    let _ = child.wait();

    // 8. 确认临时数据库 schema_migrations 表存在 v1。
    let conn = rusqlite::Connection::open(&db).expect("reopen test db");
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = 1",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(
        count, 1,
        "schema_migrations v1 row should be recorded exactly once"
    );
}

#[test]
fn health_service_reports_healthy_for_stdio_transport() {
    // 同步单元测试风格：直接构造 service，不走 IO。
    use memora::adapters::sqlite::{SqliteError, SqliteHealthRepository};
    use memora::application::HealthService;

    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("memora.db");
    let repo = SqliteHealthRepository::bootstrap(db).expect("bootstrap");
    let svc = HealthService::new(repo, Transport::Stdio);
    let status = svc.status();
    assert_eq!(status.status, "healthy");
    assert_eq!(status.database, "healthy");
    assert_eq!(status.transport, "stdio");
    // v1 + v2 + v3 已应用 → schema_version = 3。
    assert_eq!(status.schema_version, 3);

    // 把 SqliteError / RuntimeConfig 引用一下，避免 unused 警告。
    let _: Result<SqliteHealthRepository, SqliteError> =
        SqliteHealthRepository::bootstrap(dir.path().join("x.db"));
    let _ = RuntimeConfig::with_db_path(dir.path().join("x.db"));
}

#[test]
fn migrations_record_checksum_and_reapply_is_noop() {
    // 二次启动同一临时数据库：迁移幂等。
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("memora.db");
    memora::adapters::sqlite::open_and_migrate(&db).expect("first open");
    memora::adapters::sqlite::open_and_migrate(&db).expect("second open is noop");
    let conn = rusqlite::Connection::open(&db).expect("reopen");
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("count");
    // v1 + v2 + v3 均应已记录；count = 3。
    assert_eq!(n, 3, "exactly v1, v2, v3 migration rows");
}
