# Subtask 2: top-level proxy configuration contract

Scope: design for optional `proxy` in the core dynamic YAML mapping. Default is
no proxy. No reqwest/client wiring, environment resolution, docs, or changes to
named `proxies`. This contract itself changes no Rust source.

## Ownership and exact public boundary

Half A owns `crates/openduck/src/config/proxy.rs` and its module unit tests.
Half B owns `base.rs`, registration in `mod.rs`, and integration tests in
`crates/openduck/tests/config_proxy_tests.rs`. No new dependencies are needed.
Half B adds `pub mod proxy;`; no root re-exports are required. Both halves use
`crate::config::proxy::{ProxyUrl, parse_proxy_url}` (integration tests use
`openduck::config::proxy`). The complete public module API is:

```rust
pub struct ProxyUrl { /* private: url: url::Url */ }
impl ProxyUrl {
    pub fn as_str(&self) -> &str;
    pub fn url_with_credentials(&self) -> &str;
    pub fn host(&self) -> &str;
    pub fn port(&self) -> Option<u16>;
}
pub fn parse_proxy_url(key: &str, raw: &str)
    -> Result<Option<ProxyUrl>, crate::config::ConfigError>;
pub fn redact_proxy_url(raw: &str) -> String;
```

`ProxyUrl` implements `Clone`, custom `Debug`, and custom `Display`; no public
constructor, fields, setters, URL accessor, serde implementation, or Deref.
Only the parser can construct it. `as_str()` and `url_with_credentials()` are
identical explicit access to the canonical `Url::as_str()` INCLUDING credentials;
never use either in diagnostics. Canonicalization may lowercase scheme/host,
add `/`, percent-encode userinfo, or remove a default port. `host()` returns
`Url::host_str().unwrap()` (IPv6 retains URL crate brackets); `port()` returns
`Url::port()` (explicit default ports normalize to None), not inferred defaults.

## Parsing and validation (Half A)

1. Trim outer whitespace with `str::trim`. Empty or exactly lowercase literal
   `null` returns `Ok(None)`. `NULL` and `Null` as strings are invalid URLs.
2. Require a literal `://` separator and an ASCII case-insensitive `http` or
   `https` scheme. Require a nonempty original authority, ending at the first
   `/`, `?`, or `#` after `://`. Reject backslashes and interior whitespace or
   control characters anywhere before invoking `Url::parse`.
3. Validate the ORIGINAL authority before URL normalization: after removing
   userinfo through the last `@`, require a nonempty host. Reject empty host,
   empty port (`host:`), unbracketed IPv6, and trailing garbage after a closing
   IPv6 bracket. A port, if present, consists only of ASCII digits and parses
   as u16 (0 through 65535 inclusive). Bracketed IPv6 must parse via `url`.
   This prevents `url` repairing `http:///path` or `http://` into acceptance.
4. Parse using `url::Url`; require scheme http/https and `host_str()` nonempty.
   Any parse failure becomes the safe error below; never forward parser errors.
5. Accept absent path or root `/` only. Reject other paths, including encoded
   paths; check original suffix as well as normalized path so `/a/..` cannot
   bypass rejection. Reject any query or fragment, including empty `?`/`#`.
   Proxy endpoints identify an origin; accepting ignored routing components
   would mislead users about what a future HTTP client actually uses.
6. Allow username-only userinfo (`http://alice@host`), user:pass, and empty
   user/password components accepted by `url`. Username-only is a useful
   conventional credential form and does not weaken endpoint validation.
   Strip the ENTIRE userinfo from every diagnostic, including usernames.

Missing scheme (`127.0.0.1:3128`), socks5/file, invalid host, and invalid port
all fail. No DNS lookup or network calls. Credentials remain available solely
through the explicit credential-bearing accessors for a later subtask.

## Redaction and errors (Half A)

`redact_proxy_url` always returns a safe diagnostic string, never the raw input.
Trim first; empty returns `<empty>`; exact `null` returns `null`.
For every other value, use the following deterministic conservative algorithm:

- Recognize only `scheme://authority`, where scheme matches ASCII
  `[A-Za-z][A-Za-z0-9+.-]*`; reject whitespace, controls, and backslashes.
- Take authority before the first `/`, `?`, or `#`; strip userinfo through the
  LAST `@` before attempting host/port validation. Never emit userinfo or a
  replacement containing a username. Drop the entire path, query, and fragment.
- Parse `http://` plus this stripped authority plus `/` as a diagnostic candidate.
  Require a nonempty host and the same original host/port syntax checks above.
  This also supports redacting unsupported schemes such as socks5/file when
  they have a recognizable authority. Do not normalize away an invalid port.
- If valid, return lowercase original scheme plus `://` plus the candidate's
  canonical host (brackets for IPv6) and non-default explicit port. No final `/`.
  If anything is unrecognizable or invalid, return exactly `<redacted>`.

Thus parseable `https://alice:secret@host:8443/a?q=secret#secret` becomes
`https://host:8443`; unparseable `http://a:b@c:d@host:bad` becomes `<redacted>`;
`http://a:b@c:d@host` becomes `http://host`. Missing-scheme input is fully
redacted. Do not return partially sanitized raw text on any failure path.

