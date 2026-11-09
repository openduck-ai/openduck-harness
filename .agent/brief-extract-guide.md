# Brief: define Hub i18n extraction conventions (design only)

You are the DESIGN agent. Do not mass-edit components. Your job is to produce a precise,
enforceable convention document + the formatting helpers spec, so that N parallel coding
agents can each localize ~3-6 files without conflicting with each other.

Repo: /mnt/e/openduck-harness, area: ui/hub.

## Already done (DO NOT redo, DO NOT change)
- `ui/hub/src/i18n/locales.ts` — Locale type, SUPPORTED_LOCALES=['en','zh-CN'], DEFAULT_LOCALE='en',
  normalizeLocale, detectLocale, readStoredLocale/writeStoredLocale, resolveInitialLocale.
- `ui/hub/src/i18n/catalogs.ts` — imports `src/locales/en.json` + `zh.json` via
  `with { type: 'json' }`, exports `resources`.
- `ui/hub/src/i18n/index.ts` — i18next singleton (`i18nInstance`), `initI18n(locale)`,
  `changeLocale(locale)`, `syncLocaleDocument()`, non-hook `translate(key, values)`.
- `ui/hub/src/contexts/LocaleContext.tsx` — LocaleProvider + useLocale.
- `ui/hub/src/components/LocaleSwitcher.tsx` — native <select> mounted in Header.
- `src/locales/en.json` / `zh.json` — currently only 19 foundation keys
  (app.*, header.*, common.*, language.*).

Read those five source files + `docs/i18n-inventory.md` before answering.

## Hard constraints discovered by recon (respect them)
1. Tests: `node --experimental-strip-types --test src/*.test.ts`. Plain node, no vitest/jsdom,
   `.ts` only. **Tests cannot import `.tsx`.** A pure `.ts` module CAN import `src/i18n/index.ts`
   (verified: `translate()` works without init via resolveFallback over en.json).
   => Helper `.ts` modules (taskTrajectory.ts, lazyTabs.ts, configEditor.ts, providerConfig.ts,
   projectTasks.ts, runningTasks.ts, mention.ts, taskDraft.ts) must localize by calling
   `translate()` at RENDER/compute time, NOT at module import time, so English test assertions
   still pass when the runtime locale is en.
2. 13 existing test files assert exact English output. English catalog values must byte-match
   current copy (including U+2026 ellipsis, quotes, punctuation).
3. `tsconfig.json` excludes `src/**/*.test.ts`; typecheck won't see tests.
4. Must keep `pnpm run typecheck`, `pnpm test` (308 tests), `pnpm run build` green.

## What to deliver
Write ONE file: `docs/i18n-extraction-guide.md`, containing:

A. **Module/helper rule**: exactly how a pure `.ts` module should call `translate()` so that
   (a) Node tests still see English, (b) React components re-render on locale change.
   Address: module-level arrays/labels (configEditor.ts option arrays, lazyTabs.ts tab labels,
   providerConfig.ts fallback labels) — must become functions/getters, and the React consumer
   must call them during render (and must not cache them in `useMemo` without locale in deps).
   Give the canonical snippet for both cases.

B. **Key namespace map**: one row per file, the reserved top-level key prefix that file owns, so
   parallel agents never write to the same subtree. Cover ALL copy-bearing files listed in
   docs/i18n-inventory.md section 5. Use the inventory's proposed prefixes where sensible, but
   resolve collisions (e.g. trajectory.* is shared by taskTrajectory.ts + TaskTrajectoryViewer.tsx
   + AgentLogTranscript.tsx — say exactly how to sub-namespace).

C. **Key naming rules**: shape, plural/ICU guidance for i18next v26 (`_one`/`_other` suffixes,
   `count`), interpolation `{{var}}`, how to handle rich JSX (splitting rule: prefer a full
   sentence key with one placeholder over concatenated fragments; when JSX interleaving is
   unavoidable, document the `<Trans>`/component approach used by react-i18next).
   Explicitly forbid: HARDCODED Chinese in any .tsx; keys built by string concatenation of
   English copy; translating protocol tokens (itemType discriminants, git tokens, provider ids,
   URL page/tab values, config keys).

D. **Formatting layer spec**: design `src/i18n/format.ts` (pure .ts, no DOM at import) exposing
   `formatDate`, `formatDateTime`, `formatTime`, `formatNumber`, `formatPercent`,
   `formatRelativeTime`, plus duration helpers. It must map the app Locale to a BCP-47 tag
   ('en' -> 'en-US', 'zh-CN' -> 'zh-CN') and use `Intl.*` with an explicit locale argument so
   output is deterministic and testable in Node. CRITICAL: give the exact expected English
   outputs that must be unchanged vs today's `toLocaleString()`/`toLocaleDateString()` calls
   under Node's default (en-US, UTC) — i.e. confirm 'en-US' reproduces existing English test
   expectations (`Sep 1`, `<1s`, `3m ago`, etc. from projectTasks.test.ts). If today's
   behavior depends on the ambient default locale, call it out and prescribe pinning to en-US.
   Also specify how a relative-time formatter should behave ('3m ago', 'in 2h') and whether to
   hand-roll or use Intl.RelativeTimeFormat (check verbatim existing helper implementations in
   projectTasks.ts / runningTasks.ts and match them).
   NOTE: existing helpers already produce English strings like `<1s`, `3m ago`, `Sep 1`.
   Decide: keep the numeric/threshold logic, localize only the unit words. Be concrete.

E. **Review checklist** an orchestrator can grep/enforce, e.g.:
   - no CJK chars outside src/locales/
   - no bare JSX text nodes / no `aria-label="..."` / `title="..."` / `placeholder="..."`
     string literals in .tsx
   - en.json and zh.json have identical key sets and identical `{{placeholder}}` sets
   Provide the exact shell commands to check each.

F. **Open risks**: list anything that makes this risky (e.g. ProjectHarnessTab.tsx 4246 LOC,
   TaskTrajectoryViewer.tsx 2687 LOC — recommend they be split across multiple sequential
   passes by ONE agent to avoid conflicts).

## Output rules
- Write the doc. Do not modify any other file. Do not run builds.
- Be concrete and terse; this doc will be pasted into agent prompts. Aim for <= 250 lines.
- Verify every claim about current code by reading the actual file first.
