# Task: Configure HTTP/HTTPS proxy for TUI agent via agent config file

Repo: **/mnt/e/openduck-harness**. Design docs: `.agent/design-subtask2-proxy-config.md`,
`.agent/design-subtask3-proxy-client.md`. Recon: `.agent/recon-tui-agent-proxy.md`.

## Progress log
- [x] Subtask 1: recon → `.agent/recon-tui-agent-proxy.md`
- [x] Subtask 2: config key + validation
      `crates/openduck/src/config/proxy.rs` (new, ProxyUrl + parse_proxy_url + redact_proxy_url),
      `Config::get_proxy()` in `crates/openduck/src/config/base.rs`,
      `crates/openduck/tests/config_proxy_tests.rs` (16 tests)
- [x] Subtask 3: wire proxy into transport
      `crates/openduck/src/providers/proxy.rs` (new: resolve_global_proxy / resolve_outbound_proxy / apply_proxy)
      + ~39 provider files; `crates/openduck/tests/provider_proxy_tests.rs` (7 tests),
      `crates/openduck-providers/tests/api_client_proxy_tests.rs` (3 tests)
- [ ] Subtask 4: DOCS + full verification  ← CURRENT

## Subtask 4 plan (this run)
1. Docs (owner: **grok**):
   - `documentation/docs/guides/config-files.md`: add `proxy` to "Configuration Sections" bullet list,
     a dedicated subsection under section 1 explaining semantics (unset/empty/null = no explicit proxy
     → existing provider resolution / HTTP(S)_PROXY env fallback; http/https only, optional user:pass,
     credentials redacted in logs/errors; precedence over per-provider `proxies`/provider `proxy` keys),
     and add `proxy:` line to the comprehensive example config (in the network/proxy block).
   - `documentation/docs/guides/tui-sessions.md`: optional cross-reference note.
2. Tests (owner: **agy**):
   - Verify existing proxy tests already cover: unset / empty string / null, valid http+https w/ creds,
     invalid URL error naming key + value, credentials redacted, transport actually uses proxy
     (absolute-form request line through a loopback proxy), streaming request through proxy,
     env fallback preserved when unset.
   - Add any missing coverage, incl. a test asserting the documented example YAML
     (`proxy:` key from docs) parses and round-trips through `Config::get_proxy()`.
3. Orchestrator verification:
   - `cargo fmt --check`
   - `cargo test -p openduck --test config_proxy_tests --test provider_proxy_tests`
   - `cargo test -p openduck-providers --test api_client_proxy_tests`
   - `cargo build -p openduck -p openduck-providers`
   - `cargo clippy -p openduck -p openduck-providers --all-targets -- -D warnings`
   - `git diff --stat`

## Verified green baseline (earlier runs, same commit)
- build + fmt + clippy clean (only pre-existing openduck-cli multi-bin Cargo warning)
- config_proxy_tests 16 ✓, provider_proxy_tests 7 ✓, api_client_proxy_tests 3 ✓,
  openduck-providers --lib 144 ✓, google_def 5 ✓, declarative_providers 20 ✓
- KNOWN PRE-EXISTING FAILURES in `cargo test -p openduck --lib` (6–17, e.g. env-dependent);
  reproduced identically in a pristine HEAD worktree → NOT caused by this work.

## Key facts (do not re-derive)
- Config file key: top-level `proxy` in `~/.config/openduck/config.yaml` (dynamic YAML map, no serde struct).
- Accessor: `Config::get_proxy() -> Result<Option<ProxyUrl>, ConfigError>`; missing/empty/null/"null" → None.
- `ProxyUrl` Display/Debug print redacted URL (no userinfo).
- Precedence settled in subtask 3: global file `proxy` > existing per-provider/`proxies` resolution > reqwest env auto-proxy.
  `apply_proxy` never adds `.no_proxy()`, so unset ⇒ env fallback unchanged.
- Doc files that mention `harness.tui_sessions` proxy env (`tui-agent-mcp.md`, `tui-sessions.md`) describe the
  *subprocess env* surface — different from the new core `proxy` key; docs for the new key go in `config-files.md`.

## SUBTASK 4 PROGRESS (this run)

### DONE — docs (config-files.md)
Agent (agy-docs) added to `documentation/docs/guides/config-files.md` (+27 lines):
- "Configuration Sections" bullet: `**Outbound HTTP Proxy** ...`
- New `### Outbound HTTP Proxy (`proxy`)` subsection (before "Example: Comprehensive config.yaml")
  with YAML snippet (http/https/creds/disable forms), `| Value | Behavior |` table, precedence
  bullets, and a note distinguishing `harness.tui_sessions.<agent>.https_proxy` / `advisors.*.https_proxy`.
- Commented top-level `# proxy: "http://127.0.0.1:3128"` in the "# 4. Outbound Network & Advisor Proxies"
  block of the comprehensive example.
- Heading anchor (Docusaurus): `#outbound-http-proxy-proxy`.

### DONE — tests (agy-tests)
- `crates/openduck/tests/config_proxy_tests.rs`: +3 tests → **19 pass**
  `documented_example_config_parses`, `documented_disable_forms_yield_none`,
  `display_and_debug_redact_credentials` (existing `debug_redaction` also present).
- `crates/openduck/tests/provider_proxy_tests.rs`: +1 test → **8 pass**
  new `streaming_request_traverses_config_proxy` (streaming request via loopback proxy,
  absolute-form request line `POST http://proxy-target.invalid/responses HTTP/1.1`);
  `absent_proxy_still_honors_environment_fallback` extended to assert `client.proxy().is_none()`
  when config proxy is unset while HTTP_PROXY env is set and still traversed.
- `crates/openduck-providers/tests/api_client_proxy_tests.rs`: 3 pass (unchanged).

### VERIFIED (orchestrator, this run)
- `cargo fmt --check` → PASS (exit 0)
- `cargo test -p openduck --test config_proxy_tests --test provider_proxy_tests` → 19 pass / 8 pass, 0 failed
- `cargo test -p openduck-providers --test api_client_proxy_tests` → 3 pass, 0 failed

### REMAINING
- [ ] Optional cross-reference note in `documentation/docs/guides/tui-sessions.md` and
      `documentation/docs/mcp/tui-agent-mcp.md` (nice-to-have; config-files.md doc already satisfies
      the requirement "docs/example config mention the proxy key").
- [ ] `cargo build -p openduck -p openduck-providers` and
      `cargo clippy -p openduck -p openduck-providers --all-targets -- -D warnings`
- [ ] `git diff --stat`

### NOTE on TUI tooling
`tui_agent` / `tui_agent_start` often return "session not found" right after creation and the grok
composer stays in `working` state (never accepts the prompt). Workaround: use `tui_agent_prompt`
with `session_id` after a short delay, or launch `agy` (which reliably accepts prompts).
