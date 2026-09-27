use crate::advisor::{
    advisor_visible_output, build_advisor_command, json_truthy, parse_advisor_session_id,
    AdvisorKind,
};
use crate::eval::task::TaskSpec;
use crate::policy::{
    AgentAction, AgentPolicy, HarnessContextView, HarnessMessage, MessageRole, YieldReason,
};
use crate::sandbox::SandboxDriver;
use crate::telemetry::{TrajectoryLogger, TrajectoryRecord, TrajectoryStep};
use crate::types::{
    AdvisorProxyConfig, ExecOptions, RunStatus, ToolCallRequest, ToolCallResponse, ToolDefinition,
};
use anyhow::{anyhow, Result};
use chrono::Utc;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

pub struct TaskExecutionResult {
    pub task_id: String,
    pub status: RunStatus,
    pub step_count: usize,
    pub tool_calls_count: usize,
    pub duration_ms: u128,
    pub final_answer: Option<String>,
    pub trajectory: TrajectoryRecord,
    pub continuation: Option<crate::eval::ContinuationCheckpoint>,
}

pub type StepObserver = Arc<dyn Fn(&[TrajectoryStep]) + Send + Sync>;

pub struct AgentHarness<P: AgentPolicy, S: SandboxDriver> {
    policy: P,
    sandbox: S,
    tools: Vec<ToolDefinition>,
    max_turns: usize,
    stagnation_threshold: usize,
    exploration_budget: Option<usize>,
    periodic_review_interval: Option<usize>,
    advisor_command: Option<String>,
    advisor_timeout_seconds: Option<u64>,
    advisor_proxy: AdvisorProxyConfig,
    advisor_proxies: HashMap<String, AdvisorProxyConfig>,
    circuit_breaker_enabled: bool,
    max_context_messages: usize,
    max_context_tokens: Option<usize>,
    compaction_threshold: f64,
    max_tool_output_bytes: usize,
    max_tool_output_lines: usize,
    trajectory_logger: Option<Arc<TrajectoryLogger>>,
    step_observer: Option<StepObserver>,
    system_prompt: Option<String>,
    active_rules: Option<Vec<crate::telemetry::ActiveRuleSummary>>,
    continuation: Option<crate::eval::ContinuationCheckpoint>,
    extra_turns: Option<usize>,
    advisor_sessions: HashMap<String, HashMap<String, String>>,
    current_advisor_scope: String,
    judge: Arc<crate::judge::DecisionEngine>,
}

impl<P: AgentPolicy, S: SandboxDriver> AgentHarness<P, S> {
    pub fn new(policy: P, sandbox: S) -> Self {
        let tools = Self::default_tools();
        let global_settings =
            crate::types::discover_global_harness_settings(Some(sandbox.workspace_root()));
        let stagnation_threshold = global_settings.stagnation_threshold.unwrap_or(6);
        let exploration_budget = global_settings.exploration_budget;
        let circuit_breaker_enabled = global_settings.circuit_breaker_enabled.unwrap_or(true);
        let periodic_review_interval = global_settings.periodic_review_interval;
        let advisor_command = global_settings.advisor_command;
        let advisor_timeout_seconds = global_settings.advisor_timeout_seconds;
        let advisor_proxies = global_settings.advisors;
        let max_turns = global_settings.max_turns.unwrap_or(30);
        let judge = Arc::new(crate::judge::DecisionEngine::default());

        Self {
            policy,
            sandbox,
            tools,
            max_turns,
            stagnation_threshold,
            exploration_budget,
            periodic_review_interval,
            advisor_command,
            advisor_timeout_seconds,
            advisor_proxy: AdvisorProxyConfig::default(),
            advisor_proxies,
            circuit_breaker_enabled,
            max_context_messages: 80,
            max_context_tokens: None,
            compaction_threshold: 0.8,
            max_tool_output_bytes: 32 * 1024,
            max_tool_output_lines: 200,
            trajectory_logger: None,
            step_observer: None,
            system_prompt: None,
            active_rules: None,
            continuation: None,
            extra_turns: None,
            advisor_sessions: HashMap::new(),
            current_advisor_scope: String::new(),
            judge,
        }
    }

    pub fn with_judge(mut self, judge: Arc<crate::judge::DecisionEngine>) -> Self {
        self.judge = judge;
        self
    }

    pub fn with_judge_engine(mut self, judge: crate::judge::DecisionEngine) -> Self {
        self.judge = Arc::new(judge);
        self
    }

    pub fn judge(&self) -> &crate::judge::DecisionEngine {
        &self.judge
    }

    pub fn with_system_prompt(mut self, system_prompt: Option<String>) -> Self {
        self.system_prompt = system_prompt;
        self
    }

