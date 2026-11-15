pub mod observer;
pub mod policy;

use crate::base::{collect_stream, MessageStream, Provider, ProviderRoutingStatus};
use crate::conversation::message::Message;
use crate::conversation::token_usage::ProviderUsage;
use crate::errors::ProviderError;
use crate::model::ModelConfig;
use crate::retry::{retry_operation, RetryConfig};
use crate::thinking::ThinkingEffortSupport;
use async_trait::async_trait;
use futures::future::BoxFuture;
use observer::{
    classify_error_outcome, is_fallback_eligible, AttemptState, FallbackStreamFn,
    ObservationRecorder, ObservedStream,
};
use policy::{
    AdaptivePolicyConfig, AdaptivePolicyState, CandidateKind, Clock, RequestObservation,
    RequestPurpose, RoutingDecision, RoutingMode, SystemClock,
};
use rmcp::model::Tool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct CandidateTarget {
    pub provider: Arc<dyn Provider>,
    pub model_config: ModelConfig,
}

struct AdaptiveRouterRuntime {
    mode: RoutingMode,
    config_version: u64,
    policy: AdaptivePolicyState,
}

pub struct AdaptiveProvider {
    regular: CandidateTarget,
    economy: CandidateTarget,
    runtime: Arc<Mutex<AdaptiveRouterRuntime>>,
    rng_state: AtomicU64,
}

impl AdaptiveProvider {
    pub fn new(
        regular: CandidateTarget,
        economy: CandidateTarget,
        policy_config: AdaptivePolicyConfig,
        initial_mode: RoutingMode,
    ) -> anyhow::Result<Self> {
        Self::with_clock(
            regular,
            economy,
            policy_config,
            initial_mode,
            Arc::new(SystemClock),
        )
    }

    pub fn with_clock(
        regular: CandidateTarget,
        economy: CandidateTarget,
        policy_config: AdaptivePolicyConfig,
        initial_mode: RoutingMode,
        clock: Arc<dyn Clock>,
    ) -> anyhow::Result<Self> {
        policy_config.validate()?;
        let mut policy = AdaptivePolicyState::new(policy_config, clock);
        policy.reset_for_mode_change(initial_mode);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        Ok(Self {
            regular,
            economy,
            runtime: Arc::new(Mutex::new(AdaptiveRouterRuntime {
                mode: initial_mode,
                config_version: 1,
                policy,
            })),
            rng_state: AtomicU64::new(seed | 1),
        })
    }

