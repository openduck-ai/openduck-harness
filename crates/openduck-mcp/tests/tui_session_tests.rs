use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

use openduck_mcp::tui_session::backend::MockMultiplexerBackend;
use openduck_mcp::tui_session::{
    profile_for_kind, AgentCliKind, MultiplexerBackend, TmuxBackend, TuiPaneState,
    TuiSessionConfig, TuiSessionManager,
};

async fn mock_agy_session(
    backend: Arc<MockMultiplexerBackend>,
    cwd: &std::path::Path,
) -> openduck_mcp::tui_session::session::TuiSessionHandle {
    use std::sync::atomic::AtomicU64;
    use tokio::sync::Mutex;

    let config = TuiSessionConfig {
        session_id: "agy_submit".to_string(),
        kind: AgentCliKind::Agy,
        cwd: cwd.to_path_buf(),
        ..Default::default()
    };
    let pid = backend
        .spawn_pane(&config.session_id, &[], cwd, 120, 30, &[])
        .await
        .unwrap();
    openduck_mcp::tui_session::session::TuiSessionHandle {
        config,
        backend,
        profile: profile_for_kind(AgentCliKind::Agy),
        pid,
        created_at_unix_secs: 0,
        last_activity_unix_secs: AtomicU64::new(0),
        exited_at_unix_secs: Mutex::new(None),
        transcript_offset: Mutex::new(0),
        last_screen: Mutex::new(String::new()),
        last_state: Mutex::new(TuiPaneState::Initializing),
    }
}

#[test]
fn test_agy_composer_distinguishes_pending_input_from_history() {
    let profile = profile_for_kind(AgentCliKind::Agy);
    let pending = "> old prompt\nOld response\n────────────────────\n> review\n  the project\n────────────────────\n? for shortcuts";
    assert_eq!(
        profile.composer_text(pending).as_deref(),
        Some("review\nthe project")
    );
    let empty = "> old prompt\nOld response\n────────────────────\n>\n────────────────────\n? for shortcuts";
    assert_eq!(profile.composer_text(empty).as_deref(), Some(""));
    assert_eq!(profile.composer_text("Starting agy..."), None);

    let long_input = format!(
        "────────────────────\n> review\n{}\n────────────────────",
        "more text\n".repeat(20)
    );
    assert_eq!(
        profile.classify_screen(&long_input),
        TuiPaneState::IdlePrompt
    );
}

#[tokio::test]
async fn test_agy_retries_enter_only_while_prompt_remains_in_composer() {
    let dir = tempdir().unwrap();
    let backend = Arc::new(MockMultiplexerBackend::new());
    let handle = mock_agy_session(backend.clone(), dir.path()).await;
    backend.ignore_next_enter_keys("agy_submit", 1);

    let turn = handle
        .send_prompt("review\nthe project", false, Duration::from_secs(10))
        .await
        .unwrap();
    let pane = backend.get_pane("agy_submit").unwrap();
    assert_eq!(pane.pasted.len(), 1);
    assert_eq!(pane.keys_sent, vec!["Enter", "Enter"]);
    assert_eq!(turn.state, TuiPaneState::IdlePrompt);
    assert_eq!(
        handle
            .profile
            .composer_text(&turn.screen_snapshot)
            .as_deref(),
        Some("")
    );
}

#[tokio::test]
async fn test_agy_reports_unsubmitted_prompt_instead_of_completion() {
    let dir = tempdir().unwrap();
    let backend = Arc::new(MockMultiplexerBackend::new());
    let handle = mock_agy_session(backend.clone(), dir.path()).await;
    backend.ignore_next_enter_keys("agy_submit", 3);

    let error = handle
        .send_prompt("review the project", true, Duration::from_secs(10))
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("could not confirm agy submitted"));
    let pane = backend.get_pane("agy_submit").unwrap();
    assert_eq!(pane.pasted.len(), 1);
    assert_eq!(pane.keys_sent.len(), 3);
}

