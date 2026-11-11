use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex, OnceCell};
use tracing::{error, warn};

use anyhow::{anyhow, Result};
use chrono::Utc;
use etcetera::base_strategy::BaseStrategy;
use openduck_mcp::tui_session::{
    AgentCliKind, AgentModelCatalog, TuiPaneState, TuiSessionConfig, TuiSessionManager,
    TuiTurnResult,
};

use super::models::{
    SessionOperationRecord, SessionRunBindingRecord, TuiManagementEvent, TuiSessionRecord,
};
use super::store::SqliteTuiStore;

static GLOBAL_TUI_SERVICE: OnceCell<Arc<TuiManagementService>> = OnceCell::const_new();
static SHARED_SESSION_MANAGER: OnceLock<Arc<TuiSessionManager>> = OnceLock::new();
static SHARED_CATALOG: OnceLock<Arc<AgentModelCatalog>> = OnceLock::new();

pub struct TuiManagementService {
    pub store: Arc<SqliteTuiStore>,
    pub session_manager: Arc<TuiSessionManager>,
    pub catalog: Arc<AgentModelCatalog>,
    pub event_tx: broadcast::Sender<TuiManagementEvent>,
    sweeper_started: Mutex<bool>,
}

#[derive(Debug, Clone)]
pub struct StartManagedSessionRequest {
    pub agent: String,
    pub cwd: PathBuf,
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

fn inherited_tui_proxy(kind: &str, cwd: &Path) -> Option<String> {
    let settings = openduck_harness::types::discover_global_harness_settings(Some(cwd));
    let from_file = |key: &str| {
        settings
            .tui_sessions
            .get(key)
            .and_then(|defaults| defaults.https_proxy.clone())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    from_file(kind)
        .or_else(|| from_file("default"))
        .or_else(|| {
            crate::config::proxies::get_proxy_entry(crate::config::Config::global(), "default")
                .map(|entry| entry.url)
        })
}

fn inherited_tui_model_for_kind(kind: &str, cwd: &Path) -> Option<String> {
    let settings = openduck_harness::types::discover_global_harness_settings(Some(cwd));
    settings
        .tui_sessions
        .get(kind)
        .and_then(|defaults| defaults.resolved_model().map(ToOwned::to_owned))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn inherited_tui_model_default(cwd: &Path) -> Option<String> {
    let settings = openduck_harness::types::discover_global_harness_settings(Some(cwd));
    settings
        .tui_sessions
        .get("default")
        .and_then(|defaults| defaults.resolved_model().map(ToOwned::to_owned))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn validate_model_name(model: &str) -> Result<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Model name cannot be empty"));
    }
    if trimmed.len() > 128
        || trimmed.starts_with('/')
        || trimmed.starts_with('-')
        || trimmed.chars().any(|c| {
            c.is_control()
                || matches!(
                    c,
                    ';' | '|' | '&' | '`' | '$' | '\n' | '\r' | '\0' | '"' | '\'' | '\\'
                )
        })
    {
        return Err(anyhow!(
            "Invalid model '{trimmed}': rejected unsafe characters"
        ));
    }
    Ok(trimmed.to_string())
}

impl TuiManagementService {
    pub fn new(store: Arc<SqliteTuiStore>, session_manager: Arc<TuiSessionManager>) -> Self {
        Self::with_catalog(store, session_manager, Self::shared_catalog())
    }

    pub fn with_catalog(
        store: Arc<SqliteTuiStore>,
        session_manager: Arc<TuiSessionManager>,
        catalog: Arc<AgentModelCatalog>,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            store,
            session_manager,
            catalog,
            event_tx,
            sweeper_started: Mutex::new(false),
        }
    }

    pub fn shared_catalog() -> Arc<AgentModelCatalog> {
        SHARED_CATALOG
            .get_or_init(|| Arc::new(AgentModelCatalog::default_discovery()))
            .clone()
    }

    pub fn shared_session_manager() -> Arc<TuiSessionManager> {
        SHARED_SESSION_MANAGER
            .get_or_init(|| Arc::new(TuiSessionManager::default()))
            .clone()
    }

    pub async fn init_global(db_path: impl AsRef<Path>) -> Result<Arc<Self>> {
        let store = Arc::new(SqliteTuiStore::open(db_path).await?);
        let session_manager = Self::shared_session_manager();
        let service = Arc::new(Self::new(store, session_manager));
        let _ = GLOBAL_TUI_SERVICE.set(service.clone());
        service.ensure_background_tasks().await;
        Ok(service)
    }

    pub async fn global() -> Arc<Self> {
        if let Some(svc) = GLOBAL_TUI_SERVICE.get() {
            return svc.clone();
        }
        let fallback_db = etcetera::base_strategy::choose_base_strategy()
            .map(|s| s.data_dir().join("openduck").join("control.db"))
            .unwrap_or_else(|_| PathBuf::from("control.db"));
        Self::init_global(fallback_db).await.unwrap_or_else(|_| {
            let rt = tokio::runtime::Handle::current();
            let store =
                rt.block_on(async { Arc::new(SqliteTuiStore::open(":memory:").await.unwrap()) });
            Arc::new(Self::new(store, Self::shared_session_manager()))
        })
    }

    pub async fn ensure_background_tasks(self: &Arc<Self>) {
        let mut started = self.sweeper_started.lock().await;
        if *started {
            return;
        }
        *started = true;

        let self_recon = self.clone();
        tokio::spawn(async move {
            if let Err(e) = self_recon.reconcile_startup_sessions().await {
                error!("TUI session startup reconciliation error: {:#}", e);
            }
        });

        let self_sweep = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                if let Err(e) = self_sweep.run_sweeper_cycle().await {
                    warn!("TUI sweeper cycle error: {:#}", e);
                }
            }
        });
    }

    pub fn emit_event(
        &self,
        event_type: impl Into<String>,
        session_id: Option<String>,
        session_name: Option<String>,
        payload: serde_json::Value,
    ) {
        let _ = self.event_tx.send(TuiManagementEvent {
            event_type: event_type.into(),
            session_id,
            session_name,
            timestamp: Utc::now().to_rfc3339(),
            payload,
        });
    }

    pub async fn reconcile_startup_sessions(&self) -> Result<()> {
        let active_sessions = self.store.list_sessions(None, true, 500).await?;
        let now_str = Utc::now().to_rfc3339();

        for mut sess in active_sessions {
            let is_alive = self
                .session_manager
                .get_session(&sess.session_name)
                .await
                .is_ok();
            if sess.stop_intent.is_some() {
                let _ = self.session_manager.stop_session(&sess.session_name).await;
                sess.lifecycle_state = "reclaimed".to_string();
                sess.process_state = "exited".to_string();
                sess.stopped_at = Some(now_str.clone());
                sess.reclaimed_at = Some(now_str.clone());
                let _ = self.store.update_session(&sess).await;
                continue;
            }

            if !is_alive {
                sess.process_state = "exited".to_string();
                sess.tui_pane_state = "exited".to_string();
                sess.lifecycle_state = "reclaimed".to_string();
                sess.reclaimed_at = Some(now_str.clone());
                let _ = self.store.update_session(&sess).await;
            }
        }
        Ok(())
    }

    pub async fn run_sweeper_cycle(&self) -> Result<Vec<String>> {
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let sessions = self.store.list_sessions(None, true, 200).await?;
        let mut reclaimed_ids = Vec::new();

        for mut sess in sessions {
            let mut should_reclaim = false;
            let mut reason = "expired";

            if sess.lifecycle_state == "retained" {
                if let Some(ref reclaim_after_str) = sess.reclaim_after {
                    if let Ok(reclaim_after) =
                        chrono::DateTime::parse_from_rfc3339(reclaim_after_str)
                    {
                        if now >= reclaim_after {
                            should_reclaim = true;
                            reason = "retention_expired";
                        }
                    }
                }
            } else if sess.lifecycle_state == "active" {
                let idle_timeout = 30 * 60; // default 30 mins
                let last_act_str = sess
                    .last_input_at
                    .as_ref()
                    .or(sess.last_progress_at.as_ref())
                    .unwrap_or(&sess.created_at);
                if let Ok(last_act) = chrono::DateTime::parse_from_rfc3339(last_act_str) {
                    if now.signed_duration_since(last_act).num_seconds() > idle_timeout {
                        should_reclaim = true;
                        reason = "idle_timeout";
                    }
                }
            }

            if should_reclaim {
                let _ = self.session_manager.stop_session(&sess.session_name).await;
                sess.lifecycle_state = "reclaimed".to_string();
                sess.process_state = "exited".to_string();
                sess.tui_pane_state = "exited".to_string();
                sess.reclaimed_at = Some(now_str.clone());
                let _ = self.store.update_session(&sess).await;

                let op_id = format!("op_{}", uuid::Uuid::new_v4());
                let _ = self
                    .store
                    .insert_operation(&SessionOperationRecord {
                        id: op_id,
                        session_id: sess.id.clone(),
                        session_name: sess.session_name.clone(),
                        operation_type: "reclaim".to_string(),
                        operator: "system_reclaimer".to_string(),
                        project_id: sess.created_by_project_id.clone(),
                        task_id: sess.created_by_task_id.clone(),
                        run_id: sess.owner_run_id.clone(),
                        subtask_id: sess.created_by_subtask_id.clone(),
                        summary: Some(format!("Auto-reclaimed session due to {reason}")),
                        content_ref: None,
                        status: "succeeded".to_string(),
                        error: None,
                        duration_ms: Some(0),
                        started_at: now_str.clone(),
                        finished_at: Some(now_str.clone()),
                    })
                    .await;

                self.emit_event(
                    "session_reclaimed",
                    Some(sess.id.clone()),
                    Some(sess.session_name.clone()),
                    serde_json::json!({ "reason": reason }),
                );
                reclaimed_ids.push(sess.session_name);
            }
        }

        Ok(reclaimed_ids)
    }

    pub async fn start_session(
        &self,
        req: StartManagedSessionRequest,
    ) -> Result<(TuiSessionRecord, Option<TuiTurnResult>)> {
        let kind = AgentCliKind::from_str(&req.agent)?;
        let session_name = req
            .session_name
            .clone()
            .unwrap_or_else(|| format!("{}_{}", kind, chrono::Utc::now().timestamp_millis()));
        let internal_id = format!("sess_{}", uuid::Uuid::new_v4());
        let now_str = Utc::now().to_rfc3339();

        let profile = self
            .store
            .get_profile_by_agent(&req.agent)
            .await
            .ok()
            .flatten();
        let mut working_dir = req.cwd.clone();
        if let Some(ref slug) = req.project_slug {
            if let Ok(entry) = crate::sources::read_project(slug) {
                if let Some(proj_dir) = entry
                    .properties
                    .get("workingDirs")
                    .and_then(|dirs| dirs.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                {
                    let p = Path::new(proj_dir);
                    if working_dir == Path::new(".") || !working_dir.is_absolute() {
                        working_dir = p.to_path_buf();
                    }
                }
            }
        }

        let explicit_model = req
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(validate_model_name)
            .transpose()?;

        let resolved_model = explicit_model
            .or_else(|| inherited_tui_model_for_kind(&kind.to_string(), &working_dir))
            .or_else(|| {
                profile
                    .as_ref()
                    .and_then(|p| p.default_model.clone())
                    .map(|m| m.trim().to_string())
                    .filter(|m| !m.is_empty())
            })
            .or_else(|| inherited_tui_model_default(&working_dir));

        if let Some(ref m) = resolved_model {
            validate_model_name(m)?;
        }
        let resolved_effort = req
            .effort
            .clone()
            .or_else(|| profile.as_ref().and_then(|p| p.default_effort.clone()));
        let resolved_proxy_mode = req.proxy_mode.unwrap_or_else(|| {
            profile
                .as_ref()
                .map(|p| p.proxy_mode.clone())
                .unwrap_or_else(|| "inherit".to_string())
        });
        let resolved_proxy_id = req
            .proxy_id
            .or_else(|| profile.as_ref().and_then(|p| p.proxy_id.clone()));

        let effective_proxy = if resolved_proxy_mode == "direct" {
            None
        } else if let Some(p) = req.https_proxy {
            Some(p)
        } else if resolved_proxy_mode == "custom" {
            resolved_proxy_id.clone().filter(|s| !s.trim().is_empty())
        } else if resolved_proxy_mode == "named" {
            if let Some(ref pid) = resolved_proxy_id {
                let config = crate::config::Config::global();
                crate::config::proxies::get_proxy_entry(config, pid)
                    .map(|e| e.url)
                    .or_else(|| {
                        if pid.starts_with("http://")
                            || pid.starts_with("https://")
                            || pid.starts_with("socks5://")
                            || pid.starts_with("socks5h://")
                        {
                            Some(pid.clone())
                        } else {
                            None
                        }
                    })
            } else {
                None
            }
        } else {
            inherited_tui_proxy(&kind.to_string(), &working_dir)
        };

        let op_id = format!("op_{}", uuid::Uuid::new_v4());
        let start_time = Instant::now();
        let _ = self
            .store
            .insert_operation(&SessionOperationRecord {
                id: op_id.clone(),
                session_id: internal_id.clone(),
                session_name: session_name.clone(),
                operation_type: "start".to_string(),
                operator: req
                    .creation_source
                    .clone()
                    .unwrap_or_else(|| "manual".to_string()),
                project_id: req.project_slug.clone(),
                task_id: req.task_id.clone(),
                run_id: req.run_id.clone(),
                subtask_id: req.subtask_id.clone(),
                summary: Some(format!("Started {} agent session", kind)),
                content_ref: None,
                status: "started".to_string(),
                error: None,
                duration_ms: None,
                started_at: now_str.clone(),
                finished_at: None,
            })
            .await;

        let mut record = TuiSessionRecord {
            id: internal_id.clone(),
            session_name: session_name.clone(),
            agent_kind: kind.to_string(),
            cwd: working_dir.to_string_lossy().to_string(),
            model: resolved_model.clone(),
            active_model: resolved_model.clone(),
            current_model: resolved_model.clone(),
            effort: resolved_effort.clone(),
            auto_approve: req.auto_approve.unwrap_or(true),
            proxy_mode: resolved_proxy_mode.clone(),
            proxy_id: resolved_proxy_id,
            effective_proxy: effective_proxy.clone(),
            account_id: req.account_id,
            created_by_project_id: req.project_slug.clone(),
            created_by_task_id: req.task_id.clone(),
            created_by_run_id: req.run_id.clone(),
            created_by_subtask_id: req.subtask_id.clone(),
            creation_source: req
                .creation_source
                .clone()
                .unwrap_or_else(|| "manual".to_string()),
            owner_run_id: req.run_id.clone(),
            ownership_mode: if req.run_id.is_some() {
                "run_exclusive".to_string()
            } else {
                "manual".to_string()
            },
            generation: 1,
            lifecycle_state: "active".to_string(),
            process_state: "alive".to_string(),
            tui_pane_state: "initializing".to_string(),
            stop_intent: None,
            cleanup_error: None,
            created_at: now_str.clone(),
            last_operation_at: Some(now_str.clone()),
            last_input_at: None,
            last_progress_at: None,
            last_observed_at: Some(now_str.clone()),
            idle_since: None,
            owner_terminal_at: None,
            reclaim_after: None,
            stopped_at: None,
            reclaimed_at: None,
        };

        self.store.insert_session(&record).await?;

        if let Some(ref rid) = req.run_id {
            let bind_id = format!("bind_{}", uuid::Uuid::new_v4());
            let _ = self
                .store
                .insert_binding(&SessionRunBindingRecord {
                    id: bind_id,
                    session_id: internal_id.clone(),
                    run_id: rid.clone(),
                    project_id: req.project_slug.clone(),
                    task_id: req.task_id.clone(),
                    bound_at: now_str.clone(),
                    unbound_at: None,
                    reason: Some("Initial session creation".to_string()),
                })
                .await;
        }

        let launch_proxy = if resolved_proxy_mode == "direct" {
            Some("".to_string())
        } else {
            effective_proxy.clone()
        };

        let config = TuiSessionConfig {
            session_id: session_name.clone(),
            kind,
            cwd: working_dir,
            model: resolved_model,
            effort: resolved_effort,
            auto_approve: req.auto_approve.unwrap_or(true),
            resume_id: req.resume_id,
            cols: 160,
            rows: 48,
            extra_args: req.extra_args.unwrap_or_default(),
            https_proxy: launch_proxy,
            env: Vec::new(),
            idle_timeout_mins: req.idle_timeout_mins,
            exited_retention_mins: req.exited_retention_mins,
        };

        let handle = match self.session_manager.start_session(config).await {
            Ok(h) => h,
            Err(e) => {
                let dur = start_time.elapsed().as_millis() as u64;
                let _ = self
                    .store
                    .complete_operation(
                        &op_id,
                        "failed",
                        Some(&e.to_string()),
                        dur,
                        &Utc::now().to_rfc3339(),
                    )
                    .await;
                return Err(e);
            }
        };

        let dur = start_time.elapsed().as_millis() as u64;
        let _ = self
            .store
            .complete_operation(&op_id, "succeeded", None, dur, &Utc::now().to_rfc3339())
            .await;

        let active = handle.active_model().await;
        let launch_model = handle.config.model.clone();
        record.active_model = active.clone();
        record.current_model = active;
        if record.model.is_none() {
            record.model = launch_model;
        }
        let _ = self.store.update_session(&record).await;

        let mut initial_turn = None;
        if let Some(prompt) = req.initial_prompt {
            let current_state = handle.last_state.lock().await.clone();
            if !matches!(current_state, TuiPaneState::WaitingApproval { .. }) {
                if let Ok(turn) = self
                    .send_prompt(
                        &session_name,
                        &prompt,
                        true,
                        Duration::from_secs(600),
                        req.run_id.as_deref(),
                    )
                    .await
                {
                    initial_turn = Some(turn);
                }
            }
        }

        self.emit_event(
            "session_started",
            Some(internal_id.clone()),
            Some(session_name.clone()),
            serde_json::to_value(&record).unwrap_or_default(),
        );

        Ok((record, initial_turn))
    }

    pub async fn send_prompt(
        &self,
        session_id_or_name: &str,
        prompt: &str,
        wait: bool,
        timeout: Duration,
        caller_run_id: Option<&str>,
    ) -> Result<TuiTurnResult> {
        let sess_rec = self
            .get_session_record(session_id_or_name)
            .await?
            .ok_or_else(|| anyhow!("Session '{}' not found", session_id_or_name))?;

        if sess_rec.lifecycle_state == "stopping" || sess_rec.lifecycle_state == "reclaimed" {
            return Err(anyhow!(
                "Session is in terminal state '{}'",
                sess_rec.lifecycle_state
            ));
        }

        if let Some(caller) = caller_run_id {
            if sess_rec.ownership_mode == "run_exclusive" {
                if let Some(ref owner) = sess_rec.owner_run_id {
                    if owner != caller {
                        return Err(anyhow!("Session is exclusively owned by run '{}'", owner));
                    }
                }
            }
        }

        let op_id = format!("op_{}", uuid::Uuid::new_v4());
        let start_time = Instant::now();
        let now_str = Utc::now().to_rfc3339();

        let _ = self
            .store
            .insert_operation(&SessionOperationRecord {
                id: op_id.clone(),
                session_id: sess_rec.id.clone(),
                session_name: sess_rec.session_name.clone(),
                operation_type: "prompt".to_string(),
                operator: caller_run_id.unwrap_or("manual").to_string(),
                project_id: sess_rec.created_by_project_id.clone(),
                task_id: sess_rec.created_by_task_id.clone(),
                run_id: caller_run_id
                    .map(|s| s.to_string())
                    .or_else(|| sess_rec.owner_run_id.clone()),
                subtask_id: sess_rec.created_by_subtask_id.clone(),
                summary: Some(if prompt.chars().count() > 100 {
                    let truncated: String = prompt.chars().take(100).collect();
                    format!("{truncated}...")
                } else {
                    prompt.to_string()
                }),
                content_ref: None,
                status: "started".to_string(),
                error: None,
                duration_ms: None,
                started_at: now_str.clone(),
                finished_at: None,
            })
            .await;

        let handle = self
            .session_manager
            .get_session(&sess_rec.session_name)
            .await?;
        let turn_res = handle.send_prompt(prompt, wait, timeout).await;

        let dur = start_time.elapsed().as_millis() as u64;
        let finished_str = Utc::now().to_rfc3339();

        match &turn_res {
            Ok(turn) => {
                let _ = self
                    .store
                    .complete_operation(&op_id, "succeeded", None, dur, &finished_str)
                    .await;
                let mut updated = sess_rec;
                updated.last_operation_at = Some(finished_str.clone());
                updated.last_input_at = Some(finished_str.clone());
                updated.last_progress_at = Some(finished_str.clone());
                updated.tui_pane_state = turn.state.label().to_string();
                let _ = self.store.update_session(&updated).await;

                self.emit_event(
                    "prompt_completed",
                    Some(updated.id),
                    Some(updated.session_name),
                    serde_json::json!({ "state": turn.state.label(), "duration_ms": dur }),
                );
            }
            Err(e) => {
                let _ = self
                    .store
                    .complete_operation(&op_id, "failed", Some(&e.to_string()), dur, &finished_str)
                    .await;
            }
        }

        turn_res
    }

    pub async fn send_keys(&self, session_id_or_name: &str, keys: &[String]) -> Result<()> {
        let sess_rec = self
            .get_session_record(session_id_or_name)
            .await?
            .ok_or_else(|| anyhow!("Session '{}' not found", session_id_or_name))?;

        let op_id = format!("op_{}", uuid::Uuid::new_v4());
        let now_str = Utc::now().to_rfc3339();
        let _ = self
            .store
            .insert_operation(&SessionOperationRecord {
                id: op_id.clone(),
                session_id: sess_rec.id.clone(),
                session_name: sess_rec.session_name.clone(),
                operation_type: "keys".to_string(),
                operator: "manual".to_string(),
                project_id: sess_rec.created_by_project_id.clone(),
                task_id: sess_rec.created_by_task_id.clone(),
                run_id: sess_rec.owner_run_id.clone(),
                subtask_id: sess_rec.created_by_subtask_id.clone(),
                summary: Some(format!("Keys: {}", keys.join(" "))),
                content_ref: None,
                status: "succeeded".to_string(),
                error: None,
                duration_ms: Some(0),
                started_at: now_str.clone(),
                finished_at: Some(now_str.clone()),
            })
            .await;

        let handle = self
            .session_manager
            .get_session(&sess_rec.session_name)
            .await?;
        handle.send_keys(keys).await?;

        let mut updated = sess_rec;
        updated.last_operation_at = Some(now_str.clone());
        updated.last_input_at = Some(now_str);
        let _ = self.store.update_session(&updated).await;
        Ok(())
    }

    pub async fn read_state(
        &self,
        session_id_or_name: &str,
        scrollback_lines: Option<i32>,
        include_ansi: bool,
    ) -> Result<TuiTurnResult> {
        let sess_rec = self
            .get_session_record(session_id_or_name)
            .await?
            .ok_or_else(|| anyhow!("Session '{}' not found", session_id_or_name))?;

        let handle = self
            .session_manager
            .get_session(&sess_rec.session_name)
            .await?;
        let res = handle.read_state(scrollback_lines, include_ansi).await?;

        let now_str = Utc::now().to_rfc3339();
        let mut updated = sess_rec;
        updated.last_observed_at = Some(now_str);
        updated.tui_pane_state = res.state.label().to_string();
        let _ = self.store.update_session(&updated).await;

        Ok(res)
    }

    pub async fn interrupt_session(&self, session_id_or_name: &str) -> Result<()> {
        let sess_rec = self
            .get_session_record(session_id_or_name)
            .await?
            .ok_or_else(|| anyhow!("Session '{}' not found", session_id_or_name))?;

        let handle = self
            .session_manager
            .get_session(&sess_rec.session_name)
            .await?;
        let _ = handle
            .backend
            .send_keys(
                &sess_rec.session_name,
                &["Escape".to_string(), "C-c".to_string()],
            )
            .await;

        let now_str = Utc::now().to_rfc3339();
        let op_id = format!("op_{}", uuid::Uuid::new_v4());
        let _ = self
            .store
            .insert_operation(&SessionOperationRecord {
                id: op_id,
                session_id: sess_rec.id.clone(),
                session_name: sess_rec.session_name.clone(),
                operation_type: "interrupt".to_string(),
                operator: "manual".to_string(),
                project_id: sess_rec.created_by_project_id.clone(),
                task_id: sess_rec.created_by_task_id.clone(),
                run_id: sess_rec.owner_run_id.clone(),
                subtask_id: sess_rec.created_by_subtask_id.clone(),
                summary: Some("User triggered interrupt (Escape + C-c)".to_string()),
                content_ref: None,
                status: "succeeded".to_string(),
                error: None,
                duration_ms: Some(0),
                started_at: now_str.clone(),
                finished_at: Some(now_str.clone()),
            })
            .await;

        self.emit_event(
            "session_interrupted",
            Some(sess_rec.id),
            Some(sess_rec.session_name),
            serde_json::json!({ "status": "interrupted" }),
        );
        Ok(())
    }

    pub async fn stop_session(&self, session_id_or_name: &str, reason: Option<&str>) -> Result<()> {
        let sess_rec = self
            .get_session_record(session_id_or_name)
            .await?
            .ok_or_else(|| anyhow!("Session '{}' not found", session_id_or_name))?;

        let mut stopping = sess_rec.clone();
        stopping.lifecycle_state = "stopping".to_string();
        stopping.stop_intent = Some(reason.unwrap_or("user_stopped").to_string());
        let _ = self.store.update_session(&stopping).await;

        let now_str = Utc::now().to_rfc3339();
        let _ = self
            .session_manager
            .stop_session(&sess_rec.session_name)
            .await;

        let mut reclaimed = stopping;
        reclaimed.lifecycle_state = "reclaimed".to_string();
        reclaimed.process_state = "exited".to_string();
        reclaimed.tui_pane_state = "exited".to_string();
        reclaimed.stopped_at = Some(now_str.clone());
        reclaimed.reclaimed_at = Some(now_str.clone());
        let _ = self.store.update_session(&reclaimed).await;

        let op_id = format!("op_{}", uuid::Uuid::new_v4());
        let _ = self
            .store
            .insert_operation(&SessionOperationRecord {
                id: op_id,
                session_id: sess_rec.id.clone(),
                session_name: sess_rec.session_name.clone(),
                operation_type: "stop".to_string(),
                operator: "manual".to_string(),
                project_id: sess_rec.created_by_project_id.clone(),
                task_id: sess_rec.created_by_task_id.clone(),
                run_id: sess_rec.owner_run_id.clone(),
                subtask_id: sess_rec.created_by_subtask_id.clone(),
                summary: Some(format!(
                    "Session stopped: {}",
                    reason.unwrap_or("User requested")
                )),
                content_ref: None,
                status: "succeeded".to_string(),
                error: None,
                duration_ms: Some(0),
                started_at: now_str.clone(),
                finished_at: Some(now_str.clone()),
            })
            .await;

        self.emit_event(
            "session_stopped",
            Some(sess_rec.id),
            Some(sess_rec.session_name),
            serde_json::json!({ "reason": reason }),
        );
        Ok(())
    }

    pub async fn takeover_session(&self, session_id_or_name: &str) -> Result<TuiSessionRecord> {
        let mut sess = self
            .get_session_record(session_id_or_name)
            .await?
            .ok_or_else(|| anyhow!("Session '{}' not found", session_id_or_name))?;

        sess.ownership_mode = "manual".to_string();
        sess.owner_run_id = None;
        sess.generation += 1;
        sess.reclaim_after = None;
        let now_str = Utc::now().to_rfc3339();
        sess.last_operation_at = Some(now_str.clone());
        self.store.update_session(&sess).await?;

        let op_id = format!("op_{}", uuid::Uuid::new_v4());
        let _ = self
            .store
            .insert_operation(&SessionOperationRecord {
                id: op_id,
                session_id: sess.id.clone(),
                session_name: sess.session_name.clone(),
                operation_type: "takeover".to_string(),
                operator: "manual".to_string(),
                project_id: sess.created_by_project_id.clone(),
                task_id: sess.created_by_task_id.clone(),
                run_id: None,
                subtask_id: sess.created_by_subtask_id.clone(),
                summary: Some("Manual user takeover of session ownership".to_string()),
                content_ref: None,
                status: "succeeded".to_string(),
                error: None,
                duration_ms: Some(0),
                started_at: now_str.clone(),
                finished_at: Some(now_str.clone()),
            })
            .await;

        self.emit_event(
            "session_takeover",
            Some(sess.id.clone()),
            Some(sess.session_name.clone()),
            serde_json::to_value(&sess).unwrap_or_default(),
        );
        Ok(sess)
    }

    pub async fn transfer_session(
        &self,
        session_id_or_name: &str,
        target_run_id: &str,
        project_id: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<TuiSessionRecord> {
        let mut sess = self
            .get_session_record(session_id_or_name)
            .await?
            .ok_or_else(|| anyhow!("Session '{}' not found", session_id_or_name))?;

        sess.owner_run_id = Some(target_run_id.to_string());
        sess.ownership_mode = "run_exclusive".to_string();
        sess.generation += 1;
        sess.reclaim_after = None;
        let now_str = Utc::now().to_rfc3339();
        sess.last_operation_at = Some(now_str.clone());
        self.store.update_session(&sess).await?;

        let bind_id = format!("bind_{}", uuid::Uuid::new_v4());
        let _ = self
            .store
            .insert_binding(&SessionRunBindingRecord {
                id: bind_id,
                session_id: sess.id.clone(),
                run_id: target_run_id.to_string(),
                project_id: project_id
                    .map(|s| s.to_string())
                    .or_else(|| sess.created_by_project_id.clone()),
                task_id: task_id
                    .map(|s| s.to_string())
                    .or_else(|| sess.created_by_task_id.clone()),
                bound_at: now_str.clone(),
                unbound_at: None,
                reason: Some("Ownership transfer".to_string()),
            })
            .await;

        let op_id = format!("op_{}", uuid::Uuid::new_v4());
        let _ = self
            .store
            .insert_operation(&SessionOperationRecord {
                id: op_id,
                session_id: sess.id.clone(),
                session_name: sess.session_name.clone(),
                operation_type: "transfer".to_string(),
                operator: "manual".to_string(),
                project_id: project_id
                    .map(|s| s.to_string())
                    .or_else(|| sess.created_by_project_id.clone()),
                task_id: task_id
                    .map(|s| s.to_string())
                    .or_else(|| sess.created_by_task_id.clone()),
                run_id: Some(target_run_id.to_string()),
                subtask_id: sess.created_by_subtask_id.clone(),
                summary: Some(format!(
                    "Transferred session ownership to run '{target_run_id}'"
                )),
                content_ref: None,
                status: "succeeded".to_string(),
                error: None,
                duration_ms: Some(0),
                started_at: now_str.clone(),
                finished_at: Some(now_str.clone()),
            })
            .await;

        self.emit_event(
            "session_transfer",
            Some(sess.id.clone()),
            Some(sess.session_name.clone()),
            serde_json::to_value(&sess).unwrap_or_default(),
        );
        Ok(sess)
    }

    pub async fn extend_session(
        &self,
        session_id_or_name: &str,
        additional_mins: u64,
    ) -> Result<TuiSessionRecord> {
        let mut sess = self
            .get_session_record(session_id_or_name)
            .await?
            .ok_or_else(|| anyhow!("Session '{}' not found", session_id_or_name))?;

        let base_time = sess
            .reclaim_after
            .as_ref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let new_reclaim = base_time + chrono::Duration::minutes(additional_mins as i64);
        sess.reclaim_after = Some(new_reclaim.to_rfc3339());
        let now_str = Utc::now().to_rfc3339();
        sess.last_operation_at = Some(now_str.clone());
        self.store.update_session(&sess).await?;

        let op_id = format!("op_{}", uuid::Uuid::new_v4());
        let _ = self
            .store
            .insert_operation(&SessionOperationRecord {
                id: op_id,
                session_id: sess.id.clone(),
                session_name: sess.session_name.clone(),
                operation_type: "extend".to_string(),
                operator: "manual".to_string(),
                project_id: sess.created_by_project_id.clone(),
                task_id: sess.created_by_task_id.clone(),
                run_id: sess.owner_run_id.clone(),
                subtask_id: sess.created_by_subtask_id.clone(),
                summary: Some(format!(
                    "Extended session retention by {} minutes",
                    additional_mins
                )),
                content_ref: None,
                status: "succeeded".to_string(),
                error: None,
                duration_ms: Some(0),
                started_at: now_str.clone(),
                finished_at: Some(now_str.clone()),
            })
            .await;

        self.emit_event(
            "session_extended",
            Some(sess.id.clone()),
            Some(sess.session_name.clone()),
            serde_json::to_value(&sess).unwrap_or_default(),
        );
        Ok(sess)
    }

    pub async fn switch_session_model(
        &self,
        session_id_or_name: &str,
        new_model: &str,
    ) -> Result<(TuiSessionRecord, String)> {
        let trimmed_model = validate_model_name(new_model)?;

        let mut sess_rec = self
            .get_session_record(session_id_or_name)
            .await?
            .ok_or_else(|| anyhow!("Session '{}' not found", session_id_or_name))?;

        if sess_rec.lifecycle_state == "stopping"
            || sess_rec.lifecycle_state == "reclaimed"
            || sess_rec.lifecycle_state == "stopped"
        {
            return Err(anyhow!(
                "Session '{}' is not running",
                sess_rec.session_name
            ));
        }

        let handle = self
            .session_manager
            .get_session(&sess_rec.session_name)
            .await
            .map_err(|_| anyhow!("Live session '{}' not found", sess_rec.session_name))?;

        let op_id = format!("op_{}", uuid::Uuid::new_v4());
        let start_time = Instant::now();
        let now_str = Utc::now().to_rfc3339();

        let _ = self
            .store
            .insert_operation(&SessionOperationRecord {
                id: op_id.clone(),
                session_id: sess_rec.id.clone(),
                session_name: sess_rec.session_name.clone(),
                operation_type: "switch_model".to_string(),
                operator: "manual".to_string(),
                project_id: sess_rec.created_by_project_id.clone(),
                task_id: sess_rec.created_by_task_id.clone(),
                run_id: sess_rec.owner_run_id.clone(),
                subtask_id: sess_rec.created_by_subtask_id.clone(),
                summary: Some(format!("Request model switch to '{}'", trimmed_model)),
                content_ref: None,
                status: "running".to_string(),
                error: None,
                duration_ms: None,
                started_at: now_str.clone(),
                finished_at: None,
            })
            .await;

        match handle.switch_model(&trimmed_model).await {
            Ok(switched) => {
                let dur = start_time.elapsed().as_millis() as u64;
                let _ = self
                    .store
                    .complete_operation(&op_id, "succeeded", None, dur, &Utc::now().to_rfc3339())
                    .await;

                // Preserve sess_rec.model (the original configured model)
                sess_rec.active_model = Some(switched.clone());
                sess_rec.current_model = Some(switched.clone());
                sess_rec.last_operation_at = Some(Utc::now().to_rfc3339());
                self.store
                    .update_session(&sess_rec)
                    .await
                    .map_err(|e| anyhow!("Failed to persist switched session model: {e}"))?;

                self.emit_event(
                    "session_model_switched",
                    Some(sess_rec.id.clone()),
                    Some(sess_rec.session_name.clone()),
                    serde_json::json!({
                        "model": sess_rec.model,
                        "activeModel": switched,
                        "currentModel": switched,
                    }),
                );

                Ok((sess_rec, switched))
            }
            Err(e) => {
                let dur = start_time.elapsed().as_millis() as u64;
                let err_msg = e.to_string();
                let _ = self
                    .store
                    .complete_operation(
                        &op_id,
                        "failed",
                        Some(&err_msg),
                        dur,
                        &Utc::now().to_rfc3339(),
                    )
                    .await;

                Err(e)
            }
        }
    }

    pub async fn on_task_run_ended(
        &self,
        project_slug: Option<&str>,
        task_id: Option<&str>,
        run_id: &str,
        status: &str,
    ) -> Result<()> {
        let owned_sessions = self.store.list_active_sessions_for_run(run_id).await?;
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        for mut sess in owned_sessions {
            sess.owner_terminal_at = Some(now_str.clone());

            if status == "cancelled" || status == "stopped" {
                // Immediately interrupt and terminate within grace period
                let _ = self.interrupt_session(&sess.session_name).await;
                sess.lifecycle_state = "reclaim_pending".to_string();
                sess.stop_intent = Some("owner_run_cancelled".to_string());
                sess.reclaim_after = Some((now + chrono::Duration::seconds(30)).to_rfc3339());
                let _ = self.store.update_session(&sess).await;

                let service_clone = self.session_manager.clone();
                let store_clone = self.store.clone();
                let s_name = sess.session_name.clone();
                let s_id = sess.id.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    let _ = service_clone.stop_session(&s_name).await;
                    if let Ok(Some(mut rec)) = store_clone.get_session_by_id(&s_id).await {
                        rec.lifecycle_state = "reclaimed".to_string();
                        rec.process_state = "exited".to_string();
                        rec.tui_pane_state = "exited".to_string();
                        rec.reclaimed_at = Some(Utc::now().to_rfc3339());
                        let _ = store_clone.update_session(&rec).await;
                    }
                });
            } else if status == "success" {
                // Succeeded: interrupt if working, retain idle session for 10 minutes
                let _ = self.interrupt_session(&sess.session_name).await;
                sess.lifecycle_state = "retained".to_string();
                sess.reclaim_after = Some((now + chrono::Duration::minutes(10)).to_rfc3339());
                let _ = self.store.update_session(&sess).await;
            } else {
                // Failed: interrupt, preserve static scene for 10 minutes
                let _ = self.interrupt_session(&sess.session_name).await;
                sess.lifecycle_state = "retained".to_string();
                sess.reclaim_after = Some((now + chrono::Duration::minutes(10)).to_rfc3339());
                let _ = self.store.update_session(&sess).await;
            }

            let op_id = format!("op_{}", uuid::Uuid::new_v4());
            let _ = self
                .store
                .insert_operation(&SessionOperationRecord {
                    id: op_id,
                    session_id: sess.id.clone(),
                    session_name: sess.session_name.clone(),
                    operation_type: "run_lifecycle".to_string(),
                    operator: "harness".to_string(),
                    project_id: project_slug.map(|s| s.to_string()),
                    task_id: task_id.map(|s| s.to_string()),
                    run_id: Some(run_id.to_string()),
                    subtask_id: sess.created_by_subtask_id.clone(),
                    summary: Some(format!(
                        "Owner run '{}' transitioned to status '{}'",
                        run_id, status
                    )),
                    content_ref: None,
                    status: "succeeded".to_string(),
                    error: None,
                    duration_ms: Some(0),
                    started_at: now_str.clone(),
                    finished_at: Some(now_str.clone()),
                })
                .await;
        }

        Ok(())
    }

    pub async fn get_session_record(&self, id_or_name: &str) -> Result<Option<TuiSessionRecord>> {
        if let Some(s) = self.store.get_session_by_id(id_or_name).await? {
            return Ok(Some(s));
        }
        self.store.get_session_by_name(id_or_name).await
    }
}
