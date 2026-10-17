use super::{
    detect_approval_modal, detect_common_working_status, detect_composer_idle_prompt,
    strip_common_tui_chrome, user_home, AgentCliKind, AgentTuiProfile, TuiPaneState,
    TuiSessionConfig,
};
use crate::tui_session::screen::ScreenSnapshot;
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub struct AgyTuiProfile;

impl AgyTuiProfile {
    pub fn agy_home() -> PathBuf {
        env::var("AGY_HOME")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| user_home().map(|h| h.join(".gemini/antigravity-cli")))
            .unwrap_or_else(|| PathBuf::from(".gemini/antigravity-cli"))
    }

    pub fn resolve_binary() -> Result<PathBuf> {
        if let Ok(explicit) = env::var("AGY_COMMAND") {
            let p = PathBuf::from(explicit);
            if p.is_file() {
                return Ok(p);
            }
            return Err(anyhow!(
                "AGY_COMMAND is set but does not point to a file: {}",
                p.display()
            ));
        }
        if let Ok(p) = which::which("agy") {
            return Ok(p);
        }
        let fallbacks = [
            user_home().map(|h| h.join(".local/bin/agy")),
            env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("agy/bin/agy.exe")),
        ];
        for candidate in fallbacks.into_iter().flatten() {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        Err(anyhow!(
            "Could not find `agy` binary on PATH. Set AGY_COMMAND or install Antigravity CLI."
        ))
    }

    pub fn locate_transcript_file(cwd: &Path, resume_id: Option<&str>) -> Option<PathBuf> {
        let home = Self::agy_home();
        if let Some(conv_id) =
            resume_id.filter(|id| !id.is_empty() && *id != "continue" && *id != "last")
        {
            let path = home
                .join("brain")
                .join(conv_id)
                .join(".system_generated/logs/transcript.jsonl");
            if path.is_file() {
                return Some(path);
            }
        }

        let meta_path = home.join("cache/conversation_metadata.json");
        if let Ok(contents) = fs::read_to_string(&meta_path) {
            if let Ok(meta) = serde_json::from_str::<Value>(&contents) {
                if let Some(conversations) = meta.get("conversations").and_then(Value::as_object) {
                    let cwd_canon = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
                    let cwd_str = cwd_canon.to_string_lossy();
                    let mut best: Option<(String, String)> = None;

                    for (id, entry) in conversations {
                        if entry.get("is_internal").and_then(Value::as_bool) == Some(true) {
                            continue;
                        }
                        let summary = entry.get("summary").unwrap_or(&Value::Null);
                        let matches_cwd = summary
                            .get("WorkspaceURIs")
                            .and_then(Value::as_array)
                            .map(|arr| {
                                arr.iter().any(|u| {
                                    u.as_str()
                                        .map(|s| {
                                            let stripped = s.strip_prefix("file://").unwrap_or(s);
                                            stripped == cwd_str || cwd_str.starts_with(stripped)
                                        })
                                        .unwrap_or(false)
                                })
                            })
                            .unwrap_or(false);

                        if matches_cwd {
                            let updated = entry
                                .get("last_modified_time")
                                .and_then(Value::as_str)
                                .or_else(|| summary.get("UpdatedAt").and_then(Value::as_str))
                                .unwrap_or("")
                                .to_string();
                            if best.as_ref().map(|(_, t)| &updated > t).unwrap_or(true) {
                                best = Some((id.clone(), updated));
                            }
                        }
                    }

                    if let Some((best_id, _)) = best {
                        let path = home
                            .join("brain")
                            .join(best_id)
                            .join(".system_generated/logs/transcript.jsonl");
                        if path.is_file() {
                            return Some(path);
                        }
                    }
                }
            }
        }

        None
    }

    pub fn parse_transcript_bytes(bytes: &[u8]) -> Option<String> {
        let text = String::from_utf8_lossy(bytes);
        let mut responses = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(val) = serde_json::from_str::<Value>(line) {
                let step_type = val.get("type").and_then(Value::as_str).unwrap_or("");
                if step_type == "PLANNER_RESPONSE" {
                    if let Some(content) = val.get("content").and_then(Value::as_str) {
                        let trimmed = content.trim();
                        if !trimmed.is_empty() {
                            responses.push(trimmed.to_string());
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

impl AgentTuiProfile for AgyTuiProfile {
    fn kind(&self) -> AgentCliKind {
        AgentCliKind::Agy
    }

    fn build_launch_command(&self, config: &TuiSessionConfig) -> Result<Vec<String>> {
        let bin = Self::resolve_binary()?;
        let mut cmd = vec![bin.to_string_lossy().into_owned()];
        if config.auto_approve {
            cmd.push("--dangerously-skip-permissions".to_string());
        }
        if let Some(resume) = config
            .resume_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if resume == "continue" || resume == "last" {
                cmd.push("--continue".to_string());
            } else {
                cmd.push("--conversation".to_string());
                cmd.push(resume.to_string());
            }
        }
        if let Some(model) = config
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            cmd.push("--model".to_string());
            cmd.push(model.to_string());
        }
        if let Some(effort) = config
            .effort
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            cmd.push("--effort".to_string());
            cmd.push(effort.to_string());
        }
        cmd.extend(config.extra_args.iter().cloned());
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
        if let Some(status) = detect_common_working_status(&bottom) {
            return TuiPaneState::Working {
                status_line: Some(status),
            };
        }
        if detect_composer_idle_prompt(&bottom) {
            return TuiPaneState::IdlePrompt;
        }
        TuiPaneState::Initializing
    }

    fn strip_chrome(&self, captured_scrollback: &str, submitted_prompt: &str) -> String {
        strip_common_tui_chrome(captured_scrollback, submitted_prompt)
    }

    fn current_transcript_offset(&self, cwd: &Path, _pid: u32, resume_id: Option<&str>) -> u64 {
        Self::locate_transcript_file(cwd, resume_id)
            .and_then(|p| fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0)
    }

    fn read_transcript_delta(
        &self,
        cwd: &Path,
        _pid: u32,
        resume_id: Option<&str>,
        since_offset: &mut u64,
    ) -> Option<String> {
        let path = Self::locate_transcript_file(cwd, resume_id)?;
        let mut file = File::open(&path).ok()?;
        let len = file.metadata().ok()?.len();
        if len <= *since_offset {
            return None;
        }
        file.seek(SeekFrom::Start(*since_offset)).ok()?;
        let mut buf = Vec::with_capacity((len - *since_offset) as usize);
        file.read_to_end(&mut buf).ok()?;
        *since_offset = len;
        Self::parse_transcript_bytes(&buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agy_classify_screen_identifies_busy_and_idle() {
        let profile = AgyTuiProfile;
        let busy_screen = "\
⣷  Running command...
└ Tip: Use /voice (or press f5) to record your prompt with your microphone.
────────────────────────────────────────────────────────────────────────────────
>
────────────────────────────────────────────────────────────────────────────────
esc to cancel                                          Gemini 4 Argon · medium";
        assert_eq!(
            profile.classify_screen(busy_screen),
            TuiPaneState::Working {
                status_line: Some("esc to cancel                                          Gemini 4 Argon · medium".to_string())
            }
        );

        let idle_screen = "\
> Refactor auth module
I have refactored the auth module and updated the unit tests.
────────────────────────────────────────────────────────────────────────────────
>
────────────────────────────────────────────────────────────────────────────────
                                                       Gemini 4 Argon · medium";
        assert_eq!(
            profile.classify_screen(idle_screen),
            TuiPaneState::IdlePrompt
        );
        let stripped = profile.strip_chrome(idle_screen, "Refactor auth module");
        assert_eq!(
            stripped,
            "I have refactored the auth module and updated the unit tests."
        );
    }

    #[test]
    fn agy_parse_transcript_extracts_planner_responses() {
        let sample = br#"
{"step_index":1,"type":"USER_INPUT","content":"hello"}
{"step_index":2,"type":"PLANNER_RESPONSE","content":"First tool step","tool_calls":[{"name":"view_file"}]}
{"step_index":3,"type":"PLANNER_RESPONSE","content":"Final complete answer from agy."}
"#;
        let extracted = AgyTuiProfile::parse_transcript_bytes(sample).unwrap();
        assert!(extracted.contains("First tool step"));
        assert!(extracted.contains("Final complete answer from agy."));
    }
}
