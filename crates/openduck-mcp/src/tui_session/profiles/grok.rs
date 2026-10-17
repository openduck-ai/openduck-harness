use anyhow::{anyhow, Result};
use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::AgentTuiProfile;
use super::{AgentCliKind, TuiPaneState, TuiSessionConfig};

#[derive(Debug, Default, Clone)]
pub struct GrokTuiProfile;

impl AgentTuiProfile for GrokTuiProfile {
    fn kind(&self) -> AgentCliKind {
        AgentCliKind::Grok
    }

    fn build_launch_command(&self, config: &TuiSessionConfig) -> Result<Vec<String>> {
        let binary = resolve_grok_bin()?;
        let mut cmd = vec![binary.to_string_lossy().to_string()];

        if config.auto_approve {
            cmd.push("--always-approve".to_string());
        }

        // Keep scrollback natively accessible
        cmd.push("--no-alt-screen".to_string());

        if let Some(ref m) = config.model {
            cmd.push("-m".to_string());
            cmd.push(m.clone());
        }

        if let Some(ref e) = config.effort {
            cmd.push("--reasoning-effort".to_string());
            cmd.push(e.clone());
        }

        if let Some(ref r) = config.resume_id {
            cmd.push("-r".to_string());
            cmd.push(r.clone());
        }

        for arg in &config.extra_args {
            cmd.push(arg.clone());
        }

        Ok(cmd)
    }

    fn classify_screen(&self, screen_text: &str) -> TuiPaneState {
        let text = screen_text.trim();

        // 1. Approval modal
        if text.contains("[y/n]")
            || text.contains("[y/N]")
            || text.contains("[Y/n]")
            || text.contains("Approve?")
            || text.contains("Confirm action")
        {
            let snippet = text
                .lines()
                .filter(|l| l.contains('?') || l.contains("[y/"))
                .take(3)
                .collect::<Vec<_>>()
                .join(" ");
            return TuiPaneState::WaitingApproval {
                prompt_snippet: if snippet.is_empty() {
                    "Approval required".to_string()
                } else {
                    snippet
                },
            };
        }

        // 2. Working state
        let has_spinner = text.contains('⠋')
            || text.contains('⠙')
            || text.contains('⠹')
            || text.contains('⠸')
            || text.contains('⠼')
            || text.contains('⠴')
            || text.contains('⠦')
            || text.contains('⠧')
            || text.contains('⠇')
            || text.contains('⠏')
            || text.contains('⣷');

        if has_spinner || text.contains("Thinking...") || text.contains("Running tool") {
            let status_line = text
                .lines()
                .find(|l| l.contains("Thinking") || l.contains("Running tool"))
                .map(|l| l.trim().to_string());
            return TuiPaneState::Working { status_line };
        }

        // 3. Idle Composer prompt
        if text.contains("\n>") || text.starts_with('>') || text.ends_with('>') {
            return TuiPaneState::IdlePrompt;
        }

        if text.is_empty() {
            TuiPaneState::Initializing
        } else {
            TuiPaneState::IdlePrompt
        }
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

            if trimmed.starts_with("Grok Build") || trimmed.contains("esc to interrupt") {
                continue;
            }

            clean_lines.push(line);
        }

        clean_lines.join("\n").trim().to_string()
    }

    fn read_transcript_delta(
        &self,
        cwd: &Path,
        _pid: u32,
        resume_id: Option<&str>,
        since_offset: &mut u64,
    ) -> Option<String> {
        let home = grok_home()?;
        let transcript_path = find_grok_chat_history(&home, cwd, resume_id)?;

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
                let role = value.get("role").and_then(Value::as_str).unwrap_or("");
                if role == "assistant" {
                    if let Some(content) = value.get("content").and_then(Value::as_str) {
                        if !content.trim().is_empty() {
                            responses.push(content.to_string());
                        }
                    }
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

fn grok_home() -> Option<PathBuf> {
    env::var("GROK_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| user_home().map(|h| h.join(".grok")))
}

fn resolve_grok_bin() -> Result<PathBuf> {
    if let Ok(explicit) = env::var("GROK_COMMAND") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
    }

    if let Ok(p) = which::which("grok") {
        return Ok(p);
    }

    let fallbacks = [
        user_home().map(|h| h.join(".local/bin/grok")),
        user_home().map(|h| h.join(".grok/bin/grok")),
    ];
    for candidate in fallbacks.into_iter().flatten() {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(anyhow!(
        "Could not find 'grok' binary. Please set GROK_COMMAND or ensure grok is on PATH."
    ))
}

fn find_grok_chat_history(
    grok_home: &Path,
    _cwd: &Path,
    resume_id: Option<&str>,
) -> Option<PathBuf> {
    let sessions_dir = grok_home.join("sessions");
    if !sessions_dir.is_dir() {
        return None;
    }

    if let Some(sid) = resume_id {
        // Look for session_id in any subfolder
        if let Ok(groups) = fs::read_dir(&sessions_dir) {
            for g in groups.flatten() {
                let candidate = g.path().join(sid).join("chat_history.jsonl");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    // Find newest chat_history.jsonl
    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    if let Ok(groups) = fs::read_dir(&sessions_dir) {
        for g in groups.flatten() {
            if let Ok(sess_dirs) = fs::read_dir(g.path()) {
                for s in sess_dirs.flatten() {
                    let candidate = s.path().join("chat_history.jsonl");
                    if candidate.is_file() {
                        if let Ok(mod_time) = candidate.metadata().and_then(|m| m.modified()) {
                            if newest.as_ref().map(|(_, t)| mod_time > *t).unwrap_or(true) {
                                newest = Some((candidate, mod_time));
                            }
                        }
                    }
                }
            }
        }
    }

    newest.map(|(p, _)| p)
}
