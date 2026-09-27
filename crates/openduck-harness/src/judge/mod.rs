pub mod laya;
pub mod ledger;
pub mod points;

pub use laya::{
    LayaAnswer, LayaClient, LayaQuestion, LayaRequest, LayaResponse, LayaUsage, QuestionType,
    DEFAULT_LAYA_ENDPOINT, DEFAULT_LAYA_TIMEOUT_MS,
};
pub use ledger::{DecisionLedger, DecisionMode, JudgmentRecord, Verdict};
pub use points::{DecisionPointId, DecisionPointSpec};

use chrono::Utc;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct DecisionEngine {
    client: Option<LayaClient>,
    ledger: DecisionLedger,
    modes: HashMap<String, DecisionMode>,
    drift_threshold: f64,
    completion_threshold: f64,
    risk_threshold: f64,
}

impl Default for DecisionEngine {
    fn default() -> Self {
        let mut modes = HashMap::new();
        modes.insert("turn.drift".to_string(), DecisionMode::Shadow);
        modes.insert("turn.completion".to_string(), DecisionMode::Shadow);
        modes.insert("context.forget".to_string(), DecisionMode::Off);
        modes.insert("tool.risk".to_string(), DecisionMode::Active);

        Self {
            client: Some(LayaClient::default()),
            ledger: DecisionLedger::new(),
            modes,
            drift_threshold: 0.80,
            completion_threshold: 0.35,
            risk_threshold: 0.70,
        }
    }
}

impl DecisionEngine {
    pub fn new(client: Option<LayaClient>, modes: HashMap<String, DecisionMode>) -> Self {
        Self {
            client,
            ledger: DecisionLedger::new(),
            modes,
            drift_threshold: 0.80,
            completion_threshold: 0.35,
            risk_threshold: 0.70,
        }
    }

    pub fn from_config(config: &crate::project::JudgeConfig) -> Self {
        let mut modes = HashMap::new();
        for (point, mode_str) in &config.points {
            modes.insert(point.clone(), DecisionMode::from_str_name(mode_str));
        }

        let client = if config.provider.to_lowercase() == "laya" {
            Some(LayaClient::new(
                &config.endpoint,
                std::time::Duration::from_millis(config.timeout_ms),
                config.fallback_on_error,
            ))
        } else {
            None
        };

        Self {
            client,
            ledger: DecisionLedger::new(),
            modes,
            drift_threshold: 0.80,
            completion_threshold: 0.35,
            risk_threshold: 0.70,
        }
    }

    pub fn mode_for(&self, point: &DecisionPointId) -> DecisionMode {
        self.modes
            .get(point.as_str())
            .copied()
            .unwrap_or(DecisionMode::Off)
    }

    pub fn set_mode(&mut self, point: &DecisionPointId, mode: DecisionMode) {
        self.modes.insert(point.as_str().to_string(), mode);
    }

    pub fn ledger(&self) -> &DecisionLedger {
        &self.ledger
    }

