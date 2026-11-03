use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use openduck::acp::server::AcpBuiltinSelection;
use openduck::acp::server_factory::{AcpServer, AcpServerFactoryConfig};
use openduck::agents::GoosePlatform;
use openduck::config::Config;
use openduck::control::app_config::{self, AppConfigError, ModelSource};
use openduck::providers::inventory::ProviderInventoryService;
use openduck::session::session_manager::SessionStorage;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

struct Fixture {
    config: Config,
    path: std::path::PathBuf,
    _config_file: tempfile::NamedTempFile,
    _secrets_file: tempfile::NamedTempFile,
}

fn quiet_env() -> env_lock::EnvGuard<'static> {
    env_lock::lock_env([
        ("OPENDUCK_PROVIDER", None::<&str>),
        ("GOOSE_PROVIDER", None::<&str>),
        ("PROVIDER", None::<&str>),
        ("OPENDUCK_MODEL", None::<&str>),
        ("GOOSE_MODEL", None::<&str>),
        ("MODEL", None::<&str>),
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

fn fixture_with(yaml: &str) -> Fixture {
    let config_file = tempfile::NamedTempFile::new().unwrap();
    let secrets_file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(config_file.path(), yaml).unwrap();
    let config = Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap();
    Fixture {
        path: config_file.path().to_path_buf(),
        config,
        _config_file: config_file,
        _secrets_file: secrets_file,
    }
}

fn file_bytes(fixture: &Fixture) -> Vec<u8> {
    std::fs::read(&fixture.path).unwrap()
}

fn saved_yaml(fixture: &Fixture) -> serde_yaml::Value {
    let text = std::fs::read_to_string(&fixture.path).unwrap();
    serde_yaml::from_str(&text).unwrap()
}

fn invalid_message(error: AppConfigError) -> String {
    match error {
        AppConfigError::Invalid(message) => message,
        AppConfigError::Storage(message) => {
            panic!("expected a validation error, got storage: {message}")
        }
    }
}

fn openai_v2(model: Value, fast_model: Value) -> Value {
    json!({
        "name": "openai",
        "enabled": true,
        "model": model,
        "fastModel": fast_model,
    })
}

fn v2_body(
    major: Value,
    major_model: Value,
    secondary: Value,
    secondary_model: Value,
    providers: Value,
) -> Value {
    json!({
        "selectionVersion": 2,
        "majorProvider": major,
        "majorModel": major_model,
        "secondaryProvider": secondary,
        "secondaryProviderModel": secondary_model,
        "providers": providers,
    })
}

async fn put(
    fixture: &Fixture,
    body: Value,
) -> Result<app_config::ProvidersConfigResponse, AppConfigError> {
    app_config::update_providers_config(&fixture.config, body, None).await
}

fn assert_no_secret_keys(value: &Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let normalized = key.to_ascii_lowercase().replace('-', "_");
                assert!(
                    normalized != "api_key" && normalized != "apikey" && normalized != "token",
                    "serialized provider config contains secret key {key}"
                );
                assert_no_secret_keys(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                assert_no_secret_keys(item);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn v2_requires_both_role_fields() {
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
    let before = file_bytes(&fixture);
    let mut body = v2_body(
        json!("openai"),
        json!("gpt-4o"),
        Value::Null,
        Value::Null,
        json!([openai_v2(json!("gpt-4o"), Value::Null)]),
    );
    body.as_object_mut().unwrap().remove("majorModel");
    let message = invalid_message(put(&fixture, body).await.unwrap_err());
    assert!(
        message.contains("majorModel field is required"),
        "{message}"
    );
    assert_eq!(file_bytes(&fixture), before);

    let mut body = v2_body(
        json!("openai"),
        json!("gpt-4o"),
        Value::Null,
        Value::Null,
        json!([openai_v2(json!("gpt-4o"), Value::Null)]),
    );
    body.as_object_mut()
        .unwrap()
        .remove("secondaryProviderModel");
    let message = invalid_message(put(&fixture, body).await.unwrap_err());
    assert!(
        message.contains("secondaryProviderModel field is required"),
        "{message}"
    );
    assert_eq!(file_bytes(&fixture), before);
}

#[tokio::test]
async fn provider_null_requires_model_null() {
    let _guard = quiet_env();
    let fixture = fixture_with(
        r#"
active_provider: openai
active_model: keep-me
providers:
  openai:
    enabled: true
    configured: true
    model: gpt-4o
"#,
    );
    let before = file_bytes(&fixture);
    let body = v2_body(
        Value::Null,
        json!("gpt-4o"),
        Value::Null,
        Value::Null,
        json!([openai_v2(json!("gpt-4o"), Value::Null)]),
    );
    let message = invalid_message(put(&fixture, body).await.unwrap_err());
    assert!(
        message.contains("majorModel must be null when majorProvider is null"),
        "{message}"
    );
    assert_eq!(file_bytes(&fixture), before);
}

#[tokio::test]
async fn model_null_clears_explicit_role_model_and_restores_provider_default() {
    let _guard = quiet_env();
    let fixture = fixture_with(
        r#"
active_provider: openai
active_model: explicit-role
providers:
  openai:
    enabled: true
    configured: true
    model: provider-default
"#,
    );
    let body = v2_body(
        json!("openai"),
        Value::Null,
        Value::Null,
        Value::Null,
        json!([openai_v2(json!("provider-default"), Value::Null)]),
    );
    let response = put(&fixture, body).await.unwrap();
    let major = response.major_model.unwrap();
    assert_eq!(major.value.as_deref(), Some("provider-default"));
    assert_eq!(major.file_value, None);
    assert!(!major.from_env);
    assert_eq!(major.source, ModelSource::ProviderDefault);
    let yaml = saved_yaml(&fixture);
    assert!(yaml.get("active_model").is_none());
    assert_eq!(
        yaml["providers"]["openai"]["model"].as_str(),
        Some("provider-default")
    );
}

#[tokio::test]
async fn provider_row_model_omit_keeps_empty_clears_and_null_rejects() {
    let _guard = quiet_env();
    let yaml = r#"
active_provider: openai
active_model: keep-role
providers:
  openai:
    enabled: true
    configured: true
    model: old-default
  anthropic:
    enabled: true
    configured: true
    model: claude-old
"#;
    let fixture = fixture_with(yaml);
    let before = file_bytes(&fixture);
    let providers = json!([
        {
            "name": "openai",
            "enabled": true,
            "model": "old-default"
        },
        {
            "name": "anthropic",
            "enabled": true,
            "model": null
        }
    ]);
    let message = invalid_message(
        put(
            &fixture,
            v2_body(
                json!("openai"),
                json!("keep-role"),
                Value::Null,
                Value::Null,
                providers,
            ),
        )
        .await
        .unwrap_err(),
    );
    assert!(message.contains("cannot be null"), "{message}");
    assert_eq!(file_bytes(&fixture), before);

    let whitespace = fixture_with(yaml);
    let before = file_bytes(&whitespace);
    let message = invalid_message(
        put(
            &whitespace,
            v2_body(
                json!("openai"),
                json!("   "),
                Value::Null,
                Value::Null,
                json!([
                    {"name": "openai", "enabled": true, "model": "old-default"},
                    {"name": "anthropic", "enabled": true, "model": "   "}
                ]),
            ),
        )
        .await
        .unwrap_err(),
    );
    assert!(
        message.contains("cannot be blank") || message.contains("blank or padded"),
        "{message}"
    );
    assert_eq!(file_bytes(&whitespace), before);

    let kept = fixture_with(yaml);
    let response = put(
        &kept,
        v2_body(
            json!("openai"),
            json!("keep-role"),
            Value::Null,
            Value::Null,
            json!([
                {"name": "openai", "enabled": true},
                {"name": "anthropic", "enabled": true, "model": ""}
            ]),
        ),
    )
    .await
    .unwrap();
    let yaml = saved_yaml(&kept);
    assert_eq!(
        yaml["providers"]["openai"]["model"].as_str(),
        Some("old-default")
    );
    assert_eq!(yaml["providers"]["anthropic"]["model"].as_str(), Some(""));
    assert_eq!(yaml["active_model"].as_str(), Some("keep-role"));
    assert_eq!(
        response.major_model.unwrap().value.as_deref(),
        Some("keep-role")
    );
}

#[tokio::test]
async fn model_ids_are_stored_verbatim_and_fast_model_equal_to_default_is_kept() {
    let _guard = quiet_env();
    let fixture = fixture_with(
        r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: true
    model: old-default
"#,
    );
    let response = put(
        &fixture,
        v2_body(
            json!("openai"),
            json!("Role Model Id"),
            json!("openai"),
            json!("Secondary Model Id"),
            json!([openai_v2(json!("row model id"), json!("row model id"))]),
        ),
    )
    .await
    .unwrap();
    let yaml = saved_yaml(&fixture);
    assert_eq!(yaml["active_model"].as_str(), Some("Role Model Id"));
    assert_eq!(
        yaml["secondary_provider_model"].as_str(),
        Some("Secondary Model Id")
    );
    assert_eq!(
        yaml["providers"]["openai"]["model"].as_str(),
        Some("row model id")
    );
    assert_eq!(
        yaml["providers"]["openai"]["fast_model"].as_str(),
        Some("row model id")
    );
    assert_eq!(yaml["active_provider"].as_str(), Some("openai"));
    assert_eq!(yaml["secondary_provider"].as_str(), Some("openai"));
    assert_eq!(
        response.major_model.unwrap().value.as_deref(),
        Some("Role Model Id")
    );
    assert_eq!(
        response.secondary_provider_model.unwrap().value.as_deref(),
        Some("Secondary Model Id")
    );
    let openai = response
        .providers
        .iter()
        .find(|provider| provider.name == "openai")
        .unwrap();
    assert_eq!(openai.fast_model.as_deref(), Some("row model id"));
    assert!(!openai.fast_model_uses_main);
}

#[tokio::test]
async fn env_locked_role_compares_the_file_value() {
    let _guard = env_lock::lock_env([
        ("OPENDUCK_PROVIDER", None::<&str>),
        ("GOOSE_PROVIDER", None::<&str>),
        ("PROVIDER", None::<&str>),
        ("OPENDUCK_MODEL", None::<&str>),
        ("GOOSE_MODEL", Some("env-model")),
        ("MODEL", None::<&str>),
        ("OPENDUCK_FAST_MODEL", None::<&str>),
        ("GOOSE_FAST_MODEL", None::<&str>),
        ("FAST_MODEL", None::<&str>),
        ("OPENDUCK_SECONDARY_PROVIDER", None::<&str>),
        ("GOOSE_SECONDARY_PROVIDER", None::<&str>),
        ("SECONDARY_PROVIDER", None::<&str>),
        ("OPENDUCK_SECONDARY_MODEL", None::<&str>),
        ("GOOSE_SECONDARY_MODEL", None::<&str>),
        ("SECONDARY_MODEL", None::<&str>),
    ]);
    let fixture = fixture_with(
        r#"
active_provider: openai
active_model: file-model
providers:
  openai:
    enabled: true
    configured: true
    model: row-default
"#,
    );
    let before = file_bytes(&fixture);
    let rejected = v2_body(
        json!("openai"),
        json!("env-model"),
        Value::Null,
        Value::Null,
        json!([openai_v2(json!("row-default"), Value::Null)]),
    );
    let message = invalid_message(put(&fixture, rejected).await.unwrap_err());
    assert!(message.contains("locked by environment"), "{message}");
    assert_eq!(file_bytes(&fixture), before);

    let accepted = v2_body(
        json!("openai"),
        json!("file-model"),
        Value::Null,
        Value::Null,
        json!([openai_v2(json!("row-default"), Value::Null)]),
    );
    put(&fixture, accepted).await.unwrap();
    let yaml = saved_yaml(&fixture);
    assert_eq!(yaml["active_model"].as_str(), Some("file-model"));
    assert_ne!(yaml["active_model"].as_str(), Some("env-model"));
}

#[tokio::test]
async fn selected_role_must_resolve_a_non_empty_model() {
    let _guard = quiet_env();
    let fixture = fixture_with(
        r#"
active_provider: openai
providers:
  openai:
    enabled: true
    configured: false
    model: ""
"#,
    );
    let before = file_bytes(&fixture);
    let body = v2_body(
        json!("openai"),
        Value::Null,
        Value::Null,
        Value::Null,
        json!([openai_v2(json!(""), Value::Null)]),
    );
    let message = invalid_message(put(&fixture, body).await.unwrap_err());
    assert!(message.contains("non-empty model"), "{message}");
    assert!(!message.contains("not configured"), "{message}");
    assert_eq!(file_bytes(&fixture), before);
}

#[tokio::test]
async fn v1_legacy_default_model_edit_syncs_the_role_model() {
    let _guard = quiet_env();
    let fixture = fixture_with(
        r#"
active_provider: openai
active_model: old-role
providers:
  openai:
    enabled: true
    configured: true
    model: old-default
"#,
    );
    let unchanged = json!({
        "majorProvider": "openai",
        "secondaryProvider": null,
        "providers": [{
            "name": "openai",
            "enabled": true,
            "model": "old-default",
            "fastModel": "old-default",
            "secondaryModel": null
        }]
    });
    put(&fixture, unchanged).await.unwrap();
    let yaml = saved_yaml(&fixture);
    assert_eq!(yaml["active_model"].as_str(), Some("old-role"));
    assert_eq!(
        yaml["providers"]["openai"]["model"].as_str(),
        Some("old-default")
    );
    assert_eq!(
        yaml["providers"]["openai"]["fast_model"].as_str(),
        Some("old-default")
    );

    let changed = json!({
        "majorProvider": "openai",
        "secondaryProvider": null,
        "providers": [{
            "name": "openai",
            "enabled": true,
            "model": "new-default",
            "fastModel": null,
            "secondaryModel": null
        }]
    });
    put(&fixture, changed).await.unwrap();
    let yaml = saved_yaml(&fixture);
    assert_eq!(yaml["active_model"].as_str(), Some("new-default"));
    assert_eq!(
        yaml["providers"]["openai"]["model"].as_str(),
        Some("new-default")
    );
}

#[tokio::test]
async fn get_reports_source_fast_fallback_and_inventory_without_secrets() {
    let _guard = quiet_env();
    let default_wins = fixture_with(
        r#"
active_provider: openai
GOOSE_MODEL: legacy-model
providers:
  openai:
    enabled: true
    configured: true
    model: entry-default
"#,
    );
    let response = app_config::get_providers_config(&default_wins.config, None)
        .await
        .unwrap();
    let major = response.major_model.unwrap();
    assert_eq!(response.selection_version, Some(2));
    assert_eq!(major.value.as_deref(), Some("entry-default"));
    assert_eq!(major.file_value, None);
    assert_eq!(major.source, ModelSource::ProviderDefault);

    let legacy = fixture_with(
        r#"
active_provider: openai
GOOSE_MODEL: legacy-model
providers:
  openai:
    enabled: true
    configured: true
    model: ""
"#,
    );
    let major = app_config::get_providers_config(&legacy.config, None)
        .await
        .unwrap()
        .major_model
        .unwrap();
    assert_eq!(major.value.as_deref(), Some("legacy-model"));
    assert_eq!(major.file_value, None);
    assert_eq!(major.source, ModelSource::Legacy);

    let roles = fixture_with(
        r#"
active_provider: openai
active_model: role-model
secondary_provider: openai
secondary_provider_model: secondary-role
providers:
  openai:
    enabled: true
    configured: true
    model: entry-default
"#,
    );
    let dir = TempDir::new().unwrap();
    let inventory =
        ProviderInventoryService::new(Arc::new(SessionStorage::new(dir.path().to_path_buf())));
    inventory
        .store_refreshed_models("openai", &["gpt-4o".to_string()])
        .await
        .unwrap();
    inventory
        .store_refresh_error(
            "openai",
            "Authorization: Bearer sk-abcdefghijklmnop at https://user:pw@example.com/v1?api_key=sekret",
        )
        .await
        .unwrap();
    let response = app_config::get_providers_config(&roles.config, Some(&inventory))
        .await
        .unwrap();
    assert_eq!(response.selection_version, Some(2));
    let major = response.major_model.as_ref().unwrap();
    assert_eq!(major.value.as_deref(), Some("role-model"));
    assert_eq!(major.file_value.as_deref(), Some("role-model"));
    assert_eq!(major.source, ModelSource::Role);
    assert!(!major.from_env);
    let secondary = response.secondary_provider_model.as_ref().unwrap();
    assert_eq!(secondary.value.as_deref(), Some("secondary-role"));
    assert_eq!(secondary.file_value.as_deref(), Some("secondary-role"));
    assert_eq!(secondary.source, ModelSource::Role);
    let openai = response
        .providers
        .iter()
        .find(|provider| provider.name == "openai")
        .unwrap();
    assert_eq!(openai.resolved_fast_model, "role-model");
    assert!(openai.fast_model_uses_main);
    assert!(openai.models.is_some());
    assert!(openai.inventory_status.is_some());
    assert!(openai.supports_refresh.is_some());
    assert!(openai.refreshing.is_some());
    assert!(openai.last_updated_at.is_some());
    let refresh_error = openai.last_refresh_error.as_deref().unwrap();
    assert!(!refresh_error.contains("sk-abcdefghijklmnop"));
    assert!(!refresh_error.contains("sekret"));
    assert!(!refresh_error.contains("user:pw"));

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["selectionVersion"], 2);
    for key in ["value", "fileValue", "fromEnv", "source"] {
        assert!(json["majorModel"].get(key).is_some(), "majorModel.{key}");
        assert!(
            json["secondaryProviderModel"].get(key).is_some(),
            "secondaryProviderModel.{key}"
        );
    }
    let provider = json["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|provider| provider["name"] == "openai")
        .unwrap();
    for key in [
        "models",
        "inventoryStatus",
        "supportsRefresh",
        "refreshing",
        "lastUpdatedAt",
        "lastRefreshError",
    ] {
        assert!(provider.get(key).is_some(), "missing {key}");
    }
    assert_no_secret_keys(&json);
    let serialized = serde_json::to_string(&json).unwrap();
    assert!(!serialized.contains("sk-abcdefghijklmnop"));
    assert!(!serialized.contains("sekret"));
}

#[tokio::test]
async fn refresh_handler_returns_accepted() {
    let dir = TempDir::new().unwrap();
    let server = Arc::new(AcpServer::new(AcpServerFactoryConfig {
        builtins: AcpBuiltinSelection::default(),
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        goose_platform: GoosePlatform::GooseCli,
        additional_source_roots: Vec::new(),
        enable_scheduler: false,
    }));
    let router = openduck::control::http::routes(server);
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/config/providers/not-a-real-provider/models/refresh")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}
