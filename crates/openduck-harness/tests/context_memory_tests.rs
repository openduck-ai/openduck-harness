use async_trait::async_trait;
use openduck_harness::policy::{
    AgentAction, AgentPolicy, HarnessContextView, HarnessMessage, StepTelemetry,
};
use openduck_harness::runtime::{
    compact_context_messages, extract_context_facts, AgentHarness, RepetitiveCallDetector,
};
use openduck_harness::sandbox::local::LocalSandbox;
use openduck_harness::types::{RunStatus, ToolCallRequest, ToolCallResponse, ToolDefinition};
use openduck_harness::TaskSpec;
use serde_json::json;
use std::sync::{Arc, Mutex};

fn history() -> Vec<HarnessMessage> {
    vec![
        HarnessMessage::system("Instructions"),
        HarnessMessage::user("Diagnose the bug"),
    ]
}

fn pad(messages: &mut Vec<HarnessMessage>, count: usize) {
    for index in 0..count {
        messages.push(HarnessMessage::user(format!("Continue {index}")));
    }
}

#[test]
fn repeated_compaction_preserves_findings_and_successful_writes() {
    let mut messages = history();
    messages.push(HarnessMessage::assistant_with_tools(
        "The power_save setting is never consumed.",
        vec![ToolCallRequest::new(
            "write",
            "write_file",
            json!({"path": "src/power.c", "content": "fixed"}),
        )],
    ));
    messages.push(HarnessMessage::tool_response(vec![ToolCallResponse {
        id: "write".into(),
        name: "write_file".into(),
        output: "written".into(),
        is_error: false,
    }]));
    pad(&mut messages, 10);
    for _ in 0..3 {
        compact_context_messages(&mut messages, 10, None, 0.8);
        let summary = &messages[2];
        assert!(summary
            .content
            .contains("power_save setting is never consumed"));
        assert!(summary.content.contains("src/power.c"));
        assert!(summary.compacted_facts.is_some());
        // Replay serialization must preserve the accumulated memory too.
        messages = serde_json::from_value(serde_json::to_value(&messages).unwrap()).unwrap();
        pad(&mut messages, 10);
    }
}

#[test]
fn large_model_budget_keeps_a_hundred_small_messages() {
    let mut messages = history();
    pad(&mut messages, 100);
    compact_context_messages(&mut messages, 300, Some(128_000), 0.8);
    assert_eq!(messages.len(), 102);
    assert!(messages
        .iter()
        .all(|message| message.compacted_facts.is_none()));
}

#[test]
fn explicit_small_message_limit_is_honored() {
    let mut messages = history();
    pad(&mut messages, 40);
    compact_context_messages(&mut messages, 10, Some(128_000), 0.8);
    assert!(messages.len() <= 10);
    assert!(messages[2].content.contains("COMPACTED CONTEXT SUMMARY"));
}

#[test]
fn large_message_compaction_leaves_headroom_for_more_turns() {
    let mut messages = history();
    pad(&mut messages, 310);
    compact_context_messages(&mut messages, 300, None, 0.8);
    assert!(messages.len() < 40);
}

#[test]
fn facts_retain_tool_evidence_but_do_not_claim_failed_writes() {
    let messages = vec![
        HarnessMessage::assistant_with_tools(
            "",
            vec![
                ToolCallRequest::new("read", "grep_search", json!({"query": "power_save"})),
                ToolCallRequest::new(
                    "write",
                    "write_file",
                    json!({"path": "src/power.c", "content": "fixed"}),
                ),
            ],
        ),
        HarnessMessage::tool_response(vec![
            ToolCallResponse {
                id: "read".into(),
                name: "grep_search".into(),
                output: "power.c:12: cfg->power_save = enabled;".into(),
                is_error: false,
            },
            ToolCallResponse {
                id: "write".into(),
                name: "write_file".into(),
                output: "Permission denied".into(),
                is_error: true,
            },
        ]),
    ];
    let facts = extract_context_facts(&messages);
    assert!(facts
        .observations
        .iter()
        .any(|line| line.contains("cfg->power_save")));
    assert!(!facts.modified_files.contains("src/power.c"));
    assert!(facts
        .recent_errors
        .iter()
        .any(|line| line.contains("Permission denied")));
}

