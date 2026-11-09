# Brief: make `i18n-merge.mjs` prune deleted/renamed keys (idempotent rebuild)

Repo: /mnt/e/openduck-harness. Area: `ui/hub`.

## Problem
`ui/hub/scripts/i18n-merge.mjs` currently only ADDS fragment keys into
`ui/hub/src/locales/en.json` / `zh.json`. When a worker renames or deletes a key, the old key
stays in the catalog forever. Real breakage right now: `tuiAgents.page.transferAssignPrefix`
and `tuiAgents.page.transferAssignSuffix` were replaced by a single `tuiAgents.page.transferAssign`
key, but the two stale keys remain in both catalogs, and
`ui/hub/src/i18n.test.ts` ("catalog parity") fails on them.

## Fix
Make the merge **idempotent**: the catalog must converge to
`foundation keys  +  union(fragments)`, so re-running the merge after a key is removed from a
fragment actually removes it from the catalog.

### 1. New file `.agent/i18n-fragments/owners.json`
An explicit ownership manifest listing, for each fragment owner id (file basename without
`.en.json`/`.zh.json`), the dot-prefixes it exclusively owns:

```json
{
  "W1": ["harness.page"],
  "W2": ["trajectory.viewer"],
  "W3": ["tuiAgents.page"],
  "W4": ["config.panel", "config.editor", "config.providers"],
  "W5": ["trajectory.log", "trajectory.helper", "chat.tasks"],
  "W6": ["terminal.page", "git.page", "git.graph", "rules.page", "rules.helper"],
  "W7": ["export.pdf", "files.page", "files.preview", "markdown.preview", "projectRegistration.modal"],
  "W8": ["settings.server", "chat.page", "projectList.page", "harness.draft.panel", "projectSettings.page", "harness.tasks"],
  "W9": ["network.panel", "harness.history", "runningTasks.page", "runningTasks.helper", "memory.panel", "insights.page", "app.shell"],
  "W10": ["mentions.popover", "mentions.helper", "projectDetail.page", "dynamicPrompt.menu", "dynamicPrompt.helper", "projectRoots", "tabs", "skeletons", "harness.tasks.creator", "harness.draft.helper", "main"]
}
```

### 2. Prune step in `i18n-merge.mjs`
Before applying fragments for a locale:
- Collect the union of owned prefixes whose owner has at least one fragment file present
  (`.en.json` for `en`, `.zh.json` for `zh`). An owner with no fragment file for that locale
  is skipped entirely — do NOT prune its prefix.
- Delete from the catalog every leaf whose dot-path equals an owned prefix, or starts with
  `<ownedPrefix>.`. Never delete a `PROTECTED` key.
- If `owners.json` lists a prefix that no present fragment actually provides, that is fine:
  it just means the owner removed everything.
- If a fragment provides a key OUTSIDE its declared owned prefixes, fail with a clear error
  (that would break the disjointness the whole scheme relies on). Also fail if two owners
  declare overlapping prefixes.
- Missing `owners.json` -> fall back to today's additive behavior and print a warning.
- Keep the existing lock, sorted output, protected keys, and parity `checkParity` call.

### 3. `i18n-check.mjs`
Keep/extend so it reports, in addition to current checks: stale catalog leaves that belong to a
present owner's prefix but appear in NO fragment (these are exactly the prunable ones). Exit
non-zero if any exist, so drift is caught.

## Files you may modify
- `ui/hub/scripts/i18n-merge.mjs`
- `ui/hub/scripts/i18n-check.mjs`
- `.agent/i18n-fragments/owners.json` (new)

Do not modify `ui/hub/src/**`, other fragment JSON files, `package.json`, lockfiles, or tests.

## Verification (run all, report transcripts)
1. `cd ui/hub && pnpm run i18n:merge` -> must now REMOVE the two stale
   `tuiAgents.page.transferAssignPrefix/Suffix` keys. Afterwards:
   `grep -c 'transferAssignPrefix' src/locales/en.json src/locales/zh.json` must be 0
   (exit 1 from grep means 0 matches — that is success).
2. `cd ui/hub && pnpm test 2>&1 | tail -20` -> **317 pass / 0 fail** (currently 316/1, and that
   1 failure is the stale pair above).
3. `cd ui/hub && pnpm run typecheck` -> pass.
4. `cd ui/hub && pnpm run i18n:merge` a SECOND time -> must report `changed=0` (idempotent).
5. Confirm the 19 foundation keys still exist: `node -e "const e=require('./src/locales/en.json'); console.log(e.language.label, e.common.cancel, e.header.projects)"`.
6. Confirm no keys from a not-yet-run owner were removed: W5..W10 have no fragments yet, so
   their prefixes must be untouched.
Report: what you changed, the merge output lines, and the final test counts.