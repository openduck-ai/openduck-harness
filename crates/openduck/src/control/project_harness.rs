use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, RwLock};

use anyhow::{anyhow, Context, Result};
use axum::{
    extract::Path as AxumPath,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use chrono::Utc;
use openduck_harness::eval::{
    is_continuable_status, ContinuationCheckpoint, EvalRunner, EvalRunnerConfig, EvaluationReport,
    TaskSpec,
};
use openduck_harness::policy::adapters::EchoPolicy;
use openduck_harness::project::{
    compose_dynamic_run_prompt, ProjectHarnessConfig, ProjectTaskDefinition, ProjectTaskStore,
    ProjectTaskSummary,
};
use openduck_harness::replay::{Cassette, ReplayPolicy};
use openduck_harness::runtime::AgentHarness;
use openduck_harness::sandbox::local::LocalSandbox;
use openduck_harness::sandbox::SandboxDriver;
use openduck_sdk_types::custom_requests::SourceEntry;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;

use crate::control::harness::{
    active_jobs_for_project, apply_history_task_names, broadcast_job_live_event,
    broadcast_project_event, clean_stale_task_scratchpad, failed_run_dto, get_active_job,
    get_live_snapshot, inspect_active_job, internal_list_cassettes, internal_list_reports,
    list_history_for_project, new_harness_job_id, persist_harness_run, project_runs_dir,
    read_history_run, register_abort_handle, register_active_job, remove_active_job,
    resolve_harness_policy, stop_active_job, subscribe_job_live_events, subscribe_project_events,
    to_run_dto, try_register_active_job, upsert_live_snapshot, with_live_snapshot,
    with_task_notes_plan, HarnessActiveJobDto, HarnessCassetteSummaryDto, HarnessHistorySummaryDto,
    HarnessJobInspectDto, HarnessJobLiveEvent, HarnessProjectEvent, HarnessReportSummaryDto,
    HarnessRunResponseDto, TaskAlreadyRunningError,
};
use crate::sources;

fn bad<T: std::fmt::Display>(e: T) -> (StatusCode, Json<serde_json::Value>) {
    let message = format!("{e:#}");
    tracing::error!(error = %message, "Project harness request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": message })),
    )
}

fn not_found<T: std::fmt::Display>(e: T) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
}

fn conflict(body: serde_json::Value) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::CONFLICT, Json(body))
}

fn launch_error(e: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    if let Some(err) = e.downcast_ref::<TaskAlreadyRunningError>() {
        return conflict(serde_json::json!({
            "error": err.to_string(),
            "code": "task_already_running",
            "taskId": err.task_id,
            "jobId": err.job_id,
        }));
    }
    if e.to_string().contains("not found") {
        return not_found(e);
    }
    bad(e)
}

static PROJECT_ROOT_CACHE: LazyLock<RwLock<HashMap<String, PathBuf>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn invalidate_project_root_cache(slug: &str) {
    if let Ok(mut lock) = PROJECT_ROOT_CACHE.write() {
        lock.remove(slug);
    }
}

pub fn resolve_project_root(slug: &str) -> Result<(SourceEntry, PathBuf)> {
    let entry = sources::read_project(slug)?;
    if let Ok(cache) = PROJECT_ROOT_CACHE.read() {
        if let Some(cached) = cache.get(slug) {
            return Ok((entry, cached.clone()));
        }
    }

    let dirs: Vec<String> = entry
        .properties
        .get("workingDirs")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| anyhow!("Failed to parse workingDirs: {e}"))?
        .unwrap_or_default();

    let root_str = dirs
        .first()
        .ok_or_else(|| anyhow!("Project '{}' has no working directory configured", slug))?;

    let root = PathBuf::from(root_str);
    if let Ok(mut cache) = PROJECT_ROOT_CACHE.write() {
        cache.insert(slug.to_string(), root.clone());
    }
    Ok((entry, root))
}

// ---------------- DTOs ----------------

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTaskRunRequest {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub max_turns: Option<usize>,
    pub echo: Option<bool>,
    pub record: Option<bool>,
    pub record_path: Option<String>,
    pub continue_from_run_id: Option<String>,
    pub extra_turns: Option<usize>,
    /// One-run instructions appended to the saved prompt. Requires `dynamicPrompt` on the task.
    #[serde(default)]
    pub extra_prompt: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTaskSchedulePatch {
    pub cron: Option<String>,
    pub paused: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHarnessEvalRequest {
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub task_ids: Option<Vec<String>>,
    pub concurrency: Option<usize>,
    pub max_turns: Option<usize>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub echo: Option<bool>,
    pub output_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTaskEstimateRequest {
    pub prompt: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(
        default,
        alias = "user_feedback",
        alias = "comment",
        alias = "user_comment",
        alias = "correction"
    )]
    pub feedback: Option<String>,
    #[serde(default, alias = "previous_estimate")]
    pub previous_estimate: Option<ProjectTaskEstimateResponse>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTaskEstimateResponse {
    pub suggested_max_turns: usize,
    pub suggested_timeout_seconds: u64,
    pub complexity: String,
    pub reasoning: String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHarnessOverviewDto {
    pub config: ProjectHarnessConfig,
    pub tasks: Vec<ProjectTaskSummary>,
    pub reports: Vec<HarnessReportSummaryDto>,
    pub cassettes: Vec<HarnessCassetteSummaryDto>,
    pub active_jobs: Vec<HarnessActiveJobDto>,
    pub history: Vec<HarnessHistorySummaryDto>,
}

fn prepare_task_schedule(
    task: &mut ProjectTaskDefinition,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if let Some(cron) = task.cron.take() {
        let trimmed = cron.trim();
        if trimmed.is_empty() {
            task.cron = None;
            task.schedule_paused = false;
        } else {
            super::task_scheduler::normalize_cron(trimmed).map_err(bad)?;
            task.cron = Some(trimmed.to_string());
        }
    }
    Ok(())
}

async fn sync_task_schedule(slug: &str, task: &ProjectTaskDefinition) {
    match super::task_scheduler::global().await {
        Ok(scheduler) => {
            if let Err(error) = scheduler
                .upsert_schedule(slug, &task.id, task.cron.as_deref(), task.schedule_paused)
                .await
            {
                tracing::error!(
                    %error,
                    project = slug,
                    task = %task.id,
                    "Failed to sync project task schedule"
                );
            }
        }
        Err(error) => {
            tracing::error!(%error, "Project task scheduler is unavailable");
        }
    }
}

async fn unschedule_task(slug: &str, task_id: &str) {
    match super::task_scheduler::global().await {
        Ok(scheduler) => {
            if let Err(error) = scheduler.remove(slug, task_id).await {
                tracing::error!(
                    %error,
                    project = slug,
                    task = task_id,
                    "Failed to remove project task schedule"
                );
            }
        }
        Err(error) => {
            tracing::error!(%error, "Project task scheduler is unavailable");
        }
    }
}

fn enrich_task_summaries(slug: &str, tasks: &mut [ProjectTaskSummary]) {
    let running: std::collections::HashSet<String> = active_jobs_for_project(slug)
        .into_iter()
        .filter(|job| job.job_type == "task")
        .map(|job| job.task_id)
        .collect();
    for task in tasks.iter_mut() {
        task.currently_running = running.contains(&task.id);
        if let Some(cron) = task
            .cron
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        {
            if !task.schedule_paused {
                task.next_run_at =
                    super::task_scheduler::calculate_next_run_at(cron).map(|ts| ts.to_rfc3339());
            }
        }
    }
}

// ---------------- REST HANDLERS ----------------

pub async fn get_project_harness_overview(
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<ProjectHarnessOverviewDto>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;

    let tasks_dir = config.resolve_tasks_dir(&root);
    let results_dir = config.resolve_results_dir(&root);
    let cassettes_dir = config.resolve_cassettes_dir(&root);
    let root_clone = root.clone();
    let slug_for_tasks = slug.clone();
    let slug_for_sync = slug.clone();

    let tasks_fut = async {
        let mut tasks = ProjectTaskStore::list_tasks(&tasks_dir).await?;
        enrich_task_summaries(&slug_for_tasks, &mut tasks);
        Ok::<_, anyhow::Error>(tasks)
    };

    let sync_fut = tokio::task::spawn_blocking(move || {
        let reports = internal_list_reports(&results_dir).unwrap_or_default();
        let cassettes = internal_list_cassettes(&cassettes_dir).unwrap_or_default();
        let history = list_history_for_project(&slug_for_sync, Some(&root_clone));
        (reports, cassettes, history)
    });

    let (tasks_res, sync_res) = tokio::join!(tasks_fut, sync_fut);
    let tasks = tasks_res.map_err(bad)?;
    let (reports, cassettes, history) =
        sync_res.map_err(|e| bad(format!("Failed to read project artifacts: {e}")))?;
    let active_jobs = active_jobs_for_project(&slug);
    let names: HashMap<String, String> = tasks
        .iter()
        .map(|task| (task.id.clone(), task.name.clone()))
        .collect();
    let history = apply_history_task_names(history, &names);

    Ok(Json(ProjectHarnessOverviewDto {
        config,
        tasks,
        reports,
        cassettes,
        active_jobs,
        history,
    }))
}

pub async fn get_project_harness_config(
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<ProjectHarnessConfig>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        ProjectHarnessConfig::load_or_default(&root),
    )
    .await
    .map_err(|_| bad("Timed out loading harness config"))?;
    Ok(Json(config))
}

pub async fn update_project_harness_config(
    AxumPath(slug): AxumPath<String>,
    Json(config): Json<ProjectHarnessConfig>,
) -> Result<Json<ProjectHarnessConfig>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    config.save_to_file(&root).await.map_err(bad)?;
    Ok(Json(config))
}

pub async fn list_project_tasks(
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<Vec<ProjectTaskSummary>>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        ProjectHarnessConfig::load_or_default(&root),
    )
    .await
    .map_err(|_| bad("Timed out loading harness config"))?;
    let tasks_dir = config.resolve_tasks_dir(&root);
    let mut tasks = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        ProjectTaskStore::list_tasks(&tasks_dir),
    )
    .await
    .map_err(|_| bad("Timed out listing project tasks"))?
    .map_err(bad)?;
    enrich_task_summaries(&slug, &mut tasks);
    Ok(Json(tasks))
}