#[test]
fn fingerprints_distinguish_queries_and_ranges_and_ignore_object_key_order() {
    let first = ToolCallRequest::new(
        "1",
        "shell",
        json!({"command": "sed -n '1,20p' src/power.c"}),
    );
    let second = ToolCallRequest::new(
        "2",
        "shell",
        json!({"command": "sed -n '21,40p' src/power.c"}),
    );
    assert_ne!(
        RepetitiveCallDetector::fingerprint(&first),
        RepetitiveCallDetector::fingerprint(&second)
    );
    let mut first = ToolCallRequest::new(
        "1",
        "read_file",
        serde_json::from_str(r#"{"path":"a","range":{"start":1,"end":2}}"#).unwrap(),
    );
    let second = ToolCallRequest::new(
        "2",
        "read_file",
        serde_json::from_str(r#"{"range":{"end":2,"start":1},"path":"a"}"#).unwrap(),
    );
    assert_eq!(
        RepetitiveCallDetector::fingerprint(&first),
        RepetitiveCallDetector::fingerprint(&second)
    );
    first.arguments["range"]["start"] = json!(2);
    assert_ne!(
        RepetitiveCallDetector::fingerprint(&first),
        RepetitiveCallDetector::fingerprint(&second)
    );
}

#[test]
fn repeat_count_never_exceeds_the_window_and_can_be_reset() {
    let call = ToolCallRequest::new("1", "read_file", json!({"path": "a"}));
    let mut detector = RepetitiveCallDetector::new(3);
    for _ in 0..8 {
        assert!(detector.record_and_count(&call) <= 3);
    }
    detector.clear();
    assert_eq!(detector.record_and_count(&call), 1);
    assert_eq!(RepetitiveCallDetector::new(0).record_and_count(&call), 0);
}

struct ObservingPolicy {
    turns: usize,
    seen: Arc<Mutex<Vec<String>>>,
    write_on: Option<usize>,
    stop_on: usize,
}

#[async_trait]
impl AgentPolicy for ObservingPolicy {
    fn name(&self) -> &str {
        "context-observer"
    }
    async fn step(
        &mut self,
        context: &HarnessContextView,
        _: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        self.turns += 1;
        self.seen.lock().unwrap().extend(
            context
                .messages
                .iter()
                .map(|message| message.content.clone()),
        );
        if self.turns == self.stop_on {
            return Ok(AgentAction::FinalAnswer(
                "Root cause found; implementation remains.".into(),
            ));
        }
        let call = if self.write_on == Some(self.turns) {
            ToolCallRequest::new(
                format!("write-{}", self.turns),
                "write_file",
                json!({"path": ".agent/notes-plan-parent.md", "content": "Verified the setting is unused."}),
            )
        } else {
            ToolCallRequest::new(
                format!("read-{}", self.turns),
                "shell",
                json!({"command": "cat power.c"}),
            )
        };
        Ok(AgentAction::CallTools(vec![call]))
    }
    async fn step_detailed(
        &mut self,
        context: &HarnessContextView,
        tools: &[ToolDefinition],
    ) -> anyhow::Result<(AgentAction, Option<StepTelemetry>)> {
        Ok((
            self.step(context, tools).await?,
            Some(StepTelemetry {
                llm_response: Some(json!({"text": "The power_save setting is unused."})),
                ..Default::default()
            }),
        ))
    }
}

async fn run_observer(
    write_on: Option<usize>,
    enabled: bool,
    stop_on: usize,
) -> (openduck_harness::runtime::TaskExecutionResult, Vec<String>) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("power.c"), "int power_save;").unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let policy = ObservingPolicy {
        turns: 0,
        seen: seen.clone(),
        write_on,
        stop_on,
    };
    let mut harness = AgentHarness::new(policy, LocalSandbox::new(directory.path().into()))
        .with_circuit_breaker(enabled)
        .with_max_turns(20);
    let result = harness
        .run_task(&TaskSpec::new("parent", "test", "Diagnose power saving").with_turns(20))
        .await
        .unwrap();
    let messages = seen.lock().unwrap().clone();
    (result, messages)
}

