# Hub role-level provider and model selection: implementation plan

Design-only reconnaissance, 2026-11-02. No application code changes or builds/tests are part of this task. All anchors below refer to the files as inspected, before implementation.

## Scope and contract

Implement phase 4 in `design-docs/provider-model-decoupling-plan.md:496`, following the HTTP DTO at `design-docs/provider-model-decoupling-plan.md:283`, PUT semantics at `design-docs/provider-model-decoupling-plan.md:353`, and interactions at `design-docs/provider-model-decoupling-plan.md:382`. The document's section 4 contains the phased plan; the concrete DTO examples currently appear in section 3.6. The user's supplied DTO is authoritative alongside that design. No current Rust source was inspected or used as a contract.

One configured provider may serve both roles with different models. Provider defaults are optional; inventory is advisory, and IDs, not display names, are submitted. A role model is separate from the provider's `secondaryModel` preference. Existing sessions, judge/planner settings, credentials, and Desktop are outside scope. Before upstream implementation, follow the repository's Ready-issue workflow; this local design artifact does not require a Ready issue.

## 1. Shared client types

Modify `ui/hub-core/src/types.ts:176`, `:182`, `:212`, `:223`, and `:232` as follows. Keep existing response fields and their types, including legacy environment booleans. Only newly added GET fields are optional for old-server compatibility; normalize absent booleans to false in UI state.

New exported types:

```ts
export type RoleModelSource =
  | 'environment' | 'role' | 'providerDefault' | 'legacy' | 'unset'

export interface RoleModelSelectorField extends ProviderSelectorField {
  source: RoleModelSource
}

export interface ProviderModelOption {
  id: string
  name: string
}
```

`ProviderSelectorField` remains `{ value: string | null; fileValue: string | null; fromEnv: boolean }`. Do not invent a `source` property on the existing provider-selector DTO: the supplied contract adds source to the model-selector objects.

Add to `ProvidersConfigResponse`:

```ts
selectionVersion?: 2
majorModel?: RoleModelSelectorField
secondaryProviderModel?: RoleModelSelectorField
```

Add to `ProviderDetail`:

```ts
models?: ProviderModelOption[]
inventoryStatus?: string
supportsRefresh?: boolean
refreshing?: boolean
lastUpdatedAt?: string | null
lastRefreshError?: string | null
```

The supplied DTO specifies `ready` as an example status, not a complete status enum or timestamp encoding. Keep `inventoryStatus` a string and display unknown statuses without disabling manual entry; treat `lastUpdatedAt` as nullable server text and avoid date arithmetic until encoding is confirmed. Do not inspect Rust to manufacture a status union. Existing `ProviderDetail.model: string` remains required, permitting `''`. No recommendation metadata is present in this DTO, so use the entire catalog in server order and search by ID/name; no client-side recommended-only filter.

Use a discriminated PUT union so an explicitly v2 request cannot accidentally omit a role field:

- Keep exported `ProviderEntryPutItem` as the legacy row: required `model: string`, existing required `fastModel`/`secondaryModel: string | null`, existing optional `proxy?: string | null`, and `name`/`enabled` unchanged.
- Add `ProviderEntryPutItemV2 extends Omit<ProviderEntryPutItem, 'model'>` with `model?: string`. Never permit `null` for a provider default.
- Add exported `ProviderConfigPutBodyV1`: `selectionVersion?: never`, required `majorProvider: string | null`, `secondaryProvider: string | null`, `providers: ProviderEntryPutItem[]`. Mark `majorModel?: never` and `secondaryProviderModel?: never` to prevent accidental v2 fields.
- Add exported `ProviderConfigPutBodyV2`: required `selectionVersion: 2`, `majorProvider: string | null`, `majorModel: string | null`, `secondaryProvider: string | null`, `secondaryProviderModel: string | null`, `providers: ProviderEntryPutItemV2[]`.
- Change exported `ProviderConfigPutBody` from interface to `ProviderConfigPutBodyV1 | ProviderConfigPutBodyV2`. Existing legacy object callers remain assignable. Audit property consumers for union narrowing rather than weakening types.

All wire fields use the exact camelCase names above. `ui/hub-core/src/index.ts:1` and `:3` already re-export types and REST functions; no barrel changes should be needed.

## 2. REST changes

