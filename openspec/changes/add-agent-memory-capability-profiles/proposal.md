## Why

memora must work consistently for both clients with opaque, built-in memory and
stateless clients such as OpenCode deployments without a native memory feature.
Without an explicit capability contract, the server either assumes lifecycle
hooks that a client cannot provide or duplicates and reinjects memories without
a defined owner, scope, or conflict policy.

## What Changes

- Add client capability negotiation so the MCP server selects behavior from
  declared lifecycle, capture, injection, and native-memory support instead of
  hard-coding agent-specific branches.
- Define stateless-client continuity: bounded startup context, idempotent
  checkpoints, resumable handoffs, and recovery when a session never closes.
- Define memory ownership, provenance, deduplication, and authority rules that
  keep native agent memory outside memora's consistency domain while allowing
  memora data to be shared deliberately across agents.
- Keep L1 session records private by default; require an explicit promotion to
  L2 before a session-derived item becomes shared project memory.
- **BREAKING**: none. These are additive contracts for the future MCP API; the
  initial Phase 1 tools remain available and gain only optional fields.

## Capabilities

### New Capabilities

- `agent-memory-capability-profiles`: declares how an MCP client reports its
  memory, lifecycle, tool-capture, and context-injection capabilities and how
  memora selects a compatible operation mode.
- `session-continuity-and-handoff`: defines bounded context preparation,
  checkpoints, handoff records, session recovery, and the stateless-client
  resume flow.
- `memory-provenance-and-isolation`: defines record origin, scope, ownership,
  deduplication, authority ordering, conflict visibility, and L1-to-L2
  promotion boundaries.

### Modified Capabilities

None. `openspec/specs/` has no established capability specifications yet; the
active OpenSpec-to-L2 bridge change remains independent and is not modified.

## Impact

- Future Rust memory engine schema and repository interfaces.
- Existing Phase 1 MCP tools (`session_start`, `observe`, `search`, and
  `session_end`) plus additive tools for context preparation and checkpoints.
- Optional client adapters or generated client instructions for hosts that do
  not expose lifecycle hooks.
- L2 OpenSpec integration will consume the authority and promotion contracts,
  but this change does not alter its existing bridge specification.