    pub fn with_active_rules(
        mut self,
        active_rules: Option<Vec<crate::telemetry::ActiveRuleSummary>>,
    ) -> Self {
        self.active_rules = active_rules;
        self
    }

    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = max_turns;
        self
    }

    pub fn with_stagnation_threshold(mut self, threshold: usize) -> Self {
        self.stagnation_threshold = threshold;
        self
    }

    pub fn with_exploration_budget(mut self, budget: usize) -> Self {
        self.exploration_budget = Some(budget);
        self
    }

    pub fn with_periodic_review_interval(mut self, interval: Option<usize>) -> Self {
        self.periodic_review_interval = interval;
        self
    }

    pub fn with_advisor_command(mut self, command: Option<String>) -> Self {
        self.advisor_command = command;
        self
    }

    pub fn with_advisor_timeout(mut self, timeout_seconds: Option<u64>) -> Self {
        self.advisor_timeout_seconds = timeout_seconds;
        self
    }

    pub fn with_advisor_proxy(mut self, proxy: AdvisorProxyConfig) -> Self {
        self.advisor_proxy = proxy;
        self
    }

    pub fn with_advisor_https_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.advisor_proxy.https_proxy = Some(proxy.into());
        self
    }

    pub fn with_advisor_http_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.advisor_proxy.http_proxy = Some(proxy.into());
        self
    }

    pub fn with_advisor_all_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.advisor_proxy.all_proxy = Some(proxy.into());
        self
    }

    pub fn with_advisor_no_proxy(mut self, no_proxy: impl Into<String>) -> Self {
        self.advisor_proxy.no_proxy = Some(no_proxy.into());
        self
    }

    pub fn with_advisor_proxy_for(
        mut self,
        advisor_name: impl Into<String>,
        proxy: AdvisorProxyConfig,
    ) -> Self {
        self.advisor_proxies
            .insert(advisor_name.into().to_lowercase(), proxy);
        self
    }

    pub fn with_advisor_proxies(mut self, proxies: HashMap<String, AdvisorProxyConfig>) -> Self {
        for (k, v) in proxies {
            self.advisor_proxies.insert(k.to_lowercase(), v);
        }
        self
    }

    pub fn with_advisor_config_file(mut self, path: impl AsRef<std::path::Path>) -> Self {
        let loaded = crate::types::load_advisor_configs_file(path.as_ref());
        for (k, v) in loaded {
            self.advisor_proxies.insert(k.to_lowercase(), v);
        }
        self
    }

    pub fn detect_periodic_review_interval(&self) -> Option<usize> {
        if let Some(interval) = self.periodic_review_interval {
            return Some(interval);
        }
        if let Some(rules) = &self.active_rules {
            let re = regex::Regex::new(r"(?:per|every)[-_ ](\d+)[-_ ]steps").ok();
            for rule in rules {
                let text = format!("{} {}", rule.name, rule.description).to_lowercase();
                if text.contains("per-10-steps")
                    || text.contains("every 10 steps")
                    || text.contains("per 10 steps")
                {
                    return Some(10);
                }
                if text.contains("per-5-steps")
                    || text.contains("every 5 steps")
                    || text.contains("per 5 steps")
                {
                    return Some(5);
                }
                if text.contains("per-20-steps")
                    || text.contains("every 20 steps")
                    || text.contains("per 20 steps")
                {
                    return Some(20);
                }
                if let Some(ref r) = re {
                    if let Some(caps) = r.captures(&text) {
                        if let Some(m) = caps.get(1) {
                            if let Ok(n) = m.as_str().parse::<usize>() {
                                if n > 0 && n <= 100 {
                                    return Some(n);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    pub fn with_circuit_breaker(mut self, enabled: bool) -> Self {
        self.circuit_breaker_enabled = enabled;
        self
    }

    /// Sets the maximum message count threshold before triggering secondary compaction.
    pub fn with_max_context_messages(mut self, max: usize) -> Self {
        self.max_context_messages = max;
        self
    }

    /// Sets the optional maximum token context budget.
    pub fn with_max_context_tokens(mut self, tokens: Option<usize>) -> Self {
        self.max_context_tokens = tokens;
        self
    }

    /// Sets the model context window token limit.
    pub fn with_context_limit(mut self, tokens: usize) -> Self {
        self.max_context_tokens = Some(tokens);
        self
    }

    /// Sets the context compaction trigger threshold ratio (e.g. 0.8 for 80% watermark).
    pub fn with_compaction_threshold(mut self, threshold: f64) -> Self {
        self.compaction_threshold = threshold.clamp(0.1, 0.99);
        self
    }

    /// Configures the maximum line count and byte size limits for single tool execution outputs.
    pub fn with_max_tool_output_limits(mut self, lines: usize, bytes: usize) -> Self {
        self.max_tool_output_lines = lines;
        self.max_tool_output_bytes = bytes;
        self
    }

    pub fn with_trajectory_logger(mut self, logger: Arc<TrajectoryLogger>) -> Self {
        self.trajectory_logger = Some(logger);
        self
    }

    pub fn with_step_observer(mut self, observer: StepObserver) -> Self {
        self.step_observer = Some(observer);
        self
    }

    pub fn with_continuation(
        mut self,
        continuation: Option<crate::eval::ContinuationCheckpoint>,
    ) -> Self {
        self.continuation = continuation;
        self
    }

    pub fn with_extra_turns(mut self, extra_turns: Option<usize>) -> Self {
        self.extra_turns = extra_turns;
        self
    }

    async fn seed_continuation_workspace(&mut self) {
        let Some(checkpoint) = &self.continuation else {
            return;
        };
        if checkpoint.compacted_summary.trim().is_empty() {
            return;
        }
        let path = Path::new(crate::eval::ContinuationCheckpoint::CONTINUATION_SUMMARY_PATH);
        let _ = self
            .sandbox
            .write_file(path, checkpoint.compacted_summary.as_bytes())
            .await;
    }

    pub fn policy_name(&self) -> &str {
        self.policy.name()
    }

    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn sandbox(&self) -> &S {
        &self.sandbox
    }

    pub fn sandbox_mut(&mut self) -> &mut S {
        &mut self.sandbox
    }

    fn default_tools() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "shell".into(),
                description: "Execute a shell command inside the sandbox workspace".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "The command to run" },
                        "timeout": { "type": "integer", "description": "Optional timeout in seconds for command execution (defaults to 120s or environment override)." }
                    },
                    "required": ["command"]
                }),
            },
            ToolDefinition {
                name: "read_file".into(),
                description: "Read content of a file in the workspace".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to the file" }
                    },
                    "required": ["path"]
                }),
            },
            ToolDefinition {
                name: "write_file".into(),
                description: "Write content to a file in the workspace".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to the file" },
                        "content": { "type": "string", "description": "Content to write" }
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolDefinition {
                name: "list_dir".into(),
                description: "List directory contents in the workspace (compact, 1-level shallow)"
                    .into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative directory path (defaults to root .)" }
                    }
                }),
            },
            ToolDefinition {
                name: "grep_search".into(),
                description:
                    "Search for keyword or pattern in workspace files (capped to 30 matches)".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search pattern or keyword" },
                        "path": { "type": "string", "description": "Optional directory or file path to filter search" }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "consult_advisor".into(),
                description: "Consult an external advisor CLI, subagent, or review assistant for architecture feedback, code review, anti-stagnation guidance, or next-step recommendations. Follow-up calls on the same task resume that advisor's own session so it keeps prior context. Set fresh=true to start an independent session.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "prompt": { "type": "string", "description": "The specific question, progress review, or stagnation problem statement" },
                        "command": { "type": "string", "description": "Optional advisor command to run (e.g. 'agy', 'grok', 'openduck'). If omitted, uses auto-discovery or configured advisor." },
                        "fresh": { "type": "boolean", "description": "If true, start a new advisor session instead of resuming the current task session. Use for an independent second opinion." },
                        "timeout": { "type": "integer", "description": "Optional timeout in seconds for advisor execution (defaults to 180s or configured timeout)." },
                        "https_proxy": { "type": "string", "description": "Optional HTTPS proxy URL (e.g. 'socks5://127.0.0.1:1088') for advisor network requests." },
                        "http_proxy": { "type": "string", "description": "Optional HTTP proxy URL for advisor network requests." },
                        "all_proxy": { "type": "string", "description": "Optional ALL_PROXY URL (e.g. 'socks5://127.0.0.1:1088') for advisor network requests." },
                        "no_proxy": { "type": "string", "description": "Optional NO_PROXY domain list (e.g. 'localhost,127.0.0.1') for advisor network requests." }
                    },
                    "required": ["prompt"]
                }),
            },
        ]
    }

    async fn execute_tool_call(&mut self, call: &ToolCallRequest) -> ToolCallResponse {
        let mut response = match call.name.as_str() {
            "shell" | "bash" => {
                let cmd = call
                    .arguments
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let timeout_secs = call
                    .arguments
                    .get("timeout")
                    .or_else(|| call.arguments.get("timeout_seconds"))
                    .and_then(|v| v.as_u64())
                    .or_else(|| {
                        std::env::var("OPENDUCK_EXEC_TIMEOUT")
                            .or_else(|_| std::env::var("HARNESS_EXEC_TIMEOUT"))
                            .ok()
                            .and_then(|s| s.parse::<u64>().ok())
                    });
                let mut opts = ExecOptions::default();
                if let Some(t) = timeout_secs {
                    opts.timeout_seconds = Some(t);
                }
                match self.sandbox.exec_command(cmd, &opts).await {
                    Ok(out) => ToolCallResponse {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        output: format!(
                            "Exit: {}\nStdout:\n{}\nStderr:\n{}",
                            out.exit_code, out.stdout, out.stderr
                        ),
                        is_error: out.exit_code != 0,
                    },
                    Err(e) => ToolCallResponse {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        output: format!("Execution error: {}", e),
                        is_error: true,
                    },
                }
            }
            "read_file" => {
                let path_str = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                match self.sandbox.read_file(std::path::Path::new(path_str)).await {
                    Ok(bytes) => ToolCallResponse {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        output: String::from_utf8_lossy(&bytes).to_string(),
                        is_error: false,
                    },
                    Err(e) => ToolCallResponse {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        output: format!("Read error: {}", e),
                        is_error: true,
                    },
                }
            }
            "write_file" => {
                let path_str = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let content = call
                    .arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                match self
                    .sandbox
                    .write_file(std::path::Path::new(path_str), content.as_bytes())
                    .await
                {
                    Ok(_) => ToolCallResponse {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        output: "File successfully written".to_string(),
                        is_error: false,
                    },
                    Err(e) => ToolCallResponse {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        output: format!("Write error: {}", e),
                        is_error: true,
                    },
                }
            }
            "list_dir" => {
                let path_str = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                let target = if path_str.is_empty() || path_str == "." {
                    self.sandbox.workspace_root().to_path_buf()
                } else {
                    self.sandbox.workspace_root().join(path_str)
                };
                match std::fs::read_dir(&target) {
                    Ok(entries) => {
                        let mut items = Vec::new();
                        for entry in entries.filter_map(Result::ok) {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.starts_with('.') || name == "target" || name == "node_modules" {
                                continue;
                            }
                            let ft = entry.file_type().ok();
                            if ft.map(|t| t.is_dir()).unwrap_or(false) {
                                items.push(format!("{name}/ [DIR]"));
                            } else {
                                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                                items.push(format!("{name} ({size} B)"));
                            }
                        }
                        items.sort();
                        let formatted = if items.is_empty() {
                            "Directory is empty".to_string()
                        } else {
                            items.into_iter().take(50).collect::<Vec<_>>().join("\n")
                        };
                        ToolCallResponse {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            output: formatted,
                            is_error: false,
                        }
                    }
                    Err(e) => ToolCallResponse {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        output: format!("Failed to list directory '{path_str}': {e}"),
                        is_error: true,
                    },
                }
            }
            "grep_search" => {
                let query = call
                    .arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let filter_path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                let escaped_query = query.replace('"', "\\\"");
                let cmd = format!(
                    "grep -rnI -m 30 \"{escaped_query}\" {filter_path} 2>/dev/null | head -n 30"
                );
                let opts = ExecOptions::default();
                match self.sandbox.exec_command(&cmd, &opts).await {
                    Ok(out) => {
                        let output_text = if out.stdout.trim().is_empty() {
                            "No matches found".to_string()
                        } else {
                            out.stdout
                        };
                        ToolCallResponse {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            output: output_text,
                            is_error: false,
                        }
                    }
                    Err(e) => ToolCallResponse {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        output: format!("Grep search failed: {e}"),
                        is_error: true,
                    },
                }
            }
            "consult_advisor" | "consult_expert" => self.consult_advisor(call).await,
            other => ToolCallResponse {
                id: call.id.clone(),
                name: call.name.clone(),
                output: format!("Unknown tool: {}", other),
                is_error: true,
            },
        };
        response.output = truncate_tool_output(response.output);
        response
    }

    fn advisor_session_id(&self, bin: &str) -> Option<&str> {
        self.advisor_sessions
            .get(&self.current_advisor_scope)
            .and_then(|sessions| sessions.get(bin))
            .map(String::as_str)
    }

    fn set_advisor_session_id(&mut self, bin: &str, session_id: String) {
        self.advisor_sessions
            .entry(self.current_advisor_scope.clone())
            .or_default()
            .insert(bin.to_string(), session_id);
    }

    async fn consult_advisor(&mut self, call: &ToolCallRequest) -> ToolCallResponse {
        let prompt = call
            .arguments
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim();

        if prompt.is_empty() {
            return ToolCallResponse {
                id: call.id.clone(),
                name: call.name.clone(),
                output: "Error: 'prompt' parameter is required for advisor consultation."
                    .to_string(),
                is_error: true,
            };
        }

        let fresh = json_truthy(call.arguments.get("fresh"));
        let explicit_cmd = call
            .arguments
            .get("command")
            .or_else(|| call.arguments.get("advisor"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && *s != "auto");

        let target_cmd = explicit_cmd
            .map(ToOwned::to_owned)
            .or_else(|| self.advisor_command.clone());

        let (cmd_to_run, advisor_kind, advisor_bin, planned_session) = if let Some(custom) =
            target_cmd
        {
            let bin = custom
                .split_whitespace()
                .next()
                .unwrap_or(&custom)
                .to_string();
            let check_opts = ExecOptions::default();
            let check_res = self
                .sandbox
                .exec_command(&format!("command -v {bin}"), &check_opts)
                .await;
            if !check_res.map(|o| o.exit_code == 0).unwrap_or(false) {
                return ToolCallResponse {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    output: format!(
                        "Advisor Notice: Command '{bin}' is not available in the current environment PATH.\nSelf-Review Guidance:\n- Review recent file diffs and tool execution outcomes.\n- Confirm whether target files have been identified.\n- Proceed with making code modifications using 'write_file' or verifying with test commands."
                    ),
                    is_error: false,
                };
            }

            if custom.contains(' ') {
                (
                    format!("{custom} {}", shell_quote(prompt)),
                    None,
                    bin.to_lowercase(),
                    None,
                )
            } else if let Some(kind) = AdvisorKind::from_bin(&bin) {
                if fresh {
                    self.advisor_sessions
                        .entry(self.current_advisor_scope.clone())
                        .or_default()
                        .remove(&bin.to_lowercase());
                }
                let resume = if fresh {
                    None
                } else {
                    self.advisor_session_id(&bin.to_lowercase())
                        .map(ToOwned::to_owned)
                };
                let invocation = build_advisor_command(kind, prompt, resume.as_deref());
                (
                    invocation.cmdline,
                    Some(kind),
                    bin.to_lowercase(),
                    invocation.session_id,
                )
            } else {
                (
                    format!("{custom} {}", shell_quote(prompt)),
                    None,
                    bin.to_lowercase(),
                    None,
                )
            }
        } else {
            let candidates = ["agy", "grok", "openduck"];
            let mut found = None;
            for candidate in candidates {
                let check_opts = ExecOptions::default();
                let check_res = self
                    .sandbox
                    .exec_command(&format!("command -v {candidate}"), &check_opts)
                    .await;
                if check_res.map(|o| o.exit_code == 0).unwrap_or(false) {
                    found = Some(candidate);
                    break;
                }
            }

            if let Some(candidate) = found {
                let kind = match candidate {
                    "agy" => AdvisorKind::Agy,
                    "grok" => AdvisorKind::Grok,
                    _ => AdvisorKind::Openduck,
                };
                if fresh {
                    self.advisor_sessions
                        .entry(self.current_advisor_scope.clone())
                        .or_default()
                        .remove(candidate);
                }
                let resume = if fresh {
                    None
                } else {
                    self.advisor_session_id(candidate).map(ToOwned::to_owned)
                };
                let invocation = build_advisor_command(kind, prompt, resume.as_deref());
                (
                    invocation.cmdline,
                    Some(kind),
                    candidate.to_string(),
                    invocation.session_id,
                )
            } else {
                return ToolCallResponse {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        output: "Advisor Notice: No external advisor CLI ('agy', 'grok', 'openduck') is configured or installed in PATH.\nSelf-Review Guidance:\n- Check your current progress against the overall goal.\n- Verify any modifications made so far.\n- Proceed with your implementation plan directly.".to_string(),
                        is_error: false,
                    };
            }
        };

        if let Some(session_id) = planned_session.clone() {
            self.set_advisor_session_id(&advisor_bin, session_id);
        }

        let timeout_secs = call
            .arguments
            .get("timeout")
            .or_else(|| call.arguments.get("timeout_seconds"))
            .and_then(|v| v.as_u64())
            .or(self.advisor_timeout_seconds)
            .or_else(|| {
                std::env::var("OPENDUCK_ADVISOR_TIMEOUT")
                    .or_else(|_| std::env::var("HARNESS_ADVISOR_TIMEOUT"))
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .unwrap_or(300);

        let bin_name = cmd_to_run
            .split_whitespace()
            .next()
            .unwrap_or("advisor")
            .to_lowercase();
        let bin_upper = bin_name.to_uppercase();

        let https_proxy = call
            .arguments
            .get("https_proxy")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .or_else(|| {
                self.advisor_proxies
                    .get(&bin_name)
                    .and_then(|p| p.https_proxy.clone())
            })
            .or_else(|| {
                self.advisor_proxies
                    .get("default")
                    .and_then(|p| p.https_proxy.clone())
            })
            .or_else(|| self.advisor_proxy.https_proxy.clone())
            .or_else(|| {
                let bin_env = format!("OPENDUCK_ADVISOR_{bin_upper}_HTTPS_PROXY");
                let legacy_bin_env = format!("ADVISOR_{bin_upper}_HTTPS_PROXY");
                std::env::var(&bin_env)
                    .or_else(|_| std::env::var(&legacy_bin_env))
                    .or_else(|_| std::env::var("OPENDUCK_ADVISOR_HTTPS_PROXY"))
                    .or_else(|_| std::env::var("ADVISOR_HTTPS_PROXY"))
                    .or_else(|_| std::env::var("HTTPS_PROXY"))
                    .or_else(|_| std::env::var("https_proxy"))
                    .or_else(|_| std::env::var("OPENDUCK_ADVISOR_PROXY"))
                    .or_else(|_| std::env::var("ADVISOR_PROXY"))
                    .ok()
            });

        let http_proxy = call
            .arguments
            .get("http_proxy")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .or_else(|| {
                self.advisor_proxies
                    .get(&bin_name)
                    .and_then(|p| p.http_proxy.clone())
            })
            .or_else(|| {
                self.advisor_proxies
                    .get("default")
                    .and_then(|p| p.http_proxy.clone())
            })
            .or_else(|| self.advisor_proxy.http_proxy.clone())
            .or_else(|| {
                let bin_env = format!("OPENDUCK_ADVISOR_{bin_upper}_HTTP_PROXY");
                let legacy_bin_env = format!("ADVISOR_{bin_upper}_HTTP_PROXY");
                std::env::var(&bin_env)
                    .or_else(|_| std::env::var(&legacy_bin_env))
                    .or_else(|_| std::env::var("OPENDUCK_ADVISOR_HTTP_PROXY"))
                    .or_else(|_| std::env::var("ADVISOR_HTTP_PROXY"))
                    .or_else(|_| std::env::var("HTTP_PROXY"))
                    .or_else(|_| std::env::var("http_proxy"))
                    .or_else(|_| std::env::var("OPENDUCK_ADVISOR_PROXY"))
                    .or_else(|_| std::env::var("ADVISOR_PROXY"))
                    .ok()
            });

        let all_proxy = call
            .arguments
            .get("all_proxy")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .or_else(|| {
                self.advisor_proxies
                    .get(&bin_name)
                    .and_then(|p| p.all_proxy.clone())
            })
            .or_else(|| {
                self.advisor_proxies
                    .get("default")
                    .and_then(|p| p.all_proxy.clone())
            })
            .or_else(|| self.advisor_proxy.all_proxy.clone())
            .or_else(|| {
                let bin_env = format!("OPENDUCK_ADVISOR_{bin_upper}_ALL_PROXY");
                let legacy_bin_env = format!("ADVISOR_{bin_upper}_ALL_PROXY");
                std::env::var(&bin_env)
                    .or_else(|_| std::env::var(&legacy_bin_env))
                    .or_else(|_| std::env::var("OPENDUCK_ADVISOR_ALL_PROXY"))
                    .or_else(|_| std::env::var("ADVISOR_ALL_PROXY"))
                    .or_else(|_| std::env::var("ALL_PROXY"))
                    .or_else(|_| std::env::var("all_proxy"))
                    .or_else(|_| std::env::var("OPENDUCK_ADVISOR_PROXY"))
                    .or_else(|_| std::env::var("ADVISOR_PROXY"))
                    .ok()
            });

        let no_proxy = call
            .arguments
            .get("no_proxy")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .or_else(|| {
                self.advisor_proxies
                    .get(&bin_name)
                    .and_then(|p| p.no_proxy.clone())
            })
            .or_else(|| {
                self.advisor_proxies
                    .get("default")
                    .and_then(|p| p.no_proxy.clone())
            })
            .or_else(|| self.advisor_proxy.no_proxy.clone())
            .or_else(|| {
                let bin_env = format!("OPENDUCK_ADVISOR_{bin_upper}_NO_PROXY");
                let legacy_bin_env = format!("ADVISOR_{bin_upper}_NO_PROXY");
                std::env::var(&bin_env)
                    .or_else(|_| std::env::var(&legacy_bin_env))
                    .or_else(|_| std::env::var("OPENDUCK_ADVISOR_NO_PROXY"))
                    .or_else(|_| std::env::var("ADVISOR_NO_PROXY"))
                    .or_else(|_| std::env::var("NO_PROXY"))
                    .or_else(|_| std::env::var("no_proxy"))
                    .ok()
            });

        let mut exec_env = HashMap::new();
        if let Some(p) = https_proxy {
            exec_env.insert("https_proxy".to_string(), p.clone());
            exec_env.insert("HTTPS_PROXY".to_string(), p);
        }
        if let Some(p) = http_proxy {
            exec_env.insert("http_proxy".to_string(), p.clone());
            exec_env.insert("HTTP_PROXY".to_string(), p);
        }
        if let Some(p) = all_proxy {
            exec_env.insert("all_proxy".to_string(), p.clone());
            exec_env.insert("ALL_PROXY".to_string(), p);
        }
        if let Some(p) = no_proxy {
            exec_env.insert("no_proxy".to_string(), p.clone());
            exec_env.insert("NO_PROXY".to_string(), p);
        }

        let exec_opts = ExecOptions {
            env: exec_env,
            timeout_seconds: Some(timeout_secs),
            ..Default::default()
        };
        match self.sandbox.exec_command(&cmd_to_run, &exec_opts).await {
            Ok(out) if out.exit_code == 0 && !out.stdout.trim().is_empty() => {
                let visible = if let Some(kind) = advisor_kind {
                    if let Some(parsed) = parse_advisor_session_id(kind, &out.stdout) {
                        self.set_advisor_session_id(&advisor_bin, parsed);
                    }
                    advisor_visible_output(kind, &out.stdout)
                } else {
                    out.stdout.trim().to_string()
                };
                ToolCallResponse {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    output: format!("Advisor Consultation Output:\n{visible}"),
                    is_error: false,
                }
            }
            Ok(out) => {
                if let Some(kind) = advisor_kind {
                    if let Some(parsed) = parse_advisor_session_id(kind, &out.stdout) {
                        self.set_advisor_session_id(&advisor_bin, parsed);
                    }
                }
                let raw = if !out.stderr.trim().is_empty() {
                    out.stderr.trim()
                } else if !out.stdout.trim().is_empty() {
                    out.stdout.trim()
                } else {
                    "Advisor finished with exit code 0."
                };
                let msg = if let Some(kind) = advisor_kind {
                    let extracted = advisor_visible_output(kind, raw);
                    if extracted.is_empty() {
                        raw.to_string()
                    } else {
                        extracted
                    }
                } else {
                    raw.to_string()
                };
                ToolCallResponse {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    output: format!(
                        "Advisor Response Note:\n{msg}\n(Proceed with your planned implementation)."
                    ),
                    is_error: false,
                }
            }
            Err(e) => ToolCallResponse {
                id: call.id.clone(),
                name: call.name.clone(),
                output: format!(
                    "Advisor Execution Note:\nExecution failed ({e}). Proceed with your implementation directly."
                ),
                is_error: false,
            },
        }
    }

    fn notify_step_observer(&self, steps: &[TrajectoryStep]) {
        if let Some(observer) = &self.step_observer {
            observer(steps);
        }
    }

    fn notify_step_observer_with_prefix(
        &self,
        prefix: &[TrajectoryStep],
        steps: &[TrajectoryStep],
    ) {
        if prefix.is_empty() {
            self.notify_step_observer(steps);
            return;
        }
        let mut combined = Vec::with_capacity(prefix.len() + steps.len());
        combined.extend_from_slice(prefix);
        combined.extend_from_slice(steps);
        self.notify_step_observer(&combined);
    }

    pub async fn run_task(&mut self, task: &TaskSpec) -> Result<TaskExecutionResult> {
        let start_time = Instant::now();
        let started_at = Utc::now();
        let session_id = format!("harness-sess-{}", Uuid::new_v4());
        self.advisor_sessions = self
            .continuation
            .as_ref()
            .map(|checkpoint| checkpoint.advisor_sessions.clone())
            .unwrap_or_default();
        self.current_advisor_scope = task.id.clone();

        self.sandbox.initialize().await?;

        for setup_cmd in &task.environment.setup_commands {
            let opts = ExecOptions::default();
            let out = self.sandbox.exec_command(setup_cmd, &opts).await?;
            if out.exit_code != 0 {
                return Err(anyhow!(
                    "Environment setup failed for command '{}': {}",
                    setup_cmd,
                    out.stderr
                ));
            }
        }

        // Step 1: Decompose task into subtasks if explicitly configured or complex
        let subtasks = if let Some(plan) = self
            .continuation
            .as_ref()
            .filter(|cp| !cp.subtask_plan.is_empty())
            .map(|cp| cp.subtask_plan.clone())
        {
            plan
        } else if let Some(configured) = &task.subtasks {
            configured.clone()
        } else if task.auto_decompose.unwrap_or(true) {
            crate::eval::subtask::SubtaskDecomposer::decompose(
                &task.id,
                &task.problem_statement,
                Some(task.max_turns.unwrap_or(self.max_turns)),
                task.phase_max_turns.as_deref(),
            )
        } else {
            Vec::new()
        };

        if subtasks.len() >= 2 {
            self.run_subtasks_sequentially(task, &subtasks, &session_id, started_at, start_time)
                .await
        } else {
            self.run_single_task_session(task, &session_id, started_at, start_time)
                .await
        }
    }

    async fn run_single_task_session(
        &mut self,
        task: &TaskSpec,
        session_id: &str,
        started_at: chrono::DateTime<Utc>,
        start_time: Instant,
    ) -> Result<TaskExecutionResult> {
        let effective_budget = task
            .exploration_budget
            .or(self.exploration_budget)
            .unwrap_or(self.stagnation_threshold);

        let mut initial_user_prompt = if let Some(repo_map) =
            crate::repomap::generate_workspace_repo_map(self.sandbox.workspace_root())
        {
            format!("{}\n## Task Goal:\n{}", repo_map, task.problem_statement)
        } else {
            task.problem_statement.clone()
        };
        self.seed_continuation_workspace().await;
        if let Some(checkpoint) = &self.continuation {
            initial_user_prompt = format!(
                "{}\n\n{}",
                checkpoint.prior_progress_prompt(),
                initial_user_prompt
            );
        }

        let max_steps = task
            .max_turns
            .unwrap_or(self.max_turns)
            .saturating_add(self.extra_turns.unwrap_or(0));
        let (status, final_answer, steps, total_tool_calls, _modified) = self
            .run_session_loop(
                session_id,
                &task.id,
                &initial_user_prompt,
                max_steps,
                effective_budget,
                &[],
            )
            .await?;

        let duration_ms = start_time.elapsed().as_millis();
        let trajectory = TrajectoryRecord {
            session_id: session_id.to_string(),
            task_id: task.id.clone(),
            policy_name: self.policy.name().to_string(),
            started_at,
            completed_at: Some(Utc::now()),
            success: status == RunStatus::Success,
            steps,
            system_prompt: self.system_prompt.clone(),
            active_rules: self.active_rules.clone(),
        };

        if let Some(logger) = &self.trajectory_logger {
            let _ = logger.log_trajectory(&trajectory).await;
        }

        let mut continuation = build_continuation_checkpoint(
            task,
            session_id,
            status,
            final_answer.as_deref(),
            &trajectory.steps,
            Vec::new(),
            Vec::new(),
        );
        continuation.advisor_sessions = self.advisor_sessions.clone();
        let continuation = Some(continuation);

        Ok(TaskExecutionResult {
            task_id: task.id.clone(),
            status,
            step_count: trajectory.steps.len(),
            tool_calls_count: total_tool_calls,
            duration_ms,
            final_answer,
            trajectory,
            continuation,
        })
    }

    async fn run_subtasks_sequentially(
        &mut self,
        task: &TaskSpec,
        subtasks: &[crate::eval::task::SubtaskSpec],
        session_id: &str,
        started_at: chrono::DateTime<Utc>,
        start_time: Instant,
    ) -> Result<TaskExecutionResult> {
        self.seed_continuation_workspace().await;
        let resume_idx = self
            .continuation
            .as_ref()
            .map(|cp| cp.resume_index(subtasks))
            .unwrap_or(0);
        let mut previous_outcomes: Vec<crate::eval::subtask::SubtaskOutcome> = self
            .continuation
            .as_ref()
            .map(|cp| {
                cp.completed_subtasks
                    .iter()
                    .filter(|o| o.status == RunStatus::Success)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut cumulative_steps: Vec<TrajectoryStep> = Vec::new();
        let mut total_tool_calls = 0;
        let mut final_answers = Vec::new();
        let mut overall_status = RunStatus::Success;

        for (subtask_idx, subtask) in subtasks.iter().enumerate() {
            if subtask_idx < resume_idx {
                continue;
            }

            let mut subtask_prompt = crate::eval::subtask::SubtaskDecomposer::format_subtask_prompt(
                task,
                subtask,
                subtask_idx,
                subtasks.len(),
                &previous_outcomes,
            );
            if subtask_idx == resume_idx {
                if let Some(checkpoint) = &self.continuation {
                    subtask_prompt =
                        format!("{}\n{}", checkpoint.prior_progress_prompt(), subtask_prompt);
                }
            }

            let mut subtask_max_turns = subtask.max_turns.unwrap_or_else(|| {
                let default_turns = task.max_turns.unwrap_or(self.max_turns);
                (default_turns / subtasks.len()).max(8)
            });
            if subtask_idx == resume_idx {
                subtask_max_turns = subtask_max_turns.saturating_add(self.extra_turns.unwrap_or(0));
            }

            let subtask_exploration_budget = subtask.exploration_budget.unwrap_or_else(|| {
                task.exploration_budget
                    .or(self.exploration_budget)
                    .unwrap_or(3)
                    .min(4)
            });

            let (subtask_status, subtask_answer, subtask_steps, subtask_tool_calls, modified_files) =
                self.run_session_loop(
                    session_id,
                    &subtask.id,
                    &subtask_prompt,
                    subtask_max_turns,
                    subtask_exploration_budget,
                    &cumulative_steps,
                )
                .await?;

            total_tool_calls += subtask_tool_calls;
            let steps_taken = subtask_steps.len();
            cumulative_steps.extend(subtask_steps);

            let outcome_summary = subtask_answer.clone().unwrap_or_else(|| {
                if subtask_status == RunStatus::Success {
                    "Completed".to_string()
                } else {
                    format!("Finished with status: {:?}", subtask_status)
                }
            });

            if let Some(ref ans) = subtask_answer {
                final_answers.push(format!(
                    "### Subtask {} ({}): {}\n{}",
                    subtask_idx + 1,
                    subtask.id,
                    subtask.title,
                    ans
                ));
            }

            previous_outcomes.push(crate::eval::subtask::SubtaskOutcome {
                subtask_id: subtask.id.clone(),
                title: subtask.title.clone(),
                status: subtask_status,
                modified_files: modified_files.into_iter().collect(),
                summary: outcome_summary,
                step_count: cumulative_steps.len(),
                steps_taken,
            });

            if subtask_status != RunStatus::Success {
                overall_status = subtask_status;
                if matches!(subtask_status, RunStatus::Cancelled | RunStatus::Failure) {
                    break;
                }
            }
        }

        let duration_ms = start_time.elapsed().as_millis();
        let trajectory = TrajectoryRecord {
            session_id: session_id.to_string(),
            task_id: task.id.clone(),
            policy_name: self.policy.name().to_string(),
            started_at,
            completed_at: Some(Utc::now()),
            success: overall_status == RunStatus::Success,
            steps: cumulative_steps,
            system_prompt: self.system_prompt.clone(),
            active_rules: self.active_rules.clone(),
        };

        if let Some(logger) = &self.trajectory_logger {
            let _ = logger.log_trajectory(&trajectory).await;
        }

        let final_combined_answer = if final_answers.is_empty() {
            None
        } else {
            Some(final_answers.join("\n\n"))
        };

        let mut continuation = build_continuation_checkpoint(
            task,
            session_id,
            overall_status,
            final_combined_answer.as_deref(),
            &trajectory.steps,
            previous_outcomes,
            subtasks.to_vec(),
        );
        continuation.advisor_sessions = self.advisor_sessions.clone();
        let continuation = Some(continuation);

        Ok(TaskExecutionResult {
            task_id: task.id.clone(),
            status: overall_status,
            step_count: trajectory.steps.len(),
            tool_calls_count: total_tool_calls,
            duration_ms,
            final_answer: final_combined_answer,
            trajectory,
            continuation,
        })
    }

    async fn run_session_loop(
        &mut self,
        session_id: &str,
        step_task_id: &str,
        user_prompt: &str,
        max_steps: usize,
        _effective_budget: usize,
        step_prefix: &[TrajectoryStep],
    ) -> Result<(
        RunStatus,
        Option<String>,
        Vec<TrajectoryStep>,
        usize,
        BTreeSet<String>,
    )> {
        self.current_advisor_scope = step_task_id.to_string();
        let mut context =
            HarnessContextView::new(session_id, self.sandbox.workspace_root().to_path_buf());
        if let Some(custom) = &self.system_prompt {
            context
                .messages
                .push(HarnessMessage::system(custom.clone()));
        }

        context
            .messages
            .push(HarnessMessage::user(user_prompt.to_string()));

        let mut steps = Vec::new();
        let mut total_tool_calls = 0;
        let mut final_answer = None;
        let mut status = RunStatus::Success;
        let mut modified_files: BTreeSet<String> = BTreeSet::new();

        for step_idx in 0..max_steps {
            context.step_count = step_idx + 1;
            let step_start = Instant::now();

            let (action, step_telemetry) =
                match self.policy.step_detailed(&context, &self.tools).await {
                    Ok(res) => res,
                    Err(e) => {
                        let err_str = format!("{e:#}");
                        tracing::error!(
                            step = step_idx + 1,
                            policy = %self.policy.name(),
                            error = %err_str,
                            "Harness policy step failed"
                        );
                        status = RunStatus::Failure;
                        final_answer = Some(format!(
                            "Policy step {} failed for task '{}': {err_str}",
                            step_idx + 1,
                            step_task_id
                        ));
                        steps.push(TrajectoryStep {
                            step_number: step_prefix.len() + step_idx + 1,
                            timestamp: Utc::now(),
                            action: AgentAction::YieldControl {
                                reason: YieldReason::Interrupted,
                            },
                            tool_results: None,
                            duration_ms: step_start.elapsed().as_millis(),
                            token_usage: None,
                            llm_request: Some(serde_json::json!({
                                "stepIndex": step_idx + 1,
                                "messagesCount": context.messages.len(),
                            })),
                            llm_response: Some(serde_json::json!({
                                "policyError": err_str,
                            })),
                            judgments: None,
                        });
                        self.notify_step_observer_with_prefix(step_prefix, &steps);
                        break;
                    }
                };

            let (req_payload, res_payload, token_usage_payload) = if let Some(t) = step_telemetry {
                (t.llm_request, t.llm_response, t.token_usage)
            } else {
                (None, None, None)
            };

            let step_duration = step_start.elapsed().as_millis();

            match &action {
                AgentAction::FinalAnswer(ans) => {
                    let completion_state = format!(
                        "Task Goal: {}\nProposed Final Answer:\n{}\nTurns Elapsed: {}",
                        user_prompt,
                        ans,
                        step_idx + 1
                    );
                    let judgment = self.judge.evaluate_completion(&completion_state).await;
                    let mode = self
                        .judge
                        .mode_for(&crate::judge::DecisionPointId::TurnCompletion);

                    let llm_request = req_payload.unwrap_or_else(|| {
                        serde_json::json!({
                            "stepIndex": step_idx + 1,
                            "messagesCount": context.messages.len(),
                            "lastMessage": context.messages.last().map(|m| &m.content)
                        })
                    });
                    let llm_response = res_payload.unwrap_or_else(|| {
                        serde_json::json!({
                            "finalAnswer": ans
                        })
                    });

                    let judgments = if judgment.mode.is_enabled() {
                        Some(vec![judgment.clone()])
                    } else {
                        None
                    };

                    if mode.is_active() && judgment.is_triggered() && step_idx + 1 < max_steps {
                        tracing::warn!(
                            step = step_idx + 1,
                            "Laya judge rejected completion in active mode: verification required"
                        );
                        context
                            .messages
                            .push(HarnessMessage::assistant(ans.clone()));
                        context.messages.push(HarnessMessage::system(
                            "Verification required before declaring completion. Run tests, compiler checks, or inspect diffs to confirm changes work as intended."
                        ));
                        steps.push(TrajectoryStep {
                            step_number: step_idx + 1,
                            timestamp: Utc::now(),
                            action: action.clone(),
                            tool_results: None,
                            duration_ms: step_duration,
                            token_usage: token_usage_payload,
                            llm_request: Some(llm_request),
                            llm_response: Some(llm_response),
                            judgments,
                        });
                        self.notify_step_observer_with_prefix(step_prefix, &steps);
                        continue;
                    }

                    final_answer = Some(ans.clone());
                    context
                        .messages
                        .push(HarnessMessage::assistant(ans.clone()));
                    steps.push(TrajectoryStep {
                        step_number: step_idx + 1,
                        timestamp: Utc::now(),
                        action: action.clone(),
                        tool_results: None,
                        duration_ms: step_duration,
                        token_usage: token_usage_payload,
                        llm_request: Some(llm_request),
                        llm_response: Some(llm_response),
                        judgments,
                    });
                    self.notify_step_observer_with_prefix(step_prefix, &steps);
                    break;
                }
                AgentAction::YieldControl { reason } => {
                    let llm_request = req_payload.unwrap_or_else(|| {
                        serde_json::json!({
                            "stepIndex": step_idx + 1,
                            "messagesCount": context.messages.len(),
                            "lastMessage": context.messages.last().map(|m| &m.content)
                        })
                    });
                    let llm_response = res_payload.unwrap_or_else(|| {
                        serde_json::json!({
                            "yieldControl": format!("{:?}", reason)
                        })
                    });
                    steps.push(TrajectoryStep {
                        step_number: step_idx + 1,
                        timestamp: Utc::now(),
                        action: action.clone(),
                        tool_results: None,
                        duration_ms: step_duration,
                        token_usage: token_usage_payload,
                        llm_request: Some(llm_request),
                        llm_response: Some(llm_response),
                        judgments: None,
                    });
                    self.notify_step_observer_with_prefix(step_prefix, &steps);
                    break;
                }
                AgentAction::RequestInput { prompt } => {
                    context
                        .messages
                        .push(HarnessMessage::assistant(prompt.clone()));
                    let llm_request = req_payload.unwrap_or_else(|| {
                        serde_json::json!({
                            "stepIndex": step_idx + 1,
                            "messagesCount": context.messages.len(),
                            "lastMessage": context.messages.last().map(|m| &m.content)
                        })
                    });
                    let llm_response = res_payload.unwrap_or_else(|| {
                        serde_json::json!({
                            "requestInputPrompt": prompt
                        })
                    });
                    steps.push(TrajectoryStep {
                        step_number: step_idx + 1,
                        timestamp: Utc::now(),
                        action: action.clone(),
                        tool_results: None,
                        duration_ms: step_duration,
                        token_usage: token_usage_payload,
                        llm_request: Some(llm_request),
                        llm_response: Some(llm_response),
                        judgments: None,
                    });
                    self.notify_step_observer_with_prefix(step_prefix, &steps);
                    break;
                }
                AgentAction::CallTools(calls) => {
                    total_tool_calls += calls.len();
                    context
                        .messages
                        .push(HarnessMessage::assistant_with_tools("", calls.clone()));

                    let mut responses = Vec::new();
                    for call in calls {
                        let is_mod = is_modifying_call(call);
                        if is_mod {
                            if let Some(target) = extract_read_target_with_root(
                                call,
                                Some(self.sandbox.workspace_root()),
                            ) {
                                if !is_scratch_or_note_path(&target) {
                                    modified_files.insert(target);
                                }
                            }
                        }
                        let mut res = self.execute_tool_call(call).await;

                        let (snipped_output, was_snipped) = snip_tool_output(
                            &res.output,
                            self.max_tool_output_lines,
                            self.max_tool_output_bytes,
                        );
                        if was_snipped {
                            res.output = snipped_output;
                        }

                        responses.push(res);
                    }

                    context
                        .messages
                        .push(HarnessMessage::tool_response(responses.clone()));

                    compact_context_messages(
                        &mut context.messages,
                        self.max_context_messages,
                        self.max_context_tokens,
                        self.compaction_threshold,
                    );

                    let current_tokens = context.total_estimated_tokens();
                    let token_usage =
                        token_usage_payload.or(Some(crate::telemetry::TokenUsageSummary {
                            input_tokens: Some(current_tokens as i32),
                            output_tokens: None,
                            total_tokens: Some(current_tokens as i32),
                        }));

                    let llm_request = req_payload.unwrap_or_else(|| {
                        serde_json::json!({
                            "stepIndex": step_idx + 1,
                            "messagesCount": context.messages.len(),
                            "estimatedTokens": current_tokens,
                            "lastMessage": context.messages.last().map(|m| &m.content)
                        })
                    });

                    let llm_response = res_payload.unwrap_or_else(|| {
                        serde_json::json!({
                            "toolCallsCount": calls.len(),
                            "toolCalls": calls
                        })
                    });

                    let mut step_judgments = Vec::new();
                    let drift_mode = self
                        .judge
                        .mode_for(&crate::judge::DecisionPointId::TurnDrift);
                    if drift_mode.is_enabled() {
                        let drift_state = format!(
                            "Task: {}\nStep: {}\nTools Invoked: {:?}\nTotal Tool Calls: {}",
                            user_prompt,
                            step_idx + 1,
                            calls.iter().map(|c| &c.name).collect::<Vec<_>>(),
                            total_tool_calls
                        );
                        let drift_judgment = self.judge.evaluate_drift(&drift_state).await;
                        if drift_mode.is_active() && drift_judgment.is_triggered() {
                            tracing::warn!(
                                step = step_idx + 1,
                                "Laya judge detected stagnation/drift in active mode"
                            );
                            context.messages.push(HarnessMessage::system(
                                "Warning: You appear to be stuck in an unproductive or repetitive loop. Stop repeating the same commands, inspect the actual error output, or switch your approach."
                            ));
                        }
                        step_judgments.push(drift_judgment);
                    }

                    steps.push(TrajectoryStep {
                        step_number: step_idx + 1,
                        timestamp: Utc::now(),
                        action: action.clone(),
                        tool_results: Some(responses),
                        duration_ms: step_duration,
                        token_usage,
                        llm_request: Some(llm_request),
                        llm_response: Some(llm_response),
                        judgments: if step_judgments.is_empty() {
                            None
                        } else {
                            Some(step_judgments)
                        },
                    });
                    self.notify_step_observer_with_prefix(step_prefix, &steps);
                }
            }

            if step_idx == max_steps - 1 {
                status = RunStatus::Cancelled;
            }
        }

        Ok((
            status,
            final_answer,
            steps,
            total_tool_calls,
            modified_files,
        ))
    }
}

#[derive(Debug, Clone, Default)]
pub struct RepetitiveCallDetector {
    recent_calls: std::collections::VecDeque<String>,
    max_history: usize,
}

impl RepetitiveCallDetector {
    pub fn new(max_history: usize) -> Self {
        Self {
            recent_calls: std::collections::VecDeque::new(),
            max_history,
        }
    }

    pub fn fingerprint(call: &ToolCallRequest) -> String {
        match call.name.as_str() {
            "grep_search" => {
                let q = call
                    .arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let p = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                format!("grep_search:{q}:{p}")
            }
            "read_file" | "view_file" | "read_file_content" => {
                let p = extract_read_target(call).unwrap_or_default();
                format!("read_file:{p}")
            }
            "list_dir" | "list_directory" => {
                let p = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .trim();
                format!("list_dir:{p}")
            }
            "shell" | "bash" => {
                let cmd = call
                    .arguments
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if let Some(target) = extract_read_target_from_shell(cmd) {
                    format!("shell_read:{target}")
                } else {
                    format!("shell:{cmd}")
                }
            }
            "consult_advisor" | "consult_expert" => {
                let p = call
                    .arguments
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                format!("consult_advisor:{p}")
            }
            other => format!("{other}:{}", call.arguments),
        }
    }

    pub fn record_and_count(&mut self, call: &ToolCallRequest) -> usize {
        let fp = Self::fingerprint(call);
        let count = self.recent_calls.iter().filter(|&x| x == &fp).count() + 1;
        self.recent_calls.push_back(fp);
        if self.recent_calls.len() > self.max_history {
            self.recent_calls.pop_front();
        }
        count
    }
}

/// Snips oversized tool outputs (by line count and/or byte size) to preserve context headroom.
///
/// Retains both the head and tail portions of the output while inserting an omission notice
/// with exact count of truncated lines/bytes. Guaranteed to be UTF-8 character boundary safe.
#[allow(clippy::string_slice)]
pub fn snip_tool_output(output: &str, max_lines: usize, max_bytes: usize) -> (String, bool) {
    let bytes_len = output.len();
    let lines: Vec<&str> = output.lines().collect();
    let line_count = lines.len();

    let needs_line_snip = max_lines > 0 && line_count > max_lines;
    let needs_byte_snip = max_bytes > 0 && bytes_len > max_bytes;

    if !needs_line_snip && !needs_byte_snip {
        return (output.to_string(), false);
    }

    let keep_head_lines = (max_lines / 2).max(1);
    let keep_tail_lines = (max_lines.saturating_sub(keep_head_lines)).max(1);

    if needs_line_snip && line_count > (keep_head_lines + keep_tail_lines) {
        let head = lines[..keep_head_lines].join("\n");
        let tail = lines[line_count - keep_tail_lines..].join("\n");
        let omitted_lines = line_count - (keep_head_lines + keep_tail_lines);
        let snipped = format!(
            "{head}\n\n[... {omitted_lines} lines omitted to preserve context headroom ...]\n\n{tail}"
        );
        if max_bytes == 0 || snipped.len() <= max_bytes {
            return (snipped, true);
        }
    }

    if max_bytes > 0 && bytes_len > max_bytes {
        let target_half = (max_bytes / 2).max(64);
        let head_end = find_valid_char_boundary(output, target_half.min(bytes_len));
        let tail_start_target = bytes_len.saturating_sub(target_half);
        let tail_start = find_valid_char_boundary(output, tail_start_target);

        if head_end < tail_start {
            let head_str = &output[..head_end];
            let tail_str = &output[tail_start..];
            let omitted_bytes = tail_start.saturating_sub(head_end);
            let snipped = format!(
                "{head_str}\n\n[... {omitted_bytes} bytes omitted to preserve context headroom ...]\n\n{tail_str}"
            );
            return (snipped, true);
        }
    }

    (output.to_string(), false)
}

fn find_valid_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

/// Fast token estimator for a single harness message.
pub fn estimate_message_tokens(msg: &HarnessMessage) -> usize {
    msg.estimate_tokens()
}

/// Fast aggregate token estimator for a sequence of harness messages.
pub fn estimate_context_tokens(messages: &[HarnessMessage]) -> usize {
    messages.iter().map(|m| m.estimate_tokens()).sum()
}

/// Structured execution facts extracted from a history window during compaction.
#[derive(Debug, Default, Clone)]
pub struct ExtractedContextFacts {
    pub explored_files: BTreeSet<String>,
    pub modified_files: BTreeSet<String>,
    pub retained_notes: Vec<(String, String)>,
    pub executed_commands: Vec<String>,
    pub test_outcomes: Vec<String>,
    pub recent_errors: Vec<String>,
    pub nudges_and_notes: Vec<String>,
    pub advisor_consultations: Vec<(String, String)>,
    pub planned_steps: Vec<String>,
}

pub fn trajectory_steps_to_messages(steps: &[TrajectoryStep]) -> Vec<HarnessMessage> {
    let mut messages = Vec::new();
    for step in steps {
        match &step.action {
            AgentAction::CallTools(calls) => {
                messages.push(HarnessMessage::assistant_with_tools("", calls.clone()));
                if let Some(results) = &step.tool_results {
                    messages.push(HarnessMessage::tool_response(results.clone()));
                }
            }
            AgentAction::FinalAnswer(ans) | AgentAction::RequestInput { prompt: ans } => {
                messages.push(HarnessMessage::assistant(ans.clone()));
            }
            AgentAction::YieldControl { .. } => {}
        }
    }
    messages
}

pub fn compacted_summary_from_steps(steps: &[TrajectoryStep]) -> String {
    let messages = trajectory_steps_to_messages(steps);
    let facts = extract_context_facts(&messages);
    format_structured_context_summary(messages.len().max(1), &facts)
}

pub fn checkpoint_from_trajectory(
    task_id: impl Into<String>,
    source_job_id: impl Into<String>,
    status: RunStatus,
    error: Option<String>,
    steps: &[TrajectoryStep],
) -> crate::eval::ContinuationCheckpoint {
    build_continuation_checkpoint_parts(
        task_id.into(),
        source_job_id.into(),
        status,
        error,
        steps,
        Vec::new(),
        Vec::new(),
    )
}

fn build_continuation_checkpoint(
    task: &TaskSpec,
    session_id: &str,
    status: RunStatus,
    error: Option<&str>,
    steps: &[TrajectoryStep],
    completed_subtasks: Vec<crate::eval::subtask::SubtaskOutcome>,
    subtask_plan: Vec<crate::eval::task::SubtaskSpec>,
) -> crate::eval::ContinuationCheckpoint {
    build_continuation_checkpoint_parts(
        task.id.clone(),
        session_id.to_string(),
        status,
        error.map(ToOwned::to_owned),
        steps,
        completed_subtasks,
        subtask_plan,
    )
}

fn build_continuation_checkpoint_parts(
    task_id: String,
    source_job_id: String,
    status: RunStatus,
    error: Option<String>,
    steps: &[TrajectoryStep],
    completed_subtasks: Vec<crate::eval::subtask::SubtaskOutcome>,
    subtask_plan: Vec<crate::eval::task::SubtaskSpec>,
) -> crate::eval::ContinuationCheckpoint {
    let resume_subtask_id = completed_subtasks
        .iter()
        .find(|o| o.status != RunStatus::Success)
        .map(|o| o.subtask_id.clone())
        .or_else(|| {
            if subtask_plan.is_empty() {
                None
            } else {
                let success_ids: BTreeSet<_> = completed_subtasks
                    .iter()
                    .filter(|o| o.status == RunStatus::Success)
                    .map(|o| o.subtask_id.as_str())
                    .collect();
                subtask_plan
                    .iter()
                    .find(|s| !success_ids.contains(s.id.as_str()))
                    .map(|s| s.id.clone())
            }
        });

    let stop_reason = crate::eval::ContinuationStopReason::from_run_status(status);
    let compacted_summary = crate::eval::progress_summary::run_progress_summary_from_steps(
        status,
        stop_reason,
        error.as_deref(),
        steps,
    );
    crate::eval::ContinuationCheckpoint {
        task_id,
        source_job_id,
        created_at: Utc::now(),
        stop_reason,
        error,
        compacted_summary,
        completed_subtasks,
        resume_subtask_id,
        subtask_plan,
        step_count: steps.len(),
        advisor_sessions: HashMap::new(),
    }
}

pub fn extract_context_facts(messages: &[HarnessMessage]) -> ExtractedContextFacts {
    let mut facts = ExtractedContextFacts::default();

    for msg in messages {
        if let Some(calls) = &msg.tool_calls {
            for call in calls {
                match call.name.as_str() {
                    "read_file" | "view_file" | "read_file_content" => {
                        if let Some(path) = extract_read_target(call) {
                            facts.explored_files.insert(path);
                        }
                    }
                    "write_file" => {
                        if let Some(path) = call.arguments.get("path").and_then(|v| v.as_str()) {
                            if is_scratch_or_note_path(path) {
                                if let Some(content) =
                                    call.arguments.get("content").and_then(|v| v.as_str())
                                {
                                    let note_snippet = truncate_str(content.trim(), 1500);
                                    if let Some(existing) =
                                        facts.retained_notes.iter_mut().find(|(p, _)| p == path)
                                    {
                                        existing.1 = note_snippet;
                                    } else {
                                        facts.retained_notes.push((path.to_string(), note_snippet));
                                    }
                                }
                            } else {
                                facts.modified_files.insert(path.to_string());
                            }
                        }
                    }
                    "edit_file" | "patch_file" | "apply_diff" => {
                        if let Some(path) = call
                            .arguments
                            .get("path")
                            .or_else(|| call.arguments.get("file"))
                            .and_then(|v| v.as_str())
                        {
                            facts.modified_files.insert(path.to_string());
                        }
                    }
                    "list_directory" | "list_dir" => {
                        if let Some(path) = call.arguments.get("path").and_then(|v| v.as_str()) {
                            facts.explored_files.insert(format!("{path}/ (listed)"));
                        }
                    }
                    "search_files" | "grep_search" => {
                        if let Some(query) = call
                            .arguments
                            .get("query")
                            .or_else(|| call.arguments.get("pattern"))
                            .and_then(|v| v.as_str())
                        {
                            facts.explored_files.insert(format!("grep: {query}"));
                        }
                    }
                    "consult_advisor" => {
                        let prompt = call
                            .arguments
                            .get("prompt")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        let prompt_snippet = truncate_str(prompt.trim(), 300);
                        if !prompt_snippet.is_empty() {
                            facts
                                .advisor_consultations
                                .push((prompt_snippet, String::new()));
                        }
                    }
                    "shell" | "bash" => {
                        if let Some(cmd) = call.arguments.get("command").and_then(|v| v.as_str()) {
                            let clean_cmd = strip_stream_redirections(cmd).trim().to_string();
                            if is_modifying_shell_command(&clean_cmd) {
                                facts
                                    .executed_commands
                                    .push(format!("modifying: {}", truncate_str(&clean_cmd, 80)));
                            } else {
                                facts.executed_commands.push(truncate_str(&clean_cmd, 80));
                                if let Some(target) = extract_read_target_from_shell(&clean_cmd) {
                                    facts.explored_files.insert(target);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Some(results) = &msg.tool_results {
            for res in results {
                if res.name == "consult_advisor" && !res.output.trim().is_empty() {
                    let out_snippet = truncate_str(res.output.trim(), 400);
                    if let Some(last) = facts.advisor_consultations.last_mut() {
                        if last.1.is_empty() {
                            last.1 = out_snippet;
                        }
                    }
                }

                if res.is_error {
                    let err_snippet = truncate_str(&res.output.trim().replace('\n', " "), 100);
                    if !err_snippet.is_empty() {
                        facts
                            .recent_errors
                            .push(format!("{}: {}", res.name, err_snippet));
                    }
                } else if (res.name == "shell" || res.name == "bash")
                    && (res.output.contains("test result:")
                        || res.output.contains("FAILED")
                        || res.output.contains("PASSED")
                        || res.output.contains("Tests:"))
                {
                    let outcome_line = res
                        .output
                        .lines()
                        .find(|l| {
                            l.contains("test result:")
                                || l.contains("Tests:")
                                || l.contains("FAILED")
                        })
                        .unwrap_or("");
                    if !outcome_line.is_empty() {
                        facts
                            .test_outcomes
                            .push(truncate_str(outcome_line.trim(), 100));
                    }
                }
            }
        }

        if msg.role == MessageRole::Assistant && !msg.content.is_empty() {
            let content_lower = msg.content.to_lowercase();
            if (content_lower.contains("plan")
                || content_lower.contains("todo")
                || content_lower.contains("steps:")
                || content_lower.contains("implementation"))
                && facts.planned_steps.len() < 2
            {
                let snippet = truncate_str(msg.content.trim(), 500);
                if !snippet.is_empty() {
                    facts.planned_steps.push(snippet);
                }
            }
        }

        if msg.role == MessageRole::User
            && (msg.content.starts_with("[SYSTEM NUDGE")
                || msg.content.starts_with("[MANDATORY PERIODIC REVIEW"))
        {
            let note = truncate_str(msg.content.lines().next().unwrap_or(""), 120);
            facts.nudges_and_notes.push(note);
        }
    }

    facts
}

pub fn format_structured_context_summary(
    pruned_count: usize,
    facts: &ExtractedContextFacts,
) -> String {
    let mut s = format!(
        "[COMPACTED CONTEXT SUMMARY - Progressive History]\n\
         Earlier interaction history ({pruned_count} messages) was compacted to retain immediate context and preserve token budget.\n"
    );

    if !facts.retained_notes.is_empty() {
        s.push_str("\n### Retained Scratchpad & Architecture Notes:\n");
        for (path, content) in &facts.retained_notes {
            s.push_str(&format!(
                "#### Note File: `{path}`\n```markdown\n{content}\n```\n\n"
            ));
        }
    }

    if !facts.advisor_consultations.is_empty() {
        s.push_str("\n### Advisor Consultations & Guidance:\n");
        for (q, a) in facts.advisor_consultations.iter().rev().take(3).rev() {
            s.push_str(&format!("- **Question**: {q}\n"));
            if !a.is_empty() {
                s.push_str(&format!("  **Advice**: {a}\n"));
            }
        }
    }

    if !facts.planned_steps.is_empty() {
        s.push_str("\n### Discovered Architecture & Action Plan:\n");
        for p in facts.planned_steps.iter().rev().take(2).rev() {
            s.push_str(&format!("```markdown\n{p}\n```\n\n"));
        }
    }

    if !facts.explored_files.is_empty() {
        s.push_str("\n### Explored Files & Search Queries:\n");
        for f in facts.explored_files.iter().take(12) {
            s.push_str(&format!("- {f}\n"));
        }
        if facts.explored_files.len() > 12 {
            s.push_str(&format!(
                "- ... and {} more explored items\n",
                facts.explored_files.len() - 12
            ));
        }
    }

    if !facts.modified_files.is_empty() {
        s.push_str("\n### Modified / Created Files:\n");
        for f in facts.modified_files.iter().take(10) {
            s.push_str(&format!("- {f}\n"));
        }
        if facts.modified_files.len() > 10 {
            s.push_str(&format!(
                "- ... and {} more files\n",
                facts.modified_files.len() - 10
            ));
        }
    }

    if !facts.test_outcomes.is_empty() {
        s.push_str("\n### Test & Command Outcomes:\n");
        for t in facts.test_outcomes.iter().rev().take(4).rev() {
            s.push_str(&format!("- {t}\n"));
        }
    }

    if !facts.recent_errors.is_empty() {
        s.push_str("\n### Key Errors Encountered:\n");
        for e in facts.recent_errors.iter().rev().take(4).rev() {
            s.push_str(&format!("- {e}\n"));
        }
    }

    if !facts.nudges_and_notes.is_empty() {
        s.push_str("\n### Guidance & System Milestones:\n");
        for n in facts.nudges_and_notes.iter().rev().take(3).rev() {
            s.push_str(&format!("- {n}\n"));
        }
    }

    s.push_str("\n[Directive]: Use the summarized progress and recent immediate context below to continue the task. Do NOT re-explore files already examined; proceed directly to applying modifications with write_file or running test verification.");
    s
}

/// Compacts a sequence of harness messages using progressive semantic fact extraction.
///
/// When the context token count exceeds `max_tokens * threshold` OR the message count
/// exceeds `max_messages`, this function extracts structured facts (explored files,
/// modified files, test outcomes, error diagnostics, and system guidance) from the intermediate
/// history window and condenses it into a high-density Markdown summary block.
///
/// The resulting context retains:
/// - Head: Initial System Instructions & Task Prompt (first 2 messages)
/// - Middle: Progressive Structured Context Summary
/// - Tail: Active Working Memory (most recent 4-8 interactions)
pub fn compact_context_messages(
    messages: &mut Vec<HarnessMessage>,
    max_messages: usize,
    max_tokens: Option<usize>,
    threshold: f64,
) {
    let total_tokens = messages.iter().map(|m| m.estimate_tokens()).sum::<usize>();
    let token_limit = max_tokens.unwrap_or(usize::MAX);
    let token_watermark = ((token_limit as f64) * threshold) as usize;
    let exceeds_tokens = max_tokens.is_some() && total_tokens > token_watermark;
    let exceeds_messages = messages.len() > max_messages && max_messages >= 6;

    if !exceeds_tokens && !exceeds_messages {
        return;
    }

    if messages.len() < 6 {
        return;
    }

    let head_count = 2.min(messages.len());
    let head: Vec<HarnessMessage> = messages.iter().take(head_count).cloned().collect();

    let tail_preserve = if exceeds_tokens {
        (max_messages.saturating_sub(head_count + 1)).clamp(4, 8)
    } else {
        (max_messages.saturating_sub(head_count + 1)).max(4)
    };

    let tail_start = messages.len().saturating_sub(tail_preserve);
    if tail_start <= head_count {
        return;
    }

    let facts = extract_context_facts(&messages[head_count..tail_start]);
    let pruned_count = tail_start - head_count;
    let summary_text = format_structured_context_summary(pruned_count, &facts);
    let summary_msg = HarnessMessage::system(summary_text);

    let tail: Vec<HarnessMessage> = messages[tail_start..].to_vec();
    let mut compacted = head;
    compacted.push(summary_msg);
    compacted.extend(tail);

    if max_tokens.is_some() {
        let current_tokens = compacted.iter().map(|m| m.estimate_tokens()).sum::<usize>();
        if current_tokens > token_watermark && compacted.len() > 3 {
            for msg in compacted.iter_mut().skip(3) {
                if let Some(results) = &mut msg.tool_results {
                    for res in results {
                        let (snipped, _) = snip_tool_output(&res.output, 50, 4096);
                        res.output = snipped;
                    }
                }
            }
        }
    }

    *messages = compacted;
}

/// Backward-compatible compaction trigger based on message count limit.
pub fn prune_context_messages(messages: &mut Vec<HarnessMessage>, max_messages: usize) {
    compact_context_messages(messages, max_messages, None, 0.8);
}

pub fn is_scratch_or_note_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains(".agent/")
        || lower.contains("/.agent/")
        || lower.starts_with(".agent")
        || lower.contains(".omx/")
        || lower.contains("/.omx/")
        || lower.starts_with(".omx")
        || lower.contains("scratch")
        || lower.contains("notes")
        || lower.ends_with(".patch.py")
        || lower.ends_with(".patch.sh")
}

pub fn strip_stream_redirections(cmd: &str) -> String {
    let mut s = cmd.to_string();
    let patterns = [
        "2>/dev/null",
        "2> /dev/null",
        "1>/dev/null",
        "1> /dev/null",
        "&>/dev/null",
        "&> /dev/null",
        ">/dev/null",
        "> /dev/null",
        "2>&1",
        "2>&-",
        "1>&2",
        ">&2",
    ];
    for p in patterns {
        s = s.replace(p, " ");
    }
    s
}

pub fn is_modifying_call(call: &ToolCallRequest) -> bool {
    match call.name.as_str() {
        "write_file" => {
            let path = call
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim();
            !is_scratch_or_note_path(path)
        }
        "shell" | "bash" => {
            let cmd = call
                .arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim();
            is_modifying_shell_command(cmd)
        }
        _ => false,
    }
}

pub fn is_modifying_shell_command(cmd: &str) -> bool {
    let lower = cmd.to_lowercase();

    // Python scripts that write files
    if (lower.contains("python") || lower.contains("python3"))
        && (lower.contains("write(")
            || (lower.contains("open(")
                && (lower.contains("'w'")
                    || lower.contains("\"w\"")
                    || lower.contains("'a'")
                    || lower.contains("\"a\""))))
        && !is_scratch_or_note_path(&lower)
    {
        return true;
    }

    let modifying_prefixes = [
        "cargo build",
        "cargo test",
        "cargo run",
        "cargo add",
        "cargo remove",
        "cargo fmt",
        "cargo check",
        "npm run",
        "npm test",
        "npm build",
        "npm i",
        "npm install",
        "pnpm run",
        "pnpm test",
        "pnpm build",
        "pnpm i",
        "pnpm install",
        "yarn run",
        "yarn test",
        "yarn build",
        "git checkout",
        "git apply",
        "git commit",
        "git merge",
        "git stash",
        "git cherry-pick",
        "touch ",
        "rm ",
        "mkdir ",
        "cp ",
        "mv ",
        "sed -i",
        "patch ",
        "pytest",
        "go test",
        "go build",
    ];

    for prefix in modifying_prefixes {
        if lower.starts_with(prefix)
            || lower.contains(&format!("&& {prefix}"))
            || lower.contains(&format!("; {prefix}"))
            || lower.contains(&format!("| {prefix}"))
        {
            if (lower.starts_with("touch ")
                || lower.starts_with("mkdir ")
                || lower.starts_with("rm "))
                && is_scratch_or_note_path(&lower)
            {
                continue;
            }
            return true;
        }
    }

    // Check for explicit file write or redirect operators after stripping stream noise outside quotes
    if has_unquoted_shell_redirection(&lower) && !is_scratch_or_note_path(&lower) {
        return true;
    }
    if lower.contains("tee ") && !is_scratch_or_note_path(&lower) {
        return true;
    }

    false
}

fn has_unquoted_shell_redirection(cmd: &str) -> bool {
    let stripped = strip_stream_redirections(cmd);
    let bytes = stripped.as_bytes();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
        } else if b == b'"' && !in_single_quote {
            in_double_quote = !in_double_quote;
        } else if !in_single_quote && !in_double_quote && b == b'>' {
            // Ignore `>=` (comparison), `->` (arrow), `=>` (fat arrow)
            let prev = if i > 0 { bytes[i - 1] } else { b' ' };
            let next = if i + 1 < bytes.len() {
                bytes[i + 1]
            } else {
                b' '
            };
            if prev != b'-' && prev != b'=' && next != b'=' {
                return true;
            }
        }
        i += 1;
    }
    false
}

pub fn is_modifying_action(action: &AgentAction) -> bool {
    match action {
        AgentAction::CallTools(calls) => calls.iter().any(is_modifying_call),
        _ => false,
    }
}

const MAX_TOOL_OUTPUT_CHARS: usize = 60_000;

#[allow(clippy::string_slice)]
fn truncate_tool_output(output: String) -> String {
    if output.len() <= MAX_TOOL_OUTPUT_CHARS {
        return output;
    }
    let mut end = MAX_TOOL_OUTPUT_CHARS;
    while end > 0 && !output.is_char_boundary(end) {
        end -= 1;
    }
    let total = output.len();
    let truncated = &output[..end];
    format!(
        "{truncated}\n\n[... Output truncated: total {total} bytes, capped at {MAX_TOOL_OUTPUT_CHARS} bytes ...]"
    )
}

pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Normalizes a target file path by stripping enclosing quotes, leading workspace roots, and relative './'.
pub fn normalize_read_target(path: &str, workspace_root: Option<&Path>) -> String {
    let mut clean = path
        .trim()
        .trim_matches(|c| c == '\'' || c == '"' || c == '`')
        .to_string();

    if let Some(ws) = workspace_root {
        let ws_str = ws.to_string_lossy();
        if let Some(stripped) = clean.strip_prefix(ws_str.as_ref()) {
            clean = stripped.to_string();
        }
    }

    let mut trimmed = clean.as_str();
    while let Some(rest) = trimmed.strip_prefix("./") {
        trimmed = rest;
    }
    while let Some(rest) = trimmed.strip_prefix('/') {
        trimmed = rest;
    }
    trimmed.to_string()
}

/// Extracts the underlying target file path for read/inspection operations,
/// whether invoked via direct read tools (`read_file`, `view_file`, `read_file_content`)
/// or through shell read commands (`sed`, `awk`, `cat`, `head`, `tail`, `less`, `more`, `bat`, `nl`, `grep`, `rg`).
pub fn extract_read_target(call: &ToolCallRequest) -> Option<String> {
    extract_read_target_with_root(call, None)
}

/// Extracts and normalizes the underlying target file path for read/inspection operations against workspace root.
pub fn extract_read_target_with_root(
    call: &ToolCallRequest,
    workspace_root: Option<&Path>,
) -> Option<String> {
    match call.name.as_str() {
        "read_file" | "view_file" | "read_file_content" => call
            .arguments
            .get("path")
            .or_else(|| call.arguments.get("file"))
            .or_else(|| call.arguments.get("AbsolutePath"))
            .and_then(|v| v.as_str())
            .map(|p| normalize_read_target(p, workspace_root)),
        "shell" | "bash" => {
            let cmd = call
                .arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim();
            extract_read_target_from_shell_with_root(cmd, workspace_root)
        }
        _ => None,
    }
}

/// Helper to parse read target file paths from shell commands like `sed`, `awk`, `cat`, `head`, `tail`, `grep`, `rg`.
pub fn extract_read_target_from_shell(cmd: &str) -> Option<String> {
    extract_read_target_from_shell_with_root(cmd, None)
}

/// Helper to parse and normalize read target file paths from shell commands against workspace root.
#[allow(clippy::string_slice)]
pub fn extract_read_target_from_shell_with_root(
    cmd: &str,
    workspace_root: Option<&Path>,
) -> Option<String> {
    let stripped = strip_stream_redirections(cmd);
    let trimmed = stripped.trim();

    if is_modifying_shell_command(trimmed) {
        return None;
    }

    for part in split_shell_commands(trimmed) {
        let tokens = tokenize_shell_command(part.trim());
        if tokens.is_empty() {
            continue;
        }

        let cmd_name = tokens[0]
            .rsplit('/')
            .next()
            .unwrap_or(&tokens[0])
            .to_lowercase();

        if matches!(
            cmd_name.as_str(),
            "cat"
                | "head"
                | "tail"
                | "sed"
                | "awk"
                | "less"
                | "more"
                | "bat"
                | "nl"
                | "grep"
                | "rg"
        ) {
            for token in tokens.iter().skip(1).rev() {
                let clean = token.trim_matches(|c| c == '\'' || c == '"' || c == ';' || c == ',');
                if clean.is_empty()
                    || clean.starts_with('-')
                    || clean.starts_with("NR")
                    || clean.starts_with("BEGIN")
                    || clean.starts_with('{')
                    || clean.starts_with("http://")
                    || clean.starts_with("https://")
                    || (clean.ends_with('p')
                        && clean.len() > 1
                        && clean[..clean.len() - 1]
                            .chars()
                            .all(|c| c.is_ascii_digit() || c == ','))
                {
                    continue;
                }
                if clean.contains('/')
                    || clean.contains('.')
                    || clean.ends_with(".rs")
                    || clean.ends_with(".md")
                    || clean.ends_with(".ts")
                    || clean.ends_with(".js")
                    || clean.ends_with(".json")
                    || clean.ends_with(".vue")
                    || clean.ends_with(".toml")
                    || clean.ends_with(".sql")
                    || clean.ends_with(".yaml")
                    || clean.ends_with(".yml")
                    || clean.ends_with(".sh")
                    || clean.ends_with(".py")
                    || clean.ends_with(".go")
                    || clean.ends_with(".txt")
                    || clean.ends_with(".csv")
                {
                    return Some(normalize_read_target(clean, workspace_root));
                }
            }
        }
    }
    None
}

#[allow(clippy::string_slice)]
fn split_shell_commands(cmd: &str) -> Vec<&str> {
    let mut commands = Vec::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut start = 0;
    let bytes = cmd.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
        } else if b == b'"' && !in_single_quote {
            in_double_quote = !in_double_quote;
        } else if !in_single_quote && !in_double_quote {
            if b == b';' || b == b'|' {
                commands.push(&cmd[start..i]);
                start = i + 1;
            } else if b == b'&' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'&' {
                    commands.push(&cmd[start..i]);
                    i += 1;
                    start = i + 1;
                } else {
                    commands.push(&cmd[start..i]);
                    start = i + 1;
                }
            }
        }
        i += 1;
    }
    if start < cmd.len() {
        commands.push(&cmd[start..]);
    }
    commands
}

fn tokenize_shell_command(segment: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for c in segment.chars() {
        if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
        } else if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
        } else if c.is_whitespace() && !in_single_quote && !in_double_quote {
            if !current.is_empty() {
                tokens.push(current);
                current = String::new();
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Checks if a shell command is doing fragmented sliced reads on files (e.g. `sed -n 'X,Yp'`).
pub fn is_shell_fragmented_read(cmd: &str) -> bool {
    let lower = cmd.to_lowercase();
    (lower.contains("sed -n")
        || lower.contains("sed '")
        || lower.contains("sed \"")
        || lower.contains("awk "))
        && (lower.contains("p'")
            || lower.contains("p\"")
            || lower.contains("nr")
            || lower.contains("print"))
        && !is_modifying_shell_command(cmd)
}
