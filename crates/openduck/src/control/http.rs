use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path as AxumPath, Query, State},
    http::{header, StatusCode},
    response::Response,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};

use super::insight::{inspect_project, ProjectInsight};
use super::jobs::{JobRun, SqliteJobRunStore};
use super::project_roots::ProjectRootsStore;
use super::projects::{
    validate_project_metadata, validate_project_slug, ProjectKind, ProjectMetadata, ProjectStatus,
};
use crate::recipe::validate_recipe::validate_recipe_template_from_content;
use crate::recipe::Recipe;
use crate::scheduler::{get_default_scheduled_recipes_dir, ScheduledJob, SchedulerError};
use crate::session::session_manager::{SessionListFilters, SessionListPageQuery, SessionType};
use crate::session::{Session, SessionManager};
use crate::sources;
use openduck_sdk_types::custom_requests::SourceType;

const PROJECT_SESSION_TYPES: [SessionType; 3] =
    [SessionType::User, SessionType::Scheduled, SessionType::Acp];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub path: String,
    pub working_dirs: Vec<String>,
    pub kind: ProjectKind,
    pub language: Option<String>,
    pub status: ProjectStatus,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email_recipients: Option<Vec<String>>,
    pub notes: String,
    pub source_path: String,
    pub last_activity_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRequest {
    pub slug: String,
    pub title: Option<String>,
    #[serde(default)]
    pub description: String,
    pub path: String,
    pub kind: ProjectKind,
    pub language: Option<String>,
    pub status: ProjectStatus,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub email_recipients: Option<Vec<String>>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub allow_untrusted_path: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub path: Option<String>,
    pub kind: Option<ProjectKind>,
    pub language: Option<String>,
    pub status: Option<ProjectStatus>,
    pub tags: Option<Vec<String>>,
    pub email_recipients: Option<Vec<String>>,
    pub notes: Option<String>,
    #[serde(default)]
    pub allow_untrusted_path: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ProjectListQuery {
    status: Option<ProjectStatus>,
    kind: Option<ProjectKind>,
    tag: Option<String>,
    q: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateJobRequest {
    id: String,
    cron: String,
    recipe: Recipe,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct JobPatch {
    cron: Option<String>,
    paused: Option<bool>,
    recipe: Option<Recipe>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobDto {
    id: String,
    source: String,
    cron: String,
    last_run: Option<String>,
    currently_running: bool,
    paused: bool,
    current_session_id: Option<String>,
    process_start_time: Option<String>,
    project_id: Option<String>,
    working_dir: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionSummary {
    id: String,
    name: String,
    updated_at: String,
    session_type: String,
}

fn bad(error: impl ToString) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": error.to_string()})),
    )
}

fn not_found(error: impl ToString) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": error.to_string()})),
    )
}

fn conflict(body: serde_json::Value) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::CONFLICT, Json(body))
}

fn server_error(error: impl ToString) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": error.to_string()})),
    )
}

fn session_manager(server: &crate::acp::server_factory::AcpServer) -> SessionManager {
    SessionManager::new(server.data_dir().to_path_buf())
}

async fn require_scheduler(
    server: &crate::acp::server_factory::AcpServer,
) -> Result<Arc<dyn crate::scheduler_trait::SchedulerTrait>, (StatusCode, Json<serde_json::Value>)>
{
    server
        .scheduler()
        .await
        .map_err(bad)?
        .ok_or_else(|| bad(anyhow::anyhow!("scheduler is disabled")))
}

fn job_dto(job: &ScheduledJob) -> JobDto {
    JobDto {
        id: job.id.clone(),
        source: job.source.clone(),
        cron: job.cron.clone(),
        last_run: job.last_run.map(|value| value.to_rfc3339()),
        currently_running: job.currently_running,
        paused: job.paused,
        current_session_id: job.current_session_id.clone(),
        process_start_time: job.process_start_time.map(|value| value.to_rfc3339()),
        project_id: job.project_id.clone(),
        working_dir: job.working_dir.clone(),
    }
}

fn project_dto(
    entry: &openduck_sdk_types::custom_requests::SourceEntry,
) -> Result<ProjectDto, String> {
    let dirs: Vec<String> = entry
        .properties
        .get("workingDirs")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let kind = entry
        .properties
        .get("kind")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(ProjectKind::Other);
    let status = entry
        .properties
        .get("status")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(ProjectStatus::Active);
    Ok(ProjectDto {
        slug: entry.name.clone(),
        title: entry
            .properties
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(&entry.name)
            .to_string(),
        description: entry.description.clone(),
        path: dirs.first().cloned().unwrap_or_default(),
        working_dirs: dirs,
        kind,
        language: entry
            .properties
            .get("language")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        status,
        tags: entry
            .properties
            .get("tags")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default(),
        email_recipients: entry
            .properties
            .get("emailRecipients")
            .or_else(|| entry.properties.get("email_recipients"))
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?,
        notes: entry.content.clone(),
        source_path: entry.path.clone(),
        last_activity_at: None,
    })
}

