use std::path::Path;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::scheduler::{JobRunStore, JobRunTerminal, JobRunTrigger};

#[derive(Clone)]
pub struct SqliteJobRunStore {
    pool: SqlitePool,
}

impl SqliteJobRunStore {
    pub async fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
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
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                disabled INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS job_runs (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                project_id TEXT,
                session_id TEXT,
                trigger TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                error TEXT
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_job_runs_project ON job_runs(project_id, started_at DESC)")
            .execute(&pool)
            .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_job_runs_job ON job_runs(job_id, started_at DESC)",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn runs_for_job(&self, job_id: &str, limit: i64) -> anyhow::Result<Vec<JobRun>> {
        Ok(sqlx::query_as::<_, JobRun>(
            "SELECT id, job_id, project_id, session_id, trigger, status, started_at, finished_at, error
             FROM job_runs WHERE job_id = ? ORDER BY started_at DESC LIMIT ?",
        )
        .bind(job_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn runs_for_project(
        &self,
        project_id: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<JobRun>> {
        Ok(sqlx::query_as::<_, JobRun>(
            "SELECT id, job_id, project_id, session_id, trigger, status, started_at, finished_at, error
             FROM job_runs WHERE project_id = ? ORDER BY started_at DESC LIMIT ?",
        )
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_run(&self, run_id: &str) -> anyhow::Result<Option<JobRun>> {
        Ok(sqlx::query_as::<_, JobRun>(
            "SELECT id, job_id, project_id, session_id, trigger, status, started_at, finished_at, error
             FROM job_runs WHERE id = ?",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?)
    }
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRun {
    pub id: String,
    pub job_id: String,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub trigger: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error: Option<String>,
}

#[async_trait]
impl JobRunStore for SqliteJobRunStore {
    async fn insert_running(
        &self,
        run_id: &str,
        job_id: &str,
        project_id: Option<&str>,
        session_id: &str,
        trigger: JobRunTrigger,
        started_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO job_runs (id, job_id, project_id, session_id, trigger, status, started_at)
             VALUES (?, ?, ?, ?, ?, 'running', ?)",
        )
        .bind(run_id)
        .bind(job_id)
        .bind(project_id)
        .bind(session_id)
        .bind(trigger.to_string())
        .bind(started_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn finish(
        &self,
        run_id: &str,
        status: JobRunTerminal,
        finished_at: DateTime<Utc>,
        error: Option<String>,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE job_runs SET status = ?, finished_at = ?, error = ? WHERE id = ?")
            .bind(status.to_string())
            .bind(finished_at.to_rfc3339())
            .bind(error)
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

pub fn new_run_id() -> String {
    Uuid::now_v7().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_missing_database_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("control.db");
        let store = SqliteJobRunStore::open(&path).await.unwrap();
        store
            .insert_running(
                "run",
                "job",
                None,
                "session",
                JobRunTrigger::Manual,
                Utc::now(),
            )
            .await
            .unwrap();
        assert!(path.is_file());
    }

    #[tokio::test]
    async fn stores_and_finishes_job_runs() {
        let store = SqliteJobRunStore::open(":memory:").await.unwrap();
        store
            .insert_running(
                "run",
                "job",
                Some("project"),
                "session",
                JobRunTrigger::Manual,
                Utc::now(),
            )
            .await
            .unwrap();
        store
            .finish("run", JobRunTerminal::Succeeded, Utc::now(), None)
            .await
            .unwrap();
        let rows = store.runs_for_job("job", 10).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "succeeded");
    }
}
