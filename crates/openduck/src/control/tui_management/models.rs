use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Starting,
    Active,
    Retained,
    ReclaimPending,
    Stopping,
    Reclaimed,
    CleanupFailed,
}

impl LifecycleState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Active => "active",
            Self::Retained => "retained",
            Self::ReclaimPending => "reclaim_pending",
            Self::Stopping => "stopping",
            Self::Reclaimed => "reclaimed",
            Self::CleanupFailed => "cleanup_failed",
        }
    }

    pub fn from_str_opt(s: &str) -> Self {
        match s {
            "starting" => Self::Starting,
            "active" => Self::Active,
            "retained" => Self::Retained,
            "reclaim_pending" => Self::ReclaimPending,
            "stopping" => Self::Stopping,
            "reclaimed" => Self::Reclaimed,
            "cleanup_failed" => Self::CleanupFailed,
            _ => Self::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    Alive,
    Exited,
    Unknown,
}

impl ProcessState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Alive => "alive",
            Self::Exited => "exited",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str_opt(s: &str) -> Self {
        match s {
            "alive" => Self::Alive,
            "exited" => Self::Exited,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipMode {
    RunExclusive,
    Manual,
    Shared,
}

impl OwnershipMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RunExclusive => "run_exclusive",
            Self::Manual => "manual",
            Self::Shared => "shared",
        }
    }

    pub fn from_str_opt(s: &str) -> Self {
        match s {
            "manual" => Self::Manual,
            "shared" => Self::Shared,
            _ => Self::RunExclusive,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    Inherit,
    Direct,
    Named,
    Custom,
}

impl ProxyMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::Direct => "direct",
            Self::Named => "named",
            Self::Custom => "custom",
        }
    }

    pub fn from_str_opt(s: &str) -> Self {
        match s {
            "direct" => Self::Direct,
            "named" => Self::Named,
            "custom" => Self::Custom,
            _ => Self::Inherit,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiSessionRecord {
    pub id: String,
    pub session_name: String,
    pub agent_kind: String,
    pub cwd: String,
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_model: Option<String>,
    pub effort: Option<String>,
    pub auto_approve: bool,
    pub proxy_mode: String,
    pub proxy_id: Option<String>,
    pub effective_proxy: Option<String>,
    pub account_id: Option<String>,
    pub created_by_project_id: Option<String>,
    pub created_by_task_id: Option<String>,
    pub created_by_run_id: Option<String>,
    pub created_by_subtask_id: Option<String>,
    pub creation_source: String,
    pub owner_run_id: Option<String>,
    pub ownership_mode: String,
    pub generation: u64,
    pub lifecycle_state: String,
    pub process_state: String,
    pub tui_pane_state: String,
    pub stop_intent: Option<String>,
    pub cleanup_error: Option<String>,
    pub created_at: String,
    pub last_operation_at: Option<String>,
    pub last_input_at: Option<String>,
    pub last_progress_at: Option<String>,
    pub last_observed_at: Option<String>,
    pub idle_since: Option<String>,
    pub owner_terminal_at: Option<String>,
    pub reclaim_after: Option<String>,
    pub stopped_at: Option<String>,
    pub reclaimed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRunBindingRecord {
    pub id: String,
    pub session_id: String,
    pub run_id: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub bound_at: String,
    pub unbound_at: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOperationRecord {
    pub id: String,
    pub session_id: String,
    pub session_name: String,
    pub operation_type: String,
    pub operator: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub subtask_id: Option<String>,
    pub summary: Option<String>,
    pub content_ref: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub duration_ms: Option<u64>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfileRecord {
    pub id: String,
    pub agent_kind: String,
    pub default_model: Option<String>,
    pub default_effort: Option<String>,
    pub account_id: Option<String>,
    pub proxy_mode: String,
    pub proxy_id: Option<String>,
    pub max_concurrency: Option<u32>,
    pub idle_timeout_mins: Option<u64>,
    pub exited_retention_mins: Option<u64>,
    pub max_turn_timeout_secs: Option<u64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRecord {
    pub id: String,
    pub provider: String,
    pub account_name: String,
    pub auth_ref: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSnapshotRecord {
    pub id: String,
    pub pool_id: String,
    pub account_id: Option<String>,
    pub agent_kind: Option<String>,
    pub model_pattern: Option<String>,
    pub quota_type: String,
    pub unit: String,
    pub used_amount: f64,
    pub limit_amount: Option<f64>,
    pub remaining_percent: Option<f64>,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub reset_at: Option<String>,
    pub source: String,
    pub status: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReservationRecord {
    pub id: String,
    pub session_id: String,
    pub account_id: Option<String>,
    pub run_id: Option<String>,
    pub amount: f64,
    pub unit: String,
    pub status: String,
    pub created_at: String,
    pub settled_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiManagementEvent {
    pub event_type: String,
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub timestamp: String,
    pub payload: serde_json::Value,
}
