# Task: add tests proving ApiClient routes ordinary AND streaming requests through the explicit proxy

Repo root: /mnt/e/openduck-harness

Create ONE new file: `crates/openduck-providers/tests/api_client_proxy_tests.rs` (new isolated test binary — must not mutate process env).

## What to prove
`ApiClient`'s shared transport really routes BOTH ordinary AND streaming requests through a configured explicit proxy
(i.e. the proxy survives `rebuild_client()` when headers are added).

## Fixture
- Bind a local `TcpListener` on `127.0.0.1:0` that acts as an HTTP proxy.
- Target origin is an unreachable host, e.g. `http://proxy-target.invalid`.
- A forwarding HTTP proxy receives an ABSOLUTE-form request line, e.g. `POST http://proxy-target.invalid/responses HTTP/1.1`.
- Read the request until the blank line terminating the headers; capture the request line and headers.
- Respond with a small `Content-Length` JSON body for the ordinary case.
- Respond with `content-type: text/event-stream` + `data: {...}\n\n` body for the streaming case.
- Wrap every accept/read in `tokio::time::timeout(Duration::from_secs(5), ..)` so regressions fail fast rather than hang.

## Tests (all `#[tokio::test]`)
1. `explicit_proxy_non_streaming_uses_absolute_form`
   - `ApiClient::new_with_tls(origin, AuthMethod::NoAuth, None).unwrap().with_proxy(Some(proxy_url)).unwrap()`
   - `client.response_post("responses", &json!({"n": 1]))`
   - assert the captured request line == `POST http://proxy-target.invalid/responses HTTP/1.1`, and the JSON response body parses.
2. `explicit_proxy_streaming_uses_absolute_form`
   - same client; `client.request("responses").streaming(true).response_post(&json!({"stream": true}))`
   - assert the same absolute-form request line reached the proxy, and reading the response body yields the SSE `data:` payload.
3. `proxy_survives_header_rebuild`
   - `.with_proxy(Some(..))` then `.with_header("x-test", "value").unwrap()` then `.with_header("x-test-2", "v2").unwrap()`
   - assert the request still arrives at the proxy and carries both headers (proves `rebuild_client()` retains the proxy).

## Constraints
- Imports: `openduck_providers::api_client::{ApiClient, AuthMethod}`.
- Deps already available: tokio (macros, rt-multi-thread, time, net, io-util), serde_json, reqwest. Do NOT add new dependencies.
- No comments restating code. Never log the proxy URL or credentials.
- Check the real signature of `ApiClient::new_with_tls`, `with_proxy`, `with_header`, `response_post`, `request(...).streaming(...).response_post(...)` in
  `crates/openduck-providers/src/api_client.rs` before writing, and match them exactly.

## Verify
```
cargo test -p openduck-providers --features rustls-tls --test api_client_proxy_tests
cargo fmt
```
All 3 tests must pass. Reply with the test output summary (pass/fail counts).
