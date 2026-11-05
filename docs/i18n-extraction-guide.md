# Hub i18n extraction contract

Design only, 2026-11-04. Paths are relative to `ui/hub/src` unless stated otherwise.
The existing i18next 26 / react-i18next 17 runtime is authoritative. The inventory's older
react-intl, ICU, descriptor-catalog and IntlProvider proposals are superseded. Keep the five
foundation modules and existing keys intact. No components or helpers are implemented here.

## A. Runtime and helper rules
- Pure `.ts` helpers import `translate` from `./i18n/index.ts` (adjust relative depth); never import `.tsx`, context barrels, browser globals or Node-only APIs. Keep explicit `.ts` specifiers and `import type`.
- Translate inside each invocation, never at module initialization. With no initialization, `translate()` reads English JSON and interpolates values; it does not detect browser preferences. With initialization it reads the current singleton language.
- React consumers must subscribe through `useTranslation()` or `useLocale()`, then recompute helper output during render. Calling `translate()` alone does not subscribe React. Keep locale in dependencies of every memo/callback/effect that computes or captures localized output; avoid storing translated labels in state.
- Replace `MODE_OPTIONS`, `JUDGE_MODES`, `JUDGE_POINT_LABELS` and known `CONFIG_LABELS` strings with `getModeOptions()`, `getJudgeModes()`, `getJudgePointLabels()` and invocation-time `configLabel(key)` lookups. Stable value/key arrays may stay constant.
- `lazyTabs.ts` already has `getTabLabel(tab)`: retain its switch and translate each literal-key branch at invocation time; do not translate `PROJECT_SUBTABS` or `getTabModulePath()`.
- `providerConfig.ts` already constructs model labels inside `modelChoiceState()`: translate there. Any extracted fallback-label maps must store keys or become getters. Leave provider/model IDs, names and option values unchanged.

Canonical helper and render consumer (examples show proposed APIs):
```ts
import { translate } from './i18n/index.ts'
export function getModeOptions() {
  return [{ value: '', label: translate('config.editor.mode.notSet') }] as const
}
export function describeSubtask(n: number): string {
  return translate('harness.draft.validation.subtask', { n })
}
```
```tsx
import { useLocale } from '../contexts/LocaleContext.tsx'
import { getModeOptions } from '../configEditor.ts'
function ModeSelect() {
  const { locale } = useLocale()
  const options = getModeOptions()
  // If memoization is necessary: useMemo(() => getModeOptions(), [locale]).
  return <select lang={locale}>{options.map(o =>
    <option key={o.value} value={o.value}>{o.label}</option>)}</select>
}
```

Node tests remain plain `node --experimental-strip-types --test src/*.test.ts`; no TSX imports.
Tests that change language must initialize explicit `en`, restore it afterward, and avoid
concurrent singleton mutations. Existing English expectations remain unchanged.

## B. Exclusive key ownership
Each row owns ONLY the indicated subtree in both nested JSON catalogs; no row owns its
parent. Thus `trajectory.viewer.*`, `trajectory.helper.*`, `trajectory.log.*` are disjoint.
All 48 copy-bearing files in inventory section 5 are covered. Paths below are literal.

