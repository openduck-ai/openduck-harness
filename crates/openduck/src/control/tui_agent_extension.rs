use anyhow::{anyhow, Result};
use openduck_harness::runtime::HarnessToolExtension;
use openduck_harness::types::{ToolCallRequest, ToolCallResponse, ToolDefinition};
use openduck_mcp::tui_session::{AgentCliKind, TuiPaneState, TuiSessionManager};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

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

        let handle = self.manager.get_session(&rec.session_name).await?;
        let current_state = handle.last_state.lock().await.clone();

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

        Ok(ToolCallResponse {
            id: call.id.clone(),
            name: call.name.clone(),
            output: format!(
                "Session: {}\nState: {}\nResponse:\n{}",
                session_id,
                turn.state.label(),
                output
            ),
            is_error: false,
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
            .read_state(session_id, lines.map(|l| l as i32), raw_ansi)
            .await?;

        let text = if !turn.response.trim().is_empty() {
            format!(
                "Response:\n{}\n\nScreen Snapshot:\n{}",
                turn.response, turn.screen_snapshot
            )
        } else {
            turn.screen_snapshot
        };

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
            if self.manager.get_session(sid).await.is_ok() {
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
                description: "Sends a prompt to an existing interactive agent CLI session (codex, grok, agy, claude), waits for it to complete its turn, and returns the response.".into(),
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
                            "description": "Timeout in seconds to wait for turn completion (defaults to 600s)."
                        }
                    },
                    "required": ["session_id", "prompt"]
                }),
            },
            ToolDefinition {
                name: "tui_agent_read".into(),
                description: "Reads the live terminal screen capture and latest output from an active agent session.".into(),
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
                            "description": "Timeout in seconds to wait for turn completion (defaults to 600s)."
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
            Err(e) => ToolCallResponse {
                id: call.id.clone(),
                name: call.name.clone(),
                output: format!("Error in tool '{}': {:#}", call.name, e),
                is_error: true,
            },
        })
    }
}
