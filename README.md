<div align="center">

# OpenDuck 🦆

**The Open-Source AI Agent Harness & Enterprise Agent System**

_Sandboxed execution, benchmark evaluation, deterministic replay, multi-project workspaces, and extensible agent protocols across CLI, Web Hub, Desktop, Mobile, and SDKs._

<p align="center">
  <a href="https://opensource.org/licenses/Apache-2.0"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="License: Apache 2.0"></a>
  <a href="https://github.com/openduck-ai/openduck-harness"><img src="https://img.shields.io/badge/Rust-1.94%2B-brightgreen.svg" alt="Rust Edition 2021"></a>
</p>

<p align="center">
  <strong>English</strong> | <a href="README.zh-CN.md">简体中文</a>
</p>
</div>

---

## 🌟 What is OpenDuck?

**OpenDuck** is an open-source AI **Agent Harness** and **Enterprise Agent System** built in Rust for high performance, portability, and safety. 

OpenDuck transcends traditional single-loop AI assistants by providing a modular, sandboxed execution runtime and an unified control plane. Whether you are running interactive development sessions, executing deterministic benchmarks, managing multi-project workspaces with visual Git graphs, or scheduling unattended background tasks with email alerts, OpenDuck provides the full infrastructure needed to run, test, and steer autonomous agents at scale.

The CLI binary is `openduck` (legacy aliases `goose` and `duck` still work). Workspace crates use the `openduck-*` prefix. Configuration and environment variables prefer `OPENDUCK_*`.

---

## ✨ Key Features

### 🧪 1. Agent Harness & Evaluation Runtime
- **Decoupled Agent Policies**: Modular `AgentPolicy` trait cleanly separating agent decision logic (ReAct, state machine, external adapters) from the execution runtime.
- **Pluggable Sandboxing (`SandboxDriver`)**: Run tasks in host-isolated environments (`LocalSandbox`, isolated Git branch `WorktreeSandbox` with symlinked untracked configs, shared `CARGO_TARGET_DIR`, and optional auto-commit, or Docker/OCI `ContainerSandbox`) with workspace snapshots, checkpoints, and rollbacks.
- **Per-Project FIFO Task Serialization**: Projects enqueue task executions into a dedicated FIFO queue to serialize runs per project, preventing conflicting Git working copy modifications while enabling parallel executions across separate projects. Queued tasks report live status in the Web Hub.
- **Automated Task Git Commits & Mutation Locking**: Automatically stage and commit workspace changes upon task completion with configurable commit message formats, project settings, and UI toggles. Protected by in-process repository mutation write locks (`with_repo_write_lock`) and automatic stale `index.lock` recovery for interrupted processes.
- **Pluggable Harness Tool Extensions (`HarnessToolExtension`)**: Extend `AgentHarness` directly with custom tool suites—including `TuiAgentHarnessExtension` (`tui_agent_start`, `tui_agent_prompt`, `tui_agent_read`, `tui_agent_keys`, `tui_agent_stop`, `tui_agent_list`) so harness tasks can delegate work to persistent interactive coding agents with wait-based screen polling (`wait_secs`), unchanged screen suppression, `stallHint` detection, and bounded non-blocking prompt submission—and `ProjectTasksExtension` for interacting with workspace tasks.
- **AI-Assisted Task Drafting & Inline Autosaving Creator**: Draft structured task specifications and dependency-ordered subtask plans with acceptance criteria, `ExpectedOutputVerifier` checks, dynamic `max_turns` synchronization, and an inline autosaving task creator with local draft persistence per project.
- **Evaluation & Benchmark Suite (`openduck harness eval`)**: Run batch benchmark suites (JSONL datasets, YAML recipes, SWE-bench style tasks) with parallel execution, metric scoring (pass/fail, turns, latency, token costs), and trajectory export.
- **Deterministic Record & Replay (`openduck harness replay`)**: Record agent LLM completions and MCP tool interactions into VCR-style cassettes for zero-cost, reproducible regression testing and offline debugging.
- **Task Continuation & Subtask Resume**: Resume, re-run, or branch follow-up executions directly from task history and replay cassettes with preserved context, subtask outcome tracking, failure reason handling, and live visual status tags in trajectory logs.
- **Enhanced Context Compaction, Fact Carry-Forward & Anti-Stagnation**: Compacted summaries, notices, and repetition warnings are forwarded directly into the provider context (`[HARNESS CONTEXT]`). `ExtractedContextFacts` accumulates and deduplicates explored files, tool observations, errors, and advisor findings across successive compactions. Expanded default message budget to 300 messages while preserving tool pairs and visible assistant text. `RepetitiveCallDetector` injects non-blocking warnings when repetitive tool loops occur, resetting on mutations, alongside periodic 15-turn unchanged-notes reminders.
- **Provider Telemetry, Adaptive Routing & Mid-Turn Switching**: Track token consumption (prompt/completion/cached), request latencies, time to first token (TTFT), network phase timings, volume, and estimated costs across harness task executions, backed by a **Provider Circuit Breaker** with automatic adaptive failover. The `adaptive` provider (`auto`, `regular`, `economy` modes) automatically routes between regular and economy models based on off-peak windows and observed first-token latency with periodic probing, while `AgentRoutingHandle` enables live mid-turn model and provider switching (taking effect on the next inference request) across both the legacy loop and `OPENDUCK_STATE_MACHINE=1`. Per-call `routing_tier` and resolved model (`last_actual_candidate`) are propagated through `ProviderUsage` into `TaskProviderStats` and `EvaluationReport` (`by_model` keyed by `adaptive_tier`).
- **Memory Gating & Jemalloc Heap Profiling**: In-process memory monitoring, logging, and pre-execution memory gates to prevent out-of-memory (OOM) failures under heavy loads. Optional `jemalloc-prof` heap sampling in `openduck serve` via `SIGUSR2` profile dumps.
- **Automated Verifiers & Safety Controls**: Built-in `CommandVerifier` (test runner output), `DiffVerifier` (git patch diff against expected golden state), and `ExpectedOutputVerifier` (subtask output validation), along with configurable `max_turns` limits (`OPENDUCK_MAX_TURNS`), anti-stagnation guards, and graceful handling of malformed tool calls.

