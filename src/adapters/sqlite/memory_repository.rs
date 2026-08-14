//! SQLite adapter —— L1 业务表的 `MemoryRepository` 实现。
//!
//! 关键约束：
//! - 不在 domain / application 持有 rusqlite 句柄；所有 SQL 都在 adapter 内部。
//! - 每个 port 方法在 `spawn_blocking` 边界上打开独立短生命周期连接
//!   （design D5：避免 connection / statement / lock 跨 `.await` 持有）。
//! - 所有 SQL 走 prepared statement，禁止字符串拼接。
//! - 错误一律经 `MemoryError::Storage` 包装，绝不向上抛绝对路径或原始内容。

use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::application::ids::uuid_v4;
use crate::application::ports::{
    MemoryRepository, ObserveInput, SearchInput, SearchKind, SessionEndInput, SessionStartInput,
};
use crate::application::MemoryError;
use crate::domain::{
    Observation, ObservationId, OperationMode, SearchHit, Session, SessionId, SummaryId,
};

/// 启动期实例化的 repository。运行期每个调用通过 `db_path` 在
/// `spawn_blocking` 边界上打开短生命周期连接。
pub struct SqliteMemoryRepository {
    db_path: PathBuf,
}

impl SqliteMemoryRepository {
    /// 启动期调用：仅记录数据库路径，不打开连接。
    pub fn bootstrap(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    /// 在当前线程上打开并配置连接（含 FK 与 busy timeout）。
    fn open(&self) -> Result<Connection, MemoryError> {
        // SQLite 启动配置（FK + busy timeout）已在 `open_and_migrate` 完成；
        // 这里复用相同配置，确保任何线程上打开的连接都有一致 pragma。
        let conn =
            Connection::open(&self.db_path).map_err(|e| MemoryError::Storage(Box::new(e)))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;
        conn.busy_timeout(std::time::Duration::from_millis(5_000))
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;
        Ok(conn)
    }
}

impl MemoryRepository for SqliteMemoryRepository {
    fn start_session(&self, input: SessionStartInput) -> Result<Session, MemoryError> {
        let conn = self.open()?;
        let id = uuid_v4();
        let created_at = now_utc();
        // v003：capability profile 1.3 落地 —— INSERT 写齐 v003 新增 4 列 +
        // v002 预留 3 列。`operation_mode` wire 字符串由 use case 算好后透传。
        conn.execute(
            "INSERT INTO sessions (\
                id, name, created_at, \
                agent_id, project_id, external_session_ref, \
                capabilities_json, operation_mode, last_active_at, archived_at\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?3, ?9)",
            params![
                id,
                input.name,
                created_at,
                input.agent_id,
                input.project_id,
                input.external_session_ref,
                input.capabilities_json,
                input.operation_mode.as_wire_str(),
                None::<Option<String>>, // archived_at = NULL on create
            ],
        )
        .map_err(|e| MemoryError::Storage(Box::new(e)))?;
        let session = self
            .read_session(&conn, &SessionId(id.clone()))
            .ok_or_else(|| {
                MemoryError::Storage(Box::new(std::io::Error::other(
                    "freshly inserted session row is missing",
                )))
            })?;
        Ok(session)
    }

