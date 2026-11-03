# Task B1: config-layer decoupling fixes (agy)

Repo: /mnt/e/openduck-harness, crate `openduck`. Implement design-docs/provider-model-decoupling-plan.md
phases 1-2 config semantics. `crates/openduck/tests/provider_model_selection_test.rs` currently fails
9 of 18 tests. Make it green and keep the unified resolver as the single source of truth.

## Read first
1. design-docs/provider-model-decoupling-plan.md (sections 3.x on role keys, resolution precedence,
   configured-vs-model separation, same-provider roles, normalize).
2. .agent/notes-plan-provider-model-decouple-full.md (root causes RC1..RC3, G4..G7, A1.7, test seeding).

## Files you own
- crates/openduck/src/config/providers.rs
- crates/openduck/src/config/base.rs
- crates/openduck/src/config/migrations.rs (only if strictly needed)
- crates/openduck/tests/provider_model_selection_test.rs (test seeding corrections only)

Do NOT edit crates/openduck/src/control/*, crates/openduck/src/acp/*, crates/openduck/src/providers/*,
or crates/openduck/tests/provider_config_api_test.rs — another agent owns those files right now.

## Fixes

### FIX 1 (critical) — save_role_selection destroys the providers map
In `save_role_selection` (providers.rs ~316-355) the code builds
    serde_yaml::to_value(indexmap::IndexMap::from([(provider.to_string(), entry)]))
and pushes it under PROVIDERS_CONFIG_KEY. `Config::update_role_selection` (base.rs ~939) REPLACES
that whole key, so every other provider entry disappears.
Change to: read `let mut map = get_providers_map(config);`, insert/update only this provider
(enabled = true, configured = true, other fields preserved), serialize the FULL map, then submit.
Verified symptom: config with ["openai","anthropic"] becomes ["openai"] after one role save.

### FIX 2 — set_active_provider must accept an empty model
`set_active_provider(config, name, model)` currently forwards `Some(model)`, so
`set_active_provider(&c, "openai", "")` fails with `DeserializeError("Major model cannot be blank")`.
Map blank/whitespace model to `None` there (empty means "no explicit role model"). Keep the strict
blank rejection INSIDE `save_role_selection` because the HTTP v2 contract rejects explicit blanks.
Update the lib unit test `test_set_active_provider_rejects_empty_model` (in the providers.rs tests
module) so it asserts `save_role_selection(..., Some(""))` is an Err instead.

### FIX 3 — env_role_model misses the bare env var
`env_role_model(role)` uses `openduck_env::get_var("MODEL")` / `get_var("SECONDARY_MODEL")`.
`get_var` only resolves `OPENDUCK_x` and `GOOSE_x`; a bare `MODEL` is invisible. Use
`openduck_env::env_lookup(key)` (tries OPENDUCK_, GOOSE_, then bare), then trim and drop empties.

### FIX 4 — env provider switch must not inherit the role model
In `resolve_role_model_for_provider`, `provider_switched` compares the target against the
ENV-EFFECTIVE accessors `get_active_provider` / `get_secondary_provider`. With file
`active_provider=A` + `active_model=m` and environment `PROVIDER=B`, target B is judged "not
switched" and inherits A's model. Compare against FILE values only:
`config.file_string(ACTIVE_PROVIDER_KEY)` (trimmed) for Major,
`config.file_string(SECONDARY_PROVIDER_KEY)` (trimmed) for Secondary.
Keep env-effective accessors everywhere else (e.g. `resolve_role_model`).

### FIX 5 — file role readers must not expose legacy keys or blanks
`file_active_model` / `file_secondary_provider_model` chain legacy keys (`GOOSE_MODEL`,
`GOOSE_SECONDARY_MODEL`, `OPENDUCK_SECONDARY_MODEL`) and can return blank strings, so legacy values
enter the role-priority branch and bypass the `provider_roles_migrated` guard.
Make them read ONLY `active_model` / `secondary_provider_model`, trimmed, non-empty.
Legacy stays exclusively in `legacy_role_model()` (which keeps its migration guard).
These readers are also used by tests to assert "nothing persisted", so legacy inclusion breaks them.

### FIX 6 — Config::get_goose_secondary_model bypasses the resolver
base.rs ~1331-1400 has its own chain reading `entry.model` and the legacy keys.
Replace the body with a delegation:
    crate::config::providers::resolve_role_model(self, Role::Secondary).ok().map(|r| r.model)
so secondary resolution matches major resolution.

### FIX 7 — set_secondary_provider keeps a stale role model
`set_secondary_provider(config, Some(x))` leaves the previous `secondary_provider_model` in place.
When the secondary provider name changes (including clearing it), clear `secondary_provider_model`
in the SAME atomic write.

### FIX 8 — ProviderEntry::normalize drops an explicit fast model
Delete the `trimmed == self.model` branch: an explicit fast_model equal to entry.model must be
retained (role models may differ now). Only blank => None, otherwise trim.

## Test corrections (after the code fixes; verify green)
- `save_role_selection_with_none_clears_role_model`: after clearing the role key,
  `get_active_model` legitimately falls back to the provider default. Assert
  `file_active_model(&config).is_none()` AND `get_active_model(&config) == Some("entry-default")`.
- `legacy_goose_model_used_when_unmigrated` and `v1_config_with_legacy_keys_resolves`: seed RAW YAML
  (`GOOSE_PROVIDER`, `GOOSE_MODEL`) by writing the YAML directly into the config temp file BEFORE
  constructing `Config`. `set_param`/`update_param` go through full migrations which strip the legacy
  keys and set `provider_roles_migrated=true`; the read path only runs read-migrations, so raw seeded
  legacy keys stay observable. `assert!(!provider_roles_migrated(&config))` must then hold.
- `provider_default_used_when_no_role_model` and `missing_model_everywhere_is_an_error` should pass
  once FIX 2 lands.

## Verification
    cd /mnt/e/openduck-harness && . bin/activate-hermit
    cargo fmt
    cargo test -p openduck --test provider_model_selection_test     # must be 18/18
    cargo test -p openduck --lib config                            # no regressions

Update any lib unit test that encoded the OLD coupled semantics (new semantics win).

## Rules
ConfigError stays at config boundaries, anyhow::Result elsewhere. No comments restating code.
No new dependencies, do not edit Cargo.toml. Iterate until green.

## Report
Files+functions changed, FIX -> what landed, final test summary lines.