### 🚀 2. Enterprise Agent System & Multi-Project Workspace
- **Project Dashboard**: Register and manage multiple software projects with automatic directory binding, metadata tags, and insight summaries.
- **Multi-Repository Visual Git Management**: Built-in Git control module with an interactive commit graph, branch & tag ref tooltips, file diff viewer, worktree management, status inspection, serialized mutation locking, and automated commit message generation. Automatically discovers nested Git repositories, `.gitmodules` submodules (including nested submodules inside symlinked worktrees), symlinked repositories, and worktrees within a project root, and supports project-level custom repository registration merged into discovery results—providing a repository switcher, repo-scoped Git endpoints (`repo` / `repo_path` targeting for status, diff, log, staging, and commit), upstream ahead/behind tracking, and remote sync operations (pull, commit, push) backed by in-process write locks (`with_repo_write_lock`).
- **Unified TUI Agent Management & Quota Pools**: Centralized control plane and Web Hub dashboard (`/tui-agents`) to manage agent profiles (`agy`, `claude`, `codex`, `grok`, `custom`), account credentials, shared quota pools, session run bindings, and operation audit history.
- **Model-Scoped Quota Monitoring, Admission Gating & `/quota` Slash Command**: Real-time quota exhaustion and rate limit detection across agent profiles (Grok, Claude, Codex, Agy)—including native quota readers for Agy (`fetchAvailableModels`) and Codex (session JSONL `rate_limits`). Persists and queries quota snapshots by agent, model, and account; enforces fail-open quota admission gating on TUI launch and prompt (blocking only on fresh exhausted snapshots with a known future reset); supports per-model Agy startup fallback across the discovered catalog; and provides `/quota` and `/usage` slash commands for live inspection, refresh, manual reset, and suspension.
- **Dynamic CLI Model Discovery & Hybrid Picker**: Dynamic runtime CLI model extraction via `discovery_command` / `cli_discovery_command` stdout parsing (Codex, AGY, Grok, Claude) with fallback catalogs (Gemini 4 Argon / 3.8, Claude 3.7 / 3.5, GPT-4.5 / o3, Grok 3), dual-mode picker (fuzzy-search catalog selector vs. custom model input), and dynamic catalog refresh via REST and Web Hub.
- **Per-Agent TUI Outbound Proxy Settings**: Configure independent HTTPS proxy settings per agent profile (`harness.tui_sessions.<agent>.https_proxy`) directly via REST and the Web Hub.
- **Project Tasks Agent Extension & Interactive Drawer**: Expose project task queries and actions directly to interactive chat agents with a dedicated UI drawer.
- **Project-Level Task Harness**: Define, draft, and execute repeatable task specs in `.openduck/tasks/` configured via `.openduck/harness.yaml`.
- **Interactive Project Terminal & TUI Sessions**: Manage persistent `tmux`-backed agent TUI sessions (`agy`, `claude`, `codex`, `grok`, `custom`) directly from each project workspace.
- **Integrated Markdown & File Explorer**: Inspect repository files with rich Markdown previews directly in the Web Hub.
- **Historical Tool Argument Snipping**: Automatic compaction of historical tool call arguments in `openduck-context-management` to curb token inflation and drastically reduce step latency in long trajectories.

