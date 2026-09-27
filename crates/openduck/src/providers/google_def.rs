use anyhow::Result;
use futures::future::BoxFuture;
use openduck_providers::api_client::TlsConfig;
use openduck_providers::base::{ProviderDescriptor, ProviderMetadata};
use openduck_providers::google::{
    GoogleProvider, GEMINI_API_KEY_ENV, GOOGLE_API_HOST, GOOGLE_API_KEY_ENV,
};

use crate::config::{Config, ConfigError, ExtensionConfig};
use crate::providers::base::ProviderDef;

pub struct GoogleProviderDef;

impl ProviderDescriptor for GoogleProviderDef {
    fn metadata() -> ProviderMetadata {
        GoogleProvider::metadata().with_setup(
            crate::providers::catalog::ProviderSetupMetadata::api_key(
                crate::providers::catalog::ProviderSetupGroup::Default,
            )
            .with_docs_url("https://aistudio.google.com/apikey")
            .with_aliases(&["google", "gemini"]),
        )
    }
}

impl ProviderDef for GoogleProviderDef {
    type Provider = GoogleProvider;

    fn from_env(
        _extensions: Vec<ExtensionConfig>,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(from_env(tls_config))
    }
}

pub fn resolve_google_api_key(config: &Config) -> Result<String, ConfigError> {
    config
        .get_secret(GOOGLE_API_KEY_ENV)
        .or_else(|_| config.get_secret(GEMINI_API_KEY_ENV))
        .map_err(|_| ConfigError::NotFound(format!("{GOOGLE_API_KEY_ENV} or {GEMINI_API_KEY_ENV}")))
}

pub fn google_api_key_is_configured(config: &Config) -> bool {
    resolve_google_api_key(config).is_ok()
}

pub async fn from_env(tls_config: Option<TlsConfig>) -> Result<GoogleProvider> {
    let config = Config::global();
    let api_key = resolve_google_api_key(config)?;
    let host: String = config
        .get_param("GOOGLE_HOST")
        .unwrap_or_else(|_| GOOGLE_API_HOST.to_string());

    let thinking_budget = config.get_param("GEMINI25_THINKING_BUDGET").ok();

    GoogleProvider::new(
        host,
        api_key,
        tls_config,
        Some(crate::session_context::session_id_request_builder()),
        thinking_budget,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn test_config() -> Config {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap()
    }

    #[test]
    fn prefers_google_api_key_over_gemini_api_key() {
        let _guard = env_lock::lock_env([
            (GOOGLE_API_KEY_ENV, Some("google-key")),
            (GEMINI_API_KEY_ENV, Some("gemini-key")),
        ]);
        let config = test_config();
        assert_eq!(resolve_google_api_key(&config).unwrap(), "google-key");
    }

    #[test]
    fn falls_back_to_gemini_api_key() {
        let _guard = env_lock::lock_env([
            (GOOGLE_API_KEY_ENV, None::<&str>),
            (GEMINI_API_KEY_ENV, Some("gemini-key")),
        ]);
        let config = test_config();
        assert_eq!(resolve_google_api_key(&config).unwrap(), "gemini-key");
        assert!(google_api_key_is_configured(&config));
    }

    #[test]
    fn missing_both_google_and_gemini_keys_is_an_error() {
        let _guard = env_lock::lock_env([
            (GOOGLE_API_KEY_ENV, None::<&str>),
            (GEMINI_API_KEY_ENV, None::<&str>),
        ]);
        let config = test_config();
        let err = resolve_google_api_key(&config).unwrap_err();
        assert!(err.to_string().contains(GOOGLE_API_KEY_ENV));
        assert!(err.to_string().contains(GEMINI_API_KEY_ENV));
        assert!(!google_api_key_is_configured(&config));
    }
}
