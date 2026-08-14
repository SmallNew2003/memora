//! Configuration 解析。
//!
//! 仅依赖标准库与环境变量；不触发数据库打开或目录创建副作用。

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::domain::Transport;

#[derive(Debug, Error)]
pub enum ConfigError {
    /// `MEMORA_DB_PATH` 包含非 UTF-8 字节。在 Linux/macOS 上系统原生允许任意字节路径，
    /// 但本项目选择仅支持可移植为字符串的路径；非 UTF-8 路径必须由调用方自行决定
    /// 是否落地（典型做法是改用 `MEMORA_DB_PATH` 的 ASCII 等价值）。
    #[error("MEMORA_DB_PATH is not a valid UTF-8 path: {0}")]
    InvalidDbPath(String),
}

/// Runtime 启动所需的全部配置。
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// 数据库文件绝对路径。优先 `MEMORA_DB_PATH`，否则使用默认本地数据目录。
    pub db_path: PathBuf,
    /// MCP transport。当前唯一支持 stdio。
    pub transport: Transport,
    /// 会话归档超时（秒）。超过该时间未活跃的 session 在 recovery 时被归档。
    /// 默认 30 天 = 2592000 秒。
    pub archive_after_seconds: u64,
}

impl RuntimeConfig {
    /// 从环境变量解析配置；不触发 IO，仅路径字符串运算。
    pub fn from_env() -> Result<Self, ConfigError> {
        let db_path = match std::env::var_os("MEMORA_DB_PATH") {
            Some(value) => PathBuf::from(
                value
                    .into_string()
                    .map_err(|os| ConfigError::InvalidDbPath(os.to_string_lossy().into_owned()))?,
            ),
            None => default_db_path(),
        };
        Ok(Self {
            db_path,
            transport: Transport::Stdio,
            archive_after_seconds: 30 * 24 * 60 * 60, // 30 days
        })
    }

    /// 用于测试的构造器。允许显式传入数据库路径。
    pub fn with_db_path(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            transport: Transport::Stdio,
            archive_after_seconds: 30 * 24 * 60 * 60,
        }
    }
}

/// `MEMORA_DB_PATH` 缺失时的默认本地数据目录。
///
/// macOS: `~/Library/Application Support/memora/memora.db`
/// Linux: `~/.local/share/memora/memora.db`
/// Windows: `%LOCALAPPDATA%/memora/memora.db`
fn default_db_path() -> PathBuf {
    let base = dirs_next::data_dir().unwrap_or_else(|| Path::new(".").to_path_buf());
    base.join("memora").join("memora.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_db_path_round_trips() {
        let cfg = RuntimeConfig::with_db_path("/tmp/foo.db");
        assert_eq!(cfg.db_path, PathBuf::from("/tmp/foo.db"));
        assert_eq!(cfg.transport, Transport::Stdio);
    }

    #[test]
    fn default_db_path_ends_with_memora_db() {
        let path = default_db_path();
        assert!(path.ends_with("memora.db"));
    }
}
