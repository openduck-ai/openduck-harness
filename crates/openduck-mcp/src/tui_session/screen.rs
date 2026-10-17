use once_cell::sync::Lazy;
use regex::Regex;
use std::time::{Duration, Instant};

static ANSI_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[@-_]")
        .expect("valid ANSI regex")
});

const BRAILLE_SPINNERS: &[char] = &[
    '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '⣷', '⣯', '⣟', '⡿', '⢿', '⣻', '⣽', '⣾', '◐',
    '◓', '◑', '◒',
];

pub fn strip_ansi_codes(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "");
    ANSI_REGEX.replace_all(&normalized, "").into_owned()
}

pub use strip_ansi_codes as strip_ansi;

pub fn contains_braille_spinner(text: &str) -> bool {
    text.chars().any(|c| BRAILLE_SPINNERS.contains(&c))
}

pub fn is_horizontal_rule_line(line: &str) -> bool {
    let trimmed = line.trim();
    let char_count = trimmed.chars().count();
    if char_count < 10 {
        return false;
    }
    trimmed.chars().all(|c| {
        matches!(
            c,
            '─' | '━' | '═' | '-' | '╭' | '╮' | '╰' | '╯' | '┌' | '┐' | '└' | '┘'
        )
    })
}

#[derive(Debug, Clone)]
pub struct ScreenSnapshot {
    pub raw_text: String,
    pub clean_text: String,
    pub captured_at: Instant,
}

impl ScreenSnapshot {
    pub fn new(raw_text: String) -> Self {
        let clean_text = strip_ansi_codes(&raw_text);
        Self {
            raw_text,
            clean_text,
            captured_at: Instant::now(),
        }
    }

    pub fn lines(&self) -> Vec<&str> {
        self.clean_text.lines().collect()
    }

    pub fn bottom_non_empty_lines(&self, count: usize) -> Vec<&str> {
        let mut lines: Vec<&str> = self
            .clean_text
            .lines()
            .map(str::trim_end)
            .filter(|l| !l.trim().is_empty())
            .rev()
            .take(count)
            .collect();
        lines.reverse();
        lines
    }

    pub fn trimmed_clean(&self) -> String {
        let lines: Vec<&str> = self.clean_text.lines().map(str::trim_end).collect();
        let start = lines.iter().position(|l| !l.is_empty()).unwrap_or(0);
        let end = lines
            .iter()
            .rposition(|l| !l.is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        if start >= end {
            String::new()
        } else {
            lines[start..end].join("\n")
        }
    }
}

pub fn diff_scrollback(before: &str, after: &str) -> String {
    let before_clean = strip_ansi_codes(before);
    let after_clean = strip_ansi_codes(after);

    let before_lines: Vec<&str> = before_clean
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    let after_lines: Vec<&str> = after_clean.lines().map(str::trim_end).collect();

    if before_lines.is_empty() {
        return after_lines.join("\n").trim().to_string();
    }

    // Find the last non-rule, non-prompt anchor line from `before` in `after`
    let anchor = before_lines
        .iter()
        .rev()
        .find(|line| {
            let t = line.trim();
            !t.is_empty()
                && !is_horizontal_rule_line(t)
                && t != ">"
                && t != "❯"
                && !t.starts_with("└ Tip:")
                && !t.starts_with("esc to ")
        })
        .copied();

    if let Some(anchor_line) = anchor {
        if let Some(pos) = after_lines
            .iter()
            .rposition(|l| l.trim_end() == anchor_line)
        {
            return after_lines[pos + 1..].join("\n").trim().to_string();
        }
    }

    after_lines.join("\n").trim().to_string()
}

pub struct QuiescenceDetector {
    debounce_window: Duration,
    last_clean_screen: Option<String>,
    idle_since: Option<Instant>,
    saw_activity: bool,
}

impl QuiescenceDetector {
    pub fn new(debounce_window: Duration) -> Self {
        Self {
            debounce_window,
            last_clean_screen: None,
            idle_since: None,
            saw_activity: false,
        }
    }

    pub fn mark_activity(&mut self) {
        self.saw_activity = true;
        self.idle_since = None;
    }

    pub fn observe(
        &mut self,
        clean_screen: &str,
        is_idle_prompt: bool,
        has_new_transcript: bool,
    ) -> bool {
        let screen_changed = self
            .last_clean_screen
            .as_deref()
            .map(|prev| prev != clean_screen)
            .unwrap_or(false);

        if self.last_clean_screen.is_none() {
            self.last_clean_screen = Some(clean_screen.to_string());
        } else if screen_changed {
            self.saw_activity = true;
            self.last_clean_screen = Some(clean_screen.to_string());
            self.idle_since = None;
        }

        if has_new_transcript {
            self.saw_activity = true;
        }

        if !is_idle_prompt {
            self.saw_activity = true;
            self.idle_since = None;
            return false;
        }

        if !self.saw_activity {
            return false;
        }

        let now = Instant::now();
        match self.idle_since {
            None => {
                self.idle_since = Some(now);
                self.debounce_window.is_zero()
            }
            Some(since) => now.duration_since(since) >= self.debounce_window,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_csi_and_osc_sequences() {
        let raw = "\x1b[38;5;39m⣷  Running command...\x1b[0m\r\n\x1b]0;agy\x07> hello";
        let clean = strip_ansi_codes(raw);
        assert_eq!(clean, "⣷  Running command...\n> hello");
    }

    #[test]
    fn detects_braille_spinners_and_horizontal_rules() {
        assert!(contains_braille_spinner("⣷  Running command..."));
        assert!(contains_braille_spinner("⠋ Thinking"));
        assert!(!contains_braille_spinner("> Ready for input"));

        assert!(is_horizontal_rule_line(
            "────────────────────────────────────────"
        ));
        assert!(!is_horizontal_rule_line("---"));
    }

    #[test]
    fn quiescence_requires_activity_and_debounce() {
        let mut detector = QuiescenceDetector::new(Duration::from_millis(20));
        // Initial idle screen without activity does not trigger completion
        assert!(!detector.observe(">\n───", true, false));
        // Working screen marks activity
        assert!(!detector.observe("⣷ Running...\nesc to cancel", false, false));
        // First observation back at idle prompt starts debounce timer
        assert!(!detector.observe("Done!\n───\n>\n───", true, false));
        std::thread::sleep(Duration::from_millis(25));
        assert!(detector.observe("Done!\n───\n>\n───", true, false));
    }
}
