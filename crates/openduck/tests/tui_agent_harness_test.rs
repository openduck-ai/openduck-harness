use anyhow::Result;
use async_trait::async_trait;
use openduck::control::terminal::get_tui_session_manager;
use openduck::control::tui_agent_extension::TuiAgentHarnessExtension;
use openduck_harness::eval::task::TaskSpec;
use openduck_harness::policy::adapters::MockPolicy;
use openduck_harness::policy::{AgentAction, AgentPolicy, HarnessContextView};
use openduck_harness::runtime::{AgentHarness, HarnessToolExtension};
use openduck_harness::sandbox::local::LocalSandbox;
use openduck_harness::types::{RunStatus, ToolCallRequest, ToolDefinition};
use serde_json::json;
use std::sync::Arc;

#[test]
fn test_tui_agent_extension_tool_definitions() {
    let extension = TuiAgentHarnessExtension::new(get_tui_session_manager());
    let tools = extension.tools();

    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

    assert!(tool_names.contains(&"tui_agent_start"));
    assert!(tool_names.contains(&"tui_agent_prompt"));
    assert!(tool_names.contains(&"tui_agent_read"));
    assert!(tool_names.contains(&"tui_agent_keys"));
    assert!(tool_names.contains(&"tui_agent_list"));
    assert!(tool_names.contains(&"tui_agent_stop"));
    assert!(tool_names.contains(&"tui_agent"));
    assert_eq!(tools.len(), 7);
}

#[tokio::test]
async fn test_harness_executes_tui_agent_list_tool() {
    let sandbox = LocalSandbox::ephemeral().expect("Failed to create ephemeral sandbox");

    let actions = vec![
        AgentAction::CallTools(vec![ToolCallRequest::new(
            "call_tui_list_1",
            "tui_agent_list",
            json!({}),
        )]),
        AgentAction::FinalAnswer("All done listing sessions".into()),
    ];

    let policy = MockPolicy::new("mock-tui-policy", actions);
    let extension = Arc::new(TuiAgentHarnessExtension::new(get_tui_session_manager()));

    let mut harness = AgentHarness::new(policy, sandbox)
        .with_max_turns(5)
        .with_tool_extension(extension);

    let task = TaskSpec::new(
        "test-tui-agent-list",
        "tui-suite",
        "Check running TUI sessions",
    );

    let result = harness.run_task(&task).await.expect("Harness run failed");
    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(result.trajectory.steps.len(), 2);

    let step0_results = result.trajectory.steps[0]
        .tool_results
        .as_ref()
        .expect("missing tool results");
    assert_eq!(step0_results.len(), 1);
    assert_eq!(step0_results[0].name, "tui_agent_list");
    assert!(!step0_results[0].is_error);
    assert!(
        step0_results[0]
            .output
            .contains("active TUI agent sessions")
            || step0_results[0].output.contains("ID:")
    );
}

#[tokio::test]
async fn test_harness_handles_tui_agent_start_invalid_agent() {
    let sandbox = LocalSandbox::ephemeral().expect("Failed to create ephemeral sandbox");

    let actions = vec![
        AgentAction::CallTools(vec![ToolCallRequest::new(
            "call_tui_start_invalid",
            "tui_agent_start",
            json!({
                "agent": "nonexistent_cli_agent_xyz"
            }),
        )]),
        AgentAction::FinalAnswer("Done".into()),
    ];

    let policy = MockPolicy::new("mock-tui-invalid", actions);
    let extension = Arc::new(TuiAgentHarnessExtension::new(get_tui_session_manager()));

    let mut harness = AgentHarness::new(policy, sandbox)
        .with_max_turns(5)
        .with_tool_extension(extension);

    let task = TaskSpec::new("test-tui-invalid-agent", "tui-suite", "Start invalid agent");

    let result = harness.run_task(&task).await.expect("Harness run failed");
    assert_eq!(result.status, RunStatus::Success);

    let step0_results = result.trajectory.steps[0]
        .tool_results
        .as_ref()
        .expect("missing tool results");
    assert_eq!(step0_results.len(), 1);
    assert!(step0_results[0].is_error);
    assert!(step0_results[0].output.contains("Unsupported agent"));
}

