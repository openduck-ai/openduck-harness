//! TUI Agent Model Catalog.
//!
//! Provides cached model discovery, dynamic loader integration, static fallback models,
//! and free-text model resolution for interactive agent CLIs.

use crate::tui_session::profiles::{user_home, AgentCliKind};
use crate::tui_session::screen::{
    contains_braille_spinner, is_spinner_activity_line, strip_ansi_codes,
};
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock as TokioRwLock};

/// Origin of a model catalog entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelSource {
    StaticFallback,
    DynamicLoader,
    Custom,
}

/// Metadata describing a single model for an agent CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CatalogModelEntry {
    /// Model identifier passed to the CLI flag (e.g. `gemini-2.5-pro`, `grok-4`).
    pub id: String,
    /// Human-friendly display label (e.g. `Gemini 2.5 Pro`, `Grok 4`).
    pub display_name: String,
    /// Optional short description of capabilities or context window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether this is the default model for the agent CLI.
    #[serde(default)]
    pub is_default: bool,
    /// Where this model entry originated from.
    pub source: ModelSource,
}

/// The complete catalog entry for a specific agent CLI kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentModelCatalogEntry {
    /// Agent CLI kind (agy, grok, codex, claude, custom).
    pub agent: AgentCliKind,
    /// Available models for this agent.
    pub models: Vec<CatalogModelEntry>,
    /// Default model slug if identified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Whether this CLI accepts custom arbitrary model slugs via free-text input.
    pub supports_free_text: bool,
    /// Indicates whether these models are static fallbacks due to missing or failed dynamic loader.
    pub is_fallback: bool,
    /// Unix timestamp in seconds when this entry was cached.
    pub cached_at_unix_secs: u64,
}

/// Dynamic loader trait for querying live agent models (e.g., from CLI or API).
#[async_trait::async_trait]
pub trait ModelLoader: Send + Sync {
    async fn load_models(&self, agent: AgentCliKind) -> anyhow::Result<Vec<CatalogModelEntry>>;
}

#[inline]
const fn agent_key(agent: AgentCliKind) -> &'static str {
    match agent {
        AgentCliKind::Agy => "agy",
        AgentCliKind::Grok => "grok",
        AgentCliKind::Codex => "codex",
        AgentCliKind::Claude => "claude",
        AgentCliKind::Custom => "custom",
    }
}

#[derive(Clone)]
struct CacheRecord {
    entry: AgentModelCatalogEntry,
    fetched_at: Instant,
}

struct AgentModelCatalogInner {
    ttl: Duration,
    loaders: RwLock<HashMap<&'static str, Arc<dyn ModelLoader>>>,
    global_loader: RwLock<Option<Arc<dyn ModelLoader>>>,
    discovery_locks: HashMap<&'static str, Mutex<()>>,
    cache: TokioRwLock<HashMap<&'static str, CacheRecord>>,
}

/// Thread-safe cached catalog managing model availability for interactive agent CLIs.
#[derive(Clone)]
pub struct AgentModelCatalog {
    inner: Arc<AgentModelCatalogInner>,
}

