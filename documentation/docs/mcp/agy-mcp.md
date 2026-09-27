---
title: Antigravity CLI Extension
description: Call the Antigravity CLI (agy) from goose without changing your LLM provider
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import GooseBuiltinInstaller from '@site/src/components/GooseBuiltinInstaller';

The Antigravity CLI extension lets goose send prompts to the local [`agy`](https://antigravity.google/docs/cli/overview) CLI, then inspect those Antigravity conversations. goose stays on whatever provider you already configured. Antigravity CLI does the delegated work.

## Prerequisites

1. Install Antigravity CLI:

   ```bash
   curl -fsSL https://antigravity.google/cli/install.sh | bash
   ```

2. Authenticate with an interactive session:

   ```bash
   agy
   ```

   Or, for headless/CI runs, set `modelProvider` to `gemini` in `~/.gemini/antigravity-cli/settings.json` and export `GEMINI_API_KEY`. If `agy` is not on your PATH, set `AGY_COMMAND` to the binary.

## Configuration

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseBuiltinInstaller
    extensionName="Antigravity CLI"
    description="Call the Antigravity CLI (agy) to run prompts and inspect conversations"
    extensionId="agy"
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">

  1. Run the `configure` command:
  ```sh
  goose configure
  ```

  2. Choose `Toggle Extensions` and enable `agy`.

  Or start a session with the extension for this run only:

  ```sh
  goose session --with-builtin agy
  ```

  </TabItem>
</Tabs>

Use a longer timeout for large jobs (Desktop defaults to 600 seconds). In a recipe or config:

```yaml
extensions:
  - type: builtin
    name: agy
    timeout: 600
```

## Tool Reference

| Tool | What it does |
|------|--------------|
| `agy_run(prompt, cwd?, conversation_id?, continue_last?, model?, effort?, agent?, output_format?, print_timeout?)` | Run `agy -p` (defaults to `json`) and return the result plus an Antigravity `conversation_id` |
| `agy_sessions(query?, limit?)` | List recent Antigravity conversations, or search titles, previews, and workspaces |
| `agy_status(conversation_id)` | Read title, preview, workspace, step count, and timestamps for a conversation |

`agy_run` always passes `--dangerously-skip-permissions` so Antigravity can finish without a TTY. Conversations are stored under `~/.gemini/antigravity-cli/`.

## Example

```text
Use Antigravity CLI to review crates/goose/src/providers for ACP wiring.
Then show me the Antigravity conversation id and status.
```

goose should call `agy_run`, then `agy_status` with the returned `conversation_id`. To continue that same Antigravity conversation:

```text
Resume that Antigravity conversation and ask it to list the remaining follow-ups.
```
