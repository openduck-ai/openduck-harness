use anyhow::Result;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::acp::{
    configured_model_for_provider, extension_configs_to_mcp_servers, AcpProvider,
    AcpProviderConfig, ACP_CURRENT_MODEL,
};
use crate::config::search_path::SearchPaths;
use crate::config::{Config, GooseMode};
use crate::providers::base::{
    current_working_dir, ProviderDef, ProviderDescriptor, ProviderMetadata,
};
use crate::providers::catalog::ProviderSetupMetadata;

pub const GROK_ACP_PROVIDER_NAME: &str = "grok-acp";
const GROK_ACP_DOC_URL: &str = "https://agentclientprotocol.com/overview/introduction";
pub const GROK_ACP_BINARY: &str = "grok";

pub struct GrokAcpProvider;

impl openduck_providers::base::ProviderDescriptor for GrokAcpProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            GROK_ACP_PROVIDER_NAME,
            "Grok CLI (ACP)",
            "Use goose with your Grok subscription via the Grok CLI over ACP.",
            ACP_CURRENT_MODEL,
            vec![],
            GROK_ACP_DOC_URL,
            vec![],
        )
        .with_setup_steps(vec![
            "Install the Grok CLI",
            "Ensure your Grok CLI is authenticated (run `grok` to verify)",
        ])
        .with_setup(
            ProviderSetupMetadata::cli_agent(GROK_ACP_BINARY, &["grok-acp", "grok"])
                .with_acp()
                .with_capabilities(true, true, true),
        )
    }
}

impl GrokAcpProvider {
    fn create(
        extensions: Vec<crate::config::ExtensionConfig>,
        working_dir: PathBuf,
        use_default_model: bool,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Box::pin(async move {
            let config = Config::global();
            let resolved_command = SearchPaths::builder().with_npm().resolve(GROK_ACP_BINARY)?;
            let goose_mode = config.get_goose_mode().unwrap_or(GooseMode::Auto);
            let model = if use_default_model {
                ACP_CURRENT_MODEL.to_string()
            } else {
                configured_model_for_provider(config, GROK_ACP_PROVIDER_NAME)
            };

            let args = vec!["agent".to_string(), "stdio".to_string()];
            let session_config_options = if model == ACP_CURRENT_MODEL {
                vec![]
            } else {
                vec![("model".to_string(), model)]
            };

            let mode_mapping = HashMap::from([
                (
                    GooseMode::Auto,
                    vec![
                        "bypassPermissions".to_string(),
                        "dontAsk".to_string(),
                        "agent".to_string(),
                    ],
                ),
                (GooseMode::Approve, vec!["default".to_string()]),
                (GooseMode::SmartApprove, vec!["acceptEdits".to_string()]),
                (GooseMode::Chat, vec!["plan".to_string()]),
            ]);

            let provider_config = AcpProviderConfig {
                command: resolved_command,
                args,
                env: vec![],
                env_remove: vec![],
                work_dir: working_dir,
                mcp_servers: extension_configs_to_mcp_servers(&extensions),
                session_mode_id: mode_mapping[&goose_mode].first().cloned(),
                session_config_options,
                model_config_option_id: Some("model".to_string()),
                mode_mapping,
                notification_callback: None,
            };

            let metadata = Self::metadata();
            AcpProvider::connect(metadata.name, goose_mode, provider_config).await
        })
    }
}

impl ProviderDef for GrokAcpProvider {
    type Provider = AcpProvider;

    fn from_env(
        extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Self::from_env_with_working_dir(extensions, current_working_dir(), tls_config)
    }

    fn from_env_with_working_dir(
        extensions: Vec<crate::config::ExtensionConfig>,
        working_dir: PathBuf,
        _tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Self::create(extensions, working_dir, false)
    }

    fn from_env_with_default_model(
        extensions: Vec<crate::config::ExtensionConfig>,
        _tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Self::create(extensions, current_working_dir(), true)
    }
}
