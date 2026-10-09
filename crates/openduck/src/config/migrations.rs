use crate::agents::extension::PLATFORM_EXTENSIONS;
use crate::agents::ExtensionConfig;
use crate::config::extensions::ExtensionEntry;
use crate::config::providers::ProviderEntry;
use serde_yaml::Mapping;

const EXTENSIONS_CONFIG_KEY: &str = "extensions";
const PROVIDERS_CONFIG_KEY: &str = "providers";
const ACTIVE_PROVIDER_KEY: &str = "active_provider";
const SECONDARY_PROVIDER_KEY: &str = "secondary_provider";
const PROVIDER_ROLES_MIGRATED_KEY: &str = "provider_roles_migrated";

pub fn run_migrations(config: &mut Mapping) -> bool {
    let mut changed = false;
    changed |= migrate_platform_extensions(config);
    changed |= migrate_provider_config(config);
    changed
}

/// Run only non-destructive migrations suitable for in-memory read paths.
/// Provider migration is excluded because it removes flat keys that
/// `get_param()` callers may still look up directly.
pub fn run_read_migrations(config: &mut Mapping) {
    migrate_platform_extensions(config);
}

fn read_enabled_field(value: &serde_yaml::Value) -> Option<bool> {
    value
        .as_mapping()?
        .get(serde_yaml::Value::String("enabled".to_string()))?
        .as_bool()
}

fn migrate_platform_extensions(config: &mut Mapping) -> bool {
    let extensions_key = serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string());

    let extensions_value = config
        .get(&extensions_key)
        .cloned()
        .unwrap_or(serde_yaml::Value::Mapping(Mapping::new()));

    let mut extensions_map: Mapping = match extensions_value {
        serde_yaml::Value::Mapping(m) => m,
        _ => Mapping::new(),
    };

    let mut needs_save = false;

    for (name, def) in PLATFORM_EXTENSIONS.iter() {
        let ext_key = serde_yaml::Value::String(name.to_string());
        let existing = extensions_map.get(&ext_key);

        let needs_migration = match existing {
            None => true,
            Some(value) => match serde_yaml::from_value::<ExtensionEntry>(value.clone()) {
                Ok(entry) => match &entry.config {
                    ExtensionConfig::Platform {
                        description,
                        display_name,
                        ..
                    }
                    | ExtensionConfig::Builtin {
                        description,
                        display_name,
                        ..
                    } => {
                        description != def.description
                            || display_name.as_deref() != Some(def.display_name)
                    }
                    _ => true,
                },
                Err(_) => true,
            },
        };

        if needs_migration {
            let existing_entry =
                existing.and_then(|v| serde_yaml::from_value::<ExtensionEntry>(v.clone()).ok());

            let enabled = existing
                .and_then(read_enabled_field)
                .unwrap_or(def.default_enabled);

            let available_tools = existing_entry
                .as_ref()
                .and_then(|entry| match &entry.config {
                    ExtensionConfig::Builtin {
                        available_tools, ..
                    }
                    | ExtensionConfig::Platform {
                        available_tools, ..
                    } => Some(available_tools.clone()),
                    _ => None,
                })
                .unwrap_or_default();

            // If the extension already exists as type 'builtin', preserve that type
            let is_existing_builtin = existing_entry
                .as_ref()
                .is_some_and(|e| matches!(e.config, ExtensionConfig::Builtin { .. }));

            let config = if is_existing_builtin {
                ExtensionConfig::Builtin {
                    name: def.name.to_string(),
                    description: def.description.to_string(),
                    display_name: Some(def.display_name.to_string()),
                    timeout: None,
                    bundled: Some(true),
                    available_tools,
                }
            } else {
                ExtensionConfig::Platform {
                    name: def.name.to_string(),
                    description: def.description.to_string(),
                    display_name: Some(def.display_name.to_string()),
                    bundled: Some(true),
                    available_tools,
                }
            };

            let new_entry = ExtensionEntry { config, enabled };

            if let Ok(value) = serde_yaml::to_value(&new_entry) {
                extensions_map.insert(ext_key, value);
                needs_save = true;
            }
        }
    }

    if needs_save {
        config.insert(extensions_key, serde_yaml::Value::Mapping(extensions_map));
    }

    needs_save
}

