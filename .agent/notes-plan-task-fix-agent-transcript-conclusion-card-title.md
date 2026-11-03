# Task: fix agent-transcript-conclusion-card title

## Bug (verified, no code changed yet)
`ui/hub/src/components/AgentLogTranscript.tsx` ~L307-361: card className `agent-transcript-conclusion-card` renders title via ternary `status.toLowerCase() === 'cancelled' ? '⚠️ Task Incomplete / Cancelled' : '❌ Task Failed'`; border (L314-317), title color (L335), reason color (L346) hardcoded the same way. So non-cancelled runs incl. `success` are titled "Task Failed".
Visibility expression (must stay): `finalAnswer && (!lastFinalId || status.toLowerCase() !== 'success')` (L307).
`lastFinalId` = last final_answer turn id in `allTurns ?? turns` (L151-157) — not "last turn".

## Location map
- Props `status: string` (L60), `failureReason?: string | null` (L81), `finalAnswer?: string | null` (L80).
- `TaskTrajectoryViewer.tsx:1659` passes `status={String(run.status)}`; `:1684-1685` finalAnswer/failureReason; `:726-727` derives them.
- Rust statuses: `crates/openduck-harness/src/types.rs:94` RunStatus { Success, Failure, Timeout, Cancelled, Running, Skipped }.
- `ui/hub/src/taskTrajectory.ts:106-181` extractFailureReason (null for success/running; synthesizes for others).
- `ui/hub/src/taskTrajectory.ts:208-216` extractFinalAnswer hardcodes `## ❌ Task Incomplete / Failed (status)` at :213 and :216.
- `ui/hub/src/taskTrajectory.test.ts` cancelled extractFinalAnswer test ~250-298, old-heading regex at :294.
- Tests: `cd ui/hub && pnpm test` (node --experimental-strip-types --test src/*.test.ts), typecheck `pnpm run typecheck`.

## Design (codex, .agent/codex-analysis-conclusion-card.md; spec at .agent/spec-conclusion-card.md)
Add to taskTrajectory.ts:
```ts
export type TranscriptConclusionTone = 'success' | 'failure' | 'warning' | 'neutral'
export function describeTranscriptConclusion(o: { status: string; hasFailureReason: boolean }): { title: string; tone: TranscriptConclusionTone }
```
Mapping: success+no reason -> `✅ Task Completed`/success; success+reason -> `❌ Task Failed`/failure; failure -> `❌ Task Failed`/failure; timeout -> `⚠️ Task Incomplete / Timed Out`/warning; cancelled -> `⚠️ Task Incomplete / Cancelled`/warning; skipped -> `Task Skipped`/neutral; running -> `Task In Progress`/neutral; else -> `Task Conclusion`/neutral.
Component: `CONCLUSION_TONE_STYLES` map (success border rgba(34,197,94,.4)/#22c55e/#86efac; failure rgba(239,68,68,.4)/#ef4444/#f87171; warning rgba(245,158,11,.4)/#f59e0b/#fbbf24; neutral rgba(148,163,184,.4)/#cbd5e1/#94a3b8).
extractFinalAnswer parity: replace both hardcoded headings with `## ${title} (${run.status})`; keep bypass/reason/separator; skip prefixing if `ans.includes(heading)`.

## Progress / blockers
- codex analysis: DONE.
- grok: quota exhausted ("Weekly limit left: 0%") — unusable.
- agy: 4 sessions launched; every one stalls at "Generating/Loading..." producing zero file changes (files' mtimes unchanged). Unusable right now.
- claude: not logged in ("Not logged in · Run /login") — unusable.
- No source edits made yet by any agent.

## Next steps
1. Retry coding agents (agy fresh single-shot short prompt; verify file mtime changes).
2. If none work, implement myself (Section A: taskTrajectory.ts helper + extractFinalAnswer + tests; Section B: AgentLogTranscript.tsx wiring).
3. Verify: `cd ui/hub && pnpm test`, `pnpm run typecheck`; review diff; update self-test docs not needed (hub-only).
