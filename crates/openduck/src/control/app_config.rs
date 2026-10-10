//! Hub-facing view of the user OpenDuck config file.
//!
//! Secret keys and extension definitions stay out of this API. Environment
//! variables that override a setting are reported and left unchanged.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::config::{env_lookup, Config, ConfigError};
use openduck_harness::JudgeConfig;

#[derive(Debug)]
pub enum AppConfigError {
    Invalid(String),
    Storage(String),
}

impl AppConfigError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

impl From<ConfigError> for AppConfigError {
    fn from(error: ConfigError) -> Self {
        Self::Storage(error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigField {
    pub value: Option<String>,
    pub from_env: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEntry {
    pub key: String,
    pub value: Value,
    pub from_env: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HarnessField<T> {
    pub value: Option<T>,
    pub from_env: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HarnessSettingsView {
    pub max_turns: HarnessField<usize>,
    pub stagnation_threshold: HarnessField<usize>,
    pub exploration_budget: HarnessField<usize>,
    pub periodic_review_interval: HarnessField<usize>,
    pub circuit_breaker_enabled: HarnessField<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessSettingsUpdate {
    pub max_turns: Option<usize>,
    pub stagnation_threshold: Option<usize>,
    pub exploration_budget: Option<usize>,
    pub periodic_review_interval: Option<usize>,
    pub circuit_breaker_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JudgePoint {
    pub id: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JudgeSettingsView {
    pub configured: bool,
    pub provider: String,
    pub endpoint: String,
    pub timeout_ms: u64,
    pub fallback_on_error: bool,
    pub points: Vec<JudgePoint>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgePointUpdate {
    pub id: String,
    pub mode: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeSettingsUpdate {
    pub provider: String,
    pub endpoint: String,
    pub timeout_ms: u64,
    pub fallback_on_error: bool,
    pub points: Vec<JudgePointUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfigDocument {
    pub provider: ConfigField,
    pub model: ConfigField,
    pub mode: ConfigField,
    pub harness: HarnessSettingsView,
    pub judge: JudgeSettingsView,
    pub entries: Vec<ConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSelectorField {
    pub value: Option<String>,
    pub file_value: Option<String>,
    pub from_env: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDetail {
    pub name: String,
    pub enabled: bool,
    pub configured: bool,
    pub model: String,
    pub model_env_value: Option<String>,
    pub fast_model: Option<String>,
    pub fast_model_env_value: Option<String>,
    pub secondary_model: Option<String>,
    pub secondary_model_env_value: Option<String>,
    pub resolved_fast_model: String,
    pub fast_model_uses_main: bool,
    pub suggested_fast_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyItem {
    pub id: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxiesConfigResponse {
    pub proxies: Vec<ProxyItem>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ProxiesConfigPutBody {
    pub proxies: Vec<ProxyItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersConfigResponse {
    pub major_provider: ProviderSelectorField,
    pub secondary_provider: ProviderSelectorField,
    pub model_from_env: bool,
    pub fast_model_from_env: bool,
    pub secondary_model_from_env: bool,
    pub available_providers: Vec<String>,
    pub providers: Vec<ProviderDetail>,
    #[serde(default)]
    pub proxies: Vec<ProxyItem>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigPutBody {
    pub major_provider: Option<String>,
    pub secondary_provider: Option<String>,
    pub providers: Vec<ProviderEntryPutItem>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEntryPutItem {
    pub name: String,
    pub enabled: bool,
    pub model: String,
    pub fast_model: Option<String>,
    pub secondary_model: Option<String>,
    #[serde(default)]
    pub proxy: Option<String>,
}

const KNOWN_JUDGE_POINTS: &[&str] = &[
    "turn.drift",
    "turn.completion",
    "context.forget",
    "tool.risk",
];

const STRUCTURAL_KEYS: &[&str] = &[
    "extensions",
    "providers",
    "proxies",
    "harness",
    "judge",
    "advisors",
    "notifications",
    "secrets",
    "active_provider",
    "secondary_provider",
    "provider_roles_migrated",
];

const DEDICATED_KEYS: &[&str] = &[
    "GOOSE_PROVIDER",
    "OPENDUCK_PROVIDER",
    "GOOSE_MODEL",
    "OPENDUCK_MODEL",
    "GOOSE_MODE",
    "OPENDUCK_MODE",
];

const LEGACY_PROVIDER_ROLE_KEYS: &[&str] = &[
    "GOOSE_FAST_MODEL",
    "OPENDUCK_FAST_MODEL",
    "GOOSE_SECONDARY_PROVIDER",
    "OPENDUCK_SECONDARY_PROVIDER",
    "GOOSE_SECONDARY_MODEL",
    "OPENDUCK_SECONDARY_MODEL",
];

fn is_legacy_provider_role_key(key: &str) -> bool {
    LEGACY_PROVIDER_ROLE_KEYS
        .iter()
        .any(|k| k.eq_ignore_ascii_case(key))
}

const MAX_TURNS_ENV: &[&str] = &["OPENDUCK_MAX_TURNS", "HARNESS_MAX_TURNS", "GOOSE_MAX_TURNS"];
const MAX_TURNS_FILE: &[&str] = &[
    "max_turns",
    "maxTurns",
    "GOOSE_MAX_TURNS",
    "OPENDUCK_MAX_TURNS",
    "HARNESS_MAX_TURNS",
];
const STAGNATION_ENV: &[&str] = &[
    "OPENDUCK_STAGNATION_THRESHOLD",
    "HARNESS_STAGNATION_THRESHOLD",
    "GOOSE_STAGNATION_THRESHOLD",
];
const STAGNATION_FILE: &[&str] = &[
    "stagnation_threshold",
    "stagnationThreshold",
    "GOOSE_STAGNATION_THRESHOLD",
    "OPENDUCK_STAGNATION_THRESHOLD",
    "HARNESS_STAGNATION_THRESHOLD",
];
const EXPLORATION_ENV: &[&str] = &[
    "OPENDUCK_EXPLORATION_BUDGET",
    "HARNESS_EXPLORATION_BUDGET",
    "GOOSE_EXPLORATION_BUDGET",
];
const EXPLORATION_FILE: &[&str] = &[
    "exploration_budget",
    "explorationBudget",
    "max_exploration_steps",
    "GOOSE_EXPLORATION_BUDGET",
    "OPENDUCK_EXPLORATION_BUDGET",
    "HARNESS_EXPLORATION_BUDGET",
];
const REVIEW_ENV: &[&str] = &[
    "OPENDUCK_PERIODIC_REVIEW_INTERVAL",
    "HARNESS_PERIODIC_REVIEW_INTERVAL",
    "GOOSE_PERIODIC_REVIEW_INTERVAL",
];
const REVIEW_FILE: &[&str] = &[
    "periodic_review_interval",
    "periodicReviewInterval",
    "GOOSE_PERIODIC_REVIEW_INTERVAL",
    "OPENDUCK_PERIODIC_REVIEW_INTERVAL",
    "HARNESS_PERIODIC_REVIEW_INTERVAL",
];
const CIRCUIT_ENV: &[&str] = &[
    "OPENDUCK_CIRCUIT_BREAKER",
    "HARNESS_CIRCUIT_BREAKER",
    "GOOSE_CIRCUIT_BREAKER",
];
const CIRCUIT_FILE: &[&str] = &[
    "circuit_breaker_enabled",
    "circuit_breaker",
    "circuitBreaker",
    "circuitBreakerEnabled",
    "GOOSE_CIRCUIT_BREAKER",
    "OPENDUCK_CIRCUIT_BREAKER",
    "HARNESS_CIRCUIT_BREAKER",
];

pub fn read_document(config: &Config) -> Result<AppConfigDocument, AppConfigError> {
    let harness_map = load_harness_map(config)?;
    Ok(AppConfigDocument {
        provider: read_provider(config),
        model: read_model(config),
        mode: read_mode(config)?,
        harness: read_harness(config, harness_map.as_ref())?,
        judge: read_judge(config)?,
        entries: read_entries(config)?,
    })
}

pub fn upsert_entry(
    config: &Config,
    key: &str,
    value: Value,
) -> Result<AppConfigDocument, AppConfigError> {
    let key = key.trim();
    validate_key(key)?;
    if is_legacy_provider_role_key(key) {
        return Err(AppConfigError::invalid("edit this on the provider"));
    }
    if env_overrides_key(key) {
        return Err(AppConfigError::invalid(format!(
            "{key} is set by the environment and cannot be changed here"
        )));
    }
    match canonical_kind(key) {
        SettingKind::Provider => write_provider(config, &value)?,
        SettingKind::Model => write_model(config, &value)?,
        SettingKind::Mode => write_mode(config, &value)?,
        SettingKind::HarnessOwned => {
            return Err(AppConfigError::invalid(
                "edit this value in the harness section",
            ));
        }
        SettingKind::Generic => {
            ensure_scalar(&value)?;
            config.set_param(key, &value)?;
        }
    }
    read_document(config)
}

pub fn remove_entry(config: &Config, key: &str) -> Result<AppConfigDocument, AppConfigError> {
    let key = key.trim();
    validate_key(key)?;
    if is_legacy_provider_role_key(key) {
        return Err(AppConfigError::invalid("edit this on the provider"));
    }
    if env_overrides_key(key) {
        return Err(AppConfigError::invalid(format!(
            "{key} is set by the environment and cannot be changed here"
        )));
    }
    match canonical_kind(key) {
        SettingKind::Provider => {
            crate::config::clear_active_provider(config)?;
        }
        SettingKind::Model => clear_model(config)?,
        SettingKind::Mode => delete_keys(config, &["GOOSE_MODE", "OPENDUCK_MODE"])?,
        SettingKind::HarnessOwned => {
            return Err(AppConfigError::invalid(
                "edit this value in the harness section",
            ));
        }
        SettingKind::Generic => {
            config.delete(key)?;
        }
    }
    read_document(config)
}

pub fn update_harness(
    config: &Config,
    update: HarnessSettingsUpdate,
) -> Result<AppConfigDocument, AppConfigError> {
    let existing = load_harness_map(config)?;
    let had_harness = existing.is_some();
    let mut map = existing.unwrap_or_default();
    let max_turns_env = positive_env(MAX_TURNS_ENV).is_some();
    let stagnation_env = first_present_usize(STAGNATION_ENV).is_some();
    let exploration_env = first_present_usize(EXPLORATION_ENV).is_some();
    let review_env = first_present_usize(REVIEW_ENV).is_some();
    let circuit_env = first_present_flag(CIRCUIT_ENV).is_some();
    if !max_turns_env {
        validate_count("Max turns", update.max_turns)?;
    }
    if !stagnation_env {
        validate_count("Stagnation threshold", update.stagnation_threshold)?;
    }
    if !exploration_env {
        validate_count("Exploration budget", update.exploration_budget)?;
    }
    if !review_env {
        validate_count("Review interval", update.periodic_review_interval)?;
    }

    apply_count(
        &mut map,
        max_turns_env,
        update.max_turns,
        &["max_turns", "maxTurns"],
    );
    apply_count(
        &mut map,
        stagnation_env,
        update.stagnation_threshold,
        &["stagnation_threshold", "stagnationThreshold"],
    );
    apply_count(
        &mut map,
        exploration_env,
        update.exploration_budget,
        &[
            "exploration_budget",
            "explorationBudget",
            "max_exploration_steps",
        ],
    );
    apply_count(
        &mut map,
        review_env,
        update.periodic_review_interval,
        &["periodic_review_interval", "periodicReviewInterval"],
    );
    apply_flag(
        &mut map,
        circuit_env,
        update.circuit_breaker_enabled,
        CIRCUIT_FILE,
    );
    if map.is_empty() {
        if had_harness {
            config.delete("harness")?;
        }
    } else {
        config.set_param("harness", Value::Object(map))?;
    }

    if !max_turns_env {
        delete_keys(config, MAX_TURNS_FILE)?;
    }
    if !stagnation_env {
        delete_keys(config, STAGNATION_FILE)?;
    }
    if !exploration_env {
        delete_keys(config, EXPLORATION_FILE)?;
    }
    if !review_env {
        delete_keys(config, REVIEW_FILE)?;
    }
    if !circuit_env {
        delete_keys(config, CIRCUIT_FILE)?;
    }
    read_document(config)
}

pub fn update_judge(
    config: &Config,
    update: JudgeSettingsUpdate,
) -> Result<AppConfigDocument, AppConfigError> {
    let provider = require_token(&update.provider, "Judge provider")?;
    let endpoint = update.endpoint.trim();
    if endpoint.is_empty() || endpoint.len() > 512 || endpoint.chars().any(|c| c.is_control()) {
        return Err(AppConfigError::invalid(
            "Judge endpoint must be a non-empty URL",
        ));
    }
    if update.timeout_ms < 1 {
        return Err(AppConfigError::invalid(
            "Judge timeout must be at least 1 millisecond",
        ));
    }
    let points = normalize_judge_points(&update.points)?;
    let mut map = match load_object(config, "judge")? {
        Some(map) => map,
        None => Map::new(),
    };
    map.insert("provider".to_string(), Value::String(provider));
    map.insert("endpoint".to_string(), Value::String(endpoint.to_string()));
    map.insert("timeout_ms".to_string(), Value::from(update.timeout_ms));
    map.remove("timeoutMs");
    map.insert(
        "fallback_on_error".to_string(),
        Value::Bool(update.fallback_on_error),
    );
    map.remove("fallbackOnError");
    map.insert("points".to_string(), Value::Object(points));
    config.set_param("judge", Value::Object(map))?;
    read_document(config)
}

pub fn remove_judge(config: &Config) -> Result<AppConfigDocument, AppConfigError> {
    config.delete("judge")?;
    read_document(config)
}

fn check_no_secret_keys(val: &Value) -> Result<(), AppConfigError> {
    match val {
        Value::Object(map) => {
            for (key, child) in map {
                let upper = key.to_ascii_uppercase();
                if is_secret_key(key)
                    || upper.contains("APIKEY")
                    || upper.contains("API_KEY")
                    || upper == "KEY"
                    || upper == "TOKEN"
                    || upper == "SECRET"
                {
                    return Err(AppConfigError::invalid(
                        "API keys and tokens are not edited here",
                    ));
                }
                check_no_secret_keys(child)?;
            }
        }
        Value::Array(arr) => {
            for item in arr {
                check_no_secret_keys(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub async fn get_providers_config(
    config: &Config,
) -> Result<ProvidersConfigResponse, AppConfigError> {
    let major_file_value = config
        .file_string("active_provider")
        .or_else(|| config.file_string("GOOSE_PROVIDER"));
    let major_env_val = crate::config::get_var("PROVIDER")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let (major_val, major_from_env) = if let Some(env_val) = major_env_val {
        (Some(env_val), true)
    } else {
        (major_file_value.clone(), false)
    };
    let major_provider = ProviderSelectorField {
        value: major_val,
        file_value: major_file_value,
        from_env: major_from_env,
    };

    let secondary_file_value =
        if config.file_has_key(crate::config::providers::SECONDARY_PROVIDER_KEY) {
            config.file_string(crate::config::providers::SECONDARY_PROVIDER_KEY)
        } else if !crate::config::providers::provider_roles_migrated(config) {
            config
                .file_string("GOOSE_SECONDARY_PROVIDER")
                .or_else(|| config.file_string("OPENDUCK_SECONDARY_PROVIDER"))
        } else {
            None
        };
    let secondary_env_val = crate::config::get_var("SECONDARY_PROVIDER")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let (secondary_val, secondary_from_env) = if let Some(env_val) = secondary_env_val {
        (Some(env_val), true)
    } else {
        (secondary_file_value.clone(), false)
    };
    let secondary_provider = ProviderSelectorField {
        value: secondary_val,
        file_value: secondary_file_value,
        from_env: secondary_from_env,
    };

    let model_from_env = crate::config::get_var("MODEL")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .is_some();
    let fast_model_from_env = crate::config::get_var("FAST_MODEL")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .is_some();
    let secondary_model_from_env = crate::config::get_var("SECONDARY_MODEL")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .is_some();

    let _ = crate::providers::refresh_custom_providers().await;
    let registry_list = crate::providers::providers().await;
    let map = crate::config::providers::get_providers_map(config);

    let mut available_providers: Vec<String> = registry_list
        .iter()
        .map(|(meta, _)| meta.name.clone())
        .filter(|name| !map.contains_key(name))
        .collect();
    available_providers.sort();
    available_providers.dedup();

    let mut provider_details = Vec::new();
    for (name, entry) in map.into_iter() {
        let is_effective_major = major_provider.value.as_deref() == Some(&name);
        let is_effective_secondary = secondary_provider.value.as_deref() == Some(&name);

        let model_env_value = if model_from_env && is_effective_major {
            crate::config::get_var("MODEL").filter(|s| !s.trim().is_empty())
        } else {
            None
        };

        let fast_model_env_value = if fast_model_from_env {
            crate::config::get_var("FAST_MODEL").filter(|s| !s.trim().is_empty())
        } else {
            None
        };

        let secondary_model_env_value = if secondary_model_from_env && is_effective_secondary {
            crate::config::get_var("SECONDARY_MODEL").filter(|s| !s.trim().is_empty())
        } else {
            None
        };

        let resolved_fast_model = if let Some(fm_env) =
            crate::config::get_var("FAST_MODEL").filter(|s| !s.trim().is_empty())
        {
            fm_env
        } else if let Some(fm) = entry.fast_model.as_deref().filter(|s| !s.trim().is_empty()) {
            fm.to_string()
        } else if !crate::config::providers::provider_roles_migrated(config) && is_effective_major {
            config
                .file_string("GOOSE_FAST_MODEL")
                .or_else(|| config.file_string("OPENDUCK_FAST_MODEL"))
                .unwrap_or_else(|| entry.model.clone())
        } else {
            entry.model.clone()
        };

        let fast_model_uses_main = resolved_fast_model == entry.model;
        let suggested_fast_model = crate::model_config::suggested_fast_model_name(&name).await;

        provider_details.push(ProviderDetail {
            name,
            enabled: entry.enabled,
            configured: entry.configured,
            model: entry.model,
            model_env_value,
            fast_model: entry.fast_model,
            fast_model_env_value,
            secondary_model: entry.secondary_model,
            secondary_model_env_value,
            resolved_fast_model,
            fast_model_uses_main,
            suggested_fast_model,
            proxy: entry.proxy,
        });
    }

    let proxies_map = crate::config::proxies::get_proxies_map(config);
    let proxies: Vec<ProxyItem> = proxies_map
        .into_iter()
        .map(|(id, entry)| ProxyItem {
            id,
            url: entry.url,
            description: entry.description,
        })
        .collect();

    Ok(ProvidersConfigResponse {
        major_provider,
        secondary_provider,
        model_from_env,
        fast_model_from_env,
        secondary_model_from_env,
        available_providers,
        providers: provider_details,
        proxies,
    })
}

pub async fn update_providers_config(
    config: &Config,
    raw_body: Value,
) -> Result<ProvidersConfigResponse, AppConfigError> {
    check_no_secret_keys(&raw_body)?;

    let raw_obj = raw_body
        .as_object()
        .ok_or_else(|| AppConfigError::invalid("request body must be a JSON object"))?;
    if !raw_obj.contains_key("majorProvider") {
        return Err(AppConfigError::invalid("majorProvider field is required"));
    }
    if !raw_obj.contains_key("secondaryProvider") {
        return Err(AppConfigError::invalid(
            "secondaryProvider field is required",
        ));
    }
    let raw_providers = raw_obj
        .get("providers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            AppConfigError::invalid("providers field is required and must be an array")
        })?;
    for (idx, item) in raw_providers.iter().enumerate() {
        let item_obj = item.as_object().ok_or_else(|| {
            AppConfigError::invalid(format!("providers[{idx}] must be an object"))
        })?;
        if !item_obj.contains_key("name") {
            return Err(AppConfigError::invalid(format!(
                "providers[{idx}].name is required"
            )));
        }
        if !item_obj.contains_key("enabled") {
            return Err(AppConfigError::invalid(format!(
                "providers[{idx}].enabled is required"
            )));
        }
        if !item_obj.contains_key("model") {
            return Err(AppConfigError::invalid(format!(
                "providers[{idx}].model is required"
            )));
        }
        if item_obj.get("model").map(|v| v.is_null()).unwrap_or(false) {
            return Err(AppConfigError::invalid(format!(
                "providers[{idx}].model cannot be null"
            )));
        }
        if !item_obj.contains_key("fastModel") {
            return Err(AppConfigError::invalid(format!(
                "providers[{idx}].fastModel is required"
            )));
        }
        if !item_obj.contains_key("secondaryModel") {
            return Err(AppConfigError::invalid(format!(
                "providers[{idx}].secondaryModel is required"
            )));
        }
    }

    let body: ProviderConfigPutBody =
        serde_json::from_value(raw_body).map_err(|e| AppConfigError::invalid(e.to_string()))?;

    let req_major = body
        .major_provider
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let req_secondary = body
        .secondary_provider
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    if let (Some(m), Some(s)) = (&req_major, &req_secondary) {
        if m == s {
            return Err(AppConfigError::invalid(
                "Secondary provider cannot equal major provider",
            ));
        }
    }

    // 1. PROVIDER env lock
    if crate::config::get_var("PROVIDER")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        let file_major = config
            .file_string("active_provider")
            .or_else(|| config.file_string("GOOSE_PROVIDER"));
        if req_major != file_major {
            return Err(AppConfigError::invalid(
                "Major provider is locked by environment",
            ));
        }
    }

    // 2. SECONDARY_PROVIDER env lock
    if crate::config::get_var("SECONDARY_PROVIDER")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        let file_secondary =
            if config.file_has_key(crate::config::providers::SECONDARY_PROVIDER_KEY) {
                config.file_string(crate::config::providers::SECONDARY_PROVIDER_KEY)
            } else if !crate::config::providers::provider_roles_migrated(config) {
                config
                    .file_string("GOOSE_SECONDARY_PROVIDER")
                    .or_else(|| config.file_string("OPENDUCK_SECONDARY_PROVIDER"))
            } else {
                None
            };
        if req_secondary != file_secondary {
            return Err(AppConfigError::invalid(
                "Secondary provider is locked by environment",
            ));
        }
    }

    let existing_map = crate::config::providers::get_providers_map(config);

    // 3. MODEL env lock
    if crate::config::get_var("MODEL")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        let file_major = config
            .file_string("active_provider")
            .or_else(|| config.file_string("GOOSE_PROVIDER"));
        if let Some(major_name) = file_major {
            let file_model = existing_map
                .get(&major_name)
                .map(|e| e.model.trim())
                .unwrap_or("");
            if let Some(sub_item) = body.providers.iter().find(|p| p.name.trim() == major_name) {
                if sub_item.model.trim() != file_model {
                    return Err(AppConfigError::invalid(
                        "Model on major provider is locked by environment",
                    ));
                }
            }
        }
    }

    // 4. FAST_MODEL env lock
    if crate::config::get_var("FAST_MODEL")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        for item in &body.providers {
            let file_fast = existing_map
                .get(item.name.trim())
                .and_then(|e| e.fast_model.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let sub_fast = item
                .fast_model
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty() && *s != item.model.trim());
            if file_fast != sub_fast {
                return Err(AppConfigError::invalid(format!(
                    "Fast model on {} is locked by environment",
                    item.name
                )));
            }
        }
    }

    // 5. SECONDARY_MODEL env lock
    if crate::config::get_var("SECONDARY_MODEL")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        let file_secondary =
            if config.file_has_key(crate::config::providers::SECONDARY_PROVIDER_KEY) {
                config.file_string(crate::config::providers::SECONDARY_PROVIDER_KEY)
            } else if !crate::config::providers::provider_roles_migrated(config) {
                config
                    .file_string("GOOSE_SECONDARY_PROVIDER")
                    .or_else(|| config.file_string("OPENDUCK_SECONDARY_PROVIDER"))
            } else {
                None
            };
        if let Some(sec_name) = file_secondary {
            let file_model = existing_map
                .get(&sec_name)
                .map(|e| e.model.trim())
                .unwrap_or("");
            if let Some(sub_item) = body.providers.iter().find(|p| p.name.trim() == sec_name) {
                if sub_item.model.trim() != file_model {
                    return Err(AppConfigError::invalid(
                        "Secondary model is locked by environment",
                    ));
                }
            }
        }
    }

    let _ = crate::providers::refresh_custom_providers().await;
    let registry_list = crate::providers::providers().await;
    let registry_names: std::collections::HashSet<String> =
        registry_list.into_iter().map(|(m, _)| m.name).collect();

    for item in &body.providers {
        let name = item.name.trim();
        if !existing_map.contains_key(name) && !registry_names.contains(name) {
            return Err(AppConfigError::invalid(format!(
                "Provider '{name}' is not in available providers"
            )));
        }
    }

    let mut seen = std::collections::HashSet::new();
    for item in &body.providers {
        let name = require_token(&item.name, "Provider name")?;
        if !seen.insert(name) {
            return Err(AppConfigError::invalid(format!(
                "Duplicate provider: {}",
                item.name
            )));
        }
    }

    let mut new_map = indexmap::IndexMap::new();
    for item in &body.providers {
        let name = require_token(&item.name, "Provider name")?;
        let is_major = req_major.as_deref() == Some(&name);
        let is_secondary = req_secondary.as_deref() == Some(&name);

        let model = item.model.trim();
        let model = if model.is_empty() {
            if is_major || is_secondary {
                return Err(AppConfigError::invalid(format!(
                    "Selected provider '{name}' cannot have an empty model"
                )));
            }
            String::new()
        } else {
            require_token(model, "Model")?
        };

        let fast_model = item
            .fast_model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != model);
        let fast_model = if let Some(fm) = fast_model {
            Some(require_token(fm, "Fast model")?)
        } else {
            None
        };

        let secondary_model = item
            .secondary_model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let secondary_model = if let Some(sm) = secondary_model {
            Some(require_token(sm, "Secondary model")?)
        } else {
            None
        };

        let is_live = registry_names.contains(&name);
        let previously_configured = existing_map
            .get(&name)
            .map(|e| e.configured)
            .unwrap_or(false);
        let configured = !model.is_empty() && (is_live || previously_configured);

        let proxy = item
            .proxy
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        new_map.insert(
            name,
            crate::config::providers::ProviderEntry {
                enabled: item.enabled,
                configured,
                model,
                fast_model,
                secondary_model,
                proxy,
            },
        );
    }

    if let Some(ref major) = req_major {
        let entry = new_map.get(major).ok_or_else(|| {
            AppConfigError::invalid(format!(
                "Selected major provider '{major}' not found in providers"
            ))
        })?;
        if !entry.enabled {
            return Err(AppConfigError::invalid(format!(
                "Selected major provider '{major}' is disabled"
            )));
        }
        if !entry.configured {
            return Err(AppConfigError::invalid(format!(
                "Selected major provider '{major}' is not configured"
            )));
        }
        if entry.model.trim().is_empty() {
            return Err(AppConfigError::invalid(format!(
                "Selected major provider '{major}' has an empty model"
            )));
        }
    }

    if let Some(ref sec) = req_secondary {
        let entry = new_map.get(sec).ok_or_else(|| {
            AppConfigError::invalid(format!(
                "Selected secondary provider '{sec}' not found in providers"
            ))
        })?;
        if !entry.enabled {
            return Err(AppConfigError::invalid(format!(
                "Selected secondary provider '{sec}' is disabled"
            )));
        }
        if !entry.configured {
            return Err(AppConfigError::invalid(format!(
                "Selected secondary provider '{sec}' is not configured"
            )));
        }
        if entry.model.trim().is_empty() {
            return Err(AppConfigError::invalid(format!(
                "Selected secondary provider '{sec}' has an empty model"
            )));
        }
    }

    config.save_provider_configuration(req_major.as_deref(), req_secondary.as_deref(), &new_map)?;

    get_providers_config(config).await
}

pub fn get_proxies_config(config: &Config) -> Result<ProxiesConfigResponse, AppConfigError> {
    let map = crate::config::proxies::get_proxies_map(config);
    let proxies = map
        .into_iter()
        .map(|(id, entry)| ProxyItem {
            id,
            url: entry.url,
            description: entry.description,
        })
        .collect();
    Ok(ProxiesConfigResponse { proxies })
}

pub fn update_proxies_config(
    config: &Config,
    raw_body: Value,
) -> Result<ProxiesConfigResponse, AppConfigError> {
    check_no_secret_keys(&raw_body)?;
    let body: ProxiesConfigPutBody = serde_json::from_value(raw_body)
        .map_err(|e| AppConfigError::invalid(format!("Invalid proxies payload: {e}")))?;

    let mut seen_ids = std::collections::HashSet::new();
    let mut clean_map = indexmap::IndexMap::new();

    for item in body.proxies {
        let id = require_token(&item.id, "Proxy ID")?;
        if id.contains(':') || id.contains('/') || id.contains('\\') {
            return Err(AppConfigError::invalid(format!(
                "Proxy ID '{id}' contains invalid characters"
            )));
        }
        if !seen_ids.insert(id.clone()) {
            return Err(AppConfigError::invalid(format!("Duplicate proxy ID: {id}")));
        }
        let url = item.url.trim();
        if url.is_empty() {
            return Err(AppConfigError::invalid(format!(
                "Proxy '{id}' has an empty URL"
            )));
        }
        if !url.starts_with("http://")
            && !url.starts_with("https://")
            && !url.starts_with("socks5://")
            && !url.starts_with("socks5h://")
        {
            return Err(AppConfigError::invalid(format!(
                "Proxy '{id}' URL must start with http://, https://, or socks5://"
            )));
        }

        let description = item.description.and_then(|d| {
            let trimmed = d.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

        clean_map.insert(
            id,
            crate::config::proxies::ProxyEntry::new(url, description),
        );
    }

    crate::config::proxies::set_proxies_map(config, &clean_map)?;
    get_proxies_config(config)
}

enum SettingKind {
    Provider,
    Model,
    Mode,
    HarnessOwned,
    Generic,
}

fn canonical_kind(key: &str) -> SettingKind {
    match key.to_ascii_uppercase().as_str() {
        "GOOSE_PROVIDER" | "OPENDUCK_PROVIDER" => SettingKind::Provider,
        "GOOSE_MODEL" | "OPENDUCK_MODEL" => SettingKind::Model,
        "GOOSE_MODE" | "OPENDUCK_MODE" => SettingKind::Mode,
        _ if is_harness_owned(key) => SettingKind::HarnessOwned,
        _ => SettingKind::Generic,
    }
}

fn validate_key(key: &str) -> Result<(), AppConfigError> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        || key.starts_with('.')
    {
        return Err(AppConfigError::invalid(
            "use letters, numbers, underscores, dots, or hyphens",
        ));
    }
    if is_secret_key(key) {
        return Err(AppConfigError::invalid(
            "API keys and tokens are not edited here",
        ));
    }
    if is_structural_key(key) {
        return Err(AppConfigError::invalid(
            "this block is not edited as a single value",
        ));
    }
    Ok(())
}

fn is_secret_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper.contains("_KEY")
        || upper.contains("_TOKEN")
        || upper.contains("_SECRET")
        || upper.contains("PASSWORD")
        || upper.contains("CREDENTIAL")
        || upper.contains("AUTHORIZATION")
}

fn is_structural_key(key: &str) -> bool {
    STRUCTURAL_KEYS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(key))
}

fn is_harness_owned(key: &str) -> bool {
    MAX_TURNS_FILE
        .iter()
        .chain(STAGNATION_FILE)
        .chain(EXPLORATION_FILE)
        .chain(REVIEW_FILE)
        .chain(CIRCUIT_FILE)
        .any(|candidate| candidate.eq_ignore_ascii_case(key))
}

fn is_dedicated_key(key: &str) -> bool {
    DEDICATED_KEYS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(key))
}

fn env_overrides_key(key: &str) -> bool {
    match canonical_kind(key) {
        SettingKind::Provider => crate::config::get_var("PROVIDER").is_some(),
        SettingKind::Model => crate::config::get_var("MODEL").is_some(),
        SettingKind::Mode => env_lookup("GOOSE_MODE").is_some(),
        SettingKind::HarnessOwned | SettingKind::Generic => env_lookup(key).is_some(),
    }
}

fn ensure_scalar(value: &Value) -> Result<(), AppConfigError> {
    if matches!(value, Value::Bool(_) | Value::Number(_) | Value::String(_)) {
        Ok(())
    } else {
        Err(AppConfigError::invalid(
            "value must be a string, number, or boolean",
        ))
    }
}

fn require_text(value: &Value, label: &str) -> Result<String, AppConfigError> {
    let Some(text) = value.as_str() else {
        return Err(AppConfigError::invalid(format!("{label} must be a string")));
    };
    Ok(text.to_string())
}

fn require_token(value: &str, label: &str) -> Result<String, AppConfigError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 128 || trimmed.chars().any(|c| c.is_control()) {
        return Err(AppConfigError::invalid(format!(
            "{label} must be a short name without blank lines"
        )));
    }
    Ok(trimmed.to_string())
}

fn write_provider(config: &Config, value: &Value) -> Result<(), AppConfigError> {
    let text = require_text(value, "Provider")?;
    if text.trim().is_empty() {
        crate::config::clear_active_provider(config)?;
        return Ok(());
    }
    let name = require_token(&text, "Provider")?;
    let entry = crate::config::get_provider_entry(config, &name)
        .ok_or_else(|| AppConfigError::invalid(format!("Provider '{name}' is not configured")))?;
    if !entry.enabled {
        return Err(AppConfigError::invalid(format!(
            "Provider '{name}' is disabled"
        )));
    }
    if !entry.configured {
        return Err(AppConfigError::invalid(format!(
            "Provider '{name}' is not configured"
        )));
    }
    if entry.model.trim().is_empty() {
        return Err(AppConfigError::invalid(format!(
            "Provider '{name}' has an empty model"
        )));
    }
    config.set_param(crate::config::providers::ACTIVE_PROVIDER_KEY, &name)?;
    Ok(())
}

fn write_model(config: &Config, value: &Value) -> Result<(), AppConfigError> {
    let text = require_text(value, "Model")?;
    if text.trim().is_empty() {
        return clear_model(config);
    }
    let model = require_token(&text, "Model")?;
    if let Some(provider) = crate::config::get_active_provider(config) {
        crate::config::set_active_provider(config, &provider, &model)?;
    } else {
        config.set_param("GOOSE_MODEL", &model)?;
    }
    config.delete("OPENDUCK_MODEL")?;
    Ok(())
}

fn clear_model(config: &Config) -> Result<(), AppConfigError> {
    if let Some(provider) = crate::config::get_active_provider(config) {
        crate::config::clear_provider_model(config, &provider)?;
    }
    delete_keys(config, &["GOOSE_MODEL", "OPENDUCK_MODEL"])?;
    Ok(())
}

fn write_mode(config: &Config, value: &Value) -> Result<(), AppConfigError> {
    let text = require_text(value, "Mode")?;
    if text.trim().is_empty() {
        delete_keys(config, &["GOOSE_MODE", "OPENDUCK_MODE"])?;
        return Ok(());
    }
    let mode = normalize_mode(&text)?;
    config.set_param("GOOSE_MODE", mode)?;
    config.delete("OPENDUCK_MODE")?;
    Ok(())
}

fn normalize_mode(value: &str) -> Result<&'static str, AppConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok("auto"),
        "approve" => Ok("approve"),
        "smart_approve" | "smart-approve" | "smartapprove" => Ok("smart_approve"),
        "chat" => Ok("chat"),
        _ => Err(AppConfigError::invalid(
            "mode must be auto, approve, smart_approve, or chat",
        )),
    }
}

fn normalize_judge_mode(value: &str) -> Result<&'static str, AppConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Ok("off"),
        "shadow" => Ok("shadow"),
        "active" => Ok("active"),
        _ => Err(AppConfigError::invalid(
            "judge point mode must be off, shadow, or active",
        )),
    }
}

fn normalize_judge_points(
    points: &[JudgePointUpdate],
) -> Result<Map<String, Value>, AppConfigError> {
    let mut map = Map::new();
    for point in points {
        let id = point.id.trim();
        if id.is_empty()
            || id.len() > 64
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(AppConfigError::invalid(
                "decision point ids use letters, numbers, dots, or hyphens",
            ));
        }
        if map.contains_key(id) {
            return Err(AppConfigError::invalid(format!(
                "decision point {id} is listed twice"
            )));
        }
        let mode = normalize_judge_mode(&point.mode)?;
        map.insert(id.to_string(), Value::String(mode.to_string()));
    }
    Ok(map)
}

fn validate_count(label: &str, value: Option<usize>) -> Result<(), AppConfigError> {
    if matches!(value, Some(0)) {
        return Err(AppConfigError::invalid(format!(
            "{label} must be at least 1"
        )));
    }
    Ok(())
}

fn read_provider(config: &Config) -> ConfigField {
    ConfigField {
        value: crate::config::get_active_provider(config).filter(|value| !value.trim().is_empty()),
        from_env: crate::config::get_var("PROVIDER").is_some(),
    }
}

fn read_model(config: &Config) -> ConfigField {
    ConfigField {
        value: crate::config::get_active_model(config).filter(|value| !value.trim().is_empty()),
        from_env: crate::config::get_var("MODEL").is_some(),
    }
}

fn read_mode(config: &Config) -> Result<ConfigField, AppConfigError> {
    let from_env = env_lookup("GOOSE_MODE").is_some();
    let value = match config.get_param::<String>("GOOSE_MODE") {
        Ok(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(ConfigError::NotFound(_)) => match config.get_param::<String>("OPENDUCK_MODE") {
            Ok(value) => {
                let trimmed = value.trim().to_string();
                (!trimmed.is_empty()).then_some(trimmed)
            }
            Err(ConfigError::NotFound(_)) => None,
            Err(error) => return Err(error.into()),
        },
        Err(error) => return Err(error.into()),
    };
    Ok(ConfigField { value, from_env })
}

fn read_entries(config: &Config) -> Result<Vec<ConfigEntry>, AppConfigError> {
    let values = config.all_values()?;
    let mut keys: Vec<String> = values.keys().cloned().collect();
    keys.sort();
    let mut entries = Vec::new();
    for key in keys {
        if is_secret_key(&key)
            || is_structural_key(&key)
            || is_dedicated_key(&key)
            || is_harness_owned(&key)
            || is_legacy_provider_role_key(&key)
        {
            continue;
        }
        let Some(file_value) = values.get(&key) else {
            continue;
        };
        if !is_public_scalar(file_value) {
            continue;
        }
        let from_env = env_lookup(&key).is_some();
        let value = if from_env {
            match config.get_param::<Value>(&key) {
                Ok(env_value) if is_public_scalar(&env_value) => env_value,
                _ => file_value.clone(),
            }
        } else {
            file_value.clone()
        };
        entries.push(ConfigEntry {
            key,
            value,
            from_env,
        });
    }
    Ok(entries)
}

fn is_public_scalar(value: &Value) -> bool {
    matches!(value, Value::Bool(_) | Value::Number(_) | Value::String(_))
}

fn read_harness(
    config: &Config,
    harness_map: Option<&Map<String, Value>>,
) -> Result<HarnessSettingsView, AppConfigError> {
    let empty = Map::new();
    let map = harness_map.unwrap_or(&empty);
    Ok(HarnessSettingsView {
        max_turns: count_field(
            config,
            positive_env(MAX_TURNS_ENV),
            map,
            &["max_turns", "maxTurns"],
            MAX_TURNS_FILE,
        )?,
        stagnation_threshold: count_field(
            config,
            first_present_usize(STAGNATION_ENV),
            map,
            &["stagnation_threshold", "stagnationThreshold"],
            STAGNATION_FILE,
        )?,
        exploration_budget: count_field(
            config,
            first_present_usize(EXPLORATION_ENV),
            map,
            &[
                "exploration_budget",
                "explorationBudget",
                "max_exploration_steps",
            ],
            EXPLORATION_FILE,
        )?,
        periodic_review_interval: count_field(
            config,
            first_present_usize(REVIEW_ENV),
            map,
            &["periodic_review_interval", "periodicReviewInterval"],
            REVIEW_FILE,
        )?,
        circuit_breaker_enabled: flag_field(config, map)?,
    })
}

fn count_field(
    config: &Config,
    from_env: Option<usize>,
    map: &Map<String, Value>,
    harness_keys: &[&str],
    top_level_keys: &[&str],
) -> Result<HarnessField<usize>, AppConfigError> {
    if let Some(value) = from_env {
        return Ok(HarnessField {
            value: Some(value),
            from_env: true,
        });
    }
    if let Some(value) = first_usize_in_map(map, harness_keys) {
        return Ok(HarnessField {
            value: Some(value),
            from_env: false,
        });
    }
    for key in top_level_keys {
        if let Some(value) = optional_usize_param(config, key)? {
            return Ok(HarnessField {
                value: Some(value),
                from_env: false,
            });
        }
    }
    Ok(HarnessField {
        value: None,
        from_env: false,
    })
}

fn flag_field(
    config: &Config,
    map: &Map<String, Value>,
) -> Result<HarnessField<bool>, AppConfigError> {
    if let Some(value) = first_present_flag(CIRCUIT_ENV) {
        return Ok(HarnessField {
            value: Some(value),
            from_env: true,
        });
    }
    if let Some(value) = first_bool_in_map(map, CIRCUIT_FILE) {
        return Ok(HarnessField {
            value: Some(value),
            from_env: false,
        });
    }
    for key in CIRCUIT_FILE {
        match config.get_param::<Value>(key) {
            Ok(value) => {
                if let Some(flag) = json_bool(&value) {
                    return Ok(HarnessField {
                        value: Some(flag),
                        from_env: false,
                    });
                }
            }
            Err(ConfigError::NotFound(_)) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(HarnessField {
        value: None,
        from_env: false,
    })
}

fn read_judge(config: &Config) -> Result<JudgeSettingsView, AppConfigError> {
    let defaults = JudgeConfig::default();
    let Some(map) = load_object(config, "judge")? else {
        return Ok(judge_from_defaults(&defaults, false));
    };
    let provider = map
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(defaults.provider.as_str())
        .to_string();
    let endpoint = map
        .get("endpoint")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(defaults.endpoint.as_str())
        .to_string();
    let timeout_ms = map
        .get("timeout_ms")
        .or_else(|| map.get("timeoutMs"))
        .and_then(json_u64)
        .unwrap_or(defaults.timeout_ms);
    let fallback_on_error = map
        .get("fallback_on_error")
        .or_else(|| map.get("fallbackOnError"))
        .and_then(json_bool)
        .unwrap_or(defaults.fallback_on_error);
    let points = judge_points(&defaults, map.get("points"));
    Ok(JudgeSettingsView {
        configured: true,
        provider,
        endpoint,
        timeout_ms,
        fallback_on_error,
        points,
    })
}

fn judge_from_defaults(defaults: &JudgeConfig, configured: bool) -> JudgeSettingsView {
    JudgeSettingsView {
        configured,
        provider: defaults.provider.clone(),
        endpoint: defaults.endpoint.clone(),
        timeout_ms: defaults.timeout_ms,
        fallback_on_error: defaults.fallback_on_error,
        points: judge_points(defaults, None),
    }
}

fn judge_points(defaults: &JudgeConfig, raw: Option<&Value>) -> Vec<JudgePoint> {
    let mut explicit = BTreeMap::new();
    let points_present = matches!(raw, Some(Value::Object(_)));
    if let Some(Value::Object(map)) = raw {
        for (id, mode) in map {
            if let Some(mode) = mode.as_str() {
                explicit.insert(id.clone(), mode.to_ascii_lowercase());
            }
        }
    }
    let mut points = Vec::new();
    for id in KNOWN_JUDGE_POINTS {
        let mode = if let Some(mode) = explicit.remove(*id) {
            normalize_stored_judge_mode(&mode)
        } else if points_present {
            "off".to_string()
        } else {
            defaults
                .points
                .get(*id)
                .map(|mode| normalize_stored_judge_mode(mode))
                .unwrap_or_else(|| "off".to_string())
        };
        points.push(JudgePoint {
            id: (*id).to_string(),
            mode,
        });
    }
    for (id, mode) in explicit {
        points.push(JudgePoint {
            id,
            mode: normalize_stored_judge_mode(&mode),
        });
    }
    points
}

fn normalize_stored_judge_mode(mode: &str) -> String {
    match mode.trim().to_ascii_lowercase().as_str() {
        "active" => "active".to_string(),
        "shadow" => "shadow".to_string(),
        _ => "off".to_string(),
    }
}

fn load_harness_map(config: &Config) -> Result<Option<Map<String, Value>>, AppConfigError> {
    match load_object(config, "harness")? {
        Some(map) => Ok(Some(map)),
        None => Ok(None),
    }
}

fn load_object(config: &Config, key: &str) -> Result<Option<Map<String, Value>>, AppConfigError> {
    match config.get_param::<Value>(key) {
        Ok(Value::Object(map)) => Ok(Some(map)),
        Ok(Value::Null) => Ok(None),
        Ok(_) => Err(AppConfigError::invalid(format!(
            "{key} in config.yaml must be a mapping"
        ))),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn apply_count(map: &mut Map<String, Value>, from_env: bool, value: Option<usize>, keys: &[&str]) {
    if from_env {
        return;
    }
    for key in keys {
        map.remove(*key);
    }
    if let Some(value) = value {
        map.insert(keys[0].to_string(), Value::from(value));
    }
}

fn apply_flag(map: &mut Map<String, Value>, from_env: bool, value: Option<bool>, keys: &[&str]) {
    if from_env {
        return;
    }
    for key in keys {
        map.remove(*key);
    }
    if let Some(value) = value {
        map.insert(keys[0].to_string(), Value::Bool(value));
    }
}

fn delete_keys(config: &Config, keys: &[&str]) -> Result<(), AppConfigError> {
    for key in keys {
        config.delete(key)?;
    }
    Ok(())
}

fn optional_usize_param(config: &Config, key: &str) -> Result<Option<usize>, AppConfigError> {
    match config.get_param::<Value>(key) {
        Ok(value) => Ok(json_usize(&value).filter(|value| *value >= 1)),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn first_usize_in_map(map: &Map<String, Value>, keys: &[&str]) -> Option<usize> {
    keys.iter().find_map(|key| {
        map.get(*key)
            .and_then(json_usize)
            .filter(|value| *value >= 1)
    })
}

fn first_bool_in_map(map: &Map<String, Value>, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(json_bool))
}

fn json_usize(value: &Value) -> Option<usize> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
}

fn json_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn json_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(flag) => Some(*flag),
        Value::Number(number) => number.as_u64().map(|value| value != 0),
        Value::String(text) => parse_flag(text),
        _ => None,
    }
}

fn parse_flag(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

fn positive_env(keys: &[&str]) -> Option<usize> {
    for key in keys {
        if let Ok(raw) = std::env::var(key) {
            if let Ok(value) = raw.trim().parse::<usize>() {
                if value >= 1 {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn first_present_usize(keys: &[&str]) -> Option<usize> {
    for key in keys {
        if let Ok(raw) = std::env::var(key) {
            return raw.trim().parse::<usize>().ok();
        }
    }
    None
}

fn first_present_flag(keys: &[&str]) -> Option<bool> {
    for key in keys {
        if let Ok(raw) = std::env::var(key) {
            return parse_flag(&raw);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    struct Fixture {
        config: Config,
        path: std::path::PathBuf,
        _config_file: NamedTempFile,
        _secrets_file: NamedTempFile,
    }

    fn fixture_with(yaml: &str) -> Fixture {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        std::fs::write(config_file.path(), yaml).unwrap();
        let config =
            Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap();
        Fixture {
            path: config_file.path().to_path_buf(),
            config,
            _config_file: config_file,
            _secrets_file: secrets_file,
        }
    }

    fn quiet_env() -> env_lock::EnvGuard<'static> {
        env_lock::lock_env([
            ("OPENDUCK_PROVIDER", None::<&str>),
            ("GOOSE_PROVIDER", None::<&str>),
            ("OPENDUCK_MODEL", None::<&str>),
            ("GOOSE_MODEL", None::<&str>),
            ("OPENDUCK_MODE", None::<&str>),
            ("GOOSE_MODE", None::<&str>),
            ("OPENDUCK_MAX_TURNS", None::<&str>),
            ("HARNESS_MAX_TURNS", None::<&str>),
            ("GOOSE_MAX_TURNS", None::<&str>),
            ("OPENDUCK_STAGNATION_THRESHOLD", None::<&str>),
            ("HARNESS_STAGNATION_THRESHOLD", None::<&str>),
            ("GOOSE_STAGNATION_THRESHOLD", None::<&str>),
            ("OPENDUCK_EXPLORATION_BUDGET", None::<&str>),
            ("HARNESS_EXPLORATION_BUDGET", None::<&str>),
            ("GOOSE_EXPLORATION_BUDGET", None::<&str>),
            ("OPENDUCK_PERIODIC_REVIEW_INTERVAL", None::<&str>),
            ("HARNESS_PERIODIC_REVIEW_INTERVAL", None::<&str>),
            ("GOOSE_PERIODIC_REVIEW_INTERVAL", None::<&str>),
            ("OPENDUCK_CIRCUIT_BREAKER", None::<&str>),
            ("HARNESS_CIRCUIT_BREAKER", None::<&str>),
            ("GOOSE_CIRCUIT_BREAKER", None::<&str>),
            ("OPENDUCK_HARNESS", None::<&str>),
            ("GOOSE_HARNESS", None::<&str>),
            ("HARNESS", None::<&str>),
            ("OPENDUCK_JUDGE", None::<&str>),
            ("GOOSE_JUDGE", None::<&str>),
            ("JUDGE", None::<&str>),
            ("HUB_EXAMPLE_FLAG", None::<&str>),
            ("OPENDUCK_FAST_MODEL", None::<&str>),
            ("GOOSE_FAST_MODEL", None::<&str>),
            ("FAST_MODEL", None::<&str>),
            ("OPENDUCK_SECONDARY_PROVIDER", None::<&str>),
            ("GOOSE_SECONDARY_PROVIDER", None::<&str>),
            ("SECONDARY_PROVIDER", None::<&str>),
            ("OPENDUCK_SECONDARY_MODEL", None::<&str>),
            ("GOOSE_SECONDARY_MODEL", None::<&str>),
            ("SECONDARY_MODEL", None::<&str>),
        ])
    }

    fn saved_yaml(fixture: &Fixture) -> serde_yaml::Value {
        let text = std::fs::read_to_string(&fixture.path).unwrap();
        serde_yaml::from_str(&text).unwrap()
    }

    #[test]
    fn hides_secrets_and_extension_blocks() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
GOOSE_MODE: auto
OPENAI_API_KEY: sk-test
HUB_EXAMPLE_FLAG: "1"
extensions:
  developer:
    enabled: true
"#,
        );
        let document = read_document(&fixture.config).unwrap();
        assert_eq!(document.mode.value.as_deref(), Some("auto"));
        assert!(document
            .entries
            .iter()
            .all(|entry| entry.key != "OPENAI_API_KEY"));
        assert!(document
            .entries
            .iter()
            .all(|entry| entry.key != "extensions"));
        assert_eq!(
            document
                .entries
                .iter()
                .find(|entry| entry.key == "HUB_EXAMPLE_FLAG")
                .and_then(|entry| entry.value.as_str()),
            Some("1")
        );
    }

    #[test]
    fn writes_provider_model_and_mode() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        );
        upsert_entry(
            &fixture.config,
            "GOOSE_PROVIDER",
            Value::String("openai".to_string()),
        )
        .unwrap();
        upsert_entry(
            &fixture.config,
            "GOOSE_MODEL",
            Value::String("gpt-4o".to_string()),
        )
        .unwrap();
        let document = upsert_entry(
            &fixture.config,
            "GOOSE_MODE",
            Value::String("smart-approve".to_string()),
        )
        .unwrap();
        assert_eq!(document.provider.value.as_deref(), Some("openai"));
        assert_eq!(document.model.value.as_deref(), Some("gpt-4o"));
        assert_eq!(document.mode.value.as_deref(), Some("smart_approve"));
        assert!(upsert_entry(
            &fixture.config,
            "GOOSE_MODE",
            Value::String("nope".to_string()),
        )
        .is_err());
    }

    #[test]
    fn rejects_secret_and_nested_writes() {
        let _guard = quiet_env();
        let fixture = fixture_with("{}\n");
        assert!(upsert_entry(
            &fixture.config,
            "OPENAI_API_KEY",
            Value::String("sk-test".to_string()),
        )
        .is_err());
        assert!(upsert_entry(&fixture.config, "extensions", Value::Object(Map::new()),).is_err());
        assert!(upsert_entry(
            &fixture.config,
            "HUB_EXAMPLE_FLAG",
            Value::Array(Vec::new()),
        )
        .is_err());
    }

    #[test]
    fn harness_update_preserves_advisors_and_clears_top_level_turns() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
GOOSE_MAX_TURNS: 12
harness:
  stagnation_threshold: 4
  advisors:
    grok:
      https_proxy: http://127.0.0.1:2087
"#,
        );
        let document = update_harness(
            &fixture.config,
            HarnessSettingsUpdate {
                max_turns: Some(40),
                stagnation_threshold: Some(9),
                exploration_budget: None,
                periodic_review_interval: Some(10),
                circuit_breaker_enabled: Some(false),
            },
        )
        .unwrap();
        assert_eq!(document.harness.max_turns.value, Some(40));
        assert!(!document.harness.max_turns.from_env);
        assert_eq!(document.harness.stagnation_threshold.value, Some(9));
        assert_eq!(document.harness.circuit_breaker_enabled.value, Some(false));
        let yaml = saved_yaml(&fixture);
        assert!(yaml.get("GOOSE_MAX_TURNS").is_none());
        assert_eq!(yaml["harness"]["max_turns"].as_u64(), Some(40));
        assert_eq!(
            yaml["harness"]["advisors"]["grok"]["https_proxy"].as_str(),
            Some("http://127.0.0.1:2087")
        );
    }

    #[test]
    fn env_max_turns_is_reported_and_not_overwritten() {
        let _guard = env_lock::lock_env([
            ("OPENDUCK_PROVIDER", None::<&str>),
            ("GOOSE_PROVIDER", None::<&str>),
            ("OPENDUCK_MODEL", None::<&str>),
            ("GOOSE_MODEL", None::<&str>),
            ("OPENDUCK_MODE", None::<&str>),
            ("GOOSE_MODE", None::<&str>),
            ("OPENDUCK_MAX_TURNS", Some("77")),
            ("HARNESS_MAX_TURNS", None::<&str>),
            ("GOOSE_MAX_TURNS", None::<&str>),
            ("OPENDUCK_STAGNATION_THRESHOLD", None::<&str>),
            ("HARNESS_STAGNATION_THRESHOLD", None::<&str>),
            ("GOOSE_STAGNATION_THRESHOLD", None::<&str>),
            ("OPENDUCK_EXPLORATION_BUDGET", None::<&str>),
            ("HARNESS_EXPLORATION_BUDGET", None::<&str>),
            ("GOOSE_EXPLORATION_BUDGET", None::<&str>),
            ("OPENDUCK_PERIODIC_REVIEW_INTERVAL", None::<&str>),
            ("HARNESS_PERIODIC_REVIEW_INTERVAL", None::<&str>),
            ("GOOSE_PERIODIC_REVIEW_INTERVAL", None::<&str>),
            ("OPENDUCK_CIRCUIT_BREAKER", None::<&str>),
            ("HARNESS_CIRCUIT_BREAKER", None::<&str>),
            ("GOOSE_CIRCUIT_BREAKER", None::<&str>),
            ("OPENDUCK_HARNESS", None::<&str>),
            ("GOOSE_HARNESS", None::<&str>),
            ("HARNESS", None::<&str>),
            ("OPENDUCK_JUDGE", None::<&str>),
            ("GOOSE_JUDGE", None::<&str>),
            ("JUDGE", None::<&str>),
        ]);
        let fixture = fixture_with(
            r#"
harness:
  max_turns: 10
"#,
        );
        let document = read_document(&fixture.config).unwrap();
        assert_eq!(document.harness.max_turns.value, Some(77));
        assert!(document.harness.max_turns.from_env);
        update_harness(
            &fixture.config,
            HarnessSettingsUpdate {
                max_turns: Some(0),
                stagnation_threshold: Some(8),
                exploration_budget: None,
                periodic_review_interval: None,
                circuit_breaker_enabled: None,
            },
        )
        .unwrap();
        let yaml = saved_yaml(&fixture);
        assert_eq!(yaml["harness"]["max_turns"].as_u64(), Some(10));
        assert_eq!(yaml["harness"]["stagnation_threshold"].as_u64(), Some(8));
    }

    #[test]
    fn judge_round_trip_preserves_unknown_fields_and_can_be_removed() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
judge:
  provider: laya
  note: keep-me
  points:
    tool.risk: "off"
"#,
        );
        let document = read_document(&fixture.config).unwrap();
        assert!(document.judge.configured);
        assert_eq!(
            document
                .judge
                .points
                .iter()
                .find(|point| point.id == "tool.risk")
                .map(|point| point.mode.as_str()),
            Some("off")
        );
        assert_eq!(
            document
                .judge
                .points
                .iter()
                .find(|point| point.id == "turn.drift")
                .map(|point| point.mode.as_str()),
            Some("off")
        );
        let updated = update_judge(
            &fixture.config,
            JudgeSettingsUpdate {
                provider: "laya".to_string(),
                endpoint: document.judge.endpoint,
                timeout_ms: document.judge.timeout_ms,
                fallback_on_error: false,
                points: document
                    .judge
                    .points
                    .iter()
                    .map(|point| JudgePointUpdate {
                        id: point.id.clone(),
                        mode: if point.id == "turn.drift" {
                            "active".to_string()
                        } else {
                            point.mode.clone()
                        },
                    })
                    .collect(),
            },
        )
        .unwrap();
        assert_eq!(
            updated
                .judge
                .points
                .iter()
                .find(|point| point.id == "turn.drift")
                .map(|point| point.mode.as_str()),
            Some("active")
        );
        assert!(!updated.judge.fallback_on_error);
        let yaml = saved_yaml(&fixture);
        assert_eq!(yaml["judge"]["note"].as_str(), Some("keep-me"));
        let cleared = remove_judge(&fixture.config).unwrap();
        assert!(!cleared.judge.configured);
        assert!(saved_yaml(&fixture).get("judge").is_none());
    }

    #[tokio::test]
    async fn test_provider_put_rejects_api_key_field() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        );
        let body = serde_json::json!({
            "majorProvider": "openai",
            "secondaryProvider": null,
            "apiKey": "secret-token",
            "providers": [
                {
                    "name": "openai",
                    "enabled": true,
                    "model": "gpt-4o",
                    "fastModel": null,
                    "secondaryModel": null
                }
            ]
        });
        let err = update_providers_config(&fixture.config, body)
            .await
            .unwrap_err();
        match err {
            AppConfigError::Invalid(msg) => {
                assert!(
                    msg.contains("API keys and tokens are not edited here")
                        || msg.contains("apiKey")
                );
            }
            _ => panic!("Expected Invalid error"),
        }

        let body_nested = serde_json::json!({
            "majorProvider": "openai",
            "secondaryProvider": null,
            "providers": [
                {
                    "name": "openai",
                    "enabled": true,
                    "model": "gpt-4o",
                    "fastModel": null,
                    "secondaryModel": null,
                    "api_key": "secret-val"
                }
            ]
        });
        let err2 = update_providers_config(&fixture.config, body_nested)
            .await
            .unwrap_err();
        match err2 {
            AppConfigError::Invalid(msg) => {
                assert!(
                    msg.contains("API keys and tokens are not edited here")
                        || msg.contains("api_key")
                );
            }
            _ => panic!("Expected Invalid error"),
        }
    }

    #[tokio::test]
    async fn test_provider_put_rejects_secondary_equal_major() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        );
        let body = serde_json::json!({
            "majorProvider": "openai",
            "secondaryProvider": "openai",
            "providers": [
                {
                    "name": "openai",
                    "enabled": true,
                    "model": "gpt-4o",
                    "fastModel": null,
                    "secondaryModel": null
                }
            ]
        });
        let err = update_providers_config(&fixture.config, body)
            .await
            .unwrap_err();
        match err {
            AppConfigError::Invalid(msg) => {
                assert!(msg.contains("Secondary provider cannot equal major provider"));
            }
            _ => panic!("Expected Invalid error"),
        }
    }

    #[tokio::test]
    async fn test_provider_put_blank_fast_model_stores_absent() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
    fast_model: gpt-4o-mini
"#,
        );
        let body = serde_json::json!({
            "majorProvider": "openai",
            "secondaryProvider": null,
            "providers": [
                {
                    "name": "openai",
                    "enabled": true,
                    "model": "gpt-4o",
                    "fastModel": "",
                    "secondaryModel": ""
                }
            ]
        });
        let resp = update_providers_config(&fixture.config, body)
            .await
            .unwrap();
        let openai_entry = resp.providers.iter().find(|p| p.name == "openai").unwrap();
        assert_eq!(openai_entry.fast_model, None);

        let yaml = saved_yaml(&fixture);
        assert!(yaml["providers"]["openai"].get("fast_model").is_none());
    }

    #[tokio::test]
    async fn test_provider_put_null_fast_model_stores_absent() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
    fast_model: gpt-4o-mini
"#,
        );
        let body = serde_json::json!({
            "majorProvider": "openai",
            "secondaryProvider": null,
            "providers": [
                {
                    "name": "openai",
                    "enabled": true,
                    "model": "gpt-4o",
                    "fastModel": null,
                    "secondaryModel": null
                }
            ]
        });
        let resp = update_providers_config(&fixture.config, body)
            .await
            .unwrap();
        let openai_entry = resp.providers.iter().find(|p| p.name == "openai").unwrap();
        assert_eq!(openai_entry.fast_model, None);

        let yaml = saved_yaml(&fixture);
        assert!(yaml["providers"]["openai"].get("fast_model").is_none());
    }

    #[tokio::test]
    async fn test_provider_put_does_not_touch_unrelated_scalar() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
GOOSE_MODE: auto
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        );
        let body = serde_json::json!({
            "majorProvider": "openai",
            "secondaryProvider": null,
            "providers": [
                {
                    "name": "openai",
                    "enabled": true,
                    "model": "gpt-4o",
                    "fastModel": null,
                    "secondaryModel": null
                }
            ]
        });
        update_providers_config(&fixture.config, body)
            .await
            .unwrap();
        let yaml = saved_yaml(&fixture);
        assert_eq!(yaml["GOOSE_MODE"].as_str(), Some("auto"));
    }

    #[tokio::test]
    async fn test_provider_put_null_major_clears_active_provider() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        );
        let body = serde_json::json!({
            "majorProvider": null,
            "secondaryProvider": null,
            "providers": [
                {
                    "name": "openai",
                    "enabled": true,
                    "model": "gpt-4o",
                    "fastModel": null,
                    "secondaryModel": null
                }
            ]
        });
        let resp = update_providers_config(&fixture.config, body)
            .await
            .unwrap();
        assert_eq!(resp.major_provider.value, None);
        assert_eq!(resp.major_provider.file_value, None);

        let yaml = saved_yaml(&fixture);
        assert!(yaml.get("active_provider").is_none());
    }