#[tokio::test]
async fn test_harness_executes_tui_agent_stop_nonexistent_session() {
    let sandbox = LocalSandbox::ephemeral().expect("Failed to create ephemeral sandbox");

    let actions = vec![
        AgentAction::CallTools(vec![ToolCallRequest::new(
            "call_tui_stop_1",
            "tui_agent_stop",
            json!({
                "session_id": "nonexistent_sess_99999"
            }),
        )]),
        AgentAction::FinalAnswer("Stopped".into()),
    ];

    let policy = MockPolicy::new("mock-tui-stop", actions);
    let extension = Arc::new(TuiAgentHarnessExtension::new(get_tui_session_manager()));

    let mut harness = AgentHarness::new(policy, sandbox)
        .with_max_turns(5)
        .with_tool_extension(extension);

    let task = TaskSpec::new("test-tui-stop", "tui-suite", "Stop session");

    let result = harness.run_task(&task).await.expect("Harness run failed");
    assert_eq!(result.status, RunStatus::Success);

    let step0_results = result.trajectory.steps[0]
        .tool_results
        .as_ref()
        .expect("missing tool results");
    assert_eq!(step0_results.len(), 1);
    assert!(step0_results[0].is_error);
    assert!(step0_results[0].output.contains("not found"));
}

#[tokio::test]
async fn test_harness_executes_convenience_tui_agent_missing_params() {
    let sandbox = LocalSandbox::ephemeral().expect("Failed to create ephemeral sandbox");

    let actions = vec![
        AgentAction::CallTools(vec![ToolCallRequest::new(
            "call_tui_conv_1",
            "tui_agent",
            json!({
                "agent": "codex"
                // Missing required "prompt"
            }),
        )]),
        AgentAction::FinalAnswer("Handled".into()),
    ];

    let policy = MockPolicy::new("mock-tui-conv", actions);
    let extension = Arc::new(TuiAgentHarnessExtension::new(get_tui_session_manager()));

    let mut harness = AgentHarness::new(policy, sandbox)
        .with_max_turns(5)
        .with_tool_extension(extension);

    let task = TaskSpec::new("test-tui-convenience", "tui-suite", "Call convenience tool");

    let result = harness.run_task(&task).await.expect("Harness run failed");
    assert_eq!(result.status, RunStatus::Success);

    let step0_results = result.trajectory.steps[0]
        .tool_results
        .as_ref()
        .expect("missing tool results");
    assert_eq!(step0_results.len(), 1);
    assert!(step0_results[0].is_error);
    assert!(step0_results[0].output.contains("prompt"));
}

/// A policy that dynamically parses the user prompt and routes to TUI agent when requested by prompt.
struct PromptDrivenPolicy {
    turned: bool,
}

impl PromptDrivenPolicy {
    fn new() -> Self {
        Self { turned: false }
    }
}

#[async_trait]
impl AgentPolicy for PromptDrivenPolicy {
    fn name(&self) -> &str {
        "prompt-driven-policy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        tools: &[ToolDefinition],
    ) -> Result<AgentAction> {
        if self.turned {
            return Ok(AgentAction::FinalAnswer(
                "Completed work via delegated agent.".into(),
            ));
        }
        self.turned = true;

        // Inspect the task prompt from user context
        let user_prompt = context
            .messages
            .first()
            .map(|m| m.content.as_str())
            .unwrap_or_default();

        // Verify that the extension tools (tui_agent, tui_agent_start, etc.) are available
        let has_tui_agent_tool = tools.iter().any(|t| t.name == "tui_agent");
        assert!(
            has_tui_agent_tool,
            "Harness must expose 'tui_agent' tool to policy"
        );

        if user_prompt.contains("TUI agent codex") || user_prompt.contains("use codex") {
            // As instructed by prompt: dispatch to TUI agent codex
            Ok(AgentAction::CallTools(vec![ToolCallRequest::new(
                "call-tui-prompt-1",
                "tui_agent",
                json!({
                    "agent": "codex",
                    "prompt": "Design the firmware replication module for beiwan."
                }),
            )]))
        } else if user_prompt.contains("consult_advisor") || user_prompt.contains("advisor grok") {
            Ok(AgentAction::CallTools(vec![ToolCallRequest::new(
                "call-advisor-prompt-1",
                "consult_advisor",
                json!({
                    "advisor": "grok",
                    "prompt": "General architectural advice."
                }),
            )]))
        } else {
            Ok(AgentAction::FinalAnswer(
                "No agent requested in prompt.".into(),
            ))
        }
    }
}

