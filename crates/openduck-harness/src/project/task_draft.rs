use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::eval::task::{SubtaskExpectedOutput, SubtaskSpec};

/// One turn in the Hub chat that drafts a project task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskDraftMessage {
    pub role: String,
    pub content: String,
}

/// A task the chat is ready to save. Subtasks are present only for a long run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskDraft {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub prompt: String,
    pub max_turns: usize,
    pub timeout_seconds: u64,
    pub large_task: bool,
    pub complexity: String,
    pub reasoning: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtasks: Vec<SubtaskSpec>,
}

/// The assistant reply, plus a draft once the request is specific enough to save.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskDraftTurn {
    pub reply: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<TaskDraft>,
}

/// Drafts a task from the chat without calling a model.
///
/// A long run is a complex or deep request, or any request that already lists
/// two or more steps. Those become ordered subtasks with expected outputs.
/// A focused change stays one task.
pub fn heuristic_task_draft(
    messages: &[TaskDraftMessage],
    current: Option<&TaskDraft>,
    max_turns_limit: usize,
) -> TaskDraftTurn {
    let limit = max_turns_limit.max(1);
    let user_messages: Vec<&str> = messages
        .iter()
        .filter(|message| message.role.eq_ignore_ascii_case("user"))
        .map(|message| message.content.trim())
        .filter(|text| !text.is_empty())
        .collect();

    if user_messages.is_empty() {
        if let Some(current) = current.filter(|draft| !draft.prompt.trim().is_empty()) {
            return TaskDraftTurn {
                reply: reply_for(current),
                draft: Some(current.clone()),
            };
        }
        return TaskDraftTurn {
            reply: "Describe the task you want to run. If it is a long job, I will split it into subtasks you can edit before saving.".to_string(),
            draft: None,
        };
    }

    let last = *user_messages.last().unwrap();
    let prompt = task_prompt_from_messages(&user_messages, current);
    if prompt.trim().is_empty() || user_messages.iter().all(|text| is_clarifying_only(text)) {
        return TaskDraftTurn {
            reply: "What should the agent change, and how will you know that change is done?"
                .to_string(),
            draft: None,
        };
    }

    let force_single = latest_says_single(last);
    let force_split = latest_says_split(last);
    let steps = extract_steps(&prompt);
    let complexity = classify_complexity(&prompt, steps.len());
    let large = if force_single {
        false
    } else {
        force_split || steps.len() >= 2 || matches!(complexity.as_str(), "complex" | "deep")
    };

    let max_turns = turns_for(&complexity, limit);
    let mut draft = TaskDraft {
        id: task_id_from(current, &prompt),
        name: task_name_from(current, &prompt, &steps),
        category: Some(category_for(&prompt)),
        tags: current.map(|draft| draft.tags.clone()).unwrap_or_default(),
        prompt,
        max_turns,
        timeout_seconds: timeout_for(max_turns, &complexity),
        large_task: large,
        complexity,
        reasoning: String::new(),
        subtasks: Vec::new(),
    };

    if large {
        draft.subtasks = if steps.len() >= 2 {
            subtasks_from_steps(&draft.id, &steps, draft.max_turns, limit)
        } else {
            synthesize_phases(&draft.id, &draft.prompt, draft.max_turns, limit)
        };
        draft.reasoning = format!(
            "Long {} task. Split into {} subtasks that run in order, each with an output check.",
            draft.complexity,
            draft.subtasks.len()
        );
    } else {
        draft.reasoning = format!(
            "Focused {} task. It runs as one session until you ask to split it.",
            draft.complexity
        );
    }

    let reply = reply_for(&draft);
    TaskDraftTurn {
        reply,
        draft: Some(draft),
    }
}

