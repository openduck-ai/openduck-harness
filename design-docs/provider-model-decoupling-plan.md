# OpenDuck Provider and Model Decoupling Design

> Status: Design Proposal, Not Yet Implemented. Verified against the current workspace codebase (codex design output, referenced line numbers cross-checked).

## 1. Problem Statement

The current configuration couples a provider's default model with role runtime models. Modifying the major role's model changes the `model` field of the corresponding provider entry; the secondary role also reads the provider entry's `model`. Furthermore, configuration validation, Harness, and Hub all prohibit the major and secondary roles from using the same provider.

Consequently, even if a single service connection supports multiple models, users still need to duplicate provider entries to use those models across different roles.

This design decouples these two concepts:

- **Provider**: Connection identity, engine, endpoint, credentials, and available model inventory.
- **Role Selection**: Which provider and which model are used for the major and secondary roles.

`providers.<id>.model` is retained as a backward-compatible default, but is no longer the sole storage location for role selections.

### 1.1 Goals

1. Configuring a provider does not require specifying a single runtime model.
2. Store provider and model independently for major and secondary roles.
3. Allow major and secondary roles to use different models under the same provider.
4. Unify model pickers in Hub using the inventory service, while supporting custom models outside the inventory.
5. Legacy configurations continue to run without requiring manual migration.
6. Explicitly specifying a model for a session or single run does not alter global default configuration.

Typical result:

```yaml
active_provider: ark
active_model: deepseek-v3

secondary_provider: ark
secondary_provider_model: glm-4.6

providers:
  ark:
    enabled: true
    configured: true
```

Here, `ark` denotes the registered provider ID; the actual requested model ID is determined by server requirements and may be a deployment or endpoint ID.

### 1.2 Non-Goals

- Refactoring provider engines, credentials, or declarative provider file formats.
- Automatically merging or deleting existing `ark-deepseek` or `ark-glm` entries.
- Changing Harness retry counts, fallback trigger conditions, or invocation ordering.
- Automatically enabling `ProviderEntry.secondary_model`.
- Changing role configurations for independent subsystems such as planner or judge.
- Automatically updating models for active/running sessions after saving default configuration.
- Treating the inventory catalog as an exhaustive proof of remote model accessibility.

## 2. Current State Analysis

### 2.1 Configuration Structure and Connection Definitions Belong to Different Layers

The actual configuration entry resides in [crates/openduck/src/config/providers.rs](/mnt/e/openduck-harness/crates/openduck/src/config/providers.rs:14), not `config/config.rs`:

```rust
ProviderEntry {
    enabled: bool,
    model: String,
    configured: bool,
    fast_model: Option<String>,
    secondary_model: Option<String>,
    proxy: Option<String>,
}
```

`model` already has `serde(default)`, so it is allowed to be missing when reading; the primary constraints stem from write validation and runtime resolution.

This struct does not contain `engine`, `api_key`, or `base_url`. Declarative connection definitions reside in [crates/openduck-providers/src/declarative.rs](/mnt/e/openduck-harness/crates/openduck-providers/src/declarative.rs:116), containing `engine`, `api_key_env`, `base_url`, and `models`. Core loading logic lives in [config/declarative_providers.rs](/mnt/e/openduck-harness/crates/openduck/src/config/declarative_providers.rs:1).

Therefore, there is no need to migrate connection fields into `ProviderEntry`, nor should API keys be included in Hub config DTOs.

### 2.2 Coupling in Current Major Model Read and Write Paths

`get_active_model` in [config/providers.rs](/mnt/e/openduck-harness/crates/openduck/src/config/providers.rs:122) reads in order:

1. `MODEL` environment variable overrides;
2. `entry.model` of the current provider;
3. Legacy file key `GOOSE_MODEL`.

In the same file, `set_active_provider(config, name, model)` requires a non-empty model and mutates `entry.model`.

