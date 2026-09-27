use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerCondition {
    #[default]
    Always,
    OnSuccess,
    OnFailure,
    OnStatusChange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRule {
    #[serde(default)]
    pub trigger_on: TriggerCondition,
    #[serde(default)]
    pub recipients: Vec<String>,
    #[serde(default)]
    pub min_duration_seconds: Option<u64>,
    #[serde(default)]
    pub projects: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectNotificationConfig {
    #[serde(default)]
    pub recipients: Vec<String>,
    #[serde(default)]
    pub rules: Vec<NotificationRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum ProjectNotificationEntry {
    Recipients(Vec<String>),
    Detailed(ProjectNotificationConfig),
}

fn deserialize_projects_map<'de, D>(
    deserializer: D,
) -> Result<std::collections::HashMap<String, ProjectNotificationConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use std::collections::HashMap;
    let map: Option<HashMap<String, ProjectNotificationEntry>> = Option::deserialize(deserializer)?;
    let result = match map {
        Some(m) => m
            .into_iter()
            .map(|(k, v)| {
                let cfg = match v {
                    ProjectNotificationEntry::Recipients(recipients) => ProjectNotificationConfig {
                        recipients,
                        rules: Vec::new(),
                    },
                    ProjectNotificationEntry::Detailed(cfg) => cfg,
                };
                (k, cfg)
            })
            .collect(),
        None => HashMap::new(),
    };
    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    #[serde(default = "default_smtp_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub use_tls: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    pub from: String,
}

fn default_smtp_port() -> u16 {
    587
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmailConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub smtp: Option<SmtpConfig>,
    #[serde(default)]
    pub rules: Vec<NotificationRule>,
    #[serde(default)]
    pub default_recipients: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_projects_map")]
    pub projects: std::collections::HashMap<String, ProjectNotificationConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default)]
    pub email: EmailConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NotificationConfigEntry {
    FilePath(String),
    FileObject { file: String },
    Inline(NotificationConfig),
}

pub fn resolve_config_file_path(path_str: &str) -> std::path::PathBuf {
    let expanded = shellexpand::tilde(path_str).to_string();
    let path = std::path::PathBuf::from(&expanded);
    if path.is_absolute() && path.exists() {
        tracing::debug!(path = %path.display(), "Resolved notification config file (absolute path exists)");
        return path;
    }

    // 1. Try relative to config dir (e.g. ~/.config/goose/notifications.yaml)
    let config_dir_path = crate::config::paths::Paths::config_dir().join(&expanded);
    if config_dir_path.exists() {
        tracing::debug!(path = %config_dir_path.display(), "Resolved notification config file in config directory");
        return config_dir_path;
    }

    // 2. Try relative to current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_path = cwd.join(&expanded);
        if cwd_path.exists() {
            tracing::debug!(path = %cwd_path.display(), "Resolved notification config file in current working directory");
            return cwd_path;
        }
    }

    if path.is_absolute() {
        path
    } else {
        crate::config::paths::Paths::config_dir().join(&expanded)
    }
}

impl NotificationConfig {
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    fn from_file_or_default(resolved: std::path::PathBuf, source: &str) -> Self {
        match Self::from_file(&resolved) {
            Ok(cfg) => {
                tracing::info!(
                    source,
                    path = %resolved.display(),
                    email_enabled = cfg.email.enabled,
                    smtp_configured = cfg.email.smtp.is_some(),
                    rule_count = cfg.email.rules.len(),
                    default_recipient_count = cfg.email.default_recipients.len(),
                    project_count = cfg.email.projects.len(),
                    "Loaded notification config from file"
                );
                cfg
            }
            Err(err) => {
                tracing::warn!(
                    source,
                    path = %resolved.display(),
                    %err,
                    "Failed to load notification config file; using defaults (email disabled)"
                );
                Self::default()
            }
        }
    }

    fn log_ready(&self) {
        tracing::info!(
            email_enabled = self.email.enabled,
            smtp_configured = self.email.smtp.is_some(),
            smtp_host = self.email.smtp.as_ref().map(|s| s.host.as_str()),
            smtp_port = self.email.smtp.as_ref().map(|s| s.port),
            smtp_use_tls = self.email.smtp.as_ref().map(|s| s.use_tls),
            smtp_from = self.email.smtp.as_ref().map(|s| s.from.as_str()),
            rule_count = self.email.rules.len(),
            default_recipient_count = self.email.default_recipients.len(),
            default_recipients = ?self.email.default_recipients,
            project_count = self.email.projects.len(),
            "Notification configuration ready"
        );
    }