    fn end_session(&self, input: SessionEndInput) -> Result<Session, MemoryError> {
        let mut conn = self.open()?;
        let ended_at = now_utc();

        // 先在事务里 SELECT 既有 session 行（验证存在性 + 拿到 created_at 等）。
        let tx = conn
            .transaction()
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;

        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT id, created_at FROM sessions WHERE id = ?1",
                params![input.session_id.0],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;

        let Some((_, _created_at)) = existing else {
            // 未提交事务由 drop 自动回滚。
            return Err(MemoryError::SessionNotFound(input.session_id.0));
        };

        // 1. 更新 sessions.ended_at 与 sessions.summary（幂等）。
        tx.execute(
            "UPDATE sessions SET ended_at = ?1, last_active_at = ?1, summary = COALESCE(?2, summary) WHERE id = ?3",
            params![ended_at, input.summary, input.session_id.0],
        )
        .map_err(|e| MemoryError::Storage(Box::new(e)))?;

        // 2. 若提供 summary，写入 summaries 行（幂等：相同 session 仅保留一行，
        //    用 DELETE + INSERT 保证 latest 内容）。
        if let Some(summary_content) = input.summary.as_deref() {
            // 删除既有 summary 行，保证一对一（spec "summaries 手动写入边界"）。
            tx.execute(
                "DELETE FROM summaries WHERE session_id = ?1",
                params![input.session_id.0],
            )
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;
            let summary_id = uuid_v4();
            // v003：summary INSERT 写齐 10 列（content_hash + 9 个 provenance 列）。
            tx.execute(
                "INSERT INTO summaries (\
                    id, session_id, content, created_at, \
                    content_hash, scope, kind, origin, project_id, \
                    authority, source_refs_json, expires_at, supersedes_id, fact_key\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    summary_id,
                    input.session_id.0,
                    summary_content,
                    ended_at,
                    input.summary_content_hash,
                    input.summary_scope,
                    input.summary_kind,
                    input.summary_origin,
                    None::<Option<String>>, // summary project_id 暂无入参
                    input.summary_authority,
                    input.summary_source_refs_json,
                    None::<Option<String>>, // summary expires_at 暂无入参
                    None::<Option<String>>, // summary supersedes_id 暂无入参
                    None::<Option<String>>, // summary fact_key 暂无入参
                ],
            )
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;
        }

        tx.commit().map_err(|e| MemoryError::Storage(Box::new(e)))?;

        // 重新读取最新行返回。
        self.read_session(&conn, &input.session_id)
            .ok_or(MemoryError::SessionNotFound(input.session_id.0.clone()))
    }

    fn observe(&self, input: ObserveInput) -> Result<Observation, MemoryError> {
        let mut conn = self.open()?;
        // 先获得 write reservation，避免两个并发请求同时 SELECT miss 后竞争 INSERT。
        // 后到的请求会等首个事务提交，再读到首次写入的 observation。
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;

        // 0. 验证 session 存在。
        let session_exists: bool = tx
            .query_row(
                "SELECT 1 FROM sessions WHERE id = ?1",
                params![input.session_id.0],
                |_row| Ok(()),
            )
            .optional()
            .map_err(|e| MemoryError::Storage(Box::new(e)))?
            .is_some();
        if !session_exists {
            return Err(MemoryError::SessionNotFound(input.session_id.0));
        }

        // 1. 若提供 idempotency_key，先按 (session_id, key) 查既有行。
        if let Some(key) = input.idempotency_key.as_deref() {
            let existing: Option<Observation> = tx
                .query_row(
                    "SELECT id, session_id, content, tool_name, created_at, \
                            content_hash, scope, kind, origin, project_id, \
                            authority, source_refs_json, expires_at, supersedes_id, fact_key \
                     FROM observations \
                     WHERE session_id = ?1 AND idempotency_key = ?2",
                    params![input.session_id.0, key],
                    map_observation,
                )
                .optional()
                .map_err(|e| MemoryError::Storage(Box::new(e)))?;

            if let Some(obs) = existing {
                // 命中既有行，事务回滚（无 INSERT），直接返回。
                drop(tx);
                return Ok(obs);
            }
        }

        // 2. INSERT 新行（v003：写齐全部 14 列）。
        let id = uuid_v4();
        let created_at = now_utc();
        tx.execute(
            "INSERT INTO observations (\
                id, session_id, content, tool_name, idempotency_key, created_at, \
                content_hash, scope, kind, origin, project_id, \
                authority, source_refs_json, expires_at, supersedes_id, fact_key\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                id,
                input.session_id.0,
                input.content,
                input.tool_name,
                input.idempotency_key,
                created_at,
                input.content_hash,
                input.scope,
                input.kind,
                input.origin,
                input.project_id,
                input.authority,
                input.source_refs_json,
                input.expires_at,
                input.supersedes_id,
                input.fact_key,
            ],
        )
        .map_err(|e| MemoryError::Storage(Box::new(e)))?;

        tx.commit().map_err(|e| MemoryError::Storage(Box::new(e)))?;

