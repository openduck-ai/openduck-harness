# Task plan: inline project-task creation + draft persistence

## Subtask 1 — research (DONE)
- Wrote `.agent/task-create-inline-plan.md` with `## 相关文件` + `## 改造方案`.

Key findings (see plan doc for full detail):
- Main dialog: `ui/hub/src/components/ProjectHarnessTab.tsx` — `isEditingTask` (L215) gates a
  `modal-backdrop task-editor-backdrop` + `harness-card task-editor-drawer` block (~L2004-2645).
  Open: `handleOpenCreateTask` (L946), `handleOpenEditTask` (L1262). Save: `handleSaveTask` (L1408).
  ESC: effect L1744 using `taskEditorDismiss.ts`.
- Second (chat drawer) form: `ui/hub/src/components/ProjectChatTasksDrawer.tsx` (leave as-is).
- Storage precedent: raw `localStorage` in `App.tsx`, `api.ts`, `ServerSettings.tsx`;
  mobile has `ui/mobile/src/storage.ts` helper pattern. No i18n framework (English literals).
- Test convention: `node --experimental-strip-types --test src/*.test.ts` in `ui/hub`.

## Subtask 2 — draft persistence module (DONE)
- `ui/hub/src/taskCreateDraft.ts`: `saveTaskDraft` / `loadTaskDraft` / `clearTaskDraft` (+ `persistTaskDraft`,
  `isMeaningfulTaskDraft`, `defaultTaskCreateDraftData`, key `openduck.taskCreateDraft.<projectId>`).
  Versioned + TTL(30d) + sanitizing loader; all storage access try/catch; injectable `{storage, now}`.
- `ui/hub/src/taskCreateDraft.test.ts`: 11 node:test cases (round-trip, per-project isolation, clear,
  corrupt JSON, stale version / project mismatch, TTL, malformed coercion, persist-skips-blank).
- Verified: `pnpm test` 240 pass, `pnpm run typecheck`, `pnpm run build` all green.

## Next subtasks
3. Inline form in ProjectHarnessTab (remove overlay/dialog, keep form JSX), debounced save, restore banner.
4. Remove dead dialog CSS, keep reusable form bits.
5. Run tests / typecheck / build.
