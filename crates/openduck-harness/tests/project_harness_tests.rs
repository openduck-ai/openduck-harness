use openduck_harness::project::{
    compose_dynamic_run_prompt, ProjectHarnessConfig, ProjectTaskDefinition, ProjectTaskStore,
    TaskVerifierSpec,
};
use tempfile::tempdir;

#[tokio::test]
async fn test_project_harness_config_lifecycle() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // 1. Default config when no file exists
    let default_cfg = ProjectHarnessConfig::load_or_default(root).await;
    assert_eq!(default_cfg.version, "1.0");
    assert_eq!(default_cfg.execution.max_turns, 25);
    assert_eq!(default_cfg.paths.tasks_dir, ".goose/tasks");

    // 2. Custom config save
    let mut custom_cfg = default_cfg.clone();
    custom_cfg.policy.provider = Some("anthropic".into());
    custom_cfg.policy.model = Some("claude-3-7-sonnet".into());
    custom_cfg.execution.max_turns = 40;
    custom_cfg.execution.history_timeout_seconds = Some(120);
    custom_cfg.sandbox.setup_commands = vec!["echo 'setup'".into()];

    let saved_path = custom_cfg.save_to_file(root).await.unwrap();
    assert!(saved_path.exists());

    // 3. Reload config
    let reloaded = ProjectHarnessConfig::load_or_default(root).await;
    assert_eq!(reloaded.policy.provider.as_deref(), Some("anthropic"));
    assert_eq!(reloaded.policy.model.as_deref(), Some("claude-3-7-sonnet"));
    assert_eq!(reloaded.execution.max_turns, 40);
    assert_eq!(reloaded.execution.history_timeout_seconds, Some(120));
    assert_eq!(reloaded.sandbox.setup_commands, vec!["echo 'setup'"]);

    // 4. Resolve paths
    assert_eq!(reloaded.resolve_tasks_dir(root), root.join(".goose/tasks"));
    assert_eq!(
        reloaded.resolve_results_dir(root),
        root.join(".goose/harness_results")
    );
    assert_eq!(
        reloaded.resolve_cassettes_dir(root),
        root.join(".goose/cassettes")
    );
}

#[tokio::test]
async fn test_project_task_store_crud() {
    let dir = tempdir().unwrap();
    let tasks_dir = dir.path().join(".goose/tasks");

    // 1. List on empty directory
    let list = ProjectTaskStore::list_tasks(&tasks_dir).await.unwrap();
    assert!(list.is_empty());

    // 2. Create task
    let task1 = ProjectTaskDefinition {
        id: "task-fetch-polymarket".into(),
        name: Some("Fetch Polymarket Data".into()),
        category: Some("data-api".into()),
        tags: vec!["python".into(), "web3".into()],
        prompt: "Fetch top 5 markets".into(),
        environment: None,
        verifier: Some(TaskVerifierSpec::SimpleCommand {
            command: "python3 -c 'print(1)'".into(),
            expected_exit_code: Some(0),
            expected_stdout: Some("1".into()),
        }),
        max_turns: Some(15),
        timeout_seconds: Some(120),
        cron: None,
        schedule_paused: false,
        subtasks: None,
        auto_decompose: Some(true),
        phase_max_turns: Some(vec![80, 200, 220]),
        dynamic_prompt: false,
    };

    let saved_path = ProjectTaskStore::save_task(&tasks_dir, &task1)
        .await
        .unwrap();
    assert!(saved_path.exists());

    // 3. List tasks
    let list = ProjectTaskStore::list_tasks(&tasks_dir).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "task-fetch-polymarket");
    assert_eq!(list[0].name, "Fetch Polymarket Data");
    assert_eq!(list[0].tags, vec!["python", "web3"]);
    assert!(list[0].has_verifier);

    // 4. Get task
    let fetched = ProjectTaskStore::get_task(&tasks_dir, "task-fetch-polymarket")
        .await
        .unwrap();
    assert_eq!(fetched.id, "task-fetch-polymarket");
    assert_eq!(fetched.prompt, "Fetch top 5 markets");
    assert_eq!(fetched.max_turns, Some(15));
    assert_eq!(fetched.cron, None);
    assert!(!fetched.schedule_paused);
    assert_eq!(fetched.auto_decompose, Some(true));
    assert_eq!(
        fetched.phase_max_turns.as_deref(),
        Some(&[80, 200, 220][..])
    );

    // 5. Convert to BenchmarkItem
    let benchmark_item = ProjectTaskStore::to_benchmark_item(&fetched, None);
    assert_eq!(benchmark_item.task.id, "task-fetch-polymarket");
    assert_eq!(benchmark_item.task.dataset, "data-api");
    assert_eq!(benchmark_item.task.max_turns, Some(15));
    assert_eq!(
        benchmark_item.task.phase_max_turns.as_deref(),
        Some(&[80, 200, 220][..])
    );

    // 6. Delete task
    ProjectTaskStore::delete_task(&tasks_dir, "task-fetch-polymarket")
        .await
        .unwrap();
    let list_after = ProjectTaskStore::list_tasks(&tasks_dir).await.unwrap();
    assert!(list_after.is_empty());
}

