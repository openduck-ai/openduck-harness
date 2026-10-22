---
title: Interactive Agent TUI & Tmux Driver Extension
description: Run persistent, interactive coding agent CLIs (agy, grok, codex, claude) inside tmux sessions from OpenDuck
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import GooseBuiltinInstaller from '@site/src/components/GooseBuiltinInstaller';

The **Interactive Agent TUI & Tmux Driver** extension (`tui_agent` / `tui_session`) allows OpenDuck to spawn and steer interactive coding agent CLIs—including [Antigravity (`agy`)](https://antigravity.google/docs/cli/overview), [Grok Build (`grok`)](https://x.ai/build), [OpenAI Codex (`codex`)](https://github.com/openai/codex), and [Claude Code (`claude`)](https://docs.anthropic.com/en/docs/agents-and-tools/claude-code/overview)—inside persistent `tmux` terminal sessions.

Unlike one-shot headless subprocess executions, driving agents inside persistent TUI sessions provides:
- **Zero Cold-Start Overhead Across Turns**: CLI processes remain warm in memory, preserving initialized MCP servers, LSP indices, authentication tokens, and cached project graph context.
- **Human-in-the-Loop Attachability**: Developers can attach directly to the live terminal session using `tmux -L openduck attach -t <session_id>` or through the OpenDuck Web Hub TUI viewer to observe actions or provide manual inputs.
- **Interactive Slash Commands & Modals**: Programmatically execute TUI-only slash commands (such as `/plan`, `/compact`, `/model`, `/status`) and answer interactive question modals (`ask_question` / approval dialogs) via simulated keystrokes.
- **Dual-Channel Lossless Output Extraction**: Combines active VT100 terminal screen captures with side-channel disk transcript tails to prevent response truncation regardless of scrollback size.

---

## Prerequisites

1. **Terminal Multiplexer**: `tmux` (version 3.2 or later recommended) must be installed and accessible on your `PATH`:
   ```bash
   tmux -V
   ```
2. **Agent CLIs**: Install and authenticate one or more supported agent CLIs:
   - **Antigravity CLI**: `agy`
   - **Grok Build**: `grok`
   - **OpenAI Codex**: `codex`
   - **Claude Code**: `claude`

---

## Configuration

<Tabs groupId="interface">
  <TabItem value="ui" label="OpenDuck Desktop / Web Hub" default>
  <GooseBuiltinInstaller
    extensionName="Agent TUI Driver"
    description="Run persistent, interactive agent CLIs in tmux sessions"
    extensionId="tui_agent"
  />
  </TabItem>
  <TabItem value="cli" label="OpenDuck CLI">

  1. Enable `tui_agent` in your interactive configuration:
  ```sh
  openduck configure
  ```
  Choose `Toggle Extensions` and enable `tui_agent`.

  2. Or start an interactive session with the extension enabled for this run only:
  ```sh
  openduck session --with-builtin tui_agent
  ```

  </TabItem>
</Tabs>

### Declarative Configuration (`config.yaml` / `harness.yaml`)

You can declare global or per-agent defaults (such as proxies, idle timeout, and retention policies) under `harness.tui_sessions` in `~/.config/openduck/config.yaml` or project `.openduck/config.yaml`:

```yaml
extensions:
  tui_agent:
    enabled: true
    type: builtin
    name: tui_agent
    timeout: 600

harness:
  tui_sessions:
    default:
      https_proxy: http://127.0.0.1:8080
      idle_timeout_mins: 45
      exited_retention_mins: 10
    agy:
      idle_timeout_mins: 60
    grok:
      https_proxy: http://127.0.0.1:2087
      idle_timeout_mins: 30
```

---

## Tool Reference

| Tool | Description |
| :--- | :--- |
| `tui_agent_start` | Spawns an interactive coding agent CLI (`agy`, `grok`, `codex`, `claude`, `custom`) inside a persistent tmux session. |
| `tui_agent_prompt` | Injects a prompt into the TUI input composer using bracketed paste and waits for turn completion. |
| `tui_agent_read` | Captures the current visible terminal screen snapshot and optional scrollback buffer. |
| `tui_agent_keys` | Sends raw keystrokes (e.g. `["Escape"]`, `["y", "Enter"]`, `["C-c"]`) to dismiss modals or navigate menus. |
| `tui_agent_list` | Lists all active and retained TUI sessions, their execution state, attach commands, and previews. |
| `tui_agent_stop` | Gracefully terminates a running TUI session and cleans up tmux panes. |

---

### 1. `tui_agent_start`

Spawns a new persistent tmux session running the designated agent CLI.

```json
{
  "agent": "agy",
  "cwd": "/path/to/project",
  "model": "gemini-3.8-flash-medium",
  "effort": "high",
  "auto_approve": true,
  "initial_prompt": "Audit src/lib.rs for concurrency issues"
}
```

#### Parameters

| Parameter | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `agent` | `string` | **Yes** | Target agent CLI: `agy`, `grok`, `codex`, `claude`, or `custom`. |
| `cwd` | `string` | No | Working directory for the session. Defaults to current workspace root. |
| `model` | `string` | No | Model slug override (e.g. `gemini-3.8-flash-high`, `grok-4.6`). |
| `effort` | `string` | No | Reasoning effort level: `low`, `medium`, or `high`. Overrides `harness.tui_sessions.<agent>.effort` and `harness.tui_sessions.default.effort` in `config.yaml`. |
| `auto_approve` | `boolean` | No | Auto-approve permissions and interactive modals (default: `true`). |
| `resume_id` | `string` | No | Existing conversation or session ID to resume. |
| `initial_prompt`| `string` | No | Prompt to inject immediately once the TUI settles into idle prompt. |
| `session_id` | `string` | No | Custom session ID (autogenerated as `<agent>_<timestamp>` if omitted). |
| `socket_name` | `string` | No | Custom tmux socket name (defaults to `openduck`). |
| `cols` | `integer` | No | Terminal columns (default: `160`). |
| `rows` | `integer` | No | Terminal rows (default: `48`). |
| `extra_args` | `string[]` | No | Additional command-line flags passed directly to the spawned CLI. |
| `https_proxy` | `string` | No | HTTPS proxy URL (e.g. `http://127.0.0.1:2087`) set in the CLI environment. |
| `idle_timeout_mins` | `integer` | No | Inactivity timeout before session is reclaimed. |
| `exited_retention_mins` | `integer` | No | Retention period to keep exited sessions before pruning (default: `10`). |

---

### 2. `tui_agent_prompt`

Submits a prompt or slash command to the TUI session using bracketed paste mode.

```json
{
  "session_id": "agy_1729148000",
  "prompt": "Explain the architectural pattern used in crates/openduck/src/control/terminal.rs",
  "wait": true,
  "timeout_secs": 600
}
```

#### Parameters

| Parameter | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `session_id` | `string` | **Yes** | Active session ID returned by `tui_agent_start`. |
| `prompt` | `string` | **Yes** | Text or slash command (e.g. `/plan`, `/compact`) to submit. |
| `wait` | `boolean` | No | Wait for turn completion before returning (default: `true`). |
| `timeout_secs` | `integer` | No | Timeout in seconds if `wait` is true (default: `600`). |

---

### 3. `tui_agent_read`

Reads the current state and visual screen buffer of a session.

```json
{
  "session_id": "agy_1729148000",
  "scrollback_lines": 120,
  "include_ansi": false
}
```

#### Parameters

| Parameter | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `session_id` | `string` | **Yes** | Active session ID. |
| `scrollback_lines` | `integer` | No | Number of scrollback lines to capture (default: `120`). |
| `include_ansi` | `boolean` | No | Include raw ANSI terminal color codes (default: `false`). |

---

### 4. `tui_agent_keys`

Sends raw keystroke sequences to interact with modal dialogs or control the TUI.

```json
{
  "session_id": "agy_1729148000",
  "keys": ["Down", "Enter"]
}
```

#### Parameters

| Parameter | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `session_id` | `string` | **Yes** | Active session ID. |
| `keys` | `string[]` | **Yes** | Array of keystrokes (e.g. `["Escape"]`, `["y", "Enter"]`, `["C-c"]`). |

---

### 5. `tui_agent_list`

Lists all tracked TUI sessions with their status, PID, and attach commands.

```json
{
  "include_screen_preview": true
}
```

---

### 6. `tui_agent_stop`

Stops the session and kills the corresponding tmux window/pane.

```json
{
  "session_id": "agy_1729148000"
}
```

---

## Live Human Attachment

Because sessions run inside an isolated tmux server socket (`-L openduck`), any developer or co-pilot can attach directly to the running session at any time:

```bash
# Attach to the session
tmux -L openduck attach -t agy_1729148000

# Detach without closing the session
# Press: Ctrl-B followed by d
```

For more details on session architectures, state machines, and REST APIs, see the [Interactive TUI Sessions Guide](../guides/tui-sessions.md).
