use super::api_client::TlsConfig;
use super::base::{ConfigKey, Provider, ProviderDef, ProviderMetadata};
use crate::config::{Config, ExtensionConfig};
use anyhow::Result;
use futures::future::BoxFuture;
use openduck_providers::adaptive::policy::{AdaptivePolicyConfig, RoutingMode, TimeWindow};
use openduck_providers::adaptive::{AdaptiveProvider, CandidateTarget};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

pub const ADAPTIVE_PROVIDER_NAME: &str = "adaptive";
pub const ADAPTIVE_DEFAULT_MODEL: &str = "auto";
pub const ADAPTIVE_KNOWN_MODELS: &[&str] = &["auto", "regular", "economy"];

pub struct AdaptiveProviderDef;

impl openduck_providers::base::ProviderDescriptor for AdaptiveProviderDef {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            ADAPTIVE_PROVIDER_NAME,
            "Adaptive Router",
            "Automatically routes between a regular model and an economy model based on time windows and observed latency",
            ADAPTIVE_DEFAULT_MODEL,
            ADAPTIVE_KNOWN_MODELS.to_vec(),
            "",
            vec![
                ConfigKey::new("ADAPTIVE_REGULAR_MODEL", true, false, None, true),
                ConfigKey::new("ADAPTIVE_ECONOMY_MODEL", true, false, None, true),
                ConfigKey::new("ADAPTIVE_TIMEZONE", false, false, Some("Asia/Shanghai"), false),
                ConfigKey::new("ADAPTIVE_OFF_PEAK", false, false, Some("01:00-07:00"), false),
                ConfigKey::new("ADAPTIVE_PROBE_RATIO", false, false, Some("0.10"), false),
                ConfigKey::new("ADAPTIVE_COOLDOWN_SECONDS", false, false, Some("300"), false),
                ConfigKey::new("ADAPTIVE_MODE", false, false, Some("auto"), false),
                ConfigKey::new("ADAPTIVE_REGULAR_PROVIDER", false, false, None, false),
                ConfigKey::new("ADAPTIVE_ECONOMY_PROVIDER", false, false, None, false),
                ConfigKey::new("ADAPTIVE_BASE_PROVIDER", false, false, None, false),
            ],
        )
    }
}

pub async fn resolve_candidate_spec(
    spec: &str,
    explicit_provider: Option<&str>,
    base_provider: Option<&str>,
) -> Result<(String, String)> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Adaptive candidate model spec cannot be empty");
    }

    if let Some(provider) = explicit_provider.map(str::trim).filter(|s| !s.is_empty()) {
        if provider == ADAPTIVE_PROVIDER_NAME {
            anyhow::bail!("Adaptive provider cannot wrap itself");
        }
        return Ok((provider.to_string(), trimmed.to_string()));
    }

    if let Some((prefix, model)) = trimmed.split_once('/') {
        let prefix = prefix.trim();
        let model = model.trim();
        if prefix == ADAPTIVE_PROVIDER_NAME {
            anyhow::bail!("Adaptive provider cannot wrap itself");
        }
        if !prefix.is_empty() && !model.is_empty() && super::get_from_registry(prefix).await.is_ok()
        {
            return Ok((prefix.to_string(), model.to_string()));
        }
    }

    if let Some(base) = base_provider.map(str::trim).filter(|s| !s.is_empty()) {
        if base == ADAPTIVE_PROVIDER_NAME {
            anyhow::bail!("Adaptive provider cannot wrap itself");
        }
        return Ok((base.to_string(), trimmed.to_string()));
    }

    anyhow::bail!(
        "Cannot resolve provider for adaptive candidate '{trimmed}': \
         specify 'provider/model' with a registered provider or set ADAPTIVE_BASE_PROVIDER"
    )
}

fn parse_off_peak_windows(raw: &str) -> Result<Vec<TimeWindow>> {
    let mut windows = Vec::new();
    for part in raw.split(',') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            windows.push(TimeWindow::parse(trimmed)?);
        }
    }
    if windows.is_empty() {
        anyhow::bail!("ADAPTIVE_OFF_PEAK must contain at least one HH:MM-HH:MM window");
    }
    Ok(windows)
}

