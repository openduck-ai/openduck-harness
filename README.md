<div align="center">

# OpenDuck 🦆

**The Open-Source AI Agent Harness & Enterprise Agent System**

_Sandboxed execution, benchmark evaluation, deterministic replay, multi-project workspaces, and extensible agent protocols across CLI, Web Hub, Desktop, Mobile, and SDKs._

<p align="center">
  <a href="https://opensource.org/licenses/Apache-2.0"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="License: Apache 2.0"></a>
  <a href="https://github.com/openduck-ai/openduck-harness"><img src="https://img.shields.io/badge/Rust-1.94%2B-brightgreen.svg" alt="Rust Edition 2021"></a>
</p>
</div>

---

## 🌟 What is OpenDuck?

**OpenDuck** is an open-source AI **Agent Harness** and **Enterprise Agent System** built in Rust for high performance, portability, and safety. 

OpenDuck transcends traditional single-loop AI assistants by providing a modular, sandboxed execution runtime and an unified control plane. Whether you are running interactive development sessions, executing deterministic benchmarks, managing multi-project workspaces with visual Git graphs, or scheduling unattended background tasks with email alerts, OpenDuck provides the full infrastructure needed to run, test, and steer autonomous agents at scale.

The CLI binary is `openduck` (with legacy `duck` aliase). Workspace crates use the `openduck-*` prefix. Configuration and environment variables prefer `OPENDUCK_*`.

---

## ✨ Key Features

### 🧪 1. Agent Harness & Evaluation Runtime
- **Decoupled Agent Policies**: Modular `AgentPolicy` trait cleanly separating agent decision logic (ReAct, state machine, external adapters) from the execution runtime.
- **Pluggable Sandboxing (`SandboxDriver`)**: Run tasks in host-isolated environments (`LocalSandbox`, Docker/OCI `ContainerSandbox`) with workspace snapshots, checkpoints, and rollbacks.
- **Evaluation & Benchmark Suite (`openduck harness eval`)**: Run batch benchmark suites (JSONL datasets, YAML recipes, SWE-bench style tasks) with parallel execution, metric scoring (pass/fail, turns, latency, token costs), and trajectory export.
- **Deterministic Record & Replay (`openduck harness replay`)**: Record agent LLM completions and MCP tool interactions into VCR-style cassettes for zero-cost, reproducible regression testing and offline debugging.
- **Automated Verifiers**: Built-in `CommandVerifier` (test runner output) and `DiffVerifier` (git patch diff against expected golden state).

### 🚀 2. Enterprise Agent System & Multi-Project Workspace
- **Project Dashboard**: Register and manage multiple software projects with automatic directory binding, metadata tags, and insight summaries.
- **Visual Git Management**: Built-in Git control module with an interactive commit graph, branch & tag ref tooltips, file diff viewer, status inspection, and automated commit message generation.
- **Project-Level Task Harness**: Define repeatable task specs in `.openduck/tasks/` configured via `.openduck/harness.yaml`.
- **Integrated Markdown & File Explorer**: Inspect repository files with rich Markdown previews directly in the Web Hub.

### ⏰ 3. Cron Task Scheduling & Automated Locking
- **In-Process Cron Scheduler**: Schedule recurring recipes, health checks, repo scrapers, or benchmark runs.
- **Task Concurrency Locking**: Prevents overlapping execution of scheduled runs with automatic status tracking (`running`, `completed`, `failed`, `skipped`).
- **Headless Recipe Automation**: Execute multi-step structured recipes unattended with full session persistence.

### 📬 4. Notification & Reporting System
- **SMTP Email Notifications**: Automated alerts upon task completion, execution failure, or scheduled report generation.
- **Configurable Task Reports**: HTML/Markdown formatted email summaries containing task status, duration, diffs, and verification metrics.

### 🔌 5. Broad Model Context & Protocol Ecosystem
- **15+ LLM Providers**: Anthropic, OpenAI, Google Gemini, Ollama (local models), OpenRouter, Azure Foundry, AWS Bedrock, Databricks, Snowflake, and more.
- **Model Context Protocol (MCP)**: Native support for 70+ community MCP extensions plus built-in extensions:
  - `agy`: Antigravity CLI integration for delegated subagent task execution.
  - `developer`: Local shell command execution, file reading, and editing.
  - `computercontroller`: Headless browser scraping and automated web interaction.
  - `polymarket`, `memory`, `tutorial`, and more.
- **Agent Client Protocol (ACP)**: Standardized streaming protocol over HTTP/SSE, WebSockets, and stdio for rich UI client integration.

### 🖥️ 6. Unified Multi-Interface Suite
- **OpenDuck CLI**: Full-featured terminal interface (`session`, `run`, `harness`, `serve`, `schedule`, `term`, `doctor`).
- **OpenDuck Web Hub (`ui/hub`)**: Lightweight web dashboard for project workspaces, git visualizer, harness task runner, and server settings.
- **OpenDuck Desktop (`ui/desktop`)**: Native Electron desktop application for macOS, Windows, and Linux.
- **OpenDuck Mobile PWA (`ui/mobile`)**: Mobile web client for steering agents and approving permissions on the go.
- **Cross-Language SDKs**: First-class client libraries for TypeScript (`@openduck/sdk`), Python, Rust, JVM/Kotlin (`io.github.aaif-goose:gdk`), and Swift.

