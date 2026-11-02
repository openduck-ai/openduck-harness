
use openduck::config::providers::*;
use openduck::config::{Config};
use tempfile::NamedTempFile;

fn new_test_config() -> Config {
    let config_file = NamedTempFile::new().unwrap();
    let secrets_file = NamedTempFile::new().unwrap();
    Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap()
}

#[test]
fn debug_dump2() {
    let config = new_test_config();
    set_provider_entry(&config, "openai", &ProviderEntry { enabled: true, model: "entry-default".into(), configured: true, ..Default::default() }).unwrap();
    set_active_provider(&config, "openai", "").unwrap();
    println!("migrated={}", provider_roles_migrated(&config));
    println!("resolve={:?}", resolve_role_model_for_provider(&config, Role::Major, "openai", None));
    set_active_provider(&config, "openai", "role-model").unwrap();
    env_lock::lock_env([("MODEL", Some("env-model"))]);
    println!("resolve_env={:?}", resolve_role_model_for_provider(&config, Role::Major, "openai", None));
}