At `ui/hub-core/src/rest.ts:278`, retain `getProvidersConfig(): Promise<ProvidersConfigResponse>` and `updateProvidersConfig(body: ProviderConfigPutBody): Promise<ProvidersConfigResponse>` with the expanded types. The PUT response remains the full configuration response used to rebuild state.

Add the `createApi` method:

```ts
refreshProviderModels(providerId: string): Promise<void>
```

POST to `/api/v1/config/providers/${encodeURIComponent(providerId)}/models/refresh`, with no request body. Preserve the existing secret header, timeout, URL construction, and error handling. A 202 acknowledges scheduling; it does not contain a model catalog and is not completion.

The helper at `ui/hub-core/src/rest.ts:246` currently parses all successful responses except 204 as JSON (`:257`). Add a third parameter `responseKind: 'json' | 'empty' = 'json'` to `request<T>`. After the existing `response.ok` check, return `undefined as T` when `responseKind === 'empty'` or status is 204; otherwise keep existing JSON parsing. Invoke the new method with `request<void>(path, { method: 'POST' }, 'empty')`. This explicitly accepts an empty 202 without changing parsing for other endpoints, including any future JSON-bearing 202. Do not invent a refresh-response DTO or cast JSON into `ProviderDetail`.

Extend `ui/hub-core/src/rest.test.ts:22` and the existing provider endpoint coverage at `:631`: verify encoded provider IDs, POST/auth/Accept headers, absent body, empty 202 resolution, optional JSON acknowledgment being ignored, non-2xx errors through `readErrorMessage`, unchanged GET/PUT types and serialized v2 field names, and existing 204/JSON behavior.

## 3. Draft redesign and pure helpers

Replace the mixed effective/file role state in `ui/hub/src/providerConfig.ts:27` with explicit editable fields and read-only GET snapshots. Keep the exported `ProviderConfigDraft` name; add:

```ts
export type ProviderRole = 'major' | 'secondary'
export interface RoleSelectionMetadata {
  provider: ProviderSelectorField
  model: RoleModelSelectorField
}
```

The complete role-related portion of `ProviderConfigDraft` becomes:

```ts
selectionMode: 'legacy' | 'v2'
majorProvider: string | null
majorModel: string
secondaryProvider: string | null
secondaryModel: string
majorSelection: RoleSelectionMetadata
secondarySelection: RoleSelectionMetadata
```

Retain `modelFromEnv`, `fastModelFromEnv`, `secondaryModelFromEnv`, `availableProviders`, `providers`, and `proxies` for legacy/advanced behavior. Remove `majorFileValue`, `majorFromEnv`, `secondaryFileValue`, and `secondaryFromEnv`; replace uses with `majorSelection.provider.fileValue/fromEnv` and their secondary equivalents. These snapshots also retain effective `value`, independent of editable draft values. Each model snapshot has all four fields `value`, `fileValue`, `fromEnv`, and `source`; do not mutate snapshot source to pretend it is a server-resolved preview.

Keep all current `ProviderRowDraft` fields (`ui/hub/src/providerConfig.ts:7`). Its `model` remains a provider default, and its `secondaryModel` remains the advanced provider preference. Add a required `inventory: ProviderInventoryDraft` member:

```ts
export interface ProviderInventoryDraft {
  models: ProviderModelOption[]
  inventoryStatus: string | null
  supportsRefresh: boolean
  refreshing: boolean
  lastUpdatedAt: string | null
  lastRefreshError: string | null
}
```

Normalize absent GET inventory fields to `[]`, `null`, false, false, null, null. `addProvider` creates the same empty inventory and keeps `configured: false`; entering a default model must not mark a connection configured. Newly added providers become eligible after a successful server response reports configured connectivity.

### GET builder: `draftFromResponse(resp)`

Keep the existing exported function name (`ui/hub/src/providerConfig.ts:127`). Select v2 only when `resp.selectionVersion === 2`. For v2 require both model-selector objects; missing objects should surface a contract error and block saving rather than silently issuing v1 or inventing a saved role model.

Copy all selector snapshots. For each editable provider use effective `value` when env-locked and `fileValue` otherwise, matching current provider semantics. For editable role model use `field.value ?? ''` when env-locked, otherwise `field.fileValue ?? ''`. An empty editable model means inherit, even if effective `value` is a non-empty provider fallback or legacy value. Never turn a fallback value into an explicit role key. Display effective/source separately from the editable file draft.