fn properties(req: &ProjectRequest, path: String) -> HashMap<String, serde_json::Value> {
    let mut p = HashMap::new();
    p.insert("workingDirs".into(), serde_json::json!([path]));
    p.insert("kind".into(), serde_json::to_value(&req.kind).unwrap());
    p.insert("status".into(), serde_json::to_value(&req.status).unwrap());
    p.insert("tags".into(), serde_json::to_value(&req.tags).unwrap());
    if let Some(language) = &req.language {
        p.insert("language".into(), serde_json::json!(language));
    }
    if let Some(title) = &req.title {
        p.insert("title".into(), serde_json::json!(title));
    }
    if let Some(recipients) = &req.email_recipients {
        p.insert("emailRecipients".into(), serde_json::json!(recipients));
    }
    p
}

fn session_summary(session: &Session) -> SessionSummary {
    SessionSummary {
        id: session.id.clone(),
        name: session.name.clone(),
        updated_at: session.updated_at.to_rfc3339(),
        session_type: session.session_type.to_string(),
    }
}

async fn project_sessions(
    server: &crate::acp::server_factory::AcpServer,
    slug: &str,
    path: &str,
    limit: usize,
    only_sessions_with_messages: bool,
) -> Result<Vec<Session>, (StatusCode, Json<serde_json::Value>)> {
    let working_dir = PathBuf::from(path);
    session_manager(server)
        .list_sessions_paged(SessionListPageQuery {
            filters: SessionListFilters {
                types: Some(&PROJECT_SESSION_TYPES),
                project_id: Some(slug),
                project_working_dir: (!path.is_empty()).then_some(working_dir.as_path()),
                only_sessions_with_messages,
                exclude_archived: true,
                ..Default::default()
            },
            cursor: None,
            page_size: limit.max(1),
            include_last_message_snippet: false,
        })
        .await
        .map(|page| page.sessions)
        .map_err(server_error)
}

async fn fetch_last_activity(
    server: &crate::acp::server_factory::AcpServer,
    slug: &str,
    path: &str,
) -> Option<String> {
    if let Ok(sessions) = project_sessions(server, slug, path, 1, false).await {
        sessions
            .first()
            .map(|session| session.updated_at.to_rfc3339())
    } else {
        None
    }
}

async fn attach_last_activity(
    server: &crate::acp::server_factory::AcpServer,
    project: &mut ProjectDto,
) {
    project.last_activity_at = fetch_last_activity(server, &project.slug, &project.path).await;
}

async fn job_store(
    server: &crate::acp::server_factory::AcpServer,
) -> Result<SqliteJobRunStore, (StatusCode, Json<serde_json::Value>)> {
    SqliteJobRunStore::open(server.data_dir().join("control.db"))
        .await
        .map_err(server_error)
}

fn validate_job_id(id: &str) -> Result<(), String> {
    let is_valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ');
    if is_valid {
        Ok(())
    } else {
        Err(
            "Job id must use only alphanumeric characters, hyphens, underscores, or spaces"
                .to_string(),
        )
    }
}

fn validate_job_recipe(recipe: &Recipe) -> Result<(), String> {
    if recipe.check_for_security_warnings() {
        return Err(
            "This recipe contains hidden characters that could be malicious. Please remove them before trying to save."
                .to_string(),
        );
    }
    let yaml = recipe.to_yaml().map_err(|error| error.to_string())?;
    validate_recipe_template_from_content(&yaml, None).map_err(|error| error.to_string())?;
    Ok(())
}

fn scheduler_error(error: SchedulerError) -> (StatusCode, Json<serde_json::Value>) {
    match error {
        SchedulerError::JobNotFound(_) => not_found(error),
        SchedulerError::JobIdExists(id) => conflict(serde_json::json!({
            "error": "job_id_exists",
            "jobId": id
        })),
        SchedulerError::CronParseError(message) => {
            bad(format!("Invalid cron expression: {message}"))
        }
        SchedulerError::RecipeLoadError(message) => bad(format!("Recipe load error: {message}")),
        SchedulerError::AnyhowError(error)
            if error.to_string().contains("already running")
                || error.to_string().contains("not running")
                || error.to_string().contains("Cannot ") =>
        {
            conflict(serde_json::json!({"error": error.to_string()}))
        }
        other => server_error(other),
    }
}

