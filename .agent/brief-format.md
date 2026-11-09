# Brief: implement `ui/hub/src/i18n/format.ts` (formatting layer)

Repo: /mnt/e/openduck-harness. Work only inside `ui/hub`. You may ONLY create/modify:
- `ui/hub/src/i18n/format.ts`
- `ui/hub/src/format.test.ts` (new test file)

Do NOT touch any other file. Do not modify `src/locales/*.json`, `src/i18n/index.ts`,
`src/i18n/locales.ts`, `src/i18n/catalogs.ts`, or any component/helper. Do not run
`pnpm install`, git commands, or cargo.

Read first: `docs/i18n-extraction-guide.md` section D (authoritative spec),
`docs/i18n-inventory.md` section "Every date / number / relative-time formatting site",
`ui/hub/src/i18n/index.ts`, `ui/hub/src/i18n/locales.ts`,
`ui/hub/src/projectTasks.ts` (existing `formatDurationMs` / `formatRelativeTime`),
`ui/hub/src/runningTasks.ts` (`runningElapsedLabel`),
`ui/hub/src/projectTasks.test.ts` and `ui/hub/src/runningTasks.test.ts` (exact expected strings).

## Goal
Create a pure, DOM-free `.ts` module providing locale-aware formatting primitives that the
whole Hub will use instead of raw `toLocaleString()` / `toFixed()`.

## Hard constraints
1. Pure `.ts`, no React, no `.tsx` import, no DOM access at import time.
   Import with explicit `.ts` specifiers and `import type` where appropriate.
2. Export exactly these (types exported too):
```ts
type DateInput = Date | number | string   // number = epoch ms; string = ISO instant
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
3. Locale resolution at EACH call:
   `normalizeLocale(options?.locale ?? i18nInstance.language ?? DEFAULT_LOCALE)`.
   Map `en -> 'en-US'`, `zh-CN -> 'zh-CN'`. NEVER pass `undefined`/`[]`/raw locale to Intl.
   Strip the custom `locale` property from the options object before handing it to Intl.
4. Default field sets (when caller supplies NO date/time fields):
   - `formatDate`: `{ year:'numeric', month:'numeric', day:'numeric' }`
   - `formatDateTime`: date fields + `{ hour:'numeric', minute:'2-digit', second:'2-digit' }`
   - `formatTime`: `{ hour:'numeric', minute:'2-digit', second:'2-digit' }`
   If the caller supplies ANY of year/month/day/hour/minute/second/timeStyle/dateStyle,
   use ONLY the caller's fields (do not mix in defaults).
5. Invalid/unparseable dates must not throw and must not produce `Invalid Date`.
   Return `''` for `formatRelativeTime` on invalid input (existing behavior).
6. `formatRelativeTime` must reproduce EXACTLY the current behavior of the existing
   `formatRelativeTime` in `src/projectTasks.ts` when locale is `en`:
   `''` for empty/invalid; `just now` (<45s); `<N>m ago`; `<N>h ago`; `<N>d ago`;
   `>= 7d` falls back to the UTC date form `Sep 1` (month short + numeric day, timeZone UTC),
   localized via `formatDate` with `{ month:'short', day:'numeric', timeZone:'UTC' }`.
   Read the real current implementation and copy the thresholds exactly — do not guess.
   - Default (`future` omitted or `'clamp'`): negative deltas clamp to `just now` (current behavior).
   - `future:'relative'` is for NEW callers only: if the event is in the future by <45s ->
     `in <1m`; otherwise use the same minute/hour/day thresholds on the absolute difference,
     producing e.g. `in 2h`; `>=7d` uses the same UTC short date. Chinese counterparts:
     `<1分钟后` / `{{n}}分钟后` / `{{n}}小时后` / `{{n}}天后`.
7. `formatDurationMs` MUST reproduce the existing exported `formatDurationMs` behavior exactly
   for `en` (tests assert these exact strings):
   `500 -> '<1s'`, `999 -> '<1s'`, `1000 -> '1s'`, `2140 -> '2s'`, `45000 -> '45s'`,
   `120000 -> '2m'`, `134000 -> '2m 14s'`, `3600000 -> '1h'`, `3780000 -> '1h 3m'`.
   Zero components omitted.
8. `formatExportDuration` is a SEPARATE helper: `<=0 -> '0s'`; `<1000 -> <raw ms> + 'ms'`;
   `<60000 -> two decimals + 's'`; otherwise `floor(minutes)` + `m` and rounded remaining
   seconds INCLUDING zero seconds (`'1m 0s'`, and preserve the possible `'1m 60s'` rounding
   edge — no silent normalization). Read `ui/hub/src/taskPdfExport.ts` around its duration
   helper to copy the CURRENT behavior exactly, then reimplement here.
9. `formatPercent(ratio)` -> `formatNumber(ratio*100, { maximumFractionDigits: 1 })` + `%`
   is NOT required; instead accept a ratio and format `ratio*100` with default
   `{ maximumFractionDigits: 1 }` and append `%`. Keep it simple and document via code clarity.
10. Compact relative/duration wording is HAND-ROLLED message text (do not use
    `Intl.RelativeTimeFormat`), because tests require `'3m ago'` / `'2m 5s'` exactly.
    Chinese wording must be natural: `刚刚`, `{{n}}分钟前`, `{{n}}小时前`, `{{n}}天前`,
    durations `{{n}}秒`, `{{n}}分`, `{{n}}小时` combined like `2分14秒`, `1小时3分钟`.
    Implement Chinese via an internal locale switch (simple `if (locale === 'zh-CN')`) —
    do NOT add catalog keys and do NOT edit the locale JSON files. Keep it self-contained.
11. `runningElapsedLabel` behavior must remain available: expose it? NO — do not move it.
    Leave `src/runningTasks.ts` alone. But `formatDurationMs`'s compact form is what it uses
    conceptually; do not change `runningElapsedLabel` here.

## Tests (`ui/hub/src/format.test.ts`)
Use `node:test` + `node:assert/strict`, import via explicit `.ts` paths, mirror the style of
existing tests in the repo. Cover:
- `formatDurationMs` exact English strings for all 9 cases above, plus a zh-CN case.
- `formatRelativeTime` exact English strings matching `projectTasks.test.ts` expectations,
  including `''`, `just now`, `3m ago`, `5h ago`, `2d ago`, and the `Sep 1` UTC fallback.
- `formatDate`/`formatDateTime`/`formatTime` produce different output for en vs zh-CN and
  never throw for invalid input; use `timeZone: 'UTC'` for determinism.
- `formatNumber`/`formatPercent` locale differences.
- `formatExportDuration` boundaries: 0, 999, 1000, 59999, 60000, 61500.
- `locale` option overrides the global i18next language.

## Verification (run these yourself)
```
cd ui/hub && pnpm run typecheck
cd ui/hub && pnpm test 2>&1 | tail -20      # must be >= 308 pass, 0 fail
```
Report the exact test counts and confirm no other file changed (`git status --short`).
