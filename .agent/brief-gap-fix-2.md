# Brief: W12 — final i18n gap closure (last pass)

Repo: `/mnt/e/openduck-harness`. Area: `ui/hub`.

Read `.agent/i18n-agent-rules.md` and `docs/i18n-extraction-guide.md` first.

A full-repo TypeScript-AST audit (script at `/tmp/audit4.mjs`, run with
`cd /mnt/e/openduck-harness && node /tmp/audit4.mjs`) now reports 42 untranslated prose literals
across 10 files. Most are CORRECTLY not translatable (see "leave alone"). Fix only the items
under "MUST FIX".

You may ONLY modify:
- `ui/hub/src/components/useProjectRoots.ts`
- `ui/hub/src/components/RegisterProjectModal.tsx` (only if it renders its own fallback error copy)
- `ui/hub/src/components/ServerSettings.tsx` (only if it renders its own fallback error copy)
- your fragments `.agent/i18n-fragments/W12.en.json` / `W12.zh.json`
- and `.agent/i18n-fragments/owners.json` (add `"W12": ["projectRoots.errors"]`)

Do NOT touch `src/locales/*.json`, `src/i18n/index.ts|locales.ts|catalogs.ts`, `src/contexts/*`,
`LocaleSwitcher.tsx`, `package.json`, any `*.test.ts`, or any file another owner localized.

## MUST FIX

### 1. `src/components/useProjectRoots.ts` — user-visible error copy (4 strings)
These land in component state and are rendered by
`src/components/ProjectRootsEditor.tsx:102` as `<p className="error">{message}</p>`, and also via
`ServerSettings.tsx` / `RegisterProjectModal.tsx`. They are real user-facing errors.
- `describeRootError(..., fallback)` — the literal
  `'That folder is outside the home directory. Allow roots outside the home directory, then try again.'`
- the three call sites' fallbacks: `'Unable to load project roots'`,
  `'Unable to add project root'`, `'Unable to remove project root'`.
Move all four to keys `projectRoots.errors.*` (`outsideHome`, `loadFailed`, `addFailed`,
`removeFailed`). Because this hook is a React hook it may use `useTranslation()`, but
`describeRootError` is a plain function — prefer the non-hook `translate()` from
`../i18n/index.ts` (note the path from `src/components/` is `../i18n/index.ts`).
Keep the `allowUntrustedPath` / `outside $HOME` substring checks intact — they inspect the
SERVER's English error text, so do not translate those patterns.

### 2. `src/providerConfig.ts` — `IDENTICAL_ROLE_TARGET_WARNING`
`export const IDENTICAL_ROLE_TARGET_WARNING = 'Identical targets cause duplicate invocations'`;
it is asserted by `src/providerConfig.test.ts:96,403`, so it must stay an exported English
constant with that exact value. It IS user-visible (a validation warning). Do the same trick the
codebase already uses for other constants: keep the constant as-is for the tests, but add a
`getIdenticalRoleTargetWarning()` that returns a catalog value, and switch any COMPONENT that
displays the warning to the getter. Only touch `providerConfig.ts` if you can do this without
breaking `providerConfig.test.ts`; otherwise leave it and say so in your report.

## LEAVE ALONE (verified NOT user-visible — do not "fix")
- `gitDiff.ts` — git porcelain/diff parser tokens (`'new file'`, `'rename from'`, `'similarity index'`). Protocol data.
- `taskTrajectory.ts` — `historicalTitles` are HISTORICAL markdown headings that must stay English
  (there is already a comment: "Historical headings are stored in English, regardless of the
  display locale"); the `.includes('Failure Reason')` / `'Completed Upstream Subtasks'` strings are
  parsers matching already-stored English text.
- `ProjectTerminal.tsx` — shell commands shown as examples (`git status`, `cargo test`), agent
  brand names, URL/path placeholders, `Esc` / `Enter ↵` / `Ctrl+C` keycaps.
- `OpenDuckConfigPanel.tsx` — `http://127.0.0.1:8080`, `custom.point`, `true`/`false`.
- `ProjectRootsEditor.tsx` — `/home/user/projects` example path placeholder.
- `contexts/LocaleContext.tsx`, `contexts/ThemeContext.tsx` — developer-only "must be used within
  a Provider" invariant errors, never shown to users.
- `taskAttachments.ts` — LLM prompt scaffolding, not UI.
- `taskPdfExport.ts` — PDF binary stream markers.
- `ProjectHarnessTab.tsx` — CSS class strings.

## Verify
```bash
cd ui/hub
pnpm run i18n:merge                    # run TWICE; second run must say changed=0
pnpm run typecheck                     # must pass
pnpm test 2>&1 | tail -20              # must be 317 pass / 0 fail
cd /mnt/e/openduck-harness && node /tmp/audit4.mjs | head -5   # expect hits to drop to ~30 and
                                                              # useProjectRoots/providerConfig gone
```
Report: keys added, exact isZh-free evidence, and final counts.
