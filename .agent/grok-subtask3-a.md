# Subtask 3 half A — proxy resolution and ApiClient wiring

`cargo fmt` was run on the files below. `cargo test` and `cargo clippy` were not run.

## New

- `crates/openduck/src/providers/proxy.rs` — `resolve_global_proxy` (file `proxy` via `Config::get_proxy`, credential URL, errors propagated), `resolve_outbound_proxy` (global wins, otherwise unchanged `get_provider_proxy`), `apply_proxy` (absent or whitespace leaves the builder alone; invalid URL is `Invalid proxy configuration` with no URL and no reqwest source). Unit tests cover absence, credential redaction, and isolated temp configs.
- `crates/openduck/tests/provider_proxy_tests.rs` — local TCP proxy on `127.0.0.1:0` and origin `http://proxy-target.invalid`. Tests: `config_proxy_reaches_transport`, `config_proxy_absent_leaves_transport_unchanged`, `invalid_config_proxy_is_an_error_and_redacted`, `provider_specific_proxy_still_used_when_global_absent`, `global_proxy_wins_over_provider_setting`.

## Wired

`pub mod proxy` registered in `crates/openduck/src/providers/mod.rs` after `provider_test` and before `retry`.

Global-first `resolve_outbound_proxy`, still passed into the existing proxy constructor:

- `crates/openduck/src/providers/openai_def.rs`
- `crates/openduck/src/providers/anthropic_def.rs`
- `crates/openduck/src/providers/ollama_def.rs`

Global first, then the previous chain only when global is absent:

- `crates/openduck/src/providers/google_def.rs` (google → gemini → `GOOGLE_PROXY` / `GEMINI_PROXY`)
- `crates/openduck/src/config/declarative_providers.rs` (global `Some` replaces the definition proxy; otherwise definition expansion and `get_provider_proxy` stay)

Global-only `resolve_global_proxy`, then `.with_proxy(Some(p))?` only when `Some`, immediately after `ApiClient` construction and before header, query, and request-builder decorators:

- `crates/openduck/src/providers/avian.rs`
- `crates/openduck/src/providers/azure.rs`
- `crates/openduck/src/providers/litellm.rs`
- `crates/openduck/src/providers/nanogpt.rs` (`build_client`, before `x-client`)
- `crates/openduck/src/providers/githubcopilot.rs` (`authenticated_api_client` only)
- `crates/openduck/src/providers/huggingface.rs` (custom config and `from_env`)
- `crates/openduck/src/providers/openrouter.rs` (`from_env` only)
- `crates/openduck/src/providers/ollama_cloud.rs`
- `crates/openduck/src/providers/tetrate.rs`
- `crates/openduck/src/providers/xai.rs`
- `crates/openduck/src/providers/xai_oauth.rs` (`from_env` only)
- `crates/openduck/src/dictation/providers.rs` (`build_api_client` and `transcribe_with_model`)

`get_provider_proxy`, the named `proxies` registry, SOCKS support, and the config schema were not changed. Provider-crate files and the raw reqwest sites assigned to the other agent were not edited. `cargo fmt` on `githubcopilot.rs` also reordered imports in that file; the raw `Client::builder()` block was left as the other agent wrote it.
