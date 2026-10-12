use async_trait::async_trait;
use chrono::Utc;
use openduck_harness::eval::progress_summary::run_progress_summary_from_steps;
use openduck_harness::policy::adapters::EchoPolicy;
use openduck_harness::policy::{AgentAction, AgentPolicy, HarnessContextView, MessageRole};
use openduck_harness::runtime::{prior_history_messages, AgentHarness, HistoryFollowUp};
use openduck_harness::sandbox::local::LocalSandbox;
use openduck_harness::telemetry::TrajectoryStep;
use openduck_harness::types::{RunStatus, ToolCallRequest, ToolCallResponse, ToolDefinition};
use openduck_harness::{
    checkpoint_from_trajectory, is_history_follow_up_status, ContinuationCheckpoint,
    ContinuationStopReason, SubtaskSpec, TaskSpec,
};
use serde_json::json;

fn tool_step(
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
            error: None,
        }]),
        tool_results: Some(vec![ToolCallResponse {
            id: format!("c{index}"),
            name: name.to_string(),
            output: output.to_string(),
            is_error,
        }]),
        duration_ms: 10,
        token_usage: None,
        llm_request: None,
        llm_response: None,
        judgments: None,
        provider_calls: None,
    }
}

#[test]
fn cancelled_run_summary_is_for_continuation_not_compaction() {
    let steps = vec![
        tool_step(
            80,
            "write_file",
            json!({"path": ".agent/notes-plan-task-297953.md", "content": "## Remaining work\n- Add export route\n"}),
            "ok",
            false,
        ),
        tool_step(
            88,
            "write_file",
            json!({"path": "rmqx_admin/src/util/xlsx.rs", "content": "pub fn write_xlsx() {}"}),
            "ok",
            false,
        ),
        tool_step(
            261,
            "shell",
            json!({"command": "cargo test -p rmqx_admin --lib bms_export"}),
            "test result: FAILED. 6 passed; 1 failed",
            false,
        ),
        tool_step(
            298,
            "shell",
            json!({"command": "git status --porcelain"}),
            " M rmqx_admin/src/service/mod.rs\n?? rmqx_admin/src/util/xlsx.rs\n",
            false,
        ),
    ];

    let summary = run_progress_summary_from_steps(
        RunStatus::Cancelled,
        ContinuationStopReason::Cancelled,
        Some("hit max turns"),
        &steps,
    );
    assert!(summary.contains("# Incomplete task run summary"));
    assert!(summary.contains("`rmqx_admin/src/util/xlsx.rs`"));
    assert!(summary.contains("6 passed; 1 failed"));
    assert!(summary.contains("Add export route"));
    assert!(summary.contains("rmqx_admin/src/service/mod.rs"));
    assert!(summary.contains(".agent/notes-plan-task-297953.md"));
    assert!(!summary.contains("COMPACTED CONTEXT SUMMARY"));
    let files_section = summary.split("## Scratchpad").next().unwrap();
    assert!(
        !files_section.contains("notes-plan-task-297953.md"),
        "per-task notes-plan must not be listed as a product write: {files_section}"
    );

    let checkpoint = checkpoint_from_trajectory(
        "task-297953",
        "20260913_130755_task_task-297953_01a09ae1",
        RunStatus::Cancelled,
        Some("hit max turns".into()),
        &steps,
    );
    assert_eq!(checkpoint.stop_reason, ContinuationStopReason::Cancelled);
    assert!(checkpoint
        .compacted_summary
        .contains("Incomplete task run summary"));
    assert!(checkpoint
        .prior_progress_prompt()
        .contains(".agent/continuation-summary.md"));
}

#[tokio::test]
async fn continuing_a_run_writes_the_summary_into_the_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let sandbox = LocalSandbox::new(dir.path().to_path_buf());
    let checkpoint = ContinuationCheckpoint {
        task_id: "task-297953".into(),
        source_job_id: "20260913_130755_task_task-297953_01a09ae1".into(),
        created_at: Utc::now(),
        stop_reason: ContinuationStopReason::Cancelled,
        error: Some("hit max turns".into()),
        compacted_summary: "# Incomplete task run summary\n- Wrote `src/xlsx.rs`\n".into(),
        completed_subtasks: vec![],
        resume_subtask_id: None,
        subtask_plan: vec![],
        step_count: 300,
        advisor_sessions: Default::default(),
    };

    let mut harness = AgentHarness::new(EchoPolicy::default(), sandbox)
        .with_continuation(Some(checkpoint))
        .with_extra_turns(Some(5));
    let task = TaskSpec::new("task-297953", "feature", "Export BMS history")
        .with_auto_decompose(false)
        .with_turns(3);
    let result = harness.run_task(&task).await.unwrap();
    assert_eq!(result.status, RunStatus::Success);

    let written = std::fs::read_to_string(dir.path().join(".agent/continuation-summary.md"))
        .expect("continuation summary file");
    assert!(written.contains("Wrote `src/xlsx.rs`"));
}

struct TranscriptPolicy;

