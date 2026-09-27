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

pub const MIN_SUBTASK_TURNS: usize = 8;

const SYNTHESIZED_PHASE_WEIGHTS: [usize; 3] = [2, 3, 2];
const SYNTHESIZED_PHASE_WEIGHT_SUM: usize = 7;

/// Splits a parent turn budget across the 3 synthesized phases using a 2:3:2 ratio
/// (discovery : core implementation : verification), with a per-phase floor of
/// [`MIN_SUBTASK_TURNS`]. Remainder goes to the implementation phase.
pub fn split_synthesized_phase_turns(parent_max_turns: usize) -> [usize; 3] {
    let parent = parent_max_turns.max(MIN_SUBTASK_TURNS);
    let p1 = (parent * SYNTHESIZED_PHASE_WEIGHTS[0] / SYNTHESIZED_PHASE_WEIGHT_SUM)
        .max(MIN_SUBTASK_TURNS);
    let p3 = (parent * SYNTHESIZED_PHASE_WEIGHTS[2] / SYNTHESIZED_PHASE_WEIGHT_SUM)
        .max(MIN_SUBTASK_TURNS);
    let p2 = parent.saturating_sub(p1 + p3).max(MIN_SUBTASK_TURNS);
    [p1, p2, p3]
}

/// Resolves the 3 synthesized-phase turn budgets from the parent task budget,
/// applying optional per-phase overrides (`phaseMaxTurns` in task YAML / Hub UI).
pub fn resolve_synthesized_phase_turns(
    parent_max_turns: Option<usize>,
    overrides: Option<&[usize]>,
) -> [usize; 3] {
    let mut turns = split_synthesized_phase_turns(parent_max_turns.unwrap_or(25));
    if let Some(overrides) = overrides {
        for (slot, &value) in turns.iter_mut().zip(overrides.iter()) {
            if value > 0 {
                *slot = value;
            }
        }
    }
    turns
}

fn apply_turn_overrides(subtasks: &mut [SubtaskSpec], overrides: Option<&[usize]>) {
    let Some(overrides) = overrides else {
        return;
    };
    if overrides.len() != subtasks.len() {
        return;
    }
    for (subtask, &turns) in subtasks.iter_mut().zip(overrides.iter()) {
        if turns > 0 {
            subtask.max_turns = Some(turns);
        }
    }
}

pub struct SubtaskDecomposer;

impl SubtaskDecomposer {
    /// Attempts to decompose a task's problem statement into ordered subtasks.
    /// Returns an empty vector if the task is simple and should execute as a single monolithic turn.
    ///
    /// Synthesized phases inherit a 2:3:2 split of `parent_max_turns`, unless
    /// `phase_max_turns` provides per-phase overrides from task config / Hub UI.
    pub fn decompose(
        task_id: &str,
        prompt: &str,
        parent_max_turns: Option<usize>,
        phase_max_turns: Option<&[usize]>,
    ) -> Vec<SubtaskSpec> {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // 1. Try structured extraction (numbered lists, markdown checklists, or phase headings)
        let mut extracted = Self::extract_structured_steps(task_id, trimmed);
        if extracted.len() >= 2 {
            apply_turn_overrides(&mut extracted, phase_max_turns);
            return extracted;
        }

        // 2. If broad or complex prose, synthesize a multi-phase subtask plan
        if Self::is_complex_task(trimmed) {
            return Self::synthesize_phases(task_id, trimmed, parent_max_turns, phase_max_turns);
        }

        Vec::new()
    }

    /// Extracts structured subtasks from numbered lists, markdown task lists, or phase headings.
    fn extract_structured_steps(task_id: &str, prompt: &str) -> Vec<SubtaskSpec> {
        let mut subtasks = Vec::new();
        let mut current_title = String::new();
        let mut current_body = Vec::new();
        let mut current_target_files = Vec::new();

        for line in prompt.lines() {
            let line_trimmed = line.trim();
            if line_trimmed.is_empty() {
                continue;
            }

            if let Some(heading) = Self::parse_step_boundary(line_trimmed) {
                if !current_title.is_empty() {
                    let idx = subtasks.len() + 1;
                    let desc = if current_body.is_empty() {
                        current_title.clone()
                    } else {
                        current_body.join("\n")
                    };
                    let mut spec = SubtaskSpec::new(
                        format!("{}-subtask-{}", task_id, idx),
                        current_title.clone(),
                        desc,
                    );
                    if !current_target_files.is_empty() {
                        spec.target_files = Some(current_target_files.clone());
                    }
                    subtasks.push(spec);
                    current_body.clear();
                    current_target_files.clear();
                }
                Self::extract_file_references(&heading, &mut current_target_files);
                current_title = heading;
            } else if !current_title.is_empty() {
                Self::extract_file_references(line_trimmed, &mut current_target_files);
                current_body.push(line_trimmed.to_string());
            }
        }

        if !current_title.is_empty() {
            let idx = subtasks.len() + 1;
            let desc = if current_body.is_empty() {
                current_title.clone()
            } else {
                current_body.join("\n")
            };
            let mut spec =
                SubtaskSpec::new(format!("{}-subtask-{}", task_id, idx), current_title, desc);
            if !current_target_files.is_empty() {
                spec.target_files = Some(current_target_files);
            }
            subtasks.push(spec);
        }

        subtasks
    }

