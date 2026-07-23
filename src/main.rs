//! Binary entry point.
//!
//! 仅负责：
//! 1. 解析配置；
//! 2. 初始化 tracing（仅写 stderr，避免污染 stdio JSON-RPC）；
//! 3. 调用 composition root。
//!
//! 不承载任何业务逻辑。

use std::process::ExitCode;

use memora::app::run_stdio;
use memora::config::RuntimeConfig;

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    let config = match RuntimeConfig::from_env() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("memora: invalid configuration: {err:#}");
            return ExitCode::from(2);
        }
    };

    match run_stdio(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // 设计 D2 / spec rust-runtime-foundation "配置与协议日志隔离"：
            // 错误一律走 stderr，绝不写入 stdout。
            // `{err:#}` 会顺 `source()` 链展开（路径不可写 / busy / checksum 漂移等
            // 都能直接落到 stderr 上，便于 Agent / 用户定位）。
            eprintln!("memora: startup failed: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// 初始化 tracing，所有日志写到 stderr。env_filter 允许通过
/// `MEMORA_LOG` 控制级别；未设置时默认 `info`。
fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_env("MEMORA_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(false)
                .with_target(false),
        )
        .init();
}
