use openduck_harness::judge::{
    DecisionEngine, DecisionMode, LayaClient, LayaQuestion, LayaRequest, Verdict,
};
use openduck_harness::policy::AgentAction;
use openduck_harness::sandbox::local::LocalSandbox;
use openduck_harness::{AgentHarness, MockPolicy, ProjectHarnessConfig, TaskSpec};
use std::collections::HashMap;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_laya_client_live_noul_and_choice() {
    let client = LayaClient::new(
        "http://localhost:8732/api/laya",
        Duration::from_secs(2),
        true,
    );

    // 1. Test binary noul question (e.g. drift / stagnation)
    let noul_req = LayaRequest::single(
        "Agent has run `git status` 5 times with identical 0 changes output",
        "is_stagnant",
        LayaQuestion::noul("Has the agent been in an unproductive or stagnant loop?"),
    );

    let noul_resp = match client.predict(&noul_req).await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Laya predict error: {e:#}");
            return;
        }
    };
    assert!(noul_resp.answers.contains_key("is_stagnant"));
    let ans = noul_resp.answers.get("is_stagnant").unwrap();
    assert_eq!(ans.answer_type.as_deref(), Some("noul"));
    assert!(ans.noul.is_some());
    assert!(ans.confidence.is_some());

    // 2. Test categorical choice question (e.g. action routing or context triage)
    let mut criteria = HashMap::new();
    criteria.insert(
        "keep_active".to_string(),
        "Keep tool output in context".to_string(),
    );
    criteria.insert(
        "collapse_to_tombstone".to_string(),
        "Replace output with tombstone".to_string(),
    );
    let choice_req = LayaRequest::single(
        "Large compile log of 500 lines that completed with error: missing semicolon",
        "action_route",
        LayaQuestion::choice("How should this chunk be managed?", criteria),
    );

    let choice_resp = client
        .predict(&choice_req)
        .await
        .expect("Laya choice predict failed");
    assert!(choice_resp.answers.contains_key("action_route"));
    let choice_ans = choice_resp.answers.get("action_route").unwrap();
    assert_eq!(choice_ans.answer_type.as_deref(), Some("choice"));
    assert!(choice_ans.choice.is_some());
}

#[tokio::test]
async fn test_decision_engine_modes_and_ledger() {
    let client = LayaClient::new(
        "http://localhost:8732/api/laya",
        Duration::from_secs(1),
        true,
    );
    let is_live = client.is_available().await;

    let mut modes = HashMap::new();
    modes.insert("turn.drift".to_string(), DecisionMode::Shadow);
    modes.insert("turn.completion".to_string(), DecisionMode::Active);
    modes.insert("context.forget".to_string(), DecisionMode::Off);

    let engine = DecisionEngine::new(if is_live { Some(client) } else { None }, modes);

    // 1. turn.drift evaluation in Shadow mode
    let drift_record = engine
        .evaluate_drift("Agent ran cargo test 3 times with same failing assertion")
        .await;
    assert_eq!(drift_record.point, "turn.drift");
    assert_eq!(drift_record.mode, DecisionMode::Shadow);

    // 2. context.forget evaluation in Off mode (bypassed)
    let forget_record = engine.evaluate_context_forget("Some log chunk").await;
    assert_eq!(forget_record.point, "context.forget");
    assert_eq!(forget_record.mode, DecisionMode::Off);
    assert!(matches!(forget_record.verdict, Verdict::Bypassed { .. }));

    // 3. Ledger verification
    let ledger = engine.ledger();
    assert_eq!(ledger.len(), 2);
    assert_eq!(ledger.for_point("turn.drift").len(), 1);
    assert_eq!(ledger.for_point("context.forget").len(), 1);
}

#[tokio::test]
async fn test_harness_integration_captures_judgments() {
    let dir = tempdir().unwrap();
    let sandbox = LocalSandbox::new(dir.path().to_path_buf());

    let policy = MockPolicy::new(
        "mock",
        vec![
            AgentAction::CallTools(vec![openduck_harness::types::ToolCallRequest {
                id: "call_1".to_string(),
                name: "list_dir".to_string(),
                arguments: serde_json::json!({"path": "."}),
            }]),
            AgentAction::FinalAnswer("Completed all requested tasks".to_string()),
        ],
    );

    let mut harness = AgentHarness::new(policy, sandbox);

    // Configure judge with shadow mode for drift and completion
    let mut modes = HashMap::new();
    modes.insert("turn.drift".to_string(), DecisionMode::Shadow);
    modes.insert("turn.completion".to_string(), DecisionMode::Shadow);
    let engine = DecisionEngine::new(Some(LayaClient::default()), modes);
    harness = harness.with_judge_engine(engine);

    let task = TaskSpec::new(
        "test-laya-task",
        "test",
        "Inspect the repository and summarize",
    )
    .with_turns(5);

    let result = harness.run_task(&task).await.unwrap();
    assert_eq!(result.status, openduck_harness::types::RunStatus::Success);

    // Verify trajectory steps captured judgments
    let has_judgments = result
        .trajectory
        .steps
        .iter()
        .any(|step| step.judgments.is_some());
    assert!(
        has_judgments,
        "Expected at least one step to contain judge telemetry"
    );
}

#[test]
fn test_project_harness_config_judge_defaults() {
    let config = ProjectHarnessConfig::default();
    assert_eq!(config.judge.provider, "laya");
    assert_eq!(config.judge.endpoint, "http://localhost:8732/api/laya");
    assert_eq!(config.judge.timeout_ms, 350);
    assert!(config.judge.fallback_on_error);
    assert_eq!(
        config.judge.points.get("turn.drift").map(|s| s.as_str()),
        Some("shadow")
    );
    assert_eq!(
        config
            .judge
            .points
            .get("turn.completion")
            .map(|s| s.as_str()),
        Some("shadow")
    );
    assert_eq!(
        config
            .judge
            .points
            .get("context.forget")
            .map(|s| s.as_str()),
        Some("off")
    );
    assert_eq!(
        config.judge.points.get("tool.risk").map(|s| s.as_str()),
        Some("active")
    );
}
