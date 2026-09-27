use crate::judge::laya::QuestionType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DecisionMode {
    #[default]
    Off,
    Shadow,
    Active,
}

impl DecisionMode {
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Shadow | Self::Active)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn from_str_name(s: &str) -> Self {
        match s.to_ascii_lowercase().trim() {
            "active" => Self::Active,
            "shadow" => Self::Shadow,
            _ => Self::Off,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Verdict {
    Noul {
        value: f64,
        threshold: f64,
        triggered: bool,
    },
    Choice {
        selected: String,
    },
    Score {
        value: f64,
    },
    Bypassed {
        reason: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgmentRecord {
    pub point: String,
    pub mode: DecisionMode,
    pub state_digest: String,
    pub question_type: QuestionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criteria: Option<HashMap<String, String>>,
    pub verdict: Verdict,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probabilities: Option<HashMap<String, f64>>,
    pub latency_ms: u128,
    pub timestamp: DateTime<Utc>,
}

impl JudgmentRecord {
    pub fn is_triggered(&self) -> bool {
        match &self.verdict {
            Verdict::Noul { triggered, .. } => *triggered,
            Verdict::Choice { selected } => selected != "keep_active" && selected != "retry",
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DecisionLedger {
    records: Arc<Mutex<Vec<JudgmentRecord>>>,
}

impl DecisionLedger {
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn record(&self, record: JudgmentRecord) {
        if let Ok(mut lock) = self.records.lock() {
            lock.push(record);
        }
    }

    pub fn all(&self) -> Vec<JudgmentRecord> {
        self.records.lock().map(|l| l.clone()).unwrap_or_default()
    }

    pub fn for_point(&self, point: &str) -> Vec<JudgmentRecord> {
        self.records
            .lock()
            .map(|l| l.iter().filter(|r| r.point == point).cloned().collect())
            .unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut lock) = self.records.lock() {
            lock.clear();
        }
    }

    pub fn len(&self) -> usize {
        self.records.lock().map(|l| l.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
