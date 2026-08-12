//! Application port —— `MemoryRepository` trait 与输入 DTO。
//!
//! 该模块定义 application 与 persistence adapter 之间的全部边界。
//! 返回值全部是 domain 值对象；入参是 application 层的 DTO（避免
//! adapter 直接耦合 MCP 参数结构）。

use crate::domain::{
    ClientCapabilities, Observation, OperationMode, SearchHit, Session, SessionId,
};

use super::errors::MemoryError;

/// session_start 的入参。server 拥有主键与时间戳生成权（design D2）。
///
/// use case 层负责：
/// - 校验 `name` / `agent_id` / `project_id` / `external_session_ref`；
/// - 从 `client_capabilities` 解析出 `operation_mode`（调 `resolve_operation_mode`）
///   并把 caps 序列化为 `capabilities_json`（或 None）。
///   adapter 层只负责把已计算好的字段落库。
#[derive(Debug, Clone)]
pub struct SessionStartInput {
    pub name: String,
    /// v002 预留字段；adapter 原样写入。
    pub agent_id: Option<String>,
    /// v002 预留字段；adapter 原样写入。
    pub project_id: Option<String>,
    /// v002 预留字段；adapter 原样写入。
    pub external_session_ref: Option<String>,
    /// 原始能力声明（use case 用它计算 `operation_mode` / `capabilities_json`）。
    pub client_capabilities: Option<ClientCapabilities>,
    /// 由 use case 计算；adapter 写入 `sessions.operation_mode`。
    pub operation_mode: OperationMode,
    /// 由 use case 计算；`None` ⇒ `capabilities_json IS NULL`。
    pub capabilities_json: Option<String>,
}

/// session_end 的入参。`summary == None` 时仅更新 `ended_at`。
///
/// v003 落地（capability profile 1.4）：当 `summary.is_some()` 时，
/// use case 层计算 SHA-256(content) 并填默认 kind/authority/origin/scope，
/// 把 pre-computed 值塞进 `summary_content_hash` / `summary_kind` /
/// `summary_authority` / `summary_origin` / `summary_scope` / `summary_source_refs_json`。
/// adapter 透传到 summary INSERT 路径。
#[derive(Debug, Clone)]
pub struct SessionEndInput {
    pub session_id: SessionId,
    pub summary: Option<String>,
    /// use case 算出的 summary content SHA-256 hex。
    pub summary_content_hash: Option<String>,
    /// use case 算出的 summary kind（默认 `summary`）。
    pub summary_kind: Option<String>,
    /// use case 算出的 summary authority（默认 `l1_summary`）。
    pub summary_authority: Option<String>,
    /// use case 算出的 summary origin（默认 `user`）。
    pub summary_origin: Option<String>,
    /// use case 算出的 summary scope（默认 `session`）。
    pub summary_scope: Option<String>,
    /// use case 序列化后的 summary source_refs JSON 字符串（默认 `[]`）。
    pub summary_source_refs_json: Option<String>,
}

impl SessionEndInput {
    /// 测试用 helper：构造一个空 summary 的输入（summary_* 字段全 None）。
    pub fn empty(session_id: SessionId) -> Self {
        Self {
            session_id,
            summary: None,
            summary_content_hash: None,
            summary_kind: None,
            summary_authority: None,
            summary_origin: None,
            summary_scope: None,
            summary_source_refs_json: None,
        }
    }
}

/// observe 的入参。`idempotency_key == None` 视为一次性写入。
///
/// v003 扩展（capability profile 1.4 落地）：
/// - `content_hash` 由 use case 层在写入前 SHA-256 计算；adapter 只透传。
/// - `origin` / `scope` / `kind` / `authority` 由 use case 层在缺失时填默认
///   值（origin='user' / scope='session' / kind='observation' / authority='l1_observation'）。
/// - `source_refs_json` 由 use case 层序列化为 JSON 字符串（`[]` / 列表）。
/// - `fact_key` / `supersedes_id` / `expires_at` / `project_id` 直传 NULL 即可。
#[derive(Debug, Clone)]
pub struct ObserveInput {
    pub session_id: SessionId,
    pub content: String,
    pub tool_name: Option<String>,
    pub idempotency_key: Option<String>,
    /// 写入前由 use case 计算的 SHA-256（hex 小写 64 字符）。
    pub content_hash: Option<String>,
    /// 来源标签，缺失时 use case 填 `'user'`。
    pub origin: Option<String>,
    /// 项目 ID（v002 预留，v003 起回写）。
    pub project_id: Option<String>,
    /// 事实去重 key（同 session 唯一）。
    pub fact_key: Option<String>,
    /// 作用域（`session` / `project` / `user`）；缺失时 use case 填 `'session'`。
    pub scope: Option<String>,
    /// 类型（`observation` / `summary`）；缺失时 use case 填 `'observation'`。
    pub kind: Option<String>,
    /// 权威等级（`l1_observation` / `l1_summary` 等）；缺失时 use case 填默认值。
    pub authority: Option<String>,
    /// 引用来源列表（观察来自哪些 prompt / tool_result）。
    pub source_refs: Option<Vec<String>>,
    /// 引用来源 JSON 字符串（由 use case 序列化 `source_refs` 后传入，adapter 直接落库）。
    /// 当 caller 已经提供时，`source_refs_json` 可以覆盖 use case 的默认序列化结果。
    pub source_refs_json: Option<String>,
    /// 过期时间（ISO-8601 字符串；None = 不过期）。
    pub expires_at: Option<String>,
    /// 指向被取代的 observation_id（用于"矛盾检测 / 替换"语义）。
    pub supersedes_id: Option<String>,
}

/// `kind` 过滤：`Both` = observation + summary 都返回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Observation,
    Summary,
    Both,
}

impl SearchKind {
    pub fn from_wire(value: Option<&str>) -> Result<Self, MemoryError> {
        match value.unwrap_or("both") {
            "observation" => Ok(SearchKind::Observation),
            "summary" => Ok(SearchKind::Summary),
            "both" => Ok(SearchKind::Both),
            other => Err(MemoryError::InvalidInput(format!(
                "kind must be observation|summary|both, got {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchInput {
    pub query: String,
    pub session_id: Option<SessionId>,
    pub kind: SearchKind,
    pub limit: u32,
}

/// 持久化层 port —— 6 个方法对应 6 个 L1 MCP tool。
///
/// 约束：
/// - 该 trait MUST NOT 暴露 rusqlite 句柄或 SQL 字符串；
/// - 所有 SQL 由 adapter 内部 prepared statement 承担；
/// - adapter 负责 SELECT-then-INSERT 实现 `idempotency_key` 幂等。
pub trait MemoryRepository: Send + Sync + 'static {
    fn start_session(&self, input: SessionStartInput) -> Result<Session, MemoryError>;
    fn end_session(&self, input: SessionEndInput) -> Result<Session, MemoryError>;
    fn observe(&self, input: ObserveInput) -> Result<Observation, MemoryError>;
    fn recent_observations(
        &self,
        session_id: Option<&SessionId>,
        limit: u32,
    ) -> Result<Vec<Observation>, MemoryError>;
    fn recent_sessions(&self, limit: u32) -> Result<Vec<Session>, MemoryError>;
    fn search(&self, input: SearchInput) -> Result<Vec<SearchHit>, MemoryError>;
}