    fn next_rng_unit(&self) -> f64 {
        let mut x = self.rng_state.load(Ordering::Relaxed);
        loop {
            let next = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            match self.rng_state.compare_exchange_weak(
                x,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let upper = (next >> 11) as f64;
                    return upper / ((1u64 << 53) as f64);
                }
                Err(actual) => x = actual,
            }
        }
    }

    pub fn set_mode(&self, mode: RoutingMode, config_version: u64) {
        let mut guard = self
            .runtime
            .lock()
            .expect("adaptive runtime mutex poisoned");
        guard.mode = mode;
        guard.config_version = config_version;
        guard.policy.reset_for_mode_change(mode);
    }

    pub fn mode(&self) -> RoutingMode {
        self.runtime
            .lock()
            .expect("adaptive runtime mutex poisoned")
            .mode
    }

    pub fn config_version(&self) -> u64 {
        self.runtime
            .lock()
            .expect("adaptive runtime mutex poisoned")
            .config_version
    }

    pub fn primary_candidate(&self) -> CandidateKind {
        self.runtime
            .lock()
            .expect("adaptive runtime mutex poisoned")
            .policy
            .primary()
    }

    pub fn in_flight_probes(&self) -> usize {
        self.runtime
            .lock()
            .expect("adaptive runtime mutex poisoned")
            .policy
            .in_flight_probes()
    }

    pub fn status(&self) -> ProviderRoutingStatus {
        let guard = self
            .runtime
            .lock()
            .expect("adaptive runtime mutex poisoned");
        let primary_model = match guard.policy.primary() {
            CandidateKind::Regular => self.regular.model_config.model_name.clone(),
            CandidateKind::Economy => self.economy.model_config.model_name.clone(),
        };
        let last_actual_model = guard
            .policy
            .last_actual_model()
            .map(str::to_string)
            .or_else(|| {
                guard.policy.last_actual().map(|c| match c {
                    CandidateKind::Regular => self.regular.model_config.model_name.clone(),
                    CandidateKind::Economy => self.economy.model_config.model_name.clone(),
                })
            });
        ProviderRoutingStatus {
            mode: guard.mode.as_str().to_string(),
            primary_model,
            last_actual_model,
            last_is_probe: guard.policy.last_is_probe(),
            last_switch_reason: guard.policy.last_switch_reason().to_string(),
            config_version: guard.config_version,
        }
    }

    fn candidate_target(&self, kind: CandidateKind) -> &CandidateTarget {
        match kind {
            CandidateKind::Regular => &self.regular,
            CandidateKind::Economy => &self.economy,
        }
    }

    fn merge_model_config(
        &self,
        candidate: &CandidateTarget,
        caller_config: &ModelConfig,
    ) -> ModelConfig {
        let mut merged = candidate.model_config.clone();
        if merged.temperature.is_none() {
            merged.temperature = caller_config.temperature;
        }
        if merged.max_tokens.is_none() {
            merged.max_tokens = caller_config.max_tokens;
        }
        if merged.request_headers.is_none() {
            merged.request_headers = caller_config.request_headers.clone();
        }
        if merged.request_params.is_none() {
            merged.request_params = caller_config.request_params.clone();
        }
        merged
    }

    fn decide(&self, purpose: RequestPurpose) -> RoutingDecision {
        let rng_unit = self.next_rng_unit();
        let mut guard = self
            .runtime
            .lock()
            .expect("adaptive runtime mutex poisoned");
        let mode = guard.mode;
        let version = guard.config_version;
        guard.policy.decide(mode, purpose, version, rng_unit)
    }

    fn observation_recorder(&self) -> ObservationRecorder {
        let runtime = Arc::clone(&self.runtime);
        Arc::new(move |obs: RequestObservation| {
            let mut guard = runtime.lock().expect("adaptive runtime mutex poisoned");
            let mode = guard.mode;
            let version = guard.config_version;
            guard.policy.record_observation(mode, version, obs);
        })
    }

    pub fn record_observation_for_test(&self, obs: RequestObservation) {
        let mut guard = self
            .runtime
            .lock()
            .expect("adaptive runtime mutex poisoned");
        let mode = guard.mode;
        let version = guard.config_version;
        guard.policy.record_observation(mode, version, obs);
    }

    pub async fn stream_with_purpose(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
        purpose: RequestPurpose,
    ) -> Result<MessageStream, ProviderError> {
        let decision = self.decide(purpose);
        let recorder = self.observation_recorder();

        let primary_target = self.candidate_target(decision.selected);
        let primary_model_cfg = self.merge_model_config(primary_target, model_config);

        tracing::debug!(
            target: "openduck::adaptive",
            mode = decision.mode.as_str(),
            primary = decision.primary.as_str(),
            selected = decision.selected.as_str(),
            selected_model = %primary_model_cfg.model_name,
            is_probe = decision.is_probe,
            config_version = decision.config_version,
            reason = %decision.reason,
            "adaptive routing decision"
        );

        let mut primary_attempt = AttemptState::new(
            decision.selected,
            primary_model_cfg.model_name.clone(),
            decision.is_probe,
            purpose,
            decision.config_version,
            1,
        );

        let fallback_target = self.candidate_target(decision.selected.other()).clone();
        let fallback_model_cfg = self.merge_model_config(&fallback_target, model_config);

        let primary_stream = match primary_target
            .provider
            .stream(&primary_model_cfg, system, messages, tools)
            .await
        {
            Ok(s) => s,
            Err(err) => {
                let outcome = classify_error_outcome(&err);
                if let Some(obs) = primary_attempt.into_observation(outcome) {
                    recorder(obs);
                }

                if !is_fallback_eligible(&err) {
                    return Err(err);
                }

                let mut fallback_attempt = AttemptState::new(
                    decision.selected.other(),
                    fallback_model_cfg.model_name.clone(),
                    false,
                    purpose,
                    decision.config_version,
                    2,
                );
                match fallback_target
                    .provider
                    .stream(&fallback_model_cfg, system, messages, tools)
                    .await
                {
                    Ok(fb_stream) => {
                        return Ok(Box::pin(ObservedStream::new(
                            fb_stream,
                            fallback_attempt,
                            None,
                            recorder,
                        )));
                    }
                    Err(fb_err) => {
                        let fb_outcome = classify_error_outcome(&fb_err);
                        if let Some(obs) = fallback_attempt.into_observation(fb_outcome) {
                            recorder(obs);
                        }
                        return Err(fb_err);
                    }
                }
            }
        };

        let system_owned = system.to_string();
        let messages_owned = messages.to_vec();
        let tools_owned = tools.to_vec();
        let fallback_fn: FallbackStreamFn = Box::new(move || {
            let model_name = fallback_model_cfg.model_name.clone();
            Box::pin(async move {
                let stream = fallback_target
                    .provider
                    .stream(
                        &fallback_model_cfg,
                        &system_owned,
                        &messages_owned,
                        &tools_owned,
                    )
                    .await?;
                Ok((stream, model_name))
            }) as BoxFuture<'static, Result<(MessageStream, String), ProviderError>>
        });

        Ok(Box::pin(ObservedStream::new(
            primary_stream,
            primary_attempt,
            Some(fallback_fn),
            recorder,
        )))
    }
}

