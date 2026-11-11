use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path as AxumPath, Query};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use futures::stream::Stream;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use super::models::{AccountRecord, AgentProfileRecord, QuotaSnapshotRecord};
use super::service::TuiManagementService;
use openduck_mcp::tui_session::{AgentCliKind, AgentModelCatalogEntry};

fn bad(err: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
}

fn not_found(err: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsQuery {
    pub project_id: Option<String>,
    pub run_id: Option<String>,
    pub active_only: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListOperationsQuery {
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub project_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSessionBody {
    pub target_run_id: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendSessionBody {
    pub additional_mins: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopSessionBody {
    pub reason: Option<String>,
}

pub async fn list_tui_agents(
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let profiles = service.store.list_profiles().await.map_err(bad)?;
    Ok(Json(serde_json::json!({ "agents": profiles })))
}

pub async fn create_or_update_tui_agent(
    Json(mut req): Json<AgentProfileRecord>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    if req.id.is_empty() {
        req.id = format!("prof_{}", uuid::Uuid::new_v4());
    }
    let now = Utc::now().to_rfc3339();
    if req.created_at.is_empty() {
        req.created_at = now.clone();
    }
    req.updated_at = now;
    service.store.upsert_profile(&req).await.map_err(bad)?;
    Ok(Json(serde_json::json!({ "success": true, "agent": req })))
}

pub async fn get_tui_agent(
    AxumPath(agent_kind): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let profile = service
        .store
        .get_profile_by_agent(&agent_kind)
        .await
        .map_err(bad)?;
    let p =
        profile.ok_or_else(|| not_found(format!("Profile for agent '{agent_kind}' not found")))?;
    Ok(Json(serde_json::json!({ "agent": p })))
}

pub async fn patch_tui_agent(
    AxumPath(agent_kind): AxumPath<String>,
    Json(updates): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let mut profile = service
        .store
        .get_profile_by_agent(&agent_kind)
        .await
        .map_err(bad)?
        .unwrap_or_else(|| AgentProfileRecord {
            id: format!("prof_{}", uuid::Uuid::new_v4()),
            agent_kind: agent_kind.clone(),
            default_model: None,
            default_effort: None,
            account_id: None,
            proxy_mode: "inherit".to_string(),
            proxy_id: None,
            max_concurrency: None,
            idle_timeout_mins: None,
            exited_retention_mins: None,
            max_turn_timeout_secs: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        });

    if let Some(m) = updates.get("defaultModel").and_then(|v| v.as_str()) {
        profile.default_model = Some(m.to_string());
    }
    if let Some(e) = updates.get("defaultEffort").and_then(|v| v.as_str()) {
        profile.default_effort = Some(e.to_string());
    }
    if let Some(acc) = updates.get("accountId").and_then(|v| v.as_str()) {
        profile.account_id = Some(acc.to_string());
    }
    if let Some(pm) = updates.get("proxyMode").and_then(|v| v.as_str()) {
        profile.proxy_mode = pm.to_string();
    }
    if let Some(pi_val) = updates.get("proxyId") {
        if pi_val.is_null() {
            profile.proxy_id = None;
        } else if let Some(pi) = pi_val.as_str() {
            profile.proxy_id = if pi.trim().is_empty() {
                None
            } else {
                Some(pi.trim().to_string())
            };
        }
    }
    if let Some(c) = updates.get("maxConcurrency").and_then(|v| v.as_u64()) {
        profile.max_concurrency = Some(c as u32);
    }
    if let Some(t) = updates.get("idleTimeoutMins").and_then(|v| v.as_u64()) {
        profile.idle_timeout_mins = Some(t);
    }
    if let Some(r) = updates.get("exitedRetentionMins").and_then(|v| v.as_u64()) {
        profile.exited_retention_mins = Some(r);
    }
    profile.updated_at = Utc::now().to_rfc3339();

    service.store.upsert_profile(&profile).await.map_err(bad)?;
    Ok(Json(
        serde_json::json!({ "success": true, "agent": profile }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAgentModelsQuery {
    pub refresh: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchSessionModelBody {
    pub model: String,
}

async fn resolve_agent_kind(
    service: &TuiManagementService,
    agent_or_id: &str,
) -> Result<AgentCliKind, (StatusCode, Json<serde_json::Value>)> {
    match agent_or_id.parse::<AgentCliKind>() {
        Ok(k) => Ok(k),
        Err(_) => {
            if let Ok(Some(profile)) = service.store.get_profile_by_agent(agent_or_id).await {
                profile.agent_kind.parse::<AgentCliKind>().map_err(bad)
            } else {
                Err(bad(format!(
                    "Unsupported agent CLI kind or profile '{agent_or_id}'"
                )))
            }
        }
    }
}

fn catalog_to_json(entry: &AgentModelCatalogEntry) -> serde_json::Value {
    serde_json::json!({
        "agent": entry.agent.to_string(),
        "models": entry.models,
        "defaultModel": entry.default_model,
        "default_model": entry.default_model,
        "supportsFreeText": entry.supports_free_text,
        "supports_free_text": entry.supports_free_text,
        "isFallback": entry.is_fallback,
        "is_fallback": entry.is_fallback,
        "cachedAtUnixSecs": entry.cached_at_unix_secs,
        "cached_at_unix_secs": entry.cached_at_unix_secs,
        "catalog": entry,
    })
}

pub async fn get_tui_agent_models(
    AxumPath(agent): AxumPath<String>,
    Query(query): Query<GetAgentModelsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let kind = resolve_agent_kind(&service, &agent).await?;
    let entry = if query.refresh.unwrap_or(false) {
        service.catalog.refresh(kind).await
    } else {
        service.catalog.get_models(kind).await
    };
    Ok(Json(catalog_to_json(&entry)))
}

pub async fn refresh_tui_agent_models(
    AxumPath(agent): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let kind = resolve_agent_kind(&service, &agent).await?;
    let entry = service.catalog.refresh(kind).await;
    Ok(Json(catalog_to_json(&entry)))
}

pub async fn list_tui_sessions(
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let limit = query.limit.unwrap_or(100);
    let active_only = query.active_only.unwrap_or(false);

    let sessions = if let Some(ref rid) = query.run_id {
        service
            .store
            .list_sessions_for_run(rid)
            .await
            .map_err(bad)?
    } else {
        service
            .store
            .list_sessions(query.project_id.as_deref(), active_only, limit)
            .await
            .map_err(bad)?
    };

    let mut enriched = Vec::with_capacity(sessions.len());
    for mut s in sessions {
        if let Ok(handle) = service.session_manager.get_session(&s.session_name).await {
            if let Some(active) = handle.active_model().await {
                s.active_model = Some(active.clone());
                s.current_model = Some(active);
            }
        }
        if s.active_model.is_none() {
            s.active_model = s.model.clone();
            s.current_model = s.model.clone();
        }
        enriched.push(s);
    }

    Ok(Json(serde_json::json!({ "sessions": enriched })))
}

pub async fn get_tui_session(
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let mut rec = service
        .get_session_record(&id)
        .await
        .map_err(bad)?
        .ok_or_else(|| not_found(format!("Session '{id}' not found")))?;

    let screen = if let Ok(handle) = service.session_manager.get_session(&rec.session_name).await {
        if let Some(active) = handle.active_model().await {
            rec.active_model = Some(active.clone());
            rec.current_model = Some(active);
        }
        handle
            .backend
            .capture_screen(&rec.session_name, false, Some(100))
            .await
            .ok()
    } else {
        None
    };

    if rec.active_model.is_none() {
        rec.active_model = rec.model.clone();
        rec.current_model = rec.model.clone();
    }

    let bindings = service
        .store
        .list_bindings_for_session(&rec.id)
        .await
        .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "session": rec,
        "screen": screen,
        "bindings": bindings,
    })))
}

pub async fn switch_tui_session_model(
    AxumPath(id): AxumPath<String>,
    Json(body): Json<SwitchSessionModelBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let target_model = body.model.trim();
    if target_model.is_empty() {
        return Err(bad("Model name cannot be empty"));
    }

    let rec_opt = service.get_session_record(&id).await.map_err(bad)?;
    let rec = rec_opt.ok_or_else(|| not_found(format!("Session '{id}' not found")))?;
    let current_active =
        if let Ok(handle) = service.session_manager.get_session(&rec.session_name).await {
            handle.active_model().await
        } else {
            rec.active_model.clone().or_else(|| rec.model.clone())
        };

    match service.switch_session_model(&id, target_model).await {
        Ok((updated_rec, active_model)) => {
            let mut record_val = serde_json::to_value(&updated_rec).unwrap_or_default();
            if let Some(obj) = record_val.as_object_mut() {
                obj.insert("activeModel".to_string(), serde_json::json!(active_model));
                obj.insert("currentModel".to_string(), serde_json::json!(active_model));
            }
            Ok(Json(serde_json::json!({
                "success": true,
                "activeModel": active_model,
                "session": record_val,
                "message": format!("Switched model to {active_model}"),
            })))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string(),
                "message": e.to_string(),
                "activeModel": current_active,
            })),
        )),
    }
}

pub async fn get_tui_session_operations(
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let rec = service
        .get_session_record(&id)
        .await
        .map_err(bad)?
        .ok_or_else(|| not_found(format!("Session '{id}' not found")))?;

    let ops = service
        .store
        .list_operations(Some(&rec.id), None, None, 100, 0)
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::json!({ "operations": ops })))
}

