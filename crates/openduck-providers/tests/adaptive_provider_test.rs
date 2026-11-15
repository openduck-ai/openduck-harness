use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use futures::{stream, StreamExt};
use openduck_provider_types::base::{collect_stream, MessageStream, Provider};
use openduck_provider_types::conversation::message::{
    Message, MessageContentBlock, ThinkingContentBlock, ToolRequest,
};
use openduck_provider_types::conversation::token_usage::{ProviderUsage, Usage};
use openduck_provider_types::errors::ProviderError;
use openduck_provider_types::model::ModelConfig;
use openduck_providers::adaptive::policy::{
    AdaptivePolicyConfig, AdaptivePolicyState, CandidateKind, Clock, RequestObservation,
    RequestOutcome, RequestPurpose, RoutingMode, TimeWindow,
};
use openduck_providers::adaptive::{AdaptiveProvider, CandidateTarget};
use rmcp::model::{CallToolRequestParams, Tool};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug)]
struct MockClock {
    now: Mutex<DateTime<Utc>>,
}

impl MockClock {
    fn new(now: DateTime<Utc>) -> Self {
        Self {
            now: Mutex::new(now),
        }
    }

    fn advance(&self, delta: ChronoDuration) {
        let mut guard = self.now.lock().unwrap();
        *guard += delta;
    }

    fn set(&self, now: DateTime<Utc>) {
        *self.now.lock().unwrap() = now;
    }
}

impl Clock for MockClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock().unwrap()
    }
}

type StreamFactory = Box<dyn FnMut() -> Result<MessageStream, ProviderError> + Send>;

struct ScriptedProvider {
    name: String,
    context_limit: usize,
    calls: AtomicUsize,
    responses: Mutex<VecDeque<StreamFactory>>,
}

impl ScriptedProvider {
    fn new(name: &str, context_limit: usize) -> Self {
        Self {
            name: name.to_string(),
            context_limit,
            calls: AtomicUsize::new(0),
            responses: Mutex::new(VecDeque::new()),
        }
    }

    fn push_stream<F>(&self, f: F)
    where
        F: FnMut() -> Result<MessageStream, ProviderError> + Send + 'static,
    {
        self.responses.lock().unwrap().push_back(Box::new(f));
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let next = self.responses.lock().unwrap().pop_front();
        if let Some(mut factory) = next {
            factory()
        } else {
            let msg =
                Message::assistant().with_text(format!("ok from {}", model_config.model_name));
            let usage = ProviderUsage::new(model_config.model_name.clone(), Usage::default());
            Ok(Box::pin(stream::iter(vec![Ok((Some(msg), Some(usage)))])))
        }
    }

    async fn get_context_limit(&self, _model_config: &ModelConfig) -> Result<usize, ProviderError> {
        Ok(self.context_limit)
    }
}

fn shanghai_utc(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
        .unwrap()
        - ChronoDuration::hours(8)
}

fn ok_obs(
    candidate: CandidateKind,
    model: &str,
    ttft_ms: u64,
    total_ms: u64,
    input_tokens: usize,
    output_tokens: usize,
    config_version: u64,
) -> RequestObservation {
    RequestObservation {
        candidate,
        actual_model: model.to_string(),
        is_probe: false,
        purpose: RequestPurpose::MainLoop,
        config_version,
        input_tokens,
        output_tokens,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        ttft: Some(Duration::from_millis(ttft_ms)),
        total_duration: Duration::from_millis(total_ms),
        attempts: 1,
        outcome: RequestOutcome::Success,
    }
}

