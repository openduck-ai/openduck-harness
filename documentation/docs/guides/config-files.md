---
sidebar_position: 10
title: Configuration Files
sidebar_label: Configuration Files
---

# Configuration Files & Directory Structure

OpenDuck uses standard XDG directories and YAML/JSON configuration files to manage settings, provider credentials, permissions, and extensions:

* **Linux**: `~/.config/openduck/` (legacy fallback: `~/.config/goose/`)
* **macOS**: `~/Library/Application Support/OpenDuck/` (legacy fallback: `~/Library/Application Support/Block/goose/`)
* **Windows**: `%APPDATA%\OpenDuck\config\` (legacy fallback: `%APPDATA%\Block\goose\`)

Existing Goose config directories are migrated automatically. Override the configuration and data root entirely at runtime with the `OPENDUCK_PATH_ROOT` (or `GOOSE_PATH_ROOT`) environment variable.

---

## 📁 Directory Structure Overview (`~/.config/openduck/`)

```text
~/.config/openduck/
├── config.yaml          # Main configuration (providers, models, MCP extensions, harness, judge)
├── secrets.yaml         # Provider API keys and secret tokens (when system keyring is disabled)
├── notifications.yaml   # Email alert engine & SMTP server rules
├── permission.yaml      # Granular tool permission access policies (always_allow, ask_before, never_allow)
├── project-roots.json   # Security allowlist of root filesystem directories for project workspaces
├── serve.env            # Environment variables loaded by openduck serve daemon and startup runners
├── custom_providers/    # Declarative custom provider definition JSONs (Unsloth, vLLM, Ollama, etc.)
│   └── unsloth.json
├── nginx/               # Reverse proxy configuration for local Web Hub deployments
│   └── nginx.conf
├── tls/                 # TLS/SSL certificates and private keys for secure serving
│   ├── server.key
│   └── server.pem
├── gemini_oauth/        # Cached Google Gemini OAuth session tokens
├── chatgpt_codex/       # Cached OpenAI / ChatGPT Codex OAuth session credentials
├── xai_oauth/           # Cached xAI OAuth tokens
├── databricks/          # Cached Databricks OAuth tokens
└── mcp-apps-cache/      # Cached manifests and tool schemas for discovered MCP applications
```

---

## 1. `config.yaml` — Primary Configuration

`config.yaml` is the central configuration file loaded on startup by the OpenDuck CLI, server daemon, and agent runtime.

### Configuration Sections

- **Default Provider & Models**: Defines active LLM provider, default model, and fast routing model.
- **Provider Host Overrides**: Custom host endpoints and base paths for standard providers (e.g. OpenAI proxies, Anthropic proxies, local Ollama).
- **Execution Mode**: `auto` (autonomous tool use), `chat` (conversational only, no tools), or `approve` (prompt before running tools).
- **Extensions / MCP Servers**: Declares platform, builtin, and external stdio/SSE Model Context Protocol (MCP) servers.
- **Advisors & Proxy Routing**: Per-advisor HTTP/SOCKS5 proxy settings for routing outbound requests (e.g. for `agy`, `grok`, or `openduck`).
- **Harness Settings**: Circuit breaker thresholds, stagnation limits, review intervals, and turn bounds.
- **Judge Configuration**: Evaluator service endpoints, timeouts, and scoring drift/completion detection.

### Example: Comprehensive `config.yaml`

```yaml
# ~/.config/openduck/config.yaml

# ------------------------------------------------------------------
# 1. Provider & Model Configuration
# ------------------------------------------------------------------
OPENDUCK_PROVIDER: openai
OPENDUCK_MODEL: "gpt-4o"
OPENDUCK_FAST_MODEL: "gpt-4o-mini"
GOOSE_MODE: auto
GOOSE_TELEMETRY_ENABLED: false

# Endpoint & Host Overrides (Optional reverse proxies / local gateways)
OPENAI_HOST: "https://api.openai.com"
OPENAI_BASE_PATH: "v1/chat/completions"
ANTHROPIC_HOST: "https://api.anthropic.com"
OLLAMA_HOST: "http://127.0.0.1:11434"

# ------------------------------------------------------------------
# 2. External Notifications Reference
# ------------------------------------------------------------------
notifications: notifications.yaml

