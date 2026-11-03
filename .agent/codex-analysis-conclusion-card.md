# Transcript conclusion card: implementation design

Analysis only; no source changes or tests run. Locations below refer to the inspected working tree.

## Verified current behavior and code sites

- `ui/hub/src/components/AgentLogTranscript.tsx:60,80-81`: `status: string`, optional nullable `finalAnswer` and `failureReason` props.
- `ui/hub/src/components/TaskTrajectoryViewer.tsx:726-727`: derives answer/reason with `extractFinalAnswer` / `extractFailureReason`; passes `String(run.status)` at `:1659` and answer/reason at `:1684-1685`.
- `crates/openduck-harness/src/types.rs:94-101`: statuses are Success, Failure, Timeout, Cancelled, Running, Skipped. No `Completed`, `failed`, or `error` enum variants; do not invent aliases.
- `AgentLogTranscript.tsx:151-157`: `lastFinalId` is the last encountered final-answer turn ID in `allTurns ?? turns`. It does **not** indicate whether that turn is the last turn. A success card appears when `finalAnswer` is truthy and no such turn exists (including filtered logs).
- `AgentLogTranscript.tsx:307`: visibility is `finalAnswer && (!lastFinalId || status.toLowerCase() !== 'success')`.
- `AgentLogTranscript.tsx:314-317`: cancelled border `1px solid rgba(245, 158, 11, 0.4)`; every other status red `1px solid rgba(239, 68, 68, 0.4)`.
- `AgentLogTranscript.tsx:335`: cancelled title color `#f59e0b`; every other status `#ef4444`.
- `AgentLogTranscript.tsx:338-340`: cancelled title `⚠️ Task Incomplete / Cancelled`; every other status `❌ Task Failed`.
- `AgentLogTranscript.tsx:342-350`: truthy reason renders `Failure Reason: ...`, always `#f87171` (`:346`). Background is constant `rgba(15, 23, 42, 0.6)` (`:318`).
- `ui/hub/src/taskTrajectory.ts:108-109`: reason extraction returns null for success/running. For other statuses it can synthesize a reason (`:176-181`), so presence of a reason does not itself mean Failure.
- `taskTrajectory.ts:208-216`: non-success/non-running answers get the hardcoded markdown heading at **both `:213` and `:216`**, subject to the reason and existing-text guard at `:215`.

## Minimal pure helper and component consumption

Add beside the extractors in `ui/hub/src/taskTrajectory.ts`:

```ts
export type TranscriptConclusionTone = 'success' | 'failure' | 'warning' | 'neutral'

export function describeTranscriptConclusion(options: {
  status: string
  hasFailureReason: boolean
}): { title: string; tone: TranscriptConclusionTone }
```

Normalize status once with `status.toLowerCase()`, matching existing helpers. Require the boolean; callers pass `Boolean(failureReason?.trim())`. Omit `hasFinalAnswer`: content existence governs visibility, not classification, and the helper should work for reason-only synthesized markdown too. Never classify by searching answer prose.

Use the exact mapping below. Known timeout/cancelled/skipped/running states take precedence over reason presence, because the existing extractor synthesizes reasons for non-success states. Success plus a supplied nonblank failure reason is the explicit inconsistent-input exception; classify it as failure. Unknown status is neutral even with a reason because its extractor fallback is also synthesized and cannot prove failure. The helper changes presentation only; it does not modify run status or continuation eligibility.

| Normalized status | hasFailureReason | Title | Tone |
| --- | --- | --- | --- |
| success | false | ✅ Task Completed | success |
| success | true | ❌ Task Failed | failure |
| failure | either | ❌ Task Failed | failure |
| timeout | either | ⚠️ Task Incomplete / Timed Out | warning |
| cancelled | either | ⚠️ Task Incomplete / Cancelled | warning |
| skipped | either | Task Skipped | neutral |
| running | either | Task In Progress | neutral |
| unknown / empty / unrecognized | either | Task Conclusion | neutral |

Align with `ui/hub/src/taskContinuation.ts:3-11`: case-insensitive matching, exact canonical status names, timeout/cancelled distinct from Failure. Do not use `isContinuableStatus` as a failure predicate: it intentionally includes all three. Neither continuation helper expresses presentation semantics, so importing them would obscure the mapping; leave them unchanged.

In `AgentLogTranscript`, import and call the helper once with the props. Define a small exhaustive tone-to-style map in the component module and use it for all three current color decisions:

| Tone | Border | Title color | Reason color |
| --- | --- | --- | --- |
| success | `1px solid rgba(34, 197, 94, 0.4)` | `#22c55e` | `#86efac` |
| failure | `1px solid rgba(239, 68, 68, 0.4)` | `#ef4444` | `#f87171` |
| warning | `1px solid rgba(245, 158, 11, 0.4)` | `#f59e0b` | `#fbbf24` |
| neutral | `1px solid rgba(148, 163, 184, 0.4)` | `#cbd5e1` | `#94a3b8` |

