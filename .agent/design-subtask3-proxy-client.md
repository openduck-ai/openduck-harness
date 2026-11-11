# Subtask 3: configured proxy → agent HTTP transport

Design contract; no Rust implementation in this change. References are current worktree lines, before implementation. Paths in tables are relative to the repository root.

## Resolution and compatibility

Add `crates/openduck/src/providers/proxy.rs`, exported as `pub mod proxy;` from `crates/openduck/src/providers/mod.rs` (module list starts at :2). Exact new signatures:

```rust
pub fn resolve_outbound_proxy(
    config: &crate::config::Config,
    provider_name: &str,
) -> anyhow::Result<Option<String>>;

pub fn resolve_global_proxy(
    config: &crate::config::Config,
) -> anyhow::Result<Option<String>>;

pub fn apply_proxy(
    builder: reqwest::ClientBuilder,
    proxy: Option<&str>,
) -> anyhow::Result<reqwest::ClientBuilder>;
```

Core already depends on reqwest (`crates/openduck/Cargo.toml:100`). Keep config access/resolution here; never add an openduck dependency to openduck-providers. Pass owned resolved strings across that boundary.

Resolution is **global file `proxy` > the complete existing provider resolution > reqwest automatic environment fallback**. Call `Config::get_proxy(&self) -> Result<Option<crate::config::proxy::ProxyUrl>, ConfigError>` (`crates/openduck/src/config/base.rs:541`) FIRST. On Some, return `url_with_credentials().to_owned()` immediately. On error, propagate; never fall back after invalid config. On None, call the unchanged `get_provider_proxy(config: &Config, name: &str) -> Option<String>` (`crates/openduck/src/config/providers.rs:588`).

Consequences, deliberately settled:

| Input | Winner / behavior |
|---|---|
| Nonempty global file proxy + provider env, `providers.<name>.proxy`, or named registry reference | Global file wins over ALL of them, including provider-specific env. This corrects the provider-first orchestrator draft. |
| Global absent/null/empty/literal lowercase `null` | Existing provider behavior, including its env-first ordering, remains intact. Empty is absence, not a request to disable all proxies. |
| No global and no legacy provider result | None; do not change the builder. |
| Legacy provider setting names a `proxies` entry or uses SOCKS | Existing registry lookup/SOCKS support stays unchanged; do not run this value through the new HTTP-only config parser. |
| Global value names a registry entry or uses SOCKS | Invalid under the already implemented top-level schema. No new registry interpretation. |

Legacy order remains OPENDUCK_PROVIDER_<N>_PROXY → GOOSE_PROVIDER_<N>_PROXY → <N>_PROXY env → <N>_PROXY via get_param → providers.<name>.proxy (`providers.rs:605-628`). Do not edit that function or split its precedence. Thus backward compatibility applies when the NEW global override is absent; it cannot simultaneously mean that old settings beat an explicitly configured global override.

Strict absence compatibility: sites that previously had NO explicit proxy resolution must use `resolve_global_proxy`, not introduce legacy provider resolution there. This helper is exactly get_proxy + credential-bearing string conversion, with errors propagated. `resolve_outbound_proxy` composes it with the existing legacy resolver only for sites that already used that resolver. This prevents absent global config from newly activating a dormant providers.<name>.proxy or provider-specific env variable at previously unproxied sites.

For Google, first check global; only on None evaluate the entire current google → gemini → GOOGLE_PROXY/GEMINI_PROXY fallback chain (`crates/openduck/src/providers/google_def.rs:57-60`). For declarative providers, first check global; on None retain the existing explicit definition proxy and subsequent legacy fallback (`crates/openduck/src/config/declarative_providers.rs:382,410-414`). Do not put a global fallback after an already env-resolved value.

`apply_proxy` returns the original builder for None or whitespace-only Some. Otherwise use `reqwest::Proxy::all(trimmed)` and `.proxy(...)`. Never add `.no_proxy()` on this path. reqwest 0.13.5 `src/async_impl/client.rs:1423-1425` explicitly disables automatic system proxy discovery when `.proxy()` is used. With no explicit proxy, existing HTTP(S)_PROXY/ALL_PROXY and NO_PROXY behavior remains whatever the existing transport policy permits. In particular, preserve intentional loopback no-proxy policy. Do not copy environment variables into explicit proxies or mutate process environment.

