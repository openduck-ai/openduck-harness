use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::eval::datasets::jsonl::BenchmarkItem;
use crate::eval::task::{
    CommandVerifier, DiffVerifier, EnvironmentSpec, SubtaskSpec, TaskSpec, Verifier,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged, rename_all = "camelCase")]
pub enum TaskVerifierSpec {
    TaggedCommand {
        #[serde(rename = "type")]
        kind: String,
        command: String,
        #[serde(default, alias = "expected_exit_code")]
        expected_exit_code: Option<i32>,
        #[serde(default, alias = "expected_stdout")]
        expected_stdout: Option<String>,
    },
    SimpleCommand {
        command: String,
        #[serde(default, alias = "expected_exit_code")]
        expected_exit_code: Option<i32>,
        #[serde(default, alias = "expected_stdout")]
        expected_stdout: Option<String>,
    },
    Diff {
        #[serde(rename = "type")]
        kind: String,
        #[serde(alias = "file_path")]
        file_path: String,
        #[serde(alias = "expected_content")]
        expected_content: String,
    },
}

impl TaskVerifierSpec {
    pub fn build_verifier(&self) -> Arc<dyn Verifier> {
        match self {
            TaskVerifierSpec::TaggedCommand {
                command,
                expected_exit_code,
                expected_stdout,
                ..
            }
            | TaskVerifierSpec::SimpleCommand {
                command,
                expected_exit_code,
                expected_stdout,
            } => {
                let mut v = CommandVerifier::new(command.clone());
                if let Some(code) = expected_exit_code {
                    v.expected_exit_code = *code;
                }
                if let Some(stdout) = expected_stdout {
                    v = v.with_expected_stdout(stdout.clone());
                }
                Arc::new(v)
            }
            TaskVerifierSpec::Diff {
                file_path,
                expected_content,
                ..
            } => Arc::new(DiffVerifier::new(
                PathBuf::from(file_path),
                expected_content.clone(),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTaskDefinition {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub prompt: String,
    #[serde(default)]
    pub environment: Option<EnvironmentSpec>,
    #[serde(default)]
    pub verifier: Option<TaskVerifierSpec>,
    #[serde(default, alias = "max_turns")]
    pub max_turns: Option<usize>,
    #[serde(default, alias = "timeout_seconds")]
    pub timeout_seconds: Option<u64>,
    /// 5- or 6-field cron expression in the host local timezone.
    #[serde(
        default,
        alias = "cron_schedule",
        skip_serializing_if = "Option::is_none"
    )]
    pub cron: Option<String>,
    #[serde(default, alias = "schedule_paused", skip_serializing_if = "is_false")]
    pub schedule_paused: bool,
    #[serde(
        default,
        alias = "subtasks",
        alias = "sub_tasks",
        skip_serializing_if = "Option::is_none"
    )]
    pub subtasks: Option<Vec<SubtaskSpec>>,
    #[serde(
        default,
        alias = "auto_decompose",
        alias = "autoDecompose",
        alias = "decompose_subtasks",
        skip_serializing_if = "Option::is_none"
    )]
    pub auto_decompose: Option<bool>,
    #[serde(
        default,
        alias = "phase_max_turns",
        alias = "phaseMaxTurns",
        skip_serializing_if = "Option::is_none"
    )]
    pub phase_max_turns: Option<Vec<usize>>,
    /// When true, a run may append extra instructions without rewriting the saved prompt.
    #[serde(default, alias = "dynamic_prompt", skip_serializing_if = "is_false")]
    pub dynamic_prompt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTaskSummary {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub file_path: String,
    pub has_verifier: bool,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub schedule_paused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_run_at: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub currently_running: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dynamic_prompt: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Append one-run instructions to a task prompt.
///
/// Empty extra text leaves the saved prompt unchanged. A non-empty extra prompt is rejected
/// unless the task opted in with `dynamicPrompt`.
pub fn compose_dynamic_run_prompt(
    base_prompt: &str,
    dynamic_prompt: bool,
    extra_prompt: Option<&str>,
) -> Result<String> {
    let extra = extra_prompt.map(str::trim).filter(|text| !text.is_empty());
    let Some(extra) = extra else {
        return Ok(base_prompt.to_string());
    };
    if !dynamic_prompt {
        return Err(anyhow!(
            "This task does not accept a dynamic prompt. Enable dynamicPrompt to supply extra instructions when the task runs."
        ));
    }
    let base = base_prompt.trim_end();
    if base.is_empty() {
        return Ok(extra.to_string());
    }
    Ok(format!(
        "{base}\n\nAdditional instructions for this run:\n{extra}"
    ))
}

fn created_at_from_file_times(
    created: std::io::Result<SystemTime>,
    modified: std::io::Result<SystemTime>,
) -> (SystemTime, String) {
    let time = created.or(modified).unwrap_or(SystemTime::UNIX_EPOCH);
    let rfc3339 = DateTime::<Utc>::from(time).to_rfc3339_opts(SecondsFormat::Millis, true);
    (time, rfc3339)
}

pub struct ProjectTaskStore;

impl ProjectTaskStore {
    pub async fn list_tasks(tasks_dir: &Path) -> Result<Vec<ProjectTaskSummary>> {
        let mut summaries: Vec<(SystemTime, ProjectTaskSummary)> = Vec::new();
        let mut entries = match fs::read_dir(tasks_dir).await {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("Failed to read tasks dir: {:?}", tasks_dir));
            }
        };

        while let Some(entry) = entries.next_entry().await? {
            let metadata = match entry.metadata().await {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if !metadata.is_file() {
                continue;
            }
            let path = entry.path();
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if ext != "yaml" && ext != "yml" && ext != "json" {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path).await {
                if let Ok(task_def) = serde_yaml::from_str::<ProjectTaskDefinition>(&content) {
                    let name = task_def.name.unwrap_or_else(|| task_def.id.clone());
                    let (created, created_at) =
                        created_at_from_file_times(metadata.created(), metadata.modified());
                    summaries.push((
                        created,
                        ProjectTaskSummary {
                            id: task_def.id,
                            name,
                            category: task_def.category,
                            tags: task_def.tags,
                            file_path: path.to_string_lossy().to_string(),
                            has_verifier: task_def.verifier.is_some(),
                            created_at,
                            cron: task_def.cron,
                            schedule_paused: task_def.schedule_paused,
                            next_run_at: None,
                            currently_running: false,
                            dynamic_prompt: task_def.dynamic_prompt,
                        },
                    ));
                }
            }
        }

        summaries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
        Ok(summaries.into_iter().map(|(_, summary)| summary).collect())
    }

    pub async fn get_task(tasks_dir: &Path, task_id: &str) -> Result<ProjectTaskDefinition> {
        let candidates = [
            tasks_dir.join(format!("{}.yaml", task_id)),
            tasks_dir.join(format!("{}.yml", task_id)),
            tasks_dir.join(format!("{}.json", task_id)),
        ];

        for path in &candidates {
            if path.exists() {
                let content = fs::read_to_string(path)
                    .await
                    .with_context(|| format!("Failed to read task file: {:?}", path))?;
                let task_def: ProjectTaskDefinition = serde_yaml::from_str(&content)
                    .with_context(|| format!("Failed to parse task YAML from {:?}", path))?;
                return Ok(task_def);
            }
        }

        // If not matched directly by filename, check all files in tasks_dir
        if tasks_dir.exists() {
            let mut entries = fs::read_dir(tasks_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(content) = fs::read_to_string(&path).await {
                        if let Ok(task_def) =
                            serde_yaml::from_str::<ProjectTaskDefinition>(&content)
                        {
                            if task_def.id == task_id {
                                return Ok(task_def);
                            }
                        }
                    }
                }
            }
        }

        Err(anyhow!("Task '{}' not found in {:?}", task_id, tasks_dir))
    }

    pub async fn save_task(tasks_dir: &Path, task: &ProjectTaskDefinition) -> Result<PathBuf> {
        if !tasks_dir.exists() {
            fs::create_dir_all(tasks_dir)
                .await
                .with_context(|| format!("Failed to create tasks directory: {:?}", tasks_dir))?;
        }

        let file_path = tasks_dir.join(format!("{}.yaml", task.id));
        let yaml_str = serde_yaml::to_string(task).context("Failed to serialize task to YAML")?;
        fs::write(&file_path, yaml_str)
            .await
            .with_context(|| format!("Failed to write task file {:?}", file_path))?;
        Ok(file_path)
    }

    pub async fn delete_task(tasks_dir: &Path, task_id: &str) -> Result<()> {
        let candidates = [
            tasks_dir.join(format!("{}.yaml", task_id)),
            tasks_dir.join(format!("{}.yml", task_id)),
            tasks_dir.join(format!("{}.json", task_id)),
        ];

        for path in &candidates {
            if path.exists() {
                fs::remove_file(path)
                    .await
                    .with_context(|| format!("Failed to remove task file {:?}", path))?;
                return Ok(());
            }
        }

        Err(anyhow!(
            "Task '{}' not found for deletion in {:?}",
            task_id,
            tasks_dir
        ))
    }

    pub fn to_benchmark_item(
        task_def: &ProjectTaskDefinition,
        project_env: Option<&EnvironmentSpec>,
    ) -> BenchmarkItem {
        let mut env = project_env.cloned().unwrap_or_default();
        if let Some(task_env) = &task_def.environment {
            if let Some(base_image) = &task_env.base_image {
                env.base_image = Some(base_image.clone());
            }
            env.setup_commands.extend(task_env.setup_commands.clone());
            env.env_vars.extend(task_env.env_vars.clone());
        }

        let dataset = task_def
            .category
            .clone()
            .unwrap_or_else(|| "project-task".to_string());

        let mut spec = TaskSpec::new(&task_def.id, dataset, &task_def.prompt);
        spec.environment = env;
        if let Some(max_turns) = task_def.max_turns {
            spec.max_turns = Some(max_turns);
        }
        if let Some(timeout) = task_def.timeout_seconds {
            spec.timeout_seconds = Some(timeout);
        }
        spec.subtasks = task_def.subtasks.clone();
        spec.auto_decompose = task_def.auto_decompose;
        spec.phase_max_turns = task_def.phase_max_turns.clone();

        let verifier: Arc<dyn Verifier> = if let Some(v_spec) = &task_def.verifier {
            v_spec.build_verifier()
        } else {
            Arc::new(CommandVerifier::new("true"))
        };

        BenchmarkItem {
            task: spec,
            verifier,
        }
    }
}
