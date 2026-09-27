use crate::policy::{AgentAction, AgentPolicy, HarnessContextView};
use crate::replay::cassette::{Cassette, RecordMode};
use crate::types::ToolDefinition;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ReplayPolicy<P: AgentPolicy> {
    inner: Option<P>,
    cassette: Arc<Mutex<Cassette>>,
    mode: RecordMode,
}

impl<P: AgentPolicy> ReplayPolicy<P> {
    pub fn new_record(inner: P, cassette: Arc<Mutex<Cassette>>) -> Self {
        Self {
            inner: Some(inner),
            cassette,
            mode: RecordMode::Record,
        }
    }

    pub fn new_replay(cassette: Arc<Mutex<Cassette>>) -> Self {
        Self {
            inner: None,
            cassette,
            mode: RecordMode::Replay,
        }
    }
}

#[derive(serde::Serialize)]
struct CanonicalMessageRef<'a> {
    role: &'a crate::policy::MessageRole,
    content: &'a str,
    tool_calls: &'a Option<Vec<crate::types::ToolCallRequest>>,
    tool_results: &'a Option<Vec<crate::types::ToolCallResponse>>,
}

#[derive(serde::Serialize)]
struct CanonicalContextRef<'a> {
    step_count: usize,
    messages: Vec<CanonicalMessageRef<'a>>,
    tools: &'a [ToolDefinition],
}

impl<P: AgentPolicy> ReplayPolicy<P> {
    fn canonical_key(context: &HarnessContextView, tools: &[ToolDefinition]) -> Result<String> {
        let canonical = CanonicalContextRef {
            step_count: context.step_count,
            messages: context
                .messages
                .iter()
                .map(|m| CanonicalMessageRef {
                    role: &m.role,
                    content: &m.content,
                    tool_calls: &m.tool_calls,
                    tool_results: &m.tool_results,
                })
                .collect(),
            tools,
        };
        Ok(serde_json::to_string(&canonical)?)
    }
}

#[async_trait]
impl<P: AgentPolicy> AgentPolicy for ReplayPolicy<P> {
    fn name(&self) -> &str {
        match self.mode {
            RecordMode::Record => "recording-policy",
            RecordMode::Replay => "replay-policy",
            RecordMode::Passthrough => "passthrough-policy",
        }
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
    ) -> Result<(AgentAction, Option<crate::policy::StepTelemetry>)> {
        match self.mode {
            RecordMode::Record => {
                let key = Self::canonical_key(context, tools)?;
                let inner = self
                    .inner
                    .as_mut()
                    .ok_or_else(|| anyhow!("Inner policy required for Record mode"))?;

                let (action, telemetry) = inner.step_detailed(context, tools).await?;
                let serialized_action = serde_json::to_string(&action)?;

                let mut cassette = self.cassette.lock().await;
                cassette.record(&key, &serialized_action, false);

                Ok((action, telemetry))
            }
            RecordMode::Replay => {
                let key = Self::canonical_key(context, tools)?;
                let cassette = self.cassette.lock().await;
                if let Some(frame) = cassette.replay(&key, false) {
                    let action: AgentAction = serde_json::from_str(&frame.response)?;
                    Ok((action, None))
                } else {
                    Err(anyhow!(
                        "Deterministic replay mismatch: No recorded frame for step context"
                    ))
                }
            }
            RecordMode::Passthrough => {
                let inner = self
                    .inner
                    .as_mut()
                    .ok_or_else(|| anyhow!("Inner policy required for Passthrough mode"))?;
                inner.step_detailed(context, tools).await
            }
        }
    }
}