ACP's [on_defaults_save](/mnt/e/openduck-harness/crates/openduck/src/acp/server/config.rs:183) already receives provider/model, but still ends up calling this setter. Thus, the ACP message format supports independent parameters, while persistence semantics remain coupled.

### 2.3 Secondary Also Relies on Provider Default Model

[Config::get_goose_secondary_model](/mnt/e/openduck-harness/crates/openduck/src/config/base.rs:1331) reads environment variable overrides, falls back to the secondary provider's `entry.model`, and then checks unmigrated legacy keys.

[resolve_secondary_target](/mnt/e/openduck-harness/crates/openduck/src/control/harness.rs:1373) implements another set of resolution rules and explicitly rejects secondary provider being identical to primary.

The primary model resolution in [resolve_harness_policy_with_secondary](/mnt/e/openduck-harness/crates/openduck/src/control/harness.rs:1456) also reads `entry.model` directly; modifying only `get_active_model` is insufficient.

### 2.4 HTTP Routes Reside in the Core Crate

There is no `crates/openduck-server` in the current workspace. The actual endpoints are:

- Routes: [control/http.rs](/mnt/e/openduck-harness/crates/openduck/src/control/http.rs:1598)
- GET handler: [control/http.rs](/mnt/e/openduck-harness/crates/openduck/src/control/http.rs:1544)
- PUT handler: [control/http.rs](/mnt/e/openduck-harness/crates/openduck/src/control/http.rs:1552)
- Business logic: [control/app_config.rs](/mnt/e/openduck-harness/crates/openduck/src/control/app_config.rs:537)

The existing `GET/PUT /api/v1/config/providers` endpoints can be reused.

[ProviderDetail](/mnt/e/openduck-harness/crates/openduck/src/control/app_config.rs:130) returns a single `model` plus fast/secondary fields, without a model inventory list.

[ProviderConfigPutBody](/mnt/e/openduck-harness/crates/openduck/src/control/app_config.rs:186) saves role providers and provider rows; `ProviderEntryPutItem.model` is a required string. PUT also explicitly checks for missing/null model, and requires the selected provider itself to have a non-empty `model`.

[app_config.rs](/mnt/e/openduck-harness/crates/openduck/src/control/app_config.rs:956) currently couples `configured` with whether model is non-empty; both need to be decoupled.

The existing persistence workflow calls `save_provider_configuration`; this batch persistence entry point should be extended rather than writing role keys in multiple individual steps.

### 2.5 Model Inventory Has an Existing Unified Implementation

[ProviderInventoryEntry](/mnt/e/openduck-harness/crates/openduck/src/providers/inventory/mod.rs:31) already contains:

- `provider_id`, `default_model`
- `configured`, `available`
- `models: Vec<InventoryModel>`
- Refresh capabilities, refresh status, update timestamps, and errors
- `model_selection_hint`

[InventoryModel](/mnt/e/openduck-harness/crates/openduck/src/providers/inventory/mod.rs:71) contains `id`, `name`, family, context length, and recommended flags.

[ProviderInventoryService](/mnt/e/openduck-harness/crates/openduck/src/providers/inventory/mod.rs:196) provides `entry_for_provider`, `entries`, and refresh mechanisms. Inventory aggregation already handles snapshots and static models, eliminating the need to reconstruct catalogs in Hub.

Other entry points include:

- [ProviderMetadata.known_models](/mnt/e/openduck-harness/crates/openduck-provider-types/src/base.rs:26)
- [Provider::fetch_supported_models](/mnt/e/openduck-harness/crates/openduck-provider-types/src/base.rs:552)
- ACP [providers.rs](/mnt/e/openduck-harness/crates/openduck/src/acp/server/providers.rs:39) inventory DTO mapping and refresh logic
- ACP supported-models method definitions: [custom_requests.rs](/mnt/e/openduck-harness/crates/openduck-sdk-types/src/custom_requests.rs:1706)

READMEs are explanatory material and not runtime catalog sources.

ACP defaults persistence already allows models outside the inventory; local inference is an exception, see [config.rs](/mnt/e/openduck-harness/crates/openduck/src/acp/server/config.rs:217). HTTP should maintain identical semantics.