#[async_trait]
impl Provider for AdaptiveProvider {
    fn get_name(&self) -> &str {
        "adaptive"
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        self.stream_with_purpose(
            model_config,
            system,
            messages,
            tools,
            RequestPurpose::MainLoop,
        )
        .await
    }

    async fn complete(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        retry_operation(&self.retry_config(), || async {
            let stream = self
                .stream_with_purpose(
                    model_config,
                    system,
                    messages,
                    tools,
                    RequestPurpose::Auxiliary,
                )
                .await?;
            collect_stream(stream).await
        })
        .await
    }

    async fn get_context_limit(&self, _model_config: &ModelConfig) -> Result<usize, ProviderError> {
        let reg_limit = self
            .regular
            .provider
            .get_context_limit(&self.regular.model_config)
            .await
            .unwrap_or_else(|_| self.regular.model_config.context_limit());
        let eco_limit = self
            .economy
            .provider
            .get_context_limit(&self.economy.model_config)
            .await
            .unwrap_or_else(|_| self.economy.model_config.context_limit());

        match self.mode() {
            RoutingMode::FixedRegular => Ok(reg_limit),
            RoutingMode::FixedEconomy => Ok(eco_limit),
            RoutingMode::Auto => Ok(reg_limit.min(eco_limit)),
        }
    }

    fn retry_config(&self) -> RetryConfig {
        let mut cfg = self.regular.provider.retry_config();
        cfg.max_retries = cfg.max_retries.min(1);
        cfg
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "auto".to_string(),
            self.regular.model_config.model_name.clone(),
            self.economy.model_config.model_name.clone(),
        ])
    }

    fn manages_own_context(&self) -> bool {
        self.regular.provider.manages_own_context() || self.economy.provider.manages_own_context()
    }

    fn thinking_effort_support(&self) -> ThinkingEffortSupport {
        match (
            self.regular.provider.thinking_effort_support(),
            self.economy.provider.thinking_effort_support(),
        ) {
            (ThinkingEffortSupport::Unspecified, other)
            | (other, ThinkingEffortSupport::Unspecified) => other,
            (a, b) if a == b => a,
            _ => ThinkingEffortSupport::Unsupported,
        }
    }

    async fn apply_model_selection(&self, model_config: &ModelConfig) -> Result<(), ProviderError> {
        let name = model_config.model_name.trim();
        let next_mode =
            if name.eq_ignore_ascii_case("auto") || name.eq_ignore_ascii_case("adaptive") {
                Some(RoutingMode::Auto)
            } else if name.eq_ignore_ascii_case("regular")
                || name.eq_ignore_ascii_case("fixed_regular")
                || name == self.regular.model_config.model_name
            {
                Some(RoutingMode::FixedRegular)
            } else if name.eq_ignore_ascii_case("economy")
                || name.eq_ignore_ascii_case("fixed_economy")
                || name == self.economy.model_config.model_name
            {
                Some(RoutingMode::FixedEconomy)
            } else {
                None
            };

        if let Some(mode) = next_mode {
            let mut guard = self
                .runtime
                .lock()
                .expect("adaptive runtime mutex poisoned");
            if guard.mode != mode {
                guard.mode = mode;
                guard.config_version = guard.config_version.saturating_add(1);
                guard.policy.reset_for_mode_change(mode);
            }
        }
        Ok(())
    }

    fn routing_status(&self) -> Option<ProviderRoutingStatus> {
        Some(self.status())
    }
}