#[tokio::test]
async fn test_agy_waits_for_ready_composer_before_pasting() {
    let dir = tempdir().unwrap();
    let backend = Arc::new(MockMultiplexerBackend::new());
    let handle = mock_agy_session(backend.clone(), dir.path()).await;
    backend.set_screen("agy_submit", "Starting agy...");

    let error = handle
        .send_prompt("review the project", false, Duration::from_millis(200))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("did not become ready"));
    let pane = backend.get_pane("agy_submit").unwrap();
    assert!(pane.pasted.is_empty());
    assert!(pane.keys_sent.is_empty());
}

#[tokio::test]
async fn test_harness_file_configures_new_tui_sessions() {
    let dir = tempdir().unwrap();
    let config_dir = dir.path().join(".openduck");
    std::fs::create_dir(&config_dir).unwrap();
    let config_path = config_dir.join("config.yaml");
    std::fs::write(
        &config_path,
        "harness:\n  tui_sessions:\n    default:\n      https_proxy: http://default:8080\n",
    )
    .unwrap();

    let backend = Arc::new(MockMultiplexerBackend::new());
    let manager = TuiSessionManager::with_backend(backend.clone());
    let config = TuiSessionConfig {
        session_id: "from_default".to_string(),
        kind: AgentCliKind::Custom,
        cwd: dir.path().to_path_buf(),
        extra_args: vec!["bash".to_string()],
        ..Default::default()
    };
    let handle = manager.start_session(config.clone()).await.unwrap();
    assert_eq!(
        handle.config.https_proxy.as_deref(),
        Some("http://default:8080")
    );

    std::fs::write(
        &config_path,
        "harness:\n  tui_sessions:\n    default:\n      https_proxy: http://default:8080\n    custom:\n      https_proxy: http://127.0.0.1:2087\n",
    )
    .unwrap();
    let handle = manager
        .start_session(TuiSessionConfig {
            session_id: "from_agent".to_string(),
            ..config.clone()
        })
        .await
        .unwrap();
    assert_eq!(
        handle.config.https_proxy.as_deref(),
        Some("http://127.0.0.1:2087")
    );
    let pane = backend.get_pane("from_agent").unwrap();
    for key in ["HTTPS_PROXY", "https_proxy"] {
        assert!(pane
            .env
            .contains(&(key.to_string(), "http://127.0.0.1:2087".to_string())));
    }

    let handle = manager
        .start_session(TuiSessionConfig {
            session_id: "from_request".to_string(),
            https_proxy: Some("http://request:8080".to_string()),
            ..config
        })
        .await
        .unwrap();
    assert_eq!(
        handle.config.https_proxy.as_deref(),
        Some("http://request:8080")
    );
    assert_eq!(
        manager
            .get_session("from_default")
            .await
            .unwrap()
            .config
            .https_proxy
            .as_deref(),
        Some("http://default:8080")
    );
}

#[tokio::test]
async fn test_harness_effort_defaults_and_request_precedence() {
    let dir = tempdir().unwrap();
    let config_dir = dir.path().join(".openduck");
    std::fs::create_dir(&config_dir).unwrap();
    let config_path = config_dir.join("config.yaml");
    std::fs::write(
        &config_path,
        "harness:\n  tui_sessions:\n    default:\n      effort: medium\n    custom:\n      idle_timeout_mins: 30\n",
    )
    .unwrap();

    let backend = Arc::new(MockMultiplexerBackend::new());
    let manager = TuiSessionManager::with_backend(backend);
    let config = TuiSessionConfig {
        session_id: "default_effort".to_string(),
        kind: AgentCliKind::Custom,
        cwd: dir.path().to_path_buf(),
        extra_args: vec!["bash".to_string()],
        https_proxy: Some("http://request:8080".to_string()),
        idle_timeout_mins: Some(45),
        exited_retention_mins: Some(10),
        ..Default::default()
    };
    let handle = manager.start_session(config.clone()).await.unwrap();
    assert_eq!(handle.config.effort.as_deref(), Some("medium"));

    std::fs::write(
        &config_path,
        "harness:\n  tui_sessions:\n    default:\n      effort: medium\n    custom:\n      effort: high\n",
    )
    .unwrap();
    let handle = manager
        .start_session(TuiSessionConfig {
            session_id: "agent_effort".to_string(),
            ..config.clone()
        })
        .await
        .unwrap();
    assert_eq!(handle.config.effort.as_deref(), Some("high"));

    let handle = manager
        .start_session(TuiSessionConfig {
            session_id: "request_effort".to_string(),
            effort: Some("low".to_string()),
            https_proxy: None,
            ..config.clone()
        })
        .await
        .unwrap();
    assert_eq!(handle.config.effort.as_deref(), Some("low"));
    std::fs::write(
        &config_path,
        "harness:\n  tui_sessions:\n    default: {}\n    custom: {}\n",
    )
    .unwrap();
    let handle = manager
        .start_session(TuiSessionConfig {
            session_id: "cli_default_effort".to_string(),
            ..config
        })
        .await
        .unwrap();
    assert_eq!(handle.config.effort, None);
    assert_eq!(
        manager
            .get_session("default_effort")
            .await
            .unwrap()
            .config
            .effort
            .as_deref(),
        Some("medium")
    );
}