All URL validation failures use `ConfigError::DeserializeError` with payload:

```rust
format!("Invalid config key `{key}` value `{}`: expected an http:// or https:// proxy URL with a host, optional port (0..=65535) and userinfo, root path only, and no query or fragment; use empty string or null to disable", redact_proxy_url(raw))
```

`key` is a trusted caller-supplied config-key identifier, never raw user input;
Half B always passes `"proxy"`. The surrounding ConfigError Display prefix is
unchanged. Do not append parser/source errors, original YAML, or raw input.
`Display` writes only `redact_proxy_url(self.as_str())`.
`Debug` writes exactly `ProxyUrl(<safe form>)`, e.g. `ProxyUrl(https://host:8443)`.
Do not derive Debug for this type or log raw values at any level.

## Config integration (Half B)

Required method (no setter in this subtask; avoid an unnecessary write API):

```rust
impl Config {
    pub fn get_proxy(&self) -> Result<Option<crate::config::proxy::ProxyUrl>, ConfigError>;
}
```

Read `self.load()` once and inspect the merged mapping's exact top-level
`proxy` key. This is file-only across configured layers; later files win.
Never call `get_param`, environment helpers, secret storage, or `file_string`
(the latter loses load/type errors). No OPENDUCK_PROXY, GOOSE_PROXY, PROXY,
HTTP_PROXY, HTTPS_PROXY, or ALL_PROXY fallback, even when the key is absent.

- Absent key or YAML `Value::Null`: `Ok(None)`.
- String: `parse_proxy_url("proxy", value)`; includes trimmed empty and `null`.
- Any other YAML type, including tagged values: `DeserializeError` payload
  exactly `Invalid config key `proxy` value `<redacted>`: expected a string proxy URL or null`.
  Do not serialize/reprint the offending YAML value.
- Load errors must not be swallowed. To prevent YAML parser diagnostics exposing
  credential-bearing source snippets, map any `load()` error at this boundary
  to `DeserializeError` payload exactly
  `Unable to read config key `proxy`: configuration could not be loaded`.
  Do not attach/log its source error. This intentionally sacrifices detailed
  loader diagnostics for this accessor; other config APIs are unchanged.

No startup validation hook in this subtask: validation occurs on `get_proxy()`.
A malformed value must return Err, never silently behave as disabled.

## Required test matrix

A = module unit tests; B = integration tests using isolated temporary configs.
No real credentials or network access. Assertions check ConfigError payloads
as well as absence of synthetic credential markers in Display/Debug.

| Owner / test name | Assertion |
|---|---|
| B / unset | Missing file and absent key both yield None. |
| A+B / empty_string | Empty/whitespace string yields None. |
| A+B / null | YAML null, `proxy:` and string `null` yield None; string `NULL` fails. |
| A+B / valid_http | `http://127.0.0.1:3128` exposes host, port and canonical URL. |
| A+B / valid_https_credentials | `https://alice:secret@host:8443` preserves credentials only in explicit accessors. |
| A / scheme_case | `HtTpS://host` succeeds with canonical lowercase scheme. |
| A+B / invalid_scheme | socks5 and file fail with expected key/expectation. |
| A+B / missing_host | `http://`, `http:///path`, `http://user:pass@` fail. |
| A+B / missing_scheme | `127.0.0.1:3128` fails with `<redacted>`. |
| A+B / invalid_port | `:bad`, `:65536`, `:-1`, empty `:` fail; ports 0 and 65535 succeed. |
| A / endpoint_components | Root succeeds; `/a`, `/a/..`, `/%61`, `?`, `#` fail. |
| A / username_only | `http://alice@host` succeeds; diagnostics omit alice. |
| A / ipv6_default_port | Bracketed IPv6 succeeds; unbracketed fails; :80 on http yields port None. |
| A+B / credential_redaction_error | Invalid credential-bearing inputs never expose username/password in error payload or Display. |
| A / redaction_fallbacks | Assert every example above, last-@ handling, missing scheme, controls, and query/fragment secrets. |
| A+B / debug_redaction | Exact Debug/Display forms contain origin only, no username/password. |
| B / wrong_yaml_type | Number, boolean, sequence, mapping and tagged value fail with fixed safe type error. |
| B / broken_yaml | Malformed credential-bearing YAML yields fixed load error without source snippet. |
| B / env_ignored | Set all env names above to invalid credential-bearing values: absent stays None; valid file wins. |
| B / layered_disable | Later empty string/null overrides earlier URL; later URL overrides earlier URL. |

Environment tests must use repository environment-isolation conventions and
restore variables; serialize mutation to avoid races. Module tests do not depend
on Config loading. Half B relies only on the public API listed above.

## Migration and merge behavior

No migration or schema/default insertion is needed in `migrations.rs`.
Existing files lacking `proxy` retain behavior. Do not rename/copy `proxies`,
provider proxy settings, or environment values into this key. Existing provider
and platform-extension migrations must preserve this unrelated mapping entry.
`merge_config_values` already replaces ordinary top-level values, including
null and empty strings; no special handling or merge change is needed. Layered
integration tests establish the disabling behavior. No agent-loop changes.