        Ok(Observation {
            id: ObservationId(id),
            session_id: input.session_id,
            content: input.content,
            tool_name: input.tool_name,
            created_at,
            content_hash: input.content_hash,
            scope: input.scope,
            kind: input.kind,
            origin: input.origin,
            project_id: input.project_id,
            authority: input.authority,
            source_refs_json: input.source_refs_json,
            expires_at: input.expires_at,
            supersedes_id: input.supersedes_id,
            fact_key: input.fact_key,
        })
    }

    fn recent_observations(
        &self,
        session_id: Option<&SessionId>,
        limit: u32,
    ) -> Result<Vec<Observation>, MemoryError> {
        let conn = self.open()?;
        let limit_i64 = i64::from(limit);
        // v003：SELECT 读齐全部 15 列（content_hash + 9 个 provenance 列）。
        let select_sql = "SELECT id, session_id, content, tool_name, created_at, \
                                 content_hash, scope, kind, origin, project_id, \
                                 authority, source_refs_json, expires_at, supersedes_id, fact_key \
                          FROM observations";
        let rows = if let Some(sid) = session_id {
            let mut stmt = conn
                .prepare(&format!(
                    "{select_sql} WHERE session_id = ?1 \
                     ORDER BY created_at DESC, id DESC \
                     LIMIT ?2"
                ))
                .map_err(|e| MemoryError::Storage(Box::new(e)))?;
            let iter = stmt
                .query_map(params![sid.0, limit_i64], map_observation)
                .map_err(|e| MemoryError::Storage(Box::new(e)))?;
            iter.collect::<Result<Vec<_>, _>>()
                .map_err(|e| MemoryError::Storage(Box::new(e)))?
        } else {
            let mut stmt = conn
                .prepare(&format!(
                    "{select_sql} \
                     ORDER BY created_at DESC, id DESC \
                     LIMIT ?1"
                ))
                .map_err(|e| MemoryError::Storage(Box::new(e)))?;
            let iter = stmt
                .query_map(params![limit_i64], map_observation)
                .map_err(|e| MemoryError::Storage(Box::new(e)))?;
            iter.collect::<Result<Vec<_>, _>>()
                .map_err(|e| MemoryError::Storage(Box::new(e)))?
        };
        Ok(rows)
    }

    fn recent_sessions(&self, limit: u32) -> Result<Vec<Session>, MemoryError> {
        let conn = self.open()?;
        let limit_i64 = i64::from(limit);
        let mut stmt = conn
            .prepare(
                "SELECT id, name, created_at, ended_at, summary, \
                        agent_id, project_id, external_session_ref, \
                        capabilities_json, operation_mode, last_active_at, archived_at \
                 FROM sessions \
                 ORDER BY created_at DESC, id DESC \
                 LIMIT ?1",
            )
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;
        let iter = stmt
            .query_map(params![limit_i64], map_session)
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;
        iter.collect::<Result<Vec<_>, _>>()
            .map_err(|e| MemoryError::Storage(Box::new(e)))
    }

    fn search(&self, input: SearchInput) -> Result<Vec<SearchHit>, MemoryError> {
        let conn = self.open()?;
        let limit_i64 = i64::from(input.limit);
        let session_filter = input.session_id.as_ref().map(|s| s.0.clone());
        let query = input.query.clone();

        let mut hits = Vec::new();

        if matches!(input.kind, SearchKind::Observation | SearchKind::Both) {
            hits.extend(run_fts_query(
                &conn,
                "observations_fts",
                "observations",
                &query,
                session_filter.as_deref(),
                limit_i64,
                |id, session_id, content, tool_name, created_at, score| SearchHit {
                    kind: "observation",
                    id,
                    session_id: SessionId(session_id),
                    content,
                    tool_name,
                    created_at,
                    score,
                },
            )?);
        }

        if matches!(input.kind, SearchKind::Summary | SearchKind::Both) {
            hits.extend(run_fts_query(
                &conn,
                "summaries_fts",
                "summaries",
                &query,
                session_filter.as_deref(),
                limit_i64,
                |id, session_id, content, _tool_name_unused, created_at, score| SearchHit {
                    kind: "summary",
                    id,
                    session_id: SessionId(session_id),
                    content,
                    tool_name: None,
                    created_at,
                    score,
                },
            )?);
        }

        // 跨 kind 时按 BM25 升序统一排序：score 越小越相关。
        hits.sort_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // 最终截断到 limit（避免 observation + summary 各自返回 limit 条时超量）。
        hits.truncate(input.limit as usize);

        Ok(hits)
    }

    fn find_session(&self, session_id: &SessionId) -> Result<Option<Session>, MemoryError> {
        let conn = self.open()?;
        Ok(self.read_session(&conn, session_id))
    }

    fn find_by_session_idempotency_and_hash(
        &self,
        session_id: &SessionId,
        idempotency_key: &str,
        content_hash: &str,
    ) -> Result<Option<ObservationId>, MemoryError> {
        let conn = self.open()?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM observations \
                 WHERE session_id = ?1 AND idempotency_key = ?2 AND content_hash = ?3 \
                 LIMIT 1",
                params![session_id.0, idempotency_key, content_hash],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;
        Ok(existing.map(ObservationId))
    }

    fn find_active_session_by_project_and_ref(
        &self,
        project_id: Option<&str>,
        external_session_ref: Option<&str>,
    ) -> Result<Option<Session>, MemoryError> {
        let conn = self.open()?;
        let sid: Option<String> = conn
            .query_row(
                "SELECT id FROM sessions \
                 WHERE project_id = ?1 AND external_session_ref = ?2 AND archived_at IS NULL \
                 LIMIT 1",
                params![project_id, external_session_ref],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;
        match sid {
            Some(id) => Ok(self.read_session(&conn, &SessionId(id))),
            None => Ok(None),
        }
    }

    fn archive_session(
        &self,
        session_id: &SessionId,
        archived_at: &str,
    ) -> Result<(), MemoryError> {
        let conn = self.open()?;
        conn.execute(
            "UPDATE sessions SET archived_at = ?1 WHERE id = ?2",
            params![archived_at, session_id.0],
        )
        .map_err(|e| MemoryError::Storage(Box::new(e)))?;
        Ok(())
    }
}