#[tokio::test]
async fn test_project_task_schedule_roundtrip() {
    let dir = tempdir().unwrap();
    let tasks_dir = dir.path().join(".goose/tasks");

    let task = ProjectTaskDefinition {
        id: "nightly-eval".into(),
        name: Some("Nightly eval".into()),
        category: None,
        tags: vec![],
        prompt: "Run the eval".into(),
        environment: None,
        verifier: None,
        max_turns: None,
        timeout_seconds: None,
        cron: Some("0 2 * * *".into()),
        schedule_paused: true,
        subtasks: None,
        auto_decompose: None,
        phase_max_turns: None,
        dynamic_prompt: false,
    };

    ProjectTaskStore::save_task(&tasks_dir, &task)
        .await
        .unwrap();

    let fetched = ProjectTaskStore::get_task(&tasks_dir, "nightly-eval")
        .await
        .unwrap();
    assert_eq!(fetched.cron.as_deref(), Some("0 2 * * *"));
    assert!(fetched.schedule_paused);

    let listed = ProjectTaskStore::list_tasks(&tasks_dir).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].cron.as_deref(), Some("0 2 * * *"));
    assert!(listed[0].schedule_paused);

    let yaml = tokio::fs::read_to_string(tasks_dir.join("nightly-eval.yaml"))
        .await
        .unwrap();
    assert!(yaml.contains("cron:"));
    assert!(yaml.contains("schedulePaused: true"));
}

fn sample_task(id: &str, tags: Vec<String>) -> ProjectTaskDefinition {
    ProjectTaskDefinition {
        id: id.into(),
        name: Some(id.into()),
        category: Some("demo".into()),
        tags,
        prompt: "do the work".into(),
        environment: None,
        verifier: None,
        max_turns: None,
        timeout_seconds: None,
        cron: None,
        schedule_paused: false,
        subtasks: None,
        auto_decompose: None,
        phase_max_turns: None,
        dynamic_prompt: false,
    }
}

fn set_mtime(path: &std::path::Path, unix_secs: u64) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix_secs))
        .unwrap();
}

#[tokio::test]
async fn test_list_tasks_orders_newest_created_first() {
    let dir = tempdir().unwrap();
    let tasks_dir = dir.path().join(".goose/tasks");

    // IDs would sort aaa, mmm, zzz. Created/modified times are the opposite of that.
    let oldest = ProjectTaskStore::save_task(&tasks_dir, &sample_task("aaa", vec!["alpha".into()]))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    let middle = ProjectTaskStore::save_task(&tasks_dir, &sample_task("zzz", vec!["beta".into()]))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    let newest = ProjectTaskStore::save_task(
        &tasks_dir,
        &sample_task("mmm", vec!["alpha".into(), "beta".into()]),
    )
    .await
    .unwrap();

    // Birth time is often unavailable; mtime is the accepted fallback.
    set_mtime(&oldest, 1_700_000_000);
    set_mtime(&middle, 1_750_000_000);
    set_mtime(&newest, 1_800_000_000);

    let list = ProjectTaskStore::list_tasks(&tasks_dir).await.unwrap();
    let ids: Vec<&str> = list.iter().map(|task| task.id.as_str()).collect();
    assert_eq!(ids, vec!["mmm", "zzz", "aaa"]);
    assert!(list[0].created_at > list[1].created_at);
    assert!(list[1].created_at > list[2].created_at);
}

#[test]
fn test_compose_dynamic_run_prompt() {
    assert_eq!(
        compose_dynamic_run_prompt("Base prompt", true, None).unwrap(),
        "Base prompt"
    );
    assert_eq!(
        compose_dynamic_run_prompt("Base prompt", false, Some("  ")).unwrap(),
        "Base prompt"
    );

    let composed =
        compose_dynamic_run_prompt("Base prompt", true, Some("  Look at the parser. ")).unwrap();
    assert_eq!(
        composed,
        "Base prompt\n\nAdditional instructions for this run:\nLook at the parser."
    );

    let err = compose_dynamic_run_prompt("Base prompt", false, Some("extra")).unwrap_err();
    assert!(err.to_string().contains("does not accept a dynamic prompt"));
}

#[tokio::test]
async fn test_dynamic_prompt_roundtrip() {
    let dir = tempdir().unwrap();
    let tasks_dir = dir.path().join(".goose/tasks");

    let yaml = r#"
id: task-review
name: Review selection
prompt: Review the code.
dynamic_prompt: true
"#;
    tokio::fs::create_dir_all(&tasks_dir).await.unwrap();
    tokio::fs::write(tasks_dir.join("task-review.yaml"), yaml)
        .await
        .unwrap();

    let fetched = ProjectTaskStore::get_task(&tasks_dir, "task-review")
        .await
        .unwrap();
    assert!(fetched.dynamic_prompt);
    assert_eq!(fetched.prompt, "Review the code.");

    let listed = ProjectTaskStore::list_tasks(&tasks_dir).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].dynamic_prompt);

    let mut saved = fetched.clone();
    saved.prompt = compose_dynamic_run_prompt(
        &saved.prompt,
        saved.dynamic_prompt,
        Some("Selected excerpt from `src/lib.rs`:\n\nfn main() {}"),
    )
    .unwrap();
    let item = ProjectTaskStore::to_benchmark_item(&saved, None);
    assert!(item
        .task
        .problem_statement
        .contains("Additional instructions for this run:"));
    assert!(item.task.problem_statement.contains("fn main() {}"));
    assert!(item.task.problem_statement.starts_with("Review the code."));

    saved.prompt = "Review the code.".into();
    ProjectTaskStore::save_task(&tasks_dir, &saved)
        .await
        .unwrap();
    let yaml_out = tokio::fs::read_to_string(tasks_dir.join("task-review.yaml"))
        .await
        .unwrap();
    assert!(yaml_out.contains("dynamicPrompt: true"));
}
