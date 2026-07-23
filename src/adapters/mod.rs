//! 适配器层聚合：把 `adapters::sqlite` 与 `adapters::mcp` 统一对外暴露。

pub mod mcp;
pub mod sqlite;
