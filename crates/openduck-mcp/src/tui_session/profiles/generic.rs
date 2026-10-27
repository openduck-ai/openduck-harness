use anyhow::{anyhow, Result};
use std::path::Path;

use super::AgentTuiProfile;
use super::{
    detect_approval_modal, detect_composer_idle_prompt, AgentCliKind, TuiPaneState,
    TuiSessionConfig,
};
use crate::tui_session::screen::{contains_braille_spinner, ScreenSnapshot};

#[derive(Debug, Default, Clone)]
pub struct GenericTuiProfile {
    pub prompt_pattern: Option<String>,
}

impl AgentTuiProfile for GenericTuiProfile {
    fn kind(&self) -> AgentCliKind {
        AgentCliKind::Custom
    }

    fn build_launch_command(&self, config: &TuiSessionConfig) -> Result<Vec<String>> {
        if config.extra_args.is_empty() {
            return Err(anyhow!(
                "Generic / Custom agent profile requires launch command in 'extra_args'"
            ));
        }
        Ok(config.extra_args.clone())
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
            || text.contains("Working...")
            || text.contains("Running...");

        if has_spinner {
            return TuiPaneState::Working {
                status_line: Some("Working...".to_string()),
            };
        }

        if let Some(ref pattern) = self.prompt_pattern {
            if text.contains(pattern) {
                return TuiPaneState::IdlePrompt;
            }
        } else if detect_composer_idle_prompt(&bottom)
            || text.contains("\n>")
            || text.starts_with('>')
            || text.ends_with('>')
            || text.ends_with("$ ")
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

            clean_lines.push(line);
        }

        clean_lines.join("\n").trim().to_string()
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