---

## 🏗️ Architecture Overview

```
+-----------------------------------------------------------------------------------+
|                                OPENDUCK SYSTEM                                    |
+-----------------------------------------------------------------------------------+
|  [ Interfaces & Frontends ]                                                       |
|   ├── OpenDuck CLI (`openduck`, `duck`)                                  |
|   ├── OpenDuck Web Hub (`ui/hub` & `ui/hub-core` React Dashboard)                 |
|   ├── OpenDuck Desktop (`ui/desktop` Electron App)                                |
|   ├── OpenDuck Mobile PWA (`ui/mobile`)                                           |
|   └── Cross-Language SDKs (TypeScript `@openduck/sdk`, Python, Kotlin, Rust)      |
+-----------------------------------------------------------------------------------+
|  [ Control Plane & Server (`openduck serve` / ACP & REST) ]                       |
|   ├── Project Workspace Registry & Markdown Source Store                          |
|   ├── Interactive Git Management (Graph, Diffs, Commit Gen)                       |
|   ├── Task Scheduler (Cron, Task Locking, Concurrency Control)                    |
|   ├── SMTP Notification & Email Alert Engine                                      |
|   └── Auth & Multi-Session Database (SQLite `sessions.db`, `control.db`)          |
+-----------------------------------------------------------------------------------+
|  [ Agent Harness Runtime (`openduck-harness`) ]                                   |
|   ├── Pluggable Agent Policies (`AgentPolicy`: ReAct, State Machine, Adapters)    |
|   ├── Sandbox Drivers (`SandboxDriver`: LocalSandbox, Docker ContainerSandbox)    |
|   ├── Evaluation & Benchmark Engine (`EvalRunner`, `TaskSpec`, Verifiers)         |
|   └── Deterministic Record & Replay Layer (`Cassette`, `MockProvider`)            |
+-----------------------------------------------------------------------------------+
|  [ Tool & Extensibility Layer ]                                                   |
|   ├── Model Context Protocol (MCP) Servers (Built-in: agy, dev, browser, memory)  |
|   └── 15+ LLM Providers (OpenAI, Anthropic, Gemini, Ollama, Azure, Bedrock, etc.)  |
+-----------------------------------------------------------------------------------+
```

---

## 🚀 Quickstart

### 1. Install the CLI

Install the pre-built binary using the official installer script:

```bash
curl -fsSL https://github.com/openduck-ai/openduck-harness/releases/download/stable/download_cli.sh | bash
```

_This installs `openduck` along with `duck` compatibility aliase._

Or build from source using Rust:

```bash
git clone https://github.com/openduck-ai/openduck-harness
cd goose
source bin/activate-hermit   # activates pinned Rust, Node, pnpm toolchains
cargo build --release
```

### 2. Configure Your Provider

Run the interactive configuration wizard:

```bash
openduck configure
```

Or set your provider environment variables directly:

```bash
# Example: Anthropic Claude
export ANTHROPIC_API_KEY="your-api-key"
export OPENDUCK_PROVIDER__TYPE="anthropic"
export OPENDUCK_PROVIDER__MODEL="claude-3-7-sonnet-latest"

# Example: OpenAI GPT
export OPENAI_API_KEY="your-api-key"
export OPENDUCK_PROVIDER__TYPE="openai"
export OPENDUCK_PROVIDER__MODEL="gpt-4o"

# Example: Local Ollama
export OPENDUCK_PROVIDER__TYPE="ollama"
export OPENDUCK_PROVIDER__HOST="http://localhost:11434"
export OPENDUCK_PROVIDER__MODEL="qwen2.5-coder:latest"
```

Verify your setup:

```bash
openduck doctor
openduck info --check
```

### 3. Background Daemon & Web Hub

Start the OpenDuck ACP + REST background server:

```bash
# Start the background server (e.g., port 3000 or default 3284)
export OPENDUCK_SERVER__SECRET_KEY="your-strong-secret-key"
openduck serve --platform desktop --host 0.0.0.0 --port 3000 --enable-scheduler
```

Start the OpenDuck Web Hub frontend dashboard:

```bash
# In a separate terminal, launch the Web Hub UI
cd ui/hub
pnpm install
pnpm run dev
```

