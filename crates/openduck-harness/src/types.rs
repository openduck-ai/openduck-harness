use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCallResponse {
    pub id: String,
    pub name: String,
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SnapshotId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecOptions {
    pub working_dir: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub timeout_seconds: Option<u64>,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            working_dir: None,
            env: HashMap::new(),
            timeout_seconds: Some(120),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunStatus {
    Success,
    Failure,
    Timeout,
    Cancelled,
    Running,
    Skipped,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdvisorProxyConfig {
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "httpsProxy")]
    pub https_proxy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "httpProxy")]
    pub http_proxy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "allProxy")]
    pub all_proxy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "noProxy")]
    pub no_proxy: Option<String>,
}

impl AdvisorProxyConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_https_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.https_proxy = Some(proxy.into());
        self
    }

    pub fn with_http_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.http_proxy = Some(proxy.into());
        self
    }

    pub fn with_all_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.all_proxy = Some(proxy.into());
        self
    }

    pub fn with_no_proxy(mut self, no_proxy: impl Into<String>) -> Self {
        self.no_proxy = Some(no_proxy.into());
        self
    }

    pub fn is_empty(&self) -> bool {
        self.https_proxy.is_none()
            && self.http_proxy.is_none()
            && self.all_proxy.is_none()
            && self.no_proxy.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdvisorsConfigFile {
    #[serde(default)]
    pub advisors: HashMap<String, AdvisorProxyConfig>,
}

pub fn parse_advisor_configs_str(content: &str) -> HashMap<String, AdvisorProxyConfig> {
    if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(content) {
        if let Some(advisors_val) = val.get("advisors") {
            if let Ok(map) =
                serde_yaml::from_value::<HashMap<String, AdvisorProxyConfig>>(advisors_val.clone())
            {
                return map;
            }
        }
        if let Ok(map) = serde_yaml::from_value::<HashMap<String, AdvisorProxyConfig>>(val) {
            return map;
        }
    }
    HashMap::new()
}

pub fn load_advisor_configs_file(path: &std::path::Path) -> HashMap<String, AdvisorProxyConfig> {
    if let Ok(content) = std::fs::read_to_string(path) {
        parse_advisor_configs_str(&content)
    } else {
        HashMap::new()
    }
}

pub fn standard_advisor_config_paths(workspace_root: Option<&std::path::Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for var_name in &[
        "OPENDUCK_CONFIG_PATH",
        "GOOSE_CONFIG_PATH",
        "OPENDUCK_ADVISOR_CONFIG",
        "ADVISOR_CONFIG_FILE",
    ] {
        if let Ok(p) = std::env::var(var_name) {
            paths.push(PathBuf::from(p));
        }
    }

    if let Some(ws) = workspace_root {
        paths.push(ws.join(".openduck").join("config.yaml"));
        paths.push(ws.join(".goose").join("config.yaml"));
        paths.push(ws.join(".openduck").join("harness.yaml"));
        paths.push(ws.join(".goose").join("harness.yaml"));
        paths.push(ws.join(".openduck").join("advisors.yaml"));
        paths.push(ws.join(".goose").join("advisors.yaml"));
        paths.push(ws.join("advisors.yaml"));
    }

    let home_dir = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from);

    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        let xdg_path = PathBuf::from(xdg);
        paths.push(xdg_path.join("openduck").join("config.yaml"));
        paths.push(xdg_path.join("goose").join("config.yaml"));
        paths.push(xdg_path.join("openduck").join("advisors.yaml"));
        paths.push(xdg_path.join("goose").join("advisors.yaml"));
    }

    if let Some(home) = home_dir {
        paths.push(home.join(".config").join("openduck").join("config.yaml"));
        paths.push(home.join(".config").join("goose").join("config.yaml"));
        paths.push(home.join(".config").join("openduck").join("advisors.yaml"));
        paths.push(home.join(".config").join("goose").join("advisors.yaml"));
        paths.push(home.join(".openduck").join("config.yaml"));
        paths.push(home.join(".goose").join("config.yaml"));
    }

    #[cfg(windows)]
    if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata_path = PathBuf::from(appdata);
        paths.push(
            appdata_path
                .join("OpenDuck")
                .join("config")
                .join("config.yaml"),
        );
        paths.push(
            appdata_path
                .join("goose")
                .join("config")
                .join("config.yaml"),
        );
    }

    paths.push(PathBuf::from("/etc/openduck/config.yaml"));
    paths.push(PathBuf::from("/etc/goose/config.yaml"));

    paths
}

pub fn discover_advisor_configs(
    workspace_root: Option<&std::path::Path>,
) -> HashMap<String, AdvisorProxyConfig> {
    discover_global_harness_settings(workspace_root).advisors
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessGlobalSettings {
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
        alias = "max_turns",
        alias = "maxTurns"
    )]
    pub max_turns: Option<usize>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "advisor_command",
        alias = "advisorCommand"
    )]
    pub advisor_command: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "advisor_timeout_seconds",
        alias = "advisorTimeoutSeconds"
    )]
    pub advisor_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub advisors: HashMap<String, AdvisorProxyConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge: Option<crate::project::JudgeConfig>,
}

