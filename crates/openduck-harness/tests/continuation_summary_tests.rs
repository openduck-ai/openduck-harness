use chrono::Utc;
use openduck_harness::eval::progress_summary::run_progress_summary_from_steps;
use openduck_harness::policy::adapters::EchoPolicy;
use openduck_harness::policy::AgentAction;
use openduck_harness::runtime::AgentHarness;
use openduck_harness::sandbox::local::LocalSandbox;
use openduck_harness::telemetry::TrajectoryStep;
use openduck_harness::types::{RunStatus, ToolCallRequest, ToolCallResponse};
use openduck_harness::{
    checkpoint_from_trajectory, ContinuationCheckpoint, ContinuationStopReason, TaskSpec,
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
