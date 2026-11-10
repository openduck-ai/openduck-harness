use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;

use super::config::{PolicyConfig, ProjectProviderEntry};
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
    /// When true, a run may append extra instructions without rewriting the saved prompt.
    #[serde(default, alias = "dynamic_prompt", skip_serializing_if = "is_false")]
    pub dynamic_prompt: bool,
    /// Major provider for this task. Blank inherits the project policy, then the global provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Model for `provider`. Blank uses that provider's configured model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Fallback provider for this task. Blank inherits the project policy, then the global fallback.
    #[serde(
        default,
        alias = "secondary_provider",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_provider: Option<String>,
    /// Model for `secondary_provider`. Blank uses that provider's configured model.
    #[serde(
        default,
        alias = "secondary_model",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_model: Option<String>,
}

/// One-run provider overrides. Blank strings are unset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskProviderRequest {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub secondary_provider: Option<String>,
    pub secondary_model: Option<String>,
}

/// Provider and model a task run should use after request, task, and project policy are applied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedTaskProviders {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub secondary_provider: Option<String>,
    pub secondary_model: Option<String>,
}

fn trimmed_nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

fn blank_option(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else if trimmed.len() == text.len() {
            Some(text)
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// A set provider replaces both sides of the pair. A set model alone replaces only the model.
fn overlay_pair<'a>(
    layer_provider: Option<&'a str>,
    layer_model: Option<&'a str>,
    base_provider: Option<&'a str>,
    base_model: Option<&'a str>,
) -> (Option<&'a str>, Option<&'a str>) {
    let provider = trimmed_nonempty(layer_provider);
    let model = trimmed_nonempty(layer_model);
    if provider.is_some() {
        (provider, model)
    } else if model.is_some() {
        (base_provider, model)
    } else {
        (base_provider, base_model)
    }
}

fn entry_model(entry: &ProjectProviderEntry, secondary: bool) -> Option<String> {
    let preferred = if secondary {
        entry.model.as_deref().or(entry.secondary_model.as_deref())
    } else {
        entry.model.as_deref()
    };
    blank_option(preferred.map(str::to_string))
}

impl ProjectTaskDefinition {
    /// Drops blank provider fields so a cleared selection is not stored.
    pub fn normalize_providers(&mut self) {
        self.provider = blank_option(self.provider.take());
        self.model = blank_option(self.model.take());
        self.secondary_provider = blank_option(self.secondary_provider.take());
        self.secondary_model = blank_option(self.secondary_model.take());
    }
}

/// Resolves the providers for one task run.
///
/// Order is the run request, then the task file, then the project policy. A provider set at a
/// layer drops the model from the layers below it. A missing model is filled from that
/// provider's project entry. Project evaluation still uses the suite request and project policy.
pub fn resolve_task_providers(
    request: &TaskProviderRequest,
    task: &ProjectTaskDefinition,
    policy: &PolicyConfig,
    providers: &HashMap<String, ProjectProviderEntry>,
) -> ResolvedTaskProviders {
    let (provider, model) = overlay_pair(
        policy.provider.as_deref(),
        policy.model.as_deref(),
        None,
        None,
    );
    let (provider, model) = overlay_pair(
        task.provider.as_deref(),
        task.model.as_deref(),
        provider,
        model,
    );
    let (provider, model) = overlay_pair(
        request.provider.as_deref(),
        request.model.as_deref(),
        provider,
        model,
    );

    let (secondary_provider, secondary_model) = overlay_pair(
        policy.secondary_provider.as_deref(),
        policy.secondary_model.as_deref(),
        None,
        None,
    );
    let (secondary_provider, secondary_model) = overlay_pair(
        task.secondary_provider.as_deref(),
        task.secondary_model.as_deref(),
        secondary_provider,
        secondary_model,
    );
    let (secondary_provider, secondary_model) = overlay_pair(
        request.secondary_provider.as_deref(),
        request.secondary_model.as_deref(),
        secondary_provider,
        secondary_model,
    );

    let provider = provider.map(str::to_string);
    let secondary_provider = secondary_provider.map(str::to_string);
    let model = model.map(str::to_string).or_else(|| {
        provider
            .as_deref()
            .and_then(|name| providers.get(name))
            .and_then(|entry| entry_model(entry, false))
    });
    let secondary_model = secondary_model.map(str::to_string).or_else(|| {
        secondary_provider
            .as_deref()
            .and_then(|name| providers.get(name))
            .and_then(|entry| entry_model(entry, true))
    });

    ResolvedTaskProviders {
        provider,
        model,
        secondary_provider,
        secondary_model,
    }
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
    pub prompt_preview: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(
        default,
        alias = "secondary_provider",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_provider: Option<String>,
    #[serde(
        default,
        alias = "secondary_model",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_model: Option<String>,
}

/// Builds a short prompt preview from a task prompt.
///
/// Takes the first two non-empty trimmed lines, joined with `\n`, capped at 240
/// characters on a char boundary, with `…` if truncated. Returns `None` if the prompt is empty.
pub fn build_prompt_preview(prompt: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in prompt.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed);
            if lines.len() == 2 {
                break;
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    let joined = lines.join("\n");
    if joined.chars().count() <= 240 {
        Some(joined)
    } else {
        let truncated: String = joined.chars().take(240).collect();
        Some(format!("{truncated}…"))
    }
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
                            prompt_preview: build_prompt_preview(&task_def.prompt),
                            cron: task_def.cron,
                            schedule_paused: task_def.schedule_paused,
                            next_run_at: None,
                            currently_running: false,
                            dynamic_prompt: task_def.dynamic_prompt,
                            provider: blank_option(task_def.provider),
                            model: blank_option(task_def.model),
                            secondary_provider: blank_option(task_def.secondary_provider),
                            secondary_model: blank_option(task_def.secondary_model),
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

        let mut task = task.clone();
        task.normalize_providers();
        let file_path = tasks_dir.join(format!("{}.yaml", task.id));
        let yaml_str = serde_yaml::to_string(&task).context("Failed to serialize task to YAML")?;
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
