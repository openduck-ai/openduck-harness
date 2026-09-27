use std::collections::{BTreeMap, BTreeSet};

use crate::eval::continuation::ContinuationStopReason;
use crate::policy::AgentAction;
use crate::telemetry::TrajectoryStep;
use crate::types::{RunStatus, ToolCallRequest, ToolCallResponse};

const MAX_WRITES: usize = 16;
const MAX_TESTS: usize = 6;
const MAX_ERRORS: usize = 6;
const MAX_LAST_ACTIONS: usize = 8;
const MAX_NOTES_CHARS: usize = 1200;
const MAX_CMD_CHARS: usize = 140;

/// Build a continuation-oriented summary of an incomplete task run.
///
/// This is not the in-session compaction dump. It is meant to be shown in Hub
/// and injected as the seed for a follow-up run so the next agent does not
/// re-explore work that already landed.
pub fn run_progress_summary_from_steps(
    status: RunStatus,
    stop_reason: ContinuationStopReason,
    error: Option<&str>,
    steps: &[TrajectoryStep],
) -> String {
    let facts = ProgressFacts::from_steps(steps);
    format_progress_summary(status, stop_reason, error, steps.len(), &facts)
}

struct ProgressFacts {
    tool_counts: BTreeMap<String, usize>,
    written_files: Vec<(usize, String)>,
    notes: Vec<(String, String)>,
    tests: Vec<(usize, String)>,
    errors: Vec<(usize, String)>,
    last_actions: Vec<(usize, String)>,
    git_modified: BTreeSet<String>,
}

impl ProgressFacts {
    fn from_steps(steps: &[TrajectoryStep]) -> Self {
        let mut facts = Self {
            tool_counts: BTreeMap::new(),
            written_files: Vec::new(),
            notes: Vec::new(),
            tests: Vec::new(),
            errors: Vec::new(),
            last_actions: Vec::new(),
            git_modified: BTreeSet::new(),
        };

        for step in steps {
            let index = step.step_number;
            let AgentAction::CallTools(calls) = &step.action else {
                if let AgentAction::FinalAnswer(ans) = &step.action {
                    facts.last_actions.push((
                        index,
                        format!("final_answer {}", truncate(ans, MAX_CMD_CHARS)),
                    ));
                }
                continue;
            };
            let results = step.tool_results.as_deref().unwrap_or(&[]);
            for call in calls {
                *facts.tool_counts.entry(call.name.clone()).or_insert(0) += 1;
                facts.record_call(index, call, results);
            }
        }

        if facts.last_actions.len() > MAX_LAST_ACTIONS {
            let skip = facts.last_actions.len() - MAX_LAST_ACTIONS;
            facts.last_actions.drain(..skip);
        }
        facts
    }

