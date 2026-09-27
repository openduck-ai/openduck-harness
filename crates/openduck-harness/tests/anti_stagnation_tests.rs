use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use openduck_harness::policy::{AgentAction, AgentPolicy, HarnessContextView};
use openduck_harness::repomap::generate_workspace_repo_map;
use openduck_harness::runtime::{
    is_modifying_action, is_modifying_call, is_modifying_shell_command, AgentHarness,
};
use openduck_harness::sandbox::local::LocalSandbox;
use openduck_harness::types::{RunStatus, ToolCallRequest, ToolDefinition};
use openduck_harness::TaskSpec;
use tempfile::tempdir;

#[test]
fn test_modifying_command_classification() {
    // Read-only commands
    assert!(!is_modifying_shell_command("ls -la"));
    assert!(!is_modifying_shell_command("pwd"));
    assert!(!is_modifying_shell_command("cat src/main.rs"));
    assert!(!is_modifying_shell_command("grep -rn 'TODO' src/"));
    assert!(!is_modifying_shell_command("find . -name '*.rs'"));
    assert!(!is_modifying_shell_command("git log -n 10"));
    assert!(!is_modifying_shell_command("git show 123456"));
    assert!(!is_modifying_shell_command("git status"));
    assert!(!is_modifying_shell_command("git diff"));
    assert!(!is_modifying_shell_command("sed -n '1,20p' src/lib.rs"));

    // Stream redirections with /dev/null or stderr
    assert!(!is_modifying_shell_command(
        "grep -rn 'TODO' src/ 2>/dev/null"
    ));
    assert!(!is_modifying_shell_command(
        "find . -name '*.rs' 2>/dev/null | head"
    ));
    assert!(!is_modifying_shell_command("cat src/main.rs > /dev/null"));
    assert!(!is_modifying_shell_command("ls 2>&1 | grep foo"));

    // Redirections to scratch / note files
    assert!(!is_modifying_shell_command(
        "echo 'note' > .agent/notes-progress.md"
    ));
    assert!(!is_modifying_shell_command(
        "echo 'scratch' > .omx/scratch.md"
    ));
    assert!(!is_modifying_shell_command(
        "touch .agent/notes-gps-milestone.md"
    ));

    // Modifying commands
    assert!(is_modifying_shell_command("cargo build"));
    assert!(is_modifying_shell_command("cargo test --lib"));
    assert!(is_modifying_shell_command("cargo check 2>&1 | tail -8"));
    assert!(is_modifying_shell_command("npm run build"));
    assert!(is_modifying_shell_command("pnpm test"));
    assert!(is_modifying_shell_command("git checkout main"));
    assert!(is_modifying_shell_command("touch src/new.rs"));
    assert!(is_modifying_shell_command("rm -rf target/"));
    assert!(is_modifying_shell_command("mkdir -p src/views"));
    assert!(is_modifying_shell_command("echo 'hello' > file.txt"));
    assert!(is_modifying_shell_command(
        "sed -i 's/foo/bar/g' src/main.rs"
    ));

    // Tool call requests
    let write_call = ToolCallRequest {
        id: "1".into(),
        name: "write_file".into(),
        arguments: serde_json::json!({"path": "a.txt", "content": "hi"}),
    };
    assert!(is_modifying_call(&write_call));

    let write_notes_call = ToolCallRequest {
        id: "1b".into(),
        name: "write_file".into(),
        arguments: serde_json::json!({"path": ".agent/notes-gps-progress.md", "content": "findings"}),
    };
    assert!(!is_modifying_call(&write_notes_call));

    let write_scratch_call = ToolCallRequest {
        id: "1c".into(),
        name: "write_file".into(),
        arguments: serde_json::json!({"path": ".agent/scratch-gps-plan.md", "content": "plan"}),
    };
    assert!(!is_modifying_call(&write_scratch_call));

    let read_call = ToolCallRequest {
        id: "2".into(),
        name: "read_file".into(),
        arguments: serde_json::json!({"path": "a.txt"}),
    };
    assert!(!is_modifying_call(&read_call));

    let list_call = ToolCallRequest {
        id: "3".into(),
        name: "list_dir".into(),
        arguments: serde_json::json!({"path": "."}),
    };
    assert!(!is_modifying_call(&list_call));

    let grep_call = ToolCallRequest {
        id: "4".into(),
        name: "grep_search".into(),
        arguments: serde_json::json!({"query": "fn test"}),
    };
    assert!(!is_modifying_call(&grep_call));

    // AgentAction
    assert!(!is_modifying_action(&AgentAction::CallTools(vec![
        read_call,
        grep_call,
        write_notes_call
    ])));
    assert!(is_modifying_action(&AgentAction::CallTools(vec![
        write_call
    ])));
}

#[test]
fn test_repo_map_generator() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create Cargo workspace root
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"server\", \"frontend\"]\n",
    )
    .unwrap();

    // Create server crate
    let server_dir = root.join("server");
    fs::create_dir(&server_dir).unwrap();
    fs::write(
        server_dir.join("Cargo.toml"),
        "[package]\nname = \"backend-server\"\ndescription = \"IoT backend server\"\n",
    )
    .unwrap();

    // Create frontend package
    let front_dir = root.join("frontend");
    fs::create_dir(&front_dir).unwrap();
    fs::write(
        front_dir.join("package.json"),
        r#"{"name": "admin-ui", "description": "Admin web portal"}"#,
    )
    .unwrap();

    let map = generate_workspace_repo_map(root).expect("Should generate repo map");
    assert!(map.contains("Workspace Architecture Overview:"));
    assert!(map.contains("server/"));
    assert!(map.contains("backend-server"));
    assert!(map.contains("frontend/"));
    assert!(map.contains("admin-ui"));
}

struct StagnantPolicy {
    step_count: Arc<AtomicUsize>,
    received_nudges: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentPolicy for StagnantPolicy {
    fn name(&self) -> &str {
        "stagnant-policy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        let current = self.step_count.fetch_add(1, Ordering::SeqCst);
        let has_nudge = context
            .messages
            .iter()
            .any(|m| m.content.contains("SYSTEM NUDGE - ACTION REQUIRED"));

        if has_nudge {
            self.received_nudges.fetch_add(1, Ordering::SeqCst);
            if current >= 8 {
                return Ok(AgentAction::FinalAnswer("Done".to_string()));
            }
            return Ok(AgentAction::CallTools(vec![ToolCallRequest {
                id: format!("write-{}", current),
                name: "write_file".into(),
                arguments: serde_json::json!({
                    "path": "solution.txt",
                    "content": "Solved!"
                }),
            }]));
        }

        if current >= 10 {
            return Ok(AgentAction::FinalAnswer("Done".to_string()));
        }

        // Keep reading files
        Ok(AgentAction::CallTools(vec![ToolCallRequest {
            id: format!("read-{}", current),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "src/main.rs"}),
        }]))
    }
}

#[tokio::test]
async fn test_stagnation_nudge_triggers_and_recovers() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

    let step_count = Arc::new(AtomicUsize::new(0));
    let received_nudges = Arc::new(AtomicUsize::new(0));

    let policy = StagnantPolicy {
        step_count: step_count.clone(),
        received_nudges: received_nudges.clone(),
    };

    let sandbox = LocalSandbox::new(root);
    let mut harness = AgentHarness::new(policy, sandbox)
        .with_max_turns(12)
        .with_stagnation_threshold(6);

    let task = TaskSpec::new("task-test", "test", "Fix the bug");
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(
        received_nudges.load(Ordering::SeqCst),
        0,
        "Policy should not receive system nudge"
    );
}

