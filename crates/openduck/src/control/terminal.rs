use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::files::resolve_project_path;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_TIMEOUT_SECS: u64 = 120;
const MAX_OUTPUT_BYTES: usize = 512 * 1024; // 512 KB

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecCommandRequest {
    pub command: String,
    pub cwd: Option<String>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecCommandResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub duration_ms: u64,
    pub cwd: String,
}

pub async fn execute_project_command(
    project_root: &Path,
    req: ExecCommandRequest,
) -> Result<ExecCommandResponse> {
    let working_dir = if let Some(cwd_rel) = req.cwd.as_deref().filter(|s| !s.trim().is_empty()) {
        resolve_project_path(project_root, cwd_rel)?
    } else {
        project_root.canonicalize()?
    };

    if !working_dir.is_dir() {
        return Err(anyhow!("Working directory is not valid"));
    }

    let timeout_duration = Duration::from_secs(
        req.timeout_secs
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .clamp(1, MAX_TIMEOUT_SECS),
    );

    let start = Instant::now();

    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.args(["/C", &req.command]);
        c
    };

    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.args(["-c", &req.command]);
        c
    };

    cmd.current_dir(&working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(true);

    let output_result = tokio::time::timeout(timeout_duration, cmd.output()).await;

    let duration_ms = start.elapsed().as_millis() as u64;

    let output = match output_result {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(anyhow!("Failed to run command: {e}")),
        Err(_) => {
            return Ok(ExecCommandResponse {
                stdout: String::new(),
                stderr: format!("Command timed out after {}s", timeout_duration.as_secs()),
                exit_code: Some(124),
                success: false,
                duration_ms,
                cwd: working_dir.to_string_lossy().into_owned(),
            });
        }
    };

    let mut stdout_bytes = output.stdout;
    if stdout_bytes.len() > MAX_OUTPUT_BYTES {
        stdout_bytes.truncate(MAX_OUTPUT_BYTES);
    }
    let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();

    let mut stderr_bytes = output.stderr;
    if stderr_bytes.len() > MAX_OUTPUT_BYTES {
        stderr_bytes.truncate(MAX_OUTPUT_BYTES);
    }
    let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();

    let exit_code = output.status.code();
    let success = output.status.success();

    Ok(ExecCommandResponse {
        stdout,
        stderr,
        exit_code,
        success,
        duration_ms,
        cwd: working_dir.to_string_lossy().into_owned(),
    })
}

use once_cell::sync::Lazy;
use openduck_mcp::tui_session::{
    TuiSessionConfig, TuiSessionManager, TuiSessionSummary, TuiTurnResult,
};
use std::sync::Arc;

static TUI_SESSION_MANAGER: Lazy<Arc<TuiSessionManager>> =
    Lazy::new(|| Arc::new(TuiSessionManager::default()));

pub fn get_tui_session_manager() -> Arc<TuiSessionManager> {
    TUI_SESSION_MANAGER.clone()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTuiSessionRequest {
    pub agent: String,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub auto_approve: Option<bool>,
    pub resume_id: Option<String>,
    pub initial_prompt: Option<String>,
    pub session_id: Option<String>,
    pub extra_args: Option<Vec<String>>,
    pub https_proxy: Option<String>,
    pub idle_timeout_mins: Option<u64>,
    pub exited_retention_mins: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTuiSessionRequest {
    pub prompt: String,
    pub wait: Option<bool>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeysTuiSessionRequest {
    pub keys: Vec<String>,
}

pub async fn start_project_tui_session(
    project_root: &Path,
    req: StartTuiSessionRequest,
) -> Result<serde_json::Value> {
    let working_dir = if let Some(cwd_rel) = req.cwd.as_deref().filter(|s| !s.trim().is_empty()) {
        resolve_project_path(project_root, cwd_rel)?
    } else {
        project_root.canonicalize()?
    };

    let kind = std::str::FromStr::from_str(&req.agent)
        .map_err(|e: anyhow::Error| anyhow!(e.to_string()))?;

    let session_id = req
        .session_id
        .unwrap_or_else(|| format!("{}_{}", kind, chrono::Utc::now().timestamp_millis()));

    let config = TuiSessionConfig {
        session_id: session_id.clone(),
        kind,
        cwd: working_dir,
        model: req.model,
        effort: req.effort,
        auto_approve: req.auto_approve.unwrap_or(true),
        resume_id: req.resume_id,
        cols: 160,
        rows: 48,
        extra_args: req.extra_args.unwrap_or_default(),
        https_proxy: req.https_proxy,
        env: Vec::new(),
        idle_timeout_mins: req.idle_timeout_mins,
        exited_retention_mins: req.exited_retention_mins,
    };

    let manager = get_tui_session_manager();
    let handle = manager.start_session(config).await?;
    let current_state = handle.last_state.lock().await.clone();

    let mut initial_turn = None;
    if let Some(prompt) = req.initial_prompt {
        if !matches!(
            current_state,
            openduck_mcp::tui_session::TuiPaneState::WaitingApproval { .. }
        ) {
            if let Ok(turn) = handle
                .send_prompt(&prompt, true, Duration::from_secs(600))
                .await
            {
                initial_turn = Some(turn);
            }
        }
    }

    let screen = handle
        .backend
        .capture_screen(&session_id, false, Some(30))
        .await
        .unwrap_or_default();

    Ok(serde_json::json!({
        "sessionId": session_id,
        "agent": kind.to_string(),
        "pid": handle.pid,
        "attachCommand": handle.backend.attach_command(&session_id),
        "initialScreen": screen,
        "initialTurn": initial_turn,
        "state": current_state,
    }))
}

pub async fn list_project_tui_sessions(
    _project_root: &Path,
    preview: bool,
) -> Result<Vec<TuiSessionSummary>> {
    let manager = get_tui_session_manager();
    Ok(manager.list_sessions(preview).await)
}

pub async fn reclaim_project_tui_sessions() -> Result<Vec<String>> {
    let manager = get_tui_session_manager();
    Ok(manager.reclaim_stale_sessions().await)
}

pub async fn prompt_project_tui_session(
    session_id: &str,
    req: PromptTuiSessionRequest,
) -> Result<TuiTurnResult> {
    let manager = get_tui_session_manager();
    let handle = manager.get_session(session_id).await?;
    let timeout = Duration::from_secs(req.timeout_secs.unwrap_or(600));
    handle
        .send_prompt(&req.prompt, req.wait.unwrap_or(true), timeout)
        .await
}

pub async fn keys_project_tui_session(session_id: &str, req: KeysTuiSessionRequest) -> Result<()> {
    let manager = get_tui_session_manager();
    let handle = manager.get_session(session_id).await?;
    handle.send_keys(&req.keys).await
}

pub async fn read_project_tui_session(
    session_id: &str,
    scrollback_lines: Option<i32>,
    include_ansi: bool,
) -> Result<TuiTurnResult> {
    let manager = get_tui_session_manager();
    let handle = manager.get_session(session_id).await?;
    handle.read_state(scrollback_lines, include_ansi).await
}

pub async fn stop_project_tui_session(session_id: &str) -> Result<()> {
    let manager = get_tui_session_manager();
    manager.stop_session(session_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_exec_echo_command() {
        let dir = tempdir().unwrap();
        let res = execute_project_command(
            dir.path(),
            ExecCommandRequest {
                command: "echo hello goose".to_string(),
                cwd: None,
                timeout_secs: Some(5),
            },
        )
        .await
        .unwrap();

        assert!(res.success);
        assert_eq!(res.exit_code, Some(0));
        assert!(res.stdout.contains("hello goose"));
    }
}