pub async fn create_project_task(
    AxumPath(slug): AxumPath<String>,
    Json(mut task_def): Json<ProjectTaskDefinition>,
) -> Result<(StatusCode, Json<ProjectTaskDefinition>), (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let tasks_dir = config.resolve_tasks_dir(&root);

    if task_def.id.trim().is_empty() {
        return Err(bad("Task ID cannot be empty"));
    }
    prepare_task_schedule(&mut task_def)?;

    ProjectTaskStore::save_task(&tasks_dir, &task_def)
        .await
        .map_err(bad)?;
    let slug_clone = slug.clone();
    let task_def_clone = task_def.clone();
    tokio::spawn(async move {
        sync_task_schedule(&slug_clone, &task_def_clone).await;
    });
    broadcast_project_event(HarnessProjectEvent::TaskUpserted {
        project_slug: slug,
        task_id: task_def.id.clone(),
    });

    Ok((StatusCode::CREATED, Json(task_def)))
}

pub async fn get_project_task(
    AxumPath((slug, task_id)): AxumPath<(String, String)>,
) -> Result<Json<ProjectTaskDefinition>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let tasks_dir = config.resolve_tasks_dir(&root);

    let task_def = ProjectTaskStore::get_task(&tasks_dir, &task_id)
        .await
        .map_err(not_found)?;

    Ok(Json(task_def))
}

pub async fn update_project_task(
    AxumPath((slug, task_id)): AxumPath<(String, String)>,
    Json(mut task_def): Json<ProjectTaskDefinition>,
) -> Result<Json<ProjectTaskDefinition>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let tasks_dir = config.resolve_tasks_dir(&root);

    task_def.id = task_id;
    prepare_task_schedule(&mut task_def)?;
    ProjectTaskStore::save_task(&tasks_dir, &task_def)
        .await
        .map_err(bad)?;
    let slug_clone = slug.clone();
    let task_def_clone = task_def.clone();
    tokio::spawn(async move {
        sync_task_schedule(&slug_clone, &task_def_clone).await;
    });
    broadcast_project_event(HarnessProjectEvent::TaskUpserted {
        project_slug: slug,
        task_id: task_def.id.clone(),
    });

    Ok(Json(task_def))
}

pub async fn delete_project_task(
    AxumPath((slug, task_id)): AxumPath<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let tasks_dir = config.resolve_tasks_dir(&root);

    ProjectTaskStore::delete_task(&tasks_dir, &task_id)
        .await
        .map_err(not_found)?;
    let slug_clone = slug.clone();
    let task_id_clone = task_id.clone();
    tokio::spawn(async move {
        unschedule_task(&slug_clone, &task_id_clone).await;
    });
    broadcast_project_event(HarnessProjectEvent::TaskDeleted {
        project_slug: slug,
        task_id: task_id.clone(),
    });

    Ok(Json(
        serde_json::json!({ "success": true, "deleted": task_id }),
    ))
}

pub async fn patch_project_task_schedule(
    AxumPath((slug, task_id)): AxumPath<(String, String)>,
    Json(patch): Json<ProjectTaskSchedulePatch>,
) -> Result<Json<ProjectTaskDefinition>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let tasks_dir = config.resolve_tasks_dir(&root);

    let mut task_def = ProjectTaskStore::get_task(&tasks_dir, &task_id)
        .await
        .map_err(not_found)?;

    if let Some(cron) = patch.cron {
        task_def.cron = Some(cron);
    }
    if let Some(paused) = patch.paused {
        task_def.schedule_paused = paused;
    }
    prepare_task_schedule(&mut task_def)?;

    ProjectTaskStore::save_task(&tasks_dir, &task_def)
        .await
        .map_err(bad)?;
    let slug_clone = slug.clone();
    let task_def_clone = task_def.clone();
    tokio::spawn(async move {
        sync_task_schedule(&slug_clone, &task_def_clone).await;
    });
    broadcast_project_event(HarnessProjectEvent::TaskUpserted {
        project_slug: slug,
        task_id: task_def.id.clone(),
    });
    Ok(Json(task_def))
}

pub async fn run_project_task(
    AxumPath((slug, task_id)): AxumPath<(String, String)>,
    Json(req): Json<ProjectTaskRunRequest>,
) -> Result<Json<HarnessRunResponseDto>, (StatusCode, Json<serde_json::Value>)> {
    let rx = launch_project_task(&slug, &task_id, req)
        .await
        .map_err(launch_error)?;

    match rx.await {
        Ok(Ok(dto)) => Ok(Json(dto)),
        Ok(Err(e)) => Err(bad(e)),
        Err(_) => Err(bad("Project harness task worker ended unexpectedly")),
    }
}

