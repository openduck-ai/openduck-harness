use anyhow::{anyhow, Context, Result};
use openduck_harness::types::discover_global_harness_settings;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

use super::backend::{MultiplexerBackend, TmuxBackend};
use super::profiles::{
    profile_for_kind, suggested_approval_keys, AgentCliKind, AgentTuiProfile, TuiPaneState,
    TuiSessionConfig, TuiSessionSummary, TuiTurnResult,
};
use super::screen::{strip_ansi_codes, QuiescenceDetector};

pub struct TuiSessionHandle {
    pub config: TuiSessionConfig,
    pub backend: Arc<dyn MultiplexerBackend>,
    pub profile: Arc<dyn AgentTuiProfile>,
    pub pid: u32,
    pub created_at_unix_secs: u64,
    pub last_activity_unix_secs: AtomicU64,
    pub exited_at_unix_secs: Mutex<Option<u64>>,
    pub transcript_offset: Mutex<u64>,
    pub last_screen: Mutex<String>,
    pub last_state: Mutex<TuiPaneState>,
}

impl TuiSessionHandle {
    pub async fn settle_startup(&self, timeout: Duration) -> Result<TuiPaneState> {
        let deadline = Instant::now() + timeout;
        let poll_interval = Duration::from_millis(200);
        let mut approved_screen = None;

        while Instant::now() < deadline {
            if !self.backend.is_alive(&self.config.session_id).await? {
                let mut exited = self.exited_at_unix_secs.lock().await;
                if exited.is_none() {
                    *exited = Some(chrono::Utc::now().timestamp() as u64);
                }
                let state = TuiPaneState::Exited { exit_code: None };
                *self.last_state.lock().await = state.clone();
                return Ok(state);
            }

            let screen = self
                .backend
                .capture_screen(&self.config.session_id, false, Some(100))
                .await?;

            let state = self.profile.classify_screen(&screen);
            *self.last_screen.lock().await = screen.clone();
            *self.last_state.lock().await = state.clone();

            match &state {
                TuiPaneState::IdlePrompt => {
                    return Ok(state);
                }
                TuiPaneState::WaitingApproval { .. } => {
                    if !self.config.auto_approve {
                        return Ok(state);
                    }
                    let clean_screen = strip_ansi_codes(&screen);
                    if approved_screen.as_ref() != Some(&clean_screen) {
                        let keys = suggested_approval_keys(&screen);
                        self.backend
                            .send_keys(&self.config.session_id, &keys)
                            .await?;
                        approved_screen = Some(clean_screen);
                    }
                    tokio::time::sleep(poll_interval).await;
                }
                TuiPaneState::Exited { .. } => {
                    return Ok(state);
                }
                TuiPaneState::Working { .. } | TuiPaneState::Initializing => {
                    approved_screen = None;
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }

        let final_state = self.last_state.lock().await.clone();
        Ok(final_state)
    }

    pub async fn send_prompt(
        &self,
        prompt: &str,
        wait: bool,
        timeout: Duration,
    ) -> Result<TuiTurnResult> {
        if !self.backend.is_alive(&self.config.session_id).await? {
            let mut exited = self.exited_at_unix_secs.lock().await;
            if exited.is_none() {
                *exited = Some(chrono::Utc::now().timestamp() as u64);
            }
            return Err(anyhow!(
                "Session '{}' is not running",
                self.config.session_id
            ));
        }

        self.last_activity_unix_secs
            .store(chrono::Utc::now().timestamp() as u64, Ordering::Relaxed);
        let start_time = Instant::now();

        let state = self
            .settle_startup(timeout.min(Duration::from_secs(10)))
            .await?;
        match state {
            TuiPaneState::IdlePrompt => {}
            TuiPaneState::WaitingApproval { prompt_snippet } => {
                return Err(anyhow!(
                    "Session '{}' is waiting for approval: {}. Inspect with tui_agent_read or confirm with tui_agent_keys before sending a prompt.",
                    self.config.session_id,
                    prompt_snippet
                ));
            }
            _ => {
                return Err(anyhow!(
                    "Session '{}': input composer did not become ready ({state:?}); prompt was not submitted",
                    self.config.session_id
                ));
            }
        }

        if self.config.kind == AgentCliKind::Agy {
            if prompt.trim().is_empty() {
                return Err(anyhow!("agy prompt must not be empty"));
            }
            self.wait_for_agy_composer(
                true,
                timeout
                    .saturating_sub(start_time.elapsed())
                    .min(Duration::from_secs(15)),
            )
            .await?;
        }

        // Inject prompt using bracketed paste mode to prevent early submission on newlines
        self.backend
            .paste_text(&self.config.session_id, prompt, true)
            .await
            .context("Failed to paste prompt into TUI session")?;

        if self.config.kind == AgentCliKind::Agy {
            self.submit_agy_prompt(timeout.saturating_sub(start_time.elapsed()))
                .await?;
        } else {
            tokio::time::sleep(Duration::from_millis(80)).await;
            self.backend
                .send_keys(&self.config.session_id, &["Enter".to_string()])
                .await
                .context("Failed to send Enter key to TUI session")?;
        }

        if !wait {
            let screen = self
                .backend
                .capture_screen(&self.config.session_id, false, Some(100))
                .await
                .unwrap_or_default();
            let state = self.profile.classify_screen(&screen);
            *self.last_screen.lock().await = screen.clone();
            *self.last_state.lock().await = state.clone();

            return Ok(TuiTurnResult {
                session_id: self.config.session_id.clone(),
                agent: self.config.kind,
                state,
                response: String::new(),
                screen_snapshot: screen,
                extraction_source: "none (wait=false)".to_string(),
                duration_ms: start_time.elapsed().as_millis() as u64,
                attach_command: self.backend.attach_command(&self.config.session_id),
            });
        }

        // Wait loop: Poll until turn completes or waiting approval
        let mut detector = QuiescenceDetector::new(Duration::from_millis(800));
        let poll_interval = Duration::from_millis(200);
        let mut final_state = TuiPaneState::Working { status_line: None };
        let mut final_screen = String::new();

        // Brief settling delay before first check
        tokio::time::sleep(Duration::from_millis(300)).await;

        while start_time.elapsed() < timeout {
            if !self.backend.is_alive(&self.config.session_id).await? {
                final_state = TuiPaneState::Exited { exit_code: None };
                let mut exited = self.exited_at_unix_secs.lock().await;
                if exited.is_none() {
                    *exited = Some(chrono::Utc::now().timestamp() as u64);
                }
                break;
            }

            let raw_screen = self
                .backend
                .capture_screen(&self.config.session_id, false, Some(100))
                .await
                .unwrap_or_default();
            final_screen = raw_screen.clone();
            let state = self.profile.classify_screen(&raw_screen);

            match &state {
                TuiPaneState::WaitingApproval { .. } => {
                    final_state = state;
                    break;
                }
                TuiPaneState::Working { .. } => {
                    detector.mark_activity();
                    final_state = state;
                }
                TuiPaneState::IdlePrompt => {
                    final_state = state;
                    if detector.observe(&raw_screen, true, false) {
                        // Screen has settled and input prompt is steady
                        break;
                    }
                }
                _ => {
                    if detector.observe(&raw_screen, false, false) {
                        final_state = state;
                        break;
                    }
                }
            }

            tokio::time::sleep(poll_interval).await;
        }

        // Extract response: Lossless side-channel transcript first, fallback to stripped scrollback
        let mut offset_guard = self.transcript_offset.lock().await;
        let transcript_resp = self.profile.read_transcript_delta(
            &self.config.cwd,
            self.pid,
            self.config.resume_id.as_deref(),
            &mut offset_guard,
        );

        let (response, source) = if let Some(resp) = transcript_resp {
            (resp, "transcript+tui".to_string())
        } else {
            let scrollback = self
                .backend
                .capture_screen(&self.config.session_id, false, Some(500))
                .await
                .unwrap_or_default();
            let stripped = self.profile.strip_chrome(&scrollback, prompt);
            (stripped, "tui_scrollback".to_string())
        };

        *self.last_screen.lock().await = final_screen.clone();
        *self.last_state.lock().await = final_state.clone();
        self.last_activity_unix_secs
            .store(chrono::Utc::now().timestamp() as u64, Ordering::Relaxed);

        Ok(TuiTurnResult {
            session_id: self.config.session_id.clone(),
            agent: self.config.kind,
            state: final_state,
            response,
            screen_snapshot: final_screen,
            extraction_source: source,
            duration_ms: start_time.elapsed().as_millis() as u64,
            attach_command: self.backend.attach_command(&self.config.session_id),
        })
    }

    async fn wait_for_agy_composer(&self, expect_empty: bool, timeout: Duration) -> Result<String> {
        let deadline = Instant::now() + timeout;
        let mut previous = None;
        let mut stable_since = Instant::now();
        while Instant::now() < deadline {
            let screen = self
                .backend
                .capture_screen(&self.config.session_id, false, Some(100))
                .await?;
            if self.profile.classify_screen(&screen) == TuiPaneState::IdlePrompt {
                if let Some(text) = self.profile.composer_text(&screen) {
                    if text.is_empty() == expect_empty {
                        if previous.as_ref() != Some(&text) {
                            stable_since = Instant::now();
                            previous = Some(text.clone());
                        } else if stable_since.elapsed() >= Duration::from_millis(500) {
                            return Ok(text);
                        }
                    } else {
                        previous = None;
                    }
                } else {
                    previous = None;
                }
            } else {
                previous = None;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(anyhow!(
            "Session '{}': agy input composer did not become ready; prompt was not submitted",
            self.config.session_id
        ))
    }

    async fn submit_agy_prompt(&self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout.min(Duration::from_secs(10));
        let pasted = self
            .wait_for_agy_composer(false, deadline.saturating_duration_since(Instant::now()))
            .await?;
        for _ in 0..3 {
            if Instant::now() >= deadline {
                break;
            }
            self.backend
                .send_keys(&self.config.session_id, &["Enter".to_string()])
                .await?;
            let check_until = (Instant::now() + Duration::from_secs(1)).min(deadline);
            let mut still_pending = false;
            while Instant::now() < check_until {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let screen = self
                    .backend
                    .capture_screen(&self.config.session_id, false, Some(100))
                    .await?;
                match self.profile.classify_screen(&screen) {
                    TuiPaneState::Working { .. } | TuiPaneState::WaitingApproval { .. } => {
                        return Ok(());
                    }
                    TuiPaneState::IdlePrompt => match self.profile.composer_text(&screen) {
                        Some(text) if text.is_empty() => return Ok(()),
                        Some(text) if text == pasted => still_pending = true,
                        _ => still_pending = false,
                    },
                    _ => still_pending = false,
                }
            }
            // Retry only while the original text is visibly still in the idle composer.
            if !still_pending {
                break;
            }
        }
        Err(anyhow!(
            "Session '{}': could not confirm agy submitted the prompt after Enter; inspect the TUI before resending",
            self.config.session_id
        ))
    }

    pub async fn send_keys(&self, keys: &[String]) -> Result<()> {
        self.last_activity_unix_secs
            .store(chrono::Utc::now().timestamp() as u64, Ordering::Relaxed);
        self.backend.send_keys(&self.config.session_id, keys).await
    }

    pub async fn read_state(
        &self,
        scrollback_lines: Option<i32>,
        include_ansi: bool,
    ) -> Result<TuiTurnResult> {
        let lines = scrollback_lines.unwrap_or(120);
        let screen = self
            .backend
            .capture_screen(&self.config.session_id, include_ansi, Some(lines))
            .await?;

        let plain = if include_ansi {
            strip_ansi_codes(&screen)
        } else {
            screen.clone()
        };

        let state = if self.backend.is_alive(&self.config.session_id).await? {
            self.profile.classify_screen(&plain)
        } else {
            let mut exited = self.exited_at_unix_secs.lock().await;
            if exited.is_none() {
                *exited = Some(chrono::Utc::now().timestamp() as u64);
            }
            TuiPaneState::Exited { exit_code: None }
        };

        let mut offset_guard = self.transcript_offset.lock().await;
        let delta = self.profile.read_transcript_delta(
            &self.config.cwd,
            self.pid,
            self.config.resume_id.as_deref(),
            &mut offset_guard,
        );

        *self.last_screen.lock().await = screen.clone();
        *self.last_state.lock().await = state.clone();

        Ok(TuiTurnResult {
            session_id: self.config.session_id.clone(),
            agent: self.config.kind,
            state,
            response: delta.unwrap_or_default(),
            screen_snapshot: screen,
            extraction_source: "read_poll".to_string(),
            duration_ms: 0,
            attach_command: self.backend.attach_command(&self.config.session_id),
        })
    }

    pub async fn stop(&self) -> Result<()> {
        // Attempt clean interrupt/exit before destroying session
        let _ = self
            .backend
            .send_keys(
                &self.config.session_id,
                &["Escape".to_string(), "C-c".to_string()],
            )
            .await;
        tokio::time::sleep(Duration::from_millis(150)).await;

        self.backend.kill_pane(&self.config.session_id).await?;
        *self.last_state.lock().await = TuiPaneState::Exited { exit_code: Some(0) };
        let mut exited = self.exited_at_unix_secs.lock().await;
        if exited.is_none() {
            *exited = Some(chrono::Utc::now().timestamp() as u64);
        }
        Ok(())
    }
}

pub struct TuiSessionManager {
    sessions: Arc<RwLock<HashMap<String, Arc<TuiSessionHandle>>>>,
    backend: Arc<dyn MultiplexerBackend>,
}

impl Default for TuiSessionManager {
    fn default() -> Self {
        Self::new("openduck")
    }
}

impl TuiSessionManager {
    pub fn new(socket_name: impl Into<String>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            backend: Arc::new(TmuxBackend::new(socket_name)),
        }
    }

    pub fn with_backend(backend: Arc<dyn MultiplexerBackend>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            backend,
        }
    }

    pub async fn start_session(
        &self,
        mut config: TuiSessionConfig,
    ) -> Result<Arc<TuiSessionHandle>> {
        let _ = self.reclaim_stale_sessions().await;

        if config.effort.is_none()
            || config.https_proxy.is_none()
            || config.idle_timeout_mins.is_none()
            || config.exited_retention_mins.is_none()
        {
            let settings = discover_global_harness_settings(Some(&config.cwd));
            if config.effort.is_none() {
                config.effort = settings
                    .tui_sessions
                    .get(&config.kind.to_string())
                    .and_then(|defaults| defaults.effort.clone())
                    .or_else(|| {
                        settings
                            .tui_sessions
                            .get("default")
                            .and_then(|defaults| defaults.effort.clone())
                    });
            }
            if config.https_proxy.is_none() {
                config.https_proxy = settings
                    .tui_sessions
                    .get(&config.kind.to_string())
                    .and_then(|defaults| defaults.https_proxy.clone())
                    .or_else(|| {
                        settings
                            .tui_sessions
                            .get("default")
                            .and_then(|defaults| defaults.https_proxy.clone())
                    });
            }
            if config.idle_timeout_mins.is_none() {
                config.idle_timeout_mins = settings
                    .tui_sessions
                    .get(&config.kind.to_string())
                    .and_then(|defaults| defaults.idle_timeout_mins)
                    .or_else(|| {
                        settings
                            .tui_sessions
                            .get("default")
                            .and_then(|defaults| defaults.idle_timeout_mins)
                    });
            }
            if config.exited_retention_mins.is_none() {
                config.exited_retention_mins = settings
                    .tui_sessions
                    .get(&config.kind.to_string())
                    .and_then(|defaults| defaults.exited_retention_mins)
                    .or_else(|| {
                        settings
                            .tui_sessions
                            .get("default")
                            .and_then(|defaults| defaults.exited_retention_mins)
                    });
            }
        }
        let profile = profile_for_kind(config.kind);
        let cmd = profile
            .build_launch_command(&config)
            .context("Failed to build agent CLI launch command")?;

        let mut env = config.env.clone();
        if let Some(proxy) = &config.https_proxy {
            env.retain(|(key, _)| key != "HTTPS_PROXY" && key != "https_proxy");
            env.push(("HTTPS_PROXY".to_string(), proxy.clone()));
            env.push(("https_proxy".to_string(), proxy.clone()));
        }

        let pid = self
            .backend
            .spawn_pane(
                &config.session_id,
                &cmd,
                &config.cwd,
                config.cols,
                config.rows,
                &env,
            )
            .await
            .context("Failed to spawn tmux session pane")?;

        // Initialize transcript byte offset so we only capture future turns
        let mut initial_offset = 0u64;
        let _ = profile.read_transcript_delta(
            &config.cwd,
            pid,
            config.resume_id.as_deref(),
            &mut initial_offset,
        );

        let now = chrono::Utc::now().timestamp() as u64;
        let handle = Arc::new(TuiSessionHandle {
            config: config.clone(),
            backend: self.backend.clone(),
            profile,
            pid,
            created_at_unix_secs: now,
            last_activity_unix_secs: AtomicU64::new(now),
            exited_at_unix_secs: Mutex::new(None),
            transcript_offset: Mutex::new(initial_offset),
            last_screen: Mutex::new(String::new()),
            last_state: Mutex::new(TuiPaneState::Initializing),
        });

        self.sessions
            .write()
            .await
            .insert(config.session_id.clone(), handle.clone());

        handle
            .settle_startup(Duration::from_secs(10))
            .await
            .with_context(|| {
                format!(
                    "Session '{}' started, but startup handling failed; inspect it with tui_agent_read",
                    config.session_id
                )
            })?;

        Ok(handle)
    }

    pub async fn get_session(&self, id: &str) -> Result<Arc<TuiSessionHandle>> {
        let guard = self.sessions.read().await;
        guard
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("TUI session '{}' not found", id))
    }

    pub async fn list_sessions(&self, include_screen_preview: bool) -> Vec<TuiSessionSummary> {
        let _ = self.reclaim_stale_sessions().await;
        let guard = self.sessions.read().await;
        let mut summaries = Vec::new();
        let now = chrono::Utc::now().timestamp() as u64;

        for (id, handle) in guard.iter() {
            let is_alive = handle.backend.is_alive(id).await.unwrap_or(false);
            let state = if is_alive {
                handle.last_state.lock().await.clone()
            } else {
                let mut exited = handle.exited_at_unix_secs.lock().await;
                if exited.is_none() {
                    *exited = Some(now);
                }
                TuiPaneState::Exited { exit_code: None }
            };

            let screen_preview = if include_screen_preview {
                handle
                    .backend
                    .capture_screen(id, false, Some(24))
                    .await
                    .ok()
            } else {
                None
            };

            summaries.push(TuiSessionSummary {
                session_id: id.clone(),
                kind: handle.config.kind,
                pid: Some(handle.pid),
                cwd: handle.config.cwd.to_string_lossy().to_string(),
                state,
                attach_command: handle.backend.attach_command(id),
                created_at_unix_secs: handle.created_at_unix_secs,
                last_activity_unix_secs: handle.last_activity_unix_secs.load(Ordering::Relaxed),
                exited_at_unix_secs: *handle.exited_at_unix_secs.lock().await,
                screen_preview,
            });
        }

        summaries.sort_by_key(|b| std::cmp::Reverse(b.created_at_unix_secs));
        summaries
    }

    pub async fn reclaim_stale_sessions(&self) -> Vec<String> {
        let now = chrono::Utc::now().timestamp() as u64;
        let mut to_reclaim = Vec::new();

        {
            let guard = self.sessions.read().await;
            for (id, handle) in guard.iter() {
                let is_alive = handle.backend.is_alive(id).await.unwrap_or(false);
                if !is_alive {
                    let mut exited_guard = handle.exited_at_unix_secs.lock().await;
                    let exited_at = match *exited_guard {
                        Some(t) => t,
                        None => {
                            *exited_guard = Some(now);
                            now
                        }
                    };
                    let retention_secs = handle.config.exited_retention_mins.unwrap_or(10) * 60;
                    if now.saturating_sub(exited_at) >= retention_secs {
                        to_reclaim.push(id.clone());
                    }
                } else if let Some(idle_mins) = handle.config.idle_timeout_mins {
                    let state = handle.last_state.lock().await.clone();
                    if !state.is_working() {
                        let last_activity = handle.last_activity_unix_secs.load(Ordering::Relaxed);
                        if now.saturating_sub(last_activity) >= idle_mins * 60 {
                            to_reclaim.push(id.clone());
                        }
                    }
                }
            }
        }

        let mut reclaimed = Vec::new();
        for id in to_reclaim {
            if let Ok(handle) = self.get_session(&id).await {
                let _ = handle.stop().await;
                self.sessions.write().await.remove(&id);
                reclaimed.push(id);
            }
        }

        reclaimed
    }

    pub async fn stop_session(&self, id: &str) -> Result<()> {
        let session = self.get_session(id).await?;
        session.stop().await?;
        self.sessions.write().await.remove(id);
        Ok(())
    }
}