impl Default for AgentModelCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentModelCatalog {
    /// Creates a new model catalog with a default 5-minute cache TTL.
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(300))
    }

    /// Creates a new model catalog with a custom cache TTL.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(AgentModelCatalogInner {
                ttl,
                loaders: RwLock::new(HashMap::new()),
                global_loader: RwLock::new(None),
                discovery_locks: ["agy", "grok", "codex", "claude", "custom"]
                    .into_iter()
                    .map(|key| (key, Mutex::new(())))
                    .collect(),
                cache: TokioRwLock::new(HashMap::new()),
            }),
        }
    }

    /// Creates a new model catalog preconfigured with production CLI dynamic discovery loaders
    /// for supported interactive agent CLIs (`agy models`, `grok models`) and default 5-minute TTL.
    pub fn with_cli_discovery() -> Self {
        Self::with_cli_discovery_and_ttl(Duration::from_secs(300), CliModelLoader::DEFAULT_TIMEOUT)
    }

    /// Creates a new model catalog with production CLI discovery and custom cache/command timeouts.
    pub fn with_cli_discovery_and_ttl(ttl: Duration, command_timeout: Duration) -> Self {
        let catalog = Self::with_ttl(ttl);
        let loader = Arc::new(CliModelLoader::with_timeout(command_timeout));
        if let Ok(mut loaders) = catalog.inner.loaders.write() {
            loaders.insert(agent_key(AgentCliKind::Agy), loader.clone());
            loaders.insert(agent_key(AgentCliKind::Grok), loader);
        }
        catalog
    }

    /// Default discovery factory creating an [`AgentModelCatalog`] with CLI discovery configured.
    pub fn default_discovery() -> Self {
        Self::with_cli_discovery()
    }

    /// Alias for [`with_cli_discovery`].
    pub fn with_production_discovery() -> Self {
        Self::with_cli_discovery()
    }

    /// Registers a dedicated dynamic model loader for a specific agent CLI kind.
    pub async fn set_loader(&self, agent: AgentCliKind, loader: Arc<dyn ModelLoader>) {
        if let Ok(mut loaders) = self.inner.loaders.write() {
            loaders.insert(agent_key(agent), loader);
        }
    }

    /// Registers a fallback dynamic model loader used for all agents without a dedicated loader.
    pub async fn set_global_loader(&self, loader: Arc<dyn ModelLoader>) {
        if let Ok(mut global) = self.inner.global_loader.write() {
            *global = Some(loader);
        }
    }

    /// Returns static fallback models for an agent CLI when dynamic discovery is unavailable.
    pub fn static_fallback_models(agent: AgentCliKind) -> Vec<CatalogModelEntry> {
        match agent {
            AgentCliKind::Agy => vec![
                CatalogModelEntry {
                    id: "gemini-2.5-pro".to_string(),
                    display_name: "Gemini 2.5 Pro".to_string(),
                    description: Some(
                        "Google flagship balanced reasoning & coding model".to_string(),
                    ),
                    is_default: true,
                    source: ModelSource::StaticFallback,
                },
                CatalogModelEntry {
                    id: "gemini-2.5-flash".to_string(),
                    display_name: "Gemini 2.5 Flash".to_string(),
                    description: Some("Google fast lightweight multimodal model".to_string()),
                    is_default: false,
                    source: ModelSource::StaticFallback,
                },
                CatalogModelEntry {
                    id: "gemini-2.0-flash".to_string(),
                    display_name: "Gemini 2.0 Flash".to_string(),
                    description: Some("Google next-gen low-latency model".to_string()),
                    is_default: false,
                    source: ModelSource::StaticFallback,
                },
            ],
            AgentCliKind::Grok => vec![
                CatalogModelEntry {
                    id: "grok-4".to_string(),
                    display_name: "Grok 4".to_string(),
                    description: Some("xAI flagship reasoning model".to_string()),
                    is_default: true,
                    source: ModelSource::StaticFallback,
                },
                CatalogModelEntry {
                    id: "grok-3".to_string(),
                    display_name: "Grok 3".to_string(),
                    description: Some("xAI frontier model with deep reasoning".to_string()),
                    is_default: false,
                    source: ModelSource::StaticFallback,
                },
                CatalogModelEntry {
                    id: "grok-3-mini".to_string(),
                    display_name: "Grok 3 Mini".to_string(),
                    description: Some("xAI lightweight fast reasoning model".to_string()),
                    is_default: false,
                    source: ModelSource::StaticFallback,
                },
            ],
            AgentCliKind::Codex => vec![
                CatalogModelEntry {
                    id: "gpt-5".to_string(),
                    display_name: "GPT-5".to_string(),
                    description: Some("OpenAI frontier flagship coding model".to_string()),
                    is_default: true,
                    source: ModelSource::StaticFallback,
                },
                CatalogModelEntry {
                    id: "gpt-4o".to_string(),
                    display_name: "GPT-4o".to_string(),
                    description: Some("OpenAI omni multimodal model".to_string()),
                    is_default: false,
                    source: ModelSource::StaticFallback,
                },
                CatalogModelEntry {
                    id: "gpt-4o-mini".to_string(),
                    display_name: "GPT-4o Mini".to_string(),
                    description: Some("OpenAI fast affordable model".to_string()),
                    is_default: false,
                    source: ModelSource::StaticFallback,
                },
                CatalogModelEntry {
                    id: "o3-mini".to_string(),
                    display_name: "o3-mini".to_string(),
                    description: Some("OpenAI high-speed reasoning model".to_string()),
                    is_default: false,
                    source: ModelSource::StaticFallback,
                },
            ],
            AgentCliKind::Claude => vec![
                CatalogModelEntry {
                    id: "claude-3-7-sonnet".to_string(),
                    display_name: "Claude 3.7 Sonnet".to_string(),
                    description: Some("Anthropic hybrid reasoning & coding model".to_string()),
                    is_default: true,
                    source: ModelSource::StaticFallback,
                },
                CatalogModelEntry {
                    id: "claude-3-5-sonnet".to_string(),
                    display_name: "Claude 3.5 Sonnet".to_string(),
                    description: Some("Anthropic high-capability coding model".to_string()),
                    is_default: false,
                    source: ModelSource::StaticFallback,
                },
                CatalogModelEntry {
                    id: "claude-3-5-haiku".to_string(),
                    display_name: "Claude 3.5 Haiku".to_string(),
                    description: Some("Anthropic fastest lightweight model".to_string()),
                    is_default: false,
                    source: ModelSource::StaticFallback,
                },
                CatalogModelEntry {
                    id: "claude-3-opus".to_string(),
                    display_name: "Claude 3 Opus".to_string(),
                    description: Some("Anthropic high-intelligence reasoning model".to_string()),
                    is_default: false,
                    source: ModelSource::StaticFallback,
                },
            ],
            AgentCliKind::Custom => vec![CatalogModelEntry {
                id: "default".to_string(),
                display_name: "Default CLI Model".to_string(),
                description: Some("Uses the underlying custom CLI default model".to_string()),
                is_default: true,
                source: ModelSource::StaticFallback,
            }],
        }
    }

    /// Returns the static default model slug for an agent CLI kind.
    pub fn default_model_for_agent(agent: AgentCliKind) -> &'static str {
        match agent {
            AgentCliKind::Agy => "gemini-2.5-pro",
            AgentCliKind::Grok => "grok-4",
            AgentCliKind::Codex => "gpt-5",
            AgentCliKind::Claude => "claude-3-7-sonnet",
            AgentCliKind::Custom => "default",
        }
    }

    /// Returns whether the agent CLI accepts free-text custom model identifiers.
    pub fn supports_free_text(agent: AgentCliKind) -> bool {
        match agent {
            AgentCliKind::Agy
            | AgentCliKind::Grok
            | AgentCliKind::Codex
            | AgentCliKind::Claude
            | AgentCliKind::Custom => true,
        }
    }

    /// Builds a static fallback catalog entry for the agent.
    fn create_fallback_entry(agent: AgentCliKind) -> AgentModelCatalogEntry {
        AgentModelCatalogEntry {
            agent,
            models: Self::static_fallback_models(agent),
            default_model: Some(Self::default_model_for_agent(agent).to_string()),
            supports_free_text: Self::supports_free_text(agent),
            is_fallback: true,
            cached_at_unix_secs: current_unix_secs(),
        }
    }

    /// Retrieves configured loader for an agent CLI kind, checking dedicated first then global.
    fn get_loader_for(&self, agent: AgentCliKind) -> Option<Arc<dyn ModelLoader>> {
        if let Ok(loaders) = self.inner.loaders.read() {
            if let Some(loader) = loaders.get(agent_key(agent)) {
                return Some(loader.clone());
            }
        }
        if let Ok(global) = self.inner.global_loader.read() {
            if let Some(ref loader) = *global {
                return Some(loader.clone());
            }
        }
        None
    }

    /// Retrieves models for an agent CLI, using cached values until the TTL expires.
    pub async fn get_models(&self, agent: AgentCliKind) -> AgentModelCatalogEntry {
        self.load(agent, false).await
    }

    /// Re-runs discovery, preserving the last catalog if discovery fails or returns no models.
    pub async fn refresh(&self, agent: AgentCliKind) -> AgentModelCatalogEntry {
        self.load(agent, true).await
    }

    async fn fresh_entry(&self, key: &str) -> Option<AgentModelCatalogEntry> {
        self.inner.cache.read().await.get(key).and_then(|cached| {
            (cached.fetched_at.elapsed() < self.inner.ttl).then(|| cached.entry.clone())
        })
    }

    async fn load(&self, agent: AgentCliKind, refresh: bool) -> AgentModelCatalogEntry {
        let key = agent_key(agent);
        if !refresh {
            if let Some(entry) = self.fresh_entry(key).await {
                return entry;
            }
        }

        // Serialize discovery per agent without blocking cached reads or other agents.
        let _discovery = self.inner.discovery_locks[key].lock().await;
        if !refresh {
            if let Some(entry) = self.fresh_entry(key).await {
                return entry;
            }
        }

        let discovered = match self.get_loader_for(agent) {
            Some(loader) => match loader.load_models(agent).await {
                Ok(models) => models,
                Err(err) => {
                    tracing::warn!("Model discovery failed for agent {agent}: {err}");
                    Vec::new()
                }
            },
            None => Vec::new(),
        };
        let mut cache = self.inner.cache.write().await;
        let entry = if discovered.is_empty() {
            cache
                .get(key)
                .map(|cached| cached.entry.clone())
                .unwrap_or_else(|| Self::create_fallback_entry(agent))
        } else {
            let default_model = discovered
                .iter()
                .find(|model| model.is_default)
                .or_else(|| discovered.first())
                .map(|model| model.id.clone());
            AgentModelCatalogEntry {
                agent,
                models: discovered,
                default_model,
                supports_free_text: Self::supports_free_text(agent),
                is_fallback: false,
                cached_at_unix_secs: current_unix_secs(),
            }
        };
        cache.insert(
            key,
            CacheRecord {
                entry: entry.clone(),
                fetched_at: Instant::now(),
            },
        );
        entry
    }

    /// Resolves an optional requested model string to a valid model ID for the agent CLI.
    pub async fn resolve_model(&self, agent: AgentCliKind, requested: Option<&str>) -> String {
        let trimmed = requested.map(str::trim).unwrap_or("");
        if trimmed.is_empty() {
            return self
                .get_models(agent)
                .await
                .default_model
                .unwrap_or_else(|| Self::default_model_for_agent(agent).to_string());
        }

        let entry = self.get_models(agent).await;
        if let Some(matched) = entry.models.iter().find(|m| {
            m.id.eq_ignore_ascii_case(trimmed) || m.display_name.eq_ignore_ascii_case(trimmed)
        }) {
            return matched.id.clone();
        }

        if entry.supports_free_text {
            trimmed.to_string()
        } else {
            entry
                .default_model
                .unwrap_or_else(|| Self::default_model_for_agent(agent).to_string())
        }
    }
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Production dynamic model loader using CLI commands (`agy models`, `grok models`) with safe timeouts and output parsing.
#[derive(Debug, Clone)]
pub struct CliModelLoader {
    timeout: Duration,
}

