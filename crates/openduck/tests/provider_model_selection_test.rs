use openduck::config::providers::{
    clear_active_provider, clear_secondary_provider_model, get_active_model, get_active_provider,
    get_provider_entry, get_secondary_provider, provider_roles_migrated, resolve_role_model,
    resolve_role_model_for_provider, save_role_selection, set_active_provider, set_provider_entry,
    set_secondary_provider, set_secondary_provider_model, validate_selection, ProviderEntry, Role,
};
use openduck::config::{Config, ConfigError};
use tempfile::NamedTempFile;

fn env_set(key: &'static str, val: &'static str) -> impl std::any::Any {
    env_lock::lock_env([(key, Some(val))])
}

fn new_test_config() -> Config {
    let config_file = NamedTempFile::new().unwrap();
    let secrets_file = NamedTempFile::new().unwrap();
    Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap()
}

fn config_from_raw_yaml(yaml: &str) -> (Config, NamedTempFile, NamedTempFile) {
    let config_file = NamedTempFile::new().unwrap();
    let secrets_file = NamedTempFile::new().unwrap();
    std::fs::write(config_file.path(), yaml).unwrap();
    let config = Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap();
    (config, config_file, secrets_file)
}

fn enable_provider(config: &Config, name: &str, model: &str) {
    set_provider_entry(
        config,
        name,
        &ProviderEntry {
            enabled: true,
            model: model.to_string(),
            configured: true,
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn explicit_request_model_wins_over_everything() {
    let config = new_test_config();
    enable_provider(&config, "openai", "entry-default");
    set_active_provider(&config, "openai", "role-model").unwrap();
    let _env = env_set("MODEL", "env-model");

    let resolved =
        resolve_role_model_for_provider(&config, Role::Major, "openai", Some("explicit")).unwrap();
    assert_eq!(resolved.provider, "openai");
    assert_eq!(resolved.model, "explicit");
}

#[test]
fn env_model_overrides_role_and_entry_default() {
    let config = new_test_config();
    enable_provider(&config, "openai", "entry-default");
    set_active_provider(&config, "openai", "role-model").unwrap();
    let _env = env_set("MODEL", "env-model");

    let resolved = resolve_role_model_for_provider(&config, Role::Major, "openai", None).unwrap();
    assert_eq!(resolved.model, "env-model");
}

#[test]
fn role_key_wins_over_provider_default() {
    let config = new_test_config();
    enable_provider(&config, "openai", "entry-default");
    set_active_provider(&config, "openai", "role-model").unwrap();

    let resolved = resolve_role_model_for_provider(&config, Role::Major, "openai", None).unwrap();
    assert_eq!(resolved.model, "role-model");
}

#[test]
fn provider_default_used_when_no_role_model() {
    let config = new_test_config();
    enable_provider(&config, "openai", "entry-default");
    set_active_provider(&config, "openai", "").unwrap();

    let resolved = resolve_role_model_for_provider(&config, Role::Major, "openai", None).unwrap();
    assert_eq!(resolved.model, "entry-default");
}

#[test]
fn legacy_goose_model_used_when_unmigrated() {
    let (config, _config_file, _secrets_file) = config_from_raw_yaml(
        r#"
active_provider: openai
GOOSE_PROVIDER: openai
GOOSE_MODEL: legacy-model
providers:
  openai:
    enabled: true
    configured: true
    model: ""
"#,
    );
    assert!(!provider_roles_migrated(&config));

    let resolved = resolve_role_model_for_provider(&config, Role::Major, "openai", None).unwrap();
    assert_eq!(resolved.model, "legacy-model");
}

#[test]
fn missing_model_everywhere_is_an_error() {
    let config = new_test_config();
    enable_provider(&config, "openai", "");
    set_active_provider(&config, "openai", "").unwrap();

    let err = resolve_role_model_for_provider(&config, Role::Major, "openai", None).unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn provider_switch_does_not_inherit_old_role_model() {
    let config = new_test_config();
    enable_provider(&config, "openai", "entry-openai");
    enable_provider(&config, "anthropic", "entry-anthropic");
    set_active_provider(&config, "openai", "openai-role-model").unwrap();

    let resolved =
        resolve_role_model_for_provider(&config, Role::Major, "anthropic", None).unwrap();
    assert_eq!(resolved.model, "entry-anthropic");
}

#[test]
fn env_provider_switch_does_not_inherit_role_model() {
    let config = new_test_config();
    enable_provider(&config, "openai", "entry-openai");
    enable_provider(&config, "anthropic", "entry-anthropic");
    set_active_provider(&config, "openai", "openai-role-model").unwrap();
    let _env = env_set("PROVIDER", "anthropic");

    let resolved =
        resolve_role_model_for_provider(&config, Role::Major, "anthropic", None).unwrap();
    assert_eq!(resolved.model, "entry-anthropic");
}

#[test]
fn secondary_role_resolution_follows_precedence() {
    let config = new_test_config();
    enable_provider(&config, "anthropic", "entry-secondary");
    set_secondary_provider(&config, Some("anthropic")).unwrap();

    let resolved =
        resolve_role_model_for_provider(&config, Role::Secondary, "anthropic", None).unwrap();
    assert_eq!(resolved.model, "entry-secondary");

    set_secondary_provider_model(&config, "secondary-role-model").unwrap();
    let resolved =
        resolve_role_model_for_provider(&config, Role::Secondary, "anthropic", None).unwrap();
    assert_eq!(resolved.model, "secondary-role-model");

    let _env = env_set("SECONDARY_MODEL", "secondary-env-model");
    let resolved =
        resolve_role_model_for_provider(&config, Role::Secondary, "anthropic", None).unwrap();
    assert_eq!(resolved.model, "secondary-env-model");
}

#[test]
fn same_provider_can_fill_both_roles_with_different_models() {
    let config = new_test_config();
    enable_provider(&config, "ark", "entry-ark");
    set_active_provider(&config, "ark", "major-model").unwrap();
    set_secondary_provider(&config, Some("ark")).unwrap();
    set_secondary_provider_model(&config, "secondary-model").unwrap();

    let major = resolve_role_model(&config, Role::Major).unwrap();
    assert_eq!(major.provider, "ark");
    assert_eq!(major.model, "major-model");

    let secondary = resolve_role_model(&config, Role::Secondary).unwrap();
    assert_eq!(secondary.provider, "ark");
    assert_eq!(secondary.model, "secondary-model");
}

#[test]
fn save_role_selection_keeps_provider_entry_model_untouched() {
    let config = new_test_config();
    enable_provider(&config, "openai", "entry-default");

    save_role_selection(&config, Role::Major, "openai", Some("picked")).unwrap();

    let entry = get_provider_entry(&config, "openai").unwrap();
    assert_eq!(entry.model, "entry-default");
    assert_eq!(get_active_provider(&config), Some("openai".to_string()));
    assert_eq!(get_active_model(&config), Some("picked".to_string()));
}

#[test]
fn save_role_selection_with_none_clears_role_model() {
    let config = new_test_config();
    enable_provider(&config, "openai", "entry-default");
    set_active_provider(&config, "openai", "old").unwrap();

    save_role_selection(&config, Role::Major, "openai", None).unwrap();

    assert!(openduck::config::providers::file_active_model(&config).is_none());
    assert_eq!(get_active_model(&config), Some("entry-default".to_string()));
}

#[test]
fn clear_active_provider_removes_role_and_legacy_keys() {
    let config = new_test_config();
    set_active_provider(&config, "openai", "role-model").unwrap();
    config.set_param("GOOSE_MODEL", "legacy").unwrap();

    clear_active_provider(&config).unwrap();

    assert!(get_active_provider(&config).is_none());
    assert!(get_active_model(&config).is_none());
    assert!(openduck::config::providers::file_active_model(&config).is_none());
}

#[test]
fn set_secondary_provider_none_clears_secondary_model() {
    let config = new_test_config();
    enable_provider(&config, "anthropic", "entry");
    set_secondary_provider(&config, Some("anthropic")).unwrap();
    set_secondary_provider_model(&config, "model").unwrap();

    set_secondary_provider(&config, None).unwrap();

    assert!(get_secondary_provider(&config).is_none());
    assert!(openduck::config::providers::file_secondary_provider_model(&config).is_none());
}

#[test]
fn clear_secondary_provider_model_removes_only_role_key() {
    let config = new_test_config();
    enable_provider(&config, "anthropic", "entry");
    set_secondary_provider(&config, Some("anthropic")).unwrap();
    set_secondary_provider_model(&config, "model").unwrap();

    clear_secondary_provider_model(&config).unwrap();

    assert!(get_secondary_provider(&config).is_some());
    assert!(openduck::config::providers::file_secondary_provider_model(&config).is_none());
}

#[test]
fn validate_selection_checks_existence_and_enabled_only() {
    let config = new_test_config();
    set_provider_entry(
        &config,
        "openai",
        &ProviderEntry {
            enabled: true,
            model: String::new(),
            configured: false,
            ..Default::default()
        },
    )
    .unwrap();

    validate_selection(&config, Some("openai"), None).unwrap();

    let err = validate_selection(&config, Some("missing"), None).unwrap_err();
    assert!(matches!(err, ConfigError::NotFound(_)));

    set_provider_entry(
        &config,
        "disabled",
        &ProviderEntry {
            enabled: false,
            model: "m".to_string(),
            configured: true,
            ..Default::default()
        },
    )
    .unwrap();
    let err = validate_selection(&config, Some("disabled"), None).unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn fast_model_is_kept_even_when_equal_to_entry_model() {
    let config = new_test_config();
    let mut entry = ProviderEntry {
        enabled: true,
        model: "same-model".to_string(),
        configured: true,
        fast_model: Some("same-model".to_string()),
        ..Default::default()
    };
    entry.normalize();
    set_provider_entry(&config, "openai", &entry).unwrap();

    let loaded = get_provider_entry(&config, "openai").unwrap();
    assert_eq!(loaded.fast_model.as_deref(), Some("same-model"));
}

#[test]
fn v1_config_with_legacy_keys_resolves() {
    let (config, _config_file, _secrets_file) = config_from_raw_yaml(
        r#"
GOOSE_PROVIDER: openai
GOOSE_MODEL: legacy-model
"#,
    );
    assert!(!provider_roles_migrated(&config));

    let resolved = resolve_role_model_for_provider(&config, Role::Major, "openai", None).unwrap();
    assert_eq!(resolved.model, "legacy-model");
}