#[tokio::test]
async fn test_https_proxy_is_forwarded_and_scoped_to_session() {
    let backend = Arc::new(MockMultiplexerBackend::new());
    let manager = TuiSessionManager::with_backend(backend.clone());
    let original_env = vec![
        (
            "HTTPS_PROXY".to_string(),
            "http://old-upper:8080".to_string(),
        ),
        (
            "https_proxy".to_string(),
            "http://old-lower:8080".to_string(),
        ),
        ("NO_PROXY".to_string(), "localhost".to_string()),
    ];
    let config = TuiSessionConfig {
        session_id: "proxied".to_string(),
        kind: AgentCliKind::Custom,
        extra_args: vec!["bash".to_string()],
        https_proxy: Some("http://127.0.0.1:2087".to_string()),
        env: original_env.clone(),
        ..Default::default()
    };

    manager.start_session(config.clone()).await.unwrap();
    let pane = backend.get_pane("proxied").unwrap();
    assert_eq!(
        pane.env,
        vec![
            ("NO_PROXY".to_string(), "localhost".to_string()),
            (
                "HTTPS_PROXY".to_string(),
                "http://127.0.0.1:2087".to_string()
            ),
            (
                "https_proxy".to_string(),
                "http://127.0.0.1:2087".to_string()
            ),
        ]
    );

    manager
        .start_session(TuiSessionConfig {
            session_id: "unconfigured".to_string(),
            https_proxy: None,
            ..config
        })
        .await
        .unwrap();
    assert_eq!(backend.get_pane("unconfigured").unwrap().env, original_env);
}

#[test]
fn test_profile_classification_agy() {
    let profile = profile_for_kind(AgentCliKind::Agy);

    // Working state
    let working_screen = "⣷  Running command...\n────────────────────────────────────────\n>\n────────────────────────────────────────\nesc to cancel";
    assert!(matches!(
        profile.classify_screen(working_screen),
        TuiPaneState::Working { .. }
    ));

    // Approval state
    let approval_screen = "Do you trust the files in this folder? [y/N]";
    assert!(matches!(
        profile.classify_screen(approval_screen),
        TuiPaneState::WaitingApproval { .. }
    ));

    // Idle prompt
    let idle_screen = "────────────────────────────────────────\n>\n────────────────────────────────────────\nGemini 4 Argon · medium";
    assert_eq!(
        profile.classify_screen(idle_screen),
        TuiPaneState::IdlePrompt
    );
}

#[test]
fn test_profile_classification_grok() {
    let profile = profile_for_kind(AgentCliKind::Grok);

    let working = "⠋ Thinking...\n> prompt";
    assert!(matches!(
        profile.classify_screen(working),
        TuiPaneState::Working { .. }
    ));

    let approval = "Approve tool execution? [y/n]";
    assert!(matches!(
        profile.classify_screen(approval),
        TuiPaneState::WaitingApproval { .. }
    ));

    let idle = "Grok Build ready\n>";
    assert_eq!(profile.classify_screen(idle), TuiPaneState::IdlePrompt);
}

