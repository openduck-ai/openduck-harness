use anyhow::{anyhow, Result};
use async_trait::async_trait;
use openduck_harness::eval::task::{CommandVerifier, DiffVerifier, TaskSpec};
use openduck_harness::eval::EvalRunnerConfig;
use openduck_harness::policy::adapters::MockPolicy;
use openduck_harness::policy::{AgentAction, AgentPolicy, HarnessContextView};
use openduck_harness::replay::{Cassette, ReplayPolicy};
use openduck_harness::runtime::AgentHarness;
use openduck_harness::sandbox::local::LocalSandbox;
use openduck_harness::sandbox::SandboxDriver;
use openduck_harness::types::{ExecOptions, RunStatus, ToolCallRequest, ToolDefinition};
use openduck_harness::{BenchmarkItem, EvalRunner};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

struct FailingPolicy;

#[async_trait]
impl AgentPolicy for FailingPolicy {
    fn name(&self) -> &str {
        "failing-policy"
    }

    async fn step(
        &mut self,
        _context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> Result<AgentAction> {
        Err(anyhow!(
            "Goose provider complete failed: Request failed: 401"
        ))
    }
}

#[tokio::test]
async fn test_local_sandbox_lifecycle() {
    let mut sandbox = LocalSandbox::ephemeral().expect("Failed to create ephemeral sandbox");
    sandbox.initialize().await.expect("Failed to initialize");

    let test_file = PathBuf::from("hello.txt");
    sandbox
        .write_file(&test_file, b"Hello Harness!")
        .await
        .expect("write_file failed");

    let read_bytes = sandbox
        .read_file(&test_file)
        .await
        .expect("read_file failed");
    assert_eq!(read_bytes, b"Hello Harness!");

    let snap = sandbox.snapshot("v1").await.expect("snapshot failed");

    sandbox
        .write_file(&test_file, b"Modified Content")
        .await
        .expect("write_file modified failed");
    assert_eq!(
        sandbox.read_file(&test_file).await.unwrap(),
        b"Modified Content"
    );

    sandbox
        .restore_snapshot(&snap)
        .await
        .expect("restore_snapshot failed");
    assert_eq!(
        sandbox.read_file(&test_file).await.unwrap(),
        b"Hello Harness!"
    );

    let opts = ExecOptions::default();
    let out = sandbox
        .exec_command("echo test_echo", &opts)
        .await
        .expect("exec_command failed");
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("test_echo"));

    sandbox.cleanup().await.expect("cleanup failed");
}

#[tokio::test]
async fn test_policy_step_failure_preserves_cause_in_error() {
    let sandbox = LocalSandbox::ephemeral().unwrap();
    let mut harness = AgentHarness::new(FailingPolicy, sandbox);
    let task = TaskSpec::new("fail-1", "smoke-test", "This should fail");

    let result = harness
        .run_task(&task)
        .await
        .expect("provider failures should complete as Failure, not abort the run");
    assert_eq!(result.status, RunStatus::Failure);
    assert!(result.step_count >= 1, "failed step should be recorded");
    let rendered = result
        .final_answer
        .expect("failure should record the error");
    assert!(
        rendered.contains("Policy step 1 failed"),
        "outer context missing: {rendered}"
    );
    assert!(
        rendered.contains("Goose provider complete failed: Request failed: 401"),
        "provider cause missing: {rendered}"
    );
}

#[tokio::test]
async fn test_policy_step_failure_after_success_keeps_prior_steps() {
    struct FirstToolThenFail {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl AgentPolicy for FirstToolThenFail {
        fn name(&self) -> &str {
            "first-tool-then-fail"
        }

        async fn step(
            &mut self,
            _context: &HarnessContextView,
            _tools: &[ToolDefinition],
        ) -> Result<AgentAction> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                return Ok(AgentAction::CallTools(vec![ToolCallRequest {
                    id: "call-1".into(),
                    name: "write_file".into(),
                    arguments: serde_json::json!({
                        "path": "notes.txt",
                        "content": "partial progress"
                    }),
                }]));
            }
            Err(anyhow!("Goose provider complete failed: Network error"))
        }
    }

    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed_clone = observed.clone();
    let sandbox = LocalSandbox::ephemeral().unwrap();
    let mut harness = AgentHarness::new(
        FirstToolThenFail {
            calls: AtomicUsize::new(0),
        },
        sandbox,
    )
    .with_step_observer(Arc::new(move |steps| {
        observed_clone.lock().unwrap().push(steps.len());
    }));
    let task = TaskSpec::new("fail-2", "smoke-test", "Partial then fail").with_turns(4);

    let result = harness
        .run_task(&task)
        .await
        .expect("should return Failure");
    assert_eq!(result.status, RunStatus::Failure);
    assert!(result.step_count >= 2);
    let last_observed = observed.lock().unwrap().last().copied().unwrap_or(0);
    assert!(last_observed >= 2, "observer should see the failed step");
}