#[test]
fn time_window_midnight_wrap_and_shanghai_tz() {
    let cfg = AdaptivePolicyConfig {
        off_peak: vec![TimeWindow::parse("23:00-06:00").unwrap()],
        ..Default::default()
    };

    // 23:30 Asia/Shanghai -> off-peak
    assert!(cfg.is_off_peak(shanghai_utc(2026, 11, 14, 23, 30)));
    // 05:59 Asia/Shanghai -> off-peak
    assert!(cfg.is_off_peak(shanghai_utc(2026, 11, 15, 5, 59)));
    // 06:00 Asia/Shanghai -> peak
    assert!(!cfg.is_off_peak(shanghai_utc(2026, 11, 15, 6, 0)));
    // 14:00 Asia/Shanghai -> peak
    assert!(!cfg.is_off_peak(shanghai_utc(2026, 11, 15, 14, 0)));
}

#[test]
fn boundary_once_does_not_override_health_decision_every_request() {
    let clock = Arc::new(MockClock::new(shanghai_utc(2026, 11, 14, 2, 0))); // 02:00 off-peak
    let mut state = AdaptivePolicyState::new(AdaptivePolicyConfig::default(), clock.clone());
    assert_eq!(state.primary(), CandidateKind::Economy);

    // Economy suffers severe timeout -> immediately switches to Regular & pauses economy
    state.record_observation(
        RoutingMode::Auto,
        1,
        RequestObservation {
            candidate: CandidateKind::Economy,
            actual_model: "ark/deepseek-v4.1-flash-economy".to_string(),
            is_probe: false,
            purpose: RequestPurpose::MainLoop,
            config_version: 1,
            input_tokens: 1000,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            ttft: None,
            total_duration: Duration::from_secs(35),
            attempts: 1,
            outcome: RequestOutcome::Timeout,
        },
    );
    assert_eq!(state.primary(), CandidateKind::Regular);

    // Next request still in off-peak (02:01) must NOT flip back to Economy
    let decision = state.decide(RoutingMode::Auto, RequestPurpose::MainLoop, 1, 0.99);
    assert_eq!(decision.selected, CandidateKind::Regular);
    assert_eq!(decision.primary, CandidateKind::Regular);
}

#[test]
fn leaving_off_peak_keeps_economy_if_healthy_and_returns_if_insufficient_evidence() {
    // Case 1: Insufficient evidence -> returns to Regular when leaving off-peak
    let clock = Arc::new(MockClock::new(shanghai_utc(2026, 11, 14, 6, 55)));
    let mut state = AdaptivePolicyState::new(AdaptivePolicyConfig::default(), clock.clone());
    assert_eq!(state.primary(), CandidateKind::Economy);

    clock.set(shanghai_utc(2026, 11, 14, 7, 5));
    let decision = state.decide(RoutingMode::Auto, RequestPurpose::MainLoop, 1, 0.99);
    assert_eq!(decision.primary, CandidateKind::Regular);

    // Case 2: Fresh comparable evidence within budget -> stays on Economy after leaving off-peak
    let clock2 = Arc::new(MockClock::new(shanghai_utc(2026, 11, 14, 6, 55)));
    let mut state2 = AdaptivePolicyState::new(AdaptivePolicyConfig::default(), clock2.clone());
    for _ in 0..3 {
        state2.record_observation(
            RoutingMode::Auto,
            1,
            ok_obs(CandidateKind::Regular, "reg", 1000, 3000, 1000, 100, 1),
        );
        state2.record_observation(
            RoutingMode::Auto,
            1,
            ok_obs(CandidateKind::Economy, "eco", 1100, 3200, 1000, 100, 1),
        );
    }
    clock2.set(shanghai_utc(2026, 11, 14, 7, 5));
    let decision2 = state2.decide(RoutingMode::Auto, RequestPurpose::MainLoop, 1, 0.99);
    assert_eq!(decision2.primary, CandidateKind::Economy);
}