    pub fn load() -> Self {
        // 1. Check if an explicit notification config file path is set via env var
        let custom_file = std::env::var("OPENDUCK_NOTIFICATION_CONFIG")
            .or_else(|_| std::env::var("GOOSE_NOTIFICATION_CONFIG"))
            .ok();

        let mut config = if let Some(path) = custom_file {
            tracing::info!(
                path = %path,
                "Loading notification config from OPENDUCK_NOTIFICATION_CONFIG / GOOSE_NOTIFICATION_CONFIG"
            );
            let resolved = resolve_config_file_path(&path);
            Self::from_file_or_default(resolved, "env_var")
        } else {
            // 2. Check if a notifications_file is specified in config.yaml
            let file_param = crate::config::Config::global()
                .get_param::<String>("notifications_file")
                .or_else(|_| {
                    crate::config::Config::global().get_param::<String>("notification_config")
                })
                .or_else(|_| {
                    crate::config::Config::global()
                        .get_param::<String>("openduck_notifications_file")
                });

            if let Ok(file_name) = file_param {
                tracing::info!(
                    file = %file_name,
                    "Loading notification config from notifications_file config param"
                );
                let resolved = resolve_config_file_path(&file_name);
                Self::from_file_or_default(resolved, "config_param")
            } else {
                // 3. Otherwise check `notifications:` in config.yaml (supports string path, {file: ...}, or inline config)
                let entry_res = crate::config::Config::global()
                    .get_param::<NotificationConfigEntry>("notifications")
                    .or_else(|_| {
                        crate::config::Config::global()
                            .get_param::<NotificationConfigEntry>("openduck_notifications")
                    });

                match entry_res {
                    Ok(NotificationConfigEntry::FilePath(file_path)) => {
                        tracing::info!(
                            file = %file_path,
                            "Loading notification config from notifications file path"
                        );
                        let resolved = resolve_config_file_path(&file_path);
                        Self::from_file_or_default(resolved, "notifications_path")
                    }
                    Ok(NotificationConfigEntry::FileObject { file }) => {
                        tracing::info!(
                            file = %file,
                            "Loading notification config from notifications.file"
                        );
                        let resolved = resolve_config_file_path(&file);
                        Self::from_file_or_default(resolved, "notifications_file_object")
                    }
                    Ok(NotificationConfigEntry::Inline(cfg)) => {
                        tracing::info!(
                            email_enabled = cfg.email.enabled,
                            smtp_configured = cfg.email.smtp.is_some(),
                            rule_count = cfg.email.rules.len(),
                            default_recipient_count = cfg.email.default_recipients.len(),
                            project_count = cfg.email.projects.len(),
                            "Loaded inline notification config"
                        );
                        cfg
                    }
                    Err(_) => {
                        tracing::info!(
                            "No notification config found in configuration files; using defaults (email disabled unless SMTP env vars are set)"
                        );
                        Self::default()
                    }
                }
            }
        };

        // 4. Merge / fallback to environment variables
        let env_config = Self::from_env();
        if env_config.email.enabled {
            if !config.email.enabled {
                tracing::info!(
                    smtp_host = env_config.email.smtp.as_ref().map(|s| s.host.as_str()),
                    smtp_port = env_config.email.smtp.as_ref().map(|s| s.port),
                    default_recipient_count = env_config.email.default_recipients.len(),
                    "Applying SMTP notification settings from environment variables"
                );
                config.email = env_config.email;
            } else {
                if config.email.smtp.is_none() {
                    tracing::info!(
                        smtp_host = env_config.email.smtp.as_ref().map(|s| s.host.as_str()),
                        smtp_port = env_config.email.smtp.as_ref().map(|s| s.port),
                        "Filling missing SMTP config from environment variables"
                    );
                    config.email.smtp = env_config.email.smtp;
                }
                if config.email.default_recipients.is_empty() {
                    tracing::info!(
                        default_recipient_count = env_config.email.default_recipients.len(),
                        "Filling missing default notification recipients from environment variables"
                    );
                    config.email.default_recipients = env_config.email.default_recipients;
                }
            }
        }

        config.log_ready();
        config
    }

    pub fn from_env() -> Self {
        let mut config = Self::default();

        let host = std::env::var("SMTP_HOST").ok();
        let from = std::env::var("SMTP_FROM").ok();

        match (&host, &from) {
            (Some(h), Some(f)) => {
                let port = std::env::var("SMTP_PORT")
                    .ok()
                    .and_then(|p| p.parse::<u16>().ok())
                    .unwrap_or(587);

                let use_tls = std::env::var("SMTP_USE_TLS")
                    .map(|v| v != "0" && v.to_lowercase() != "false")
                    .unwrap_or(true);

                let username = std::env::var("SMTP_USER")
                    .or_else(|_| std::env::var("SMTP_USERNAME"))
                    .ok();
                let password = std::env::var("SMTP_PASS")
                    .or_else(|_| std::env::var("SMTP_PASSWORD"))
                    .ok();

                let recipients = std::env::var("OPENDUCK_NOTIFY_EMAIL")
                    .or_else(|_| std::env::var("GOOSE_NOTIFY_EMAIL"))
                    .map(|emails| {
                        emails
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                tracing::info!(
                    smtp_host = %h,
                    smtp_from = %f,
                    smtp_port = port,
                    smtp_use_tls = use_tls,
                    has_auth = username.is_some(),
                    recipients = ?recipients,
                    "Loaded SMTP notification configuration from environment variables"
                );

                config.email = EmailConfig {
                    enabled: true,
                    smtp: Some(SmtpConfig {
                        host: h.clone(),
                        port,
                        use_tls,
                        username,
                        password,
                        from: f.clone(),
                    }),
                    rules: vec![NotificationRule {
                        trigger_on: TriggerCondition::Always,
                        recipients: recipients.clone(),
                        min_duration_seconds: None,
                        projects: None,
                    }],
                    default_recipients: recipients,
                    projects: std::collections::HashMap::new(),
                };
            }
            (Some(h), None) => {
                tracing::warn!(
                    smtp_host = %h,
                    "SMTP_HOST is set in environment, but SMTP_FROM is missing; email notifications cannot be configured from environment variables without SMTP_FROM"
                );
            }
            (None, Some(f)) => {
                tracing::warn!(
                    smtp_from = %f,
                    "SMTP_FROM is set in environment, but SMTP_HOST is missing; email notifications cannot be configured from environment variables without SMTP_HOST"
                );
            }
            (None, None) => {
                tracing::trace!("No SMTP environment variables (SMTP_HOST / SMTP_FROM) found");
            }
        }

        config
    }
}