# ------------------------------------------------------------------
# 3. Model Context Protocol (MCP) Extensions
# ------------------------------------------------------------------
extensions:
  developer:
    enabled: true
    type: platform
    name: developer
    description: "Read, write, edit files and execute terminal shell commands"
    bundled: true

  skills:
    enabled: true
    type: platform
    name: skills
    description: "Discover and execute dynamic prompt workflows and skills"
    bundled: true

  todo:
    enabled: true
    type: platform
    name: todo
    description: "Track and organize multi-step agent plans"
    bundled: true

  computercontroller:
    enabled: true
    type: builtin
    name: computercontroller
    description: "Browser automation, web scraping, and file caching"
    timeout: 60
    bundled: true

  custom_everything:
    enabled: false
    type: stdio
    cmd: "npx"
    args: ["-y", "@modelcontextprotocol/server-everything"]
    env:
      DEBUG: "mcp:*"
    timeout: 30

# ------------------------------------------------------------------
# 4. Outbound Network & Advisor Proxies
# ------------------------------------------------------------------
advisors:
  agy:
    https_proxy: "socks5://127.0.0.1:1088"
    all_proxy: "socks5://127.0.0.1:1088"
    no_proxy: "localhost,127.0.0.1"

  grok:
    https_proxy: "http://127.0.0.1:2087"
    http_proxy: "http://127.0.0.1:2087"
    no_proxy: "localhost,127.0.0.1"

  default:
    http_proxy: "http://127.0.0.1:2087"
    https_proxy: "http://127.0.0.1:2087"

# ------------------------------------------------------------------
# 5. Agent Harness & Runtime Guardrails
# ------------------------------------------------------------------
harness:
  circuit_breaker_enabled: true
  stagnation_threshold: 15          # Tier 3 termination triggered if no write progress
  periodic_review_interval: 10      # Review cadence for progress checks
  max_turns: 40                     # Turn cap per task invocation

# ------------------------------------------------------------------
# 6. Evaluation Judge Service
# ------------------------------------------------------------------
judge:
  provider: laya
  endpoint: "http://localhost:8732/api/laya"
  timeout_ms: 350
  fallback_on_error: true
  points:
    turn.drift: active              # off | shadow | active
    turn.completion: active
    context.forget: active
    tool.risk: active
```

---

## 2. `secrets.yaml` — Secure Credential Storage

`secrets.yaml` stores sensitive API tokens and credentials when system keyring is not in use (e.g. headless Linux servers, Docker containers, or when `OPENDUCK_DISABLE_KEYRING=1` is specified).

- **Permissions**: Ensure file permissions are restricted to owner-only (`chmod 600 ~/.config/openduck/secrets.yaml`).
- **Resolution**: OpenDuck matches uppercase keys directly to provider API requirements.

:::warning Never commit secrets.yaml
Do not place API keys in `config.yaml`. Store them either in your OS keyring (via `openduck configure`), in `secrets.yaml`, or as environment variables (e.g., `OPENAI_API_KEY`).
:::

### Example: `secrets.yaml`

```yaml
# ~/.config/openduck/secrets.yaml
# Ensure file permissions: chmod 600 ~/.config/openduck/secrets.yaml

OPENAI_API_KEY: "sk-proj-abc123..."
ANTHROPIC_API_KEY: "sk-ant-api03-xyz789..."
GEMINI_API_KEY: "AIzaSy..."
OPENCODE_API_KEY: "sk-..."
GITHUB_PERSONAL_ACCESS_TOKEN: "ghp_..."
```

---

## 3. `notifications.yaml` — SMTP & Email Alert Engine

OpenDuck contains an automated notification engine used for alerting developers on cron schedule executions, harness benchmark batch completions, and task failures.

### Supported Fields

- **`email.enabled`**: Set to `true` to activate email dispatch.
- **`email.smtp`**: Connection settings (`host`, `port`, `use_tls`, `username`, `password`, `from`).
- **`email.default_recipients`**: List of default email addresses to notify.
- **`email.rules`**: Trigger criteria:
  - `trigger_on`: `always`, `on_success`, `on_failure`, or `on_status_change`.
  - `min_duration_seconds`: Only fire if the task duration met or exceeded this threshold (filters trivial runs).
  - `recipients`: Targeted recipient list for the rule.
  - `projects`: Limit the rule to specific project root identifiers.
- **`email.projects`**: Per-project recipient list overrides.

### Example: `notifications.yaml`

```yaml
# ~/.config/openduck/notifications.yaml
email:
  enabled: true
  smtp:
    host: "smtp.feishu.cn"
    port: 465
    use_tls: true
    username: "alerts@example.com"
    password: "your-smtp-app-password"
    from: "OpenDuck Harness <alerts@example.com>"

  # Fallback recipients for general alerts
  default_recipients:
    - "lead-developer@example.com"

  # Event-based dispatch rules
  rules:
    # Rule 1: Always send immediate alert on failure
    - trigger_on: "on_failure"
      recipients:
        - "oncall@example.com"
        - "lead-developer@example.com"

    # Rule 2: Send success report only for long-running batch benchmark tasks
    - trigger_on: "on_success"
      min_duration_seconds: 120
      recipients:
        - "benchmarks@example.com"

  # Per-project overrides
  projects:
    "openduck-harness":
      recipients:
        - "core-maintainers@example.com"
