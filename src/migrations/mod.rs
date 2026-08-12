//! 嵌入式迁移。
//!
//! 规则（见 design D4 / spec sqlite-schema-bootstrap）：
//! - 每个迁移拥有连续递增的 `version`（u32）。
//! - 校验和是迁移 SQL 原始 UTF-8 字节的 SHA-256，不做空白/换行标准化。
//! - 已记录迁移 MUST 构成当前 binary 内嵌迁移集合的连续版本前缀；
//!   漂移 / 缺口 / 未来未知版本 → 拒绝启动。
//! - 未应用迁移 MUST 在单一 `BEGIN IMMEDIATE` 事务中按版本顺序应用。

use rusqlite::{params, Connection, Transaction};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// 单个迁移：版本号 + 原始 SQL 字节（用于校验和）。
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

/// 启动期内嵌的迁移集合。版本必须连续：从 1 开始，每次递增 1。
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("v001__schema_migrations.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("v002__l1_memory.sql"),
    },
];

const SCHEMA_MIGRATIONS_DDL: &str = "\
CREATE TABLE IF NOT EXISTS schema_migrations (
    version    INTEGER PRIMARY KEY,
    checksum   TEXT    NOT NULL,
    applied_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
";

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("schema_migrations is corrupted: applied version {found} not in migration set")]
    UnknownAppliedVersion { found: u32 },
    #[error(
        "schema version gap: applied versions stop at {applied}, next migration is {expected}"
    )]
    VersionGap { applied: u32, expected: u32 },
    #[error("checksum mismatch for migration {version}: expected {expected}, recorded {recorded}")]
    ChecksumMismatch {
        version: u32,
        expected: String,
        recorded: String,
    },
    #[error("database is from a newer memora binary: recorded version {recorded}, binary supports up to {binary_max}")]
    DatabaseTooNew { recorded: u32, binary_max: u32 },
    #[error("database is busy after retries: {0}")]
    Busy(String),
    #[error("migration {version} failed: {message}")]
    MigrationFailed { version: u32, message: String },
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// 计算单条迁移的 SHA-256 校验和（原始 UTF-8 字节，不做任何归一化）。
pub fn checksum(sql: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(sql.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// 应用所有尚未落库的迁移，并校验已记录迁移的连续前缀与校验和。
///
/// 该函数假定连接独占（即调用者持有 `BEGIN IMMEDIATE` 或在并发安全边界内），
/// 它本身负责事务边界并对 `SQLITE_BUSY` / `SQLITE_LOCKED` 实施 100/300/900ms
/// 三次退避重试，覆盖 DDL、读取 `schema_migrations` 与事务应用三个阶段：
/// - `SQLITE_BUSY`：另一连接持有写锁竞争。
/// - `SQLITE_LOCKED`：Schema/连接上的元数据锁被竞争；迁移路径上同样需要重试。
pub fn apply_pending(conn: &mut Connection) -> Result<u32, MigrationError> {
    let backoffs_ms = [100u64, 300, 900];
    let mut attempt: usize = 0;
    loop {
        match try_apply_pending(conn) {
            Ok(version) => return Ok(version),
            Err(MigrationError::Sqlite(err)) if is_retryable_lock_error(&err) => {
                if attempt < backoffs_ms.len() {
                    std::thread::sleep(std::time::Duration::from_millis(backoffs_ms[attempt]));
                    attempt += 1;
                    continue;
                }
                return Err(MigrationError::Busy(format!(
                    "after {} retries: {err}",
                    backoffs_ms.len()
                )));
            }
            Err(other) => return Err(other),
        }
    }
}

/// 判断错误是否属于「暂时性并发竞争」并应继续重试。
///
/// 迁移路径上唯一需要重试的就是锁竞争；其它 `SqliteFailure` 一律不重试。
fn is_retryable_lock_error(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::DatabaseBusy,
                ..
            },
            _,
        ) | rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::DatabaseLocked,
                ..
            },
            _,
        )
    )
}