| File | Exclusive prefix |
| --- | --- |
| `components/ProjectHarnessTab.tsx` | `harness.page.*` |
| `components/TaskTrajectoryViewer.tsx` | `trajectory.viewer.*` |
| `components/TuiAgentsPage.tsx` | `tuiAgents.page.*` |
| `components/OpenDuckConfigPanel.tsx` | `config.panel.*` |
| `components/AgentLogTranscript.tsx` | `trajectory.log.*` |
| `components/ProviderStatsDashboard.tsx` | `providers.stats.*` |
| `components/ProjectChatTasksDrawer.tsx` | `chat.tasks.*` |
| `taskTrajectory.ts` | `trajectory.helper.*` |
| `components/ProjectTerminal.tsx` | `terminal.page.*` |
| `components/ProjectGit.tsx` | `git.page.*` |
| `components/ProjectRules.tsx` | `rules.page.*` |
| `taskPdfExport.ts` | `export.pdf.*` |
| `components/ProjectFiles.tsx` | `files.page.*` |
| `components/RegisterProjectModal.tsx` | `projectRegistration.modal.*` |
| `components/ServerSettings.tsx` | `settings.server.*` |
| `configEditor.ts` | `config.editor.*` |
| `components/ProjectChat.tsx` | `chat.page.*` |
| `components/ProjectList.tsx` | `projectList.page.*` |
| `components/TaskDraftPanel.tsx` | `harness.draft.panel.*` |
| `components/ProjectSettings.tsx` | `projectSettings.page.*` |
| `components/NetworkStatusPanel.tsx` | `network.panel.*` |
| `components/HarnessHistoryTable.tsx` | `harness.history.*` |
| `components/RunningTasksPage.tsx` | `runningTasks.page.*` |
| `providerConfig.ts` | `config.providers.*` |
| `components/FilePreviewModal.tsx` | `files.preview.*` |
| `projectTasks.ts` | `harness.tasks.*` |
| `components/MentionPopover.tsx` | `mentions.popover.*` |
| `components/ProjectDetail.tsx` | `projectDetail.page.*` |
| `components/MemoryMonitorPanel.tsx` | `memory.panel.*` |
| `components/DynamicPromptSelectionMenu.tsx` | `dynamicPrompt.menu.*` |
| `components/ProjectInsights.tsx` | `insights.page.*` |
| `App.tsx` | `app.shell.*` |
| `components/Header.tsx` | `header.*` |
| `runningTasks.ts` | `runningTasks.helper.*` |
| `components/ProjectRootsEditor.tsx` | `projectRoots.editor.*` |
| `lazyTabs.ts` | `projectDetail.tabs.*` |
| `mention.ts` | `mentions.helper.*` |
| `components/Skeletons.tsx` | `skeletons.*` |
| `components/MarkdownPreview.tsx` | `markdown.preview.*` |
| `components/useProjectRoots.ts` | `projectRoots.hook.*` |
| `taskCreator.ts` | `harness.draft.restore.*` |
| `components/GitGraph.tsx` | `git.graph.*` |
| `main.tsx` | `app.errorBoundary.*` |
| `rules.ts` | `rules.validation.*` |
| `taskDraft.ts` | `harness.draft.validation.*` |
| `taskContinuation.ts` | `harness.continue.*` |
| `dynamicPrompt.ts` | `dynamicPrompt.helper.*` |
| `taskAttachments.ts` | `attachments.helper.*` |

Infrastructure owner alone owns `format.*`, proposed `schedule.*`, `language.*`, `common.*`
and `app.documentTitle`. Other agents may reuse existing shared keys only when semantics,
capitalization and punctuation match exactly; propose new shared keys to that owner.
To avoid physical JSON conflicts, parallel agents submit key/value patches for their assigned
subtrees; ONE integrator merges catalogs. Never rewrite/sort the entire catalog from a worker.
No two agents edit one source file. A consumer change outside an assignment goes to its owner.

## C. Messages and fidelity
- Nested JSON string leaves; dot-separated semantic lowerCamelCase keys: `trajectory.viewer.empty.title`. No English-copy keys, array indexes, or concatenating English text into a key. Prefer literal keys or an exhaustive stable-token-to-literal-key map.
- English values must byte-match today's rendered copy: case, U+2026 versus `...`, quotes, emoji, punctuation, whitespace and newlines. Do not change existing tests to accommodate wording changes. Preserve existing bilingual English-mode PDF copy inside English catalog values.
- Use `{{name}}` interpolation, e.g. `Subtask {{n}}`. Keep values as user/backend data; never translate paths, prompts, terminal output or raw error details. Catalogs must have identical key and placeholder sets.
- Native i18next v26 plurals use `steps_one` / `steps_other` with numeric `{ count }`; call `t('runningTasks.helper.steps', { count })`. Both variants must exist in BOTH catalogs (Chinese `_one` may equal `_other`) for strict parity. Do not use ICU `{count, plural, ...}` or add an ICU plugin.
- IMPORTANT: the current uninitialized `translate()` fallback only resolves literal keys and interpolation; it does NOT select plural suffixes. Node-facing helpers must explicitly select literal `_one`/`_other` keys, e.g. `translate(count === 1 ? 'runningTasks.helper.steps_one' : 'runningTasks.helper.steps_other', { count })`. This preserves no-init English behavior and Chinese parity without changing foundation code. Use this only for current integer English singular/plural logic; expanding languages needs a separate design.
- Prefer one complete sentence with named placeholders over concatenated fragments. For rich JSX, use `useTranslation()` plus `<Trans t={t} i18nKey="files.page.openHint" values={{ name }} components={{ link: <a href={url} /> }} />`; catalog: `Open <link>{{name}}</link>.` Preserve named component tags in both catalogs; keep URLs/behavior in code and never insert catalog HTML with `dangerouslySetInnerHTML`.
- Forbid hardcoded Chinese in ANY `.tsx`, including comments. No owned visible literal text, accessibility labels, tooltips, placeholders or alt text in JSX. Punctuation-only decoration may remain after review.
- Never translate protocol tokens: `itemType` discriminants, git grammar/status tokens, provider/model IDs, URL page/tab values, config keys, CSS classes, cron/input serialization or API payloads. Translate their presentation through explicit mappings.

