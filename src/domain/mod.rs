//! Domain layer — pure business types with no IO.
//!
//! Domain 与 application、adapters 之间通过类型隔离；
//! 本模块 MUST NOT 依赖 rusqlite、RMCP、Tokio、文件系统或时间相关 API。

pub mod capability;
pub mod observation;
pub mod search;
pub mod session;
pub mod summary;

pub use capability::{
    resolve_operation_mode, ClientCapabilities, FallbackReason, OperationMode,
    NATIVE_MEMORY_OPAQUE_TAG, SESSION_LIFECYCLE_HOOK_TAG,
};
pub use observation::{Observation, ObservationId};
pub use search::SearchHit;
pub use session::{Session, SessionId};
pub use summary::{Summary, SummaryId};

/// memora runtime 的版本字符串，编译期从 `Cargo.toml` 注入。
///
/// 详见 design D10：避免运行时读取 manifest 导致的双源真相与读取失败模式。
pub const RUNTIME_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 数据库 schema 的当前版本，对应当前 binary 内嵌迁移的最高版本号。
///
/// 变更规则：每次新增迁移 SQL 时，本常量同步递增。
///
/// 仅 `crate::adapters::sqlite` 用它做 sanity-check（确认从启动期读到的
/// 实际 schema_version 不高于 binary 自身支持的最高版本）。Health query 的
/// schema_version 永远走 `HealthRepository::current_schema_version()`，
/// 避免出现「domain 常量 vs repo 上报值」的双源真相。
pub(crate) const CURRENT_SCHEMA_VERSION: u32 = 3;

/// MCP transport 标识。当前仅启用 stdio；后续 transport 通过 enum 扩展，
/// 切勿硬编码到业务层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Stdio,
}

impl Transport {
    /// 固定字符串字面量，作为 wire-level 字段值使用。
    pub const fn as_str(self) -> &'static str {
        match self {
            Transport::Stdio => "stdio",
        }
    }
}

/// memora_status 的五字段响应。严格遵守 spec mcp-runtime-health "只读运行时状态工具"：
/// - `status` / `database` 初始值仅允许 `healthy`
/// - `transport` 固定为 `stdio`
/// - 不暴露绝对路径或记忆内容
///
/// 单一真理之源：`HealthService::status()` 是构造 RuntimeStatus 的唯一入口；
/// domain 不再暴露「便捷构造器」，避免与 application 双源实现。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RuntimeStatus {
    pub status: String,
    pub runtime_version: String,
    pub schema_version: u32,
    pub database: String,
    pub transport: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HealthService 是 RuntimeStatus 的唯一构造点；domain 单元测试通过它走一遍。
    fn status_via_service(version: u32) -> RuntimeStatus {
        struct FixedVersion(u32);
        impl crate::application::HealthRepository for FixedVersion {
            fn current_schema_version(&self) -> u32 {
                self.0
            }
        }
        crate::application::HealthService::new(FixedVersion(version), Transport::Stdio).status()
    }

    #[test]
    fn runtime_status_has_all_five_required_fields() {
        let status = status_via_service(1);
        let json = serde_json::to_value(&status).expect("serialize");
        let obj = json.as_object().expect("object");
        assert_eq!(obj.len(), 5);
        assert_eq!(obj["status"], "healthy");
        assert_eq!(obj["database"], "healthy");
        assert_eq!(obj["transport"], "stdio");
        assert!(obj["runtime_version"].is_string());
        assert!(obj["schema_version"].is_u64());
    }

    #[test]
    fn schema_version_tracks_repo_not_constant() {
        // 单一真理之源：repo 上报啥就是啥。domain 常量值入
        // CURRENT_SCHEMA_VERSION 不应被 runtime 直接读取。
        assert_eq!(status_via_service(7).schema_version, 7);
    }

    #[test]
    fn transport_stdio_serializes_to_constant() {
        assert_eq!(Transport::Stdio.as_str(), "stdio");
    }

    #[test]
    fn current_schema_version_is_three() {
        // guard：binary 内嵌最高迁移版本号 = 3；新增迁移时同步递增。
        assert_eq!(CURRENT_SCHEMA_VERSION, 3);
    }
}