#[async_trait]
impl AgentPolicy for TranscriptPolicy {
    fn name(&self) -> &str {
        "transcript"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        let mut lines = Vec::new();
        for message in &context.messages {
            let role = match message.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };
            let body = if !message.content.is_empty() {
                message.content.clone()
            } else if let Some(results) = &message.tool_results {
                results
                    .first()
                    .map(|result| result.output.clone())
                    .unwrap_or_default()
            } else if message.tool_calls.is_some() {
                "tool-call".to_string()
            } else {
                String::new()
            };
            lines.push(format!("{role}:{body}"));
        }
        Ok(AgentAction::FinalAnswer(lines.join("\n")))
    }
}

#[test]
fn prior_history_keeps_an_earlier_follow_up_and_skips_the_opening_prompt() {
    let mut step = tool_step(2, "shell", json!({"command": "cargo test"}), "ok", false);
    step.action = AgentAction::FinalAnswer("Done".into());
    step.tool_results = None;
    step.llm_request = Some(json!({
        "messages": [
            {
                "role": "user",
                "content": [{ "type": "text", "text": "old map\n## Task Goal:\nExport the report" }]
            },
            { "role": "user", "content": "Tighten the summary" },
            { "role": "user", "content": [{ "type": "toolResponse", "id": "t1" }] }
        ]
    }));

    let opening = "new map\n## Task Goal:\nExport the report";
    let messages = prior_history_messages(&[step], opening);
    let user_texts: Vec<_> = messages
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(user_texts, vec!["Tighten the summary"]);
    assert!(messages
        .iter()
        .any(|message| message.role == MessageRole::Assistant && message.content == "Done"));
    assert!(!is_history_follow_up_status(RunStatus::Failure));
    assert!(is_history_follow_up_status(RunStatus::Success));
    assert!(is_history_follow_up_status(RunStatus::Cancelled));
}

#[tokio::test]
async fn history_follow_up_replays_the_conversation_and_appends_the_new_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let sandbox = LocalSandbox::new(dir.path().to_path_buf());
    let checkpoint = ContinuationCheckpoint {
        task_id: "task-export".into(),
        source_job_id: "run-1".into(),
        created_at: Utc::now(),
        stop_reason: ContinuationStopReason::Cancelled,
        error: Some("hit max turns".into()),
        compacted_summary: "# Incomplete task run summary\n- Wrote `src/xlsx.rs`\n".into(),
        completed_subtasks: vec![],
        resume_subtask_id: None,
        subtask_plan: vec![],
        step_count: 2,
        advisor_sessions: Default::default(),
    };
    let prior = vec![
        tool_step(
            1,
            "write_file",
            json!({"path": "src/report.rs", "content": "export"}),
            "wrote report",
            false,
        ),
        {
            let mut step = tool_step(2, "shell", json!({"command": "true"}), "ok", false);
            step.action = AgentAction::FinalAnswer("Done".into());
            step.tool_results = None;
            step.llm_request = Some(json!({
                "messages": [
                    { "role": "user", "content": "Export the report" },
                    { "role": "user", "content": "Tighten the summary" }
                ]
            }));
            step
        },
    ];

    let mut harness = AgentHarness::new(TranscriptPolicy, sandbox)
        .with_continuation(Some(checkpoint))
        .with_history_follow_up(Some(HistoryFollowUp {
            steps: prior,
            prompt: "Add a changelog".into(),
        }))
        .with_extra_turns(Some(4));
    let task = TaskSpec::new("task-export", "feature", "Export the report")
        .with_auto_decompose(true)
        .with_subtasks(vec![
            SubtaskSpec::new("write", "Write", "Write the exporter"),
            SubtaskSpec::new("test", "Test", "Run the tests"),
        ])
        .with_turns(2);
    let result = harness.run_task(&task).await.unwrap();
    assert_eq!(result.status, RunStatus::Success);
    let answer = result.final_answer.unwrap();
    assert!(
        answer.contains("user:Export the report"),
        "opening task prompt missing:\n{answer}"
    );
    assert!(
        answer.contains("user:Tighten the summary"),
        "earlier follow-up missing:\n{answer}"
    );
    assert!(
        answer.contains("user:Add a changelog"),
        "new prompt missing:\n{answer}"
    );
    assert!(
        answer.contains("tool:wrote report"),
        "prior tool result missing:\n{answer}"
    );
    assert!(
        answer.contains("assistant:Done"),
        "prior answer missing:\n{answer}"
    );
    assert!(
        !answer.contains("Prior incomplete run summary"),
        "summary continuation must not replace the transcript:\n{answer}"
    );
    assert!(
        !answer.contains("Current Subtask"),
        "follow-up must not restart the subtask plan:\n{answer}"
    );
    assert!(answer.ends_with("user:Add a changelog"));
    assert_eq!(result.trajectory.steps.len(), 3);
    assert_eq!(result.trajectory.steps[0].step_number, 1);
    assert_eq!(result.trajectory.steps[2].step_number, 3);
    assert!(matches!(
        result.trajectory.steps[2].action,
        AgentAction::FinalAnswer(_)
    ));
    assert!(!dir.path().join(".agent/continuation-summary.md").exists());
}
