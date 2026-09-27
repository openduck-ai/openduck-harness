use crate::policy::{AgentAction, AgentPolicy, HarnessContextView};
use crate::types::ToolDefinition;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::VecDeque;

pub struct MockPolicy {
    name: String,
    actions: VecDeque<AgentAction>,
}

impl MockPolicy {
    pub fn new(name: impl Into<String>, actions: Vec<AgentAction>) -> Self {
        Self {
            name: name.into(),
            actions: actions.into(),
        }
    }
}

#[async_trait]
impl AgentPolicy for MockPolicy {
    fn name(&self) -> &str {
        &self.name
    }

    async fn step(
        &mut self,
        _context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> Result<AgentAction> {
        if let Some(action) = self.actions.pop_front() {
            Ok(action)
        } else {
            Ok(AgentAction::FinalAnswer("No more scripted actions.".into()))
        }
    }
}

pub struct EchoPolicy {
    name: String,
}

impl EchoPolicy {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Default for EchoPolicy {
    fn default() -> Self {
        Self::new("echo-policy")
    }
}

#[async_trait]
impl AgentPolicy for EchoPolicy {
    fn name(&self) -> &str {
        &self.name
    }

    async fn step(
        &mut self,
        context: &HarnessContextView,
        _tools: &[ToolDefinition],
    ) -> Result<AgentAction> {
        let last_user_msg = context
            .messages
            .iter()
            .rev()
            .find(|m| m.role == crate::policy::MessageRole::User)
            .map(|m| m.content.as_str())
            .unwrap_or("Hello");

        Ok(AgentAction::FinalAnswer(format!(
            "Echo from {}: {}",
            self.name, last_user_msg
        )))
    }
}