async fn list_projects(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    Query(query): Query<ProjectListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let entries = sources::list_sources(Some(SourceType::Project), None, false).map_err(bad)?;
    let mut projects = entries
        .iter()
        .map(project_dto)
        .collect::<Result<Vec<_>, _>>()
        .map_err(bad)?;
    if let Some(status) = query.status {
        projects.retain(|project| project.status == status);
    }
    if let Some(kind) = query.kind {
        projects.retain(|project| project.kind == kind);
    }
    if let Some(tag) = query.tag.as_deref() {
        projects.retain(|project| project.tags.iter().any(|value| value == tag));
    }
    if let Some(q) = query.q.as_deref().map(str::to_lowercase) {
        projects.retain(|project| {
            project.slug.to_lowercase().contains(&q)
                || project.title.to_lowercase().contains(&q)
                || project.description.to_lowercase().contains(&q)
                || project.path.to_lowercase().contains(&q)
        });
    }
    let mut futures = Vec::with_capacity(projects.len());
    for project in &projects {
        let server_ref = server.clone();
        let slug = project.slug.clone();
        let path = project.path.clone();
        futures.push(async move { fetch_last_activity(&server_ref, &slug, &path).await });
    }
    let last_activities = futures::future::join_all(futures).await;
    for (project, last_activity) in projects.iter_mut().zip(last_activities) {
        project.last_activity_at = last_activity;
    }
    Ok(Json(serde_json::json!({"projects": projects})))
}

async fn create_project(
    Json(req): Json<ProjectRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    validate_project_slug(&req.slug).map_err(bad)?;
    let path = ProjectRootsStore::from_config()
        .require_child_of_root(Path::new(&req.path))
        .map_err(bad)?
        .to_string_lossy()
        .into_owned();
    validate_project_metadata(&ProjectMetadata {
        working_dirs: vec![path.clone()],
        kind: req.kind.clone(),
        status: req.status.clone(),
        language: req.language.clone(),
        tags: req.tags.clone(),
        email_recipients: req.email_recipients.clone(),
    })
    .map_err(bad)?;
    let entry = sources::create_source(
        SourceType::Project,
        &req.slug,
        &req.description,
        &req.notes,
        true,
        None,
        properties(&req, path),
    )
    .map_err(bad)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"project": project_dto(&entry).map_err(bad)?})),
    ))
}

#[derive(Debug, Deserialize, Default)]
struct GetProjectQuery {
    #[serde(default)]
    lazy: Option<bool>,
}

async fn get_project(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<GetProjectQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let mut project = project_dto(&entry).map_err(bad)?;

    if query.lazy.unwrap_or(false) {
        attach_last_activity(&server, &mut project).await;
        return Ok(Json(serde_json::json!({
            "project": project,
            "insight": ProjectInsight::empty(),
            "recents": {
                "sessions": [],
                "jobRuns": []
            }
        })));
    }

    let activity_fut = fetch_last_activity(&server, &slug, &project.path);
    let insight_fut = inspect_project(Path::new(&project.path));
    let sessions_fut = project_sessions(&server, &slug, &project.path, 10, true);
    let jobs_fut = async {
        match job_store(&server).await {
            Ok(store) => store.runs_for_project(&slug, 10).await.unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    };

    let (last_activity, insight, sessions_res, job_runs) =
        tokio::join!(activity_fut, insight_fut, sessions_fut, jobs_fut);
    project.last_activity_at = last_activity;
    let recents_sessions = sessions_res?
        .iter()
        .map(session_summary)
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({
        "project": project,
        "insight": insight,
        "recents": {
            "sessions": recents_sessions,
            "jobRuns": job_runs
        }
    })))
}

async fn get_project_insight(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;

    let insight_fut = inspect_project(Path::new(&project.path));
    let sessions_fut = project_sessions(&server, &slug, &project.path, 10, true);
    let jobs_fut = async {
        match job_store(&server).await {
            Ok(store) => store.runs_for_project(&slug, 10).await.unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    };

    let (insight, sessions_res, job_runs) = tokio::join!(insight_fut, sessions_fut, jobs_fut);
    let recents_sessions = sessions_res?
        .iter()
        .map(session_summary)
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({
        "insight": insight,
        "recents": {
            "sessions": recents_sessions,
            "jobRuns": job_runs
        }
    })))
}

