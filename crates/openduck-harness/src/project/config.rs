use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PolicyConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "system_prompt"
    )]
    pub system_prompt: Option<String>,
}

fn default_sandbox_kind() -> String {
    "local".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SandboxConfig {
    #[serde(default = "default_sandbox_kind")]
    pub kind: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "container_image"
    )]
    pub container_image: Option<String>,
    #[serde(default, alias = "setup_commands")]
    pub setup_commands: Vec<String>,
    #[serde(default, alias = "env_vars")]
    pub env_vars: HashMap<String, String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            kind: default_sandbox_kind(),
            container_image: None,
            setup_commands: Vec::new(),
            env_vars: HashMap::new(),
        }
    }
}

fn default_max_turns() -> usize {
    25
}

fn default_timeout_seconds() -> u64 {
    300
}

fn default_concurrency() -> usize {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionConfig {
    #[serde(default = "default_max_turns", alias = "max_turns")]
    pub max_turns: usize,
    #[serde(default = "default_timeout_seconds", alias = "timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "stagnation_threshold",
        alias = "stagnationThreshold"
    )]
    pub stagnation_threshold: Option<usize>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "exploration_budget",
        alias = "explorationBudget",
        alias = "max_exploration_steps"
    )]
    pub exploration_budget: Option<usize>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "circuit_breaker_enabled",
        alias = "circuit_breaker",
        alias = "circuitBreaker",
        alias = "circuitBreakerEnabled"
    )]
    pub circuit_breaker_enabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "periodic_review_interval",
        alias = "periodicReviewInterval"
    )]
    pub periodic_review_interval: Option<usize>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "history_timeout_seconds",
        alias = "historyTimeoutSeconds"
    )]
    pub history_timeout_seconds: Option<u64>,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_turns: default_max_turns(),
            timeout_seconds: default_timeout_seconds(),
            concurrency: default_concurrency(),
            stagnation_threshold: None,
            exploration_budget: None,
            circuit_breaker_enabled: None,
            periodic_review_interval: None,
            history_timeout_seconds: None,
        }
    }
}

fn default_tasks_dir() -> String {
    ".goose/tasks".to_string()
}

fn default_results_dir() -> String {
    ".goose/harness_results".to_string()
}

fn default_cassettes_dir() -> String {
    ".goose/cassettes".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PathsConfig {
    #[serde(default = "default_tasks_dir", alias = "tasks_dir")]
    pub tasks_dir: String,
    #[serde(default = "default_results_dir", alias = "results_dir")]
    pub results_dir: String,
    #[serde(default = "default_cassettes_dir", alias = "cassettes_dir")]
    pub cassettes_dir: String,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            tasks_dir: default_tasks_dir(),
            results_dir: default_results_dir(),
            cassettes_dir: default_cassettes_dir(),
        }
    }
}

fn default_judge_provider() -> String {
    "laya".to_string()
}

fn default_judge_endpoint() -> String {
    crate::judge::DEFAULT_LAYA_ENDPOINT.to_string()
}

fn default_judge_timeout_ms() -> u64 {
    crate::judge::DEFAULT_LAYA_TIMEOUT_MS
}

fn default_fallback_on_error() -> bool {
    true
}

fn default_judge_points() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("turn.drift".to_string(), "shadow".to_string());
    map.insert("turn.completion".to_string(), "shadow".to_string());
    map.insert("context.forget".to_string(), "off".to_string());
    map.insert("tool.risk".to_string(), "active".to_string());
    map
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JudgeConfig {
    #[serde(default = "default_judge_provider")]
    pub provider: String,
    #[serde(default = "default_judge_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_judge_timeout_ms", alias = "timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_fallback_on_error", alias = "fallback_on_error")]
    pub fallback_on_error: bool,
    #[serde(default = "default_judge_points")]
    pub points: HashMap<String, String>,
}

