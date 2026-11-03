use std::sync::Arc;

use chrono::Utc;
use openduck::control::tui_management::{
    AccountRecord, AgentProfileRecord, QuotaSnapshotRecord, SessionOperationRecord,
    SessionRunBindingRecord, SqliteTuiStore, TuiManagementService, TuiSessionRecord,
};
use openduck_mcp::tui_session::TuiSessionManager;

#[tokio::test]
async fn test_store_session_crud_and_attribution() {
    let store = SqliteTuiStore::open(":memory:")
        .await
        .expect("open memory db");
    let now = Utc::now().to_rfc3339();

    let rec = TuiSessionRecord {
        id: "sess_test_1".to_string(),
        session_name: "test_grok_session".to_string(),
        agent_kind: "grok".to_string(),
        cwd: "/tmp".to_string(),
        model: Some("grok-beta".to_string()),
        effort: Some("high".to_string()),
        auto_approve: true,
        proxy_mode: "named".to_string(),
        proxy_id: Some("clash".to_string()),
        effective_proxy: Some("http://127.0.0.1:7890".to_string()),
        account_id: Some("acc_1".to_string()),
        created_by_project_id: Some("proj_alpha".to_string()),
        created_by_task_id: Some("task_101".to_string()),
        created_by_run_id: Some("run_5001".to_string()),
        created_by_subtask_id: None,
        creation_source: "harness".to_string(),
        owner_run_id: Some("run_5001".to_string()),
        ownership_mode: "run_exclusive".to_string(),
        generation: 1,
        lifecycle_state: "active".to_string(),
        process_state: "alive".to_string(),
        tui_pane_state: "idle_prompt".to_string(),
        stop_intent: None,
        cleanup_error: None,
        created_at: now.clone(),
        last_operation_at: Some(now.clone()),
        last_input_at: None,
        last_progress_at: None,
        last_observed_at: Some(now.clone()),
        idle_since: None,
        owner_terminal_at: None,
        reclaim_after: None,
        stopped_at: None,
        reclaimed_at: None,
    };

    store.insert_session(&rec).await.expect("insert session");

    let found = store
        .get_session_by_id("sess_test_1")
        .await
        .expect("get session")
        .expect("session exists");
    assert_eq!(found.session_name, "test_grok_session");
    assert_eq!(found.agent_kind, "grok");
    assert_eq!(found.created_by_run_id.as_deref(), Some("run_5001"));
    assert_eq!(
        found.effective_proxy.as_deref(),
        Some("http://127.0.0.1:7890")
    );

    let active_runs = store
        .list_active_sessions_for_run("run_5001")
        .await
        .expect("list runs");
    assert_eq!(active_runs.len(), 1);
    assert_eq!(active_runs[0].id, "sess_test_1");
}

#[tokio::test]
async fn test_operations_ledger_and_bindings() {
    let store = SqliteTuiStore::open(":memory:")
        .await
        .expect("open memory db");
    let now = Utc::now().to_rfc3339();

    let op = SessionOperationRecord {
        id: "op_test_1".to_string(),
        session_id: "sess_test_1".to_string(),
        session_name: "test_session".to_string(),
        operation_type: "prompt".to_string(),
        operator: "harness".to_string(),
        project_id: Some("proj_1".to_string()),
        task_id: Some("task_1".to_string()),
        run_id: Some("run_100".to_string()),
        subtask_id: None,
        summary: Some("Execute unit test".to_string()),
        content_ref: None,
        status: "started".to_string(),
        error: None,
        duration_ms: None,
        started_at: now.clone(),
        finished_at: None,
    };

    store.insert_operation(&op).await.expect("insert op");
    store
        .complete_operation("op_test_1", "succeeded", None, 350, &now)
        .await
        .expect("complete op");

    let ops = store
        .list_operations(Some("sess_test_1"), None, None, 10, 0)
        .await
        .expect("list ops");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].status, "succeeded");
    assert_eq!(ops[0].duration_ms, Some(350));

    let binding = SessionRunBindingRecord {
        id: "bind_1".to_string(),
        session_id: "sess_test_1".to_string(),
        run_id: "run_100".to_string(),
        project_id: Some("proj_1".to_string()),
        task_id: Some("task_1".to_string()),
        bound_at: now.clone(),
        unbound_at: None,
        reason: Some("Initial attribution".to_string()),
    };
    store
        .insert_binding(&binding)
        .await
        .expect("insert binding");
    let bindings = store
        .list_bindings_for_session("sess_test_1")
        .await
        .expect("list bindings");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].run_id, "run_100");
}

