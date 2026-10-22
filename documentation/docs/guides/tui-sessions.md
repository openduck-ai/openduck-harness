---
title: Interactive Agent CLI TUI & Tmux Sessions
sidebar_label: TUI & Tmux Sessions
description: Guide to persistent, interactive coding agent CLIs (agy, grok, codex, claude) inside tmux sessions with live human attachment and automatic reclamation.
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Interactive Agent CLI TUI & Tmux Sessions

OpenDuck provides a native **Interactive Agent TUI & Tmux Driver** (`tui_session` module, available via the `tui_agent` MCP extension and the OpenDuck Control Plane REST API). 

Rather than executing subagents as one-off headless subprocesses (`agy -p`, `grok -p`, `codex exec`), OpenDuck manages persistent, interactive terminal sessions inside dedicated `tmux` panes. This enables stateful agent steering, live human observation and takeover, interactive slash command execution, and multi-turn performance with zero process restart overhead.

---

## 🌟 Why Persistent TUI Sessions?

| Capability | Headless CLI Subprocess (`-p`) | Persistent TUI Driver (`tui_session`) |
| :--- | :--- | :--- |
| **Startup Overhead** | Fresh process per turn (reloads tools, LSP, env) | **Zero cold-start** (process stays warm in memory) |
| **Human Co-Piloting** | No visual display or attachment possible | **Live attach** (`tmux -L openduck attach -t <id>`) |
| **Interactive Modals** | Skipped or fails without TTY | **Full interaction** via `tui_agent_keys` (e.g. `[y/n]`, arrow navigation) |
| **TUI Slash Commands** | Often disabled in non-interactive modes | Full access to `/plan`, `/compact`, `/model`, `/status`, etc. |
| **Universal Support** | Requires CLI to implement a headless JSON flag | Works with **any** TUI coding agent (`agy`, `grok`, `codex`, `claude`, `custom`) |

---

## 🏗️ Dual-Channel Hybrid Architecture

Modern terminal user interfaces render into an alternate screen buffer, animate Braille spinners, wrap lines to columns, and clip viewports. To provide completely reliable and lossless interaction, OpenDuck uses a **Dual-Channel Architecture**:

```
+---------------------------------------------------------------------------------+
|                                 OpenDuck System                                 |
|                                                                                 |
|  [ Callers: MCP Tools / Web Hub UI / Desktop / Harness Agent Policy / REST API ] |
+---------------------------------------+-----------------------------------------+
                                        |
                 +----------------------+----------------------+
                 |                                             |
                 v (Control & Visual Channel)                  v (Lossless Side-Channel)
     +-----------------------+                    +--------------------------------+
     | Tmux / PTY Backend    |                    | Disk Transcript Tailer         |
     | - tmux -L openduck    |                    | - agy: transcript.jsonl        |
     | - Bracketed paste     |                    | - grok: chat_history.jsonl     |
     | - Screen state parse  |                    | - codex: sessions/*.jsonl      |
     | - Keystroke injection |                    | - claude: session_id.jsonl     |
     +-----------+-----------+                    +---------------+----------------+
                 |                                                |
                 +----------------------+-------------------------+
                                        v
                    +---------------------------------------+
                    | Live Agent CLI Process (agy/grok/...)  |
                    +---------------------------------------+
```

1. **Control & Visual Channel (`tmux` + VT100 Engine)**:
   - Manages the process inside an isolated tmux socket (`tmux -L openduck`).
   - Injects prompts using **bracketed paste mode** (`\x1b[200~... \x1b[201~`) so multi-line instructions and code blocks do not trigger premature submission on newlines.
   - Monitors terminal frame stability via a **Quiescence Detector** and classifies the pane state (`Initializing`, `IdlePrompt`, `Working`, `WaitingApproval`, `ModalOpen`, `Exited`).
2. **Lossless Side-Channel Correlator**:
   - Simultaneously correlates turn output with local session transcripts on disk (e.g. `~/.gemini/antigravity-cli/brain/<id>/.system_generated/logs/transcript.jsonl` for `agy`, `~/.grok/sessions/.../chat_history.jsonl` for `grok`).
   - Guarantees **zero response truncation** even on multi-thousand-line agent outputs.

---

## 🚀 Supported Agent CLIs

OpenDuck provides built-in profile adapters for major coding agents:

- **Antigravity CLI (`agy`)**:
  - Detects bottom composer box and busy status (`⣷ Running...`, `esc to cancel`).
  - Tails the `PLANNER_RESPONSE` JSON records from Antigravity's local session logs.
- **Grok Build (`grok`)**:
  - Automatically configures `--no-alt-screen` and `--minimal` for high-throughput scrollback retention.
  - Correlates turns with Grok's `chat_history.jsonl` and `active_sessions.json`.
- **OpenAI Codex (`codex`)**:
  - Automatically disables alternate screen mode to preserve native terminal history.
  - Handles interactive `[y/n]` approval prompts.
- **Claude Code (`claude`)**:
  - Operates with clean accessibility flags for screen reader mode.
