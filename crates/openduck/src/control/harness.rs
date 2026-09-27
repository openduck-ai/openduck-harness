use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use axum::{
    extract::{Path as AxumPath, Query},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use openduck_harness::eval::datasets::{load_jsonl_dataset, load_recipe_task};
use openduck_harness::eval::{
    ContinuationCheckpoint, EvalMetrics, EvalRunner, EvalRunnerConfig, EvaluationReport, TaskSpec,
};
use openduck_harness::policy::adapters::EchoPolicy;
use openduck_harness::policy::{
    AgentAction, AgentPolicy, HarnessContextView, MessageRole, StepTelemetry,
};
use openduck_harness::replay::{Cassette, ReplayPolicy};
use openduck_harness::runtime::AgentHarness;
use openduck_harness::sandbox::local::LocalSandbox;
use openduck_harness::sandbox::SandboxDriver;
use openduck_harness::telemetry::{ActiveRuleSummary, TrajectoryRecord, TrajectoryStep};
use openduck_harness::types::{RunStatus, ToolCallRequest, ToolDefinition};
use openduck_providers::model::ModelConfig;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Tool};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};

use crate::config::Config;
use crate::conversation::message::{Message, MessageContent};
use crate::model_config::model_config_from_user_config;
use crate::providers::base::Provider;

pub(crate) const GENERIC_SCRATCH_NOTES_INSTRUCTION: &str = "Maintain Scratchpad Notes: Keep your plan, task progress, and key facts in a per-task scratch notes file under `.agent/` named `.agent/notes-plan-<task-id>.md` (one file per task identity) in the workspace so they survive context compaction. For a new task run, initialize a fresh plan and never assume completion based on artifacts or git commits from prior runs.";

fn sanitize_task_id_for_filename(task_id: &str) -> String {
    let sanitized: String = task_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "task".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Workspace path for the scratch notes-plan file owned by one harness task.
pub fn task_notes_plan_path(task_id: &str) -> String {
    format!(
        ".agent/notes-plan-{}.md",
        sanitize_task_id_for_filename(task_id)
    )
}

pub fn task_scratch_notes_instruction(task_id: &str) -> String {
    let path = task_notes_plan_path(task_id);
    format!(
        "Maintain Scratchpad Notes: Keep your plan, task progress, and key facts in this task's scratch notes file (`{path}`) in the workspace so they survive context compaction. For a new task run, initialize a fresh plan and never assume completion based on artifacts or git commits from prior runs."
    )
}

/// Clean up any stale scratchpad notes file or continuation summary for a task when starting a fresh run.
pub fn clean_stale_task_scratchpad(root: &std::path::Path, task_id: &str) {
    let notes_rel = task_notes_plan_path(task_id);
    let notes_path = root.join(notes_rel);
    if notes_path.exists() {
        let _ = std::fs::remove_file(notes_path);
    }
    let cont_summary_path =
        root.join(openduck_harness::eval::ContinuationCheckpoint::CONTINUATION_SUMMARY_PATH);
    if cont_summary_path.exists() {
        let _ = std::fs::remove_file(cont_summary_path);
    }
}

/// Replace or append the scratch-notes instruction so it names this task's file.
pub fn with_task_notes_plan(system_prompt: &str, task_id: &str) -> String {
    let instruction = task_scratch_notes_instruction(task_id);
    let trailing_newline = system_prompt.ends_with('\n');
    let mut replaced = false;
    let mut out_lines: Vec<String> = Vec::new();
    for line in system_prompt.lines() {
        if line.trim_start().starts_with("Maintain Scratchpad Notes:") {
            if !replaced {
                out_lines.push(instruction.clone());
                replaced = true;
            }
        } else {
            out_lines.push(line.to_string());
        }
    }
    let mut out = if replaced {
        out_lines.join("\n")
    } else {
        let mut prompt = system_prompt.trim_end().to_string();
        if !prompt.is_empty() {
            prompt.push_str("\n\n");
        }
        prompt.push_str(&instruction);
        prompt
    };
    if trailing_newline && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[derive(Clone)]
pub struct GooseAgentPolicy {
    name: String,
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
    system_prompt: String,
    secondary_provider: Option<Arc<dyn Provider>>,
    secondary_model_config: Option<ModelConfig>,
}

impl GooseAgentPolicy {
    pub fn new(
        name: impl Into<String>,
        provider: Arc<dyn Provider>,
        model_config: ModelConfig,
    ) -> Self {
        let provider: Arc<dyn Provider> = if provider.manages_own_context() {
            provider
        } else {
            Arc::new(openduck_context_management::CompactingProvider::new(
                provider,
            ))
        };
        Self {
            name: name.into(),
            provider,
            model_config,
            system_prompt: format!(
                "\nAvoid Redundant Reads: Never re-read the same file multiple times without taking meaningful action or progressing the task. Retain extracted insights in your notes and proceed with execution.\n\n{GENERIC_SCRATCH_NOTES_INSTRUCTION}"
            ),

            secondary_provider: None,
            secondary_model_config: None,
        }
    }

    pub fn with_system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = system_prompt.into();
        self
    }

    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    pub fn with_secondary_provider(
        mut self,
        provider: Arc<dyn Provider>,
        model_config: ModelConfig,
    ) -> Self {
        let provider: Arc<dyn Provider> = if provider.manages_own_context() {
            provider
        } else {
            Arc::new(openduck_context_management::CompactingProvider::new(
                provider,
            ))
        };
        self.secondary_provider = Some(provider);
        self.secondary_model_config = Some(model_config);
        self
    }

    pub fn context_limit(&self) -> Option<usize> {
        self.model_config.context_limit
    }
}

const MAX_EMPTY_TURN_RETRIES: u32 = 4;
const EMPTY_TURN_NUDGE: &str = "Please continue and output a tool call or answer.";
const EMPTY_TURN_RETRY_DELAYS_MS: [u64; 4] = [2_000, 4_000, 8_000, 12_000];
const MAX_TRANSIENT_POLICY_RETRIES: u32 = 2;
const TRANSIENT_RETRY_DELAYS_MS: [u64; 2] = [5_000, 15_000];

fn is_empty_completion_error(err: &anyhow::Error) -> bool {
    let err_str = err.to_string();
    err_str.contains("empty completion") || err_str.contains("only thinking")
}

fn empty_turn_retry_delay_ms(retry_index: u32) -> u64 {
    let delays = if cfg!(test) {
        [0, 0, 0, 0]
    } else {
        EMPTY_TURN_RETRY_DELAYS_MS
    };
    delays
        .get(retry_index.saturating_sub(1) as usize)
        .copied()
        .unwrap_or(0)
}

fn is_transient_complete_error(err: &openduck_providers::errors::ProviderError) -> bool {
    matches!(
        err,
        openduck_providers::errors::ProviderError::NetworkError(_)
            | openduck_providers::errors::ProviderError::ServerError(_)
    )
}

fn transient_retry_delay_ms(retry_index: u32) -> u64 {
    let delays = if cfg!(test) {
        [0, 0]
    } else {
        TRANSIENT_RETRY_DELAYS_MS
    };
    delays
        .get(retry_index.saturating_sub(1) as usize)
        .copied()
        .unwrap_or(0)
}

fn prune_messages_fallback(messages: &[Message]) -> Vec<Message> {
    if messages.len() <= 4 {
        return messages.to_vec();
    }
    let mut pruned = Vec::new();
    if let Some(first) = messages.first() {
        pruned.push(first.clone());
    }
    pruned.push(
        Message::user()
            .with_text("[... Earlier history pruned to fit within model context window ...]"),
    );
    let tail_count = 4.min(messages.len().saturating_sub(1));
    pruned.extend_from_slice(&messages[messages.len() - tail_count..]);
    crate::conversation::fix_conversation(crate::conversation::Conversation::new_unvalidated(
        pruned,
    ))
    .0
    .messages()
    .to_vec()
}

struct ProviderStepResult {
    action: AgentAction,
    reply: Message,
    prompt_messages: Vec<Message>,
    usage: Option<openduck_providers::conversation::token_usage::ProviderUsage>,
}

async fn complete_provider_with_retry(
    provider: &Arc<dyn Provider>,
    model_config: &ModelConfig,
    system_prompt: &str,
    messages: &[Message],
    tools: &[Tool],
    policy_name: &str,
) -> Result<ProviderStepResult> {
    let mut working_messages = messages.to_vec();
    let mut empty_retries = 0;
    let mut transient_retries = 0;

    loop {
        let res = provider
            .complete(model_config, system_prompt, &working_messages, tools)
            .await;

        let (reply, usage) = match res {
            Ok(res) => res,
            Err(openduck_providers::errors::ProviderError::ContextLengthExceeded(ref err_msg)) => {
                tracing::warn!(
                    policy = %policy_name,
                    error = %err_msg,
                    "Context length exceeded, falling back to emergency context pruning"
                );
                let pruned = prune_messages_fallback(&working_messages);
                provider
                    .complete(model_config, system_prompt, &pruned, tools)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?
            }
            Err(e)
                if is_transient_complete_error(&e)
                    && transient_retries < MAX_TRANSIENT_POLICY_RETRIES =>
            {
                transient_retries += 1;
                let delay_ms = transient_retry_delay_ms(transient_retries);
                tracing::warn!(
                    policy = %policy_name,
                    retry = transient_retries,
                    max_retries = MAX_TRANSIENT_POLICY_RETRIES,
                    delay_ms,
                    error = %e,
                    "Transient provider error after HTTP retries; retrying the same step after backoff"
                );
                if delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                continue;
            }
            Err(e) => return Err(anyhow::anyhow!(e)),
        };

        match action_from_provider_reply(&reply) {
            Ok(action) => {
                return Ok(ProviderStepResult {
                    action,
                    reply,
                    prompt_messages: working_messages,
                    usage: Some(usage),
                });
            }
            Err(err)
                if is_empty_completion_error(&err) && empty_retries < MAX_EMPTY_TURN_RETRIES =>
            {
                empty_retries += 1;
                let delay_ms = empty_turn_retry_delay_ms(empty_retries);
                tracing::warn!(
                    policy = %policy_name,
                    retry = empty_retries,
                    max_retries = MAX_EMPTY_TURN_RETRIES,
                    delay_ms,
                    "Model returned an empty completion; retrying with nudge prompt after backoff"
                );
                if delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                working_messages.push(Message::user().with_text(EMPTY_TURN_NUDGE));
            }
            Err(err) => return Err(err),
        }
    }
}

#[async_trait]
impl AgentPolicy for GooseAgentPolicy {
    fn name(&self) -> &str {
        &self.name
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        tools: &[ToolDefinition],
    ) -> Result<AgentAction> {
        let (action, _) = self.step_detailed(context, tools).await?;
        Ok(action)
    }

    async fn step_detailed(
        &mut self,
        context: &HarnessContextView,
        tools: &[ToolDefinition],
    ) -> Result<(AgentAction, Option<StepTelemetry>)> {
        let rmcp_tools: Vec<Tool> = tools
            .iter()
            .map(|t| {
                let input_schema = match &t.parameters {
                    serde_json::Value::Object(map) => map.clone(),
                    _ => serde_json::Map::new(),
                };
                Tool::new(t.name.clone(), t.description.clone(), input_schema)
            })
            .collect();

        let mut messages: Vec<Message> = Vec::new();

        for h_msg in &context.messages {
            match h_msg.role {
                MessageRole::System => {}
                MessageRole::User => {
                    messages.push(Message::user().with_text(&h_msg.content));
                }
                MessageRole::Assistant => {
                    let mut msg = Message::assistant();
                    if !h_msg.content.is_empty() {
                        msg = msg.with_text(&h_msg.content);
                    }
                    if let Some(calls) = &h_msg.tool_calls {
                        for call in calls {
                            let mut params = CallToolRequestParams::new(call.name.clone());
                            if let Some(args) = call.arguments.as_object() {
                                params = params.with_arguments(args.clone());
                            }
                            msg = msg.with_tool_request(call.id.clone(), Ok(params));
                        }
                    }
                    messages.push(msg);
                }
                MessageRole::Tool => {
                    if let Some(results) = &h_msg.tool_results {
                        let mut msg = Message::user();
                        for res in results {
                            let (output, _) = openduck_context_management::snip_tool_output(
                                &res.output,
                                openduck_context_management::snip::DEFAULT_MAX_TOOL_OUTPUT_LINES,
                                openduck_context_management::snip::DEFAULT_MAX_TOOL_OUTPUT_BYTES,
                            );
                            let tool_res = if res.is_error {
                                CallToolResult::error(vec![ContentBlock::text(&output)])
                            } else {
                                CallToolResult::success(vec![ContentBlock::text(&output)])
                            };
                            msg = msg.with_tool_response(res.id.clone(), Ok(tool_res));
                        }
                        messages.push(msg);
                    }
                }
            }
        }

        let mut model_config = self.model_config.clone();
        let mut headers = model_config.request_headers.clone().unwrap_or_default();
        headers
            .entry("x-opencode-session".to_string())
            .or_insert_with(|| context.session_id.clone());
        headers
            .entry("agent-session-id".to_string())
            .or_insert_with(|| context.session_id.clone());
        model_config.request_headers = Some(headers);

        let primary_res = crate::session_context::with_session_id(
            Some(context.session_id.clone()),
            complete_provider_with_retry(
                &self.provider,
                &model_config,
                &self.system_prompt,
                &messages,
                &rmcp_tools,
                &self.name,
            ),
        )
        .await;

        let mut step_res = match primary_res {
            Ok(res) => res,
            Err(primary_err) => {
                if let (Some(sec_provider), Some(sec_model_config)) =
                    (&self.secondary_provider, &self.secondary_model_config)
                {
                    tracing::warn!(
                        policy = %self.name,
                        primary_error = %primary_err,
                        secondary_provider = %sec_provider.get_name(),
                        secondary_model = %sec_model_config.model_name,
                        "Primary provider failed or returned empty completion after retries; switching to secondary provider"
                    );

                    let mut sec_model_config = sec_model_config.clone();
                    let mut sec_headers =
                        sec_model_config.request_headers.clone().unwrap_or_default();
                    sec_headers
                        .entry("x-opencode-session".to_string())
                        .or_insert_with(|| context.session_id.clone());
                    sec_headers
                        .entry("agent-session-id".to_string())
                        .or_insert_with(|| context.session_id.clone());
                    sec_model_config.request_headers = Some(sec_headers);

                    let sec_res = crate::session_context::with_session_id(
                        Some(context.session_id.clone()),
                        complete_provider_with_retry(
                            sec_provider,
                            &sec_model_config,
                            &self.system_prompt,
                            &messages,
                            &rmcp_tools,
                            &self.name,
                        ),
                    )
                    .await;

                    match sec_res {
                        Ok(res) => res,
                        Err(sec_err) => {
                            tracing::error!(
                                policy = %self.name,
                                primary_error = %primary_err,
                                secondary_error = %sec_err,
                                "Both primary and secondary providers failed in GooseAgentPolicy"
                            );
                            return Err(anyhow::anyhow!(
                                "Goose provider complete failed ({}): {primary_err}; secondary provider ({}) also failed: {sec_err}",
                                self.name,
                                sec_provider.get_name()
                            ));
                        }
                    }
                } else {
                    tracing::error!(
                        policy = %self.name,
                        error = %primary_err,
                        "Goose harness provider complete failed"
                    );
                    return Err(anyhow::anyhow!(
                        "Goose provider complete failed ({}): {primary_err}",
                        self.name
                    ));
                }
            }
        };

        let token_usage = step_res
            .usage
            .map(|u| openduck_harness::telemetry::TokenUsageSummary {
                input_tokens: u.usage.input_tokens,
                output_tokens: u.usage.output_tokens,
                total_tokens: u.usage.total_tokens,
            });

        let mut coalesced_content: Vec<MessageContent> = Vec::new();
        for item in step_res.reply.content {
            match (coalesced_content.last_mut(), &item) {
                (Some(MessageContent::Text(last_text)), MessageContent::Text(new_text))
                    if last_text
                        .annotations
                        .as_ref()
                        .and_then(|a| a.audience.as_ref())
                        == new_text
                            .annotations
                            .as_ref()
                            .and_then(|a| a.audience.as_ref()) =>
                {
                    last_text.text.push_str(&new_text.text);
                }
                (
                    Some(MessageContent::Thinking(last_thinking)),
                    MessageContent::Thinking(new_thinking),
                ) if last_thinking.signature.is_empty()
                    || new_thinking.signature == last_thinking.signature =>
                {
                    last_thinking.thinking.push_str(&new_thinking.thinking);
                    if !new_thinking.signature.is_empty() {
                        last_thinking.signature = new_thinking.signature.clone();
                    }
                }
                _ => coalesced_content.push(item),
            }
        }
        step_res.reply.content = coalesced_content;

        let mut reply_text = String::new();
        for item in &step_res.reply.content {
            if let MessageContent::Text(raw) = item {
                if !raw.text.is_empty() {
                    if !reply_text.is_empty() {
                        reply_text.push('\n');
                    }
                    reply_text.push_str(&raw.text);
                }
            }
        }

        let llm_request = serde_json::json!({
            "stepIndex": context.step_count,
            "systemPrompt": self.system_prompt,
            "messagesCount": step_res.prompt_messages.len(),
            "messages": step_res.prompt_messages,
        });

        let llm_response = serde_json::json!({
            "reply": step_res.reply,
            "text": if reply_text.is_empty() { None } else { Some(reply_text) },
            "action": step_res.action,
        });

        let telemetry = StepTelemetry {
            llm_request: Some(llm_request),
            llm_response: Some(llm_response),
            token_usage,
        };

        Ok((step_res.action, Some(telemetry)))
    }
}

fn action_from_provider_reply(reply: impl std::borrow::Borrow<Message>) -> Result<AgentAction> {
    let reply = reply.borrow();
    let mut tool_calls = Vec::new();
    let mut text_parts = Vec::new();
    let mut has_thinking = false;

    for item in &reply.content {
        match item {
            MessageContent::ToolRequest(req) => match &req.tool_call {
                Ok(params) => {
                    let arguments = params
                        .arguments
                        .as_ref()
                        .map(|o| serde_json::Value::Object(o.clone()))
                        .unwrap_or_else(|| serde_json::json!({}));
                    tool_calls.push(ToolCallRequest {
                        id: req.id.clone(),
                        name: params.name.to_string(),
                        arguments,
                    });
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Model attempted tool call '{}' but arguments were malformed: {e:?}",
                        req.id
                    ));
                }
            },
            MessageContent::Text(raw) => {
                if !raw.text.is_empty() {
                    text_parts.push(raw.text.clone());
                }
            }
            MessageContent::Thinking(t) if !t.thinking.is_empty() => {
                has_thinking = true;
            }
            _ => {}
        }
    }

    if !tool_calls.is_empty() {
        Ok(AgentAction::CallTools(tool_calls))
    } else if !text_parts.is_empty() {
        Ok(AgentAction::FinalAnswer(text_parts.join("\n")))
    } else if has_thinking {
        Err(anyhow::anyhow!(
            "Model produced only thinking and no final answer or tool call"
        ))
    } else {
        Err(anyhow::anyhow!(
            "Model returned an empty completion (no text or tool calls)"
        ))
    }
}