impl Default for CliModelLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl CliModelLoader {
    /// Default execution timeout when running CLI model list commands (5 seconds).
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

    /// Creates a new CLI model loader with the default 5-second timeout.
    pub fn new() -> Self {
        Self::with_timeout(Self::DEFAULT_TIMEOUT)
    }

    /// Creates a new CLI model loader with a custom timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Default discovery factory returning an `Arc<dyn ModelLoader>`.
    pub fn default_factory() -> Arc<dyn ModelLoader> {
        Arc::new(Self::new())
    }

    /// Parses `agy models` non-interactive output into structured catalog entries.
    ///
    /// Live `agy models` writes `id\\tDisplay Name` rows on stdout and a
    /// "Fetching available models..." status line on stderr.
    pub fn parse_agy_output(output: &str) -> Vec<CatalogModelEntry> {
        let stripped = strip_ansi_codes(output);
        if is_cli_help_output(&stripped) {
            return Vec::new();
        }
        if let Some(models) = parse_json_models(&stripped) {
            return finalize_discovered_models(models, Some("gemini-2.5-pro"));
        }

        let mut models = Vec::new();
        for raw_line in stripped.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || is_noise_line(trimmed) || is_cli_flag_line(trimmed) {
                continue;
            }

            let item = strip_list_prefix(trimmed);
            if item.is_empty() || is_cli_flag_line(item) {
                continue;
            }

            let Some((id, display_name, tagged_default)) = split_model_id_and_name(item) else {
                continue;
            };
            let is_default = tagged_default || id == "gemini-2.5-pro";
            push_discovered_model(&mut models, id, display_name, is_default);
        }