async fn patch_project(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(slug): AxumPath<String>,
    Json(patch): Json<ProjectPatch>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let mut dirs: Vec<String> = entry
        .properties
        .get("workingDirs")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(bad)?
        .unwrap_or_default();
    let mut path_changed = false;
    if let Some(path) = patch.path {
        dirs = vec![ProjectRootsStore::from_config()
            .require_child_of_root(Path::new(&path))
            .map_err(bad)?
            .to_string_lossy()
            .into_owned()];
        path_changed = true;
    }
    let kind = patch
        .kind
        .or_else(|| {
            entry
                .properties
                .get("kind")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
        })
        .unwrap_or(ProjectKind::Other);
    let status = patch
        .status
        .or_else(|| {
            entry
                .properties
                .get("status")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
        })
        .unwrap_or(ProjectStatus::Active);
    let tags = patch
        .tags
        .or_else(|| {
            entry
                .properties
                .get("tags")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
        })
        .unwrap_or_default();
    let language = patch.language.or_else(|| {
        entry
            .properties
            .get("language")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
    });
    let email_recipients = patch.email_recipients.or_else(|| {
        entry
            .properties
            .get("emailRecipients")
            .or_else(|| entry.properties.get("email_recipients"))
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
    });
    validate_project_metadata(&ProjectMetadata {
        working_dirs: dirs.clone(),
        kind: kind.clone(),
        status: status.clone(),
        language: language.clone(),
        tags: tags.clone(),
        email_recipients: email_recipients.clone(),
    })
    .map_err(bad)?;
    let mut props = entry.properties.clone();
    props.insert("workingDirs".into(), serde_json::json!(dirs));
    props.insert("kind".into(), serde_json::to_value(kind).unwrap());
    props.insert("status".into(), serde_json::to_value(status).unwrap());
    props.insert("tags".into(), serde_json::to_value(tags).unwrap());
    if let Some(l) = language {
        props.insert("language".into(), serde_json::json!(l));
    }
    if let Some(t) = patch.title {
        props.insert("title".into(), serde_json::json!(t));
    }
    if let Some(recipients) = email_recipients {
        props.insert("emailRecipients".into(), serde_json::json!(recipients));
    }
    let description = patch.description.unwrap_or(entry.description);
    let content = patch.notes.unwrap_or(entry.content);
    let updated = sources::update_source_with_roots(
        SourceType::Project,
        &entry.path,
        &slug,
        &description,
        &content,
        crate::sources::UpdateSourceOptions {
            properties: Some(props),
            additional_roots: &[],
        },
    )
    .map_err(bad)?;
    if path_changed {
        super::project_harness::invalidate_project_root_cache(&slug);
        if let Some(path) = updated.properties.get("path").and_then(|v| v.as_str()) {
            require_scheduler(&server)
                .await?
                .rewrite_project_working_dir(&slug, path)
                .await
                .map_err(server_error)?;
        }
    }
    Ok(Json(
        serde_json::json!({"project": project_dto(&updated).map_err(bad)?}),
    ))
}

async fn delete_project(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(slug): AxumPath<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let job_ids = require_scheduler(&server)
        .await?
        .list_scheduled_jobs()
        .await
        .into_iter()
        .filter(|job| job.project_id.as_deref() == Some(slug.as_str()))
        .map(|job| job.id)
        .collect::<Vec<_>>();
    if !job_ids.is_empty() {
        return Err(conflict(serde_json::json!({
            "error": "project_has_jobs",
            "jobIds": job_ids
        })));
    }
    sources::delete_source(SourceType::Project, &entry.path).map_err(bad)?;
    super::project_harness::invalidate_project_root_cache(&slug);
    Ok(StatusCode::NO_CONTENT)
}

async fn list_project_sessions(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let project = project_dto(&sources::read_project(&slug).map_err(not_found)?).map_err(bad)?;
    let sessions = project_sessions(
        &server,
        &slug,
        &project.path,
        query.limit.unwrap_or(50),
        true,
    )
    .await?
    .iter()
    .map(session_summary)
    .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({"sessions": sessions})))
}

async fn list_jobs(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let jobs = require_scheduler(&server)
        .await?
        .list_scheduled_jobs()
        .await
        .iter()
        .map(job_dto)
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({"jobs": jobs})))
}

async fn list_project_jobs(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if sources::read_project(&slug).is_err() {
        return Err(not_found(format!("Project '{slug}' not found")));
    }
    let jobs = require_scheduler(&server)
        .await?
        .list_scheduled_jobs()
        .await
        .into_iter()
        .filter(|job| job.project_id.as_deref() == Some(slug.as_str()))
        .map(|job| job_dto(&job))
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({"jobs": jobs})))
}

async fn create_project_job(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<CreateJobRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let project = project_dto(&sources::read_project(&slug).map_err(not_found)?).map_err(bad)?;
    validate_job_id(&req.id).map_err(bad)?;
    validate_job_recipe(&req.recipe).map_err(bad)?;
    let scheduled_recipes_dir = get_default_scheduled_recipes_dir().map_err(bad)?;
    let recipe_path = scheduled_recipes_dir.join(format!("{}.yaml", req.id));
    let yaml = req.recipe.to_yaml().map_err(bad)?;
    tokio::fs::write(&recipe_path, yaml).await.map_err(bad)?;
    let job = ScheduledJob {
        id: req.id,
        source: recipe_path.to_string_lossy().into_owned(),
        cron: req.cron,
        last_run: None,
        currently_running: false,
        paused: false,
        current_session_id: None,
        process_start_time: None,
        parameters: vec![],
        recipe_base_dir: None,
        project_id: Some(slug),
        working_dir: Some(project.path),
    };
    require_scheduler(&server)
        .await?
        .add_scheduled_job(job.clone(), false)
        .await
        .map_err(scheduler_error)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"job": job_dto(&job)})),
    ))
}

async fn find_job(
    server: &crate::acp::server_factory::AcpServer,
    job_id: &str,
) -> Result<ScheduledJob, (StatusCode, Json<serde_json::Value>)> {
    require_scheduler(server)
        .await?
        .list_scheduled_jobs()
        .await
        .into_iter()
        .find(|job| job.id == job_id)
        .ok_or_else(|| not_found(format!("Job '{job_id}' not found")))
}