pub async fn resolve_harness_policy(
    provider_opt: Option<&str>,
    model_opt: Option<&str>,
) -> Result<GooseAgentPolicy> {
    resolve_harness_policy_with_secondary(provider_opt, model_opt, None, None).await
}

pub async fn resolve_harness_policy_with_secondary(
    provider_opt: Option<&str>,
    model_opt: Option<&str>,
    sec_provider_opt: Option<&str>,
    sec_model_opt: Option<&str>,
) -> Result<GooseAgentPolicy> {
    let config = Config::global();
    let provider_name = match provider_opt {
        Some(p) => p.to_string(),
        None => config.get_goose_provider().context(
            "No default provider configured. Use --provider <name> or run 'goose configure'",
        )?,
    };

    let model_name = match model_opt {
        Some(m) => m.to_string(),
        None => config
            .get_goose_model()
            .unwrap_or_else(|_| "default".to_string()),
    };

    let model_config = model_config_from_user_config(&provider_name, &model_name)?;
    let provider = crate::providers::create(&provider_name, vec![]).await?;

    let policy_name = format!("{}:{}", provider_name, model_name);
    let mut policy = GooseAgentPolicy::new(policy_name, provider, model_config);

    let sec_provider_name = sec_provider_opt
        .map(|s| s.to_string())
        .or_else(|| config.get_goose_secondary_provider().ok());

    if let Some(sec_p) = sec_provider_name {
        let sec_model_name = sec_model_opt
            .map(|s| s.to_string())
            .or_else(|| config.get_goose_secondary_model().ok())
            .unwrap_or_else(|| "default".to_string());
        if let Ok(sec_model_config) = model_config_from_user_config(&sec_p, &sec_model_name) {
            if let Ok(sec_provider) = crate::providers::create(&sec_p, vec![]).await {
                policy = policy.with_secondary_provider(sec_provider, sec_model_config);
            }
        }
    }

    Ok(policy)
}

