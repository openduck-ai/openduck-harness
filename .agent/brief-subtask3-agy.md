# agy task — Subtask 3 half B: raw reqwest client sites + providers-crate proxy plumbing

Repo: /mnt/e/openduck-harness (work there). Read `.agent/design-subtask3-proxy-client.md` first — it is the contract.

Another agent (`grok`) is editing these files at the same time. Do NOT touch them:
`crates/openduck/src/providers/proxy.rs` (NEW), `providers/mod.rs`, `openai_def.rs`, `anthropic_def.rs`,
`ollama_def.rs`, `google_def.rs`, `config/declarative_providers.rs`, `avian.rs`, `azure.rs`, `litellm.rs`,
`nanogpt.rs`, `huggingface.rs`, `openrouter.rs`, `ollama_cloud.rs`, `tetrate.rs`, `xai.rs`, `xai_oauth.rs`,
`crates/openduck/src/dictation/providers.rs`, `crates/openduck/tests/provider_proxy_tests.rs`, and in
`githubcopilot.rs` ONLY the `authenticated_api_client` fn (~:263); you own its raw `Client::builder()` at ~:292.

Do NOT run `cargo clippy` / `cargo test` (the worktree will not compile until grok finishes). `cargo fmt` at the end only.

The production-side resolver is being written by grok in `crates/openduck/src/providers/proxy.rs`. Assume this
public API exists and use it (do not create it yourself):
```rust
pub fn resolve_global_proxy(config: &crate::config::Config) -> anyhow::Result<Option<String>>;
pub fn apply_proxy(builder: reqwest::ClientBuilder, proxy: Option<&str>) -> anyhow::Result<reqwest::ClientBuilder>;
```
`resolve_global_proxy` returns the credential-bearing URL string (`Some`) or `None` (no explicit proxy; reqwest then
keeps its automatic HTTP(S)_PROXY env fallback). `apply_proxy` returns the builder unchanged for `None`/whitespace
and applies `reqwest::Proxy::all(trimmed)` otherwise, with a credential-free error. If those do not exist yet when
you finish, that is fine — code review is mine.

## A. Raw `reqwest::Client::builder()` sites in `crates/openduck/src/providers/`

For each: resolve with `resolve_global_proxy(&config)?` (or `crate::config::Config::global()`), then
`builder = apply_proxy(builder, proxy.as_deref())?;` BEFORE `.build()`. When `None`, the builder must be identical to today.

| File:line | Notes |
|---|---|
| `chatgpt_codex.rs:927` (`post_streaming`) | Streaming path — MUST be covered. Resolve once via `Config::global()`. Keep connect/read timeouts. |
| `kimicode.rs:176` | |
| `gcpvertexai.rs:183` | |
| `githubcopilot.rs:292` (only this builder, not `authenticated_api_client`) | |
| `toolshim.rs:604` | Currently `.build().expect(...)`; keep the `.expect` style but apply the proxy before `.build()`; if proxy resolution errors, return a `ProviderError` instead of panicking. |
| `bedrock.rs:293` (`http_client`) | This struct field is used for mantle/HTTP traffic; wire it. Leave the aws-sdk `Client` untouched. |
| `gemini_oauth.rs:41-47` (`static HTTP_CLIENT: LazyLock<reqwest::Client>`) | Static cannot return an error. Resolve with `resolve_global_proxy(Config::global())` inside the closure and apply to the builder; on resolver/apply error, fall back to a builder with no explicit proxy (do NOT panic and do NOT log the proxy value). Add a short `// why` comment explaining the static fallback. Leave the browser callback listener at ~:636 alone. |
| `oauth.rs:45` | EXCLUDED — legacy OAuth loopback transport; leave unchanged. |

Out of scope: `control/network.rs:317`, `security/classification_client.rs`, `tracing/langfuse_layer.rs`,
`agents/extension_manager.rs`, `platform_extensions/developer/image.rs`, `gateway/telegram.rs`,
`crates/openduck-harness/src/judge/laya.rs`. Do not edit these.

