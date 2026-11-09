# Subtask 3 audit: language switcher, persistence, and rerender

Audited the working tree on 2026-11-04. All paths below are relative to the workspace root; line numbers refer to the files as audited. This is an audit and fix list only; no implementation files were changed.

The user-supplied subtask identifier is `switcher`; the on-disk task uses `language-switcher` at `.goose/tasks/task-ui-hub-i18n-en-zh.yaml:40`. The requirement and single `kind: note` output match at lines 42–48. Do not change task metadata. String extraction is the separate subtask at lines 49–57.

## 1. Immediate rerender and the alleged changeLanguage race — OK

**Proof:** `ui/hub/src/i18n/index.ts:24–34` initializes both bundled catalogs, and installed i18next completes a canonical language change synchronously before `changeLocale()` calls `syncLocaleDocument()` at `ui/hub/src/i18n/index.ts:53–54`.

Installed versions are i18next **26.7.1** and react-i18next **17.1.12** (`ui/node_modules/i18next/package.json:3`, `ui/node_modules/react-i18next/package.json:3`). Exact relevant semantics:

- `ui/node_modules/i18next/dist/esm/i18next.js:2152–2155`: `changeLanguage` creates a deferred promise and emits `languageChanging`.
- Lines 2177–2180 select the requested code directly when it already has bundled translations. Both canonical inputs do (`ui/hub/src/i18n/catalogs.ts:5–8`). No asynchronous detector is involved: an explicit language reaches `setLng(lng)` at lines 2207–2208.
- `loadResources` calls its callback inline when `resources` exists and `partialBundledLanguages` is false (`ui/node_modules/i18next/dist/esm/i18next.js:2031–2055`; false is the default at line 1801). There is no backend registered in `ui/hub/src/i18n/index.ts:13`.
- Completion sets `language`, the resolution hierarchy, and `resolvedLanguage`, updates the translator, then emits `languageChanged` (`ui/node_modules/i18next/dist/esm/i18next.js:2156–2168`). Event listeners run synchronously (lines 293–299).
- Only afterward does completion resolve the promise and call the optional callback (lines 2174–2175). Normal resource-loading errors go to that callback; this path resolves rather than rejects on a loading error. Arbitrary plugin/listener exceptions can still throw; they are not an existing failure of these bundled canonical switches.
- react-i18next defaults to `bindI18n: 'languageChanged'` (`ui/node_modules/react-i18next/src/defaults.js:4`). Its hook subscribes to that event, increments a revision, and reads a language-specific snapshot through `useSyncExternalStore` (`ui/node_modules/react-i18next/src/useTranslation.js:61–75,93–142`). It does not attach a `.then()` render to `changeLanguage`.

Thus `syncLocaleDocument()` reads the new language and translates the new title when the switch handler continues (`ui/hub/src/i18n/index.ts:41–46`). An event observer can see the old document metadata *inside* `languageChanged`, before this next statement, but that is same-stack sequencing, not an asynchronous one-tick lag or demonstrated stale paint. React render scheduling/commit count should not be inferred solely from event count. The provider also syncs metadata in a layout effect (`ui/hub/src/contexts/LocaleContext.tsx:33–35`). Both catalog titles currently equal `OpenDuck` (`ui/hub/src/locales/en.json:3`, `ui/hub/src/locales/zh.json:3`), so title text alone cannot demonstrate a language transition.

A read-only Node probe of the actual `initI18n`/`changeLocale` functions observed `zh-CN`, `项目`, and one event immediately after the call; after one microtask there was still one event; switching back immediately produced `en`, `Projects`, and the second event.

**Minimal fix: none for the alleged race.** Do not add `syncLocaleDocument(localeOverride)` or await solely to fix a nonexistent asynchronous change. An override for `lang` alone would also not fix a genuinely asynchronous title lookup. If asynchronous resource loading is introduced later, move metadata synchronization to actual completion/event handling and account for superseded requests; that design is outside this subtask.