    pub async fn evaluate_drift(&self, state_text: &str) -> JudgmentRecord {
        let point_id = DecisionPointId::TurnDrift;
        let mode = self.mode_for(&point_id);
        let spec = DecisionPointSpec::turn_drift();

        if !mode.is_enabled() || self.client.is_none() {
            let record = JudgmentRecord {
                point: point_id.as_str().to_string(),
                mode,
                state_digest: summarize_state_digest(state_text),
                question_type: spec.question_type,
                criteria: spec.default_criteria,
                verdict: Verdict::Bypassed {
                    reason: if !mode.is_enabled() {
                        "Mode is off".to_string()
                    } else {
                        "No judge client configured".to_string()
                    },
                },
                confidence: None,
                probabilities: None,
                latency_ms: 0,
                timestamp: Utc::now(),
            };
            self.ledger.record(record.clone());
            return record;
        }

        let client = self.client.as_ref().unwrap();
        let question = spec.to_laya_question();
        let req = LayaRequest::single(state_text, "is_stagnant", question);

        let start = Instant::now();
        match client.predict(&req).await {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_millis();
                let answer = resp.answers.get("is_stagnant");
                let noul_val = answer.and_then(|a| a.noul).unwrap_or(0.0);
                let confidence = answer.and_then(|a| a.confidence);
                let triggered = noul_val >= self.drift_threshold;

                let record = JudgmentRecord {
                    point: point_id.as_str().to_string(),
                    mode,
                    state_digest: summarize_state_digest(state_text),
                    question_type: spec.question_type,
                    criteria: spec.default_criteria,
                    verdict: Verdict::Noul {
                        value: noul_val,
                        threshold: self.drift_threshold,
                        triggered,
                    },
                    confidence,
                    probabilities: answer.and_then(|a| a.probabilities.clone()),
                    latency_ms,
                    timestamp: Utc::now(),
                };
                self.ledger.record(record.clone());
                record
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_millis();
                let record = JudgmentRecord {
                    point: point_id.as_str().to_string(),
                    mode,
                    state_digest: summarize_state_digest(state_text),
                    question_type: spec.question_type,
                    criteria: spec.default_criteria,
                    verdict: Verdict::Error {
                        message: e.to_string(),
                    },
                    confidence: None,
                    probabilities: None,
                    latency_ms,
                    timestamp: Utc::now(),
                };
                self.ledger.record(record.clone());
                record
            }
        }
    }

    pub async fn evaluate_completion(&self, state_text: &str) -> JudgmentRecord {
        let point_id = DecisionPointId::TurnCompletion;
        let mode = self.mode_for(&point_id);
        let spec = DecisionPointSpec::turn_completion();

        if !mode.is_enabled() || self.client.is_none() {
            let record = JudgmentRecord {
                point: point_id.as_str().to_string(),
                mode,
                state_digest: summarize_state_digest(state_text),
                question_type: spec.question_type,
                criteria: spec.default_criteria,
                verdict: Verdict::Bypassed {
                    reason: if !mode.is_enabled() {
                        "Mode is off".to_string()
                    } else {
                        "No judge client configured".to_string()
                    },
                },
                confidence: None,
                probabilities: None,
                latency_ms: 0,
                timestamp: Utc::now(),
            };
            self.ledger.record(record.clone());
            return record;
        }

        let client = self.client.as_ref().unwrap();
        let question = spec.to_laya_question();
        let req = LayaRequest::single(state_text, "is_verified", question);

        let start = Instant::now();
        match client.predict(&req).await {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_millis();
                let answer = resp.answers.get("is_verified");
                let noul_val = answer.and_then(|a| a.noul).unwrap_or(0.0);
                let confidence = answer.and_then(|a| a.confidence);
                let triggered = noul_val < self.completion_threshold;

                let record = JudgmentRecord {
                    point: point_id.as_str().to_string(),
                    mode,
                    state_digest: summarize_state_digest(state_text),
                    question_type: spec.question_type,
                    criteria: spec.default_criteria,
                    verdict: Verdict::Noul {
                        value: noul_val,
                        threshold: self.completion_threshold,
                        triggered,
                    },
                    confidence,
                    probabilities: answer.and_then(|a| a.probabilities.clone()),
                    latency_ms,
                    timestamp: Utc::now(),
                };
                self.ledger.record(record.clone());
                record
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_millis();
                let record = JudgmentRecord {
                    point: point_id.as_str().to_string(),
                    mode,
                    state_digest: summarize_state_digest(state_text),
                    question_type: spec.question_type,
                    criteria: spec.default_criteria,
                    verdict: Verdict::Error {
                        message: e.to_string(),
                    },
                    confidence: None,
                    probabilities: None,
                    latency_ms,
                    timestamp: Utc::now(),
                };
                self.ledger.record(record.clone());
                record
            }
        }
    }

    pub async fn evaluate_tool_risk(&self, command: &str, task_goal: &str) -> JudgmentRecord {
        let point_id = DecisionPointId::ToolRisk;
        let mode = self.mode_for(&point_id);
        let spec = DecisionPointSpec::tool_risk();
        let state_text = format!("Proposed action: {command}\nTask goal: {task_goal}");

        if !mode.is_enabled() || self.client.is_none() {
            let record = JudgmentRecord {
                point: point_id.as_str().to_string(),
                mode,
                state_digest: summarize_state_digest(&state_text),
                question_type: spec.question_type,
                criteria: spec.default_criteria,
                verdict: Verdict::Bypassed {
                    reason: if !mode.is_enabled() {
                        "Mode is off".to_string()
                    } else {
                        "No judge client configured".to_string()
                    },
                },
                confidence: None,
                probabilities: None,
                latency_ms: 0,
                timestamp: Utc::now(),
            };
            self.ledger.record(record.clone());
            return record;
        }

        let client = self.client.as_ref().unwrap();
        let question = spec.to_laya_question();
        let req = LayaRequest::single(&state_text, "is_risky", question);

        let start = Instant::now();
        match client.predict(&req).await {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_millis();
                let answer = resp.answers.get("is_risky");
                let noul_val = answer.and_then(|a| a.noul).unwrap_or(0.0);
                let confidence = answer.and_then(|a| a.confidence);
                let triggered = noul_val >= self.risk_threshold;

                let record = JudgmentRecord {
                    point: point_id.as_str().to_string(),
                    mode,
                    state_digest: summarize_state_digest(&state_text),
                    question_type: spec.question_type,
                    criteria: spec.default_criteria,
                    verdict: Verdict::Noul {
                        value: noul_val,
                        threshold: self.risk_threshold,
                        triggered,
                    },
                    confidence,
                    probabilities: answer.and_then(|a| a.probabilities.clone()),
                    latency_ms,
                    timestamp: Utc::now(),
                };
                self.ledger.record(record.clone());
                record
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_millis();
                let record = JudgmentRecord {
                    point: point_id.as_str().to_string(),
                    mode,
                    state_digest: summarize_state_digest(&state_text),
                    question_type: spec.question_type,
                    criteria: spec.default_criteria,
                    verdict: Verdict::Error {
                        message: e.to_string(),
                    },
                    confidence: None,
                    probabilities: None,
                    latency_ms,
                    timestamp: Utc::now(),
                };
                self.ledger.record(record.clone());
                record
            }
        }
    }

    pub async fn evaluate_context_forget(&self, chunk_text: &str) -> JudgmentRecord {
        let point_id = DecisionPointId::ContextForget;
        let mode = self.mode_for(&point_id);
        let spec = DecisionPointSpec::context_forget();

        if !mode.is_enabled() || self.client.is_none() {
            let record = JudgmentRecord {
                point: point_id.as_str().to_string(),
                mode,
                state_digest: summarize_state_digest(chunk_text),
                question_type: spec.question_type,
                criteria: spec.default_criteria.clone(),
                verdict: Verdict::Bypassed {
                    reason: if !mode.is_enabled() {
                        "Mode is off".to_string()
                    } else {
                        "No judge client configured".to_string()
                    },
                },
                confidence: None,
                probabilities: None,
                latency_ms: 0,
                timestamp: Utc::now(),
            };
            self.ledger.record(record.clone());
            return record;
        }

        let client = self.client.as_ref().unwrap();
        let question = spec.to_laya_question();
        let req = LayaRequest::single(chunk_text, "action_route", question);

        let start = Instant::now();
        match client.predict(&req).await {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_millis();
                let answer = resp.answers.get("action_route");
                let selected = answer
                    .and_then(|a| a.choice.clone())
                    .unwrap_or_else(|| "keep_active".to_string());
                let confidence = answer.and_then(|a| a.confidence);

                let record = JudgmentRecord {
                    point: point_id.as_str().to_string(),
                    mode,
                    state_digest: summarize_state_digest(chunk_text),
                    question_type: spec.question_type,
                    criteria: spec.default_criteria,
                    verdict: Verdict::Choice { selected },
                    confidence,
                    probabilities: answer.and_then(|a| a.probabilities.clone()),
                    latency_ms,
                    timestamp: Utc::now(),
                };
                self.ledger.record(record.clone());
                record
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_millis();
                let record = JudgmentRecord {
                    point: point_id.as_str().to_string(),
                    mode,
                    state_digest: summarize_state_digest(chunk_text),
                    question_type: spec.question_type,
                    criteria: spec.default_criteria,
                    verdict: Verdict::Error {
                        message: e.to_string(),
                    },
                    confidence: None,
                    probabilities: None,
                    latency_ms,
                    timestamp: Utc::now(),
                };
                self.ledger.record(record.clone());
                record
            }
        }
    }
}

fn summarize_state_digest(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.len() <= 200 {
        trimmed.to_string()
    } else {
        let prefix: String = trimmed.chars().take(100).collect();
        let suffix: String = trimmed
            .chars()
            .rev()
            .take(80)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{prefix} ... [snipped] ... {suffix}")
    }
}
