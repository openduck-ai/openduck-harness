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

#[test]
fn test_subtask_decomposition_from_numbered_list() {
    let prompt = r#"
Please complete the GPS notification feature:
1. Define DTOs and configuration models in rmqx_admin/src/domain/dto/gps_platform.rs
2. Implement SMS notification dispatch in rmqx_admin/src/domain/services/gps_notify.rs
3. Update frontend alarm rule configuration form in ui/src/views/AlarmRuleList.vue
4. Run cargo test -p rmqx_admin to verify all integration test cases
"#;

    let subtasks = SubtaskDecomposer::decompose("task-gps", prompt, None, None);
    assert_eq!(subtasks.len(), 4, "Should extract exactly 4 subtasks");

    assert!(subtasks[0].title.contains("Define DTOs"));
    assert_eq!(
        subtasks[0].target_files.as_deref(),
        Some(&["rmqx_admin/src/domain/dto/gps_platform.rs".to_string()][..])
    );

    assert!(subtasks[1].title.contains("Implement SMS notification"));
    assert_eq!(
        subtasks[1].target_files.as_deref(),
        Some(&["rmqx_admin/src/domain/services/gps_notify.rs".to_string()][..])
    );

    assert!(subtasks[2].title.contains("Update frontend alarm rule"));
    assert_eq!(
        subtasks[2].target_files.as_deref(),
        Some(&["ui/src/views/AlarmRuleList.vue".to_string()][..])
    );

    assert!(subtasks[3].title.contains("Run cargo test"));
}

#[test]
fn test_subtask_decomposition_from_markdown_checklist() {
    let prompt = r#"
We need to migrate telemetry storage:
- [ ] Phase A: Add clickhouse client dependency and connection pool in src/db.rs
- [ ] Phase B: Create telemetry table migration and schema in migrations/001_telemetry.sql
- [ ] Phase C: Implement telemetry batch writer in src/telemetry/writer.rs
"#;

    let subtasks = SubtaskDecomposer::decompose("task-telemetry", prompt, None, None);
    assert_eq!(subtasks.len(), 3);
    assert!(subtasks[0].title.contains("Phase A"));
    assert!(subtasks[1].title.contains("Phase B"));
    assert!(subtasks[2].title.contains("Phase C"));
}

#[test]
fn test_subtask_decomposition_for_complex_prose() {
    let prompt = "Please implement the new multi-channel notification engine supporting SMS, AppPush, and Webhook dispatching with appropriate DTO schemas, service handlers, DB records, and end-to-end integration tests.";
    let subtasks = SubtaskDecomposer::decompose("task-prose", prompt, Some(28), None);
    assert_eq!(
        subtasks.len(),
        3,
        "Complex prose should synthesize 3 phases"
    );
    assert!(subtasks[0].title.contains("Phase 1"));
    assert!(subtasks[1].title.contains("Phase 2"));
    assert!(subtasks[2].title.contains("Phase 3"));
    assert_eq!(subtasks[0].max_turns, Some(8));
    assert_eq!(subtasks[1].max_turns, Some(12));
    assert_eq!(subtasks[2].max_turns, Some(8));
}

#[test]
fn test_synthesized_phases_split_parent_max_turns() {
    assert_eq!(
        openduck_harness::split_synthesized_phase_turns(28),
        [8, 12, 8]
    );
    let split_500 = openduck_harness::split_synthesized_phase_turns(500);
    assert_eq!(split_500, [142, 216, 142]);
    assert!(split_500[1] > split_500[0]);
    assert_eq!(split_500.iter().sum::<usize>(), 500);

    let prompt = "Please implement the new multi-channel notification engine supporting SMS, AppPush, and Webhook dispatching with appropriate DTO schemas, service handlers, DB records, and end-to-end integration tests.";
    let subtasks = SubtaskDecomposer::decompose("task-prose", prompt, Some(500), None);
    assert_eq!(subtasks[0].max_turns, Some(142));
    assert_eq!(subtasks[1].max_turns, Some(216));
    assert_eq!(subtasks[2].max_turns, Some(142));
}

#[test]
fn test_synthesized_phases_honor_phase_max_turns_overrides() {
    let prompt = "Please implement the new multi-channel notification engine supporting SMS, AppPush, and Webhook dispatching with appropriate DTO schemas, service handlers, DB records, and end-to-end integration tests.";
    let subtasks =
        SubtaskDecomposer::decompose("task-prose", prompt, Some(500), Some(&[80, 200, 220][..]));
    assert_eq!(subtasks[0].max_turns, Some(80));
    assert_eq!(subtasks[1].max_turns, Some(200));
    assert_eq!(subtasks[2].max_turns, Some(220));
}

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
                return Ok(AgentAction::CallTools(vec![ToolCallRequest {
                    id: "call-1".into(),
                    name: "write_file".into(),
                    arguments: serde_json::json!({
                        "path": "src/dto.rs",
                        "content": "pub struct GpsPayload { pub lat: f64, pub lon: f64 }\n"
                    }),
                }]));
            } else {
                return Ok(AgentAction::FinalAnswer("DTO defined in src/dto.rs".into()));
            }
        }

        // Subtask 2: Read src/dto.rs and write src/service.rs
        if active_subtask_title.contains("Subtask (2/3)") {
            if current_step == 2 {
                return Ok(AgentAction::CallTools(vec![ToolCallRequest {
                    id: "call-2".into(),
                    name: "read_file".into(),
                    arguments: serde_json::json!({"path": "src/dto.rs"}),
                }]));
            } else if current_step == 3 {
                return Ok(AgentAction::CallTools(vec![ToolCallRequest {
                    id: "call-3".into(),
                    name: "write_file".into(),
                    arguments: serde_json::json!({
                        "path": "src/service.rs",
                        "content": "use crate::dto::GpsPayload;\npub fn notify(_p: GpsPayload) -> bool { true }\n"
                    }),
                }]));
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
        ),
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