### ⏰ 3. Cron Task Scheduling & Automated Locking
- **In-Process Cron Scheduler**: Schedule recurring recipes, health checks, repo scrapers, or benchmark runs.
- **Task Concurrency Locking**: Prevents overlapping execution of scheduled runs with automatic status tracking (`running`, `completed`, `failed`, `skipped`).
- **Headless Recipe Automation**: Execute multi-step structured recipes unattended with full session persistence.

### 📬 4. Notification & Reporting System
- **SMTP Email Notifications**: Automated alerts upon task completion, execution failure, or scheduled report generation.
- **Configurable Task Reports & PDF Exports**: HTML/Markdown formatted email summaries containing task status, duration, diffs, and verification metrics, plus standalone PDF export of task trajectories and conclusions with full CJK character support.

### 🔌 5. Broad Model Context & Protocol Ecosystem
- **15+ LLM Providers & Reasoning Models**: Anthropic, OpenAI, Google Gemini (with thought signature extraction), Ollama (local models), OpenRouter, Azure Foundry, AWS Bedrock, Databricks, Snowflake, and Qwen (with reasoning effort mapping for Qwen 3.8 Flash, QwQ, and thinking variants).
- **Decoupled Provider & Model Selection & Adaptive Routing**: Independent role model resolution (`active_model`, `secondary_provider_model`) decoupled from provider definitions, with a precedence hierarchy allowing major and secondary roles to share the same provider with independent models (V2 configuration API `selectionVersion: 2`). The `adaptive` provider (`auto`, `regular`, `economy` modes) routes between regular and economy models using off-peak windows, observed TTFT latency, and periodic probing, while `AgentRoutingHandle` supports live mid-turn switching on the next inference request.
- **Global Outbound HTTP & SOCKS Proxy Support**: Connect providers and MCP servers through HTTP or SOCKS5 proxies configured via declarative files (`config.yaml`), environment variables (`HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`), or the Web Hub UI, complete with credential redaction, built-in network health monitoring, and endpoint reachability diagnostics.
- **Model Context Protocol (MCP)**: Native support for 70+ community MCP extensions plus built-in extensions:
  - `tui_agent` / `tui_session`: Persistent interactive coding agent TUI & tmux driver (`agy`, `grok`, `codex`, `claude`, `custom`) with bracketed paste, idle prompt detection (including bordered and unbordered composers), spinner vs braille art classification, interactive modal/keystroke answering, wait-based `tui_agent_read` (`wait_secs`, unchanged screen suppression, `stallHint`), bounded 10s prompt submission with busy-session rejection, lossless side-channel transcript correlation, live human attachment, dynamic CLI model discovery, model-scoped quota tracking and admission gating, and native `AgentHarness` tool integration.
  - `agy`: Antigravity CLI integration for delegated subagent task execution.
  - `developer`: Local shell command execution, file reading, and editing.
  - `computercontroller`: Headless browser scraping and automated web interaction.
  - `polymarket`, `memory`, `tutorial`, and more.