pub fn draft_system_prompt(max_turns_limit: usize) -> String {
    let limit = max_turns_limit.max(1);
    format!(
        r#"You draft project tasks for an autonomous coding agent by chatting with the user.
Decide whether the request is a long run. A long run is a multi-step, complex, or deep change that should be split before it starts. A focused bugfix or single-file change is not a long run.

When you know what to run, return a draft. When the request is still ambiguous, ask one clarifying question and set draft to null.

If the task is a long run, draft.subtasks must contain at least two subtasks that can run in order in one workspace. Each subtask needs expectedOutputs the harness can check before the next subtask starts:
- kind "file" with filePath, and expectedContent when a specific snippet must exist
- kind "command" with command, optional expectedExitCode (default 0), and optional expectedStdout
- kind "note" when the only signal is the subtask's own final answer

If it is not a long run, subtasks must be an empty array and largeTask must be false.
Keep the saved prompt as the user's task, not your commentary. Preserve edits in currentDraft unless the latest message changes them.
maxTurns is an integer from 8 to {limit}. Split that budget across subtasks.

Output ONLY a JSON object:
{{
  "reply": "<short chat reply>",
  "draft": null or {{
    "id": "<kebab-case id>",
    "name": "<short name>",
    "category": "<bug|feature|refactor|test>",
    "tags": [],
    "prompt": "<task instructions>",
    "maxTurns": <integer>,
    "timeoutSeconds": <integer 120-7200>,
    "largeTask": <true|false>,
    "complexity": "<simple|medium|complex|deep>",
    "reasoning": "<one sentence>",
    "subtasks": [
      {{
        "id": "<id>",
        "title": "<title>",
        "description": "<what this step does>",
        "targetFiles": ["<optional path>"],
        "maxTurns": <integer>,
        "expectedOutputs": [
          {{
            "description": "<what must be true>",
            "kind": "<note|file|command>",
            "filePath": "<for file>",
            "expectedContent": "<optional snippet>",
            "command": "<for command>",
            "expectedExitCode": 0,
            "expectedStdout": "<optional>"
          }}
        ]
      }}
    ]
  }}
}}"#
    )
}

/// Parses the model's JSON draft. Invalid JSON is an error so the caller can
/// fall back to the heuristic.
pub fn parse_model_task_draft(raw: &str, max_turns_limit: usize) -> Result<TaskDraftTurn> {
    let clean = extract_json_object(raw);
    let parsed: ModelEnvelope =
        serde_json::from_str(&clean).map_err(|err| anyhow!("draft JSON: {err}"))?;
    let loose = parsed.draft.clone().or_else(|| parsed.as_loose_draft());
    let reply = parsed.reply.unwrap_or_default().trim().to_string();
    let Some(loose) = loose else {
        if reply.is_empty() {
            return Err(anyhow!("model draft had no reply"));
        }
        return Ok(TaskDraftTurn { reply, draft: None });
    };

    let prompt = loose.prompt.unwrap_or_default().trim().to_string();
    if prompt.is_empty() && reply.is_empty() {
        return Err(anyhow!("model draft had no prompt"));
    }
    if prompt.is_empty() {
        return Ok(TaskDraftTurn { reply, draft: None });
    }

    let limit = max_turns_limit.max(1);
    let complexity = normalize_complexity(loose.complexity.as_deref().unwrap_or("medium"));
    let subtasks = loose
        .subtasks
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .filter_map(|(index, subtask)| loose_subtask(index, subtask, limit))
        .collect::<Vec<_>>();
    let mut large = parsed
        .large_task
        .or(loose.large_task)
        .unwrap_or(subtasks.len() >= 2);
    let mut subtasks = if large { subtasks } else { Vec::new() };
    let name = loose.name.unwrap_or_default().trim().to_string();
    let name = if name.is_empty() {
        first_line(&prompt, 80)
    } else {
        name
    };
    let id = slug_task_id(loose.id.as_deref().unwrap_or(&name));
    let max_turns = clamp_turns(
        loose
            .max_turns
            .unwrap_or_else(|| turns_for(&complexity, limit)),
        limit,
    );
    if large && subtasks.is_empty() {
        subtasks = synthesize_phases(&id, &prompt, max_turns, limit);
    }
    if subtasks.len() >= 2 {
        large = true;
        assign_missing_turns(&mut subtasks, max_turns, limit);
        unique_ids(&id, &mut subtasks);
    } else {
        // One subtask cannot run ahead of another, so the task stays a single session.
        large = false;
        subtasks.clear();
    }

    let reasoning = loose
        .reasoning
        .or(parsed.reasoning)
        .unwrap_or_default()
        .trim()
        .to_string();
    let reasoning = if reasoning.is_empty() {
        if large {
            format!("Split into {} subtasks.", subtasks.len())
        } else {
            "Runs as one session.".to_string()
        }
    } else {
        reasoning
    };
    let reply = if reply.is_empty() {
        if large {
            format!(
                "This looks like a long task. I split it into {} subtasks. Edit them before you save.",
                subtasks.len()
            )
        } else {
            "This looks like one task. Edit the draft, or ask me to split it.".to_string()
        }
    } else {
        reply
    };

    Ok(TaskDraftTurn {
        reply,
        draft: Some(TaskDraft {
            id,
            name,
            category: loose.category.and_then(|value| {
                let trimmed = value.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }),
            tags: loose.tags.unwrap_or_default(),
            prompt,
            max_turns,
            timeout_seconds: loose
                .timeout_seconds
                .unwrap_or_else(|| timeout_for(max_turns, &complexity))
                .clamp(120, 7200),
            large_task: large,
            complexity,
            reasoning,
            subtasks,
        }),
    })
}