fn read_policy_from_config(config: &Config) -> Result<(AdaptivePolicyConfig, RoutingMode)> {
    let mut policy = AdaptivePolicyConfig::default();

    if let Ok(tz) = config.get_param::<String>("ADAPTIVE_TIMEZONE") {
        if !tz.trim().is_empty() {
            policy.timezone = tz;
        }
    }
    if let Ok(off_peak) = config.get_param::<String>("ADAPTIVE_OFF_PEAK") {
        if !off_peak.trim().is_empty() {
            policy.off_peak = parse_off_peak_windows(&off_peak)?;
        }
    }
    if let Ok(ratio_str) = config.get_param::<String>("ADAPTIVE_PROBE_RATIO") {
        if !ratio_str.trim().is_empty() {
            policy.probe_ratio = ratio_str.trim().parse::<f64>()?;
        }
    }
    if let Ok(cooldown_str) = config.get_param::<String>("ADAPTIVE_COOLDOWN_SECONDS") {
        if !cooldown_str.trim().is_empty() {
            let secs = cooldown_str.trim().parse::<u64>()?;
            policy.cooldown = Duration::from_secs(secs);
        }
    }
    if let Ok(stats_str) = config.get_param::<String>("ADAPTIVE_STATS_WINDOW_SECONDS") {
        if !stats_str.trim().is_empty() {
            let secs = stats_str.trim().parse::<u64>()?;
            policy.stats_window = Duration::from_secs(secs);
        }
    }
    if let Ok(min_samples_str) = config.get_param::<String>("ADAPTIVE_MIN_SAMPLES") {
        if !min_samples_str.trim().is_empty() {
            policy.min_samples_per_side = min_samples_str.trim().parse::<usize>()?;
        }
    }
    if let Ok(confirm_str) = config.get_param::<String>("ADAPTIVE_CONFIRM_COUNT") {
        if !confirm_str.trim().is_empty() {
            policy.confirm_count = confirm_str.trim().parse::<usize>()?;
        }
    }
    if let Ok(pause_str) = config.get_param::<String>("ADAPTIVE_PAUSE_SECONDS") {
        if !pause_str.trim().is_empty() {
            let secs = pause_str.trim().parse::<u64>()?;
            policy.economy_severe_timeout_pause = Duration::from_secs(secs);
        }
    }

    let mode = match config.get_param::<String>("ADAPTIVE_MODE") {
        Ok(m) if !m.trim().is_empty() => RoutingMode::from_str(&m)?,
        _ => RoutingMode::Auto,
    };

    policy.validate()?;
    Ok((policy, mode))
}

async fn build_candidate(
    provider_name: &str,
    model_name: &str,
    extensions: &[ExtensionConfig],
    working_dir: Option<&PathBuf>,
    cached_provider: Option<(&str, &Arc<dyn Provider>)>,
) -> Result<CandidateTarget> {
    let entry = super::get_from_registry(provider_name).await?;
    let model_config =
        crate::model_config::model_config_from_user_config(provider_name, model_name)?;
    let model_config = entry.normalize_model_config(model_config)?;

    let provider = match cached_provider {
        Some((cached_name, cached)) if cached_name == provider_name => Arc::clone(cached),
        _ => match working_dir {
            Some(dir) => {
                entry
                    .create_with_working_dir(extensions.to_vec(), dir.clone())
                    .await?
            }
            None => entry.create(extensions.to_vec()).await?,
        },
    };

    Ok(CandidateTarget {
        provider,
        model_config,
    })
}

async fn build_adaptive_provider(
    extensions: Vec<ExtensionConfig>,
    working_dir: Option<PathBuf>,
) -> Result<AdaptiveProvider> {
    let config = Config::global();
    let regular_spec: String = config.get_param("ADAPTIVE_REGULAR_MODEL")?;
    let economy_spec: String = config.get_param("ADAPTIVE_ECONOMY_MODEL")?;
    let regular_provider_override = config.get_param::<String>("ADAPTIVE_REGULAR_PROVIDER").ok();
    let economy_provider_override = config.get_param::<String>("ADAPTIVE_ECONOMY_PROVIDER").ok();
    let base_provider = config.get_param::<String>("ADAPTIVE_BASE_PROVIDER").ok();

    let (reg_provider_name, reg_model_name) = resolve_candidate_spec(
        &regular_spec,
        regular_provider_override.as_deref(),
        base_provider.as_deref(),
    )
    .await?;
    let (eco_provider_name, eco_model_name) = resolve_candidate_spec(
        &economy_spec,
        economy_provider_override.as_deref(),
        base_provider.as_deref(),
    )
    .await?;

    let (policy, mode) = read_policy_from_config(config)?;

    let regular = build_candidate(
        &reg_provider_name,
        &reg_model_name,
        &extensions,
        working_dir.as_ref(),
        None,
    )
    .await?;
    let economy = build_candidate(
        &eco_provider_name,
        &eco_model_name,
        &extensions,
        working_dir.as_ref(),
        Some((&reg_provider_name, &regular.provider)),
    )
    .await?;

    AdaptiveProvider::new(regular, economy, policy, mode)
}

impl ProviderDef for AdaptiveProviderDef {
    type Provider = AdaptiveProvider;

    fn from_env(
        extensions: Vec<ExtensionConfig>,
        _tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<AdaptiveProvider>> {
        Box::pin(async move { build_adaptive_provider(extensions, None).await })
    }

    fn from_env_with_working_dir(
        extensions: Vec<ExtensionConfig>,
        working_dir: PathBuf,
        _tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<AdaptiveProvider>> {
        Box::pin(async move { build_adaptive_provider(extensions, Some(working_dir)).await })
    }
}
