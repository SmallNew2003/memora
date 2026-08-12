//! Application layer — use cases and ports.
//!
//! 定义 repository port 与 health query；MUST NOT 依赖 rusqlite、RMCP、文件系统。
//! 端口的具体实现在 `adapters::sqlite` 中提供。

pub mod errors;
pub mod ids;
pub mod ports;
pub mod use_cases;

pub use errors::MemoryError;
pub use ids::uuid_v4;
pub use ports::{
    MemoryRepository, ObserveInput, SearchInput, SearchKind, SessionEndInput, SessionStartInput,
};
pub use use_cases::MemoryService;

use crate::domain::{RuntimeStatus, Transport};

/// Repository port：application 通过该 trait 访问持久化层，
/// 而不直接接触 SQL、连接或迁移细节。
pub trait HealthRepository: Send + Sync + 'static {
    /// 返回当前 binary 已知的最新 schema version。
    /// 启动期由 SQLite adapter 在 migration 后读取。
    fn current_schema_version(&self) -> u32;
}

/// Application service：组合端口并产出领域结果。
pub struct HealthService<R: HealthRepository> {
    repo: R,
    transport: Transport,
}

impl<R: HealthRepository> HealthService<R> {
    pub fn new(repo: R, transport: Transport) -> Self {
        Self { repo, transport }
    }

    /// 端到端健康查询。该方法是 `memora_status` tool 的唯一合法入口。
    pub fn status(&self) -> RuntimeStatus {
        RuntimeStatus {
            status: "healthy".to_string(),
            runtime_version: crate::domain::RUNTIME_VERSION.to_string(),
            schema_version: self.repo.current_schema_version(),
            database: "healthy".to_string(),
            transport: self.transport.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用 fake repository，避免任何 IO。
    struct FakeRepo {
        version: u32,
    }

    impl HealthRepository for FakeRepo {
        fn current_schema_version(&self) -> u32 {
            self.version
        }
    }

    #[test]
    fn health_service_reports_repo_schema_version() {
        let svc = HealthService::new(FakeRepo { version: 2 }, Transport::Stdio);
        let s = svc.status();
        assert_eq!(s.schema_version, 2);
        assert_eq!(s.status, "healthy");
        assert_eq!(s.database, "healthy");
        assert_eq!(s.transport, "stdio");
    }
}
