use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex, OnceCell};
use tracing::{error, warn};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use etcetera::base_strategy::BaseStrategy;
use openduck_mcp::tui_session::profiles::detect_quota_limit;
use openduck_mcp::tui_session::{
    AgentCliKind, AgentModelCatalog, CatalogModelEntry, TuiPaneState, TuiSessionConfig,
    TuiSessionManager, TuiTurnResult,
};

use super::models::{
    QuotaSnapshotRecord, SessionOperationRecord, SessionRunBindingRecord, TuiManagementEvent,
    TuiSessionRecord,
};
use super::quota::{QuotaAdmission, QuotaAgentStatus, DEFAULT_QUOTA_STALE_AFTER_SECONDS};
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

fn resolve_effective_proxy(
    proxy_mode: &str,
    proxy_id: Option<&str>,
    explicit_https_proxy: Option<String>,
    agent_kind: &str,
    working_dir: &Path,
) -> Option<String> {
    if proxy_mode == "direct" {
        None
    } else if let Some(p) = explicit_https_proxy {
        Some(p)
    } else if proxy_mode == "custom" {
        proxy_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
    } else if proxy_mode == "named" {
        if let Some(pid) = proxy_id {
            let config = crate::config::Config::global();
            crate::config::proxies::get_proxy_entry(config, pid)
                .map(|e| e.url)
                .or_else(|| {
                    if pid.starts_with("http://")
                        || pid.starts_with("https://")
                        || pid.starts_with("socks5://")
                        || pid.starts_with("socks5h://")
                    {
                        Some(pid.to_string())
                    } else {
                        None
                    }
                })
        } else {
            None
        }
    } else {
        inherited_tui_proxy(agent_kind, working_dir)
    }
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

    /// Scheduling admission for one quota book.
    ///
    /// This does not start a session, read a provider, or rewrite the book.
    /// Session launch and prompt send keep their existing checks.
    #[must_use]
    pub fn admit_agent_quota(status: &QuotaAgentStatus, now: DateTime<Utc>) -> QuotaAdmission {
        status.admission_at(now)
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

    async fn select_agy_startup_model(
        &self,
        desired: Option<&str>,
        account_id: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Option<String>> {
        if let Some(exhausted) = self.blocking_agy_wildcard_record(account_id, now).await {
            return Err(suspended_agent_error(
                "agy",
                &exhausted.quota_type,
                exhausted.reset_at.as_deref(),
            ));
        }

        let entry = self.catalog.get_models(AgentCliKind::Agy).await;
        let candidates =
            agy_candidate_models(desired, entry.default_model.as_deref(), &entry.models);
        let mut decided = Vec::with_capacity(candidates.len());
        for model in &candidates {
            let read = self.read_model_quota("agy", model, account_id).await;
            decided.push(AgyCandidate {
                model: model.as_str(),
                eligibility: agy_eligibility(&read, now),
            });
        }

        match choose_agy_model(&decided) {
            Some(AgyModelChoice::Selected(model)) => Ok(Some(model)),
            Some(AgyModelChoice::AllBlocked {
                quota_type,
                reset_at,
                models,
            }) => Err(all_agy_models_blocked_error(
                &quota_type,
                reset_at.as_deref(),
                &models,
            )),
            None => Ok(None),
        }
    }

    async fn reject_blocked_agy_prompt(&self, session: &TuiSessionRecord) -> Result<()> {
        let now = Utc::now();
        let account_id = session.account_id.as_deref();
        if let Some(exhausted) = self.blocking_agy_wildcard_record(account_id, now).await {
            return Err(session_quota_error(
                &session.session_name,
                "agy",
                None,
                &exhausted.quota_type,
                exhausted.reset_at.as_deref(),
            ));
        }
        let Some(model) = session_active_quota_model(session) else {
            return Ok(());
        };
        let read = self.read_model_quota("agy", model, account_id).await;
        if let AgyModelEligibility::Blocked(block) = agy_eligibility(&read, now) {
            return Err(session_quota_error(
                &session.session_name,
                "agy",
                Some(model),
                &block.quota_type,
                block.reset_at.as_deref(),
            ));
        }
        Ok(())
    }

    async fn blocking_agy_wildcard_record(
        &self,
        account_id: Option<&str>,
        now: DateTime<Utc>,
    ) -> Option<QuotaSnapshotRecord> {
        let mut accounts = Vec::with_capacity(2);
        if account_id.is_some() {
            accounts.push(account_id);
        }
        accounts.push(None);
        for account in accounts {
            let read = self
                .read_model_quota("agy", SHARED_MODEL_PATTERN, account)
                .await;
            if let AgyQuotaRead::Record(quota) = read {
                if agy_model_quota_blocks(&quota, now) {
                    return Some(quota);
                }
            }
        }
        None
    }

    async fn read_model_quota(
        &self,
        agent_kind: &str,
        model_pattern: &str,
        account_id: Option<&str>,
    ) -> AgyQuotaRead {
        match self
            .store
            .find_quota_by_agent_model(agent_kind, model_pattern, account_id)
            .await
        {
            Ok(Some(quota)) => AgyQuotaRead::Record(quota),
            Ok(None) => AgyQuotaRead::Missing,
            Err(err) => {
                warn!(
                    error = %err,
                    agent = %agent_kind,
                    model = %model_pattern,
                    "quota lookup failed"
                );
                AgyQuotaRead::Failed
            }
        }
    }

    pub async fn check_agent_quota(
        &self,
        agent_kind: &str,
        account_id: Option<&str>,
    ) -> Result<Option<QuotaSnapshotRecord>> {
        let normalized = agent_kind.trim().to_ascii_lowercase();
        let now = Utc::now();

        // Model-scoped agy rows must not suspend the whole agent. Only the
        // shared wildcard observation does; per-model admission is separate.
        if normalized == "agy" {
            return Ok(self.blocking_agy_wildcard_record(account_id, now).await);
        }

        if let Some(quota) = self.store.find_quota_by_agent(&normalized).await? {
            if quota_snapshot_is_exhausted(&quota) {
                return Ok(quota_snapshot_blocks(&quota, now).then_some(quota));
            } else if quota.status == "valid" {
                return Ok(None);
            }
        }

        // 2. Proactive check for grok if not yet recorded in store
        if normalized == "grok" {
            if let Some(info) = openduck_mcp::tui_session::profiles::grok::read_grok_quota_info() {
                if info.is_exhausted {
                    let is_still_exhausted = info
                        .end
                        .as_deref()
                        .and_then(|end| DateTime::parse_from_rfc3339(end).ok())
                        .is_some_and(|end| now < end);

                    if is_still_exhausted {
                        let existing = self.store.find_quota_by_agent(&normalized).await?;
                        let q_id = existing
                            .map(|e| e.id)
                            .unwrap_or_else(|| format!("quota_{}", uuid::Uuid::new_v4()));
                        let record = QuotaSnapshotRecord {
                            id: q_id,
                            pool_id: format!("pool_{normalized}"),
                            account_id: account_id.map(ToString::to_string),
                            agent_kind: Some(normalized.clone()),
                            model_pattern: Some("*".to_string()),
                            quota_type: info.quota_type,
                            unit: "percent".to_string(),
                            used_amount: info.credit_usage_percent,
                            limit_amount: Some(100.0),
                            remaining_percent: Some(0.0),
                            window_start: info.start,
                            window_end: info.end.clone(),
                            reset_at: info.end,
                            source: "cli_billing_log".to_string(),
                            status: "exhausted".to_string(),
                            observed_at: now.to_rfc3339(),
                        };
                        let _ = self.store.upsert_quota(&record).await;
                        return Ok(Some(record));
                    }
                }
            }
        }

        // 3. Also check account_id if provided
        if let Some(acc_id) = account_id {
            let quotas = self.store.list_quotas(Some(acc_id)).await?;
            if let Some(blocking) = quotas
                .into_iter()
                .find(|q| quota_snapshot_is_exhausted(q) && quota_snapshot_blocks(q, now))
            {
                return Ok(Some(blocking));
            }
        }

        Ok(None)
    }

    pub async fn record_quota_exhausted(
        &self,
        agent_kind: &str,
        account_id: Option<&str>,
        quota_type: &str,
        message: &str,
        reset_at: Option<String>,
    ) -> Result<QuotaSnapshotRecord> {
        self.record_quota_exhausted_scoped(
            agent_kind, account_id, None, quota_type, message, reset_at,
        )
        .await
    }

    async fn record_prompt_quota(
        &self,
        session: &TuiSessionRecord,
        quota_type: &str,
        message: &str,
        reset_at: Option<String>,
    ) -> Result<QuotaSnapshotRecord> {
        let model = prompt_quota_model(&session.agent_kind, session);
        self.record_quota_exhausted_scoped(
            &session.agent_kind,
            session.account_id.as_deref(),
            model,
            quota_type,
            message,
            reset_at,
        )
        .await
    }

    async fn record_quota_exhausted_scoped(
        &self,
        agent_kind: &str,
        account_id: Option<&str>,
        model: Option<&str>,
        quota_type: &str,
        _message: &str,
        reset_at: Option<String>,
    ) -> Result<QuotaSnapshotRecord> {
        let normalized = agent_kind.trim().to_ascii_lowercase();
        let now = Utc::now();
        let model_pattern = scoped_model_pattern(&normalized, model);

        let (exact_reset, window_start, window_end) = if normalized == "grok" {
            if let Some(info) = openduck_mcp::tui_session::profiles::grok::read_grok_quota_info() {
                (info.end.clone(), info.start, info.end)
            } else {
                (
                    reset_at.or_else(|| Some((now + chrono::Duration::days(7)).to_rfc3339())),
                    None,
                    None,
                )
            }
        } else {
            (
                reset_at.or_else(|| Some((now + chrono::Duration::days(7)).to_rfc3339())),
                None,
                None,
            )
        };

        let existing = if normalized == "agy" {
            match self
                .store
                .find_quota_by_agent_model(&normalized, &model_pattern, account_id)
                .await
            {
                Ok(found) => found,
                Err(err) => {
                    warn!(
                        error = %err,
                        agent = %normalized,
                        model = %model_pattern,
                        "quota lookup failed while recording exhaustion"
                    );
                    None
                }
            }
        } else {
            self.store.find_quota_by_agent(&normalized).await?
        };
        let q_id = existing
            .map(|e| e.id)
            .unwrap_or_else(|| format!("quota_{}", uuid::Uuid::new_v4()));

        let record = QuotaSnapshotRecord {
            id: q_id,
            pool_id: format!("pool_{normalized}"),
            account_id: account_id.map(ToString::to_string),
            agent_kind: Some(normalized.clone()),
            model_pattern: Some(model_pattern),
            quota_type: quota_type.to_string(),
            unit: "percent".to_string(),
            used_amount: 100.0,
            limit_amount: Some(100.0),
            remaining_percent: Some(0.0),
            window_start,
            window_end,
            reset_at: exact_reset,
            source: "tui_detection".to_string(),
            status: "exhausted".to_string(),
            observed_at: now.to_rfc3339(),
        };

        self.store.upsert_quota(&record).await?;

        self.emit_event(
            "quota_exhausted",
            None,
            None,
            serde_json::to_value(&record).unwrap_or_default(),
        );

        Ok(record)
    }

    pub async fn refresh_all_quotas_for_agent(
        &self,
        agent_kind: &str,
    ) -> Result<Vec<QuotaSnapshotRecord>> {
        let normalized = match agent_kind.trim().to_ascii_lowercase().as_str() {
            "antigravity" => "agy".to_string(),
            other => other.to_string(),
        };
        let now = Utc::now().to_rfc3339();

        match normalized.as_str() {
            "grok" => {
                if let Some(info) =
                    openduck_mcp::tui_session::profiles::grok::read_grok_quota_info()
                {
                    let existing = self.store.find_quota_by_agent(&normalized).await?;
                    let q_id = existing
                        .map(|e| e.id)
                        .unwrap_or_else(|| format!("quota_{}", uuid::Uuid::new_v4()));
                    let remaining = (100.0 - info.credit_usage_percent).max(0.0);
                    let record = QuotaSnapshotRecord {
                        id: q_id,
                        pool_id: format!("pool_{normalized}"),
                        account_id: None,
                        agent_kind: Some(normalized.clone()),
                        model_pattern: Some("*".to_string()),
                        quota_type: info.quota_type,
                        unit: "percent".to_string(),
                        used_amount: info.credit_usage_percent,
                        limit_amount: Some(100.0),
                        remaining_percent: Some(remaining),
                        window_start: info.start,
                        window_end: info.end.clone(),
                        reset_at: info.end,
                        source: "cli_billing_log".to_string(),
                        status: if info.is_exhausted {
                            "exhausted".to_string()
                        } else {
                            "valid".to_string()
                        },
                        observed_at: now,
                    };
                    self.store.upsert_quota(&record).await?;
                    return Ok(vec![record]);
                }
            }
            "codex" => {
                if let Some(info) =
                    openduck_mcp::tui_session::profiles::codex::read_codex_quota_info()
                {
                    let existing = self.store.list_quotas_by_agent(&normalized).await?;
                    let mut out = Vec::with_capacity(info.windows.len());
                    for window in info.windows {
                        let q_id = existing
                            .iter()
                            .find(|e| e.quota_type == window.quota_type)
                            .map(|e| e.id.clone())
                            .unwrap_or_else(|| format!("quota_codex_{}", window.quota_type));
                        let record = QuotaSnapshotRecord {
                            id: q_id,
                            pool_id: format!("pool_codex_{}", window.quota_type),
                            account_id: None,
                            agent_kind: Some(normalized.clone()),
                            model_pattern: Some("*".to_string()),
                            quota_type: window.quota_type,
                            unit: "percent".to_string(),
                            used_amount: window.used_percent,
                            limit_amount: Some(100.0),
                            remaining_percent: Some(window.remaining_percent),
                            window_start: None,
                            window_end: window.reset_at.clone(),
                            reset_at: window.reset_at,
                            source: "codex_session_log".to_string(),
                            status: if window.is_exhausted {
                                "exhausted".to_string()
                            } else {
                                "valid".to_string()
                            },
                            observed_at: info.observed_at.clone().unwrap_or_else(|| now.clone()),
                        };
                        self.store.upsert_quota(&record).await?;
                        out.push(record);
                    }
                    if !out.is_empty() {
                        return Ok(out);
                    }
                }
            }
            "agy" => {
                let profile = self.store.get_profile_by_agent(&normalized).await?;
                let proxy_mode = profile
                    .as_ref()
                    .map(|p| p.proxy_mode.as_str())
                    .unwrap_or("inherit");
                let proxy_id = profile.as_ref().and_then(|p| p.proxy_id.as_deref());
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let effective_proxy =
                    resolve_effective_proxy(proxy_mode, proxy_id, None, &normalized, &cwd);
                let agy_proxy = if proxy_mode == "direct" {
                    openduck_mcp::tui_session::profiles::agy::AgyQuotaProxy::Direct
                } else if let Some(ref url) = effective_proxy {
                    openduck_mcp::tui_session::profiles::agy::AgyQuotaProxy::Url(url.as_str())
                } else {
                    openduck_mcp::tui_session::profiles::agy::AgyQuotaProxy::SystemEnv
                };
                let info =
                    openduck_mcp::tui_session::profiles::agy::read_agy_quota_info_with_proxy(
                        agy_proxy,
                    )
                    .await?;
                let existing = self.store.list_quotas_by_agent(&normalized).await?;
                let mut out = Vec::with_capacity(info.pools.len());
                for pool in info.pools {
                    let pool_slug = pool.model_pattern.trim_end_matches("-*");
                    let q_id = existing
                        .iter()
                        .find(|e| e.model_pattern.as_deref() == Some(pool.model_pattern.as_str()))
                        .map(|e| e.id.clone())
                        .unwrap_or_else(|| format!("quota_agy_{pool_slug}"));
                    let record = QuotaSnapshotRecord {
                        id: q_id,
                        pool_id: format!("pool_agy_{pool_slug}"),
                        account_id: None,
                        agent_kind: Some(normalized.clone()),
                        model_pattern: Some(pool.model_pattern),
                        quota_type: "rolling".to_string(),
                        unit: "percent".to_string(),
                        used_amount: pool.used_percent,
                        limit_amount: Some(100.0),
                        remaining_percent: Some(pool.remaining_percent),
                        window_start: None,
                        window_end: pool.reset_at.clone(),
                        reset_at: pool.reset_at,
                        source: "agy_api".to_string(),
                        status: if pool.is_exhausted {
                            "exhausted".to_string()
                        } else {
                            "valid".to_string()
                        },
                        observed_at: now.clone(),
                    };
                    self.store.upsert_quota(&record).await?;
                    out.push(record);
                }
                if !out.is_empty() {
                    return Ok(out);
                }
            }
            _ => {}
        }

        self.store.list_quotas_by_agent(&normalized).await
    }

    pub async fn refresh_quotas_for_agent(
        &self,
        agent_kind: &str,
    ) -> Result<Option<QuotaSnapshotRecord>> {
        let mut records = self.refresh_all_quotas_for_agent(agent_kind).await?;
        if records.is_empty() {
            return Ok(None);
        }
        records.sort_by(|a, b| {
            a.remaining_percent
                .unwrap_or(100.0)
                .partial_cmp(&b.remaining_percent.unwrap_or(100.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(records.into_iter().next())
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
        let desired_model = resolved_model;
        let resolved_model = if kind == AgentCliKind::Agy {
            self.select_agy_startup_model(
                desired_model.as_deref(),
                req.account_id.as_deref(),
                Utc::now(),
            )
            .await?
        } else if let Some(exhausted) = self
            .check_agent_quota(&kind.to_string(), req.account_id.as_deref())
            .await?
        {
            return Err(suspended_agent_error(
                &kind.to_string(),
                &exhausted.quota_type,
                exhausted.reset_at.as_deref(),
            ));
        } else {
            desired_model
        };
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

        let effective_proxy = resolve_effective_proxy(
            &resolved_proxy_mode,
            resolved_proxy_id.as_deref(),
            req.https_proxy,
            &kind.to_string(),
            &working_dir,
        );

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

        if sess_rec.agent_kind.eq_ignore_ascii_case("agy") {
            self.reject_blocked_agy_prompt(&sess_rec).await?;
        } else if let Some(exhausted) = self
            .check_agent_quota(&sess_rec.agent_kind, sess_rec.account_id.as_deref())
            .await?
        {
            return Err(session_quota_error(
                &sess_rec.session_name,
                &sess_rec.agent_kind,
                None,
                &exhausted.quota_type,
                exhausted.reset_at.as_deref(),
            ));
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

                if let TuiPaneState::QuotaExceeded { ref message } = turn.state {
                    let _ = self
                        .record_prompt_quota(&updated, "weekly", message, None)
                        .await;
                } else if let Some(msg) =
                    detect_quota_limit(&[&turn.screen_snapshot, &turn.response])
                {
                    updated.tui_pane_state = "quota_exceeded".to_string();
                    let _ = self
                        .record_prompt_quota(&updated, "weekly", &msg, None)
                        .await;
                }

                let _ = self.store.update_session(&updated).await;

                self.emit_event(
                    "prompt_completed",
                    Some(updated.id),
                    Some(updated.session_name),
                    serde_json::json!({ "state": turn.state.label(), "duration_ms": dur }),
                );
            }
            Err(e) => {
                let err_str = e.to_string();
                if let Some(msg) = detect_quota_limit(&[&err_str]) {
                    let _ = self
                        .record_prompt_quota(&sess_rec, "weekly", &msg, None)
                        .await;
                    let mut updated = sess_rec;
                    updated.tui_pane_state = "quota_exceeded".to_string();
                    let _ = self.store.update_session(&updated).await;
                }
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

    pub async fn read_state_wait(
        &self,
        session_id_or_name: &str,
        scrollback_lines: Option<i32>,
        include_ansi: bool,
        wait_secs: Option<u64>,
    ) -> Result<TuiTurnResult> {
        let sess_rec = self
            .get_session_record(session_id_or_name)
            .await?
            .ok_or_else(|| anyhow!("Session '{}' not found", session_id_or_name))?;

        let handle = self
            .session_manager
            .get_session(&sess_rec.session_name)
            .await?;
        let res = handle
            .read_for_tool_with(scrollback_lines, include_ansi, wait_secs)
            .await?;

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

const SHARED_MODEL_PATTERN: &str = "*";

#[derive(Debug, Clone, PartialEq, Eq)]
enum AgyModelEligibility {
    Eligible,
    Blocked(AgyQuotaBlock),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgyQuotaBlock {
    quota_type: String,
    reset_at: Option<String>,
}

enum AgyQuotaRead {
    Record(QuotaSnapshotRecord),
    Missing,
    Failed,
}

struct AgyCandidate<'a> {
    model: &'a str,
    eligibility: AgyModelEligibility,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AgyModelChoice {
    Selected(String),
    AllBlocked {
        quota_type: String,
        reset_at: Option<String>,
        models: Vec<String>,
    },
}

fn nonempty_model(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|model| !model.is_empty())
}

fn session_active_quota_model(session: &TuiSessionRecord) -> Option<&str> {
    nonempty_model(session.active_model.as_deref())
        .or_else(|| nonempty_model(session.current_model.as_deref()))
        .or_else(|| nonempty_model(session.model.as_deref()))
}

fn prompt_quota_model<'a>(agent_kind: &str, session: &'a TuiSessionRecord) -> Option<&'a str> {
    if agent_kind.eq_ignore_ascii_case("agy") {
        session_active_quota_model(session)
    } else {
        None
    }
}

fn scoped_model_pattern(agent_kind: &str, model: Option<&str>) -> String {
    if agent_kind == "agy" {
        nonempty_model(model)
            .unwrap_or(SHARED_MODEL_PATTERN)
            .to_string()
    } else {
        SHARED_MODEL_PATTERN.to_string()
    }
}

fn push_candidate(candidates: &mut Vec<String>, model: &str) {
    let model = model.trim();
    if model.is_empty() || candidates.iter().any(|existing| existing == model) {
        return;
    }
    candidates.push(model.to_string());
}

fn agy_candidate_models(
    desired: Option<&str>,
    default_model: Option<&str>,
    models: &[CatalogModelEntry],
) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(model) = nonempty_model(desired) {
        push_candidate(&mut candidates, model);
    } else if let Some(model) = nonempty_model(default_model) {
        push_candidate(&mut candidates, model);
    }
    for model in models {
        push_candidate(&mut candidates, &model.id);
    }
    candidates
}

/// Missing, stale, expired, failed, and non-observation reads stay eligible.
/// A model is blocked only by a fresh exhausted observation whose reset is still ahead.
fn agy_eligibility(read: &AgyQuotaRead, now: DateTime<Utc>) -> AgyModelEligibility {
    match read {
        AgyQuotaRead::Record(quota) if agy_model_quota_blocks(quota, now) => {
            AgyModelEligibility::Blocked(AgyQuotaBlock {
                quota_type: quota.quota_type.clone(),
                reset_at: quota.reset_at.clone(),
            })
        }
        AgyQuotaRead::Record(_) | AgyQuotaRead::Missing | AgyQuotaRead::Failed => {
            AgyModelEligibility::Eligible
        }
    }
}

fn agy_model_quota_blocks(quota: &QuotaSnapshotRecord, now: DateTime<Utc>) -> bool {
    if !agy_status_is_quota_observation(&quota.status) {
        return false;
    }
    quota_observation_is_fresh(quota, now)
        && quota_snapshot_is_exhausted(quota)
        && quota_snapshot_blocks(quota, now)
}

fn agy_status_is_quota_observation(status: &str) -> bool {
    !matches!(
        status,
        "failed" | "error" | "unavailable" | "unsupported" | "unknown"
    )
}

fn quota_observation_is_fresh(quota: &QuotaSnapshotRecord, now: DateTime<Utc>) -> bool {
    let Some(observed) = DateTime::parse_from_rfc3339(&quota.observed_at)
        .ok()
        .map(|observed| observed.with_timezone(&Utc))
    else {
        return false;
    };
    if observed > now {
        return false;
    }
    now.signed_duration_since(observed)
        <= chrono::Duration::seconds(DEFAULT_QUOTA_STALE_AFTER_SECONDS as i64)
}

fn choose_agy_model(candidates: &[AgyCandidate<'_>]) -> Option<AgyModelChoice> {
    if candidates.is_empty() {
        return None;
    }
    for candidate in candidates {
        if candidate.eligibility == AgyModelEligibility::Eligible {
            return Some(AgyModelChoice::Selected(candidate.model.to_string()));
        }
    }
    let quota_type = candidates
        .iter()
        .find_map(|candidate| match &candidate.eligibility {
            AgyModelEligibility::Blocked(block) => Some(block.quota_type.clone()),
            AgyModelEligibility::Eligible => None,
        })
        .unwrap_or_else(|| "quota".to_string());
    let reset_at = candidates
        .iter()
        .filter_map(|candidate| match &candidate.eligibility {
            AgyModelEligibility::Blocked(block) => block.reset_at.as_deref().and_then(|reset| {
                DateTime::parse_from_rfc3339(reset)
                    .ok()
                    .map(|parsed| (parsed.with_timezone(&Utc), reset.to_string()))
            }),
            AgyModelEligibility::Eligible => None,
        })
        .max_by_key(|(parsed, _)| *parsed)
        .map(|(_, reset)| reset);
    Some(AgyModelChoice::AllBlocked {
        quota_type,
        reset_at,
        models: candidates
            .iter()
            .map(|candidate| candidate.model.to_string())
            .collect(),
    })
}

fn reset_phrase(reset_at: Option<&str>) -> String {
    match reset_at {
        Some(reset) => format!("resets at {reset}"),
        None => "waiting for limit reset".to_string(),
    }
}

fn suspended_agent_error(agent: &str, quota_type: &str, reset_at: Option<&str>) -> anyhow::Error {
    let reset_info = reset_phrase(reset_at);
    anyhow!(
        "Cannot use agent '{agent}': hit {quota_type} limit ({reset_info}). Agent is suspended until quota resets."
    )
}

fn all_agy_models_blocked_error(
    quota_type: &str,
    reset_at: Option<&str>,
    models: &[String],
) -> anyhow::Error {
    let reset_info = reset_phrase(reset_at);
    let listed = models.join(", ");
    anyhow!(
        "Cannot use agent 'agy': hit {quota_type} limit for all known models ({listed}) ({reset_info}). Agent is suspended until quota resets."
    )
}

fn session_quota_error(
    session_name: &str,
    agent: &str,
    model: Option<&str>,
    quota_type: &str,
    reset_at: Option<&str>,
) -> anyhow::Error {
    let reset_info = reset_phrase(reset_at);
    match model {
        Some(model) => anyhow!(
            "Cannot use session '{session_name}' (agent '{agent}', model '{model}'): hit {quota_type} limit ({reset_info}). Agent is suspended until quota resets."
        ),
        None => anyhow!(
            "Cannot use session '{session_name}' (agent '{agent}'): hit {quota_type} limit ({reset_info}). Agent is suspended until quota resets."
        ),
    }
}

fn quota_snapshot_is_exhausted(quota: &QuotaSnapshotRecord) -> bool {
    quota.status == "exhausted" || quota.remaining_percent == Some(0.0)
}

fn quota_snapshot_blocks(quota: &QuotaSnapshotRecord, now: DateTime<Utc>) -> bool {
    quota
        .reset_at
        .as_deref()
        .and_then(|reset| DateTime::parse_from_rfc3339(reset).ok())
        .is_some_and(|reset| reset.with_timezone(&Utc) > now)
}

#[cfg(test)]
mod snapshot_gate_tests {
    use super::{quota_snapshot_blocks, quota_snapshot_is_exhausted, QuotaSnapshotRecord};
    use chrono::{DateTime, Utc};

    fn snapshot(reset_at: Option<&str>) -> QuotaSnapshotRecord {
        QuotaSnapshotRecord {
            id: "q".into(),
            pool_id: "p".into(),
            account_id: None,
            agent_kind: Some("grok".into()),
            model_pattern: None,
            quota_type: "credits".into(),
            unit: "percent".into(),
            used_amount: 100.0,
            limit_amount: Some(100.0),
            remaining_percent: Some(0.0),
            window_start: None,
            window_end: None,
            reset_at: reset_at.map(Into::into),
            source: "test".into(),
            status: "exhausted".into(),
            observed_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn now() -> DateTime<Utc> {
        "2026-01-01T12:00:00Z".parse().unwrap()
    }

    #[test]
    fn blocks_only_for_future_parseable_reset() {
        assert!(quota_snapshot_is_exhausted(&snapshot(None)));
        assert!(quota_snapshot_blocks(
            &snapshot(Some("2026-01-01T12:00:01Z")),
            now()
        ));
        assert!(quota_snapshot_blocks(
            &snapshot(Some("2026-01-01T08:00:00-05:00")),
            now()
        ));
        assert!(!quota_snapshot_blocks(
            &snapshot(Some("2026-01-01T12:00:00Z")),
            now()
        ));
        assert!(!quota_snapshot_blocks(
            &snapshot(Some("2025-12-31T00:00:00Z")),
            now()
        ));
        assert!(!quota_snapshot_blocks(&snapshot(Some("soon")), now()));
        assert!(!quota_snapshot_blocks(&snapshot(None), now()));
    }
}

#[cfg(test)]
mod admission_tests {
    use super::super::quota::{
        QuotaAdmission, QuotaAgentStatus, QuotaAvailability, QuotaCollection, QuotaObservation,
        QuotaReset, QuotaScope, NORMAL_SAMPLING_WEIGHT,
    };
    use super::TuiManagementService;
    use chrono::{DateTime, Utc};

    fn ts(epoch: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(epoch, 0).expect("timestamp")
    }

    #[test]
    fn admit_agent_quota_uses_policy_without_mutating_observation() {
        let observed_at = ts(1_000);
        let reset_at = ts(5_000);
        let observation = QuotaObservation::try_new(
            QuotaScope::try_from_parts("grok", None, None, "credits", "weekly").expect("scope"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::at(reset_at),
            "cli_billing_log",
            observed_at,
        )
        .expect("observation");
        let mut book = QuotaAgentStatus::never_observed(observation.scope.agent).apply_refresh(
            observed_at,
            QuotaCollection::Observed(vec![observation.clone()]),
        );
        book.stale_after_seconds = 10_000;
        book.statuses[0].stale_after_seconds = 10_000;
        let before = book.clone();

        assert_eq!(
            TuiManagementService::admit_agent_quota(&book, ts(1_200)),
            QuotaAdmission::Blocked { until: reset_at }
        );
        assert_eq!(
            TuiManagementService::admit_agent_quota(&book, reset_at),
            QuotaAdmission::Eligible {
                weight: NORMAL_SAMPLING_WEIGHT,
            }
        );
        assert_eq!(book, before);
        assert_eq!(book.statuses[0].observation.as_ref(), Some(&observation));
    }
}

#[cfg(test)]
mod agy_model_quota_tests {
    use std::sync::Arc;

    use chrono::{DateTime, Duration, Utc};
    use openduck_mcp::tui_session::{
        AgentCliKind, AgentModelCatalog, CatalogModelEntry, ModelLoader, ModelSource,
        TuiSessionManager,
    };

    use super::super::models::{QuotaSnapshotRecord, TuiSessionRecord};
    use super::super::quota::DEFAULT_QUOTA_STALE_AFTER_SECONDS;
    use super::{
        agy_candidate_models, agy_eligibility, choose_agy_model, prompt_quota_model,
        scoped_model_pattern, AgyCandidate, AgyModelChoice, AgyModelEligibility, AgyQuotaRead,
        TuiManagementService,
    };

    struct FixedAgyModels(Vec<CatalogModelEntry>);

    #[async_trait::async_trait]
    impl ModelLoader for FixedAgyModels {
        async fn load_models(&self, agent: AgentCliKind) -> anyhow::Result<Vec<CatalogModelEntry>> {
            if agent == AgentCliKind::Agy {
                Ok(self.0.clone())
            } else {
                Ok(Vec::new())
            }
        }
    }

    fn ts(epoch: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(epoch, 0).expect("timestamp")
    }

    fn catalog_model(id: &str, is_default: bool) -> CatalogModelEntry {
        CatalogModelEntry {
            id: id.to_string(),
            display_name: id.to_string(),
            description: None,
            is_default,
            source: ModelSource::DynamicLoader,
        }
    }

    fn models() -> Vec<CatalogModelEntry> {
        vec![
            catalog_model("zeta-model", false),
            catalog_model("mid-model", true),
            catalog_model("alpha-model", false),
        ]
    }

    fn snapshot(
        status: &str,
        remaining: Option<f64>,
        reset_at: Option<&str>,
        observed_at: &str,
    ) -> QuotaSnapshotRecord {
        QuotaSnapshotRecord {
            id: "q".into(),
            pool_id: "pool_agy".into(),
            account_id: None,
            agent_kind: Some("agy".into()),
            model_pattern: Some("zeta-model".into()),
            quota_type: "weekly".into(),
            unit: "percent".into(),
            used_amount: 100.0,
            limit_amount: Some(100.0),
            remaining_percent: remaining,
            window_start: None,
            window_end: None,
            reset_at: reset_at.map(str::to_string),
            source: "test".into(),
            status: status.into(),
            observed_at: observed_at.into(),
        }
    }

    fn eligibility(
        status: &str,
        remaining: Option<f64>,
        reset_at: Option<&str>,
        observed_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> AgyModelEligibility {
        let record = snapshot(status, remaining, reset_at, &observed_at.to_rfc3339());
        agy_eligibility(&AgyQuotaRead::Record(record), now)
    }

    fn blocked<'a>(model: &'a str, quota_type: &str, reset_at: &str) -> AgyCandidate<'a> {
        AgyCandidate {
            model,
            eligibility: AgyModelEligibility::Blocked(super::AgyQuotaBlock {
                quota_type: quota_type.to_string(),
                reset_at: Some(reset_at.to_string()),
            }),
        }
    }

    fn eligible(model: &str) -> AgyCandidate<'_> {
        AgyCandidate {
            model,
            eligibility: AgyModelEligibility::Eligible,
        }
    }

    fn session(agent: &str, active: Option<&str>, model: Option<&str>) -> TuiSessionRecord {
        TuiSessionRecord {
            id: "sess".into(),
            session_name: "sess-name".into(),
            agent_kind: agent.into(),
            cwd: "/tmp".into(),
            model: model.map(str::to_string),
            active_model: active.map(str::to_string),
            current_model: None,
            effort: None,
            auto_approve: true,
            proxy_mode: "inherit".into(),
            proxy_id: None,
            effective_proxy: None,
            account_id: None,
            created_by_project_id: None,
            created_by_task_id: None,
            created_by_run_id: None,
            created_by_subtask_id: None,
            creation_source: "test".into(),
            owner_run_id: None,
            ownership_mode: "manual".into(),
            generation: 1,
            lifecycle_state: "active".into(),
            process_state: "alive".into(),
            tui_pane_state: "idle".into(),
            stop_intent: None,
            cleanup_error: None,
            created_at: "2026-11-13T00:00:00Z".into(),
            last_operation_at: None,
            last_input_at: None,
            last_progress_at: None,
            last_observed_at: None,
            idle_since: None,
            owner_terminal_at: None,
            reclaim_after: None,
            stopped_at: None,
            reclaimed_at: None,
        }
    }

    async fn service_with(catalog_models: Vec<CatalogModelEntry>) -> TuiManagementService {
        let store = Arc::new(
            super::super::store::SqliteTuiStore::open(":memory:")
                .await
                .expect("memory store"),
        );
        let catalog = Arc::new(AgentModelCatalog::new());
        catalog
            .set_loader(AgentCliKind::Agy, Arc::new(FixedAgyModels(catalog_models)))
            .await;
        TuiManagementService::with_catalog(store, Arc::new(TuiSessionManager::default()), catalog)
    }

    async fn insert_quota(
        service: &TuiManagementService,
        id: &str,
        model: &str,
        account: Option<&str>,
        status: &str,
        reset_at: DateTime<Utc>,
        observed_at: DateTime<Utc>,
    ) {
        service
            .store
            .upsert_quota(&QuotaSnapshotRecord {
                id: id.into(),
                pool_id: "pool_agy".into(),
                account_id: account.map(str::to_string),
                agent_kind: Some("agy".into()),
                model_pattern: Some(model.into()),
                quota_type: "weekly".into(),
                unit: "percent".into(),
                used_amount: 100.0,
                limit_amount: Some(100.0),
                remaining_percent: Some(0.0),
                window_start: None,
                window_end: Some(reset_at.to_rfc3339()),
                reset_at: Some(reset_at.to_rfc3339()),
                source: "test".into(),
                status: status.into(),
                observed_at: observed_at.to_rfc3339(),
            })
            .await
            .expect("upsert quota");
    }

    #[test]
    fn candidate_order_is_requested_then_stable_catalog_order() {
        let listed = models();
        assert_eq!(
            agy_candidate_models(Some("zeta-model"), Some("mid-model"), &listed),
            vec!["zeta-model", "mid-model", "alpha-model"]
        );
        assert_eq!(
            agy_candidate_models(None, Some("mid-model"), &listed),
            vec!["mid-model", "zeta-model", "alpha-model"]
        );
        assert_eq!(
            agy_candidate_models(Some("custom-model"), None, &listed),
            vec!["custom-model", "zeta-model", "mid-model", "alpha-model"]
        );
    }

    #[test]
    fn unknown_stale_expired_and_failed_reads_stay_eligible() {
        let now = ts(1_700_000_000);
        let future = now + Duration::hours(2);
        let past = now - Duration::hours(1);
        let fresh = now - Duration::seconds(1);
        let stale_after = i64::try_from(DEFAULT_QUOTA_STALE_AFTER_SECONDS).expect("stale window");
        let still_fresh = now - Duration::seconds(stale_after);
        let stale = now - Duration::seconds(stale_after + 1);

        assert_eq!(
            agy_eligibility(&AgyQuotaRead::Missing, now),
            AgyModelEligibility::Eligible
        );
        assert_eq!(
            agy_eligibility(&AgyQuotaRead::Failed, now),
            AgyModelEligibility::Eligible
        );
        assert_eq!(
            eligibility(
                "exhausted",
                Some(0.0),
                Some(&future.to_rfc3339()),
                stale,
                now
            ),
            AgyModelEligibility::Eligible
        );
        assert_eq!(
            eligibility("exhausted", Some(0.0), Some(&past.to_rfc3339()), fresh, now),
            AgyModelEligibility::Eligible
        );
        assert_eq!(
            eligibility("exhausted", Some(0.0), Some("next week"), fresh, now),
            AgyModelEligibility::Eligible
        );
        assert_eq!(
            eligibility("exhausted", Some(0.0), None, fresh, now),
            AgyModelEligibility::Eligible
        );
        assert_eq!(
            eligibility("failed", Some(0.0), Some(&future.to_rfc3339()), fresh, now),
            AgyModelEligibility::Eligible
        );
        assert_eq!(
            eligibility("unknown", Some(0.0), Some(&future.to_rfc3339()), fresh, now),
            AgyModelEligibility::Eligible
        );
        assert_eq!(
            eligibility(
                "exhausted",
                Some(0.0),
                Some(&future.to_rfc3339()),
                now + Duration::seconds(5),
                now
            ),
            AgyModelEligibility::Eligible
        );
        let unparsed = snapshot("exhausted", Some(0.0), Some(&future.to_rfc3339()), "soon");
        assert_eq!(
            agy_eligibility(&AgyQuotaRead::Record(unparsed), now),
            AgyModelEligibility::Eligible
        );
        assert_eq!(
            eligibility(
                "exhausted",
                Some(0.0),
                Some(&future.to_rfc3339()),
                still_fresh,
                now
            ),
            AgyModelEligibility::Blocked(super::AgyQuotaBlock {
                quota_type: "weekly".into(),
                reset_at: Some(future.to_rfc3339()),
            })
        );
        assert_eq!(
            eligibility("valid", Some(0.0), Some(&future.to_rfc3339()), fresh, now),
            AgyModelEligibility::Blocked(super::AgyQuotaBlock {
                quota_type: "weekly".into(),
                reset_at: Some(future.to_rfc3339()),
            })
        );
    }

    #[test]
    fn choose_falls_back_in_order_until_one_is_eligible() {
        let choice = choose_agy_model(&[
            blocked("zeta-model", "weekly", "2026-12-01T00:00:00Z"),
            eligible("mid-model"),
            blocked("alpha-model", "weekly", "2026-12-02T00:00:00Z"),
        ]);
        assert_eq!(
            choice,
            Some(AgyModelChoice::Selected("mid-model".to_string()))
        );

        let blocked_choice = choose_agy_model(&[
            blocked("zeta-model", "weekly", "2026-12-01T00:00:00Z"),
            blocked("mid-model", "daily", "2026-12-03T00:00:00Z"),
        ])
        .expect("decision");
        assert_eq!(
            blocked_choice,
            AgyModelChoice::AllBlocked {
                quota_type: "weekly".into(),
                reset_at: Some("2026-12-03T00:00:00Z".into()),
                models: vec!["zeta-model".into(), "mid-model".into()],
            }
        );
        assert!(choose_agy_model(&[]).is_none());
    }

    #[test]
    fn prompt_model_pattern_uses_agy_active_model_only() {
        let agy = session("agy", Some("mid-model"), Some("zeta-model"));
        assert_eq!(prompt_quota_model("agy", &agy), Some("mid-model"));
        assert_eq!(
            scoped_model_pattern("agy", prompt_quota_model("agy", &agy)),
            "mid-model"
        );
        let configured = session("agy", Some("  "), Some("zeta-model"));
        assert_eq!(prompt_quota_model("agy", &configured), Some("zeta-model"));
        let grok = session("grok", Some("grok-4.6"), Some("grok-4.6"));
        assert_eq!(prompt_quota_model("grok", &grok), None);
        assert_eq!(scoped_model_pattern("grok", Some("grok-4.6")), "*");
        assert_eq!(scoped_model_pattern("agy", None), "*");
    }

    #[tokio::test]
    async fn fallback_follows_discovered_catalog_order() {
        let now = Utc::now();
        let reset = now + Duration::hours(4);
        let service = service_with(models()).await;
        let entry = service.catalog.get_models(AgentCliKind::Agy).await;
        assert_eq!(
            entry
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["zeta-model", "mid-model", "alpha-model"]
        );
        assert_eq!(entry.default_model.as_deref(), Some("mid-model"));

        insert_quota(
            &service,
            "q-zeta",
            "zeta-model",
            None,
            "exhausted",
            reset,
            now,
        )
        .await;
        let chosen = service
            .select_agy_startup_model(Some("zeta-model"), None, now)
            .await
            .expect("fallback");
        assert_eq!(chosen.as_deref(), Some("mid-model"));

        insert_quota(
            &service,
            "q-zeta-account",
            "zeta-model",
            Some("acct-1"),
            "exhausted",
            reset,
            now,
        )
        .await;
        let other_account = service
            .select_agy_startup_model(Some("zeta-model"), Some("acct-2"), now)
            .await
            .expect("other account");
        assert_eq!(other_account.as_deref(), Some("zeta-model"));
        let same_account = service
            .select_agy_startup_model(Some("zeta-model"), Some("acct-1"), now)
            .await
            .expect("same account");
        assert_eq!(same_account.as_deref(), Some("mid-model"));

        assert!(service
            .check_agent_quota("agy", None)
            .await
            .expect("agent gate")
            .is_none());
    }

    #[tokio::test]
    async fn all_known_models_exhausted_is_a_clear_error() {
        let now = Utc::now();
        let earlier = now + Duration::hours(1);
        let later = now + Duration::hours(5);
        let service = service_with(models()).await;
        insert_quota(
            &service,
            "q-zeta",
            "zeta-model",
            None,
            "exhausted",
            earlier,
            now,
        )
        .await;
        insert_quota(
            &service,
            "q-mid",
            "mid-model",
            None,
            "exhausted",
            later,
            now,
        )
        .await;
        insert_quota(
            &service,
            "q-alpha",
            "alpha-model",
            None,
            "exhausted",
            earlier,
            now,
        )
        .await;

        let err = service
            .select_agy_startup_model(Some("zeta-model"), None, now)
            .await
            .expect_err("all blocked");
        let message = err.to_string();
        assert!(message.contains("all known models"), "{message}");
        assert!(message.contains("zeta-model"), "{message}");
        assert!(message.contains("mid-model"), "{message}");
        assert!(message.contains("alpha-model"), "{message}");
        assert!(message.contains(&later.to_rfc3339()), "{message}");
        assert!(
            message.contains("suspended until quota resets"),
            "{message}"
        );

        insert_quota(&service, "q-wild", "*", None, "exhausted", later, now).await;
        let wildcard = service
            .select_agy_startup_model(Some("custom-model"), None, now)
            .await
            .expect_err("wildcard");
        let wildcard_message = wildcard.to_string();
        assert!(
            wildcard_message.contains("suspended until quota resets"),
            "{wildcard_message}"
        );
        assert!(
            !wildcard_message.contains("all known models"),
            "{wildcard_message}"
        );
        assert!(service
            .check_agent_quota("agy", None)
            .await
            .expect("wildcard gate")
            .is_some());
    }

    #[tokio::test]
    async fn missing_stale_and_expired_rows_do_not_block_the_requested_model() {
        let now = Utc::now();
        let future = now + Duration::hours(3);
        let past = now - Duration::hours(2);
        let stale_after = i64::try_from(DEFAULT_QUOTA_STALE_AFTER_SECONDS).expect("stale window");
        let service = service_with(models()).await;

        let missing = service
            .select_agy_startup_model(Some("zeta-model"), None, now)
            .await
            .expect("missing");
        assert_eq!(missing.as_deref(), Some("zeta-model"));

        insert_quota(
            &service,
            "q-zeta",
            "zeta-model",
            None,
            "exhausted",
            future,
            now - Duration::seconds(stale_after + 1),
        )
        .await;
        let stale = service
            .select_agy_startup_model(Some("zeta-model"), None, now)
            .await
            .expect("stale");
        assert_eq!(stale.as_deref(), Some("zeta-model"));

        insert_quota(
            &service,
            "q-zeta",
            "zeta-model",
            None,
            "exhausted",
            past,
            now,
        )
        .await;
        let expired = service
            .select_agy_startup_model(Some("zeta-model"), None, now)
            .await
            .expect("expired");
        assert_eq!(expired.as_deref(), Some("zeta-model"));

        let unset = service
            .select_agy_startup_model(None, None, now)
            .await
            .expect("default");
        assert_eq!(unset.as_deref(), Some("mid-model"));
    }

    #[tokio::test]
    async fn prompt_exhaustion_persists_under_the_agy_active_model() {
        let service = service_with(Vec::new()).await;
        let agy = session("agy", Some("mid-model"), Some("zeta-model"));
        let first = service
            .record_prompt_quota(&agy, "weekly", "hit weekly limit", None)
            .await
            .expect("record agy");
        assert_eq!(first.model_pattern.as_deref(), Some("mid-model"));
        assert_eq!(first.status, "exhausted");
        let second = service
            .record_prompt_quota(&agy, "weekly", "hit weekly limit", None)
            .await
            .expect("record agy again");
        assert_eq!(second.id, first.id);

        let alpha_session = session("agy", Some("alpha-model"), None);
        let alpha = service
            .record_prompt_quota(&alpha_session, "weekly", "hit", None)
            .await
            .expect("record alpha");
        assert_eq!(alpha.model_pattern.as_deref(), Some("alpha-model"));
        assert_ne!(alpha.id, first.id);

        let grok = session("grok", Some("grok-4.6"), Some("grok-4.6"));
        let grok_row = service
            .record_prompt_quota(&grok, "weekly", "hit", None)
            .await
            .expect("record grok");
        assert_eq!(grok_row.model_pattern.as_deref(), Some("*"));

        let stored_mid = service
            .store
            .find_quota_by_agent_model("agy", "mid-model", None)
            .await
            .expect("query mid")
            .expect("mid row");
        assert_eq!(stored_mid.id, first.id);
        let stored_alpha = service
            .store
            .find_quota_by_agent_model("agy", "alpha-model", None)
            .await
            .expect("query alpha")
            .expect("alpha row");
        assert_eq!(stored_alpha.id, alpha.id);
        assert!(service
            .store
            .find_quota_by_agent_model("agy", "*", None)
            .await
            .expect("no wildcard")
            .is_none());

        let err = service
            .reject_blocked_agy_prompt(&agy)
            .await
            .expect_err("active model blocked");
        let message = err.to_string();
        assert!(message.contains("mid-model"), "{message}");
        assert!(
            message.contains("suspended until quota resets"),
            "{message}"
        );
        service
            .reject_blocked_agy_prompt(&alpha_session)
            .await
            .expect_err("alpha also blocked");
        let zeta_session = session("agy", Some("zeta-model"), None);
        service
            .reject_blocked_agy_prompt(&zeta_session)
            .await
            .expect("unknown model stays eligible");
    }
}
