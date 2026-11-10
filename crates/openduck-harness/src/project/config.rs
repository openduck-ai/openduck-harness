use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectProviderEntry {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "fast_model")]
    pub fast_model: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "secondary_model"
    )]
    pub secondary_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
}

impl Default for ProjectProviderEntry {
    fn default() -> Self {
        Self {
            enabled: true,
            model: None,
            fast_model: None,
            secondary_model: None,
            proxy: None,
        }
    }
}

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
        alias = "secondary_provider"
    )]
    pub secondary_provider: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "secondary_model"
    )]
    pub secondary_model: Option<String>,
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

fn default_auto_commit() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionConfig {
    #[serde(default = "default_auto_commit", alias = "auto_commit")]
    pub auto_commit: bool,
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
            auto_commit: default_auto_commit(),
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub providers: HashMap<String, ProjectProviderEntry>,
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
            providers: HashMap::new(),
        }
    }
}

fn execution_specifies_max_turns(raw: &serde_yaml::Value) -> bool {
    raw.get("execution")
        .and_then(|exec| exec.get("maxTurns").or_else(|| exec.get("max_turns")))
        .is_some()
}

fn apply_global_max_turns(
    mut cfg: ProjectHarnessConfig,
    raw: Option<&serde_yaml::Value>,
    global_max_turns: Option<usize>,
) -> ProjectHarnessConfig {
    let explicit = raw.is_some_and(execution_specifies_max_turns);
    if !explicit {
        if let Some(turns) = global_max_turns.filter(|turns| *turns >= 1) {
            cfg.execution.max_turns = turns;
        }
    }
    cfg
}

fn merge_project_config_yaml_fallbacks(
    mut cfg: ProjectHarnessConfig,
    project_root: &Path,
) -> ProjectHarnessConfig {
    let config_yaml_candidates = [
        project_root.join(".openduck").join("config.yaml"),
        project_root.join(".goose").join("config.yaml"),
    ];
    for cand in config_yaml_candidates {
        if cand.is_file() {
            if let Ok(content) = std::fs::read_to_string(&cand) {
                if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                    if cfg.policy.provider.is_none() {
                        if let Some(p) = val
                            .get("active_provider")
                            .or_else(|| val.get("GOOSE_PROVIDER"))
                            .and_then(|v| v.as_str())
                        {
                            if !p.trim().is_empty() {
                                cfg.policy.provider = Some(p.trim().to_string());
                            }
                        }
                    }
                    if cfg.policy.model.is_none() {
                        if let Some(m) = val
                            .get("active_model")
                            .or_else(|| val.get("GOOSE_MODEL"))
                            .and_then(|v| v.as_str())
                        {
                            if !m.trim().is_empty() {
                                cfg.policy.model = Some(m.trim().to_string());
                            }
                        }
                    }
                    if cfg.policy.secondary_provider.is_none() {
                        if let Some(sp) = val
                            .get("secondary_provider")
                            .or_else(|| val.get("GOOSE_SECONDARY_PROVIDER"))
                            .and_then(|v| v.as_str())
                        {
                            if !sp.trim().is_empty() {
                                cfg.policy.secondary_provider = Some(sp.trim().to_string());
                            }
                        }
                    }
                    if cfg.policy.secondary_model.is_none() {
                        if let Some(sm) = val
                            .get("secondary_provider_model")
                            .or_else(|| val.get("GOOSE_SECONDARY_MODEL"))
                            .and_then(|v| v.as_str())
                        {
                            if !sm.trim().is_empty() {
                                cfg.policy.secondary_model = Some(sm.trim().to_string());
                            }
                        }
                    }
                    if let Some(providers_val) = val.get("providers").and_then(|p| p.as_mapping()) {
                        for (k, v) in providers_val {
                            if let (Some(provider_name), Ok(entry)) = (
                                k.as_str(),
                                serde_yaml::from_value::<ProjectProviderEntry>(v.clone()),
                            ) {
                                cfg.providers
                                    .entry(provider_name.to_string())
                                    .or_insert(entry);
                            }
                        }
                    }
                    break;
                }
            }
        }
    }
    cfg
}

impl ProjectHarnessConfig {
    pub const CONFIG_FILE_REL_PATH: &'static str = ".openduck/harness.yaml";
    pub const LEGACY_CONFIG_FILE_REL_PATH: &'static str = ".goose/harness.yaml";

