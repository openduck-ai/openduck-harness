use crate::sandbox::SandboxDriver;
use crate::types::ExecOptions;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentSpec {
    pub base_image: Option<String>,
    pub setup_commands: Vec<String>,
    pub env_vars: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationResult {
    pub passed: bool,
    pub details: String,
    pub logs: String,
}

#[async_trait]
pub trait Verifier: Send + Sync {
    async fn verify(&self, sandbox: &dyn SandboxDriver) -> Result<VerificationResult>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandVerifier {
    pub test_command: String,
    pub expected_exit_code: i32,
    pub expected_stdout_substring: Option<String>,
}

impl CommandVerifier {
    pub fn new(test_command: impl Into<String>) -> Self {
        Self {
            test_command: test_command.into(),
            expected_exit_code: 0,
            expected_stdout_substring: None,
        }
    }

    pub fn with_expected_stdout(mut self, substring: impl Into<String>) -> Self {
        self.expected_stdout_substring = Some(substring.into());
        self
    }
}

#[async_trait]
impl Verifier for CommandVerifier {
    async fn verify(&self, sandbox: &dyn SandboxDriver) -> Result<VerificationResult> {
        let opts = ExecOptions::default();
        let output = sandbox.exec_command(&self.test_command, &opts).await?;

        let exit_matches = output.exit_code == self.expected_exit_code;
        let stdout_matches = match &self.expected_stdout_substring {
            Some(sub) => output.stdout.contains(sub),
            None => true,
        };

        let passed = exit_matches && stdout_matches;
        let details = if passed {
            "Command verification succeeded".to_string()
        } else {
            format!(
                "Command verification failed: exit_code={}, stdout_matched={}",
                output.exit_code, stdout_matches
            )
        };

        Ok(VerificationResult {
            passed,
            details,
            logs: format!(
                "=== STDOUT ===\n{}\n=== STDERR ===\n{}",
                output.stdout, output.stderr
            ),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffVerifier {
    pub file_path: PathBuf,
    pub expected_content_contains: String,
}

impl DiffVerifier {
    pub fn new(file_path: PathBuf, expected_content_contains: impl Into<String>) -> Self {
        Self {
            file_path,
            expected_content_contains: expected_content_contains.into(),
        }
    }
}

#[async_trait]
impl Verifier for DiffVerifier {
    async fn verify(&self, sandbox: &dyn SandboxDriver) -> Result<VerificationResult> {
        let content_bytes = sandbox.read_file(&self.file_path).await?;
        let content_str = String::from_utf8_lossy(&content_bytes);

        let passed = content_str.contains(&self.expected_content_contains);
        let details = if passed {
            format!("File {:?} contains expected content", self.file_path)
        } else {
            format!(
                "File {:?} does not contain expected snippet: {}",
                self.file_path, self.expected_content_contains
            )
        };

        Ok(VerificationResult {
            passed,
            details,
            logs: content_str.to_string(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SubtaskSpec {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_files: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exploration_budget: Option<usize>,
}

impl SubtaskSpec {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            target_files: None,
            max_turns: None,
            exploration_budget: None,
        }
    }

    pub fn with_target_files(mut self, files: Vec<String>) -> Self {
        self.target_files = Some(files);
        self
    }

    pub fn with_max_turns(mut self, turns: usize) -> Self {
        self.max_turns = Some(turns);
        self
    }

    pub fn with_exploration_budget(mut self, budget: usize) -> Self {
        self.exploration_budget = Some(budget);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub id: String,
    pub dataset: String,
    pub repo: Option<String>,
    pub base_commit: Option<String>,
    pub problem_statement: String,
    pub environment: EnvironmentSpec,
    pub max_turns: Option<usize>,
    pub timeout_seconds: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "exploration_budget",
        alias = "explorationBudget",
        alias = "max_exploration_steps"
    )]
    pub exploration_budget: Option<usize>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "subtasks",
        alias = "sub_tasks"
    )]
    pub subtasks: Option<Vec<SubtaskSpec>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "auto_decompose",
        alias = "autoDecompose",
        alias = "decompose_subtasks"
    )]
    pub auto_decompose: Option<bool>,
    /// Per-phase turn budgets for auto-synthesized (or matching extracted) subtasks.
    /// When omitted, synthesized phases split `max_turns` with a 2:3:2 ratio.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "phase_max_turns",
        alias = "phaseMaxTurns"
    )]
    pub phase_max_turns: Option<Vec<usize>>,
}

impl TaskSpec {
    pub fn new(
        id: impl Into<String>,
        dataset: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            dataset: dataset.into(),
            repo: None,
            base_commit: None,
            problem_statement: prompt.into(),
            environment: EnvironmentSpec::default(),
            max_turns: Some(25),
            timeout_seconds: Some(300),
            exploration_budget: None,
            subtasks: None,
            auto_decompose: None,
            phase_max_turns: None,
        }
    }

    pub fn with_turns(mut self, turns: usize) -> Self {
        self.max_turns = Some(turns);
        self
    }

    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = Some(seconds);
        self
    }

    pub fn with_exploration_budget(mut self, budget: usize) -> Self {
        self.exploration_budget = Some(budget);
        self
    }

    pub fn with_subtasks(mut self, subtasks: Vec<SubtaskSpec>) -> Self {
        self.subtasks = Some(subtasks);
        self
    }

    pub fn with_auto_decompose(mut self, auto_decompose: bool) -> Self {
        self.auto_decompose = Some(auto_decompose);
        self
    }

    pub fn with_phase_max_turns(mut self, phase_max_turns: Vec<usize>) -> Self {
        self.phase_max_turns = Some(phase_max_turns);
        self
    }
}