With no `selectionVersion`, use legacy mode. Preserve existing row model initialization and legacy environment behavior; set the unused role-model drafts to `''`. Synthesize model metadata for display only from the selected provider's default/environment projection, with `fileValue: null` (old servers expose no role model key), `source: 'environment'` for an applicable env override, otherwise `'providerDefault'` for a non-empty default or `'unset'`. Preserve `fromEnv` from the legacy role flags. This metadata does not authorize v2 writes. Hide independent model controls and refresh controls in this mode; retain provider-row model editing and old validation.

### Role updates and validation

Add exported `setRoleProvider(draft: ProviderConfigDraft, role: ProviderRole, providerId: string | null): ProviderConfigDraft`. Return unchanged if the provider selector is env-locked or the ID is unchanged. When switching, clear only that role's unlocked model draft to `''`; clearing the provider clears its unlocked model too. If the model is independently env-locked, preserve its effective display and its original file snapshot; do not overwrite the locked file key. The backend validates any incompatible resulting target. No per-provider draft buffering in this iteration. Switching one role must never change the other role or any provider default.

Add exported `roleProviderCandidates(draft: ProviderConfigDraft, role: ProviderRole): ProviderRowDraft[]`. In v2 filter `enabled && configured` for both roles and do not exclude the major provider from secondary. In legacy preserve the existing `enabled && model.trim()` and secondary exclusion. Render an existing unavailable selection separately as a disabled current-value option; do not silently clear it or offer it as an eligible new choice.

Change `validateDraft` (`:192`) to branch on selection mode. Retain the old v1 rules. In v2 allow identical providers; check selected rows exist, are enabled and configured, and require either an explicit nonblank role model or a nonblank provider default. Use the file values for env-locked fields when validating the intended persisted pair, not the effective environment strings. Preserve unchanged inherited legacy targets reported as `source: 'legacy'` with an effective value: the backend remains authoritative for fallback resolution that GET cannot reconstruct. Inventory membership is never validation. Reject a model with no provider, except that locked display values are not editable persistence inputs. Provider=null means PUT model=null. Exact empty draft means inherit; whitespace-only nonempty input is an error. Use trim only for emptiness checks on model IDs, not to replace a valid ID or substitute its display name. Server validation errors leave the draft intact.

### PUT builders

Keep `buildPutBody(draft): ProviderConfigPutBody` (`:232`) as dispatcher. Add exported `buildV2PutBody(draft): ProviderConfigPutBodyV2`, which rejects a legacy draft and calls the v2 validation. Move existing v1 serialization into a private `buildLegacyPutBody` and retain its behavior.

V2 always emits all four role fields plus `selectionVersion: 2`. For an unlocked role, take current editable provider/model; model `''` serializes as null, and provider null forces model null. For an env-locked provider/model field use that field's original `fileValue`, including null, never its effective value. Resolve lock decisions independently for provider and model; do not copy an env role model into a provider default.

Serialize provider rows with `name`, `enabled`, `fastModel`, `secondaryModel`, and `proxy`. Include `model` only when the edited default differs from `fileModel`; an edited empty default emits `model: ''`. Unchanged defaults omit the property entirely, including when blank. Never emit `model: null`. Keep advanced fast-model env preservation and proxy normalization. V2 role MODEL/secondary-role locks no longer lock provider defaults. Normalize fastModel against the provider default as the existing helper does; do not normalize it against either role selection. Do not send configured state, inventory, snapshot metadata, or effective values.

### Every existing export: disposition

| Existing export and anchor in providerConfig.ts | Disposition |
| --- | --- |
| `ProviderRowDraft` :7 | Extend with required normalized inventory; retain existing fields. |
| `ProviderConfigDraft` :27 | Change role shape as specified above. |
| `normalizeFastModel` :42 | Keep signature and behavior. |
| `normalizeSecondaryModel` :49 | Keep; applies to provider advanced preference only. |
| `normalizeProxy` :56 | Keep. |
| `validateProxyUrl` :62 | Keep. |
| `validateProxyId` :76 | Keep. |
| `resolveProxyLabel` :92 | Keep. |
| `findMatchingProxyId` :106 | Keep. |
| `fastModelLabel` :117 | Keep helper; clarify default/fast meaning in surrounding advanced UI. |
| `secondaryModelLabel` :122 | Keep; advanced preference label only. |
| `draftFromResponse` :127 | Change to capability-aware builder with role snapshots. |
| `addProvider` :162 | Change initialization to include inventory; preserve current add semantics. |
| `validateDraft` :192 | Change to v1/v2 branches. |
| `buildPutBody` :232 | Change to v1/v2 dispatcher. |

