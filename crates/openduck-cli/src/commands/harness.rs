use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use openduck::control::harness::resolve_harness_policy;
use openduck_harness::eval::datasets::{load_jsonl_dataset, load_recipe_task};
use openduck_harness::eval::{EvalRunner, EvalRunnerConfig, TaskSpec};
use openduck_harness::policy::adapters::EchoPolicy;
use openduck_harness::policy::AgentPolicy;
use openduck_harness::replay::{Cassette, ReplayPolicy};
use openduck_harness::runtime::AgentHarness;
use openduck_harness::sandbox::local::LocalSandbox;
use openduck_harness::sandbox::SandboxDriver;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Subcommand)]
pub enum HarnessCommand {
    /// Evaluate a benchmark suite across tasks
    Eval(EvalArgs),

    /// Execute a single task with optional recording
    Run(RunArgs),

    /// Deterministically replay a previously recorded cassette
    Replay(ReplayArgs),
}

#[derive(Debug, Args)]
pub struct EvalArgs {
    /// Path to the dataset file (.jsonl or recipe .yaml)
    #[arg(short, long)]
    pub dataset: PathBuf,

    /// Number of concurrent tasks to evaluate
    #[arg(short, long, default_value_t = 4)]
    pub concurrency: usize,

    /// Output directory for evaluation report and trajectories
    #[arg(short, long, default_value = "harness_eval_results")]
    pub output_dir: PathBuf,

    /// Maximum turns allowed per task
    #[arg(long, default_value_t = 25)]
    pub max_turns: usize,

    /// Provider to evaluate (e.g., openai, anthropic, google). Defaults to configured provider.
    #[arg(long)]
    pub provider: Option<String>,

    /// Model to evaluate. Defaults to configured model for provider.
    #[arg(long)]
    pub model: Option<String>,

    /// Use lightweight mock EchoPolicy instead of live model
    #[arg(long)]
    pub echo: bool,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Path to a task specification or recipe file
    #[arg(short, long)]
    pub task: Option<PathBuf>,

    /// Direct prompt to execute as a task
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// Path to record cassette output to
    #[arg(long)]
    pub record: Option<PathBuf>,

    /// Maximum turns allowed
    #[arg(long, default_value_t = 25)]
    pub max_turns: usize,

    /// Provider to use (e.g., openai, anthropic, google). Defaults to configured provider.
    #[arg(long)]
    pub provider: Option<String>,

    /// Model to use. Defaults to configured model for provider.
    #[arg(long)]
    pub model: Option<String>,

    /// Use lightweight mock EchoPolicy instead of live model
    #[arg(long)]
    pub echo: bool,
}

#[derive(Debug, Args)]
pub struct ReplayArgs {
    /// Path to the recorded cassette file (.json)
    #[arg(short, long)]
    pub cassette: PathBuf,

    /// Optional path to task recipe or dataset file to replay against
    #[arg(short, long)]
    pub task: Option<PathBuf>,
}

pub async fn handle_harness_command(command: HarnessCommand) -> Result<()> {
    match command {
        HarnessCommand::Eval(args) => handle_eval(args).await,
        HarnessCommand::Run(args) => handle_run(args).await,
        HarnessCommand::Replay(args) => handle_replay(args).await,
    }
}

async fn handle_eval(args: EvalArgs) -> Result<()> {
    println!("Loading benchmark dataset from: {:?}", args.dataset);

    let items = if args.dataset.extension().and_then(|e| e.to_str()) == Some("yaml")
        || args.dataset.extension().and_then(|e| e.to_str()) == Some("yml")
    {
        let item = load_recipe_task(&args.dataset).await?;
        vec![item]
    } else {
        load_jsonl_dataset(&args.dataset).await?
    };

    println!("Loaded {} tasks to evaluate.", items.len());

    let config = EvalRunnerConfig {
        concurrency: args.concurrency,
        output_dir: args.output_dir.clone(),
        max_turns: args.max_turns,
    };

    let runner = EvalRunner::new(config);
    let suite_name = args
        .dataset
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "benchmark".into());

    let report = if args.echo {
        println!("Evaluating with Mock EchoPolicy...");
        runner
            .run_suite(&suite_name, items, EchoPolicy::default)
            .await?
    } else {
        let goose_policy =
            resolve_harness_policy(args.provider.as_deref(), args.model.as_deref()).await?;
        println!("Evaluating with Goose Agent ({})", goose_policy.name());
        let policy_template = goose_policy.clone();
        runner
            .run_suite(&suite_name, items, move || policy_template.clone())
            .await?
    };

    println!("\n=== Evaluation Summary ===");
    println!("Total Tasks:    {}", report.metrics.total_tasks);
    println!("Passed Tasks:   {}", report.metrics.passed_tasks);
    println!("Failed Tasks:   {}", report.metrics.failed_tasks);
    println!("Pass Rate:      {:.2}%", report.metrics.pass_rate * 100.0);
    println!("Avg Duration:   {:.2} ms", report.metrics.avg_duration_ms);
    println!("Total Tools:    {}", report.metrics.total_tool_calls);
    println!(
        "\nReport written to: {:?}",
        args.output_dir.join("eval_report.json")
    );

    Ok(())
}

