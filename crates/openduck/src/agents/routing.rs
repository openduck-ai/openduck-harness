use crate::providers::base::{Provider, ProviderRoutingStatus};
use anyhow::{anyhow, Result};
use openduck_providers::adaptive::policy::RoutingMode;
use openduck_providers::model::ModelConfig;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RoutingSnapshot {
    pub version: u64,
    pub provider: Arc<dyn Provider>,
    pub model_config: ModelConfig,
    pub mode: RoutingMode,
}

#[derive(Clone)]
pub struct AgentRoutingHandle {
    snapshot: Arc<RwLock<Option<RoutingSnapshot>>>,
    update_lock: Arc<Mutex<()>>,
    next_version: Arc<AtomicU64>,
}

impl Default for AgentRoutingHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRoutingHandle {
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(None)),
            update_lock: Arc::new(Mutex::new(())),
            next_version: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn from_static(provider: Arc<dyn Provider>, model_config: ModelConfig) -> Self {
        let handle = Self::new();
        let mode = mode_for_provider_and_model(provider.as_ref(), &model_config);
        handle.publish_unlocked(provider, model_config, mode);
        handle
    }

    pub fn snapshot(&self) -> Result<RoutingSnapshot> {
        self.snapshot
            .read()
            .expect("routing snapshot lock poisoned")
            .clone()
            .ok_or_else(|| anyhow!("Provider not set"))
    }

    pub fn current_version(&self) -> u64 {
        self.snapshot
            .read()
            .expect("routing snapshot lock poisoned")
            .as_ref()
            .map(|s| s.version)
            .unwrap_or(0)
    }

    pub fn ensure_initialized(&self, provider: Arc<dyn Provider>, model_config: ModelConfig) {
        let mut guard = self
            .snapshot
            .write()
            .expect("routing snapshot lock poisoned");
        if guard.is_none() {
            let version = self.next_version.fetch_add(1, Ordering::SeqCst) + 1;
            let mode = mode_for_provider_and_model(provider.as_ref(), &model_config);
            *guard = Some(RoutingSnapshot {
                version,
                provider,
                model_config,
                mode,
            });
        }
    }

    fn publish_unlocked(
        &self,
        provider: Arc<dyn Provider>,
        model_config: ModelConfig,
        mode: RoutingMode,
    ) -> u64 {
        let version = self.next_version.fetch_add(1, Ordering::SeqCst) + 1;
        let mut guard = self
            .snapshot
            .write()
            .expect("routing snapshot lock poisoned");
        *guard = Some(RoutingSnapshot {
            version,
            provider,
            model_config,
            mode,
        });
        version
    }

    pub async fn update_serialized<F, Fut>(&self, prepare_and_persist: F) -> Result<u64>
    where
        F: FnOnce(u64) -> Fut,
        Fut: Future<Output = Result<(Arc<dyn Provider>, ModelConfig, RoutingMode)>>,
    {
        let _guard = self.update_lock.lock().await;
        let proposed_version = self.next_version.load(Ordering::SeqCst) + 1;
        let (provider, model_config, mode) = prepare_and_persist(proposed_version).await?;
        let version = self.publish_unlocked(provider, model_config, mode);
        Ok(version)
    }

    pub fn status(&self) -> Option<ProviderRoutingStatus> {
        let snap = self.snapshot().ok()?;
        if let Some(mut status) = snap.provider.routing_status() {
            status.config_version = snap.version;
            return Some(status);
        }
        Some(ProviderRoutingStatus {
            mode: snap.mode.as_str().to_string(),
            primary_model: snap.model_config.model_name.clone(),
            last_actual_model: Some(snap.model_config.model_name),
            last_actual_candidate: None,
            last_is_probe: false,
            last_switch_reason: "fixed provider".to_string(),
            config_version: snap.version,
        })
    }
}

pub fn mode_for_provider_and_model(
    provider: &dyn Provider,
    model_config: &ModelConfig,
) -> RoutingMode {
    if let Some(status) = provider.routing_status() {
        return status.mode.parse().unwrap_or(RoutingMode::Auto);
    }
    let name = model_config.model_name.trim();
    if name.eq_ignore_ascii_case("auto") || name.eq_ignore_ascii_case("adaptive") {
        RoutingMode::Auto
    } else if name.eq_ignore_ascii_case("economy") || name.eq_ignore_ascii_case("fixed_economy") {
        RoutingMode::FixedEconomy
    } else {
        RoutingMode::FixedRegular
    }
}
