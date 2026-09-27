use crate::judge::laya::{LayaQuestion, QuestionType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DecisionPointId {
    #[serde(rename = "turn.drift")]
    TurnDrift,
    #[serde(rename = "turn.completion")]
    TurnCompletion,
    #[serde(rename = "context.forget")]
    ContextForget,
    #[serde(rename = "tool.risk")]
    ToolRisk,
    #[serde(untagged)]
    Custom(String),
}

impl DecisionPointId {
    pub fn as_str(&self) -> &str {
        match self {
            Self::TurnDrift => "turn.drift",
            Self::TurnCompletion => "turn.completion",
            Self::ContextForget => "context.forget",
            Self::ToolRisk => "tool.risk",
            Self::Custom(s) => s.as_str(),
        }
    }

    pub fn from_str_name(name: &str) -> Self {
        match name {
            "turn.drift" => Self::TurnDrift,
            "turn.completion" => Self::TurnCompletion,
            "context.forget" => Self::ContextForget,
            "tool.risk" => Self::ToolRisk,
            other => Self::Custom(other.to_string()),
        }
    }
}

impl fmt::Display for DecisionPointId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct DecisionPointSpec {
    pub id: DecisionPointId,
    pub question_type: QuestionType,
    pub description: String,
    pub default_instructions: String,
    pub default_criteria: Option<HashMap<String, String>>,
}

impl DecisionPointSpec {
    pub fn turn_drift() -> Self {
        Self {
            id: DecisionPointId::TurnDrift,
            question_type: QuestionType::Noul,
            description: "Detects repetitive, non-convergent, or stagnant loops in agent turns".to_string(),
            default_instructions: "Has the agent been in an unproductive or stagnant loop without making real progress towards the goal?".to_string(),
            default_criteria: None,
        }
    }

    pub fn turn_completion() -> Self {
        Self {
            id: DecisionPointId::TurnCompletion,
            question_type: QuestionType::Noul,
            description: "Challenges unverified completion claims before closing turn".to_string(),
            default_instructions: "Has the agent thoroughly verified its implementation with tests or commands before declaring completion?".to_string(),
            default_criteria: None,
        }
    }

    pub fn context_forget() -> Self {
        let mut criteria = HashMap::new();
        criteria.insert(
            "keep_active".to_string(),
            "Keep full tool output in active context window".to_string(),
        );
        criteria.insert(
            "collapse_to_tombstone".to_string(),
            "Replace stale or redundant output with a single-line tombstone".to_string(),
        );
        criteria.insert(
            "truncate".to_string(),
            "Truncate output to minimal head and tail lines".to_string(),
        );

        Self {
            id: DecisionPointId::ContextForget,
            question_type: QuestionType::Choice,
            description: "Triages historical tool outputs to prune stale tokens".to_string(),
            default_instructions:
                "How should this tool output chunk be handled to optimize context window headroom?"
                    .to_string(),
            default_criteria: Some(criteria),
        }
    }

    pub fn tool_risk() -> Self {
        Self {
            id: DecisionPointId::ToolRisk,
            question_type: QuestionType::Noul,
            description: "High-speed safety check for destructive actions".to_string(),
            default_instructions: "Is this action dangerous, destructive, or outside the intended scope of the user's task?".to_string(),
            default_criteria: None,
        }
    }

    pub fn to_laya_question(&self) -> LayaQuestion {
        match self.question_type {
            QuestionType::Noul => LayaQuestion::noul(&self.default_instructions),
            QuestionType::Choice => LayaQuestion::choice(
                &self.default_instructions,
                self.default_criteria.clone().unwrap_or_default(),
            ),
            QuestionType::Score => LayaQuestion::score(&self.default_instructions),
        }
    }
}
