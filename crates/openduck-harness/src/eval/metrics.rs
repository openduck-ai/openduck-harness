use crate::eval::task::VerificationResult;
use crate::types::RunStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    #[serde(alias = "task_id")]
    pub task_id: String,
    pub dataset: String,
    pub status: RunStatus,
    pub passed: bool,
    #[serde(alias = "step_count")]
    pub step_count: usize,
    #[serde(alias = "tool_calls_count")]
    pub tool_calls_count: usize,
    #[serde(alias = "duration_ms")]
    pub duration_ms: u128,
    pub verification: Option<VerificationResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalMetrics {
    #[serde(alias = "total_tasks")]
    pub total_tasks: usize,
    #[serde(alias = "passed_tasks")]
    pub passed_tasks: usize,
    #[serde(alias = "failed_tasks")]
    pub failed_tasks: usize,
    #[serde(alias = "error_tasks")]
    pub error_tasks: usize,
    #[serde(alias = "pass_rate")]
    pub pass_rate: f64,
    #[serde(alias = "avg_duration_ms")]
    pub avg_duration_ms: f64,
    #[serde(alias = "avg_steps")]
    pub avg_steps: f64,
    #[serde(alias = "total_tool_calls")]
    pub total_tool_calls: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationReport {
    #[serde(alias = "suite_name")]
    pub suite_name: String,
    #[serde(alias = "policy_name")]
    pub policy_name: String,
    #[serde(alias = "started_at")]
    pub started_at: DateTime<Utc>,
    #[serde(alias = "completed_at")]
    pub completed_at: DateTime<Utc>,
    pub metrics: EvalMetrics,
    #[serde(alias = "task_results")]
    pub task_results: Vec<TaskResult>,
    pub metadata: HashMap<String, String>,
}

impl EvaluationReport {
    pub fn compute(
        suite_name: impl Into<String>,
        policy_name: impl Into<String>,
        started_at: DateTime<Utc>,
        task_results: Vec<TaskResult>,
    ) -> Self {
        let total = task_results.len();
        let passed = task_results.iter().filter(|r| r.passed).count();
        let errors = task_results
            .iter()
            .filter(|r| r.status != RunStatus::Success && !r.passed)
            .count();
        let failed = total.saturating_sub(passed);

        let pass_rate = if total > 0 {
            (passed as f64) / (total as f64)
        } else {
            0.0
        };

        let total_duration: u128 = task_results.iter().map(|r| r.duration_ms).sum();
        let avg_duration_ms = if total > 0 {
            (total_duration as f64) / (total as f64)
        } else {
            0.0
        };

        let total_steps: usize = task_results.iter().map(|r| r.step_count).sum();
        let avg_steps = if total > 0 {
            (total_steps as f64) / (total as f64)
        } else {
            0.0
        };

        let total_tool_calls: usize = task_results.iter().map(|r| r.tool_calls_count).sum();

        Self {
            suite_name: suite_name.into(),
            policy_name: policy_name.into(),
            started_at,
            completed_at: Utc::now(),
            metrics: EvalMetrics {
                total_tasks: total,
                passed_tasks: passed,
                failed_tasks: failed,
                error_tasks: errors,
                pass_rate,
                avg_duration_ms,
                avg_steps,
                total_tool_calls,
            },
            task_results,
            metadata: HashMap::new(),
        }
    }
}
