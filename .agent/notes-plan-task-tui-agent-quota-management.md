# TUI agent quota management — subtask 4/4

## Scope and implementation
- Preserved prior work. TUI agent review (Codex) found no specific behavior regressions in the implementation; no agy quota CLI/provider reader exists, so startup selection intentionally consumes recorded snapshots only.
- `service.rs`: agy resolves model before quota admission; tests/flow make desired model first choice, then stable discovered catalog order. Models block only on fresh exhausted observations with known future reset; absent/stale/expired/unparsed/failed reads remain eligible. Lookup errors intentionally fail open (not exhaustion). Shared wildcard exhaustion still applies to all models. All known blocked returns explanatory error listing candidates. Prompt exhaustion persists against agy session active/current/configured model.
- `store.rs`: exact agent+model+account lookup; wildcard and model-specific observations are kept independent.
- Tests: 25 tui_management lib tests cover model order/fallback, no model sharing, failed/stale/reset/unknown reads, all blocked, prompt persistence and account scopes. Related integration test validates existing generic reset behavior.

## Verification (verified in this run)
- `cargo test -p openduck --lib tui_management:: --no-fail-fast`: 25 passed, 0 failed.
- `cargo test -p openduck --test tui_management_test test_quota_exhausted_blocks_agent_until_reset`: 1 passed.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.
- Agent reported clippy for openduck lib/lib tests exit 0 with warnings (large_enum_variant AgyQuotaRead; assertion_on_constants in earlier quota test), not errors. Full `cargo clippy --all-targets -- -D warnings` was not run.

## Known boundary
- No native agy quota reader/API has been identified; a failed or unavailable observation is never interpreted as exhausted, but therefore is allowed. The new feature switches only among models with persisted quota snapshots that are currently considered blocked/eligible. UI quota can inspect snapshots as implemented upstream.
- The additional isolated `catalog.rs` coding request was not implemented because the agy interactive agent reported its individual quota exhausted; catalog model ordering was implemented/tested in service helpers against `AgentModelCatalog::get_models`.
- There are no outstanding changes required within this subtask. Existing unrelated dirty changes from prior tasks remain untouched.
