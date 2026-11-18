---
sidebar_position: 2
title: AI Agent Setup & Runbook
sidebar_label: AI Agent Setup Guide
---

# OpenDuck AI Agent Installation & Configuration Runbook

> **Audience**: Autonomous coding agents, CI runners, and headless automation.  
> **Design principles**: Every step is non-interactive, deterministic, and includes an explicit verification command. Avoid interactive TTY wizards (`openduck configure`) unless a human is present.

---

## Execution Contract (Read First)

- **Workspace root**: All `cargo` and `bin/activate-hermit` commands assume your working directory is `openduck-harness/`.
- **Toolchain activation**: Run `source bin/activate-hermit` in every new shell session before invoking `cargo`, `node`, or `pnpm`.
- **Headless secret storage**: Always set `export OPENDUCK_DISABLE_KEYRING=1` in non-interactive shells so OpenDuck does not block waiting on an OS keyring prompt.
- **Port coupling**: `ui/hub` dev server (`vite.config.ts`) proxies `/api`, `/acp`, `/hub/api`, and `/hub/acp` to `http://127.0.0.1:3000`. Start `openduck serve` on `--port 3000` when running `ui/hub`.

---

## Step 1: Clone the Repository

```bash
git clone https://github.com/openduck-ai/openduck-harness.git
cd openduck-harness
```

**Verify**:
```bash
test -f Cargo.toml && test -f bin/activate-hermit && echo "OK: repo ready"
```

---

## Step 2: Activate Toolchain & Compile

Activate the pinned Hermit toolchain (Rust, Node.js, pnpm), then build the Rust workspace:

```bash
source bin/activate-hermit
cargo build --release
```

> For faster iteration during development, use `cargo build` (binary at `target/debug/openduck`).

Expose the compiled binary on `PATH` for the current shell:

```bash
export PATH="$PWD/target/release:$PATH"
```

Install frontend workspace dependencies (needed for `ui/hub`):

```bash
cd ui
pnpm install
cd ..
```

**Verify**:
```bash
openduck --version
pnpm --version
```

---

## Step 3: Configure LLM Provider (Non-Interactive)

Do **not** run `openduck configure` (it is an interactive TUI wizard). Use **Method A** (environment variables, recommended for ephemeral/CI runs) or **Method B** (declarative YAML files, recommended for persistent environments).

Always disable the OS keyring in headless environments:

```bash
export OPENDUCK_DISABLE_KEYRING=1
```

### Method A: Environment Variables (Fastest)

Pick **one** provider block and export it in your shell:

```bash
# Option 1: Anthropic
export OPENDUCK_PROVIDER__TYPE="anthropic"
export OPENDUCK_PROVIDER__MODEL="claude-3-7-sonnet-latest"
export ANTHROPIC_API_KEY="sk-ant-..."

# Option 2: OpenAI
export OPENDUCK_PROVIDER__TYPE="openai"
export OPENDUCK_PROVIDER__MODEL="gpt-4o"
export OPENAI_API_KEY="sk-..."

# Option 3: Google Gemini
export OPENDUCK_PROVIDER__TYPE="google"
export OPENDUCK_PROVIDER__MODEL="gemini-2.5-pro"
export GEMINI_API_KEY="AIza..."

# Option 4: Local Ollama (no API key required)
export OPENDUCK_PROVIDER__TYPE="ollama"
export OPENDUCK_PROVIDER__HOST="http://127.0.0.1:11434"
export OPENDUCK_PROVIDER__MODEL="qwen2.5-coder:latest"
```

Optional outbound proxy (if required by network environment):

```bash
export OPENDUCK_PROXY="http://127.0.0.1:3128"
```

### Method B: Declarative Config Files (`~/.config/openduck/`)

Create `~/.config/openduck/config.yaml` and `~/.config/openduck/secrets.yaml`:

```bash
mkdir -p ~/.config/openduck

cat <<'EOF' > ~/.config/openduck/config.yaml
active_provider: anthropic
providers:
  anthropic:
    enabled: true
    configured: true
    model: "claude-3-7-sonnet-latest"
GOOSE_MODE: auto
EOF

cat <<'EOF' > ~/.config/openduck/secrets.yaml
ANTHROPIC_API_KEY: "sk-ant-..."
EOF
chmod 600 ~/.config/openduck/secrets.yaml
```

#### Custom Providers (`~/.config/openduck/custom_providers/`)

To connect to self-hosted or third-party endpoints (e.g., vLLM, LiteLLM, Unsloth, or private gateways) without modifying code, place `*.json` definitions in `~/.config/openduck/custom_providers/`. Supported `engine` values are `openai`, `anthropic`, and `ollama` (plus `*_compatible` aliases).

```bash
mkdir -p ~/.config/openduck/custom_providers

cat <<'EOF' > ~/.config/openduck/custom_providers/vllm_local.json
{
  "name": "vllm_local",
  "engine": "openai",
  "display_name": "vLLM Local",
  "description": "Self-hosted OpenAI-compatible inference server",
  "base_url": "http://127.0.0.1:8000/v1/chat/completions",
  "api_key_env": "VLLM_API_KEY",
  "requires_auth": true,
  "supports_streaming": true,
  "models": [
    {
      "name": "Qwen/Qwen2.5-Coder-32B-Instruct"
    }
  ]
}
EOF
```

Then activate the custom provider in `~/.config/openduck/config.yaml` (using its `name` as the provider key) and store its key in `~/.config/openduck/secrets.yaml` (if `requires_auth` is `true`):