New exports are `ProviderRole`, `RoleSelectionMetadata`, `ProviderInventoryDraft`, `setRoleProvider`, `roleProviderCandidates`, and `buildV2PutBody`. Keep request/polling orchestration out of this pure draft module.

## 4. Provider configuration test matrix

Extend `ui/hub/src/providerConfig.test.ts:20` with a separately typed v2 fixture; retain the current fixture as the old-server case. Keep the Node `node:test` / `assert` style at `:1`. Add these cases:

1. Parse v2 with one configured provider, blank default, and two different role model IDs. Assert independent drafts, all metadata fields, inventory mapping, and no model-to-default copying.
2. Parse each source (`environment`, `role`, `providerDefault`, `legacy`, `unset`). A non-env fallback with `fileValue: null` produces empty editable model and retains its effective display. Missing v2 model objects errors; optional inventory fields normalize correctly.
3. Build v2 for a shared provider and assert all four role keys and version are present, with exact model IDs. Assert provider rows contain no inventory/metadata. Known model display names must never appear as submitted IDs.
4. Default tri-state: unchanged populated/empty defaults omit `model` (assert `Object.hasOwn` is false); edited default emits string; explicit clear emits `''`; never null. Advanced provider `secondaryModel` remains independent of top-level `secondaryProviderModel`.
5. Blank role draft emits null for inheritance; a cleared role emits provider/model null together; whitespace-only explicit model errors. A selected role with no explicit/default model errors, while an unchanged server-reported legacy fallback is preserved.
6. V2 accepts a configured provider without a default when the role model is explicit, allows equal providers, rejects disabled/unconfigured/missing candidates, and never checks catalog membership. Candidate lists for both roles include the major provider.
7. Provider switches clear only that role's unlocked model, preserve row defaults and the other role, and do nothing when selecting the same provider. Clearing a provider clears its unlocked model. Locked provider changes are ignored; model-only env locks preserve file/effective metadata on provider changes.
8. Independent env locks on major/secondary providers and models: effective values differ from file values, including null file values; manually tampering with draft inputs still submits file values. V2 default row remains editable under role model locks. Preserve fast-model file values when its independent lock applies.
9. Custom/unlisted ID survives GET/build/PUT and inventory updates; preserve IDs verbatim. Empty/failed inventory never invalidates or clears selection. Search matches name and ID while selected custom IDs remain visible even outside search results.
10. No-version response builds a body without version or role model keys, retains required provider-row model and v1 same-provider/empty-model restrictions, old environment handling, proxy helpers, and add-provider behavior. Existing tests at `:77`, `:97`, and `:103` remain explicit legacy tests rather than being deleted to make v2 pass.

For the locked-model edge case, if clearing a provider would require changing a locked non-null model file key, disable that clear action and report it in validation. Do not silently coerce the locked file value to null. A locked model with null fileValue may clear with its provider. This follows both the lock-preservation and provider-null/model-null contracts.

## 5. OpenDuckConfigPanel integration

### Placement and props

At `ui/hub/src/components/OpenDuckConfigPanel.tsx:399`, expand the existing block immediately above the provider table (`:465`) into Major provider / Major model and Secondary provider / Secondary model. Show a visible compatibility note for legacy mode: this server supports provider defaults only and needs an upgrade for independent role models. Preserve legacy provider/table behavior in that branch.

Add `'refreshProviderModels'` to the `ConfigApi` Pick at `:30`. External `OpenDuckConfigPanelProps` at `:44` retain `api` and `isConnected`; update any typed API mocks to include the new method. Import `useRef` for async generation guards.

Define a local presentational `RoleModelPicker` with precisely these props:

```ts
interface RoleModelPickerProps {
  role: ProviderRole
  providerId: string | null
  model: string
  defaultModel: string
  metadata: RoleModelSelectorField
  inventory: ProviderInventoryDraft | null
  disabled: boolean
  refreshPending: boolean
  refreshError: string | null
  onModelChange: (model: string) => void
  onRefresh: () => void
}
```

