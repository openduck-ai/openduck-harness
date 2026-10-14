use std::path::Path;

use crate::eval::task::{SubtaskExpectedOutput, SubtaskSpec, TaskSpec};
use crate::sandbox::SandboxDriver;
use crate::types::{ExecOptions, RunStatus};
use serde::{Deserialize, Serialize};

/// Summary of a completed subtask's execution in a multi-step harness run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtaskOutcome {
    pub subtask_id: String,
    pub title: String,
    pub status: RunStatus,
    pub modified_files: Vec<String>,
    pub summary: String,
    pub step_count: usize,
    #[serde(default)]
    pub steps_taken: usize,
    /// What the expected-output check reported. Empty when the subtask has no checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_check: Option<String>,
}

/// Result of checking a subtask's expected outputs in the shared sandbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputCheckReport {
    pub passed: bool,
    pub details: String,
}

pub struct SubtaskDecomposer;

impl SubtaskDecomposer {
    /// Formats the prompt sent to the agent for a specific subtask within a sequence.
    pub fn format_subtask_prompt(
        task: &TaskSpec,
        subtask: &SubtaskSpec,
        subtask_idx: usize,
        total_subtasks: usize,
        previous_outcomes: &[SubtaskOutcome],
    ) -> String {
        let mut p = format!(
            "# Overall Task Objective\n{}\n\n---\n\n## Current Subtask ({}/{}): {}\n**Goal**:\n{}\n",
            task.problem_statement.trim(),
            subtask_idx + 1,
            total_subtasks,
            subtask.title,
            subtask.description.trim()
        );

        if let Some(files) = &subtask.target_files {
            if !files.is_empty() {
                p.push_str("\n**Target Files**:\n");
                for f in files {
                    p.push_str(&format!("- `{f}`\n"));
                }
            }
        }

        if let Some(outputs) = &subtask.expected_outputs {
            let visible: Vec<&SubtaskExpectedOutput> = outputs
                .iter()
                .filter(|output| !output.description.trim().is_empty() || output_has_check(output))
                .collect();
            if !visible.is_empty() {
                p.push_str(
                    "\n**Expected outputs** (the next subtask starts only after these checks pass):\n",
                );
                for output in visible {
                    let description = output.description.trim();
                    if description.is_empty() {
                        p.push_str("- (no description)\n");
                    } else {
                        p.push_str(&format!("- {description}\n"));
                    }
                    if let Some(hint) = output_check_hint(output) {
                        p.push_str(&format!("  - Check: {hint}\n"));
                    }
                }
            }
        }

        if !previous_outcomes.is_empty() {
            p.push_str("\n### Completed Upstream Subtasks:\n");
            for o in previous_outcomes {
                let status_label = match o.status {
                    RunStatus::Success => "Completed",
                    RunStatus::Failure => "Failed (Issues remain)",
                    _ => "Done",
                };
                let files_str = if o.modified_files.is_empty() {
                    "None".to_string()
                } else {
                    o.modified_files.join(", ")
                };
                p.push_str(&format!(
                    "- **Subtask [{}] `{}`**: {}\n  - Modified Files: `{}`\n  - Summary: {}\n",
                    status_label,
                    o.subtask_id,
                    o.title,
                    files_str,
                    o.summary.trim()
                ));
            }
        }

        p.push_str(&format!(
            "\n[Subtask Execution Contract]:\n\
             1. Focus strictly on completing Subtask ({}/{}): '{}'.\n\
             2. Do NOT re-explore completed work from earlier subtasks.\n\
             3. Apply all required code changes directly using write_file or editing tools.\n\
             4. When this subtask is completed, provide a concise final answer describing your implementation so execution can advance to the next subtask.\n",
            subtask_idx + 1,
            total_subtasks,
            subtask.title
        ));

        p
    }
}

enum OutputKind {
    Note,
    File,
    Command,
}

fn resolved_kind(output: &SubtaskExpectedOutput) -> OutputKind {
    let labeled = output
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .map(|kind| kind.to_ascii_lowercase());
    match labeled.as_deref() {
        Some("file") | Some("diff") => return OutputKind::File,
        Some("command") | Some("cmd") => return OutputKind::Command,
        Some("note") | Some("text") | Some("answer") => return OutputKind::Note,
        _ => {}
    }
    if output
        .command
        .as_deref()
        .is_some_and(|command| !command.trim().is_empty())
    {
        OutputKind::Command
    } else if output
        .file_path
        .as_deref()
        .is_some_and(|path| !path.trim().is_empty())
    {
        OutputKind::File
    } else {
        OutputKind::Note
    }
}

