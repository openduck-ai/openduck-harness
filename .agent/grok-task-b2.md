# Task B2: HTTP / ACP / inventory config-layer fixes (grok)

Repo: /mnt/e/openduck-harness, crate `openduck`.

Read FIRST:
1. design-docs/provider-model-decoupling-plan.md (DTO shape, v2 PUT contract, `source` semantics).
2. .agent/notes-plan-provider-model-decouple-full.md (bug list G8..G16 with file:line evidence).

## Files you own (do NOT edit others)
- crates/openduck/src/control/app_config.rs
- crates/openduck/src/control/http.rs
- crates/openduck/src/acp/server/config.rs
- crates/openduck/src/providers/inventory/mod.rs
- crates/openduck/tests/provider_config_api_test.rs  (create/extend)

Another agent (agy) concurrently edits crates/openduck/src/config/providers.rs, config/base.rs,
config/migrations.rs and crates/openduck/tests/provider_model_selection_test.rs. Do not touch them.
Resolver API is stable:
  crate::config::providers::{resolve_role_model, resolve_role_model_for_provider,
    save_role_selection, ResolvedProviderModel, Role, RoleModelUpdate, RoleSelectionUpdate,
    get_providers_map}

## Defects to fix
G8  v1 PUT (app_config.rs ~915): legacy no-selectionVersion request must sync the role model when it
    modifies the default model of a provider currently holding that role (Set(new); Clear if the new
    row model is empty). v2 path untouched.
G9  ACP on_defaults_save (acp/server/config.rs ~183-250) validates AFTER persisting. Resolve/validate
    first, then persist via save_role_selection (Major role + chosen model, never overwriting the
    provider's default `model`). Out-of-inventory models allowed; local-inference validation kept.
G10 v2 (app_config.rs ~1263-1620) resolves role models against the OLD config. Validate the FINAL
    proposed state (proposed providers map + proposed role keys) and then save atomically.
    Providers validated for existence + enabled only.
G11 Explicit fastModel dropped when equal to the provider default model (v1 ~1165, v2 ~1407): keep it.
G12 v2 marks any registered provider configured without a live check: use
    is_live_provider(...) || previously_configured.
G13 GET source inference / fast-model projection (~600-700, ~813): `source` must be exactly one of
    environment|role|providerDefault|legacy|unset matching the branch that supplied the value;
    fileValue only the persisted role key (None when unset); fast projection must not use
    entry.model as a fake role model.
G14 Inventory aggregation aborts on one entry failure (providers/inventory/mod.rs ~368): aggregate
    per-entry errors; GET must not drop all inventory fields on error (~775, ~855); PUT responses
    must carry the same inventory projection as GET (~1258, ~1606).
G16 ACP builds its own inventory service (acp/server.rs ~816) vs HTTP cached one
    (acp/server_factory.rs ~39): route both through one shared ProviderInventoryService so
    dedup/staleness guards are shared; refresh scheduling must not discard planned provider
    identities leaving a stranded `refreshing` key (~376, 607, 625); sanitize refresh error text
    projected into the DTO (~688, ~582, app_config.rs ~853).

## Tests (crates/openduck/tests/provider_config_api_test.rs)
Cover: v2 requires both role fields; provider null => model must be null; model null => clear explicit
role model restoring fallback; provider row model omitted => keep old default, "" => explicit clear,
null => reject; blank/whitespace model => reject; model IDs stored verbatim; env-locked role compares
the FILE value and never writes the env-effective value back; selected role must resolve a non-empty
model; failed validation leaves the config file byte-identical (capture before/after); v1 legacy
default-model edit syncs role model; GET shape (selectionVersion=2, majorModel/secondaryProviderModel
with value/fileValue/fromEnv/source, per-provider models[]/inventoryStatus/supportsRefresh/refreshing/
lastUpdatedAt/lastRefreshError, no api_key/apiKey/token keys anywhere in the serialized JSON); same
provider holding both roles with different models; refresh handler returns 202.

## Verification
    . bin/activate-hermit
    cargo fmt
    cargo test -p openduck --test provider_config_api_test
    cargo test -p openduck --lib control::app_config

Iterate until green. If a compile error originates in agy-owned files, report it, do not edit them.

## Rules
Keep AppConfigError/ConfigError at their boundaries, anyhow::Result elsewhere. No restating comments.
No new dependencies (do not edit Cargo.toml). Tests live under crates/openduck/tests/.

## Report back
Files+functions changed, G-item -> fix mapping, final test summary lines.