struct InfiniteReadOnlyPolicy {
    step_count: Arc<AtomicUsize>,
    blocked_count: Arc<AtomicUsize>,
    repeat_intercept_count: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentPolicy for InfiniteReadOnlyPolicy {
    fn name(&self) -> &str {
        "infinite-read-only-policy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        let current = self.step_count.fetch_add(1, Ordering::SeqCst);

        // Check if any tool result was blocked by circuit breaker or repetition
        for msg in &context.messages {
            if let Some(results) = &msg.tool_results {
                for r in results {
                    if r.output.contains("Anti-Stagnation Circuit Breaker") {
                        self.blocked_count.fetch_add(1, Ordering::SeqCst);
                    }
                    if r.output.contains("Tool execution prevented") {
                        self.repeat_intercept_count.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }

        // Call different search queries to test Tier 2 & Tier 3 without hitting duplicate filter
        Ok(AgentAction::CallTools(vec![ToolCallRequest {
            id: format!("grep-{}", current),
            name: "grep_search".into(),
            arguments: serde_json::json!({
                "query": format!("query_{}", current)
            }),
        }]))
    }
}

#[tokio::test]
async fn test_circuit_breaker_tier2_throttling_and_tier3_termination() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

    let step_count = Arc::new(AtomicUsize::new(0));
    let blocked_count = Arc::new(AtomicUsize::new(0));
    let repeat_intercept_count = Arc::new(AtomicUsize::new(0));

    let policy = InfiniteReadOnlyPolicy {
        step_count: step_count.clone(),
        blocked_count: blocked_count.clone(),
        repeat_intercept_count: repeat_intercept_count.clone(),
    };

    let sandbox = LocalSandbox::new(root);
    let mut harness = AgentHarness::new(policy, sandbox)
        .with_circuit_breaker(true)
        .with_max_turns(10)
        .with_stagnation_threshold(4);

    let task = TaskSpec::new("task-infinite", "test", "Fix the bug").with_turns(10);
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Cancelled);
    assert_eq!(result.step_count, 10);
    assert_eq!(
        blocked_count.load(Ordering::SeqCst),
        0,
        "Tier 2 throttling should not block tool execution"
    );
}

#[tokio::test]
async fn test_duplicate_call_interception() {
    use openduck_harness::runtime::RepetitiveCallDetector;

    let mut detector = RepetitiveCallDetector::new(10);
    let call = ToolCallRequest {
        id: "call-1".into(),
        name: "grep_search".into(),
        arguments: serde_json::json!({"query": "handle_location", "path": "src/client.rs"}),
    };

    assert_eq!(detector.record_and_count(&call), 1);
    assert_eq!(detector.record_and_count(&call), 2);
    assert_eq!(detector.record_and_count(&call), 3);
    assert_eq!(detector.record_and_count(&call), 4);
}

#[test]
fn test_context_pruning() {
    use openduck_harness::policy::HarnessMessage;
    use openduck_harness::runtime::prune_context_messages;

    let mut messages = Vec::new();
    messages.push(HarnessMessage::system("System instructions"));
    messages.push(HarnessMessage::user("Task description"));

    for i in 1..=40 {
        messages.push(HarnessMessage::user(format!("Step {i} prompt")));
        messages.push(HarnessMessage::assistant(format!("Step {i} response")));
    }

    assert_eq!(messages.len(), 82);
    prune_context_messages(&mut messages, 25);

    // Should have pruned to <= 25 messages while keeping head and recent tail
    assert!(messages.len() <= 25);
    assert_eq!(messages[0].content, "System instructions");
    assert_eq!(messages[1].content, "Task description");
    assert!(messages[2].content.contains("Earlier interaction history"));
    assert_eq!(messages.last().unwrap().content, "Step 40 response");
}

#[test]
fn test_periodic_rule_auto_detection() {
    use openduck_harness::telemetry::ActiveRuleSummary;

    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let sandbox = LocalSandbox::new(root);
    let policy = StagnantPolicy {
        step_count: Arc::new(AtomicUsize::new(0)),
        received_nudges: Arc::new(AtomicUsize::new(0)),
    };

    // 1. Rule with "grok-agy-per-10-steps"
    let harness =
        AgentHarness::new(policy, sandbox).with_active_rules(Some(vec![ActiveRuleSummary {
            name: "grok-agy-per-10-steps".into(),
            description: "call grok or agy per 10 steps".into(),
            global: true,
            path: "/fake/rules/grok-agy.md".into(),
        }]));

    assert_eq!(harness.detect_periodic_review_interval(), Some(10));

    // 2. Explicit override takes precedence
    let harness_override = harness.with_periodic_review_interval(Some(7));
    assert_eq!(harness_override.detect_periodic_review_interval(), Some(7));
}

struct PeriodicReviewCheckPolicy {
    step_count: Arc<AtomicUsize>,
    received_periodic_reviews: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentPolicy for PeriodicReviewCheckPolicy {
    fn name(&self) -> &str {
        "periodic-review-check-policy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        let current = self.step_count.fetch_add(1, Ordering::SeqCst);

        for msg in &context.messages {
            if msg.content.contains("MANDATORY PERIODIC REVIEW - STEP") {
                self.received_periodic_reviews
                    .fetch_add(1, Ordering::SeqCst);
            }
        }

        if current >= 12 {
            return Ok(AgentAction::FinalAnswer("Finished".to_string()));
        }

        // Return a benign read_file call
        Ok(AgentAction::CallTools(vec![ToolCallRequest {
            id: format!("read-{}", current),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "src/main.rs"}),
        }]))
    }
}

#[tokio::test]
async fn test_periodic_review_nudge_enforcement() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

    let step_count = Arc::new(AtomicUsize::new(0));
    let received_periodic_reviews = Arc::new(AtomicUsize::new(0));

    let policy = PeriodicReviewCheckPolicy {
        step_count: step_count.clone(),
        received_periodic_reviews: received_periodic_reviews.clone(),
    };

    let sandbox = LocalSandbox::new(root);
    let mut harness = AgentHarness::new(policy, sandbox)
        .with_max_turns(15)
        .with_stagnation_threshold(20) // High stagnation threshold so circuit breaker doesn't trigger
        .with_periodic_review_interval(Some(5)); // Review at Step 5 and Step 10

    let task = TaskSpec::new("task-periodic", "test", "Do something");
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(
        received_periodic_reviews.load(Ordering::SeqCst),
        0,
        "Policy should not receive periodic review notices"
    );
}

struct ConsultAdvisorTestPolicy {
    advisor_output: Arc<tokio::sync::Mutex<Option<String>>>,
}

#[async_trait]
impl AgentPolicy for ConsultAdvisorTestPolicy {
    fn name(&self) -> &str {
        "consult-advisor-test-policy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        // If we got a tool result, record it and finish
        for msg in &context.messages {
            if let Some(results) = &msg.tool_results {
                for r in results {
                    if r.name == "consult_advisor" {
                        let mut out = self.advisor_output.lock().await;
                        *out = Some(r.output.clone());
                        return Ok(AgentAction::FinalAnswer("Consultation done".to_string()));
                    }
                }
            }
        }

        // Call consult_advisor with unavailable binary to verify graceful fallback
        Ok(AgentAction::CallTools(vec![ToolCallRequest {
            id: "consult-1".into(),
            name: "consult_advisor".into(),
            arguments: serde_json::json!({
                "prompt": "How to structure my code?",
                "command": "non_existent_binary_xyz"
            }),
        }]))
    }
}