    #[tokio::test]
    async fn test_provider_put_env_lock_compares_to_file_not_env() {
        let _guard = env_lock::lock_env([
            ("OPENDUCK_FAST_MODEL", Some("gpt-4o-mini")),
            ("GOOSE_FAST_MODEL", None::<&str>),
            ("FAST_MODEL", None::<&str>),
        ]);
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        );
        // File has fast_model absent.
        // Submitting null or "" matches file (absent) and succeeds even though env is "gpt-4o-mini".
        let body_null = serde_json::json!({
            "majorProvider": "openai",
            "secondaryProvider": null,
            "providers": [
                {
                    "name": "openai",
                    "enabled": true,
                    "model": "gpt-4o",
                    "fastModel": null,
                    "secondaryModel": null
                }
            ]
        });
        let resp = update_providers_config(&fixture.config, body_null)
            .await
            .unwrap();
        assert!(resp.fast_model_from_env);

        let body_empty = serde_json::json!({
            "majorProvider": "openai",
            "secondaryProvider": null,
            "providers": [
                {
                    "name": "openai",
                    "enabled": true,
                    "model": "gpt-4o",
                    "fastModel": "",
                    "secondaryModel": ""
                }
            ]
        });
        update_providers_config(&fixture.config, body_empty)
            .await
            .unwrap();