## D. Formatting layer specification (future implementation)
Create `i18n/format.ts`, pure `.ts`, no DOM access at import. Import the existing singleton,
`translate`, `normalizeLocale`, and `Locale` with explicit `.ts` paths. Resolve locale at EACH
call as `normalizeLocale(i18nInstance.language || 'en')`, unless the caller passes `locale`.
Map only `en -> en-US`, `zh-CN -> zh-CN`; never pass `undefined`/`[]` as an Intl locale.
Proposed signatures (all return `string`; option types are exported):

```ts
type DateInput = Date | number | string // number = epoch milliseconds; string = ISO instant
type DateOptions = Intl.DateTimeFormatOptions & { locale?: Locale }
type NumberOptions = Intl.NumberFormatOptions & { locale?: Locale }
formatDate(value: DateInput, options?: DateOptions): string
formatDateTime(value: DateInput, options?: DateOptions): string
formatTime(value: DateInput, options?: DateOptions): string
formatNumber(value: number, options?: NumberOptions): string
formatPercent(ratio: number, options?: NumberOptions): string
formatRelativeTime(iso: string | null | undefined, now: number,
  options?: { locale?: Locale; future?: 'clamp' | 'relative' }): string
formatDurationMs(ms: number, options?: { locale?: Locale }): string
formatExportDuration(ms: number, options?: { locale?: Locale }): string
```

Strip `locale` before passing options to Intl. Dates use `Intl.DateTimeFormat(tag, options)`;
numbers use `Intl.NumberFormat(tag, options)`. Default date fields: numeric year/month/day;
datetime adds numeric hour, 2-digit minute/second; time uses only those time fields. When a
caller supplies date/time fields or styles, replace default fields rather than mixing them.
Keep local timezone for existing unzoned UI timestamps; explicit `timeZone` overrides it.
Tests pass `timeZone: 'UTC'` (or `TZ=UTC`); locale alone does not make timezone deterministic.
Invalid dates return `''`; callers retain their existing `—`/empty fallback decisions.
Default number formatting matches native numeric `toLocaleString()`; fixed decimal sites
must set BOTH fraction-digit bounds and `useGrouping: false` to preserve `toFixed` output.
Percent takes a RATIO, defaults to percent style with one fractional digit; a value already
in 0–100 percentage points must be divided by 100. Keep dollar/unit placement in messages.

English compatibility, observed with Node en-US and UTC for `2024-09-01T12:34:56Z`:

| Current operation / input | Required English output |
| --- | --- |
| default `toLocaleDateString()` / `formatDate` | `9/1/2024` |
| default `toLocaleString()` / `formatDateTime` | `9/1/2024, 12:34:56 PM` |
| default `toLocaleTimeString()` / `formatTime` | `12:34:56 PM` |
| date with `{ month: 'short', day: 'numeric', timeZone: 'UTC' }` | `Sep 1` |
| number `1234567.89` | `1,234,567.89` |
| percent ratio `0.125`, one fractional digit | `12.5%` |
| duration ms `500, 999, 1000, 2140, 45000` | `<1s`, `<1s`, `1s`, `2s`, `45s` |
| duration ms `120000, 134000, 3600000, 3780000` | `2m`, `2m 14s`, `1h`, `1h 3m` |
| relative at `2024-10-07T12:00:00Z`: 30s, 3m, 5h, 2d ago; Sep 1 | `just now`, `3m ago`, `5h ago`, `2d ago`, `Sep 1` |
| export duration ms `0, 450, 1250, 65000, 125000` | `0s`, `450ms`, `1.25s`, `1m 5s`, `2m 5s` |