#[tokio::test]
async fn test_agent_profiles_and_accounts() {
    let store = SqliteTuiStore::open(":memory:")
        .await
        .expect("open memory db");
    let now = Utc::now().to_rfc3339();

    let prof = AgentProfileRecord {
        id: "prof_codex".to_string(),
        agent_kind: "codex".to_string(),
        default_model: Some("gpt-4o".to_string()),
        default_effort: Some("high".to_string()),
        account_id: Some("acc_openai".to_string()),
        proxy_mode: "direct".to_string(),
        proxy_id: None,
        max_concurrency: Some(4),
        idle_timeout_mins: Some(15),
        exited_retention_mins: Some(10),
        max_turn_timeout_secs: Some(300),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    store.upsert_profile(&prof).await.expect("upsert profile");

    let loaded_prof = store
        .get_profile_by_agent("codex")
        .await
        .expect("get profile")
        .expect("profile exists");
    assert_eq!(loaded_prof.default_model.as_deref(), Some("gpt-4o"));
    assert_eq!(loaded_prof.proxy_mode, "direct");

    let acc = AccountRecord {
        id: "acc_openai".to_string(),
        provider: "openai".to_string(),
        account_name: "OpenAI-Work".to_string(),
        auth_ref: Some("env:OPENAI_API_KEY".to_string()),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    store.upsert_account(&acc).await.expect("upsert account");

    let accounts = store.list_accounts().await.expect("list accounts");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].account_name, "OpenAI-Work");

    let quota = QuotaSnapshotRecord {
        id: "q_1".to_string(),
        pool_id: "pool_openai".to_string(),
        account_id: Some("acc_openai".to_string()),
        agent_kind: Some("codex".to_string()),
        model_pattern: Some("gpt-4o*".to_string()),
        quota_type: "weekly".to_string(),
        unit: "percent".to_string(),
        used_amount: 25.0,
        limit_amount: Some(100.0),
        remaining_percent: Some(75.0),
        window_start: Some(now.clone()),
        window_end: None,
        reset_at: None,
        source: "cli_status".to_string(),
        status: "valid".to_string(),
        observed_at: now.clone(),
    };
    store.upsert_quota(&quota).await.expect("upsert quota");
    let quotas = store
        .list_quotas(Some("acc_openai"))
        .await
        .expect("list quotas");
    assert_eq!(quotas.len(), 1);
    assert_eq!(quotas[0].remaining_percent, Some(75.0));
}

