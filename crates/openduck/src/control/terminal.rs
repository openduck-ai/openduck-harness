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