fn reply_for(draft: &TaskDraft) -> String {
    if draft.large_task {
        format!(
            "This looks like a long {} task. I split it into {} subtasks that run one after another. Each one lists the outputs the next step waits for. Edit the titles, instructions, and checks, then save.",
            draft.complexity,
            draft.subtasks.len()
        )
    } else {
        format!(
            "This looks like one {} task, so I left it as a single run. Edit the draft, or tell me to split it into subtasks.",
            draft.complexity
        )
    }
}

fn task_prompt_from_messages(messages: &[&str], current: Option<&TaskDraft>) -> String {
    let mut parts = Vec::new();
    if let Some(current) = current {
        let prompt = current.prompt.trim();
        if !prompt.is_empty() {
            parts.push(prompt.to_string());
        }
    }
    for text in messages {
        if is_control_phrase(text) || is_clarifying_only(text) {
            continue;
        }
        if parts.iter().any(|existing| existing == *text) {
            continue;
        }
        parts.push((*text).to_string());
    }
    parts.join("\n\n")
}

fn task_id_from(current: Option<&TaskDraft>, prompt: &str) -> String {
    if let Some(id) = current
        .map(|draft| draft.id.trim())
        .filter(|id| !id.is_empty())
    {
        if !is_generated_task_id(id) {
            return slug_task_id(id);
        }
    }
    slug_task_id(&task_name_from(current, prompt, &[]))
}

fn is_generated_task_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("task-") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit())
}

fn task_name_from(current: Option<&TaskDraft>, prompt: &str, steps: &[String]) -> String {
    if let Some(name) = current
        .map(|draft| draft.name.trim())
        .filter(|name| !name.is_empty())
    {
        return name.to_string();
    }
    if let Some(step) = steps.first() {
        return first_line(step, 80);
    }
    first_line(prompt, 80)
}

fn first_line(text: &str, max_chars: usize) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(text);
    let trimmed = line.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        trimmed.chars().take(max_chars).collect()
    }
}

fn slug_task_id(raw: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for ch in raw.chars().flat_map(|ch| ch.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
        if slug.len() >= 48 {
            break;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    let slug = if slug.is_empty() {
        "task".to_string()
    } else {
        slug
    };
    if slug.starts_with("task-") {
        slug
    } else {
        format!("task-{slug}")
    }
}

fn classify_complexity(prompt: &str, step_count: usize) -> String {
    let lower = prompt.to_lowercase();
    let deep = [
        "full-stack",
        "whole project",
        "entire repository",
        "full migration",
        "from scratch",
        "end-to-end rewrite",
        "autonomous workflow",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword));
    let complex = [
        "refactor",
        "migration",
        "migrate",
        "architecture",
        "end-to-end",
        "e2e",
        "benchmark",
        "distributed",
        "rewrite",
        "optimize",
        "concurrency",
        "docker",
        "pipeline",
        "subagent",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword));
    let simple = [
        "typo",
        "spelling",
        "rename",
        "fix comment",
        "readme",
        "docs",
        "docstring",
        "explain",
        "what is",
        "where is",
        "format",
        "lint",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword));
    if deep || (step_count >= 5 && complex) {
        "deep".to_string()
    } else if complex || step_count >= 3 {
        "complex".to_string()
    } else if simple && prompt.trim().chars().count() < 180 && step_count < 2 {
        "simple".to_string()
    } else {
        "medium".to_string()
    }
}

fn normalize_complexity(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "simple" | "medium" | "complex" | "deep" => value.trim().to_ascii_lowercase(),
        _ => "medium".to_string(),
    }
}