pub async fn launch_project_task(
    slug: &str,
    task_id: &str,
    req: ProjectTaskRunRequest,
) -> Result<tokio::sync::oneshot::Receiver<Result<HarnessRunResponseDto>>> {
    let (_entry, root) = resolve_project_root(slug)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let tasks_dir = config.resolve_tasks_dir(&root);

    let mut task_def = ProjectTaskStore::get_task(&tasks_dir, task_id).await?;
    let prompt = compose_dynamic_run_prompt(
        &task_def.prompt,
        task_def.dynamic_prompt,
        req.extra_prompt.as_deref(),
    )?;
    task_def.prompt = prompt;

    let benchmark_item = ProjectTaskStore::to_benchmark_item(&task_def, None);
    let task_spec = benchmark_item.task;

    let max_turns = req
        .max_turns
        .or(task_def.max_turns)
        .unwrap_or(config.execution.max_turns);

    let record_requested = req.record.unwrap_or(false) || req.record_path.is_some();
    let cassette_path = if record_requested {
        let path_str = req.record_path.clone().unwrap_or_else(|| {
            let cassettes_dir = config.resolve_cassettes_dir(&root);
            cassettes_dir
                .join(format!("{}.cassette.json", task_id))
                .to_string_lossy()
                .to_string()
        });
        Some(PathBuf::from(path_str))
    } else {
        None
    };

    let job_id = new_harness_job_id("task");
    let started_at_dt = Utc::now();
    let started_at = started_at_dt.to_rfc3339();
    let task_id_for_fail = task_spec.id.clone();
    let notify_task_name = task_def
        .name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| task_spec.problem_statement.clone());
    let notify_model = req.model.clone().or(config.policy.model.clone());

    try_register_active_job(HarnessActiveJobDto {
        job_id: job_id.clone(),
        task_id: task_spec.id.clone(),
        job_type: "task".to_string(),
        description: notify_task_name.clone(),
        started_at: started_at.clone(),
        current_status: "running".to_string(),
        provider: req.provider.clone().or(config.policy.provider.clone()),
        model: notify_model.clone(),
        project_slug: Some(slug.to_string()),
    })
    .map_err(anyhow::Error::new)?;

    let persist_root = root.clone();
    let persist_slug = slug.to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let spawn_job_id = job_id.clone();
    let join_handle = tokio::spawn(async move {
        let run_res = execute_project_task(
            root,
            config,
            req,
            task_spec,
            max_turns,
            cassette_path,
            spawn_job_id.clone(),
        )
        .await;

        let (task_status, duration_seconds, summary_result, error_message) = match &run_res {
            Ok(dto) => {
                upsert_live_snapshot(&spawn_job_id, dto.clone());
                let _ = persist_harness_run(
                    "task",
                    dto,
                    &started_at,
                    Some(&persist_slug),
                    Some(&persist_root),
                    None,
                );
                let st = match dto.status {
                    openduck_harness::types::RunStatus::Success => {
                        crate::notification::TaskStatus::Succeeded
                    }
                    openduck_harness::types::RunStatus::Cancelled => {
                        crate::notification::TaskStatus::Cancelled
                    }
                    _ => crate::notification::TaskStatus::Failed,
                };
                (
                    st,
                    (dto.duration_ms / 1000) as u64,
                    dto.final_answer.clone(),
                    None,
                )
            }
            Err(e) => {
                let err_str = format!("{e:#}");
                let live_snapshot = get_live_snapshot(&spawn_job_id);
                let failed = failed_run_dto(&task_id_for_fail, &err_str, live_snapshot);
                upsert_live_snapshot(&spawn_job_id, failed.clone());
                let _ = persist_harness_run(
                    "task",
                    &failed,
                    &started_at,
                    Some(&persist_slug),
                    Some(&persist_root),
                    Some(&err_str),
                );
                (
                    crate::notification::TaskStatus::Failed,
                    (failed.duration_ms / 1000) as u64,
                    None,
                    Some(err_str),
                )
            }
        };

        let report = crate::notification::TaskExecutionReport {
            job_id: spawn_job_id.clone(),
            job_name: notify_task_name,
            session_id: format!("{persist_slug}:{task_id_for_fail}"),
            trigger_type: "harness_task".to_string(),
            status: task_status,
            started_at: started_at_dt,
            finished_at: Utc::now(),
            duration_seconds,
            total_tokens_used: None,
            model_name: notify_model,
            project_id: Some(persist_slug.clone()),
            summary_result,
            error_message,
            log_url: None,
        };

        tokio::spawn(async move {
            tracing::info!(
                job_id = %report.job_id,
                job_name = %report.job_name,
                session_id = %report.session_id,
                status = %report.status,
                trigger = %report.trigger_type,
                duration_seconds = report.duration_seconds,
                project_id = report.project_id.as_deref(),
                "Dispatching project harness task completion notification"
            );
            let notifier = crate::notification::NotificationService::load();
            if let Err(err) = notifier.handle_task_completion(report).await {
                tracing::error!(%err, "Failed to send project harness task completion notification");
            } else {
                tracing::info!(
                    "Project harness task completion notification processed successfully"
                );
            }
        });

        let (run_status, final_ans, dur_ms) = match &run_res {
            Ok(dto) => (dto.status, dto.final_answer.clone(), dto.duration_ms),
            Err(e) => {
                let snapshot = get_live_snapshot(&spawn_job_id);
                let dur = snapshot.as_ref().map(|s| s.duration_ms).unwrap_or(0);
                (
                    openduck_harness::types::RunStatus::Failure,
                    Some(format!("{e:#}")),
                    dur,
                )
            }
        };

        broadcast_job_live_event(HarnessJobLiveEvent::Finished {
            job_id: spawn_job_id.clone(),
            status: run_status,
            duration_ms: dur_ms,
            final_answer: final_ans,
        });

        remove_active_job(&spawn_job_id);
        let _ = tx.send(run_res);
    });
    register_abort_handle(&job_id, join_handle.abort_handle());

    Ok(rx)
}

fn resolve_continuation_checkpoint(
    root: &std::path::Path,
    task_id: &str,
    req: &ProjectTaskRunRequest,
) -> Result<Option<ContinuationCheckpoint>> {
    let Some(run_id) = req.continue_from_run_id.as_deref() else {
        return Ok(None);
    };
    let runs_dir = project_runs_dir(root);
    let history = read_history_run(run_id, &[root, runs_dir.as_path()])?;
    if history.task_id != task_id {
        return Err(anyhow!(
            "History run '{run_id}' belongs to task '{}', not '{task_id}'",
            history.task_id
        ));
    }
    if !is_continuable_status(history.status) {
        return Err(anyhow!(
            "Cannot continue run '{run_id}' with status {:?}",
            history.status
        ));
    }
    let mut checkpoint = history.continuation.unwrap_or_else(|| {
        openduck_harness::checkpoint_from_trajectory(
            &history.task_id,
            run_id,
            history.status,
            history.final_answer.clone(),
            &history.trajectory.steps,
        )
    });
    // Rebuild the progress summary from the stored trajectory so older runs
    // (which only kept a compaction dump) continue from a usable step summary.
    checkpoint.stop_reason =
        openduck_harness::ContinuationStopReason::from_run_status(history.status);
    checkpoint.step_count = history.trajectory.steps.len();
    checkpoint.compacted_summary = openduck_harness::run_progress_summary_from_steps(
        history.status,
        checkpoint.stop_reason,
        history.final_answer.as_deref(),
        &history.trajectory.steps,
    );
    Ok(Some(checkpoint))
}

fn build_project_task_system_prompt(
    base_prompt: &str,
    task_id: &str,
    formatted_rules: &str,
    hints: &str,
) -> String {
    let mut prompt = base_prompt.to_string();
    if !formatted_rules.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(formatted_rules);
    }
    if !hints.is_empty() {
        prompt.push_str("\n\n# Project Context Hints:\n\n");
        prompt.push_str(hints);
    }
    with_task_notes_plan(&prompt, task_id)
}

