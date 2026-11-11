# Task: add two missing core-crate proxy-behavior tests

Repo: /mnt/e/openduck-harness. File to edit: `crates/openduck/tests/provider_proxy_tests.rs` (append two tests; do
not rewrite existing ones). Do NOT touch any `src/` file.

Read the file first. Reusable helpers already there:
- `const ORIGIN: &str = "http://proxy-target.invalid";`
- `const REQUEST_LINE: &str = "POST http://proxy-target.invalid/responses HTTP/1.1";`
- `fn isolated_config(yaml: &str) -> (Config, NamedTempFile, NamedTempFile)`
- `async fn absolute_form_request_line(listener: TcpListener) -> String` — accepts one connection, returns the
  request line, replies 200 JSON, closes.
Pattern to copy from `config_proxy_reaches_transport`: `TcpListener::bind("127.0.0.1:0")`, `tokio::spawn(capture)`,
then `tokio::time::timeout(Duration::from_secs(5), captured)` to read the result.

## Test A — `raw_client_config_proxy_reaches_transport` (`#[tokio::test]`)
Proves the raw-builder helper `openduck::providers::proxy::apply_proxy` actually configures the transport (this is the
path used by bedrock / kimicode / gcpvertexai / toolshim / chatgpt_codex / gemini_oauth), not merely that it returns a
builder.

1. Lock env so ambient proxies cannot mask a failure:
   `env_lock::lock_env([("HTTP_PROXY", Some("http://127.0.0.1:1")), ("HTTPS_PROXY", Some("http://127.0.0.1:1")), ("ALL_PROXY", Some("http://127.0.0.1:1")), ("http_proxy", Some("http://127.0.0.1:1")), ("https_proxy", Some("http://127.0.0.1:1")), ("all_proxy", Some("http://127.0.0.1:1")), ("NO_PROXY", None::<&str>), ("no_proxy", None::<&str>)])`
2. Bind listener, `let port = listener.local_addr().unwrap().port();`
3. `let (config, _config_file, _secrets_file) = isolated_config(&format!("proxy: \"http://127.0.0.1:{port}\"\n"));`
4. `let proxy = proxy::resolve_global_proxy(&config).unwrap().expect("configured proxy");`
5. `let builder = proxy::apply_proxy(reqwest::Client::builder(), Some(proxy.as_str())).unwrap();` then
   `let client = builder.build().unwrap();`
6. spawn `absolute_form_request_line(listener)`; send
   `client.post(format!("{ORIGIN}/responses")).json(&serde_json::json!({"n": 1}))` `.send()` under a 5s timeout; assert
   captured request line == `REQUEST_LINE`.

## Test B — `absent_proxy_still_honors_environment_fallback` (`#[tokio::test]`)
Proves env proxy vars remain a *fallback* when config has no proxy (backward compatibility).

1. Bind listener first, capture its URL: `let proxy_uri = format!("http://{}", listener.local_addr().unwrap());`
2. `env_lock::lock_env([("HTTP_PROXY", Some(proxy_uri.as_str())), ("http_proxy", Some(proxy_uri.as_str())), ("HTTPS_PROXY", None::<&str>), ("https_proxy", None::<&str>), ("ALL_PROXY", None::<&str>), ("all_proxy", None::<&str>), ("NO_PROXY", None::<&str>), ("no_proxy", None::<&str>)])`
3. `let (config, _config_file, _secrets_file) = isolated_config("");`
   assert `proxy::resolve_global_proxy(&config).unwrap().is_none()`.
4. Build the client WITHOUT an explicit proxy:
   `ApiClient::new_with_tls(ORIGIN.to_string(), AuthMethod::NoAuth, None).unwrap()`, assert `client.proxy().is_none()`.
5. spawn capture, send `client.response_post("responses", &serde_json::json!({"n": 1}))` under timeout, assert the
   captured request line == `REQUEST_LINE` (i.e. it reached the env proxy in absolute form).

Note: keep the `env_lock` guard alive for the whole test (bind it to a named local, as the existing tests do).

## Constraints
- No new dependencies; `reqwest`, `serde_json`, `env_lock`, `tokio`, `tempfile` are already available to this target.
- Never log/print proxy URLs or credentials in assertion messages.
- No comments restating code.

## Verify
```
cargo test -p openduck --features rustls-tls --test provider_proxy_tests
cargo fmt
```
All 7 tests (5 existing + 2 new) must pass. If Test B is flaky or cannot pass because reqwest ignores HTTP_PROXY for
`http://` origins in this configuration, STOP and report the exact reason rather than weakening the assertion. Reply
with the pass/fail summary.