Keep its search/custom-input mode as local UI state, reset on provider change (key by role and provider ID). Provider selects call `setRoleProvider` via functional state updates. Model edits likewise use functional updates and respect `metadata.fromEnv`. Filter candidates through `roleProviderCandidates`, replacing v2 filters at `:413` and `:443`. Do not accidentally retain the secondary exclusion at `:454` in the current-selection rendering.

### Model selection behavior

Use a labeled search input (`type="search"`) plus a native model select filtered by ID and display name, and an explicit custom-input action. This provides search without a new component library. Options use IDs as values and names plus IDs as labels; keep selected IDs visible when search filters them out. No model-ID sentinel collision: use a separate custom-mode button/state rather than storing a magic custom string in the model draft.

When `defaultModel` is nonblank, offer value `''` labeled `Inherit default (xxx)`. With no default, show an empty `Select a model` placeholder and require explicit input (except the unchanged server-reported legacy fallback). Show inherited legacy effective values and their source separately; do not relabel them as provider defaults.

If inventory is empty, unavailable, or reports a refresh failure, expose manual input with an inline status and retry when supported. Retain cached candidates on errors. Custom input remains available even with a populated list. A selected explicit ID absent from inventory gets `Custom / Unlisted` beside the value; inventory failure does not erase it. Initial page loading remains the current loading view (`:390`); inventory refreshing keeps existing controls/values visible. Disable model editing when no provider, during save, or when the model field is env-locked.

For each env-locked provider or model, render labeled read-only effective value and file value (null shown as `Unset / inherit`) plus source for models. Lock corresponding edits independently. Do not display effective environment values in an editable file field. Existing unlocked unavailable provider selections stay visible with an explanation and a corrective choice.

### Refresh lifecycle and race protection

Add `handleRefreshProviderModels(providerId: string): Promise<void>` in the panel. Maintain `refreshRequests: Record<string, { pending: boolean; error: string | null }>` as UI state, `refreshSequence: Map<string, number>` in a ref, and a `configEpoch` ref that advances on disconnect, API replacement, save start/success, and unmount cleanup. Tokens are scoped to provider IDs, not roles; two roles sharing a provider share one request/button state.

1. Enable refresh only in v2 with `inventory.supportsRefresh`, a connected provider, no pending/server refresh, and no provider save. Env locks do not prevent inventory refresh.
2. Capture API/config epoch and increment this provider's sequence. POST using the captured provider ID, then GET. Because 202 schedules async work, poll GET sequentially at roughly 1-second intervals while that provider says `refreshing`. Allow a small initial grace window (for example three reads) if the first snapshot has not yet marked refreshing; bound the attempt to 30 reads and offer retry on timeout. GET has no refresh-job token, so the UI can report observed server status, not claim a particular job completed when no transition was observed.
3. Before every update and scheduled poll, verify epoch and provider sequence; stop when superseded, removed, disconnected, or unmounted. Clear timers in cleanup. Never overlap polling GETs for the same provider; independent provider requests may coexist.
4. Merge only the captured provider's inventory into the matching current draft row by name using a functional update. Never call `draftFromResponse` on a refresh response, replace the provider array, change configured/default fields, or overwrite role drafts. Results for A may update A's cache after switching to B, but never B's picker or either role selection. Different provider polls returning full GET snapshots must each merge only their own provider.
5. Errors stay keyed to provider ID; preserve inventory and manual input. A subsequent successful inventory snapshot clears that provider's request error. Both pickers sharing the provider see the same result.

### Provider defaults and advanced fields

At `:471`, rename the v2 table column to exactly `Default model (optional)`. Keep its free text edit and allow blank; the field edits `ProviderRowDraft.model` only. Remove v2 role-environment locking from table default logic at `:487`–`:497`; retain that legacy behavior only in legacy mode.

Move fast-model and provider-secondary-model cells (`:534` and `:558`) into a per-provider `<details><summary>Advanced model preferences</summary>…</details>` cell. Label them as provider preferences, explicitly distinguishing provider secondary preference from the secondary role model. Keep file/env fast-model behavior and normalization. UI labels may mention persisted `fast_model` and `secondary_model` in help text; DTO/draft properties remain camelCase. Keep proxy, enable, add, and remove actions; adjust empty-row `colSpan` from seven to six after replacing two model columns with one advanced column. Prevent removal of providers referenced by either editable or locked file/effective role selectors.

