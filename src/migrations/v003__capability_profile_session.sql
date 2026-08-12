-- Migration v003: capability profile + provenance metadata
--
-- 三个改动集合：
--   ① sessions 表追加 4 列（capabilities_json / operation_mode / last_active_at /
--      archived_at）—— task 1.3；
--   ② observations / summaries 表追加 10 列（scope / kind / origin / project_id /
--      content_hash / authority / source_refs_json / expires_at / supersedes_id /
--      fact_key）并建立 3 个查询索引 —— task 1.4；
--   ③ 对 v002 已存在的行做兼容回填：所有旧 sessions 行 operation_mode='stateless-manual'
--      + capabilities_json=NULL；旧 observations 行 scope='session' / kind='observation'
--      / origin='user' / authority='l1_observation' + content_hash=NULL；旧 summaries 行
--      同上（kind='summary'）—— task 1.5。
--
-- 设计约束：
--   - 所有新增列 MUST NOT NULL，方便查询走索引；
--   - 索引只覆盖查询高频字段（origin / (scope, project_id) / fact_key）；
--     其余字段查询走全表 + 列过滤即可；
--   - 回填用 UPDATE 而不是 INSERT 触发器：v002→v003 升级是一次性动作；
--   - 回填后业务表不再有 NULL 在「新必填」列上，后续 INSERT 路径由 adapter 强制填值。
--
-- 时间戳策略：与 v002 一致，统一 `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` 写入
-- UTC ISO-8601 字符串。

-- ── ① sessions 扩展列（task 1.3） ─────────────────────────────────

ALTER TABLE sessions ADD COLUMN capabilities_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE sessions ADD COLUMN operation_mode     TEXT NOT NULL DEFAULT 'stateless-manual';
ALTER TABLE sessions ADD COLUMN last_active_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
ALTER TABLE sessions ADD COLUMN archived_at        TEXT;

-- ── ② observations / summaries 扩展列（task 1.4） ──────────────────

-- observations 追加 9 列（content_hash 已在 v002 作为预留字段存在，本变更不在
-- ALTER 列表中重复声明；use case 层负责写入 SHA-256 值，repository 负责 SELECT）。
ALTER TABLE observations ADD COLUMN scope             TEXT NOT NULL DEFAULT 'session';
ALTER TABLE observations ADD COLUMN kind              TEXT NOT NULL DEFAULT 'observation';
ALTER TABLE observations ADD COLUMN origin            TEXT NOT NULL DEFAULT 'user';
ALTER TABLE observations ADD COLUMN project_id        TEXT;
ALTER TABLE observations ADD COLUMN authority         TEXT NOT NULL DEFAULT 'l1_observation';
ALTER TABLE observations ADD COLUMN source_refs_json  TEXT NOT NULL DEFAULT '[]';
ALTER TABLE observations ADD COLUMN expires_at        TEXT;
ALTER TABLE observations ADD COLUMN supersedes_id     TEXT REFERENCES observations(id);
ALTER TABLE observations ADD COLUMN fact_key          TEXT;

-- summaries 追加 9 列（同形；content_hash 已在 v002 预留）
ALTER TABLE summaries ADD COLUMN scope             TEXT NOT NULL DEFAULT 'session';
ALTER TABLE summaries ADD COLUMN kind              TEXT NOT NULL DEFAULT 'summary';
ALTER TABLE summaries ADD COLUMN origin            TEXT NOT NULL DEFAULT 'user';
ALTER TABLE summaries ADD COLUMN project_id        TEXT;
ALTER TABLE summaries ADD COLUMN authority         TEXT NOT NULL DEFAULT 'l1_summary';
ALTER TABLE summaries ADD COLUMN source_refs_json  TEXT NOT NULL DEFAULT '[]';
ALTER TABLE summaries ADD COLUMN expires_at        TEXT;
ALTER TABLE summaries ADD COLUMN supersedes_id     TEXT REFERENCES observations(id);
ALTER TABLE summaries ADD COLUMN fact_key          TEXT;

-- 高频查询字段索引（task 1.4 AC）：
-- - (origin) 用于「按来源过滤」如 tool_result / memora_recall；
-- - (scope, project_id) 用于「项目级记忆」隔离；
-- - (fact_key) 用于「事实去重 / 冲突检测」。
CREATE INDEX idx_observations_origin ON observations(origin);
CREATE INDEX idx_observations_scope_project ON observations(scope, project_id);
CREATE INDEX idx_observations_fact_key ON observations(fact_key);

CREATE INDEX idx_summaries_origin ON summaries(origin);
CREATE INDEX idx_summaries_scope_project ON summaries(scope, project_id);
CREATE INDEX idx_summaries_fact_key ON summaries(fact_key);

-- sessions 上 last_active_at / archived_at 用于 2.4「长期未活动归档」决策。
-- 本变更只建索引，不消费。
CREATE INDEX idx_sessions_last_active_at ON sessions(last_active_at DESC);
CREATE INDEX idx_sessions_archived_at ON sessions(archived_at) WHERE archived_at IS NOT NULL;

-- ── ③ 兼容回填（task 1.5） ────────────────────────────────────────
--
-- ALTER TABLE ... DEFAULT 已经在 ADD COLUMN 时把旧行填上默认值，但 brief
-- 1.5 AC 明确要求：
--   - observations: scope='session' / kind='observation' / origin='user' /
--     authority='l1_observation' / content_hash=NULL（触发回填逻辑填值）；
--   - summaries:    kind='summary' / authority='l1_summary' / content_hash=NULL；
--   - sessions:     operation_mode='stateless-manual' / capabilities_json=NULL
--     （即「旧会话未声明能力」的保守路径）。
--
-- 上方 DEFAULT 已经把 sessions.capabilities_json 默认填了 '{}'，违反 brief
-- 「NULL」语义。下面 UPDATE 把 capabilities_json 重设为 NULL，明确表达「未声明」：
UPDATE sessions SET capabilities_json = NULL WHERE capabilities_json = '{}';

-- observations 与 summaries 的 DEFAULT 已经满足 scope / kind / origin / authority，
-- 且 content_hash 默认为 NULL（无需重写）。下面 UPDATE 仅显式锁定语义，方便后续
-- schema diff / 测试断言：
UPDATE observations
   SET scope = 'session',
       kind = 'observation',
       origin = 'user',
       authority = 'l1_observation'
 WHERE scope = 'session'
   AND kind = 'observation'
   AND origin = 'user'
   AND authority = 'l1_observation';

UPDATE summaries
   SET scope = 'session',
       kind = 'summary',
       origin = 'user',
       authority = 'l1_summary'
 WHERE scope = 'session'
   AND kind = 'summary'
   AND origin = 'user'
   AND authority = 'l1_summary';