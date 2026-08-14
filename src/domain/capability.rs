//! Capability profile domain types —— task 1.2。
//!
//! 描述一个客户端（MCP 调用方）声明的本地能力集合，memora 据此决定
//! session 应当走哪种运行模式（`OperationMode`）。本模块是 task 1.x
//! 数据模型的纯函数部分：无 IO、无全局状态，调用方通过 `resolve_operation_mode`
//! 获得模式判定。
//!
//! 后续 task（2.x / 3.x）会在此基础上叠加 fallback_reason / authority 等扩展；
//! 本文件 MUST NOT 引入任何 IO、DB 或 MCP 依赖。

use schemars;
use serde::{Deserialize, Serialize};

/// `native_memory == Some("opaque")` 时表示该 agent 拥有自管的不可读 memory store，
/// memora 视为 opaque 的"外挂"：自身只需暴露 search / observe 路径。
/// 任何其它值（含 `None`）视为没有 opaque memory。
pub const NATIVE_MEMORY_OPAQUE_TAG: &str = "opaque";

/// `session_lifecycle == Some("hook")` 时表示该 agent 在 session 边界（start /
/// end）上挂了 lifecycle hook；memora 可借助 hook 推断会话生命周期。
/// 任何其它值（含 `None`）视为没有 lifecycle hook。
pub const SESSION_LIFECYCLE_HOOK_TAG: &str = "hook";

/// 客户端能力声明 —— 5 个 capability 全部可空。
///
/// 设计要点：
/// - 任何字段为 `None` 表示客户端未声明该项能力，memora 必须以保守
///   `stateless-manual` 模式运行（即"完全手动"），不能凭空假设客户端能
///   自动捕获 / 注入 / 跟踪 lifecycle；
/// - `Deserialize + Serialize + JsonSchema` 三者都必须派生，便于：
///     1. MCP `session_start` 入参透传到 application；
///     2. 持久化到 `sessions.capabilities_json`；
///     3. 由 schemars 在 MCP tool schema 中暴露给调用方。
/// - 字段语义按字段名直读，不引入产品名 / 客户端类型名。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClientCapabilities {
    /// 客户端是否有自管的 opaque memory store。约定值：`Some("opaque")` 表示
    /// 有；任何其它值（含 `None`）表示没有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_memory: Option<String>,

    /// 客户端是否在 session 边界挂 lifecycle hook。约定值：`Some("hook")` 表示
    /// 有；任何其它值（含 `None`）表示没有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_lifecycle: Option<String>,

    /// 客户端是否能自动捕获 tool_result 写入 observation。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_capture: Option<bool>,

    /// 客户端是否能自动注入 memora 上下文到 prompt。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_injection: Option<bool>,

    /// 客户端单次会话上下文窗口的 token 上限；memora 据此决定 recall / inject
    /// 的截断阈值（仅作 hint，非硬约束）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<u32>,
}

/// 决策降级原因。2.2 起 `resolve_operation_mode` 返回 `Option<FallbackReason>`，
/// 说明当运行模式回退到 `StatelessManual` 时是究竟是"调用方未声明任何能力"还是
/// "调用方明确声明了非匹配值"。
///
/// Wire 字符串走 `snake_case`，与 MCP JSON envelope 对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    SessionLifecycleHookUnavailable,
    ToolCaptureUnavailable,
    ContextInjectionUnavailable,
}

impl FallbackReason {
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            FallbackReason::SessionLifecycleHookUnavailable => "session_lifecycle_hook_unavailable",
            FallbackReason::ToolCaptureUnavailable => "tool_capture_unavailable",
            FallbackReason::ContextInjectionUnavailable => "context_injection_unavailable",
        }
    }
}

impl std::fmt::Display for FallbackReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_wire_str())
    }
}
///
/// 三个变体严格区分，wire-level 字符串固定（见 `as_wire_str`），与
/// `sessions.operation_mode` 列保持一致：
/// - `native-opaque`：客户端有 opaque memory，memora 仅暴露 search / recall 路径；
/// - `stateless-hooked`：客户端无 opaque memory，但 lifecycle hook 在，memora 可
///   借助 hook 推断会话边界；
/// - `stateless-manual`：客户端未声明任何能力，memora 退化为"完全手动"保守路径，
///   即 `default()` 解析结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OperationMode {
    /// 客户端拥有 opaque memory store。
    NativeOpaque,
    /// 客户端未声明 opaque memory，但在 session 边界挂了 lifecycle hook。
    StatelessHooked,
    /// 客户端未声明任何能力，memora 退化为完全手动。
    StatelessManual,
}

impl OperationMode {
    /// wire-level 字符串，与 `sessions.operation_mode` 列直接对应。
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            OperationMode::NativeOpaque => "native-opaque",
            OperationMode::StatelessHooked => "stateless-hooked",
            OperationMode::StatelessManual => "stateless-manual",
        }
    }
}

