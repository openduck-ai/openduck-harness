use super::base::{Config, ConfigError};
use super::env as openduck_env;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Mapping;
use tracing::warn;

pub const PROVIDERS_CONFIG_KEY: &str = "providers";
pub const ACTIVE_PROVIDER_KEY: &str = "active_provider";
pub const SECONDARY_PROVIDER_KEY: &str = "secondary_provider";
pub const PROVIDER_ROLES_MIGRATED_KEY: &str = "provider_roles_migrated";

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Default)]
pub struct ProviderEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub configured: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
}

impl ProviderEntry {
    pub fn normalize(&mut self) {
        if let Some(fm) = &self.fast_model {
            let trimmed = fm.trim();
            if trimmed.is_empty() || trimmed == self.model.trim() {
                self.fast_model = None;
            } else if trimmed != fm {
                self.fast_model = Some(trimmed.to_string());
            }
        }
        if let Some(sm) = &self.secondary_model {
            let trimmed = sm.trim();
            if trimmed.is_empty() {
                self.secondary_model = None;
            } else if trimmed != sm {
                self.secondary_model = Some(trimmed.to_string());
            }
        }
        if let Some(proxy) = &self.proxy {
            let trimmed = proxy.trim();
            if trimmed.is_empty() {
                self.proxy = None;
            } else if trimmed != proxy {
                self.proxy = Some(trimmed.to_string());
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderModels {
    pub model: Option<String>,
    pub fast_model: Option<String>,
    pub secondary_model: Option<String>,
}

fn parse_providers_map(raw: Mapping) -> IndexMap<String, ProviderEntry> {
    let mut map = IndexMap::with_capacity(raw.len());
    for (k, v) in raw {
        match (k, serde_yaml::from_value::<ProviderEntry>(v)) {
            (serde_yaml::Value::String(key), Ok(entry)) => {
                map.insert(key, entry);
            }
            (k, v) => {
                warn!(
                    key = ?k,
                    value = ?v,
                    "Skipping malformed provider config entry"
                );
            }
        }
    }
    map
}

pub fn get_providers_map(config: &Config) -> IndexMap<String, ProviderEntry> {
    let raw: Mapping = config
        .get_param(PROVIDERS_CONFIG_KEY)
        .unwrap_or_else(|_| Default::default());
    parse_providers_map(raw)
}

pub fn get_provider_entry(config: &Config, name: &str) -> Option<ProviderEntry> {
    get_providers_map(config).get(name).cloned()
}

pub fn set_provider_entry(
    config: &Config,
    name: &str,
    entry: &ProviderEntry,
) -> Result<(), ConfigError> {
    let name = name.to_string();
    let mut entry = entry.clone();
    entry.normalize();
    config.update_param::<Mapping, _, _>(PROVIDERS_CONFIG_KEY, |raw| {
        let mut map = parse_providers_map(raw);
        map.insert(name, entry);
        map
    })
}

pub fn get_active_provider(config: &Config) -> Option<String> {
    if let Some(val) = openduck_env::get_var("PROVIDER") {
        return Some(val);
    }
    if let Ok(val) = config.get_param::<String>(ACTIVE_PROVIDER_KEY) {
        return Some(val);
    }
    config.get_param::<String>("GOOSE_PROVIDER").ok()
}

pub fn get_active_model(config: &Config) -> Option<String> {
    if let Some(val) = openduck_env::get_var("MODEL") {
        return Some(val);
    }
    if let Some(provider_name) = get_active_provider(config) {
        if let Some(entry) = get_provider_entry(config, &provider_name) {
            if !entry.model.is_empty() {
                return Some(entry.model);
            }
        }
    }
    config.get_param::<String>("GOOSE_MODEL").ok()
}

pub fn set_active_provider(config: &Config, name: &str, model: &str) -> Result<(), ConfigError> {
    let name_trimmed = name.trim();
    if name_trimmed.is_empty() {
        return Err(ConfigError::DeserializeError(
            "Provider name cannot be empty".to_string(),
        ));
    }
    let model_trimmed = model.trim();
    if model_trimmed.is_empty() {
        return Err(ConfigError::DeserializeError(
            "Active provider model cannot be empty".to_string(),
        ));
    }

    config.set_param(ACTIVE_PROVIDER_KEY, name_trimmed)?;

    let mut entry = get_provider_entry(config, name_trimmed).unwrap_or_default();
    entry.enabled = true;
    entry.configured = true;
    entry.model = model_trimmed.to_string();
    entry.normalize();

    set_provider_entry(config, name_trimmed, &entry)
}

pub fn clear_active_provider(config: &Config) -> Result<(), ConfigError> {
    for key in [ACTIVE_PROVIDER_KEY, "GOOSE_PROVIDER", "GOOSE_MODEL"] {
        match config.delete(key) {
            Ok(()) | Err(ConfigError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

pub fn clear_provider_model(config: &Config, name: &str) -> Result<(), ConfigError> {
    let name_trimmed = name.trim();
    if let Some(mut entry) = get_provider_entry(config, name_trimmed) {
        entry.model = String::new();
        entry.normalize();
        set_provider_entry(config, name_trimmed, &entry)?;
    }
    Ok(())
}

pub fn get_provider_fast_model_name(config: &Config, provider_name: &str) -> Option<String> {
    get_provider_entry(config, provider_name.trim()).and_then(|entry| {
        entry.fast_model.and_then(|m| {
            let trimmed = m.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
    })
}

pub fn get_provider_secondary_model(config: &Config, provider_name: &str) -> Option<String> {
    get_provider_entry(config, provider_name.trim()).and_then(|entry| {
        entry.secondary_model.and_then(|m| {
            let trimmed = m.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
    })
}

pub fn provider_roles_migrated(config: &Config) -> bool {
    config
        .file_bool(PROVIDER_ROLES_MIGRATED_KEY)
        .unwrap_or(false)
}

pub fn get_secondary_provider(config: &Config) -> Option<String> {
    if let Some(val) = openduck_env::get_var("SECONDARY_PROVIDER") {
        let trimmed = val.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    if config.file_has_key(SECONDARY_PROVIDER_KEY) {
        return config.file_string(SECONDARY_PROVIDER_KEY);
    }
    if !provider_roles_migrated(config) {
        return config
            .file_string("GOOSE_SECONDARY_PROVIDER")
            .or_else(|| config.file_string("OPENDUCK_SECONDARY_PROVIDER"));
    }
    None
}

pub fn set_secondary_provider(config: &Config, name: Option<&str>) -> Result<(), ConfigError> {
    match name.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(sec) => config.set_param(SECONDARY_PROVIDER_KEY, sec),
        None => match config.delete(SECONDARY_PROVIDER_KEY) {
            Ok(()) | Err(ConfigError::NotFound(_)) => Ok(()),
            Err(e) => Err(e),
        },
    }
}

pub fn get_unmigrated_legacy_fast_model(config: &Config, provider_name: &str) -> Option<String> {
    if provider_roles_migrated(config) {
        return None;
    }
    if get_active_provider(config).as_deref() != Some(provider_name.trim()) {
        return None;
    }
    config
        .file_string("GOOSE_FAST_MODEL")
        .or_else(|| config.file_string("OPENDUCK_FAST_MODEL"))
}

pub fn resolve_provider_fast_model(config: &Config, provider_name: &str) -> Option<String> {
    if let Some(val) = openduck_env::get_var("FAST_MODEL") {
        let trimmed = val.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    if let Some(entry_fast) = get_provider_fast_model_name(config, provider_name) {
        return Some(entry_fast);
    }
    get_unmigrated_legacy_fast_model(config, provider_name)
}

pub fn merge_acp_provider_entry(
    config: &Config,
    provider_id: &str,
    default_model: &str,
) -> Result<(), ConfigError> {
    let provider_id = provider_id.trim();
    let mut entry = get_provider_entry(config, provider_id).unwrap_or_default();
    entry.enabled = true;
    entry.configured = true;
    if entry.model.trim().is_empty() {
        entry.model = default_model.to_string();
    }
    entry.normalize();
    set_provider_entry(config, provider_id, &entry)
}

pub fn upsert_provider_models(
    config: &Config,
    name: &str,
    models: ProviderModels,
) -> Result<(), ConfigError> {
    let name_trimmed = name.trim();
    if name_trimmed.is_empty() {
        return Err(ConfigError::DeserializeError(
            "Provider name cannot be empty".to_string(),
        ));
    }
    let mut entry = get_provider_entry(config, name_trimmed).unwrap_or_default();

    if let Some(m) = models.model {
        entry.model = m.trim().to_string();
    }
    if let Some(fm) = models.fast_model {
        let trimmed = fm.trim();
        if trimmed.is_empty() || trimmed == entry.model {
            entry.fast_model = None;
        } else {
            entry.fast_model = Some(trimmed.to_string());
        }
    }
    if let Some(sm) = models.secondary_model {
        let trimmed = sm.trim();
        if trimmed.is_empty() {
            entry.secondary_model = None;
        } else {
            entry.secondary_model = Some(trimmed.to_string());
        }
    }
    entry.normalize();
    set_provider_entry(config, name_trimmed, &entry)
}

pub fn get_provider_proxy(config: &Config, name: &str) -> Option<String> {
    let name_trimmed = name.trim();
    let upper = name_trimmed.to_ascii_uppercase().replace('-', "_");

    let resolve_proxy_value = |raw: &str| -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(proxy_entry) = crate::config::proxies::get_proxy_entry(config, trimmed) {
            if !proxy_entry.url.trim().is_empty() {
                return Some(proxy_entry.url.trim().to_string());
            }
        }
        Some(trimmed.to_string())
    };

    if let Some(val) = openduck_env::get_var(&format!("OPENDUCK_PROVIDER_{upper}_PROXY")) {
        if let Some(resolved) = resolve_proxy_value(&val) {
            return Some(resolved);
        }
    }
    if let Some(val) = openduck_env::get_var(&format!("GOOSE_PROVIDER_{upper}_PROXY")) {
        if let Some(resolved) = resolve_proxy_value(&val) {
            return Some(resolved);
        }
    }
    if let Some(val) = openduck_env::get_var(&format!("{upper}_PROXY")) {
        if let Some(resolved) = resolve_proxy_value(&val) {
            return Some(resolved);
        }
    }
    if let Ok(val) = config.get_param::<String>(&format!("{upper}_PROXY")) {
        if let Some(resolved) = resolve_proxy_value(&val) {
            return Some(resolved);
        }
    }
    get_provider_entry(config, name_trimmed)
        .and_then(|entry| entry.proxy.and_then(|p| resolve_proxy_value(&p)))
}

pub fn set_provider_proxy(
    config: &Config,
    name: &str,
    proxy: Option<String>,
) -> Result<(), ConfigError> {
    let name_trimmed = name.trim();
    if let Some(mut entry) = get_provider_entry(config, name_trimmed) {
        entry.proxy = proxy;
        entry.normalize();
        set_provider_entry(config, name_trimmed, &entry)?;
    } else if let Some(proxy) = proxy {
        let mut entry = ProviderEntry {
            proxy: Some(proxy),
            ..Default::default()
        };
        entry.normalize();
        set_provider_entry(config, name_trimmed, &entry)?;
    }
    Ok(())
}

pub fn validate_selection(
    config: &Config,
    major: Option<&str>,
    secondary: Option<&str>,
) -> Result<(), ConfigError> {
    let major_clean = major.map(|s| s.trim()).filter(|s| !s.is_empty());
    let sec_clean = secondary.map(|s| s.trim()).filter(|s| !s.is_empty());

    if let (Some(m), Some(s)) = (major_clean, sec_clean) {
        if m == s {
            return Err(ConfigError::DeserializeError(
                "Secondary provider cannot be the same as major provider".to_string(),
            ));
        }
    }

    if let Some(m) = major_clean {
        let entry = get_provider_entry(config, m).ok_or_else(|| {
            ConfigError::NotFound(format!(
                "Provider '{m}' not found in providers configuration"
            ))
        })?;
        if !entry.enabled {
            return Err(ConfigError::DeserializeError(format!(
                "Major provider '{m}' is not enabled"
            )));
        }
        if !entry.configured {
            return Err(ConfigError::DeserializeError(format!(
                "Major provider '{m}' is not configured"
            )));
        }
        if entry.model.trim().is_empty() {
            return Err(ConfigError::DeserializeError(format!(
                "Major provider '{m}' has an empty model"
            )));
        }
    }

    if let Some(s) = sec_clean {
        let entry = get_provider_entry(config, s).ok_or_else(|| {
            ConfigError::NotFound(format!(
                "Provider '{s}' not found in providers configuration"
            ))
        })?;
        if !entry.enabled {
            return Err(ConfigError::DeserializeError(format!(
                "Secondary provider '{s}' is not enabled"
            )));
        }
        if !entry.configured {
            return Err(ConfigError::DeserializeError(format!(
                "Secondary provider '{s}' is not configured"
            )));
        }
        if entry.model.trim().is_empty() {
            return Err(ConfigError::DeserializeError(format!(
                "Secondary provider '{s}' has an empty model"
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, val: &str) -> Self {
            let prev = std::env::var(key).ok();
            std::env::set_var(key, val);
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(val) = &self.prev {
                std::env::set_var(self.key, val);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn new_test_config() -> Config {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap()
    }

    #[test]
    fn test_set_and_get_provider_entry() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4o".to_string(),
            configured: true,
            ..Default::default()
        };
        set_provider_entry(&config, "openai", &entry).unwrap();

        let loaded = get_provider_entry(&config, "openai").unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.model, "gpt-4o");
        assert!(loaded.configured);
        assert_eq!(loaded.fast_model, None);
        assert_eq!(loaded.secondary_model, None);
    }

    #[test]
    fn test_set_active_provider_writes_structured_keys() {
        let config = new_test_config();
        set_active_provider(&config, "claude-acp", "current").unwrap();

        let active: String = config.get_param(ACTIVE_PROVIDER_KEY).unwrap();
        assert_eq!(active, "claude-acp");

        let entry = get_provider_entry(&config, "claude-acp").unwrap();
        assert!(entry.enabled);
        assert!(entry.configured);
        assert_eq!(entry.model, "current");
    }

    #[test]
    fn test_clear_active_provider_preserves_provider_entries() {
        let config = new_test_config();
        set_active_provider(&config, "openai", "gpt-4o").unwrap();

        clear_active_provider(&config).unwrap();

        assert!(get_active_provider(&config).is_none());
        let entry = get_provider_entry(&config, "openai").unwrap();
        assert_eq!(entry.model, "gpt-4o");
        assert!(entry.configured);
    }

    #[test]
    fn test_clear_active_provider_removes_legacy_keys() {
        let config = new_test_config();
        config.set_param("GOOSE_PROVIDER", "anthropic").unwrap();
        config.set_param("GOOSE_MODEL", "claude").unwrap();

        clear_active_provider(&config).unwrap();

        assert!(get_active_provider(&config).is_none());
        assert!(get_active_model(&config).is_none());
    }

    #[test]
    fn test_get_active_model_from_provider_entry() {
        let config = new_test_config();
        set_active_provider(&config, "openai", "gpt-4o").unwrap();

        let result = get_active_model(&config);
        assert_eq!(result, Some("gpt-4o".to_string()));
    }

    #[test]
    fn test_multiple_providers_preserved() {
        let config = new_test_config();
        set_active_provider(&config, "openai", "gpt-4o").unwrap();
        set_active_provider(&config, "anthropic", "claude-3-opus").unwrap();

        let openai = get_provider_entry(&config, "openai").unwrap();
        assert_eq!(openai.model, "gpt-4o");
        assert!(openai.configured);

        let anthropic = get_provider_entry(&config, "anthropic").unwrap();
        assert_eq!(anthropic.model, "claude-3-opus");
        assert!(anthropic.configured);

        assert_eq!(get_active_provider(&config), Some("anthropic".to_string()));
    }

    #[test]
    fn test_fast_model_unset_resolves_to_none_at_entry() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4o".to_string(),
            configured: true,
            fast_model: None,
            secondary_model: None,
            proxy: None,
        };
        set_provider_entry(&config, "openai", &entry).unwrap();
        assert_eq!(get_provider_fast_model_name(&config, "openai"), None);
    }

    #[test]
    fn test_fast_model_blank_is_stored_absent() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4o".to_string(),
            configured: true,
            fast_model: Some("   ".to_string()),
            secondary_model: None,
            proxy: None,
        };
        set_provider_entry(&config, "openai", &entry).unwrap();
        let loaded = get_provider_entry(&config, "openai").unwrap();
        assert_eq!(loaded.fast_model, None);
        assert_eq!(get_provider_fast_model_name(&config, "openai"), None);
    }

    #[test]
    fn test_secondary_model_unset_is_none() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4o".to_string(),
            configured: true,
            fast_model: None,
            secondary_model: None,
            proxy: None,
        };
        set_provider_entry(&config, "openai", &entry).unwrap();
        assert_eq!(get_provider_secondary_model(&config, "openai"), None);
    }

    #[test]
    fn test_secondary_model_blank_is_none() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4o".to_string(),
            configured: true,
            fast_model: None,
            secondary_model: Some("  ".to_string()),
            proxy: None,
        };
        set_provider_entry(&config, "openai", &entry).unwrap();
        let loaded = get_provider_entry(&config, "openai").unwrap();
        assert_eq!(loaded.secondary_model, None);
        assert_eq!(get_provider_secondary_model(&config, "openai"), None);
    }

    #[test]
    fn test_secondary_model_does_not_alias_model() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4o".to_string(),
            configured: true,
            fast_model: None,
            secondary_model: None,
            proxy: None,
        };
        set_provider_entry(&config, "openai", &entry).unwrap();
        assert_eq!(get_provider_secondary_model(&config, "openai"), None);
    }

    #[test]
    fn test_set_active_provider_preserves_fast_and_secondary_model() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4".to_string(),
            configured: true,
            fast_model: Some("gpt-4o-mini".to_string()),
            secondary_model: Some("gpt-4.1".to_string()),
            proxy: None,
        };
        set_provider_entry(&config, "openai", &entry).unwrap();

        set_active_provider(&config, "openai", "gpt-4o").unwrap();

        let loaded = get_provider_entry(&config, "openai").unwrap();
        assert_eq!(loaded.model, "gpt-4o");
        assert_eq!(loaded.fast_model, Some("gpt-4o-mini".to_string()));
        assert_eq!(loaded.secondary_model, Some("gpt-4.1".to_string()));
    }

    #[test]
    fn test_set_active_provider_rejects_empty_model() {
        let config = new_test_config();
        assert!(set_active_provider(&config, "openai", "").is_err());
        assert!(set_active_provider(&config, "openai", "   ").is_err());
    }

    #[test]
    fn test_clear_provider_model_does_not_drop_fast_model() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4".to_string(),
            configured: true,
            fast_model: Some("gpt-4o-mini".to_string()),
            secondary_model: Some("gpt-4.1".to_string()),
            proxy: None,
        };
        set_provider_entry(&config, "openai", &entry).unwrap();

        clear_provider_model(&config, "openai").unwrap();

        let loaded = get_provider_entry(&config, "openai").unwrap();
        assert_eq!(loaded.model, "");
        assert_eq!(loaded.fast_model, Some("gpt-4o-mini".to_string()));
        assert_eq!(loaded.secondary_model, Some("gpt-4.1".to_string()));
    }

    #[test]
    fn test_acp_provider_config_save_preserves_fast_and_secondary_model() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: false,
            model: "current-model".to_string(),
            configured: false,
            fast_model: Some("fast-m".to_string()),
            secondary_model: Some("sec-m".to_string()),
            proxy: None,
        };
        set_provider_entry(&config, "acp-provider", &entry).unwrap();