### 2.6 Hub Limitations

[OpenDuckConfigPanel.tsx](/mnt/e/openduck-harness/ui/hub/src/components/OpenDuckConfigPanel.tsx:396) currently:

- Major candidates require `enabled && model.trim()`;
- Secondary candidates additionally exclude the major provider;
- Models are edited as text fields in the provider table rows.

Draft state logic is in [providerConfig.ts](/mnt/e/openduck-harness/ui/hub/src/providerConfig.ts:1), with interface types and request wrappers located in:

- [ui/hub-core/src/types.ts](/mnt/e/openduck-harness/ui/hub-core/src/types.ts:212)
- [ui/hub-core/src/rest.ts](/mnt/e/openduck-harness/ui/hub-core/src/rest.ts:259)

Therefore, a targeted change in `ui/hub-core` is required, rather than changing `ui/hub` alone.

### 2.7 Relationship with Prior Designs

[provider-configuration-plan.md](/mnt/e/openduck-harness/design-docs/provider-configuration-plan.md:17) explicitly adopted "provider entry stores model, secondary must be a different provider". This design modifies both constraints.

Portions of that document's current-state descriptions are now outdated (e.g., setters now preserve other fields of the existing entry). Implementation must adhere to the current codebase and the new acceptance criteria.

As this is a substantive modification to prior design, it should be agreed upon and confirmed as Ready in the associated tracking issue before upstream implementation.

## 3. Proposed Design

### 3.1 Data Model

Retain field types in `ProviderEntry` to avoid unnecessarily converting `String` to `Option<String>`.

Adjusted semantics:

| Field | New Semantics |
|---|---|
| `ProviderEntry.model` | Provider compatibility default / fallback model; allowed to be absent or empty |
| `enabled` | Whether the user has enabled this provider |
| `configured` | Whether the provider connection configuration is complete, independent of model selection |
| `fast_model` | Explicit fast model preference for the provider |
| `secondary_model` | Existing second-model preference for the same provider, keeping its original purpose |
| `proxy` | Retained as-is |

New file keys:

| Key | Meaning |
|---|---|
| `active_model` | Explicit model for major role |
| `secondary_provider_model` | Explicit model for secondary role |

Retain `active_provider` and `secondary_provider`.

`secondary_provider_model` is chosen to avoid confusion with `ProviderEntry.secondary_model`.

New internal concepts:

- `ResolvedProviderModel { provider, model }`: Resolved, executable target.
- Role selection in save requests: provider can be cleared; model can be explicitly specified or reset to inherited fallback.

New business functions use `anyhow::Result`; existing public `ConfigError` and HTTP error boundaries are retained with errors translated at the boundary, avoiding unnecessary signature churn.

### 3.2 Resolution Rules

Centralize resolution logic into a unified resolver reused by config wrappers, ACP, and Harness.

Major resolution order:

1. Session/request explicit model;
2. `MODEL` environment variable override;
3. `active_model` matching the current target provider;
4. Target provider's non-empty `ProviderEntry.model`;
5. Legacy file keys applicable under existing conditions;
6. Return an explicit missing model error.

Secondary resolution order:

1. Request explicit secondary model;
2. `SECONDARY_MODEL` environment variable override;
3. `secondary_provider_model` matching the current target provider;
4. Target provider's non-empty `ProviderEntry.model`;
5. Legacy keys applicable under unmigrated conditions;
6. Return a missing model error.

Constraints:

- When a request specifies a different provider, it does not inherit the role model saved for the original provider.
- When an environment variable replaces a role provider, the previous provider's role model is not applied to the new provider.
- The first inventory item, recommended models, and registry default models do not automatically become runtime selections.
- If an explicitly provided model does not exist, the provider returns a business error; local models retain existing existence validation.
- A provider without a model can be marked configured, but cannot start runs that require a model.

Global model environment variable overrides maintain existing semantics; GET should indicate their source so users understand the effective runtime value.

