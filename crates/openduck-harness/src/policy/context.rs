use crate::types::{ToolCallRequest, ToolCallResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

pub const HARNESS_CONTEXT_PREFIX: &str = "[HARNESS CONTEXT]\n";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Structured execution facts extracted from a history window during compaction.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
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
    pub findings: Vec<String>,
    pub observations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessMessage {
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCallRequest>>,
    pub tool_results: Option<Vec<ToolCallResponse>>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacted_facts: Option<Box<ExtractedContextFacts>>,
}

impl HarnessMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_calls: None,
            tool_results: None,
            created_at: Utc::now(),
            compacted_facts: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_calls: None,
            tool_results: None,
            created_at: Utc::now(),
            compacted_facts: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_results: None,
            created_at: Utc::now(),
            compacted_facts: None,
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
            compacted_facts: None,
        }
    }

    pub fn tool_response(results: Vec<ToolCallResponse>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: String::new(),
            tool_calls: None,
            tool_results: Some(results),
            created_at: Utc::now(),
            compacted_facts: None,
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
