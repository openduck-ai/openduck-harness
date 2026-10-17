pub mod agy;
pub mod claude;
pub mod codex;
pub mod generic;
pub mod grok;

use crate::tui_session::screen::{
    contains_braille_spinner, is_horizontal_rule_line, strip_ansi_codes,
};
use anyhow::{anyhow, Result};
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

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

    pub fn label(&self) -> &'static str {
        match self {
            TuiPaneState::Initializing => "initializing",
            TuiPaneState::IdlePrompt => "idle_prompt",
            TuiPaneState::Working { .. } => "working",
            TuiPaneState::WaitingApproval { .. } => "waiting_approval",
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

pub trait AgentTuiProfile: Send + Sync {
    fn kind(&self) -> AgentCliKind;
    fn build_launch_command(&self, config: &TuiSessionConfig) -> Result<Vec<String>>;
    fn classify_screen(&self, screen_text: &str) -> TuiPaneState;
    fn strip_chrome(&self, captured_scrollback: &str, submitted_prompt: &str) -> String;
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

pub(crate) fn detect_approval_modal(bottom_lines: &[&str]) -> Option<String> {
    let joined = bottom_lines.join("\n");
    let lower = joined.to_ascii_lowercase();

    let approval_patterns = [
        "do you trust the files in this folder",
        "do you want to allow",
        "allow this action",
        "approve this command",
        "requires approval",
        "[y/n]",
        "(y/n)",
        "[y/n/a]",
        "yes, allow",
        "press enter to confirm",
    ];

    for pat in approval_patterns {
        if lower.contains(pat) {
            let snippet = bottom_lines
                .iter()
                .rev()
                .take(4)
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" | ");
            return Some(snippet);
        }
    }
    None
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
        if contains_braille_spinner(trimmed) {
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
        let trimmed = line.trim();
        if trimmed == ">"
            || trimmed == "❯"
            || trimmed.starts_with("> ")
            || trimmed.starts_with("❯ ")
            || trimmed.ends_with("$")
            || trimmed.ends_with("#")
            || trimmed.ends_with("%")
        {
            return true;
        }
    }
    false
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