async fn handle_run(args: RunArgs) -> Result<()> {
    let task_spec = if let Some(task_file) = args.task {
        if task_file.extension().and_then(|e| e.to_str()) == Some("yaml")
            || task_file.extension().and_then(|e| e.to_str()) == Some("yml")
        {
            let item = load_recipe_task(&task_file).await?;
            item.task
        } else {
            let items = load_jsonl_dataset(&task_file).await?;
            items
                .into_iter()
                .next()
                .map(|i| i.task)
                .context("Dataset contains no tasks")?
        }
    } else if let Some(prompt) = args.prompt {
        TaskSpec::new("cli-task", "cli-adhoc", prompt)
    } else {
        anyhow::bail!("Must provide either --task <path> or --prompt <text>");
    };

    let mut sandbox = LocalSandbox::ephemeral()?;
    sandbox.initialize().await?;

    if args.echo {
        let base_policy = EchoPolicy::default();
        if let Some(record_path) = args.record {
            let mut cas_obj = Cassette::new(&task_spec.id);
            cas_obj.task_spec = Some(task_spec.clone());
            let cassette = Arc::new(Mutex::new(cas_obj));
            let policy = ReplayPolicy::new_record(base_policy, cassette.clone());
            let mut harness = AgentHarness::new(policy, sandbox).with_max_turns(args.max_turns);

            println!(
                "Running task '{}' with cassette recording (EchoPolicy)...",
                task_spec.id
            );
            let res = harness.run_task(&task_spec).await?;

            let cas = cassette.lock().await;
            cas.save_to_file(&record_path).await?;
            println!("Cassette saved to {:?}", record_path);
            println!("Task finished with status: {:?}", res.status);
        } else {
            let mut harness =
                AgentHarness::new(base_policy, sandbox).with_max_turns(args.max_turns);
            println!("Running task '{}' with EchoPolicy...", task_spec.id);
            let res = harness.run_task(&task_spec).await?;
            println!("Task finished with status: {:?}", res.status);
        }
    } else {
        let base_policy =
            resolve_harness_policy(args.provider.as_deref(), args.model.as_deref()).await?;
        println!(
            "Running task '{}' with Goose Agent ({})",
            task_spec.id,
            base_policy.name()
        );

        if let Some(record_path) = args.record {
            let mut cas_obj = Cassette::new(&task_spec.id);
            cas_obj.task_spec = Some(task_spec.clone());
            let cassette = Arc::new(Mutex::new(cas_obj));
            let policy = ReplayPolicy::new_record(base_policy, cassette.clone());
            let mut harness = AgentHarness::new(policy, sandbox).with_max_turns(args.max_turns);

            let res = harness.run_task(&task_spec).await?;

            let cas = cassette.lock().await;
            cas.save_to_file(&record_path).await?;
            println!("Cassette saved to {:?}", record_path);
            println!("Task finished with status: {:?}", res.status);
            if let Some(ans) = res.final_answer {
                println!("Final Answer:\n{}", ans);
            }
        } else {
            let mut harness =
                AgentHarness::new(base_policy, sandbox).with_max_turns(args.max_turns);
            let res = harness.run_task(&task_spec).await?;
            println!("Task finished with status: {:?}", res.status);
            if let Some(ans) = res.final_answer {
                println!("Final Answer:\n{}", ans);
            }
        }
    }

    Ok(())
}

async fn handle_replay(args: ReplayArgs) -> Result<()> {
    println!("Loading cassette from {:?}", args.cassette);
    let cassette = Cassette::load_from_file(&args.cassette).await?;

    let task_spec = if let Some(task_file) = args.task {
        if task_file.extension().and_then(|e| e.to_str()) == Some("yaml")
            || task_file.extension().and_then(|e| e.to_str()) == Some("yml")
        {
            let item = load_recipe_task(&task_file).await?;
            item.task
        } else {
            let items = load_jsonl_dataset(&task_file).await?;
            items
                .into_iter()
                .next()
                .map(|i| i.task)
                .context("Dataset contains no tasks")?
        }
    } else if let Some(spec) = &cassette.task_spec {
        spec.clone()
    } else {
        TaskSpec::new(&cassette.name, "replay", "Offline deterministic replay")
    };

    let cassette_arc = Arc::new(Mutex::new(cassette));
    let replay_policy: ReplayPolicy<EchoPolicy> = ReplayPolicy::new_replay(cassette_arc);
    let mut sandbox = LocalSandbox::ephemeral()?;
    sandbox.initialize().await?;

    let mut harness = AgentHarness::new(replay_policy, sandbox);
    println!(
        "Starting offline replay execution for task '{}'...",
        task_spec.id
    );
    let res = harness.run_task(&task_spec).await?;
    println!("Replay finished successfully with status: {:?}", res.status);
    if let Some(ans) = res.final_answer {
        println!("Replayed Answer:\n{}", ans);
    }

    Ok(())
}
