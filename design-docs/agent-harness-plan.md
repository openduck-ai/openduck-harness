# Goose Agent Harness — Architecture & Implementation Plan

| Field | Value |
|---|---|
| **Title** | Transforming Goose from an Agent to an Agent Harness |
| **Status** | Proposed / Draft |
| **Target Crates** | `crates/goose`, `crates/goose-harness` (new), `crates/goose-cli`, `crates/goose-sdk-types`, `ui/hub` |
| **Audience** | Goose core architecture, evaluation, and runtime engineers |
| **Workspace** | `/mnt/e/goose` |

---

## 1. Executive Summary & Vision

### Current State
Today, **Goose** is an autonomous AI agent application:
- Prompt construction, LLM provider interaction, tool execution, session management, and the ReAct decision loop are tightly bound inside `crates/goose/src/agents/agent.rs`.
- It executes directly on the host machine using local subprocesses (`execute_commands.rs`) and system-level MCP extensions.
- Benchmarking, comparative evaluation, deterministic replay, workspace rollback, and pluggable agent decision policies are not first-class abstractions.

### Target State: The Agent Harness
An **Agent Harness** is an evaluation, sandboxing, and execution runtime that manages the environment lifecycle, safety gates, tool protocols, evaluation benchmarks, and observability for **any** agent implementation.

```
+-----------------------------------------------------------------------------------+
|                                 GOOSE HARNESS                                     |
+-----------------------------------------------------------------------------------+
|  [ Agent Policies / Decision Engines ]                                            |
|   ├── Builtin Default ReAct Policy (Goose Agent)                                 |
|   ├── Plan-and-Solve / Reflexion / SWE-agent Policies                             |
|   └── External Agent Adapter (ACP / Custom Subprocess / REST)                     |
+-----------------------------------------------------------------------------------+
|  [ Control Plane & Steering ]                                                     |
|   ├── Step-level Interceptors (Pre/Post Tool, Pre/Post LLM)                      |
|   ├── Human-in-the-loop & Safety Approvals                                       |
|   └── Trajectory Tracer & OTel Telemetry                                         |
+-----------------------------------------------------------------------------------+
|  [ Evaluation & Benchmark Engine ]                                                |
|   ├── Benchmark Task Spec (Setup, Prompt, Golden Diff, Test Graders)              |
|   ├── Batch Runner (SWE-bench, GAIA, RepoQA, Custom Suites)                      |
|   └── Deterministic Record & Replay Layer (Mock Providers & Tool Cache)          |
+-----------------------------------------------------------------------------------+
|  [ Environment & Sandbox Lifecycle ]                                              |
|   ├── Sandbox Drivers: Local | Docker / OCI Container | MicroVM                  |
|   └── Workspace State: Checkpoint, Snapshot, Rollback, OverlayFS                 |
+-----------------------------------------------------------------------------------+
|  [ Tool & Protocol Layer ]                                                        |
|   └── Model Context Protocol (MCP) | Subagents | ACP                             |
+-----------------------------------------------------------------------------------+
```

---

## 2. Core Architectural Pillars

### Pillar A: Decoupled Agent Policy (`AgentPolicy` Trait)
Break the monolithic `Agent::reply` loop into a clean separation of concerns:
1. **`AgentHarness` (Runtime)**: Owns session history, tool registration (MCP), sandbox management, token budgets, safety constraints, telemetry emission, and evaluation hooks.
2. **`AgentPolicy` (Decision Strategy)**: A stateless or stateful decision function that takes the current context and tool schema, returning an `AgentAction` (e.g., `CallTools`, `FinalOutput`, `Delegate`, `RequestHumanClarification`).

```rust
#[async_trait]
pub trait AgentPolicy: Send + Sync {
    /// Identifier for benchmark logging & telemetry
    fn name(&self) -> &str;

    /// Produces the next action given the current harness context view
    async fn step(
        &mut self,
        context: &HarnessContextView,
        tools: &[ToolDefinition],
    ) -> Result<AgentAction>;
}

pub enum AgentAction {
    CallTools(Vec<ToolCallRequest>),
    FinalAnswer(String),
    YieldControl { reason: YieldReason },
    RequestInput { prompt: String },
}
```

### Pillar B: Pluggable Environment Sandboxing (`SandboxDriver`)
Replace raw host process execution with a unified `Sandbox` abstraction:
- **`LocalSandbox`**: Host execution (current behavior, fast for local CLI usage).
- **`ContainerSandbox`**: Ephemeral Docker/Podman container per run/task with isolated network and filesystem.
- **`MicroVMSandbox`**: Firecracker / QEMU for multi-tenant or untrusted evaluation.

```rust
#[async_trait]
pub trait SandboxDriver: Send + Sync {
    async fn initialize(&mut self, env_spec: &EnvironmentSpec) -> Result<()>;
    async fn exec_command(&self, cmd: &str, opts: &ExecOptions) -> Result<ExecOutput>;
    async fn read_file(&self, path: &Path) -> Result<Vec<u8>>;
    async fn write_file(&self, path: &Path, content: &[u8]) -> Result<()>;
    async fn snapshot(&self, label: &str) -> Result<SnapshotId>;
    async fn restore_snapshot(&mut self, id: &SnapshotId) -> Result<()>;
    async fn teardown(self) -> Result<()>;
}
```