```bash
cat <<'EOF' > ~/.config/openduck/config.yaml
active_provider: vllm_local
providers:
  vllm_local:
    enabled: true
    configured: true
    model: "Qwen/Qwen2.5-Coder-32B-Instruct"
GOOSE_MODE: auto
EOF

cat <<'EOF' > ~/.config/openduck/secrets.yaml
VLLM_API_KEY: "sk-local-..."
EOF
chmod 600 ~/.config/openduck/secrets.yaml
```

See [config-files.md](./config-files.md#7-custom_providersjson--declarative-custom-providers) for the full list of supported fields (`headers`, `timeout_seconds`, `env_vars`, `dynamic_models`, `proxy`, etc.).

**Verify**:
```bash
openduck doctor
openduck info --check
```

---

## Step 4: Start & Verify the Agent Harness

### 4.1 Smoke Test (Zero API Cost)

Run the built-in `EchoPolicy` to verify the harness runtime without calling an LLM:

```bash
openduck harness run --echo --prompt "smoke test"
```

### 4.2 Run Harness CLI Tasks

```bash
# Single task execution
openduck harness run --prompt "Inspect repository structure and summarize"

# Isolated Git worktree execution with auto-commit
openduck harness run \
  --prompt "Fix failing unit test" \
  --worktree \
  --worktree-branch "agent/fix-test" \
  --auto-commit

# Record execution to a replay cassette, then replay offline
openduck harness run --prompt "Summarize README.md" --record cassettes/smoke.json
openduck harness replay --cassette cassettes/smoke.json

# Batch benchmark evaluation
openduck harness eval \
  --dataset benchmarks/swe_tasks.jsonl \
  --concurrency 4 \
  --output-dir harness_eval_results/
```

### 4.3 Start the Harness Control Plane (`openduck serve`)

The control plane serves ACP + REST APIs required by `ui/hub`, task queues, and scheduled jobs. Start it on port `3000` (matching `ui/hub/vite.config.ts`):

```bash
export OPENDUCK_DISABLE_KEYRING=1
export OPENDUCK_SERVER__SECRET_KEY="replace-with-strong-secret"

openduck serve \
  --platform desktop \
  --host 0.0.0.0 \
  --port 3000 \
  --enable-scheduler
```

> For automated background execution, redirect logs to a file and run in the background:  
> `nohup openduck serve --platform desktop --host 0.0.0.0 --port 3000 --enable-scheduler > /tmp/openduck-serve.log 2>&1 &`

**Verify**:
```bash
curl -fsS http://127.0.0.1:3000/health
```

---

## Step 5: Start `ui/hub` (Web Hub Dashboard)

Ensure `openduck serve` is already listening on port `3000` (see Step 4.3).

### Development Server (Hot Reload)

```bash
source bin/activate-hermit
cd ui/hub
pnpm install
pnpm run dev
```

> For background execution:  
> `nohup pnpm run dev > /tmp/openduck-hub.log 2>&1 &`

**Verify**:
```bash
curl -fsS -o /dev/null -w "%{http_code}\n" http://127.0.0.1:5173/hub/
# Expected: 200
```

Access URL: `http://localhost:5173/hub/`  
Authenticate in the UI using the `OPENDUCK_SERVER__SECRET_KEY` configured in Step 4.3.

### Production Build (Optional)

```bash
source bin/activate-hermit
cd ui/hub
pnpm run build
```

Static assets are emitted to `ui/hub/dist/`.

---

## Fast-Path Checklist (Copy-Paste Sequence)

```bash
# 1. Clone & activate
git clone https://github.com/openduck-ai/openduck-harness.git
cd openduck-harness
source bin/activate-hermit

# 2. Build binary & install UI deps
cargo build --release
export PATH="$PWD/target/release:$PATH"
(cd ui && pnpm install)

# 3. Configure provider (headless)
export OPENDUCK_DISABLE_KEYRING=1
export OPENDUCK_PROVIDER__TYPE="anthropic"
export OPENDUCK_PROVIDER__MODEL="claude-3-7-sonnet-latest"
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENDUCK_SERVER__SECRET_KEY="replace-with-strong-secret"

# 4. Verify & start backend + harness smoke test
openduck doctor
openduck harness run --echo --prompt "smoke test"
nohup openduck serve --platform desktop --host 0.0.0.0 --port 3000 --enable-scheduler > /tmp/openduck-serve.log 2>&1 &
sleep 2 && curl -fsS http://127.0.0.1:3000/health

# 5. Start ui/hub
cd ui/hub
nohup pnpm run dev > /tmp/openduck-hub.log 2>&1 &
sleep 2 && curl -fsS -o /dev/null -w "%{http_code}\n" http://127.0.0.1:5173/hub/
```

---

## Troubleshooting for Agents

| Symptom | Cause | Fix |
|---|---|---|
| `cargo` or `pnpm` not found | Hermit toolchain not activated in current shell | Run `source bin/activate-hermit` from repo root |
| CLI hangs on provider/secret access | Waiting for OS keyring in headless environment | Export `OPENDUCK_DISABLE_KEYRING=1` |
| `openduck serve` fails to start | Missing `OPENDUCK_SERVER__SECRET_KEY` | Export `OPENDUCK_SERVER__SECRET_KEY` (or pass `--dangerously-unauthenticated` for local-only testing) |
| `ui/hub` API/ACP requests return `502` / `ECONNREFUSED` | `openduck serve` not running on port `3000` | Restart `openduck serve` with `--port 3000` (default CLI port is `3284`) |
| `pnpm run dev` fails on workspace packages | Dependencies not installed from `ui/` workspace root | Run `cd ui && pnpm install` |