- **Agent Client Protocol (ACP)**: Standardized streaming protocol over HTTP/SSE, WebSockets, and stdio for rich UI client integration.

### 🖥️ 6. Unified Multi-Interface Suite
- **OpenDuck CLI**: Full-featured terminal interface (`session`, `run`, `harness`, `serve`, `schedule`, `term`, `doctor`) with built-in interactive slash commands including `/quota` and `/usage` for agent quota inspection and management.
- **OpenDuck Web Hub (`ui/hub`)**: Enterprise web dashboard featuring:
  - **Full Internationalization (i18n)**: English and Chinese (EN / ZH) language support with a real-time language switcher.
  - **Tasks & Running Tasks Page (`/running-tasks`)**: Unified view of active harness jobs and historical task runs with status filtering, pagination, real-time phase, current tool invocation, truncated args, elapsed timers, cancel controls, inline trajectory/log inspection, and direct navigation to project chat and logs.
  - **TUI Agents Page (`/tui-agents`)**: Unified management for agent profiles, account bindings, quota pools, active sessions, and audit history, featuring dynamic CLI model discovery, dual-mode model picker (fuzzy-search catalog and custom model freeform input), and model-scoped quota status badges with availability/stall hints and manual reset controls.
  - **Inline Autosaving Task Creator**: Collapsible, autosaving draft editor per project with local draft persistence and discard controls.
  - **PDF Trajectory & Conclusion Export**: Download, print, or preview full task trajectories and conclusions as styled PDFs with CJK font support (STSong-Light).
  - **Deep-Linking & History Navigation**: URL query parameter synchronization for task history, agent logs, file previews, and split-view states.
  - **Settings Section Navigation & Adaptive Routing**: Localized sidebar navigation with smooth scroll tracking in Server Settings, including Adaptive Routing configuration (`auto`/`regular`/`economy` modes, off-peak windows) and live routing labels in the model picker.
  - **Multi-Repository Git & Project Workspaces**: Discovers nested Git repositories, `.gitmodules` submodules (including nested submodules inside symlinked worktrees), symlinked repositories, and worktrees within project directories, alongside project-level custom repository registration, a repository selector dropdown, repo-scoped status/diff/staging/commit workflows, ahead/behind upstream commit counters, toolbar and row-level remote actions (pull, push, and subrepo commit) with conflict protection, commit graph, and commit generator.
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
|   ├── OpenDuck CLI (`openduck`, `goose`, `duck`)                                  |
|   ├── OpenDuck Web Hub (`ui/hub` & `ui/hub-core` React Dashboard, i18n EN/ZH)     |
|   ├── OpenDuck Desktop (`ui/desktop` Electron App)                                |
|   ├── OpenDuck Mobile PWA (`ui/mobile`)                                           |
|   └── Cross-Language SDKs (TypeScript `@openduck/sdk`, Python, Kotlin, Rust)      |
+-----------------------------------------------------------------------------------+
|  [ Control Plane & Server (`openduck serve` / ACP & REST) ]                       |
|   ├── Project Workspace Registry, TUI Session Manager & Model-Scoped Quota Store    |
|   ├── Per-Project FIFO Task Queue & Serialized Git Mutation Locks                 |
|   ├── Multi-Repository Git Engine (Submodules, Symlinks, Worktrees, Custom Repos)   |
|   ├── In-Browser Config Manager, Outbound Proxy, Adaptive & Decoupled Routing     |
|   ├── Task Scheduler (Cron, Task Locking, Concurrency Control)                    |
|   ├── SMTP Alerts & PDF Trajectory Exporter (CJK-aware STSong-Light)              |
|   ├── Memory Monitor, OOM Gate & Heap Profiling (`jemalloc-prof`)                 |
|   └── Auth & Multi-Session Database (SQLite `sessions.db`, `control.db`)          |
+-----------------------------------------------------------------------------------+
|  [ Agent Harness Runtime (`openduck-harness`) ]                                   |
|   ├── Pluggable Agent Policies (`AgentPolicy`: ReAct, State Machine, Adapters)    |
|   ├── Live Mid-Turn Routing (`AgentRoutingHandle`, `ProviderRoutingStatus`)       |
|   ├── Sandbox Drivers (`SandboxDriver`: Local, Git Worktree, Docker Container)    |
|   ├── Pluggable Tool Extensions (`HarnessToolExtension`: TuiAgent, ProjectTasks)  |
|   ├── Task Drafting, Inline Autosaving Creator & Evaluation Engine (`EvalRunner`) |
|   ├── Task Continuations, Subtask Resume & Trajectory Status Tracking             |
|   ├── Provider Telemetry, Routing Tiers & Trajectory Metrics (Tokens, TTFT)       |
|   └── Deterministic Record & Replay Layer (`Cassette`, `MockProvider`)            |
+-----------------------------------------------------------------------------------+
|  [ Context & Optimization Layer (`openduck-context-management`) ]                 |
|   ├── Fact Carry-Forward (`ExtractedContextFacts`, `[HARNESS CONTEXT]`)           |
|   ├── Anti-Stagnation Loop Protection (`RepetitiveCallDetector`)                  |
|   └── Compaction, Pruning & Historical Tool Call Argument Snipping                |
+-----------------------------------------------------------------------------------+
|  [ Tool & Extensibility Layer ]                                                   |
|   ├── Model Context Protocol (MCP) Servers (tui_session, agy, dev, browser, etc.) |
|   ├── Dynamic CLI Model Discovery & Live Catalog Switching (Codex, Grok, Claude)  |
|   └── 15+ LLM Providers (OpenAI, Anthropic, Gemini, Ollama, Qwen, Azure, Proxies) |
+-----------------------------------------------------------------------------------+
```

---

## 🚀 Quickstart

### 1. Install the CLI

Install the pre-built binary using the official installer script:

```bash
curl -fsSL https://github.com/openduck-ai/openduck-harness/releases/download/stable/download_cli.sh | bash
```

_This installs `openduck` along with legacy `goose` and `duck` compatibility aliases._

Or build from source using Rust:

```bash
git clone https://github.com/openduck-ai/openduck-harness
cd openduck-harness
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
openduck session --with-builtin developer,agy,tui_agent
```

### 🖥️ Persistent Agent TUI & Tmux Sessions
Drive external coding agents interactively with zero cold-start, multi-line bracketed paste, dynamic model switching, and live terminal attachability:

```bash
# Start an OpenDuck session with the tui_agent driver enabled
openduck session --with-builtin tui_agent

