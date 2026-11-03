use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};

use super::models::{
    AccountRecord, AgentProfileRecord, QuotaSnapshotRecord, SessionOperationRecord,
    SessionRunBindingRecord, TuiSessionRecord, UsageReservationRecord,
};

#[derive(Clone)]
pub struct SqliteTuiStore {
    pool: SqlitePool,
}

impl SqliteTuiStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let options = if path == Path::new(":memory:") {
            SqliteConnectOptions::from_str("sqlite::memory:")?
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
        };
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        Self::init_tables(&pool).await?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn init_tables(pool: &SqlitePool) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tui_sessions (
                id TEXT PRIMARY KEY,
                session_name TEXT NOT NULL,
                agent_kind TEXT NOT NULL,
                cwd TEXT NOT NULL,
                model TEXT,
                effort TEXT,
                auto_approve INTEGER NOT NULL DEFAULT 1,
                proxy_mode TEXT NOT NULL DEFAULT 'inherit',
                proxy_id TEXT,
                effective_proxy TEXT,
                account_id TEXT,
                created_by_project_id TEXT,
                created_by_task_id TEXT,
                created_by_run_id TEXT,
                created_by_subtask_id TEXT,
                creation_source TEXT NOT NULL,
                owner_run_id TEXT,
                ownership_mode TEXT NOT NULL DEFAULT 'run_exclusive',
                generation INTEGER NOT NULL DEFAULT 1,
                lifecycle_state TEXT NOT NULL DEFAULT 'starting',
                process_state TEXT NOT NULL DEFAULT 'alive',
                tui_pane_state TEXT NOT NULL DEFAULT 'initializing',
                stop_intent TEXT,
                cleanup_error TEXT,
                created_at TEXT NOT NULL,
                last_operation_at TEXT,
                last_input_at TEXT,
                last_progress_at TEXT,
                last_observed_at TEXT,
                idle_since TEXT,
                owner_terminal_at TEXT,
                reclaim_after TEXT,
                stopped_at TEXT,
                reclaimed_at TEXT
            )",
        )
        .execute(pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tui_sess_name ON tui_sessions(session_name)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tui_sess_owner ON tui_sessions(owner_run_id)")
            .execute(pool)
            .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_tui_sess_proj ON tui_sessions(created_by_project_id)",
        )
        .execute(pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_tui_sess_state ON tui_sessions(lifecycle_state)",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tui_session_run_bindings (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                run_id TEXT NOT NULL,
                project_id TEXT,
                task_id TEXT,
                bound_at TEXT NOT NULL,
                unbound_at TEXT,
                reason TEXT
            )",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_tui_bind_sess ON tui_session_run_bindings(session_id)",
        )
        .execute(pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_tui_bind_run ON tui_session_run_bindings(run_id)",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tui_session_operations (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                session_name TEXT NOT NULL,
                operation_type TEXT NOT NULL,
                operator TEXT NOT NULL,
                project_id TEXT,
                task_id TEXT,
                run_id TEXT,
                subtask_id TEXT,
                summary TEXT,
                content_ref TEXT,
                status TEXT NOT NULL,
                error TEXT,
                duration_ms INTEGER,
                started_at TEXT NOT NULL,
                finished_at TEXT
            )",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_tui_ops_sess ON tui_session_operations(session_id)",
        )
        .execute(pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tui_ops_run ON tui_session_operations(run_id)")
            .execute(pool)
            .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_tui_ops_started ON tui_session_operations(started_at)",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tui_agent_profiles (
                id TEXT PRIMARY KEY,
                agent_kind TEXT NOT NULL UNIQUE,
                default_model TEXT,
                default_effort TEXT,
                account_id TEXT,
                proxy_mode TEXT NOT NULL DEFAULT 'inherit',
                proxy_id TEXT,
                max_concurrency INTEGER,
                idle_timeout_mins INTEGER,
                exited_retention_mins INTEGER,
                max_turn_timeout_secs INTEGER,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tui_accounts (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                account_name TEXT NOT NULL UNIQUE,
                auth_ref TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tui_quotas (
                id TEXT PRIMARY KEY,
                pool_id TEXT NOT NULL,
                account_id TEXT,
                agent_kind TEXT,
                model_pattern TEXT,
                quota_type TEXT NOT NULL,
                unit TEXT NOT NULL,
                used_amount REAL NOT NULL DEFAULT 0.0,
                limit_amount REAL,
                remaining_percent REAL,
                window_start TEXT,
                window_end TEXT,
                reset_at TEXT,
                source TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'valid',
                observed_at TEXT NOT NULL
            )",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tui_usage_reservations (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                account_id TEXT,
                run_id TEXT,
                amount REAL NOT NULL,
                unit TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'reserved',
                created_at TEXT NOT NULL,
                settled_at TEXT
            )",
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    fn row_to_session(row: SqliteRow) -> TuiSessionRecord {
        let auto_approve_int: i64 = row.get("auto_approve");
        let generation_int: i64 = row.get("generation");
        TuiSessionRecord {
            id: row.get("id"),
            session_name: row.get("session_name"),
            agent_kind: row.get("agent_kind"),
            cwd: row.get("cwd"),
            model: row.get("model"),
            effort: row.get("effort"),
            auto_approve: auto_approve_int != 0,
            proxy_mode: row.get("proxy_mode"),
            proxy_id: row.get("proxy_id"),
            effective_proxy: row.get("effective_proxy"),
            account_id: row.get("account_id"),
            created_by_project_id: row.get("created_by_project_id"),
            created_by_task_id: row.get("created_by_task_id"),
            created_by_run_id: row.get("created_by_run_id"),
            created_by_subtask_id: row.get("created_by_subtask_id"),
            creation_source: row.get("creation_source"),
            owner_run_id: row.get("owner_run_id"),
            ownership_mode: row.get("ownership_mode"),
            generation: generation_int as u64,
            lifecycle_state: row.get("lifecycle_state"),
            process_state: row.get("process_state"),
            tui_pane_state: row.get("tui_pane_state"),
            stop_intent: row.get("stop_intent"),
            cleanup_error: row.get("cleanup_error"),
            created_at: row.get("created_at"),
            last_operation_at: row.get("last_operation_at"),
            last_input_at: row.get("last_input_at"),
            last_progress_at: row.get("last_progress_at"),
            last_observed_at: row.get("last_observed_at"),
            idle_since: row.get("idle_since"),
            owner_terminal_at: row.get("owner_terminal_at"),
            reclaim_after: row.get("reclaim_after"),
            stopped_at: row.get("stopped_at"),
            reclaimed_at: row.get("reclaimed_at"),
        }
    }

    pub async fn insert_session(&self, rec: &TuiSessionRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO tui_sessions (
                id, session_name, agent_kind, cwd, model, effort, auto_approve,
                proxy_mode, proxy_id, effective_proxy, account_id,
                created_by_project_id, created_by_task_id, created_by_run_id, created_by_subtask_id,
                creation_source, owner_run_id, ownership_mode, generation,
                lifecycle_state, process_state, tui_pane_state, stop_intent, cleanup_error,
                created_at, last_operation_at, last_input_at, last_progress_at, last_observed_at,
                idle_since, owner_terminal_at, reclaim_after, stopped_at, reclaimed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&rec.id)
        .bind(&rec.session_name)
        .bind(&rec.agent_kind)
        .bind(&rec.cwd)
        .bind(&rec.model)
        .bind(&rec.effort)
        .bind(if rec.auto_approve { 1i64 } else { 0i64 })
        .bind(&rec.proxy_mode)
        .bind(&rec.proxy_id)
        .bind(&rec.effective_proxy)
        .bind(&rec.account_id)
        .bind(&rec.created_by_project_id)
        .bind(&rec.created_by_task_id)
        .bind(&rec.created_by_run_id)
        .bind(&rec.created_by_subtask_id)
        .bind(&rec.creation_source)
        .bind(&rec.owner_run_id)
        .bind(&rec.ownership_mode)
        .bind(rec.generation as i64)
        .bind(&rec.lifecycle_state)
        .bind(&rec.process_state)
        .bind(&rec.tui_pane_state)
        .bind(&rec.stop_intent)
        .bind(&rec.cleanup_error)
        .bind(&rec.created_at)
        .bind(&rec.last_operation_at)
        .bind(&rec.last_input_at)
        .bind(&rec.last_progress_at)
        .bind(&rec.last_observed_at)
        .bind(&rec.idle_since)
        .bind(&rec.owner_terminal_at)
        .bind(&rec.reclaim_after)
        .bind(&rec.stopped_at)
        .bind(&rec.reclaimed_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_session(&self, rec: &TuiSessionRecord) -> Result<()> {
        sqlx::query(
            "UPDATE tui_sessions SET
                session_name = ?, agent_kind = ?, cwd = ?, model = ?, effort = ?, auto_approve = ?,
                proxy_mode = ?, proxy_id = ?, effective_proxy = ?, account_id = ?,
                created_by_project_id = ?, created_by_task_id = ?, created_by_run_id = ?, created_by_subtask_id = ?,
                creation_source = ?, owner_run_id = ?, ownership_mode = ?, generation = ?,
                lifecycle_state = ?, process_state = ?, tui_pane_state = ?, stop_intent = ?, cleanup_error = ?,
                last_operation_at = ?, last_input_at = ?, last_progress_at = ?, last_observed_at = ?,
                idle_since = ?, owner_terminal_at = ?, reclaim_after = ?, stopped_at = ?, reclaimed_at = ?
            WHERE id = ?"
        )
        .bind(&rec.session_name)
        .bind(&rec.agent_kind)
        .bind(&rec.cwd)
        .bind(&rec.model)
        .bind(&rec.effort)
        .bind(if rec.auto_approve { 1i64 } else { 0i64 })
        .bind(&rec.proxy_mode)
        .bind(&rec.proxy_id)
        .bind(&rec.effective_proxy)
        .bind(&rec.account_id)
        .bind(&rec.created_by_project_id)
        .bind(&rec.created_by_task_id)
        .bind(&rec.created_by_run_id)
        .bind(&rec.created_by_subtask_id)
        .bind(&rec.creation_source)
        .bind(&rec.owner_run_id)
        .bind(&rec.ownership_mode)
        .bind(rec.generation as i64)
        .bind(&rec.lifecycle_state)
        .bind(&rec.process_state)
        .bind(&rec.tui_pane_state)
        .bind(&rec.stop_intent)
        .bind(&rec.cleanup_error)
        .bind(&rec.last_operation_at)
        .bind(&rec.last_input_at)
        .bind(&rec.last_progress_at)
        .bind(&rec.last_observed_at)
        .bind(&rec.idle_since)
        .bind(&rec.owner_terminal_at)
        .bind(&rec.reclaim_after)
        .bind(&rec.stopped_at)
        .bind(&rec.reclaimed_at)
        .bind(&rec.id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_session_by_id(&self, id: &str) -> Result<Option<TuiSessionRecord>> {
        let row = sqlx::query("SELECT * FROM tui_sessions WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Self::row_to_session))
    }

    pub async fn get_session_by_name(&self, name: &str) -> Result<Option<TuiSessionRecord>> {
        let row = sqlx::query(
            "SELECT * FROM tui_sessions WHERE session_name = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Self::row_to_session))
    }

    pub async fn list_sessions(
        &self,
        project_id: Option<&str>,
        active_only: bool,
        limit: usize,
    ) -> Result<Vec<TuiSessionRecord>> {
        let mut builder =
            sqlx::QueryBuilder::<sqlx::Sqlite>::new("SELECT * FROM tui_sessions WHERE 1=1");
        if let Some(pid) = project_id {
            builder.push(" AND created_by_project_id = ").push_bind(pid);
        }
        if active_only {
            builder.push(
                " AND lifecycle_state IN ('starting', 'active', 'retained', 'reclaim_pending')",
            );
        }
        builder
            .push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(limit as i64);

        let rows = builder.build().fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Self::row_to_session).collect())
    }

    pub async fn list_active_sessions_for_run(
        &self,
        run_id: &str,
    ) -> Result<Vec<TuiSessionRecord>> {
        let rows = sqlx::query(
            "SELECT * FROM tui_sessions WHERE owner_run_id = ? AND lifecycle_state IN ('starting', 'active', 'retained', 'reclaim_pending')"
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Self::row_to_session).collect())
    }

    pub async fn list_sessions_for_run(&self, run_id: &str) -> Result<Vec<TuiSessionRecord>> {
        let rows = sqlx::query(
            "SELECT DISTINCT s.* FROM tui_sessions s
             LEFT JOIN tui_session_run_bindings b ON s.id = b.session_id
             WHERE s.created_by_run_id = ? OR s.owner_run_id = ? OR b.run_id = ?
             ORDER BY s.created_at DESC",
        )
        .bind(run_id)
        .bind(run_id)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Self::row_to_session).collect())
    }

    pub async fn insert_operation(&self, op: &SessionOperationRecord) -> Result<()> {
        let dur = op.duration_ms.map(|d| d as i64);
        sqlx::query(
            "INSERT INTO tui_session_operations (
                id, session_id, session_name, operation_type, operator,
                project_id, task_id, run_id, subtask_id, summary, content_ref,
                status, error, duration_ms, started_at, finished_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&op.id)
        .bind(&op.session_id)
        .bind(&op.session_name)
        .bind(&op.operation_type)
        .bind(&op.operator)
        .bind(&op.project_id)
        .bind(&op.task_id)
        .bind(&op.run_id)
        .bind(&op.subtask_id)
        .bind(&op.summary)
        .bind(&op.content_ref)
        .bind(&op.status)
        .bind(&op.error)
        .bind(dur)
        .bind(&op.started_at)
        .bind(&op.finished_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn complete_operation(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
        duration_ms: u64,
        finished_at: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE tui_session_operations SET status = ?, error = ?, duration_ms = ?, finished_at = ? WHERE id = ?"
        )
        .bind(status)
        .bind(error)
        .bind(duration_ms as i64)
        .bind(finished_at)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_operations(
        &self,
        session_id: Option<&str>,
        run_id: Option<&str>,
        project_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SessionOperationRecord>> {
        let mut builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT * FROM tui_session_operations WHERE 1=1",
        );
        if let Some(sid) = session_id {
            builder
                .push(" AND (session_id = ")
                .push_bind(sid)
                .push(" OR session_name = ")
                .push_bind(sid)
                .push(")");
        }
        if let Some(rid) = run_id {
            builder.push(" AND run_id = ").push_bind(rid);
        }
        if let Some(pid) = project_id {
            builder.push(" AND project_id = ").push_bind(pid);
        }
        builder
            .push(" ORDER BY started_at DESC LIMIT ")
            .push_bind(limit as i64)
            .push(" OFFSET ")
            .push_bind(offset as i64);

        let rows = builder.build().fetch_all(&self.pool).await?;
        let ops = rows
            .into_iter()
            .map(|row| {
                let dur: Option<i64> = row.get("duration_ms");
                SessionOperationRecord {
                    id: row.get("id"),
                    session_id: row.get("session_id"),
                    session_name: row.get("session_name"),
                    operation_type: row.get("operation_type"),
                    operator: row.get("operator"),
                    project_id: row.get("project_id"),
                    task_id: row.get("task_id"),
                    run_id: row.get("run_id"),
                    subtask_id: row.get("subtask_id"),
                    summary: row.get("summary"),
                    content_ref: row.get("content_ref"),
                    status: row.get("status"),
                    error: row.get("error"),
                    duration_ms: dur.map(|d| d as u64),
                    started_at: row.get("started_at"),
                    finished_at: row.get("finished_at"),
                }
            })
            .collect();
        Ok(ops)
    }

    pub async fn insert_binding(&self, b: &SessionRunBindingRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO tui_session_run_bindings (id, session_id, run_id, project_id, task_id, bound_at, unbound_at, reason)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&b.id)
        .bind(&b.session_id)
        .bind(&b.run_id)
        .bind(&b.project_id)
        .bind(&b.task_id)
        .bind(&b.bound_at)
        .bind(&b.unbound_at)
        .bind(&b.reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_bindings_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionRunBindingRecord>> {
        let rows = sqlx::query(
            "SELECT * FROM tui_session_run_bindings WHERE session_id = ? ORDER BY bound_at ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        let bindings = rows
            .into_iter()
            .map(|row| SessionRunBindingRecord {
                id: row.get("id"),
                session_id: row.get("session_id"),
                run_id: row.get("run_id"),
                project_id: row.get("project_id"),
                task_id: row.get("task_id"),
                bound_at: row.get("bound_at"),
                unbound_at: row.get("unbound_at"),
                reason: row.get("reason"),
            })
            .collect();
        Ok(bindings)
    }

    pub async fn list_profiles(&self) -> Result<Vec<AgentProfileRecord>> {
        let rows = sqlx::query("SELECT * FROM tui_agent_profiles ORDER BY agent_kind ASC")
            .fetch_all(&self.pool)
            .await?;

        let list = rows
            .into_iter()
            .map(|row| {
                let max_c: Option<i64> = row.get("max_concurrency");
                let idle_t: Option<i64> = row.get("idle_timeout_mins");
                let ret_t: Option<i64> = row.get("exited_retention_mins");
                let turn_t: Option<i64> = row.get("max_turn_timeout_secs");
                AgentProfileRecord {
                    id: row.get("id"),
                    agent_kind: row.get("agent_kind"),
                    default_model: row.get("default_model"),
                    default_effort: row.get("default_effort"),
                    account_id: row.get("account_id"),
                    proxy_mode: row.get("proxy_mode"),
                    proxy_id: row.get("proxy_id"),
                    max_concurrency: max_c.map(|v| v as u32),
                    idle_timeout_mins: idle_t.map(|v| v as u64),
                    exited_retention_mins: ret_t.map(|v| v as u64),
                    max_turn_timeout_secs: turn_t.map(|v| v as u64),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                }
            })
            .collect();
        Ok(list)
    }

    pub async fn get_profile_by_agent(
        &self,
        agent_kind: &str,
    ) -> Result<Option<AgentProfileRecord>> {
        let row = sqlx::query("SELECT * FROM tui_agent_profiles WHERE agent_kind = ?")
            .bind(agent_kind)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| {
            let max_c: Option<i64> = r.get("max_concurrency");
            let idle_t: Option<i64> = r.get("idle_timeout_mins");
            let ret_t: Option<i64> = r.get("exited_retention_mins");
            let turn_t: Option<i64> = r.get("max_turn_timeout_secs");
            AgentProfileRecord {
                id: r.get("id"),
                agent_kind: r.get("agent_kind"),
                default_model: r.get("default_model"),
                default_effort: r.get("default_effort"),
                account_id: r.get("account_id"),
                proxy_mode: r.get("proxy_mode"),
                proxy_id: r.get("proxy_id"),
                max_concurrency: max_c.map(|v| v as u32),
                idle_timeout_mins: idle_t.map(|v| v as u64),
                exited_retention_mins: ret_t.map(|v| v as u64),
                max_turn_timeout_secs: turn_t.map(|v| v as u64),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            }
        }))
    }

    pub async fn upsert_profile(&self, p: &AgentProfileRecord) -> Result<()> {
        let max_c = p.max_concurrency.map(|v| v as i64);
        let idle_t = p.idle_timeout_mins.map(|v| v as i64);
        let ret_t = p.exited_retention_mins.map(|v| v as i64);
        let turn_t = p.max_turn_timeout_secs.map(|v| v as i64);

        sqlx::query(
            "INSERT INTO tui_agent_profiles (
                id, agent_kind, default_model, default_effort, account_id, proxy_mode, proxy_id,
                max_concurrency, idle_timeout_mins, exited_retention_mins, max_turn_timeout_secs,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(agent_kind) DO UPDATE SET
                default_model = excluded.default_model,
                default_effort = excluded.default_effort,
                account_id = excluded.account_id,
                proxy_mode = excluded.proxy_mode,
                proxy_id = excluded.proxy_id,
                max_concurrency = excluded.max_concurrency,
                idle_timeout_mins = excluded.idle_timeout_mins,
                exited_retention_mins = excluded.exited_retention_mins,
                max_turn_timeout_secs = excluded.max_turn_timeout_secs,
                updated_at = excluded.updated_at",
        )
        .bind(&p.id)
        .bind(&p.agent_kind)
        .bind(&p.default_model)
        .bind(&p.default_effort)
        .bind(&p.account_id)
        .bind(&p.proxy_mode)
        .bind(&p.proxy_id)
        .bind(max_c)
        .bind(idle_t)
        .bind(ret_t)
        .bind(turn_t)
        .bind(&p.created_at)
        .bind(&p.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_accounts(&self) -> Result<Vec<AccountRecord>> {
        let rows = sqlx::query("SELECT * FROM tui_accounts ORDER BY account_name ASC")
            .fetch_all(&self.pool)
            .await?;

        let list = rows
            .into_iter()
            .map(|r| AccountRecord {
                id: r.get("id"),
                provider: r.get("provider"),
                account_name: r.get("account_name"),
                auth_ref: r.get("auth_ref"),
                status: r.get("status"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect();
        Ok(list)
    }

    pub async fn upsert_account(&self, acc: &AccountRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO tui_accounts (id, provider, account_name, auth_ref, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_name) DO UPDATE SET
                provider = excluded.provider,
                auth_ref = excluded.auth_ref,
                status = excluded.status,
                updated_at = excluded.updated_at"
        )
        .bind(&acc.id)
        .bind(&acc.provider)
        .bind(&acc.account_name)
        .bind(&acc.auth_ref)
        .bind(&acc.status)
        .bind(&acc.created_at)
        .bind(&acc.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_quotas(&self, account_id: Option<&str>) -> Result<Vec<QuotaSnapshotRecord>> {
        let mut builder =
            sqlx::QueryBuilder::<sqlx::Sqlite>::new("SELECT * FROM tui_quotas WHERE 1=1");
        if let Some(acc) = account_id {
            builder.push(" AND account_id = ").push_bind(acc);
        }
        builder.push(" ORDER BY observed_at DESC");

        let rows = builder.build().fetch_all(&self.pool).await?;
        let quotas = rows
            .into_iter()
            .map(|r| QuotaSnapshotRecord {
                id: r.get("id"),
                pool_id: r.get("pool_id"),
                account_id: r.get("account_id"),
                agent_kind: r.get("agent_kind"),
                model_pattern: r.get("model_pattern"),
                quota_type: r.get("quota_type"),
                unit: r.get("unit"),
                used_amount: r.get("used_amount"),
                limit_amount: r.get("limit_amount"),
                remaining_percent: r.get("remaining_percent"),
                window_start: r.get("window_start"),
                window_end: r.get("window_end"),
                reset_at: r.get("reset_at"),
                source: r.get("source"),
                status: r.get("status"),
                observed_at: r.get("observed_at"),
            })
            .collect();
        Ok(quotas)
    }

    pub async fn upsert_quota(&self, q: &QuotaSnapshotRecord) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO tui_quotas (
                id, pool_id, account_id, agent_kind, model_pattern, quota_type, unit,
                used_amount, limit_amount, remaining_percent, window_start, window_end,
                reset_at, source, status, observed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&q.id)
        .bind(&q.pool_id)
        .bind(&q.account_id)
        .bind(&q.agent_kind)
        .bind(&q.model_pattern)
        .bind(&q.quota_type)
        .bind(&q.unit)
        .bind(q.used_amount)
        .bind(q.limit_amount)
        .bind(q.remaining_percent)
        .bind(&q.window_start)
        .bind(&q.window_end)
        .bind(&q.reset_at)
        .bind(&q.source)
        .bind(&q.status)
        .bind(&q.observed_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_reservation(&self, res: &UsageReservationRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO tui_usage_reservations (id, session_id, account_id, run_id, amount, unit, status, created_at, settled_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&res.id)
        .bind(&res.session_id)
        .bind(&res.account_id)
        .bind(&res.run_id)
        .bind(res.amount)
        .bind(&res.unit)
        .bind(&res.status)
        .bind(&res.created_at)
        .bind(&res.settled_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn settle_reservation(&self, id: &str, settled_at: &str) -> Result<()> {
        sqlx::query(
            "UPDATE tui_usage_reservations SET status = 'settled', settled_at = ? WHERE id = ?",
        )
        .bind(settled_at)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn release_reservation(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE tui_usage_reservations SET status = 'released' WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