impl Default for JudgeConfig {
    fn default() -> Self {
        Self {
            provider: default_judge_provider(),
            endpoint: default_judge_endpoint(),
            timeout_ms: default_judge_timeout_ms(),
            fallback_on_error: default_fallback_on_error(),
            points: default_judge_points(),
        }
    }
}

impl JudgeConfig {
    pub fn is_enabled(&self) -> bool {
        self.points
            .values()
            .any(|mode| crate::judge::DecisionMode::from_str_name(mode).is_enabled())
    }

    pub fn points_summary(&self) -> String {
        let mut entries: Vec<String> = self
            .points
            .iter()
            .map(|(point, mode)| {
                let label = match crate::judge::DecisionMode::from_str_name(mode) {
                    crate::judge::DecisionMode::Active => "active",
                    crate::judge::DecisionMode::Shadow => "shadow",
                    crate::judge::DecisionMode::Off => "off",
                };
                format!("{point}={label}")
            })
            .collect();
        entries.sort();
        entries.join(", ")
    }
}

fn default_version() -> String {
    "1.0".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHarnessConfig {
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub advisors: HashMap<String, crate::types::AdvisorProxyConfig>,
    #[serde(default)]
    pub judge: JudgeConfig,
}

impl Default for ProjectHarnessConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            policy: PolicyConfig::default(),
            sandbox: SandboxConfig::default(),
            execution: ExecutionConfig::default(),
            paths: PathsConfig::default(),
            advisors: HashMap::new(),
            judge: JudgeConfig::default(),
        }
    }
}

impl ProjectHarnessConfig {
    pub const CONFIG_FILE_REL_PATH: &'static str = ".goose/harness.yaml";

    pub fn config_path(project_root: &Path) -> PathBuf {
        project_root.join(Self::CONFIG_FILE_REL_PATH)
    }

    pub async fn load_or_default(project_root: &Path) -> Self {
        let path = Self::config_path(project_root);
        let global = crate::types::discover_global_harness_settings(Some(project_root));
        match fs::read_to_string(&path).await {
            Ok(content) => {
                let mut cfg: Self = serde_yaml::from_str(&content).unwrap_or_default();
                if let Ok(raw_val) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                    if raw_val.get("judge").is_none() {
                        if let Some(global_judge) = global.judge {
                            cfg.judge = global_judge;
                        }
                    }
                }
                cfg
            }
            Err(_) => {
                let mut cfg = Self::default();
                if let Some(global_judge) = global.judge {
                    cfg.judge = global_judge;
                }
                cfg
            }
        }
    }

    pub async fn save_to_file(&self, project_root: &Path) -> Result<PathBuf> {
        let path = Self::config_path(project_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }
        let yaml_str = serde_yaml::to_string(self)
            .context("Failed to serialize ProjectHarnessConfig to YAML")?;
        fs::write(&path, yaml_str)
            .await
            .with_context(|| format!("Failed to write harness config to {:?}", path))?;
        Ok(path)
    }

    pub fn resolve_tasks_dir(&self, project_root: &Path) -> PathBuf {
        project_root.join(&self.paths.tasks_dir)
    }

    pub fn resolve_results_dir(&self, project_root: &Path) -> PathBuf {
        project_root.join(&self.paths.results_dir)
    }

    pub fn resolve_cassettes_dir(&self, project_root: &Path) -> PathBuf {
        project_root.join(&self.paths.cassettes_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::JudgeConfig;

    #[test]
    fn judge_is_enabled_when_any_point_is_shadow_or_active() {
        let config = JudgeConfig::default();
        assert!(config.is_enabled());
        assert_eq!(
            config.points_summary(),
            "context.forget=off, tool.risk=active, turn.completion=shadow, turn.drift=shadow"
        );
    }

    #[test]
    fn judge_is_disabled_when_every_point_is_off() {
        let mut config = JudgeConfig::default();
        for mode in config.points.values_mut() {
            *mode = "off".to_string();
        }
        assert!(!config.is_enabled());
        assert_eq!(
            config.points_summary(),
            "context.forget=off, tool.risk=off, turn.completion=off, turn.drift=off"
        );
    }
}