async fn execute_project_task(
    root: PathBuf,
    config: ProjectHarnessConfig,
    req: ProjectTaskRunRequest,
    task_spec: TaskSpec,
    max_turns: usize,
    cassette_path: Option<PathBuf>,
    job_id: String,
) -> Result<HarnessRunResponseDto> {
    let continuation = resolve_continuation_checkpoint(&root, &task_spec.id, &req)?;
    let extra_turns = req.extra_turns;
    if continuation.is_none() {
        clean_stale_task_scratchpad(&root, &task_spec.id);
    }
    let mut sandbox = LocalSandbox::new(root);
    sandbox.initialize().await?;

    if req.echo.unwrap_or(false) {
        let base_policy = EchoPolicy::default();
        return run_with_optional_record(
            base_policy,
            sandbox,
            &task_spec,
            max_turns,
            cassette_path,
            &job_id,
            None,
            None,
            Some(&config.execution),
            Some(&config.judge),
            continuation,
            extra_turns,
        )
        .await;
    }

    let provider = req
        .provider
        .as_deref()
        .or(config.policy.provider.as_deref());
    let model = req.model.as_deref().or(config.policy.model.as_deref());
    let mut base_policy = resolve_harness_policy(provider, model).await?;

    // Load active global and project-level rules
    let rules = crate::rules::discover_rules(Some(sandbox.workspace_root()), &[]);
    let active =
        crate::rules::RuleMatcher::filter_active_rules(&rules, sandbox.workspace_root(), &[]);
    let active_owned: Vec<crate::rules::Rule> = active.into_iter().cloned().collect();
    let formatted_rules = crate::rules::format_rules_for_prompt(&active_owned);

    let active_rules_summary: Vec<openduck_harness::telemetry::ActiveRuleSummary> = active_owned
        .iter()
        .map(|r| openduck_harness::telemetry::ActiveRuleSummary {
            name: r.name.clone(),
            description: r.description.clone(),
            global: r.global,
            path: r.path.to_string_lossy().to_string(),
        })
        .collect();

    let base_prompt = config
        .policy
        .system_prompt
        .clone()
        .unwrap_or_else(|| base_policy.system_prompt().to_string());

    // Load project context hints (.openduckhints, AGENTS.md)
    let hints_filenames = crate::hints::get_context_filenames();
    let ignore_patterns = crate::hints::build_gitignore(sandbox.workspace_root());
    let hints =
        crate::hints::load_hint_files(sandbox.workspace_root(), &hints_filenames, &ignore_patterns);

    let final_system_prompt =
        build_project_task_system_prompt(&base_prompt, &task_spec.id, &formatted_rules, &hints);

    base_policy = base_policy.with_system_prompt(final_system_prompt.clone());

    run_with_optional_record(
        base_policy,
        sandbox,
        &task_spec,
        max_turns,
        cassette_path,
        &job_id,
        Some(final_system_prompt),
        Some(active_rules_summary),
        Some(&config.execution),
        Some(&config.judge),
        continuation,
        extra_turns,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_with_optional_record<P: openduck_harness::policy::AgentPolicy>(
    base_policy: P,
    sandbox: LocalSandbox,
    task_spec: &TaskSpec,
    max_turns: usize,
    cassette_path: Option<PathBuf>,
    job_id: &str,
    system_prompt: Option<String>,
    active_rules: Option<Vec<openduck_harness::telemetry::ActiveRuleSummary>>,
    execution_config: Option<&openduck_harness::project::ExecutionConfig>,
    judge_config: Option<&openduck_harness::project::JudgeConfig>,
    continuation: Option<ContinuationCheckpoint>,
    extra_turns: Option<usize>,
) -> Result<HarnessRunResponseDto> {
    if let Some(c_path) = cassette_path {
        let mut cas_obj = Cassette::new(&task_spec.id);
        cas_obj.task_spec = Some(task_spec.clone());
        let cassette = Arc::new(Mutex::new(cas_obj));
        let policy = ReplayPolicy::new_record(base_policy, cassette.clone());
        let mut harness = AgentHarness::new(policy, sandbox)
            .with_max_turns(max_turns)
            .with_system_prompt(system_prompt.clone())
            .with_active_rules(active_rules.clone())
            .with_continuation(continuation.clone())
            .with_extra_turns(extra_turns);
        if let Some(exec) = execution_config {
            if let Some(threshold) = exec.stagnation_threshold {
                harness = harness.with_stagnation_threshold(threshold);
            }
            if let Some(enabled) = exec.circuit_breaker_enabled {
                harness = harness.with_circuit_breaker(enabled);
            }
            if let Some(interval) = exec.periodic_review_interval {
                harness = harness.with_periodic_review_interval(Some(interval));
            }
        }
        if let Some(judge_cfg) = judge_config {
            harness = harness.with_judge_engine(
                openduck_harness::judge::DecisionEngine::from_config(judge_cfg),
            );
        }
        let mut harness = with_live_snapshot(harness, job_id, &task_spec.id);
        let run_res = harness.run_task(task_spec).await;
        if let Ok(cas) = cassette.try_lock() {
            if !cas.frames.is_empty() {
                let _ = cas.save_to_file(&c_path).await;
            }
        }
        let res = run_res?;
        let cas = cassette.lock().await;
        cas.save_to_file(&c_path).await?;
        let mut dto = to_run_dto(res, Some(c_path.to_string_lossy().to_string()));
        if dto.system_prompt.is_none() {
            dto.system_prompt = system_prompt;
        }
        if dto.active_rules.is_none() {
            dto.active_rules = active_rules;
        }
        Ok(dto)
    } else {
        let mut harness = AgentHarness::new(base_policy, sandbox)
            .with_max_turns(max_turns)
            .with_system_prompt(system_prompt.clone())
            .with_active_rules(active_rules.clone())
            .with_continuation(continuation)
            .with_extra_turns(extra_turns);
        if let Some(exec) = execution_config {
            if let Some(threshold) = exec.stagnation_threshold {
                harness = harness.with_stagnation_threshold(threshold);
            }
            if let Some(enabled) = exec.circuit_breaker_enabled {
                harness = harness.with_circuit_breaker(enabled);
            }
            if let Some(interval) = exec.periodic_review_interval {
                harness = harness.with_periodic_review_interval(Some(interval));
            }
        }
        if let Some(judge_cfg) = judge_config {
            harness = harness.with_judge_engine(
                openduck_harness::judge::DecisionEngine::from_config(judge_cfg),
            );
        }
        let mut harness = with_live_snapshot(harness, job_id, &task_spec.id);
        let res = harness.run_task(task_spec).await?;
        let mut dto = to_run_dto(res, None);
        if dto.system_prompt.is_none() {
            dto.system_prompt = system_prompt;
        }
        if dto.active_rules.is_none() {
            dto.active_rules = active_rules;
        }
        Ok(dto)
    }
}

pub async fn eval_project_tasks(
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<ProjectHarnessEvalRequest>,
) -> Result<Json<EvaluationReport>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let tasks_dir = config.resolve_tasks_dir(&root);

    let all_summaries = ProjectTaskStore::list_tasks(&tasks_dir)
        .await
        .map_err(bad)?;

    if all_summaries.is_empty() {
        return Err(bad("No tasks found for project evaluation"));
    }

    let mut filtered_ids = Vec::new();
    for s in all_summaries {
        if let Some(ids) = &req.task_ids {
            if !ids.contains(&s.id) {
                continue;
            }
        }
        if let Some(cat) = &req.category {
            if s.category.as_deref() != Some(cat.as_str()) {
                continue;
            }
        }
        if let Some(tags) = &req.tags {
            if !tags.iter().any(|t| s.tags.contains(t)) {
                continue;
            }
        }
        filtered_ids.push(s.id);
    }

    if filtered_ids.is_empty() {
        return Err(bad("No tasks matched the filter criteria"));
    }

    let mut benchmark_items = Vec::new();
    for tid in filtered_ids {
        let task_def = ProjectTaskStore::get_task(&tasks_dir, &tid)
            .await
            .map_err(bad)?;
        benchmark_items.push(ProjectTaskStore::to_benchmark_item(&task_def, None));
    }

    let results_dir = req
        .output_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| config.resolve_results_dir(&root));

    let concurrency = req.concurrency.unwrap_or(config.execution.concurrency);
    let max_turns = req.max_turns.unwrap_or(config.execution.max_turns);

    let runner_config = EvalRunnerConfig {
        concurrency,
        output_dir: results_dir,
        max_turns,
    };

    let runner = EvalRunner::new(runner_config);
    let suite_name = format!("project-{}", slug);

    let eval_started_at = Utc::now();
    let job_id = new_harness_job_id("eval");
    register_active_job(HarnessActiveJobDto {
        job_id: job_id.clone(),
        task_id: suite_name.clone(),
        job_type: "eval".to_string(),
        description: format!(
            "Project evaluation on {} ({} tasks)",
            slug,
            benchmark_items.len()
        ),
        started_at: eval_started_at.to_rfc3339(),
        current_status: "running".to_string(),
        provider: req.provider.clone().or(config.policy.provider.clone()),
        model: req.model.clone().or(config.policy.model.clone()),
        project_slug: Some(slug.clone()),
    });

    let report_res = if req.echo.unwrap_or(false) {
        runner
            .run_suite(&suite_name, benchmark_items, EchoPolicy::default)
            .await
    } else {
        let provider = req
            .provider
            .as_deref()
            .or(config.policy.provider.as_deref());
        let model = req.model.as_deref().or(config.policy.model.as_deref());

        match resolve_harness_policy(provider, model).await {
            Ok(mut policy) => {
                if let Some(sys_prompt) = &config.policy.system_prompt {
                    policy = policy.with_system_prompt(sys_prompt);
                }
                let policy_template = policy.clone();
                runner
                    .run_suite(&suite_name, benchmark_items, move || {
                        policy_template.clone()
                    })
                    .await
            }
            Err(e) => Err(e),
        }
    };

    remove_active_job(&job_id);

    let (eval_status, duration_seconds, summary_result, error_msg) = match &report_res {
        Ok(rep) => {
            let st = if rep.metrics.failed_tasks == 0 && rep.metrics.error_tasks == 0 {
                crate::notification::TaskStatus::Succeeded
            } else {
                crate::notification::TaskStatus::Failed
            };
            let summary = format!(
                "Evaluation finished: {}/{} tasks passed ({:.1}% pass rate, {} failed)",
                rep.metrics.passed_tasks,
                rep.metrics.total_tasks,
                rep.metrics.pass_rate * 100.0,
                rep.metrics.failed_tasks + rep.metrics.error_tasks
            );
            let dur = rep
                .completed_at
                .signed_duration_since(rep.started_at)
                .num_seconds()
                .max(0) as u64;
            (st, dur, Some(summary), None)
        }
        Err(e) => (
            crate::notification::TaskStatus::Failed,
            0,
            None,
            Some(format!("{e:#}")),
        ),
    };

    let eval_report = crate::notification::TaskExecutionReport {
        job_id: job_id.clone(),
        job_name: format!("Project evaluation on {slug}"),
        session_id: format!("{slug}:eval"),
        trigger_type: "harness_eval".to_string(),
        status: eval_status,
        started_at: eval_started_at,
        finished_at: Utc::now(),
        duration_seconds,
        total_tokens_used: None,
        model_name: req.model.clone().or(config.policy.model.clone()),
        project_id: Some(slug.clone()),
        summary_result,
        error_message: error_msg,
        log_url: None,
    };

    tokio::spawn(async move {
        tracing::info!(
            job_id = %eval_report.job_id,
            job_name = %eval_report.job_name,
            session_id = %eval_report.session_id,
            status = %eval_report.status,
            trigger = %eval_report.trigger_type,
            duration_seconds = eval_report.duration_seconds,
            project_id = eval_report.project_id.as_deref(),
            "Dispatching project evaluation completion notification"
        );
        let notifier = crate::notification::NotificationService::load();
        if let Err(err) = notifier.handle_task_completion(eval_report).await {
            tracing::error!(%err, "Failed to send project evaluation completion notification");
        } else {
            tracing::info!("Project evaluation completion notification processed successfully");
        }
    });

    if report_res.is_ok() {
        broadcast_project_event(HarnessProjectEvent::OverviewInvalidated {
            project_slug: slug.clone(),
        });
    }

    Ok(Json(report_res.map_err(bad)?))
}

