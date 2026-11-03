# Task: Provider/Model Decoupling — Full Implementation (run 3)

Repo: /mnt/e/openduck-harness  Design: design-docs/provider-model-decoupling-plan.md
Branch: develop. Prior commits: a38080d "feat(config): decouple provider and model selection with role-level resolution",
2f80680 "feat: store active model top-level and add model refresh endpoint".

## Prior-run state (from git HEAD notes; treat as HINTS not truth)
- A ~85%: resolver/config/harness/acp/inventory service done; HTTP GET v2 + POST refresh + PUT v2 written.
- Known blocking issue: migrations.rs `migrate_provider_config` writes legacy keys into entry.model and
  cleanup_legacy_provider_keys deletes GOOSE_MODEL at write time => provider_model_selection_test 9/18.
- Deleted files in working tree: .agent/notes... (restored now), crates/openduck/tests/debug_dump.rs.
- provider_config_api_test.rs exists untracked (possibly incomplete).

## DESIGN FACTS (must verify in code)
- Role keys: active_model, secondary_provider_model. Resolver precedence major: explicit > MODEL env >
  active_model (only if target provider == stored role provider) > entry.model > legacy GOOSE_MODEL > error.
- Secondary: explicit > SECONDARY_MODEL env > secondary_provider_model > entry.model >
  GOOSE_SECONDARY_MODEL/OPENDUCK_SECONDARY_MODEL > error.
- GET adds selectionVersion=2, majorModel/secondaryProviderModel {value,fileValue,fromEnv,source}
  source ∈ environment|role|providerDefault|legacy|unset. ProviderDetail adds models[{id,name}],
  inventoryStatus, supportsRefresh, refreshing, lastUpdatedAt, lastRefreshError.
- POST /api/v1/config/providers/{providerId}/models/refresh -> 202, shared inventory service (no ACP handler call).
- PUT v2: selectionVersion=2; all 4 role fields required; provider null => model must be null; model null =>
  clear explicit role model; blank rejected; IDs raw; provider-row model omitted=keep, ""=clear, null=reject;
  no selectionVersion => v1 (legacy) semantics incl. syncing role model when legacy client changes that
  provider's default; env lock compares FILE values; atomic save.
- Same provider for both roles allowed everywhere (config validate_selection, harness resolve_secondary_target,
  HTTP, Hub).
- normalize keeps fast_model even when == entry.model.
- configured must be decoupled from model non-empty.

## PLAN
1. Verify actual current state: build -p openduck, run provider_model_selection_test.
2. Fix A remainder (migrations, tests, API tests) → A acceptance gate.
3. Subtask B (hub UI).
4. Subtask C (full verify).

## PROGRESS LOG
- (init) read design doc; restored prior notes; need build state check.
