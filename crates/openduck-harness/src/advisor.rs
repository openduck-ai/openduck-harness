use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvisorKind {
    Agy,
    Grok,
    Openduck,
}

impl AdvisorKind {
    pub fn from_bin(bin: &str) -> Option<Self> {
        match bin.to_ascii_lowercase().as_str() {
            "agy" => Some(Self::Agy),
            "grok" => Some(Self::Grok),
            "openduck" | "goose" | "duck" => Some(Self::Openduck),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisorCommand {
    pub cmdline: String,
    pub session_id: Option<String>,
}

pub fn json_truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(true)) => true,
        Some(Value::Number(n)) => n.as_u64() == Some(1) || n.as_i64() == Some(1),
        Some(Value::String(s)) => {
            matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")
        }
        _ => false,
    }
}

pub fn build_advisor_command(
    kind: AdvisorKind,
    prompt: &str,
    resume_session: Option<&str>,
) -> AdvisorCommand {
    let prompt_q = quote(prompt);
    match kind {
        AdvisorKind::Grok => {
            if let Some(session_id) = resume_session.map(str::trim).filter(|s| !s.is_empty()) {
                AdvisorCommand {
                    cmdline: format!(
                        "grok --always-approve --no-subagents --reasoning-effort low --output-format json --resume {} -p {}",
                        quote(session_id),
                        prompt_q
                    ),
                    session_id: Some(session_id.to_string()),
                }
            } else {
                let session_id = Uuid::new_v4().to_string();
                AdvisorCommand {
                    cmdline: format!(
                        "grok --always-approve --no-subagents --reasoning-effort low --output-format json --session-id {} -p {}",
                        quote(&session_id),
                        prompt_q
                    ),
                    session_id: Some(session_id),
                }
            }
        }
        AdvisorKind::Agy => {
            if let Some(session_id) = resume_session.map(str::trim).filter(|s| !s.is_empty()) {
                AdvisorCommand {
                    cmdline: format!(
                        "agy --dangerously-skip-permissions --effort low --output-format json --conversation {} -p {}",
                        quote(session_id),
                        prompt_q
                    ),
                    session_id: Some(session_id.to_string()),
                }
            } else {
                AdvisorCommand {
                    cmdline: format!(
                        "agy --dangerously-skip-permissions --effort low --output-format json -p {prompt_q}"
                    ),
                    session_id: None,
                }
            }
        }
        AdvisorKind::Openduck => {
            let session_id = resume_session
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("openduck-advisor-{}", Uuid::new_v4()));
            let name_q = quote(&session_id);
            let cmdline = if resume_session.map(str::trim).is_some_and(|s| !s.is_empty()) {
                format!("openduck run --resume --name {name_q} -q -t {prompt_q}")
            } else {
                format!("openduck run --name {name_q} -q -t {prompt_q}")
            };
            AdvisorCommand {
                cmdline,
                session_id: Some(session_id),
            }
        }
    }
}

pub fn parse_advisor_session_id(kind: AdvisorKind, stdout: &str) -> Option<String> {
    match kind {
        AdvisorKind::Grok => parse_grok_session_id(stdout),
        AdvisorKind::Agy => parse_agy_conversation_id(stdout),
        AdvisorKind::Openduck => None,
    }
}

