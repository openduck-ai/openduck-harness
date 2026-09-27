pub mod adapters;
pub mod context;

use crate::telemetry::TokenUsageSummary;
use crate::types::{ToolCallRequest, ToolDefinition};
use anyhow::Result;
use async_trait::async_trait;
pub use context::{HarnessContextView, HarnessMessage, MessageRole};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum YieldReason {
    MaxTurnsReached,
    WaitingForExternalEvent,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentAction {
    CallTools(Vec<ToolCallRequest>),
    FinalAnswer(String),
    YieldControl { reason: YieldReason },
    RequestInput { prompt: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StepTelemetry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_request: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_response: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsageSummary>,
}

#[async_trait]
pub trait AgentPolicy: Send + Sync {
    fn name(&self) -> &str;

    async fn step(
        &mut self,
        context: &HarnessContextView,
        tools: &[ToolDefinition],
    ) -> Result<AgentAction>;

    async fn step_detailed(
        &mut self,
        context: &HarnessContextView,
        tools: &[ToolDefinition],
    ) -> Result<(AgentAction, Option<StepTelemetry>)> {
        let action = self.step(context, tools).await?;
        Ok((action, None))
    }
}