```

---

## 4. `permission.yaml` — Tool Execution Security Policy

OpenDuck enforces a three-tier permission matrix on tool invocations to safeguard host systems:

1. **`always_allow`**: Tools executed automatically without interactive approval.
2. **`ask_before`**: Tools requiring explicit user confirmation in the CLI prompt or Web Hub modal dialog.
3. **`never_allow`**: Tools completely blocked from execution.

Tool names can be specified as base names (`shell`, `edit`, `write`, `read`) or namespaced MCP tools (`<extension>__<tool>`, e.g. `grok__grok_run`, `todo__todo_write`).

### Example: `permission.yaml`

```yaml
# ~/.config/openduck/permission.yaml
user:
  always_allow:
    - read
    - write
    - edit
    - shell
    - load
    - analyze
    - tree
    - todo__todo_write
    - grok__grok_run
    - grok__grok_status
    - grok__grok_sessions

  ask_before:
    - bash_destructive
    - git_push_force
    - deploy_production

  never_allow:
    - delete_root_filesystem
    - format_disk
```

---

## 5. `project-roots.json` — Workspace Path Allowlist

To prevent directory traversal attacks when using OpenDuck Web Hub and the Control Plane REST/ACP API, only filesystem locations registered in `project-roots.json` may be opened as project workspaces.

### Example: `project-roots.json`

```json
{
  "roots": [
    "/mnt/e/openduck-harness",
    "/home/user/workspace",
    "/var/repos"
  ]
}
```

New workspace roots can also be registered interactively from the Web Hub dashboard or via CLI:
```bash
openduck project add /path/to/my-repo
```

---

## 6. `serve.env` — Server Daemon Environment

The `serve.env` file contains runtime environment variables loaded when starting the background ACP and REST API server (`openduck serve` or `./start_openduck.sh`).

### Example: `serve.env`

```bash
# ~/.config/openduck/serve.env

# Server Shared Secret (Used to authenticate ACP & REST requests)
OPENDUCK_SERVER__SECRET_KEY="my-production-secret-token"

# Circuit Breaker (0 = disabled, 1 = enabled)
OPENDUCK_CIRCUIT_BREAKER=0

# Default Provider & Model for Daemon Sessions
OPENDUCK_PROVIDER=openai
OPENDUCK_MODEL=gpt-4o