### 3.3 Write and Compatibility Strategies

1. Legacy file with only `providers.ark.model`: continues running with the original model.
2. New client saving role models: updates only role keys without mutating `entry.model`.
3. Provider-only configuration allows omitting `model`.
4. `set_active_provider` is updated to modify the major selection via the new role persistence function; audit all call sites, and calls that genuinely need to update the provider default model should explicitly invoke `upsert_provider_models`.
5. `clear_active_provider` also clears `active_model`.
6. Clearing secondary also deletes `secondary_provider_model` to prevent stale state.
7. Provider and model are committed together in a single configuration write.
8. No destructive migration is performed on read, and `provider_roles_migrated` is not repurposed for new migration states.

When legacy clients submit only provider row models, continue accepting legacy semantics. If a legacy client modifies the default model corresponding to an existing explicit role selection, synchronize the corresponding role model to avoid "saved successfully but runtime value unchanged." This behavior must be scoped strictly to legacy-format requests and backed by regression tests.

When the same provider is used by both roles, legacy clients cannot represent two distinct models; GET adds capability flags, and documentation clarifies this limitation.

### 3.4 Major and Secondary Sharing the Same Provider

Remove the following restrictions:

- Provider name mutual exclusivity in config-layer `validate_selection`;
- HTTP save validation;
- Name mutual exclusivity in `resolve_secondary_target`;
- Hub secondary candidate filtering.

Role configurations remain independent: separate `ModelConfig` instances are built even if the provider is identical.

No recursive fallback is introduced. If a secondary invocation fails, the process terminates following existing workflows.

Both roles are permitted to resolve to the same model, but the Hub will display a prompt warning that "identical targets result in duplicate invocations"; no new implicit disabling rules are introduced.

### 3.5 fast_model and secondary_model

**Reuse existing mechanisms without replacement.**

- `fast_model` retains provider-level preferences.
- When fast model is unset, follow the rule in [model_config.rs](/mnt/e/openduck-harness/crates/openduck/src/model_config.rs:94), defaulting to the primary model selected for that invocation.
- When both roles share the same `ark` provider and fast model is unset, each role falls back to its own runtime model.
- `ProviderEntry.secondary_model` continues to represent the original same-provider secondary model preference and does not automatically become the secondary role model.

Adjust `normalize`: retain explicit `fast_model` even if it equals the legacy `entry.model`. After decoupling, role models may differ, and removing it would alter the user's explicit preference.

Fast model values already purged by normalization in older versions cannot be recovered and continue to be treated as unset.

### 3.6 Backend API

Reuse:

```text
GET /api/v1/config/providers
PUT /api/v1/config/providers
```

GET retains existing fields and adds:

```json
{
  "selectionVersion": 2,
  "majorModel": {
    "value": "deepseek-v3",
    "fileValue": "deepseek-v3",
    "fromEnv": false,
    "source": "role"
  },
  "secondaryProviderModel": {
    "value": "glm-4.6",
    "fileValue": "glm-4.6",
    "fromEnv": false,
    "source": "role"
  },
  "providers": [
    {
      "name": "ark",
      "enabled": true,
      "configured": true,
      "model": "",
      "models": [
        { "id": "deepseek-v3", "name": "DeepSeek V3" },
        { "id": "glm-4.6", "name": "GLM 4.6" }
      ],
      "inventoryStatus": "ready",
      "supportsRefresh": true,
      "refreshing": false,
      "lastUpdatedAt": null,
      "lastRefreshError": null
    }
  ]
}
```

`source` distinguishes between `environment`, `role`, `providerDefault`, `legacy`, and `unset`. `fileValue` indicates only the value persisted in role keys without disguising fallback results as saved values.

`models` in GET originates from `ProviderInventoryService`. This is an inventory projection consumed by the settings page; the frontend does not invoke providers directly, nor does it maintain a separate catalog.

GET reads inventory snapshots and static catalogs by default, without initiating synchronous network discovery for each provider. Add:

