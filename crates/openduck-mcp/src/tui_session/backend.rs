use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub const DEFAULT_TMUX_SOCKET: &str = "openduck";
pub const DEFAULT_HISTORY_LIMIT: usize = 50_000;

#[async_trait]
pub trait MultiplexerBackend: Send + Sync {
    async fn spawn_pane(
        &self,
        id: &str,
        cmd: &[String],
        cwd: &Path,
        cols: u16,
        rows: u16,
        env_vars: &[(String, String)],
    ) -> Result<u32>;
    async fn paste_text(&self, id: &str, text: &str, bracketed: bool) -> Result<()>;
    async fn send_keys(&self, id: &str, keys: &[String]) -> Result<()>;
    async fn capture_screen(
        &self,
        id: &str,
        include_ansi: bool,
        scrollback_lines: Option<i32>,
    ) -> Result<String>;
    async fn history_size(&self, id: &str) -> Result<usize>;
    async fn is_alive(&self, id: &str) -> Result<bool>;
    async fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()>;
    async fn kill_pane(&self, id: &str) -> Result<()>;
    fn attach_command(&self, id: &str) -> String;
}

#[derive(Debug, Clone)]
pub struct TmuxBackend {
    socket_name: String,
}

impl Default for TmuxBackend {
    fn default() -> Self {
        Self::new(
            env::var("OPENDUCK_TMUX_SOCKET")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_TMUX_SOCKET.to_string()),
        )
    }
}

impl TmuxBackend {
    pub fn new(socket_name: impl Into<String>) -> Self {
        Self {
            socket_name: socket_name.into(),
        }
    }

    pub fn socket_name(&self) -> &str {
        &self.socket_name
    }

    fn resolve_tmux_binary() -> Result<PathBuf> {
        if let Ok(explicit) = env::var("TMUX_COMMAND") {
            let path = PathBuf::from(explicit);
            if path.is_file() {
                return Ok(path);
            }
        }
        if let Ok(path) = which::which("tmux") {
            return Ok(path);
        }
        for candidate in [
            "/usr/bin/tmux",
            "/usr/local/bin/tmux",
            "/opt/homebrew/bin/tmux",
        ] {
            let p = PathBuf::from(candidate);
            if p.is_file() {
                return Ok(p);
            }
        }
        Err(anyhow!(
            "Could not find `tmux` on PATH. Install tmux (e.g. `apt install tmux` or `brew install tmux`) to use persistent TUI agent sessions."
        ))
    }

    fn base_command(&self) -> Result<Command> {
        let bin = Self::resolve_tmux_binary()?;
        let mut cmd = Command::new(bin);
        cmd.arg("-L").arg(&self.socket_name);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        Ok(cmd)
    }

