use crate::policy::AgentAction;
use crate::types::ToolCallResponse;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveRuleSummary {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub global: bool,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageSummary {
    #[serde(default, alias = "input_tokens", alias = "inputTokens")]
    pub input_tokens: Option<i32>,
    #[serde(default, alias = "output_tokens", alias = "outputTokens")]
    pub output_tokens: Option<i32>,
    #[serde(default, alias = "total_tokens", alias = "totalTokens")]
    pub total_tokens: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryStep {
    #[serde(
        rename = "stepIndex",
        alias = "step_number",
        alias = "step_index",
        alias = "stepNumber"
    )]
    pub step_number: usize,
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "agentAction", alias = "action", alias = "agent_action")]
    pub action: AgentAction,
    #[serde(default, alias = "tool_results")]
    pub tool_results: Option<Vec<ToolCallResponse>>,
    #[serde(alias = "duration_ms")]
    pub duration_ms: u128,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "token_usage"
    )]
    pub token_usage: Option<TokenUsageSummary>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "llm_request"
    )]
    pub llm_request: Option<serde_json::Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "llm_response"
    )]
    pub llm_response: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "judgments")]
    pub judgments: Option<Vec<crate::judge::JudgmentRecord>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRecord {
    #[serde(alias = "session_id")]
    pub session_id: String,
    #[serde(alias = "task_id")]
    pub task_id: String,
    #[serde(alias = "policy_name")]
    pub policy_name: String,
    #[serde(alias = "started_at")]
    pub started_at: DateTime<Utc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "completed_at"
    )]
    pub completed_at: Option<DateTime<Utc>>,
    pub success: bool,
    pub steps: Vec<TrajectoryStep>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "system_prompt"
    )]
    pub system_prompt: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "active_rules"
    )]
    pub active_rules: Option<Vec<ActiveRuleSummary>>,
}

pub struct TrajectoryLogger {
    output_path: PathBuf,
    lock: Mutex<()>,
}

impl TrajectoryLogger {
    pub fn new(output_path: PathBuf) -> Self {
        Self {
            output_path,
            lock: Mutex::new(()),
        }
    }

    pub async fn log_trajectory(&self, record: &TrajectoryRecord) -> Result<()> {
        let _guard = self.lock.lock().await;

        if let Some(parent) = self.output_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let json_line = serde_json::to_string(record)
            .context("Failed to serialize trajectory record to JSON")?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.output_path)
            .await
            .with_context(|| format!("Failed to open trajectory log at {:?}", self.output_path))?;

        file.write_all(json_line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;

        Ok(())
    }
}