### Pillar C: Evaluation & Benchmark Engine
A dedicated evaluation suite to benchmark any agent policy across standardized datasets:
1. **`TaskSpec`**:
   - `id`, `dataset`, `repo`, `base_commit`.
   - `environment_setup`: Shell scripts or Dockerfile to prepare dependencies.
   - `problem_statement`: Issue text, task prompt, or goal instructions.
   - `verifier`: Golden patch test runner (`cargo test`, `pytest`, diff comparison, or LLM-as-judge).
2. **Batch Eval Runner**:
   - Parallel task scheduling across isolated sandboxes.
   - Live streaming of pass/fail rates, trajectory tokens, cost, and time-to-solve.
   - Export standard benchmark artifacts (`eval_report.json`, `trajectories.jsonl`, `patches/`).

### Pillar D: Deterministic Replay & Mocking Layer
- Intercept provider completions (`Provider::complete`) and MCP tool executions.
- In `Record` mode: serialize request/response pairs with hash keys.
- In `Replay` mode: replay cached tool results and LLM outputs to debug agent loops deterministically without network calls or API costs.

### Pillar E: Steering, Interventions & Human-in-the-Loop
- **Hook Lifecycle**:
  - `on_before_policy_step` (inspect/modify prompt context).
  - `on_after_policy_step` (inspect/override proposed actions).
  - `on_before_tool_exec` (confirm, alter arguments, or mock result).
  - `on_after_tool_exec` (filter or summarize tool output before policy sees it).
- **Interactive Rewind & Branching**:
  - Roll back state to step $N$, alter the prompt or tool output, and branch execution.

---

## 3. Codebase Delta & Module Plan

### New Crate: `crates/goose-harness`
Contains the evaluation engine, sandbox drivers, and task runners:
```
crates/goose-harness/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── policy.rs             # AgentPolicy trait & standard adapters
    ├── sandbox/
    │   ├── mod.rs            # SandboxDriver trait
    │   ├── local.rs          # Local execution driver
    │   └── container.rs      # Docker / OCI container driver
    ├── eval/
    │   ├── mod.rs            # Eval harness core
    │   ├── task.rs           # TaskSpec & Benchmark definitions
    │   ├── runner.rs         # Concurrency & Batch evaluation runner
    │   ├── metrics.rs        # Token, latency, tool error, benchmark scoring
    │   └── datasets/         # Adapters: SWE-bench, GAIA, Custom JSONL
    ├── replay/
    │   ├── mod.rs            # Recording & Replay manager
    │   └── cassette.rs       # VCR-style serialization of LLM & Tool calls
    └── telemetry/
        └── trajectory.rs     # Trajectory exporter (JSONL, OTel spans)
```

### Changes to Existing Crates

| Target | Changes |
|---|---|
| `crates/goose/src/agents/agent.rs` | Refactor into `AgentRuntime` + extract `DefaultGoosePolicy`. Implement `AgentPolicy` trait. |
| `crates/goose/src/agents/execute_commands.rs` | Delegate command execution through `SandboxDriver`. |
| `crates/goose/src/providers/` | Add replay interceptor hooks (`RecordedProvider`). |
| `crates/goose/src/control/` | Add REST endpoints for harness benchmarks, task execution, and trajectory inspection. |
| `crates/goose-cli/src/` | Add CLI subcommands: `goose harness eval`, `goose harness run`, `goose harness replay`. |
| `ui/hub/` | Add UI views to view benchmark runs, compare agent trajectories, and inspect workspace diffs. |

---

## 4. Phased Implementation Roadmap

### Phase 1: Policy & Runtime Decoupling (Foundation)
- Define `AgentPolicy` trait in `crates/goose` or `crates/goose-harness`.
- Wrap existing `Agent::reply` loop into `DefaultReActPolicy`.
- Ensure 100% backward compatibility with all existing CLI commands, ACP sessions, and UI desktop.

### Phase 2: Sandbox & Workspace Isolation
- Implement `SandboxDriver` trait with `LocalSandbox` (default) and `ContainerSandbox` (Docker).
- Move workspace file operations and shell execution behind `SandboxDriver`.
- Add Git state checkpointing (`git stash create` / worktree branch isolation) for step rollbacks.

### Phase 3: Benchmark & Evaluation Core
- Implement `TaskSpec` and dataset parsers (SWE-bench format, custom JSON/YAML recipe tasks).
- Implement batch parallel task runner with metric collection (time, tokens, pass/fail rate).
- Add CLI commands: `goose harness eval --dataset <path> --concurrency <N>`.

### Phase 4: Deterministic Replay & Trajectory Tracing
- Build VCR-style cassette recording for provider completions and tool results.
- Implement `goose harness replay --cassette <path>` for regression testing without API calls.
- Standardize trajectory logging into standalone JSONL and OpenTelemetry traces.

### Phase 5: Control Plane, UI & Multi-Agent ACP Adapter
- Connect `goose serve` / Hub API to trigger and stream harness benchmark runs.
- Add trajectory visualizer in Hub/UI for side-by-side agent comparison.
- Enable running external agents (via ACP / CLI) inside the Goose harness.

---

## 5. Verification & Acceptance Criteria

1. **Unit & Integration Tests**:
   - `cargo test -p goose-harness` covers sandbox lifecycle, task verification, and replay engine.
   - `cargo test -p goose` retains complete parity for all existing agent features and recipes.
2. **Benchmark Smoke Test**:
   - Run a 5-task sample benchmark evaluation using `goose harness eval` and verify metric report generation.
3. **Deterministic Replay Test**:
   - Record a multi-step session, replay it offline with disconnected network, and assert exact trajectory match.