#[test]
fn hysteresis_confirmations_cooldown_and_sample_expiration() {
    let clock = Arc::new(MockClock::new(shanghai_utc(2026, 11, 14, 14, 0))); // peak -> Regular
    let cfg = AdaptivePolicyConfig {
        min_samples_per_side: 3,
        confirm_count: 2,
        cooldown: Duration::from_secs(300),
        ..Default::default()
    };

    let mut state = AdaptivePolicyState::new(cfg, clock.clone());
    assert_eq!(state.primary(), CandidateKind::Regular);

    // Seed 3 regular samples and 2 economy samples (< min_samples_per_side) -> no switch
    for _ in 0..3 {
        state.record_observation(
            RoutingMode::Auto,
            1,
            ok_obs(CandidateKind::Regular, "reg", 1000, 3000, 500, 50, 1),
        );
    }
    for _ in 0..2 {
        state.record_observation(
            RoutingMode::Auto,
            1,
            ok_obs(CandidateKind::Economy, "eco", 1100, 3100, 500, 50, 1),
        );
    }
    assert_eq!(state.primary(), CandidateKind::Regular);

    // 3rd economy sample -> 1st confirmation (confirm_count = 2), still Regular
    state.record_observation(
        RoutingMode::Auto,
        1,
        ok_obs(CandidateKind::Economy, "eco", 1100, 3100, 500, 50, 1),
    );
    assert_eq!(state.primary(), CandidateKind::Regular);

    // 4th economy sample -> 2nd confirmation -> switches to Economy and enters cooldown
    state.record_observation(
        RoutingMode::Auto,
        1,
        ok_obs(CandidateKind::Economy, "eco", 1100, 3100, 500, 50, 1),
    );
    assert_eq!(state.primary(), CandidateKind::Economy);

    // During cooldown, slow economy samples do NOT trigger switch back to Regular
    for _ in 0..5 {
        state.record_observation(
            RoutingMode::Auto,
            1,
            ok_obs(CandidateKind::Economy, "eco", 5000, 10000, 500, 50, 1),
        );
    }
    assert_eq!(state.primary(), CandidateKind::Economy);

    // Advance past cooldown (301s) and feed 2 slow economy samples -> switches to Regular
    clock.advance(ChronoDuration::seconds(301));
    state.record_observation(
        RoutingMode::Auto,
        1,
        ok_obs(CandidateKind::Economy, "eco", 6000, 12000, 500, 50, 1),
    );
    assert_eq!(state.primary(), CandidateKind::Economy);
    state.record_observation(
        RoutingMode::Auto,
        1,
        ok_obs(CandidateKind::Economy, "eco", 6000, 12000, 500, 50, 1),
    );
    assert_eq!(state.primary(), CandidateKind::Regular);

    // Advance 20 minutes -> samples expire
    clock.advance(ChronoDuration::minutes(20));
    state.record_observation(
        RoutingMode::Auto,
        1,
        ok_obs(CandidateKind::Economy, "eco", 1000, 3000, 500, 50, 1),
    );
    assert_eq!(state.primary(), CandidateKind::Regular);
}

#[test]
fn probe_concurrency_limit_and_cancellation_and_version_isolation() {
    let clock = Arc::new(MockClock::new(shanghai_utc(2026, 11, 14, 14, 0)));
    let mut state = AdaptivePolicyState::new(AdaptivePolicyConfig::default(), clock);

    // First probe allowed (rng_unit = 0.01 < 0.10)
    let d1 = state.decide(RoutingMode::Auto, RequestPurpose::MainLoop, 1, 0.01);
    assert!(d1.is_probe);
    assert_eq!(d1.selected, CandidateKind::Economy);
    assert_eq!(state.in_flight_probes(), 1);

    // Second concurrent probe blocked (max_in_flight_probes = 1)
    let d2 = state.decide(RoutingMode::Auto, RequestPurpose::MainLoop, 1, 0.01);
    assert!(!d2.is_probe);
    assert_eq!(d2.selected, CandidateKind::Regular);

    // Stale config version observation releases probe slot, but does not alter new version
    state.record_observation(
        RoutingMode::Auto,
        2,
        RequestObservation {
            candidate: CandidateKind::Economy,
            actual_model: "eco".to_string(),
            is_probe: true,
            purpose: RequestPurpose::MainLoop,
            config_version: 1,
            input_tokens: 100,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            ttft: None,
            total_duration: Duration::from_secs(40),
            attempts: 1,
            outcome: RequestOutcome::Timeout,
        },
    );
    assert_eq!(state.in_flight_probes(), 0);
    assert!(state.economy_paused_until().is_none());

    // User cancellation does not count as failure
    state.record_observation(
        RoutingMode::Auto,
        2,
        RequestObservation {
            candidate: CandidateKind::Economy,
            actual_model: "eco".to_string(),
            is_probe: false,
            purpose: RequestPurpose::MainLoop,
            config_version: 2,
            input_tokens: 100,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            ttft: None,
            total_duration: Duration::from_secs(1),
            attempts: 1,
            outcome: RequestOutcome::Cancelled,
        },
    );
    assert!(state.economy_paused_until().is_none());
}