Open [http://localhost:5173/hub/](http://localhost:5173/hub/) in your browser and connect using your configured secret key.

---

## 📖 CLI Usage Examples

### Interactive Chat Session
```bash
# Start a new interactive session
openduck session

# Resume the most recent session or a specific session by ID/name
openduck session --resume
openduck session --resume --name "auth-refactor"

# Pass built-in or custom MCP extensions
openduck session --with-builtin developer,agy
```

### Headless Instruction Execution
```bash
# Execute a task from a prompt or markdown instruction file
openduck run "Audit this repository for security vulnerabilities"
openduck run --file instructions.md
```

### 🧪 Agent Harness: Evaluation, Benchmarks & Replay
```bash
# 1. Run a batch benchmark evaluation on a task dataset
openduck harness eval --dataset benchmarks/swe_tasks.jsonl --concurrency 4 --output-dir eval_results/

# 2. Execute a single task and record execution to a cassette
openduck harness run --prompt "Fix broken test in tests/auth.rs" --record cassettes/auth_fix.json

# 3. Deterministically replay a recorded cassette offline (no API costs)
openduck harness replay --cassette cassettes/auth_fix.json
```

### Cron Task Scheduling
```bash
# Add a scheduled recipe job
openduck schedule add --name "nightly-audit" --cron "0 2 * * *" --recipe recipes/security_scan.yaml

# List scheduled jobs and inspection
openduck schedule list
openduck schedule run-now --id <JOB_ID>
```

---

## 🌐 OpenDuck Web Hub & Frontends

### OpenDuck Web Hub (`ui/hub`)
The Web Hub is an enterprise agent management dashboard that connects to `openduck serve`:
- **Project Workspaces**: Easily switch between registered repositories.
- **Git Graph & Visualizer**: Interactive commit log, branch heads, tags, and file diff views.
- **Harness Task Runner**: Run and monitor harness tasks, benchmark evaluations, and replay cassettes with live logs.
- **Chat & Session Manager**: Stream live ACP responses with tool approvals.
- **Schedules & Email Alerts**: Configure automated cron jobs and SMTP notification recipients.

To launch the Web Hub in development:
```bash
cd ui/hub
pnpm install
pnpm run dev
```

### OpenDuck Desktop (`ui/desktop`)
Cross-platform Electron application for macOS, Windows, and Linux.
```bash
cd ui/desktop
pnpm install
pnpm run start
```

---

## 📦 Workspace Repository Structure

```
crates/
├── openduck                 # Core agent logic, control plane, scheduler, git, notifications
├── openduck-harness         # Harness runtime, sandbox drivers, eval engine, replay cassettes
├── openduck-agent           # Pluggable agent policies and state machines
├── openduck-cli             # CLI entrypoints (`openduck`, `goose`, `duck`)
├── openduck-mcp             # MCP extensions (agy, developer, computer controller, polymarket, memory)
├── openduck-providers       # LLM provider implementations (Anthropic, OpenAI, Gemini, Ollama, etc.)
├── openduck-provider-types  # Canonical provider request/response type definitions
├── openduck-sdk             # Cross-language SDK bindings & UniFFI (Python, Kotlin/Maven, Swift)
├── openduck-context-management # Context pruning, prompt compaction, and memory summaries
├── openduck-download-manager   # Binary and model asset download management
├── openduck-local-inference    # Candle-based local model execution
├── openduck-test            # Integration and end-to-end test suites
└── openduck-test-support    # Mock servers, test fixtures, and assertions

ui/
├── hub                      # OpenDuck Web Hub (React 19 + Vite dashboard)
├── hub-core                 # Shared REST & ACP client logic for Web Hub
├── desktop                  # Electron desktop application
├── mobile                   # Mobile PWA client
├── sdk                      # TypeScript SDK package (`@openduck/sdk`)
└── text                     # Terminal UI (TUI) package
```

---

## ⚙️ Configuration & Environment Variables

OpenDuck stores configuration under standard XDG directories:
- **Configuration Root**: `~/.config/openduck/` (legacy fallback: `~/.config/goose/`)
- **Data & Sessions**: `~/.local/share/openduck/` (legacy fallback: `~/.local/share/goose/`)
- **State & Run**: `~/.local/state/openduck/` (legacy fallback: `~/.local/state/goose/`)

For a detailed breakdown of all configuration files in `~/.config/openduck/` (including `config.yaml`, `secrets.yaml`, `notifications.yaml`, `permission.yaml`, and custom providers), see the [Configuration Files Guide](documentation/docs/guides/config-files.md).

### Key Environment Variables

| Variable | Description |
|---|---|
| `OPENDUCK_PROVIDER__TYPE` | Active LLM provider (e.g. `anthropic`, `openai`, `google`, `ollama`) |
| `OPENDUCK_PROVIDER__MODEL` | Model identifier (e.g. `claude-3-7-sonnet-latest`, `gpt-4o`) |
| `OPENDUCK_SERVER__SECRET_KEY` | Shared secret key for authenticating ACP & REST API requests |
| `OPENDUCK_STATE_MACHINE` | Set to `1` to enable the next-generation agent state machine |
| `OPENDUCK_DISABLE_KEYRING` | Set to `1` to store secrets in `~/.config/openduck/secrets.yaml` instead of OS keyring |
| `OPENDUCK_NOTIFICATION_CONFIG`| Custom file path to override `notifications.yaml` |
| `OPENDUCK_PATH_ROOT` | Absolute root directory override for all XDG configuration and data paths |
| `OPENDUCK_LOG_LEVEL` | Tracing log level (`info`, `debug`, `trace`) |

_Note: All `GOOSE_*` environment variables and configuration paths remain supported as backwards-compatible aliases._