impl SqliteMemoryRepository {
    fn read_session(&self, conn: &Connection, sid: &SessionId) -> Option<Session> {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, created_at, ended_at, summary, \
                        agent_id, project_id, external_session_ref, \
                        capabilities_json, operation_mode, last_active_at, archived_at \
                 FROM sessions WHERE id = ?1",
            )
            .ok()?;
        stmt.query_row(params![sid.0], map_session).ok()
    }
}

/// sessions 表 12 列 → Session 值对象的统一解析器。
///
/// 列顺序与 `start_session` / `recent_sessions` / `read_session` 三处 SELECT 一致；
/// 新增列必须按相同顺序追加到所有 SELECT 站点。
fn map_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    let mode_str: String = row.get(9)?;
    let operation_mode = match mode_str.as_str() {
        "native-opaque" => OperationMode::NativeOpaque,
        "stateless-hooked" => OperationMode::StatelessHooked,
        "stateless-manual" => OperationMode::StatelessManual,
        // 数据库里若出现非契约值，落到保守路径而不是 panic —— 1.3 允许未来
        // wire 字符串扩展，旧 binary 不会因为读到新字符串直接崩。
        _ => OperationMode::StatelessManual,
    };
    Ok(Session {
        id: SessionId(row.get::<_, String>(0)?),
        name: row.get::<_, String>(1)?,
        created_at: row.get::<_, String>(2)?,
        ended_at: row.get::<_, Option<String>>(3)?,
        summary: row.get::<_, Option<String>>(4)?,
        agent_id: row.get::<_, Option<String>>(5)?,
        project_id: row.get::<_, Option<String>>(6)?,
        external_session_ref: row.get::<_, Option<String>>(7)?,
        capabilities_json: row.get::<_, Option<String>>(8)?,
        operation_mode,
        last_active_at: row.get::<_, String>(10)?,
        archived_at: row.get::<_, Option<String>>(11)?,
    })
}

/// observations 表 15 列 → Observation 值对象的统一解析器。
///
/// 列顺序与 `observe` / `recent_observations` 的 SELECT 一致；
/// 新增列必须按相同顺序追加到所有 SELECT 站点。
fn map_observation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Observation> {
    Ok(Observation {
        id: ObservationId(row.get::<_, String>(0)?),
        session_id: SessionId(row.get::<_, String>(1)?),
        content: row.get::<_, String>(2)?,
        tool_name: row.get::<_, Option<String>>(3)?,
        created_at: row.get::<_, String>(4)?,
        content_hash: row.get::<_, Option<String>>(5)?,
        scope: row.get::<_, Option<String>>(6)?,
        kind: row.get::<_, Option<String>>(7)?,
        origin: row.get::<_, Option<String>>(8)?,
        project_id: row.get::<_, Option<String>>(9)?,
        authority: row.get::<_, Option<String>>(10)?,
        source_refs_json: row.get::<_, Option<String>>(11)?,
        expires_at: row.get::<_, Option<String>>(12)?,
        supersedes_id: row.get::<_, Option<String>>(13)?,
        fact_key: row.get::<_, Option<String>>(14)?,
    })
}