pub fn advisor_visible_output(kind: AdvisorKind, stdout: &str) -> String {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match kind {
        AdvisorKind::Grok => extract_grok_text(trimmed).unwrap_or_else(|| trimmed.to_string()),
        AdvisorKind::Agy => extract_agy_text(trimmed).unwrap_or_else(|| trimmed.to_string()),
        AdvisorKind::Openduck => trimmed.to_string(),
    }
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn json_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        match value.get(*key) {
            Some(Value::String(s)) if !s.trim().is_empty() => return Some(s.clone()),
            Some(Value::Null) | None => {}
            Some(other) => {
                let rendered = other.to_string();
                if rendered != "null" && !rendered.is_empty() {
                    return Some(rendered.trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

fn parse_json_objects(stdout: &str) -> Vec<Value> {
    let mut objects = Vec::new();
    if let Ok(value) = serde_json::from_str::<Value>(stdout) {
        objects.push(value);
        return objects;
    }
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            objects.push(value);
        }
    }
    objects
}

fn parse_grok_session_id(stdout: &str) -> Option<String> {
    let mut found = None;
    for value in parse_json_objects(stdout) {
        if let Some(id) = json_string(&value, &["sessionId", "session_id"]) {
            found = Some(id);
        }
    }
    found
}

fn parse_agy_conversation_id(stdout: &str) -> Option<String> {
    let mut found = None;
    for value in parse_json_objects(stdout) {
        let payload = if value.get("event").and_then(Value::as_str) == Some("result") {
            value.get("result").cloned().unwrap_or(value)
        } else {
            value
        };
        if let Some(id) = json_string(&payload, &["conversation_id", "conversationId"]) {
            found = Some(id);
        }
    }
    found
}

fn extract_grok_text(stdout: &str) -> Option<String> {
    let objects = parse_json_objects(stdout);
    if objects.is_empty() {
        return None;
    }
    if let Some(text) = objects
        .first()
        .and_then(|value| json_string(value, &["text"]))
        .filter(|s| !s.is_empty())
    {
        return Some(text);
    }
    let mut accumulated = String::new();
    for value in objects {
        if value.get("type").and_then(Value::as_str) == Some("text") {
            if let Some(piece) = json_string(&value, &["data", "text"]) {
                accumulated.push_str(&piece);
            }
        }
    }
    if accumulated.is_empty() {
        None
    } else {
        Some(accumulated)
    }
}

fn extract_agy_text(stdout: &str) -> Option<String> {
    let mut last_response = None;
    for value in parse_json_objects(stdout) {
        let payload = if value.get("event").and_then(Value::as_str) == Some("result") {
            value.get("result").cloned().unwrap_or(value)
        } else {
            value
        };
        if let Some(text) = json_string(&payload, &["response", "text"]) {
            last_response = Some(text);
        }
    }
    last_response.filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag_value(cmd: &str, flag: &str) -> Option<String> {
        let needle = format!("{flag} ");
        let rest = cmd.split(&needle).nth(1)?;
        let token = rest.split_whitespace().next()?;
        Some(token.trim_matches('\'').to_string())
    }

    #[test]
    fn grok_first_call_pins_session_id_and_skips_resume() {
        let cmd = build_advisor_command(AdvisorKind::Grok, "review this", None);
        assert!(cmd.cmdline.contains("--session-id"));
        assert!(!cmd.cmdline.contains("--resume"));
        assert!(!cmd.cmdline.contains("--continue"));
        assert!(cmd.cmdline.contains("--output-format json"));
        assert!(cmd.cmdline.contains("-p 'review this'"));
        let sid = cmd.session_id.expect("generated grok session");
        assert_eq!(
            flag_value(&cmd.cmdline, "--session-id").as_deref(),
            Some(sid.as_str())
        );
        assert!(Uuid::parse_str(&sid).is_ok());
    }

    #[test]
    fn grok_follow_up_resumes_explicit_session() {
        let cmd = build_advisor_command(AdvisorKind::Grok, "what next?", Some("abc-123"));
        assert!(cmd.cmdline.contains("--resume 'abc-123'"));
        assert!(!cmd.cmdline.contains("--session-id"));
        assert!(!cmd.cmdline.contains("--continue"));
        assert_eq!(cmd.session_id.as_deref(), Some("abc-123"));
    }

    #[test]
    fn agy_first_call_has_no_conversation_and_print_is_last() {
        let cmd = build_advisor_command(AdvisorKind::Agy, "review this", None);
        assert!(!cmd.cmdline.contains("--conversation"));
        assert!(!cmd.cmdline.contains("--continue"));
        assert!(cmd.cmdline.contains("--output-format json"));
        assert!(cmd.cmdline.ends_with("-p 'review this'"));
        assert!(cmd.session_id.is_none());
    }

    #[test]
    fn agy_follow_up_resumes_conversation_before_print() {
        let cmd = build_advisor_command(AdvisorKind::Agy, "next", Some("conv-9"));
        assert!(cmd.cmdline.contains("--conversation 'conv-9'"));
        assert!(cmd.cmdline.ends_with("-p 'next'"));
        assert!(!cmd.cmdline.contains("--continue"));
        assert_eq!(cmd.session_id.as_deref(), Some("conv-9"));
    }

    #[test]
    fn openduck_uses_named_session_instead_of_no_session() {
        let first = build_advisor_command(AdvisorKind::Openduck, "hello", None);
        assert!(first.cmdline.contains("openduck run --name"));
        assert!(!first.cmdline.contains("--no-session"));
        assert!(!first.cmdline.contains("--resume"));
        let name = first.session_id.expect("generated openduck name");
        assert!(name.starts_with("openduck-advisor-"));

        let resume = build_advisor_command(AdvisorKind::Openduck, "hello again", Some(&name));
        assert!(resume.cmdline.contains("--resume"));
        assert!(resume.cmdline.contains(&format!("--name '{name}'")));
        assert!(!resume.cmdline.contains("--no-session"));
        assert_eq!(resume.session_id.as_deref(), Some(name.as_str()));
    }

    #[test]
    fn parse_grok_json_and_ndjson_session_ids() {
        assert_eq!(
            parse_advisor_session_id(
                AdvisorKind::Grok,
                r#"{"text":"done","stopReason":"end_turn","sessionId":"sess-1"}"#
            )
            .as_deref(),
            Some("sess-1")
        );
        let ndjson = "\
{\"type\":\"text\",\"data\":\"Hello \"}\n\
{\"type\":\"end\",\"stopReason\":\"end_turn\",\"sessionId\":\"sess-stream-1\"}\n";
        assert_eq!(
            parse_advisor_session_id(AdvisorKind::Grok, ndjson).as_deref(),
            Some("sess-stream-1")
        );
    }

    #[test]
    fn parse_agy_conversation_from_object_and_result_event() {
        assert_eq!(
            parse_advisor_session_id(
                AdvisorKind::Agy,
                r#"{"conversation_id":"sess-1","status":"SUCCESS","response":"done"}"#
            )
            .as_deref(),
            Some("sess-1")
        );
        assert_eq!(
            parse_advisor_session_id(
                AdvisorKind::Agy,
                r#"{"event":"result","result":{"conversation_id":"conv-2","response":"ok"}}"#
            )
            .as_deref(),
            Some("conv-2")
        );
    }

    #[test]
    fn visible_output_extracts_json_text() {
        assert_eq!(
            advisor_visible_output(AdvisorKind::Grok, r#"{"text":"ship it","sessionId":"s1"}"#),
            "ship it"
        );
        assert_eq!(
            advisor_visible_output(
                AdvisorKind::Agy,
                r#"{"conversation_id":"c1","response":"look at runtime.rs"}"#
            ),
            "look at runtime.rs"
        );
        assert_eq!(
            advisor_visible_output(AdvisorKind::Openduck, "plain advice\n"),
            "plain advice"
        );
    }

    #[test]
    fn json_truthy_accepts_common_fresh_encodings() {
        assert!(json_truthy(Some(&Value::Bool(true))));
        assert!(json_truthy(Some(&Value::String("true".into()))));
        assert!(json_truthy(Some(&serde_json::json!(1))));
        assert!(!json_truthy(Some(&Value::Bool(false))));
        assert!(!json_truthy(None));
    }

    #[test]
    fn known_bins_include_openduck_aliases() {
        assert_eq!(AdvisorKind::from_bin("agy"), Some(AdvisorKind::Agy));
        assert_eq!(AdvisorKind::from_bin("grok"), Some(AdvisorKind::Grok));
        assert_eq!(
            AdvisorKind::from_bin("openduck"),
            Some(AdvisorKind::Openduck)
        );
        assert_eq!(AdvisorKind::from_bin("goose"), Some(AdvisorKind::Openduck));
        assert_eq!(AdvisorKind::from_bin("duck"), Some(AdvisorKind::Openduck));
        assert_eq!(AdvisorKind::from_bin("echo"), None);
    }
}