```text
POST /api/v1/config/providers/{providerId}/models/refresh
```

The refresh endpoint reuses inventory identity, deduplication, and stale-refresh guards, returning `202`; the frontend subsequently refetches GET. If ACP scheduling logic needs extraction, sink it into a shared inventory service rather than having HTTP invoke ACP handlers.

Example v2 format PUT:

```json
{
  "selectionVersion": 2,
  "majorProvider": "ark",
  "majorModel": "deepseek-v3",
  "secondaryProvider": "ark",
  "secondaryProviderModel": "glm-4.6",
  "providers": [
    {
      "name": "ark",
      "enabled": true,
      "fastModel": null,
      "secondaryModel": null,
      "proxy": null
    }
  ]
}
```

Contract:

- v2 must include provider/model fields for both roles, preventing omissions from being mistaken as clearing selections.
- Provider as `null` indicates clearing the role; model must simultaneously be `null`.
- Model as `null` indicates clearing the explicit role model, reverting to the provider default fallback.
- Blank models are rejected; model IDs are persisted verbatim without substituting display names for request IDs.
- In v2 provider rows, omitting `model` retains the legacy default, `""` explicitly clears it, and `null` is rejected.
- Absence of `selectionVersion` is handled as an existing v1 request.
- Provider configuration validation and role model resolution are executed separately.
- Selected roles must resolve to a valid non-empty model.
- Environment lock comparison evaluates against file values, preventing active environment overrides from being written back to configuration.
- Atomically save only after all validations succeed; failures produce no partial modifications.

DTOs must not return credentials or sensitive inputs from inventory identities. Refresh errors should reuse existing sanitization to avoid leaking headers or secret keys.

### 3.7 Inventory Service Integration

The HTTP state provides a shared `ProviderInventoryService` utilizing existing `SessionStorage`, avoiding isolated refresh states on every GET.

Inventory must be keyed by provider instance identity rather than merged solely by `engine`. The same engine with different base URLs or credentials may have different model access permissions.

When inventory retrieval fails:

- Retain configuration and current model;
- Return available cached inventory;
- Clarify failure via status fields;
- Allow manual model input;
- Do not clear role selections due to refresh failures.

### 3.8 Hub Frontend

Place role selections above the provider table:

```text
Major provider       [ark ▼]
Major model          [DeepSeek V3 ▼]

Secondary provider   [ark ▼]
Secondary model      [GLM 4.6 ▼]
```

Interaction rules:

1. Provider candidates are based on enabled and connection configured states, not requiring a non-empty provider default model.
2. Model candidates use the provider's entire inventory and support search; recommended badges only affect sort ordering.
3. Switching providers clears the previous provider's model draft; display inheritance options when defaults exist, and require explicit selection when no defaults exist.
4. Drafts can buffer selections per provider on the current page to prevent losing input during switching; saving submits only the current role targets.
5. Models not present in inventory are still displayed and preserved, labeled as "Custom / Unlisted".
6. Support refresh, loading, empty inventory, failure retry, and manual input.
7. Display file values and effective values separately under environment overrides, locking corresponding edits.
8. Rename the original `model` in the provider table to "Default Model (Optional)", moving fast/secondary fields into advanced settings.
9. Regenerate drafts from the server response upon saving, clearing dirty state.
10. Async refresh results are scoped by provider ID and must not overwrite selections if the user has already switched to another provider.

## 4. Phased Implementation Plan

### Phase 0: Design Confirmation and Scope Freeze

**Files:**

- `design-docs/provider-configuration-plan.md`
- New design document (this document)
- Corresponding issue and verification plan

**Acceptance Criteria:**

- Confirm allowing identical providers for both roles.
- Confirm new key names, v1/v2 compatibility semantics, and rollback constraints.
- Upstream implementation issue marked Ready on the project board.
- Avoid creating non-existent `openduck-server` crate.

### Phase 1: Configuration and Resolution Decoupling

**Files:**