pub fn parse_harness_global_settings_str(content: &str) -> HarnessGlobalSettings {
    let mut settings = HarnessGlobalSettings::default();
    if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(content) {
        if let Some(harness_val) = val.get("harness") {
            if let Ok(parsed) = serde_yaml::from_value::<HarnessGlobalSettings>(harness_val.clone())
            {
                settings = parsed;
            }
        }
        if let Some(judge_val) = val.get("judge") {
            if let Ok(parsed_judge) =
                serde_yaml::from_value::<crate::project::JudgeConfig>(judge_val.clone())
            {
                settings.judge = Some(parsed_judge);
            }
        }
        if let Ok(top_parsed) = serde_yaml::from_value::<HarnessGlobalSettings>(val.clone()) {
            if settings.stagnation_threshold.is_none() {
                settings.stagnation_threshold = top_parsed.stagnation_threshold;
            }
            if settings.exploration_budget.is_none() {
                settings.exploration_budget = top_parsed.exploration_budget;
            }
            if settings.circuit_breaker_enabled.is_none() {
                settings.circuit_breaker_enabled = top_parsed.circuit_breaker_enabled;
            }
            if settings.periodic_review_interval.is_none() {
                settings.periodic_review_interval = top_parsed.periodic_review_interval;
            }
            if settings.max_turns.is_none() {
                settings.max_turns = top_parsed.max_turns;
            }
            if settings.advisor_command.is_none() {
                settings.advisor_command = top_parsed.advisor_command;
            }
            if settings.advisor_timeout_seconds.is_none() {
                settings.advisor_timeout_seconds = top_parsed.advisor_timeout_seconds;
            }
            if settings.judge.is_none() {
                settings.judge = top_parsed.judge;
            }
            for (k, v) in top_parsed.advisors {
                settings.advisors.entry(k.to_lowercase()).or_insert(v);
            }
        }
    }
    settings
}

pub fn load_harness_global_settings_file(path: &std::path::Path) -> HarnessGlobalSettings {
    if let Ok(content) = std::fs::read_to_string(path) {
        parse_harness_global_settings_str(&content)
    } else {
        HarnessGlobalSettings::default()
    }
}

pub fn discover_global_harness_settings(
    workspace_root: Option<&std::path::Path>,
) -> HarnessGlobalSettings {
    let mut result = HarnessGlobalSettings::default();
    for path in standard_advisor_config_paths(workspace_root) {
        if path.exists() && path.is_file() {
            let loaded = load_harness_global_settings_file(&path);
            if result.stagnation_threshold.is_none() {
                result.stagnation_threshold = loaded.stagnation_threshold;
            }
            if result.exploration_budget.is_none() {
                result.exploration_budget = loaded.exploration_budget;
            }
            if result.circuit_breaker_enabled.is_none() {
                result.circuit_breaker_enabled = loaded.circuit_breaker_enabled;
            }
            if result.periodic_review_interval.is_none() {
                result.periodic_review_interval = loaded.periodic_review_interval;
            }
            if result.max_turns.is_none() {
                result.max_turns = loaded.max_turns;
            }
            if result.advisor_command.is_none() {
                result.advisor_command = loaded.advisor_command;
            }
            if result.advisor_timeout_seconds.is_none() {
                result.advisor_timeout_seconds = loaded.advisor_timeout_seconds;
            }
            if result.judge.is_none() {
                result.judge = loaded.judge;
            }
            for (k, v) in loaded.advisors {
                result.advisors.entry(k.to_lowercase()).or_insert(v);
            }
        }
    }

    if let Ok(val) = std::env::var("OPENDUCK_CIRCUIT_BREAKER")
        .or_else(|_| std::env::var("HARNESS_CIRCUIT_BREAKER"))
        .or_else(|_| std::env::var("GOOSE_CIRCUIT_BREAKER"))
    {
        let lower = val.trim().to_lowercase();
        if lower == "0" || lower == "false" || lower == "off" || lower == "no" {
            result.circuit_breaker_enabled = Some(false);
        } else if lower == "1" || lower == "true" || lower == "on" || lower == "yes" {
            result.circuit_breaker_enabled = Some(true);
        }
    }

    if let Ok(val) = std::env::var("OPENDUCK_STAGNATION_THRESHOLD")
        .or_else(|_| std::env::var("HARNESS_STAGNATION_THRESHOLD"))
        .or_else(|_| std::env::var("GOOSE_STAGNATION_THRESHOLD"))
    {
        if let Ok(t) = val.trim().parse::<usize>() {
            result.stagnation_threshold = Some(t);
        }
    }

    if let Ok(val) = std::env::var("OPENDUCK_EXPLORATION_BUDGET")
        .or_else(|_| std::env::var("HARNESS_EXPLORATION_BUDGET"))
        .or_else(|_| std::env::var("GOOSE_EXPLORATION_BUDGET"))
    {
        if let Ok(t) = val.trim().parse::<usize>() {
            result.exploration_budget = Some(t);
        }
    }

    if let Ok(val) = std::env::var("OPENDUCK_PERIODIC_REVIEW_INTERVAL")
        .or_else(|_| std::env::var("HARNESS_PERIODIC_REVIEW_INTERVAL"))
        .or_else(|_| std::env::var("GOOSE_PERIODIC_REVIEW_INTERVAL"))
    {
        if let Ok(i) = val.trim().parse::<usize>() {
            result.periodic_review_interval = Some(i);
        }
    }

    result
}