#[tokio::test]
async fn runtime_warns_on_repetition_and_missing_notes_without_forcing_success() {
    let (result, messages) = run_observer(None, true, 99).await;
    assert_eq!(result.status, RunStatus::Cancelled);
    assert!(messages
        .iter()
        .any(|message| message.starts_with("[REPEATED TOOL CALLS]")));
    assert!(messages
        .iter()
        .any(|message| message.contains("[SCRATCHPAD REMINDER]")
            && message.contains(".agent/notes-plan-parent.md")));
    assert!(messages
        .iter()
        .any(|message| message.starts_with("[TURN BUDGET]")));
    assert!(messages
        .iter()
        .any(|message| message == "The power_save setting is unused."));
}

#[tokio::test]
async fn runtime_resets_repetitions_on_successful_notes_writes() {
    let (_, messages) = run_observer(Some(3), true, 7).await;
    assert!(!messages
        .iter()
        .any(|message| message.starts_with("[REPEATED TOOL CALLS]")));
}

#[tokio::test]
async fn runtime_respects_disabled_repetition_guard_and_updated_notes() {
    let (result, messages) = run_observer(Some(10), false, 16).await;
    assert_eq!(result.status, RunStatus::Success);
    assert!(!messages
        .iter()
        .any(|message| message.starts_with("[REPEATED TOOL CALLS]")));
    assert!(!messages
        .iter()
        .any(|message| message.starts_with("[SCRATCHPAD REMINDER]")));
}

#[tokio::test]
async fn subtask_reminder_uses_the_parent_task_identity() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("power.c"), "int power_save;").unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let policy = ObservingPolicy {
        turns: 0,
        seen: seen.clone(),
        write_on: None,
        stop_on: 99,
    };
    let mut harness = AgentHarness::new(policy, LocalSandbox::new(directory.path().into()));
    let task = TaskSpec::new("parent", "test", "Diagnose power saving").with_subtasks(vec![
        openduck_harness::SubtaskSpec::new("diagnose", "Diagnosis", "Find the root cause")
            .with_max_turns(16),
        openduck_harness::SubtaskSpec::new("fix", "Implementation", "Apply the fix"),
    ]);
    let result = harness.run_task(&task).await.unwrap();
    assert_eq!(result.status, RunStatus::Cancelled);
    let seen = seen.lock().unwrap();
    let reminder = seen
        .iter()
        .find(|message| message.starts_with("[SCRATCHPAD REMINDER]"))
        .unwrap();
    assert!(reminder.contains(".agent/notes-plan-parent.md"));
    assert!(!reminder.contains(".agent/notes-plan-diagnose.md"));
}

#[tokio::test]
async fn continuation_filters_harness_notices_and_keeps_visible_assistant_findings() {
    let (mut result, _) = run_observer(None, false, 2).await;
    result.trajectory.steps[0].llm_request = Some(json!({"messages": [
        {"role": "user", "content": "Diagnose power saving"},
        {"role": "user", "content": [{"type": "text", "text": "[HARNESS CONTEXT]\nOld summary"}]},
        {"role": "user", "content": "Refine diagnosis"},
    ]}));
    let messages = openduck_harness::runtime::prior_history_messages(
        &result.trajectory.steps,
        "Diagnose power saving",
    );
    let user_messages: Vec<_> = messages
        .iter()
        .filter(|message| message.role == openduck_harness::MessageRole::User)
        .collect();
    assert_eq!(user_messages.len(), 1);
    assert_eq!(user_messages[0].content, "Refine diagnosis");
    assert!(messages
        .iter()
        .any(|message| message.content == "The power_save setting is unused."));
}

#[test]
fn older_serialized_messages_without_compacted_facts_still_load() {
    let original =
        HarnessMessage::system("[COMPACTED CONTEXT SUMMARY]\nPreviously found an unused setting.");
    let mut serialized = serde_json::to_value(original).unwrap();
    serialized
        .as_object_mut()
        .unwrap()
        .remove("compacted_facts");
    let message: HarnessMessage = serde_json::from_value(serialized).unwrap();
    let facts = extract_context_facts(&[message]);
    assert!(facts
        .findings
        .iter()
        .any(|finding| finding.contains("Previously found an unused setting")));
}