#[test]
fn test_profile_classification_codex() {
    let profile = profile_for_kind(AgentCliKind::Codex);

    let working = "⣷ Running...\n> ";
    assert!(matches!(
        profile.classify_screen(working),
        TuiPaneState::Working { .. }
    ));

    let approval = "Allow command? [y/n]";
    assert!(matches!(
        profile.classify_screen(approval),
        TuiPaneState::WaitingApproval { .. }
    ));

    let idle = "OpenAI Codex ready\n>";
    assert_eq!(profile.classify_screen(idle), TuiPaneState::IdlePrompt);
}

#[test]
fn test_profile_classification_claude() {
    let profile = profile_for_kind(AgentCliKind::Claude);

    let working = "⠙ Thinking... (esc to interrupt)";
    assert!(matches!(
        profile.classify_screen(working),
        TuiPaneState::Working { .. }
    ));

    let approval = "Do you want to proceed? [y/n]";
    assert!(matches!(
        profile.classify_screen(approval),
        TuiPaneState::WaitingApproval { .. }
    ));

    let idle = "Claude Code\n>";
    assert_eq!(profile.classify_screen(idle), TuiPaneState::IdlePrompt);
}

#[tokio::test]
async fn test_tmux_backend_lifecycle_and_bracketed_paste() {
    if which::which("tmux").is_err() {
        eprintln!("tmux not found; skipping tmux backend test");
        return;
    }

    let socket = format!("test_sock_{}", chrono::Utc::now().timestamp_micros());
    let backend = Arc::new(TmuxBackend::new(&socket));
    let dir = tempdir().unwrap();

    let session_id = format!("tui_test_{}", std::process::id());

    let cmd = vec!["bash".to_string()];
    let pid = backend
        .spawn_pane(&session_id, &cmd, dir.path(), 120, 30, &[])
        .await
        .expect("Failed to spawn tmux session pane");
    assert!(pid > 0);

    assert!(backend.is_alive(&session_id).await.unwrap());

    // Test bracketed paste with multi-line text
    let multiline = "echo 'LINE1_TEST'\necho 'LINE2_TEST'\n";
    backend
        .paste_text(&session_id, multiline, true)
        .await
        .expect("Failed to paste bracketed text");

    tokio::time::sleep(Duration::from_millis(100)).await;
    backend
        .send_keys(&session_id, &["Enter".to_string()])
        .await
        .expect("Failed to send Enter");

    tokio::time::sleep(Duration::from_millis(250)).await;

    let screen = backend
        .capture_screen(&session_id, false, Some(30))
        .await
        .expect("Failed to capture screen");

    assert!(screen.contains("LINE1_TEST"));
    assert!(screen.contains("LINE2_TEST"));

    backend
        .kill_pane(&session_id)
        .await
        .expect("Failed to kill session pane");
    assert!(!backend.is_alive(&session_id).await.unwrap());
}

#[tokio::test]
async fn test_tui_session_manager_custom_agent() {
    if which::which("tmux").is_err() {
        return;
    }

    let socket = format!("test_mgr_{}", chrono::Utc::now().timestamp_micros());
    let manager = Arc::new(TuiSessionManager::new(&socket));
    let dir = tempdir().unwrap();

    let config = TuiSessionConfig {
        session_id: format!("custom_{}", std::process::id()),
        kind: AgentCliKind::Custom,
        cwd: dir.path().to_path_buf(),
        cols: 120,
        rows: 30,
        extra_args: vec!["bash".to_string()],
        ..Default::default()
    };

    let handle = manager
        .start_session(config)
        .await
        .expect("Failed to start custom session");
    assert!(handle.pid > 0);

    let list = manager.list_sessions(true).await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].session_id, handle.config.session_id);

    manager
        .stop_session(&handle.config.session_id)
        .await
        .expect("Failed to stop session");
    let list_after = manager.list_sessions(false).await;
    assert_eq!(list_after.len(), 0);
}

