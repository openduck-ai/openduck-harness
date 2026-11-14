use anyhow::{anyhow, Result};
use openduck_harness::runtime::HarnessToolExtension;
use openduck_harness::types::{ToolCallRequest, ToolCallResponse, ToolDefinition};
use openduck_mcp::tui_session::{AgentCliKind, TuiPaneState, TuiSessionManager};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use crate::control::tui_management::{
    QuotaSnapshotRecord, TuiManagementService, DEFAULT_QUOTA_STALE_AFTER_SECONDS,
};
use chrono::{DateTime, Utc};

const QUOTA_REFRESH_TIMEOUT: Duration = Duration::from_secs(5);

#[async_trait::async_trait]
trait QuotaGate: Send + Sync {
    async fn refresh(&self, agent: &str) -> Result<()>;
    async fn cached(&self, agent: &str) -> Result<Option<QuotaSnapshotRecord>>;
}

#[async_trait::async_trait]
impl QuotaGate for TuiManagementService {
    async fn refresh(&self, agent: &str) -> Result<()> {
        self.refresh_quotas_for_agent(agent).await.map(|_| ())
    }

    async fn cached(&self, agent: &str) -> Result<Option<QuotaSnapshotRecord>> {
        self.check_agent_quota(agent, None).await
    }
}

fn is_fresh(record: &QuotaSnapshotRecord, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(&record.observed_at)
        .map(|t| t.with_timezone(&Utc))
        .is_ok_and(|observed| {
            let age = now - observed;
            age >= chrono::Duration::zero()
                && age <= chrono::Duration::seconds(DEFAULT_QUOTA_STALE_AFTER_SECONDS as i64)
        })
}

fn blocking_reset(record: &QuotaSnapshotRecord, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if !is_fresh(record, now) {
        return None;
    }
    let exhausted = record.status == "exhausted" || record.remaining_percent == Some(0.0);
    let reset = DateTime::parse_from_rfc3339(record.reset_at.as_deref()?)
        .ok()?
        .with_timezone(&Utc);
    (exhausted && reset > now).then_some(reset)
}

/// Rejects only when a known exhausted quota has a future reset. Query
/// failures and timeouts never count as exhaustion.
///
/// `refresh_first` launches always re-read status; prompts consult the cached
/// book and refresh only when it already looks exhausted, so unrelated status
/// never blocks them.
async fn enforce_quota_admission(
    gate: &dyn QuotaGate,
    agent: &str,
    refresh_first: bool,
    now: DateTime<Utc>,
) -> Result<()> {
    let refresh = || async {
        let _ = tokio::time::timeout(QUOTA_REFRESH_TIMEOUT, gate.refresh(agent)).await;
    };
    if refresh_first {
        refresh().await;
    }
    let mut record = gate.cached(agent).await.ok().flatten();
    if !refresh_first
        && record
            .as_ref()
            .is_some_and(|r| blocking_reset(r, now).is_some())
    {
        refresh().await;
        record = gate.cached(agent).await.ok().flatten();
    }
    match record.as_ref().and_then(|r| blocking_reset(r, now).map(|t| (r, t))) {
        Some((r, reset)) => Err(anyhow!(
            "Cannot use agent '{agent}': hit {} limit (resets at {}). Agent is suspended until quota resets.",
            r.quota_type,
            reset.to_rfc3339()
        )),
        None => Ok(()),
    }
}

pub struct TuiAgentHarnessExtension {
    manager: Arc<TuiSessionManager>,
    project_slug: Option<String>,
    task_id: Option<String>,
    run_id: Option<String>,
}

impl TuiAgentHarnessExtension {
    pub fn new(manager: Arc<TuiSessionManager>) -> Self {
        Self {
            manager,
            project_slug: None,
            task_id: None,
            run_id: None,
        }
    }

    pub fn with_context(
        mut self,
        project_slug: Option<String>,
        task_id: Option<String>,
        run_id: Option<String>,
    ) -> Self {
        self.project_slug = project_slug;
        self.task_id = task_id;
        self.run_id = run_id;
        self
    }