// ---------------- REST DTOs ----------------

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessEvalRequest {
    pub dataset: String,
    pub concurrency: Option<usize>,
    pub max_turns: Option<usize>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub secondary_provider: Option<String>,
    pub secondary_model: Option<String>,
    pub echo: Option<bool>,
    pub output_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessRunRequest {
    pub task_file: Option<String>,
    pub prompt: Option<String>,
    pub max_turns: Option<usize>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub secondary_provider: Option<String>,
    pub secondary_model: Option<String>,
    pub echo: Option<bool>,
    pub record_path: Option<String>,
    pub sandbox: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessReplayRequest {
    pub cassette_path: String,
    pub task_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessRunResponseDto {
    pub task_id: String,
    pub status: RunStatus,
    pub step_count: usize,
    pub tool_calls_count: usize,
    pub duration_ms: u128,
    pub final_answer: Option<String>,
    pub trajectory: TrajectoryRecord,
    pub recorded_cassette_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_rules: Option<Vec<ActiveRuleSummary>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<ContinuationCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessReportSummaryDto {
    pub id: String,
    pub file_name: String,
    pub path: String,
    pub suite_name: String,
    pub timestamp: String,
    pub total_tasks: usize,
    pub passed_tasks: usize,
    pub failed_tasks: usize,
    pub pass_rate: f64,
    pub avg_duration_ms: f64,
    pub total_tool_calls: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessCassetteSummaryDto {
    pub name: String,
    pub file_name: String,
    pub path: String,
    pub created_at: String,
    pub frame_count: usize,
    pub task_id: Option<String>,
    pub problem_statement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessActiveJobDto {
    pub job_id: String,
    pub task_id: String,
    pub job_type: String, // "task", "eval", "replay"
    pub description: String,
    pub started_at: String,
    pub current_status: String, // "running", "completed", "failed"
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessJobInspectDto {
    pub job: HarnessActiveJobDto,
    pub live: bool,
    pub snapshot: Option<HarnessRunResponseDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessHistorySummaryDto {
    pub id: String,
    pub file_name: String,
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_name: Option<String>,
    pub job_type: String,
    pub status: RunStatus,
    pub step_count: usize,
    pub tool_calls_count: usize,
    pub duration_ms: u128,
    pub final_answer: Option<String>,
    pub started_at: String,
    pub recorded_cassette_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub continuable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_summary: Option<String>,
}

pub fn apply_history_task_names(
    mut history: Vec<HarnessHistorySummaryDto>,
    names: &HashMap<String, String>,
) -> Vec<HarnessHistorySummaryDto> {
    for item in &mut history {
        let missing = item
            .task_name
            .as_deref()
            .map(|name| name.trim().is_empty())
            .unwrap_or(true);
        if !missing {
            continue;
        }
        if let Some(name) = names.get(&item.task_id) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                item.task_name = Some(trimmed.to_string());
            }
        }
    }
    history
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedHarnessRun {
    job_type: String,
    started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    run_response: HarnessRunResponseDto,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HarnessRunSummaryPayload {
    task_id: String,
    status: RunStatus,
    step_count: usize,
    tool_calls_count: usize,
    duration_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    final_answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recorded_cassette_path: Option<String>,
    #[serde(default)]
    continuation: Option<ContinuationSummaryLite>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContinuationSummaryLite {
    #[serde(default)]
    compacted_summary: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedHarnessRunSummaryReader {
    job_type: String,
    started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    run_response: HarnessRunSummaryPayload,
}

#[derive(Debug, Clone, Deserialize)]
struct CassetteTaskSpecSummaryReader {
    #[serde(default)]
    id: String,
    #[serde(default)]
    problem_statement: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CassetteSummaryReader {
    name: String,
    created_at: DateTime<Utc>,
    #[serde(default)]
    task_spec: Option<CassetteTaskSpecSummaryReader>,
    #[serde(default)]
    frames: HashMap<String, serde::de::IgnoredAny>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationReportSummaryReader {
    #[serde(alias = "suite_name")]
    suite_name: String,
    #[serde(alias = "started_at")]
    started_at: DateTime<Utc>,
    metrics: EvalMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HarnessProjectEvent {
    #[serde(rename_all = "camelCase")]
    TaskStatusChanged {
        project_slug: String,
        task_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        job_id: Option<String>,
        status: String,
    },
    #[serde(rename_all = "camelCase")]
    TaskUpserted {
        project_slug: String,
        task_id: String,
    },
    #[serde(rename_all = "camelCase")]
    TaskDeleted {
        project_slug: String,
        task_id: String,
    },
    #[serde(rename_all = "camelCase")]
    ReportGenerated {
        project_slug: String,
        report_id: String,
        report: HarnessReportSummaryDto,
    },
    #[serde(rename_all = "camelCase")]
    OverviewInvalidated { project_slug: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum HarnessJobLiveEvent {
    #[serde(rename_all = "camelCase")]
    StepUpdate {
        job_id: String,
        step_index: usize,
        step: TrajectoryStep,
        duration_ms: u128,
    },
    #[serde(rename_all = "camelCase")]
    Snapshot {
        job_id: String,
        snapshot: Box<HarnessRunResponseDto>,
    },
    #[serde(rename_all = "camelCase")]
    Finished {
        job_id: String,
        status: RunStatus,
        duration_ms: u128,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        final_answer: Option<String>,
    },
}

static HARNESS_PROJECT_EVENTS: LazyLock<broadcast::Sender<HarnessProjectEvent>> =
    LazyLock::new(|| broadcast::channel(1024).0);

static HARNESS_JOB_LIVE_EVENTS: LazyLock<broadcast::Sender<HarnessJobLiveEvent>> =
    LazyLock::new(|| broadcast::channel(1024).0);

pub fn broadcast_project_event(event: HarnessProjectEvent) {
    let _ = HARNESS_PROJECT_EVENTS.send(event);
}

pub fn broadcast_job_live_event(event: HarnessJobLiveEvent) {
    let _ = HARNESS_JOB_LIVE_EVENTS.send(event);
}

pub fn subscribe_project_events() -> broadcast::Receiver<HarnessProjectEvent> {
    HARNESS_PROJECT_EVENTS.subscribe()
}

pub fn subscribe_job_live_events() -> broadcast::Receiver<HarnessJobLiveEvent> {
    HARNESS_JOB_LIVE_EVENTS.subscribe()
}

static HARNESS_ACTIVE_JOBS: LazyLock<Arc<RwLock<HashMap<String, HarnessActiveJobDto>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

static HARNESS_LIVE_SNAPSHOTS: LazyLock<Arc<RwLock<HashMap<String, HarnessRunResponseDto>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

static HARNESS_ABORT_HANDLES: LazyLock<Arc<RwLock<HashMap<String, tokio::task::AbortHandle>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

#[derive(Clone)]
struct CachedSummary<T> {
    mtime: std::time::SystemTime,
    size: u64,
    data: T,
}

#[allow(clippy::type_complexity)]
static HISTORY_SUMMARY_CACHE: LazyLock<
    Arc<RwLock<HashMap<PathBuf, CachedSummary<HarnessHistorySummaryDto>>>>,
> = LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

#[allow(clippy::type_complexity)]
static REPORT_SUMMARY_CACHE: LazyLock<
    Arc<RwLock<HashMap<PathBuf, CachedSummary<HarnessReportSummaryDto>>>>,
> = LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

#[allow(clippy::type_complexity)]
static CASSETTE_SUMMARY_CACHE: LazyLock<
    Arc<RwLock<HashMap<PathBuf, CachedSummary<HarnessCassetteSummaryDto>>>>,
> = LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

#[allow(clippy::type_complexity)]
static HISTORY_DIR_CACHE: LazyLock<
    Arc<RwLock<HashMap<PathBuf, (std::time::SystemTime, Vec<HarnessHistorySummaryDto>)>>>,
> = LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

#[allow(clippy::type_complexity)]
static REPORT_DIR_CACHE: LazyLock<
    Arc<RwLock<HashMap<PathBuf, (std::time::SystemTime, Vec<HarnessReportSummaryDto>)>>>,
> = LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

#[allow(clippy::type_complexity)]
static CASSETTE_DIR_CACHE: LazyLock<
    Arc<RwLock<HashMap<PathBuf, (std::time::SystemTime, Vec<HarnessCassetteSummaryDto>)>>>,
> = LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

pub fn register_abort_handle(job_id: &str, handle: tokio::task::AbortHandle) {
    if let Ok(mut lock) = HARNESS_ABORT_HANDLES.write() {
        lock.insert(job_id.to_string(), handle);
    }
}

pub fn new_harness_job_id(prefix: &str) -> String {
    format!("{}_{}", prefix, uuid::Uuid::now_v7())
}

fn empty_live_snapshot(task_id: &str) -> HarnessRunResponseDto {
    HarnessRunResponseDto {
        task_id: task_id.to_string(),
        status: RunStatus::Running,
        step_count: 0,
        tool_calls_count: 0,
        duration_ms: 0,
        final_answer: None,
        trajectory: TrajectoryRecord {
            session_id: format!("live-{task_id}"),
            task_id: task_id.to_string(),
            policy_name: "pending".to_string(),
            started_at: Utc::now(),
            completed_at: None,
            success: false,
            steps: vec![],
            system_prompt: None,
            active_rules: None,
        },
        recorded_cassette_path: None,
        system_prompt: None,
        active_rules: None,
        continuation: None,
        project_slug: None,
    }
}

pub fn upsert_live_snapshot(job_id: &str, snapshot: HarnessRunResponseDto) {
    if let Ok(mut lock) = HARNESS_LIVE_SNAPSHOTS.write() {
        lock.insert(job_id.to_string(), snapshot);
    }
}

pub fn get_live_snapshot(job_id: &str) -> Option<HarnessRunResponseDto> {
    HARNESS_LIVE_SNAPSHOTS.read().ok()?.get(job_id).cloned()
}

fn live_snapshot_from_steps(
    job_id: &str,
    task_id: &str,
    policy_name: &str,
    started_at: chrono::DateTime<Utc>,
    steps: &[TrajectoryStep],
) -> HarnessRunResponseDto {
    let tool_calls_count = steps
        .iter()
        .map(|step| step.tool_results.as_ref().map(Vec::len).unwrap_or(0))
        .sum();
    let duration_ms = Utc::now()
        .signed_duration_since(started_at)
        .num_milliseconds()
        .max(0) as u128;
    let final_answer = steps.iter().rev().find_map(|step| match &step.action {
        AgentAction::FinalAnswer(ans) => Some(ans.clone()),
        _ => None,
    });
    HarnessRunResponseDto {
        task_id: task_id.to_string(),
        status: RunStatus::Running,
        step_count: steps.len(),
        tool_calls_count,
        duration_ms,
        final_answer,
        trajectory: TrajectoryRecord {
            session_id: format!("live-{job_id}"),
            task_id: task_id.to_string(),
            policy_name: policy_name.to_string(),
            started_at,
            completed_at: None,
            success: false,
            steps: steps.to_vec(),
            system_prompt: None,
            active_rules: None,
        },
        recorded_cassette_path: None,
        system_prompt: None,
        active_rules: None,
        continuation: Some(openduck_harness::checkpoint_from_trajectory(
            task_id,
            job_id,
            RunStatus::Running,
            None,
            steps,
        )),
        project_slug: None,
    }
}

pub fn with_live_snapshot<P, S>(
    harness: AgentHarness<P, S>,
    job_id: &str,
    task_id: &str,
) -> AgentHarness<P, S>
where
    P: openduck_harness::policy::AgentPolicy,
    S: SandboxDriver,
{
    let job_id = job_id.to_string();
    let task_id = task_id.to_string();
    let policy_name = harness.policy_name().to_string();
    let started_at = Utc::now();
    harness.with_step_observer(Arc::new(move |steps| {
        let snapshot = live_snapshot_from_steps(&job_id, &task_id, &policy_name, started_at, steps);
        upsert_live_snapshot(&job_id, snapshot.clone());
        if let Some(last_step) = steps.last() {
            broadcast_job_live_event(HarnessJobLiveEvent::StepUpdate {
                job_id: job_id.clone(),
                step_index: steps.len().saturating_sub(1),
                step: last_step.clone(),
                duration_ms: snapshot.duration_ms,
            });
        }
        broadcast_job_live_event(HarnessJobLiveEvent::Snapshot {
            job_id: job_id.clone(),
            snapshot: Box::new(snapshot),
        });
    }))
}

pub fn to_run_dto(
    res: openduck_harness::runtime::TaskExecutionResult,
    recorded_cassette_path: Option<String>,
) -> HarnessRunResponseDto {
    let system_prompt = res.trajectory.system_prompt.clone();
    let active_rules = res.trajectory.active_rules.clone();
    HarnessRunResponseDto {
        task_id: res.task_id,
        status: res.status,
        step_count: res.step_count,
        tool_calls_count: res.tool_calls_count,
        duration_ms: res.duration_ms,
        final_answer: res.final_answer,
        system_prompt,
        active_rules,
        trajectory: res.trajectory,
        recorded_cassette_path,
        continuation: res.continuation,
        project_slug: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskAlreadyRunningError {
    pub task_id: String,
    pub job_id: String,
}

impl std::fmt::Display for TaskAlreadyRunningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Task '{}' is already running (job {})",
            self.task_id, self.job_id
        )
    }
}

impl std::error::Error for TaskAlreadyRunningError {}

fn find_running_project_task<'a>(
    jobs: &'a HashMap<String, HarnessActiveJobDto>,
    job: &HarnessActiveJobDto,
) -> Option<&'a HarnessActiveJobDto> {
    if job.job_type != "task" {
        return None;
    }
    jobs.values().find(|running| {
        running.job_type == "task"
            && running.task_id == job.task_id
            && running.project_slug == job.project_slug
    })
}

pub fn register_active_job(job: HarnessActiveJobDto) {
    if let Some(slug) = &job.project_slug {
        broadcast_project_event(HarnessProjectEvent::TaskStatusChanged {
            project_slug: slug.clone(),
            task_id: job.task_id.clone(),
            job_id: Some(job.job_id.clone()),
            status: job.current_status.clone(),
        });
    }
    if let Ok(mut lock) = HARNESS_ACTIVE_JOBS.write() {
        lock.insert(job.job_id.clone(), job.clone());
    }
    upsert_live_snapshot(&job.job_id, empty_live_snapshot(&job.task_id));
}

pub fn try_register_active_job(job: HarnessActiveJobDto) -> Result<(), TaskAlreadyRunningError> {
    {
        let mut lock = HARNESS_ACTIVE_JOBS
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = find_running_project_task(&lock, &job) {
            return Err(TaskAlreadyRunningError {
                task_id: job.task_id,
                job_id: existing.job_id.clone(),
            });
        }
        lock.insert(job.job_id.clone(), job.clone());
    }
    if let Some(slug) = &job.project_slug {
        broadcast_project_event(HarnessProjectEvent::TaskStatusChanged {
            project_slug: slug.clone(),
            task_id: job.task_id.clone(),
            job_id: Some(job.job_id.clone()),
            status: job.current_status.clone(),
        });
    }
    upsert_live_snapshot(&job.job_id, empty_live_snapshot(&job.task_id));
    Ok(())
}

pub fn update_active_job_status(job_id: &str, status: &str) {
    let mut project_task_info = None;
    if let Ok(mut lock) = HARNESS_ACTIVE_JOBS.write() {
        if let Some(job) = lock.get_mut(job_id) {
            job.current_status = status.to_string();
            if let Some(slug) = &job.project_slug {
                project_task_info = Some((slug.clone(), job.task_id.clone()));
            }
        }
    }
    if let Some((slug, task_id)) = project_task_info {
        broadcast_project_event(HarnessProjectEvent::TaskStatusChanged {
            project_slug: slug,
            task_id,
            job_id: Some(job_id.to_string()),
            status: status.to_string(),
        });
    }
}

pub fn remove_active_job(job_id: &str) {
    let mut project_task_info = None;
    if let Ok(mut lock) = HARNESS_ACTIVE_JOBS.write() {
        if let Some(job) = lock.remove(job_id) {
            if let Some(slug) = job.project_slug {
                project_task_info = Some((slug, job.task_id));
            }
        }
    }
    if let Ok(mut lock) = HARNESS_LIVE_SNAPSHOTS.write() {
        lock.remove(job_id);
    }
    if let Ok(mut lock) = HARNESS_ABORT_HANDLES.write() {
        lock.remove(job_id);
    }
    if let Some((slug, task_id)) = project_task_info {
        broadcast_project_event(HarnessProjectEvent::TaskStatusChanged {
            project_slug: slug,
            task_id,
            job_id: None,
            status: "idle".to_string(),
        });
    }
}

pub fn get_active_job(job_id: &str) -> Option<HarnessActiveJobDto> {
    HARNESS_ACTIVE_JOBS.read().ok()?.get(job_id).cloned()
}

pub fn inspect_active_job(job_id: &str) -> Option<HarnessJobInspectDto> {
    let job = get_active_job(job_id)?;
    Some(HarnessJobInspectDto {
        job,
        live: true,
        snapshot: get_live_snapshot(job_id),
    })
}

pub fn get_active_jobs_list() -> Vec<HarnessActiveJobDto> {
    if let Ok(lock) = HARNESS_ACTIVE_JOBS.read() {
        let mut jobs: Vec<_> = lock.values().cloned().collect();
        jobs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        jobs
    } else {
        Vec::new()
    }
}

pub fn active_jobs_for_project(slug: &str) -> Vec<HarnessActiveJobDto> {
    get_active_jobs_list()
        .into_iter()
        .filter(|job| job.project_slug.as_deref() == Some(slug))
        .collect()
}

const LEGACY_HARNESS_RUNS_DIR: &str = "harness_runs";
pub const PROJECT_HARNESS_RUNS_REL: &str = ".goose/harness_runs";

pub fn global_runs_dir() -> PathBuf {
    crate::config::paths::Paths::in_state_dir("harness_runs")
}

pub fn project_runs_dir(project_root: &Path) -> PathBuf {
    project_root.join(PROJECT_HARNESS_RUNS_REL)
}

fn sanitize_task_id(task_id: &str) -> String {
    task_id
        .replace(['/', '\\', ' ', ':'], "_")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn history_summary_from_payload(
    path: &Path,
    payload: &PersistedHarnessRun,
) -> HarnessHistorySummaryDto {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let id = file_name
        .strip_suffix(".json")
        .unwrap_or(&file_name)
        .to_string();
    let run = &payload.run_response;
    HarnessHistorySummaryDto {
        id,
        file_name,
        task_id: run.task_id.clone(),
        task_name: None,
        job_type: payload.job_type.clone(),
        status: run.status,
        step_count: run.step_count,
        tool_calls_count: run.tool_calls_count,
        duration_ms: run.duration_ms,
        final_answer: run.final_answer.clone(),
        started_at: payload.started_at.clone(),
        recorded_cassette_path: run.recorded_cassette_path.clone(),
        project_slug: payload.project_slug.clone(),
        error: payload.error.clone(),
        continuable: openduck_harness::is_continuable_status(run.status),
        progress_summary: run
            .continuation
            .as_ref()
            .map(|c| c.compacted_summary.clone())
            .filter(|s| !s.trim().is_empty()),
    }
}

fn cache_history_summary_file(path: &Path, summary: HarnessHistorySummaryDto) {
    let meta = path.metadata().ok();
    let mtime = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let size = meta.map(|m| m.len()).unwrap_or(0);
    if let Ok(mut lock) = HISTORY_SUMMARY_CACHE.write() {
        lock.insert(
            path.to_path_buf(),
            CachedSummary {
                mtime,
                size,
                data: summary,
            },
        );
    }
}

fn append_cached_history_dir(dir: &Path, summary: HarnessHistorySummaryDto) {
    let Ok(dir_mtime) = dir.metadata().and_then(|m| m.modified()) else {
        return;
    };
    if let Ok(mut lock) = HISTORY_DIR_CACHE.write() {
        let Some((cached_mtime, list)) = lock.get_mut(dir) else {
            return;
        };
        list.retain(|item| item.file_name != summary.file_name);
        list.push(summary);
        list.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        *cached_mtime = dir_mtime;
    }
}

#[allow(clippy::string_slice)]
fn new_run_file_name(payload: &PersistedHarnessRun) -> String {
    let timestamp_prefix = Utc::now().format("%Y%m%d_%H%M%S");
    let safe_task_id = sanitize_task_id(&payload.run_response.task_id);
    let unique = uuid::Uuid::now_v7().simple().to_string();
    format!(
        "{}_{}_{}_{}.json",
        timestamp_prefix,
        payload.job_type,
        safe_task_id,
        &unique[..8]
    )
}

#[allow(dead_code)]
fn write_persisted_run(dir: &Path, payload: &PersistedHarnessRun) -> Result<PathBuf> {
    write_persisted_run_named(dir, &new_run_file_name(payload), payload)
}

fn write_persisted_run_named(
    dir: &Path,
    file_name: &str,
    payload: &PersistedHarnessRun,
) -> Result<PathBuf> {
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    }
    let file_path = dir.join(file_name);
    let json = serde_json::to_string_pretty(payload)?;
    std::fs::write(&file_path, json)?;
    let summary = history_summary_from_payload(&file_path, payload);
    cache_history_summary_file(&file_path, summary.clone());
    append_cached_history_dir(dir, summary);
    Ok(file_path)
}

pub fn persist_harness_run(
    job_type: &str,
    run_dto: &HarnessRunResponseDto,
    started_at: &str,
    project_slug: Option<&str>,
    project_root: Option<&Path>,
    error: Option<&str>,
) -> Result<()> {
    let payload = PersistedHarnessRun {
        job_type: job_type.to_string(),
        started_at: started_at.to_string(),
        project_slug: project_slug.map(ToOwned::to_owned),
        error: error.map(ToOwned::to_owned),
        run_response: run_dto.clone(),
    };
    let file_name = new_run_file_name(&payload);
    write_persisted_run_named(&global_runs_dir(), &file_name, &payload)?;
    if let Some(root) = project_root {
        write_persisted_run_named(&project_runs_dir(root), &file_name, &payload)?;
    }
    if let Some(slug) = project_slug {
        broadcast_project_event(HarnessProjectEvent::OverviewInvalidated {
            project_slug: slug.to_string(),
        });
    }
    Ok(())
}

pub fn failed_run_dto(
    task_id: &str,
    error: &str,
    snapshot: Option<HarnessRunResponseDto>,
) -> HarnessRunResponseDto {
    if let Some(mut s) = snapshot {
        s.status = RunStatus::Failure;
        s.final_answer = Some(error.to_string());
        s.trajectory.completed_at = Some(Utc::now());
        s.trajectory.success = false;
        s.continuation = Some(openduck_harness::checkpoint_from_trajectory(
            task_id,
            &s.trajectory.session_id,
            RunStatus::Failure,
            Some(error.to_string()),
            &s.trajectory.steps,
        ));
        s
    } else {
        HarnessRunResponseDto {
            task_id: task_id.to_string(),
            status: RunStatus::Failure,
            step_count: 0,
            tool_calls_count: 0,
            duration_ms: 0,
            final_answer: Some(error.to_string()),
            trajectory: TrajectoryRecord {
                session_id: format!("harness-failed-{task_id}"),
                task_id: task_id.to_string(),
                policy_name: "unknown".to_string(),
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                success: false,
                steps: vec![],
                system_prompt: None,
                active_rules: None,
            },
            recorded_cassette_path: None,
            system_prompt: None,
            active_rules: None,
            continuation: None,
            project_slug: None,
        }
    }
}

pub fn skipped_run_dto(task_id: &str, reason: &str) -> HarnessRunResponseDto {
    HarnessRunResponseDto {
        task_id: task_id.to_string(),
        status: RunStatus::Skipped,
        step_count: 0,
        tool_calls_count: 0,
        duration_ms: 0,
        final_answer: Some(reason.to_string()),
        trajectory: TrajectoryRecord {
            session_id: format!("harness-skipped-{task_id}"),
            task_id: task_id.to_string(),
            policy_name: "scheduler".to_string(),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            success: false,
            steps: vec![],
            system_prompt: None,
            active_rules: None,
        },
        recorded_cassette_path: None,
        system_prompt: None,
        active_rules: None,
        continuation: None,
        project_slug: None,
    }
}

pub fn cancelled_run_dto(
    task_id: &str,
    reason: &str,
    snapshot: Option<HarnessRunResponseDto>,
) -> HarnessRunResponseDto {
    if let Some(mut s) = snapshot {
        s.status = RunStatus::Cancelled;
        s.final_answer = Some(reason.to_string());
        s.trajectory.completed_at = Some(Utc::now());
        s.trajectory.success = false;
        s.continuation = Some(openduck_harness::checkpoint_from_trajectory(
            task_id,
            &s.trajectory.session_id,
            RunStatus::Cancelled,
            Some(reason.to_string()),
            &s.trajectory.steps,
        ));
        s
    } else {
        HarnessRunResponseDto {
            task_id: task_id.to_string(),
            status: RunStatus::Cancelled,
            step_count: 0,
            tool_calls_count: 0,
            duration_ms: 0,
            final_answer: Some(reason.to_string()),
            trajectory: TrajectoryRecord {
                session_id: format!("harness-cancelled-{task_id}"),
                task_id: task_id.to_string(),
                policy_name: "stopped".to_string(),
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                success: false,
                steps: vec![],
                system_prompt: None,
                active_rules: None,
            },
            recorded_cassette_path: None,
            system_prompt: None,
            active_rules: None,
            continuation: Some(openduck_harness::checkpoint_from_trajectory(
                task_id,
                format!("harness-cancelled-{task_id}"),
                RunStatus::Cancelled,
                Some(reason.to_string()),
                &[],
            )),
            project_slug: None,
        }
    }
}

pub fn stop_active_job(job_id: &str, reason: Option<&str>) -> Result<HarnessRunResponseDto> {
    let job = get_active_job(job_id)
        .ok_or_else(|| anyhow!("No active harness job with ID '{job_id}'"))?;

    if let Ok(mut lock) = HARNESS_ABORT_HANDLES.write() {
        if let Some(handle) = lock.remove(job_id) {
            handle.abort();
        }
    }

    let live_snapshot = get_live_snapshot(job_id);
    let reason_str = reason.unwrap_or("Task stopped by user");
    let cancelled = cancelled_run_dto(&job.task_id, reason_str, live_snapshot);

    let (project_root, slug) = if let Some(slug) = &job.project_slug {
        match super::project_harness::resolve_project_root(slug) {
            Ok((_, root)) => (Some(root), Some(slug.clone())),
            Err(_) => (None, Some(slug.clone())),
        }
    } else {
        (None, None)
    };

    let _ = persist_harness_run(
        &job.job_type,
        &cancelled,
        &job.started_at,
        slug.as_deref(),
        project_root.as_deref(),
        Some(reason_str),
    );

    remove_active_job(job_id);
    broadcast_job_live_event(HarnessJobLiveEvent::Finished {
        job_id: job_id.to_string(),
        status: RunStatus::Cancelled,
        duration_ms: cancelled.duration_ms,
        final_answer: cancelled.final_answer.clone(),
    });
    Ok(cancelled)
}

fn history_summary_from_file(path: &Path) -> Option<HarnessHistorySummaryDto> {
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let p: PersistedHarnessRunSummaryReader = serde_json::from_reader(reader).ok()?;
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let id = file_name
        .strip_suffix(".json")
        .unwrap_or(&file_name)
        .to_string();
    let run = p.run_response;
    Some(HarnessHistorySummaryDto {
        id,
        file_name,
        task_id: run.task_id,
        task_name: None,
        job_type: p.job_type,
        status: run.status,
        step_count: run.step_count,
        tool_calls_count: run.tool_calls_count,
        duration_ms: run.duration_ms,
        final_answer: run.final_answer,
        started_at: p.started_at,
        recorded_cassette_path: run.recorded_cassette_path,
        project_slug: p.project_slug,
        error: p.error,
        continuable: openduck_harness::is_continuable_status(run.status),
        progress_summary: run.continuation.and_then(|c| {
            if c.compacted_summary.trim().is_empty() {
                None
            } else {
                Some(c.compacted_summary)
            }
        }),
    })
}

fn cached_history_summary(path: &Path) -> Option<HarnessHistorySummaryDto> {
    let meta = path.metadata().ok()?;
    let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let size = meta.len();

    if let Ok(lock) = HISTORY_SUMMARY_CACHE.read() {
        if let Some(entry) = lock.get(path) {
            if entry.mtime == mtime && entry.size == size {
                return Some(entry.data.clone());
            }
        }
    }

    let summary = history_summary_from_file(path)?;
    if let Ok(mut lock) = HISTORY_SUMMARY_CACHE.write() {
        lock.insert(
            path.to_path_buf(),
            CachedSummary {
                mtime,
                size,
                data: summary.clone(),
            },
        );
    }
    Some(summary)
}

fn report_summary_from_file(path: &Path) -> Option<HarnessReportSummaryDto> {
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let report: EvaluationReportSummaryReader = serde_json::from_reader(reader).ok()?;
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    Some(HarnessReportSummaryDto {
        id: file_name.trim_end_matches(".json").to_string(),
        file_name,
        path: path.to_string_lossy().to_string(),
        suite_name: report.suite_name,
        timestamp: report.started_at.to_rfc3339(),
        total_tasks: report.metrics.total_tasks,
        passed_tasks: report.metrics.passed_tasks,
        failed_tasks: report.metrics.failed_tasks,
        pass_rate: report.metrics.pass_rate,
        avg_duration_ms: report.metrics.avg_duration_ms,
        total_tool_calls: report.metrics.total_tool_calls,
    })
}

fn cached_report_summary(path: &Path) -> Option<HarnessReportSummaryDto> {
    let meta = path.metadata().ok()?;
    let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let size = meta.len();

    if let Ok(lock) = REPORT_SUMMARY_CACHE.read() {
        if let Some(entry) = lock.get(path) {
            if entry.mtime == mtime && entry.size == size {
                return Some(entry.data.clone());
            }
        }
    }

    let summary = report_summary_from_file(path)?;
    if let Ok(mut lock) = REPORT_SUMMARY_CACHE.write() {
        lock.insert(
            path.to_path_buf(),
            CachedSummary {
                mtime,
                size,
                data: summary.clone(),
            },
        );
    }
    Some(summary)
}

fn cassette_summary_from_file(path: &Path) -> Option<HarnessCassetteSummaryDto> {
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let cas: CassetteSummaryReader = serde_json::from_reader(reader).ok()?;
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let task_id = cas
        .task_spec
        .as_ref()
        .map(|s| s.id.clone())
        .filter(|s| !s.is_empty());
    let problem_statement = cas
        .task_spec
        .as_ref()
        .map(|s| s.problem_statement.clone())
        .filter(|s| !s.is_empty());
    Some(HarnessCassetteSummaryDto {
        name: cas.name,
        file_name,
        path: path.to_string_lossy().to_string(),
        created_at: cas.created_at.to_rfc3339(),
        frame_count: cas.frames.len(),
        task_id,
        problem_statement,
    })
}

fn cached_cassette_summary(path: &Path) -> Option<HarnessCassetteSummaryDto> {
    let meta = path.metadata().ok()?;
    let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let size = meta.len();

    if let Ok(lock) = CASSETTE_SUMMARY_CACHE.read() {
        if let Some(entry) = lock.get(path) {
            if entry.mtime == mtime && entry.size == size {
                return Some(entry.data.clone());
            }
        }
    }

    let summary = cassette_summary_from_file(path)?;
    if let Ok(mut lock) = CASSETTE_SUMMARY_CACHE.write() {
        lock.insert(
            path.to_path_buf(),
            CachedSummary {
                mtime,
                size,
                data: summary.clone(),
            },
        );
    }
    Some(summary)
}

pub fn internal_list_history(dir: &Path) -> Result<Vec<HarnessHistorySummaryDto>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let dir_mtime = dir
        .metadata()
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    if let Ok(lock) = HISTORY_DIR_CACHE.read() {
        if let Some((cached_mtime, cached_list)) = lock.get(dir) {
            if *cached_mtime == dir_mtime {
                return Ok(cached_list.clone());
            }
        }
    }

    let mut list = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Some(summary) = cached_history_summary(&path) {
                list.push(summary);
            }
        }
    }
    list.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    let dir_mtime = dir
        .metadata()
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    if let Ok(mut lock) = HISTORY_DIR_CACHE.write() {
        lock.insert(dir.to_path_buf(), (dir_mtime, list.clone()));
    }
    Ok(list)
}

fn merge_history_lists(
    lists: impl IntoIterator<Item = Vec<HarnessHistorySummaryDto>>,
) -> Vec<HarnessHistorySummaryDto> {
    let mut by_id: HashMap<String, HarnessHistorySummaryDto> = HashMap::new();
    for list in lists {
        for item in list {
            by_id.entry(item.id.clone()).or_insert(item);
        }
    }
    let mut merged: Vec<_> = by_id.into_values().collect();
    merged.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    merged
}

pub fn list_all_history() -> Vec<HarnessHistorySummaryDto> {
    let global = internal_list_history(&global_runs_dir()).unwrap_or_default();
    let legacy = internal_list_history(Path::new(LEGACY_HARNESS_RUNS_DIR)).unwrap_or_default();
    merge_history_lists([global, legacy])
}

pub fn list_history_for_project(
    slug: &str,
    project_root: Option<&Path>,
) -> Vec<HarnessHistorySummaryDto> {
    if let Some(root) = project_root {
        let project_list = internal_list_history(&project_runs_dir(root)).unwrap_or_default();
        if !project_list.is_empty() {
            return project_list;
        }
    }
    list_all_history()
        .into_iter()
        .filter(|h| h.project_slug.as_deref() == Some(slug))
        .collect()
}

fn history_search_dirs(extra_dirs: &[&Path]) -> Vec<PathBuf> {
    let mut dirs = vec![global_runs_dir(), PathBuf::from(LEGACY_HARNESS_RUNS_DIR)];
    for extra in extra_dirs {
        dirs.push(extra.to_path_buf());
        dirs.push(extra.join(PROJECT_HARNESS_RUNS_REL));
    }
    dirs
}

fn run_file_stem(run_id: &str) -> &str {
    run_id.strip_suffix(".json").unwrap_or(run_id)
}

fn strip_run_timestamp_prefix(stem: &str) -> &str {
    // YYYYMMDD_HHMMSS_<rest>
    if stem.len() > 16
        && stem.as_bytes().get(8) == Some(&b'_')
        && stem.as_bytes().get(15) == Some(&b'_')
    {
        stem.get(16..).unwrap_or(stem)
    } else {
        stem
    }
}

fn history_run_aliases(stem: &str, candidate_stem: &str) -> bool {
    if candidate_stem == stem {
        return true;
    }
    strip_run_timestamp_prefix(candidate_stem) == strip_run_timestamp_prefix(stem)
}

fn load_persisted_run(path: &Path) -> Result<HarnessRunResponseDto> {
    let content = std::fs::read_to_string(path)?;
    let p: PersistedHarnessRun = serde_json::from_str(&content)?;
    let mut run = p.run_response;
    if run.project_slug.is_none() {
        run.project_slug = p.project_slug;
    }
    Ok(run)
}

fn find_history_run_by_alias(run_id: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let stem = run_file_stem(run_id);
    let mut matches = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let name = path.file_name()?.to_string_lossy();
            let candidate = run_file_stem(&name);
            if history_run_aliases(stem, candidate) {
                let mtime = path.metadata().and_then(|m| m.modified()).ok();
                matches.push((mtime, path));
            }
        }
    }
    matches.sort_by_key(|b| std::cmp::Reverse(b.0));
    matches.into_iter().next().map(|(_, path)| path)
}

pub fn read_history_run(run_id: &str, extra_dirs: &[&Path]) -> Result<HarnessRunResponseDto> {
    let dirs = history_search_dirs(extra_dirs);
    let file_name = if run_id.ends_with(".json") {
        run_id.to_string()
    } else {
        format!("{run_id}.json")
    };

    for dir in &dirs {
        let target = dir.join(&file_name);
        if target.exists() {
            return load_persisted_run(&target);
        }
    }

    if let Some(path) = find_history_run_by_alias(run_id, &dirs) {
        return load_persisted_run(&path);
    }

    Err(anyhow::anyhow!("History run not found: {run_id}"))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessOverviewResponse {
    pub available_datasets: Vec<String>,
    pub reports: Vec<HarnessReportSummaryDto>,
    pub cassettes: Vec<HarnessCassetteSummaryDto>,
    pub active_jobs: Vec<HarnessActiveJobDto>,
    pub history: Vec<HarnessHistorySummaryDto>,
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
}

// ---------------- REST HANDLERS ----------------

fn bad<T: std::fmt::Display>(e: T) -> (StatusCode, Json<serde_json::Value>) {
    let message = format!("{e:#}");
    tracing::error!(error = %message, "Harness request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": message })),
    )
}

fn not_found<T: std::fmt::Display>(e: T) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
}

pub async fn list_active_harness_jobs() -> Json<Vec<HarnessActiveJobDto>> {
    Json(get_active_jobs_list())
}

pub async fn inspect_harness_job(
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<HarnessJobInspectDto>, (StatusCode, Json<serde_json::Value>)> {
    inspect_active_job(&job_id)
        .map(Json)
        .ok_or_else(|| not_found(format!("No running harness job: {job_id}")))
}

pub async fn stop_active_harness_job(
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cancelled = stop_active_job(&job_id, Some("Stopped by user")).map_err(bad)?;
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Stopped job {job_id}"),
        "jobId": job_id,
        "taskId": cancelled.task_id,
        "status": "cancelled",
    })))
}

pub async fn list_harness_history(
) -> Result<Json<Vec<HarnessHistorySummaryDto>>, (StatusCode, Json<serde_json::Value>)> {
    let history = tokio::task::spawn_blocking(list_all_history)
        .await
        .map_err(|e| bad(e.to_string()))?;
    Ok(Json(history))
}

pub async fn get_harness_history_detail(
    AxumPath(run_id): AxumPath<String>,
) -> Result<Json<HarnessRunResponseDto>, (StatusCode, Json<serde_json::Value>)> {
    read_history_run(&run_id, &[]).map(Json).map_err(not_found)
}

pub async fn get_harness_overview(
) -> Result<Json<HarnessOverviewResponse>, (StatusCode, Json<serde_json::Value>)> {
    tokio::task::spawn_blocking(|| {
        let mut datasets = Vec::new();
        let root = Path::new(".");

        // Look for common dataset files (.yaml, .jsonl) in root and evals/
        let check_paths = [
            PathBuf::from("polymarket_task.yaml"),
            PathBuf::from("goose-self-test.yaml"),
        ];
        for p in &check_paths {
            if p.exists() {
                datasets.push(p.to_string_lossy().to_string());
            }
        }
        if let Ok(entries) = std::fs::read_dir("evals") {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "yaml" || ext == "yml" || ext == "jsonl" {
                        datasets.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }

        let reports = internal_list_reports(Path::new("harness_eval_results")).unwrap_or_default();
        let cassettes = internal_list_cassettes(root).unwrap_or_default();
        let active_jobs = get_active_jobs_list();
        let history = list_all_history();

        let config = Config::global();
        let default_provider = config.get_goose_provider().ok();
        let default_model = config.get_goose_model().ok();

        Ok(Json(HarnessOverviewResponse {
            available_datasets: datasets,
            reports,
            cassettes,
            active_jobs,
            history,
            default_provider,
            default_model,
        }))
    })
    .await
    .map_err(|e| bad(format!("Failed to build harness overview: {e}")))?
}

pub(crate) fn internal_list_reports(dir: &Path) -> Result<Vec<HarnessReportSummaryDto>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let dir_mtime = dir
        .metadata()
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    if let Ok(lock) = REPORT_DIR_CACHE.read() {
        if let Some((cached_mtime, cached_list)) = lock.get(dir) {
            if *cached_mtime == dir_mtime {
                return Ok(cached_list.clone());
            }
        }
    }

    let mut list = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Some(summary) = cached_report_summary(&path) {
                list.push(summary);
            }
        }
    }
    list.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    if let Ok(mut lock) = REPORT_DIR_CACHE.write() {
        lock.insert(dir.to_path_buf(), (dir_mtime, list.clone()));
    }
    Ok(list)
}