fn turns_for(complexity: &str, limit: usize) -> usize {
    let turns = match complexity {
        "deep" => 220,
        "complex" => 110,
        "simple" => 20,
        _ => 50,
    };
    turns.clamp(8.min(limit), limit)
}

fn timeout_for(turns: usize, complexity: &str) -> u64 {
    let base = match complexity {
        "deep" => 3600,
        "complex" => 1800,
        "simple" => 240,
        _ => 900,
    };
    base.max((turns as u64).saturating_mul(12)).clamp(120, 7200)
}

fn clamp_turns(turns: usize, limit: usize) -> usize {
    turns.clamp(1, limit.max(1))
}

fn category_for(prompt: &str) -> String {
    let lower = prompt.to_lowercase();
    if lower.contains("refactor") || lower.contains("rewrite") {
        "refactor".to_string()
    } else if lower.contains("bug") || lower.contains("fix ") || lower.starts_with("fix") {
        "bug".to_string()
    } else if lower.contains("test") && !lower.contains("implement") {
        "test".to_string()
    } else {
        "feature".to_string()
    }
}

fn latest_says_single(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "don't split",
        "do not split",
        "dont split",
        "one task",
        "single task",
        "no subtask",
        "without subtask",
        "keep it together",
        "don't break",
        "do not break",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

fn latest_says_split(text: &str) -> bool {
    if latest_says_single(text) {
        return false;
    }
    let lower = text.to_lowercase();
    [
        "split",
        "break into",
        "break this",
        "subtasks",
        "sub-tasks",
        "subtask",
        "long run",
        "long-running",
        "long running",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

fn is_clarifying_only(text: &str) -> bool {
    let trimmed = text.trim();
    if !trimmed.ends_with('?') || trimmed.chars().count() > 220 {
        return false;
    }
    let lower = trimmed.to_lowercase();
    ![
        "implement",
        "add ",
        "fix ",
        "refactor",
        "create",
        "build",
        "write",
        "migrate",
        "update",
        "change",
        "rename",
    ]
    .iter()
    .any(|verb| lower.contains(verb))
}

fn is_control_phrase(text: &str) -> bool {
    let words = text.split_whitespace().count();
    words <= 14
        && (latest_says_single(text) || latest_says_split(text))
        && extract_files(text).is_empty()
        && !text.to_lowercase().contains("implement")
}

fn extract_steps(text: &str) -> Vec<String> {
    let mut steps = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = strip_step_prefix(trimmed) {
            let rest = rest.trim().trim_start_matches(['-', ':']).trim();
            if !rest.is_empty() {
                steps.push(rest.to_string());
            }
        }
    }
    steps
}

fn strip_step_prefix(line: &str) -> Option<&str> {
    if let Some(rest) = line
        .strip_prefix("- [ ]")
        .or_else(|| line.strip_prefix("* [ ]"))
        .or_else(|| line.strip_prefix("- [x]"))
        .or_else(|| line.strip_prefix("- [X]"))
        .or_else(|| line.strip_prefix("* [x]"))
        .or_else(|| line.strip_prefix("* [X]"))
    {
        return Some(rest);
    }
    if let Some(index) = line.find(['.', ')']) {
        let head = &line[..index];
        if !head.is_empty()
            && head.chars().all(|ch| ch.is_ascii_digit())
            && line[index + 1..].starts_with([' ', '\t'])
        {
            return Some(&line[index + 1..]);
        }
    }
    let lower = line.to_ascii_lowercase();
    for prefix in ["step ", "phase ", "task "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if let Some(pos) = rest.find([':', '-']) {
                let label = rest[..pos].trim();
                if !label.is_empty()
                    && label
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == ' ')
                {
                    let start = prefix.len() + pos + 1;
                    return line.get(start..);
                }
            }
        }
    }
    None
}

fn extract_files(text: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | '\\') {
            current.push(ch);
            in_token = true;
        } else if in_token {
            push_file(&mut files, &current);
            current.clear();
            in_token = false;
        }
    }
    if in_token {
        push_file(&mut files, &current);
    }
    files
}

