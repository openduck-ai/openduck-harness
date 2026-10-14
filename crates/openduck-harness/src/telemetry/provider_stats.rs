use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::telemetry::trajectory::{TokenUsageSummary, TrajectoryStep};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCallRecord {
    pub provider: String,
    pub model: String,
    pub latency_ms: u64,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsageSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_attempt: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_secondary: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_timing: Option<NetworkTiming>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkTiming {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp_connect_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_handshake_ms: Option<u64>,
    #[serde(default)]
    pub connection_ms: u64,
    #[serde(default)]
    pub connection_reused: bool,
    #[serde(default)]
    pub send_ms: u64,
    #[serde(default)]
    pub wait_ms: u64,
    #[serde(default)]
    pub receive_ms: u64,
    #[serde(default)]
    pub total_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCallStats {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub total_calls: usize,
    pub successful_calls: usize,
    pub failed_calls: usize,
    pub total_duration_ms: u64,
    pub avg_duration_ms: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,
    pub retries: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_connection_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_send_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_wait_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_receive_ms: Option<f64>,
}

impl ProviderCallStats {
    pub fn new(provider: impl Into<String>, model: Option<String>) -> Self {
        Self {
            provider: provider.into(),
            model,
            ..Default::default()
        }
    }

    pub fn record(&mut self, call: &ProviderCallRecord) {
        self.total_calls += 1;
        if call.success {
            self.successful_calls += 1;
        } else {
            self.failed_calls += 1;
            if let Some(err) = &call.error {
                if !self.errors.contains(err) {
                    self.errors.push(err.clone());
                }
            }
        }

        self.total_duration_ms += call.latency_ms;
        self.avg_duration_ms = (self.total_duration_ms as f64) / (self.total_calls as f64);

        if let Some(timing) = &call.network_timing {
            let n = self.total_calls as f64;
            let prev_n = (self.total_calls - 1) as f64;
            let prev_conn = self.avg_connection_ms.unwrap_or(0.0);
            self.avg_connection_ms = Some((prev_conn * prev_n + timing.connection_ms as f64) / n);
            let prev_send = self.avg_send_ms.unwrap_or(0.0);
            self.avg_send_ms = Some((prev_send * prev_n + timing.send_ms as f64) / n);
            let prev_wait = self.avg_wait_ms.unwrap_or(0.0);
            self.avg_wait_ms = Some((prev_wait * prev_n + timing.wait_ms as f64) / n);
            let prev_recv = self.avg_receive_ms.unwrap_or(0.0);
            self.avg_receive_ms = Some((prev_recv * prev_n + timing.receive_ms as f64) / n);
        }

        if let Some(usage) = &call.token_usage {
            if let Some(inp) = usage.input_tokens {
                self.input_tokens += inp as i64;
            }
            if let Some(out) = usage.output_tokens {
                self.output_tokens += out as i64;
            }
            if let Some(tot) = usage.total_tokens {
                self.total_tokens += tot as i64;
            } else {
                self.total_tokens +=
                    (usage.input_tokens.unwrap_or(0) + usage.output_tokens.unwrap_or(0)) as i64;
            }
        }

        if let Some(cost) = call.cost {
            let current = self.total_cost.unwrap_or(0.0);
            self.total_cost = Some(current + cost);
        }

        if call.retry_attempt.is_some() || call.retry_reason.is_some() {
            self.retries += 1;
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskProviderStats {
    pub total_calls: usize,
    pub successful_calls: usize,
    pub failed_calls: usize,
    pub total_retries: usize,
    pub total_duration_ms: u64,
    pub avg_duration_ms: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_connection_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_send_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_wait_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_receive_ms: Option<f64>,
    pub by_provider: HashMap<String, ProviderCallStats>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub by_model: HashMap<String, ProviderCallStats>,
}

impl TaskProviderStats {
    pub fn record_call(&mut self, call: &ProviderCallRecord) {
        self.total_calls += 1;
        if call.success {
            self.successful_calls += 1;
        } else {
            self.failed_calls += 1;
        }

        if call.retry_attempt.is_some() || call.retry_reason.is_some() {
            self.total_retries += 1;
        }

        self.total_duration_ms += call.latency_ms;
        self.avg_duration_ms = (self.total_duration_ms as f64) / (self.total_calls as f64);

        if let Some(timing) = &call.network_timing {
            let n = self.total_calls as f64;
            let prev_n = (self.total_calls - 1) as f64;
            let prev_conn = self.avg_connection_ms.unwrap_or(0.0);
            self.avg_connection_ms = Some((prev_conn * prev_n + timing.connection_ms as f64) / n);
            let prev_send = self.avg_send_ms.unwrap_or(0.0);
            self.avg_send_ms = Some((prev_send * prev_n + timing.send_ms as f64) / n);
            let prev_wait = self.avg_wait_ms.unwrap_or(0.0);
            self.avg_wait_ms = Some((prev_wait * prev_n + timing.wait_ms as f64) / n);
            let prev_recv = self.avg_receive_ms.unwrap_or(0.0);
            self.avg_receive_ms = Some((prev_recv * prev_n + timing.receive_ms as f64) / n);
        }

        if let Some(usage) = &call.token_usage {
            if let Some(inp) = usage.input_tokens {
                self.input_tokens += inp as i64;
            }
            if let Some(out) = usage.output_tokens {
                self.output_tokens += out as i64;
            }
            if let Some(tot) = usage.total_tokens {
                self.total_tokens += tot as i64;
            } else {
                self.total_tokens +=
                    (usage.input_tokens.unwrap_or(0) + usage.output_tokens.unwrap_or(0)) as i64;
            }
        }

        if let Some(cost) = call.cost {
            let current = self.total_cost.unwrap_or(0.0);
            self.total_cost = Some(current + cost);
        }

        let provider_entry = self
            .by_provider
            .entry(call.provider.clone())
            .or_insert_with(|| ProviderCallStats::new(&call.provider, None));
        provider_entry.record(call);

        let model_key = format!("{}:{}", call.provider, call.model);
        let model_entry = self
            .by_model
            .entry(model_key)
            .or_insert_with(|| ProviderCallStats::new(&call.provider, Some(call.model.clone())));
        model_entry.record(call);
    }

    pub fn from_calls(calls: &[ProviderCallRecord]) -> Self {
        let mut stats = Self::default();
        for call in calls {
            stats.record_call(call);
        }
        stats
    }

    /// Aggregates provider call statistics across trajectory steps.
    /// If steps have explicit `provider_calls`, they are accumulated.
    /// Otherwise, if a `policy_name` (e.g. `provider:model`) is known, synthesizes stats from step tokens/durations.
    pub fn from_steps(steps: &[TrajectoryStep], policy_name: Option<&str>) -> Self {
        let mut stats = Self::default();
        let mut found_explicit_calls = false;

        for step in steps {
            if let Some(calls) = &step.provider_calls {
                for call in calls {
                    stats.record_call(call);
                    found_explicit_calls = true;
                }
            }
        }

        if !found_explicit_calls && !steps.is_empty() {
            let (provider, model) = if let Some(p_name) = policy_name {
                if let Some((p, m)) = p_name.split_once(':') {
                    (p.to_string(), m.to_string())
                } else {
                    (p_name.to_string(), "default".to_string())
                }
            } else {
                ("unknown".to_string(), "unknown".to_string())
            };

            for step in steps {
                let call = ProviderCallRecord {
                    provider: provider.clone(),
                    model: model.clone(),
                    latency_ms: step.duration_ms as u64,
                    success: true,
                    error: None,
                    token_usage: step.token_usage.clone(),
                    cost: None,
                    retry_attempt: None,
                    retry_reason: None,
                    is_secondary: None,
                    network_timing: None,
                };
                stats.record_call(&call);
            }
        }

        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_provider_stats_aggregation() {
        let mut stats = TaskProviderStats::default();

        let call1 = ProviderCallRecord {
            provider: "anthropic".to_string(),
            model: "claude-3-5-sonnet".to_string(),
            latency_ms: 1200,
            success: true,
            error: None,
            token_usage: Some(TokenUsageSummary {
                input_tokens: Some(100),
                output_tokens: Some(50),
                total_tokens: Some(150),
            }),
            cost: Some(0.001),
            retry_attempt: None,
            retry_reason: None,
            is_secondary: None,
            network_timing: Some(NetworkTiming {
                dns_ms: Some(10),
                tcp_connect_ms: Some(20),
                tls_handshake_ms: Some(30),
                connection_ms: 60,
                connection_reused: false,
                send_ms: 15,
                wait_ms: 350,
                receive_ms: 775,
                total_ms: 1200,
            }),
        };

        let call2 = ProviderCallRecord {
            provider: "anthropic".to_string(),
            model: "claude-3-5-sonnet".to_string(),
            latency_ms: 800,
            success: false,
            error: Some("Rate limited".to_string()),
            token_usage: None,
            cost: None,
            retry_attempt: Some(1),
            retry_reason: Some("transient_error".to_string()),
            is_secondary: None,
            network_timing: Some(NetworkTiming {
                dns_ms: None,
                tcp_connect_ms: None,
                tls_handshake_ms: None,
                connection_ms: 0,
                connection_reused: true,
                send_ms: 10,
                wait_ms: 790,
                receive_ms: 0,
                total_ms: 800,
            }),
        };

        let call3 = ProviderCallRecord {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            latency_ms: 500,
            success: true,
            error: None,
            token_usage: Some(TokenUsageSummary {
                input_tokens: Some(200),
                output_tokens: Some(100),
                total_tokens: Some(300),
            }),
            cost: Some(0.002),
            retry_attempt: None,
            retry_reason: None,
            is_secondary: Some(true),
            network_timing: None,
        };

        stats.record_call(&call1);
        stats.record_call(&call2);
        stats.record_call(&call3);

        assert_eq!(stats.total_calls, 3);
        assert_eq!(stats.successful_calls, 2);
        assert_eq!(stats.failed_calls, 1);
        assert_eq!(stats.total_retries, 1);
        assert_eq!(stats.total_duration_ms, 2500);
        assert!((stats.avg_duration_ms - 833.333).abs() < 1.0);
        assert_eq!(stats.input_tokens, 300);
        assert_eq!(stats.output_tokens, 150);
        assert_eq!(stats.total_tokens, 450);
        assert_eq!(stats.total_cost, Some(0.003));

        // Network timing aggregation checks
        assert_eq!(stats.avg_connection_ms, Some(30.0));
        assert_eq!(stats.avg_send_ms, Some(12.5));
        assert_eq!(stats.avg_wait_ms, Some(570.0));
        assert_eq!(stats.avg_receive_ms, Some(387.5));

        let anthropic = stats.by_provider.get("anthropic").unwrap();
        assert_eq!(anthropic.total_calls, 2);
        assert_eq!(anthropic.successful_calls, 1);
        assert_eq!(anthropic.failed_calls, 1);
        assert_eq!(anthropic.retries, 1);
        assert_eq!(anthropic.errors, vec!["Rate limited"]);
        assert_eq!(anthropic.avg_connection_ms, Some(30.0));
        assert_eq!(anthropic.avg_wait_ms, Some(570.0));

        let openai = stats.by_provider.get("openai").unwrap();
        assert_eq!(openai.total_calls, 1);
        assert_eq!(openai.successful_calls, 1);
    }
}
