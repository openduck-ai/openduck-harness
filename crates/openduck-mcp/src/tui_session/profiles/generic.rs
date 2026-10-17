use anyhow::{anyhow, Result};
use std::path::Path;

use super::AgentTuiProfile;
use super::{AgentCliKind, TuiPaneState, TuiSessionConfig};

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
        let text = screen_text.trim();

        if text.contains("[y/n]")
            || text.contains("[y/N]")
            || text.contains("[Y/n]")
            || text.contains("Confirm:")
            || text.contains("Proceed?")
        {
            let snippet = text
                .lines()
                .filter(|l| l.contains('?') || l.contains("[y/"))
                .take(2)
                .collect::<Vec<_>>()
                .join(" ");
            return TuiPaneState::WaitingApproval {
                prompt_snippet: snippet,
            };
        }

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

        if has_spinner || text.contains("Working...") || text.contains("Running...") {
            return TuiPaneState::Working {
                status_line: Some("Working...".to_string()),
            };
        }

        if let Some(ref pattern) = self.prompt_pattern {
            if text.contains(pattern) {
                return TuiPaneState::IdlePrompt;
            }
        } else if text.contains("\n>")
            || text.starts_with('>')
            || text.ends_with('>')
            || text.ends_with("$ ")
        {
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
