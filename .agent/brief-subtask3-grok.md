# Grok task — Subtask 3 half A: proxy resolution module + ApiClient wiring

Repo: /mnt/e/openduck-harness (work there). Read `.agent/design-subtask3-proxy-client.md` first — it is the contract.

Another agent (`agy`) is editing OTHER files in the same worktree at the same time:
`crates/openduck-providers/src/{snowflake,databricks,databricks_v2,azure_foundry}.rs`,
`crates/openduck/src/providers/{snowflake_def,databricks_def,databricks_v2_def,azure_foundry_def,chatgpt_codex,kimicode,gcpvertexai,toolshim,gemini_oauth,bedrock}.rs`,
`crates/openduck/src/providers/githubcopilot.rs` (only its raw `Client::builder()` at ~:292).
Do NOT touch those files. Do NOT run `cargo clippy` / `cargo test` (the worktree will not compile until agy finishes);
only `cargo fmt` on your own files is allowed at the end.

## 1. NEW file `crates/openduck/src/providers/proxy.rs`

```rust
pub fn resolve_outbound_proxy(
    config: &crate::config::Config,
    provider_name: &str,
) -> anyhow::Result<Option<String>>;

pub fn resolve_global_proxy(config: &crate::config::Config) -> anyhow::Result<Option<String>>;

pub fn apply_proxy(
    builder: reqwest::ClientBuilder,
    proxy: Option<&str>,
) -> anyhow::Result<reqwest::ClientBuilder>;
```
- `resolve_global_proxy`: `config.get_proxy()?` (see `crates/openduck/src/config/base.rs:~541`), map `Some(p)` to
  `Some(p.url_with_credentials().to_owned())`, `None` -> `None`. Errors propagate (`ConfigError` -> `anyhow`).
- `resolve_outbound_proxy`: global first; if `Some`/`Err` return it. Else
  `crate::config::providers::get_provider_proxy(config, provider_name)` (unchanged fn, `crates/openduck/src/config/providers.rs:588`).
- `apply_proxy`: `None` or whitespace-only -> return builder unchanged (NO `.no_proxy()`, so reqwest keeps
  automatic env proxy fallback). Otherwise `reqwest::Proxy::all(trimmed)` and `.proxy(..)`.
  On error return a credential-free anyhow error: `anyhow!("Invalid proxy configuration")` — never include the URL
  or the reqwest source chain (it can echo the URL). Do not add `tracing` logs with proxy values.
- Add `#[cfg(test)]` unit tests in the module: `apply_proxy_absent_leaves_builder_usable` (build a client and
  assert it constructs), `apply_proxy_rejects_invalid_url_without_leaking_credentials` (pass
  `"http://user:secret@bad host"`-ish invalid value, assert error text does not contain `secret`),
  `resolve_global_proxy_reads_config_key` and `resolve_global_proxy_absent_is_none` using isolated temp configs
  (see `crates/openduck/tests/config_proxy_tests.rs` for the isolated-config pattern; use `NamedTempFile` +
  `Config::new_with_file_secrets`).
- Register `pub mod proxy;` in `crates/openduck/src/providers/mod.rs` (alphabetical: after `mod provider_registry`/
  `pub mod private_file`, before `pub mod retry` — check the actual list and keep rustfmt order).

## 2. Wire these sites (exact edits; keep existing decorator order)

Pattern: resolve once near the top of the fn with `?`, then apply only when `Some`:
```rust
let proxy = crate::providers::proxy::resolve_outbound_proxy(config, "<name>")?;
```
then `if let Some(proxy) = proxy { api_client = api_client.with_proxy(Some(proxy))?; }` — only when the site
has `ApiClient`; the three sites that already pass `proxy` into `with_timeout_tls_and_proxy` just get the new
resolver.