#[tokio::test]
async fn test_tui_session_reclaim_exited_grace_period() {
    let backend = Arc::new(MockMultiplexerBackend::new());
    let manager = TuiSessionManager::with_backend(backend.clone());
    let dir = tempdir().unwrap();

    // 1. Session with 0 retention minutes should be reclaimed immediately upon exit
    let _handle_zero = manager
        .start_session(TuiSessionConfig {
            session_id: "reclaim_zero".to_string(),
            kind: AgentCliKind::Custom,
            cwd: dir.path().to_path_buf(),
            extra_args: vec!["bash".to_string()],
            exited_retention_mins: Some(0),
            ..Default::default()
        })
        .await
        .unwrap();

    backend.set_alive("reclaim_zero", false);
    let reclaimed = manager.reclaim_stale_sessions().await;
    assert_eq!(reclaimed, vec!["reclaim_zero"]);
    assert!(manager.get_session("reclaim_zero").await.is_err());

    // 2. Session with 10 minutes retention should be retained during grace period
    let handle_retained = manager
        .start_session(TuiSessionConfig {
            session_id: "reclaim_retained".to_string(),
            kind: AgentCliKind::Custom,
            cwd: dir.path().to_path_buf(),
            extra_args: vec!["bash".to_string()],
            exited_retention_mins: Some(10),
            ..Default::default()
        })
        .await
        .unwrap();

    backend.set_alive("reclaim_retained", false);
    let reclaimed_early = manager.reclaim_stale_sessions().await;
    assert!(reclaimed_early.is_empty());
    assert!(manager.get_session("reclaim_retained").await.is_ok());

    // After grace period expires, it is reclaimed
    let past = chrono::Utc::now().timestamp() as u64 - 700;
    *handle_retained.exited_at_unix_secs.lock().await = Some(past);

    let reclaimed_after = manager.reclaim_stale_sessions().await;
    assert_eq!(reclaimed_after, vec!["reclaim_retained"]);
    assert!(manager.get_session("reclaim_retained").await.is_err());
}

#[tokio::test]
async fn test_tui_session_reclaim_idle_timeout() {
    use std::sync::atomic::Ordering;

    let backend = Arc::new(MockMultiplexerBackend::new());
    let manager = TuiSessionManager::with_backend(backend.clone());
    let dir = tempdir().unwrap();

    let handle = manager
        .start_session(TuiSessionConfig {
            session_id: "reclaim_idle".to_string(),
            kind: AgentCliKind::Custom,
            cwd: dir.path().to_path_buf(),
            extra_args: vec!["bash".to_string()],
            idle_timeout_mins: Some(30),
            ..Default::default()
        })
        .await
        .unwrap();

    *handle.last_state.lock().await = TuiPaneState::IdlePrompt;

    // Initially active: not reclaimed
    let reclaimed_early = manager.reclaim_stale_sessions().await;
    assert!(reclaimed_early.is_empty());

    // Stale activity older than 30 minutes
    let old_activity = chrono::Utc::now().timestamp() as u64 - 2000;
    handle
        .last_activity_unix_secs
        .store(old_activity, Ordering::Relaxed);

    let reclaimed_after = manager.reclaim_stale_sessions().await;
    assert_eq!(reclaimed_after, vec!["reclaim_idle"]);
    assert!(manager.get_session("reclaim_idle").await.is_err());
}