#[tokio::test]
async fn test_harness_uses_tui_agent_by_prompt_instruction() {
    let sandbox = LocalSandbox::ephemeral().expect("Failed to create ephemeral sandbox");
    let extension = Arc::new(TuiAgentHarnessExtension::new(get_tui_session_manager()));

    let policy = PromptDrivenPolicy::new();
    let mut harness = AgentHarness::new(policy, sandbox)
        .with_max_turns(5)
        .with_tool_extension(extension);

    // This replicates the exact prompt pattern from task-detailed-design-firmware-replication-beiwan:
    // "由 TUI agent codex 负责详细设计... 不要用 consult_advisor，而要用 TUI Agent"
    let task = TaskSpec::new(
        "task-detailed-design-firmware-replication",
        "firmware-design",
        "由 TUI agent codex 负责详细设计固件复刻逻辑，不要用 consult_advisor，而要用 TUI Agent 进行设计协作。",
    );

    let result = harness
        .run_task(&task)
        .await
        .expect("Harness execution failed");
    assert_eq!(result.status, RunStatus::Success);

    // Verify step 0 called tui_agent as directed by the prompt
    let step0_results = result.trajectory.steps[0]
        .tool_results
        .as_ref()
        .expect("missing tool results for step 0");
    assert_eq!(step0_results.len(), 1);
    assert_eq!(step0_results[0].name, "tui_agent");
    assert_eq!(step0_results[0].id, "call-tui-prompt-1");

    // Trajectory reflects completion
    assert_eq!(result.trajectory.steps.len(), 2);
}

#[tokio::test]
async fn test_harness_prompt_dispatches_between_advisor_and_tui_agent() {
    let extension = Arc::new(TuiAgentHarnessExtension::new(get_tui_session_manager()));

    // Run 1: Prompt specifies advisor grok
    {
        let sandbox = LocalSandbox::ephemeral().expect("Failed to create sandbox");
        let policy = PromptDrivenPolicy::new();
        let mut harness = AgentHarness::new(policy, sandbox)
            .with_max_turns(5)
            .with_tool_extension(extension.clone());

        let task = TaskSpec::new(
            "task-advisor-prompt",
            "prompt-suite",
            "Please use advisor grok for high level review.",
        );
        let result = harness.run_task(&task).await.expect("Run failed");
        let step0 = result.trajectory.steps[0].tool_results.as_ref().unwrap();
        assert_eq!(step0[0].name, "consult_advisor");
    }

    // Run 2: Prompt specifies TUI agent codex
    {
        let sandbox = LocalSandbox::ephemeral().expect("Failed to create sandbox");
        let policy = PromptDrivenPolicy::new();
        let mut harness = AgentHarness::new(policy, sandbox)
            .with_max_turns(5)
            .with_tool_extension(extension.clone());

        let task = TaskSpec::new(
            "task-tui-prompt",
            "prompt-suite",
            "Please use TUI agent codex to write code.",
        );
        let result = harness.run_task(&task).await.expect("Run failed");
        let step0 = result.trajectory.steps[0].tool_results.as_ref().unwrap();
        assert_eq!(step0[0].name, "tui_agent");
    }
}

/// Tests multi-turn steering where the prompt specifies step-by-step TUI agent interaction
#[tokio::test]
async fn test_harness_prompt_multi_turn_tui_steering() {
    let sandbox = LocalSandbox::ephemeral().expect("Failed to create sandbox");
    let extension = Arc::new(TuiAgentHarnessExtension::new(get_tui_session_manager()));

    // Scripted multi-turn flow driven by user prompt:
    // 1. Check sessions (tui_agent_list)
    // 2. Start session (tui_agent_start)
    // 3. Stop session (tui_agent_stop)
    let actions = vec![
        AgentAction::CallTools(vec![ToolCallRequest::new(
            "step1_list",
            "tui_agent_list",
            json!({}),
        )]),
        AgentAction::CallTools(vec![ToolCallRequest::new(
            "step2_stop",
            "tui_agent_stop",
            json!({
                "session_id": "nonexistent_mock_session_1"
            }),
        )]),
        AgentAction::FinalAnswer("Steering workflow complete.".into()),
    ];

    let policy = MockPolicy::new("multi-turn-prompt-policy", actions);
    let mut harness = AgentHarness::new(policy, sandbox)
        .with_max_turns(5)
        .with_tool_extension(extension);

    let task = TaskSpec::new(
        "task-multi-turn-steering",
        "steering-suite",
        "Orchestrate TUI agents: list sessions, then cleanly stop worker session.",
    );

    let result = harness.run_task(&task).await.expect("Execution failed");
    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(result.trajectory.steps.len(), 3);

    let step0 = result.trajectory.steps[0].tool_results.as_ref().unwrap();
    assert_eq!(step0[0].name, "tui_agent_list");

    let step1 = result.trajectory.steps[1].tool_results.as_ref().unwrap();
    assert_eq!(step1[0].name, "tui_agent_stop");
}
