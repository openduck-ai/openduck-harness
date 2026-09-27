use crate::types::{ToolCallRequest, ToolCallResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessMessage {
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCallRequest>>,
    pub tool_results: Option<Vec<ToolCallResponse>>,
    pub created_at: DateTime<Utc>,
}

impl HarnessMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_calls: None,
            tool_results: None,
            created_at: Utc::now(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_calls: None,
            tool_results: None,
            created_at: Utc::now(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_results: None,
            created_at: Utc::now(),
        }
    }

    pub fn assistant_with_tools(
        content: impl Into<String>,
        tool_calls: Vec<ToolCallRequest>,
    ) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: Some(tool_calls),
            tool_results: None,
            created_at: Utc::now(),
        }
    }

    pub fn tool_response(results: Vec<ToolCallResponse>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: String::new(),
            tool_calls: None,
            tool_results: Some(results),
            created_at: Utc::now(),
        }
    }

    /// Fast, deterministic token estimation for this message including role framing,
    /// text content, tool call arguments, and tool execution outputs.
    pub fn estimate_tokens(&self) -> usize {
        let mut tokens = 4; // base role/message framing
        if !self.content.is_empty() {
            tokens += self.content.len().div_ceil(4);
        }
        if let Some(calls) = &self.tool_calls {
            for call in calls {
                tokens += 8;
                tokens += call.name.len().div_ceil(4);
                tokens += call.id.len().div_ceil(4);
                let args_str = call.arguments.to_string();
                tokens += args_str.len().div_ceil(4);
            }
        }
        if let Some(results) = &self.tool_results {
            for res in results {
                tokens += 6;
                tokens += res.name.len().div_ceil(4);
                tokens += res.id.len().div_ceil(4);
                tokens += res.output.len().div_ceil(4);
            }
        }
        tokens
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessContextView {
    pub session_id: String,
    pub workspace_root: PathBuf,
    pub messages: Vec<HarnessMessage>,
    pub metadata: HashMap<String, String>,
    pub step_count: usize,
}

impl HarnessContextView {
    pub fn new(session_id: impl Into<String>, workspace_root: PathBuf) -> Self {
        Self {
            session_id: session_id.into(),
            workspace_root,
            messages: Vec::new(),
            metadata: HashMap::new(),
            step_count: 0,
        }
    }

    /// Computes the aggregate estimated token count across all messages in the context view.
    pub fn total_estimated_tokens(&self) -> usize {
        self.messages.iter().map(|m| m.estimate_tokens()).sum()
    }
}
