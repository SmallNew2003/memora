//! Summary 值对象 —— 手动写入的会话摘要。
//!
//! Phase 1 边界（design / spec "summaries 手动写入边界"）：
//! memora MUST NOT 调用任何 LLM、不生成 AI 压缩摘要；`summaries` 行只能由
//! `session_end` 在调用方传入 `summary` 字符串时产生。
//!
//! v003 落地（capability profile 1.4）：与 Observation 同形地补上 9 个新字段
//! （scope / kind / origin / authority / source_refs_json / expires_at /
//! supersedes_id / fact_key / project_id）+ content_hash。end_session use case
//! 在写入 summary 时同样通过 use case 层补默认值 + SHA-256。

use serde::{Deserialize, Serialize};

use super::session::SessionId;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SummaryId(pub String);

impl SummaryId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SummaryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub id: SummaryId,
    pub session_id: SessionId,
    pub content: String,
    pub created_at: String,

    // v002 已有列（v003 起回读）
    pub content_hash: Option<String>,

    // v003 新增列（与 Observation 同形，kind 默认 'summary' / authority 默认 'l1_summary'）
    pub scope: Option<String>,
    pub kind: Option<String>,
    pub origin: Option<String>,
    pub project_id: Option<String>,
    pub authority: Option<String>,
    pub source_refs_json: Option<String>,
    pub expires_at: Option<String>,
    pub supersedes_id: Option<String>,
    pub fact_key: Option<String>,
}
