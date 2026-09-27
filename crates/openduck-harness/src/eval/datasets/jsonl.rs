use crate::eval::task::{CommandVerifier, DiffVerifier, TaskSpec, Verifier};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlTaskEntry {
    pub instance_id: String,
    pub dataset: Option<String>,
    pub problem_statement: String,
    pub repo: Option<String>,
    pub base_commit: Option<String>,
    pub setup_commands: Option<Vec<String>>,
    pub test_command: Option<String>,
    pub expected_stdout: Option<String>,
    pub check_file: Option<String>,
    pub expected_file_content: Option<String>,
}

pub struct BenchmarkItem {
    pub task: TaskSpec,
    pub verifier: Arc<dyn Verifier>,
}

pub async fn load_jsonl_dataset(path: &Path) -> Result<Vec<BenchmarkItem>> {
    let content = fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read dataset JSONL file: {:?}", path))?;

    let mut items = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let entry: JsonlTaskEntry = serde_json::from_str(trimmed)
            .with_context(|| format!("Failed to parse line {} as JsonlTaskEntry", line_idx + 1))?;

        let dataset_name = entry.dataset.unwrap_or_else(|| "default-benchmark".into());

        let mut task = TaskSpec::new(&entry.instance_id, &dataset_name, &entry.problem_statement);
        task.repo = entry.repo;
        task.base_commit = entry.base_commit;

        if let Some(cmds) = entry.setup_commands {
            task.environment.setup_commands = cmds;
        }

        let verifier: Arc<dyn Verifier> = if let Some(cmd) = entry.test_command {
            let mut v = CommandVerifier::new(cmd);
            if let Some(out) = entry.expected_stdout {
                v = v.with_expected_stdout(out);
            }
            Arc::new(v)
        } else if let Some(file_str) = entry.check_file {
            let expected = entry.expected_file_content.unwrap_or_default();
            Arc::new(DiffVerifier::new(PathBuf::from(file_str), expected))
        } else {
            Arc::new(CommandVerifier::new("true"))
        };

        items.push(BenchmarkItem { task, verifier });
    }

    Ok(items)
}
