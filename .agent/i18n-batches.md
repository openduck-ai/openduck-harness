# Extraction batches (W1..W10) — 5 grok / 5 agy, disjoint files & key prefixes

Rules for every worker: read `.agent/i18n-agent-rules.md` AND `docs/i18n-extraction-guide.md`
first. You own ONLY your row's files and your row's key prefixes. Your fragment files are
`.agent/i18n-fragments/W<n>.en.json` and `.agent/i18n-fragments/W<n>.zh.json`.

Approximate string counts are from `docs/i18n-inventory.md`; they are guidance, not targets.
Localize EVERY user-visible string you find, including ones the inventory missed.

| # | Agent | Files (under `ui/hub/src/`) | Owned prefixes | ~strings |
|---|---|---|---|---|
| W1 | grok | `components/ProjectHarnessTab.tsx` | `harness.page.*` | 284 |
| W2 | agy | `components/TaskTrajectoryViewer.tsx` | `trajectory.viewer.*` | 235 |
| W3 | grok | `components/TuiAgentsPage.tsx` | `tuiAgents.page.*` | 203 |
| W4 | agy | `components/OpenDuckConfigPanel.tsx`, `configEditor.ts`, `providerConfig.ts` | `config.panel.*`, `config.editor.*`, `config.providers.*` | 225 |
| W5 | grok | `components/AgentLogTranscript.tsx`, `taskTrajectory.ts`, `components/ProjectChatTasksDrawer.tsx` | `trajectory.log.*`, `trajectory.helper.*`, `chat.tasks.*` | 204 |
| W6 | agy | `components/ProjectTerminal.tsx`, `components/ProjectGit.tsx`, `components/GitGraph.tsx`, `components/ProjectRules.tsx`, `rules.ts` | `terminal.page.*`, `git.page.*`, `git.graph.*`, `rules.page.*`, `rules.helper.*` | 180 |
| W7 | grok | `taskPdfExport.ts`, `components/ProjectFiles.tsx`, `components/FilePreviewModal.tsx`, `components/MarkdownPreview.tsx`, `components/RegisterProjectModal.tsx` | `export.pdf.*`, `files.page.*`, `files.preview.*`, `markdown.preview.*`, `projectRegistration.modal.*` | 184 |
| W8 | agy | `components/ServerSettings.tsx`, `components/ProjectChat.tsx`, `components/ProjectList.tsx`, `components/TaskDraftPanel.tsx`, `components/ProjectSettings.tsx`, `projectTasks.ts` | `settings.server.*`, `chat.page.*`, `projectList.page.*`, `harness.draft.panel.*`, `projectSettings.page.*`, `harness.tasks.*` | 227 |
| W9 | grok | `components/NetworkStatusPanel.tsx`, `components/HarnessHistoryTable.tsx`, `components/RunningTasksPage.tsx`, `runningTasks.ts`, `components/MemoryMonitorPanel.tsx`, `components/ProjectInsights.tsx`, `components/Header.tsx`, `App.tsx` | `network.panel.*`, `harness.history.*`, `runningTasks.page.*`, `runningTasks.helper.*`, `memory.panel.*`, `insights.page.*`, `app.shell.*` | 166 |
| W10 | agy | `components/MentionPopover.tsx`, `mention.ts`, `components/ProjectDetail.tsx`, `components/DynamicPromptSelectionMenu.tsx`, `dynamicPrompt.ts`, `components/ProjectRootsEditor.tsx`, `components/useProjectRoots.ts`, `lazyTabs.ts`, `components/Skeletons.tsx`, `taskCreator.ts`, `taskDraft.ts`, `taskContinuation.ts`, `taskAttachments.ts`, `main.tsx` | `mentions.popover.*`, `mentions.helper.*`, `projectDetail.page.*`, `dynamicPrompt.menu.*`, `dynamicPrompt.helper.*`, `projectRoots.*`, `tabs.*`, `skeletons.*`, `harness.tasks.creator.*`, `harness.draft.helper.*`, `main.*` | 122 |

## Notes that apply to specific workers

- **W1 / W2 / W3 / W4** are the four largest files. Work in sequential passes over the SAME
  file (labels+aria+placeholders → sentences/interpolation → formatting → final rescan).
  Never leave a partially-translated file; commit all edits to disk before you report.
- **W5 `taskTrajectory.ts`**: it currently DETECTS English headings (`Failure Reason`,
  `Task Failed`, `Task Incomplete`) to decide semantics. Keep that detection working for
  historical English data — detect on the raw stored string, then produce DISPLAY text via
  translation. Read the file before editing. `ATTACHED_FILES_PROMPT_HEADER` is a serialization
  sentinel used for dedup: keep it byte-stable, do not translate.
- **W7 `taskPdfExport.ts`**: contains pre-existing bilingual copy (English + Chinese) inside a
  PDF HTML template, plus a hardcoded `<html lang="zh-CN">`. Requirements:
  * English catalog values must reproduce the current English PDF text byte-for-byte.
  * The Sichuan/Chinese strings currently hardcoded there must MOVE into the zh catalog
    (`export.pdf.*`) — no hardcoded CJK left in the file.
  * Replace the hardcoded `lang="zh-CN"` with the active locale (read from
    `i18nInstance.language` in `./i18n/index.ts`), normalizing via `normalizeLocale`.
  * `src/taskPdfExport.test.ts` asserts English output — keep it passing.
- **W8 `projectTasks.ts`**: exports are asserted by `src/projectTasks.test.ts`
  (`<1s`, `2m 14s`, `just now`, `3m ago`, `Sep 1`, `Running · …`, `Queued · …`, `N steps`,
  `Skipped · …`). Delegate formatting to `src/i18n/format.ts` (already implemented) and
  translate the surrounding words via `translate()`. All English expectations must still pass
  verbatim.
- **W9 `runningTasks.ts`**: `src/runningTasks.test.ts` asserts exact English status/activity
  strings (`Thinking…`, `Starting…`, `Writing the final answer`, `<1s`, `2m 5s`). Same rule.
- **W10 `lazyTabs.ts`**: `src/skeletons.test.ts` asserts exact English tab labels. Keep
  `getTabLabel()` returning identical English when locale is en.
  `main.tsx` already has the error-boundary wiring; only its remaining literals move.
- **W9 `Header.tsx`** already uses `t('header.*')` for nav + theme. Its owned prefix here is
  `app.shell.*` for anything else it still hardcodes; leave the existing `header.*` keys alone.

## Do NOT touch (any worker)
`src/locales/*.json`, `src/i18n/**` (except that you may READ `format.ts`/`index.ts`),
`src/contexts/**`, `src/components/LocaleSwitcher.tsx`, all `*.test.ts`, `package.json`,
`tsconfig.json`, lockfiles, and other workers' files.