pub async fn list_project_active_jobs(
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<Vec<HarnessActiveJobDto>>, (StatusCode, Json<serde_json::Value>)> {
    sources::read_project(&slug).map_err(not_found)?;
    Ok(Json(active_jobs_for_project(&slug)))
}

pub async fn inspect_project_harness_job(
    AxumPath((slug, job_id)): AxumPath<(String, String)>,
) -> Result<Json<HarnessJobInspectDto>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, _root) = resolve_project_root(&slug).map_err(not_found)?;
    let inspect = inspect_active_job(&job_id)
        .ok_or_else(|| not_found(format!("No running harness job: {job_id}")))?;
    if inspect.job.project_slug.as_deref() != Some(slug.as_str()) {
        return Err(not_found(format!(
            "Job {job_id} is not running in project {slug}"
        )));
    }
    Ok(Json(inspect))
}

pub async fn stop_project_task(
    AxumPath((slug, task_id)): AxumPath<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, _root) = resolve_project_root(&slug).map_err(not_found)?;
    let active_jobs = active_jobs_for_project(&slug);
    let target_job = active_jobs
        .into_iter()
        .find(|j| j.task_id == task_id && j.job_type == "task")
        .ok_or_else(|| {
            not_found(format!(
                "Task '{task_id}' is not currently running in project '{slug}'"
            ))
        })?;

    let cancelled =
        stop_active_job(&target_job.job_id, Some("Task stopped by user")).map_err(bad)?;
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Task '{task_id}' was stopped"),
        "project": slug,
        "taskId": cancelled.task_id,
        "jobId": target_job.job_id,
        "status": "cancelled",
    })))
}

pub async fn stop_project_harness_job(
    AxumPath((slug, job_id)): AxumPath<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, _root) = resolve_project_root(&slug).map_err(not_found)?;
    let job = get_active_job(&job_id)
        .ok_or_else(|| not_found(format!("No active harness job: {job_id}")))?;
    if job.project_slug.as_deref() != Some(slug.as_str()) {
        return Err(not_found(format!(
            "Job {job_id} does not belong to project {slug}"
        )));
    }
    let cancelled = stop_active_job(&job_id, Some("Task stopped by user")).map_err(bad)?;
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Stopped job {job_id} in project {slug}"),
        "project": slug,
        "jobId": job_id,
        "taskId": cancelled.task_id,
        "status": "cancelled",
    })))
}

const DEFAULT_HARNESS_HISTORY_TIMEOUT_SECS: u64 = 60;

fn resolve_history_timeout(config: &ProjectHarnessConfig) -> std::time::Duration {
    if let Some(secs) = crate::config::env::get_var("HARNESS_HISTORY_TIMEOUT_SECS")
        .and_then(|v| v.parse::<u64>().ok())
    {
        return std::time::Duration::from_secs(secs);
    }
    if let Some(secs) = config.execution.history_timeout_seconds {
        return std::time::Duration::from_secs(secs);
    }
    std::time::Duration::from_secs(DEFAULT_HARNESS_HISTORY_TIMEOUT_SECS)
}

pub async fn list_project_history(
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<Vec<HarnessHistorySummaryDto>>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let tasks_dir = config.resolve_tasks_dir(&root);
    let timeout_duration = resolve_history_timeout(&config);
    let slug_for_list = slug.clone();
    let root_for_list = root.clone();
    let history_fut = tokio::time::timeout(
        timeout_duration,
        tokio::task::spawn_blocking(move || {
            list_history_for_project(&slug_for_list, Some(&root_for_list))
        }),
    );
    let tasks_fut = ProjectTaskStore::list_tasks(&tasks_dir);
    let (history_res, tasks_res) = tokio::join!(history_fut, tasks_fut);
    let history = match history_res {
        Ok(Ok(history)) => history,
        Ok(Err(e)) => return Err(bad(format!("Failed to list project history: {e}"))),
        Err(_) => {
            tracing::warn!(
                project = %slug,
                timeout_secs = timeout_duration.as_secs(),
                "Project history listing timed out"
            );
            Vec::new()
        }
    };
    let names: HashMap<String, String> = tasks_res
        .unwrap_or_else(|_| Vec::new())
        .into_iter()
        .map(|task| (task.id, task.name))
        .collect();
    Ok(Json(apply_history_task_names(history, &names)))
}

pub async fn get_project_history_detail(
    AxumPath((slug, run_id)): AxumPath<(String, String)>,
) -> Result<Json<HarnessRunResponseDto>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let project_dir = project_runs_dir(&root);
    let run =
        tokio::task::spawn_blocking(move || read_history_run(&run_id, &[project_dir.as_path()]))
            .await
            .map_err(|e| bad(format!("Failed to read project history: {e}")))?
            .map_err(not_found)?;
    Ok(Json(run))
}

pub async fn list_project_reports(
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<Vec<HarnessReportSummaryDto>>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let results_dir = config.resolve_results_dir(&root);
    let reports = tokio::task::spawn_blocking(move || internal_list_reports(&results_dir))
        .await
        .map_err(|e| bad(format!("Failed to list project reports: {e}")))?
        .map_err(bad)?;
    Ok(Json(reports))
}

pub async fn get_project_report(
    AxumPath((slug, report_id)): AxumPath<(String, String)>,
) -> Result<Json<EvaluationReport>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let results_dir = config.resolve_results_dir(&root);

    let target = if report_id.ends_with(".json") {
        results_dir.join(&report_id)
    } else {
        results_dir.join(format!("{}.json", report_id))
    };

    if !target.exists() {
        return Err(not_found(format!("Report not found at {:?}", target)));
    }
    let content = std::fs::read_to_string(&target).map_err(bad)?;
    let report: EvaluationReport = serde_json::from_str(&content).map_err(bad)?;
    Ok(Json(report))
}

pub async fn list_project_cassettes(
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<Vec<HarnessCassetteSummaryDto>>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let cassettes_dir = config.resolve_cassettes_dir(&root);
    let cassettes = tokio::task::spawn_blocking(move || internal_list_cassettes(&cassettes_dir))
        .await
        .map_err(|e| bad(format!("Failed to list project cassettes: {e}")))?
        .map_err(bad)?;
    Ok(Json(cassettes))
}

pub async fn stream_project_harness_events(
    AxumPath(slug): AxumPath<String>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = subscribe_project_events();
    let stream =
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |item| match item {
            Ok(event) => {
                let matches = match &event {
                    HarnessProjectEvent::TaskStatusChanged { project_slug, .. }
                    | HarnessProjectEvent::TaskUpserted { project_slug, .. }
                    | HarnessProjectEvent::TaskDeleted { project_slug, .. }
                    | HarnessProjectEvent::ReportGenerated { project_slug, .. }
                    | HarnessProjectEvent::OverviewInvalidated { project_slug } => {
                        project_slug == &slug
                    }
                };
                if matches {
                    serde_json::to_string(&event)
                        .ok()
                        .map(|data| Ok(Event::default().data(data)))
                } else {
                    None
                }
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!(slug = %slug, lagged = n, "Project SSE stream lagged");
                let event = HarnessProjectEvent::OverviewInvalidated {
                    project_slug: slug.clone(),
                };
                serde_json::to_string(&event)
                    .ok()
                    .map(|data| Ok(Event::default().data(data)))
            }
        });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

