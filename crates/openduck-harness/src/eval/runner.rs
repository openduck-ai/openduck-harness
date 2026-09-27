use crate::eval::datasets::jsonl::BenchmarkItem;
use crate::eval::metrics::{EvaluationReport, TaskResult};
use crate::policy::AgentPolicy;
use crate::runtime::AgentHarness;
use crate::sandbox::local::LocalSandbox;
use crate::sandbox::SandboxDriver;
use crate::telemetry::TrajectoryLogger;
use crate::types::RunStatus;
use anyhow::{Context, Result};
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct EvalRunnerConfig {
    pub concurrency: usize,
    pub output_dir: PathBuf,
    pub max_turns: usize,
}

impl Default for EvalRunnerConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            output_dir: PathBuf::from("harness_eval_results"),
            max_turns: 25,
        }
    }
}

pub struct EvalRunner {
    config: EvalRunnerConfig,
}

impl EvalRunner {
    pub fn new(config: EvalRunnerConfig) -> Self {
        Self { config }
    }

    pub async fn run_suite<P, F>(
        &self,
        suite_name: &str,
        items: Vec<BenchmarkItem>,
        policy_factory: F,
    ) -> Result<EvaluationReport>
    where
        P: AgentPolicy + 'static,
        F: Fn() -> P + Send + Sync + 'static,
    {
        let started_at = Utc::now();
        let semaphore = Arc::new(Semaphore::new(self.config.concurrency));
        let policy_sample = policy_factory();
        let policy_name = policy_sample.name().to_string();

        let traj_path = self.config.output_dir.join("trajectories.jsonl");
        let trajectory_logger = Arc::new(TrajectoryLogger::new(traj_path));

        let mut join_handles = Vec::new();

        for item in items {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let policy = policy_factory();
            let logger = trajectory_logger.clone();
            let max_turns = self.config.max_turns;

            let handle = tokio::spawn(async move {
                let _permit = permit;
                let task = item.task;
                let verifier = item.verifier;

                let sandbox = match LocalSandbox::ephemeral() {
                    Ok(s) => s,
                    Err(e) => {
                        return TaskResult {
                            task_id: task.id.clone(),
                            dataset: task.dataset.clone(),
                            status: RunStatus::Failure,
                            passed: false,
                            step_count: 0,
                            tool_calls_count: 0,
                            duration_ms: 0,
                            verification: None,
                            error: Some(format!("Sandbox initialization error: {}", e)),
                        };
                    }
                };

                let step_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let tool_calls_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let sc = step_count.clone();
                let tc = tool_calls_count.clone();
                let start_time = std::time::Instant::now();

                let mut harness = AgentHarness::new(policy, sandbox)
                    .with_max_turns(max_turns)
                    .with_trajectory_logger(logger)
                    .with_step_observer(Arc::new(move |steps| {
                        sc.store(steps.len(), std::sync::atomic::Ordering::Relaxed);
                        let tools: usize = steps
                            .iter()
                            .map(|s| s.tool_results.as_ref().map(Vec::len).unwrap_or(0))
                            .sum();
                        tc.store(tools, std::sync::atomic::Ordering::Relaxed);
                    }));

                let exec_res = harness.run_task(&task).await;

                let task_result = match exec_res {
                    Ok(exec) => {
                        let ver_res = verifier.verify(harness.sandbox()).await;
                        let (passed, ver_opt, ver_err) = match ver_res {
                            Ok(v) => (v.passed, Some(v), None),
                            Err(e) => (false, None, Some(format!("Verifier error: {}", e))),
                        };

                        let final_error = ver_err;

                        TaskResult {
                            task_id: task.id.clone(),
                            dataset: task.dataset.clone(),
                            status: exec.status,
                            passed,
                            step_count: exec.step_count,
                            tool_calls_count: exec.tool_calls_count,
                            duration_ms: exec.duration_ms,
                            verification: ver_opt,
                            error: final_error,
                        }
                    }
                    Err(e) => TaskResult {
                        task_id: task.id.clone(),
                        dataset: task.dataset.clone(),
                        status: RunStatus::Failure,
                        passed: false,
                        step_count: step_count.load(std::sync::atomic::Ordering::Relaxed),
                        tool_calls_count: tool_calls_count
                            .load(std::sync::atomic::Ordering::Relaxed),
                        duration_ms: start_time.elapsed().as_millis(),
                        verification: None,
                        error: Some(format!("Execution failed: {e:#}")),
                    },
                };

                let _ = harness.sandbox_mut().cleanup().await;
                task_result
            });

            join_handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in join_handles {
            if let Ok(res) = handle.await {
                results.push(res);
            }
        }

        let report = EvaluationReport::compute(suite_name, policy_name, started_at, results);

        let report_path = self.config.output_dir.join("eval_report.json");
        if let Some(parent) = report_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let report_json = serde_json::to_string_pretty(&report)
            .context("Failed to serialize evaluation report")?;
        tokio::fs::write(&report_path, report_json)
            .await
            .with_context(|| format!("Failed to write report to {:?}", report_path))?;

        Ok(report)
    }
}
