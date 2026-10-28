use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

use async_trait::async_trait;
use openduck_harness::eval::subtask::SubtaskDecomposer;
use openduck_harness::policy::{AgentAction, AgentPolicy, HarnessContextView};
use openduck_harness::runtime::AgentHarness;
use openduck_harness::sandbox::local::LocalSandbox;
use openduck_harness::types::{RunStatus, ToolCallRequest, ToolDefinition};
use openduck_harness::{SubtaskSpec, TaskSpec};

#[derive(Clone)]
struct SequentialSubtaskPolicy {
    step: Arc<AtomicUsize>,
    executed_subtasks: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl AgentPolicy for SequentialSubtaskPolicy {
    fn name(&self) -> &str {
        "SequentialSubtaskPolicy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        let current_step = self.step.fetch_add(1, Ordering::SeqCst);

        // Find which subtask is active from user message
        let active_subtask_title = context
            .messages
            .iter()
            .find_map(|m| {
                if m.content.contains("## Current Subtask") {
                    Some(
                        m.content
                            .lines()
                            .find(|l| l.starts_with("## Current Subtask"))
                            .unwrap_or("")
                            .to_string(),
                    )
                } else {
                    None
                }
            })
            .unwrap_or_default();

        if !active_subtask_title.is_empty() {
            let mut list = self.executed_subtasks.lock().unwrap();
            if !list.contains(&active_subtask_title) {
                list.push(active_subtask_title.clone());
            }
        }

        // Subtask 1: Write src/dto.rs
        if active_subtask_title.contains("Subtask (1/3)") {
            if current_step == 0 {
                return Ok(AgentAction::CallTools(vec![ToolCallRequest::new(
                    "call-1",
                    "write_file",
                    serde_json::json!({
                        "path": "src/dto.rs",
                        "content": "pub struct GpsPayload { pub lat: f64, pub lon: f64 }\n"
                    }),
                )]));
            } else {
                return Ok(AgentAction::FinalAnswer("DTO defined in src/dto.rs".into()));
            }
        }

        // Subtask 2: Read src/dto.rs and write src/service.rs
        if active_subtask_title.contains("Subtask (2/3)") {
            if current_step == 2 {
                return Ok(AgentAction::CallTools(vec![ToolCallRequest::new(
                    "call-2",
                    "read_file",
                    serde_json::json!({"path": "src/dto.rs"}),
                )]));
            } else if current_step == 3 {
                return Ok(AgentAction::CallTools(vec![ToolCallRequest::new(
                    "call-3",
                    "write_file",
                    serde_json::json!({
                        "path": "src/service.rs",
                        "content": "use crate::dto::GpsPayload;\npub fn notify(_p: GpsPayload) -> bool { true }\n"
                    }),
                )]));
            } else {
                return Ok(AgentAction::FinalAnswer(
                    "Service implemented in src/service.rs".into(),
                ));
            }
        }

        // Subtask 3: Finalize and verify
        Ok(AgentAction::FinalAnswer("All subtasks verified".into()))
    }
}

#[tokio::test]
async fn test_sequential_subtask_execution_in_shared_sandbox() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("src")).unwrap();

    let step = Arc::new(AtomicUsize::new(0));
    let executed_subtasks = Arc::new(std::sync::Mutex::new(Vec::new()));

    let policy = SequentialSubtaskPolicy {
        step: step.clone(),
        executed_subtasks: executed_subtasks.clone(),
    };

    let sandbox = LocalSandbox::new(root.clone());
    let mut harness = AgentHarness::new(policy, sandbox);

    let subtasks = vec![
        SubtaskSpec::new(
            "st-1",
            "Define DTOs",
            "Define GpsPayload struct in src/dto.rs",
        )
        .with_expected_outputs(vec![openduck_harness::SubtaskExpectedOutput {
            description: "GpsPayload is defined".into(),
            kind: Some("file".into()),
            command: None,
            expected_exit_code: None,
            expected_stdout: None,
            file_path: Some("src/dto.rs".into()),
            expected_content: Some("struct GpsPayload".into()),
        }]),
        SubtaskSpec::new(
            "st-2",
            "Implement Service",
            "Implement notify function in src/service.rs",
        ),
        SubtaskSpec::new("st-3", "Verification", "Verify integration"),
    ];

    let task = TaskSpec::new("task-sequential", "test", "Build GPS system").with_subtasks(subtasks);

    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(result.step_count, 6);

    // Verify files created in Subtask 1 and 2 persist in the shared sandbox
    assert!(
        root.join("src/dto.rs").exists(),
        "src/dto.rs must exist in workspace"
    );
    assert!(
        root.join("src/service.rs").exists(),
        "src/service.rs must exist in workspace"
    );

    let subtask_history = executed_subtasks.lock().unwrap().clone();
    assert_eq!(
        subtask_history.len(),
        3,
        "All 3 subtasks must execute in order"
    );
    assert!(subtask_history[0].contains("Subtask (1/3)"));
    assert!(subtask_history[1].contains("Subtask (2/3)"));
    assert!(subtask_history[2].contains("Subtask (3/3)"));

    let final_ans = result.final_answer.expect("Final answer should be present");
    assert!(final_ans.contains("DTO defined in src/dto.rs"));
    assert!(final_ans.contains("Service implemented in src/service.rs"));
}

