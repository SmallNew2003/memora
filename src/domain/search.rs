//! `search` tool 的响应值对象。
//!
//! 字段稳定性约束（spec l1-search-retrieval "响应字段稳定性"）：
//! - `kind` 区分 observation / summary；调用方据此决定是否读 `tool_name`。
//! - `score` 是 BM25 数值，越小越相关；调用方可直接使用或忽略。
//! - 任何既有字段的类型 / 含义 / 顺序 MUST NOT 改变；新增字段视为 minor 演进。

use serde::{Deserialize, Serialize};

use super::observation::Observation;
use super::session::SessionId;
use super::summary::Summary;

/// `search` 工具返回的扁平 hit 列表元素。
///
/// 用统一结构体而非 enum 是为了让 JSON 响应字段集合对调用方完全固定；
/// 避免在 transport 层暴露 discriminator 字段而破坏「字段集合固定」契约。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// "observation" | "summary"
    pub kind: &'static str,
    /// 对应 `observations.id` 或 `summaries.id`。
    pub id: String,
    pub session_id: SessionId,
    pub content: String,
    /// 仅 `kind == "observation"` 时有值；`kind == "summary"` 时为 None。
    pub tool_name: Option<String>,
    pub created_at: String,
    /// BM25 数值，越小越相关；调用方可直接使用或忽略。
    pub score: f64,
}

impl SearchHit {
    /// 从 observation + BM25 score 构造 hit。`tool_name` 透传（None 表示无）。
    pub fn from_observation(obs: Observation, score: f64) -> Self {
        Self {
            kind: "observation",
            id: obs.id.0,
            session_id: obs.session_id,
            content: obs.content,
            tool_name: obs.tool_name,
            created_at: obs.created_at,
            score,
        }
    }

    /// 从 summary + BM25 score 构造 hit。`tool_name` 固定为 None。
    pub fn from_summary(sum: Summary, score: f64) -> Self {
        Self {
            kind: "summary",
            id: sum.id.0,
            session_id: sum.session_id,
            content: sum.content,
            tool_name: None,
            created_at: sum.created_at,
            score,
        }
    }
}
