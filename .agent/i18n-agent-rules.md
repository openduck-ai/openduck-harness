# Hub i18n extraction — SHARED RULES (read before every edit)

Repo: /mnt/e/openduck-harness. Area: `ui/hub`. Read `docs/i18n-extraction-guide.md` first
(authoritative conventions) and `docs/i18n-inventory.md` (per-file copy inventory).
You are ONE of several agents working in parallel on disjoint file sets.

## ABSOLUTE file boundaries
You may ONLY modify:
1. The specific source files assigned to you in your task prompt.
2. Your own two fragment files: `.agent/i18n-fragments/<OWNER>.en.json` and
   `.agent/i18n-fragments/<OWNER>.zh.json` (`<OWNER>` given in your prompt).

NEVER modify, and never even open for writing:
- `ui/hub/src/locales/en.json`, `ui/hub/src/locales/zh.json`  (a merge step owns these)
- `ui/hub/src/i18n/*`, `ui/hub/src/contexts/LocaleContext.tsx`, `ui/hub/src/components/LocaleSwitcher.tsx`
- `ui/hub/package.json`, `ui/hub/tsconfig.json`, `ui/*/pnpm-lock.yaml`, root `package.json`
- any `*.test.ts`, any file assigned to another agent
- any other agent's `.agent/i18n-fragments/*.json`

Do not run `pnpm install`, `git commit`, `git checkout`, `git stash`, or `cargo` anything.

## Fragment format (this is how your catalog entries get merged)
Write ONE nested JSON object per fragment file holding exactly your owned subtree,
using `{{placeholder}}` interpolation. Both `en` and `zh-CN` fragments MUST contain
byte-identical key sets and byte-identical placeholder sets.

`.agent/i18n-fragments/<OWNER>.en.json`:
```json
{ "yourPrefix": { "someThing": "Some thing", "greeting": "Hello {{name}}" } }
```

Plural keys: use i18next suffixes `_one` / `_other`, and include BOTH in BOTH languages.

## English fidelity (hard requirement)
English catalog values must reproduce today's rendered copy EXACTLY: capitalization,
`…` (U+2026) vs `...`, curly vs straight quotes, punctuation, emoji, spacing.
Do NOT "improve" wording. If the current code says `Retry request`, the English value is
`Retry request`.

## Whitespace / sentence integrity (ENFORCED by `src/i18n.test.ts`)
Catalog values must be TRIMMED: no leading/trailing spaces or newlines, no double spaces,
no empty strings. Keep that whitespace in the JSX/code instead:
- `<Icon /> Attach files` → `<Icon />{' '}{t('...')}` with value `Attach files`
- `` ` (${n} tok/s)` `` → `` ` ${t('...', { n })}` `` with value `({{n}} tok/s)`
- `` `\nTools: ${x}` `` → `` `\n${t('...', { tools: x })}` `` with value `Tools: {{tools}}`
NEVER split one sentence into `...Prefix` / `...Suffix` keys around an element. Use
`<Trans i18nKey="..." values={{...}} components={{ strong: <strong /> }} />` with the whole
sentence in one catalog value, e.g. `Assign session <strong>{{name}}</strong> to a target Task Run ID.`
Both catalogs must keep the same component tag names.

## Chinese quality
Natural, idiomatic Simplified Chinese. Consistent terminology: Project=项目, Task=任务,
Subtask=子任务, Session=会话, Provider=提供商, Model=模型, Workspace=工作区,
Settings=设置, Run=运行, Failed=失败, Completed=已完成, In progress=进行中,
Transcript=记录, Trajectory=执行轨迹, Rule=规则, File=文件, Repository=仓库, Branch=分支,
Commit=提交, Token=令牌, Budget=预算, Verification=验证, Schedule=调度, Draft=草稿.
Use full-width Chinese punctuation （，。：） where natural. Keep technical tokens, file
paths, IDs, HTTP verbs, and command names untranslated.

## Code rules
- React components: `const { t } = useTranslation()`; replace literals with `t('prefix.key')`.
  Import path from `src/components/*.tsx` is `'react-i18next'`.
- Pure `.ts` helpers: `import { translate } from './i18n/index.ts'` (mind the relative depth),
  and call it INSIDE functions, never at module top level.
- NEVER translate protocol values: URL `page`/`tab`/`split` tokens, `itemType` discriminants,
  git diff grammar/status tokens, provider/model IDs and names, config keys, CSS class names,
  cron strings, keyboard event keys, API payload fields, file paths, terminal output, raw
  backend error text.
- Recompute localized labels during render. If you use `useMemo`/`useCallback`/`useEffect` and
  the value contains translated text, add `locale` from `useLocale()` (or `i18n.language`) to
  the dependency array. Do not freeze translated strings into `useState`.
- Preserve every existing comment's meaning; never introduce Chinese in code or comments.
- Keep JSX structure, classNames, ids, data-* attributes, keys, and behavior identical.

## Formatting (dates / numbers / durations)
Use the shared helper module `src/i18n/format.ts` (already implemented). Import from a
component: `import { formatDateTime, formatNumber, ... } from '../i18n/format.ts'`.
Replace raw `someDate.toLocaleString()` / `.toLocaleDateString()` / `.toLocaleTimeString()`
with `formatDateTime(...)` / `formatDate(...)` / `formatTime(...)`; replace `n.toFixed(k)`
used for DISPLAY with `formatNumber(n, { minimumFractionDigits: k, maximumFractionDigits: k })`
ONLY when the digit shape must stay locale-aware; if a test asserts an exact English shape
(like `2m 5s`), prefer the existing helper or keep `toFixed` for that specific value.
Read `src/i18n/format.ts` first for real signatures — do not guess.

## Existing tests must stay green
`cd ui/hub && pnpm test` (308 tests baseline) asserts exact English strings produced by pure
`.ts` helpers (`taskTrajectory.ts`, `lazyTabs.ts`, `configEditor.ts`, `providerConfig.ts`,
`projectTasks.ts`, `runningTasks.ts`, `taskDraft.ts`, `taskCreator.ts`, `taskContinuation.ts`,
`taskPdfExport.ts`, `rules.ts`, `dynamicPrompt.ts`, `mention.ts`). Because `translate()`
falls back to English before init, byte-identical English values keep those tests passing.
If a test breaks, your English value is wrong — fix the value, never the test.

## Self-check before you finish
1. `cd ui/hub && pnpm run typecheck` → must pass.
2. `cd ui/hub && pnpm test 2>&1 | tail -20` → must stay at 308 pass / 0 fail.
3. `grep -nP '[\x{4e00}-\x{9fff}]' <your files>` → must return nothing.
4. Manually re-scan your files for remaining user-visible English literals in JSX text
   nodes, `title=`, `aria-label=`, `placeholder=`, `alt=`, empty states, error strings,
   button labels, table headers, tooltips, and toast messages.
5. Fragment files valid JSON, en/zh key parity verified (you may run
   `node -e` to diff key sets).

Report: files changed, count of keys added, and the two fragment paths.