### Save and rebuild

Retain the successful save pattern already present at `:128`–`:130`: `buildPutBody`, PUT, then `draftFromResponse(saved)`. Rebuild all role snapshots, defaults, inventory, and the clean baseline from the returned server document; reset picker search/custom UI state and refresh request state. Set `newProviderSelect` unconditionally to `saved.availableProviders[0] ?? ''` so an empty list clears the old selection. Invalidate pending refresh generations before a save to prevent pre-save inventories from overwriting the result. Failed saves preserve the draft and display the server validation error. Do not optimistically change source/effective metadata before a successful response.

## 6. Verification commands and UI checks

Confirmed by reading `ui/hub/package.json:7` and `ui/hub-core/package.json:14`:

| Directory | Exact pnpm command | Script expansion |
| --- | --- | --- |
| `ui/hub` | `pnpm test` | `node --experimental-strip-types --test src/*.test.ts` |
| `ui/hub` | `pnpm run typecheck` | `tsc --noEmit` |
| `ui/hub` | `pnpm run build` | `tsc -b && vite build` |
| `ui/hub-core` | `pnpm test` | `node --experimental-strip-types --test src/*.test.ts` |
| `ui/hub-core` | `pnpm run typecheck` | `tsc --noEmit` |

From the repository root these can be invoked with `pnpm --dir ui/hub test`, `pnpm --dir ui/hub run typecheck`, `pnpm --dir ui/hub run build`, `pnpm --dir ui/hub-core test`, and `pnpm --dir ui/hub-core run typecheck`. Hub also has `pnpm run dev` (`vite`) and `pnpm run preview` (`vite preview`). Hub-core has no build/dev script. Neither package declares lint, test:unit, test:e2e, Vitest, or a React component-test script. Do not invent these commands or claim `pnpm test` discovers TSX/component subdirectory tests.

During implementation, run the pure draft and REST suites and both typechecks; build Hub when build validation is requested. For deterministic refresh race tests, extract the polling coordinator into `ui/hub/src/providerModelRefresh.ts` with injected API and scheduler and test it in `ui/hub/src/providerModelRefresh.test.ts`, which the existing glob discovers. Cover shared-provider deduplication, A→B switch with late A response, overlapping different-provider GET snapshots, timeout/error preservation, and save/disconnect invalidation. This helper should receive typed callbacks and never own role edits.

Manual UI verification with `pnpm run dev`: one provider/two models; configured blank default; inheritance and explicit clear; searchable IDs/names; unlisted custom ID; empty inventory; failed refresh with cached list; provider switch mid-refresh; two roles sharing refresh; independent env provider/model overrides with file/effective display; advanced preferences/proxy preservation; save response normalization; and no-version server compatibility. If automated component tests are required, agree on the test harness separately rather than pretending the current scripts supply DOM rendering. No tests or build commands were run for this reconnaissance.

## 7. Risks and implementation order

- Draft state churn: migrate all removed flat metadata reads together, including proxy/add/remove callbacks and typed mocks. Keep role `secondaryModel` and row `secondaryModel` visibly scoped; only the role serializes to `secondaryProviderModel`.
- Wire compatibility: new GET properties are optional, while v2 PUT is strict. Never infer v2 from inventory presence or downgrade an advertised malformed v2 response. Backend status/timestamp details remain deliberately uninterpreted strings until documented.
- Async state: 202 is not completion; provider ID, sequence, and configuration epoch guards are all needed. Refresh must never rebuild editable drafts.
- Environment/fallback semantics: null file values are meaningful. Avoid promoting resolved fallback/environment values into explicit role keys. The server owns final resolution/validation, particularly unchanged legacy inheritance and env-constrained targets.
- Type safety: no `any`, `as any`, ts-ignore, or unchecked model casts. Use DTO unions, `unknown` errors, and explicit metadata/default normalization.
- Scope: no `ui/desktop/src/api` changes, generated OpenAPI code, dependency additions, backend edits, or separate client catalog. Existing package exports suffice.

Implement in order: shared DTO/REST and REST tests; draft types/builders and pure tests; panel role controls/defaults/advanced layout; refresh coordinator and race tests; authorized typechecks/tests and UI verification. Backend v2 capability must precede enabling v2 UI against a deployed server.
