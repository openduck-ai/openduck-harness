use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::eval::subtask::SubtaskOutcome;
use crate::eval::task::SubtaskSpec;
use crate::types::RunStatus;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContinuationStopReason {
    ProviderError,
    Cancelled,
    Timeout,
    Failure,
}

impl ContinuationStopReason {
    pub fn from_run_status(status: RunStatus) -> Self {
        match status {
            RunStatus::Cancelled => Self::Cancelled,
            RunStatus::Timeout => Self::Timeout,
            RunStatus::Failure => Self::Failure,
            _ => Self::Failure,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinuationCheckpoint {
    pub task_id: String,
    #[serde(default)]
    pub source_job_id: String,
    pub created_at: DateTime<Utc>,
    pub stop_reason: ContinuationStopReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub compacted_summary: String,
    #[serde(default)]
    pub completed_subtasks: Vec<SubtaskOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_subtask_id: Option<String>,
    #[serde(default)]
    pub subtask_plan: Vec<SubtaskSpec>,
    #[serde(default)]
    pub step_count: usize,
    /// Advisor CLI sessions keyed by task/subtask id, then advisor binary name.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub advisor_sessions: HashMap<String, HashMap<String, String>>,
}

impl ContinuationCheckpoint {
    pub fn is_continuable(&self) -> bool {
        !matches!(self.stop_reason, ContinuationStopReason::Timeout)
            || !self.compacted_summary.is_empty()
            || !self.completed_subtasks.is_empty()
    }

    pub fn resume_index(&self, subtasks: &[SubtaskSpec]) -> usize {
        if let Some(resume_id) = &self.resume_subtask_id {
            if let Some(idx) = subtasks.iter().position(|s| &s.id == resume_id) {
                return idx;
            }
        }
        let mut idx = 0;
        for outcome in &self.completed_subtasks {
            if outcome.status != RunStatus::Success {
                break;
            }
            if idx < subtasks.len() && subtasks[idx].id == outcome.subtask_id {
                idx += 1;
            } else if let Some(found) = subtasks.iter().position(|s| s.id == outcome.subtask_id) {
                idx = found + 1;
            }
        }
        idx.min(subtasks.len())
    }

    pub fn prior_progress_prompt(&self) -> String {
        format!(
            "### Prior incomplete run summary\n{}\n\n[Continuation contract]: This is a new run seeded from a previous incomplete execution. The summary above is the source of truth for what already happened. A copy is at `.agent/continuation-summary.md`. Do NOT re-explore files already examined. Do NOT redo completed writes unless the summary marks them broken. Continue from Remaining work using the workspace as it stands.\n",
            self.compacted_summary.trim()
        )
    }

    pub const CONTINUATION_SUMMARY_PATH: &'static str = ".agent/continuation-summary.md";
}

pub fn is_continuable_status(status: RunStatus) -> bool {
    matches!(
        status,
        RunStatus::Failure | RunStatus::Cancelled | RunStatus::Timeout
    )
}