Errors: never format a raw resolved string or log `ApiClient::proxy()` (`api_client.rs:324`). Map any NEW proxy parse/build error boundary to a credential-free message; use `redact_proxy_url` if identifying a proxy. Do not attach an unsanitized source error that could expose credentials via its chain. Existing ConfigError from get_proxy is already redacted.

## ApiClient wiring strategy

Use existing constructors and conditionally apply `.with_proxy(Some(resolved))?` immediately after construction, before existing header/transport decorators. When resolution is None, skip with_proxy entirely: avoid a gratuitous rebuild and preserve each site's existing behavior. No shared ApiClient factory and no constructor signature churn in this subtask.

Exact existing APIs (`crates/openduck-providers/src/api_client.rs`):

```rust
pub fn with_proxy(mut self, proxy: Option<String>) -> Result<Self>; // :318
pub fn with_headers(mut self, headers: HeaderMap) -> Result<Self>; // :442
pub fn with_header(mut self, key: &str, value: &str) -> Result<Self>; // :453
fn rebuild_client(&mut self) -> Result<()>; // :343
```

Here Result is anyhow::Result. Rebuild replaces the connection pool, retains timeout, default headers, transport policy and TLS (:343-361); request decorator/query/auth/host stay in struct fields. This is acceptable before any request. Header decorators rebuild again but retain the stored proxy. Keep existing header order: `with_headers` replaces the map, `with_header` inserts. Do not reorder decorators to save builds. Loopback transport applies no_proxy before stored explicit proxy on rebuild (:365 onward); preserve this deliberate explicit-override behavior.

### Core construction sites to wire

All paths below are under `crates/openduck/src/`.

| File:line | Change |
|---|---|
| providers/openai_def.rs:125; anthropic_def.rs:56; ollama_def.rs:81 | Replace legacy resolution with fallible global-first resolver; retain existing proxy-aware constructor. |
| providers/google_def.rs:57,71 | Global-first selection with full legacy alias chain preserved; retain GoogleProvider::with_proxy. |
| config/declarative_providers.rs:410 | Global Some replaces definition proxy; otherwise preserve current definition/legacy precedence and env expansion. |
| providers/avian.rs:52; azure.rs:120; litellm.rs:69; nanogpt.rs:37 | Use resolve_global_proxy; conditional with_proxy before decorators. |
| providers/githubcopilot.rs:263; huggingface.rs:94,216 | Same; cover both Hugging Face construction branches. |
| providers/openrouter.rs:65; ollama_cloud.rs:137; tetrate.rs:59; xai.rs:84 | Same. |
| providers/xai_oauth.rs:818 | Same; production site omitted from original recon. Use global-only resolution, preserving the previous absence behavior. |
| dictation/providers.rs:294,412 | Use resolve_global_proxy, preserving URL/query/TLS/header logic. |

### Providers-crate boundary

| Existing site | Required change |
|---|---|
| crates/openduck-providers/src/google.rs:87,100 | Existing `pub fn with_proxy(mut self, proxy: Option<String>) -> Result<Self>` suffices; core google_def supplies resolved value. |
| crates/openduck-providers/src/snowflake.rs:81 | Add provider-level `pub fn with_proxy(mut self, proxy: Option<String>) -> anyhow::Result<Self>` delegating to its ApiClient. Core snowflake_def.rs:47 resolves global-only and applies conditionally after new. |
| crates/openduck-providers/src/databricks.rs:127; databricks_v2.rs:95 | Same provider-level signature on each type; core databricks_def.rs:53 / databricks_v2_def.rs:54 resolves global-only and conditionally applies after new. :95 in databricks.rs is not a second ApiClient construction. |
| crates/openduck-providers/src/azure_foundry.rs:378 | Thread resolved proxy through a new create_with_proxy path into EVERY configured_client invocation, including deployments, chat, responses and Anthropic; retain create's existing behavior by delegating with None. Core azure_foundry_def.rs:79 resolves global-only and uses new path. |

