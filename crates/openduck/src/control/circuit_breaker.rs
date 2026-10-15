use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use openduck_providers::base::Provider;
use openduck_providers::conversation::message::Message;
use openduck_providers::model::ModelConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    PrimaryHealthy,
    HedgedRacing,
    PrimaryDegraded,
    SecondaryActive,
    HalfOpenTesting,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::PrimaryHealthy => write!(f, "primary_healthy"),
            CircuitState::HedgedRacing => write!(f, "hedged_racing"),
            CircuitState::PrimaryDegraded => write!(f, "primary_degraded"),
            CircuitState::SecondaryActive => write!(f, "secondary_active"),
            CircuitState::HalfOpenTesting => write!(f, "half_open_testing"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: usize,
    pub recovery_probe_interval: Duration,
    pub required_consecutive_successes: usize,
    pub probe_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 2,
            recovery_probe_interval: Duration::from_secs(20),
            required_consecutive_successes: 2,
            probe_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Clone)]
pub struct ProviderCircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    config: CircuitBreakerConfig,
    consecutive_failures: Arc<AtomicUsize>,
    probe_running: Arc<AtomicBool>,
    probe_abort: Arc<tokio::sync::Notify>,
}

impl ProviderCircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::PrimaryHealthy)),
            config,
            consecutive_failures: Arc::new(AtomicUsize::new(0)),
            probe_running: Arc::new(AtomicBool::new(false)),
            probe_abort: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    pub async fn state(&self) -> CircuitState {
        *self.state.read().await
    }

    pub fn failure_count(&self) -> usize {
        self.consecutive_failures.load(Ordering::SeqCst)
    }

    pub async fn record_primary_success(&self) {
        let prev_failures = self.consecutive_failures.swap(0, Ordering::SeqCst);
        let mut st = self.state.write().await;
        let old_state = *st;
        if old_state != CircuitState::PrimaryHealthy {
            info!(
                prev_state = %old_state,
                prev_failures,
                "Primary provider succeeded; circuit breaker transitioned to PrimaryHealthy"
            );
            *st = CircuitState::PrimaryHealthy;
            self.stop_probe();
        }
    }

    pub async fn record_primary_failure(&self, err_msg: &str) -> CircuitState {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
        let mut st = self.state.write().await;
        let new_state = if failures >= self.config.failure_threshold {
            CircuitState::SecondaryActive
        } else {
            CircuitState::PrimaryDegraded
        };

        if *st != new_state {
            warn!(
                prev_state = %*st,
                new_state = %new_state,
                consecutive_failures = failures,
                threshold = self.config.failure_threshold,
                error = %err_msg,
                "Primary provider failed; circuit state updated"
            );
            *st = new_state;
        }

        new_state
    }

    pub async fn force_state(&self, state: CircuitState) {
        let mut st = self.state.write().await;
        *st = state;
        if state == CircuitState::PrimaryHealthy {
            self.consecutive_failures.store(0, Ordering::SeqCst);
            self.stop_probe();
        }
    }

    pub fn stop_probe(&self) {
        if self.probe_running.swap(false, Ordering::SeqCst) {
            self.probe_abort.notify_waiters();
        }
    }

    pub fn start_background_probe_if_needed(
        &self,
        major_provider: Arc<dyn Provider>,
        major_model_config: ModelConfig,
    ) {
        if self
            .probe_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let state = self.state.clone();
        let probe_running = self.probe_running.clone();
        let abort_notify = self.probe_abort.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            info!(
                provider = %major_provider.get_name(),
                model = %major_model_config.model_name,
                interval = ?config.recovery_probe_interval,
                "Starting background health probe for major provider"
            );

            let mut consecutive_successes = 0usize;
            let interval_duration = config.recovery_probe_interval;

            loop {
                tokio::select! {
                    _ = abort_notify.notified() => {
                        debug!("Background probe received abort signal; shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(interval_duration) => {}
                }

                if !probe_running.load(Ordering::SeqCst) {
                    break;
                }

                {
                    let current_state = *state.read().await;
                    if current_state != CircuitState::SecondaryActive {
                        debug!(
                            state = %current_state,
                            "Circuit is no longer SecondaryActive; stopping background probe"
                        );
                        break;
                    }
                }

                let ping_messages = vec![Message::user().with_text("ping")];
                let timeout_duration = config.probe_timeout;
                let probe_result = tokio::time::timeout(
                    timeout_duration,
                    major_provider.complete(
                        &major_model_config,
                        "respond with pong",
                        &ping_messages,
                        &[],
                    ),
                )
                .await;

                match probe_result {
                    Ok(Ok(_)) => {
                        consecutive_successes += 1;
                        info!(
                            consecutive_successes,
                            required = config.required_consecutive_successes,
                            provider = %major_provider.get_name(),
                            "Major provider health probe succeeded"
                        );

                        if consecutive_successes >= config.required_consecutive_successes {
                            let mut st = state.write().await;
                            if *st == CircuitState::SecondaryActive {
                                *st = CircuitState::HalfOpenTesting;
                                info!(
                                    provider = %major_provider.get_name(),
                                    "Major provider verified healthy; transitioned circuit to HalfOpenTesting"
                                );
                            }
                            break;
                        }
                    }
                    Ok(Err(err)) => {
                        consecutive_successes = 0;
                        debug!(
                            provider = %major_provider.get_name(),
                            error = %err,
                            "Major provider probe failed with error"
                        );
                    }
                    Err(_) => {
                        consecutive_successes = 0;
                        debug!(
                            provider = %major_provider.get_name(),
                            timeout = ?config.probe_timeout,
                            "Major provider probe timed out"
                        );
                    }
                }
            }

            probe_running.store(false, Ordering::SeqCst);
        });
    }
}