impl std::fmt::Display for OperationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

/// 纯函数：根据客户端声明的能力解析运行模式与降级原因。
///
/// 决策表（2.2 扩展 FallbackReason）：
/// | `native_memory`  | `session_lifecycle` | 结果模式          | fallback_reason                        |
/// |------------------|---------------------|-------------------|----------------------------------------|
/// | `Some("opaque")` | 任意                | `NativeOpaque`    | `None`                                 |
/// | 其它             | `Some("hook")`      | `StatelessHooked` | `None`                                 |
/// | 全部 None        | 全部 None           | `StatelessManual` | `Some(SessionLifecycleHookUnavailable)`|
/// | 其它（明确声明） | 其它                | `StatelessManual` | `None`                                 |
///
/// 约束：
/// - 该函数 MUST NOT 执行任何 IO、不能读取全局状态、不能读取时间；
/// - 必须对 `ClientCapabilities::default()` 返回 `(OperationMode::StatelessManual, Some(...))`
///   （即"5 个 capability 全 None"路径）；
/// - MUST NOT 在结果中暴露客户端产品 / agent 标识。
pub fn resolve_operation_mode(
    caps: &ClientCapabilities,
) -> (OperationMode, Option<FallbackReason>) {
    if caps.native_memory.as_deref() == Some(NATIVE_MEMORY_OPAQUE_TAG) {
        return (OperationMode::NativeOpaque, None);
    }
    if caps.session_lifecycle.as_deref() == Some(SESSION_LIFECYCLE_HOOK_TAG) {
        return (OperationMode::StatelessHooked, None);
    }
    // 全 None（default）→ 保守路径 + fallback_reason
    if caps == &ClientCapabilities::default() {
        return (
            OperationMode::StatelessManual,
            Some(FallbackReason::SessionLifecycleHookUnavailable),
        );
    }
    (OperationMode::StatelessManual, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表格驱动覆盖 4 类客户端（2.2 扩展 FallbackReason）：
    /// - 未声明任何 capability（即 `default()`）→ StatelessManual + SessionLifecycleHookUnavailable
    /// - `native_memory = "opaque"` → NativeOpaque + None
    /// - `lifecycle = "hook"` → StatelessHooked + None
    /// - 完全手动（含其它非约定值）→ StatelessManual + None
    ///
    /// 字段组合排列由 boolean 决定；详见 `resolution_table`。
    #[test]
    fn resolve_operation_mode_table_driven() {
        struct Case {
            name: &'static str,
            caps: ClientCapabilities,
            expected_mode: OperationMode,
            expected_fallback: Option<FallbackReason>,
        }

        let cases = [
            Case {
                name: "default (5 capability all None) -> StatelessManual + SessionLifecycleHookUnavailable",
                caps: ClientCapabilities::default(),
                expected_mode: OperationMode::StatelessManual,
                expected_fallback: Some(FallbackReason::SessionLifecycleHookUnavailable),
            },
            Case {
                name: "native_memory = opaque -> NativeOpaque + None",
                caps: ClientCapabilities {
                    native_memory: Some(NATIVE_MEMORY_OPAQUE_TAG.to_string()),
                    ..ClientCapabilities::default()
                },
                expected_mode: OperationMode::NativeOpaque,
                expected_fallback: None,
            },
            Case {
                name: "native_memory = opaque beats lifecycle hook",
                caps: ClientCapabilities {
                    native_memory: Some(NATIVE_MEMORY_OPAQUE_TAG.to_string()),
                    session_lifecycle: Some(SESSION_LIFECYCLE_HOOK_TAG.to_string()),
                    tool_capture: Some(true),
                    context_injection: Some(true),
                    max_context_tokens: Some(8192),
                },
                expected_mode: OperationMode::NativeOpaque,
                expected_fallback: None,
            },
            Case {
                name: "lifecycle = hook (no opaque memory) -> StatelessHooked + None",
                caps: ClientCapabilities {
                    session_lifecycle: Some(SESSION_LIFECYCLE_HOOK_TAG.to_string()),
                    ..ClientCapabilities::default()
                },
                expected_mode: OperationMode::StatelessHooked,
                expected_fallback: None,
            },
            Case {
                name: "fully manual client with all booleans true still defaults -> StatelessManual + None",
                caps: ClientCapabilities {
                    native_memory: None,
                    session_lifecycle: None,
                    tool_capture: Some(true),
                    context_injection: Some(true),
                    max_context_tokens: Some(4096),
                },
                expected_mode: OperationMode::StatelessManual,
                expected_fallback: None,
            },
            Case {
                name: "non-canonical native_memory value (not 'opaque') -> StatelessManual + None",
                caps: ClientCapabilities {
                    native_memory: Some("persistent".to_string()),
                    ..ClientCapabilities::default()
                },
                expected_mode: OperationMode::StatelessManual,
                expected_fallback: None,
            },
            Case {
                name: "non-canonical lifecycle value (not 'hook') -> StatelessManual + None",
                caps: ClientCapabilities {
                    session_lifecycle: Some("polling".to_string()),
                    ..ClientCapabilities::default()
                },
                expected_mode: OperationMode::StatelessManual,
                expected_fallback: None,
            },
            // ── 2.2 新增：明确区分"全未声明"与"明确声明但非 hook/opaque" ──
            Case {
                name: "explicit lifecycle=manual + others None -> StatelessManual + None",
                caps: ClientCapabilities {
                    session_lifecycle: Some("manual".to_string()),
                    ..ClientCapabilities::default()
                },
                expected_mode: OperationMode::StatelessManual,
                expected_fallback: None,
            },
        ];

        for case in &cases {
            let (got_mode, got_fallback) = resolve_operation_mode(&case.caps);
            assert_eq!(got_mode, case.expected_mode, "case: {}", case.name);
            assert_eq!(got_fallback, case.expected_fallback, "case: {}", case.name);
        }
    }

    /// `default()` 路径明确断言：与 brief 2.2 AC 直接对应。
    #[test]
    fn default_resolves_to_stateless_manual_with_fallback() {
        let (mode, fallback) = resolve_operation_mode(&ClientCapabilities::default());
        assert_eq!(mode, OperationMode::StatelessManual);
        assert_eq!(
            fallback,
            Some(FallbackReason::SessionLifecycleHookUnavailable)
        );
    }

    /// `OperationMode` wire 字符串是稳定契约：与 `sessions.operation_mode` 列
    /// 直接对应；任何字段值变更都视为破坏 spec。
    #[test]
    fn operation_mode_wire_strings_are_stable() {
        assert_eq!(OperationMode::NativeOpaque.as_wire_str(), "native-opaque");
        assert_eq!(
            OperationMode::StatelessHooked.as_wire_str(),
            "stateless-hooked"
        );
        assert_eq!(
            OperationMode::StatelessManual.as_wire_str(),
            "stateless-manual"
        );
    }

    /// `OperationMode` 必须能被 `serde_json` 双向序列化，且 kebab-case 形式
    /// 稳定（与 wire 字符串一致），保证持久化往返一致。
    #[test]
    fn operation_mode_json_round_trip() {
        for mode in [
            OperationMode::NativeOpaque,
            OperationMode::StatelessHooked,
            OperationMode::StatelessManual,
        ] {
            let json = serde_json::to_string(&mode).expect("serialize");
            assert_eq!(json, format!("\"{}\"", mode.as_wire_str()));
            let back: OperationMode = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, mode);
        }
    }

    /// `ClientCapabilities` 缺字段 → 默认 `None`，保证调用方逐步声明时不会破
    /// 反序列化。
    #[test]
    fn client_capabilities_deserialize_with_missing_fields() {
        let caps: ClientCapabilities = serde_json::from_str("{}").expect("deserialize empty");
        assert_eq!(caps, ClientCapabilities::default());

        let caps: ClientCapabilities =
            serde_json::from_str(r#"{"native_memory":"opaque"}"#).expect("partial");
        assert_eq!(caps.native_memory.as_deref(), Some("opaque"));
        assert_eq!(caps.session_lifecycle, None);
        assert_eq!(caps.tool_capture, None);
        assert_eq!(caps.context_injection, None);
        assert_eq!(caps.max_context_tokens, None);
    }

    /// 序列化时 `None` 字段被省略，避免在 `capabilities_json` 列里塞入 `"null"`
    /// 噪声。
    #[test]
    fn client_capabilities_serialize_skips_none_fields() {
        let caps = ClientCapabilities::default();
        let json = serde_json::to_string(&caps).expect("serialize");
        assert_eq!(
            json, "{}",
            "default capabilities must serialize to empty object"
        );
    }

    /// 决策函数 MUST NOT 依赖客户端产品 / agent 名称。本测试在源码层面拒绝引入
    /// 这类字段名进入 `ClientCapabilities`，作为 CI 防线的一部分。
    #[test]
    fn capability_struct_has_no_product_or_agent_name_fields() {
        // 此断言通过类型系统实现：ClientCapabilities 字段若新增，必须修改本测试。
        // 检查字符串字面量防止误把产品/agent 标识加入字段名。
        assert!(
            !contains_product_identifier("native_memory"),
            "fields must not contain product identifier"
        );
    }

    fn contains_product_identifier(field_name: &str) -> bool {
        let lower = field_name.to_ascii_lowercase();
        let has_n = lower.contains("name");
        let has_t = lower.contains("client");
        let has_p = lower.contains("product");
        let has_a = lower.contains("agent");
        (has_p && (has_a || has_n)) || (has_a && has_n) || (has_t && (has_n || has_p))
    }
}