The earlier assumptions at `.agent/notes-plan-task-ui-hub-i18n-en-zh.md:30–36` are not established defects. This audit supersedes those race/segmented-control recommendations. Official references: [i18next changeLanguage API](https://www.i18next.com/overview/api#changelanguage) and [react-i18next event binding](https://react.i18next.com/latest/i18next-instance). The version-specific ordering above is established by the installed source, not inferred from the API's promise return type.

## 2. LocaleProvider and translation-consumer consistency — DEFECT (latent state divergence)

**Proof:** `ui/hub/src/contexts/LocaleContext.tsx:26–30` stores `initialLocale` independently and blindly assigns each requested locale, while no subscription updates that state from the exported i18next instance at `ui/hub/src/i18n/index.ts:13`.

For the current switcher flow, this works: the select validates supported values (`ui/hub/src/components/LocaleSwitcher.tsx:10–14`), i18next commits those exact bundled codes synchronously, then provider state follows. The guarantee for `useTranslation()` consumers comes from i18next's event, **not** from `setLocaleState`; updating LocaleContext only guarantees notification to its context consumers. Hardcoded components or arbitrary one-time calls to `translate()` do not acquire a subscription. Only Header and LocaleSwitcher currently call `useTranslation()`; mass conversion belongs to subtask 4. The non-hook error boundary translations at `ui/hub/src/main.tsx:35–43` are also not reactive subscriptions, and the switcher is unavailable while that boundary replaces the app.

Concrete divergence: after mounting with `initialLocale='en'`, a direct `i18nInstance.changeLanguage('zh-CN')` (or exported `changeLocale('zh-CN')` called outside the provider setter) updates translated Header text but leaves the controlled select at English. A provider mounted against an already different i18next language also displays stale initial state. There is no current external change call site in `src/`, so this is a reproducible API/state consistency gap, not evidence that every user switch fails. Later changes to an `initialLocale` prop are not inherently commands to change the running locale; the problem is treating the startup prop as authoritative over the initialized instance.

No canonical-input normalization or ordinary rejected-loading-promise mismatch exists with today's resources. Do not invent a rollback/error workflow for it.

**Minimal fix:** have LocaleProvider obtain the current language through `useTranslation()` and derive its `locale` from that hook's i18n language, using `normalizeLocale` and `initialLocale` only as a preinitialization fallback. Keep the context API and stable setter delegating to `changeLocale`; remove its independent `useState` and optimistic `setLocaleState`. Retain the layout effect keyed by the derived locale so supported external instance changes also synchronize document metadata. Reuse react-i18next's existing subscription instead of adding another custom listener/store. External raw i18next calls need not gain persistence; the authorized switcher path still goes through `changeLocale`.

## 3. Persistence and refresh round-trip — OK

**Proof:** the writer uses `LOCALE_STORAGE_KEY` and stores the raw canonical `Locale` at `ui/hub/src/i18n/index.ts:55–60`, exactly matching the reader's key and strict `'en'`/`'zh-CN'` acceptance at `ui/hub/src/i18n/locales.ts:25–37`.

The key is `goose-hub-locale` (`ui/hub/src/i18n/locales.ts:4`). `resolveInitialLocale()` returns a valid stored value before navigator detection (lines 55–59,64–78). Bootstrap resolves that preference, awaits initialization with it, and supplies it to the provider (`ui/hub/src/main.tsx:52–61`). Therefore either selection survives a refresh when storage is available. Invalid stored aliases falling through to browser detection are intentional; the write path never stores aliases. Both a throwing localStorage getter and a throwing `setItem` are caught, permitting an in-memory switch; persistence cannot be promised when the browser denies storage. No cookie, key migration, or storage-format change is needed.

## 4. Visible active state, catalog labels, accessibility, layout — OK; keep the native select

**Proof:** `ui/hub/src/components/LocaleSwitcher.tsx:18–25` binds the select's visible selected value to `locale`, supplies both named options, and obtains its accessible label and option text from the catalogs; `ui/hub/src/components/Header.tsx:94–109` mounts it in the global Header rendered unconditionally at `ui/hub/src/App.tsx:346–358`.

**One recommended design: retain this controlled native select.** Its collapsed selected option visibly identifies the active language; opening it lists both choices with native selection semantics. The requirement does not demand simultaneous display of both labels. Native focus and platform keyboard selection are already available, and the translated `aria-label` names the control. Do not add `aria-pressed` or `aria-current` to a select; its selected option already expresses selection. A segmented button group could use `aria-pressed`, but would add width, focus stops, and styling without satisfying an unmet requirement here. Do not ship both controls as redundant fallbacks.

Catalog values are correct endonyms: `English` and `简体中文` in **both** catalogs (`ui/hub/src/locales/en.json:26–29`, `ui/hub/src/locales/zh.json:26–29`). Only `language.label` changes from Language to 语言. Preserve the endonyms and their existing catalog keys; unchanged language names are not a translation failure.

The select already shares border/background/radius/hover tokens with the theme button and has a focus-visible outline (`ui/hub/src/styles.css:403–438`). The action row has a 0.85rem gap and no wrapping (lines 397–400); the select is capped at 8rem (lines 417–427). The Header stacks below 1024px (lines 7797–7803), while the 720px rule wraps navigation, not the actions (lines 7825–7830). Consequently there is no evidence to claim all narrow widths fit: manually check 320/375/720px plus desktop in both themes/locales. Keep both option labels readable and keyboard focus visible. If an overflow is actually observed, make a small responsive action-row adjustment; do not widen the control to a segmented design or redesign Header preemptively.

## 5. Achievable automated coverage — DEFECT (coverage gap)

**Proof:** `ui/hub/src/i18n.test.ts:45–69` tests storage reads and only the no-window initial-locale fallback; it never exercises a write, a write/read/startup-precedence round-trip, or an actual i18next language transition.

Keep tests in a flat file such as `ui/hub/src/locale-transition.test.ts` (runner: `ui/hub/package.json:12`). No `.tsx` imports or DOM emulation is needed. Use these exact small extractions into the existing `ui/hub/src/i18n/locales.ts`:

- `writeStoredLocale(storage, locale): boolean`, with `storage: { setItem(key: string, value: string): void } | null | undefined` and `locale: Locale`. Write the existing key/raw canonical value, return true on success, false for absent/throwing storage. This is a DOM-free injected-I/O helper, not mathematically pure. Have `changeLocale` call it; preserve the surrounding try/catch for acquiring `window.localStorage`, since a getter may throw before the helper is entered. Do not gate the in-memory change on its return value.
- `chooseInitialLocale(storedLocale: Locale | null, languages: readonly string[]): Locale`, a genuinely pure function returning the stored locale when present, otherwise `detectLocale(languages)`. Have `resolveInitialLocale` use it while preserving its current early stored-value return and guarded browser access, navigator.language fallback, and SSR default. Do not introduce a second resolver with subtly different rules.

Exact cases to add:

1. For each canonical locale, a Map-backed fake storage records exactly one write under `goose-hub-locale` with the raw code, `writeStoredLocale` returns true, and `readStoredLocale` returns that same code. Pass the result to `chooseInitialLocale` with an opposing browser language and assert the stored choice wins.
2. Writer with null/undefined storage returns false; a throwing `setItem` returns false without escaping. Existing read-error coverage stays intact.
3. `chooseInitialLocale(null, ['fr', 'zh-CN'])` yields `zh-CN`; `chooseInitialLocale(null, [])` and unsupported-only input yield `en`. Assert stored `en` beats browser `zh-CN` and stored `zh-CN` beats browser `en`.
4. Import `initI18n`, `changeLocale`, and `i18nInstance` from the existing **`.ts`** index. Initialize once; switch en → zh-CN → en; assert `language`, `resolvedLanguage`, and `header.projects` immediately after each call. For each switch assert one `languageChanged` event whose handler sees the new translation, and assert no additional event after a microtask. Use serial subtests and remove listeners/reset the singleton in cleanup. Do not claim this measures React commits.
5. With only an isolated temporary plain `window` storage adapter (not a DOM), call the real `changeLocale` and then the real `resolveInitialLocale` for both locales; assert each stored preference wins on the simulated next startup. Exercise a throwing localStorage getter and a throwing setter; neither prevents the actual instance/translation change. Restore global property descriptors and instance language in cleanup; do not mutate Node's navigator globally for parallel tests. This verifies adapter wiring as well as the helpers. It is a startup simulation, not a real browser refresh.
6. Add catalog assertions that both languages retain the two endonyms and that `language.label` differs as expected. Existing English-only fidelity assertions are at `ui/hub/src/i18n.test.ts:156–158`.

Do not extract an artificial `applyLocale` reducer: it would model invented state instead of verifying the real i18next event path. Node can import the existing `.ts` index and its installed JS dependencies; the read-only probe did so successfully. Provider/select rendering and external-update consistency still require the browser checks below, since `.tsx` cannot be imported by this runner.

## 6. Remaining English near the switcher — DEFECT, SUBTASK 4 ONLY

**Proof:** the surrounding Header still contains literal user-visible copy at `ui/hub/src/components/Header.tsx:48,50,81,120,125`; these literals cannot change when i18next emits its event.

Inventory for subtask 4, with no fixes in this subtask:

| Location | Remaining visible English |
| --- | --- |
| `ui/hub/src/components/Header.tsx:48` | `Lightweight Agent Harness` |
| `ui/hub/src/components/Header.tsx:50` | `OpenDuck Hub` (brand; subtask 4 decides whether to preserve it) |
| `ui/hub/src/components/Header.tsx:81` | `TUI Agents` |
| `ui/hub/src/components/Header.tsx:120` | `Server Connected` / `Server Offline` (the title is already translated separately at line 113) |
| `ui/hub/src/components/Header.tsx:125` | `New Project` |
| `ui/hub/src/components/Header.tsx:99–104` | Theme tooltip/accessible-label interpolation still exposes raw `system`/`light`/`dark` values even though its surrounding template is translated. |

This is a focused inventory of the Header area, not a request to audit/extract all components now.

## Prioritized minimal fix list

1. **Provider consistency:** edit only `ui/hub/src/contexts/LocaleContext.tsx` to derive its current locale from `useTranslation`'s subscribed instance, remove independent locale state, preserve `{locale, setLocale}`, and retain the derived-locale document layout effect. No bootstrap rewrite or custom subscription infrastructure.
2. **Testable persistence boundary:** add exactly `writeStoredLocale` and `chooseInitialLocale` to `ui/hub/src/i18n/locales.ts`; wire the former into the existing `changeLocale` storage block and the latter into the existing startup resolver, retaining all browser-access guards and current behavior. Do not change `changeLanguage` timing or catalog resources.
3. **Focused verification:** add the six groups above in a flat `.test.ts` file using node:test/assert and current dependencies. Complete the browser checks, especially external supported language changes and a genuine refresh. Retain the native select and existing Header mount; adjust responsive CSS only if observed overflow requires it.

## Do NOT do

- Do not extract the approximately 2,000 strings across 48 files or fix item 6 in subtask 3. Do not force whole-App remounts or promise that hardcoded strings now translate.
- Do not replace the native select with segmented controls, add duplicate fallback controls, translate endonyms per locale, or hardcode their display text in JSX.
- Do not rewrite subtask 2's i18n bootstrap, catalogs, fallback machinery, locale normalization, storage key, or provider public API wholesale. No cookies or cross-tab synchronization feature.
- Do not add dependencies, run `pnpm add`, add vitest/jsdom/testing-library, change test runners/globs, or place executable tests only in nested directories. Do not import `.tsx` in Node tests.
- Do not touch the root `package.json` convenience shim, task YAML/expected-output kinds, lockfiles, or unrelated pre-existing working-tree changes.
- Do not apply a locale override/async rewrite to fix the disproven race, add speculative rollback machinery, or add tests that merely assert source-code strings.

## Verification for the coding agent

From the workspace root, run these exact commands after implementing the minimal fixes:

```bash
cd /mnt/e/openduck-harness/ui/hub
node --experimental-strip-types --test src/*.test.ts
npx tsc --noEmit
cd /mnt/e/openduck-harness
npm run build
git diff --check
```

For browser verification, start the existing dev server:

```bash
cd /mnt/e/openduck-harness/ui/hub
npm run dev -- --host 127.0.0.1
```

Check both directions: selected language remains visible; Header's translated navigation and the select's accessible label update without a reload; `document.documentElement.lang` is the canonical code; document.title remains the translated catalog value (currently OpenDuck for both); localStorage contains the exact selected code; a genuine browser refresh retains it even against an opposing browser language. Confirm native keyboard operation/focus and readability at 320/375/720px and desktop in light/dark themes. Verify a supported external change of the same i18next instance updates both the provider's selected value and translated consumers, including a mount where initialLocale differs from the initialized instance. No new production debug API is needed for that check.

Full tests/typecheck/build were not rerun during this audit; only the read-only transition probe was executed. Browser layout and React commits were not measured. Record actual coder verification results, not assumed passes.

The harness output remains exactly one note, with no added command check:

```yaml
kind: note
description: 语言切换控件已实现并挂载到全局布局，切换会更新 locale 并在刷新后保持
```

## FINAL REVIEW

**APPROVED** — Initialization completes before render (`main.tsx:53–56`); the subscribed provider and locale-keyed document effect stay consistent without changing i18n language; writes receive canonical locales; startup precedence/fallbacks are preserved; groups 4–5 verify real events (including no extra event after a microtask) and storage integration. No required fixes; reported checks/runtime probe accepted without rerunning.
