# OpenDuck Harness (`openduck-harness`)

The core execution runtime, evaluation engine, sandboxing framework, and deterministic replay layer for **OpenDuck**.

`openduck-harness` transforms OpenDuck from a conventional agent loop into a complete **Agent Harness** capable of evaluating, benchmarking, isolating, and verifying agent runs across diverse environments.

---

## 🏛️ Architecture & Core Pillars

```
+-----------------------------------------------------------------------------------+
|                              OPENDUCK-HARNESS                                     |
+-----------------------------------------------------------------------------------+
|  [ Agent Policy Abstraction (`AgentPolicy`) ]                                     |
|   ├── Decision loop separation (stateless/stateful action generation)             |
|   └── Adapters: EchoPolicy, MockPolicy, ReplayPolicy, Live Agent Policy           |
+-----------------------------------------------------------------------------------+
|  [ Environment Sandboxing (`SandboxDriver`) ]                                     |
|   ├── LocalSandbox: Host execution with directory boundaries                      |
|   ├── ContainerSandbox: Ephemeral Docker/OCI container isolation                  |
|   └── Workspace Checkpoint / Snapshot / Restore                                   |
+-----------------------------------------------------------------------------------+
|  [ Evaluation & Benchmark Engine (`EvalRunner`, `TaskSpec`) ]                     |
|   ├── Datasets: SWE-bench style JSONL, YAML recipes, custom benchmark suites      |
|   ├── Verification: CommandVerifier (unit tests), DiffVerifier (golden diffs)     |
|   └── Metrics: Pass/fail rates, trajectory tokens, cost, latency scoring          |
+-----------------------------------------------------------------------------------+
|  [ Deterministic Replay & Telemetry (`Cassette`, `TrajectoryLogger`) ]             |
|   ├── VCR-style recording of LLM completions and MCP tool outputs                 |
|   ├── Offline zero-cost regression testing and trajectory comparison              |
|   └── OpenTelemetry and JSONL trajectory export                                   |
+-----------------------------------------------------------------------------------+
```

---

## 📦 Modules

### 1. `policy` (`AgentPolicy`)
Decouples agent decision-making from the execution harness. An `AgentPolicy` receives a context view (`HarnessContextView`) and available tools, producing an `AgentAction`:
- `AgentAction::CallTools(Vec<ToolCallRequest>)`: Dispatch one or more tool calls.
- `AgentAction::FinalAnswer(String)`: Conclude task execution with a final response.
- `AgentAction::YieldControl`: Yield execution with a given `YieldReason`.
- `AgentAction::RequestInput`: Request user or environment clarification.

### 2. `sandbox` (`SandboxDriver`)
Standardized abstraction for isolated workspace execution:
- `LocalSandbox`: Direct host execution with working directory isolation.
- `ContainerSandbox`: Ephemeral Docker / OCI container sandbox with customizable images and environment variables.
- Capabilities: `exec_command`, `read_file`, `write_file`, `snapshot`, `restore_snapshot`.

### 3. `eval` (`EvalRunner`, `TaskSpec`, `Verifier`)
High-throughput benchmark evaluation runner:
- `TaskSpec`: Self-contained task definition (problem statement, setup commands, verifiers, timeout, max turns).
- `CommandVerifier`: Runs verification shell commands (e.g. `cargo test`, `pytest`) and asserts exit codes / output.
- `DiffVerifier`: Compares workspace file changes against expected golden diffs.
- `EvalRunner`: Orchestrates concurrent task evaluation and outputs structured `EvaluationReport` summaries.

### 4. `replay` (`Cassette`, `ReplayPolicy`)
VCR-style deterministic replay for debugging and regression testing:
- **Record**: Captures prompts, LLM responses, tool calls, and results with content-hashed keys.
- **Replay**: Replays cached interactions offline with zero network calls and deterministic results.

### 5. `context` & `runtime` Compaction
Multi-tiered, model-aware token budgeting and semantic context compaction:
- **Token Estimation**: Fast and accurate estimation (`HarnessMessage::estimate_tokens`, `HarnessContextView::total_estimated_tokens`).
- **Watermark Threshold**: Dynamic triggering based on model `context_limit` and `compaction_threshold` (default 0.8 / 80%).
- **Layer 1 Tool Output Snipping**: `snip_tool_output` automatically folds high-volume tool execution outputs (lines & bytes limits) with UTF-8 safety while preserving key diagnostics and exit codes.
- **Layer 2/3 Semantic Compaction**: `compact_context_messages` extracts structured facts (explored files, modified files, test execution outcomes, errors, system nudges) and synthesizes a high-density Markdown summary block between initial task instructions and recent active context.

### 6. `project` (`ProjectHarnessConfig`, `ProjectTaskStore`)
Project-level harness integration:
- Configured via `.goose/harness.yaml` or `.openduck/harness.yaml`.
- Reads task definitions from `.goose/tasks/*.yaml` or `.openduck/tasks/*.yaml`.

---

## ⚙️ Context Management Configuration

Configure context limits, compaction watermarks, and tool output limits on `AgentHarness`:

```rust
let harness = AgentHarness::new(policy, sandbox)
    .with_context_limit(128_000)               // Model token window limit
    .with_compaction_threshold(0.8)            // Trigger compaction at 80% capacity
    .with_max_context_messages(80)             // Secondary message-count guardrail
    .with_max_tool_output_limits(200, 32*1024); // Max 200 lines / 32 KB per tool result
```

---

## 🛠️ Usage Examples (CLI)

```bash
# Evaluate a benchmark dataset across 4 parallel sandboxes
openduck harness eval --dataset benchmarks/swe_eval.jsonl --concurrency 4 --output-dir eval_results/

# Run a task and record execution to a cassette
openduck harness run --prompt "Fix bug in tokenizer" --record cassette.json

# Replay the cassette offline
openduck harness replay --cassette cassette.json
```

---

## 🧪 Testing

Run unit and integration tests:

```bash
cargo test -p openduck-harness
```