    fn resolve_cwd(raw: Option<&str>, workspace_root: &Path) -> PathBuf {
        if let Some(p) = raw.filter(|s| !s.trim().is_empty()) {
            let path = Path::new(p);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                workspace_root.join(path)
            }
        } else {
            workspace_root.to_path_buf()
        }
    }

    async fn handle_start(
        &self,
        call: &ToolCallRequest,
        workspace_root: &Path,
    ) -> Result<ToolCallResponse> {
        let agent_str = call
            .arguments
            .get("agent")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                anyhow!("Parameter 'agent' is required (e.g. 'codex', 'grok', 'agy', 'claude')")
            })?;

        let kind = AgentCliKind::from_str(agent_str)?;

        let cwd_str = call.arguments.get("cwd").and_then(|v| v.as_str());
        let cwd = Self::resolve_cwd(cwd_str, workspace_root);

        let session_id = call
            .arguments
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{}_{}", kind, chrono::Utc::now().timestamp_millis()));

        let model = call
            .arguments
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let effort = call
            .arguments
            .get("effort")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let auto_approve = call
            .arguments
            .get("auto_approve")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let timeout_secs = call
            .arguments
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(600);

        let prompt = call
            .arguments
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let proxy_mode = call
            .arguments
            .get("proxy_mode")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let proxy_id = call
            .arguments
            .get("proxy_id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let https_proxy = call
            .arguments
            .get("https_proxy")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let service = crate::control::tui_management::TuiManagementService::global().await;
        enforce_quota_admission(&*service, &kind.to_string(), true, Utc::now()).await?;
        let (rec, _initial_turn) = service
            .start_session(crate::control::tui_management::StartManagedSessionRequest {
                agent: kind.to_string(),
                cwd,
                model,
                effort,
                auto_approve: Some(auto_approve),
                resume_id: None,
                initial_prompt: None,
                session_name: Some(session_id.clone()),
                extra_args: None,
                proxy_mode,
                proxy_id,
                https_proxy,
                account_id: None,
                idle_timeout_mins: None,
                exited_retention_mins: None,
                project_slug: self.project_slug.clone(),
                task_id: self.task_id.clone(),
                run_id: self.run_id.clone(),
                subtask_id: None,
                creation_source: Some("harness".to_string()),
            })
            .await?;

        let handle = match service.session_manager.get_session(&rec.session_name).await {
            Ok(handle) => handle,
            Err(_) => self.manager.get_session(&rec.session_name).await?,
        };
        let current_state = handle.last_state.lock().await.clone();

        if let TuiPaneState::QuotaExceeded { ref message } = current_state {
            return Ok(ToolCallResponse {
                id: call.id.clone(),
                name: call.name.clone(),
                output: format!(
                    "Agent '{}' hit quota limit: {message}. Agent is suspended until quota reset. Please switch to another agent (e.g. 'agy', 'codex', 'claude').",
                    kind
                ),
                is_error: true,
            });
        }

        if let Some(p) = prompt {
            if let TuiPaneState::WaitingApproval { ref prompt_snippet } = current_state {
                return Ok(ToolCallResponse {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    output: format!(
                        "Session started: {}\nAttach command: {}\nState: Waiting for approval: {}\nNote: Initial prompt was held because confirmation is required. Use tui_agent_keys to approve.",
                        rec.session_name,
                        handle.backend.attach_command(&rec.session_name),
                        prompt_snippet
                    ),
                    is_error: false,
                });
            }
            let turn = service
                .send_prompt(
                    &rec.session_name,
                    &p,
                    true,
                    Duration::from_secs(timeout_secs),
                    self.run_id.as_deref(),
                )
                .await?;
            let output = if !turn.response.trim().is_empty() {
                turn.response
            } else {
                turn.screen_snapshot
            };
            Ok(ToolCallResponse {
                id: call.id.clone(),
                name: call.name.clone(),
                output: format!(
                    "Session started: {}\nAttach command: {}\nState: {}\nResponse:\n{}",
                    rec.session_name,
                    turn.attach_command,
                    turn.state.label(),
                    output
                ),
                is_error: false,
            })
        } else {
            let state_str = match &current_state {
                TuiPaneState::WaitingApproval { prompt_snippet } => {
                    format!("Waiting for approval: {prompt_snippet}")
                }
                TuiPaneState::IdlePrompt => "Ready".to_string(),
                other => other.label().to_string(),
            };
            Ok(ToolCallResponse {
                id: call.id.clone(),
                name: call.name.clone(),
                output: format!(
                    "Session started: {}\nAttach command: {}\nState: {}",
                    rec.session_name,
                    handle.backend.attach_command(&rec.session_name),
                    state_str
                ),
                is_error: false,
            })
        }
    }

    async fn handle_prompt(&self, call: &ToolCallRequest) -> Result<ToolCallResponse> {
        let session_id = call
            .arguments
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Parameter 'session_id' is required"))?;

        let prompt = call
            .arguments
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Parameter 'prompt' is required"))?;

        let wait = call
            .arguments
            .get("wait")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let timeout_secs = call
            .arguments
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(600);

        let service = crate::control::tui_management::TuiManagementService::global().await;
        if let Some(rec) = service.get_session_record(session_id).await? {
            enforce_quota_admission(&*service, &rec.agent_kind, false, Utc::now()).await?;
        }
        let turn = service
            .send_prompt(
                session_id,
                prompt,
                wait,
                Duration::from_secs(timeout_secs),
                self.run_id.as_deref(),
            )
            .await?;

        let output = if !turn.response.trim().is_empty() {
            turn.response
        } else {
            turn.screen_snapshot
        };

        let is_quota_exceeded = matches!(turn.state, TuiPaneState::QuotaExceeded { .. });
        Ok(ToolCallResponse {
            id: call.id.clone(),
            name: call.name.clone(),
            output: format!(
                "Session: {}\nState: {}\nResponse:\n{}",
                session_id,
                turn.state.label(),
                output
            ),
            is_error: is_quota_exceeded,
        })
    }

    async fn handle_read(&self, call: &ToolCallRequest) -> Result<ToolCallResponse> {
        let session_id = call
            .arguments
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Parameter 'session_id' is required"))?;

        let lines = call.arguments.get("lines").and_then(|v| v.as_u64());
        let raw_ansi = call
            .arguments
            .get("raw_ansi")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let service = crate::control::tui_management::TuiManagementService::global().await;
        let turn = service
            .read_state_wait(
                session_id,
                lines.map(|l| l as i32),
                raw_ansi,
                call.arguments.get("wait_secs").and_then(|v| v.as_u64()),
            )
            .await?;

        let mut text = if !turn.response.trim().is_empty() {
            format!(
                "Response:\n{}\n\nScreen Snapshot:\n{}",
                turn.response, turn.screen_snapshot
            )
        } else {
            turn.screen_snapshot
        };
        if let Some(hint) = &turn.stall_hint {
            text.push_str(&format!("\n\nStall warning: {hint}"));
        }

        Ok(ToolCallResponse {
            id: call.id.clone(),
            name: call.name.clone(),
            output: format!(
                "Session: {}\nState: {}\n{}",
                session_id,
                turn.state.label(),
                text
            ),
            is_error: false,
        })
    }

    async fn handle_keys(&self, call: &ToolCallRequest) -> Result<ToolCallResponse> {
        let session_id = call
            .arguments
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Parameter 'session_id' is required"))?;

        let keys: Vec<String> = call
            .arguments
            .get("keys")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|val| val.as_str().map(String::from))
                    .collect()
            })
            .ok_or_else(|| anyhow!("Parameter 'keys' must be an array of strings"))?;

        let service = crate::control::tui_management::TuiManagementService::global().await;
        service.send_keys(session_id, &keys).await?;

        Ok(ToolCallResponse {
            id: call.id.clone(),
            name: call.name.clone(),
            output: format!("Sent keys {:?} to session '{}'", keys, session_id),
            is_error: false,
        })
    }

    async fn handle_list(&self, call: &ToolCallRequest) -> Result<ToolCallResponse> {
        let service = crate::control::tui_management::TuiManagementService::global().await;
        let summaries = service.session_manager.list_sessions(false).await;
        if summaries.is_empty() {
            return Ok(ToolCallResponse {
                id: call.id.clone(),
                name: call.name.clone(),
                output: "No active TUI agent sessions.".to_string(),
                is_error: false,
            });
        }

        let mut lines = Vec::new();
        for s in summaries {
            lines.push(format!(
                "- ID: {} | Agent: {} | State: {} | CWD: {} | Attach: {}",
                s.session_id,
                s.kind,
                s.state.label(),
                s.cwd,
                s.attach_command
            ));
        }

        Ok(ToolCallResponse {
            id: call.id.clone(),
            name: call.name.clone(),
            output: lines.join("\n"),
            is_error: false,
        })
    }

    async fn handle_stop(&self, call: &ToolCallRequest) -> Result<ToolCallResponse> {
        let session_id = call
            .arguments
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Parameter 'session_id' is required"))?;

        let service = crate::control::tui_management::TuiManagementService::global().await;
        service
            .stop_session(session_id, Some("harness_stop"))
            .await?;

        Ok(ToolCallResponse {
            id: call.id.clone(),
            name: call.name.clone(),
            output: format!("TUI agent session '{}' stopped.", session_id),
            is_error: false,
        })
    }

    async fn handle_convenience(
        &self,
        call: &ToolCallRequest,
        workspace_root: &Path,
    ) -> Result<ToolCallResponse> {
        let _agent = call
            .arguments
            .get("agent")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow!("Parameter 'agent' is required"))?;

        let _prompt = call
            .arguments
            .get("prompt")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow!("Parameter 'prompt' is required"))?;

        let session_id_opt = call
            .arguments
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if let Some(ref sid) = session_id_opt {
            let service = crate::control::tui_management::TuiManagementService::global().await;
            if service.session_manager.get_session(sid).await.is_ok()
                || self.manager.get_session(sid).await.is_ok()
            {
                return self.handle_prompt(call).await;
            }
        }

        self.handle_start(call, workspace_root).await
    }
}