pub async fn list_tui_operations(
    Query(query): Query<ListOperationsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let ops = service
        .store
        .list_operations(
            query.session_id.as_deref(),
            query.run_id.as_deref(),
            query.project_id.as_deref(),
            limit,
            offset,
        )
        .await
        .map_err(bad)?;

    Ok(Json(serde_json::json!({ "operations": ops })))
}

pub async fn interrupt_tui_session(
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    service.interrupt_session(&id).await.map_err(bad)?;
    Ok(Json(
        serde_json::json!({ "success": true, "message": "Session interrupted" }),
    ))
}

pub async fn stop_tui_session(
    AxumPath(id): AxumPath<String>,
    body: Option<Json<StopSessionBody>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let reason = body.and_then(|b| b.reason.clone());
    service
        .stop_session(&id, reason.as_deref())
        .await
        .map_err(bad)?;
    Ok(Json(
        serde_json::json!({ "success": true, "message": "Session stopped" }),
    ))
}

pub async fn takeover_tui_session(
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let sess = service.takeover_session(&id).await.map_err(bad)?;
    Ok(Json(
        serde_json::json!({ "success": true, "session": sess }),
    ))
}

pub async fn transfer_tui_session(
    AxumPath(id): AxumPath<String>,
    Json(body): Json<TransferSessionBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let sess = service
        .transfer_session(
            &id,
            &body.target_run_id,
            body.project_id.as_deref(),
            body.task_id.as_deref(),
        )
        .await
        .map_err(bad)?;
    Ok(Json(
        serde_json::json!({ "success": true, "session": sess }),
    ))
}