    fn extract_file_references(text: &str, target_files: &mut Vec<String>) {
        for word in text.split_whitespace() {
            let clean_word = word.trim_matches(|c| {
                c == '`' || c == '\'' || c == '"' || c == '(' || c == ')' || c == ':' || c == ','
            });
            if (clean_word.contains('/')
                || clean_word.ends_with(".rs")
                || clean_word.ends_with(".ts")
                || clean_word.ends_with(".js")
                || clean_word.ends_with(".vue")
                || clean_word.ends_with(".go")
                || clean_word.ends_with(".py")
                || clean_word.ends_with(".json")
                || clean_word.ends_with(".yaml")
                || clean_word.ends_with(".md"))
                && !clean_word.starts_with("http")
                && !target_files.contains(&clean_word.to_string())
            {
                target_files.push(clean_word.to_string());
            }
        }
    }

    /// Parses step boundary prefix like "1. ", "Step 1:", "Phase 1:", "- [ ]", "### Task 1".
    #[allow(clippy::string_slice)]
    fn parse_step_boundary(line: &str) -> Option<String> {
        // Numbered list: "1. ", "2) ", "1: "
        if let Some(first_char) = line.chars().next() {
            if first_char.is_ascii_digit() {
                let rest = line.trim_start_matches(|c: char| c.is_ascii_digit());
                if rest.starts_with(". ")
                    || rest.starts_with(") ")
                    || rest.starts_with(": ")
                    || rest.starts_with(" - ")
                {
                    let title = rest[2..].trim();
                    if title.len() >= 3 {
                        return Some(title.to_string());
                    }
                }
            }
        }

        // Keywords: "Step 1:", "Phase 1:", "Subtask 1:", "Task 1:"
        let lower = line.to_lowercase();
        for prefix in &[
            "step ",
            "phase ",
            "subtask ",
            "task ",
            "milestone ",
            "part ",
        ] {
            if lower.starts_with(prefix) {
                if let Some(colon_pos) = line.find(':') {
                    let title = line[colon_pos + 1..].trim();
                    if title.len() >= 3 {
                        return Some(title.to_string());
                    }
                } else if let Some(dash_pos) = line.find(" - ") {
                    let title = line[dash_pos + 3..].trim();
                    if title.len() >= 3 {
                        return Some(title.to_string());
                    }
                }
            }
        }

        // Markdown checklist: "- [ ] " or "* [ ] "
        if line.starts_with("- [ ] ")
            || line.starts_with("* [ ] ")
            || line.starts_with("- [x] ")
            || line.starts_with("- [X] ")
        {
            let title = line[6..].trim();
            if title.len() >= 3 {
                return Some(title.to_string());
            }
        }

        // Heading with numbers: "### 1. Title" or "### Step 1: Title"
        if line.starts_with('#') {
            let heading_text = line.trim_start_matches('#').trim();
            if let Some(b) = Self::parse_step_boundary(heading_text) {
                return Some(b);
            }
        }

        None
    }

    /// Determines if a prose task is complex enough to warrant automatic phase decomposition.
    fn is_complex_task(prompt: &str) -> bool {
        let length = prompt.len();
        let lower = prompt.to_lowercase();

        // Complex keywords
        let contains_multi_feature = (lower.contains(" and ")
            || lower.contains("与")
            || lower.contains("以及")
            || lower.contains("并"))
            && (lower.contains("implement")
                || lower.contains("add")
                || lower.contains("support")
                || lower.contains("实现")
                || lower.contains("新增")
                || lower.contains("集成")
                || lower.contains("完善"));

        let has_multi_layers = (lower.contains("dto")
            || lower.contains("model")
            || lower.contains("schema")
            || lower.contains("db")
            || lower.contains("proto"))
            && (lower.contains("service")
                || lower.contains("handler")
                || lower.contains("engine")
                || lower.contains("notify")
                || lower.contains("logic")
                || lower.contains("controller")
                || lower.contains("ui")
                || lower.contains("api"));

        (length >= 140 && (contains_multi_feature || has_multi_layers)) || (length >= 250)
    }

    /// Synthesizes 3 standard phases for complex tasks lacking explicit lists.
    #[allow(clippy::string_slice)]
    fn synthesize_phases(
        task_id: &str,
        prompt: &str,
        parent_max_turns: Option<usize>,
        phase_max_turns: Option<&[usize]>,
    ) -> Vec<SubtaskSpec> {
        let snippet = if prompt.len() > 100 {
            format!("{}...", prompt.chars().take(100).collect::<String>())
        } else {
            prompt.to_string()
        };
        let [p1, p2, p3] = resolve_synthesized_phase_turns(parent_max_turns, phase_max_turns);

        vec![
            SubtaskSpec::new(
                format!("{}-subtask-1", task_id),
                "Phase 1: Target Discovery, DTO & Interface Definition",
                format!("Inspect relevant modules and define/update necessary DTOs, data structures, and traits for: {}", snippet),
            )
            .with_exploration_budget(4)
            .with_max_turns(p1),
            SubtaskSpec::new(
                format!("{}-subtask-2", task_id),
                "Phase 2: Core Logic & Service Implementation",
                format!("Implement the core backend engine, logic routing, and service handling according to the specifications in: {}", snippet),
            )
            .with_exploration_budget(3)
            .with_max_turns(p2),
            SubtaskSpec::new(
                format!("{}-subtask-3", task_id),
                "Phase 3: Integration, Verification & Test Suite",
                format!("Verify end-to-end integration by running compilation, test suites, and fixing any errors for: {}", snippet),
            )
            .with_exploration_budget(2)
            .with_max_turns(p3),
        ]
    }

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