# Attach live to a running agent session (agy, grok, codex, or claude)
tmux -L openduck attach -t <session_id>

# Discover model catalogs or switch model on a running session via REST
curl http://localhost:3000/api/tui/agents/claude/models
curl -X POST http://localhost:3000/api/tui/sessions/<session_id>/switch-model \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-3-7-sonnet"}'

# Dynamically parse available models from agent CLI help/discovery output
curl -X POST http://localhost:3000/api/v1/tui-agents/<agent_id>/parse-models

# Dispatch prompts or raw keystrokes directly to an active TUI agent session
curl -X POST http://localhost:3000/api/v1/tui-agents/<agent_id>/prompt \
  -H "Content-Type: application/json" \
  -d '{"prompt":"Refactor auth tests"}'
curl -X POST http://localhost:3000/api/v1/tui-agents/<agent_id>/keys \
  -H "Content-Type: application/json" \
  -d '{"keys":"y\n"}'
```

### 📊 Agent Quota Management & Slash Command
Monitor agent usage limits and unblock exhausted quotas interactively in chat sessions:

```markdown
- `/quota` or `/usage`         # Display table of agent quotas, remaining %, and reset timers
- `/quota refresh [agent]`     # Re-query quota status from provider/logs
- `/quota reset <agent>`       # Reset agent quota to 100% (valid) and unblock immediately
- `/quota set <agent> <pct>`   # Manually set remaining quota percentage (e.g. `/quota set grok 80%`)
- `/quota block <agent>`       # Manually mark quota as exhausted / suspend agent until reset
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