    pub fn config_path(project_root: &Path) -> PathBuf {
        let openduck_path = project_root.join(Self::CONFIG_FILE_REL_PATH);
        if openduck_path.exists() {
            return openduck_path;
        }
        let goose_path = project_root.join(Self::LEGACY_CONFIG_FILE_REL_PATH);
        if goose_path.exists() {
            return goose_path;
        }
        openduck_path
    }

    pub async fn load_or_default(project_root: &Path) -> Self {
        let path = Self::config_path(project_root);
        let global = crate::types::discover_global_harness_settings(Some(project_root));
        let loaded = match fs::read_to_string(&path).await {
            Ok(content) => {
                let mut cfg: Self = serde_yaml::from_str(&content).unwrap_or_default();
                let raw_val = serde_yaml::from_str::<serde_yaml::Value>(&content).ok();
                if let Some(raw_val) = raw_val.as_ref() {
                    if raw_val.get("judge").is_none() {
                        if let Some(global_judge) = global.judge.clone() {
                            cfg.judge = global_judge;
                        }
                    }
                }
                apply_global_max_turns(cfg, raw_val.as_ref(), global.max_turns)
            }
            Err(_) => {
                let mut cfg = Self::default();
                if let Some(global_judge) = global.judge {
                    cfg.judge = global_judge;
                }
                apply_global_max_turns(cfg, None, global.max_turns)
            }
        };
        merge_project_config_yaml_fallbacks(loaded, project_root)
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
    use super::{apply_global_max_turns, JudgeConfig, ProjectHarnessConfig};
    use serde_yaml::Value;

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

    #[test]
    fn global_max_turns_fills_execution_when_the_project_file_omits_it() {
        let cfg = apply_global_max_turns(ProjectHarnessConfig::default(), None, Some(800));
        assert_eq!(cfg.execution.max_turns, 800);
    }

    #[test]
    fn project_execution_max_turns_overrides_the_global_value() {
        let raw: Value = serde_yaml::from_str("execution:\n  maxTurns: 40\n").unwrap();
        let mut cfg = ProjectHarnessConfig::default();
        cfg.execution.max_turns = 40;
        let merged = apply_global_max_turns(cfg, Some(&raw), Some(800));
        assert_eq!(merged.execution.max_turns, 40);
    }

    #[test]
    fn snake_case_execution_max_turns_overrides_the_global_value() {
        let raw: Value = serde_yaml::from_str("execution:\n  max_turns: 12\n").unwrap();
        let mut cfg = ProjectHarnessConfig::default();
        cfg.execution.max_turns = 12;
        let merged = apply_global_max_turns(cfg, Some(&raw), Some(800));
        assert_eq!(merged.execution.max_turns, 12);
    }

    #[test]
    fn policy_and_providers_round_trip() {
        let yaml = r#"
version: "1.0"
policy:
  provider: openai
  model: gpt-4o
  secondaryProvider: anthropic
  secondaryModel: claude-3-7-sonnet
providers:
  openai:
    enabled: true
    model: gpt-4o
    fastModel: gpt-4o-mini
    proxy: http://127.0.0.1:8080
  anthropic:
    enabled: true
    model: claude-3-7-sonnet
"#;
        let parsed: ProjectHarnessConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.policy.provider.as_deref(), Some("openai"));
        assert_eq!(parsed.policy.model.as_deref(), Some("gpt-4o"));
        assert_eq!(
            parsed.policy.secondary_provider.as_deref(),
            Some("anthropic")
        );
        assert_eq!(
            parsed.policy.secondary_model.as_deref(),
            Some("claude-3-7-sonnet")
        );
        assert_eq!(parsed.providers.len(), 2);
        let openai = &parsed.providers["openai"];
        assert!(openai.enabled);
        assert_eq!(openai.model.as_deref(), Some("gpt-4o"));
        assert_eq!(openai.fast_model.as_deref(), Some("gpt-4o-mini"));
        assert_eq!(openai.proxy.as_deref(), Some("http://127.0.0.1:8080"));

        let serialized = serde_yaml::to_string(&parsed).unwrap();
        let reloaded: ProjectHarnessConfig = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(parsed, reloaded);
    }
}
