use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Succeeded,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Succeeded => write!(f, "Succeeded"),
            Self::Failed => write!(f, "Failed"),
            Self::Cancelled => write!(f, "Cancelled"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionReport {
    pub job_id: String,
    pub job_name: String,
    pub session_id: String,
    pub trigger_type: String,
    pub status: TaskStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_seconds: u64,
    pub total_tokens_used: Option<u64>,
    pub model_name: Option<String>,
    pub project_id: Option<String>,
    pub summary_result: Option<String>,
    pub error_message: Option<String>,
    pub log_url: Option<String>,
}

impl TaskExecutionReport {
    pub fn is_success(&self) -> bool {
        matches!(self.status, TaskStatus::Succeeded)
    }

    pub fn is_failure(&self) -> bool {
        matches!(self.status, TaskStatus::Failed)
    }
}