## B. `crates/openduck-providers` — provider-level `with_proxy`, plus core wiring in `*_def.rs`

These providers build `ApiClient` internally and expose no way to set a proxy. Add a
`pub fn with_proxy(mut self, proxy: Option<String>) -> anyhow::Result<Self>` to each (delegating to
`self.api_client.with_proxy(proxy)`; for `AzureFoundryProvider` this must reach every client — see below), then wire
`crates/openduck/src/providers/*_def.rs` to resolve global-only and apply conditionally.

| Provider file | Core def file | Notes |
|---|---|---|
| `crates/openduck-providers/src/snowflake.rs` (`SnowflakeProvider`, `new` at :59) | `crates/openduck/src/providers/snowflake_def.rs:47` (`from_env`) | add `with_proxy`; core: resolve `resolve_global_proxy(config)?`, build, then `if let Some(p) = proxy { provider = provider.with_proxy(Some(p))?; }` |
| `crates/openduck-providers/src/databricks.rs` (`DatabricksProvider`, `new` at :103) | `crates/openduck/src/providers/databricks_def.rs` (`from_env`) | same |
| `crates/openduck-providers/src/databricks_v2.rs` (`DatabricksV2Provider`, `new` at :73) | `crates/openduck/src/providers/databricks_v2_def.rs` (`from_env`) | same |
| `crates/openduck-providers/src/azure_foundry.rs` (`create` at :162, `configured_client` at :371, calls at :195,208,221,237) | `crates/openduck/src/providers/azure_foundry_def.rs:79` (`from_env`) | see below |

`AzureFoundryProvider` holds several clients (`chat`, `responses`, `anthropic`, `deployments_client`). Add
```rust
pub fn create_with_proxy(
    endpoint: String,
    api_version: Option<String>,
    maas_model: Option<String>,
    chat_auth: AuthMethod,
    responses_auth: AuthMethod,
    anthropic_auth: AuthMethod,
    deployments_auth: AuthMethod,
    tls_config: Option<TlsConfig>,
    request_builder: Option<RequestBuilderDecorator>,
    proxy: Option<String>,
) -> Result<Self>;
```
`create` delegates to it with `None` so existing behavior and signatures stay intact. Thread the proxy into
`configured_client` (add a `proxy: Option<&str>` param, apply via `crate::api_client`-level
`ApiClient::new_with_tls(...)?` then `.with_proxy(proxy.map(str::to_string))?` only when Some) so all four
construction sites at :195/:208/:221/:237 get it. Also add an instance method
`pub fn with_proxy(self, proxy: Option<String>) -> Result<Self>` that rebuilds chat/responses/anthropic/
deployments_client through their existing `with_proxy` methods if the types expose them (`OpenAiCompatibleProvider`,
`OpenAiProvider`, `AnthropicProvider`, `ApiClient`); if a type lacks one, add the minimal `with_proxy` delegating to
its `api_client`. Prefer `create_with_proxy` from the core def (single construction, no rebuild churn).

Core `azure_foundry_def.rs` `from_env`: resolve `resolve_global_proxy(config)?` once, call `create_with_proxy`
with it. Keep all existing auth/env logic untouched.

## C. Rules
- Absent/empty global proxy => byte-identical behavior to today; never call `.no_proxy()`; never read `HTTP_PROXY` etc.
  into an explicit proxy (reqwest already does that as the fallback).
- Never log or include a proxy URL in an error. Map proxy failures to a generic `anyhow!("Invalid proxy configuration")`
  style message (grok's `apply_proxy` already does this; reuse it, do not duplicate).
- `anyhow::Result` for new fallible fns; no new comments restating code; no new dependencies.
- `cargo fmt` at the end. When done, write a summary of every file you changed into `.agent/agy-subtask3-b.md`.