#[tokio::test]
async fn test_tui_session_reclaim_inherits_harness_config() {
    let dir = tempdir().unwrap();
    let config_dir = dir.path().join(".openduck");
    std::fs::create_dir(&config_dir).unwrap();
    let config_path = config_dir.join("config.yaml");
    std::fs::write(
        &config_path,
        "harness:\n  tui_sessions:\n    default:\n      idle_timeout_mins: 45\n      exited_retention_mins: 15\n",
    )
    .unwrap();

    let backend = Arc::new(MockMultiplexerBackend::new());
    let manager = TuiSessionManager::with_backend(backend.clone());
    let handle = manager
        .start_session(TuiSessionConfig {
            session_id: "config_reclaim".to_string(),
            kind: AgentCliKind::Custom,
            cwd: dir.path().to_path_buf(),
            extra_args: vec!["bash".to_string()],
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(handle.config.idle_timeout_mins, Some(45));
    assert_eq!(handle.config.exited_retention_mins, Some(15));
}

#[test]
fn test_detects_directory_trust_questions_across_all_profiles() {
    let trust_screens = [
        "Quick safety check: Do you trust the authors of the files in this folder? (y/n)",
        "Do you trust the files in this directory? [y/N]",
        "Trust the authors of the files in this folder before proceeding: [y/n]",
        "Workspace trust: Do you trust this folder? (y/n)",
    ];

    for kind in [
        AgentCliKind::Claude,
        AgentCliKind::Codex,
        AgentCliKind::Grok,
        AgentCliKind::Agy,
        AgentCliKind::Custom,
    ] {
        let profile = profile_for_kind(kind);
        for screen in trust_screens {
            let state = profile.classify_screen(screen);
            assert!(
                matches!(state, TuiPaneState::WaitingApproval { .. }),
                "Profile {:?} failed to detect approval modal in: {}",
                kind,
                screen
            );
        }
    }
}

#[tokio::test]
async fn test_startup_auto_approves_directory_trust_question() {
    let dir = tempdir().unwrap();
    let backend = Arc::new(MockMultiplexerBackend::new());
    let trust_screen =
        "Quick safety check: Do you trust the authors of the files in this folder? (y/n)";
    let ready_screen = "Claude Code\n>";

    backend.preconfigure_pane(
        "trust_session",
        trust_screen,
        vec![ready_screen.to_string()],
    );

    let manager = TuiSessionManager::with_backend(backend.clone());
    let handle = manager
        .start_session(TuiSessionConfig {
            session_id: "trust_session".to_string(),
            kind: AgentCliKind::Claude,
            cwd: dir.path().to_path_buf(),
            auto_approve: true,
            ..Default::default()
        })
        .await
        .unwrap();

    let pane = backend.get_pane("trust_session").unwrap();
    assert_eq!(pane.keys_sent, vec!["y", "Enter"]);
    assert_eq!(*handle.last_state.lock().await, TuiPaneState::IdlePrompt);
}

#[tokio::test]
async fn test_startup_auto_approves_multiple_sequential_questions() {
    let dir = tempdir().unwrap();
    let backend = Arc::new(MockMultiplexerBackend::new());
    let question1 = "Do you trust the authors of the files in this directory? [y/N]";
    let question2 = "Do you want to enable anonymous error reporting? [Y/n]";
    let ready_screen = "────────────────────────────────────────────\n>\n────────────────────────────────────────────";

    backend.preconfigure_pane(
        "multi_questions",
        question1,
        vec![question2.to_string(), ready_screen.to_string()],
    );

    let manager = TuiSessionManager::with_backend(backend.clone());
    let handle = manager
        .start_session(TuiSessionConfig {
            session_id: "multi_questions".to_string(),
            kind: AgentCliKind::Agy,
            cwd: dir.path().to_path_buf(),
            auto_approve: true,
            ..Default::default()
        })
        .await
        .unwrap();

    let pane = backend.get_pane("multi_questions").unwrap();
    assert_eq!(pane.keys_sent, vec!["y", "Enter", "y", "Enter"]);
    assert_eq!(*handle.last_state.lock().await, TuiPaneState::IdlePrompt);
}

#[tokio::test]
async fn test_startup_retains_waiting_approval_when_auto_approve_disabled() {
    let dir = tempdir().unwrap();
    let backend = Arc::new(MockMultiplexerBackend::new());
    let trust_screen =
        "Quick safety check: Do you trust the authors of the files in this folder? (y/n)";

    backend.preconfigure_pane("manual_trust", trust_screen, Vec::new());

    let manager = TuiSessionManager::with_backend(backend.clone());
    let handle = manager
        .start_session(TuiSessionConfig {
            session_id: "manual_trust".to_string(),
            kind: AgentCliKind::Claude,
            cwd: dir.path().to_path_buf(),
            auto_approve: false,
            ..Default::default()
        })
        .await
        .unwrap();

    let pane = backend.get_pane("manual_trust").unwrap();
    assert!(pane.keys_sent.is_empty());
    match &*handle.last_state.lock().await {
        TuiPaneState::WaitingApproval { prompt_snippet } => {
            assert!(prompt_snippet.contains("trust"));
        }
        other => panic!("Expected WaitingApproval, got {:?}", other),
    }

    let err = handle
        .send_prompt("fix bug", false, Duration::from_millis(500))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("waiting for approval"));
}

#[test]
fn test_approval_keys_select_affirmative_option_in_numbered_menus() {
    use openduck_mcp::tui_session::profiles::suggested_approval_keys;

    for (screen, expected) in [
        (
            "Do you trust this directory?\n❯ 1. Yes, proceed\n  2. No, exit",
            vec!["Enter"],
        ),
        (
            "Do you trust this directory?\n  1. Yes, proceed\n❯ 2. No, exit\nEnter to confirm",
            vec!["Up", "Enter"],
        ),
        (
            "Do you trust this directory?\n> 1) No, exit\n  2) Yes, trust this folder",
            vec!["Down", "Enter"],
        ),
        (
            "Do you trust this directory?\n1. Yes, proceed\n2. No, exit",
            vec!["1", "Enter"],
        ),
        ("Do you trust this directory? [y/N]", vec!["y", "Enter"]),
        ("Choose a theme", vec!["Enter"]),
    ] {
        assert_eq!(suggested_approval_keys(screen), expected, "{screen}");
    }
}

#[tokio::test]
async fn test_unchanged_confirmation_is_not_repeated_or_given_task_text() {
    let dir = tempdir().unwrap();
    let backend = Arc::new(MockMultiplexerBackend::new());
    let handle = mock_agy_session(backend.clone(), dir.path()).await;
    backend.set_screen("agy_submit", "Do you trust this directory? [y/N]");

    let error = handle
        .send_prompt("fix bug", false, Duration::from_millis(700))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("waiting for approval"));
    let pane = backend.get_pane("agy_submit").unwrap();
    assert_eq!(pane.keys_sent, vec!["y", "Enter"]);
    assert!(pane.pasted.is_empty());
}