Render `description.title`; choose border, title color and reason color exclusively from `description.tone`. Keep the current visibility expression, background, layout, reason label and markdown content wiring. A success-with-reason input only receives failure presentation when the existing gate renders its card; do not expand visibility in this fix. The normal viewer cannot currently produce this edge case because its success reason extractor returns null.

## Markdown heading parity: yes

Changing only the card leaves timeout/cancelled/skipped conclusions with a contradictory red-cross failure heading inside their body and in exports. Reuse the same helper inside `extractFinalAnswer` without changing answer selection or reason extraction.

Within the existing `if (reason)` block, compute:

```ts
const { title } = describeTranscriptConclusion({
  status: String(run.status || ''),
  hasFailureReason: Boolean(reason.trim()),
})
const heading = `## ${title} (${run.status})`
```

At **both `taskTrajectory.ts:213` and `:216`**, replace the literal `## ❌ Task Incomplete / Failed (${run.status})` with `${heading}`; preserve the reason line, separator and original answer. Keep the existing success/running bypass at `:208-209` (their answers remain untouched). Preserve legacy duplicate-prefix checks at `:215`; additionally guard `!ans.includes(heading)` to recognize newly generated neutral headings, without trying to rewrite old headings already stored in agent text.

Examples: Cancelled becomes `## ⚠️ Task Incomplete / Cancelled (Cancelled)`, Timeout becomes `## ⚠️ Task Incomplete / Timed Out (Timeout)`, Skipped becomes `## Task Skipped (Skipped)`, Failure becomes `## ❌ Task Failed (Failure)`. Unknown/empty uses `Task Conclusion` with the existing raw status suffix. Leave `Failure Reason` wording and extractor semantics unchanged to keep scope minimal.

## Concrete test plan

Add table-driven `assert.deepEqual` cases for the exported helper in `ui/hub/src/taskTrajectory.test.ts` (Node pure-function tests; no DOM dependencies):

1. Success without reason returns the exact completed title/success tone: regression for a rendered successful conclusion without a final-answer turn.
2. Timeout, Cancelled, Skipped and Running each return the exact non-failure mapping above; test **both boolean values** so synthesized reasons cannot turn them red/failed.
3. Failure with and without a reason returns failed/failure.
4. Success with reason returns failed/failure (direct helper test; success extraction intentionally suppresses reasons).
5. Unknown literal, empty string, and an unrecognized status each return conclusion/neutral with and without a reason.
6. Test Rust casing (`Success`, `Failure`, etc.) and uppercase/lowercase equivalents. Do not add whitespace normalization as an unrelated behavior change.

For `extractFinalAnswer`:

- Existing tests at `:187-213` (top-level Success answer) and `:215-248` (latest trajectory fallback) remain unchanged and must pass.
- Update the existing cancelled test at `:250-298`, especially the exact old-heading regex at **`:294`**, to the warning Cancelled heading; retain reason/subtask/body assertions. Rename it to describe incomplete/cancelled output accurately.
- Add Timeout, Skipped and Failure cases with a top-level answer: assert the exact new heading prefix, preserved reason, separator and original body. Also pass the extracted reason into the helper and verify its expected tone to cover the real extractor-to-presentation boundary.
- Cover the reason-only branch (`finalAnswer: null`, no final-answer step) for Cancelled, Timeout, Skipped and Failure, verifying the same headings and no invented answer body/separator.
- Running with an answer returns it verbatim; Running without one returns null. Success with an `error` still returns its original answer and `extractFailureReason` remains null.
- Re-extract a newly prefixed answer and verify no second heading is added; retain legacy `Task Failed` / `Task Incomplete` prefix protection. Check null/undefined run behavior remains null.

When implementing, run `cd ui/hub && pnpm test` (package script: `node --experimental-strip-types --test src/*.test.ts`) and `pnpm run typecheck` for TSX/style-map wiring. No new test framework or Rust changes needed. Pure tests cannot verify DOM wiring: manually inspect a Success card with a top-level answer but no final-answer turn, then Failure/Timeout/Cancelled/Skipped/Running cards, checking title and all colors. No test/build commands were executed for this analysis.

## Other parity sites

Search of `ui/hub/src` found no other conclusion renderer hardcoding these labels. The occurrences are the component title, the two extractor headings, the extractor's legacy-text guard (`taskTrajectory.ts:215`), and the old assertion (`taskTrajectory.test.ts:294`). `AgentLogTranscript.tsx:584` says `Subtask Failed`; that is a separate subtask heading and outside this fix.

Indirect consumers automatically receive the corrected generated markdown: `TaskTrajectoryViewer.tsx:726`, `taskTrajectory.ts:1838` (metrics final answer), and `ui/hub/src/taskPdfExport.ts:247`. Review affected export assertions if they assert exact generated headings; no separate export title mapping is needed. Preserve all unrelated working-tree edits.