#[tokio::test]
async fn test_agent_harness_task_execution() {
    let actions = vec![
        AgentAction::CallTools(vec![ToolCallRequest {
            id: "call-1".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({
                "path": "output.txt",
                "content": "42"
            }),
        }]),
        AgentAction::FinalAnswer("Task completed successfully".into()),
    ];

    let policy = MockPolicy::new("test-mock", actions);
    let sandbox = LocalSandbox::ephemeral().unwrap();
    let mut harness = AgentHarness::new(policy, sandbox);

    let task = TaskSpec::new("test-1", "smoke-test", "Write 42 to output.txt");
    let result = harness.run_task(&task).await.expect("run_task failed");

    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(result.tool_calls_count, 1);
    assert_eq!(
        result.final_answer.as_deref(),
        Some("Task completed successfully")
    );

    let read_back = harness
        .sandbox()
        .read_file(&PathBuf::from("output.txt"))
        .await
        .expect("read output.txt failed");
    assert_eq!(read_back, b"42");
}

#[tokio::test]
async fn test_step_observer_is_notified_after_each_step() {
    let actions = vec![
        AgentAction::CallTools(vec![ToolCallRequest {
            id: "call-1".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({
                "path": "output.txt",
                "content": "42"
            }),
        }]),
        AgentAction::FinalAnswer("done".into()),
    ];
    let policy = MockPolicy::new("obs-mock", actions);
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_cb = seen.clone();
    let sandbox = LocalSandbox::ephemeral().unwrap();
    let mut harness =
        AgentHarness::new(policy, sandbox).with_step_observer(Arc::new(move |steps| {
            seen_cb.lock().unwrap().push(steps.len());
        }));

    let task = TaskSpec::new("obs-1", "smoke-test", "Write 42 to output.txt");
    let result = harness.run_task(&task).await.expect("run_task failed");

    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
    assert_eq!(result.trajectory.steps.len(), 2);
}

#[tokio::test]
async fn test_deterministic_replay_cassette() {
    let cassette_storage = Arc::new(Mutex::new(Cassette::new("test-replay-cassette")));

    let actions = vec![
        AgentAction::CallTools(vec![ToolCallRequest {
            id: "call-1".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({
                "path": "res.txt",
                "content": "recorded"
            }),
        }]),
        AgentAction::FinalAnswer("Done".into()),
    ];

    let base_policy = MockPolicy::new("base-policy", actions);
    let record_policy = ReplayPolicy::new_record(base_policy, cassette_storage.clone());

    let sandbox1 = LocalSandbox::ephemeral().unwrap();
    let mut harness1 = AgentHarness::new(record_policy, sandbox1);
    let task = TaskSpec::new("replay-task-1", "replay-suite", "Record this");

    let res1 = harness1.run_task(&task).await.expect("Record run failed");
    assert_eq!(res1.status, RunStatus::Success);

    let replay_policy: ReplayPolicy<MockPolicy> =
        ReplayPolicy::new_replay(cassette_storage.clone());
    let sandbox2 = LocalSandbox::ephemeral().unwrap();
    let mut harness2 = AgentHarness::new(replay_policy, sandbox2);

    let res2 = harness2.run_task(&task).await.expect("Replay run failed");
    assert_eq!(res2.status, RunStatus::Success);
    assert_eq!(res2.final_answer, res1.final_answer);
}

#[tokio::test]
async fn test_eval_runner_suite() {
    let temp_out = tempfile::tempdir().unwrap();
    let config = EvalRunnerConfig {
        concurrency: 2,
        output_dir: temp_out.path().to_path_buf(),
        max_turns: 10,
    };

    let runner = EvalRunner::new(config);

    let items = vec![
        BenchmarkItem {
            task: TaskSpec::new("eval-1", "suite-1", "Create file foo.txt with bar"),
            verifier: Arc::new(DiffVerifier::new(PathBuf::from("foo.txt"), "bar")),
        },
        BenchmarkItem {
            task: TaskSpec::new("eval-2", "suite-1", "Exit with 0"),
            verifier: Arc::new(CommandVerifier::new("true")),
        },
    ];

    let report = runner
        .run_suite("smoke-suite", items, || {
            MockPolicy::new(
                "solver-policy",
                vec![
                    AgentAction::CallTools(vec![ToolCallRequest {
                        id: "call-1".into(),
                        name: "write_file".into(),
                        arguments: serde_json::json!({
                            "path": "foo.txt",
                            "content": "bar"
                        }),
                    }]),
                    AgentAction::FinalAnswer("All done".into()),
                ],
            )
        })
        .await
        .expect("run_suite failed");

    assert_eq!(report.metrics.total_tasks, 2);
    assert_eq!(report.metrics.passed_tasks, 2);
    assert_eq!(report.metrics.failed_tasks, 0);
}