fn push_file(files: &mut Vec<String>, token: &str) {
    let token = token.trim_matches(['.', '/', '\\', '-', '_']);
    let Some(ext) = token.rsplit('.').next() else {
        return;
    };
    if ![
        "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "kt", "vue", "json", "yaml", "yml",
        "md", "toml", "css", "html",
    ]
    .contains(&ext)
    {
        return;
    }
    if token.len() < 4 || files.iter().any(|existing| existing == token) {
        return;
    }
    files.push(token.to_string());
}

fn subtasks_from_steps(
    task_id: &str,
    steps: &[String],
    total_turns: usize,
    limit: usize,
) -> Vec<SubtaskSpec> {
    let weights = vec![1usize; steps.len()];
    let turns = weighted_split(total_turns, &weights, limit);
    steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let files = extract_files(step);
            let outputs = outputs_for_step(step, &files);
            SubtaskSpec::new(
                format!("{task_id}-{}", index + 1),
                first_line(step, 80),
                step.clone(),
            )
            .with_max_turns(turns[index])
            .with_expected_outputs(outputs)
            .with_target_files_if_any(files)
        })
        .collect()
}

fn synthesize_phases(
    task_id: &str,
    prompt: &str,
    total_turns: usize,
    limit: usize,
) -> Vec<SubtaskSpec> {
    let turns = weighted_split(total_turns, &[2, 3, 2], limit);
    let files = extract_files(prompt);
    let verify = verify_command(prompt);
    let discover = SubtaskSpec::new(
        format!("{task_id}-1"),
        "Discover the change",
        format!("Find the files and interfaces this task must change.\n\n{prompt}"),
    )
    .with_max_turns(turns[0])
    .with_expected_outputs(vec![SubtaskExpectedOutput {
        description: "The final answer names the files and interfaces this task will change."
            .to_string(),
        kind: Some("note".to_string()),
        command: None,
        expected_exit_code: None,
        expected_stdout: None,
        file_path: None,
        expected_content: None,
    }]);
    let mut implement = SubtaskSpec::new(
        format!("{task_id}-2"),
        "Implement the change",
        format!("Make the code change described below.\n\n{prompt}"),
    )
    .with_max_turns(turns[1]);
    implement.expected_outputs = Some(if files.is_empty() {
        vec![note_output(
            "The final answer describes the code that changed.",
        )]
    } else {
        files
            .iter()
            .map(|path| file_output(&format!("`{path}` exists"), path, None))
            .collect()
    });
    if !files.is_empty() {
        implement = implement.with_target_files(files);
    }
    let verify_step = if let Some(command) = verify {
        SubtaskSpec::new(
            format!("{task_id}-3"),
            "Verify the change",
            format!("Run `{command}` and fix failures until it passes."),
        )
        .with_max_turns(turns[2])
        .with_expected_outputs(vec![command_output(
            &format!("`{command}` passes"),
            &command,
        )])
    } else {
        SubtaskSpec::new(
            format!("{task_id}-3"),
            "Verify the change",
            "Check the change against the task and report what you verified.",
        )
        .with_max_turns(turns[2])
        .with_expected_outputs(vec![note_output(
            "The final answer says what was verified.",
        )])
    };
    vec![discover, implement, verify_step]
}

trait TargetFilesIfAny {
    fn with_target_files_if_any(self, files: Vec<String>) -> Self;
}

impl TargetFilesIfAny for SubtaskSpec {
    fn with_target_files_if_any(self, files: Vec<String>) -> Self {
        if files.is_empty() {
            self
        } else {
            self.with_target_files(files)
        }
    }
}

fn outputs_for_step(step: &str, files: &[String]) -> Vec<SubtaskExpectedOutput> {
    if let Some(command) = verify_command(step) {
        return vec![command_output(&format!("`{command}` passes"), &command)];
    }
    if files.is_empty() {
        vec![note_output(&format!(
            "The final answer reports that this step is done: {}",
            first_line(step, 120)
        ))]
    } else {
        files
            .iter()
            .map(|path| file_output(&format!("`{path}` exists"), path, None))
            .collect()
    }
}

fn verify_command(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for command in [
        "cargo test",
        "pnpm test",
        "npm test",
        "pytest",
        "go test",
        "cargo fmt",
    ] {
        if lower.contains(command) {
            return Some(command.to_string());
        }
    }
    None
}

fn note_output(description: &str) -> SubtaskExpectedOutput {
    SubtaskExpectedOutput {
        description: description.to_string(),
        kind: Some("note".to_string()),
        command: None,
        expected_exit_code: None,
        expected_stdout: None,
        file_path: None,
        expected_content: None,
    }
}

