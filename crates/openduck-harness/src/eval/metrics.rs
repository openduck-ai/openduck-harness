use crate::eval::task::VerificationResult;
use crate::telemetry::TaskProviderStats;
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "provider_stats",
        alias = "providerStats"
    )]
    pub provider_stats: Option<TaskProviderStats>,
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "provider_stats",
        alias = "providerStats"
    )]
    pub provider_stats: Option<TaskProviderStats>,
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

        let mut agg_provider_stats = TaskProviderStats::default();
        let mut has_provider_stats = false;
        for tr in &task_results {
            if let Some(ps) = &tr.provider_stats {
                has_provider_stats = true;
                agg_provider_stats.total_calls += ps.total_calls;
                agg_provider_stats.successful_calls += ps.successful_calls;
                agg_provider_stats.failed_calls += ps.failed_calls;
                agg_provider_stats.total_retries += ps.total_retries;
                agg_provider_stats.total_duration_ms += ps.total_duration_ms;
                agg_provider_stats.input_tokens += ps.input_tokens;
                agg_provider_stats.output_tokens += ps.output_tokens;
                agg_provider_stats.total_tokens += ps.total_tokens;
                if let Some(c) = ps.total_cost {
                    let cur = agg_provider_stats.total_cost.unwrap_or(0.0);
                    agg_provider_stats.total_cost = Some(cur + c);
                }
                for (p_key, p_stat) in &ps.by_provider {
                    let entry = agg_provider_stats
                        .by_provider
                        .entry(p_key.clone())
                        .or_insert_with(|| crate::telemetry::ProviderCallStats::new(p_key, None));
                    entry.total_calls += p_stat.total_calls;
                    entry.successful_calls += p_stat.successful_calls;
                    entry.failed_calls += p_stat.failed_calls;
                    entry.total_duration_ms += p_stat.total_duration_ms;
                    entry.input_tokens += p_stat.input_tokens;
                    entry.output_tokens += p_stat.output_tokens;
                    entry.total_tokens += p_stat.total_tokens;
                    entry.retries += p_stat.retries;
                    if let Some(c) = p_stat.total_cost {
                        let cur = entry.total_cost.unwrap_or(0.0);
                        entry.total_cost = Some(cur + c);
                    }
                    for err in &p_stat.errors {
                        if !entry.errors.contains(err) {
                            entry.errors.push(err.clone());
                        }
                    }
                    if entry.total_calls > 0 {
                        entry.avg_duration_ms =
                            (entry.total_duration_ms as f64) / (entry.total_calls as f64);
                    }
                }
                for (m_key, m_stat) in &ps.by_model {
                    let entry = agg_provider_stats
                        .by_model
                        .entry(m_key.clone())
                        .or_insert_with(|| {
                            crate::telemetry::ProviderCallStats::new(
                                &m_stat.provider,
                                m_stat.model.clone(),
                            )
                        });
                    entry.total_calls += m_stat.total_calls;
                    entry.successful_calls += m_stat.successful_calls;
                    entry.failed_calls += m_stat.failed_calls;
                    entry.total_duration_ms += m_stat.total_duration_ms;
                    entry.input_tokens += m_stat.input_tokens;
                    entry.output_tokens += m_stat.output_tokens;
                    entry.total_tokens += m_stat.total_tokens;
                    entry.retries += m_stat.retries;
                    if let Some(c) = m_stat.total_cost {
                        let cur = entry.total_cost.unwrap_or(0.0);
                        entry.total_cost = Some(cur + c);
                    }
                    for err in &m_stat.errors {
                        if !entry.errors.contains(err) {
                            entry.errors.push(err.clone());
                        }
                    }
                    if entry.total_calls > 0 {
                        entry.avg_duration_ms =
                            (entry.total_duration_ms as f64) / (entry.total_calls as f64);
                    }
                }
            }
        }
        if agg_provider_stats.total_calls > 0 {
            agg_provider_stats.avg_duration_ms = (agg_provider_stats.total_duration_ms as f64)
                / (agg_provider_stats.total_calls as f64);
        }

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
                provider_stats: if has_provider_stats {
                    Some(agg_provider_stats)
                } else {
                    None
                },
            },
            task_results,
            metadata: HashMap::new(),
        }
    }
}