`projectTasks.ts` already pins the old-date fallback to en-US/UTC. Other default native calls
currently depend on ambient locale/timezone; deliberately pin English locale to en-US while
preserving timezone choices. Do not use `dateStyle: 'medium'` as a blanket replacement.

Keep numeric/threshold logic verbatim; localize unit/message templates through `format.*`:
- Compact duration: `<1000ms -> <1s`; otherwise round total seconds, split hours/minutes/seconds, omit zero components, join with single spaces. Use ungrouped integer numerals. Chinese units: `小时`, `分钟`, `秒`; retain the same join/omission rules and `<1秒`.
- Relative time: missing/invalid ISO -> `''`; default clamps future timestamps to now. Floor seconds; `<45s -> just now`, `<3600s -> max(1,floor(seconds/60))m ago`, `<86400s -> floor(seconds/3600)h ago`, `<604800s -> floor(seconds/86400)d ago`; otherwise short month/day in UTC. Chinese templates: `刚刚`, `{{count}}分钟前`, `{{count}}小时前`, `{{count}}天前`.
- Optional `future: 'relative'` is for NEW callers only: future <45s -> `in <1m`; then use the same minute/hour/day thresholds on absolute difference, producing `in 2h` at +7200s; >=7d uses the same UTC date. Chinese: `<1分钟后`, `{{count}}分钟后`, `{{count}}小时后`, `{{count}}天后`. Existing callers must retain clamping.
- Hand-roll these compact message templates; `Intl.RelativeTimeFormat` does not guarantee `3m ago`. Use Intl for date/number primitives, not for changing relative wording.
- `runningElapsedLabel` continues clamping negative elapsed values and using zero elapsed for invalid starts; its tested output is `2m 5s` and `<1s` respectively.
- Export duration is a separate helper: <=0 -> `0s`; <1000 -> raw ms + `ms`; <60 seconds -> exactly two decimals + `s`; otherwise floor minutes plus rounded remaining seconds, INCLUDING zero seconds. Preserve the existing possible `1m 60s` rounding edge; no silent normalization.
- Draft-restored age keeps its own floored minute/hour/day thresholds, singular/plural and complete sentence templates in `harness.draft.restore.*`; do not replace it with compact relative text.
- Preserve existing helper exports as wrappers for callers/tests. One infrastructure owner implements shared formatting; feature owners consume it. Formatting explicit-locale overrides must also choose matching message catalog text, without mutating the global language (use initialized `getFixedT(locale)` or English/Chinese catalog lookup with the same fallback semantics).

## E. Review gates and exact commands
Run from repository root. These are implementation review commands, not builds to run for
this design-only task. Greps report candidates (exit 1 means none); review existing fixtures
and syntax, never mass-delete matches. CJK tests are excluded because they contain user data.

```bash
rg -n '[\p{Han}\p{Hiragana}\p{Katakana}\p{Hangul}]' ui/hub/src --glob '!locales/**' --glob '!**/locales/**' --glob '!*.test.ts'
rg -n '(aria-label|aria-description|title|placeholder|alt)\s*=\s*["\x27]' ui/hub/src --glob '*.tsx'
rg -n '>[^<{]*[A-Za-z][^<{]*<' ui/hub/src --glob '*.tsx'
rg -n 'toLocale(String|DateString|TimeString)\(|toFixed\(' ui/hub/src --glob '!*.test.ts'
```

Use this AST gate to catch multiline JSX and expression-wrapped string/template literals;
review flagged expressions for owned copy versus protocol/data. It writes no files:

