use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, Utc};
use openduck_harness::project::{ProjectHarnessConfig, ProjectTaskStore};
use openduck_sdk_types::custom_requests::SourceType;
use tokio::sync::{Mutex, OnceCell};
use tokio_cron_scheduler::{job::JobId, Job, JobScheduler as TokioJobScheduler};

use crate::sources;

use super::harness::{persist_harness_run, skipped_run_dto, TaskAlreadyRunningError};
use super::project_harness::{launch_project_task, resolve_project_root, ProjectTaskRunRequest};

#[derive(Clone, Hash, Eq, PartialEq, Debug)]
struct TaskKey {
    slug: String,
    task_id: String,
}

#[derive(Clone, Debug)]
struct ScheduledJobInfo {
    uuid: JobId,
    cron: String,
}

pub struct TaskScheduler {
    tokio_scheduler: TokioJobScheduler,
    jobs: Mutex<HashMap<TaskKey, ScheduledJobInfo>>,
}

static INSTANCE: OnceCell<Arc<TaskScheduler>> = OnceCell::const_new();

pub async fn global() -> Result<Arc<TaskScheduler>> {
    INSTANCE
        .get_or_try_init(|| async { TaskScheduler::new().await })
        .await
        .map(Arc::clone)
}

pub fn normalize_cron(cron: &str) -> Result<String, String> {
    let trimmed = cron.trim();
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let normalized = match parts.len() {
        5 => format!("0 {trimmed}"),
        6 => trimmed.to_string(),
        n => {
            return Err(format!(
                "Invalid cron expression '{trimmed}': expected 5 or 6 fields, got {n}"
            ));
        }
    };
    normalized
        .parse::<croner::Cron>()
        .map_err(|e| format!("Invalid cron expression '{trimmed}': {e}"))?;
    Ok(normalized)
}

