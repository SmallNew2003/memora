-- Migration v002: L1 session memory — sessions / observations / summaries + FTS5
--
-- 三张业务表 + 两个 FTS5 contentless 虚拟表 + INSERT/UPDATE/DELETE 触发器，
-- 共同支撑 6 个 L1 MCP tool：
--   - session_start / session_end
--   - observe（带 idempotency_key 幂等）
--   - recent_observations / recent_sessions（按 (created_at DESC, id DESC)）
--   - search（FTS5 BM25）
--
-- 字段分类：
--   - 必填业务列：本变更 MCP tool 读取/写入；
--   - 预留扩展列：可空形式存在，本变更 MUST NOT 读取、不索引、不校验，
--     留给后续 add-agent-memory-capability-profiles 等变更叠加 scope /
--     origin / authority / handoff 等语义。
--
-- 时间戳策略：统一 `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` 写入 UTC ISO-8601
-- 字符串，与 v001 schema_migrations.applied_at 一致。
--
-- 校验和：迁移 SQL 原始 UTF-8 字节的 SHA-256，由 application 启动期计算并写入。

-- ── 业务表 ───────────────────────────────────────────────────────

CREATE TABLE sessions (
    id                   TEXT PRIMARY KEY,
    name                 TEXT NOT NULL,
    created_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    ended_at             TEXT,
    summary              TEXT,
    -- 预留字段：本变更 MUST NOT 读取、不索引、不校验其语义。
    agent_id             TEXT,
    project_id           TEXT,
    external_session_ref TEXT
);

CREATE TABLE observations (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES sessions(id),
    content         TEXT NOT NULL,
    tool_name       TEXT,
    idempotency_key TEXT,                                  -- 同 (session_id, key) 唯一
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    -- 预留字段：本变更不在写入时计算 content_hash。
    content_hash    TEXT
);

CREATE UNIQUE INDEX idx_observations_session_idem
    ON observations(session_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE INDEX idx_observations_session_created
    ON observations(session_id, created_at DESC, id DESC);

CREATE INDEX idx_sessions_created
    ON sessions(created_at DESC, id DESC);

CREATE TABLE summaries (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL REFERENCES sessions(id),
    content      TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    -- 预留字段：本变更不在写入时计算 content_hash。
    content_hash TEXT
);

CREATE INDEX idx_summaries_session_created
    ON summaries(session_id, created_at DESC, id DESC);

-- ── FTS5 全文索引（contentless 模式） ────────────────────────────

CREATE VIRTUAL TABLE observations_fts USING fts5(
    id UNINDEXED,
    session_id UNINDEXED,
    content,
    tool_name,
    content='observations',
    content_rowid='rowid'
);

CREATE VIRTUAL TABLE summaries_fts USING fts5(
    id UNINDEXED,
    session_id UNINDEXED,
    content,
    content='summaries',
    content_rowid='rowid'
);

-- ── FTS5 ↔ 基表同步触发器 ──────────────────────────────────────
-- 用 contentless 模式 + 显式 ai/ad/au 触发器，确保 BM25 排序与基表强一致。

-- observations ↔ observations_fts
CREATE TRIGGER observations_ai AFTER INSERT ON observations BEGIN
    INSERT INTO observations_fts(rowid, id, session_id, content, tool_name)
    VALUES (new.rowid, new.id, new.session_id, new.content, new.tool_name);
END;

CREATE TRIGGER observations_ad AFTER DELETE ON observations BEGIN
    INSERT INTO observations_fts(observations_fts, rowid, id, session_id, content, tool_name)
    VALUES ('delete', old.rowid, old.id, old.session_id, old.content, old.tool_name);
END;

CREATE TRIGGER observations_au AFTER UPDATE ON observations BEGIN
    INSERT INTO observations_fts(observations_fts, rowid, id, session_id, content, tool_name)
    VALUES ('delete', old.rowid, old.id, old.session_id, old.content, old.tool_name);
    INSERT INTO observations_fts(rowid, id, session_id, content, tool_name)
    VALUES (new.rowid, new.id, new.session_id, new.content, new.tool_name);
END;

-- summaries ↔ summaries_fts
CREATE TRIGGER summaries_ai AFTER INSERT ON summaries BEGIN
    INSERT INTO summaries_fts(rowid, id, session_id, content)
    VALUES (new.rowid, new.id, new.session_id, new.content);
END;

CREATE TRIGGER summaries_ad AFTER DELETE ON summaries BEGIN
    INSERT INTO summaries_fts(summaries_fts, rowid, id, session_id, content)
    VALUES ('delete', old.rowid, old.id, old.session_id, old.content);
END;

CREATE TRIGGER summaries_au AFTER UPDATE ON summaries BEGIN
    INSERT INTO summaries_fts(summaries_fts, rowid, id, session_id, content)
    VALUES ('delete', old.rowid, old.id, old.session_id, old.content);
    INSERT INTO summaries_fts(rowid, id, session_id, content)
    VALUES (new.rowid, new.id, new.session_id, new.content);
END;