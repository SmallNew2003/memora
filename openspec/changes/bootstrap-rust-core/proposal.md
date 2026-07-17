## Why

memora currently has design artifacts but no source tree or Cargo manifest. A
discarded experiment is visible only through build output and used an obsolete
RMCP API, so feature work would otherwise begin on an unbuildable and
unreviewable foundation.

## What Changes

- Initialize a single Rust binary crate with a modular-monolith boundary:
  domain, application, SQLite adapter, MCP adapter, configuration, and a
  composition root.
- Establish a versioned SQLite database bootstrap and health check without
  introducing Phase 2 vector retrieval or Phase 3 project-memory behavior.
- Run a stdio MCP server that exposes one read-only `memora_status` tool as an
  end-to-end contract check from MCP request through application and storage.
- Pin an exact Rust toolchain, a verified RMCP server API and their lockfile;
  add deterministic formatting, linting, unit, integration, and MCP contract
  test entry points, with `--locked` used by commands that resolve dependencies.
- **BREAKING**: none. This is the first source implementation; no released
  runtime or MCP callers exist.

## Capabilities

### New Capabilities

- `rust-runtime-foundation`: defines the buildable single-binary runtime,
  dependency direction, configuration, and engineering quality gates.
- `sqlite-schema-bootstrap`: defines local SQLite database creation, versioned
  migrations, and safe startup behavior for an empty or incompatible database.
- `mcp-runtime-health`: defines the stdio MCP service lifecycle and its
  read-only status contract used to verify the runtime end to end.

### Modified Capabilities

None. Existing OpenSpec changes define future L2 knowledge and cross-client
memory contracts; this change only creates their implementation foundation.

## Impact

- Adds `Cargo.toml`, `rust-toolchain.toml`, a regenerated `Cargo.lock`, the
  `src/` module tree, migrations, and test directories. The orphaned lock file
  is treated as superseded generated output, not as a dependency contract.
- Adds Rust dependencies for the current RMCP server API, Tokio runtime,
  serde-based types, SQLite persistence, tracing, and test support.
- Creates a local database file on explicit runtime startup; no network service,
  HTTP transport, embedding model, vector index, or client-specific adapter is
  added.
- Becomes the prerequisite for the planned session-memory and
  agent-capability-profile implementation changes.