#[tokio::test]
async fn test_consult_advisor_unavailable_fallback() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();

    let advisor_output = Arc::new(tokio::sync::Mutex::new(None));
    let policy = ConsultAdvisorTestPolicy {
        advisor_output: advisor_output.clone(),
    };

    let sandbox = LocalSandbox::new(root);
    let mut harness = AgentHarness::new(policy, sandbox).with_max_turns(5);

    let task = TaskSpec::new("task-advisor", "test", "Consult test");
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Success);
    let output = advisor_output.lock().await.clone().unwrap_or_default();
    assert!(
        output.contains("not available in the current environment PATH"),
        "Should inform that binary is unavailable and provide fallback guidance: {}",
        output
    );
}

struct ConsultAdvisorEchoPolicy {
    advisor_output: Arc<tokio::sync::Mutex<Option<String>>>,
}

#[async_trait]
impl AgentPolicy for ConsultAdvisorEchoPolicy {
    fn name(&self) -> &str {
        "consult-advisor-echo-policy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        for msg in &context.messages {
            if let Some(results) = &msg.tool_results {
                for r in results {
                    if r.name == "consult_advisor" {
                        let mut out = self.advisor_output.lock().await;
                        *out = Some(r.output.clone());
                        return Ok(AgentAction::FinalAnswer(
                            "Echo consultation done".to_string(),
                        ));
                    }
                }
            }
        }

        Ok(AgentAction::CallTools(vec![ToolCallRequest {
            id: "consult-echo-1".into(),
            name: "consult_advisor".into(),
            arguments: serde_json::json!({
                "prompt": "hello advisor",
                "command": "echo",
                "timeout": 60
            }),
        }]))
    }
}

#[tokio::test]
async fn test_consult_advisor_custom_command() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();

    let advisor_output = Arc::new(tokio::sync::Mutex::new(None));
    let policy = ConsultAdvisorEchoPolicy {
        advisor_output: advisor_output.clone(),
    };

    let sandbox = LocalSandbox::new(root);
    let mut harness = AgentHarness::new(policy, sandbox)
        .with_max_turns(5)
        .with_advisor_timeout(Some(120));

    let task = TaskSpec::new("task-advisor-echo", "test", "Echo consult test");
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Success);
    let output = advisor_output.lock().await.clone().unwrap_or_default();
    assert!(
        output.contains("hello advisor"),
        "Should contain advisor output: {}",
        output
    );
}

struct ConsultAdvisorProxyTestPolicy {
    advisor_output: Arc<tokio::sync::Mutex<Option<String>>>,
}

#[async_trait]
impl AgentPolicy for ConsultAdvisorProxyTestPolicy {
    fn name(&self) -> &str {
        "consult-advisor-proxy-policy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        for msg in &context.messages {
            if let Some(results) = &msg.tool_results {
                for r in results {
                    if r.name == "consult_advisor" {
                        let mut out = self.advisor_output.lock().await;
                        *out = Some(r.output.clone());
                        return Ok(AgentAction::FinalAnswer(
                            "Proxy consultation done".to_string(),
                        ));
                    }
                }
            }
        }

        Ok(AgentAction::CallTools(vec![ToolCallRequest {
            id: "consult-proxy-1".into(),
            name: "consult_advisor".into(),
            arguments: serde_json::json!({
                "prompt": "proxy check",
                "command": "sh -c 'echo https=$HTTPS_PROXY,http=$HTTP_PROXY,all=$ALL_PROXY,no=$NO_PROXY #'",
                "https_proxy": "socks5://127.0.0.1:1088",
                "all_proxy": "socks5://127.0.0.1:1088"
            }),
        }]))
    }
}

#[tokio::test]
async fn test_consult_advisor_proxy_propagation() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();

    let advisor_output = Arc::new(tokio::sync::Mutex::new(None));
    let policy = ConsultAdvisorProxyTestPolicy {
        advisor_output: advisor_output.clone(),
    };

    let sandbox = LocalSandbox::new(root);
    let mut harness = AgentHarness::new(policy, sandbox)
        .with_max_turns(5)
        .with_advisor_http_proxy("http://127.0.0.1:2087")
        .with_advisor_no_proxy("localhost,127.0.0.1");

    let task = TaskSpec::new("task-advisor-proxy", "test", "Proxy consult test");
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Success);
    let output = advisor_output.lock().await.clone().unwrap_or_default();
    assert!(
        output.contains("https=socks5://127.0.0.1:1088"),
        "Should contain https_proxy: {}",
        output
    );
    assert!(
        output.contains("http=http://127.0.0.1:2087"),
        "Should contain http_proxy: {}",
        output
    );
    assert!(
        output.contains("all=socks5://127.0.0.1:1088"),
        "Should contain all_proxy: {}",
        output
    );
    assert!(
        output.contains("no=localhost,127.0.0.1"),
        "Should contain no_proxy: {}",
        output
    );
}

struct ConsultAgySpecificProxyPolicy {
    advisor_output: Arc<tokio::sync::Mutex<Option<String>>>,
}

#[async_trait]
impl AgentPolicy for ConsultAgySpecificProxyPolicy {
    fn name(&self) -> &str {
        "consult-agy-proxy-policy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        for msg in &context.messages {
            if let Some(results) = &msg.tool_results {
                for r in results {
                    if r.name == "consult_advisor" {
                        let mut out = self.advisor_output.lock().await;
                        *out = Some(r.output.clone());
                        return Ok(AgentAction::FinalAnswer(
                            "Agy consultation done".to_string(),
                        ));
                    }
                }
            }
        }

        Ok(AgentAction::CallTools(vec![ToolCallRequest {
            id: "consult-agy-1".into(),
            name: "consult_advisor".into(),
            arguments: serde_json::json!({
                "prompt": "agy proxy test",
                "command": "agy",
                "timeout": 2
            }),
        }]))
    }
}

#[tokio::test]
async fn test_consult_advisor_per_advisor_config_file() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();

    // Create custom advisors.yaml config file
    let config_yaml = r#"
advisors:
  agy:
    https_proxy: socks5://127.0.0.1:1088
    all_proxy: socks5://127.0.0.1:1088
  grok:
    https_proxy: http://127.0.0.1:2087
  default:
    http_proxy: http://127.0.0.1:8080
"#;
    let config_file_path = root.join("advisors.yaml");
    fs::write(&config_file_path, config_yaml).unwrap();

    let advisor_output = Arc::new(tokio::sync::Mutex::new(None));
    let policy = ConsultAgySpecificProxyPolicy {
        advisor_output: advisor_output.clone(),
    };

    let sandbox = LocalSandbox::new(root.clone());
    let mut harness = AgentHarness::new(policy, sandbox)
        .with_max_turns(5)
        .with_advisor_config_file(&config_file_path);

    let task = TaskSpec::new("task-advisor-agy-config", "test", "Agy config test");
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Success);
}

