pub mod agy;
pub mod claude;
pub mod codex;
pub mod generic;
pub mod grok;

use crate::tui_session::backend::MultiplexerBackend;
use crate::tui_session::screen::{
    contains_braille_spinner, is_horizontal_rule_line, is_spinner_activity_line, strip_ansi_codes,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use agy::AgyTuiProfile;
pub use claude::ClaudeTuiProfile;
pub use codex::CodexTuiProfile;
pub use generic::GenericTuiProfile;
pub use grok::GrokTuiProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentCliKind {
    Agy,
    Grok,
    Codex,
    Claude,
    Custom,
}

impl fmt::Display for AgentCliKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentCliKind::Agy => write!(f, "agy"),
            AgentCliKind::Grok => write!(f, "grok"),
            AgentCliKind::Codex => write!(f, "codex"),
            AgentCliKind::Claude => write!(f, "claude"),
            AgentCliKind::Custom => write!(f, "custom"),
        }
    }
}

impl FromStr for AgentCliKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "agy" | "antigravity" => Ok(AgentCliKind::Agy),
            "grok" | "grok-build" => Ok(AgentCliKind::Grok),
            "codex" | "openai-codex" => Ok(AgentCliKind::Codex),
            "claude" | "claude-code" => Ok(AgentCliKind::Claude),
            "custom" | "generic" => Ok(AgentCliKind::Custom),
            other => Err(anyhow!(
                "Unsupported agent CLI kind '{other}'. Expected one of: agy, grok, codex, claude, custom"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TuiPaneState {
    Initializing,
    IdlePrompt,
    Working {
        #[serde(skip_serializing_if = "Option::is_none")]
        status_line: Option<String>,
    },
    WaitingApproval {
        prompt_snippet: String,
    },
    QuotaExceeded {
        message: String,
    },
    Exited {
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
}

impl TuiPaneState {
    pub fn is_idle(&self) -> bool {
        matches!(self, TuiPaneState::IdlePrompt)
    }

    pub fn is_working(&self) -> bool {
        matches!(self, TuiPaneState::Working { .. })
    }

    pub fn is_quota_exceeded(&self) -> bool {
        matches!(self, TuiPaneState::QuotaExceeded { .. })
    }

    pub fn label(&self) -> &'static str {
        match self {
            TuiPaneState::Initializing => "initializing",
            TuiPaneState::IdlePrompt => "idle_prompt",
            TuiPaneState::Working { .. } => "working",
            TuiPaneState::WaitingApproval { .. } => "waiting_approval",
            TuiPaneState::QuotaExceeded { .. } => "quota_exceeded",
            TuiPaneState::Exited { .. } => "exited",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiSessionConfig {
    pub session_id: String,
    pub kind: AgentCliKind,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub auto_approve: bool,
    pub resume_id: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub extra_args: Vec<String>,
    #[serde(default)]
    pub https_proxy: Option<String>,
    pub env: Vec<(String, String)>,
    #[serde(default)]
    pub idle_timeout_mins: Option<u64>,
    #[serde(default)]
    pub exited_retention_mins: Option<u64>,
}

impl Default for TuiSessionConfig {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            kind: AgentCliKind::Agy,
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            model: None,
            effort: None,
            auto_approve: true,
            resume_id: None,
            cols: 160,
            rows: 48,
            extra_args: Vec::new(),
            https_proxy: None,
            env: Vec::new(),
            idle_timeout_mins: None,
            exited_retention_mins: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TuiTurnResult {
    pub session_id: String,
    pub agent: AgentCliKind,
    pub state: TuiPaneState,
    pub response: String,
    pub screen_snapshot: String,
    pub extraction_source: String,
    pub duration_ms: u64,
    pub attach_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TuiSessionSummary {
    pub session_id: String,
    pub kind: AgentCliKind,
    pub pid: Option<u32>,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_model: Option<String>,
    pub state: TuiPaneState,
    pub attach_command: String,
    pub created_at_unix_secs: u64,
    #[serde(default)]
    pub last_activity_unix_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exited_at_unix_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_preview: Option<String>,
}

#[async_trait]
pub trait AgentTuiProfile: Send + Sync {
    fn kind(&self) -> AgentCliKind;
    fn build_launch_command(&self, config: &TuiSessionConfig) -> Result<Vec<String>>;
    fn classify_screen(&self, screen_text: &str) -> TuiPaneState;
    fn strip_chrome(&self, captured_scrollback: &str, submitted_prompt: &str) -> String;
    fn composer_text(&self, _screen_text: &str) -> Option<String> {
        None
    }
    fn current_transcript_offset(&self, _cwd: &Path, _pid: u32, _resume_id: Option<&str>) -> u64 {
        0
    }
    fn read_transcript_delta(
        &self,
        _cwd: &Path,
        _pid: u32,
        _resume_id: Option<&str>,
        _since_offset: &mut u64,
    ) -> Option<String> {
        None
    }
    fn supports_runtime_model_switch(&self) -> bool {
        false
    }
    async fn switch_runtime_model(
        &self,
        _backend: &dyn MultiplexerBackend,
        _session_name: &str,
        _new_model: &str,
    ) -> Result<String> {
        Err(anyhow!(
            "Live process for agent '{}' does not support runtime model switching without restart",
            self.kind()
        ))
    }
}

pub(crate) fn is_safe_model_token(model: &str) -> bool {
    let trimmed = model.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return false;
    }
    if trimmed.starts_with('/') || trimmed.starts_with('-') {
        return false;
    }
    !trimmed.chars().any(|c| {
        c.is_control()
            || matches!(
                c,
                ';' | '|' | '&' | '`' | '$' | '\n' | '\r' | '\0' | '"' | '\'' | '\\'
            )
    })
}

fn model_switch_error(screen: &str) -> Option<String> {
    let clean = strip_ansi_codes(screen);
    let lower = clean.to_ascii_lowercase();
    const PATTERNS: &[&str] = &[
        "unknown model",
        "unrecognized model",
        "invalid model",
        "model not found",
        "not a valid model",
        "usage: /model",
        "exited /model",
    ];
    for pat in PATTERNS {
        if lower.contains(pat) {
            let snippet = clean
                .lines()
                .map(str::trim)
                .find(|line| line.to_ascii_lowercase().contains(pat))
                .unwrap_or(*pat);
            return Some(snippet.to_string());
        }
    }
    None
}

fn line_is_command_echo(line: &str) -> bool {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    trimmed.starts_with('>')
        || trimmed.starts_with('❯')
        || trimmed.starts_with('›')
        || lower.starts_with("/model")
        || lower.starts_with("/m ")
        || lower.contains("mock response")
}

fn is_model_token_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'/' | b':')
}

fn contains_model_token(haystack: &str, token: &str) -> bool {
    let Some(idx) = haystack.find(token) else {
        return false;
    };
    let before_ok = idx == 0
        || haystack
            .as_bytes()
            .get(idx - 1)
            .is_none_or(|c| !is_model_token_char(*c));
    let end = idx + token.len();
    let after_ok = end == haystack.len()
        || haystack
            .as_bytes()
            .get(end)
            .is_none_or(|c| !is_model_token_char(*c));
    before_ok && after_ok
}

fn model_token_matches(haystack: &str, model: &str) -> bool {
    let haystack = haystack.to_ascii_lowercase();
    let model = model.trim().to_ascii_lowercase();
    if model.is_empty() {
        return false;
    }
    if contains_model_token(&haystack, &model) {
        return true;
    }
    let spaced = model.replace(['-', '_'], " ");
    spaced != model && !spaced.trim().is_empty() && contains_model_token(&haystack, &spaced)
}

pub(crate) fn screen_confirms_model(screen: &str, model: &str) -> bool {
    if model_switch_error(screen).is_some() {
        return false;
    }
    let clean = strip_ansi_codes(screen);
    for line in clean.lines().rev().take(16) {
        let trimmed = line.trim();
        if trimmed.is_empty() || line_is_command_echo(trimmed) {
            continue;
        }
        if model_token_matches(trimmed, model) {
            return true;
        }
    }
    false
}

pub(crate) async fn switch_via_slash_command(
    backend: &dyn MultiplexerBackend,
    session_name: &str,
    new_model: &str,
) -> Result<String> {
    let command = format!("/model {new_model}");
    backend.paste_text(session_name, &command, true).await?;
    tokio::time::sleep(Duration::from_millis(80)).await;
    backend
        .send_keys(session_name, &["Enter".to_string()])
        .await?;

    let timeout = if cfg!(test) {
        Duration::from_millis(800)
    } else {
        Duration::from_secs(8)
    };
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let screen = backend
            .capture_screen(session_name, false, Some(100))
            .await
            .unwrap_or_default();
        if let Some(reason) = model_switch_error(&screen) {
            let _ = backend
                .send_keys(session_name, &["Escape".to_string()])
                .await;
            return Err(anyhow!("Agent rejected model '{new_model}': {reason}"));
        }
        if screen_confirms_model(&screen, new_model) {
            return Ok(new_model.to_string());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = backend
        .send_keys(session_name, &["Escape".to_string()])
        .await;
    Err(anyhow!(
        "Could not confirm model switch to '{new_model}'; previous model retained"
    ))
}

pub fn profile_for_kind(kind: AgentCliKind) -> Arc<dyn AgentTuiProfile> {
    match kind {
        AgentCliKind::Agy => Arc::new(AgyTuiProfile),
        AgentCliKind::Grok => Arc::new(GrokTuiProfile),
        AgentCliKind::Codex => Arc::new(CodexTuiProfile),
        AgentCliKind::Claude => Arc::new(ClaudeTuiProfile),
        AgentCliKind::Custom => Arc::new(GenericTuiProfile::default()),
    }
}

pub(crate) fn user_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn detect_approval_modal(bottom_lines: &[&str]) -> Option<String> {
    let joined = bottom_lines.join("\n");
    let lower = joined.to_ascii_lowercase();

    let approval_patterns = [
        "trust the files",
        "trust the authors",
        "trust this folder",
        "trust this directory",
        "trust the directory",
        "do you trust",
        "is this folder trusted",
        "is this directory trusted",
        "workspace trust",
        "trust this repository",
        "trust the repository",
        "do you want to allow",
        "do you want to proceed",
        "do you want to continue",
        "allow this action",
        "approve this command",
        "approve tool",
        "allow claude",
        "allow command",
        "confirm action",
        "requires approval",
        "approval required",
        "[y/n]",
        "[y/n/a]",
        "[y/n/d]",
        "[y/n/all]",
        "[y/n/always]",
        "[y/n/q]",
        "[y/n/e]",
        "[y/n/c]",
        "[y/n/d/s]",
        "[y/n/a/d]",
        "[y/n/always/never]",
        "(y/n)",
        "[y/n]",
        "(y/n)",
        "yes/no",
        "[yes/no]",
        "yes, allow",
        "press enter to confirm",
        "press enter to continue",
        "press any key to continue",
        "hit enter to continue",
        "select a theme",
        "choose a theme",
        "enable telemetry",
        "allow anonymous telemetry",
        "send crash reports",
        "send error reports",
        "share analytics",
    ];

    let is_approval = approval_patterns.iter().any(|pat| lower.contains(pat))
        || (lower.contains("trust")
            && (lower.contains("folder")
                || lower.contains("director")
                || lower.contains("author")
                || lower.contains("file")
                || lower.contains("workspace")
                || lower.contains("repo")))
        || (lower.contains("proceed")
            && (lower.contains('?') || lower.contains("[y/") || lower.contains("(y/")))
        || (lower.contains("approve")
            && (lower.contains('?') || lower.contains("[y/") || lower.contains("(y/")))
        || (lower.contains("confirm")
            && (lower.contains('?') || lower.contains("[y/") || lower.contains("(y/")));

    if is_approval {
        let non_empty: Vec<&str> = bottom_lines
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        let snippet = non_empty
            .iter()
            .rev()
            .take(4)
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ");
        return Some(if snippet.is_empty() {
            "Approval required".to_string()
        } else {
            snippet
        });
    }
    None
}

pub fn detect_quota_limit(lines: &[&str]) -> Option<String> {
    let joined = lines.join("\n");
    let lower = joined.to_ascii_lowercase();

    if (lower.contains("upgrade tier") && lower.contains("buy more credits"))
        || lower.contains("hit your weekly limit")
        || lower.contains("hit the weekly limit")
        || lower.contains("reached your weekly limit")
        || lower.contains("weekly limit left: 0%")
        || lower.contains("weekly limit reached")
    {
        return Some("Hit weekly limit".to_string());
    }
    if lower.contains("usage balance exhausted") || lower.contains("balance exhausted") {
        return Some("Usage balance exhausted".to_string());
    }
    if lower.contains("quota exceeded")
        || lower.contains("exceeded your current quota")
        || lower.contains("exceeded your quota")
        || lower.contains("insufficient_quota")
    {
        return Some("Quota exceeded".to_string());
    }
    if lower.contains("rate limit reached") || lower.contains("hit your rate limit") {
        return Some("Rate limit reached".to_string());
    }
    None
}

pub fn suggested_approval_keys(screen_text: &str) -> Vec<String> {
    let clean = strip_ansi_codes(screen_text);
    let lower = clean.to_ascii_lowercase();
    let options: Vec<_> = lower
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let selected = line.starts_with(['>', '❯', '›', '→']);
            let option = line.trim_start_matches(['>', '❯', '›', '→', ' ']);
            let digit_end = option.find(|c: char| !c.is_ascii_digit())?;
            let (number, label) = option.split_at(digit_end);
            if number.is_empty() || !label.starts_with(['.', ')']) {
                return None;
            }
            Some((number, label.get(1..).unwrap_or_default().trim(), selected))
        })
        .collect();
    if let Some(target) = options.iter().position(|(_, label, _)| {
        label.starts_with("yes") || label.starts_with("allow") || label.starts_with("trust")
    }) {
        let mut keys = if let Some(current) = options.iter().position(|(_, _, selected)| *selected)
        {
            let direction = if target < current { "Up" } else { "Down" };
            vec![direction.to_string(); target.abs_diff(current)]
        } else {
            vec![options[target].0.to_string()]
        };
        keys.push("Enter".to_string());
        return keys;
    }

    if lower.contains("[y/n") || lower.contains("(y/n") || lower.contains("yes/no") {
        vec!["y".to_string(), "Enter".to_string()]
    } else if lower.contains("press enter")
        || lower.contains("hit enter")
        || lower.contains("enter to continue")
        || lower.contains("enter to confirm")
        || lower.contains("press any key")
        || lower.contains("select a theme")
        || lower.contains("choose a theme")
    {
        vec!["Enter".to_string()]
    } else {
        vec!["y".to_string(), "Enter".to_string()]
    }
}

pub(crate) fn detect_common_working_status(bottom_lines: &[&str]) -> Option<String> {
    for line in bottom_lines.iter().rev() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("esc to cancel")
            || lower.contains("esc to interrupt")
            || lower.contains("ctrl+c to interrupt")
        {
            return Some(trimmed.to_string());
        }
        if is_spinner_activity_line(trimmed) {
            return Some(trimmed.to_string());
        }
        if lower.starts_with("running ")
            || lower.starts_with("thinking")
            || lower.starts_with("working")
            || lower.starts_with("generating")
        {
            return Some(trimmed.to_string());
        }
    }
    None
}

pub(crate) fn detect_composer_idle_prompt(bottom_lines: &[&str]) -> bool {
    for line in bottom_lines.iter().rev().take(8) {
        if line_has_composer_marker(line) {
            return true;
        }
    }
    false
}

fn line_has_composer_marker(line: &str) -> bool {
    let trimmed = line.trim();
    if is_composer_marker(trimmed) {
        return true;
    }
    let inner = trimmed
        .trim_start_matches(['│', '|', '┃', ' '])
        .trim_end_matches(['│', '|', '┃', '─', '━', ' ']);
    is_composer_marker(inner)
}

fn is_composer_marker(text: &str) -> bool {
    text == ">"
        || text == "❯"
        || text == "›"
        || text.starts_with("> ")
        || text.starts_with("❯ ")
        || text.starts_with("› ")
        || text.ends_with('$')
        || text.ends_with('#')
        || text.ends_with('%')
}

pub(crate) fn strip_common_tui_chrome(captured_scrollback: &str, submitted_prompt: &str) -> String {
    let clean = strip_ansi_codes(captured_scrollback);
    let prompt_lines: Vec<&str> = submitted_prompt
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    let mut filtered: Vec<String> = Vec::new();
    for raw_line in clean.lines() {
        let trimmed_end = raw_line.trim_end();
        let trimmed = trimmed_end.trim();

        if is_horizontal_rule_line(trimmed) {
            continue;
        }
        if trimmed == ">" || trimmed == "❯" {
            continue;
        }
        if trimmed.starts_with("└ Tip:")
            || trimmed.starts_with("esc to cancel")
            || trimmed.starts_with("esc to interrupt")
            || contains_braille_spinner(trimmed)
        {
            continue;
        }
        // Strip footer model status line like `Gemini 4 Argon · medium` or `? for shortcuts`
        if (trimmed.contains(" · ")
            && (trimmed.contains("Gemini")
                || trimmed.contains("Claude")
                || trimmed.contains("Grok")
                || trimmed.contains("GPT")))
            || trimmed == "? for shortcuts"
        {
            continue;
        }

        // Strip echoed prompt line (`> <prompt_line>` or `❯ <prompt_line>`)
        if let Some(after_prompt) = trimmed
            .strip_prefix("> ")
            .or_else(|| trimmed.strip_prefix("❯ "))
        {
            let after_trimmed = after_prompt.trim();
            if prompt_lines.contains(&after_trimmed) {
                continue;
            }
        }
        if !prompt_lines.is_empty() && prompt_lines.contains(&trimmed) && filtered.is_empty() {
            continue;
        }

        filtered.push(trimmed_end.to_string());
    }

    // Trim leading and trailing blank lines
    let start = filtered
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(0);
    let end = filtered
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);

    if start >= end {
        String::new()
    } else {
        filtered[start..end].join("\n")
    }
}
