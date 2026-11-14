use super::{
    detect_approval_modal, detect_common_working_status, detect_composer_idle_prompt,
    detect_quota_limit, strip_common_tui_chrome, switch_via_slash_command, user_home, AgentCliKind,
    AgentTuiProfile, TuiPaneState, TuiSessionConfig,
};
use crate::tui_session::backend::MultiplexerBackend;
use crate::tui_session::screen::{is_horizontal_rule_line, ScreenSnapshot};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
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

#[async_trait]
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
        if let Some(status) = detect_common_working_status(&bottom) {
            return TuiPaneState::Working {
                status_line: Some(status),
            };
        }
        if self.composer_text(screen_text).is_some() || detect_composer_idle_prompt(&bottom) {
            return TuiPaneState::IdlePrompt;
        }
        TuiPaneState::Initializing
    }

    fn strip_chrome(&self, captured_scrollback: &str, submitted_prompt: &str) -> String {
        strip_common_tui_chrome(captured_scrollback, submitted_prompt)
    }

    fn composer_text(&self, screen_text: &str) -> Option<String> {
        let snapshot = ScreenSnapshot::new(screen_text.to_string());
        let lines = snapshot.lines();
        let bottom = lines
            .iter()
            .rposition(|line| is_horizontal_rule_line(line))?;
        let top = lines[..bottom]
            .iter()
            .rposition(|line| is_horizontal_rule_line(line))?;
        let mut composer = lines[top + 1..bottom].iter();
        let first = composer.next()?.trim_start().strip_prefix('>')?;
        Some(
            std::iter::once(first)
                .chain(composer.copied())
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string(),
        )
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

    fn supports_runtime_model_switch(&self) -> bool {
        true
    }

    async fn switch_runtime_model(
        &self,
        backend: &dyn MultiplexerBackend,
        session_name: &str,
        new_model: &str,
    ) -> Result<String> {
        switch_via_slash_command(backend, session_name, new_model).await
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgyQuotaPool {
    pub model_pattern: String,
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub reset_at: Option<String>,
    pub is_exhausted: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgyQuotaInfo {
    pub pools: Vec<AgyQuotaPool>,
    pub account_email: Option<String>,
}

const AGY_FETCH_MODELS_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels";
const AGY_USER_AGENT: &str = "antigravity/1.104.0 linux/amd64";

pub fn parse_agy_available_models_quotas(
    payload: &Value,
    account_email: Option<String>,
) -> Option<AgyQuotaInfo> {
    let models = payload.get("models")?.as_object()?;
    let mut grouped: std::collections::BTreeMap<&'static str, (f64, Option<String>)> =
        std::collections::BTreeMap::new();

    for (model_id, entry) in models {
        let Some(display_name) = entry.get("displayName").and_then(Value::as_str) else {
            continue;
        };
        if display_name.trim().is_empty() {
            continue;
        }
        let Some(quota_info) = entry.get("quotaInfo") else {
            continue;
        };
        let Some(remaining_fraction) = quota_info.get("remainingFraction").and_then(Value::as_f64)
        else {
            continue;
        };
        let reset_time = quota_info
            .get("resetTime")
            .and_then(Value::as_str)
            .map(ToString::to_string);

        let group = if model_id.starts_with("gemini-") {
            "gemini-*"
        } else if model_id.starts_with("claude-") || model_id.starts_with("gpt-oss-") {
            "claude-*"
        } else {
            "other"
        };

        grouped
            .entry(group)
            .and_modify(|(min_frac, existing_reset)| {
                if remaining_fraction < *min_frac {
                    *min_frac = remaining_fraction;
                    *existing_reset = reset_time.clone();
                } else if existing_reset.is_none() {
                    *existing_reset = reset_time.clone();
                }
            })
            .or_insert((remaining_fraction, reset_time));
    }

    if grouped.is_empty() {
        return None;
    }

    let pools = grouped
        .into_iter()
        .map(|(pattern, (min_frac, reset_at))| {
            let remaining_percent = (min_frac * 100.0).clamp(0.0, 100.0);
            let used_percent = (100.0 - remaining_percent).clamp(0.0, 100.0);
            AgyQuotaPool {
                model_pattern: pattern.to_string(),
                used_percent,
                remaining_percent,
                reset_at,
                is_exhausted: remaining_percent <= 0.0,
            }
        })
        .collect();

    Some(AgyQuotaInfo {
        pools,
        account_email,
    })
}

fn read_agy_oauth_token(agy_home: &Path) -> Result<(String, Option<String>)> {
    let token_path = agy_home.join("antigravity-oauth-token");
    let raw = fs::read_to_string(&token_path).map_err(|e| {
        anyhow!(
            "Failed to read Antigravity OAuth token at {}: {e}",
            token_path.display()
        )
    })?;
    let val: Value = serde_json::from_str(&raw)?;
    let access_token = val
        .pointer("/token/access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Missing access_token in {}", token_path.display()))?
        .to_string();
    let email = val
        .get("id_token_email")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    Ok((access_token, email))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgyQuotaProxy<'a> {
    #[default]
    SystemEnv,
    Direct,
    Url(&'a str),
}

fn build_agy_http_client(proxy: AgyQuotaProxy<'_>) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15));
    match proxy {
        AgyQuotaProxy::SystemEnv => {}
        AgyQuotaProxy::Direct => {
            builder = builder.no_proxy();
        }
        AgyQuotaProxy::Url(url) => {
            let trimmed = url.trim();
            if !trimmed.is_empty() {
                builder = builder.proxy(reqwest::Proxy::all(trimmed)?);
            }
        }
    }
    Ok(builder.build()?)
}

async fn refresh_agy_token_via_cli(proxy: AgyQuotaProxy<'_>) {
    let Ok(bin) = AgyTuiProfile::resolve_binary() else {
        return;
    };
    let mut cmd = tokio::process::Command::new(bin);
    cmd.arg("models")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    match proxy {
        AgyQuotaProxy::SystemEnv => {}
        AgyQuotaProxy::Direct => {
            for key in [
                "HTTPS_PROXY",
                "https_proxy",
                "HTTP_PROXY",
                "http_proxy",
                "ALL_PROXY",
                "all_proxy",
            ] {
                cmd.env_remove(key);
            }
        }
        AgyQuotaProxy::Url(url) => {
            let trimmed = url.trim();
            if !trimmed.is_empty() {
                for key in [
                    "HTTPS_PROXY",
                    "https_proxy",
                    "HTTP_PROXY",
                    "http_proxy",
                    "ALL_PROXY",
                    "all_proxy",
                ] {
                    cmd.env(key, trimmed);
                }
            }
        }
    }
    let _ = tokio::time::timeout(std::time::Duration::from_secs(15), cmd.status()).await;
}

pub async fn read_agy_quota_info() -> Result<AgyQuotaInfo> {
    read_agy_quota_info_with_proxy(AgyQuotaProxy::SystemEnv).await
}

pub async fn read_agy_quota_info_with_proxy(proxy: AgyQuotaProxy<'_>) -> Result<AgyQuotaInfo> {
    let home = AgyTuiProfile::agy_home();
    let (mut access_token, mut email) = read_agy_oauth_token(&home)?;

    let client = build_agy_http_client(proxy)?;

    let mut resp = client
        .post(AGY_FETCH_MODELS_URL)
        .bearer_auth(&access_token)
        .header(reqwest::header::USER_AGENT, AGY_USER_AGENT)
        .json(&serde_json::json!({}))
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED
        || resp.status() == reqwest::StatusCode::FORBIDDEN
    {
        refresh_agy_token_via_cli(proxy).await;
        if let Ok((new_token, new_email)) = read_agy_oauth_token(&home) {
            access_token = new_token;
            if new_email.is_some() {
                email = new_email;
            }
            resp = client
                .post(AGY_FETCH_MODELS_URL)
                .bearer_auth(&access_token)
                .header(reqwest::header::USER_AGENT, AGY_USER_AGENT)
                .json(&serde_json::json!({}))
                .send()
                .await?;
        }
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Antigravity quota API returned {status}: {}",
            body.chars().take(200).collect::<String>()
        ));
    }

    let payload: Value = resp.json().await?;
    parse_agy_available_models_quotas(&payload, email)
        .ok_or_else(|| anyhow!("No quotaInfo entries returned by Antigravity API"))
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

    #[test]
    fn agy_parses_available_models_quotas_into_model_pools() {
        let payload = serde_json::json!({
            "models": {
                "gemini-3-flash": {
                    "displayName": "Gemini 3 Flash",
                    "quotaInfo": {
                        "remainingFraction": 0.96,
                        "resetTime": "2026-11-14T10:57:17Z"
                    }
                },
                "claude-sonnet-5-5-high": {
                    "displayName": "Claude Sonnet 5.5 (High)",
                    "quotaInfo": {
                        "remainingFraction": 0.38,
                        "resetTime": "2026-11-20T14:57:30Z"
                    }
                },
                "chat_23310": {
                    "quotaInfo": { "remainingFraction": 1.0 }
                }
            }
        });
        let info =
            parse_agy_available_models_quotas(&payload, Some("user@example.com".to_string()))
                .expect("quota info");
        assert_eq!(info.account_email.as_deref(), Some("user@example.com"));
        assert_eq!(info.pools.len(), 2);
        assert_eq!(info.pools[0].model_pattern, "claude-*");
        assert!((info.pools[0].remaining_percent - 38.0).abs() < 0.01);
        assert_eq!(info.pools[1].model_pattern, "gemini-*");
        assert!((info.pools[1].remaining_percent - 96.0).abs() < 0.01);
    }

    #[test]
    fn agy_http_client_accepts_proxy_modes() {
        assert!(build_agy_http_client(AgyQuotaProxy::SystemEnv).is_ok());
        assert!(build_agy_http_client(AgyQuotaProxy::Direct).is_ok());
        assert!(build_agy_http_client(AgyQuotaProxy::Url("http://127.0.0.1:1096")).is_ok());
        assert!(build_agy_http_client(AgyQuotaProxy::Url("socks5://127.0.0.1:1096")).is_ok());
    }
}
