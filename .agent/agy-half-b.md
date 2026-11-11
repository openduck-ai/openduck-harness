# HALF B task: `Config::get_proxy` accessor + module registration + integration tests

Repo: **/mnt/e/openduck-harness** (Rust workspace, crates prefixed `openduck-`).
Authoritative design: `.agent/design-subtask2-proxy-config.md` — read it FIRST
(sections "Ownership and exact public boundary", "Config integration (Half B)",
"Required test matrix", "Migration and merge behavior").

## Context
A parallel agent (Half A) is creating `crates/openduck/src/config/proxy.rs` with
this EXACT public API — you may rely on it and must NOT create or edit that file:

```rust
pub struct ProxyUrl { /* private */ }
impl ProxyUrl {
    pub fn as_str(&self) -> &str;
    pub fn url_with_credentials(&self) -> &str;
    pub fn host(&self) -> &str;
    pub fn port(&self) -> Option<u16>;
}
pub fn parse_proxy_url(key: &str, raw: &str) -> Result<Option<ProxyUrl>, crate::config::ConfigError>;
pub fn redact_proxy_url(raw: &str) -> String;
```
`ProxyUrl` implements `Clone`, `Display` (redacted), custom `Debug` (`ProxyUrl(<safe form>)`).

## Your scope — exactly three edits

### 1. Register the module
`crates/openduck/src/config/mod.rs`: add `pub mod proxy;` in the alphabetical
position among the other `pub mod` declarations (between `pub mod permission;`
and `pub mod providers;`). Do not add root re-exports.

### 2. Add the accessor to `crates/openduck/src/config/base.rs`
Inside `impl Config`, add exactly:

```rust
pub fn get_proxy(&self) -> Result<Option<crate::config::proxy::ProxyUrl>, ConfigError>
```

Semantics (from the design doc, follow precisely):
- Call `self.load()` ONCE; inspect the merged mapping's exact top-level `proxy` key.
  File-only across configured layers; later files win. This is NOT env-aware.
- NEVER call `get_param`, env helpers, secret storage, or `file_string`.
  No OPENDUCK_PROXY / GOOSE_PROXY / PROXY / HTTP_PROXY / HTTPS_PROXY / ALL_PROXY fallback.
- Absent key or YAML `Value::Null` => `Ok(None)`.
- `Value::String` => `parse_proxy_url("proxy", value)` (this handles trimmed-empty and
  literal `null` => `Ok(None)`).
- Any other YAML type (number, bool, sequence, mapping, tagged) =>
  `Err(ConfigError::DeserializeError(format!("Invalid config key `proxy` value `{}`: expected a string proxy URL or null", <redacted>)))`
  where `<redacted>` comes from `crate::config::proxy::redact_proxy_url`, applied to the
  offending value's `to_string()` form — never serialize/reprint the raw YAML value.
  If you cannot redact it meaningfully, use `<redacted>`.
- Any `self.load()` error must be mapped to
  `Err(ConfigError::DeserializeError("Unable to read config key `proxy`: configuration could not be loaded".to_string()))`
  WITHOUT attaching or logging the source error (YAML parser diagnostics could echo
  credential-bearing source text).
- Add a short doc comment stating the key name, accepted forms (`http://`/`https://`,
  optional userinfo/port), that empty string or `null` disables it, and that env proxy
  vars are NOT consulted by this accessor. Do not restate implementation details.
- Do NOT add a setter. Do NOT log the value at any level.

### 3. Integration tests: create `crates/openduck/tests/config_proxy_tests.rs`
Match the repo's existing integration-test style — inspect e.g.
`crates/openduck/tests/provider_model_selection_test.rs` for how configs are built
is is fine; the practical constructor is
`Config::new_with_config_paths(vec![path], secrets_path)` with `tempfile::NamedTempFile`.
Rows you own (from the design test matrix):
- `unset` — absent key (empty YAML file) => `get_proxy()` is `Ok(None)`.
- `empty_string` — `proxy: ""` and `proxy: "   "` => `Ok(None)`.
- `null` — `proxy: null` (YAML null) => `Ok(None)`; string `"null"` => `Ok(None)`;
  string `"NULL"` => `Err`.
- `valid_http` — `proxy: http://127.0.0.1:3128` => host `127.0.0.1`, port `Some(3128)`,
  `as_str()` canonical `http://127.0.0.1:3128/`.
- `valid_https_credentials` — `proxy: https://alice:s3cr3t-marker@host.example:8443` =>
  `Ok(Some(_))`, `Display`/`Debug` do NOT contain `alice` or `s3cr3t-marker`,
  `url_with_credentials()` DOES contain them.
- `invalid_scheme` — `socks5://host:1080` and `file:///tmp/x` => `Err` whose message
  contains the substring `proxy` and `expected an http:// or https:// proxy URL`.
- `missing_host` — `http://`, `http:///path`, `http://user:pass@` => `Err`.
- `missing_scheme` — `127.0.0.1:3128` => `Err`.
- `invalid_port` — `http://host:bad`, `http://host:65536`, `http://host:-1`,
  `http://host:` => `Err`; `http://host:0` and `http://host:65535` => `Ok(Some(_))`.
- `credential_redaction_error` — invalid credential-bearing values, e.g.
  `https://alice:s3cr3t-marker@host:bad` and `http://alice:s3cr3t-marker@`,
  produce `Err` whose `to_string()` contains neither `alice` nor `s3cr3t-marker`,
  and still names the key `proxy`.
- `non_string_type` — `proxy: 3128` (YAML int) and `proxy: [a, b]` => `Err` containing
  `Invalid config key \`proxy\`` and `expected a string proxy URL or null`.
- `layered_precedence` — two config paths where the later file sets
  `proxy: http://later.example:8080` and the earlier sets `http://earlier.example:8080`;
  `get_proxy()` reflects the later value (proves file-only layered resolution).
- `env_is_ignored` — set env `OPENDUCK_PROXY=http://env.example:9999` and
  `HTTP_PROXY=http://envproxy.example:9999` (guard with the repo's env-lock helper if
  one is used in tests; otherwise `std::env::set_var` carefully in a single-threaded
  test is acceptable, but prefer reusing `env_lock` if it is accessible) with a config
  that has NO `proxy` key => `Ok(None)`. Clean up afterwards.

Use synthetic credentials only; assert on plain string containment.

## HARD CONSTRAINTS
- Do NOT edit `crates/openduck/src/config/proxy.rs` (owned by Half A), docs, or any
  file other than `mod.rs`, `base.rs`, and the new tests file.
- No new dependencies. Use `tempfile` (already a dev-dependency of `openduck`).
- No comments restating code. Run `rustfmt --edition 2021` on the files you touched.
- Verification: `cargo check -p openduck` and
  `cargo test -p openduck --test config_proxy_tests`. If `proxy.rs` does not exist
  yet (Half A still working), report that plainly instead of creating it yourself.

Report back: files changed, the exact accessor signature + doc comment, test names
and `cargo test` result lines, and any deviation with justification.