        merge_acp_provider_entry(&config, "acp-provider", "default-m").unwrap();

        let loaded = get_provider_entry(&config, "acp-provider").unwrap();
        assert!(loaded.enabled);
        assert!(loaded.configured);
        assert_eq!(loaded.model, "current-model");
        assert_eq!(loaded.fast_model, Some("fast-m".to_string()));
        assert_eq!(loaded.secondary_model, Some("sec-m".to_string()));
    }

    #[test]
    fn test_secondary_provider_unset_is_none_after_marker() {
        let config = new_test_config();
        config.set_param(PROVIDER_ROLES_MIGRATED_KEY, true).unwrap();
        config
            .set_param("GOOSE_SECONDARY_PROVIDER", "anthropic")
            .unwrap();

        assert_eq!(get_secondary_provider(&config), None);
    }

    #[test]
    fn test_clear_fast_model_sticks_while_goose_fast_model_remains() {
        let config = new_test_config();
        set_active_provider(&config, "openai", "gpt-4o").unwrap();
        config.set_param(PROVIDER_ROLES_MIGRATED_KEY, true).unwrap();
        config.set_param("GOOSE_FAST_MODEL", "legacy-fast").unwrap();

        // Clear fast model on entry
        let mut entry = get_provider_entry(&config, "openai").unwrap();
        entry.fast_model = None;
        set_provider_entry(&config, "openai", &entry).unwrap();

        assert_eq!(get_provider_fast_model_name(&config, "openai"), None);
        assert_eq!(resolve_provider_fast_model(&config, "openai"), None);
    }

    #[test]
    fn test_clear_secondary_provider_sticks_while_legacy_key_remains() {
        let config = new_test_config();
        config.set_param(PROVIDER_ROLES_MIGRATED_KEY, true).unwrap();
        config
            .set_param("GOOSE_SECONDARY_PROVIDER", "anthropic")
            .unwrap();

        set_secondary_provider(&config, None).unwrap();
        assert_eq!(get_secondary_provider(&config), None);
    }

    #[test]
    fn test_unmigrated_file_still_reads_legacy_fast_model_from_the_mapping() {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        let raw_yaml = r#"
active_provider: openai
GOOSE_FAST_MODEL: legacy-fast
providers:
  openai:
    enabled: true
    model: gpt-4o
    configured: true
"#;
        std::fs::write(config_file.path(), raw_yaml).unwrap();
        let config =
            Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap();

        assert_eq!(
            get_unmigrated_legacy_fast_model(&config, "openai"),
            Some("legacy-fast".to_string())
        );
        assert_eq!(
            resolve_provider_fast_model(&config, "openai"),
            Some("legacy-fast".to_string())
        );
        // Non-active provider does not read legacy key
        assert_eq!(get_unmigrated_legacy_fast_model(&config, "anthropic"), None);
    }

    #[test]
    fn test_secondary_equal_to_major_is_rejected() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4o".to_string(),
            configured: true,
            ..Default::default()
        };
        set_provider_entry(&config, "openai", &entry).unwrap();

        let err = validate_selection(&config, Some("openai"), Some("openai"));
        assert!(err.is_err());
    }

    #[test]
    fn test_unconfigured_provider_cannot_be_selected() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4o".to_string(),
            configured: false,
            ..Default::default()
        };
        set_provider_entry(&config, "openai", &entry).unwrap();

        assert!(validate_selection(&config, Some("openai"), None).is_err());
        assert!(validate_selection(&config, None, Some("openai")).is_err());
    }

    #[test]
    fn test_disabled_provider_cannot_be_selected() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: false,
            model: "gpt-4o".to_string(),
            configured: true,
            ..Default::default()
        };
        set_provider_entry(&config, "openai", &entry).unwrap();

        assert!(validate_selection(&config, Some("openai"), None).is_err());
        assert!(validate_selection(&config, None, Some("openai")).is_err());
    }

    #[test]
    fn test_selected_provider_missing_from_map_is_error() {
        let config = new_test_config();
        assert!(validate_selection(&config, Some("missing"), None).is_err());
        assert!(validate_selection(&config, None, Some("missing")).is_err());
    }

    #[test]
    fn test_selected_provider_empty_model_is_error() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "".to_string(),
            configured: true,
            ..Default::default()
        };
        set_provider_entry(&config, "openai", &entry).unwrap();

        assert!(validate_selection(&config, Some("openai"), None).is_err());
        assert!(validate_selection(&config, None, Some("openai")).is_err());
    }

    #[test]
    fn test_secondary_provider_env_overrides_file() {
        let config = new_test_config();
        set_secondary_provider(&config, Some("file-secondary")).unwrap();

        let _guard = EnvGuard::set("OPENDUCK_SECONDARY_PROVIDER", "env-secondary");
        assert_eq!(
            get_secondary_provider(&config),
            Some("env-secondary".to_string())
        );
    }

    #[test]
    fn test_fast_model_env_overrides_entry() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4o".to_string(),
            configured: true,
            fast_model: Some("entry-fast".to_string()),
            secondary_model: None,
            proxy: None,
        };
        set_provider_entry(&config, "openai", &entry).unwrap();

        let _guard = EnvGuard::set("OPENDUCK_FAST_MODEL", "env-fast");
        assert_eq!(
            resolve_provider_fast_model(&config, "openai"),
            Some("env-fast".to_string())
        );
    }

    #[test]
    fn test_provider_proxy_config_and_env() {
        let config = new_test_config();
        assert_eq!(get_provider_proxy(&config, "google"), None);

        // Entry proxy
        set_provider_proxy(&config, "google", Some("http://127.0.0.1:2087".to_string())).unwrap();
        assert_eq!(
            get_provider_proxy(&config, "google"),
            Some("http://127.0.0.1:2087".to_string())
        );

        // Direct <NAME>_PROXY overrides entry proxy
        let _guard1 = EnvGuard::set("GOOGLE_PROXY", "socks5://127.0.0.1:1088");
        assert_eq!(
            get_provider_proxy(&config, "google"),
            Some("socks5://127.0.0.1:1088".to_string())
        );

        // OPENDUCK_PROVIDER_<NAME>_PROXY takes precedence
        let _guard2 = EnvGuard::set("OPENDUCK_PROVIDER_GOOGLE_PROXY", "http://127.0.0.1:8888");
        assert_eq!(
            get_provider_proxy(&config, "google"),
            Some("http://127.0.0.1:8888".to_string())
        );
        drop(_guard2);
        drop(_guard1);

        // Pre-configured proxy resolution
        let proxy_entry = crate::config::proxies::ProxyEntry::new(
            "http://127.0.0.1:7890",
            Some("Clash".to_string()),
        );
        crate::config::proxies::set_proxy_entry(&config, "clash", &proxy_entry).unwrap();
        set_provider_proxy(&config, "google", Some("clash".to_string())).unwrap();
        assert_eq!(
            get_provider_proxy(&config, "google"),
            Some("http://127.0.0.1:7890".to_string())
        );
    }
}