# 3. Execute a task inside an isolated Git worktree on a dedicated branch with auto-commit
openduck harness run --prompt "Refactor error handling in git module" --worktree --worktree-branch "feature/git-errors" --auto-commit

# 4. Deterministically replay a recorded cassette offline (no API costs)
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
- **Full Internationalization (i18n)**: Native English and Chinese (EN / ZH) language support with persistent language switcher.
- **Project Workspaces**: Register, switch, and inspect multiple software repositories.
- **Multi-Repository Git & Worktree Visualizer**: Discovers nested Git repositories, `.gitmodules` submodules (including nested submodules inside symlinked worktrees), symlinked repositories, and worktrees within project directories, and supports project-level custom repository registration; includes a repository switcher dropdown, repo-scoped status/diff/staging/commit workflows, ahead/behind upstream commit counters, toolbar pull/push for the selected repository and unified row actions (pull, push, and subrepo commit) with conflict protection and write lock synchronization, interactive commit log, branch heads, tags, and automated commit messages.
- **Harness Task Runner, AI Drafting & Continuations**: Inline autosaving task creator with local draft persistence, dependency-ordered subtasks with synchronized `max_turns` budgets, isolated Git worktrees with optional auto-commit, follow-ups on historical runs, and cassette replay.
- **Unified Running & Historical Tasks Page (`/running-tasks`)**: Real-time monitoring page unifying active harness jobs and historical task runs across projects with status filtering, pagination, current execution phase, active tool invocation, truncated arguments, elapsed timers, cancel controls, inline trajectory/log inspection, and direct navigation to project chat and logs.
- **Unified TUI Agents Dashboard (`/tui-agents`)**: Manage coding agent profiles (`agy`, `claude`, `codex`, `grok`, `custom`), account credential bindings, quota pools, active sessions, and operation audit history. Includes dynamic CLI model discovery ("Discover Models from CLI"), dual-mode model picker (fuzzy-search catalog selector vs. custom model freeform input), and model-scoped quota status badges with availability/stall hints and manual reset controls.
- **Interactive Project Terminal & TUI Sessions**: Launch, monitor, attach to persistent `tmux`-backed coding agent sessions, and switch active models dynamically via fuzzy search.
- **Provider Stats & Network Timing Dashboard**: Real-time telemetry monitoring for token consumption, latency, TTFT, network phase timings, request volume, and costs across providers, models, and adaptive routing tiers (`adaptive_tier` badges).
- **Paginated Trajectory & Coding-Agent Transcript View**: Server-side paginated trajectory viewer (`TaskTrajectoryViewer`) and `AgentLogTranscript` with configurable page sizes, collapsible reasoning/thinking blocks, adaptive routing tier badges, parsed TUI session state cards, and live status indicators.
- **PDF Trajectory & Conclusion Export**: Export complete execution trajectories and conclusion cards to PDF with STSong-Light UTF-16BE CJK font support for direct download, printing, or preview.
- **In-Browser Configuration & Network Diagnostics**: Inspect and edit OpenDuck settings, decoupled provider/role models, adaptive model routing (`auto`/`regular`/`economy` modes and off-peak windows), and proxies directly from the browser (`ServerSettings` & `NetworkStatusPanel`) with localized section navigation, active environment override indicators, and live connectivity diagnostics.
- **URL Navigation & Deep-Linking**: State synchronized with URL query parameters (`page`, `project`, `tab`, `split`) with full browser history support for task history, agent logs, and file previews.
- **Chat & Session Manager**: Stream live ACP responses with tool approvals, chat drawer for project tasks, and dynamic provider/model switching.
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
├── openduck-mcp             # MCP extensions (tui_session, agy, developer, computer controller, polymarket, memory)
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

