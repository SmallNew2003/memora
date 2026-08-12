//! Summary 值对象 —— 手动写入的会话摘要。
//!
//! Phase 1 边界（design / spec "summaries 手动写入边界"）：
//! memora MUST NOT 调用任何 LLM、不生成 AI 压缩摘要；`summaries` 行只能由
//! `session_end` 在调用方传入 `summary` 字符串时产生。

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
}
