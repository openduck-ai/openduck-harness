use openduck_providers::conversation::message::{Message, MessageContent};
use rmcp::model::Role;

pub const DEFAULT_MAX_TOOL_OUTPUT_LINES: usize = 60;
pub const DEFAULT_MAX_TOOL_OUTPUT_BYTES: usize = 12 * 1024; // 12 KB
pub const DEFAULT_HEAD_SNIP_LINES: usize = 40;
pub const DEFAULT_TAIL_SNIP_LINES: usize = 10;

/// Tier 1: Zero-cost pure string snip/truncation for large tool outputs.
///
/// If a tool output (such as search results or shell logs) exceeds line or byte limits,
/// this retains the head and tail lines with a clear structured omission marker.
pub fn snip_tool_output(output: &str, max_lines: usize, max_bytes: usize) -> (String, bool) {
    if output.len() <= max_bytes && output.lines().count() <= max_lines {
        return (output.to_string(), false);
    }

    let lines: Vec<&str> = output.lines().collect();
    let total_lines = lines.len();

    if total_lines <= max_lines && output.len() > max_bytes {
        // Line count is small but individual lines are huge
        let mut truncated = String::with_capacity(max_bytes + 200);
        let mut byte_count = 0;
        let mut lines_included = 0;

        for line in &lines {
            if byte_count + line.len() > max_bytes {
                break;
            }
            truncated.push_str(line);
            truncated.push('\n');
            byte_count += line.len() + 1;
            lines_included += 1;
        }

        let omitted_lines = total_lines.saturating_sub(lines_included);
        let omitted_bytes = output.len().saturating_sub(byte_count);

        truncated.push_str(&format!(
            "\n[... {omitted_lines} lines ({omitted_bytes} bytes) snipped by OpenDuck context manager to save tokens ...]\n"
        ));

        return (truncated, true);
    }

    let head_count = DEFAULT_HEAD_SNIP_LINES.min(max_lines.saturating_sub(DEFAULT_TAIL_SNIP_LINES));
    let tail_count = DEFAULT_TAIL_SNIP_LINES.min(total_lines.saturating_sub(head_count));

    let head_lines = &lines[..head_count.min(total_lines)];
    let tail_lines = if total_lines > head_count + tail_count {
        &lines[total_lines - tail_count..]
    } else {
        &[]
    };

    let mut result = String::with_capacity(max_bytes.min(output.len()) + 256);

    for line in head_lines {
        result.push_str(line);
        result.push('\n');
    }

    let omitted_lines = total_lines.saturating_sub(head_lines.len() + tail_lines.len());
    let head_bytes: usize = head_lines.iter().map(|l| l.len() + 1).sum();
    let tail_bytes: usize = tail_lines.iter().map(|l| l.len() + 1).sum();
    let omitted_bytes = output.len().saturating_sub(head_bytes + tail_bytes);

    result.push_str(&format!(
        "\n[... {omitted_lines} lines ({omitted_bytes} bytes) snipped by OpenDuck context manager to save tokens ...]\n\n"
    ));

    for line in tail_lines {
        result.push_str(line);
        result.push('\n');
    }

    (result, true)
}

/// Identifies if a message belongs to a Protected Zone (never discarded/compressed aggressively).
pub fn is_protected_zone_message(msg: &Message) -> bool {
    // 1. User messages are protected
    if matches!(msg.role, Role::User) {
        return true;
    }

    // 2. Action required / Elicitation / System notifications are protected
    for content in &msg.content {
        match content {
            MessageContent::ActionRequired(_)
            | MessageContent::ToolConfirmationRequest(_)
            | MessageContent::SystemNotification(_) => return true,
            _ => {}
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snip_tool_output_short() {
        let text = "line 1\nline 2\nline 3";
        let (snipped, was_snipped) = snip_tool_output(text, 10, 1024);
        assert_eq!(snipped, text);
        assert!(!was_snipped);
    }

    #[test]
    fn test_snip_tool_output_long_lines() {
        let lines: Vec<String> = (1..=100).map(|i| format!("log entry {i}")).collect();
        let text = lines.join("\n");
        let (snipped, was_snipped) = snip_tool_output(&text, 20, 1024 * 1024);
        assert!(was_snipped);
        assert!(snipped.contains("log entry 1"));
        assert!(snipped.contains("log entry 10"));
        assert!(snipped.contains("log entry 100"));
        assert!(snipped.contains("80 lines"));
        assert!(snipped.contains("snipped by OpenDuck context manager"));
    }
}