/// 在指定 FTS5 虚拟表上跑 BM25 检索，回写基表拿完整字段。
///
/// `hit_builder` 负责从基表字段 + score 构造 SearchHit，把 "summary 没有
/// tool_name 字段" 这类 kind 差异吸收在闭包里，避免调用方重复分支。
fn run_fts_query<F>(
    conn: &Connection,
    fts_table: &str,
    base_table: &str,
    query: &str,
    session_filter: Option<&str>,
    limit: i64,
    hit_builder: F,
) -> Result<Vec<SearchHit>, MemoryError>
where
    F: Fn(String, String, String, Option<String>, String, f64) -> SearchHit,
{
    // FTS5 语法把未引号包裹的 `foo-bar` 解析为 column:term —— `-` 是列分隔符，
    // 把 `bar` 当成 column 名查询并报 "no such column"。把整个 query 包成
    // FTS5 phrase（双引号包裹，内部双引号 escape 为两个双引号），让查询按
    // 词序列匹配，规避 syntax 注入。
    let fts_query = format!("\"{}\"", query.replace('"', "\"\""));

    let fts_sql = format!(
        "SELECT {fts_table}.rowid, bm25({fts_table}) AS score \
         FROM {fts_table} \
         JOIN {base_table} ON {base_table}.rowid = {fts_table}.rowid \
         WHERE {fts_table} MATCH ?1 \
           AND (?2 IS NULL OR {base_table}.session_id = ?2) \
         ORDER BY score ASC \
         LIMIT ?3",
    );

    let mut stmt = conn
        .prepare(&fts_sql)
        .map_err(|e| MemoryError::Storage(Box::new(e)))?;
    let fts_rows: Vec<(i64, f64)> = stmt
        .query_map(params![fts_query, session_filter, limit], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
        })
        .map_err(|e| MemoryError::Storage(Box::new(e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| MemoryError::Storage(Box::new(e)))?;

    let mut hits = Vec::with_capacity(fts_rows.len());
    // 两种表的列集不同（observations 含 tool_name，summaries 无），按表分流 SQL
    // 以避免向 summaries 查询 tool_name 列导致 SQL 错误。
    let base_sql = if base_table == "observations" {
        "SELECT id, session_id, content, tool_name, created_at FROM observations WHERE rowid = ?1"
            .to_string()
    } else {
        "SELECT id, session_id, content, NULL AS tool_name, created_at FROM summaries WHERE rowid = ?1".to_string()
    };
    let mut base_stmt = conn
        .prepare(&base_sql)
        .map_err(|e| MemoryError::Storage(Box::new(e)))?;

    for (rowid, score) in fts_rows {
        let (id, session_id, content, tool_name, created_at) = base_stmt
            .query_row(params![rowid], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| MemoryError::Storage(Box::new(e)))?;

        hits.push(hit_builder(
            id, session_id, content, tool_name, created_at, score,
        ));
    }

    Ok(hits)
}

/// 与 schema_migrations.applied_at / sessions.created_at 完全相同的 UTC 时间
/// 表达式（毫秒级 ISO-8601）。统一 SQLite 内置 `strftime` 形态，避免时区漂移。
fn now_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let secs = (nanos / 1_000_000_000) as u64;
    let millis = ((nanos % 1_000_000_000) / 1_000_000) as u32;
    let (year, month, day, hour, min, sec) = unix_secs_to_ymdhms(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}.{millis:03}Z")
}