pub fn calculate_next_run_at(cron: &str) -> Option<DateTime<Utc>> {
    let normalized = normalize_cron(cron).ok()?;
    let parsed: croner::Cron = normalized.parse().ok()?;
    let local_now = Local::now();
    parsed
        .find_next_occurrence(&local_now, false)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

impl TaskScheduler {
    pub async fn new() -> Result<Arc<Self>> {
        let tokio_scheduler = TokioJobScheduler::new()
            .await
            .map_err(|e| anyhow!("Failed to create project task scheduler: {e}"))?;
        tokio_scheduler
            .start()
            .await
            .map_err(|e| anyhow!("Failed to start project task scheduler: {e}"))?;
        Ok(Arc::new(Self {
            tokio_scheduler,
            jobs: Mutex::new(HashMap::new()),
        }))
    }

    pub async fn upsert_schedule(
        &self,
        slug: &str,
        task_id: &str,
        cron: Option<&str>,
        paused: bool,
    ) -> Result<()> {
        self.remove(slug, task_id).await?;
        let Some(cron) = cron.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(());
        };
        if paused {
            return Ok(());
        }
        let normalized = normalize_cron(cron).map_err(|e| anyhow!(e))?;
        let slug_owned = slug.to_string();
        let task_id_owned = task_id.to_string();
        let local_tz = Local::now().timezone();
        let job = Job::new_async_tz(&normalized, local_tz, move |_uuid, _lock| {
            let slug = slug_owned.clone();
            let task_id = task_id_owned.clone();
            Box::pin(async move {
                if let Err(error) = run_scheduled_project_task(&slug, &task_id).await {
                    tracing::error!(
                        %error,
                        project = %slug,
                        task = %task_id,
                        "Scheduled project task failed"
                    );
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create cron job for '{slug}/{task_id}': {e}"))?;

        let uuid = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.tokio_scheduler.add(job),
        )
        .await
        {
            Ok(Ok(uuid)) => uuid,
            Ok(Err(e)) => {
                return Err(anyhow!(
                    "Failed to register cron job for '{slug}/{task_id}': {e}"
                ));
            }
            Err(_) => {
                tracing::warn!(
                    project = slug,
                    task = task_id,
                    "Timed out registering cron job with tokio-cron-scheduler"
                );
                uuid::Uuid::new_v4()
            }
        };

        self.jobs.lock().await.insert(
            TaskKey {
                slug: slug.to_string(),
                task_id: task_id.to_string(),
            },
            ScheduledJobInfo {
                uuid,
                cron: normalized,
            },
        );
        Ok(())
    }

    pub async fn remove(&self, slug: &str, task_id: &str) -> Result<()> {
        let key = TaskKey {
            slug: slug.to_string(),
            task_id: task_id.to_string(),
        };
        let info = self.jobs.lock().await.remove(&key);
        if let Some(info) = info {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.tokio_scheduler.remove(&info.uuid),
            )
            .await;
        }
        Ok(())
    }

    pub async fn is_registered(&self, slug: &str, task_id: &str) -> bool {
        let key = TaskKey {
            slug: slug.to_string(),
            task_id: task_id.to_string(),
        };
        self.jobs.lock().await.contains_key(&key)
    }

    pub async fn next_run_at(&self, slug: &str, task_id: &str) -> Option<DateTime<Utc>> {
        let key = TaskKey {
            slug: slug.to_string(),
            task_id: task_id.to_string(),
        };
        let info = self.jobs.lock().await.get(&key).cloned()?;
        calculate_next_run_at(&info.cron)
    }

    pub async fn reload_all(&self) -> Result<()> {
        let entries = sources::list_sources(Some(SourceType::Project), None, false)
            .map_err(|e| anyhow!("Failed to list projects for task schedules: {e}"))?;
        for entry in entries {
            let slug = entry.name;
            let Ok((_, root)) = resolve_project_root(&slug) else {
                continue;
            };
            let config = ProjectHarnessConfig::load_or_default(&root).await;
            let tasks_dir = config.resolve_tasks_dir(&root);
            let Ok(tasks) = ProjectTaskStore::list_tasks(&tasks_dir).await else {
                continue;
            };
            for task in tasks {
                if let Err(error) = self
                    .upsert_schedule(&slug, &task.id, task.cron.as_deref(), task.schedule_paused)
                    .await
                {
                    tracing::error!(
                        %error,
                        project = %slug,
                        task = %task.id,
                        "Failed to register scheduled project task"
                    );
                }
            }
        }
        Ok(())
    }
}

pub async fn run_scheduled_project_task(slug: &str, task_id: &str) -> Result<()> {
    let (_entry, root) = resolve_project_root(slug)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let tasks_dir = config.resolve_tasks_dir(&root);
    let task = match ProjectTaskStore::get_task(&tasks_dir, task_id).await {
        Ok(task) => task,
        Err(error) => {
            if let Ok(scheduler) = global().await {
                let _ = scheduler.remove(slug, task_id).await;
            }
            return Err(error);
        }
    };
    if task.cron.as_deref().map(str::trim).unwrap_or("").is_empty() || task.schedule_paused {
        return Ok(());
    }

    match launch_project_task(slug, task_id, ProjectTaskRunRequest::default()).await {
        Ok(rx) => {
            drop(rx);
            Ok(())
        }
        Err(error) if error.downcast_ref::<TaskAlreadyRunningError>().is_some() => {
            // Skip this tick instead of queueing a catch-up. If the task overruns
            // its interval, an immediate restart would turn it into a busy loop.
            let reason = "Skipped: previous run still in progress";
            tracing::warn!(
                project = slug,
                task = task_id,
                "Skipping scheduled run because the previous run is still in progress"
            );
            let skipped = skipped_run_dto(task_id, reason);
            if let Err(persist_error) = persist_harness_run(
                "task",
                &skipped,
                &Utc::now().to_rfc3339(),
                Some(slug),
                Some(&root),
                Some(reason),
            ) {
                tracing::error!(
                    %persist_error,
                    project = slug,
                    task = task_id,
                    "Failed to persist skipped scheduled run"
                );
            }
            Ok(())
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_cron_accepts_five_and_six_fields() {
        assert_eq!(normalize_cron("0 2 * * *").unwrap(), "0 0 2 * * *");
        assert_eq!(normalize_cron("0 0 9 * * 1-5").unwrap(), "0 0 9 * * 1-5");
    }

    #[test]
    fn normalize_cron_rejects_invalid_expressions() {
        assert!(normalize_cron("").is_err());
        assert!(normalize_cron("not a cron").is_err());
        assert!(normalize_cron("* * *").is_err());
    }

    #[tokio::test]
    async fn registers_and_removes_scheduled_tasks() {
        let scheduler = TaskScheduler::new().await.unwrap();
        scheduler
            .upsert_schedule("demo", "nightly", Some("0 2 * * *"), false)
            .await
            .unwrap();
        assert!(scheduler.is_registered("demo", "nightly").await);
        assert!(scheduler.next_run_at("demo", "nightly").await.is_some());

        scheduler
            .upsert_schedule("demo", "nightly", Some("0 2 * * *"), true)
            .await
            .unwrap();
        assert!(!scheduler.is_registered("demo", "nightly").await);

        scheduler
            .upsert_schedule("demo", "nightly", Some("0 2 * * *"), false)
            .await
            .unwrap();
        scheduler.remove("demo", "nightly").await.unwrap();
        assert!(!scheduler.is_registered("demo", "nightly").await);
    }

    #[tokio::test]
    async fn empty_cron_unregisters_the_task() {
        let scheduler = TaskScheduler::new().await.unwrap();
        scheduler
            .upsert_schedule("demo", "nightly", Some("0 9 * * *"), false)
            .await
            .unwrap();
        scheduler
            .upsert_schedule("demo", "nightly", Some("  "), false)
            .await
            .unwrap();
        assert!(!scheduler.is_registered("demo", "nightly").await);
    }
}