fn file_output(description: &str, path: &str, content: Option<&str>) -> SubtaskExpectedOutput {
    SubtaskExpectedOutput {
        description: description.to_string(),
        kind: Some("file".to_string()),
        command: None,
        expected_exit_code: None,
        expected_stdout: None,
        file_path: Some(path.to_string()),
        expected_content: content.map(str::to_string),
    }
}

fn command_output(description: &str, command: &str) -> SubtaskExpectedOutput {
    SubtaskExpectedOutput {
        description: description.to_string(),
        kind: Some("command".to_string()),
        command: Some(command.to_string()),
        expected_exit_code: Some(0),
        expected_stdout: None,
        file_path: None,
        expected_content: None,
    }
}

fn weighted_split(total: usize, weights: &[usize], limit: usize) -> Vec<usize> {
    let count = weights.len().max(1);
    let limit = limit.max(1);
    let total = total.clamp(count, limit.max(count));
    let weight_sum = weights.iter().sum::<usize>().max(1);
    let mut turns: Vec<usize> = weights
        .iter()
        .map(|weight| ((*weight as f64 / weight_sum as f64) * total as f64).floor() as usize)
        .map(|turns| turns.clamp(1, limit))
        .collect();
    if turns.len() < count {
        turns.resize(count, 1);
    }
    let mut drift = total as isize - turns.iter().sum::<usize>() as isize;
    let mut index = 0;
    let mut guard = 0;
    while drift != 0 && guard < 10_000 {
        let slot = index % turns.len();
        if drift > 0 && turns[slot] < limit {
            turns[slot] += 1;
            drift -= 1;
        } else if drift < 0 && turns[slot] > 1 {
            turns[slot] -= 1;
            drift += 1;
        }
        index += 1;
        guard += 1;
    }
    turns
}

fn assign_missing_turns(subtasks: &mut [SubtaskSpec], total: usize, limit: usize) {
    if subtasks.is_empty() {
        return;
    }
    let missing = subtasks.iter().any(|subtask| subtask.max_turns.is_none());
    if !missing {
        for subtask in subtasks.iter_mut() {
            if let Some(turns) = subtask.max_turns {
                subtask.max_turns = Some(clamp_turns(turns, limit));
            }
        }
        return;
    }
    let split = weighted_split(total, &vec![1; subtasks.len()], limit);
    for (subtask, turns) in subtasks.iter_mut().zip(split) {
        if subtask.max_turns.is_none() {
            subtask.max_turns = Some(turns);
        } else if let Some(existing) = subtask.max_turns {
            subtask.max_turns = Some(clamp_turns(existing, limit));
        }
    }
}

fn unique_ids(task_id: &str, subtasks: &mut [SubtaskSpec]) {
    let mut seen = Vec::new();
    for (index, subtask) in subtasks.iter_mut().enumerate() {
        let mut id = subtask.id.trim().to_string();
        if id.is_empty() {
            id = format!("{task_id}-{}", index + 1);
        }
        if seen.iter().any(|existing: &String| existing == &id) {
            id = format!("{id}-{}", index + 1);
        }
        seen.push(id.clone());
        subtask.id = id;
    }
}

fn loose_subtask(index: usize, subtask: LooseSubtask, limit: usize) -> Option<SubtaskSpec> {
    let title = subtask.title.unwrap_or_default().trim().to_string();
    let description = subtask.description.unwrap_or_default().trim().to_string();
    if title.is_empty() && description.is_empty() {
        return None;
    }
    let title = if title.is_empty() {
        first_line(&description, 80)
    } else {
        title
    };
    let description = if description.is_empty() {
        title.clone()
    } else {
        description
    };
    let outputs = subtask
        .expected_outputs
        .unwrap_or_default()
        .into_iter()
        .filter_map(loose_output)
        .collect::<Vec<_>>();
    let mut spec = SubtaskSpec::new(
        subtask
            .id
            .unwrap_or_else(|| format!("subtask-{}", index + 1)),
        title,
        description,
    );
    if let Some(turns) = subtask.max_turns {
        spec.max_turns = Some(clamp_turns(turns, limit));
    }
    if let Some(budget) = subtask.exploration_budget {
        spec.exploration_budget = Some(budget);
    }
    let files = subtask
        .target_files
        .unwrap_or_default()
        .into_iter()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    if !files.is_empty() {
        spec.target_files = Some(files);
    }
    if !outputs.is_empty() {
        spec.expected_outputs = Some(outputs);
    }
    Some(spec)
}

