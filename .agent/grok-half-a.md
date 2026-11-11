# HALF A task: create `crates/openduck/src/config/proxy.rs`

Read `.agent/design-subtask2-proxy-config.md` first (authoritative contract).

Create ONE new file: `crates/openduck/src/config/proxy.rs`.

## 1. Type

```rust
pub struct ProxyUrl { url: url::Url }
```

Private field. Implements `Clone`, custom `Display` returning `redact_proxy_url(self.as_str())`,
custom `Debug` printing exactly `ProxyUrl(<safe form>)`. Methods:

- `pub fn as_str(&self) -> &str`
- `pub fn url_with_credentials(&self) -> &str`
- `pub fn host(&self) -> &str`  (`Url::host_str()`; IPv6 keeps brackets)
- `pub fn port(&self) -> Option<u16>` (`Url::port()`; explicit default port => None)

## 2. `pub fn parse_proxy_url(key: &str, raw: &str) -> Result<Option<ProxyUrl>, crate::config::ConfigError>`

- trim; empty string OR lowercase literal `null` => `Ok(None)`. String `NULL` must ERROR.
- require literal `://` and ASCII case-insensitive `http`/`https` scheme; require nonempty original authority.
- reject backslashes, interior whitespace, control chars before parsing.
- validate the ORIGINAL authority before normalization: strip userinfo through the LAST `@`;
  require nonempty host; reject empty port (`host:`), unbracketed IPv6,
  trailing garbage after a closing IPv6 bracket; a port must be ASCII digits only and parse as u16
  (0..=65535 accepted; `:65536`, `:-1`, `:bad` rejected).
- then `url::Url::parse`; require http/https scheme and nonempty `host_str()`.
- accept only absent path or root `/`; check BOTH the original suffix and the normalized path so
  `/a/..` cannot bypass rejection; reject ANY query or fragment including bare `?` and `#`.
- allow username-only userinfo (`http://alice@host`) and `user:pass`.
- any failure => `ConfigError::DeserializeError` with EXACTLY this payload:

```rust
format!("Invalid config key `{key}` value `{}`: expected an http:// or https:// proxy URL with a host, optional port (0..=65535) and userinfo, root path only, and no query or fragment; use empty string or null to disable", redact_proxy_url(raw))
```

Never surface parser errors, raw input, or credentials.

## 3. `pub fn redact_proxy_url(raw: &str) -> String`

Conservative; NEVER returns raw input on failure.

- trim; empty => `<empty>`; exact `null` => `null`
- recognise only `scheme://authority`; scheme must match ASCII `[A-Za-z][A-Za-z0-9+.-]*`
- authority ends at first `/`, `?`, `#`
- strip userinfo through the LAST `@`
- validate host/port; anything doubtful => `<redacted>`
- drop query/fragment

Required examples:
- `https://alice:secret@host:8443/a?q=secret#secret` => `https://host:8443`
- `http://a:b@c:d@host:bad` => `<redacted>`
- `http://a:b@c:d@host` => `http://host`
- schemeless input (`127.0.0.1:3128`) => `<redacted>`

## 4. `#[cfg(test)] mod tests` in the same file

Cover: empty_string; null (incl empty, string `null`, and `NULL` failing); valid_http;
valid_https_credentials; scheme_case (`HtTpS://host`); invalid_scheme (socks5 + file);
missing_host (`http://`, `http:///path`, `http://user:pass@`); missing_scheme;
invalid_port (`:bad`, `:65536`, `:-1`, empty `:`; 0 and 65535 succeed);
endpoint_components (root ok; `/a`, `/a/..`, `/%61`, `?`, `#` fail); username_only
(diagnostics omit the username); ipv6_default_port (bracketed IPv6 ok, unbracketed fails,
`:80` on http => port None); credential_redaction_error (synthetic credentials with a
unique marker string asserted ABSENT from the error payload, `Display`, and `Debug`).

## HARD CONSTRAINTS

- Do NOT edit any other file: not `base.rs`, not `crates/openduck/src/config/mod.rs`, not docs,
  not `tests/`. Another agent registers the module concurrently.
- `cargo check -p openduck` may report the file is not in any module tree — EXPECTED; report it,
  do not "fix" it by editing `mod.rs`.
- No new dependencies (`url` already present).
- No comments that restate code. Format: `rustfmt --edition 2021 crates/openduck/src/config/proxy.rs`.

Report: file path, exact public API, test count, cargo output tail, deviations with justification.
