---
title: Google Gemini
description: Use Gemini models from Google AI Studio with a Gemini API key
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PanelLeft } from 'lucide-react';

# Google Gemini

The `google` provider (alias `gemini`) connects OpenDuck to [Gemini models](https://ai.google.dev/gemini-api/docs/models) through the Gemini API. Authenticate with an API key from [Google AI Studio](https://aistudio.google.com/apikey). This is the supported path for Gemini when you have a Google API key.

| | |
|---|---|
| Provider id | `google` |
| Alias | `gemini` |
| Display name | Google Gemini (API Key) |
| Default model | `gemini-2.5-pro` |
| Fast model | `gemini-2.5-flash` |
| API host | `https://generativelanguage.googleapis.com` |

This provider is **not** Vertex AI, Gemini CLI, or Gemini OAuth:

| Path | When to use |
|---|---|
| **Google Gemini (this provider)** | You have a Gemini API key from Google AI Studio |
| [GCP Vertex AI](/docs/getting-started/providers#available-providers) | You want Gemini (or Claude) through a GCP project |
| [Gemini CLI / Gemini OAuth](/docs/guides/cli-providers) | Deprecated. Use this API-key provider or Vertex AI instead |

## Get an API key

1. Open [Google AI Studio](https://aistudio.google.com/apikey) and sign in with your Google account.
2. Create a new API key or select an existing one.
3. Copy the key. Google's SDK documents it as `GEMINI_API_KEY`; OpenDuck also accepts the existing `GOOGLE_API_KEY` name.

## Configuration

| Variable | Required | Description |
|---|---:|---|
| `GEMINI_API_KEY` or `GOOGLE_API_KEY` | Yes | Gemini API key from Google AI Studio. If both are set, `GOOGLE_API_KEY` wins |
| `GOOGLE_HOST` | No | Gemini API host. Defaults to `https://generativelanguage.googleapis.com` |
| `GEMINI3_THINKING_LEVEL` | No | Thinking level for Gemini 3 models: `low` (default) or `high` |
| `GEMINI25_THINKING_BUDGET` | No | Optional thinking token budget for Gemini 2.5 models |

<Tabs groupId="interface">
  <TabItem value="env" label="Environment" default>

Set a key and start a session. `openduck` and the legacy `goose` CLI both work.

```sh
export GEMINI_API_KEY="your-key"          # or GOOGLE_API_KEY
export OPENDUCK_PROVIDER=gemini           # or google; legacy: GOOSE_PROVIDER
export OPENDUCK_MODEL=gemini-2.5-flash    # optional; default is gemini-2.5-pro
openduck session
```

`openduck configure` treats `GEMINI_API_KEY` as already configured when that environment variable is set. Choose **No** if you do not want to copy it into the keyring.

  </TabItem>
  <TabItem value="cli" label="OpenDuck CLI">

```sh
openduck configure
```

1. Select **Configure Providers**.
2. Choose **Google Gemini**.
3. Paste your API key when prompted for `GOOGLE_API_KEY` (or skip if `GEMINI_API_KEY` / `GOOGLE_API_KEY` is already in the environment).
4. Enter a model such as `gemini-2.5-flash`.

```
┌   goose-configure
│
◇ What would you like to configure?
│ Configure Providers
│
◇ Which model provider should we use?
│ Google Gemini
│
◇ Provider Google Gemini requires GOOGLE_API_KEY, please enter a value
│▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪
│
◇ Enter a model from that provider:
│ gemini-2.5-flash
│
└ Configuration saved successfully
```

You can also pass the provider on a single run:

```sh
openduck session --provider gemini --model gemini-2.5-flash
```

  </TabItem>
  <TabItem value="ui" label="OpenDuck Desktop">

1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar.
2. Click **Settings**, then the **Models** tab.
3. Click **Configure Providers**.
4. Choose **Google Gemini**, click **Configure**, paste your API key, and submit.

  </TabItem>
</Tabs>

### Config file

Provider selection is stored under `active_provider` and `providers`. Use the canonical id `google` (the `gemini` alias is resolved at runtime):

```yaml
active_provider: google
providers:
  google:
    enabled: true
    model: gemini-2.5-flash
    configured: true
```

`OPENDUCK_PROVIDER=gemini` and `GOOSE_PROVIDER=gemini` also resolve to this provider for that process.

In the Web Hub harness policy, set `provider` to `google` or `gemini`.

## Models

OpenDuck lists known Gemini models and can also fetch the live catalog from the API. Common choices:

| Model | Notes |
|---|---|
| `gemini-2.5-pro` | Default model |
| `gemini-2.5-flash` | Default fast model (titles, classification, and similar auxiliary calls) |
| `gemini-2.5-flash-lite` | Lower-latency Flash variant |
| `gemini-2.0-flash` | Gemini 2.0 Flash |
| `gemini-3-pro-preview` | Gemini 3 Pro preview |
| `gemini-3.5-flash` | Gemini 3.5 Flash |
| `gemini-3.6-flash` | Gemini 3.6 Flash |

See Google's [model list](https://ai.google.dev/gemini-api/docs/models) for current names and limits.

Override the fast model with `OPENDUCK_FAST_MODEL` (legacy `GOOSE_FAST_MODEL`) if you want a different auxiliary model.

## Thinking

Gemini 3 models accept a thinking level. Set it in the model picker, during `openduck configure`, or globally:

```sh
export GEMINI3_THINKING_LEVEL=high   # or low
```

Priority (highest first): `request_params.thinking_level` on the model config, then `GEMINI3_THINKING_LEVEL`, then `low`. Details are in [Gemini 3 Thinking Levels](/docs/getting-started/providers#gemini-3-thinking-levels).

To show thinking output in the CLI:

```sh
export OPENDUCK_CLI_SHOW_THINKING=1
```

Desktop shows reasoning in a collapsible **Show reasoning** toggle.

## Related docs

- [Configure LLM Provider](/docs/getting-started/providers#google-gemini)
- [Environment variables](/docs/guides/environment-variables)
- [CLI providers (deprecated Gemini CLI / OAuth)](/docs/guides/cli-providers)