fn output_has_check(output: &SubtaskExpectedOutput) -> bool {
    !matches!(resolved_kind(output), OutputKind::Note)
}

fn output_check_hint(output: &SubtaskExpectedOutput) -> Option<String> {
    match resolved_kind(output) {
        OutputKind::File => {
            let path = output.file_path.as_deref().unwrap_or("").trim();
            if path.is_empty() {
                return Some("file path is missing".to_string());
            }
            if let Some(snippet) = output
                .expected_content
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                Some(format!("file `{path}` contains `{snippet}`"))
            } else {
                Some(format!("file `{path}` exists"))
            }
        }
        OutputKind::Command => {
            let command = output.command.as_deref().unwrap_or("").trim();
            if command.is_empty() {
                return Some("command is missing".to_string());
            }
            let code = output.expected_exit_code.unwrap_or(0);
            if let Some(stdout) = output
                .expected_stdout
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                Some(format!(
                    "`{command}` exits {code} and stdout contains `{stdout}`"
                ))
            } else {
                Some(format!("`{command}` exits {code}"))
            }
        }
        OutputKind::Note => None,
    }
}

/// Checks expected outputs after a subtask has already finished successfully.
///
/// An empty list passes. A note requires a non-empty final answer. File and
/// command checks run in the sandbox and all of them must pass.
pub async fn verify_expected_outputs(
    sandbox: &dyn SandboxDriver,
    outputs: &[SubtaskExpectedOutput],
    final_answer: Option<&str>,
) -> OutputCheckReport {
    if outputs.is_empty() {
        return OutputCheckReport {
            passed: true,
            details: String::new(),
        };
    }

    let mut failures = Vec::new();
    let mut passes = Vec::new();
    for output in outputs {
        match check_one(sandbox, output, final_answer).await {
            Ok(line) => passes.push(line),
            Err(line) => failures.push(line),
        }
    }

    if failures.is_empty() {
        OutputCheckReport {
            passed: true,
            details: passes.join("\n"),
        }
    } else {
        OutputCheckReport {
            passed: false,
            details: failures.join("\n"),
        }
    }
}

async fn check_one(
    sandbox: &dyn SandboxDriver,
    output: &SubtaskExpectedOutput,
    final_answer: Option<&str>,
) -> Result<String, String> {
    let description = output.description.trim();
    let label = if description.is_empty() {
        "expected output".to_string()
    } else {
        description.to_string()
    };
    match resolved_kind(output) {
        OutputKind::Note => {
            if final_answer
                .map(str::trim)
                .is_some_and(|text| !text.is_empty())
            {
                Ok(format!("Reported: {label}"))
            } else {
                Err(format!("Expected output was not reported: {label}"))
            }
        }
        OutputKind::File => {
            let path = output.file_path.as_deref().unwrap_or("").trim();
            if path.is_empty() {
                return Err(format!("File check is missing a path ({label})"));
            }
            let bytes = match sandbox.read_file(Path::new(path)).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    return Err(format!("Expected file `{path}` was not written ({label})"));
                }
            };
            if let Some(snippet) = output
                .expected_content
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                let content = String::from_utf8_lossy(&bytes);
                if content.contains(snippet) {
                    Ok(format!("File `{path}` matches ({label})"))
                } else {
                    Err(format!(
                        "File `{path}` does not contain `{snippet}` ({label})"
                    ))
                }
            } else {
                Ok(format!("File `{path}` exists ({label})"))
            }
        }
        OutputKind::Command => {
            let command = output.command.as_deref().unwrap_or("").trim();
            if command.is_empty() {
                return Err(format!("Command check is missing a command ({label})"));
            }
            let expected_code = output.expected_exit_code.unwrap_or(0);
            let mut opts = ExecOptions::default();
            opts.timeout_seconds = Some(300);
            let ran = sandbox
                .exec_command(command, &opts)
                .await
                .map_err(|err| format!("Command `{command}` failed to run: {err} ({label})"))?;
            if ran.exit_code != expected_code {
                return Err(format!(
                    "Command `{command}` exited {}, expected {expected_code} ({label})",
                    ran.exit_code
                ));
            }
            if let Some(stdout) = output
                .expected_stdout
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                if !ran.stdout.contains(stdout) {
                    return Err(format!(
                        "Command `{command}` stdout did not contain `{stdout}` ({label})"
                    ));
                }
            }
            Ok(format!("Command `{command}` passed ({label})"))
        }
    }
}
