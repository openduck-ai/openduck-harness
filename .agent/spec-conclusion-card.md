# Implementation spec — transcript conclusion card title fix

Repo: /mnt/e/openduck-harness. Scope: `ui/hub/src` only (React hub).

## Bug
`ui/hub/src/components/AgentLogTranscript.tsx` (~lines 307-361) renders the conclusion card (className `agent-transcript-conclusion-card`) with a hardcoded title: `status.toLowerCase() === 'cancelled' ? '⚠️ Task Incomplete / Cancelled' : '❌ Task Failed'`. So every non-cancelled run — including `success` — is titled "Task Failed". Border/title/reason colors are hardcoded the same way (cancelled=amber, everything else=red).

## Fix design (shared helper)
In `ui/hub/src/taskTrajectory.ts`, next to `extractFailureReason`/`extractFinalAnswer` (~lines 106-220), add:

```ts
export type TranscriptConclusionTone = 'success' | 'failure' | 'warning' | 'neutral'

export function describeTranscriptConclusion(options: {
  status: string
  hasFailureReason: boolean
}): { title: string; tone: TranscriptConclusionTone }
```

Normalize with `status.toLowerCase()` once. Known statuses take precedence over reason presence because `extractFailureReason` synthesizes reasons for non-success states.

| status | hasFailureReason | title | tone |
| --- | --- | --- | --- |
| success | false | `✅ Task Completed` | success |
| success | true | `❌ Task Failed` | failure |
| failure | either | `❌ Task Failed` | failure |
| timeout | either | `⚠️ Task Incomplete / Timed Out` | warning |
| cancelled | either | `⚠️ Task Incomplete / Cancelled` | warning |
| skipped | either | `Task Skipped` | neutral |
| running | either | `Task In Progress` | neutral |
| anything else | either | `Task Conclusion` | neutral |

Titles must match EXACTLY (single space after emoji on success/failure/warning; neutral titles have no emoji).

Tone→style map (used by the component; define there):

| tone | border | titleColor | reasonColor |
| --- | --- | --- | --- |
| success | `1px solid rgba(34, 197, 94, 0.4)` | `#22c55e` | `#86efac` |
| failure | `1px solid rgba(239, 68, 68, 0.4)` | `#ef4444` | `#f87171` |
| warning | `1px solid rgba(245, 158, 11, 0.4)` | `#f59e0b` | `#fbbf24` |
| neutral | `1px solid rgba(148, 163, 184, 0.4)` | `#cbd5e1` | `#94a3b8` |

## SECTION A — helper + extractor parity + tests (owner: agent A)
Files: `ui/hub/src/taskTrajectory.ts`, `ui/hub/src/taskTrajectory.test.ts`. Do NOT edit `AgentLogTranscript.tsx`.

1. Add the exported type + helper exactly as above.
2. In `extractFinalAnswer` (~lines 208-216) replace the hardcoded `## ❌ Task Incomplete / Failed (${run.status})` at BOTH occurrences with:
   ```ts
   const { title } = describeTranscriptConclusion({
     status: String(run.status || ''),
     hasFailureReason: Boolean(reason.trim()),
   })
   const heading = `## ${title} (${run.status})`
   ```
   Keep the existing success/running bypass, reason line, `---` separator, and original answer body unchanged. Keep the existing legacy duplicate-prefix guard and additionally skip prefixing when `ans.includes(heading)`.
3. Tests in `ui/hub/src/taskTrajectory.test.ts`:
   - Table-driven `describeTranscriptConclusion` cases: every mapping row with `hasFailureReason` false AND true; Rust-cased inputs (`Success`, `Failure`, `Timeout`, `Cancelled`, `Running`, `Skipped`); lowercase and uppercase; empty string; unrecognized status.
   - Update the existing `extractFinalAnswer` cancelled test (~lines 250-298; exact old heading regex near line 294) to the new cancelled heading, keeping reason/body assertions.
   - Add `extractFinalAnswer` cases for timeout, skipped, failure with a top-level answer: assert heading prefix, reason line, separator, body.
   - Add reason-only cases (no `finalAnswer`, no final-answer step) for cancelled/timeout/skipped/failure.
   - Assert Running with an answer returns it verbatim; Running without one returns null.
   - Assert re-extracting an already-prefixed answer adds no second heading.

## SECTION B — component wiring (owner: agent B)
File: `ui/hub/src/components/AgentLogTranscript.tsx` only. Do NOT edit `taskTrajectory.ts` / its tests.

1. Import `describeTranscriptConclusion` plus `type TranscriptConclusionTone` from `'../taskTrajectory.ts'` (add to the existing import block).
2. Add module-level `CONCLUSION_TONE_STYLES: Record<TranscriptConclusionTone, { border: string; titleColor: string; reasonColor: string }>` with the table above.
3. In the conclusion-card JSX (~307-361) compute once:
   `const conclusion = describeTranscriptConclusion({ status, hasFailureReason: Boolean(failureReason?.trim()) })`
   then use `CONCLUSION_TONE_STYLES[conclusion.tone]` for the card `border`, the title span `color`, and the failure-reason span `color`, and render `{conclusion.title}` instead of the old ternary.
4. Everything else stays byte-identical: card visibility expression, margin/padding/borderRadius/background, flex layout/gap/flexWrap, the `Failure Reason: {failureReason}` label and its truthiness gate, `MarkdownPreview` wiring. Delete now-dead locals.

## Verification
- `cd ui/hub && pnpm test` → `node --experimental-strip-types --test src/*.test.ts`; must be fully green.
- `cd ui/hub && pnpm run typecheck` → clean.
- No Rust changes needed.