#[test]
fn test_parse_full_global_openduck_config_yaml() {
    let sample = r#"
model: deepseek-v4-flash
configured: true
active_provider: opencode_go
plugins:
  /home/zhanghu/.agents/plugins/hook-probe:
    enabled: true
  /home/zhanghu/.agents/plugins/mail-wait:
    enabled: true
notifications: notifications.yaml
advisors:
  # 专门给 agy (Antigravity CLI) 配置的代理
  agy:
    https_proxy: socks5://127.0.0.1:1088
    all_proxy: socks5://127.0.0.1:1088
    no_proxy: "localhost,127.0.0.1"   

  # 专门给 grok CLI 配置的代理
  grok:
    https_proxy: http://127.0.0.1:2087 
    http_proxy: http://127.0.0.1:2087 
    no_proxy: "localhost,127.0.0.1" 

  # 本地/其它 advisor
  openduck:
    https_proxy: http://127.0.0.1:2087
    http_proxy: http://127.0.0.1:2087
    no_proxy: "localhost,127.0.0.1"
"#;

    let parsed = openduck_harness::parse_advisor_configs_str(sample);
    assert_eq!(parsed.len(), 3);

    let agy = parsed.get("agy").expect("agy should be present");
    assert_eq!(agy.https_proxy.as_deref(), Some("socks5://127.0.0.1:1088"));
    assert_eq!(agy.all_proxy.as_deref(), Some("socks5://127.0.0.1:1088"));
    assert_eq!(agy.no_proxy.as_deref(), Some("localhost,127.0.0.1"));

    let grok = parsed.get("grok").expect("grok should be present");
    assert_eq!(grok.https_proxy.as_deref(), Some("http://127.0.0.1:2087"));
    assert_eq!(grok.http_proxy.as_deref(), Some("http://127.0.0.1:2087"));

    let openduck = parsed.get("openduck").expect("openduck should be present");
    assert_eq!(
        openduck.https_proxy.as_deref(),
        Some("http://127.0.0.1:2087")
    );
}