struct FailOnSecondSubtaskPolicy {
    executed_subtasks: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl AgentPolicy for FailOnSecondSubtaskPolicy {
    fn name(&self) -> &str {
        "FailOnSecondSubtaskPolicy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        let active = context
            .messages
            .iter()
            .find_map(|m| {
                m.content
                    .lines()
                    .find(|l| l.starts_with("## Current Subtask"))
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_default();
        if !active.is_empty() {
            let mut list = self.executed_subtasks.lock().unwrap();
            if !list.contains(&active) {
                list.push(active.clone());
            }
        }
        if active.contains("Subtask (2/3)") {
            return Err(anyhow::anyhow!(
                "Goose provider complete failed: Network error"
            ));
        }
        Ok(AgentAction::FinalAnswer(format!("done: {active}")))
    }
}

#[tokio::test]
async fn test_sequential_subtasks_stop_on_failure() {
    let dir = tempdir().unwrap();
    let executed_subtasks = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed_lens = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed_clone = observed_lens.clone();

    let policy = FailOnSecondSubtaskPolicy {
        executed_subtasks: executed_subtasks.clone(),
    };
    let sandbox = LocalSandbox::new(dir.path().to_path_buf());
    let mut harness =
        AgentHarness::new(policy, sandbox).with_step_observer(Arc::new(move |steps| {
            observed_clone.lock().unwrap().push(steps.len());
        }));

    let subtasks = vec![
        SubtaskSpec::new("st-1", "Define DTOs", "Define models"),
        SubtaskSpec::new("st-2", "Implement Service", "Implement service"),
        SubtaskSpec::new("st-3", "Verification", "Verify"),
    ];
    let task = TaskSpec::new("task-fail-mid", "test", "Build GPS system").with_subtasks(subtasks);

    let result = harness.run_task(&task).await.unwrap();
    assert_eq!(result.status, RunStatus::Failure);

    let history = executed_subtasks.lock().unwrap().clone();
    assert_eq!(history.len(), 2, "subtask 3 must not run after failure");
    assert!(history[0].contains("Subtask (1/3)"));
    assert!(history[1].contains("Subtask (2/3)"));
    let last_len = observed_lens.lock().unwrap().last().copied().unwrap_or(0);
    assert!(
        last_len >= 2,
        "live observer should keep subtask 1 steps plus the failed subtask 2 step"
    );
}

#[test]
fn test_format_subtask_prompt_includes_upstream_outcomes() {
    let task = TaskSpec::new("task-gps", "test", "Build GPS notifications");
    let subtask = SubtaskSpec::new("st-2", "Implement Service", "Write notify()");
    let outcomes = vec![openduck_harness::SubtaskOutcome {
        subtask_id: "st-1".into(),
        title: "Define DTOs".into(),
        status: RunStatus::Success,
        modified_files: vec!["src/dto.rs".into()],
        summary: "Defined GpsPayload".into(),
        step_count: 2,
        steps_taken: 2,
        output_check: None,
    }];
    let prompt = SubtaskDecomposer::format_subtask_prompt(&task, &subtask, 1, 3, &outcomes);
    assert!(prompt.contains("Completed Upstream Subtasks"));
    assert!(prompt.contains("st-1"));
    assert!(prompt.contains("src/dto.rs"));
    assert!(prompt.contains("Defined GpsPayload"));
    assert!(prompt.contains("Subtask (2/3)"));
}

#[tokio::test]
async fn test_continue_from_checkpoint_skips_completed_subtask() {
    struct ResumePolicy {
        executed: Arc<std::sync::Mutex<Vec<String>>>,
    }
    #[async_trait]
    impl AgentPolicy for ResumePolicy {
        fn name(&self) -> &str {
            "resume-policy"
        }
        async fn step(
            &mut self,
            context: &HarnessContextView,
            _tools: &[ToolDefinition],
        ) -> anyhow::Result<AgentAction> {
            let active = context
                .messages
                .iter()
                .find_map(|m| {
                    m.content
                        .lines()
                        .find(|l| l.starts_with("## Current Subtask"))
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_default();
            self.executed.lock().unwrap().push(active.clone());
            if active.contains("Subtask (2/3)") {
                assert!(
                    context
                        .messages
                        .iter()
                        .any(|m| m.content.contains("Prior incomplete run summary")),
                    "resume prompt must include the prior run progress summary"
                );
            }
            Ok(AgentAction::FinalAnswer("resumed".into()))
        }
    }

    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sandbox = LocalSandbox::ephemeral().unwrap();
    let subtasks = vec![
        SubtaskSpec::new("st-1", "Define DTOs", "Define models"),
        SubtaskSpec::new("st-2", "Implement Service", "Implement service"),
        SubtaskSpec::new("st-3", "Verification", "Verify"),
    ];
    let checkpoint = openduck_harness::ContinuationCheckpoint {
        task_id: "task-resume".into(),
        source_job_id: "job-1".into(),
        created_at: chrono::Utc::now(),
        stop_reason: openduck_harness::ContinuationStopReason::Failure,
        error: Some("network".into()),
        compacted_summary: "Modified Files:\n- src/dto.rs".into(),
        completed_subtasks: vec![openduck_harness::SubtaskOutcome {
            subtask_id: "st-1".into(),
            title: "Define DTOs".into(),
            status: RunStatus::Success,
            modified_files: vec!["src/dto.rs".into()],
            summary: "DTO done".into(),
            step_count: 2,
            steps_taken: 2,
            output_check: None,
        }],
        resume_subtask_id: Some("st-2".into()),
        subtask_plan: subtasks.clone(),
        step_count: 2,
        advisor_sessions: Default::default(),
    };

    let mut harness = AgentHarness::new(
        ResumePolicy {
            executed: executed.clone(),
        },
        sandbox,
    )
    .with_continuation(Some(checkpoint));
    let task = TaskSpec::new("task-resume", "test", "Build GPS").with_subtasks(subtasks);
    let result = harness.run_task(&task).await.unwrap();
    assert_eq!(result.status, RunStatus::Success);
    let history = executed.lock().unwrap().clone();
    assert!(
        history.iter().all(|h| !h.contains("Subtask (1/3)")),
        "completed subtask 1 must be skipped: {history:?}"
    );
    assert!(history.iter().any(|h| h.contains("Subtask (2/3)")));
}

#[tokio::test]
async fn test_sequential_subtasks_stop_when_expected_output_is_missing() {
    struct AnswerOnlyPolicy {
        executed: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl AgentPolicy for AnswerOnlyPolicy {
        fn name(&self) -> &str {
            "answer-only"
        }

        async fn step(
            &mut self,
            context: &HarnessContextView,
            _tools: &[ToolDefinition],
        ) -> anyhow::Result<AgentAction> {
            let prompt = context
                .messages
                .iter()
                .find(|message| message.content.contains("## Current Subtask"))
                .map(|message| message.content.clone())
                .unwrap_or_default();
            let active = prompt
                .lines()
                .find(|line| line.starts_with("## Current Subtask"))
                .unwrap_or("")
                .to_string();
            if !active.is_empty() {
                let mut list = self.executed.lock().unwrap();
                if !list.contains(&active) {
                    list.push(active);
                }
            }
            assert!(
                prompt.contains("Expected outputs"),
                "subtask prompt must name the output check"
            );
            assert!(prompt.contains("src/missing.rs"));
            Ok(AgentAction::FinalAnswer(
                "I finished without writing the file".into(),
            ))
        }
    }

    let dir = tempdir().unwrap();
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sandbox = LocalSandbox::new(dir.path().to_path_buf());
    let mut harness = AgentHarness::new(
        AnswerOnlyPolicy {
            executed: executed.clone(),
        },
        sandbox,
    );
    let subtasks = vec![
        SubtaskSpec::new("st-1", "Write the type", "Create src/missing.rs").with_expected_outputs(
            vec![openduck_harness::SubtaskExpectedOutput {
                description: "src/missing.rs exists".into(),
                kind: Some("file".into()),
                command: None,
                expected_exit_code: None,
                expected_stdout: None,
                file_path: Some("src/missing.rs".into()),
                expected_content: None,
            }],
        ),
        SubtaskSpec::new("st-2", "Use the type", "Read src/missing.rs"),
    ];
    let task = TaskSpec::new("task-output-gate", "test", "Add a type").with_subtasks(subtasks);
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Failure);
    let history = executed.lock().unwrap().clone();
    assert_eq!(history.len(), 1, "subtask 2 must wait for the missing file");
    let answer = result.final_answer.unwrap_or_default();
    assert!(answer.contains("I finished without writing the file"));
    assert!(
        result
            .continuation
            .as_ref()
            .and_then(|checkpoint| checkpoint.completed_subtasks.first())
            .is_some_and(|outcome| {
                outcome.status == RunStatus::Failure
                    && outcome
                        .output_check
                        .as_deref()
                        .is_some_and(|text| text.contains("src/missing.rs"))
            }),
        "the failed output check is stored on the subtask outcome"
    );
}

#[tokio::test]
async fn test_failed_subtask_writes_explicit_failure_reason_in_conclusion() {
    struct HangsOnSecondPolicy;

    #[async_trait]
    impl AgentPolicy for HangsOnSecondPolicy {
        fn name(&self) -> &str {
            "hangs-on-second"
        }
        async fn step(
            &mut self,
            context: &HarnessContextView,
            _tools: &[ToolDefinition],
        ) -> anyhow::Result<AgentAction> {
            let active = context
                .messages
                .iter()
                .find_map(|m| {
                    m.content
                        .lines()
                        .find(|l| l.starts_with("## Current Subtask"))
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_default();
            if active.contains("Subtask (1/2)") {
                Ok(AgentAction::FinalAnswer(
                    "Subtask 1 completed successfully".into(),
                ))
            } else {
                // In subtask 2, call a tool repeatedly to exhaust turn budget
                Ok(AgentAction::CallTools(vec![ToolCallRequest {
                    id: "c1".into(),
                    name: "read_file".into(),
                    arguments: serde_json::json!({"path": "dummy.txt"}),
                    error: None,
                    metadata: None,
                }]))
            }
        }
    }

    let dir = tempdir().unwrap();
    let sandbox = LocalSandbox::new(dir.path().to_path_buf());
    let mut harness = AgentHarness::new(HangsOnSecondPolicy, sandbox);
    let subtasks = vec![
        SubtaskSpec::new("st-1", "Setup Base", "Setup base").with_max_turns(5),
        SubtaskSpec::new("st-2", "Audio Module", "Fix audio").with_max_turns(3),
    ];
    let task = TaskSpec::new("task-hang", "test", "Fix audio bugs").with_subtasks(subtasks);
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Cancelled);
    let conclusion = result
        .final_answer
        .expect("conclusion must be present on failure");
    assert!(
        conclusion.contains("## ❌ Task Incomplete / Failed (Cancelled)"),
        "conclusion must contain failure banner: {conclusion}"
    );
    assert!(
        conclusion.contains("Failure Reason"),
        "conclusion must state Failure Reason: {conclusion}"
    );
    assert!(
        conclusion.contains("Audio Module") || conclusion.contains("st-2"),
        "conclusion must identify the failing subtask: {conclusion}"
    );
    assert!(
        conclusion.contains("Reached maximum turn limit (3 turns)"),
        "conclusion must specify that max turn limit was exhausted: {conclusion}"
    );
    assert!(
        conclusion.contains("Subtask 1 completed successfully"),
        "conclusion must retain prior successful subtask answers: {conclusion}"
    );
}

#[tokio::test]
async fn test_continue_from_checkpoint_preserves_earlier_subtask_conclusions() {
    struct ResumeAndFinishPolicy;

    #[async_trait]
    impl AgentPolicy for ResumeAndFinishPolicy {
        fn name(&self) -> &str {
            "resume-and-finish"
        }
        async fn step(
            &mut self,
            _context: &HarnessContextView,
            _tools: &[ToolDefinition],
        ) -> anyhow::Result<AgentAction> {
            Ok(AgentAction::FinalAnswer(
                "Subtask 2 completed on resume".into(),
            ))
        }
    }

    let dir = tempdir().unwrap();
    let sandbox = LocalSandbox::new(dir.path().to_path_buf());
    let subtasks = vec![
        SubtaskSpec::new("st-1", "Button Fix", "Fix button"),
        SubtaskSpec::new("st-2", "Audio Fix", "Fix audio"),
    ];
    let checkpoint = openduck_harness::ContinuationCheckpoint {
        task_id: "task-audio".into(),
        source_job_id: "job-orig".into(),
        created_at: chrono::Utc::now(),
        stop_reason: openduck_harness::ContinuationStopReason::Cancelled,
        error: Some("Reached turn limit".into()),
        compacted_summary: "Subtask 1 was done".into(),
        completed_subtasks: vec![openduck_harness::SubtaskOutcome {
            subtask_id: "st-1".into(),
            title: "Button Fix".into(),
            status: RunStatus::Success,
            modified_files: vec!["src/btn.py".into()],
            summary: "Button relief displacement fixed".into(),
            step_count: 5,
            steps_taken: 5,
            output_check: None,
        }],
        resume_subtask_id: Some("st-2".into()),
        subtask_plan: subtasks.clone(),
        step_count: 5,
        advisor_sessions: Default::default(),
    };

    let mut harness = AgentHarness::new(ResumeAndFinishPolicy, sandbox)
        .with_continuation(Some(checkpoint))
        .with_extra_turns(Some(10));
    let task = TaskSpec::new("task-audio", "test", "Fix button and audio").with_subtasks(subtasks);
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Success);
    let conclusion = result.final_answer.expect("conclusion must exist");
    assert!(
        conclusion.contains("Button relief displacement fixed"),
        "conclusion must preserve earlier subtask summary: {conclusion}"
    );
    assert!(
        conclusion.contains("Subtask 2 completed on resume"),
        "conclusion must contain new subtask summary: {conclusion}"
    );
}

#[tokio::test]
async fn test_single_task_failure_includes_explicit_reason_in_conclusion() {
    struct InfiniteToolPolicy;

    #[async_trait]
    impl AgentPolicy for InfiniteToolPolicy {
        fn name(&self) -> &str {
            "infinite-tool"
        }
        async fn step(
            &mut self,
            _context: &HarnessContextView,
            _tools: &[ToolDefinition],
        ) -> anyhow::Result<AgentAction> {
            Ok(AgentAction::CallTools(vec![ToolCallRequest {
                id: "c1".into(),
                name: "read_file".into(),
                arguments: serde_json::json!({"path": "file.txt"}),
                error: None,
                metadata: None,
            }]))
        }
    }

    let dir = tempdir().unwrap();
    let sandbox = LocalSandbox::new(dir.path().to_path_buf());
    let mut harness = AgentHarness::new(InfiniteToolPolicy, sandbox).with_max_turns(3);
    let task = TaskSpec::new("task-single-burn", "test", "Fix bug in single task").with_turns(3);
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Cancelled);
    let conclusion = result
        .final_answer
        .expect("conclusion must exist on failure");
    assert!(
        conclusion.contains("## ❌ Task Failed: Status Cancelled"),
        "conclusion must contain failure banner: {conclusion}"
    );
    assert!(
        conclusion.contains("Reached maximum turn limit (3 turns)"),
        "conclusion must specify turn limit was reached: {conclusion}"
    );
}