#[async_trait::async_trait]
impl HarnessToolExtension for TuiAgentHarnessExtension {
    fn tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "tui_agent_start".into(),
                description: "Spawns an interactive coding agent CLI (codex, grok, agy, claude) inside a persistent tmux session. If prompt is provided, executes it and waits for completion. Returns session_id and attach command.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent": {
                            "type": "string",
                            "description": "Agent CLI to spawn: codex | grok | agy | claude | custom"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "Optional initial prompt or task instruction to execute immediately upon launch."
                        },
                        "session_id": {
                            "type": "string",
                            "description": "Optional custom session identifier. If omitted, an ID is generated automatically."
                        },
                        "cwd": {
                            "type": "string",
                            "description": "Optional working directory for the agent session (defaults to workspace root)."
                        },
                        "model": {
                            "type": "string",
                            "description": "Optional model slug override for the agent CLI."
                        },
                        "effort": {
                            "type": "string",
                            "description": "Optional reasoning effort: low, medium, or high."
                        },
                        "auto_approve": {
                            "type": "boolean",
                            "description": "Whether to auto-approve tool execution and permission prompts (defaults to true)."
                        },
                        "proxy_mode": {
                            "type": "string",
                            "description": "Optional proxy mode: inherit | direct | named | custom"
                        },
                        "proxy_id": {
                            "type": "string",
                            "description": "Optional named proxy ID or custom proxy URL."
                        },
                        "https_proxy": {
                            "type": "string",
                            "description": "Optional HTTPS proxy URL override (for example http://127.0.0.1:2087)."
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Timeout in seconds to wait for initial prompt completion (defaults to 600s)."
                        }
                    },
                    "required": ["agent"]
                }),
            },
            ToolDefinition {
                name: "tui_agent_prompt".into(),
                description: "Sends a prompt to an idle interactive agent CLI session (codex, grok, agy, claude). Busy sessions reject new prompts. Submission has a separate 10s budget. With wait=false, or if wait=true returns working, poll tui_agent_read until the previous turn completes and state is idle_prompt before sending another prompt. Handle approval, quota and exit states separately. Never blindly resend after uncertain submission.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "Session ID returned by tui_agent_start or previous turn."
                        },
                        "prompt": {
                            "type": "string",
                            "description": "The prompt or instruction to send to the agent."
                        },
                        "wait": {
                            "type": "boolean",
                            "description": "Whether to wait for turn completion (defaults to true)."
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Execution wait budget in seconds after submission (defaults to 600s); expiry does not stop the turn."
                        }
                    },
                    "required": ["session_id", "prompt"]
                }),
            },
            ToolDefinition {
                name: "tui_agent_read".into(),
                description: "Waits up to 30 seconds (maximum 60; extended automatically when a working screen stays unchanged) for completion, approval, quota or exit, then returns latest output and screen. Unchanged screens are omitted. Continue waiting while working; elapsed time alone is not a reason to interrupt. Use wait_secs=0 only for immediate inspection. If stallHint is present, the screen has been unchanged for 5+ minutes: inspect it instead of waiting again.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "Session ID to inspect."
                        },
                        "lines": {
                            "type": "integer",
                            "description": "Number of lines of terminal capture to return (defaults to 50)."
                        },
                        "wait_secs": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 60,
                            "default": 30,
                            "description": "Wait for completion or an actionable state; 0 returns immediately."
                        },
                        "raw_ansi": {
                            "type": "boolean",
                            "description": "Whether to keep raw ANSI escape codes (defaults to false)."
                        }
                    },
                    "required": ["session_id"]
                }),
            },
            ToolDefinition {
                name: "tui_agent_keys".into(),
                description: "Sends raw keyboard keys or escape sequences to an interactive agent session (e.g. ['Enter'], ['Escape'], ['y', 'Enter']).".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "Session ID to send keys to."
                        },
                        "keys": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of key names to send (e.g. ['Enter'], ['Escape'], ['y', 'Enter'], ['Up'], ['Down'], ['C-c'])."
                        }
                    },
                    "required": ["session_id", "keys"]
                }),
            },
            ToolDefinition {
                name: "tui_agent_list".into(),
                description: "Lists all active interactive agent CLI sessions and their current states.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "tui_agent_stop".into(),
                description: "Stops and cleans up an interactive agent CLI session.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "Session ID of the session to terminate and clean up."
                        }
                    },
                    "required": ["session_id"]
                }),
            },
            ToolDefinition {
                name: "tui_agent".into(),
                description: "Convenience tool to invoke a TUI agent CLI (codex, grok, agy, claude). If session_id is omitted or does not exist, starts a new session; otherwise resumes it. Executes the prompt and returns the output.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent": {
                            "type": "string",
                            "description": "Agent CLI to invoke: codex | grok | agy | claude"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "The task prompt or question to submit to the agent."
                        },
                        "session_id": {
                            "type": "string",
                            "description": "Optional session ID. If an active session with this ID exists, resumes it; otherwise starts a new session."
                        },
                        "cwd": {
                            "type": "string",
                            "description": "Optional working directory (defaults to workspace root)."
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Execution wait budget in seconds after submission (defaults to 600s); expiry does not stop the turn."
                        }
                    },
                    "required": ["agent", "prompt"]
                }),
            },
        ]
    }

    async fn execute(
        &self,
        call: &ToolCallRequest,
        workspace_root: &Path,
    ) -> Option<ToolCallResponse> {
        let result = match call.name.as_str() {
            "tui_agent_start" => self.handle_start(call, workspace_root).await,
            "tui_agent_prompt" => self.handle_prompt(call).await,
            "tui_agent_read" => self.handle_read(call).await,
            "tui_agent_keys" => self.handle_keys(call).await,
            "tui_agent_list" => self.handle_list(call).await,
            "tui_agent_stop" => self.handle_stop(call).await,
            "tui_agent" | "consult_tui_agent" => {
                self.handle_convenience(call, workspace_root).await
            }
            _ => return None,
        };

        Some(match result {
            Ok(resp) => resp,
            Err(e) => {
                let err_msg = format!("{:#}", e);
                let helpful = if err_msg.contains("quota resets")
                    || err_msg.contains("quota limit")
                    || err_msg.contains("suspended until quota")
                {
                    format!("{err_msg}\nTip: Please switch to an alternative agent (e.g. 'agy', 'codex', 'claude').")
                } else {
                    format!("Error in tool '{}': {err_msg}", call.name)
                };
                ToolCallResponse {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    output: helpful,
                    is_error: true,
                }
            }
        })
    }
}