# Process PATH override (Ensures developer toolchains are accessible)
PATH=/home/user/.cargo/bin:/home/user/.local/bin:/usr/local/bin:/usr/bin:/bin
```

---

## 7. `custom_providers/*.json` — Declarative Custom Providers

Declarative custom providers allow connecting OpenDuck to any OpenAI-compatible local or remote inference engine without modifying source code. OpenDuck automatically scans `~/.config/openduck/custom_providers/` for all `*.json` files on startup.

### Supported Fields

- **`name`**: Unique internal provider identifier.
- **`engine`**: Protocol adapter (e.g. `openai`).
- **`display_name`**: UI label shown in Web Hub and CLI menus.
- **`description`**: Human-readable description.
- **`base_url`**: Complete API endpoint URL (e.g. `http://localhost:8000/v1/chat/completions`).
- **`api_key_env`**: Environment variable name containing the API key.
- **`models`**: List of model objects with `name`.
- **`supports_streaming`**: Boolean indicating Server-Sent Events (SSE) support.
- **`env_vars`**: Array of environment variables with requirements, secrets, and defaults.

### Example: `custom_providers/unsloth.json`

```json
{
  "name": "unsloth",
  "engine": "openai",
  "display_name": "Unsloth Local (Qwen 3.8)",
  "description": "Local Unsloth high-throughput inference engine",
  "base_url": "http://192.168.1.83:8888/v1/chat/completions",
  "api_key_env": "UNSLOTH_API_KEY",
  "supports_streaming": true,
  "requires_auth": true,
  "models": [
    {
      "name": "Qwen3.8-27B-Q3_K_M"
    }
  ],
  "env_vars": [
    {
      "name": "UNSLOTH_API_KEY",
      "required": false,
      "secret": true,
      "default": "sk-local-unsloth-token"
    }
  ]
}
```

### Example: `custom_providers/vllm.json`

```json
{
  "name": "vllm_local",
  "engine": "openai",
  "display_name": "vLLM Server",
  "description": "Self-hosted vLLM inference server",
  "base_url": "http://127.0.0.1:8000/v1/chat/completions",
  "api_key_env": "VLLM_API_KEY",
  "supports_streaming": true,
  "requires_auth": false,
  "models": [
    {
      "name": "deepseek-ai/DeepSeek-Coder-V2-Lite-Instruct"
    }
  ]
}
```

---

## 8. Reverse Proxy & Infrastructure (`nginx/` & `tls/`)

### `nginx/nginx.conf`
For production or server deployments, an Nginx reverse proxy configuration can be placed in `~/.config/openduck/nginx/nginx.conf` to serve the static Web Hub frontend while reverse proxying API and ACP WebSocket connections to `openduck serve`:

```nginx
# ~/.config/openduck/nginx/nginx.conf
worker_processes 1;

events {
    worker_connections 1024;
}

http {
    include mime.types;
    default_type application/octet-stream;
    sendfile on;

    server {
        listen 0.0.0.0:5173;
        server_name _;

        # Static build of Web Hub
        root /home/user/.local/share/openduck/www;
        index index.html;

        # REST API Proxy
        location /hub/api/ {
            proxy_pass http://127.0.0.1:3000/api/;
            proxy_http_version 1.1;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
        }

        # ACP Streaming Endpoint
        location /hub/acp {
            proxy_pass http://127.0.0.1:3000/acp;
            proxy_http_version 1.1;
            proxy_set_header Host $host;
            proxy_buffering off;
            proxy_read_timeout 86400s;
        }
    }
}
```

### `tls/` (TLS/SSL Certificates)
Stores TLS certificate (`server.pem` / `server.crt`) and private key (`server.key`) for serving HTTPS/WSS encrypted traffic:

```bash
# Generate self-signed certificates for local HTTPS serving:
mkdir -p ~/.config/openduck/tls
openssl req -x509 -newkey rsa:4096 -keyout ~/.config/openduck/tls/server.key \
  -out ~/.config/openduck/tls/server.pem -days 365 -nodes -subj "/CN=localhost"
```

---

## 9. OAuth & Internal Cache Directories

- **`gemini_oauth/`**: Stores cached OAuth 2.0 refresh and access tokens for interactive Google Gemini CLI login.
- **`chatgpt_codex/`**: Stores session tokens and device authorization codes for ChatGPT / OpenAI Codex.
- **`xai_oauth/` & `databricks/oauth`**: Manages OAuth state and tokens for xAI and Databricks endpoints.
- **`mcp-apps-cache/`**: Stores cached JSON manifests and tool schemas fetched from remote MCP applications to speed up session initialization.

---

## 10. Configuration Precedence

OpenDuck resolves configuration values in the following order of priority (highest to lowest):

1. **Explicit CLI Flags** (e.g. `--provider openai --model gpt-4o`)
2. **Environment Variables** (`OPENDUCK_*`, then legacy `GOOSE_*`, then raw provider keys like `OPENAI_API_KEY`)
3. **Workspace Configuration** (`<workspace_root>/.openduck/config.yaml` or `<workspace_root>/.goose/config.yaml`)
4. **User Configuration** (`~/.config/openduck/config.yaml`, with legacy fallback `~/.config/goose/config.yaml`)
5. **System Defaults** (`/etc/goose/config.yaml` or built-in defaults)
