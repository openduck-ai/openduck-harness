use anyhow::{anyhow, Result};
use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::AgentTuiProfile;
use super::{
    detect_approval_modal, detect_common_working_status, detect_composer_idle_prompt,
    detect_quota_limit, AgentCliKind, TuiPaneState, TuiSessionConfig,
};
use crate::tui_session::screen::{contains_braille_spinner, ScreenSnapshot};
use async_trait::async_trait;

#[derive(Debug, Default, Clone)]
pub struct CodexTuiProfile;

#[async_trait]
impl AgentTuiProfile for CodexTuiProfile {
    fn kind(&self) -> AgentCliKind {
        AgentCliKind::Codex
    }

    fn build_launch_command(&self, config: &TuiSessionConfig) -> Result<Vec<String>> {
        let binary = resolve_codex_bin()?;
        let mut cmd = vec![binary.to_string_lossy().to_string()];

        if config.auto_approve {
            cmd.push("--dangerously-bypass-approvals-and-sandbox".to_string());
        }

        cmd.push("--no-alt-screen".to_string());
        cmd.push("-C".to_string());
        cmd.push(config.cwd.to_string_lossy().to_string());

        if let Some(ref m) = config.model {
            cmd.push("-m".to_string());
            cmd.push(m.clone());
        }

        if let Some(ref r) = config.resume_id {
            cmd.push("resume".to_string());
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

        if let Some(msg) = detect_quota_limit(&bottom) {
            return TuiPaneState::QuotaExceeded { message: msg };
        }
        let full_clean = snapshot.trimmed_clean();
        let full_lines: Vec<&str> = full_clean.lines().collect();
        if let Some(msg) = detect_quota_limit(&full_lines) {
            return TuiPaneState::QuotaExceeded { message: msg };
        }

        if let Some(snippet) = detect_approval_modal(&bottom) {
            return TuiPaneState::WaitingApproval {
                prompt_snippet: snippet,
            };
        }

        if let Some(status_line) = codex_activity_line(&bottom) {
            return TuiPaneState::Working {
                status_line: Some(status_line),
            };
        }

        if detect_composer_idle_prompt(&bottom) {
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

            if trimmed.starts_with("OpenAI Codex") || trimmed.contains("esc to interrupt") {
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
        let home = codex_home()?;
        let transcript_path = find_codex_transcript(&home)?;

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
                if let Some(msg) = value.get("assistant_message").and_then(Value::as_str) {
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

fn codex_activity_line(bottom: &[&str]) -> Option<String> {
    if let Some(status) = detect_common_working_status(bottom) {
        return Some(status);
    }
    bottom.iter().rev().find_map(|line| {
        let trimmed = line.trim();
        if contains_braille_spinner(trimmed)
            || trimmed.contains("Thinking...")
            || trimmed.contains("Running...")
        {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn user_home() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn codex_home() -> Option<PathBuf> {
    env::var("CODEX_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| user_home().map(|h| h.join(".codex")))
}

fn resolve_codex_bin() -> Result<PathBuf> {
    if let Ok(explicit) = env::var("CODEX_COMMAND") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
    }

    if let Ok(p) = which::which("codex") {
        return Ok(p);
    }

    let fallbacks = [
        user_home().map(|h| h.join(".local/bin/codex")),
        user_home().map(|h| h.join(".codex/bin/codex")),
        user_home().map(|h| h.join(".npm-global/bin/codex")),
    ];
    for candidate in fallbacks.into_iter().flatten() {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(anyhow!(
        "Could not find 'codex' binary. Please set CODEX_COMMAND or ensure codex is on PATH."
    ))
}

fn find_codex_transcript(codex_home: &Path) -> Option<PathBuf> {
    let sessions_dir = codex_home.join("sessions");
    if !sessions_dir.is_dir() {
        return None;
    }

    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    if let Ok(entries) = fs::read_dir(&sessions_dir) {
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodexQuotaWindow {
    pub quota_type: String,
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub window_minutes: Option<u64>,
    pub reset_at: Option<String>,
    pub is_exhausted: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodexQuotaInfo {
    pub windows: Vec<CodexQuotaWindow>,
    pub observed_at: Option<String>,
    pub plan_type: Option<String>,
}

fn collect_jsonl_files_recursive(
    dir: &Path,
    depth: usize,
    out: &mut Vec<(PathBuf, std::time::SystemTime)>,
) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files_recursive(&path, depth + 1, out);
        } else if path.extension().and_then(|x| x.to_str()) == Some("jsonl") {
            if let Ok(mod_time) = path.metadata().and_then(|m| m.modified()) {
                out.push((path, mod_time));
            }
        }
    }
}

fn parse_codex_rate_limit_window(
    window: &Value,
    default_type: &str,
    rate_limit_reached: bool,
) -> Option<CodexQuotaWindow> {
    let used_percent = window.get("used_percent").and_then(Value::as_f64)?;
    let window_minutes = window.get("window_minutes").and_then(Value::as_u64);
    let quota_type = match window_minutes {
        Some(m) if m >= 1440 => "weekly".to_string(),
        Some(m) if m > 0 => format!("{}h", (m + 59) / 60),
        _ => default_type.to_string(),
    };
    let reset_at = window
        .get("resets_at")
        .and_then(Value::as_i64)
        .and_then(|ts| chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0))
        .map(|dt| dt.to_rfc3339());
    let remaining_percent = (100.0 - used_percent).clamp(0.0, 100.0);
    let is_exhausted = used_percent >= 100.0 || rate_limit_reached;
    Some(CodexQuotaWindow {
        quota_type,
        used_percent,
        remaining_percent,
        window_minutes,
        reset_at,
        is_exhausted,
    })
}

fn read_codex_quota_from_file(path: &Path) -> Option<CodexQuotaInfo> {
    let file = File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let len = reader.get_ref().metadata().ok()?.len();
    if len > 262_144 {
        let _ = reader.seek(SeekFrom::Start(len - 262_144));
    }
    let mut buffer = String::new();
    reader.read_to_string(&mut buffer).ok()?;

    for line in buffer.lines().rev() {
        if !line.contains("rate_limits") {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(rate_limits) = val.pointer("/payload/rate_limits") else {
            continue;
        };
        let rate_limit_reached = rate_limits
            .get("rate_limit_reached_type")
            .and_then(Value::as_str)
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        let mut windows = Vec::new();
        if let Some(primary) = rate_limits.get("primary") {
            if let Some(w) = parse_codex_rate_limit_window(primary, "session", rate_limit_reached) {
                windows.push(w);
            }
        }
        if let Some(secondary) = rate_limits.get("secondary") {
            if let Some(w) = parse_codex_rate_limit_window(secondary, "weekly", rate_limit_reached)
            {
                windows.push(w);
            }
        }
        if !windows.is_empty() {
            return Some(CodexQuotaInfo {
                windows,
                observed_at: val
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                plan_type: rate_limits
                    .get("plan_type")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            });
        }
    }
    None
}

pub fn read_codex_quota_info_from_dir(codex_home_dir: &Path) -> Option<CodexQuotaInfo> {
    let sessions_dir = codex_home_dir.join("sessions");
    if !sessions_dir.is_dir() {
        return None;
    }
    let mut files = Vec::new();
    collect_jsonl_files_recursive(&sessions_dir, 0, &mut files);
    files.sort_by(|a, b| b.1.cmp(&a.1));
    for (path, _) in files.into_iter().take(5) {
        if let Some(info) = read_codex_quota_from_file(&path) {
            return Some(info);
        }
    }
    None
}

pub fn read_codex_quota_info() -> Option<CodexQuotaInfo> {
    let home = codex_home()?;
    read_codex_quota_info_from_dir(&home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_rate_limits_from_nested_session_jsonl() {
        let dir = tempfile::tempdir().expect("tempdir");
        let nested = dir.path().join("sessions/2026/11/14");
        fs::create_dir_all(&nested).expect("mkdir");
        let file_path = nested.join("rollout-test.jsonl");
        let line = r#"{"timestamp":"2026-11-14T05:59:47.660Z","ordinal":35,"type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","primary":{"used_percent":2.0,"window_minutes":300,"resets_at":1794653216},"secondary":{"used_percent":12.0,"window_minutes":10080,"resets_at":1795011948},"plan_type":"plus","rate_limit_reached_type":null}}}"#;
        fs::write(&file_path, format!("{line}\n")).expect("write");

        let info = read_codex_quota_info_from_dir(dir.path()).expect("quota info");
        assert_eq!(info.plan_type.as_deref(), Some("plus"));
        assert_eq!(info.windows.len(), 2);
        assert_eq!(info.windows[0].quota_type, "5h");
        assert_eq!(info.windows[0].remaining_percent, 98.0);
        assert!(!info.windows[0].is_exhausted);
        assert!(info.windows[0].reset_at.is_some());
        assert_eq!(info.windows[1].quota_type, "weekly");
        assert_eq!(info.windows[1].remaining_percent, 88.0);
    }
}