#[cfg(test)]
mod quota_gate_tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeGate {
        refresh_fails: bool,
        before: Option<QuotaSnapshotRecord>,
        after: Option<QuotaSnapshotRecord>,
        refreshed: Mutex<u32>,
    }

    #[async_trait::async_trait]
    impl QuotaGate for FakeGate {
        async fn refresh(&self, _: &str) -> Result<()> {
            *self.refreshed.lock().unwrap() += 1;
            if self.refresh_fails {
                Err(anyhow!("boom"))
            } else {
                Ok(())
            }
        }
        async fn cached(&self, _: &str) -> Result<Option<QuotaSnapshotRecord>> {
            let refreshed = *self.refreshed.lock().unwrap() > 0;
            Ok(if refreshed {
                self.after.clone()
            } else {
                self.before.clone()
            })
        }
    }

    fn record(reset: Option<DateTime<Utc>>, exhausted: bool) -> QuotaSnapshotRecord {
        QuotaSnapshotRecord {
            id: "q".into(),
            pool_id: "p".into(),
            account_id: None,
            agent_kind: Some("grok".into()),
            model_pattern: None,
            quota_type: "weekly".into(),
            unit: "credits".into(),
            used_amount: 0.0,
            limit_amount: None,
            remaining_percent: exhausted.then_some(0.0),
            window_start: None,
            window_end: None,
            reset_at: reset.map(|r| r.to_rfc3339()),
            source: "test".into(),
            status: if exhausted { "exhausted" } else { "valid" }.into(),
            observed_at: (Utc::now() - chrono::Duration::seconds(1)).to_rfc3339(),
        }
    }

    fn gate(
        refresh_fails: bool,
        before: Option<QuotaSnapshotRecord>,
        after: Option<QuotaSnapshotRecord>,
    ) -> FakeGate {
        FakeGate {
            refresh_fails,
            before,
            after,
            refreshed: Mutex::new(0),
        }
    }

    #[tokio::test]
    async fn launch_rejects_exhausted_with_reset_time() {
        let now = Utc::now();
        let reset = now + chrono::Duration::hours(2);
        let r = record(Some(reset), true);
        let g = gate(false, Some(r.clone()), Some(r));
        let err = enforce_quota_admission(&g, "grok", true, now)
            .await
            .unwrap_err();
        assert!(err.to_string().contains(&reset.to_rfc3339()));
        assert_eq!(*g.refreshed.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn refresh_failure_is_not_exhaustion() {
        let g = gate(true, None, None);
        enforce_quota_admission(&g, "grok", true, Utc::now())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn launch_allowed_when_refresh_clears_exhaustion() {
        let now = Utc::now();
        let stale = record(Some(now + chrono::Duration::hours(1)), true);
        let g = gate(false, Some(stale), Some(record(None, false)));
        enforce_quota_admission(&g, "grok", true, now)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn past_reset_or_unknown_reset_does_not_block() {
        let now = Utc::now();
        let past = record(Some(now - chrono::Duration::minutes(1)), true);
        enforce_quota_admission(
            &gate(false, Some(past.clone()), Some(past)),
            "grok",
            true,
            now,
        )
        .await
        .unwrap();
        let unknown = record(None, true);
        enforce_quota_admission(
            &gate(false, Some(unknown.clone()), Some(unknown)),
            "grok",
            true,
            now,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn prompt_skips_refresh_unless_cached_exhausted() {
        let now = Utc::now();
        let g = gate(false, Some(record(None, false)), None);
        enforce_quota_admission(&g, "grok", false, now)
            .await
            .unwrap();
        assert_eq!(*g.refreshed.lock().unwrap(), 0);

        let r = record(Some(now + chrono::Duration::hours(1)), true);
        let g = gate(false, Some(r.clone()), Some(r));
        assert!(enforce_quota_admission(&g, "grok", false, now)
            .await
            .is_err());
        assert_eq!(*g.refreshed.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn stale_or_malformed_observation_fails_open() {
        let now = Utc::now();
        let reset = Some(now + chrono::Duration::hours(1));
        let stale_age = DEFAULT_QUOTA_STALE_AFTER_SECONDS as i64 + 60;
        let observations = [
            (now - chrono::Duration::seconds(stale_age)).to_rfc3339(),
            (now + chrono::Duration::minutes(10)).to_rfc3339(),
            "not-a-timestamp".to_string(),
            String::new(),
        ];
        for observed_at in observations {
            let mut r = record(reset, true);
            r.observed_at = observed_at.clone();
            let g = gate(true, Some(r.clone()), Some(r));
            enforce_quota_admission(&g, "grok", true, now)
                .await
                .unwrap_or_else(|e| panic!("{observed_at:?} should fail open: {e}"));
        }
    }

    #[tokio::test]
    async fn fresh_observation_still_blocks_when_refresh_fails() {
        let now = Utc::now();
        let mut r = record(Some(now + chrono::Duration::hours(1)), true);
        r.observed_at = (now - chrono::Duration::seconds(30)).to_rfc3339();
        let g = gate(true, Some(r.clone()), Some(r));
        assert!(enforce_quota_admission(&g, "grok", true, now)
            .await
            .is_err());
    }
}
