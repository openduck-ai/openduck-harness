# Plan: inline project-task creation + draft persistence

## Status — ALL 5 SUBTASKS DONE

- Subtask 1/5 — DONE. Recon doc `.agent/task-create-inline-plan.md`.
- Subtask 2/5 — DONE. Store layer `ui/hub/src/taskCreateDraft.ts` (+ test).
- Subtask 3/5 — DONE. Dialog → inline `<section className="inline-task-creator">`; `ui/hub/src/taskCreator.ts`.
- Subtask 4/5 — DONE. Autosave/restore in `ui/hub/src/taskCreateDraftFlow.ts` (+ test), wired into component.
- **Subtask 5/5 — DONE. Verification green, block indentation fixed, summary below.**

## Final verification (all re-run green in one sweep)
| Command | Result |
|---|---|
| `cd ui/hub && npx tsc --noEmit` (hub's lint gate) | exit 0 |
| `cd ui/hub && node --experimental-strip-types --test src/*.test.ts` | 271 tests / 271 pass / 0 fail |
| focused draft tests (3 files) | 42 tests / 42 pass / 0 fail |
| `cd ui/hub && pnpm run build` (`tsc -b && vite build`) | built in 4.53s, exit 0 |
| `cd ui/hub-core && npm run typecheck` (workspace dep) | exit 0 |
| `git status --porcelain` | exit 0 |

Post-build: `git checkout ui/hub/tsconfig.tsbuildinfo` (tracked-but-generated, dirtied by `tsc -b`).
`ui/hub/dist/` is gitignored → no stray artifacts.

## Tooling facts (do not re-explore)
- `ui/hub` scripts: `build`=`tsc -b && vite build`, `typecheck`=`tsc --noEmit`,
  `test`=`node --experimental-strip-types --test src/*.test.ts`.
- **`ui/hub` has NO lint script and NO linter config** (no eslint/prettier/biome anywhere in `ui/hub` or repo
  root). Precedent: `ui/mobile` and `ui/sdk` define `lint` = `tsc --noEmit`. → `tsc --noEmit` IS the lint gate.
  Do NOT add a new linter toolchain.
- Hub has **no i18n framework** (`grep i18n|useTranslation` → none); all UI strings are literal English.
  The only new user-facing copy is "New Project Task" / "Discard draft" / "Collapse" / "Keep editing" /
  "Start over" / the restore banner — no translation file exists to update.
- Prettier exists only at `ui/node_modules/.bin/prettier` (v3.9.6) and `ProjectHarnessTab.tsx` is NOT
  prettier-formatted (baseline files `src/api.ts`, `src/projectTasks.ts`, `src/App.tsx` all fail `--check`;
  prettifying the file diverges on ~3800 lines). **Never run prettier on this file.**
- `tsc -b` dirties `ui/hub/tsconfig.tsbuildinfo`; revert it after builds.

## Verification of "no business logic lost"
- `handleSaveTask` diff vs HEAD: **only** the success branch changed — it now calls
  `clearTaskCreatorDraftImmediately()` and `setTaskCreatorRestoredFrom(null)` before the existing
  `setIsEditingTask(false)` / `loadTasksAndJobs(...)`. All validation is byte-identical in order:
  project → Task ID required → Task prompt required → `subtaskSaveError` → tag split → verifier spec
  build → `uploadAttachmentsAndComposePrompt` → `scheduleToCron` → create/update → catch `setError`.
  The `catch` branch still leaves the form intact (which is also how the draft survives a failed save).
- Form fields preserved exactly: **15 `form-label`s, 32 `<input>`, 4 `<select>`, 5 `<textarea>`** — identical
  counts to the pre-change dialog. Zero labels lost, zero added.
- New `<button>`s (58 → 66) are the intentional inline affordances: "Discard draft", "Collapse",
  the banner's "Keep editing" / "Start over" pair, and 5 collapsible section headers
  (basics/prompt/verifier/subtasks/schedule).

## Acceptance greps (all confirmed)
- `Create New Project Task|task-editor-drawer|task-editor-backdrop|modal-close-btn` → **0 hits** in
  `ProjectHarnessTab.tsx`. Remaining hits live only in the out-of-scope Chat page
  `ProjectChatTasksDrawer.tsx` (which still reuses those shared CSS classes).
- `role="dialog"` in `ProjectHarnessTab.tsx` → 1 (the unrelated dynamic-prompt/continue-run modal); the
  inline creator uses `<section aria-label>` + `aria-expanded` on headers, no `aria-modal`.
- All 9 `inline-task*` classes used in TSX have matching CSS selectors (105 added CSS lines).
- Component calls draft IO only via `prepareTaskCreator` / `createTaskDraftAutosave` — no direct
  `save/load/clear/persist/isMeaningfulTaskDraft` calls.

## Changed files
Modified:
- `ui/hub/src/components/ProjectHarnessTab.tsx` (+790/−547)
- `ui/hub/src/styles.css` (+105, inline-creator styles only)
- `.agent/notes-plan-task-inline-project-task-create-with-draft.md`
- `.agent/task-create-inline-plan.md` (new recon doc)

New source:
- `ui/hub/src/taskCreateDraft.ts` — store (key/version/TTL/skew guards, save/load/clear/persist)
- `ui/hub/src/taskCreator.ts` — pure state↔draft mapping, defaults, section toggle, restore copy, debounce const
- `ui/hub/src/taskCreateDraftFlow.ts` — `prepareTaskCreator` + `createTaskDraftAutosave`

New tests: `taskCreateDraft.test.ts` (16), `taskCreateDraftFlow.test.ts` (16), `taskCreator.test.ts` (10).

## Storage design
- Key: `openduck.taskCreateDraft.<projectId>` (prefix `TASK_DRAFT_STORAGE_PREFIX`), per-project isolation.
- Envelope: `{ version: 1, projectId, updatedAt, ui{section,expanded}, form, schedule, verifier }`.
- `TASK_DRAFT_TTL_MS` = 30 days; `TASK_DRAFT_MAX_FUTURE_SKEW_MS` = 5 min.
- Degrades to no-op on unavailable/throwing localStorage, corrupt JSON, version/project mismatch,
  stale TTL, future-stamped timestamps, unparseable dates, malformed field values.
- Attachments (`File[]`) and chat transcripts deliberately excluded — handles/blob URLs can't survive reload.

## Restore flow
1. "New Task" / "Create your first Task" → `handleOpenCreateTask` → `prepareTaskCreator(projectId, base, schedule)`.
2. A meaningful saved draft → `taskCreatorStateFromDraft` restores form + schedule + all six verifier fields +
   current section/expanded state; `restoredAt` renders the `inline-task-creator-banner`
   ("Restored your unsubmitted draft from 5 minutes ago") with **Keep editing** / **Start over**.
   An untouched/blank draft is ignored so no bogus banner appears.
3. Typing → `taskCreatorDraftInput` memo → `taskDraftAutosave.update()` → 600 ms debounce →
   `persistTaskDraft` (blank form *clears* rather than storing emptiness).
4. Collapse / ESC / project switch → `flush()` writes the newest snapshot (ref-read to avoid a stale render);
   `dispose({flush})` on unmount. App crash mid-window loses at most the last 600 ms of typing.
5. Cleared on: successful submit (`handleSaveTask` success branch) and explicit discard
   ("Discard draft"/"Start over"/Cancel). `clear()` cancels the queued timer first so a pending write
   can never resurrect the draft.