    async fn run_tmux(&self, args: &[&str]) -> Result<String> {
        let mut cmd = self.base_command()?;
        cmd.args(args);
        let output = cmd.output().await.context("Failed to execute tmux")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(anyhow!(
                "tmux {:?} exited with {}: {}",
                args,
                output.status.code().unwrap_or(-1),
                detail
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

#[async_trait]
impl MultiplexerBackend for TmuxBackend {
    async fn spawn_pane(
        &self,
        id: &str,
        cmd: &[String],
        cwd: &Path,
        cols: u16,
        rows: u16,
        env_vars: &[(String, String)],
    ) -> Result<u32> {
        if cmd.is_empty() {
            return Err(anyhow!("Cannot spawn tmux pane with empty command"));
        }

        // Clean up any stale session with the same ID first.
        let _ = self.run_tmux(&["kill-session", "-t", id]).await;

        let bin = Self::resolve_tmux_binary()?;
        let mut tmux_cmd = Command::new(bin);
        tmux_cmd
            .arg("-L")
            .arg(&self.socket_name)
            .arg("-f")
            .arg("/dev/null")
            .arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(id)
            .arg("-x")
            .arg(cols.to_string())
            .arg("-y")
            .arg(rows.to_string())
            .arg("-c")
            .arg(cwd);

        tmux_cmd.arg("-e").arg("TERM=xterm-256color");
        tmux_cmd.arg("-e").arg(format!("OPENDUCK_SESSION_ID={id}"));
        tmux_cmd
            .arg("-e")
            .arg(format!("OPENDUCK_OWNER_PID={}", std::process::id()));

        for (k, v) in env_vars {
            tmux_cmd.arg("-e").arg(format!("{k}={v}"));
        }

        let shell_cmd = shell_words::join(cmd);
        tmux_cmd.arg(&shell_cmd);

        tmux_cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let out = tmux_cmd
            .output()
            .await
            .context("Failed to spawn tmux session")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(anyhow!("tmux new-session failed: {stderr}"));
        }

        let _ = self
            .run_tmux(&[
                "set-option",
                "-t",
                id,
                "history-limit",
                &DEFAULT_HISTORY_LIMIT.to_string(),
            ])
            .await;
        let _ = self
            .run_tmux(&["set-option", "-t", id, "status", "off"])
            .await;

        let pid_str = self
            .run_tmux(&["display-message", "-p", "-t", id, "#{pane_pid}"])
            .await?;
        let pid = pid_str.trim().parse::<u32>().unwrap_or(0);
        Ok(pid)
    }

    async fn paste_text(&self, id: &str, text: &str, bracketed: bool) -> Result<()> {
        let buf_name = format!("buf-{id}");
        let bin = Self::resolve_tmux_binary()?;
        let mut load_cmd = Command::new(bin);
        load_cmd
            .arg("-L")
            .arg(&self.socket_name)
            .args(["load-buffer", "-b", &buf_name, "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = load_cmd
            .spawn()
            .context("Failed to spawn tmux load-buffer")?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .await
                .context("Failed to write prompt to tmux buffer stdin")?;
            stdin.shutdown().await.ok();
        }
        let status = child
            .wait_with_output()
            .await
            .context("Failed waiting for tmux load-buffer")?;
        if !status.status.success() {
            let stderr = String::from_utf8_lossy(&status.stderr).trim().to_string();
            return Err(anyhow!("tmux load-buffer failed: {stderr}"));
        }

        let mut paste_args = vec!["paste-buffer", "-d", "-r"];
        if bracketed {
            paste_args.push("-p");
        }
        paste_args.extend(["-b", &buf_name, "-t", id]);
        self.run_tmux(&paste_args).await?;
        Ok(())
    }

    async fn send_keys(&self, id: &str, keys: &[String]) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
        let mut args: Vec<&str> = vec!["send-keys", "-t", id];
        for key in keys {
            args.push(key.as_str());
        }
        self.run_tmux(&args).await?;
        Ok(())
    }

    async fn capture_screen(
        &self,
        id: &str,
        include_ansi: bool,
        scrollback_lines: Option<i32>,
    ) -> Result<String> {
        let mut args: Vec<String> = vec![
            "capture-pane".to_string(),
            "-t".to_string(),
            id.to_string(),
            "-p".to_string(),
            "-J".to_string(),
        ];
        if include_ansi {
            args.push("-e".to_string());
        }
        if let Some(lines) = scrollback_lines {
            let start = if lines > 0 { -lines } else { lines };
            args.push("-S".to_string());
            args.push(start.to_string());
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_tmux(&arg_refs).await
    }

    async fn history_size(&self, id: &str) -> Result<usize> {
        let out = self
            .run_tmux(&["display-message", "-p", "-t", id, "#{history_size}"])
            .await?;
        Ok(out.trim().parse::<usize>().unwrap_or(0))
    }

    async fn is_alive(&self, id: &str) -> Result<bool> {
        let out = self
            .run_tmux(&["display-message", "-p", "-t", id, "#{pane_dead}"])
            .await;
        match out {
            Ok(val) => Ok(val.trim() != "1"),
            Err(_) => Ok(false),
        }
    }

    async fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        self.run_tmux(&[
            "resize-window",
            "-t",
            id,
            "-x",
            &cols.to_string(),
            "-y",
            &rows.to_string(),
        ])
        .await?;
        Ok(())
    }

    async fn kill_pane(&self, id: &str) -> Result<()> {
        let _ = self.run_tmux(&["kill-session", "-t", id]).await;
        Ok(())
    }

    fn attach_command(&self, id: &str) -> String {
        format!("tmux -L {} attach -t {}", self.socket_name, id)
    }
}

/// Deterministic in-memory mock backend for unit testing TUI session flows without spawning tmux.
#[derive(Default, Clone)]
pub struct MockMultiplexerBackend {
    panes: Arc<Mutex<HashMap<String, MockPane>>>,
}

#[derive(Debug, Clone)]
pub struct MockPane {
    pub pid: u32,
    pub env: Vec<(String, String)>,
    pub screen: String,
    pub pasted: Vec<(String, bool)>,
    pub keys_sent: Vec<String>,
    pub enter_keys_to_ignore: usize,
    pub alive: bool,
    pub cols: u16,
    pub rows: u16,
    pub screen_transitions: Vec<String>,
}

impl MockMultiplexerBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn preconfigure_pane(&self, id: &str, screen: impl Into<String>, transitions: Vec<String>) {
        if let Ok(mut guard) = self.panes.lock() {
            let pid = 42000 + guard.len() as u32;
            guard.insert(
                id.to_string(),
                MockPane {
                    pid,
                    env: Vec::new(),
                    screen: screen.into(),
                    pasted: Vec::new(),
                    keys_sent: Vec::new(),
                    enter_keys_to_ignore: 0,
                    alive: true,
                    cols: 160,
                    rows: 48,
                    screen_transitions: transitions,
                },
            );
        }
    }

    pub fn ignore_next_enter_keys(&self, id: &str, count: usize) {
        if let Some(pane) = self.panes.lock().unwrap().get_mut(id) {
            pane.enter_keys_to_ignore = count;
        }
    }

    pub fn set_screen(&self, id: &str, screen: impl Into<String>) {
        if let Ok(mut guard) = self.panes.lock() {
            if let Some(pane) = guard.get_mut(id) {
                pane.screen = screen.into();
            }
        }
    }

    pub fn set_alive(&self, id: &str, alive: bool) {
        if let Ok(mut guard) = self.panes.lock() {
            if let Some(pane) = guard.get_mut(id) {
                pane.alive = alive;
            }
        }
    }

    pub fn get_pane(&self, id: &str) -> Option<MockPane> {
        self.panes.lock().ok()?.get(id).cloned()
    }
}

#[async_trait]
impl MultiplexerBackend for MockMultiplexerBackend {
    async fn spawn_pane(
        &self,
        id: &str,
        _cmd: &[String],
        _cwd: &Path,
        cols: u16,
        rows: u16,
        env_vars: &[(String, String)],
    ) -> Result<u32> {
        let mut guard = self.panes.lock().unwrap();
        if let Some(pane) = guard.get_mut(id) {
            pane.env = env_vars.to_vec();
            pane.cols = cols;
            pane.rows = rows;
            return Ok(pane.pid);
        }

        let pid = 42000 + guard.len() as u32;
        guard.insert(
            id.to_string(),
            MockPane {
                pid,
                env: env_vars.to_vec(),
                screen: "────────────────────────────────────────────\n>\n────────────────────────────────────────────".to_string(),
                pasted: Vec::new(),
                keys_sent: Vec::new(),
                enter_keys_to_ignore: 0,
                alive: true,
                cols,
                rows,
                screen_transitions: Vec::new(),
            },
        );
        Ok(pid)
    }

    async fn paste_text(&self, id: &str, text: &str, bracketed: bool) -> Result<()> {
        let mut guard = self.panes.lock().unwrap();
        let pane = guard
            .get_mut(id)
            .ok_or_else(|| anyhow!("Mock pane {id} not found"))?;
        pane.pasted.push((text.to_string(), bracketed));
        pane.screen = format!(
            "────────────────────────────────────────────\n> {text}\n────────────────────────────────────────────"
        );
        Ok(())
    }

    async fn send_keys(&self, id: &str, keys: &[String]) -> Result<()> {
        let mut guard = self.panes.lock().unwrap();
        let pane = guard
            .get_mut(id)
            .ok_or_else(|| anyhow!("Mock pane {id} not found"))?;
        pane.keys_sent.extend(keys.iter().cloned());
        if !pane.screen_transitions.is_empty() {
            pane.screen = pane.screen_transitions.remove(0);
        } else if keys.iter().any(|k| k == "Enter") {
            if pane.enter_keys_to_ignore > 0 {
                pane.enter_keys_to_ignore -= 1;
                return Ok(());
            }
            if let Some((last_prompt, _)) = pane.pasted.last() {
                pane.screen = format!(
                    "> {last_prompt}\nMock response for: {last_prompt}\n────────────────────────────────────────────\n>\n────────────────────────────────────────────"
                );
            }
        }
        Ok(())
    }

    async fn capture_screen(
        &self,
        id: &str,
        _include_ansi: bool,
        _scrollback_lines: Option<i32>,
    ) -> Result<String> {
        let guard = self.panes.lock().unwrap();
        let pane = guard
            .get(id)
            .ok_or_else(|| anyhow!("Mock pane {id} not found"))?;
        Ok(pane.screen.clone())
    }

    async fn history_size(&self, id: &str) -> Result<usize> {
        let guard = self.panes.lock().unwrap();
        let pane = guard
            .get(id)
            .ok_or_else(|| anyhow!("Mock pane {id} not found"))?;
        Ok(pane.screen.lines().count())
    }

    async fn is_alive(&self, id: &str) -> Result<bool> {
        let guard = self.panes.lock().unwrap();
        Ok(guard.get(id).map(|p| p.alive).unwrap_or(false))
    }

    async fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        let mut guard = self.panes.lock().unwrap();
        let pane = guard
            .get_mut(id)
            .ok_or_else(|| anyhow!("Mock pane {id} not found"))?;
        pane.cols = cols;
        pane.rows = rows;
        Ok(())
    }

    async fn kill_pane(&self, id: &str) -> Result<()> {
        let mut guard = self.panes.lock().unwrap();
        if let Some(pane) = guard.get_mut(id) {
            pane.alive = false;
        }
        guard.remove(id);
        Ok(())
    }

    fn attach_command(&self, id: &str) -> String {
        format!("tmux -L mock attach -t {id}")
    }
}
