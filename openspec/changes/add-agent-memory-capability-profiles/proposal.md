## Why

memora 必须同时稳定服务于拥有不透明内建记忆的客户端，以及 OpenCode 部署等没有原生
记忆能力的无状态客户端。若没有明确的 capability 契约，server 要么会假定客户端能够
提供其实际不具备的生命周期 Hook，要么会在没有定义 owner、scope 或冲突策略的情况下
重复并重新注入记忆。

## What Changes

- 增加 client capability negotiation，使 MCP server 根据声明的 lifecycle、capture、
  injection 和 native-memory 支持情况选择行为，而不是硬编码面向特定 Agent 的分支。
- 定义 stateless-client continuity：有预算的启动 context、幂等 checkpoint、可恢复的
  handoff，以及 session 从未关闭时的恢复路径。
- 定义 memory ownership、provenance、deduplication 和 authority 规则：既让 native
  Agent memory 保持在 memora 一致性域外，又允许 memora 数据在 Agent 间有意共享。
- 默认保持 L1 session record 私有；session 派生内容必须经过显式提升至 L2，才能成为
  共享的 project memory。
- **BREAKING**：无。这些是面向未来 MCP API 的增量契约；初始 Phase 1 tools 保持可用，
  仅增加可选字段。

## Capabilities

### New Capabilities

- `agent-memory-capability-profiles`：声明 MCP client 如何报告其 memory、lifecycle、
  tool-capture 与 context-injection 能力，以及 memora 如何选择兼容的 operation mode。
- `session-continuity-and-handoff`：定义有预算的 context preparation、checkpoint、
  handoff record、session recovery 和 stateless-client 恢复流程。
- `memory-provenance-and-isolation`：定义 record origin、scope、ownership、
  deduplication、authority 排序、冲突可见性和 L1-to-L2 提升边界。

### Modified Capabilities

无。`openspec/specs/` 尚未包含已确立的 capability specification；进行中的
OpenSpec-to-L2 bridge 变更保持独立，不受修改。

## Impact

- 未来 Rust memory engine schema 和 repository interface。
- 既有 Phase 1 MCP tools（`session_start`、`observe`、`search` 和 `session_end`），
  以及用于 context preparation 和 checkpoint 的增量 tools。
- 面向不暴露 lifecycle Hook 的宿主，提供可选 client adapter 或生成的 client instruction。
- L2 OpenSpec integration 将消费 authority 和 promotion 契约，但本变更不修改既有的
  bridge specification。