Exact new Azure signatures (existing create starts at azure_foundry.rs:162, configured_client at :371):

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
) -> anyhow::Result<Self>;

fn configured_client(
    host: String,
    auth: AuthMethod,
    tls_config: Option<TlsConfig>,
    request_builder: Option<RequestBuilderDecorator>,
    proxy: Option<String>,
) -> anyhow::Result<ApiClient>;
```

Clone the proxy for each internal client. Preserve same-origin redirect restrictions and request decorators. Existing public constructors elsewhere keep their signatures and None default; direct SDK consumers do not load core configuration.

## Raw reqwest clients

Paths below are under `crates/openduck/src/providers/`. For these previously unproxied sites resolve_global_proxy only, then apply_proxy to the existing builder before build, retaining timeouts, redirects, TLS and headers.

| File:line | Decision |
|---|---|
| chatgpt_codex.rs:927 | INCLUDE `async fn post_streaming(&self, payload: &Value) -> Result<reqwest::Response, ProviderError>` (:911). Resolve/apply before build; convert sanitized failures to ProviderError::ExecutionError. Covers the actual streaming request, not only setup. |
| kimicode.rs:176; gcpvertexai.rs:183 | INCLUDE model transports. |
| githubcopilot.rs:292 | INCLUDE companion raw client as well as ApiClient at :263. |
| toolshim.rs:604 | INCLUDE model shim transport. |
| bedrock.rs:293 | INCLUDE reqwest http_client (Mantle/bearer path). AWS SDK Client::from_conf at :279 is a distinct transport: explicit non-goal; do not claim all Bedrock traffic is covered. |
| gemini_oauth.rs:41-47 | INCLUDE. Replace HTTP_CLIENT LazyLock with fallible `fn http_client() -> anyhow::Result<reqwest::Client>` using resolve_global_proxy and existing timeouts. Replace ALL uses at :244,274,346,375,884 with `http_client()?` (adapt error conversion where necessary). No expect, cached global Result, or silent fallback. Auth-state LazyLock at :128 stays. Rebuilding per operation is acceptable here; caching/reload optimization is deferred. |
| oauth.rs:45 | EXCLUDE whole `fn secure_client_for_endpoint(endpoint: &Url) -> Result<Client>` (:44). HTTPS-only remote auth and intentional loopback no_proxy/redirect guard (:47-55) remain unchanged. This is legacy OAuth, not model inference. |

Gemini's factory also changes shared external token/setup requests because they currently use the SAME client; accept that limited consequence. Browser callback listener at gemini_oauth.rs:636 is not an outbound client and stays unchanged.

## Explicit exclusions

| File:line | Reason |
|---|---|
| crates/openduck/src/control/network.rs:173,317 | Diagnostics, not inference. Keep endpoint resolution and `async fn http(url: &str, proxy: Option<&str>, direct: bool) -> Probe` (:315) unchanged, including own Proxy::all and direct no_proxy. Its display/probe may differ from global-configured inference; diagnostics alignment is deferred. |
| crates/openduck/src/security/classification_client.rs:54 | Separate security service. |
| crates/openduck/src/tracing/langfuse_layer.rs:48 | Telemetry. |
| crates/openduck/src/agents/extension_manager.rs:718,803 | MCP/auth transport. |
| crates/openduck/src/agents/platform_extensions/developer/image.rs:186 | Tool image retrieval; this is the actual image.rs path. |
| crates/openduck/src/gateway/telegram.rs:117 | Gateway traffic. |
| crates/openduck-harness/src/judge/laya.rs:126 | Harness judge service. |
| crates/openduck/src/providers/openrouter.rs:440; crates/openduck-providers/src/openai_compatible.rs:364 | Test-only ApiClient construction; do not inject machine config. |
| crates/openduck-providers/src/api_client.rs:821,849 onward; openai.rs:1031,1514,1813,1862; anthropic.rs:530; core anthropic_def.rs:102, xai_oauth.rs:930 | Test fixtures; no global config injection. |

No changes to TUI subprocess env export, third-party CLI/ACP transports, AWS SDK transport, global env, proxy schema, registry CRUD, or agent-loop logic. Therefore no legacy/state-machine behavioral edits or parity patches are needed.

## Required implementation tests

New tests, not executed or implemented by this design-only task:

| Crate / file | Exact test names and assertions |
|---|---|
| openduck / tests/provider_proxy_tests.rs | `global_proxy_beats_provider_env_and_file_proxy`: global wins over every legacy env spelling and provider entry; include a registry reference. `absent_global_preserves_provider_proxy_precedence`: env-first chain unchanged. `empty_global_preserves_provider_proxy_resolution`: empty/null behave like absent. `legacy_named_and_socks_proxies_remain_supported`: registry/SOCKS strings unchanged. `invalid_global_proxy_is_not_ignored`: error even if valid legacy fallback exists, credentials absent from Display and Debug. `no_explicit_proxy_returns_none`: no ambient legacy settings. `absent_global_does_not_enable_legacy_proxy_at_unwired_sites`: resolve_global_proxy returns None despite populated provider env/file entries. |
| openduck / tests/provider_proxy_tests.rs | `config_proxy_reaches_transport`: load isolated temporary Config with top-level proxy, call production resolver, apply to real ApiClient, send to unreachable HTTP origin, assert listener receives absolute-form request. Must not stop at asserting returned string or proxy(). |
| openduck / tests/provider_proxy_tests.rs | `raw_client_config_proxy_reaches_transport`: same Config → resolver → apply_proxy → raw builder → request, proving raw wiring helper. `none_proxy_preserves_environment_fallback`: isolated subprocess with HTTP_PROXY pointing at listener and NO_PROXY cleared, resolver None and unchanged builder routes request via env. |
| openduck-providers / tests/api_client_proxy_tests.rs | `explicit_proxy_non_streaming_uses_absolute_form`: ApiClient::response_post routes through listener. `explicit_proxy_streaming_uses_absolute_form`: client.request("responses").streaming(true).response_post sends streaming request through same listener; consume response body. `proxy_survives_header_rebuild`: with_proxy then with_header then with_headers then another with_header; proxy receives request with expected final headers. |
| openduck / providers/google_def.rs tests module (:77 onward) | `global_proxy_beats_google_and_gemini_env`: exercise production alias selection, not a duplicated test-only resolver. |
| openduck / config/declarative_providers.rs tests module (:560 onward) | `global_proxy_beats_declarative_proxy`; `absent_global_preserves_declarative_proxy`: exercise resolve_config with definition value and legacy env collisions. |
| openduck-providers / azure_foundry.rs tests module | `configured_proxy_applies_to_all_foundry_clients`: construct native and MaaS routes with proxy; exercise chat/responses/Anthropic/deployments with controlled responses and assert each expected route arrives at proxy. |

TCP proxy fixture: bind 127.0.0.1:0; use `http://proxy-target.invalid` as origin, no real DNS/service dependency. Read headers AND declared request body, assert exact request line `POST http://proxy-target.invalid/responses HTTP/1.1`, return bounded Content-Length response (SSE data for streaming), close connection. Bound accept/read/body waits so routing regressions fail promptly. Never use api_client.rs:828's test helper, which replaces the actual transport with no_proxy. Use no real credentials. Environment-changing cases run in isolated child processes; do not race process-global environment in parallel tests. Set competing env proxy to an unusable endpoint in explicit-proxy routing cases to prove it cannot override config.

Future verification: run cargo fmt for Rust implementation; targeted tests in both crates and clippy only when requested under repository instructions. Full feature acceptance should update openduck-self-test.yaml and run its documented recipe when build/test execution is authorized. Existing reported clippy blocker at crates/openduck/src/providers/inventory/mod.rs:1417 is unrelated; recheck/report, do not silently waive a new failure. This design-only task requires no Rust formatting/build/test run.
