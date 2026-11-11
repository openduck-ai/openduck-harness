# Task: two proxy-precedence unit tests (+ tiny testability refactor)

Repo: /mnt/e/openduck-harness. Touch EXACTLY two files, both in the core crate:
- `crates/openduck/src/providers/google_def.rs`
- `crates/openduck/src/config/declarative_providers.rs`

Another agent is editing `crates/openduck-providers/src/azure_foundry.rs` concurrently — do not touch it or any other file.

## Background (already implemented, do not redo)
- Subtask 2 added a top-level `proxy` YAML key: `Config::get_proxy(&self) -> Result<Option<ProxyUrl>, ConfigError>`
  in `crates/openduck/src/config/base.rs` (~line 538).
- `crates/openduck/src/providers/proxy.rs` exposes:
  ```rust
  pub fn resolve_global_proxy(config: &crate::config::Config) -> anyhow::Result<Option<String>>;
  ```
  Returns the credential-bearing URL string (trailing `/` normalized by `Url`) or `None`.
- Both target call sites read `Config::global()` internally, which makes them untestable with an isolated temp config.
  Fix that with the minimal refactors below, preserving behavior exactly.

## A. `crates/openduck/src/providers/google_def.rs`

### A1. Extract the chain (currently inline at ~lines 57-64 inside `from_env`)
Add, next to `from_env`:
```rust
fn resolve_proxy(config: &Config) -> Result<Option<String>> {
    let proxy = crate::providers::proxy::resolve_global_proxy(config)?.or_else(|| {
        crate::config::providers::get_provider_proxy(config, "google")
            .or_else(|| crate::config::providers::get_provider_proxy(config, "gemini"))
            .or_else(|| config.get_param("GOOGLE_PROXY").ok())
            .or_else(|| config.get_param("GEMINI_PROXY").ok())
    });
    Ok(proxy)
}
```
`from_env` keeps `let config = Config::global();` and replaces the inline chain with `let proxy = resolve_proxy(config)?;`.
Precedence stays: global `proxy` key first, then google → gemini → GOOGLE_PROXY → GEMINI_PROXY.

### A2. Tests, appended inside the existing `mod tests` in that file
Add a helper mirroring `crates/openduck/tests/provider_proxy_tests.rs`:
```rust
fn yaml_config(yaml: &str) -> (Config, tempfile::NamedTempFile, tempfile::NamedTempFile) {
    let config_file = NamedTempFile::new().unwrap();
    let secrets_file = NamedTempFile::new().unwrap();
    std::fs::write(config_file.path(), yaml).unwrap();
    let config = Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap();
    (config, config_file, secrets_file)
}
```
(keep the temp files alive by binding them). Then these two `#[test]`s:

1. `global_proxy_beats_google_and_gemini_env`
   - `let _guard = env_lock::lock_env([("OPENDUCK_PROVIDER_GOOGLE_PROXY", Some("http://provider-env.example:9")), ("GOOGLE_PROXY", Some("http://legacy-env.example:9"))]);`
   - config YAML: `proxy: "http://global.example:9"\n`
   - `assert_eq!(resolve_proxy(&config).unwrap().as_deref(), Some("http://global.example:9/"));`
     (note the normalized trailing slash added by `Url`)
2. `absent_global_proxy_falls_back_to_google_env`
   - same env guard
   - config YAML: `""`
   - `assert_eq!(resolve_proxy(&config).unwrap().as_deref(), Some("http://provider-env.example:9"));`

## B. `crates/openduck/src/config/declarative_providers.rs`

### B1. Refactor (current block is at ~lines 415-422 inside `resolve_config`)
Extract the proxy precedence into a free function so it can be exercised with an isolated config:
```rust
fn apply_proxy_precedence(
    config: &mut DeclarativeProviderConfig,
    global_config: &Config,
) -> Result<()> {
    if let Some(proxy) = crate::providers::proxy::resolve_global_proxy(global_config)? {
        config.proxy = Some(proxy);
    } else if config.proxy.is_none() {
        if let Some(p) = crate::config::providers::get_provider_proxy(global_config, &config.name) {
            config.proxy = Some(p);
        }
    }
    Ok(())
}
```
`resolve_config` then calls `apply_proxy_precedence(config, global_config)?;` in place of the inline block. No other
semantic change.

### B2. Tests, appended inside the existing `mod tests` (starts ~line 559)
Reuse the existing `test_huggingface_config()` helper if it gives a fully populated struct — read it first; otherwise
construct the minimal `DeclarativeProviderConfig` you need (it derives/constructs fine with struct literal syntax; all
fields are `pub`, see `crates/openduck-providers/src/declarative.rs:116-161`). Set `.name` and `.proxy` per test, then:

1. `global_proxy_beats_declarative_proxy`
   - `let (global, _f1, _f2) = yaml_config("proxy: \"http://global.example:9\"\n");`
   - provider config `.proxy = Some("http://declarative.example:9".to_string())`
   - `apply_proxy_precedence(&mut cfg, &global).unwrap();`
   - `assert_eq!(cfg.proxy.as_deref(), Some("http://global.example:9/"));`
2. `absent_global_preserves_declarative_proxy`
   - `yaml_config("")`
   - provider config `.proxy = Some("/relative-not-touched".to_string())` — actually use a plain string like
     `"http://declarative.example:9"` (the function must leave it byte-for-byte unchanged)
   - `apply_proxy_precedence(&mut cfg, &global).unwrap();`
   - `assert_eq!(cfg.proxy.as_deref(), Some("http://declarative.example:9"));`

Also lock env in both B tests so ambient provider proxy env vars can't win:
`env_lock::lock_env([("OPENDUCK_PROVIDER_TESTDECL_PROXY", None::<&str>), ("GOOSE_PROVIDER_TESTDECL_PROXY", None::<&str>), ("OPENDUCK_TESTDECL_PROXY", None::<&str>), ("GOOSE_TESTDECL_PROXY", None::<&str>), ("TESTDECL_PROXY", None::<&str>)])`
and set the provider config `.name = "testdecl"`.

Add the same `yaml_config` helper in this file's test module if not already present.

## Verify
```
cargo test -p openduck --features rustls-tls --lib -- providers::google_def config::declarative_providers
cargo clippy -p openduck --lib --features rustls-tls -- -D warnings
cargo fmt
```
All tests must pass and clippy must be clean (`--lib` only; `--all-targets` is being edited concurrently). If a test
cannot pass without changing production semantics, STOP and report instead of weakening the assertion.
Reply with test counts + clippy result.
