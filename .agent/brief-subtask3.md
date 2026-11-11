# Brief: Subtask 3 — apply configured proxy to the agent HTTP client

Repo: /mnt/e/openduck-harness (work in this dir).

## Already done (subtask 2, verified; do NOT redo or modify beyond listed needs)
Top-level `proxy` key exists in core dynamic config:
- `crates/openduck/src/config/proxy.rs` (new, 559 lines): `ProxyUrl` (private `url`, Clone, redacted Display/Debug),
  `ProxyUrl::as_str()` / `url_with_credentials()` (INCLUDE credentials), `host()`, `port()`;
  `parse_proxy_url(key: &str, raw: &str) -> Result<Option<ProxyUrl>, ConfigError>`; `redact_proxy_url(raw) -> String`.
- `Config::get_proxy(&self) -> Result<Option<ProxyUrl>, ConfigError>` in `crates/openduck/src/config/base.rs`
  (~line 538). NOT env-first. Missing key / null / "" / "null" => Ok(None).
- `crates/openduck/tests/config_proxy_tests.rs` (16 tests) covers unset/empty/valid/invalid/layering.

## Verified code facts
### api_client (crates/openduck-providers/src/api_client.rs)
- `ApiClient::with_timeout_tls_and_proxy(host, auth, timeout, tls_config, proxy: Option<String>)` L283;
  `proxy.filter(!trim.empty)` -> `reqwest::Proxy::all(trim)?` -> `builder.proxy(...)`; stores `self.proxy`.
- `new_with_tls` and `with_timeout_and_tls` delegate with `proxy: None` (L270-281).
- `with_proxy(Option<String>)` L318 stores + `rebuild_client()`. `rebuild_client()` L342-361 re-applies
  `configure_transport(policy)` then proxy then TLS.
- `proxy()` L324 returns raw string WITH credentials — never log it.
- streaming + non-streaming share `self.client` (single transport) — one proxy covers both.
- `Debug for ApiClient` omits proxy/client — safe.
- `TransportPolicy::LoopbackHttp` calls `.no_proxy()`; rebuild adds explicit proxy after. Preserve.
- IMPORTANT reqwest semantics: `ClientBuilder::proxy(..)` sets `auto_sys_proxy = false` (reqwest 0.13.5
  client.rs L1422-1427). So "no explicit proxy" => env HTTP(S)_PROXY still honored automatically = the
  required fallback behavior. Do not add code that copies env proxy vars into the explicit proxy.

### existing proxy resolution
- `crates/openduck/src/config/providers.rs:588 get_provider_proxy(config, name) -> Option<String>`
  (env OPENDUCK_PROVIDER_<N>_PROXY -> GOOSE_PROVIDER_<N>_PROXY -> <N>_PROXY env -> <N>_PROXY config -> providers.<n>.proxy;
  resolves named entries in `proxies`; allows SOCKS — must stay untouched for back-compat).
- callers: openai_def.rs:125, anthropic_def.rs:56, google_def.rs:57 (+GOOGLE_PROXY/GEMINI_PROXY fallbacks),
  ollama_def.rs:81, config/declarative_providers.rs:410-414, control/network.rs:173-176.

### construction sites WITHOUT any proxy today (must be wired)
ApiClient but no proxy:
- core: providers/avian.rs:52, azure.rs:120, githubcopilot.rs:263, huggingface.rs:94,216,
  litellm.rs:69, nanogpt.rs:37, openrouter.rs:65,440, ollama_cloud.rs:137, tetrate.rs:59, xai.rs:84,
  dictation/providers.rs:294,412
- providers crate: snowflake.rs:81, databricks.rs:95/127, databricks_v2.rs:95, azure_foundry.rs:378,
  openai_compatible.rs:364 (only tests? verify), google.rs:87
raw `reqwest::Client::builder()` (no proxy at all):
- chatgpt_codex.rs:927 (`post_streaming`, streaming! must be wired), kimicode.rs:176, gcpvertexai.rs:183,
  githubcopilot.rs:292, toolshim.rs:604, gemini_oauth.rs:42 (LazyLock static), bedrock.rs:293 (`http_client`),
  providers/oauth.rs:45 (legacy oauth; loopback policy — decide: leave).
- control/network.rs:317 (network probe; uses own Proxy::all at ~327) — decide whether to include.
- Out of scope: security/classification_client.rs, tracing/langfuse_layer.rs, agents/extension_manager.rs,
  image.rs, gateway/telegram.rs, harness/src/judge/laya.rs (not agent model traffic).

### config access pattern in defs
`crate::config::Config::global()` is used inside `from_env` fns; they return `anyhow::Result`, so a fallible
resolver can be propagated with `?`.

## Desired contract (must be confirmed/refined by you in the design doc)
1. Precedence: file `proxy` (via `Config::get_proxy`) > existing provider-specific resolution
   (`get_provider_proxy`) > nothing (reqwest env auto-proxy remains the fallback).
2. Absent/empty config => byte-identical behavior to today (proxy arg None, transport untouched).
3. Credentials never logged/printed in errors: any new error path must use `redact_proxy_url`/ProxyUrl Display.
4. Streaming covered (shared ApiClient transport + chatgpt_codex post_streaming).
5. Tests must prove the transport is REALLY configured (local TCP listener acting as HTTP proxy, request to
   an unreachable origin like http://proxy-target.invalid arrives in absolute-form at the listener), for both
   ordinary and streaming requests, plus retention across `rebuild_client` (e.g. `.with_header(..)` after proxy).

## Hard constraints
- Do not use `reqwest::ClientBuilder::no_proxy()` when config proxy is absent (that would break env fallback).
- Keep `proxies` registry, provider `proxy` semantics, SOCKS support unchanged.
- Do not add credentials to logs; `ApiClient::proxy()` must never be traced.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` clean. Known pre-existing clippy failure:
  `crates/openduck/src/providers/inventory/mod.rs:1417:85 needless_borrow` (unrelated, ignore/report only).
