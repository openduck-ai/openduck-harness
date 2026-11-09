# Brief: W11 — close the remaining i18n gaps (final batch)

Repo: `/mnt/e/openduck-harness`. Area: `ui/hub`.

Read `.agent/i18n-agent-rules.md` and `docs/i18n-extraction-guide.md` first. You are the
LAST extraction worker. A full-repo audit found copy that no earlier worker covered.

You may ONLY modify these source files:
1. `ui/hub/src/i18n/format.ts`            (remove hardcoded `isZh ? ... : ...` branches)
2. `ui/hub/src/components/ProviderStatsDashboard.tsx`
3. `ui/hub/src/components/DynamicPromptSelectionMenu.tsx`
4. `ui/hub/src/components/ProjectRootsEditor.tsx`
5. `ui/hub/src/components/OpenDuckConfigPanel.tsx`   (only the `OLLAMA_HOST` placeholder + any leftover copy)
6. `ui/hub/src/components/ProjectDetail.tsx`
7. `ui/hub/src/components/Skeletons.tsx`             (only if copy remains)
8. `ui/hub/src/components/TaskCreateModal.tsx`       (only if copy remains after W10)
9. `ui/hub/src/components/MentionPopover.tsx`        (only if copy remains)
10. `ui/hub/src/mention.ts`                          (only `extra: 'Attached file'`, `'Rule source'` and any other user-visible strings)

Fragments: `.agent/i18n-fragments/W11.en.json` / `W11.zh.json`.
Owned prefixes: `format.relative.*`, `format.duration.*`, `config.stats.*`, `dynamicPrompt.selection.*`,
`projectRoots.editor.*`, `projectDetail.workspace.*`, `mention.items.*`.

## DO NOT TOUCH
`ui/hub/src/locales/*.json`, `src/i18n/index.ts`, `src/i18n/locales.ts`, `src/i18n/catalogs.ts`,
`src/contexts/*`, `src/components/LocaleSwitcher.tsx`, `package.json`, lockfiles, any `*.test.ts`.

## Defect 1 (highest priority): `src/i18n/format.ts` hardcodes Chinese
`formatRelativeTime` and the duration formatter contain literal `isZh ? '<1分钟后' : 'in <1m'`
ternaries and `isZh ? ... : ...` branches building `${hours}小时` etc. That is exactly the
"no hardcoded Chinese in code" violation this task forbids, and it bypasses the catalogs.

Refactor so the words come from the catalog:
- Add `format.relative.*` keys, e.g. `inLessThanMinute`, `justNow`, `minutesAgo`, `inMinutes`,
  `hoursAgo`, `inHours`, `daysAgo`, `inDays` — each with `{{count}}` where needed.
- Add `format.duration.*` keys for the hour/minute/second unit words used by the duration
  formatter (`{{hours}}小时` becomes `{{hours}}h` in en / `{{hours}}小时` in zh).
- Use the non-hook `translate(key, values, options)` from `./index.ts` INSIDE the functions.
- IMPORTANT: `formatRelativeTime` takes an explicit `options.locale`. `translate` must be
  invoked with that locale (`translate(key, values, { locale })` — check the real signature in
  `src/i18n/index.ts` and use whatever it exposes; if it has no locale option, resolve the
  string from the catalog resource for that locale instead). English output must stay
  byte-identical to today: `just now`, `in <1m`, `<1m ago` is NOT used, `5m ago`, `in 5m`,
  `3h ago`, `2d ago`.
- After the change there must be ZERO CJK in `format.ts`.

## Defect 2: copy still hardcoded in components
Audit hit list (not exhaustive — localize everything user-visible you find in your files):
- `ProviderStatsDashboard.tsx`: `Total Invocations`, `Success Rate`, `Retries & Failovers`,
  `Avg Call Latency`, `Zero retry overhead`, `Search provider, model, retry reason, error...`,
  `Filter by provider`, `Success`, `Failed`, `Retries`, `Time To First Byte / Token`,
  `Stream receive duration`, `Hide`, `Details` and any chart-axis/legend/labels.
- `DynamicPromptSelectionMenu.tsx`: `Run with selection`, `No tasks accept a dynamic prompt.`,
  `Confirm prompt`, `Current prompt`, and the rest of the menu.
- `ProjectRootsEditor.tsx`: `No roots configured yet. Add a parent folder for your projects.`,
  `Missing`, `Add root directory`.
- `OpenDuckConfigPanel.tsx`: `placeholder="OLLAMA_HOST"` -> catalog key (en value stays
  `OLLAMA_HOST`).
- `ProjectDetail.tsx`: `PROJECT WORKSPACE`.
- `Skeletons.tsx`: `Action` (likely an aria-label).
- `mention.ts`: `'Attached file'`, `'Rule source'`.

## Constraints
- Never translate protocol/enum values, `itemType` values, CSS classes, or API fields.
- `ProviderStatsDashboard` chart internals: localize legend/axis/tooltip text only.
- Every key in your fragments must be reached by real code; English values must equal the
  current hardcoded strings byte-for-byte (no re-wording, no trailing spaces).
- en and zh fragments: identical key sets, identical placeholder sets, trimmed non-empty values.

## Verify before reporting
```
cd ui/hub
pnpm run i18n:merge
pnpm run typecheck                     # must pass
pnpm test 2>&1 | tail -20              # must be 317 pass / 0 fail
grep -nP '[\x{4e00}-\x{9fff}]' src/i18n/format.ts src/components/ProviderStatsDashboard.tsx \
  src/components/DynamicPromptSelectionMenu.tsx src/components/ProjectRootsEditor.tsx \
  src/components/ProjectDetail.tsx src/mention.ts    # must print nothing
```
Report: keys added, how you removed the `isZh` ternaries, and the final test counts.
