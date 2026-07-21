//! memora — Local-first multi-layer memory for AI coding agents.
//!
//! Phase 1 仅交付 stdio MCP runtime foundation：
//! - 单二进制 crate，模块化单体（domain / application / adapters / config / app）。
//! - bundled SQLite + 版本化迁移（version + SHA-256 校验和 + busy 退避）。
//! - 只读 `memora_status` tool 端到端验证 runtime 健康。
//!
//! 详细设计见 `openspec/changes/bootstrap-rust-core/`。

pub mod adapters;
pub mod app;
pub mod application;
pub mod config;
pub mod domain;
pub mod migrations;