        // Submitting "gpt-4o-mini" (which matches the env, but differs from the file's absent value) is 400!
        let body_diff = serde_json::json!({
            "majorProvider": "openai",
            "secondaryProvider": null,
            "providers": [
                {
                    "name": "openai",
                    "enabled": true,
                    "model": "gpt-4o",
                    "fastModel": "gpt-4o-mini",
                    "secondaryModel": null
                }
            ]
        });
        let err = update_providers_config(&fixture.config, body_diff)
            .await
            .unwrap_err();
        match err {
            AppConfigError::Invalid(msg) => {
                assert!(msg.contains("Fast model on openai is locked by environment"));
            }
            _ => panic!("Expected Invalid error"),
        }
    }

    #[tokio::test]
    async fn test_provider_put_missing_fast_model_is_400() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        );
        // Omit the fastModel key entirely
        let body = serde_json::json!({
            "majorProvider": "openai",
            "secondaryProvider": null,
            "providers": [
                {
                    "name": "openai",
                    "enabled": true,
                    "model": "gpt-4o",
                    "secondaryModel": null
                }
            ]
        });
        let err = update_providers_config(&fixture.config, body)
            .await
            .unwrap_err();
        match err {
            AppConfigError::Invalid(msg) => {
                assert!(
                    msg.contains("fastModel is required")
                        || msg.contains("missing field `fastModel`")
                );
            }
            _ => panic!("Expected Invalid error"),
        }
    }

    #[tokio::test]
    async fn test_get_providers_hides_secrets() {
        let _guard = quiet_env();
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        std::fs::write(
            config_file.path(),
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        )
        .unwrap();
        std::fs::write(
            secrets_file.path(),
            r#"
OPENAI_API_KEY: sk-super-secret-12345
"#,
        )
        .unwrap();
        let config =
            Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap();
        let resp = get_providers_config(&config).await.unwrap();
        let json_str = serde_json::to_string(&resp).unwrap();
        assert!(!json_str.contains("sk-super-secret-12345"));
        assert!(!json_str.contains("OPENAI_API_KEY"));
        assert!(!json_str.contains("apiKey"));
    }

    #[tokio::test]
    async fn test_get_providers_lists_live_registry_names_not_in_the_map() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        );
        let resp = get_providers_config(&fixture.config).await.unwrap();
        assert!(!resp.available_providers.contains(&"openai".to_string()));
        assert!(!resp.available_providers.is_empty());
    }

    #[test]
    fn test_scalar_provider_put_unchanged_until_editor() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
  anthropic:
    enabled: true
    configured: true
    model: claude-3-5-sonnet