| File:line | Change |
|---|---|
| `crates/openduck/src/providers/openai_def.rs:125` | `get_provider_proxy(config,"openai")` -> `proxy::resolve_outbound_proxy(config,"openai")?` |
| `crates/openduck/src/providers/anthropic_def.rs:56` | same with `"anthropic"` |
| `crates/openduck/src/providers/ollama_def.rs:81` | same with `"ollama"` |
| `crates/openduck/src/providers/google_def.rs:57-60,71` | global first, else the existing google->gemini->GOOGLE_PROXY/GEMINI_PROXY chain; keep `.with_proxy(Some(p))` |
| `crates/openduck/src/config/declarative_providers.rs:410-414` | global `Some` replaces the definition proxy; else keep existing definition/legacy/env-expansion logic |
| `crates/openduck/src/providers/avian.rs:52` | global-only (`resolve_global_proxy`), conditionally `.with_proxy` |
| `crates/openduck/src/providers/azure.rs:120` | global-only |
| `crates/openduck/src/providers/litellm.rs:69` | global-only |
| `crates/openduck/src/providers/nanogpt.rs:37` | global-only |
| `crates/openduck/src/providers/githubcopilot.rs:263` (`authenticated_api_client`) | global-only; leave the raw `Client::builder()` at ~:292 to agy |
| `crates/openduck/src/providers/huggingface.rs:94,216` | global-only, both branches |
| `crates/openduck/src/providers/openrouter.rs:65` | global-only (`:440` is a test) |
| `crates/openduck/src/providers/ollama_cloud.rs:137` | global-only |
| `crates/openduck/src/providers/tetrate.rs:59` | global-only |
| `crates/openduck/src/providers/xai.rs:84` | global-only |
| `crates/openduck/src/providers/xai_oauth.rs:818` | global-only |
| `crates/openduck/src/dictation/providers.rs:294,412` | global-only, both |

Rules: apply `.with_proxy(Some(p))?` immediately after construction, BEFORE `with_header`/`with_headers`/
`with_request_builder`/redirect decorators. If the resolved value is None, do not call `with_proxy` at all
(no gratuitous rebuild; behavior must stay byte-identical to today).

Note: some of these fns take `config: &Config` already; others use `crate::config::Config::global()`. Use whatever
is in scope. All are `anyhow::Result` so `?` works.

## 3. Core-side tests — NEW `crates/openduck/tests/provider_proxy_tests.rs`

Local TCP proxy fixture (no external network): bind `127.0.0.1:0`, use an unreachable HTTP origin
`http://proxy-target.invalid`. For an HTTP proxy reqwest sends the absolute-form request line
(`POST http://proxy-target.invalid/responses HTTP/1.1`). Read until headers end, capture request line, reply with a
small `Content-Length` JSON body (and an SSE `text/event-stream` body for the streaming case), then close.
Bound every read/accept with `tokio::time::timeout` so regressions fail fast. Never log proxy credentials.

Tests (use `#[tokio::test]`, isolated temp configs written to `NamedTempFile`, and no process-env mutation —
if you need env isolation use `env_lock::lock_env`):
- `config_proxy_reaches_transport`: temp config with `proxy: "http://127.0.0.1:<port>"`, build an `ApiClient`
  via the production path (`ApiClient::new_with_tls(origin, AuthMethod::NoAuth, None)?` then
  `.with_proxy(proxy::resolve_global_proxy(&config)?)?`), POST, assert the listener saw the absolute-form
  request line and the response parsed.
- `config_proxy_absent_leaves_transport_unchanged`: same but no `proxy` key -> resolver returns None, builder
  untouched; assert dialing the origin directly fails (origin is `.invalid`) and no connection arrives at the
  proxy listener. This proves we never fall back to a hard-coded proxy.
- `invalid_config_proxy_is_an_error_and_redacted`: config with
  `proxy: "http://user:hunter2@proxy.example:notaport"` -> `resolve_global_proxy` returns Err and
  `format!("{err:#}")` contains neither `hunter2` nor the raw value; contains `` `proxy` `` key name.
- `provider_specific_proxy_still_used_when_global_absent`: set `providers.<n>.proxy` in the temp config (or use
  `crate::config::providers::set_provider_proxy`), assert `resolve_outbound_proxy(&config,"<n>")` returns it.
- `global_proxy_wins_over_provider_setting`: both set -> global value returned.

Keep the file under ~250 lines if possible. These are integration tests, so they can only use public APIs.

## Quality gates you must respect
- `cargo fmt` on edited files at the end. NO comments restating code. `anyhow::Result` for errors. No new logging.
- Do not change `get_provider_proxy`, the named `proxies` registry, SOCKS support, or any config schema.
- When done, write a short summary of every file you changed into `.agent/grok-subtask3-a.md`.