pub async fn stream_harness_job_live(
    AxumPath((_slug, job_id)): AxumPath<(String, String)>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = subscribe_job_live_events();
    let initial_snapshot = get_live_snapshot(&job_id);

    let stream = async_stream::stream! {
        if let Some(snapshot) = initial_snapshot {
            let is_already_finished = !matches!(snapshot.status, openduck_harness::types::RunStatus::Running);
            let initial_event = HarnessJobLiveEvent::Snapshot {
                job_id: job_id.clone(),
                snapshot: Box::new(snapshot),
            };
            if let Ok(data) = serde_json::to_string(&initial_event) {
                yield Ok(Event::default().data(data));
            }
            if is_already_finished {
                return;
            }
        } else {
            return;
        }

        let mut bstream = tokio_stream::wrappers::BroadcastStream::new(rx);
        while let Some(item) = bstream.next().await {
            match item {
                Ok(event) => {
                    let ev_job_id = match &event {
                        HarnessJobLiveEvent::StepUpdate { job_id: jid, .. }
                        | HarnessJobLiveEvent::Snapshot { job_id: jid, .. }
                        | HarnessJobLiveEvent::Finished { job_id: jid, .. } => jid,
                    };
                    if ev_job_id == &job_id {
                        let is_finished = matches!(&event, HarnessJobLiveEvent::Finished { .. });
                        if let Ok(data) = serde_json::to_string(&event) {
                            yield Ok(Event::default().data(data));
                        }
                        if is_finished {
                            break;
                        }
                    }
                }
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                    tracing::warn!(job_id = %job_id, lagged = n, "Job SSE stream lagged");
                    if let Some(snap) = get_live_snapshot(&job_id) {
                        let snap_ev = HarnessJobLiveEvent::Snapshot {
                            job_id: job_id.clone(),
                            snapshot: Box::new(snap),
                        };
                        if let Ok(data) = serde_json::to_string(&snap_ev) {
                            yield Ok(Event::default().data(data));
                        }
                    }
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

pub fn heuristic_estimate(prompt: &str) -> ProjectTaskEstimateResponse {
    heuristic_estimate_with_feedback(prompt, None, None)
}

pub fn heuristic_estimate_with_feedback(
    prompt: &str,
    feedback: Option<&str>,
    previous_estimate: Option<&ProjectTaskEstimateResponse>,
) -> ProjectTaskEstimateResponse {
    let trimmed = prompt.trim();
    let mut base = if trimmed.is_empty() {
        ProjectTaskEstimateResponse {
            suggested_max_turns: 25,
            suggested_timeout_seconds: 300,
            complexity: "simple".to_string(),
            reasoning: "Default baseline budget for empty or minimal prompt.".to_string(),
            source: "heuristic".to_string(),
        }
    } else {
        let lower = trimmed.to_lowercase();
        let lines: Vec<&str> = trimmed
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        let bullet_count = lines
            .iter()
            .filter(|l| {
                l.starts_with('-')
                    || l.starts_with('*')
                    || l.starts_with("1.")
                    || l.starts_with("2.")
                    || l.starts_with("3.")
                    || l.starts_with("4.")
                    || l.starts_with("5.")
            })
            .count();

        let deep_keywords = [
            "full-stack",
            "whole project",
            "entire repository",
            "full migration",
            "from scratch",
            "end-to-end rewrite",
            "autonomous workflow",
        ];
        let complex_keywords = [
            "refactor",
            "migration",
            "migrate",
            "architecture",
            "end-to-end",
            "e2e",
            "benchmark",
            "distributed",
            "rewrite",
            "optimize",
            "concurrency",
            "docker",
            "pipeline",
            "subagent",
        ];
        let simple_keywords = [
            "typo",
            "spelling",
            "rename",
            "fix comment",
            "readme",
            "docs",
            "docstring",
            "explain",
            "what is",
            "where is",
            "format",
            "lint",
            "check",
        ];

        let has_deep = deep_keywords.iter().any(|k| lower.contains(k));
        let has_complex = complex_keywords.iter().any(|k| lower.contains(k));
        let has_simple = simple_keywords.iter().any(|k| lower.contains(k));

        if has_deep || (bullet_count >= 5 && has_complex) {
            ProjectTaskEstimateResponse {
                suggested_max_turns: 220,
                suggested_timeout_seconds: 3600,
                complexity: "deep".to_string(),
                reasoning:
                    "Large-scale autonomous task involving cross-system architecture, extensive test cycles, and high uncertainty."
                        .to_string(),
                source: "heuristic".to_string(),
            }
        } else if has_complex || bullet_count >= 3 || lines.len() >= 5 {
            ProjectTaskEstimateResponse {
                suggested_max_turns: 110,
                suggested_timeout_seconds: 1800,
                complexity: "complex".to_string(),
                reasoning:
                    "Multi-module task requiring iterative exploration, build error diagnostics, and test validation loops."
                        .to_string(),
                source: "heuristic".to_string(),
            }
        } else if has_simple && trimmed.len() < 150 && lines.len() <= 2 {
            ProjectTaskEstimateResponse {
                suggested_max_turns: 20,
                suggested_timeout_seconds: 240,
                complexity: "simple".to_string(),
                reasoning: "Localized task with straightforward changes or inspection.".to_string(),
                source: "heuristic".to_string(),
            }
        } else {
            ProjectTaskEstimateResponse {
                suggested_max_turns: 50,
                suggested_timeout_seconds: 900,
                complexity: "medium".to_string(),
                reasoning:
                    "Standard development task involving codebase discovery, implementation, and test validation."
                        .to_string(),
                source: "heuristic".to_string(),
            }
        }
    };

    if let Some(prev) = previous_estimate {
        if prev.suggested_max_turns > 0 {
            base.suggested_max_turns = prev.suggested_max_turns;
            base.suggested_timeout_seconds = prev.suggested_timeout_seconds;
            base.complexity = prev.complexity.clone();
            base.reasoning = prev.reasoning.clone();
        }
    }

    if let Some(fb) = feedback.map(str::trim).filter(|f| !f.is_empty()) {
        let fb_lower = fb.to_lowercase();
        let mut modified = false;

        let mut explicit_turns: Option<usize> = None;
        for part in fb_lower.split(|c: char| !c.is_alphanumeric()) {
            if let Ok(n) = part.parse::<usize>() {
                if (1..=500).contains(&n) {
                    explicit_turns = Some(n);
                }
            }
        }

        let reduce_keywords = [
            "reduce",
            "decrease",
            "lower",
            "fewer",
            "less",
            "too high",
            "too many",
            "too much",
            "simple",
            "simpler",
            "small",
            "quick",
            "1 file",
            "one file",
            "single file",
            "minor",
            "overestimated",
            "short",
        ];
        let increase_keywords = [
            "increase",
            "higher",
            "more",
            "too low",
            "too few",
            "too short",
            "longer",
            "harder",
            "underestimated",
            "complex",
            "deep",
            "slow",
            "heavy",
            "e2e",
            "flaky",
            "large",
            "multi-step",
            "slow test",
        ];

        let has_reduce = reduce_keywords.iter().any(|k| fb_lower.contains(k));
        let has_increase = increase_keywords.iter().any(|k| fb_lower.contains(k));

        if let Some(turns) = explicit_turns {
            base.suggested_max_turns = turns.clamp(10, 500);
            base.suggested_timeout_seconds =
                ((base.suggested_max_turns as u64) * 20 + 120).clamp(120, 7200);
            if base.suggested_max_turns <= 30 {
                base.complexity = "simple".to_string();
            } else if base.suggested_max_turns <= 80 {
                base.complexity = "medium".to_string();
            } else if base.suggested_max_turns <= 200 {
                base.complexity = "complex".to_string();
            } else {
                base.complexity = "deep".to_string();
            }
            modified = true;
        } else if has_reduce && !has_increase {
            base.suggested_max_turns =
                ((base.suggested_max_turns as f64 * 0.55).round() as usize).clamp(15, 500);
            base.suggested_timeout_seconds =
                ((base.suggested_timeout_seconds as f64 * 0.6).round() as u64).clamp(180, 7200);
            if base.complexity == "deep" {
                base.complexity = "complex".to_string();
            } else if base.complexity == "complex" {
                base.complexity = "medium".to_string();
            } else {
                base.complexity = "simple".to_string();
            }
            modified = true;
        } else if has_increase && !has_reduce {
            base.suggested_max_turns =
                ((base.suggested_max_turns as f64 * 1.6).round() as usize).clamp(10, 500);
            base.suggested_timeout_seconds =
                ((base.suggested_timeout_seconds as f64 * 1.5).round() as u64).clamp(120, 7200);
            if base.complexity == "simple" {
                base.complexity = "medium".to_string();
            } else if base.complexity == "medium" {
                base.complexity = "complex".to_string();
            } else {
                base.complexity = "deep".to_string();
            }
            modified = true;
        }

        if modified {
            base.reasoning = format!(
                "Adjusted turn budget and timeout based on user feedback: \"{}\". {}",
                fb, base.reasoning
            );
            base.source = "heuristic (refined)".to_string();
        } else {
            base.reasoning = format!("{} (User comment noted: \"{}\")", base.reasoning, fb);
            base.source = "heuristic (refined)".to_string();
        }
    }

    base
}

pub(crate) fn clean_json_response(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(stripped) = trimmed.strip_prefix("```json") {
        if let Some(content) = stripped.strip_suffix("```") {
            return content.trim().to_string();
        }
    } else if let Some(stripped) = trimmed.strip_prefix("```") {
        if let Some(content) = stripped.strip_suffix("```") {
            return content.trim().to_string();
        }
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start < end {
            if let Some(slice) = trimmed.get(start..=end) {
                return slice.to_string();
            }
        }
    }
    trimmed.to_string()
}

fn format_history_feedback(history: &[HarnessHistorySummaryDto]) -> String {
    let recent_runs: Vec<&HarnessHistorySummaryDto> = history
        .iter()
        .filter(|h| h.step_count > 0)
        .take(5)
        .collect();

    if recent_runs.is_empty() {
        return String::new();
    }

    let mut out =
        String::from("\n### Historical Executions in This Project (Calibration Baseline):\n");
    for run in recent_runs {
        let duration_s = (run.duration_ms / 1000) as u64;
        let status_str = match &run.status {
            openduck_harness::types::RunStatus::Success => "Passed",
            openduck_harness::types::RunStatus::Failure => "Failed",
            openduck_harness::types::RunStatus::Timeout => "Timeout",
            openduck_harness::types::RunStatus::Cancelled => "Cancelled",
            openduck_harness::types::RunStatus::Running => "Running",
            openduck_harness::types::RunStatus::Skipped => "Skipped",
        };
        out.push_str(&format!(
            "- Task `{}`: actual = {} turns (duration: {}s, status: {})\n",
            run.task_id, run.step_count, duration_s, status_str
        ));
    }
    out.push_str(
        "Ground your estimate on these actual project metrics. Coding agents consume substantial turns on search, diagnostics, compiler fixes, and test loops.\n",
    );
    out
}

async fn estimate_with_model(
    config: &ProjectHarnessConfig,
    prompt: &str,
    history: &[HarnessHistorySummaryDto],
    feedback: Option<&str>,
    previous_estimate: Option<&ProjectTaskEstimateResponse>,
) -> Result<ProjectTaskEstimateResponse> {
    use crate::config::Config;
    use crate::conversation::message::{Message, MessageContent};
    use crate::model_config::model_config_from_user_config;

    let global_config = Config::global();
    let provider_name = match config.policy.provider.as_deref() {
        Some(p) => p.to_string(),
        None => global_config.get_goose_provider().context(
            "No default provider configured. Use --provider <name> or run 'goose configure'",
        )?,
    };

    let model_name = match config.policy.model.as_deref() {
        Some(m) => m.to_string(),
        None => global_config
            .get_goose_model()
            .unwrap_or_else(|_| "default".to_string()),
    };

    let model_config = model_config_from_user_config(&provider_name, &model_name)?;
    let provider = crate::providers::create(&provider_name, vec![]).await?;

    let history_text = format_history_feedback(history);

    let system = format!(
        r#"You are an expert AI task planner and execution cost estimator for autonomous software engineering agents.
Your goal is to estimate or refine the realistic `maxTurns` (tool-calling turn budget) and `timeoutSeconds` for an autonomous agent executing in this repository.

### Estimation Guidelines:
Autonomous coding agents do not follow a simple 1-to-1 linear path. Realistic task breakdown:
1. Nominal Steps (Happy Path):
   - Search & codebase navigation: 2-8 turns
   - Reading files & context gathering: 2-10 turns
   - Code edits & creation: 3-15 turns
   - Build, lint & test execution: 2-10 turns
2. Uncertainty & Retry Multiplier (1.5x - 3.5x):
   - Compiler/linter error diagnostics & fixes
   - Unit test debugging & iterative patch refinement
   - Multi-file ripple effects and dependency resolving
3. Complexity Tiers:
   - `simple` (10-30 turns, 180-600s): Localized typo/docs/small bugfix/single file inspect.
   - `medium` (30-80 turns, 600-1800s): Standard feature, API endpoint, bugfix with test verification.
   - `complex` (80-200 turns, 1800-3600s): Multi-module refactor, database/state migration, subsystem integration.
   - `deep` (200-500 turns, 3600-7200s): Full repository migration, E2E overhaul, large-scale autonomous dev.
4. User Feedback & Calibration:
   - If the user provides a comment or feedback on a previous estimate, prioritize the user's domain knowledge (e.g. fewer turns if isolated, more turns if test suite is slow or complex).
   - In `reasoning`, directly explain how the estimate was adjusted to accommodate the user's feedback.
{history_text}
Output ONLY a valid JSON object matching this schema:
{{
  "suggestedMaxTurns": <integer between 10 and 500>,
  "suggestedTimeoutSeconds": <integer between 120 and 7200>,
  "complexity": "<simple|medium|complex|deep>",
  "reasoning": "<concise 1-2 sentence explanation of nominal steps, uncertainty factor, and how user feedback was incorporated>"
}}"#
    );

    let mut user_msg = format!("Task Prompt:\n```\n{prompt}\n```");
    if let Some(prev) = previous_estimate {
        user_msg.push_str(&format!(
            "\n\n### Previous Estimate:\n- Max Turns: {}\n- Timeout: {}s\n- Complexity: {}\n- Previous Reasoning: {}",
            prev.suggested_max_turns, prev.suggested_timeout_seconds, prev.complexity, prev.reasoning
        ));
    }
    if let Some(fb) = feedback.filter(|f| !f.trim().is_empty()) {
        user_msg.push_str(&format!(
            "\n\n### User Feedback / Correction Comment:\n\"{}\"\n\nPlease review your previous estimate in light of the user's feedback and provide a corrected turn budget, timeout, and complexity. Explain your adjustment in the reasoning.",
            fb.trim()
        ));
    }

    let messages = vec![Message::user().with_text(user_msg)];
    let tools: [rmcp::model::Tool; 0] = [];

    let (reply, _) = tokio::time::timeout(
        std::time::Duration::from_secs(12),
        provider.complete(&model_config, &system, &messages, &tools),
    )
    .await
    .map_err(|_| anyhow!("Estimation request timed out"))??;

    let text = reply
        .content
        .iter()
        .filter_map(|block| match block {
            MessageContent::Text(raw) if !raw.text.is_empty() => Some(raw.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    let clean = clean_json_response(&text);

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ModelResponse {
        suggested_max_turns: Option<usize>,
        suggested_timeout_seconds: Option<u64>,
        complexity: Option<String>,
        reasoning: Option<String>,
    }

    let parsed: ModelResponse = serde_json::from_str(&clean)?;
    let suggested_max_turns = parsed.suggested_max_turns.unwrap_or(45).clamp(10, 500);
    let suggested_timeout_seconds = parsed
        .suggested_timeout_seconds
        .unwrap_or(((suggested_max_turns as u64) * 20 + 120).clamp(180, 7200))
        .clamp(120, 7200);
    let complexity = parsed.complexity.unwrap_or_else(|| "medium".to_string());
    let reasoning = parsed.reasoning.unwrap_or_else(|| {
        "Estimated based on prompt complexity and exploration overhead".to_string()
    });

    Ok(ProjectTaskEstimateResponse {
        suggested_max_turns,
        suggested_timeout_seconds,
        complexity,
        reasoning,
        source: "model".to_string(),
    })
}

pub async fn estimate_project_task(
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<ProjectTaskEstimateRequest>,
) -> Result<Json<ProjectTaskEstimateResponse>, (StatusCode, Json<serde_json::Value>)> {
    let (_entry, root) = resolve_project_root(&slug).map_err(not_found)?;
    let config = ProjectHarnessConfig::load_or_default(&root).await;
    let slug_for_history = slug.clone();
    let root_for_history = root.clone();
    let recent_history = tokio::task::spawn_blocking(move || {
        list_history_for_project(&slug_for_history, Some(&root_for_history))
    })
    .await
    .unwrap_or_default();

    let prompt = req.prompt.trim();
    let feedback = req.feedback.as_deref();
    let previous_estimate = req.previous_estimate.as_ref();

    if prompt.is_empty() {
        return Ok(Json(heuristic_estimate_with_feedback(
            "",
            feedback,
            previous_estimate,
        )));
    }

    match estimate_with_model(
        &config,
        prompt,
        &recent_history,
        feedback,
        previous_estimate,
    )
    .await
    {
        Ok(res) => Ok(Json(res)),
        Err(e) => {
            tracing::warn!(error = %e, "Model estimation failed or timed out, falling back to heuristic");
            Ok(Json(heuristic_estimate_with_feedback(
                prompt,
                feedback,
                previous_estimate,
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::harness::{task_notes_plan_path, GENERIC_SCRATCH_NOTES_INSTRUCTION};

    #[test]
    fn assembled_project_prompt_names_current_task_notes_plan() {
        let default_prompt = format!(
            "\nAvoid Redundant Reads: Never re-read the same file.\n\n{GENERIC_SCRATCH_NOTES_INSTRUCTION}"
        );
        let assembled = build_project_task_system_prompt(
            &default_prompt,
            "task-aaa",
            "# Rules\n- Be concise",
            "See AGENTS.md",
        );
        let path = task_notes_plan_path("task-aaa");
        assert!(assembled.contains(&path), "{assembled}");
        assert!(assembled.contains("# Rules\n- Be concise"));
        assert!(assembled.contains("See AGENTS.md"));
        assert!(
            !assembled.contains(".agent/notes-plan.md"),
            "must not cite shared notes-plan as the destination: {assembled}"
        );
        assert!(!assembled.contains(".agent/notes-plan-<task-id>.md"));
    }

    #[test]
    fn assembled_project_prompt_injects_notes_path_into_custom_system_prompt() {
        let custom = "Custom project policy. Do not mention a shared notes file.";
        let assembled = build_project_task_system_prompt(custom, "task-bbb", "", "");
        let path = task_notes_plan_path("task-bbb");
        assert!(assembled.contains(custom));
        assert!(assembled.contains(&path), "{assembled}");
        assert!(!assembled.contains(".agent/notes-plan.md"));
        let other = build_project_task_system_prompt(custom, "task-aaa", "", "");
        assert_ne!(
            task_notes_plan_path("task-aaa"),
            task_notes_plan_path("task-bbb")
        );
        assert!(other.contains(&task_notes_plan_path("task-aaa")));
        assert!(!other.contains(&path));
    }

    #[test]
    fn test_clean_json_response() {
        assert_eq!(
            clean_json_response("```json\n{\"suggestedMaxTurns\": 15}\n```"),
            "{\"suggestedMaxTurns\": 15}"
        );
        assert_eq!(
            clean_json_response("```\n{\"suggestedMaxTurns\": 20}\n```"),
            "{\"suggestedMaxTurns\": 20}"
        );
        assert_eq!(
            clean_json_response(
                "Here is the estimate:\n{\"suggestedMaxTurns\": 10}\nHope this helps!"
            ),
            "{\"suggestedMaxTurns\": 10}"
        );
    }

    #[test]
    fn test_heuristic_estimate() {
        let empty = heuristic_estimate("");
        assert_eq!(empty.complexity, "simple");
        assert_eq!(empty.suggested_max_turns, 25);
        assert_eq!(empty.source, "heuristic");

        let simple = heuristic_estimate("Fix the typo in README.md");
        assert_eq!(simple.complexity, "simple");
        assert_eq!(simple.suggested_max_turns, 20);
        assert_eq!(simple.suggested_timeout_seconds, 240);

        let medium = heuristic_estimate("Add a new GET /api/v1/health endpoint and test it");
        assert_eq!(medium.complexity, "medium");
        assert_eq!(medium.suggested_max_turns, 50);
        assert_eq!(medium.suggested_timeout_seconds, 900);

        let complex = heuristic_estimate(
            "Refactor the entire database storage layer to use connection pooling and migrate SQLite tables to Postgres",
        );
        assert_eq!(complex.complexity, "complex");
        assert_eq!(complex.suggested_max_turns, 110);
        assert_eq!(complex.suggested_timeout_seconds, 1800);

        let multi_step = heuristic_estimate(
            "- Step 1: Add new API\n- Step 2: Implement handlers\n- Step 3: Run integration tests\n- Step 4: Write documentation",
        );
        assert_eq!(multi_step.complexity, "complex");
        assert_eq!(multi_step.suggested_max_turns, 110);

        let deep = heuristic_estimate(
            "Full migration of whole project from scratch to distributed architecture",
        );
        assert_eq!(deep.complexity, "deep");
        assert_eq!(deep.suggested_max_turns, 220);
        assert_eq!(deep.suggested_timeout_seconds, 3600);
    }

    #[test]
    fn test_heuristic_estimate_with_feedback_reduction() {
        let prompt = "Refactor the entire database storage layer to use connection pooling";
        let initial = heuristic_estimate(prompt);
        assert_eq!(initial.suggested_max_turns, 110);

        let reduced = heuristic_estimate_with_feedback(
            prompt,
            Some("This is already mostly done, only 1 file changed, reduce turns"),
            Some(&initial),
        );
        assert!(reduced.suggested_max_turns < initial.suggested_max_turns);
        assert!(reduced.suggested_timeout_seconds < initial.suggested_timeout_seconds);
        assert!(reduced.reasoning.contains("Adjusted turn budget"));
        assert_eq!(reduced.source, "heuristic (refined)");
    }

    #[test]
    fn test_heuristic_estimate_with_feedback_explicit_turns() {
        let prompt = "Add health endpoint";
        let initial = heuristic_estimate(prompt);

        let refined = heuristic_estimate_with_feedback(
            prompt,
            Some("Please allocate 85 turns due to slow test suite"),
            Some(&initial),
        );
        assert_eq!(refined.suggested_max_turns, 85);
        assert_eq!(refined.complexity, "complex");
        assert!(refined.reasoning.contains("Adjusted turn budget"));
    }

    #[test]
    fn test_project_harness_overview_dto_serialization() {
        let overview = ProjectHarnessOverviewDto {
            config: ProjectHarnessConfig::default(),
            tasks: vec![],
            reports: vec![],
            cassettes: vec![],
            active_jobs: vec![],
            history: vec![],
        };
        let json = serde_json::to_string(&overview).unwrap();
        assert!(json.contains("\"config\":"));
        assert!(json.contains("\"tasks\":"));
        assert!(json.contains("\"activeJobs\":"));
        assert!(json.contains("\"reports\":"));
        assert!(json.contains("\"cassettes\":"));
        assert!(json.contains("\"history\":"));

        let deserialized: ProjectHarnessOverviewDto = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tasks.len(), 0);
        assert_eq!(deserialized.active_jobs.len(), 0);
    }

    #[tokio::test]
    async fn test_harness_project_events_broadcast() {
        let mut rx = subscribe_project_events();
        broadcast_project_event(HarnessProjectEvent::TaskStatusChanged {
            project_slug: "my-project".to_string(),
            task_id: "task-1".to_string(),
            job_id: Some("job-123".to_string()),
            status: "running".to_string(),
        });

        let received = rx.recv().await.expect("Failed to receive event");
        match received {
            HarnessProjectEvent::TaskStatusChanged {
                project_slug,
                task_id,
                job_id,
                status,
            } => {
                assert_eq!(project_slug, "my-project");
                assert_eq!(task_id, "task-1");
                assert_eq!(job_id.as_deref(), Some("job-123"));
                assert_eq!(status, "running");
            }
            _ => panic!("Unexpected event received"),
        }
    }

    #[tokio::test]
    async fn test_harness_job_live_events_broadcast() {
        let mut rx = subscribe_job_live_events();
        broadcast_job_live_event(HarnessJobLiveEvent::Finished {
            job_id: "job-456".to_string(),
            status: openduck_harness::types::RunStatus::Success,
            duration_ms: 1200,
            final_answer: Some("Task finished successfully".to_string()),
        });

        let received = rx.recv().await.expect("Failed to receive event");
        match received {
            HarnessJobLiveEvent::Finished {
                job_id,
                status,
                duration_ms,
                final_answer,
            } => {
                assert_eq!(job_id, "job-456");
                assert_eq!(status, openduck_harness::types::RunStatus::Success);
                assert_eq!(duration_ms, 1200);
                assert_eq!(final_answer.as_deref(), Some("Task finished successfully"));
            }
            _ => panic!("Unexpected event received"),
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_history_timeout() {
        // 1. Default fallback is 60s
        let default_config = ProjectHarnessConfig::default();
        assert_eq!(
            resolve_history_timeout(&default_config),
            std::time::Duration::from_secs(60)
        );

        // 2. Project config override
        let mut custom_config = default_config.clone();
        custom_config.execution.history_timeout_seconds = Some(45);
        assert_eq!(
            resolve_history_timeout(&custom_config),
            std::time::Duration::from_secs(45)
        );

        // 3. Env var takes highest priority
        unsafe {
            std::env::set_var("OPENDUCK_HARNESS_HISTORY_TIMEOUT_SECS", "90");
        }
        assert_eq!(
            resolve_history_timeout(&custom_config),
            std::time::Duration::from_secs(90)
        );
        unsafe {
            std::env::remove_var("OPENDUCK_HARNESS_HISTORY_TIMEOUT_SECS");
        }
    }
}