```bash
cd ui/hub
node --input-type=module <<'NODE'
import fs from 'node:fs'; import path from 'node:path'; import ts from 'typescript';
let bad = false;
function walk(dir) { for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
  const p = path.join(dir, e.name); if (e.isDirectory()) { walk(p); continue; }
  if (!p.endsWith('.tsx')) continue;
  const sf = ts.createSourceFile(p, fs.readFileSync(p, 'utf8'), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  function visit(n) {
    const attrs = ['aria-label', 'aria-description', 'title', 'placeholder', 'alt'];
    const expr = ts.isJsxExpression(n) && n.expression;
    if ((ts.isJsxText(n) && /[\p{L}\p{N}]/u.test(n.text)) ||
        (ts.isJsxAttribute(n) && attrs.includes(n.name.getText(sf)) && n.initializer && ts.isStringLiteral(n.initializer)) ||
        (expr && (ts.isStringLiteral(expr) || ts.isTemplateExpression(expr) || ts.isNoSubstitutionTemplateLiteral(expr)))) {
      console.error(`${p}:${sf.getLineAndCharacterOfPosition(n.getStart(sf)).line + 1}: ${n.getText(sf).trim()}`); bad = true;
    }
    ts.forEachChild(n, visit);
  } visit(sf);
} } walk('src'); process.exitCode = bad ? 1 : 0;
NODE
```

Catalog gate (run in `ui/hub`):
```bash
node --input-type=module <<'NODE'
import fs from 'node:fs'; import assert from 'node:assert/strict';
function flat(x, p = '', out = {}) { for (const [k, v] of Object.entries(x)) {
  const key = p ? `${p}.${k}` : k;
  if (v && typeof v === 'object' && !Array.isArray(v)) flat(v, key, out);
  else { assert.equal(typeof v, 'string', key); assert.ok(v.trim(), key); out[key] = v; }
} return out; }
const en = flat(JSON.parse(fs.readFileSync('src/locales/en.json', 'utf8')));
const zh = flat(JSON.parse(fs.readFileSync('src/locales/zh.json', 'utf8')));
assert.deepEqual(Object.keys(en).sort(), Object.keys(zh).sort());
const vars = s => [...new Set([...s.matchAll(/\{\{\s*([^{}]+?)\s*\}\}/g)].map(m => m[1].trim()))].sort();
const tags = s => [...s.matchAll(/<\/?([A-Za-z][\w]*)\s*\/?>/g)].map(m => m[0]).sort();
for (const k of Object.keys(en)) { assert.deepEqual(vars(en[k]), vars(zh[k]), k); assert.deepEqual(tags(en[k]), tags(zh[k]), k); }
console.log('Catalog key, placeholder and rich-tag parity OK');
NODE
```

Manual gates: verify literal-key ownership, protocol values unchanged, no import-time
translation, locale-dependent memo dependencies, and en/zh switching with open dialogs,
lazy tabs, helper labels, notices and exports. Greps cannot prove these properties.
Future implementation verification: `cd ui/hub && pnpm run typecheck`, `pnpm test`,
`pnpm run build`. The brief's 308-test baseline is a requirement, not a rerun result here;
`tsconfig.json` excludes tests, so typecheck alone cannot cover the 13 copy-coupled suites.

## F. Open risks
- `ProjectHarnessTab.tsx` is 4246 LOC; `TaskTrajectoryViewer.tsx` is 2687 LOC. Assign each to ONE agent with sequential passes (labels/accessibility, dynamic sentences, formatting, final scan); never split concurrent edits within either file.
- Pre-init fallback plural selection needs the explicit rule in C. Catalog key parity does not prove correct plural behavior or English byte fidelity; retain exact helper assertions.
- `taskTrajectory.ts` detects English `Failure Reason`, `Task Failed`, `Task Incomplete`; localizing generated headings must preserve semantic/dedup detection of historical text. `ATTACHED_FILES_PROMPT_HEADER` is a serialized dedup sentinel: keep it stable and localize only display wrappers unless separately redesigned.
- Shared `hub-core/schedule.ts` returns English weekdays/sentences and ambient-locale clock text. Infrastructure owner adds a Hub-only `schedule.*` adapter; preserve shared parsing, cron values and time-input serialization.
- Bilingual PDF templates, HTML language metadata, escaping, CJK fonts and PDF structural numbers need separate review; never localize offsets or JSON data. Existing bilingual copy belongs in catalogs, not source.
- Re-render subscriptions cannot update strings already stored in state/history. Store owned message keys/values and raw timestamps where practical; persisted user-generated draft titles remain data.
- Mention badge display can affect CSS selectors; retain stable `itemType` classes. Review CJK wrapping, narrow headers, tables and dialogs. Node-only tests cannot verify React/browser layout or immediate switching.