- `crates/openduck/src/config/providers.rs`
- `crates/openduck/src/config/base.rs`
- `crates/openduck/src/config/mod.rs`
- `crates/openduck/src/model_config.rs`
- `crates/openduck/tests/provider_model_selection_test.rs` (new)

**Tasks:**

- Introduce role model keys, unified resolver, and batch persistence entry point.
- Retain fallback to legacy default models.
- Decouple `configured` status from model presence.
- Adjust fast model normalization.
- Audit setters, cleanup routines, and provider deletion behaviors.

**Acceptance Criteria:**

- Legacy configuration resolution output remains unchanged.
- Providers can be configured without default models.
- Both roles can reference different models on the same provider.
- Clearing, switching, and environment overrides do not cross-contaminate models.
- Invalid save requests do not perform partial writes.

### Phase 2: Runtime and ACP Integration

**Files:**

- `crates/openduck/src/control/harness.rs`
- `crates/openduck/src/acp/server/config.rs`
- `crates/openduck/src/acp/server/providers.rs`
- `crates/openduck/src/agents/agent.rs`
- `crates/openduck/src/agents/state_machine/` affected resolution entry points
- `crates/openduck/tests/acp_custom_requests_test.rs`
- `crates/openduck/tests/provider_model_selection_test.rs`

**Tasks:**

- Harness adopts the unified resolver and removes same-provider restrictions.
- ACP defaults saves role targets without overwriting provider default models.
- Identify runtime paths directly reading `entry.model`, distinguishing default retrieval from role selection.
- Both agent loops share identical resolution results.

**Acceptance Criteria:**

- Major and secondary issue requests with their respective correct models.
- Session explicit models are not written to global configuration.
- Both agent loops behave consistently.
- Secondary trigger conditions and retry counts remain unchanged.
- ACP out-of-inventory models and local model validation remain compatible.

### Phase 3: HTTP and Inventory Projection

**Files:**

- `crates/openduck/src/control/app_config.rs`
- `crates/openduck/src/control/http.rs`
- `crates/openduck/src/providers/inventory/mod.rs`
- Necessary HTTP state definition files
- `crates/openduck/tests/provider_config_api_test.rs` (new)

**Acceptance Criteria:**

- GET returns model catalog and inventory status for each provider.
- PUT v2 saves role models independently.
- PUT v1 maintains defined compatibility behavior.
- Refresh does not block normal GET requests; cached catalog is retained on failure.
- Changing connection identity prevents stale refresh results from polluting new inventory.
- Responses contain no credentials.

### Phase 4: Hub UI and Client Types

**Files:**

- `ui/hub-core/src/types.ts`
- `ui/hub-core/src/rest.ts`
- `ui/hub-core/src/rest.test.ts`
- `ui/hub/src/providerConfig.ts`
- `ui/hub/src/providerConfig.test.ts`
- `ui/hub/src/components/OpenDuckConfigPanel.tsx`
- Corresponding component test files

**Acceptance Criteria:**

- Configuring a single ark allows selecting two different role models.
- Provider table rows no longer require default models.
- Full inventory, custom input, refresh, and error recovery are supported.
- Environment override display and persistence behave correctly.
- When an older server lacks v2 support, UI clearly indicates version capabilities and avoids silently sending incompatible requests.

### Phase 5: Regression, Documentation, and Release

**Files:**

- `openduck-self-test.yaml`
- Provider configuration documentation
- Constraints updates in existing design docs

**Acceptance Criteria:**

- Legacy configs, new configs, and mixed-client scenarios pass regression testing.
- Self-test recipe covers shared-provider role selection.
- PR links Ready issue with documented verification steps and results.
- All implementation phases are released in dependency order, backend preceding v2-enabled frontend.

## 5. Risks and Rollback

