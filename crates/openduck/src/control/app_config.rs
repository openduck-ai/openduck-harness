//! Hub-facing view of the user OpenDuck config file.
//!
//! Secret keys and extension definitions stay out of this API. Environment
//! variables that override a setting are reported and left unchanged.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::config::{Config, ConfigError, env_lookup};
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

const KNOWN_JUDGE_POINTS: &[&str] = &[
    "turn.drift",
    "turn.completion",
    "context.forget",
    "tool.risk",
];

const STRUCTURAL_KEYS: &[&str] = &[
    "extensions",
    "providers",
    "harness",
    "judge",
    "advisors",
    "notifications",
    "secrets",
    "active_provider",
];

const DEDICATED_KEYS: &[&str] = &[
    "GOOSE_PROVIDER",
    "OPENDUCK_PROVIDER",
    "GOOSE_MODEL",
    "OPENDUCK_MODEL",
    "GOOSE_MODE",
    "OPENDUCK_MODE",
];

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
        config.set_param("harness", &Value::Object(map))?;
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
    config.set_param("judge", &Value::Object(map))?;
    read_document(config)
}

pub fn remove_judge(config: &Config) -> Result<AppConfigDocument, AppConfigError> {
    config.delete("judge")?;
    read_document(config)
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
    let model = crate::config::get_active_model(config).unwrap_or_default();
    crate::config::set_active_provider(config, &name, &model)?;
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
        crate::config::set_active_provider(config, &provider, "")?;
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
        assert!(
            document
                .entries
                .iter()
                .all(|entry| entry.key != "OPENAI_API_KEY")
        );
        assert!(
            document
                .entries
                .iter()
                .all(|entry| entry.key != "extensions")
        );
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
        let fixture = fixture_with("{}\n");
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
        assert!(
            upsert_entry(
                &fixture.config,
                "GOOSE_MODE",
                Value::String("nope".to_string()),
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_secret_and_nested_writes() {
        let _guard = quiet_env();
        let fixture = fixture_with("{}\n");
        assert!(
            upsert_entry(
                &fixture.config,
                "OPENAI_API_KEY",
                Value::String("sk-test".to_string()),
            )
            .is_err()
        );
        assert!(upsert_entry(&fixture.config, "extensions", Value::Object(Map::new()),).is_err());
        assert!(
            upsert_entry(
                &fixture.config,
                "HUB_EXAMPLE_FLAG",
                Value::Array(Vec::new()),
            )
            .is_err()
        );
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
}
