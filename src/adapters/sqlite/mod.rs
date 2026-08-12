//! SQLite adapter。
//!
//! 责任：
//! - 打开 bundled SQLite 连接，验证 FTS5 能力，启用 foreign keys 与有限 busy timeout。
//! - 在 `BEGIN IMMEDIATE` 事务内应用嵌入式迁移。
//! - 通过 `spawn_blocking` 在每个 repository 操作上创建独立连接（design D5），
//!   避免 connection / statement / lock 跨 `.await` 持有。
//!
//! 该模块 MUST NOT 被 application / domain 直接依赖；调用方经 `application::HealthRepository`
//! port 间接触达。

pub mod memory_repository;

pub use memory_repository::SqliteMemoryRepository;

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use thiserror::Error;

use crate::application::HealthRepository;
use crate::migrations::{self, MigrationError};

const FOREIGN_KEYS_PRAGMA: &str = "PRAGMA foreign_keys = ON;";
const BUSY_TIMEOUT_MS: u32 = 5_000;

/// SQLite 启动错误。该错误 MUST NOT 包含业务记录内容或绝对数据库路径。
#[derive(Debug, Error)]
pub enum SqliteError {
    #[error("failed to create parent directory for database: {0}")]
    CreateDir(#[source] std::io::Error),
    #[error("sqlite open failed: {0}")]
    Open(#[from] rusqlite::Error),
    #[error("FTS5 is not available in this SQLite build")]
    Fts5Unavailable,
    #[error("migration failed: {0}")]
    Migration(#[from] MigrationError),
}

/// 打开数据库并应用所有尚未落库的迁移。
///
/// 该函数阻塞在当前线程；调用方负责放在 `spawn_blocking` 或同步测试线程中执行。
/// 返回的连接已应用 schema、设置 foreign keys 和 busy timeout，可安全用于单次查询。
pub fn open_and_migrate(path: &Path) -> Result<Connection, SqliteError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(SqliteError::CreateDir)?;
        }
    }

    // bundled feature 保证 SQLite 静态链接；FTS5 必须在编译时启用。
    let mut conn = Connection::open(path)?;
    conn.execute_batch(FOREIGN_KEYS_PRAGMA)?;
    conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS.into()))?;

    // 验证 FTS5：编译期 SQLite 必须含 FTS5 模块（design D7）。
    // PRAGMA compile_options 在 SQLite 中返回构建时启用的选项列表；
    // 该命令无需结果集，用 execute + last_insert_rowid 之外的 query_row 失败，
    // 因此采用 prepare+query 的模式逐行扫描。
    let fts5_available: bool = {
        let mut stmt = conn.prepare("PRAGMA compile_options")?;
        let mut rows = stmt.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            let opt: String = row.get(0)?;
            if opt == "ENABLE_FTS5" {
                found = true;
                break;
            }
        }
        found
    };
    if !fts5_available {
        return Err(SqliteError::Fts5Unavailable);
    }

    migrations::apply_pending(&mut conn)?;
    Ok(conn)
}

/// SQLite adapter 实现的 `HealthRepository` port。
///
/// 启动期在 composition root 中实例化一次；运行期在 `spawn_blocking` 边界上
/// 通过 `current_schema_version_blocking` 调用，避免共享 connection。
pub struct SqliteHealthRepository {
    /// 启动期校验过的数据库路径。
    ///
    /// 当前实现仅在 `bootstrap` 阶段打开一次独立连接；`db_path` 字段保留是
    /// 为了未来 `spawn_blocking` 边界上打开短生命周期独立连接（design D5），
    /// 不应在没有迁移路径前删除。
    #[allow(dead_code)]
    db_path: PathBuf,
    /// 启动期已知的 schema version；后续 health query 直接返回该值，
    /// 避免每次查询都重新打开数据库并跑一次 `MAX(version)` 查询。
    schema_version: u32,
}

impl SqliteHealthRepository {
    /// 启动期调用：打开数据库、应用迁移、缓存 schema version。
    pub fn bootstrap(db_path: PathBuf) -> Result<Self, SqliteError> {
        // 在启动线程内完成迁移（同步、阻塞），结果在 composition root 复用。
        let conn = open_and_migrate(&db_path)?;
        let version: u32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(SqliteError::Open)?;
        // 启动期读到的 schema_version 必须不超过 binary 内嵌的最高迁移；
        // 这一断言是 spec "旧 binary 打开已升级数据库" 的最后一道防线。
        // 正常启动路径已在 `apply_pending` 处捕获 DatabaseTooNew；
        // 此处是保护层，覆盖 schema_migrations 表被外部直接 INSERT 的极端情况。
        if version > crate::domain::CURRENT_SCHEMA_VERSION {
            return Err(SqliteError::Migration(
                migrations::MigrationError::DatabaseTooNew {
                    recorded: version,
                    binary_max: crate::domain::CURRENT_SCHEMA_VERSION,
                },
            ));
        }
        Ok(Self {
            db_path,
            schema_version: version,
        })
    }
}

impl HealthRepository for SqliteHealthRepository {
    fn current_schema_version(&self) -> u32 {
        // 不再重新打开数据库；启动期已锁定 schema version。
        self.schema_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_and_migrate_creates_schema_migrations_table() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("memora.db");
        let conn = open_and_migrate(&db).expect("open");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 1",
                [],
                |row| row.get(0),
            )
            .expect("query");
        assert_eq!(count, 1);
    }

    #[test]
    fn open_and_migrate_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("memora.db");
        open_and_migrate(&db).expect("first open");
        open_and_migrate(&db).expect("second open must not fail");
    }

    #[test]
    fn bootstrap_caches_schema_version_three() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("memora.db");
        let repo = SqliteHealthRepository::bootstrap(db).expect("bootstrap");
        // v1 + v2 + v3 均已应用，最高版本号 = 3。
        assert_eq!(repo.current_schema_version(), 3);
    }
}