#[tokio::test]
async fn test_task_is_not_pasted_before_composer_is_ready() {
    let dir = tempdir().unwrap();
    let backend = Arc::new(MockMultiplexerBackend::new());
    let handle = mock_agy_session(backend.clone(), dir.path()).await;
    backend.set_screen("agy_submit", "Starting agent...");

    let error = handle
        .send_prompt("fix bug", false, Duration::from_millis(300))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("prompt was not submitted"));
    let pane = backend.get_pane("agy_submit").unwrap();
    assert!(pane.keys_sent.is_empty());
    assert!(pane.pasted.is_empty());
}

#[tokio::test]
async fn test_numbered_startup_confirmation_is_completed_before_task_submission() {
    let dir = tempdir().unwrap();
    let backend = Arc::new(MockMultiplexerBackend::new());
    let ready = "────────────────────\n>\n────────────────────";
    backend.preconfigure_pane(
        "numbered_trust",
        "Do you trust this directory?\n  1. Yes, proceed\n❯ 2. No, exit\nEnter to confirm",
        vec!["Press Enter to continue".to_string(), ready.to_string()],
    );
    let manager = TuiSessionManager::with_backend(backend.clone());
    let handle = manager
        .start_session(TuiSessionConfig {
            session_id: "numbered_trust".to_string(),
            kind: AgentCliKind::Custom,
            cwd: dir.path().to_path_buf(),
            extra_args: vec!["mock-agent".to_string()],
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(*handle.last_state.lock().await, TuiPaneState::IdlePrompt);
    assert!(backend
        .get_pane("numbered_trust")
        .unwrap()
        .pasted
        .is_empty());

    handle
        .send_prompt("fix bug", false, Duration::from_secs(2))
        .await
        .unwrap();
    let pane = backend.get_pane("numbered_trust").unwrap();
    assert_eq!(pane.keys_sent, vec!["Up", "Enter", "Enter", "Enter"]);
    assert_eq!(pane.pasted, vec![("fix bug".to_string(), true)]);
}