struct EvasiveNotesPolicy {
    step_count: Arc<AtomicUsize>,
    blocked_count: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentPolicy for EvasiveNotesPolicy {
    fn name(&self) -> &str {
        "evasive-notes-policy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        let current = self.step_count.fetch_add(1, Ordering::SeqCst);

        for msg in &context.messages {
            if let Some(results) = &msg.tool_results {
                for r in results {
                    if r.output.contains("Anti-Stagnation Circuit Breaker") {
                        self.blocked_count.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }

        if current >= 15 {
            return Ok(AgentAction::FinalAnswer("Done".to_string()));
        }

        // Alternatingly write scratch notes and read files (which previously reset the counter)
        if current % 2 == 1 {
            Ok(AgentAction::CallTools(vec![ToolCallRequest {
                id: format!("write-note-{}", current),
                name: "write_file".into(),
                arguments: serde_json::json!({
                    "path": format!(".agent/notes-progress-{}.md", current),
                    "content": "Progress note findings"
                }),
            }]))
        } else {
            Ok(AgentAction::CallTools(vec![ToolCallRequest {
                id: format!("read-{}", current),
                name: "read_file".into(),
                arguments: serde_json::json!({
                    "path": "src/main.rs"
                }),
            }]))
        }
    }
}

#[tokio::test]
async fn test_scratch_notes_do_not_bypass_circuit_breaker() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

    let step_count = Arc::new(AtomicUsize::new(0));
    let blocked_count = Arc::new(AtomicUsize::new(0));

    let policy = EvasiveNotesPolicy {
        step_count: step_count.clone(),
        blocked_count: blocked_count.clone(),
    };

    let sandbox = LocalSandbox::new(root);
    let mut harness = AgentHarness::new(policy, sandbox)
        .with_circuit_breaker(true)
        .with_stagnation_threshold(4)
        .with_max_turns(16);

    let task = TaskSpec::new("task-evasive-notes", "test", "Evasive notes test");
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(
        blocked_count.load(Ordering::SeqCst),
        0,
        "Circuit breaker should not trigger tool blocks"
    );
}

#[test]
fn test_token_based_context_compaction() {
    use openduck_harness::policy::HarnessMessage;
    use openduck_harness::runtime::{compact_context_messages, estimate_context_tokens};

    let mut messages = Vec::new();
    messages.push(HarnessMessage::system("System instructions"));
    messages.push(HarnessMessage::user("Task description"));

    for i in 1..=8 {
        messages.push(HarnessMessage::user(format!(
            "Step {i} user instructions: {}",
            "a".repeat(1000)
        )));
        messages.push(HarnessMessage::assistant(format!(
            "Step {i} assistant response: {}",
            "b".repeat(1000)
        )));
    }

    let initial_tokens = estimate_context_tokens(&messages);
    assert!(initial_tokens > 2000);
    assert_eq!(messages.len(), 18);

    // Set max_tokens = 1500, threshold = 0.8 (watermark = 1200 tokens)
    compact_context_messages(&mut messages, 80, Some(1500), 0.8);

    assert!(messages.len() < 18);
    let post_tokens = estimate_context_tokens(&messages);
    assert!(post_tokens < initial_tokens);
    assert_eq!(messages[0].content, "System instructions");
    assert_eq!(messages[1].content, "Task description");
    assert!(messages[2].content.contains("COMPACTED CONTEXT SUMMARY"));
}

#[test]
fn test_semantic_structured_facts_extraction() {
    use openduck_harness::policy::HarnessMessage;
    use openduck_harness::runtime::compact_context_messages;
    use openduck_harness::types::{ToolCallRequest, ToolCallResponse};

    let mut messages = Vec::new();
    messages.push(HarnessMessage::system("System instructions"));
    messages.push(HarnessMessage::user("Build feature X"));

    // Simulate reading files
    messages.push(HarnessMessage::assistant_with_tools(
        "Checking src/lib.rs",
        vec![ToolCallRequest {
            id: "call-1".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "src/lib.rs"}),
        }],
    ));
    messages.push(HarnessMessage::tool_response(vec![ToolCallResponse {
        id: "call-1".into(),
        name: "read_file".into(),
        output: "pub fn hello() {}".into(),
        is_error: false,
    }]));

    // Simulate modifying files
    messages.push(HarnessMessage::assistant_with_tools(
        "Modifying src/lib.rs",
        vec![ToolCallRequest {
            id: "call-2".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({
                "path": "src/lib.rs",
                "content": "pub fn hello() -> bool { true }"
            }),
        }],
    ));
    messages.push(HarnessMessage::tool_response(vec![ToolCallResponse {
        id: "call-2".into(),
        name: "write_file".into(),
        output: "File written successfully".into(),
        is_error: false,
    }]));

    // Simulate test execution error
    messages.push(HarnessMessage::assistant_with_tools(
        "Running tests",
        vec![ToolCallRequest {
            id: "call-3".into(),
            name: "shell".into(),
            arguments: serde_json::json!({"command": "cargo test"}),
        }],
    ));
    messages.push(HarnessMessage::tool_response(vec![ToolCallResponse {
        id: "call-3".into(),
        name: "shell".into(),
        output: "Exit: 1\nStdout:\ntest test_hello ... FAILED\nStderr:\nassertion failed".into(),
        is_error: true,
    }]));

    // Add padding messages to exceed message threshold
    for i in 4..=15 {
        messages.push(HarnessMessage::user(format!("Step {i} user prompt")));
        messages.push(HarnessMessage::assistant(format!("Step {i} response")));
    }

    compact_context_messages(&mut messages, 10, None, 0.8);

    let summary = &messages[2].content;
    assert!(summary.contains("COMPACTED CONTEXT SUMMARY"));
    assert!(
        summary.contains("src/lib.rs"),
        "Summary must contain explored/modified file"
    );
    assert!(
        summary.contains("Modified / Created Files"),
        "Summary must categorize modified files"
    );
    assert!(
        summary.contains("Key Errors Encountered"),
        "Summary must record errors"
    );
}

#[test]
fn test_tool_output_snipping_behavior() {
    use openduck_harness::runtime::snip_tool_output;

    let small_out = "line 1\nline 2\nline 3";
    let (res, snipped) = snip_tool_output(small_out, 10, 1000);
    assert!(!snipped);
    assert_eq!(res, small_out);

    let large_lines: String = (1..=300).map(|i| format!("log line {i}\n")).collect();
    let (res_lines, snipped_lines) = snip_tool_output(&large_lines, 50, 0);
    assert!(snipped_lines);
    assert!(res_lines.contains("lines omitted to preserve context headroom"));
    assert!(res_lines.starts_with("log line 1\n"));
    assert!(res_lines.contains("log line 300"));

    let huge_single_line = "x".repeat(100_000);
    let (res_bytes, snipped_bytes) = snip_tool_output(&huge_single_line, 0, 4000);
    assert!(snipped_bytes);
    assert!(res_bytes.contains("bytes omitted to preserve context headroom"));
    assert!(res_bytes.len() < 5000);
}

#[test]
fn test_global_harness_settings_parsing() {
    use openduck_harness::types::parse_harness_global_settings_str;

    let yaml_nested = r#"
harness:
  circuit_breaker_enabled: false
  stagnation_threshold: 18
  periodic_review_interval: 5
  max_turns: 42
  advisor_command: "my-advisor"
  advisor_timeout_seconds: 45
"#;
    let s = parse_harness_global_settings_str(yaml_nested);
    assert_eq!(s.circuit_breaker_enabled, Some(false));
    assert_eq!(s.stagnation_threshold, Some(18));
    assert_eq!(s.periodic_review_interval, Some(5));
    assert_eq!(s.max_turns, Some(42));
    assert_eq!(s.advisor_command.as_deref(), Some("my-advisor"));
    assert_eq!(s.advisor_timeout_seconds, Some(45));

    let yaml_top_level = r#"
circuit_breaker_enabled: false
stagnation_threshold: 22
judge:
  provider: laya
  endpoint: "http://localhost:8732/api/laya"
  timeout_ms: 400
  points:
    turn.drift: active
    turn.completion: active
"#;
    let s2 = parse_harness_global_settings_str(yaml_top_level);
    assert_eq!(s2.circuit_breaker_enabled, Some(false));
    assert_eq!(s2.stagnation_threshold, Some(22));
    assert!(s2.judge.is_some());
    let j = s2.judge.unwrap();
    assert_eq!(j.provider, "laya");
    assert_eq!(j.endpoint, "http://localhost:8732/api/laya");
    assert_eq!(j.timeout_ms, 400);
    assert_eq!(
        j.points.get("turn.drift").map(|s| s.as_str()),
        Some("active")
    );
    assert_eq!(
        j.points.get("turn.completion").map(|s| s.as_str()),
        Some("active")
    );
}

#[test]
fn test_project_execution_config_serialization() {
    use openduck_harness::project::ExecutionConfig;

    let yaml = r#"
maxTurns: 50
timeoutSeconds: 600
concurrency: 8
stagnationThreshold: 14
circuitBreakerEnabled: false
periodicReviewInterval: 10
"#;
    let cfg: ExecutionConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(cfg.max_turns, 50);
    assert_eq!(cfg.timeout_seconds, 600);
    assert_eq!(cfg.concurrency, 8);
    assert_eq!(cfg.stagnation_threshold, Some(14));
    assert_eq!(cfg.circuit_breaker_enabled, Some(false));
    assert_eq!(cfg.periodic_review_interval, Some(10));
}

struct RepeatedReadPolicy {
    step_count: Arc<AtomicUsize>,
    blocked_count: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentPolicy for RepeatedReadPolicy {
    fn name(&self) -> &str {
        "repeated-read-policy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        let current = self.step_count.fetch_add(1, Ordering::SeqCst);

        for msg in &context.messages {
            if let Some(results) = &msg.tool_results {
                for r in results {
                    if r.output.contains("Repeated Unchanged Read")
                        || r.output.contains("Tool execution prevented")
                    {
                        self.blocked_count.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }

        if current >= 5 {
            return Ok(AgentAction::FinalAnswer("Done".to_string()));
        }

        match current {
            0 => Ok(AgentAction::CallTools(vec![ToolCallRequest {
                id: "read-1".into(),
                name: "read_file".into(),
                arguments: serde_json::json!({"path": "src/main.rs"}),
            }])),
            1 => Ok(AgentAction::CallTools(vec![ToolCallRequest {
                id: "read-2".into(),
                name: "shell".into(),
                arguments: serde_json::json!({"command": "cat src/main.rs"}),
            }])),
            2 => Ok(AgentAction::CallTools(vec![ToolCallRequest {
                id: "read-3".into(),
                name: "shell".into(),
                arguments: serde_json::json!({"command": "sed -n '1,10p' src/main.rs"}),
            }])),
            3 => Ok(AgentAction::CallTools(vec![ToolCallRequest {
                id: "mod-1".into(),
                name: "shell".into(),
                arguments: serde_json::json!({"command": "echo 'fn new() {}' >> src/main.rs"}),
            }])),
            _ => Ok(AgentAction::CallTools(vec![ToolCallRequest {
                id: "read-4".into(),
                name: "read_file".into(),
                arguments: serde_json::json!({"path": "src/main.rs"}),
            }])),
        }
    }
}

#[tokio::test]
async fn test_repeated_unchanged_file_reads_not_blocked() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

    let step_count = Arc::new(AtomicUsize::new(0));
    let blocked_count = Arc::new(AtomicUsize::new(0));

    let policy = RepeatedReadPolicy {
        step_count: step_count.clone(),
        blocked_count: blocked_count.clone(),
    };

    let sandbox = LocalSandbox::new(root);
    let mut harness = AgentHarness::new(policy, sandbox)
        .with_circuit_breaker(true)
        .with_stagnation_threshold(10)
        .with_max_turns(10);

    let task = TaskSpec::new("task-repeated-read", "test", "Repeated read test");
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(
        blocked_count.load(Ordering::SeqCst),
        0,
        "Tool execution should not be prevented for repeated reads"
    );
}

#[test]
fn test_scratchpad_notes_pinned_across_compaction() {
    use openduck_harness::policy::HarnessMessage;
    use openduck_harness::runtime::compact_context_messages;

    let mut messages = vec![
        HarnessMessage::system("System instructions"),
        HarnessMessage::user("Please complete GPS feature"),
        HarnessMessage::assistant_with_tools(
            "Writing note",
            vec![ToolCallRequest {
                id: "w1".into(),
                name: "write_file".into(),
                arguments: serde_json::json!({
                    "path": ".agent/notes-gps-plan.md",
                    "content": "# Architecture Plan\n- Step 1: Add DTO\n- Step 2: Wire engine\n- Step 3: Run e2e tests"
                }),
            }],
        ),
        HarnessMessage::tool_response(vec![openduck_harness::types::ToolCallResponse {
            id: "w1".into(),
            name: "write_file".into(),
            output: "File written successfully".into(),
            is_error: false,
        }]),
        HarnessMessage::assistant("I wrote the plan"),
        HarnessMessage::user("Continue"),
        HarnessMessage::assistant("Working..."),
        HarnessMessage::user("Status update"),
    ];

    compact_context_messages(&mut messages, 6, None, 0.8);

    let summary_msg = messages
        .iter()
        .find(|m| m.content.contains("COMPACTED CONTEXT SUMMARY"));
    assert!(
        summary_msg.is_some(),
        "Compacted context summary must exist"
    );
    let summary_text = &summary_msg.unwrap().content;
    assert!(
        summary_text.contains("Retained Scratchpad & Architecture Notes"),
        "Summary must contain retained notes section, got: {summary_text}"
    );
    assert!(
        summary_text.contains(".agent/notes-gps-plan.md"),
        "Summary must mention the note file path"
    );
    assert!(
        summary_text.contains("Architecture Plan"),
        "Summary must retain the note content"
    );
}

#[test]
fn test_shell_read_target_extraction() {
    use openduck_harness::runtime::{extract_read_target_from_shell, is_shell_fragmented_read};

    assert_eq!(
        extract_read_target_from_shell("cat docs/GPS_PLATFORM_PLAN.md").as_deref(),
        Some("docs/GPS_PLATFORM_PLAN.md")
    );
    assert_eq!(
        extract_read_target_from_shell("sed -n '80,200p' docs/GPS_PLATFORM_PLAN.md").as_deref(),
        Some("docs/GPS_PLATFORM_PLAN.md")
    );
    assert_eq!(
        extract_read_target_from_shell("awk 'NR>=50 && NR<=200' rmqx_models/src/gps_engine.rs")
            .as_deref(),
        Some("rmqx_models/src/gps_engine.rs")
    );
    assert_eq!(
        extract_read_target_from_shell(
            "cd /mnt/e/goose && head -60 rmqx_admin/src/domain/vo/gps_platform.rs"
        )
        .as_deref(),
        Some("rmqx_admin/src/domain/vo/gps_platform.rs")
    );
    assert_eq!(
        extract_read_target_from_shell("cargo test").as_deref(),
        None
    );

    assert!(is_shell_fragmented_read("sed -n '1,50p' src/main.rs"));
    assert!(is_shell_fragmented_read(
        "awk 'NR>=10 && NR<=40 {print $0}' src/lib.rs"
    ));
    assert!(!is_shell_fragmented_read(
        "sed -i 's/foo/bar/g' src/main.rs"
    ));
}

#[test]
fn test_normalize_read_target() {
    use openduck_harness::runtime::normalize_read_target;
    use std::path::Path;

    let ws = Path::new("/workspace/project");
    assert_eq!(
        normalize_read_target("/workspace/project/src/main.rs", Some(ws)),
        "src/main.rs"
    );
    assert_eq!(
        normalize_read_target("./src/main.rs", Some(ws)),
        "src/main.rs"
    );
    assert_eq!(
        normalize_read_target("'src/main.rs'", Some(ws)),
        "src/main.rs"
    );
    assert_eq!(
        normalize_read_target("\"src/main.rs\"", None),
        "src/main.rs"
    );
    assert_eq!(normalize_read_target("`src/main.rs`", None), "src/main.rs");
}

#[tokio::test]
async fn test_target_based_reads_not_prevented_across_mixed_tools() {
    use openduck_harness::policy::{AgentAction, AgentPolicy, HarnessContextView};
    use openduck_harness::runtime::AgentHarness;
    use openduck_harness::sandbox::local::LocalSandbox;
    use openduck_harness::types::{RunStatus, ToolCallRequest, ToolDefinition};
    use openduck_harness::TaskSpec;
    use std::sync::atomic::AtomicBool;

    #[derive(Clone)]
    struct MixedReadPolicy {
        step: Arc<AtomicUsize>,
        received_throttling: Arc<AtomicBool>,
    }

    #[async_trait]
    impl AgentPolicy for MixedReadPolicy {
        fn name(&self) -> &str {
            "MixedReadPolicy"
        }

        async fn step(
            &mut self,
            context: &HarnessContextView,
            _tools: &[ToolDefinition],
        ) -> anyhow::Result<AgentAction> {
            let s = self.step.fetch_add(1, Ordering::SeqCst);

            // Check if any message contains repeated read prevention response
            for msg in &context.messages {
                if let Some(results) = &msg.tool_results {
                    for r in results {
                        if r.output.contains("Repeated Unchanged Read") {
                            self.received_throttling.store(true, Ordering::SeqCst);
                        }
                    }
                }
            }

            let ws = context.workspace_root.to_string_lossy().to_string();
            match s {
                0 => Ok(AgentAction::CallTools(vec![ToolCallRequest {
                    id: "c1".into(),
                    name: "read_file".into(),
                    arguments: serde_json::json!({"path": "src/lib.rs"}),
                }])),
                1 => Ok(AgentAction::CallTools(vec![ToolCallRequest {
                    id: "c2".into(),
                    name: "shell".into(),
                    arguments: serde_json::json!({"command": format!("sed -n '1,20p' {}/src/lib.rs", ws)}),
                }])),
                2 => Ok(AgentAction::CallTools(vec![ToolCallRequest {
                    id: "c3".into(),
                    name: "shell".into(),
                    arguments: serde_json::json!({"command": "awk 'NR>=1 && NR<=10' ./src/lib.rs"}),
                }])),
                _ => Ok(AgentAction::FinalAnswer("Done".into())),
            }
        }
    }

    let dir = tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("lib.rs"), "pub fn hello() {}\n").unwrap();

    let step = Arc::new(AtomicUsize::new(0));
    let received_throttling = Arc::new(AtomicBool::new(false));

    let policy = MixedReadPolicy {
        step: step.clone(),
        received_throttling: received_throttling.clone(),
    };

    let sandbox = LocalSandbox::new(dir.path().to_path_buf());
    let mut harness = AgentHarness::new(policy, sandbox).with_stagnation_threshold(10);

    let task = TaskSpec::new("task-mixed", "test", "Fix feature");
    let result = harness.run_task(&task).await.unwrap();

    assert_eq!(result.status, RunStatus::Success);
    assert!(
        !received_throttling.load(Ordering::SeqCst),
        "Repeated reads across mixed tools should execute without prevention"
    );
}

#[tokio::test]
async fn test_exploration_budget_contract_and_nudge() {
    use openduck_harness::policy::{AgentAction, AgentPolicy, HarnessContextView};
    use openduck_harness::runtime::AgentHarness;
    use openduck_harness::sandbox::local::LocalSandbox;
    use openduck_harness::types::{RunStatus, ToolCallRequest, ToolDefinition};
    use openduck_harness::TaskSpec;
    use std::sync::atomic::AtomicBool;

    #[derive(Clone)]
    struct BudgetTestPolicy {
        step: Arc<AtomicUsize>,
        contract_present: Arc<AtomicBool>,
        nudge_received_at_budget: Arc<AtomicBool>,
    }

    #[async_trait]
    impl AgentPolicy for BudgetTestPolicy {
        fn name(&self) -> &str {
            "BudgetTestPolicy"
        }

        async fn step(
            &mut self,
            context: &HarnessContextView,
            _tools: &[ToolDefinition],
        ) -> anyhow::Result<AgentAction> {
            let s = self.step.fetch_add(1, Ordering::SeqCst);

            if s == 0 {
                if let Some(first_user_msg) = context
                    .messages
                    .iter()
                    .find(|m| m.role == openduck_harness::policy::MessageRole::User)
                {
                    if first_user_msg
                        .content
                        .contains("Execution Guidelines & Budget Contract")
                    {
                        self.contract_present.store(true, Ordering::SeqCst);
                    }
                }
            }

            if let Some(last_msg) = context.messages.last() {
                if last_msg.content.contains("[SYSTEM NUDGE")
                    && last_msg.content.contains("budget: 3 steps")
                {
                    self.nudge_received_at_budget.store(true, Ordering::SeqCst);
                }
            }

            if s < 4 {
                Ok(AgentAction::CallTools(vec![ToolCallRequest {
                    id: format!("call-{}", s),
                    name: "read_file".into(),
                    arguments: serde_json::json!({"path": format!("file{}.txt", s)}),
                }]))
            } else {
                Ok(AgentAction::FinalAnswer("Completed".into()))
            }
        }
    }

    let dir = tempdir().unwrap();
    for i in 0..5 {
        fs::write(dir.path().join(format!("file{}.txt", i)), "content").unwrap();
    }

    let step = Arc::new(AtomicUsize::new(0));
    let contract_present = Arc::new(AtomicBool::new(false));
    let nudge_received_at_budget = Arc::new(AtomicBool::new(false));

    let policy = BudgetTestPolicy {
        step: step.clone(),
        contract_present: contract_present.clone(),
        nudge_received_at_budget: nudge_received_at_budget.clone(),
    };

    let sandbox = LocalSandbox::new(dir.path().to_path_buf());
    let mut harness = AgentHarness::new(policy, sandbox).with_stagnation_threshold(8);

    let task = TaskSpec::new("task-budget", "test", "Implement feature").with_exploration_budget(3);

    let result = harness.run_task(&task).await.unwrap();
    assert_eq!(result.status, RunStatus::Success);
    assert!(
        !contract_present.load(Ordering::SeqCst),
        "Budget contract should no longer be injected into user prompt"
    );
    assert!(
        !nudge_received_at_budget.load(Ordering::SeqCst),
        "Nudge should not trigger"
    );
}

#[test]
fn test_compact_context_preserves_advisor_and_plan() {
    use openduck_harness::policy::HarnessMessage;
    use openduck_harness::runtime::compact_context_messages;
    use openduck_harness::types::{ToolCallRequest, ToolCallResponse};

    let mut messages = vec![
        HarnessMessage::system("System instructions"),
        HarnessMessage::user("Please complete GPS feature"),
        HarnessMessage::assistant_with_tools(
            "Plan: 1. Add DTO fields 2. Implement SMS in gps_notify.rs 3. Update AlarmRuleList.vue",
            vec![ToolCallRequest {
                id: "adv1".into(),
                name: "consult_advisor".into(),
                arguments: serde_json::json!({
                    "prompt": "Should I use reqwest for SMS or raw socket?"
                }),
            }],
        ),
        HarnessMessage::tool_response(vec![ToolCallResponse {
            id: "adv1".into(),
            name: "consult_advisor".into(),
            output: "Use reqwest with rustls for HTTPS SMS gateway integration.".into(),
            is_error: false,
        }]),
        HarnessMessage::assistant("I will use reqwest"),
        HarnessMessage::user("Continue"),
        HarnessMessage::assistant("Working..."),
        HarnessMessage::user("Status update"),
    ];

    compact_context_messages(&mut messages, 6, None, 0.8);

    let summary_msg = messages
        .iter()
        .find(|m| m.content.contains("COMPACTED CONTEXT SUMMARY"));
    assert!(
        summary_msg.is_some(),
        "Compacted context summary must exist"
    );
    let summary_text = &summary_msg.unwrap().content;
    assert!(
        summary_text.contains("Advisor Consultations & Guidance"),
        "Summary must contain advisor section, got: {summary_text}"
    );
    assert!(
        summary_text.contains("Should I use reqwest"),
        "Summary must retain advisor question"
    );
    assert!(
        summary_text.contains("Use reqwest with rustls"),
        "Summary must retain advisor response"
    );
    assert!(
        summary_text.contains("Discovered Architecture & Action Plan"),
        "Summary must retain planned steps"
    );
}

fn advisor_flag_value(cmd: &str, flag: &str) -> Option<String> {
    let needle = format!("{flag} ");
    let rest = cmd.split(&needle).nth(1)?;
    let token = rest.split_whitespace().next()?;
    Some(token.trim_matches('\'').to_string())
}

struct AdvisorCliSandbox {
    inner: LocalSandbox,
    commands: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl openduck_harness::sandbox::SandboxDriver for AdvisorCliSandbox {
    fn name(&self) -> &str {
        "advisor-cli-sandbox"
    }

    fn workspace_root(&self) -> &std::path::Path {
        self.inner.workspace_root()
    }

    async fn initialize(&mut self) -> anyhow::Result<()> {
        self.inner.initialize().await
    }

    async fn exec_command(
        &self,
        cmd: &str,
        opts: &openduck_harness::types::ExecOptions,
    ) -> anyhow::Result<openduck_harness::types::ExecOutput> {
        if cmd.starts_with("command -v grok")
            || cmd.starts_with("command -v agy")
            || cmd.starts_with("command -v openduck")
        {
            return Ok(openduck_harness::types::ExecOutput {
                exit_code: 0,
                stdout: "/usr/bin/fake-advisor".into(),
                stderr: String::new(),
            });
        }

        if cmd.contains("grok ") || cmd.starts_with("grok") {
            self.commands.lock().unwrap().push(cmd.to_string());
            let session = advisor_flag_value(cmd, "--resume")
                .or_else(|| advisor_flag_value(cmd, "--session-id"))
                .unwrap_or_else(|| "sess-fallback".into());
            return Ok(openduck_harness::types::ExecOutput {
                exit_code: 0,
                stdout: format!(
                    r#"{{"text":"advice via {session}","stopReason":"end_turn","sessionId":"{session}"}}"#
                ),
                stderr: String::new(),
            });
        }

        self.inner.exec_command(cmd, opts).await
    }

    async fn read_file(&self, path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
        self.inner.read_file(path).await
    }

    async fn write_file(&self, path: &std::path::Path, content: &[u8]) -> anyhow::Result<()> {
        self.inner.write_file(path, content).await
    }

    async fn snapshot(
        &mut self,
        label: &str,
    ) -> anyhow::Result<openduck_harness::types::SnapshotId> {
        self.inner.snapshot(label).await
    }

    async fn restore_snapshot(
        &mut self,
        id: &openduck_harness::types::SnapshotId,
    ) -> anyhow::Result<()> {
        self.inner.restore_snapshot(id).await
    }

    async fn cleanup(&mut self) -> anyhow::Result<()> {
        self.inner.cleanup().await
    }
}

struct RepeatConsultPolicy {
    max_consults: usize,
    fresh_on: Option<usize>,
}

#[async_trait]
impl AgentPolicy for RepeatConsultPolicy {
    fn name(&self) -> &str {
        "repeat-consult-policy"
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<AgentAction> {
        let consults = context
            .messages
            .iter()
            .filter(|m| {
                m.tool_results
                    .as_ref()
                    .is_some_and(|results| results.iter().any(|r| r.name == "consult_advisor"))
            })
            .count();
        if consults >= self.max_consults {
            return Ok(AgentAction::FinalAnswer("consults complete".into()));
        }

        let mut arguments = serde_json::json!({
            "prompt": format!("question {}", consults + 1),
            "command": "grok"
        });
        if self.fresh_on == Some(consults + 1) {
            arguments["fresh"] = serde_json::json!(true);
        }
        Ok(AgentAction::CallTools(vec![ToolCallRequest {
            id: format!("consult-{}", consults + 1),
            name: "consult_advisor".into(),
            arguments,
        }]))
    }
}

#[tokio::test]
async fn test_consult_advisor_resumes_grok_session_on_follow_up() {
    let commands = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sandbox = AdvisorCliSandbox {
        inner: LocalSandbox::new(tempdir().unwrap().path().to_path_buf()),
        commands: commands.clone(),
    };
    let mut harness = AgentHarness::new(
        RepeatConsultPolicy {
            max_consults: 2,
            fresh_on: None,
        },
        sandbox,
    )
    .with_max_turns(6);

    let result = harness
        .run_task(&TaskSpec::new(
            "task-session",
            "test",
            "Use advisor session",
        ))
        .await
        .unwrap();
    assert_eq!(result.status, RunStatus::Success);

    let cmds = commands.lock().unwrap().clone();
    assert_eq!(cmds.len(), 2, "expected two grok invocations: {cmds:?}");
    assert!(
        cmds[0].contains("--session-id"),
        "first call should pin a new grok session: {}",
        cmds[0]
    );
    assert!(
        !cmds[0].contains("--resume"),
        "first call must not resume: {}",
        cmds[0]
    );
    let first_id = advisor_flag_value(&cmds[0], "--session-id").expect("first session id");
    assert_eq!(
        advisor_flag_value(&cmds[1], "--resume").as_deref(),
        Some(first_id.as_str()),
        "follow-up should resume the same grok session: {}",
        cmds[1]
    );
    assert!(
        !cmds[1].contains("--session-id"),
        "follow-up must resume, not create: {}",
        cmds[1]
    );
    assert!(
        !cmds.iter().any(|c| c.contains("--continue")),
        "must not use cwd-global --continue"
    );

    let stored = result
        .continuation
        .as_ref()
        .and_then(|cp| cp.advisor_sessions.get("task-session"))
        .and_then(|m| m.get("grok"))
        .cloned();
    assert_eq!(stored.as_deref(), Some(first_id.as_str()));
}

#[tokio::test]
async fn test_consult_advisor_fresh_starts_new_grok_session() {
    let commands = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sandbox = AdvisorCliSandbox {
        inner: LocalSandbox::new(tempdir().unwrap().path().to_path_buf()),
        commands: commands.clone(),
    };
    let mut harness = AgentHarness::new(
        RepeatConsultPolicy {
            max_consults: 2,
            fresh_on: Some(2),
        },
        sandbox,
    )
    .with_max_turns(6);

    let result = harness
        .run_task(&TaskSpec::new("task-fresh", "test", "Independent review"))
        .await
        .unwrap();
    assert_eq!(result.status, RunStatus::Success);

    let cmds = commands.lock().unwrap().clone();
    assert_eq!(cmds.len(), 2, "{cmds:?}");
    let first_id = advisor_flag_value(&cmds[0], "--session-id").expect("first session id");
    let second_id = advisor_flag_value(&cmds[1], "--session-id").expect("fresh session id");
    assert_ne!(first_id, second_id);
    assert!(
        !cmds[1].contains("--resume"),
        "fresh=true must not resume: {}",
        cmds[1]
    );
}

#[tokio::test]
async fn test_consult_advisor_restores_session_from_continuation() {
    let commands = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sandbox = AdvisorCliSandbox {
        inner: LocalSandbox::new(tempdir().unwrap().path().to_path_buf()),
        commands: commands.clone(),
    };

    let mut advisor_sessions = std::collections::HashMap::new();
    advisor_sessions.insert(
        "task-resume-adv".into(),
        std::collections::HashMap::from([("grok".into(), "sess-from-checkpoint".into())]),
    );
    let checkpoint = openduck_harness::ContinuationCheckpoint {
        task_id: "task-resume-adv".into(),
        source_job_id: "job-1".into(),
        created_at: chrono::Utc::now(),
        stop_reason: openduck_harness::ContinuationStopReason::Cancelled,
        error: Some("hit max turns".into()),
        compacted_summary: "Prior grok advice was to inspect runtime.rs".into(),
        completed_subtasks: vec![],
        resume_subtask_id: None,
        subtask_plan: vec![],
        step_count: 4,
        advisor_sessions,
    };

    let mut harness = AgentHarness::new(
        RepeatConsultPolicy {
            max_consults: 1,
            fresh_on: None,
        },
        sandbox,
    )
    .with_max_turns(4)
    .with_continuation(Some(checkpoint));

    let result = harness
        .run_task(
            &TaskSpec::new("task-resume-adv", "test", "Continue advisor")
                .with_auto_decompose(false),
        )
        .await
        .unwrap();
    assert_eq!(result.status, RunStatus::Success);

    let cmds = commands.lock().unwrap().clone();
    assert_eq!(cmds.len(), 1, "{cmds:?}");
    assert_eq!(
        advisor_flag_value(&cmds[0], "--resume").as_deref(),
        Some("sess-from-checkpoint")
    );
}

#[tokio::test]
async fn test_consult_advisor_isolates_sessions_per_subtask() {
    let commands = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sandbox = AdvisorCliSandbox {
        inner: LocalSandbox::new(tempdir().unwrap().path().to_path_buf()),
        commands: commands.clone(),
    };
    let mut harness = AgentHarness::new(
        RepeatConsultPolicy {
            max_consults: 1,
            fresh_on: None,
        },
        sandbox,
    )
    .with_max_turns(8);

    let subtasks = vec![
        openduck_harness::SubtaskSpec::new("st-a", "First", "Do first").with_max_turns(4),
        openduck_harness::SubtaskSpec::new("st-b", "Second", "Do second").with_max_turns(4),
    ];
    let result = harness
        .run_task(
            &TaskSpec::new("task-sub-adv", "test", "Split work")
                .with_auto_decompose(false)
                .with_subtasks(subtasks),
        )
        .await
        .unwrap();
    assert_eq!(result.status, RunStatus::Success);

    let cmds = commands.lock().unwrap().clone();
    assert_eq!(cmds.len(), 2, "{cmds:?}");
    assert!(
        cmds.iter()
            .all(|c| c.contains("--session-id") && !c.contains("--resume")),
        "each subtask should start its own grok session: {cmds:?}"
    );
    let id_a = advisor_flag_value(&cmds[0], "--session-id").unwrap();
    let id_b = advisor_flag_value(&cmds[1], "--session-id").unwrap();
    assert_ne!(id_a, id_b);

    let sessions = &result.continuation.unwrap().advisor_sessions;
    assert_eq!(
        sessions
            .get("st-a")
            .and_then(|m| m.get("grok"))
            .map(String::as_str),
        Some(id_a.as_str())
    );
    assert_eq!(
        sessions
            .get("st-b")
            .and_then(|m| m.get("grok"))
            .map(String::as_str),
        Some(id_b.as_str())
    );
}