async fn union_job_runs(
    server: &crate::acp::server_factory::AcpServer,
    job_id: &str,
    limit: usize,
) -> Result<Vec<JobRun>, (StatusCode, Json<serde_json::Value>)> {
    let mut rows = match job_store(server).await {
        Ok(store) => store
            .runs_for_job(job_id, limit as i64)
            .await
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let known_sessions = rows
        .iter()
        .filter_map(|run| run.session_id.clone())
        .collect::<std::collections::HashSet<_>>();
    if let Ok(Some(sessions)) = server.scheduler().await {
        if let Ok(schedule_sessions) = sessions.sessions(job_id, limit).await {
            for (session_id, session) in schedule_sessions {
                if known_sessions.contains(&session_id) {
                    continue;
                }
                rows.push(JobRun {
                    id: session_id.clone(),
                    job_id: job_id.to_string(),
                    project_id: session.project_id,
                    session_id: Some(session_id),
                    trigger: "cron".to_string(),
                    status: "succeeded".to_string(),
                    started_at: session.created_at.to_rfc3339(),
                    finished_at: Some(session.updated_at.to_rfc3339()),
                    error: None,
                });
            }
        }
    }
    rows.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    rows.truncate(limit);
    Ok(rows)
}

async fn get_job(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let job = find_job(&server, &job_id).await?;
    let runs = union_job_runs(&server, &job_id, 50).await?;
    Ok(Json(
        serde_json::json!({"job": job_dto(&job), "runs": runs}),
    ))
}

async fn patch_job(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(job_id): AxumPath<String>,
    Json(patch): Json<JobPatch>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let scheduler = require_scheduler(&server).await?;
    find_job(&server, &job_id).await?;
    if let Some(cron) = patch.cron {
        scheduler
            .update_schedule(&job_id, cron)
            .await
            .map_err(scheduler_error)?;
    }
    if let Some(paused) = patch.paused {
        let result = if paused {
            scheduler.pause_schedule(&job_id).await
        } else {
            scheduler.unpause_schedule(&job_id).await
        };
        result.map_err(scheduler_error)?;
    }
    if let Some(recipe) = patch.recipe {
        validate_job_recipe(&recipe).map_err(bad)?;
        scheduler
            .update_job_recipe(&job_id, recipe)
            .await
            .map_err(scheduler_error)?;
    }
    let job = find_job(&server, &job_id).await?;
    Ok(Json(serde_json::json!({"job": job_dto(&job)})))
}

async fn delete_job(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    require_scheduler(&server)
        .await?
        .remove_scheduled_job(&job_id, true)
        .await
        .map_err(scheduler_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn run_job(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let result = require_scheduler(&server)
        .await?
        .start_now(&job_id)
        .await
        .map_err(scheduler_error)?;
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!(result))))
}

async fn kill_job(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    require_scheduler(&server)
        .await?
        .kill_running_job(&job_id)
        .await
        .map_err(scheduler_error)?;
    Ok(Json(serde_json::json!({"message": "cancelled"})))
}

async fn list_job_runs(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(job_id): AxumPath<String>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    find_job(&server, &job_id).await?;
    let runs = union_job_runs(&server, &job_id, query.limit.unwrap_or(50)).await?;
    Ok(Json(serde_json::json!({"runs": runs})))
}

async fn get_run(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(run_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let run = job_store(&server)
        .await?
        .get_run(&run_id)
        .await
        .map_err(server_error)?
        .ok_or_else(|| not_found(format!("Run '{run_id}' not found")))?;
    Ok(Json(
        serde_json::json!({"run": run, "sessionId": run.session_id}),
    ))
}

async fn list_session_messages(
    State(server): State<Arc<crate::acp::server_factory::AcpServer>>,
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let session = session_manager(&server)
        .get_session(&session_id, true)
        .await
        .map_err(|_| not_found(format!("Session '{session_id}' not found")))?;
    let limit = query.limit.unwrap_or(200);
    let messages = session
        .conversation
        .map(|conversation| conversation.messages().to_vec())
        .unwrap_or_default()
        .into_iter()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|message| {
            let text = message
                .content
                .iter()
                .filter_map(|content| content.as_text())
                .collect::<Vec<_>>()
                .join("\n");
            let created = Utc
                .timestamp_opt(message.created, 0)
                .single()
                .unwrap_or_else(Utc::now);
            let role = match message.role {
                rmcp::model::Role::User => "user",
                rmcp::model::Role::Assistant => "assistant",
            };
            serde_json::json!({
                "role": role,
                "text": text.chars().take(4000).collect::<String>(),
                "createdAt": created.to_rfc3339()
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({"messages": messages})))
}

#[derive(Debug, Deserialize, Default)]
struct FilePathQuery {
    path: Option<String>,
}

async fn list_project_files(
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;
    let rel_path = query.path.as_deref().unwrap_or("");
    let result = super::files::list_files(Path::new(&project.path), rel_path)
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

async fn search_project_files(
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<super::files::FileSearchQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;
    let q = query.q.as_deref().unwrap_or("");
    let limit = query.limit.unwrap_or(50);
    let result = super::files::search_files(Path::new(&project.path), q, limit)
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

async fn get_project_file_content(
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;
    let rel_path = query.path.as_deref().unwrap_or("");
    let result = super::files::read_file_content(Path::new(&project.path), rel_path)
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

async fn put_project_file_content(
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<super::files::WriteFileRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;
    super::files::write_file_content(Path::new(&project.path), &req.path, &req.content)
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::json!({"success": true, "path": req.path})))
}

async fn get_project_file_bytes(
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<FilePathQuery>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let rel_path = query
        .path
        .as_deref()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| bad("path query parameter is required"))?;
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;
    let (bytes, mime_type) = super::files::read_file_bytes(Path::new(&project.path), rel_path)
        .await
        .map_err(bad)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_DISPOSITION, "inline")
        .body(Body::from(bytes))
        .map_err(|e| bad(e.to_string()))
}

async fn put_project_file_bytes(
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<FilePathQuery>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let rel_path = query
        .path
        .as_deref()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| bad("path query parameter is required"))?;
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;
    super::files::write_file_bytes(Path::new(&project.path), rel_path, &body)
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::json!({"success": true, "path": rel_path})))
}

