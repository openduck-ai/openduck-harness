# Task B (grok): Hub role-level provider+model selection UI

Repo `/mnt/e/openduck-harness`. Design doc `design-docs/provider-model-decoupling-plan.md` phase 4 (read it).
The Rust backend is already implemented; the DTO contract below is authoritative (verified against
`crates/openduck/src/control/app_config.rs`).

You own ONLY:
- ui/hub-core/src/types.ts
- ui/hub-core/src/rest.ts
- ui/hub-core/src/rest.test.ts
- ui/hub/src/providerConfig.ts
- ui/hub/src/providerConfig.test.ts
- ui/hub/src/components/OpenDuckConfigPanel.tsx

Other agents are editing Rust files concurrently. Do not touch Rust. Do not touch ui/desktop.

## Backend contract (authoritative)

GET `/api/v1/config/providers` -> camelCase JSON:
```ts
selectionVersion?: 2            // absent on old servers
majorProvider: { value: string|null, fileValue: string|null, fromEnv: boolean }
secondaryProvider: { value: string|null, fileValue: string|null, fromEnv: boolean }
majorModel?: { value: string|null, fileValue: string|null, fromEnv: boolean,
               source: 'environment'|'role'|'providerDefault'|'legacy'|'unset' }
secondaryProviderModel?: same shape
modelFromEnv: boolean; fastModelFromEnv: boolean; secondaryModelFromEnv: boolean
availableProviders: string[]
proxies: { id: string, url: string, description?: string|null }[]
providers: ProviderDetail[]
```
`ProviderDetail`:
```ts
name: string; enabled: boolean; configured: boolean
model: string; modelEnvValue: string|null          // provider default model
fastModel: string|null; fastModelEnvValue: string|null
secondaryModel: string|null; secondaryModelEnvValue: string|null
resolvedFastModel: string; fastModelUsesMain: boolean; suggestedFastModel: string|null
proxy?: string|null
models?: { id: string; name: string }[] | null     // inventory; absent when unknown
inventoryStatus?: 'ready'|'stale'|'neverFetched'|'unsupported' | null
supportsRefresh?: boolean | null
refreshing?: boolean | null
lastUpdatedAt?: string | null
lastRefreshError?: string | null
```
PUT `/api/v1/config/providers`:
- v1 (no `selectionVersion`): `{ majorProvider, secondaryProvider, providers: [{name, enabled, model, fastModel, secondaryModel, proxy?}] }`
- v2 (`selectionVersion: 2`) REQUIRED shape:
```ts
{ selectionVersion: 2,
  majorProvider: string|null, majorModel: string|null,
  secondaryProvider: string|null, secondaryProviderModel: string|null,
  providers: [{ name, enabled, model?: string|null, fastModel: string|null, secondaryModel: string|null, proxy: string|null }] }
```
v2 rules the UI must respect: provider null => model must be null; model null => clear explicit role model
(fall back to provider default); `providers[].model` OMITTED keeps the old default, `""` clears, `null` is
rejected; blank/ padded model rejected; a selected role must still resolve a non-empty model after save.
POST `/api/v1/config/providers/{providerId}/models/refresh` -> 202 `{ started: string[], skipped: [{providerId, reason}] }`.

## UI requirements (design phase 4)
1. Above the provider table add a role selection area: `Major provider [▼] / Major model [▼]` and
   `Secondary provider [▼] / Secondary model [▼]`.
2. Provider candidates: `enabled && configured` (NOT requiring a non-empty model). The secondary list must
   NOT exclude the major-selected provider (same-provider dual roles are allowed). Keep a warning hint when
   both roles resolve to the same provider+model ("identical targets cause duplicate invocations").
3. Model candidates: `models` of the selected provider, searchable when long. A "recommended" marker may only
   affect ordering (no auto-selection). If `models` is empty/absent/errored, fall back to a free-text model
   ID input.
4. Switching provider clears the previous model draft. If the provider has a default model (`model` non-empty)
   offer an explicit "inherit default (xxx)" choice (value `null`); if it has no default model, require an
   explicit selection.
5. If the effective model (`majorModel.value`) is not in `models`, still display it, labelled
   "custom / not listed".
6. A refresh button per provider row that POSTs `.../models/refresh` then re-reads GET. Handle loading,
   empty list, and failure-with-retry. Async refresh results must be attributed by `providerId` and must NOT
   overwrite a provider the user has since switched to (guard against the race).
7. When `fromEnv` is true show BOTH the file value and the effective value and lock editing (no write-back).
8. In the provider table rename the `model` column to "Default model (optional)"; move
   `fast_model`/`secondary_model` into a collapsed "Advanced" section.
9. After a successful save, rebuild the draft from the server response and clear the dirty flag.
10. Backwards compatibility: when GET has no `selectionVersion` (old server), keep the legacy single-model
    edit mode, show an explanatory notice, and never send v2 requests.
11. No `any` escapes in TypeScript (`any`, `as any`, `@ts-ignore`, `@ts-expect-error` are forbidden). Keep the
    existing vitest patterns; extend `ui/hub/src/providerConfig.test.ts` and `ui/hub-core/src/rest.test.ts`.
    Do not recreate ui/desktop/src/api or add @hey-api/openapi-ts.

## Style
Pure, testable logic in `providerConfig.ts` (draft state, candidate derivation, v1/v2 body building,
recovering the draft from a response). The React component consumes those helpers. No new comments that
restate code; only comment non-obvious decisions.

## Verify (must pass before reporting)
```bash
cd /mnt/e/openduck-harness/ui/hub && pnpm test
cd /mnt/e/openduck-harness/ui/hub && pnpm run typecheck   # if the script exists
cd /mnt/e/openduck-harness/ui/hub-core && pnpm test       # if hub-core has its own runner; else state why not
```
Report: files changed, helper API you introduced, test names added, and the command results.
