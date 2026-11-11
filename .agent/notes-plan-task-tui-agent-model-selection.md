# Task scratch notes — TUI per-agent model selection checks (Subtask 5/5)

## Plan
1. Verify focused Rust/UI checks, formatting, diff stats, and builds.
2. Fix only regressions/fallout introduced by model-selection changes through coding agents.
3. Record remaining known unrelated lint blockers and final results.

## Verified results
- `cargo build` passed.
- `cargo fmt --all` passed and `cargo fmt --all -- --check` passed after Rust test expectations were updated and formatting applied.
- `git diff --check` passed; `git diff --stat` exits 0 (24 changed files; task changes plus pre-existing worktree content).
- `cargo test -p openduck-harness --test tui_session_config_tests`: 19 passed.
- `cargo test -p openduck-mcp --test model_catalog_test`: 35 passed.
- `cargo test -p openduck-mcp --lib tui_session::catalog::`: 3 passed.
- `cargo test -p openduck-mcp --test tui_session_tests`: 34 passed after Agy adjusted the outdated proxy test expectation to account for six proxy variables from the pre-existing session behavior; unproxied session still verifies isolation.
- `cargo test -p openduck --features rustls-tls --test tui_management_test --test tui_agent_harness_test`: both targets passed (management 9, harness 9).
- `cd ui/hub && pnpm test`: passed (303 tests).
- Behavior coverage: no configured model remains unknown/default; model settings/request precedence, static fallback and failed discovery/free-text model path, rejected switch leaves prior active model, all exercised in focused test suites above.

## Failed/blocked checks
- Workspace `cargo clippy --all-targets -- -D warnings` fails on unrelated `crates/openduck-harness/tests/project_harness_tests.rs:407` (`field_reassign_with_default`).
- Narrow implementation clippy `cargo clippy -p openduck-mcp -p openduck --lib -- -D warnings` fails at `crates/openduck/src/control/app_config.rs:1753` (`redundant_closure`, replace `.map(|map| tui_sessions_from_harness(map))` with `.map(tui_sessions_from_harness)`). An Agy agent was asked to make this minimal lint fix and rerun fmt/narrow clippy, but remains occupied with an unusually long CLI clippy/background job. No fix has been confirmed yet.
- Initial proxy test run failed because its expected env omitted HTTP/ALL proxy entries; fixed in test-only assertion, rerun passed.
- Attempt to run a multi-check chain without bash failed because shell `source` wasn't supported; reran with `bash -lc` successfully.

## Next steps
- Collect Agy result for app_config redundant closure and narrow clippy rerun. If agent does not complete, report this outstanding lint failure; do not modify source directly.
- Recheck `cargo fmt --all -- --check`, `git diff --check`, `git diff --stat` after any lint fix.

## Final follow-up
- Build: `cargo build` passed.
- Narrow implementation clippy: `cargo clippy -p openduck-mcp -p openduck --lib -- -D warnings` initially found a redundant-closure lint in app_config; changed to `.map(tui_sessions_from_harness)`. Rerun passed.
- Full workspace clippy with -D warnings remains blocked by unrelated `openduck-harness/tests/project_harness_tests.rs:407` field_reassign_with_default lint.
- Formatting check initially surfaced one formatting discrepancy in `screen.rs`; ran `cargo fmt --all`. Final `cargo fmt --all -- --check` and `git diff --check` pass.
- Full focused test chain and Hub tests pass as listed above. `git diff --stat` exits 0.
- No remaining implementation/check blockers identified. Existing many pre-existing worktree modifications were preserved.