/// 单次尝试：DDL → 读已记录迁移 → 校验连续前缀 → 应用剩余迁移。
///
/// 失败 SQL 走事务回滚；BUSY 由 `apply_pending` 的退避循环捕获并重试。
fn try_apply_pending(conn: &mut Connection) -> Result<u32, MigrationError> {
    // 1. 创建 schema_migrations（若不存在）。
    conn.execute_batch(SCHEMA_MIGRATIONS_DDL)?;

    // 2. 读取已记录迁移，按版本排序。
    let mut stmt =
        conn.prepare("SELECT version, checksum FROM schema_migrations ORDER BY version ASC")?;
    let recorded: Vec<(u32, String)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    // 3. 校验已记录迁移：必须是当前 binary 内嵌集合的连续前缀。
    for (i, (recorded_version, recorded_checksum)) in recorded.iter().enumerate() {
        let expected = MIGRATIONS.get(i).ok_or(MigrationError::DatabaseTooNew {
            recorded: *recorded_version,
            binary_max: MIGRATIONS.last().map(|m| m.version).unwrap_or(0),
        })?;
        if expected.version != *recorded_version {
            return Err(MigrationError::VersionGap {
                applied: recorded_version.saturating_sub(1),
                expected: expected.version,
            });
        }
        let expected_checksum = checksum(expected.sql);
        if &expected_checksum != recorded_checksum {
            return Err(MigrationError::ChecksumMismatch {
                version: *recorded_version,
                expected: expected_checksum,
                recorded: recorded_checksum.clone(),
            });
        }
    }

    // 4. 决定还需要应用哪些迁移。
    debug_assert!(
        recorded.len() <= MIGRATIONS.len(),
        "try_apply_pending: enforced by the prefix check above",
    );
    let pending = &MIGRATIONS[recorded.len()..];
    if pending.is_empty() {
        return Ok(MIGRATIONS.last().map(|m| m.version).unwrap_or(0));
    }

    // 5. 在 BEGIN IMMEDIATE 事务中按顺序应用。失败 SQL 自动回滚。
    run_pending_in_transaction(conn, pending)
}

