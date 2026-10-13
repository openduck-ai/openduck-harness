use crate::eval::task::{SubtaskSpec, TaskSpec};
use crate::types::RunStatus;
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