/// Unix 秒 → (year, month, day, hour, minute, second)。自实现，避免引入 chrono。
fn unix_secs_to_ymdhms(mut secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let sec = (secs % 60) as u32;
    secs /= 60;
    let min = (secs % 60) as u32;
    secs /= 60;
    let hour = (secs % 24) as u32;
    let mut days = (secs / 24) as i64;

    // 1970-01-01 是 Thursday；算法来自 Howard Hinnant `civil_from_days`。
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = (y + (m <= 2) as i64) as i32;

    (y, m, d, hour, min, sec)
}

#[allow(dead_code)]
fn _silence_summaryid(_: SummaryId) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::OperationMode;

    fn repo(dir: &tempfile::TempDir) -> SqliteMemoryRepository {
        let db = dir.path().join("memora.db");
        // 先用 open_and_migrate 建好 schema（含 v2 迁移）。
        crate::adapters::sqlite::open_and_migrate(&db).expect("migrate");
        SqliteMemoryRepository::bootstrap(db)
    }

    /// 测试 helper：构造一个 `operation_mode = stateless-manual` / 不带 capability 声明的
    /// `SessionStartInput`。覆盖 v003 新增的 4 列写入默认保守值。
    fn minimal_session_start(name: &str) -> SessionStartInput {
        SessionStartInput {
            name: name.to_string(),
            agent_id: None,
            project_id: None,
            external_session_ref: None,
            client_capabilities: None,
            operation_mode: OperationMode::StatelessManual,
            capabilities_json: None,
        }
    }

    /// 测试 helper：构造一个**已经过 use case 填默认值**的 `ObserveInput`。
    /// v003 起 schema 上 `scope / kind / origin / authority / source_refs_json`
    /// 是 NOT NULL DEFAULT，adapter 直接传 None 会触发 NOT NULL constraint。
    /// helper 模拟 use case 已经填默认的状态（与 `observe::execute` 一致），
    /// 让 L1 测试聚焦"路径覆盖 / 顺序 / 幂等"而不重复 use case 责任。
    /// `content_hash` 也由 use case 计算，helper 留 None（adapter 写 NULL）。
    fn minimal_observe(
        session_id: SessionId,
        content: String,
        tool_name: Option<String>,
        idempotency_key: Option<String>,
    ) -> ObserveInput {
        ObserveInput {
            session_id,
            content,
            tool_name,
            idempotency_key,
            content_hash: None,
            origin: Some("user".to_string()),
            project_id: None,
            fact_key: None,
            scope: Some("session".to_string()),
            kind: Some("observation".to_string()),
            authority: Some("l1_observation".to_string()),
            source_refs: None,
            source_refs_json: Some("[]".to_string()),
            expires_at: None,
            supersedes_id: None,
        }
    }

    #[test]
    fn start_session_returns_row_with_unique_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let s = r
            .start_session(minimal_session_start("first"))
            .expect("start");
        assert_eq!(s.name, "first");
        assert!(!s.id.0.is_empty());
        assert!(s.ended_at.is_none());
        assert!(s.summary.is_none());
        // created_at 形如 2026-08-11T...Z。
        assert!(s.created_at.ends_with('Z'));
    }

    #[test]
    fn end_session_marks_ended_at_and_writes_summary_row() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let s = r.start_session(minimal_session_start("s")).expect("start");
        let ended = r
            .end_session(SessionEndInput {
                session_id: s.id.clone(),
                summary: Some("wrap-up".to_string()),
                // v003 落地：summary_* 字段由 use case 填默认值与 SHA-256。
                // L1 测试绕开 use case 直调 adapter，自己填上对应默认值，
                // 与 `end_session::execute` 行为一致。
                summary_content_hash: Some(
                    "77543befcf98a9283a45bcf8a13896aec795f99dcaa9c721c263b6f5fb7f4c3f".to_string(),
                ),
                summary_kind: Some("summary".to_string()),
                summary_authority: Some("l1_summary".to_string()),
                summary_origin: Some("user".to_string()),
                summary_scope: Some("session".to_string()),
                summary_source_refs_json: Some("[]".to_string()),
            })
            .expect("end");
        assert!(ended.ended_at.is_some(), "ended_at must be set");
        assert_eq!(ended.summary.as_deref(), Some("wrap-up"));

        // summaries 表必须有一行。
        let conn = rusqlite::Connection::open(dir.path().join("memora.db")).expect("reopen");
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM summaries WHERE session_id = ?1",
                rusqlite::params![s.id.0],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(n, 1);
    }

    #[test]
    fn end_session_without_summary_keeps_summaries_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let s = r.start_session(minimal_session_start("s")).expect("start");
        r.end_session(SessionEndInput {
            session_id: s.id.clone(),
            summary: None,
            summary_content_hash: None,
            summary_kind: None,
            summary_authority: None,
            summary_origin: None,
            summary_scope: None,
            summary_source_refs_json: None,
        })
        .expect("end");
        let conn = rusqlite::Connection::open(dir.path().join("memora.db")).expect("reopen");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM summaries", [], |row| row.get(0))
            .expect("count");
        assert_eq!(n, 0);
    }

    #[test]
    fn end_session_unknown_id_returns_session_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let err = r
            .end_session(SessionEndInput {
                session_id: SessionId("nonexistent".to_string()),
                summary: None,
                summary_content_hash: None,
                summary_kind: None,
                summary_authority: None,
                summary_origin: None,
                summary_scope: None,
                summary_source_refs_json: None,
            })
            .expect_err("not found");
        assert!(matches!(err, MemoryError::SessionNotFound(_)));
    }

    #[test]
    fn observe_writes_row_and_returns_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let s = r.start_session(minimal_session_start("s")).expect("start");
        let obs = r
            .observe(minimal_observe(
                s.id.clone(),
                "first".to_string(),
                Some("Bash".to_string()),
                Some("k1".to_string()),
            ))
            .expect("observe");
        assert_eq!(obs.content, "first");
        assert_eq!(obs.tool_name.as_deref(), Some("Bash"));
        assert!(!obs.id.0.is_empty());
    }

    #[test]
    fn observe_with_idempotency_key_returns_existing_row_on_repeat() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let s = r.start_session(minimal_session_start("s")).expect("start");
        let a = r
            .observe(minimal_observe(
                s.id.clone(),
                "first".to_string(),
                None,
                Some("dup".to_string()),
            ))
            .expect("first");
        let b = r
            .observe(minimal_observe(
                s.id.clone(),
                "DIFFERENT".to_string(),
                None,
                Some("dup".to_string()),
            ))
            .expect("second");
        assert_eq!(a.id, b.id, "idempotency: same id on repeat");
        assert_eq!(a.created_at, b.created_at, "created_at must not change");
        assert_eq!(b.content, "first", "first-written content preserved");

        let conn = rusqlite::Connection::open(dir.path().join("memora.db")).expect("reopen");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM observations", [], |row| row.get(0))
            .expect("count");
        assert_eq!(n, 1, "no new row written");
    }

    #[test]
    fn concurrent_observe_with_same_key_returns_first_row() {
        use std::sync::{Arc, Barrier};

        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("memora.db");
        let r = repo(&dir);
        let session = r
            .start_session(minimal_session_start("concurrent"))
            .expect("start");
        let barrier = Arc::new(Barrier::new(2));

        let handles: Vec<_> = ["first", "second"]
            .into_iter()
            .map(|content| {
                let repository = SqliteMemoryRepository::bootstrap(db.clone());
                let session_id = session.id.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    repository.observe(minimal_observe(
                        session_id.clone(),
                        content.to_string(),
                        None,
                        Some("shared-key".to_string()),
                    ))
                })
            })
            .collect();

        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .expect("thread must not panic")
                    .expect("observe")
            })
            .collect();
        assert_eq!(results[0].id, results[1].id);
        assert_eq!(results[0].created_at, results[1].created_at);

        let conn = rusqlite::Connection::open(db).expect("reopen");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM observations", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn observe_without_idempotency_key_always_writes_new_row() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let s = r.start_session(minimal_session_start("s")).expect("start");
        r.observe(minimal_observe(s.id.clone(), "one".to_string(), None, None))
            .expect("one");
        r.observe(minimal_observe(s.id.clone(), "two".to_string(), None, None))
            .expect("two");
        let conn = rusqlite::Connection::open(dir.path().join("memora.db")).expect("reopen");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM observations", [], |row| row.get(0))
            .expect("count");
        assert_eq!(n, 2);
    }

    #[test]
    fn observe_unknown_session_returns_session_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let err = r
            .observe(minimal_observe(
                SessionId("nope".to_string()),
                "x".to_string(),
                None,
                None,
            ))
            .expect_err("not found");
        assert!(matches!(err, MemoryError::SessionNotFound(_)));
    }

    #[test]
    fn recent_observations_filters_and_orders_by_created_desc() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let s = r.start_session(minimal_session_start("s")).expect("start");
        for i in 0..5 {
            r.observe(minimal_observe(
                s.id.clone(),
                format!("item-{i}"),
                Some("Read".to_string()),
                None,
            ))
            .expect("observe");
        }
        // 限定 session 取 3 条：应按 created_at DESC 倒序。
        let top3 = r.recent_observations(Some(&s.id), 3).expect("recent");
        assert_eq!(top3.len(), 3);
        assert_eq!(top3[0].content, "item-4");
        assert_eq!(top3[1].content, "item-3");
        assert_eq!(top3[2].content, "item-2");
    }

    #[test]
    fn recent_sessions_orders_by_created_desc() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        for i in 0..3 {
            r.start_session(minimal_session_start(&format!("s-{i}")))
                .expect("start");
        }
        let recent = r.recent_sessions(2).expect("recent");
        assert_eq!(recent.len(), 2);
        // 最近的两条应该是 s-2 与 s-1（按 created_at DESC）。
        assert_eq!(recent[0].name, "s-2");
        assert_eq!(recent[1].name, "s-1");
    }

    #[test]
    fn search_finds_observation_by_keyword() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let s = r.start_session(minimal_session_start("s")).expect("start");
        r.observe(minimal_observe(
            s.id.clone(),
            "数据库迁移失败".to_string(),
            None,
            None,
        ))
        .expect("observe");
        r.observe(minimal_observe(
            s.id.clone(),
            "memory database health check".to_string(),
            None,
            None,
        ))
        .expect("observe");
        // 用 ASCII 关键词（默认 unicode61 分词器对 CJK 较差不指望）；先用 "database"。
        let hits = r
            .search(SearchInput {
                query: "database".to_string(),
                session_id: None,
                kind: SearchKind::Observation,
                limit: 10,
            })
            .expect("search");
        assert!(!hits.is_empty(), "at least one hit");
        assert_eq!(hits[0].kind, "observation");
    }

    #[test]
    fn search_kind_summary_only_returns_summaries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let s = r.start_session(minimal_session_start("s")).expect("start");
        r.end_session(SessionEndInput {
            session_id: s.id.clone(),
            summary: Some("completed task alpha".to_string()),
            // v003：summary_* 字段由 use case 填默认值与 SHA-256。
            summary_content_hash: Some(
                "6a2b799fb45b16d8fbaf3b31b96a1280c6b58d45840f379376715cfbde87ccf9".to_string(),
            ),
            summary_kind: Some("summary".to_string()),
            summary_authority: Some("l1_summary".to_string()),
            summary_origin: Some("user".to_string()),
            summary_scope: Some("session".to_string()),
            summary_source_refs_json: Some("[]".to_string()),
        })
        .expect("end");
        r.observe(minimal_observe(
            s.id.clone(),
            "alpha observation".to_string(),
            None,
            None,
        ))
        .expect("observe");
        let hits = r
            .search(SearchInput {
                query: "alpha".to_string(),
                session_id: None,
                kind: SearchKind::Summary,
                limit: 10,
            })
            .expect("search");
        assert!(!hits.is_empty());
        for h in &hits {
            assert_eq!(h.kind, "summary");
            assert!(h.tool_name.is_none());
        }
    }

    #[test]
    fn search_applies_session_filter_before_limit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = repo(&dir);
        let target = r
            .start_session(minimal_session_start("target"))
            .expect("target session");
        let other = r
            .start_session(minimal_session_start("other"))
            .expect("other session");

        r.observe(minimal_observe(
            target.id.clone(),
            "needle target".to_string(),
            None,
            None,
        ))
        .expect("target observation");
        for index in 0..5 {
            r.observe(minimal_observe(
                other.id.clone(),
                format!("needle needle needle other-{index}"),
                None,
                None,
            ))
            .expect("other observation");
        }

        let hits = r
            .search(SearchInput {
                query: "needle".to_string(),
                session_id: Some(target.id.clone()),
                kind: SearchKind::Observation,
                limit: 1,
            })
            .expect("filtered search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, target.id);
    }
}
