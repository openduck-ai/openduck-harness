use anyhow::{anyhow, Result};
use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::AgentTuiProfile;
use super::{
    detect_approval_modal, detect_composer_idle_prompt, AgentCliKind, TuiPaneState,
    TuiSessionConfig,
};
use crate::tui_session::screen::{contains_braille_spinner, ScreenSnapshot};

#[derive(Debug, Default, Clone)]
pub struct ClaudeTuiProfile;

impl AgentTuiProfile for ClaudeTuiProfile {
    fn kind(&self) -> AgentCliKind {
        AgentCliKind::Claude
    }

    fn build_launch_command(&self, config: &TuiSessionConfig) -> Result<Vec<String>> {
        let binary = resolve_claude_bin()?;
        let mut cmd = vec![binary.to_string_lossy().to_string()];

        if config.auto_approve {
            cmd.push("--dangerously-skip-permissions".to_string());
        }

        // Use accessible flat text output
        cmd.push("--ax-screen-reader".to_string());

        if let Some(ref r) = config.resume_id {
            cmd.push("--resume".to_string());
            cmd.push(r.clone());
        }

        for arg in &config.extra_args {
            cmd.push(arg.clone());
        }

        Ok(cmd)
    }

    fn classify_screen(&self, screen_text: &str) -> TuiPaneState {
        let snapshot = ScreenSnapshot::new(screen_text.to_string());
        let bottom = snapshot.bottom_non_empty_lines(14);
        if bottom.is_empty() {
            return TuiPaneState::Initializing;
        }

        if let Some(snippet) = detect_approval_modal(&bottom) {
            return TuiPaneState::WaitingApproval {
                prompt_snippet: snippet,
            };
        }

        let text = screen_text.trim();
        let has_spinner = contains_braille_spinner(text)
            || text.contains("esc to interrupt")
            || text.contains("Thinking...");

        if has_spinner {
            let status_line = text
                .lines()
                .find(|l| l.contains("Thinking") || l.contains("tokens"))
                .map(|l| l.trim().to_string());
            return TuiPaneState::Working { status_line };
        }

        if detect_composer_idle_prompt(&bottom)
            || text.contains("\n>")
            || text.starts_with('>')
            || text.ends_with('>')
        {
            return TuiPaneState::IdlePrompt;
        }

        TuiPaneState::Initializing
    }

    fn strip_chrome(&self, captured_scrollback: &str, submitted_prompt: &str) -> String {
        let mut clean_lines = Vec::new();
        let prompt_trimmed = submitted_prompt.trim();

        for line in captured_scrollback.lines() {
            let trimmed = line.trim();

            if trimmed.chars().all(|c| c == '─' || c == '-' || c == '=') && trimmed.len() >= 3 {
                continue;
            }

            if trimmed == ">"
                || trimmed == format!("> {prompt_trimmed}")
                || trimmed == prompt_trimmed
            {
                continue;
            }

            if trimmed.contains("esc to interrupt") || trimmed.contains("Claude Code") {
                continue;
            }

            clean_lines.push(line);
        }

        clean_lines.join("\n").trim().to_string()
    }

    fn read_transcript_delta(
        &self,
        _cwd: &Path,
        _pid: u32,
        _resume_id: Option<&str>,
        since_offset: &mut u64,
    ) -> Option<String> {
        let home = claude_home()?;
        let transcript_path = find_claude_transcript(&home)?;

        let mut file = File::open(&transcript_path).ok()?;
        let file_len = file.metadata().ok()?.len();

        if file_len <= *since_offset {
            return None;
        }

        file.seek(SeekFrom::Start(*since_offset)).ok()?;
        let mut buffer = String::new();
        file.read_to_string(&mut buffer).ok()?;

        *since_offset = file_len;

        let mut responses = Vec::new();

        for line in buffer.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Ok(value) = serde_json::from_str::<Value>(line) {
                if let Some(msg) = value.get("text").and_then(Value::as_str) {
                    responses.push(msg.to_string());
                } else if let Some(content) = value.get("content").and_then(Value::as_str) {
                    responses.push(content.to_string());
                }
            }
        }

        if responses.is_empty() {
            None
        } else {
            Some(responses.join("\n\n"))
        }
    }
}

fn user_home() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn claude_home() -> Option<PathBuf> {
    env::var("CLAUDE_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| user_home().map(|h| h.join(".claude")))
}

fn resolve_claude_bin() -> Result<PathBuf> {
    if let Ok(explicit) = env::var("CLAUDE_COMMAND") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
    }

    if let Ok(p) = which::which("claude") {
        return Ok(p);
    }

    let fallbacks = [
        user_home().map(|h| h.join(".local/bin/claude")),
        user_home().map(|h| h.join(".npm-global/bin/claude")),
    ];
    for candidate in fallbacks.into_iter().flatten() {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(anyhow!(
        "Could not find 'claude' binary. Please set CLAUDE_COMMAND or ensure claude is on PATH."
    ))
}

fn find_claude_transcript(claude_home: &Path) -> Option<PathBuf> {
    let projects_dir = claude_home.join("projects");
    if !projects_dir.is_dir() {
        return None;
    }

    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    if let Ok(entries) = fs::read_dir(&projects_dir) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) == Some("jsonl") {
                if let Ok(mod_time) = path.metadata().and_then(|m| m.modified()) {
                    if newest.as_ref().map(|(_, t)| mod_time > *t).unwrap_or(true) {
                        newest = Some((path, mod_time));
                    }
                }
            }
        }
    }

    newest.map(|(p, _)| p)
}