/// 在 `BEGIN IMMEDIATE` 事务中应用给定迁移切片。
///
/// 暴露为 `pub(crate)` 是为了让 `#[cfg(test)]` 注入自定义失败用例；
/// 仅测试能调用，正常 production 路径走 `apply_pending`。
pub(crate) fn run_pending_in_transaction(
    conn: &mut Connection,
    pending: &[Migration],
) -> Result<u32, MigrationError> {
    let tx: Transaction<'_> =
        conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut last_version = 0u32;
    for migration in pending {
        // execute_batch 在多语句 SQL 上失败时，整事务由 ? 路径回滚。
        tx.execute_batch(migration.sql)
            .map_err(|err| MigrationError::MigrationFailed {
                version: migration.version,
                message: err.to_string(),
            })?;
        let checksum_hex = checksum(migration.sql);
        tx.execute(
            "INSERT INTO schema_migrations (version, checksum) VALUES (?1, ?2)",
            params![migration.version, checksum_hex],
        )?;
        last_version = migration.version;
    }
    tx.commit()?;
    Ok(last_version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn checksum_is_stable_hex_64_chars() {
        // SHA-256 输出恒为 32 字节 = 64 个十六进制字符；锁定长度即锁定算法。
        let c = checksum("SELECT 1;");
        assert_eq!(c.len(), 64);
        assert!(c.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn checksum_changes_on_byte_change() {
        // 不同输入必须产生不同校验和，避免出现「空白被默默规范化」的回归。
        assert_ne!(checksum("SELECT 1;"), checksum("SELECT 1;\n"));
    }

    #[test]
    fn migrations_are_contiguous_from_one() {
        let mut expected = 1u32;
        for m in MIGRATIONS {
            assert_eq!(
                m.version, expected,
                "migration version must be contiguous from 1"
            );
            expected += 1;
        }
    }

    /// 准备一份已应用 v1 迁移的临时数据库，返回打开的连接。
    fn conn_with_v1_applied(path: &std::path::Path) -> Connection {
        let mut conn = Connection::open(path).expect("open");
        apply_pending(&mut conn).expect("apply v1");
        conn
    }

    // ── spec sqlite-schema-bootstrap 场景覆盖 ───────────────────────

    #[test]
    fn apply_pending_succeeds_on_fresh_database() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut conn = Connection::open(dir.path().join("fresh.db")).expect("open");
        let version = apply_pending(&mut conn).expect("apply");
        // binary 当前内嵌 v1 + v2；fresh db 一键应用全部，最高版本号 = 2。
        assert_eq!(version, 2);
    }

    #[test]
    fn apply_pending_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("idem.db");
        let mut conn = Connection::open(&db).expect("open");
        apply_pending(&mut conn).expect("first");
        drop(conn);
        let mut conn = Connection::open(&db).expect("reopen");
        apply_pending(&mut conn).expect("second is noop");
    }

    #[test]
    fn checksum_drift_is_rejected() {
        // 场景「迁移校验和漂移」：已记录迁移的校验和与 binary 内嵌不一致，必须拒绝启动。
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("drift.db");
        let mut conn = conn_with_v1_applied(&db);

        // 用错误校验和覆盖 v1 行。
        conn.execute(
            "UPDATE schema_migrations SET checksum = ?1 WHERE version = 1",
            ["deadbeef"],
        )
        .expect("tamper");

        let err = apply_pending(&mut conn).expect_err("must reject");
        match err {
            MigrationError::ChecksumMismatch { version, .. } => assert_eq!(version, 1),
            other => panic!("expected ChecksumMismatch, got {other:?}"),
        }
    }

    #[test]
    fn database_too_new_is_rejected() {
        // 场景「旧 binary 打开已升级数据库」：记录版本高于 binary 内嵌最高版本。
        // binary 当前内嵌最高版本 2，因此塞入 version = 3 模拟 future。
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("future.db");
        let mut conn = conn_with_v1_applied(&db);

        conn.execute(
            "INSERT INTO schema_migrations (version, checksum) VALUES (?1, ?2)",
            rusqlite::params![3u32, "future-checksum"],
        )
        .expect("seed future version");

        let err = apply_pending(&mut conn).expect_err("must reject");
        match err {
            MigrationError::DatabaseTooNew { recorded, .. } => assert_eq!(recorded, 3),
            other => panic!("expected DatabaseTooNew, got {other:?}"),
        }
    }

    #[test]
    fn version_or_checksum_recorded_out_of_range_is_rejected() {
        // 场景「记录版本超出 binary 支持」与「版本缺口」同族：
        // 当前 binary 仅 v1，因此任何 recorded[i].version != MIGRATIONS[i].version
        // 或者 recorded.len() > MIGRATIONS.len() 都会被前缀循环捕获为
        // DatabaseTooNew / VersionGap。本测试确保这条防线不被绕过。
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("gap.db");
        let mut conn = conn_with_v1_applied(&db);

        // 把 v1 行捏成 version = 5：与 MIGRATIONS[0].version(=1) 不一致。
        conn.execute(
            "UPDATE schema_migrations SET version = 5 WHERE version = 1",
            [],
        )
        .expect("force gap");
        let err = apply_pending(&mut conn).expect_err("must reject");
        assert!(
            matches!(
                err,
                MigrationError::DatabaseTooNew { .. } | MigrationError::VersionGap { .. }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn failed_migration_sql_rolls_back() {
        // 场景「无效迁移 SQL」：迁移失败时事务整体回滚，schema_migrations 不会写入。
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("rollback.db");
        let mut conn = Connection::open(&db).expect("open");
        // 先应用 v1，确保 schema_migrations 表存在。
        conn.execute_batch(SCHEMA_MIGRATIONS_DDL).expect("ddl");

        // 故意塞入一条语法错误的迁移。
        let bad = [Migration {
            version: 1,
            sql: "THIS IS NOT VALID SQL",
        }];
        let err = run_pending_in_transaction(&mut conn, &bad).expect_err("must fail");
        match err {
            MigrationError::MigrationFailed { version, .. } => assert_eq!(version, 1),
            other => panic!("expected MigrationFailed, got {other:?}"),
        }

        // 验证 schema_migrations 仍是空表，整事务被回滚。
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(count, 0, "failed migration must roll back fully");
    }

    #[test]
    fn busy_budget_exhausted_returns_retryable_error() {
        // 场景「busy 退避预算耗尽返回可重试错误」：
        // 用一个独立的连接持 BEGIN IMMEDIATE 写锁，让 apply_pending 必须等；
        // 由于我们对 blocker 连接执行了不可读 DDL 后才持锁，apply_pending 在
        // 第一步 `execute_batch(SCHEMA_MIGRATIONS_DDL)` 就会撞 SQLITE_BUSY，
        // 验证退避循环已覆盖到 DDL 阶段（不只是事务应用阶段）。
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("busy.db");

        // 让主连接走完一次迁移，建立 schema_migrations 表。
        let mut conn = Connection::open(&db).expect("open");
        apply_pending(&mut conn).expect("first apply");
        // 清表数据，使 next apply_pending 必须做点工作。
        conn.execute("DELETE FROM schema_migrations", [])
            .expect("clean");
        drop(conn);

        // 在另一个连接上 BEGIN IMMEDIATE 持锁。
        let blocker = Connection::open(&db).expect("blocker open");
        blocker
            .execute("BEGIN IMMEDIATE", [])
            .expect("begin immediate");

        // 触发退避：调用 apply_pending 应在 DDL 上撞 BUSY，三次退避后放弃。
        let mut conn = Connection::open(&db).expect("main open");
        let start = std::time::Instant::now();
        let err = apply_pending(&mut conn).expect_err("must exhaust");
        let elapsed = start.elapsed();

        // 退避预算严格上限：100+300+900 = 1300ms；busy_timeout(5000) 兜底可能让某次
        // 等待超过 1300ms，但只要最终放弃返回 Busy 即满足语义。
        assert!(
            elapsed >= std::time::Duration::from_millis(1300),
            "expected at least 3 backoff sleeps, got {elapsed:?}"
        );
        match err {
            MigrationError::Busy(_) => {}
            other => panic!("expected Busy, got {other:?}"),
        }

        blocker.execute("ROLLBACK", []).ok();
    }
}