fn loose_output(output: LooseOutput) -> Option<SubtaskExpectedOutput> {
    let loose = match output {
        LooseOutput::Text(text) => {
            let text = text.trim().to_string();
            if text.is_empty() {
                return None;
            }
            return Some(note_output(&text));
        }
        LooseOutput::Obj(obj) => obj,
    };
    let mut description = loose.description.unwrap_or_default().trim().to_string();
    let file_path = loose
        .file_path
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty());
    let command = loose
        .command
        .map(|command| command.trim().to_string())
        .filter(|command| !command.is_empty());
    if description.is_empty() {
        description = if let Some(path) = &file_path {
            format!("`{path}` matches")
        } else if let Some(command) = &command {
            format!("`{command}` passes")
        } else {
            return None;
        };
    }
    Some(SubtaskExpectedOutput {
        description,
        kind: loose.kind.map(|kind| kind.trim().to_ascii_lowercase()),
        command,
        expected_exit_code: loose.expected_exit_code,
        expected_stdout: loose
            .expected_stdout
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty()),
        file_path,
        expected_content: loose
            .expected_content
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty()),
    })
}

fn extract_json_object(raw: &str) -> String {
    let trimmed = raw.trim();
    let fenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|rest| rest.strip_suffix("```"))
        .map(str::trim);
    let candidate = fenced.unwrap_or(trimmed);
    if let (Some(start), Some(end)) = (candidate.find('{'), candidate.rfind('}')) {
        if start < end {
            if let Some(slice) = candidate.get(start..=end) {
                return slice.to_string();
            }
        }
    }
    candidate.to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelEnvelope {
    reply: Option<String>,
    large_task: Option<bool>,
    reasoning: Option<String>,
    draft: Option<LooseDraft>,
    id: Option<String>,
    name: Option<String>,
    category: Option<String>,
    tags: Option<Vec<String>>,
    prompt: Option<String>,
    max_turns: Option<usize>,
    timeout_seconds: Option<u64>,
    complexity: Option<String>,
    subtasks: Option<Vec<LooseSubtask>>,
}