pub(crate) fn internal_list_cassettes(root: &Path) -> Result<Vec<HarnessCassetteSummaryDto>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let dir_mtime = root
        .metadata()
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    if let Ok(lock) = CASSETTE_DIR_CACHE.read() {
        if let Some((cached_mtime, cached_list)) = lock.get(root) {
            if *cached_mtime == dir_mtime {
                return Ok(cached_list.clone());
            }
        }
    }

    let mut list = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name.ends_with(".cassette.json") || name == "cassette.json" {
                    if let Some(summary) = cached_cassette_summary(&path) {
                        list.push(summary);
                    }
                }
            }
        }
    }
    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    if let Ok(mut lock) = CASSETTE_DIR_CACHE.write() {
        lock.insert(root.to_path_buf(), (dir_mtime, list.clone()));
    }
    Ok(list)
}

pub async fn list_harness_reports(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<HarnessReportSummaryDto>>, (StatusCode, Json<serde_json::Value>)> {
    let output_dir = params
        .get("dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("harness_eval_results"));
    let reports = internal_list_reports(&output_dir).map_err(bad)?;
    Ok(Json(reports))
}

pub async fn get_harness_report(
    AxumPath(report_id): AxumPath<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<EvaluationReport>, (StatusCode, Json<serde_json::Value>)> {
    let output_dir = params
        .get("dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("harness_eval_results"));
    let target = if report_id.ends_with(".json") {
        output_dir.join(&report_id)
    } else {
        output_dir.join(format!("{}.json", report_id))
    };
    if !target.exists() {
        return Err(not_found(format!("Report not found at {:?}", target)));
    }
    let content = std::fs::read_to_string(&target).map_err(bad)?;
    let report: EvaluationReport = serde_json::from_str(&content).map_err(bad)?;
    Ok(Json(report))
}

pub async fn list_harness_cassettes(
) -> Result<Json<Vec<HarnessCassetteSummaryDto>>, (StatusCode, Json<serde_json::Value>)> {
    let list = internal_list_cassettes(Path::new(".")).map_err(bad)?;
    Ok(Json(list))
}

pub async fn get_harness_cassette(
    AxumPath(cassette_name): AxumPath<String>,
) -> Result<Json<Cassette>, (StatusCode, Json<serde_json::Value>)> {
    let target = PathBuf::from(&cassette_name);
    let path = if target.exists() {
        target
    } else {
        PathBuf::from(format!("{}.json", cassette_name))
    };
    if !path.exists() {
        return Err(not_found(format!("Cassette file not found: {:?}", path)));
    }
    let cas = Cassette::load_from_file(&path).await.map_err(bad)?;
    Ok(Json(cas))
}

pub async fn run_harness_eval(
    Json(req): Json<HarnessEvalRequest>,
) -> Result<Json<EvaluationReport>, (StatusCode, Json<serde_json::Value>)> {
    let dataset_path = PathBuf::from(&req.dataset);
    if !dataset_path.exists() {
        return Err(not_found(format!(
            "Dataset path does not exist: {:?}",
            dataset_path
        )));
    }

    let job_id = new_harness_job_id("eval");
    let started_at = Utc::now().to_rfc3339();
    register_active_job(HarnessActiveJobDto {
        job_id: job_id.clone(),
        task_id: req.dataset.clone(),
        job_type: "eval".to_string(),
        description: format!("Evaluation benchmark on {}", req.dataset),
        started_at: started_at.clone(),
        current_status: "running".to_string(),
        provider: req.provider.clone(),
        model: req.model.clone(),
        project_slug: None,
    });

    let items = if dataset_path.extension().and_then(|e| e.to_str()) == Some("yaml")
        || dataset_path.extension().and_then(|e| e.to_str()) == Some("yml")
    {
        match load_recipe_task(&dataset_path).await {
            Ok(item) => vec![item],
            Err(e) => {
                remove_active_job(&job_id);
                return Err(bad(e));
            }
        }
    } else {
        match load_jsonl_dataset(&dataset_path).await {
            Ok(items) => items,
            Err(e) => {
                remove_active_job(&job_id);
                return Err(bad(e));
            }
        }
    };

    let output_dir = req
        .output_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("harness_eval_results"));
    let config = EvalRunnerConfig {
        concurrency: req.concurrency.unwrap_or(4),
        output_dir: output_dir.clone(),
        max_turns: req.max_turns.unwrap_or(25),
    };

    let runner = EvalRunner::new(config);
    let suite_name = dataset_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "benchmark".into());

    let report_res = if req.echo.unwrap_or(false) {
        runner
            .run_suite(&suite_name, items, EchoPolicy::default)
            .await
    } else {
        let policy = match resolve_harness_policy_with_secondary(
            req.provider.as_deref(),
            req.model.as_deref(),
            req.secondary_provider.as_deref(),
            req.secondary_model.as_deref(),
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                remove_active_job(&job_id);
                return Err(bad(e));
            }
        };
        let policy_template = policy.clone();
        runner
            .run_suite(&suite_name, items, move || policy_template.clone())
            .await
    };

    remove_active_job(&job_id);
    let report = report_res.map_err(bad)?;
    Ok(Json(report))
}

pub async fn run_harness_task(
    Json(req): Json<HarnessRunRequest>,
) -> Result<Json<HarnessRunResponseDto>, (StatusCode, Json<serde_json::Value>)> {
    let task_spec = if let Some(task_file) = &req.task_file {
        let p = PathBuf::from(task_file);
        if !p.exists() {
            return Err(not_found(format!("Task file does not exist: {:?}", p)));
        }
        if p.extension().and_then(|e| e.to_str()) == Some("yaml")
            || p.extension().and_then(|e| e.to_str()) == Some("yml")
        {
            let item = load_recipe_task(&p).await.map_err(bad)?;
            item.task
        } else {
            let items = load_jsonl_dataset(&p).await.map_err(bad)?;
            items
                .into_iter()
                .next()
                .map(|i| i.task)
                .ok_or_else(|| bad("Dataset contains no tasks"))?
        }
    } else if let Some(prompt) = &req.prompt {
        TaskSpec::new("adhoc-task", "hub-runner", prompt)
    } else {
        return Err(bad("Must provide either taskFile or prompt"));
    };

    let job_id = new_harness_job_id("task");
    let started_at_dt = Utc::now();
    let started_at = started_at_dt.to_rfc3339();
    let task_id_for_fail = task_spec.id.clone();
    let notify_task_name = task_spec.problem_statement.clone();
    let notify_model = req.model.clone();

    register_active_job(HarnessActiveJobDto {
        job_id: job_id.clone(),
        task_id: task_spec.id.clone(),
        job_type: "task".to_string(),
        description: task_spec.problem_statement.clone(),
        started_at: started_at.clone(),
        current_status: "running".to_string(),
        provider: req.provider.clone(),
        model: req.model.clone(),
        project_slug: None,
    });

    let (tx, rx) = tokio::sync::oneshot::channel();
    let spawn_job_id = job_id.clone();
    let join_handle = tokio::spawn(async move {
        let run_res = execute_standalone_task(req, task_spec, spawn_job_id.clone()).await;
        let (task_status, duration_seconds, summary_result, error_message) = match &run_res {
            Ok(dto) => {
                upsert_live_snapshot(&spawn_job_id, dto.clone());
                let _ = persist_harness_run("task", dto, &started_at, None, None, None);
                let st = match dto.status {
                    openduck_harness::types::RunStatus::Success => {
                        crate::notification::TaskStatus::Succeeded
                    }
                    openduck_harness::types::RunStatus::Cancelled => {
                        crate::notification::TaskStatus::Cancelled
                    }
                    _ => crate::notification::TaskStatus::Failed,
                };
                (
                    st,
                    (dto.duration_ms / 1000) as u64,
                    dto.final_answer.clone(),
                    None,
                )
            }
            Err(e) => {
                let err_str = format!("{e:#}");
                let live_snapshot = get_live_snapshot(&spawn_job_id);
                let failed = failed_run_dto(&task_id_for_fail, &err_str, live_snapshot);
                upsert_live_snapshot(&spawn_job_id, failed.clone());
                let _ =
                    persist_harness_run("task", &failed, &started_at, None, None, Some(&err_str));
                (
                    crate::notification::TaskStatus::Failed,
                    (failed.duration_ms / 1000) as u64,
                    None,
                    Some(err_str),
                )
            }
        };

        let report = crate::notification::TaskExecutionReport {
            job_id: spawn_job_id.clone(),
            job_name: notify_task_name,
            session_id: task_id_for_fail,
            trigger_type: "standalone_task".to_string(),
            status: task_status,
            started_at: started_at_dt,
            finished_at: Utc::now(),
            duration_seconds,
            total_tokens_used: None,
            model_name: notify_model,
            project_id: None,
            summary_result,
            error_message,
            log_url: None,
        };

        tokio::spawn(async move {
            tracing::info!(
                job_id = %report.job_id,
                job_name = %report.job_name,
                session_id = %report.session_id,
                status = %report.status,
                trigger = %report.trigger_type,
                duration_seconds = report.duration_seconds,
                "Dispatching standalone task completion notification"
            );
            let notifier = crate::notification::NotificationService::load();
            if let Err(err) = notifier.handle_task_completion(report).await {
                tracing::error!(%err, "Failed to send standalone task completion notification");
            } else {
                tracing::info!("Standalone task completion notification processed successfully");
            }
        });

        remove_active_job(&spawn_job_id);
        let _ = tx.send(run_res);
    });
    register_abort_handle(&job_id, join_handle.abort_handle());

    match rx.await {
        Ok(Ok(dto)) => Ok(Json(dto)),
        Ok(Err(e)) => Err(bad(e)),
        Err(_) => Err(bad("Harness task worker ended unexpectedly")),
    }
}

async fn execute_standalone_task(
    req: HarnessRunRequest,
    task_spec: TaskSpec,
    job_id: String,
) -> Result<HarnessRunResponseDto> {
    let mut sandbox = LocalSandbox::ephemeral()?;
    sandbox.initialize().await?;

    let max_turns = req.max_turns.unwrap_or(25);
    let task_id = task_spec.id.clone();
    let global_settings =
        openduck_harness::types::discover_global_harness_settings(Some(sandbox.workspace_root()));
    let global_judge = global_settings.judge;

    let run_res: Result<HarnessRunResponseDto> = async {
        if req.echo.unwrap_or(false) {
            let base_policy = EchoPolicy::default();
            if let Some(record_path) = req.record_path {
                let mut cas_obj = Cassette::new(&task_spec.id);
                cas_obj.task_spec = Some(task_spec.clone());
                let cassette = Arc::new(Mutex::new(cas_obj));
                let policy = ReplayPolicy::new_record(base_policy, cassette.clone());
                let mut agent_harness =
                    AgentHarness::new(policy, sandbox).with_max_turns(max_turns);
                if let Some(ref judge) = global_judge {
                    agent_harness = agent_harness.with_judge_engine(
                        openduck_harness::judge::DecisionEngine::from_config(judge),
                    );
                }
                let mut harness = with_live_snapshot(agent_harness, &job_id, &task_id);
                let res = harness.run_task(&task_spec).await?;

                let cas = cassette.lock().await;
                cas.save_to_file(&PathBuf::from(&record_path)).await?;

                Ok(to_run_dto(res, Some(record_path)))
            } else {
                let mut agent_harness =
                    AgentHarness::new(base_policy, sandbox).with_max_turns(max_turns);
                if let Some(ref judge) = global_judge {
                    agent_harness = agent_harness.with_judge_engine(
                        openduck_harness::judge::DecisionEngine::from_config(judge),
                    );
                }
                let mut harness = with_live_snapshot(agent_harness, &job_id, &task_id);
                let res = harness.run_task(&task_spec).await?;

                Ok(to_run_dto(res, None))
            }
        } else {
            let mut base_policy = resolve_harness_policy_with_secondary(
                req.provider.as_deref(),
                req.model.as_deref(),
                req.secondary_provider.as_deref(),
                req.secondary_model.as_deref(),
            )
            .await?;
            let system_prompt = with_task_notes_plan(base_policy.system_prompt(), &task_id);
            base_policy = base_policy.with_system_prompt(system_prompt);

            let context_limit = base_policy.context_limit();

            if let Some(record_path) = req.record_path {
                let mut cas_obj = Cassette::new(&task_spec.id);
                cas_obj.task_spec = Some(task_spec.clone());
                let cassette = Arc::new(Mutex::new(cas_obj));
                let policy = ReplayPolicy::new_record(base_policy, cassette.clone());
                let mut agent_harness = AgentHarness::new(policy, sandbox)
                    .with_max_turns(max_turns)
                    .with_max_context_tokens(context_limit);
                if let Some(ref judge) = global_judge {
                    agent_harness = agent_harness.with_judge_engine(
                        openduck_harness::judge::DecisionEngine::from_config(judge),
                    );
                }
                let mut harness = with_live_snapshot(agent_harness, &job_id, &task_id);
                let res = harness.run_task(&task_spec).await?;

                let cas = cassette.lock().await;
                cas.save_to_file(&PathBuf::from(&record_path)).await?;

                Ok(to_run_dto(res, Some(record_path)))
            } else {
                let mut agent_harness = AgentHarness::new(base_policy, sandbox)
                    .with_max_turns(max_turns)
                    .with_max_context_tokens(context_limit);
                if let Some(ref judge) = global_judge {
                    agent_harness = agent_harness.with_judge_engine(
                        openduck_harness::judge::DecisionEngine::from_config(judge),
                    );
                }
                let mut harness = with_live_snapshot(agent_harness, &job_id, &task_id);
                let res = harness.run_task(&task_spec).await?;

                Ok(to_run_dto(res, None))
            }
        }
    }
    .await;

    run_res
}

pub async fn run_harness_replay(
    Json(req): Json<HarnessReplayRequest>,
) -> Result<Json<HarnessRunResponseDto>, (StatusCode, Json<serde_json::Value>)> {
    let cassette_path = PathBuf::from(&req.cassette_path);
    if !cassette_path.exists() {
        return Err(not_found(format!(
            "Cassette file not found: {:?}",
            cassette_path
        )));
    }
    let cassette = Cassette::load_from_file(&cassette_path)
        .await
        .map_err(bad)?;

    let task_spec = if let Some(task_file) = req.task_file {
        let p = PathBuf::from(task_file);
        if p.extension().and_then(|e| e.to_str()) == Some("yaml")
            || p.extension().and_then(|e| e.to_str()) == Some("yml")
        {
            let item = load_recipe_task(&p).await.map_err(bad)?;
            item.task
        } else {
            let items = load_jsonl_dataset(&p).await.map_err(bad)?;
            items
                .into_iter()
                .next()
                .map(|i| i.task)
                .ok_or_else(|| bad("Dataset contains no tasks"))?
        }
    } else if let Some(spec) = &cassette.task_spec {
        spec.clone()
    } else {
        TaskSpec::new(&cassette.name, "replay", "Offline deterministic replay")
    };

    let job_id = new_harness_job_id("replay");
    let started_at = Utc::now().to_rfc3339();
    let task_id_for_fail = task_spec.id.clone();
    register_active_job(HarnessActiveJobDto {
        job_id: job_id.clone(),
        task_id: task_spec.id.clone(),
        job_type: "replay".to_string(),
        description: format!("Replay cassette {}", req.cassette_path),
        started_at: started_at.clone(),
        current_status: "running".to_string(),
        provider: None,
        model: None,
        project_slug: None,
    });

    let (tx, rx) = tokio::sync::oneshot::channel();
    let spawn_job_id = job_id.clone();
    let join_handle = tokio::spawn(async move {
        let run_res = execute_replay_task(cassette, task_spec, spawn_job_id.clone()).await;
        match &run_res {
            Ok(dto) => {
                upsert_live_snapshot(&spawn_job_id, dto.clone());
                let _ = persist_harness_run("replay", dto, &started_at, None, None, None);
            }
            Err(e) => {
                let err_str = format!("{e:#}");
                let live_snapshot = get_live_snapshot(&spawn_job_id);
                let failed = failed_run_dto(&task_id_for_fail, &err_str, live_snapshot);
                upsert_live_snapshot(&spawn_job_id, failed.clone());
                let _ =
                    persist_harness_run("replay", &failed, &started_at, None, None, Some(&err_str));
            }
        }
        remove_active_job(&spawn_job_id);
        let _ = tx.send(run_res);
    });
    register_abort_handle(&job_id, join_handle.abort_handle());

    match rx.await {
        Ok(Ok(dto)) => Ok(Json(dto)),
        Ok(Err(e)) => Err(bad(e)),
        Err(_) => Err(bad("Harness replay worker ended unexpectedly")),
    }
}

async fn execute_replay_task(
    cassette: Cassette,
    task_spec: TaskSpec,
    job_id: String,
) -> Result<HarnessRunResponseDto> {
    let cassette_arc = Arc::new(Mutex::new(cassette));
    let replay_policy: ReplayPolicy<EchoPolicy> = ReplayPolicy::new_replay(cassette_arc);
    let mut sandbox = LocalSandbox::ephemeral()?;
    sandbox.initialize().await?;

    let mut harness = with_live_snapshot(
        AgentHarness::new(replay_policy, sandbox),
        &job_id,
        &task_spec.id,
    );
    let res = harness.run_task(&task_spec).await?;
    Ok(to_run_dto(res, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openduck_providers::base::MessageStream;
    use openduck_providers::errors::ProviderError;

    fn dummy_policy() -> GooseAgentPolicy {
        struct DummyProvider;
        #[async_trait]
        impl Provider for DummyProvider {
            fn get_name(&self) -> &str {
                "dummy"
            }
            async fn stream(
                &self,
                _model_config: &openduck_providers::model::ModelConfig,
                _system: &str,
                _messages: &[Message],
                _tools: &[rmcp::model::Tool],
            ) -> Result<openduck_providers::base::MessageStream, ProviderError> {
                Err(ProviderError::RequestFailed("unused".into()))
            }
        }
        GooseAgentPolicy::new(
            "dummy-policy",
            Arc::new(DummyProvider),
            openduck_providers::model::ModelConfig::new("dummy-model"),
        )
    }

    #[test]
    fn task_notes_plan_paths_are_distinct_per_task_id() {
        let path_a = task_notes_plan_path("task-aaa");
        let path_b = task_notes_plan_path("task-bbb");
        assert_ne!(path_a, path_b);
        assert!(path_a.contains("task-aaa"), "{path_a}");
        assert!(path_b.contains("task-bbb"), "{path_b}");
        assert_ne!(path_a, ".agent/notes-plan.md");
        assert_ne!(path_b, ".agent/notes-plan.md");
        assert_eq!(path_a, ".agent/notes-plan-task-aaa.md");
        assert_eq!(path_b, ".agent/notes-plan-task-bbb.md");
        assert_eq!(
            task_notes_plan_path("task-aaa"),
            task_notes_plan_path("task-aaa")
        );
    }

    #[test]
    fn task_notes_plan_path_sanitizes_filesystem_unsafe_ids() {
        let path = task_notes_plan_path("task/aaa\\bbb.md");
        assert_eq!(path, ".agent/notes-plan-task_aaa_bbb_md.md");
        let file_name = path.rsplit('/').next().unwrap();
        assert!(!file_name.contains('\\'), "{file_name}");
        assert_eq!(task_notes_plan_path("   "), ".agent/notes-plan-task.md");
    }

    #[test]
    fn default_goose_agent_policy_prompt_uses_per_task_notes_pattern() {
        let prompt = dummy_policy().system_prompt().to_string();
        assert!(
            prompt.contains(".agent/notes-plan-<task-id>.md"),
            "default prompt should name the per-task pattern, got: {prompt}"
        );
        assert!(
            !prompt.contains(".agent/notes-plan.md"),
            "default prompt must not cite the shared notes-plan file, got: {prompt}"
        );
        assert!(prompt.contains("Maintain Scratchpad Notes:"));
    }

    #[test]
    fn with_task_notes_plan_replaces_generic_instruction_with_concrete_path() {
        let default_prompt = dummy_policy().system_prompt().to_string();
        let assembled = with_task_notes_plan(&default_prompt, "task-aaa");
        let expected_path = task_notes_plan_path("task-aaa");
        assert!(
            assembled.contains(&expected_path),
            "assembled prompt missing {expected_path}: {assembled}"
        );
        assert!(
            !assembled.contains(".agent/notes-plan.md"),
            "assembled prompt still cites shared notes-plan: {assembled}"
        );
        assert!(!assembled.contains(".agent/notes-plan-<task-id>.md"));
        assert_eq!(assembled.matches("Maintain Scratchpad Notes:").count(), 1);
    }

    #[test]
    fn with_task_notes_plan_appends_when_custom_prompt_has_no_notes_sentence() {
        let custom = "You are a project coding agent. Follow AGENTS.md.";
        let assembled = with_task_notes_plan(custom, "task-bbb");
        let expected_path = task_notes_plan_path("task-bbb");
        assert!(assembled.contains(custom));
        assert!(assembled.contains(&expected_path));
        assert!(!assembled.contains(".agent/notes-plan.md"));
    }

    #[test]
    fn clean_stale_task_scratchpad_removes_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let agent_dir = temp.path().join(".agent");
        std::fs::create_dir_all(&agent_dir).unwrap();
        let notes_path = temp.path().join(task_notes_plan_path("task-xyz"));
        std::fs::write(&notes_path, "# Stale notes").unwrap();
        let cont_path = temp
            .path()
            .join(openduck_harness::eval::ContinuationCheckpoint::CONTINUATION_SUMMARY_PATH);
        std::fs::write(&cont_path, "# Stale continuation").unwrap();

        assert!(notes_path.exists());
        assert!(cont_path.exists());

        clean_stale_task_scratchpad(temp.path(), "task-xyz");

        assert!(!notes_path.exists());
        assert!(!cont_path.exists());
    }

    #[test]
    fn action_from_text_is_final_answer() {
        let reply = Message::assistant().with_text("done");
        let action = action_from_provider_reply(reply).unwrap();
        assert_eq!(action, AgentAction::FinalAnswer("done".into()));
    }

    #[test]
    fn action_from_empty_reply_is_error() {
        let reply = Message::assistant();
        let err = action_from_provider_reply(reply).unwrap_err();
        assert!(err.to_string().contains("empty completion"));
    }

    #[test]
    fn action_from_thinking_only_is_error() {
        let reply = Message::assistant().with_thinking("pondering", "sig");
        let err = action_from_provider_reply(reply).unwrap_err();
        assert!(err.to_string().contains("only thinking"));
    }

    #[test]
    fn action_from_empty_text_is_error() {
        let reply = Message::assistant().with_text("");
        let err = action_from_provider_reply(reply).unwrap_err();
        assert!(err.to_string().contains("empty completion"));
    }

    fn sample_run_dto(task_id: &str) -> HarnessRunResponseDto {
        HarnessRunResponseDto {
            task_id: task_id.to_string(),
            status: RunStatus::Success,
            step_count: 2,
            tool_calls_count: 1,
            duration_ms: 42,
            final_answer: Some("ok".into()),
            trajectory: TrajectoryRecord {
                session_id: "sess".into(),
                task_id: task_id.to_string(),
                policy_name: "echo".into(),
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                success: true,
                steps: vec![],
                system_prompt: None,
                active_rules: None,
            },
            recorded_cassette_path: None,
            system_prompt: None,
            active_rules: None,
            continuation: None,
            project_slug: None,
        }
    }

    #[test]
    fn persist_and_list_history_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let dto = sample_run_dto("demo-task");
        let started = Utc::now().to_rfc3339();
        write_persisted_run(
            tmp.path(),
            &PersistedHarnessRun {
                job_type: "task".into(),
                started_at: started.clone(),
                project_slug: Some("acme".into()),
                error: None,
                run_response: dto.clone(),
            },
        )
        .unwrap();

        let listed = internal_list_history(tmp.path()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].task_id, "demo-task");
        assert_eq!(listed[0].job_type, "task");
        assert_eq!(listed[0].project_slug.as_deref(), Some("acme"));
        assert_eq!(listed[0].step_count, 2);

        let loaded = read_history_run(&listed[0].file_name, &[tmp.path()]).unwrap();
        assert_eq!(loaded.task_id, "demo-task");
        assert_eq!(loaded.final_answer.as_deref(), Some("ok"));
    }

    fn sample_history_summary(task_id: &str, task_name: Option<&str>) -> HarnessHistorySummaryDto {
        HarnessHistorySummaryDto {
            id: format!("run-{task_id}"),
            file_name: format!("run-{task_id}.json"),
            task_id: task_id.to_string(),
            task_name: task_name.map(ToOwned::to_owned),
            job_type: "task".into(),
            status: RunStatus::Success,
            step_count: 1,
            tool_calls_count: 0,
            duration_ms: 1,
            final_answer: None,
            started_at: "2026-01-01T00:00:00Z".into(),
            recorded_cassette_path: None,
            project_slug: None,
            error: None,
            continuable: false,
            progress_summary: None,
        }
    }

    #[test]
    fn apply_history_task_names_fills_missing_titles() {
        let names = HashMap::from([
            ("task-1".to_string(), "Ship GPS alerts".to_string()),
            ("task-2".to_string(), "   ".to_string()),
        ]);
        let filled = apply_history_task_names(
            vec![
                sample_history_summary("task-1", None),
                sample_history_summary("task-2", None),
                sample_history_summary("task-3", None),
                sample_history_summary("task-1", Some("Original title")),
            ],
            &names,
        );
        assert_eq!(filled[0].task_name.as_deref(), Some("Ship GPS alerts"));
        assert_eq!(filled[1].task_name, None);
        assert_eq!(filled[2].task_name, None);
        assert_eq!(filled[3].task_name.as_deref(), Some("Original title"));
    }

    #[test]
    fn read_history_run_finds_project_runs_dir_from_project_root() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = PersistedHarnessRun {
            job_type: "task".into(),
            started_at: Utc::now().to_rfc3339(),
            project_slug: Some("mqtt-broker-develop".into()),
            error: None,
            run_response: sample_run_dto("task-297953"),
        };
        let runs_dir = project_runs_dir(tmp.path());
        let path = write_persisted_run_named(
            &runs_dir,
            "20260913_130755_task_task-297953_01a09ae1.json",
            &payload,
        )
        .unwrap();
        assert!(path.exists());

        let loaded = read_history_run("20260913_130755_task_task-297953_01a09ae1", &[tmp.path()])
            .expect("should find run under .goose/harness_runs");
        assert_eq!(loaded.task_id, "task-297953");
    }

    #[test]
    fn read_history_run_matches_timestamp_skewed_twin() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = PersistedHarnessRun {
            job_type: "task".into(),
            started_at: Utc::now().to_rfc3339(),
            project_slug: Some("mqtt-broker-develop".into()),
            error: None,
            run_response: sample_run_dto("task-297953"),
        };
        write_persisted_run_named(
            tmp.path(),
            "20260913_130753_task_task-297953_01a09ae1.json",
            &payload,
        )
        .unwrap();

        let loaded = read_history_run("20260913_130755_task_task-297953_01a09ae1", &[tmp.path()])
            .expect("should match the same task unique suffix despite timestamp skew");
        assert_eq!(loaded.task_id, "task-297953");
    }

    #[test]
    fn persist_harness_run_uses_the_same_filename_in_both_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = PersistedHarnessRun {
            job_type: "task".into(),
            started_at: Utc::now().to_rfc3339(),
            project_slug: Some("mqtt-broker-develop".into()),
            error: None,
            run_response: sample_run_dto("task-297953"),
        };
        let file_name = new_run_file_name(&payload);
        let global = tmp.path().join("global");
        let project = tmp.path().join("project");
        write_persisted_run_named(&global, &file_name, &payload).unwrap();
        write_persisted_run_named(&project_runs_dir(&project), &file_name, &payload).unwrap();

        let global_names: Vec<_> = std::fs::read_dir(&global)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        let project_names: Vec<_> = std::fs::read_dir(project_runs_dir(&project))
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(global_names, project_names);
        assert_eq!(global_names.len(), 1);
    }

    #[test]
    fn persist_appends_to_warm_history_dir_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let started = Utc::now().to_rfc3339();
        write_persisted_run(
            tmp.path(),
            &PersistedHarnessRun {
                job_type: "task".into(),
                started_at: started.clone(),
                project_slug: Some("acme".into()),
                error: None,
                run_response: sample_run_dto("first-task"),
            },
        )
        .unwrap();

        let first = internal_list_history(tmp.path()).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].task_id, "first-task");

        write_persisted_run(
            tmp.path(),
            &PersistedHarnessRun {
                job_type: "task".into(),
                started_at: started,
                project_slug: Some("acme".into()),
                error: None,
                run_response: sample_run_dto("second-task"),
            },
        )
        .unwrap();

        let listed = internal_list_history(tmp.path()).unwrap();
        assert_eq!(listed.len(), 2);
        let task_ids: Vec<_> = listed.iter().map(|h| h.task_id.as_str()).collect();
        assert!(task_ids.contains(&"first-task"));
        assert!(task_ids.contains(&"second-task"));
    }

    #[test]
    fn project_history_reads_project_dir_without_scanning_other_files() {
        let tmp = tempfile::tempdir().unwrap();
        let project_runs = tmp.path().join(".goose").join("harness_runs");
        std::fs::create_dir_all(&project_runs).unwrap();
        let started = Utc::now().to_rfc3339();
        write_persisted_run(
            &project_runs,
            &PersistedHarnessRun {
                job_type: "task".into(),
                started_at: started,
                project_slug: Some("acme".into()),
                error: None,
                run_response: sample_run_dto("local-only"),
            },
        )
        .unwrap();

        let listed = list_history_for_project("acme", Some(tmp.path()));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].task_id, "local-only");
    }

    #[test]
    fn history_filters_by_project_slug() {
        let tmp = tempfile::tempdir().unwrap();
        let started = Utc::now().to_rfc3339();
        write_persisted_run(
            tmp.path(),
            &PersistedHarnessRun {
                job_type: "task".into(),
                started_at: started.clone(),
                project_slug: Some("alpha".into()),
                error: None,
                run_response: sample_run_dto("alpha-task"),
            },
        )
        .unwrap();
        write_persisted_run(
            tmp.path(),
            &PersistedHarnessRun {
                job_type: "task".into(),
                started_at: started,
                project_slug: Some("beta".into()),
                error: None,
                run_response: sample_run_dto("beta-task"),
            },
        )
        .unwrap();

        let listed = internal_list_history(tmp.path()).unwrap();
        let alpha: Vec<_> = listed
            .into_iter()
            .filter(|h| h.project_slug.as_deref() == Some("alpha"))
            .collect();
        assert_eq!(alpha.len(), 1);
        assert_eq!(alpha[0].task_id, "alpha-task");
    }

    #[test]
    fn active_jobs_register_list_and_remove() {
        let job_id = new_harness_job_id("test");
        register_active_job(HarnessActiveJobDto {
            job_id: job_id.clone(),
            task_id: "live-task".into(),
            job_type: "task".into(),
            description: "running now".into(),
            started_at: Utc::now().to_rfc3339(),
            current_status: "running".into(),
            provider: None,
            model: None,
            project_slug: Some("demo".into()),
        });

        let all = get_active_jobs_list();
        assert!(all.iter().any(|j| j.job_id == job_id));
        let project = active_jobs_for_project("demo");
        assert!(project.iter().any(|j| j.job_id == job_id));
        assert!(active_jobs_for_project("other").is_empty());

        remove_active_job(&job_id);
        assert!(!get_active_jobs_list().iter().any(|j| j.job_id == job_id));
    }

    #[test]
    fn inspect_running_job_returns_live_snapshot() {
        let job_id = new_harness_job_id("inspect");
        register_active_job(HarnessActiveJobDto {
            job_id: job_id.clone(),
            task_id: "inspect-task".into(),
            job_type: "task".into(),
            description: "inspect me".into(),
            started_at: Utc::now().to_rfc3339(),
            current_status: "running".into(),
            provider: None,
            model: None,
            project_slug: Some("demo".into()),
        });

        let inspect = inspect_active_job(&job_id).expect("job should be inspectable");
        assert!(inspect.live);
        assert_eq!(inspect.job.task_id, "inspect-task");
        let snapshot = inspect.snapshot.expect("seeded snapshot");
        assert_eq!(snapshot.task_id, "inspect-task");
        assert_eq!(snapshot.status, RunStatus::Running);
        assert_eq!(snapshot.step_count, 0);

        upsert_live_snapshot(&job_id, sample_run_dto("inspect-task"));
        let inspect = inspect_active_job(&job_id).unwrap();
        assert_eq!(inspect.snapshot.unwrap().step_count, 2);

        remove_active_job(&job_id);
        assert!(inspect_active_job(&job_id).is_none());
        assert!(get_live_snapshot(&job_id).is_none());
    }

    #[test]
    fn failed_run_dto_records_error_as_answer() {
        let dto = failed_run_dto("broken", "boom", None);
        assert_eq!(dto.task_id, "broken");
        assert_eq!(dto.status, RunStatus::Failure);
        assert_eq!(dto.final_answer.as_deref(), Some("boom"));
        assert!(!dto.trajectory.success);
    }

    #[test]
    fn failed_run_dto_preserves_snapshot_steps() {
        let snapshot = sample_run_dto("failed-task");
        let failed = failed_run_dto("failed-task", "boom", Some(snapshot));
        assert_eq!(failed.task_id, "failed-task");
        assert_eq!(failed.status, RunStatus::Failure);
        assert_eq!(failed.step_count, 2);
        assert_eq!(failed.final_answer.as_deref(), Some("boom"));
        assert!(!failed.trajectory.success);
    }

    #[test]
    fn skipped_run_dto_is_not_a_failure() {
        let dto = skipped_run_dto("nightly", "previous run still in progress");
        assert_eq!(dto.task_id, "nightly");
        assert_eq!(dto.status, RunStatus::Skipped);
        assert_eq!(
            dto.final_answer.as_deref(),
            Some("previous run still in progress")
        );
        assert!(!dto.trajectory.success);
    }

    #[test]
    fn try_register_rejects_duplicate_project_task() {
        let task_id = format!("dup-task-{}", uuid::Uuid::now_v7());
        let first_id = new_harness_job_id("dup");
        let job = |job_id: String, task_id: &str| HarnessActiveJobDto {
            job_id,
            task_id: task_id.to_string(),
            job_type: "task".into(),
            description: "overlap".into(),
            started_at: Utc::now().to_rfc3339(),
            current_status: "running".into(),
            provider: None,
            model: None,
            project_slug: Some("demo".into()),
        };

        try_register_active_job(job(first_id.clone(), &task_id)).unwrap();
        let err = try_register_active_job(job(new_harness_job_id("dup"), &task_id)).unwrap_err();
        assert_eq!(err.task_id, task_id);
        assert_eq!(err.job_id, first_id);

        let other_id = new_harness_job_id("dup");
        try_register_active_job(job(other_id.clone(), "other-task")).unwrap();

        remove_active_job(&first_id);
        remove_active_job(&other_id);
        let retry_id = new_harness_job_id("dup");
        try_register_active_job(job(retry_id.clone(), &task_id)).unwrap();
        remove_active_job(&retry_id);
    }

    #[test]
    fn cancelled_run_dto_preserves_snapshot_steps() {
        let snapshot = sample_run_dto("cancel-task");
        let cancelled = cancelled_run_dto("cancel-task", "Stopped manually", Some(snapshot));
        assert_eq!(cancelled.task_id, "cancel-task");
        assert_eq!(cancelled.status, RunStatus::Cancelled);
        assert_eq!(cancelled.step_count, 2);
        assert_eq!(cancelled.final_answer.as_deref(), Some("Stopped manually"));
        assert!(!cancelled.trajectory.success);
        let continuation = cancelled
            .continuation
            .expect("cancelled run must keep a progress summary");
        assert_eq!(
            continuation.stop_reason,
            openduck_harness::ContinuationStopReason::Cancelled
        );
        assert!(continuation
            .compacted_summary
            .contains("Incomplete task run summary"));
    }

    #[tokio::test]
    async fn stop_active_job_clears_registry_and_aborts_task() {
        let job_id = new_harness_job_id("stop-test");
        register_active_job(HarnessActiveJobDto {
            job_id: job_id.clone(),
            task_id: "stoppable-task".into(),
            job_type: "task".into(),
            description: "will be stopped".into(),
            started_at: Utc::now().to_rfc3339(),
            current_status: "running".into(),
            provider: None,
            model: None,
            project_slug: None,
        });

        let handle = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });
        register_abort_handle(&job_id, handle.abort_handle());

        assert!(get_active_job(&job_id).is_some());
        let res = stop_active_job(&job_id, Some("User cancelled")).expect("should stop active job");
        assert_eq!(res.status, RunStatus::Cancelled);
        assert_eq!(res.task_id, "stoppable-task");
        assert_eq!(res.final_answer.as_deref(), Some("User cancelled"));

        assert!(get_active_job(&job_id).is_none());
        let join_err = handle.await.unwrap_err();
        assert!(join_err.is_cancelled());

        let err = stop_active_job(&job_id, None);
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_goose_agent_policy_secondary_failover() {
        use openduck_providers::base::MessageStream;
        use openduck_providers::conversation::message::Message;
        use openduck_providers::errors::ProviderError;
        use openduck_providers::model::ModelConfig;
        use rmcp::model::Tool;

        struct FailingPrimary;
        #[async_trait]
        impl Provider for FailingPrimary {
            fn get_name(&self) -> &str {
                "failing-primary"
            }
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _system: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                let s = futures::stream::iter(vec![Err(ProviderError::stream_decode_error(
                    "error decoding response body",
                ))]);
                Ok(Box::pin(s))
            }
        }

        struct WorkingSecondary;
        #[async_trait]
        impl Provider for WorkingSecondary {
            fn get_name(&self) -> &str {
                "working-secondary"
            }
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _system: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                let msg = Message::assistant().with_text("Secondary provider answer");
                let s = futures::stream::iter(vec![Ok((Some(msg), None))]);
                Ok(Box::pin(s))
            }
        }

        let primary_model = ModelConfig::new("primary-model");
        let secondary_model = ModelConfig::new("secondary-model");

        let mut policy =
            GooseAgentPolicy::new("test-policy", Arc::new(FailingPrimary), primary_model)
                .with_secondary_provider(Arc::new(WorkingSecondary), secondary_model);

        let mut context = HarnessContextView::new("test-session", std::path::PathBuf::from("."));
        context.messages = vec![openduck_harness::policy::HarnessMessage::user("Solve task")];

        let action = policy
            .step(&context, &[])
            .await
            .expect("policy step should succeed via secondary provider");
        match action {
            AgentAction::FinalAnswer(answer) => {
                assert_eq!(answer, "Secondary provider answer");
            }
            other => panic!("Expected FinalAnswer, got {:?}", other),
        }
    }

    #[test]
    fn action_from_malformed_tool_request_returns_error() {
        let mut reply = Message::assistant();
        reply.content.push(MessageContent::ToolRequest(
            crate::conversation::message::ToolRequest {
                id: "call_123".into(),
                tool_call: Err(rmcp::model::ErrorData::new(
                    rmcp::model::ErrorCode::INVALID_PARAMS,
                    "Invalid JSON parameters",
                    None,
                )),
                metadata: None,
                tool_meta: None,
            },
        ));
        let err = action_from_provider_reply(reply).unwrap_err();
        assert!(err.to_string().contains("arguments were malformed"));
    }

    #[tokio::test]
    async fn policy_step_retries_on_empty_completion_and_succeeds() {
        struct EmptyThenSuccessPrimary {
            calls: std::sync::atomic::AtomicUsize,
        }
        #[async_trait]
        impl Provider for EmptyThenSuccessPrimary {
            fn get_name(&self) -> &str {
                "empty-then-success"
            }
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _system: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                let call_count = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let msg = if call_count == 0 {
                    Message::assistant()
                } else {
                    Message::assistant().with_text("Success on retry")
                };
                let s = futures::stream::iter(vec![Ok((Some(msg), None))]);
                Ok(Box::pin(s))
            }
        }

        let primary_model = ModelConfig::new("primary-model");
        let mut policy = GooseAgentPolicy::new(
            "retry-policy",
            Arc::new(EmptyThenSuccessPrimary {
                calls: std::sync::atomic::AtomicUsize::new(0),
            }),
            primary_model,
        );

        let mut context = HarnessContextView::new("test-session", std::path::PathBuf::from("."));
        context.messages = vec![openduck_harness::policy::HarnessMessage::user("Hello")];

        let action = policy
            .step(&context, &[])
            .await
            .expect("step should succeed after retry");
        match action {
            AgentAction::FinalAnswer(ans) => assert_eq!(ans, "Success on retry"),
            other => panic!("Expected FinalAnswer, got {:?}", other),
        }
    }

    fn no_http_retry_config() -> openduck_providers::retry::RetryConfig {
        openduck_providers::retry::RetryConfig::new(0, 0, 1.0, 0)
    }

    #[tokio::test]
    async fn policy_step_retries_on_network_error_and_succeeds() {
        use openduck_providers::base::MessageStream;
        use openduck_providers::conversation::message::Message;
        use openduck_providers::errors::ProviderError;
        use openduck_providers::model::ModelConfig;
        use rmcp::model::Tool;

        struct NetworkThenSuccess {
            calls: std::sync::atomic::AtomicUsize,
        }
        #[async_trait]
        impl Provider for NetworkThenSuccess {
            fn get_name(&self) -> &str {
                "network-then-success"
            }
            fn retry_config(&self) -> openduck_providers::retry::RetryConfig {
                no_http_retry_config()
            }
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _system: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                let call_count = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if call_count == 0 {
                    return Err(ProviderError::NetworkError(
                        "Could not connect to opencode.ai — check your network connection and try again."
                            .into(),
                    ));
                }
                let msg = Message::assistant().with_text("Recovered after network retry");
                let s = futures::stream::iter(vec![Ok((Some(msg), None))]);
                Ok(Box::pin(s))
            }
        }

        let mut policy = GooseAgentPolicy::new(
            "network-retry-policy",
            Arc::new(NetworkThenSuccess {
                calls: std::sync::atomic::AtomicUsize::new(0),
            }),
            ModelConfig::new("primary-model"),
        );
        let mut context = HarnessContextView::new("test-session", std::path::PathBuf::from("."));
        context.messages = vec![openduck_harness::policy::HarnessMessage::user("Hello")];

        let action = policy
            .step(&context, &[])
            .await
            .expect("step should succeed after transient network retry");
        match action {
            AgentAction::FinalAnswer(ans) => {
                assert_eq!(ans, "Recovered after network retry")
            }
            other => panic!("Expected FinalAnswer, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn policy_step_retries_on_server_error_and_succeeds() {
        use openduck_providers::base::MessageStream;
        use openduck_providers::conversation::message::Message;
        use openduck_providers::errors::ProviderError;
        use openduck_providers::model::ModelConfig;
        use rmcp::model::Tool;

        struct ServerThenSuccess {
            calls: std::sync::atomic::AtomicUsize,
        }
        #[async_trait]
        impl Provider for ServerThenSuccess {
            fn get_name(&self) -> &str {
                "server-then-success"
            }
            fn retry_config(&self) -> openduck_providers::retry::RetryConfig {
                no_http_retry_config()
            }
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _system: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                let call_count = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if call_count == 0 {
                    return Err(ProviderError::ServerError(
                        "500 at https://opencode.ai".into(),
                    ));
                }
                let msg = Message::assistant().with_text("Recovered after server retry");
                let s = futures::stream::iter(vec![Ok((Some(msg), None))]);
                Ok(Box::pin(s))
            }
        }

        let mut policy = GooseAgentPolicy::new(
            "server-retry-policy",
            Arc::new(ServerThenSuccess {
                calls: std::sync::atomic::AtomicUsize::new(0),
            }),
            ModelConfig::new("primary-model"),
        );
        let mut context = HarnessContextView::new("test-session", std::path::PathBuf::from("."));
        context.messages = vec![openduck_harness::policy::HarnessMessage::user("Hello")];

        let action = policy
            .step(&context, &[])
            .await
            .expect("step should succeed after transient server retry");
        match action {
            AgentAction::FinalAnswer(ans) => {
                assert_eq!(ans, "Recovered after server retry")
            }
            other => panic!("Expected FinalAnswer, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn policy_step_does_not_retry_authentication() {
        use openduck_providers::base::MessageStream;
        use openduck_providers::conversation::message::Message;
        use openduck_providers::errors::ProviderError;
        use openduck_providers::model::ModelConfig;
        use rmcp::model::Tool;

        struct AuthFail {
            calls: std::sync::atomic::AtomicUsize,
        }
        #[async_trait]
        impl Provider for AuthFail {
            fn get_name(&self) -> &str {
                "auth-fail"
            }
            fn retry_config(&self) -> openduck_providers::retry::RetryConfig {
                no_http_retry_config()
            }
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _system: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(ProviderError::Authentication("401".into()))
            }
        }

        let provider = Arc::new(AuthFail {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let mut policy = GooseAgentPolicy::new(
            "auth-policy",
            provider.clone(),
            ModelConfig::new("primary-model"),
        );
        let mut context = HarnessContextView::new("test-session", std::path::PathBuf::from("."));
        context.messages = vec![openduck_harness::policy::HarnessMessage::user("Hello")];

        let err = policy
            .step(&context, &[])
            .await
            .expect_err("auth should fail");
        assert!(
            err.to_string().contains("Goose provider complete failed"),
            "{err:#}"
        );
        assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn policy_step_exhausts_transient_retries() {
        use openduck_providers::base::MessageStream;
        use openduck_providers::conversation::message::Message;
        use openduck_providers::errors::ProviderError;
        use openduck_providers::model::ModelConfig;
        use rmcp::model::Tool;

        struct AlwaysNetwork {
            calls: std::sync::atomic::AtomicUsize,
        }
        #[async_trait]
        impl Provider for AlwaysNetwork {
            fn get_name(&self) -> &str {
                "always-network"
            }
            fn retry_config(&self) -> openduck_providers::retry::RetryConfig {
                no_http_retry_config()
            }
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _system: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(ProviderError::NetworkError(
                    "Could not connect to opencode.ai".into(),
                ))
            }
        }

        let provider = Arc::new(AlwaysNetwork {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let mut policy = GooseAgentPolicy::new(
            "exhaust-policy",
            provider.clone(),
            ModelConfig::new("primary-model"),
        );
        let mut context = HarnessContextView::new("test-session", std::path::PathBuf::from("."));
        context.messages = vec![openduck_harness::policy::HarnessMessage::user("Hello")];

        let err = policy
            .step(&context, &[])
            .await
            .expect_err("exhausted retries should fail");
        assert!(
            err.to_string().contains("Goose provider complete failed"),
            "{err:#}"
        );
        assert_eq!(
            provider.calls.load(std::sync::atomic::Ordering::SeqCst),
            1 + MAX_TRANSIENT_POLICY_RETRIES as usize
        );
    }

    #[tokio::test]
    async fn policy_step_falls_back_to_secondary_on_empty_completion() {
        struct AlwaysEmptyPrimary;
        #[async_trait]
        impl Provider for AlwaysEmptyPrimary {
            fn get_name(&self) -> &str {
                "always-empty-primary"
            }
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _system: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                let msg = Message::assistant();
                let s = futures::stream::iter(vec![Ok((Some(msg), None))]);
                Ok(Box::pin(s))
            }
        }

        struct WorkingSecondary;
        #[async_trait]
        impl Provider for WorkingSecondary {
            fn get_name(&self) -> &str {
                "working-secondary"
            }
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _system: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                let msg = Message::assistant().with_text("Secondary provider answer");
                let s = futures::stream::iter(vec![Ok((Some(msg), None))]);
                Ok(Box::pin(s))
            }
        }

        let primary_model = ModelConfig::new("primary-model");
        let secondary_model = ModelConfig::new("secondary-model");

        let mut policy = GooseAgentPolicy::new(
            "test-empty-fallback-policy",
            Arc::new(AlwaysEmptyPrimary),
            primary_model,
        )
        .with_secondary_provider(Arc::new(WorkingSecondary), secondary_model);

        let mut context = HarnessContextView::new("test-session", std::path::PathBuf::from("."));
        context.messages = vec![openduck_harness::policy::HarnessMessage::user("Solve task")];

        let action = policy
            .step(&context, &[])
            .await
            .expect("policy step should succeed via secondary provider when primary returns empty completion");
        match action {
            AgentAction::FinalAnswer(answer) => {
                assert_eq!(answer, "Secondary provider answer");
            }
            other => panic!("Expected FinalAnswer, got {:?}", other),
        }
    }

    #[test]
    fn prune_messages_fallback_normalizes_message_sequence() {
        let messages = vec![
            Message::user().with_text("First task prompt"),
            Message::assistant().with_text("Step 1 done"),
            Message::user().with_text("Step 2"),
            Message::assistant().with_text("Step 2 done"),
            Message::user().with_text("Step 3"),
            Message::assistant().with_text("Step 3 done"),
            Message::user().with_text("Latest step"),
        ];

        let pruned = prune_messages_fallback(&messages);
        assert!(!pruned.is_empty());
        assert!(pruned
            .first()
            .unwrap()
            .as_concat_text()
            .contains("First task prompt"));
        assert!(pruned
            .iter()
            .any(|m| m.as_concat_text().contains("Earlier history pruned")));
    }

    #[tokio::test]
    async fn policy_propagates_session_id_to_request_headers_and_context() {
        use std::sync::Mutex as StdMutex;

        struct HeaderCapturingProvider {
            captured_headers: Arc<StdMutex<Option<HashMap<String, String>>>>,
            captured_session_context: Arc<StdMutex<Option<String>>>,
        }

        #[async_trait]
        impl Provider for HeaderCapturingProvider {
            fn get_name(&self) -> &str {
                "capturing-provider"
            }
            async fn stream(
                &self,
                model_config: &ModelConfig,
                _system: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                *self.captured_headers.lock().unwrap() = model_config.request_headers.clone();
                *self.captured_session_context.lock().unwrap() =
                    crate::session_context::current_session_id();
                let msg = Message::assistant().with_text("Task completed");
                let s = futures::stream::iter(vec![Ok((Some(msg), None))]);
                Ok(Box::pin(s))
            }
        }

        let captured_headers = Arc::new(StdMutex::new(None));
        let captured_session_context = Arc::new(StdMutex::new(None));

        let provider = Arc::new(HeaderCapturingProvider {
            captured_headers: captured_headers.clone(),
            captured_session_context: captured_session_context.clone(),
        });

        let mut policy =
            GooseAgentPolicy::new("test-policy", provider, ModelConfig::new("test-model"));
        let mut context = HarnessContextView::new("task-120405", std::path::PathBuf::from("."));
        context.messages = vec![openduck_harness::policy::HarnessMessage::user(
            "Perform step",
        )];

        let action = policy.step(&context, &[]).await.unwrap();
        assert!(matches!(action, AgentAction::FinalAnswer(_)));

        let headers = captured_headers
            .lock()
            .unwrap()
            .clone()
            .expect("headers should be set");
        assert_eq!(
            headers.get("x-opencode-session").map(|s| s.as_str()),
            Some("task-120405")
        );
        assert_eq!(
            headers.get("agent-session-id").map(|s| s.as_str()),
            Some("task-120405")
        );
        assert_eq!(
            captured_session_context.lock().unwrap().as_deref(),
            Some("task-120405")
        );
    }
}