#[tokio::test]
async fn adaptive_provider_fallback_before_valid_output_and_no_fallback_after_valid_output() {
    let reg = Arc::new(ScriptedProvider::new("ark", 128_000));
    let eco = Arc::new(ScriptedProvider::new("ark", 64_000));

    // 1. Fails before valid output (empty message then ServerError) -> falls back to economy once
    reg.push_stream(|| {
        let empty = Message::assistant();
        Ok(Box::pin(stream::iter(vec![
            Ok((Some(empty), None)),
            Err(ProviderError::ServerError("503 unavailable".to_string())),
        ])))
    });
    eco.push_stream(|| {
        let msg = Message::assistant().with_text("recovered from economy");
        let usage = ProviderUsage::new(
            "ark/deepseek-v4.1-flash-economy".to_string(),
            Usage::default(),
        );
        Ok(Box::pin(stream::iter(vec![Ok((Some(msg), Some(usage)))])))
    });

    let clock = Arc::new(MockClock::new(shanghai_utc(2026, 11, 14, 14, 0)));
    let policy = AdaptivePolicyConfig {
        probe_ratio: 0.0,
        ..Default::default()
    };
    let adaptive = AdaptiveProvider::with_clock(
        CandidateTarget {
            provider: reg.clone(),
            model_config: ModelConfig::new("deepseek-v4.1-flash"),
        },
        CandidateTarget {
            provider: eco.clone(),
            model_config: ModelConfig::new("deepseek-v4.1-flash-economy"),
        },
        policy.clone(),
        RoutingMode::Auto,
        clock.clone(),
    )
    .unwrap();

    // Context limit in Auto mode is min(128_000, 64_000)
    assert_eq!(
        adaptive
            .get_context_limit(&ModelConfig::new("auto"))
            .await
            .unwrap(),
        64_000
    );

    let s = adaptive
        .stream(&ModelConfig::new("auto"), "", &[], &[])
        .await
        .unwrap();
    let (msg, usage) = collect_stream(s).await.unwrap();
    assert_eq!(msg.as_concat_text(), "recovered from economy");
    assert_eq!(usage.model, "ark/deepseek-v4.1-flash-economy");
    assert_eq!(reg.call_count(), 1);
    assert_eq!(eco.call_count(), 1);

    // 2. Fails AFTER valid output -> must NOT splice response from economy
    reg.push_stream(|| {
        let partial = Message::assistant().with_text("partial ");
        Ok(Box::pin(stream::iter(vec![
            Ok((Some(partial), None)),
            Err(ProviderError::NetworkError("reset".to_string())),
        ])))
    });

    let s2 = adaptive
        .stream(&ModelConfig::new("auto"), "", &[], &[])
        .await
        .unwrap();
    let err = collect_stream(s2).await.unwrap_err();
    assert!(matches!(err, ProviderError::NetworkError(_)));
    assert_eq!(eco.call_count(), 1);
}