#[tokio::test]
async fn test_lifecycle_run_ended_and_sweeper() {
    let store = Arc::new(
        SqliteTuiStore::open(":memory:")
            .await
            .expect("open memory db"),
    );
    let session_mgr = Arc::new(TuiSessionManager::default());
    let _service = TuiManagementService::new(store.clone(), session_mgr);

    let now = Utc::now();
    let rec = TuiSessionRecord {
        id: "sess_life_1".to_string(),
        session_name: "sess_life_1_tmux".to_string(),
        agent_kind: "claude".to_string(),
        cwd: "/tmp".to_string(),
        model: None,
        effort: None,
        auto_approve: true,
        proxy_mode: "inherit".to_string(),
        proxy_id: None,
        effective_proxy: None,
        account_id: None,
        created_by_project_id: Some("test_proj".to_string()),
        created_by_task_id: Some("task_99".to_string()),
        created_by_run_id: Some("run_life_10".to_string()),
        created_by_subtask_id: None,
        creation_source: "harness".to_string(),
        owner_run_id: Some("run_life_10".to_string()),
        ownership_mode: "run_exclusive".to_string(),
        generation: 1,
        lifecycle_state: "active".to_string(),
        process_state: "alive".to_string(),
        tui_pane_state: "working".to_string(),
        stop_intent: None,
        cleanup_error: None,
        created_at: now.to_rfc3339(),
        last_operation_at: Some(now.to_rfc3339()),
        last_input_at: Some(now.to_rfc3339()),
        last_progress_at: Some(now.to_rfc3339()),
        last_observed_at: Some(now.to_rfc3339()),
        idle_since: None,
        owner_terminal_at: None,
        reclaim_after: None,
        stopped_at: None,
        reclaimed_at: None,
    };
    store.insert_session(&rec).await.expect("insert");

    // Simulate task run success: should mark retained with 10-minute countdown
    service
        .on_task_run_ended(Some("test_proj"), Some("task_99"), "run_life_10", "success")
        .await
        .expect("run ended success");

    let updated = store
        .get_session_by_id("sess_life_1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.lifecycle_state, "retained");
    assert!(updated.reclaim_after.is_some());

    // Extend retention
    let extended = service
        .extend_session("sess_life_1", 30)
        .await
        .expect("extend session");
    assert!(extended.reclaim_after.is_some());

    // Takeover
    let taken = service
        .takeover_session("sess_life_1")
        .await
        .expect("takeover");
    assert_eq!(taken.ownership_mode, "manual");
    assert!(taken.owner_run_id.is_none());
    assert_eq!(taken.generation, 2);

    // Transfer
    let transferred = service
        .transfer_session(
            "sess_life_1",
            "run_new_20",
            Some("test_proj"),
            Some("task_99"),
        )
        .await
        .expect("transfer");
    assert_eq!(transferred.owner_run_id.as_deref(), Some("run_new_20"));
    assert_eq!(transferred.generation, 3);
}

#[tokio::test]
async fn test_proxy_configuration_and_resolution() {
    let store = Arc::new(
        SqliteTuiStore::open(":memory:")
            .await
            .expect("open memory db"),
    );
    let session_mgr = Arc::new(TuiSessionManager::default());
    let _service = TuiManagementService::new(store.clone(), session_mgr);

    let now = Utc::now().to_rfc3339();

    // 1. Profile with named proxy
    let prof_grok = AgentProfileRecord {
        id: "prof_grok".to_string(),
        agent_kind: "grok".to_string(),
        default_model: Some("grok-beta".to_string()),
        default_effort: None,
        account_id: None,
        proxy_mode: "named".to_string(),
        proxy_id: Some("clash".to_string()),
        max_concurrency: None,
        idle_timeout_mins: None,
        exited_retention_mins: None,
        max_turn_timeout_secs: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    store
        .upsert_profile(&prof_grok)
        .await
        .expect("upsert grok profile");

    // 2. Profile with custom proxy url
    let prof_agy = AgentProfileRecord {
        id: "prof_agy".to_string(),
        agent_kind: "agy".to_string(),
        default_model: None,
        default_effort: None,
        account_id: None,
        proxy_mode: "custom".to_string(),
        proxy_id: Some("http://127.0.0.1:2087".to_string()),
        max_concurrency: None,
        idle_timeout_mins: None,
        exited_retention_mins: None,
        max_turn_timeout_secs: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    store
        .upsert_profile(&prof_agy)
        .await
        .expect("upsert agy profile");

    let grok = store.get_profile_by_agent("grok").await.unwrap().unwrap();
    assert_eq!(grok.proxy_mode, "named");
    assert_eq!(grok.proxy_id.as_deref(), Some("clash"));

    let agy = store.get_profile_by_agent("agy").await.unwrap().unwrap();
    assert_eq!(agy.proxy_mode, "custom");
    assert_eq!(agy.proxy_id.as_deref(), Some("http://127.0.0.1:2087"));
}
