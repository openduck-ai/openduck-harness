# Task A-TESTS (agy): make the pre-existing Rust test suite green under the new decoupled semantics

Repo `/mnt/e/openduck-harness`, crate `openduck`. Design doc: `design-docs/provider-model-decoupling-plan.md`
(read sections 3.2, 3.3, 3.5 first). Do NOT change production behaviour to satisfy old tests unless the
item below explicitly says so — most of these are *intended* behaviour changes and the assertions are stale.

You own ONLY these files:
- crates/openduck/src/config/migrations.rs   (TEST MODULE ONLY, `#[cfg(test)] mod tests`)
- crates/openduck/src/config/base.rs          (production: `set_goose_provider` only)
- crates/openduck/src/acp/mod.rs              (TEST MODULE ONLY)
- crates/openduck/src/control/harness.rs      (TEST MODULE: `test_missing_secondary_entry_model_is_config_error_not_default`)
- crates/openduck/src/model_config.rs         (TEST MODULE ONLY, `mod fast_model_resolution_tests`)

Other agents are editing other files concurrently. Do not touch anything else. Do not run `cargo fmt --all`
(only `cargo fmt -p openduck` is fine); do not run `git checkout`/`git stash`.

## Item 1 — production fix: `Config::set_goose_provider` must not require a pre-existing entry
`crates/openduck/src/config/base.rs` currently:
```rust
pub fn set_goose_provider(&self, v: impl Into<String>) -> Result<(), ConfigError> {
    let name = v.into();
    crate::config::providers::validate_selection(self, Some(&name), None)?;
    crate::config::providers::save_role_selection(self, Role::Major, &name, None)
}
```
`save_role_selection` already creates/updates the provider entry (`enabled = configured = true`) and rejects
empty names, so the `validate_selection` call is redundant here and BREAKS onboarding flows that select a
provider before an entry exists: `crates/openduck-cli/src/commands/configure.rs:2293` (custom provider just
added), `crates/openduck/src/config/signup_openrouter/mod.rs:168`, `crates/openduck/src/config/signup_tetrate/mod.rs:165`,
`crates/openduck/src/acp/server/onboarding.rs:333`, `crates/openduck/src/doctor.rs:145`.
Fix: drop the `validate_selection` line from `set_goose_provider` and keep the role save. Do NOT weaken
`validate_selection` itself (HTTP/harness rely on strict existence+enabled checks).

## Item 2 — `crates/openduck/src/acp/mod.rs` tests (currently FAIL)
`acp::tests::configured_model_is_not_reused_for_another_provider` and
`acp::tests::configured_model_is_used_for_the_active_provider` panic because `set_goose_provider` errors.
After Item 1 they should pass unchanged — verify; only adjust the test if a genuine semantic change remains.
Both tests must express INTENT: "the configured major model is used only for the provider it belongs to".

## Item 3 — `control::harness::tests::test_missing_secondary_entry_model_is_config_error_not_default` (FAILS)
`resolve_secondary_target` now goes through the unified resolver, whose error text is
`No model available for secondary provider 'anthropic'` (`crates/openduck/src/config/providers.rs:241`).
Update the assertion in the test (harness.rs ~5196) to the new message. Keep the test's intent
(missing model for a secondary provider is a Config error, not a silent default).

## Item 4 — migration tests whose expectations are stale (7 FAIL)
The migration now writes role models to top-level `active_model` / `secondary_provider_model` and no longer
populates `ProviderEntry.model` from `GOOSE_MODEL`, and it no longer invents a `providers.<secondary>` entry.
Rewrite the assertions to the new contract (this is intended behaviour, see design 3.3 items 1-9):
- `test_migrate_provider_config_basic` (migrations.rs:559) — assert top-level `active_model == "current"`
  and `providers["claude-acp"].model == ""` instead of `entry.model == "current"`.
- `test_migrate_provider_config_multiple_configured` (:606) — same pattern.
- `test_migrate_secondary_provider_key` (:824) — the secondary provider is NOT added to `providers`;
  assert `secondary_provider == "anthropic"` and `secondary_provider_model == "claude-sonnet"`, then only
  assert on a provider entry that actually exists.
- `test_migrate_secondary_model_fills_empty_entry_model` (:899) — rewrite to assert the top-level
  `secondary_provider_model` key is written; rename the test if the old name is now misleading.
- `test_migrate_does_not_copy_secondary_model_key_into_entry_secondary_model` (:964) — the `providers["anthropic"]`
  unwrap panics; assert instead that `providers["anthropic"].secondary_model` stays `None` when the entry exists,
  or that the legacy key is not copied into any provider entry.
- `test_migrate_secondary_equal_to_active_is_not_copied` (:855) — update to the new contract.
- `test_second_migrate_does_not_restore_cleared_secondary_entry_model` (:1158) — update to the new contract
  (a second migration pass must not resurrect a cleared role model).
Where a test name no longer matches its assertions, rename it. Keep coverage of the same behaviour.

## Item 5 — `model_config::fast_model_resolution_tests` are order-dependent (2 FAIL in full run, pass alone)
That module defines its own `ENV_MUTEX` + `EnvGuard`, while other modules use `env_lock::lock_env`, so
`control::app_config::tests::test_provider_put_env_lock_compares_to_file_not_env` (which sets
`OPENDUCK_FAST_MODEL=gpt-4o-mini`) leaks into these tests when run in the same process.
Fix: use `env_lock::lock_env((key, value))` for every env mutation in this module (delete the local
`ENV_MUTEX`/`EnvGuard`), matching the pattern in `crates/openduck/src/acp/server/schedule.rs:383` and
`crates/openduck/src/control/app_config.rs:2641`. Note `lock_env` is only imported in test modules via
`env_lock::lock_env(...)` fully-qualified — check `crates/openduck/Cargo.toml` dev-dependencies already list
`env-lock` (it does, as `env-lock` -> crate `env_lock`). Do not add dependencies.

## Verify (must all pass before you report)
```bash
cd /mnt/e/openduck-harness && . bin/activate-hermit
cargo fmt -p openduck
cargo test -p openduck --lib config
cargo test -p openduck --lib acp::tests
cargo test -p openduck --lib model_config
cargo test -p openduck --lib control::harness
cargo test -p openduck --test provider_model_selection_test
cargo test -p openduck --test provider_config_api_test
```
Report: files changed, what each stale assertion was changed to and why, and the pass/fail counts.