#[tokio::test]
async fn adaptive_provider_does_not_switch_on_auth_error_and_releases_probe_on_drop() {
    let reg = Arc::new(ScriptedProvider::new("ark", 128_000));
    let eco = Arc::new(ScriptedProvider::new("ark", 64_000));

    reg.push_stream(|| Err(ProviderError::Authentication("bad key".to_string())));

    let clock = Arc::new(MockClock::new(shanghai_utc(2026, 11, 14, 14, 0)));
    let policy = AdaptivePolicyConfig {
        probe_ratio: 0.0,
        ..Default::default()
    };
    let adaptive = AdaptiveProvider::with_clock(
        CandidateTarget {
            provider: reg.clone(),
            model_config: ModelConfig::new("deepseek-v4.1-flash"),
        },
        CandidateTarget {
            provider: eco.clone(),
            model_config: ModelConfig::new("deepseek-v4.1-flash-economy"),
        },
        policy,
        RoutingMode::Auto,
        clock.clone(),
    )
    .unwrap();

    let err = adaptive
        .stream(&ModelConfig::new("auto"), "", &[], &[])
        .await
        .err()
        .unwrap();
    assert!(matches!(err, ProviderError::Authentication(_)));
    assert_eq!(eco.call_count(), 0);

    // Probe stream dropped early -> probe slot must be released
    let probe_policy = AdaptivePolicyConfig {
        probe_ratio: 1.0,
        probe_jitter: 0.0,
        ..Default::default()
    };
    let adaptive_probe = AdaptiveProvider::with_clock(
        CandidateTarget {
            provider: reg.clone(),
            model_config: ModelConfig::new("deepseek-v4.1-flash"),
        },
        CandidateTarget {
            provider: eco.clone(),
            model_config: ModelConfig::new("deepseek-v4.1-flash-economy"),
        },
        probe_policy,
        RoutingMode::Auto,
        clock,
    )
    .unwrap();

    eco.push_stream(|| {
        let thinking = Message::assistant().with_content(MessageContentBlock::Thinking(
            ThinkingContentBlock {
                thinking: "hmm".to_string(),
                signature: String::new(),
            },
        ));
        let tool =
            Message::assistant().with_content(MessageContentBlock::ToolRequest(ToolRequest {
                id: "call_1".to_string(),
                tool_call: Ok(CallToolRequestParams::new("read_file")),
                metadata: None,
                tool_meta: None,
            }));
        Ok(Box::pin(stream::iter(vec![
            Ok((Some(thinking), None)),
            Ok((Some(tool), None)),
        ])))
    });

    let mut stream = adaptive_probe
        .stream(&ModelConfig::new("auto"), "", &[], &[])
        .await
        .unwrap();
    assert_eq!(adaptive_probe.in_flight_probes(), 1);
    let _first = stream.next().await;
    drop(stream);
    assert_eq!(adaptive_probe.in_flight_probes(), 0);
}

#[tokio::test]
async fn auxiliary_complete_uses_primary_without_probing() {
    let reg = Arc::new(ScriptedProvider::new("ark", 128_000));
    let eco = Arc::new(ScriptedProvider::new("ark", 64_000));

    let clock = Arc::new(MockClock::new(shanghai_utc(2026, 11, 14, 14, 0)));
    let policy = AdaptivePolicyConfig {
        probe_ratio: 1.0, // would always probe if MainLoop
        probe_jitter: 0.0,
        ..Default::default()
    };

    let adaptive = AdaptiveProvider::with_clock(
        CandidateTarget {
            provider: reg.clone(),
            model_config: ModelConfig::new("deepseek-v4.1-flash"),
        },
        CandidateTarget {
            provider: eco.clone(),
            model_config: ModelConfig::new("deepseek-v4.1-flash-economy"),
        },
        policy,
        RoutingMode::Auto,
        clock,
    )
    .unwrap();

    let (msg, _) = adaptive
        .complete(&ModelConfig::new("auto"), "", &[], &[])
        .await
        .unwrap();
    assert_eq!(msg.as_concat_text(), "ok from deepseek-v4.1-flash");
    assert_eq!(reg.call_count(), 1);
    assert_eq!(eco.call_count(), 0);
}
