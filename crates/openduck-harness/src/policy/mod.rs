pub mod adapters;
pub mod context;

use std::sync::Arc;

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartialGeneration {
    pub thinking: String,
    pub text: String,
}

pub type PartialThinkingObserver = Arc<dyn Fn(PartialGeneration) + Send + Sync>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StepTelemetry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_request: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_response: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsageSummary>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "provider_calls",
        alias = "providerCalls"
    )]
    pub provider_calls: Option<Vec<crate::telemetry::ProviderCallRecord>>,
}

#[async_trait]
pub trait AgentPolicy: Send + Sync {
    fn name(&self) -> &str;

    fn set_partial_observer(&mut self, _observer: Option<PartialThinkingObserver>) {}

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