/// Remove leftover legacy flat keys when `providers:` block already exists.
fn cleanup_legacy_provider_keys(config: &mut Mapping) -> bool {
    let configured_suffix = "_configured";
    let mut changed = false;

    let stale_keys: Vec<serde_yaml::Value> = config
        .keys()
        .filter(|k| {
            k.as_str()
                .map(|s| {
                    s == "GOOSE_PROVIDER" || s == "GOOSE_MODEL" || s.ends_with(configured_suffix)
                })
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    for key in stale_keys {
        config.shift_remove(&key);
        changed = true;
    }

    changed
}

/// Migrate flat provider keys to the structured `providers:` block.
///
/// Old layout (flat keys):
/// ```yaml
/// GOOSE_PROVIDER: claude-acp
/// GOOSE_MODEL: current
/// claude-acp_configured: true
/// lmstudio_configured: true
/// ```
///
/// New layout:
/// ```yaml
/// active_provider: claude-acp
/// providers:
///   claude-acp:
///     enabled: true
///     model: current
///     configured: true
///   lmstudio:
///     enabled: true
///     model: ""
///     configured: true
/// ```
///
fn get_legacy_mapping_str(cfg: &Mapping, k1: &str, k2: &str) -> Option<String> {
    cfg.get(k1)
        .or_else(|| cfg.get(k2))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Migrate flat provider keys to the structured `providers:` block and migrate roles.
fn migrate_provider_config(config: &mut Mapping) -> bool {
    let providers_key = serde_yaml::Value::String(PROVIDERS_CONFIG_KEY.to_string());
    let mut changed = false;

    if config.contains_key(&providers_key) {
        let ap_key = serde_yaml::Value::String(ACTIVE_PROVIDER_KEY.to_string());
        if !config.contains_key(&ap_key) {
            if let Some(legacy) = config
                .get("GOOSE_PROVIDER")
                .or_else(|| config.get("OPENDUCK_PROVIDER"))
                .and_then(|v| v.as_str())
            {
                let trimmed = legacy.trim();
                if !trimmed.is_empty() {
                    config.insert(ap_key, serde_yaml::Value::String(trimmed.to_string()));
                    changed = true;
                }
            }
        }
    } else {
        // Read the old flat keys, if present.
        let active_provider = config
            .get("GOOSE_PROVIDER")
            .or_else(|| config.get("OPENDUCK_PROVIDER"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let active_model = config
            .get("GOOSE_MODEL")
            .or_else(|| config.get("OPENDUCK_MODEL"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        // Scan for `*_configured` keys to discover all previously-used providers.
        let configured_suffix = "_configured";
        let mut discovered_providers: Vec<String> = config
            .keys()
            .filter_map(|k| {
                k.as_str().and_then(|s| {
                    if s.ends_with(configured_suffix) {
                        Some(s.trim_end_matches(configured_suffix).to_string())
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Ensure the active provider is in the list even if no `*_configured`
        // marker exists for it yet.
        if let Some(ref ap) = active_provider {
            if !discovered_providers.contains(ap) {
                discovered_providers.push(ap.clone());
            }
        }

        let has_legacy_roles = get_legacy_mapping_str(
            config,
            "GOOSE_SECONDARY_PROVIDER",
            "OPENDUCK_SECONDARY_PROVIDER",
        )
        .is_some()
            || get_legacy_mapping_str(config, "GOOSE_FAST_MODEL", "OPENDUCK_FAST_MODEL").is_some();

        // If there is nothing to migrate, bail out.
        if discovered_providers.is_empty() && active_provider.is_none() && !has_legacy_roles {
            return false;
        }

        // Build the providers mapping.
        let mut providers_map = Mapping::new();
        for name in &discovered_providers {
            let is_active = active_provider.as_deref() == Some(name.as_str());
            let model = if is_active {
                active_model.clone()
            } else {
                String::new()
            };
            let entry = ProviderEntry {
                enabled: true,
                model,
                configured: true,
                ..Default::default()
            };
            if let Ok(value) = serde_yaml::to_value(&entry) {
                providers_map.insert(serde_yaml::Value::String(name.clone()), value);
            }
        }

        config.insert(
            providers_key.clone(),
            serde_yaml::Value::Mapping(providers_map),
        );
        changed = true;

        // Write `active_provider` top-level key.
        if let Some(ref ap) = active_provider {
            config.insert(
                serde_yaml::Value::String(ACTIVE_PROVIDER_KEY.to_string()),
                serde_yaml::Value::String(ap.clone()),
            );
        }

        // Remove old flat keys.
        config.shift_remove("GOOSE_PROVIDER");
        config.shift_remove("OPENDUCK_PROVIDER");
        config.shift_remove("GOOSE_MODEL");
        config.shift_remove("OPENDUCK_MODEL");
        for name in &discovered_providers {
            let marker_key = format!("{}{}", name, configured_suffix);
            config.shift_remove(marker_key.as_str());
        }
    }

    // Role migration: runs only when provider_roles_migrated is not already true.
    let roles_migrated_key = serde_yaml::Value::String(PROVIDER_ROLES_MIGRATED_KEY.to_string());
    let already_migrated = config
        .get(PROVIDER_ROLES_MIGRATED_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !already_migrated {
        let active_provider = config
            .get(ACTIVE_PROVIDER_KEY)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let sec_key = serde_yaml::Value::String(SECONDARY_PROVIDER_KEY.to_string());
        let mut secondary_written = false;
        let mut secondary_name_opt = None;

        // Step 1: If secondary_provider is absent, read non-blank legacy secondary provider.
        if !config.contains_key(&sec_key) {
            if let Some(sec_val) = get_legacy_mapping_str(
                config,
                "GOOSE_SECONDARY_PROVIDER",
                "OPENDUCK_SECONDARY_PROVIDER",
            ) {
                if active_provider.as_deref() == Some(&sec_val) {
                    tracing::warn!(
                        provider = ?sec_val,
                        "Legacy secondary provider equals active provider; skipping secondary provider migration"
                    );
                } else {
                    config.insert(sec_key, serde_yaml::Value::String(sec_val.clone()));
                    secondary_written = true;
                    secondary_name_opt = Some(sec_val);
                }
            }
        }

        // Step 2 & 3: Ensure copied name has an entry, and migrate secondary model if entry model is empty.
        if secondary_written {
            if let Some(sec_name) = secondary_name_opt {
                let sec_model_opt = get_legacy_mapping_str(
                    config,
                    "GOOSE_SECONDARY_MODEL",
                    "OPENDUCK_SECONDARY_MODEL",
                );
                if let Some(serde_yaml::Value::Mapping(ref mut p_map)) =
                    config.get_mut(&providers_key)
                {
                    let entry_key = serde_yaml::Value::String(sec_name.clone());
                    let mut entry: ProviderEntry = p_map
                        .get(&entry_key)
                        .and_then(|v| serde_yaml::from_value(v.clone()).ok())
                        .unwrap_or_else(|| ProviderEntry {
                            enabled: true,
                            configured: true,
                            model: String::new(),
                            ..Default::default()
                        });

                    if let Some(sec_model) = sec_model_opt {
                        if entry.model.trim().is_empty() {
                            entry.model = sec_model;
                        } else if entry.model.trim() != sec_model {
                            tracing::warn!(
                                provider = ?sec_name,
                                entry_model = ?entry.model,
                                legacy_secondary_model = ?sec_model,
                                "Secondary provider model conflict; keeping entry model"
                            );
                        }
                    }

                    entry.normalize();
                    if let Ok(val) = serde_yaml::to_value(&entry) {
                        p_map.insert(entry_key, val);
                    }
                }
            }
        }

        // Step 4: If active provider entry exists and fast_model is absent/blank, copy legacy fast model.
        if let Some(ref ap_name) = active_provider {
            let fast_model_opt =
                get_legacy_mapping_str(config, "GOOSE_FAST_MODEL", "OPENDUCK_FAST_MODEL");
            if let Some(fast_model_val) = fast_model_opt {
                if let Some(serde_yaml::Value::Mapping(ref mut p_map)) =
                    config.get_mut(&providers_key)
                {
                    let entry_key = serde_yaml::Value::String(ap_name.clone());
                    if let Some(raw_entry) = p_map.get(&entry_key) {
                        if let Ok(mut entry) =
                            serde_yaml::from_value::<ProviderEntry>(raw_entry.clone())
                        {
                            let needs_fast_model = entry
                                .fast_model
                                .as_ref()
                                .map(|s| s.trim().is_empty())
                                .unwrap_or(true);
                            if needs_fast_model {
                                entry.fast_model = Some(fast_model_val);
                                entry.normalize();
                                if let Ok(val) = serde_yaml::to_value(&entry) {
                                    p_map.insert(entry_key, val);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Step 5: Set provider_roles_migrated to true.
        config.insert(roles_migrated_key, serde_yaml::Value::Bool(true));
        changed = true;
    }

    // Step 6: cleanup_legacy_provider_keys removes GOOSE_PROVIDER, GOOSE_MODEL, and *_configured only.
    changed |= cleanup_legacy_provider_keys(config);

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate_platform_extensions_empty_config() {
        let mut config = Mapping::new();
        let changed = run_migrations(&mut config);

        assert!(changed);
        let extensions_key = serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string());
        assert!(config.contains_key(&extensions_key));
    }

    #[test]
    fn test_migrate_platform_extensions_preserves_restrictions() {
        let mut config = Mapping::new();
        let mut extensions = Mapping::new();
        let todo_entry = ExtensionEntry {
            config: ExtensionConfig::Platform {
                name: "todo".to_string(),
                description: "old description".to_string(),
                display_name: Some("Old Name".to_string()),
                bundled: Some(true),
                available_tools: vec!["todo_read".to_string()],
            },
            enabled: false,
        };
        extensions.insert(
            serde_yaml::Value::String("todo".to_string()),
            serde_yaml::to_value(&todo_entry).unwrap(),
        );
        config.insert(
            serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string()),
            serde_yaml::Value::Mapping(extensions),
        );

        let changed = run_migrations(&mut config);
        assert!(changed);

        let extensions_key = serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string());
        let extensions = config.get(&extensions_key).unwrap().as_mapping().unwrap();
        let todo_key = serde_yaml::Value::String("todo".to_string());
        let todo_value = extensions.get(&todo_key).unwrap();
        let todo_entry: ExtensionEntry = serde_yaml::from_value(todo_value.clone()).unwrap();

        assert!(!todo_entry.enabled);
        assert!(matches!(
            todo_entry.config,
            ExtensionConfig::Platform {
                available_tools,
                ..
            } if available_tools == ["todo_read"]
        ));
        assert!(!run_migrations(&mut config));
    }

    #[test]
    fn test_migrate_builtin_extensions_preserves_restrictions() {
        let def = PLATFORM_EXTENSIONS.get("todo").unwrap();
        let mut config = Mapping::new();
        let mut extensions = Mapping::new();
        let todo_entry = ExtensionEntry {
            config: ExtensionConfig::Builtin {
                name: def.name.to_string(),
                description: "old description".to_string(),
                display_name: Some("Old Name".to_string()),
                timeout: Some(30),
                bundled: Some(true),
                available_tools: vec!["todo_read".to_string()],
            },
            enabled: true,
        };
        extensions.insert(
            serde_yaml::Value::String("todo".to_string()),
            serde_yaml::to_value(&todo_entry).unwrap(),
        );
        config.insert(
            serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string()),
            serde_yaml::Value::Mapping(extensions),
        );

        assert!(run_migrations(&mut config));

        let extensions_key = serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string());
        let extensions = config.get(&extensions_key).unwrap().as_mapping().unwrap();
        let todo_entry: ExtensionEntry = serde_yaml::from_value(
            extensions
                .get(serde_yaml::Value::String("todo".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();

        assert!(todo_entry.enabled);
        assert!(matches!(
            todo_entry.config,
            ExtensionConfig::Builtin {
                available_tools,
                ..
            } if available_tools == ["todo_read"]
        ));
    }

    #[test]
    fn test_migrate_platform_extensions_idempotent() {
        let mut config = Mapping::new();
        run_migrations(&mut config);

        let changed = run_migrations(&mut config);
        assert!(!changed);
    }

    // -----------------------------------------------------------------------
    // Provider migration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_migrate_provider_config_basic() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("claude-acp".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("current".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("claude-acp_configured".to_string()),
            serde_yaml::Value::Bool(true),
        );

        let changed = migrate_provider_config(&mut config);
        assert!(changed);

        // active_provider should be set
        let active = config
            .get(serde_yaml::Value::String("active_provider".to_string()))
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(active, "claude-acp");

        // providers block should exist with the entry
        let providers = config
            .get(serde_yaml::Value::String("providers".to_string()))
            .unwrap()
            .as_mapping()
            .unwrap();
        let entry: ProviderEntry = serde_yaml::from_value(
            providers
                .get(serde_yaml::Value::String("claude-acp".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert!(entry.enabled);
        assert!(entry.configured);
        assert_eq!(entry.model, "current");

        // Old flat keys should be removed
        assert!(!config.contains_key(serde_yaml::Value::String("GOOSE_PROVIDER".to_string())));
        assert!(!config.contains_key(serde_yaml::Value::String("GOOSE_MODEL".to_string())));
        assert!(!config.contains_key(serde_yaml::Value::String(
            "claude-acp_configured".to_string()
        )));
    }

    #[test]
    fn test_migrate_provider_config_multiple_configured() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("claude-acp".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("current".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("claude-acp_configured".to_string()),
            serde_yaml::Value::Bool(true),
        );
        config.insert(
            serde_yaml::Value::String("lmstudio_configured".to_string()),
            serde_yaml::Value::Bool(true),
        );

        let changed = migrate_provider_config(&mut config);
        assert!(changed);

        let providers = config
            .get(serde_yaml::Value::String("providers".to_string()))
            .unwrap()
            .as_mapping()
            .unwrap();

        // Both providers should exist
        let claude: ProviderEntry = serde_yaml::from_value(
            providers
                .get(serde_yaml::Value::String("claude-acp".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert_eq!(claude.model, "current");
        assert!(claude.configured);

        let lmstudio: ProviderEntry = serde_yaml::from_value(
            providers
                .get(serde_yaml::Value::String("lmstudio".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();
        // lmstudio was not the active provider, so model should be empty
        assert_eq!(lmstudio.model, "");
        assert!(lmstudio.configured);

        // Old markers removed
        assert!(!config.contains_key(serde_yaml::Value::String(
            "claude-acp_configured".to_string()
        )));
        assert!(!config.contains_key(serde_yaml::Value::String("lmstudio_configured".to_string())));
    }

    #[test]
    fn test_migrate_provider_config_idempotent() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o".to_string()),
        );

        let changed_first = migrate_provider_config(&mut config);
        assert!(changed_first);

        let changed_second = migrate_provider_config(&mut config);
        assert!(!changed_second, "Second migration run should be a no-op");
    }

    #[test]
    fn test_migrate_provider_config_empty_config() {
        let mut config = Mapping::new();

        let changed = migrate_provider_config(&mut config);
        assert!(!changed, "Empty config should not trigger migration");
    }

    #[test]
    fn test_migrate_provider_config_no_model() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("anthropic".to_string()),
        );
        // No GOOSE_MODEL key

        let changed = migrate_provider_config(&mut config);
        assert!(changed);

        let providers = config
            .get(serde_yaml::Value::String("providers".to_string()))
            .unwrap()
            .as_mapping()
            .unwrap();
        let entry: ProviderEntry = serde_yaml::from_value(
            providers
                .get(serde_yaml::Value::String("anthropic".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert_eq!(entry.model, "");
    }

    #[test]
    fn test_cleanup_legacy_keys_when_providers_exists() {
        let mut config = Mapping::new();
        // Simulate state: providers block exists but stale flat keys remain
        let mut providers_map = Mapping::new();
        if let Ok(value) = serde_yaml::to_value(&ProviderEntry {
            enabled: true,
            model: "current".to_string(),
            configured: true,
            ..Default::default()
        }) {
            providers_map.insert(serde_yaml::Value::String("claude-acp".to_string()), value);
        }
        config.insert(
            serde_yaml::Value::String("providers".to_string()),
            serde_yaml::Value::Mapping(providers_map),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("lmstudio".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("some-model".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("claude-acp_configured".to_string()),
            serde_yaml::Value::Bool(true),
        );

        let changed = migrate_provider_config(&mut config);
        assert!(changed);

        // Legacy keys should be gone
        assert!(!config.contains_key(serde_yaml::Value::String("GOOSE_PROVIDER".to_string())));
        assert!(!config.contains_key(serde_yaml::Value::String("GOOSE_MODEL".to_string())));
        assert!(!config.contains_key(serde_yaml::Value::String(
            "claude-acp_configured".to_string()
        )));

        // Providers block should be untouched
        assert!(config.contains_key(serde_yaml::Value::String("providers".to_string())));

        // active_provider should be backfilled from legacy GOOSE_PROVIDER
        assert_eq!(
            config
                .get(serde_yaml::Value::String("active_provider".to_string()))
                .and_then(|v| v.as_str()),
            Some("lmstudio")
        );
    }

    #[test]
    fn test_migrate_fast_model_onto_active_provider_when_empty() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_FAST_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o-mini".to_string()),
        );
        let changed = migrate_provider_config(&mut config);
        assert!(changed);
        let providers = config.get("providers").unwrap().as_mapping().unwrap();
        let openai: ProviderEntry =
            serde_yaml::from_value(providers.get("openai").unwrap().clone()).unwrap();
        assert_eq!(openai.fast_model, Some("gpt-4o-mini".to_string()));
        assert_eq!(
            config
                .get("provider_roles_migrated")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_migrate_fast_model_does_not_overwrite_existing() {
        let mut config = Mapping::new();
        let mut providers_map = Mapping::new();
        providers_map.insert(
            serde_yaml::Value::String("openai".to_string()),
            serde_yaml::to_value(&ProviderEntry {
                enabled: true,
                model: "gpt-4o".to_string(),
                configured: true,
                fast_model: Some("existing-fast".to_string()),
                ..Default::default()
            })
            .unwrap(),
        );
        config.insert(
            serde_yaml::Value::String("providers".to_string()),
            serde_yaml::Value::Mapping(providers_map),
        );
        config.insert(
            serde_yaml::Value::String("active_provider".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_FAST_MODEL".to_string()),
            serde_yaml::Value::String("legacy-fast".to_string()),
        );

        migrate_provider_config(&mut config);
        let providers = config.get("providers").unwrap().as_mapping().unwrap();
        let openai: ProviderEntry =
            serde_yaml::from_value(providers.get("openai").unwrap().clone()).unwrap();
        assert_eq!(openai.fast_model, Some("existing-fast".to_string()));
    }

    #[test]
    fn test_migrate_secondary_provider_key() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_PROVIDER".to_string()),
            serde_yaml::Value::String("anthropic".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_MODEL".to_string()),
            serde_yaml::Value::String("claude-sonnet".to_string()),
        );

        migrate_provider_config(&mut config);
        assert_eq!(
            config.get("secondary_provider").and_then(|v| v.as_str()),
            Some("anthropic")
        );
        let providers = config.get("providers").unwrap().as_mapping().unwrap();
        let anthropic: ProviderEntry =
            serde_yaml::from_value(providers.get("anthropic").unwrap().clone()).unwrap();
        assert_eq!(anthropic.model, "claude-sonnet");
        assert!(anthropic.enabled);
        assert!(anthropic.configured);
    }

    #[test]
    fn test_migrate_secondary_equal_to_active_is_not_copied() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o-mini".to_string()),
        );

        migrate_provider_config(&mut config);
        assert!(!config.contains_key("secondary_provider"));
        let providers = config.get("providers").unwrap().as_mapping().unwrap();
        let openai: ProviderEntry =
            serde_yaml::from_value(providers.get("openai").unwrap().clone()).unwrap();
        assert_eq!(openai.model, "gpt-4o");
        assert_eq!(
            config
                .get("provider_roles_migrated")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_migrate_secondary_model_fills_empty_entry_model() {
        let mut config = Mapping::new();
        let mut providers_map = Mapping::new();
        providers_map.insert(
            serde_yaml::Value::String("anthropic".to_string()),
            serde_yaml::to_value(&ProviderEntry {
                enabled: true,
                model: "".to_string(),
                configured: true,
                ..Default::default()
            })
            .unwrap(),
        );
        config.insert(
            serde_yaml::Value::String("providers".to_string()),
            serde_yaml::Value::Mapping(providers_map),
        );
        config.insert(
            serde_yaml::Value::String("active_provider".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_PROVIDER".to_string()),
            serde_yaml::Value::String("anthropic".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_MODEL".to_string()),
            serde_yaml::Value::String("claude-sonnet".to_string()),
        );

        migrate_provider_config(&mut config);
        let providers = config.get("providers").unwrap().as_mapping().unwrap();
        let anthropic: ProviderEntry =
            serde_yaml::from_value(providers.get("anthropic").unwrap().clone()).unwrap();
        assert_eq!(anthropic.model, "claude-sonnet");
    }

    #[test]
    fn test_migrate_secondary_model_keeps_entry_model_on_conflict() {
        let mut config = Mapping::new();
        let mut providers_map = Mapping::new();
        providers_map.insert(
            serde_yaml::Value::String("anthropic".to_string()),
            serde_yaml::to_value(&ProviderEntry {
                enabled: true,
                model: "claude-opus".to_string(),
                configured: true,
                ..Default::default()
            })
            .unwrap(),
        );
        config.insert(
            serde_yaml::Value::String("providers".to_string()),
            serde_yaml::Value::Mapping(providers_map),
        );
        config.insert(
            serde_yaml::Value::String("active_provider".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_PROVIDER".to_string()),
            serde_yaml::Value::String("anthropic".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_MODEL".to_string()),
            serde_yaml::Value::String("claude-sonnet".to_string()),
        );

        migrate_provider_config(&mut config);
        let providers = config.get("providers").unwrap().as_mapping().unwrap();
        let anthropic: ProviderEntry =
            serde_yaml::from_value(providers.get("anthropic").unwrap().clone()).unwrap();
        assert_eq!(anthropic.model, "claude-opus");
        assert_eq!(anthropic.secondary_model, None);
    }

    #[test]
    fn test_migrate_does_not_copy_secondary_model_key_into_entry_secondary_model() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_PROVIDER".to_string()),
            serde_yaml::Value::String("anthropic".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_MODEL".to_string()),
            serde_yaml::Value::String("claude-sonnet".to_string()),
        );

        migrate_provider_config(&mut config);
        let providers = config.get("providers").unwrap().as_mapping().unwrap();
        let anthropic: ProviderEntry =
            serde_yaml::from_value(providers.get("anthropic").unwrap().clone()).unwrap();
        assert_eq!(anthropic.secondary_model, None);
    }

    #[test]
    fn test_migrate_legacy_fast_and_secondary_keys_remain() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_FAST_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o-mini".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_PROVIDER".to_string()),
            serde_yaml::Value::String("anthropic".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_MODEL".to_string()),
            serde_yaml::Value::String("claude-sonnet".to_string()),
        );

        migrate_provider_config(&mut config);
        assert!(config.contains_key("GOOSE_FAST_MODEL"));
        assert!(config.contains_key("GOOSE_SECONDARY_PROVIDER"));
        assert!(config.contains_key("GOOSE_SECONDARY_MODEL"));
        assert!(!config.contains_key("GOOSE_PROVIDER"));
        assert!(!config.contains_key("GOOSE_MODEL"));
    }

    #[test]
    fn test_migrate_provider_model_roles_idempotent() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_FAST_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o-mini".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_PROVIDER".to_string()),
            serde_yaml::Value::String("anthropic".to_string()),
        );

        let changed1 = migrate_provider_config(&mut config);
        assert!(changed1);
        let changed2 = migrate_provider_config(&mut config);
        assert!(!changed2);
    }

    #[test]
    fn test_migrate_runs_when_providers_map_already_exists() {
        let mut config = Mapping::new();
        let mut providers_map = Mapping::new();
        providers_map.insert(
            serde_yaml::Value::String("openai".to_string()),
            serde_yaml::to_value(&ProviderEntry {
                enabled: true,
                model: "gpt-4o".to_string(),
                configured: true,
                ..Default::default()
            })
            .unwrap(),
        );
        config.insert(
            serde_yaml::Value::String("providers".to_string()),
            serde_yaml::Value::Mapping(providers_map),
        );
        config.insert(
            serde_yaml::Value::String("active_provider".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_FAST_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o-mini".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_PROVIDER".to_string()),
            serde_yaml::Value::String("anthropic".to_string()),
        );

        let changed = migrate_provider_config(&mut config);
        assert!(changed);
        let providers = config.get("providers").unwrap().as_mapping().unwrap();
        let openai: ProviderEntry =
            serde_yaml::from_value(providers.get("openai").unwrap().clone()).unwrap();
        assert_eq!(openai.fast_model, Some("gpt-4o-mini".to_string()));
        assert_eq!(
            config.get("secondary_provider").and_then(|v| v.as_str()),
            Some("anthropic")
        );
    }

    #[test]
    fn test_second_migrate_does_not_restore_cleared_fast_model() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_FAST_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o-mini".to_string()),
        );

        migrate_provider_config(&mut config);
        // Clear fast model on the entry
        {
            let providers_val = config.get_mut("providers").unwrap();
            let p_map = providers_val.as_mapping_mut().unwrap();
            let mut openai: ProviderEntry =
                serde_yaml::from_value(p_map.get("openai").unwrap().clone()).unwrap();
            openai.fast_model = None;
            p_map.insert(
                serde_yaml::Value::String("openai".to_string()),
                serde_yaml::to_value(&openai).unwrap(),
            );
        }

        // Run migration again; because provider_roles_migrated is true, it does not restore fast_model
        migrate_provider_config(&mut config);
        let providers = config.get("providers").unwrap().as_mapping().unwrap();
        let openai: ProviderEntry =
            serde_yaml::from_value(providers.get("openai").unwrap().clone()).unwrap();
        assert_eq!(openai.fast_model, None);
    }

    #[test]
    fn test_second_migrate_does_not_restore_cleared_secondary_provider() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_PROVIDER".to_string()),
            serde_yaml::Value::String("anthropic".to_string()),
        );

        migrate_provider_config(&mut config);
        // Clear secondary_provider
        config.shift_remove("secondary_provider");

        // Run migration again; because provider_roles_migrated is true, it does not recreate secondary_provider
        migrate_provider_config(&mut config);
        assert!(!config.contains_key("secondary_provider"));
    }

    #[test]
    fn test_second_migrate_does_not_restore_cleared_secondary_entry_model() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_PROVIDER".to_string()),
            serde_yaml::Value::String("anthropic".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_SECONDARY_MODEL".to_string()),
            serde_yaml::Value::String("claude-sonnet".to_string()),
        );

        migrate_provider_config(&mut config);
        // Clear anthropic model
        {
            let providers_val = config.get_mut("providers").unwrap();
            let p_map = providers_val.as_mapping_mut().unwrap();
            let mut anthropic: ProviderEntry =
                serde_yaml::from_value(p_map.get("anthropic").unwrap().clone()).unwrap();
            anthropic.model = String::new();
            p_map.insert(
                serde_yaml::Value::String("anthropic".to_string()),
                serde_yaml::to_value(&anthropic).unwrap(),
            );
        }

        // Run migration again; because provider_roles_migrated is true, it does not restore model
        migrate_provider_config(&mut config);
        let providers = config.get("providers").unwrap().as_mapping().unwrap();
        let anthropic: ProviderEntry =
            serde_yaml::from_value(providers.get("anthropic").unwrap().clone()).unwrap();
        assert_eq!(anthropic.model, "");
    }
}