    fn record_call(&mut self, index: usize, call: &ToolCallRequest, results: &[ToolCallResponse]) {
        let result = results.iter().find(|r| r.id == call.id);
        match call.name.as_str() {
            "write_file" | "write" => {
                if let Some(path) = arg_path(call) {
                    let action = format!("write_file {path}");
                    if is_scratch_or_note_path(&path) {
                        if let Some(content) =
                            call.arguments.get("content").and_then(|v| v.as_str())
                        {
                            upsert_note(&mut self.notes, path.clone(), content);
                        }
                    } else {
                        push_unique_write(&mut self.written_files, index, path);
                    }
                    self.last_actions.push((index, action));
                }
            }
            "edit_file" | "patch_file" | "apply_diff" | "str_replace" => {
                if let Some(path) = arg_path(call) {
                    push_unique_write(&mut self.written_files, index, path.clone());
                    self.last_actions
                        .push((index, format!("{} {path}", call.name)));
                }
            }
            "shell" | "bash" => {
                let cmd = call
                    .arguments
                    .get("command")
                    .or_else(|| call.arguments.get("cmd"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                self.last_actions.push((
                    index,
                    format!("shell {}", truncate(cmd.trim(), MAX_CMD_CHARS)),
                ));
                for path in python_write_paths(cmd) {
                    if is_scratch_or_note_path(&path) {
                        continue;
                    }
                    push_unique_write(&mut self.written_files, index, path);
                }
                if let Some(res) = result {
                    if looks_like_test_output(&res.output) {
                        if let Some(line) = test_outcome_line(&res.output) {
                            self.tests.push((index, line));
                        }
                    }
                    if cmd.contains("git status") {
                        for path in git_status_paths(&res.output) {
                            self.git_modified.insert(path);
                        }
                    }
                    if res.is_error || is_failed_exit(&res.output) {
                        self.errors.push((
                            index,
                            format!(
                                "{}: {}",
                                call.name,
                                truncate(&res.output.replace('\n', " "), 160)
                            ),
                        ));
                    }
                }
            }
            _ => {
                let extra = arg_path(call).unwrap_or_default();
                self.last_actions.push((
                    index,
                    format!("{} {}", call.name, truncate(&extra, MAX_CMD_CHARS)),
                ));
            }
        }
    }
}

fn format_progress_summary(
    status: RunStatus,
    stop_reason: ContinuationStopReason,
    error: Option<&str>,
    step_count: usize,
    facts: &ProgressFacts,
) -> String {
    let mut out = String::new();
    out.push_str("# Incomplete task run summary\n\n");
    out.push_str(&format!("- Status: {status:?}\n"));
    out.push_str(&format!("- Stop reason: {stop_reason:?}\n"));
    out.push_str(&format!("- Steps completed: {step_count}\n"));
    if !facts.tool_counts.is_empty() {
        let tools: Vec<String> = facts
            .tool_counts
            .iter()
            .map(|(name, count)| format!("{name} ×{count}"))
            .collect();
        out.push_str(&format!("- Tools: {}\n", tools.join(", ")));
    }
    if let Some(error) = error.filter(|e| !e.is_empty()) {
        out.push_str(&format!("- Stop message: {}\n", truncate(error, 240)));
    }

    if !facts.written_files.is_empty() {
        out.push_str("\n## Files created or modified\n");
        for (index, path) in facts.written_files.iter().take(MAX_WRITES) {
            out.push_str(&format!("- `{path}` (step {index})\n"));
        }
        if facts.written_files.len() > MAX_WRITES {
            out.push_str(&format!(
                "- ... and {} more\n",
                facts.written_files.len() - MAX_WRITES
            ));
        }
    }

    if !facts.git_modified.is_empty() {
        out.push_str("\n## Workspace git status at stop\n");
        for path in facts.git_modified.iter().take(20) {
            out.push_str(&format!("- `{path}`\n"));
        }
    }

    if !facts.notes.is_empty() {
        out.push_str("\n## Scratchpad (last version)\n");
        for (path, content) in &facts.notes {
            out.push_str(&format!("### `{path}`\n```markdown\n{content}\n```\n"));
        }
    }

    if !facts.tests.is_empty() {
        out.push_str("\n## Tests\n");
        for (index, line) in facts.tests.iter().rev().take(MAX_TESTS).rev() {
            out.push_str(&format!("- step {index}: {line}\n"));
        }
    }

    if !facts.errors.is_empty() {
        out.push_str("\n## Errors\n");
        for (index, line) in facts.errors.iter().rev().take(MAX_ERRORS).rev() {
            out.push_str(&format!("- step {index}: {line}\n"));
        }
    }

    if !facts.last_actions.is_empty() {
        out.push_str("\n## Last actions\n");
        for (index, action) in &facts.last_actions {
            out.push_str(&format!("- step {index}: {action}\n"));
        }
    }

    let remaining = remaining_work(facts);
    out.push_str("\n## Remaining work\n");
    for item in remaining {
        out.push_str(&format!("- {item}\n"));
    }

    out.push_str(
        "\n## Continuation contract\n\
         This summary is the source of truth for prior progress. A copy is written to `.agent/continuation-summary.md`.\n\
         Do NOT re-explore files already examined. Do NOT redo completed writes unless this summary marks them broken.\n\
         Continue from Remaining work using the workspace as it stands.\n",
    );
    out
}

fn remaining_work(facts: &ProgressFacts) -> Vec<String> {
    let mut items = Vec::new();
    for (_, content) in &facts.notes {
        items.extend(remaining_lines(content));
    }
    if items.is_empty() {
        if facts
            .tests
            .iter()
            .any(|(_, line)| line.to_ascii_lowercase().contains("failed"))
        {
            items.push(
                "Fix the failing tests from the last run, then resume unfinished wiring.".into(),
            );
        } else {
            items.push(
                "The previous run stopped before finishing. Continue from the last modified files and any unconnected API/UI wiring.".into(),
            );
        }
    }
    items.into_iter().take(8).collect()
}

fn remaining_lines(notes: &str) -> Vec<String> {
    let mut in_remaining = false;
    let mut items = Vec::new();
    for line in notes.lines() {
        let trimmed = line.trim();
        let heading = trimmed.to_ascii_lowercase();
        if heading.starts_with("## remaining")
            || heading.starts_with("## not started")
            || heading.starts_with("## still open")
            || heading.starts_with("## broken")
        {
            in_remaining = true;
            continue;
        }
        if in_remaining && heading.starts_with("## ") {
            break;
        }
        if in_remaining {
            let item = trimmed
                .trim_start_matches('-')
                .trim_start_matches('*')
                .trim_start_matches("[ ]")
                .trim();
            if !item.is_empty() {
                items.push(item.to_string());
            }
        }
    }
    items
}

fn arg_path(call: &ToolCallRequest) -> Option<String> {
    call.arguments
        .get("path")
        .or_else(|| call.arguments.get("file_path"))
        .or_else(|| call.arguments.get("target_file"))
        .or_else(|| call.arguments.get("file"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn python_write_paths(cmd: &str) -> Vec<String> {
    if !(cmd.contains("python") || cmd.contains("python3")) {
        return Vec::new();
    }
    let mut paths = Vec::new();
    for marker in ["open(", "p="] {
        let mut haystack = cmd;
        while let Some(found) = haystack.find(marker) {
            let Some(after_marker) = haystack.get(found + marker.len()..) else {
                break;
            };
            let after_marker = after_marker.trim_start();
            let mut chars = after_marker.chars();
            let Some(quote) = chars.next().filter(|c| *c == '\'' || *c == '"') else {
                haystack = after_marker;
                continue;
            };
            let remainder = chars.as_str();
            match remainder.split_once(quote) {
                Some((path, rest)) => {
                    if path.contains('/') || path.contains('.') {
                        paths.push(path.to_string());
                    }
                    haystack = rest;
                }
                None => break,
            }
        }
    }
    paths
}

fn git_status_paths(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("M ")
                || trimmed.starts_with("MM ")
                || trimmed.starts_with("A ")
                || trimmed.starts_with("?? ")
                || trimmed.starts_with(" M ")
            {
                Some(
                    trimmed
                        .trim_start_matches('?')
                        .trim_start_matches('M')
                        .trim_start_matches('A')
                        .trim()
                        .to_string(),
                )
            } else {
                None
            }
        })
        .filter(|p| !p.is_empty())
        .collect()
}

fn looks_like_test_output(output: &str) -> bool {
    output.contains("test result:")
        || output.contains("FAILED")
        || output.contains("PASSED")
        || output.contains("Tests:")
}

fn test_outcome_line(output: &str) -> Option<String> {
    output
        .lines()
        .find(|l| l.contains("test result:") || l.contains("Tests:") || l.contains("FAILED"))
        .map(|l| truncate(l.trim(), 160))
}

fn is_failed_exit(output: &str) -> bool {
    output.contains("Exit: 1")
        || output.contains("Exit: 101")
        || output.contains("error[")
        || output.contains("FAILED")
}

fn is_scratch_or_note_path(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    lower.contains("/.agent/")
        || lower.contains(".agent/notes")
        || lower.contains("notes-plan")
        || lower.contains("scratch")
}

fn upsert_note(notes: &mut Vec<(String, String)>, path: String, content: &str) {
    let snippet = truncate(content.trim(), MAX_NOTES_CHARS);
    if let Some(existing) = notes.iter_mut().find(|(p, _)| p == &path) {
        existing.1 = snippet;
    } else {
        notes.push((path, snippet));
    }
}

fn push_unique_write(writes: &mut Vec<(usize, String)>, index: usize, path: String) {
    if let Some(existing) = writes.iter_mut().find(|(_, p)| p == &path) {
        existing.0 = index;
    } else {
        writes.push((index, path));
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn step(
        index: usize,
        name: &str,
        args: serde_json::Value,
        output: &str,
        is_error: bool,
    ) -> TrajectoryStep {
        TrajectoryStep {
            step_number: index,
            timestamp: Utc::now(),
            action: AgentAction::CallTools(vec![ToolCallRequest {
                id: format!("c{index}"),
                name: name.to_string(),
                arguments: args,
            }]),
            tool_results: Some(vec![ToolCallResponse {
                id: format!("c{index}"),
                name: name.to_string(),
                output: output.to_string(),
                is_error,
            }]),
            duration_ms: 1,
            token_usage: None,
            llm_request: None,
            llm_response: None,
            judgments: None,
        }
    }

    #[test]
    fn summary_lists_writes_tests_and_remaining() {
        let steps = vec![
            step(
                1,
                "write_file",
                json!({"path": "src/xlsx.rs", "content": "fn write() {}"}),
                "ok",
                false,
            ),
            step(
                2,
                "write_file",
                json!({
                    "path": ".agent/notes-plan-task-aaa.md",
                    "content": "# Plan\n## Remaining work\n- Wire export route\n- Add Excel button\n"
                }),
                "ok",
                false,
            ),
            step(
                3,
                "shell",
                json!({"command": "cargo test -p rmqx_admin --lib bms_export"}),
                "test result: FAILED. 6 passed; 1 failed",
                false,
            ),
        ];
        let summary = run_progress_summary_from_steps(
            RunStatus::Cancelled,
            ContinuationStopReason::Cancelled,
            Some("hit max turns"),
            &steps,
        );
        assert!(summary.contains("Incomplete task run summary"));
        assert!(summary.contains("`src/xlsx.rs`"));
        assert!(summary.contains("6 passed; 1 failed"));
        assert!(summary.contains("Wire export route"));
        assert!(summary.contains("Continuation contract"));
        assert!(summary.contains(".agent/notes-plan-task-aaa.md"));
        assert!(!summary.contains("COMPACTED CONTEXT SUMMARY"));
        let files_section = summary.split("## Scratchpad").next().unwrap();
        assert!(files_section.contains("`src/xlsx.rs`"));
        assert!(
            !files_section.contains("notes-plan-task-aaa.md"),
            "per-task notes-plan must not be listed as a product write: {files_section}"
        );
    }

    #[test]
    fn per_task_notes_plan_write_is_scratch_not_product() {
        let steps = vec![
            step(
                1,
                "write_file",
                json!({"path": "src/export.rs", "content": "fn export() {}"}),
                "ok",
                false,
            ),
            step(
                2,
                "write_file",
                json!({
                    "path": ".agent/notes-plan-task-bbb.md",
                    "content": "## Remaining work\n- Finish Excel export button\n"
                }),
                "ok",
                false,
            ),
        ];
        let summary = run_progress_summary_from_steps(
            RunStatus::Cancelled,
            ContinuationStopReason::Cancelled,
            Some("hit max turns"),
            &steps,
        );
        assert!(summary.contains("Finish Excel export button"));
        assert!(summary.contains("## Scratchpad"));
        assert!(summary.contains(".agent/notes-plan-task-bbb.md"));
        let files_section = summary.split("## Scratchpad").next().unwrap();
        assert!(files_section.contains("`src/export.rs`"));
        assert!(!files_section.contains("notes-plan-task-bbb.md"));
    }

    #[test]
    fn python_open_counts_as_a_write() {
        let steps = vec![step(
            10,
            "shell",
            json!({"command": "python3 - <<'PY'\np='rmqx_admin/src/service/bms_export.rs'\nopen(p,'w').write('x')\nPY"}),
            "ok",
            false,
        )];
        let summary = run_progress_summary_from_steps(
            RunStatus::Cancelled,
            ContinuationStopReason::Cancelled,
            None,
            &steps,
        );
        assert!(summary.contains("bms_export.rs"));
    }
}