"#,
        );
        let doc = upsert_entry(
            &fixture.config,
            "GOOSE_PROVIDER",
            serde_json::Value::String("anthropic".to_string()),
        )
        .unwrap();
        assert_eq!(doc.provider.value, Some("anthropic".to_string()));
    }

    #[test]
    fn test_scalar_put_rejects_goose_fast_model() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        );
        let err = upsert_entry(
            &fixture.config,
            "GOOSE_FAST_MODEL",
            serde_json::Value::String("gpt-4o-mini".to_string()),
        )
        .unwrap_err();
        match err {
            AppConfigError::Invalid(msg) => {
                assert!(msg.contains("edit this on the provider"));
            }
            _ => panic!("Expected Invalid error"),
        }
    }

    #[test]
    fn test_scalar_provider_put_rejects_unconfigured_name() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        );
        let err = upsert_entry(
            &fixture.config,
            "GOOSE_PROVIDER",
            serde_json::Value::String("unconfigured_provider".to_string()),
        )
        .unwrap_err();
        match err {
            AppConfigError::Invalid(msg) => {
                assert!(msg.contains("not configured"));
            }
            _ => panic!("Expected Invalid error"),
        }
    }

    #[test]
    fn test_proxies_get_and_update() {
        let _guard = quiet_env();
        let fixture = fixture_with(
            r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
        );
        let resp = get_proxies_config(&fixture.config).unwrap();
        assert!(resp.proxies.is_empty());

        let put_val = serde_json::json!({
            "proxies": [
                {
                    "id": "clash",
                    "url": "http://127.0.0.1:7890",
                    "description": "Clash proxy"
                },
                {
                    "id": "socks",
                    "url": "socks5://127.0.0.1:1088"
                }
            ]
        });
        let updated = update_proxies_config(&fixture.config, put_val).unwrap();
        assert_eq!(updated.proxies.len(), 2);
        assert_eq!(updated.proxies[0].id, "clash");
        assert_eq!(updated.proxies[0].url, "http://127.0.0.1:7890");
        assert_eq!(
            updated.proxies[0].description.as_deref(),
            Some("Clash proxy")
        );
        assert_eq!(updated.proxies[1].id, "socks");
        assert_eq!(updated.proxies[1].url, "socks5://127.0.0.1:1088");

        // Reject invalid URL protocol
        let invalid_url = serde_json::json!({
            "proxies": [
                {
                    "id": "bad",
                    "url": "ftp://127.0.0.1:21"
                }
            ]
        });
        assert!(update_proxies_config(&fixture.config, invalid_url).is_err());

        // Reject duplicate id
        let dup_id = serde_json::json!({
            "proxies": [
                {
                    "id": "clash",
                    "url": "http://127.0.0.1:7890"
                },
                {
                    "id": "clash",
                    "url": "http://127.0.0.1:7891"
                }
            ]
        });
        assert!(update_proxies_config(&fixture.config, dup_id).is_err());
    }
}