async fn create_project_file(
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<super::files::CreateFileRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;
    super::files::create_entry(
        Path::new(&project.path),
        &req.path,
        &req.kind,
        req.content.as_deref(),
    )
    .await
    .map_err(bad)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"success": true, "path": req.path})),
    ))
}

async fn delete_project_file(
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<FilePathQuery>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;
    let rel_path = query.path.as_deref().unwrap_or("");
    super::files::delete_entry(Path::new(&project.path), rel_path)
        .await
        .map_err(bad)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn rename_project_file(
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<super::files::RenameFileRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;
    super::files::rename_entry(Path::new(&project.path), &req.old_path, &req.new_path)
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::json!({
        "success": true,
        "oldPath": req.old_path,
        "newPath": req.new_path
    })))
}

async fn exec_project_terminal(
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<super::terminal::ExecCommandRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(&slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;
    let result = super::terminal::execute_project_command(Path::new(&project.path), req)
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

fn project_dir(slug: &str) -> Result<PathBuf, (StatusCode, Json<serde_json::Value>)> {
    let entry = sources::read_project(slug).map_err(not_found)?;
    let project = project_dto(&entry).map_err(bad)?;
    Ok(PathBuf::from(project.path))
}

async fn git_status(
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = project_dir(&slug)?;
    let result = super::git::status(&path).await.map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

async fn git_log(
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<super::git::GitLogQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = project_dir(&slug)?;
    let result = super::git::log(&path, query.limit).await.map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

async fn git_diff(
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<super::git::GitDiffQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = project_dir(&slug)?;
    let rel = query.path.as_deref().unwrap_or("");
    let result = super::git::diff(&path, rel, query.staged.unwrap_or(false))
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

async fn git_show(
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<super::git::GitShowQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = project_dir(&slug)?;
    let result = super::git::show(&path, &query.sha).await.map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

async fn git_stage(
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<super::git::GitPathsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = project_dir(&slug)?;
    let result = super::git::stage(&path, &req.paths).await.map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

async fn git_unstage(
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<super::git::GitPathsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = project_dir(&slug)?;
    let result = super::git::unstage(&path, &req.paths).await.map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

async fn git_discard(
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<super::git::GitPathsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = project_dir(&slug)?;
    let result = super::git::discard(&path, &req.paths).await.map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

async fn git_commit(
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<super::git::GitCommitRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = project_dir(&slug)?;
    let result = super::git::commit(&path, &req.message, &req.paths)
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

async fn git_commit_message(
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<super::git::GitCommitMessageRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = project_dir(&slug)?;
    let result = super::git::suggest_commit_message(&path, &req.paths)
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::to_value(result).map_err(bad)?))
}

fn project_root_json(root: &super::project_roots::ProjectRoot) -> serde_json::Value {
    serde_json::json!({
        "path": root.path.to_string_lossy(),
        "name": root.name,
        "available": root.available,
    })
}

fn project_directory_json(directory: &super::project_roots::ProjectDirectory) -> serde_json::Value {
    serde_json::json!({
        "path": directory.path.to_string_lossy(),
        "name": directory.name,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddProjectRootRequest {
    path: String,
    #[serde(default)]
    allow_untrusted_path: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProjectDirectoryRequest {
    root: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct RootPathQuery {
    path: String,
}

#[derive(Debug, Deserialize)]
struct RootDirectoryQuery {
    root: String,
}

async fn list_project_roots(
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let roots = ProjectRootsStore::from_config().list().map_err(bad)?;
    Ok(Json(serde_json::json!({
        "roots": roots.iter().map(project_root_json).collect::<Vec<_>>()
    })))
}

async fn add_project_root(
    Json(req): Json<AddProjectRootRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = req.path.trim();
    if path.is_empty() {
        return Err(bad("enter an absolute directory path"));
    }
    let root = ProjectRootsStore::from_config()
        .add(Path::new(path), req.allow_untrusted_path)
        .map_err(bad)?;
    Ok(Json(
        serde_json::json!({ "root": project_root_json(&root) }),
    ))
}

async fn remove_project_root(
    Query(query): Query<RootPathQuery>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let path = query.path.trim();
    if path.is_empty() {
        return Err(bad("enter the project root to remove"));
    }
    ProjectRootsStore::from_config()
        .remove(Path::new(path))
        .map_err(bad)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_project_root_directories(
    Query(query): Query<RootDirectoryQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let root = query.root.trim();
    if root.is_empty() {
        return Err(bad("choose a project root"));
    }
    let listing = ProjectRootsStore::from_config()
        .list_directories(Path::new(root))
        .map_err(bad)?;
    Ok(Json(serde_json::json!({
        "root": listing.root.to_string_lossy(),
        "directories": listing.directories.iter().map(project_directory_json).collect::<Vec<_>>(),
        "truncated": listing.truncated,
    })))
}

async fn create_project_root_directory(
    Json(req): Json<CreateProjectDirectoryRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let root = req.root.trim();
    let name = req.name.trim();
    if root.is_empty() {
        return Err(bad("choose a project root"));
    }
    if name.is_empty() {
        return Err(bad("enter a folder name"));
    }
    let created = ProjectRootsStore::from_config()
        .create_directory(Path::new(root), name)
        .map_err(bad)?;
    Ok(Json(serde_json::json!({
        "directory": project_directory_json(&created.directory),
        "created": created.created,
    })))
}

pub fn routes(server: Arc<crate::acp::server_factory::AcpServer>) -> Router {
    tokio::spawn(async {
        match super::task_scheduler::global().await {
            Ok(scheduler) => {
                if let Err(error) = scheduler.reload_all().await {
                    tracing::error!(%error, "Failed to load scheduled project tasks");
                }
            }
            Err(error) => {
                tracing::error!(%error, "Failed to start project task scheduler");
            }
        }
    });
    Router::new()
        .route(
            "/api/v1/project-roots",
            get(list_project_roots)
                .post(add_project_root)
                .delete(remove_project_root),
        )
        .route(
            "/api/v1/project-roots/directories",
            get(list_project_root_directories).post(create_project_root_directory),
        )
        .route("/api/v1/projects", get(list_projects).post(create_project))
        .route(
            "/api/v1/projects/{slug}",
            get(get_project).patch(patch_project).delete(delete_project),
        )
        .route("/api/v1/projects/{slug}/insight", get(get_project_insight))
        .route(
            "/api/v1/projects/{slug}/sessions",
            get(list_project_sessions),
        )
        .route(
            "/api/v1/projects/{slug}/jobs",
            get(list_project_jobs).post(create_project_job),
        )
        .route(
            "/api/v1/projects/{slug}/files",
            get(list_project_files).delete(delete_project_file),
        )
        .route(
            "/api/v1/projects/{slug}/files/search",
            get(search_project_files),
        )
        .route(
            "/api/v1/projects/{slug}/files/content",
            get(get_project_file_content).put(put_project_file_content),
        )
        .route(
            "/api/v1/projects/{slug}/files/bytes",
            get(get_project_file_bytes)
                .put(put_project_file_bytes)
                .layer(DefaultBodyLimit::max(super::files::MAX_FILE_SIZE_BYTES)),
        )
        .route(
            "/api/v1/projects/{slug}/files/create",
            post(create_project_file),
        )
        .route(
            "/api/v1/projects/{slug}/files/rename",
            post(rename_project_file),
        )
        .route(
            "/api/v1/projects/{slug}/terminal/exec",
            post(exec_project_terminal),
        )
        .route("/api/v1/projects/{slug}/git/status", get(git_status))
        .route("/api/v1/projects/{slug}/git/log", get(git_log))
        .route("/api/v1/projects/{slug}/git/diff", get(git_diff))
        .route("/api/v1/projects/{slug}/git/show", get(git_show))
        .route("/api/v1/projects/{slug}/git/stage", post(git_stage))
        .route("/api/v1/projects/{slug}/git/unstage", post(git_unstage))
        .route("/api/v1/projects/{slug}/git/discard", post(git_discard))
        .route("/api/v1/projects/{slug}/git/commit", post(git_commit))
        .route(
            "/api/v1/projects/{slug}/git/commit-message",
            post(git_commit_message),
        )
        .route(
            "/api/v1/projects/{slug}/harness/overview",
            get(super::project_harness::get_project_harness_overview),
        )
        .route(
            "/api/v1/projects/{slug}/harness/config",
            get(super::project_harness::get_project_harness_config)
                .put(super::project_harness::update_project_harness_config),
        )
        .route(
            "/api/v1/projects/{slug}/harness/tasks",
            get(super::project_harness::list_project_tasks)
                .post(super::project_harness::create_project_task),
        )
        .route(
            "/api/v1/projects/{slug}/harness/tasks/estimate",
            post(super::project_harness::estimate_project_task),
        )
        .route(
            "/api/v1/projects/{slug}/harness/tasks/{task_id}",
            get(super::project_harness::get_project_task)
                .put(super::project_harness::update_project_task)
                .delete(super::project_harness::delete_project_task),
        )
        .route(
            "/api/v1/projects/{slug}/harness/tasks/{task_id}/run",
            post(super::project_harness::run_project_task),
        )
        .route(
            "/api/v1/projects/{slug}/harness/tasks/{task_id}/stop",
            post(super::project_harness::stop_project_task),
        )
        .route(
            "/api/v1/projects/{slug}/harness/tasks/{task_id}/kill",
            post(super::project_harness::stop_project_task),
        )
        .route(
            "/api/v1/projects/{slug}/harness/tasks/{task_id}/schedule",
            patch(super::project_harness::patch_project_task_schedule),
        )
        .route(
            "/api/v1/projects/{slug}/harness/eval",
            post(super::project_harness::eval_project_tasks),
        )
        .route(
            "/api/v1/projects/{slug}/harness/reports",
            get(super::project_harness::list_project_reports),
        )
        .route(
            "/api/v1/projects/{slug}/harness/reports/{report_id}",
            get(super::project_harness::get_project_report),
        )
        .route(
            "/api/v1/projects/{slug}/harness/cassettes",
            get(super::project_harness::list_project_cassettes),
        )
        .route(
            "/api/v1/projects/{slug}/harness/events",
            get(super::project_harness::stream_project_harness_events),
        )
        .route(
            "/api/v1/projects/{slug}/harness/jobs",
            get(super::project_harness::list_project_active_jobs),
        )
        .route(
            "/api/v1/projects/{slug}/harness/jobs/{job_id}",
            get(super::project_harness::inspect_project_harness_job),
        )
        .route(
            "/api/v1/projects/{slug}/harness/jobs/{job_id}/stream",
            get(super::project_harness::stream_harness_job_live),
        )
        .route(
            "/api/v1/projects/{slug}/harness/jobs/{job_id}/stop",
            post(super::project_harness::stop_project_harness_job),
        )
        .route(
            "/api/v1/projects/{slug}/harness/jobs/{job_id}/kill",
            post(super::project_harness::stop_project_harness_job),
        )
        .route(
            "/api/v1/projects/{slug}/harness/history",
            get(super::project_harness::list_project_history),
        )
        .route(
            "/api/v1/projects/{slug}/harness/history/{run_id}",
            get(super::project_harness::get_project_history_detail),
        )
        .route("/api/v1/jobs", get(list_jobs))
        .route(
            "/api/v1/jobs/{job_id}",
            get(get_job).patch(patch_job).delete(delete_job),
        )
        .route("/api/v1/jobs/{job_id}/run", post(run_job))
        .route("/api/v1/jobs/{job_id}/kill", post(kill_job))
        .route("/api/v1/jobs/{job_id}/runs", get(list_job_runs))
        .route("/api/v1/runs/{run_id}", get(get_run))
        .route(
            "/api/v1/sessions/{session_id}/messages",
            get(list_session_messages),
        )
        .route(
            "/api/v1/harness/jobs",
            get(super::harness::list_active_harness_jobs),
        )
        .route(
            "/api/v1/harness/jobs/{job_id}",
            get(super::harness::inspect_harness_job),
        )
        .route(
            "/api/v1/harness/jobs/{job_id}/stop",
            post(super::harness::stop_active_harness_job),
        )
        .route(
            "/api/v1/harness/jobs/{job_id}/kill",
            post(super::harness::stop_active_harness_job),
        )
        .route(
            "/api/v1/harness/history",
            get(super::harness::list_harness_history),
        )
        .route(
            "/api/v1/harness/history/{run_id}",
            get(super::harness::get_harness_history_detail),
        )
        .route(
            "/api/v1/harness/overview",
            get(super::harness::get_harness_overview),
        )
        .route(
            "/api/v1/harness/reports",
            get(super::harness::list_harness_reports),
        )
        .route(
            "/api/v1/harness/reports/{report_id}",
            get(super::harness::get_harness_report),
        )
        .route(
            "/api/v1/harness/cassettes",
            get(super::harness::list_harness_cassettes),
        )
        .route(
            "/api/v1/harness/cassettes/{cassette_name}",
            get(super::harness::get_harness_cassette),
        )
        .route(
            "/api/v1/harness/eval",
            post(super::harness::run_harness_eval),
        )
        .route(
            "/api/v1/harness/run",
            post(super::harness::run_harness_task),
        )
        .route(
            "/api/v1/harness/replay",
            post(super::harness::run_harness_replay),
        )
        .with_state(server)
}
