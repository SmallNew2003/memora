//! Composition root。
//!
//! 唯一允许把所有模块拼装在一起的位置；`main` 仅调用本模块。
//! 不应在此放置任何业务逻辑。

use std::sync::Arc;

use thiserror::Error;

use crate::adapters::mcp::{serve_stdio, MemoraServer};
use crate::adapters::sqlite::{SqliteError, SqliteHealthRepository, SqliteMemoryRepository};
use crate::application::{HealthService, MemoryService};
use crate::config::RuntimeConfig;
use crate::domain::Transport;

#[derive(Debug, Error)]
pub enum AppError {
    /// SQLite 启动失败。`'static` 错误链通过 `#[source]` 透传，
    /// `main` 打印时建议使用 `{err:#}` 风格以暴露完整原因。
    #[error("database initialization failed: {source}", source = _0)]
    Sqlite(#[source] SqliteError),
    /// 后台任务失败：bootstrap 线程 panic 或被取消。
    #[error("database bootstrap task failed: {0}")]
    BootstrapJoin(#[from] tokio::task::JoinError),
    /// MCP transport 错误。
    #[error("mcp transport error: {0}")]
    Mcp(#[from] rmcp::ErrorData),
}

/// 启动 stdio MCP server 并阻塞运行。
///
/// 启动阶段在 `spawn_blocking` 中打开并迁移数据库（design D5）：
/// 迁移失败最坏会同步 sleep 1.3 秒，必须放在 blocking 线程上以避免
/// 卡住 tokio runtime。
pub async fn run_stdio(config: RuntimeConfig) -> Result<(), AppError> {
    // 1. 启动期：在 blocking 线程上打开数据库并应用所有迁移。
    let db_path = config.db_path.clone();
    let (repo, mem_repo) = tokio::task::spawn_blocking(move || {
        // health repo：单独打开一次连接以拿 schema_version
        let health_repo = SqliteHealthRepository::bootstrap(db_path.clone())?;
        // memory repo：复用同一路径，运行期在每次调用时打开独立连接
        let mem_repo = SqliteMemoryRepository::bootstrap(db_path);
        Ok::<_, SqliteError>((health_repo, mem_repo))
    })
    .await
    .map_err(AppError::BootstrapJoin)?
    .map_err(AppError::Sqlite)?;

    // 2. 组装 application services。
    let health = Arc::new(HealthService::new(repo, Transport::Stdio));
    let memory = Arc::new(MemoryService::new(mem_repo));

    // 3. 组装 MCP adapter。
    let server =
        MemoraServer::<SqliteMemoryRepository>::new(health, memory, config.archive_after_seconds);

    // 4. 启动 stdio transport 并等待生命周期结束。
    serve_stdio(server).await.map_err(AppError::Mcp)?;
    Ok(())
}