For a detailed breakdown of all configuration files in `~/.config/openduck/` (including `config.yaml`, `secrets.yaml`, `notifications.yaml`, `permission.yaml`, global `proxy`, and custom providers), see the [Configuration Files Guide](documentation/docs/guides/config-files.md). For running and scheduling tasks, see the [Running Tasks Guide](documentation/docs/guides/running-tasks.md). For isolated Git worktree task execution, see the [Git Worktree Task Runner Guide](documentation/docs/guides/git-worktree-task-runner.md). For persistent agent CLI TUI sessions, quotas, and proxy configuration, see the [Interactive TUI Sessions Guide](documentation/docs/guides/tui-sessions.md) and [TUI Driver Extension Reference](documentation/docs/mcp/tui-agent-mcp.md). For server memory diagnostics and heap profiling, see the [Jemalloc Heap Profiles Guide](documentation/docs/guides/jemalloc-prof.md).

### Key Environment Variables

| Variable | Description |
|---|---|
| `OPENDUCK_PROVIDER__TYPE` | Active LLM provider (e.g. `anthropic`, `openai`, `google`, `ollama`) |
| `OPENDUCK_PROVIDER__MODEL` | Provider default model identifier (e.g. `claude-3-7-sonnet-latest`, `gpt-4o`) |
| `OPENDUCK_ACTIVE_MODEL` | Decoupled model override for primary agent role (precedence over provider default) |
| `OPENDUCK_SECONDARY_MODEL` | Decoupled model override for secondary/evaluator agent role |
| `OPENDUCK_PROXY` | Outbound HTTP/SOCKS proxy URL for provider and MCP network requests |
| `OPENDUCK_MAX_TURNS` | Maximum turn limit for harness and task executions (aliases: `GOOSE_MAX_TURNS`, `HARNESS_MAX_TURNS`) |
| `OPENDUCK_TASK_TIMEOUT` | Per-task execution timeout in seconds |
| `OPENDUCK_MEMORY_LIMIT_MB` | Memory threshold ceiling to activate pre-execution memory gating |
| `OPENDUCK_SERVER__SECRET_KEY` | Shared secret key for authenticating ACP & REST API requests |
| `OPENDUCK_STATE_MACHINE` | Set to `1` to enable the next-generation agent state machine |
| `OPENDUCK_DISABLE_KEYRING` | Set to `1` to store secrets in `~/.config/openduck/secrets.yaml` instead of OS keyring |
| `OPENDUCK_NOTIFICATION_CONFIG`| Custom file path to override `notifications.yaml` |
| `OPENDUCK_PATH_ROOT` | Absolute root directory override for all XDG configuration and data paths |
| `OPENDUCK_LOG_LEVEL` | Tracing log level (`info`, `debug`, `trace`) |
| `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` | Standard HTTP/SOCKS proxy URLs for routing provider and MCP network requests |

_Note: All `GOOSE_*` environment variables and configuration paths remain supported as backwards-compatible aliases._
