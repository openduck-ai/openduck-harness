use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub const DEFAULT_LAYA_ENDPOINT: &str = "http://localhost:8732/api/laya";
pub const DEFAULT_LAYA_TIMEOUT_MS: u64 = 350;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType {
    Noul,
    Choice,
    Score,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayaQuestion {
    #[serde(rename = "type")]
    pub question_type: QuestionType,
    pub instructions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criteria: Option<HashMap<String, String>>,
}

impl LayaQuestion {
    pub fn noul(instructions: impl Into<String>) -> Self {
        Self {
            question_type: QuestionType::Noul,
            instructions: instructions.into(),
            criteria: None,
        }
    }

    pub fn choice(instructions: impl Into<String>, criteria: HashMap<String, String>) -> Self {
        Self {
            question_type: QuestionType::Choice,
            instructions: instructions.into(),
            criteria: Some(criteria),
        }
    }

    pub fn score(instructions: impl Into<String>) -> Self {
        Self {
            question_type: QuestionType::Score,
            instructions: instructions.into(),
            criteria: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayaRequest {
    pub state: String,
    pub questions: HashMap<String, LayaQuestion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub laya_checkpoint: Option<String>,
}

impl LayaRequest {
    pub fn single(
        state: impl Into<String>,
        key: impl Into<String>,
        question: LayaQuestion,
    ) -> Self {
        let mut questions = HashMap::new();
        questions.insert(key.into(), question);
        Self {
            state: state.into(),
            questions,
            laya_checkpoint: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayaAnswer {
    #[serde(rename = "type")]
    pub answer_type: Option<String>,
    pub noul: Option<f64>,
    pub choice: Option<String>,
    pub score: Option<f64>,
    pub confidence: Option<f64>,
    pub probabilities: Option<HashMap<String, f64>>,
    pub action: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayaUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayaResponse {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub answers: HashMap<String, LayaAnswer>,
    #[serde(default)]
    pub usage: Option<LayaUsage>,
    #[serde(default)]
    pub routing: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct LayaClient {
    client: reqwest::Client,
    endpoint: String,
    timeout: Duration,
    fallback_on_error: bool,
}

impl Default for LayaClient {
    fn default() -> Self {
        Self::new(
            DEFAULT_LAYA_ENDPOINT,
            Duration::from_millis(DEFAULT_LAYA_TIMEOUT_MS),
            true,
        )
    }
}

impl LayaClient {
    pub fn new(endpoint: impl Into<String>, timeout: Duration, fallback_on_error: bool) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .pool_max_idle_per_host(8)
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            endpoint: endpoint.into(),
            timeout,
            fallback_on_error,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn fallback_on_error(&self) -> bool {
        self.fallback_on_error
    }

    pub async fn predict(&self, request: &LayaRequest) -> Result<LayaResponse> {
        let response = self
            .client
            .post(&self.endpoint)
            .json(request)
            .send()
            .await
            .context("Failed to send request to Laya sidecar")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            anyhow::bail!("Laya sidecar returned HTTP {}: {}", status, error_text);
        }

        let resp: LayaResponse = response
            .json()
            .await
            .context("Failed to parse Laya sidecar response")?;

        Ok(resp)
    }

    pub async fn is_available(&self) -> bool {
        let test_req =
            LayaRequest::single("ping", "ping", LayaQuestion::noul("Is the service online?"));

        self.predict(&test_req).await.is_ok()
    }
}