        finalize_discovered_models(models, Some("gemini-2.5-pro"))
    }

    /// Parses `grok models` non-interactive output into structured catalog entries.
    ///
    /// Live `grok models` prints a login banner, `Default model: <slug>`, then
    /// an `Available models:` list with `-` items and `* <slug> (default)`.
    pub fn parse_grok_output(output: &str) -> Vec<CatalogModelEntry> {
        let stripped = strip_ansi_codes(output);
        if is_cli_help_output(&stripped) {
            return Vec::new();
        }
        if let Some(models) = parse_json_models(&stripped) {
            return finalize_discovered_models(models, None);
        }

        let mut default_slug: Option<String> = None;
        let mut in_available_list = false;
        let mut models = Vec::new();

        for raw_line in stripped.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let lower = trimmed.to_ascii_lowercase();
            if let Some(rest) = lower
                .strip_prefix("default model:")
                .or_else(|| lower.strip_prefix("default:"))
            {
                let slug = rest.trim().trim_matches('"').trim_matches('`');
                if looks_like_model_id(slug) {
                    default_slug = Some(slug.to_string());
                }
                continue;
            }
            if lower.starts_with("available models") {
                in_available_list = true;
                continue;
            }
            if is_noise_line(trimmed) || is_cli_flag_line(trimmed) {
                continue;
            }

            let has_list_marker = starts_with_list_marker(trimmed);
            if !has_list_marker && !in_available_list {
                continue;
            }

            let item = strip_list_prefix(trimmed);
            if item.is_empty() || is_cli_flag_line(item) {
                continue;
            }
            let Some((id, display_name, tagged_default)) = split_model_id_and_name(item) else {
                continue;
            };
            let is_star = trimmed.starts_with('*');
            let is_default = is_star
                || tagged_default
                || default_slug
                    .as_deref()
                    .is_some_and(|slug| slug.eq_ignore_ascii_case(&id));
            push_discovered_model(&mut models, id, display_name, is_default);
        }

        finalize_discovered_models(models, default_slug.as_deref())
    }

    #[cfg(test)]
    async fn run_command(
        &self,
        agent: AgentCliKind,
        default_cmd: &str,
        args: &[&str],
    ) -> anyhow::Result<String> {
        let captured = self.capture_command(agent, default_cmd, args).await?;
        if !captured.success {
            anyhow::bail!(
                "`{default_cmd}` exited with status {}: {}",
                captured.status,
                captured.stderr
            );
        }
        Ok(captured.stdout)
    }

    async fn capture_command(
        &self,
        agent: AgentCliKind,
        default_cmd: &str,
        args: &[&str],
    ) -> anyhow::Result<CapturedCommand> {
        let bin = resolve_cli_bin(agent).unwrap_or_else(|| PathBuf::from(default_cmd));
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(args);
        cmd.stdin(std::process::Stdio::null());
        cmd.kill_on_drop(true);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.env("TERM", "dumb");
        cmd.env("NO_COLOR", "1");
        cmd.env("CI", "1");
        cmd.env("CLICOLOR", "0");
        cmd.env("FORCE_COLOR", "0");
        cmd.env("PAGER", "cat");

        let child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn `{default_cmd}`: {e}"))?;

        let output = match tokio::time::timeout(self.timeout, child.wait_with_output()).await {
            Ok(res) => res.map_err(|e| anyhow::anyhow!("`{default_cmd}` failed: {e}"))?,
            Err(_) => {
                anyhow::bail!(
                    "timed out after {:?} waiting for `{default_cmd}`",
                    self.timeout
                )
            }
        };

        Ok(CapturedCommand {
            success: output.status.success(),
            status: output.status.to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    async fn load_cli_models(
        &self,
        agent: AgentCliKind,
        default_cmd: &str,
        parse: fn(&str) -> Vec<CatalogModelEntry>,
    ) -> anyhow::Result<Vec<CatalogModelEntry>> {
        let captured = self
            .capture_command(agent, default_cmd, &["models"])
            .await?;
        let combined = combine_command_output(&captured.stdout, &captured.stderr);
        let models = parse(&combined);
        if models.is_empty() {
            if captured.success {
                anyhow::bail!("no models found in `{default_cmd} models` output");
            }
            anyhow::bail!(
                "`{default_cmd}` exited with status {}: {}",
                captured.status,
                captured.stderr
            );
        }
        Ok(models)
    }
}

struct CapturedCommand {
    success: bool,
    status: String,
    stdout: String,
    stderr: String,
}

#[async_trait::async_trait]
impl ModelLoader for CliModelLoader {
    async fn load_models(&self, agent: AgentCliKind) -> anyhow::Result<Vec<CatalogModelEntry>> {
        match agent {
            AgentCliKind::Agy => {
                self.load_cli_models(AgentCliKind::Agy, "agy", Self::parse_agy_output)
                    .await
            }
            AgentCliKind::Grok => {
                self.load_cli_models(AgentCliKind::Grok, "grok", Self::parse_grok_output)
                    .await
            }
            _ => anyhow::bail!("CLI model discovery not supported for agent: {agent}"),
        }
    }
}

fn resolve_cli_bin(agent: AgentCliKind) -> Option<PathBuf> {
    match agent {
        AgentCliKind::Agy => {
            if let Ok(explicit) = std::env::var("AGY_COMMAND") {
                let p = PathBuf::from(explicit);
                if p.is_file() {
                    return Some(p);
                }
            }
            if let Ok(p) = which::which("agy") {
                return Some(p);
            }
            if let Some(home) = user_home() {
                let p = home.join(".local/bin/agy");
                if p.is_file() {
                    return Some(p);
                }
            }
            if let Some(p) =
                std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("agy/bin/agy.exe"))
            {
                if p.is_file() {
                    return Some(p);
                }
            }
            None
        }
        AgentCliKind::Grok => {
            if let Ok(explicit) = std::env::var("GROK_COMMAND") {
                let p = PathBuf::from(explicit);
                if p.is_file() {
                    return Some(p);
                }
            }
            if let Ok(p) = which::which("grok") {
                return Some(p);
            }
            if let Some(home) = user_home() {
                for cand in [home.join(".local/bin/grok"), home.join(".grok/bin/grok")] {
                    if cand.is_file() {
                        return Some(cand);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn combine_command_output(stdout: &str, stderr: &str) -> String {
    match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (true, true) => String::new(),
        (false, false) => format!("{stderr}\n{stdout}"),
    }
}

fn is_cli_help_output(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("usage: agy")
        || lower.contains("usage: grok")
        || (lower.contains("list available models and exit")
            && (lower.contains("--help") || lower.contains("options:")))
}

fn is_noise_line(trimmed: &str) -> bool {
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("fetching")
        || lower.starts_with("available models")
        || lower.starts_with("usage:")
        || lower.starts_with("flags:")
        || lower.starts_with("options:")
        || lower.starts_with("list available")
        || lower.starts_with("show help")
        || lower.starts_with("print help")
        || lower.contains("show help")
        || lower.starts_with("you are logged in")
        || lower.starts_with("finishing session")
        || lower.starts_with("error:")
        || lower.starts_with("please log in")
        || contains_braille_spinner(trimmed)
        || is_spinner_activity_line(trimmed)
}

fn is_cli_flag_line(trimmed: &str) -> bool {
    let stripped = trimmed.trim_start_matches(['*', '•', '·', '●', '+']).trim();
    stripped.starts_with("--")
        || matches!(
            stripped.as_bytes(),
            [b'-', b, ..] if b.is_ascii_alphabetic()
        )
}

fn starts_with_list_marker(trimmed: &str) -> bool {
    matches!(
        trimmed.chars().next(),
        Some('*' | '•' | '·' | '●' | '+' | '-' | '–' | '—')
    )
}

fn strip_list_prefix(trimmed: &str) -> &str {
    if trimmed.starts_with("--") {
        return trimmed;
    }
    let rest = trimmed.trim_start_matches(['*', '•', '·', '●', '+', '–', '—', ' ']);
    if let Some(after_dash) = rest.strip_prefix('-') {
        if after_dash.starts_with([' ', '\t']) || looks_like_model_id(after_dash.trim()) {
            return after_dash.trim();
        }
        return rest.trim();
    }
    rest.trim()
}

fn looks_like_model_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 128 || id.starts_with('-') {
        return false;
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':'))
    {
        return false;
    }
    id.contains(['-', '.', '/', ':', '_'])
}

fn split_model_id_and_name(item: &str) -> Option<(String, String, bool)> {
    let tagged_default = item.to_ascii_lowercase().contains("(default)");
    let clean = item
        .replace("(default)", "")
        .replace("(Default)", "")
        .trim()
        .to_string();
    if clean.is_empty() {
        return None;
    }

    let (id_raw, name_raw) = if clean.contains('\t') {
        let mut parts = clean.splitn(2, '\t');
        (
            parts.next().unwrap_or("").trim().to_string(),
            parts.next().unwrap_or("").trim().to_string(),
        )
    } else if let Some((left, right)) = clean.split_once('|') {
        (left.trim().to_string(), right.trim().to_string())
    } else {
        let mut parts = clean.split_whitespace();
        let id = parts.next().unwrap_or("").to_string();
        let rest = parts.collect::<Vec<_>>().join(" ");
        (id, rest)
    };

    if !looks_like_model_id(&id_raw) {
        return None;
    }
    let display_name = if name_raw.is_empty() {
        format_display_name(&id_raw)
    } else {
        name_raw
    };
    Some((id_raw, display_name, tagged_default))
}

fn push_discovered_model(
    models: &mut Vec<CatalogModelEntry>,
    id: String,
    display_name: String,
    is_default: bool,
) {
    if let Some(existing) = models
        .iter_mut()
        .find(|model| model.id.eq_ignore_ascii_case(&id))
    {
        if existing.display_name == format_display_name(&existing.id) && display_name != existing.id
        {
            existing.display_name = display_name;
        }
        existing.is_default |= is_default;
        return;
    }
    models.push(CatalogModelEntry {
        id,
        display_name,
        description: None,
        is_default,
        source: ModelSource::DynamicLoader,
    });
}

fn finalize_discovered_models(
    mut models: Vec<CatalogModelEntry>,
    preferred_default: Option<&str>,
) -> Vec<CatalogModelEntry> {
    if let Some(preferred) = preferred_default {
        if !models.iter().any(|model| model.is_default) {
            if let Some(matched) = models
                .iter_mut()
                .find(|model| model.id.eq_ignore_ascii_case(preferred))
            {
                matched.is_default = true;
            }
        }
    }

    if !models.iter().any(|model| model.is_default) {
        if let Some(first) = models.first_mut() {
            first.is_default = true;
        }
    } else {
        let mut seen = false;
        for model in &mut models {
            if model.is_default {
                if seen {
                    model.is_default = false;
                } else {
                    seen = true;
                }
            }
        }
    }
    models
}

fn parse_json_models(output: &str) -> Option<Vec<CatalogModelEntry>> {
    let trimmed = output.trim();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let mut models = Vec::new();
    collect_json_models(&value, &mut models);
    if models.is_empty() {
        return Some(Vec::new());
    }
    Some(models)
}

fn collect_json_models(value: &serde_json::Value, models: &mut Vec<CatalogModelEntry>) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_json_models(item, models);
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(nested) = map
                .get("models")
                .or_else(|| map.get("data"))
                .or_else(|| map.get("items"))
            {
                collect_json_models(nested, models);
            }
            let id = map
                .get("id")
                .or_else(|| map.get("model"))
                .or_else(|| map.get("slug"))
                .or_else(|| map.get("name"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| looks_like_model_id(s));
            if let Some(id) = id {
                let display_name = map
                    .get("display_name")
                    .or_else(|| map.get("displayName"))
                    .or_else(|| map.get("label"))
                    .or_else(|| map.get("title"))
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format_display_name(id));
                let is_default = map
                    .get("default")
                    .or_else(|| map.get("is_default"))
                    .or_else(|| map.get("isDefault"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                push_discovered_model(models, id.to_string(), display_name, is_default);
            }
        }
        serde_json::Value::String(s) => {
            let id = s.trim();
            if looks_like_model_id(id) {
                push_discovered_model(models, id.to_string(), format_display_name(id), false);
            }
        }
        _ => {}
    }
}

fn format_display_name(id: &str) -> String {
    let parts: Vec<String> = id
        .split(['-', '_'])
        .filter(|p| !p.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut s = first.to_uppercase().to_string();
                    s.push_str(chars.as_str());
                    s
                }
                None => String::new(),
            }
        })
        .collect();

    if parts.is_empty() {
        id.to_string()
    } else {
        parts.join(" ")
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn discovery_command_has_no_interactive_stdin() {
        let output = CliModelLoader::new()
            .run_command(
                AgentCliKind::Custom,
                "/bin/sh",
                &["-c", "read value || printf closed"],
            )
            .await
            .unwrap();
        assert_eq!(output, "closed");
    }

    #[tokio::test]
    async fn discovery_command_rejects_unsuccessful_exit() {
        let error = CliModelLoader::new()
            .run_command(
                AgentCliKind::Custom,
                "/bin/sh",
                &["-c", "printf unavailable >&2; exit 7"],
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("unavailable"));
        assert!(error.to_string().contains("7"));
    }

    #[tokio::test]
    async fn discovery_command_timeout_kills_child() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("pid");
        let script = "echo $$ > \"$1\"; exec sleep 30";
        let result = CliModelLoader::with_timeout(Duration::from_millis(200))
            .run_command(
                AgentCliKind::Custom,
                "/bin/sh",
                &["-c", script, "discovery", pid_path.to_str().unwrap()],
            )
            .await;
        assert!(result.unwrap_err().to_string().contains("timed out"));
        let pid: i32 = std::fs::read_to_string(pid_path)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while tokio::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(std::process::Stdio::null())
                .status()
                .await
                .unwrap()
                .success()
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("timed out discovery child should be killed and reaped");
    }
}