- **Custom (`custom`)**:
  - Execute arbitrary interactive CLI tools, scripts, or debuggers inside managed tmux panes.

---

## ⚙️ Configuration & Lifecycle Management

Configure persistent TUI agents in `~/.config/openduck/config.yaml` or in your project's `.openduck/config.yaml`:

```yaml
harness:
  tui_sessions:
    default:
      https_proxy: http://127.0.0.1:8080
      idle_timeout_mins: 45
      exited_retention_mins: 10
    agy:
      effort: medium
      idle_timeout_mins: 60
    grok:
      https_proxy: http://127.0.0.1:2087
      idle_timeout_mins: 30
    codex:
      idle_timeout_mins: 45
```

### Settings Reference

| Property | Description | Default |
| :--- | :--- | :--- |
| `effort` | Reasoning effort passed to the agent CLI, such as `low`, `medium`, or `high`. Supported values depend on the CLI. | CLI default |
| `https_proxy` | Outbound proxy URL set as `HTTPS_PROXY` and `https_proxy` in the subagent environment. | Inherited from system |
| `idle_timeout_mins` | Inactivity limit in minutes. If a session stays in `IdlePrompt` with no activity, it is automatically stopped and reclaimed. | `None` (unlimited) |
| `exited_retention_mins` | Grace period in minutes to keep an exited or terminated session in the registry and UI list before memory eviction. | `10` minutes |

An explicit session effort overrides the agent's configured effort, which overrides
`harness.tui_sessions.default.effort`. Settings are read for each new session;
existing sessions keep their launch configuration.

---

## 👥 Live Human Attachment & Co-Piloting

Because sessions run inside an isolated tmux server (`socket: openduck`), developers can view or co-pilot any active session directly from their terminal:

### Attach to a Session

```bash
# Attach to an active session by its ID
tmux -L openduck attach -t agy_1729148000
```

### Detach Safely

While attached, you are in a native tmux terminal. To disconnect and leave the session running in the background:
1. Press `Ctrl-b`
2. Press `d`

---

## 🔌 MCP Tools (`tui_agent`)

The `tui_agent` MCP server exposes six tools:

1. `tui_agent_start`:
   ```json
   {
     "agent": "agy",
     "cwd": "/path/to/project",
     "model": "gemini-3.8-flash-medium",
     "effort": "high",
     "initial_prompt": "Refactor auth middleware to use async-trait"
   }
   ```
2. `tui_agent_prompt`:
   ```json
   {
     "session_id": "agy_1729148000",
     "prompt": "/plan Run regression test suite and analyze failures",
     "wait": true
   }
   ```
3. `tui_agent_read`:
   ```json
   {
     "session_id": "agy_1729148000",
     "scrollback_lines": 80
   }
   ```
4. `tui_agent_keys`:
   ```json
   {
     "session_id": "agy_1729148000",
     "keys": ["Escape"]
   }
   ```
5. `tui_agent_list`:
   ```json
   {
     "include_screen_preview": true
   }
   ```
6. `tui_agent_stop`:
   ```json
   {
     "session_id": "agy_1729148000"
   }
   ```

For detailed tool schemas and arguments, see the [Agent TUI Driver Extension Reference](../mcp/tui-agent-mcp.md).

---

## 🌐 Control Plane REST API

When `openduck serve` is running, the OpenDuck Web Hub and external automations can control TUI sessions over HTTP:

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` | `/api/projects/:id/tui-sessions` | List active sessions for a project (supports `?preview=true`). |
| `POST` | `/api/projects/:id/tui-sessions` | Start a new TUI session in the project directory. |
| `GET` | `/api/projects/:id/tui-sessions/:session_id` | Read live screen snapshot and execution state. |
| `POST` | `/api/projects/:id/tui-sessions/:session_id/prompt` | Inject a prompt or slash command and optionally wait. |
| `POST` | `/api/projects/:id/tui-sessions/:session_id/keys` | Send raw keystrokes (e.g. `["y", "Enter"]`). |
| `DELETE` | `/api/projects/:id/tui-sessions/:session_id` | Stop and terminate a session. |

---

## 🛠️ Diagnostics & Troubleshooting

### Check Running Tmux Sessions
```bash
tmux -L openduck list-sessions
```

### Inspect Stored Transcripts
- **Antigravity CLI**: `~/.gemini/antigravity-cli/brain/<id>/.system_generated/logs/transcript.jsonl`
- **Grok Build**: `~/.grok/sessions/`
- **OpenAI Codex**: `~/.codex/sessions/`

### Session Lifecycle Status
- `Initializing`: Spawning the process and waiting for the terminal composer to settle.
- `IdlePrompt`: Ready to accept the next prompt or slash command.
- `Working`: Actively running model inference, executing tools, or editing files.
- `WaitingApproval` / `ModalOpen`: Blocked on interactive confirmation or input modal. Keystrokes can be sent with `tui_agent_keys`.
- `Exited`: The underlying process terminated. Kept in memory for `exited_retention_mins` before cleanup.