pub async fn extend_tui_session(
    AxumPath(id): AxumPath<String>,
    Json(body): Json<ExtendSessionBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let additional = body.additional_mins.unwrap_or(30);
    let sess = service.extend_session(&id, additional).await.map_err(bad)?;
    Ok(Json(
        serde_json::json!({ "success": true, "session": sess }),
    ))
}

pub async fn list_tui_accounts(
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let accounts = service.store.list_accounts().await.map_err(bad)?;
    Ok(Json(serde_json::json!({ "accounts": accounts })))
}

pub async fn create_or_update_tui_account(
    Json(mut req): Json<AccountRecord>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    if req.id.is_empty() {
        req.id = format!("acc_{}", uuid::Uuid::new_v4());
    }
    let now = Utc::now().to_rfc3339();
    if req.created_at.is_empty() {
        req.created_at = now.clone();
    }
    req.updated_at = now;
    service.store.upsert_account(&req).await.map_err(bad)?;
    Ok(Json(serde_json::json!({ "success": true, "account": req })))
}

pub async fn list_tui_quotas(
    AxumPath(account_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let quotas = service
        .store
        .list_quotas(Some(&account_id))
        .await
        .map_err(bad)?;
    Ok(Json(serde_json::json!({ "quotas": quotas })))
}

pub async fn refresh_tui_quotas(
    AxumPath(account_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let now = Utc::now().to_rfc3339();
    let sample = QuotaSnapshotRecord {
        id: format!("quota_{}", uuid::Uuid::new_v4()),
        pool_id: format!("pool_{}", account_id),
        account_id: Some(account_id.clone()),
        agent_kind: None,
        model_pattern: Some("*".to_string()),
        quota_type: "weekly".to_string(),
        unit: "percent".to_string(),
        used_amount: 15.0,
        limit_amount: Some(100.0),
        remaining_percent: Some(85.0),
        window_start: Some(now.clone()),
        window_end: None,
        reset_at: None,
        source: "cli_status".to_string(),
        status: "valid".to_string(),
        observed_at: now,
    };
    service.store.upsert_quota(&sample).await.map_err(bad)?;
    let quotas = service
        .store
        .list_quotas(Some(&account_id))
        .await
        .map_err(bad)?;
    Ok(Json(
        serde_json::json!({ "success": true, "quotas": quotas }),
    ))
}

pub async fn tui_events_sse() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let service = TuiManagementService::global().await;
    let rx = service.event_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(evt) => {
            let json_str = serde_json::to_string(&evt).unwrap_or_default();
            Some(Ok(Event::default().event(evt.event_type).data(json_str)))
        }
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartManagedSessionBody {
    pub agent: String,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub auto_approve: Option<bool>,
    pub resume_id: Option<String>,
    pub initial_prompt: Option<String>,
    pub session_name: Option<String>,
    pub extra_args: Option<Vec<String>>,
    pub proxy_mode: Option<String>,
    pub proxy_id: Option<String>,
    pub https_proxy: Option<String>,
    pub account_id: Option<String>,
    pub idle_timeout_mins: Option<u64>,
    pub exited_retention_mins: Option<u64>,
    pub project_slug: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub subtask_id: Option<String>,
    pub creation_source: Option<String>,
}

pub async fn start_managed_session_route(
    Json(body): Json<StartManagedSessionBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let service = TuiManagementService::global().await;
    let cwd = if let Some(c) = body.cwd {
        std::path::PathBuf::from(c)
    } else {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    };

    let (rec, initial_turn) = service
        .start_session(super::service::StartManagedSessionRequest {
            agent: body.agent,
            cwd,
            model: body.model,
            effort: body.effort,
            auto_approve: body.auto_approve,
            resume_id: body.resume_id,
            initial_prompt: body.initial_prompt,
            session_name: body.session_name,
            extra_args: body.extra_args,
            proxy_mode: body.proxy_mode,
            proxy_id: body.proxy_id,
            https_proxy: body.https_proxy,
            account_id: body.account_id,
            idle_timeout_mins: body.idle_timeout_mins,
            exited_retention_mins: body.exited_retention_mins,
            project_slug: body.project_slug,
            task_id: body.task_id,
            run_id: body.run_id,
            subtask_id: body.subtask_id,
            creation_source: body.creation_source.or_else(|| Some("hub_ui".to_string())),
        })
        .await
        .map_err(bad)?;

    Ok(Json(serde_json::json!({
        "success": true,
        "session": rec,
        "initialTurn": initial_turn,
    })))
}

pub fn tui_management_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/api/v1/tui-agents",
            get(list_tui_agents).post(create_or_update_tui_agent),
        )
        .route(
            "/api/v1/tui-agents/{id}",
            get(get_tui_agent).patch(patch_tui_agent),
        )
        .route("/api/v1/tui-agents/{id}/models", get(get_tui_agent_models))
        .route(
            "/api/v1/tui-agents/{id}/models/refresh",
            post(refresh_tui_agent_models),
        )
        .route(
            "/api/v1/tui-sessions",
            get(list_sessions_route).post(start_managed_session_route),
        )
        .route("/api/v1/tui-sessions/{id}", get(get_tui_session))
        .route(
            "/api/v1/tui-sessions/{id}/model",
            post(switch_tui_session_model),
        )
        .route(
            "/api/v1/tui-sessions/{id}/operations",
            get(get_tui_session_operations),
        )
        .route("/api/v1/tui-operations", get(list_tui_operations))
        .route(
            "/api/v1/tui-sessions/{id}/interrupt",
            post(interrupt_tui_session),
        )
        .route("/api/v1/tui-sessions/{id}/stop", post(stop_tui_session))
        .route(
            "/api/v1/tui-sessions/{id}/takeover",
            post(takeover_tui_session),
        )
        .route(
            "/api/v1/tui-sessions/{id}/transfer",
            post(transfer_tui_session),
        )
        .route("/api/v1/tui-sessions/{id}/extend", post(extend_tui_session))
        .route(
            "/api/v1/tui-accounts",
            get(list_tui_accounts).post(create_or_update_tui_account),
        )
        .route("/api/v1/tui-accounts/{id}/quotas", get(list_tui_quotas))
        .route(
            "/api/v1/tui-accounts/{id}/quotas/refresh",
            post(refresh_tui_quotas),
        )
        .route("/api/v1/tui-events", get(tui_events_sse))
}

async fn list_sessions_route(
    q: Query<ListSessionsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    list_tui_sessions(q).await
}