| Risk | Mitigation |
|---|---|
| Confusion between new and legacy model fields | DTO displays source; UI explicitly distinguishes role models from default models |
| Legacy clients overwriting role selections | Branch v1/v2 handling and secure with compatibility tests |
| Incomplete or stale inventory | Allow custom inputs, preserve active selection, avoid automatic model switching |
| Same-engine instances cross-contaminating inventory | Reuse inventory identity including endpoint and credential identity factors |
| Inconsistent resolution rules across call sites | Unified resolver, audit all direct `entry.model` reads |
| Same-provider fallback cannot mitigate full service outages | UI prompts note it is primarily for model-level differentiation, retaining cross-provider configuration capabilities |
| Implicit changes to fast model | Retain explicit values; fallback relies on per-invocation target when unset |
| Older binaries ignoring new role keys | Export compatible configuration and verify models before downgrading |

Rollback strategy:

1. Back up configuration prior to rollout; the new design does not remove legacy `ProviderEntry.model`.
2. UI can be rolled back first while preserving backend v1/v2 compatibility.
3. Before binary downgrade, write role models back into legacy format.
4. When major and secondary use different providers, write back to their respective `model` fields.
5. When both roles share a provider with different models, the legacy format cannot express this losslessly; users must explicitly export a second provider alias with identical connection settings, or unassign secondary.
6. Do not automatically dual-write provider default models to disguise complete downgrade compatibility, as this reintroduces coupling.

New configuration files being parsed by older versions by ignoring unknown keys does not guarantee identical runtime behavior after downgrade.

## 6. Test Plan

### 6.1 Rust Unit Tests

Prefer placing in `crates/openduck/tests/`; retain tests requiring private access in their respective modules.

Coverage:

- Deserialization of legacy `ProviderEntry` with missing model.
- Resolution compatibility for legacy configurations without new keys.
- Precedence among request, environment, role, provider default, and legacy keys.
- Provider switching does not inherit role models from other providers.
- Clearing roles simultaneously clears models.
- Providers without models can still be marked configured.
- Explicit fast model identical to legacy default model is retained.
- Dual-role selection under identical provider.
- Omitted, null, and blank value semantics under v1/v2.
- Validation failures do not modify configuration.

### 6.2 Rust Integration Tests

Use recordable mock providers to avoid dependencies on real API keys.

Scenarios:

1. One provider configured with two inventory models.
2. Save major=A, secondary=B.
3. Only call A when primary succeeds.
4. Call B after primary exhausts retries under existing rules.
5. Terminate upon secondary failure without recursive fallback.
6. Selections persist after reloading configuration.
7. Session model C does not alter global A/B.
8. Legacy loop and `OPENDUCK_STATE_MACHINE=1` both use correct models.
9. ACP defaults save does not modify provider default values.
10. Inventory refresh failure, identity changes, and local model validation behave as expected.

HTTP tests verify status codes, response fields, atomic saves, environment locks, and absence of credential leaks.

### 6.3 Frontend Tests

Extend existing `providerConfig.test.ts` and `rest.test.ts`, adding component interaction tests:

- Display corresponding model list upon provider selection.
- Same provider selectable for both roles.
- Providers with empty default models still appear in candidate list.
- Different role models save and echo independently.
- Switching providers does not carry over old models.
- Preserve out-of-inventory models.
- Loading, empty lists, refresh failures, and retries.
- Refresh race conditions do not overwrite newer selections.
- Environment values are not written back.
- Compatibility adaptation between new and legacy servers.

### 6.4 Implementation Verification Requirements

This task is design-only; do not run builds, tests, or formatting.

During subsequent implementation:

- Rust modifications must execute `cargo fmt`.
- Follow repository rules: run checks only after receiving authorization to build/test.
- Run `cargo test -p openduck`, affected frontend tests, and necessary workspace builds.
- Run `cargo clippy --all-targets -- -D warnings` prior to merging.
- Feature implementation must update `openduck-self-test.yaml`; after authorization, rebuild and run `openduck run --recipe openduck-self-test.yaml`.
- Do not introduce unnecessary dependencies; if required, use `cargo add` and keep `Cargo.lock` consistent.
- Do not add comments that restate code or log standard flow operations.