impl ModelEnvelope {
    fn as_loose_draft(&self) -> Option<LooseDraft> {
        if self.prompt.is_none() && self.name.is_none() && self.subtasks.is_none() {
            return None;
        }
        Some(LooseDraft {
            id: self.id.clone(),
            name: self.name.clone(),
            category: self.category.clone(),
            tags: self.tags.clone(),
            prompt: self.prompt.clone(),
            max_turns: self.max_turns,
            timeout_seconds: self.timeout_seconds,
            large_task: self.large_task,
            complexity: self.complexity.clone(),
            reasoning: self.reasoning.clone(),
            subtasks: self.subtasks.clone(),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LooseDraft {
    id: Option<String>,
    name: Option<String>,
    category: Option<String>,
    tags: Option<Vec<String>>,
    prompt: Option<String>,
    max_turns: Option<usize>,
    timeout_seconds: Option<u64>,
    large_task: Option<bool>,
    complexity: Option<String>,
    reasoning: Option<String>,
    subtasks: Option<Vec<LooseSubtask>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LooseSubtask {
    id: Option<String>,
    title: Option<String>,
    description: Option<String>,
    target_files: Option<Vec<String>>,
    max_turns: Option<usize>,
    exploration_budget: Option<usize>,
    expected_outputs: Option<Vec<LooseOutput>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum LooseOutput {
    Text(String),
    Obj(LooseOutputObj),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LooseOutputObj {
    description: Option<String>,
    kind: Option<String>,
    command: Option<String>,
    expected_exit_code: Option<i32>,
    expected_stdout: Option<String>,
    file_path: Option<String>,
    expected_content: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> TaskDraftMessage {
        TaskDraftMessage {
            role: "user".to_string(),
            content: text.to_string(),
        }
    }

    #[test]
    fn focused_fix_stays_one_task() {
        let turn = heuristic_task_draft(&[user("Fix the typo in README.md")], None, 500);
        let draft = turn.draft.expect("draft");
        assert!(!draft.large_task);
        assert!(draft.subtasks.is_empty());
        assert_eq!(draft.complexity, "simple");
        assert!(turn.reply.contains("single run"));
    }

    #[test]
    fn numbered_steps_become_subtasks_with_output_checks() {
        let turn = heuristic_task_draft(
            &[user(
                "1. Define GpsPayload in src/dto.rs\n2. Implement notify in src/service.rs\n3. Run cargo test",
            )],
            None,
            500,
        );
        let draft = turn.draft.expect("draft");
        assert!(draft.large_task);
        assert_eq!(draft.subtasks.len(), 3);
        assert_eq!(
            draft.subtasks[0].expected_outputs.as_ref().unwrap()[0]
                .kind
                .as_deref(),
            Some("file")
        );
        assert_eq!(
            draft.subtasks[0].expected_outputs.as_ref().unwrap()[0]
                .file_path
                .as_deref(),
            Some("src/dto.rs")
        );
        assert_eq!(
            draft.subtasks[2].expected_outputs.as_ref().unwrap()[0]
                .command
                .as_deref(),
            Some("cargo test")
        );
        let total: usize = draft
            .subtasks
            .iter()
            .filter_map(|subtask| subtask.max_turns)
            .sum();
        assert_eq!(total, draft.max_turns);
    }

    #[test]
    fn complex_prose_is_split_into_three_phases() {
        let turn = heuristic_task_draft(
            &[user(
                "Refactor the entire database storage layer to use connection pooling and migrate SQLite tables to Postgres",
            )],
            None,
            500,
        );
        let draft = turn.draft.expect("draft");
        assert!(draft.large_task);
        assert_eq!(draft.complexity, "complex");
        assert_eq!(draft.subtasks.len(), 3);
        assert!(draft.subtasks.iter().all(|subtask| subtask
            .expected_outputs
            .as_ref()
            .is_some_and(|outputs| !outputs.is_empty())));
    }

    #[test]
    fn follow_up_can_keep_a_single_task() {
        let first = heuristic_task_draft(
            &[user(
                "1. Add the endpoint\n2. Write the handler\n3. Run cargo test",
            )],
            None,
            500,
        );
        let draft = first.draft.expect("draft");
        assert!(draft.large_task);
        let second = heuristic_task_draft(
            &[
                user("1. Add the endpoint\n2. Write the handler\n3. Run cargo test"),
                user("Don't split this. Keep it as one task."),
            ],
            Some(&draft),
            500,
        );
        let draft = second.draft.expect("draft");
        assert!(!draft.large_task);
        assert!(draft.subtasks.is_empty());
    }

    #[test]
    fn clarifying_question_does_not_invent_a_task() {
        let turn = heuristic_task_draft(&[user("Can you help me plan something?")], None, 500);
        assert!(turn.draft.is_none());
        assert!(turn.reply.contains('?'));
    }

    #[test]
    fn parses_fenced_model_json_and_string_outputs() {
        let raw = r#"```json
        {"reply":"Split it.","draft":{"id":"gps","name":"GPS","prompt":"Build GPS notifications","largeTask":true,"complexity":"complex","maxTurns":40,"subtasks":[{"title":"DTOs","description":"Define types","expectedOutputs":["src/dto.rs exists",{"description":"tests pass","kind":"command","command":"cargo test"}]},{"title":"Verify","description":"Run the tests","maxTurns":8}]}}
        ```"#;
        let turn = parse_model_task_draft(raw, 500).unwrap();
        assert_eq!(turn.reply, "Split it.");
        let draft = turn.draft.expect("draft");
        assert!(draft.large_task);
        assert_eq!(draft.subtasks.len(), 2);
        assert_eq!(draft.id, "task-gps");
        let outputs = draft.subtasks[0].expected_outputs.as_ref().unwrap();
        assert_eq!(outputs[0].kind.as_deref(), Some("note"));
        assert_eq!(outputs[1].command.as_deref(), Some("cargo test"));
        assert_eq!(draft.subtasks[1].max_turns, Some(8));
    }

    #[test]
    fn model_can_answer_without_a_draft() {
        let turn = parse_model_task_draft(
            r#"{"reply":"Which module should change?","draft":null}"#,
            500,
        )
        .unwrap();
        assert!(turn.draft.is_none());
        assert!(turn.reply.contains("Which module"));
    }
}
