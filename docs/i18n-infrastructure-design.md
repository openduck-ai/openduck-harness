# Hub i18n infrastructure design

Scope: infrastructure and the seeded shell strings below; page extraction follows separately. React 19.2.8, Vite 6.4.3, TS 5.9.3, ESM; use existing `i18next@^26.7.1` / `react-i18next@^17.1.12`.
This design supersedes inventory §§2/4/7/8's react-intl, descriptor JSON, ICU, dependency-install and checker proposals. Use nested string catalogs and i18next interpolation; no new dependencies/scripts.
Only this document changes now. Future implementation touches only the files listed below; leave theme files, `index.html`, Desktop, hub-core, root `I18N.md`, Rust and existing user data unchanged. No new Chinese literals in source outside `zh.json`.

## Files (paths relative to ui/hub)

| File | Purpose |
| --- | --- |
| `src/locales/en.json`, `src/locales/zh.json` | Eager nested string catalogs; exactly the seeded keys below. |
| `src/i18n/locales.ts` | Pure locale constants, normalization, storage reading and detection; browser access only inside resolver calls. |
| `src/i18n/catalogs.ts` | Node-safe JSON imports and resources mapping. |
| `src/i18n/index.ts` | One i18next instance, initialization, switching, document sync and non-hook translation. |
| `src/contexts/LocaleContext.tsx` | LocaleProvider and useLocale; independent of ThemeContext. |
| `src/components/LocaleSwitcher.tsx` | Accessible controlled native language select. |
| `src/i18n.test.ts` | Root-level Node tests; flat discovery glob cannot collect nested tests. |
| `src/main.tsx`, `src/components/Header.tsx` | Bootstrap/provider/ErrorBoundary wiring; switcher and seeded Header copy. |

## Pure locale contract

```ts
export type Locale = 'en' | 'zh-CN'
export const SUPPORTED_LOCALES: readonly Locale[] = ['en', 'zh-CN']
export const DEFAULT_LOCALE: Locale = 'en'
export const LOCALE_STORAGE_KEY = 'goose-hub-locale'
export function normalizeLocale(tag: string): Locale
export function readStoredLocale(storage: { getItem(key: string): string | null } | null | undefined): Locale | null
export function detectLocale(languages: readonly string[]): Locale
export function resolveInitialLocale(): Locale
```
Normalize by trimming, replacing `_` with `-`, and case-insensitive matching: `zh`, `zh-CN`, `zh-SG`, `zh-MY`, `zh-Hans` and `zh-Hans-*` → `zh-CN`; everything else → `en`.
Internally share a supported-tag matcher returning `Locale | null`: recognize these Chinese aliases and `en`/`en-*`; reject other languages and Traditional Chinese (`zh-Hant`, `zh-TW`, `zh-HK`, `zh-MO`). Accept suffixes only at hyphen-separated subtag boundaries.
`detectLocale` walks preferences in order, skips unsupported entries using that matcher (never the fallback-normalizing public function), returns the first supported match, otherwise `en`. Thus `['fr', 'zh-CN']` → Chinese, `['en-US', 'zh']` → English.
`readStoredLocale` accepts only canonical stored `en` / `zh-CN`; absent, corrupt, unsupported, alias or throwing storage returns null. This prevents corrupt preferences silently overriding browser detection.
`resolveInitialLocale` guards browser globals and the localStorage property getter/read with try/catch; stored preference wins, then navigator.languages, then `en`. Use navigator.language only when languages is unavailable/empty; absent navigator means an empty list. No module-scope browser reads, React imports or side effects.

## Catalog shape and complete seed

Dots below denote nested JSON paths, e.g. `{ "app": { "errorBoundary": { "heading": "..." } } }`; feature prefixes are keys within one `translation` namespace, not separate i18next namespaces.
Header/main English literals are quoted verbatim below; interpolation tokens replace original template expressions without changing rendered English bytes. `OpenDuck` is the existing index.html title; common seeds preserve the listed capitalization/Unicode ellipsis. Language options are new self-names.

| key | en | zh |
| --- | --- | --- |
| app.documentTitle | `OpenDuck` | `OpenDuck` |
| app.errorBoundary.heading | `Something went wrong in Goose Hub` | `Goose Hub 出现问题` |
| app.errorBoundary.message | `Unknown error` | `未知错误` |
| app.errorBoundary.reload | `Reload application` | `重新加载应用` |
| header.projects | `Projects` | `项目` |
| header.running | `Running` | `运行中` |
| header.settings | `Settings` | `设置` |
| header.themeToggleTitle | `Theme: {{preference}} (showing {{resolved}}). Click to switch to {{next}} mode.` | `主题：{{preference}}（当前显示 {{resolved}}）。点击切换至 {{next}} 模式。` |
| header.themeToggleLabel | `Toggle theme (currently {{resolved}})` | `切换主题（当前为 {{resolved}}）` |
| header.connectedTitle | `Connected to Goose / OpenDuck server` | `已连接到 Goose / OpenDuck 服务器` |
| header.disconnectedTitle | `Disconnected from server` | `已断开与服务器的连接` |
| common.cancel | `Cancel` | `取消` |
| common.save | `Save` | `保存` |
| common.retry | `Retry` | `重试` |
| common.close | `Close` | `关闭` |
| common.loading | `Loading…` | `加载中…` |
| language.label | `Language` | `语言` |
| language.english | `English` | `English` |
| language.chinese | `简体中文` | `简体中文` |

The required Chinese option self-name is catalog data in both languages; never duplicate it in TS/TSX. Encode its en.json value as `"\u7b80\u4f53\u4e2d\u6587"` so literal Chinese characters in implementation files remain confined to zh.json while both catalogs display the exact requested self-name.
Header uses `useTranslation()` for only these seeded keys. Pass raw `themePreference`, `resolvedTheme`, and the existing dark/light ternary as `preference`, `resolved`, `next`; do not change theme enums. Translating their value labels needs later keys. Keep TUI Agents, branding, status text and New Project copy for later extraction.
`catalogs.ts` imports `en` from `../locales/en.json` and `zh` from `../locales/zh.json`, each with `with { type: 'json' }`; import Locale with `import type` from `./locales.ts`. Export `resources = { en: { translation: en }, 'zh-CN': { translation: zh } } satisfies Record<Locale, { translation: typeof en }>`; no React/DOM imports.

## Runtime and React ownership

`index.ts` exports `i18n = createInstance()` and attaches `initReactI18next` once. Exact init object passed by `initI18n(locale)`:
```ts
{ resources, lng: locale, fallbackLng: 'en', supportedLngs: [...SUPPORTED_LOCALES],
  load: 'currentOnly', ns: ['translation'], defaultNS: 'translation',
  interpolation: { escapeValue: false }, initAsync: false,
  react: { useSuspense: false } }
```
`initI18n(locale): Promise<void>` memoizes the initialization promise; repeated calls reuse it without resetting language. No import-time initialization, backend, language detector, network or lazy catalogs. Bootstrap awaits it before rendering.
`changeLocale(locale: Locale): void` requires initialized runtime, calls `void i18n.changeLanguage(locale)`, then `syncLocaleDocument()`, then guarded localStorage.setItem. With eager resources/no backend the language change is synchronous despite the promise API; storage failures cannot block the in-memory change. All UI callers use the context setter.
Export `syncLocaleDocument(): void`: guard document, assign documentElement.lang from current canonical i18n.language and document.title via `translate('app.documentTitle')`. Initialization also invokes it after init; initial detection does not persist until an explicit selection.
Export `translate(key: string, values?: Record<string, string | number>): string`: resolve against current initialized instance with fallback English; before initialization, on errors or missing/non-string results, resolve the same dotted path in resources.en.translation and interpolate named values, otherwise return key. No hooks/provider dependency; never return objects to JSX.
The runtime barrel imports React integration but no TSX and re-exports no context/components. Node tests deliberately import only `./i18n/locales.ts` and `./i18n/catalogs.ts`, with explicit `.ts` extensions throughout their helper graph; no barrel, DOM, jsdom or TSX imports.
`LocaleProvider({ initialLocale, children })` receives bootstrap's Locale, owns state, and exports `useLocale(): { locale: Locale; setLocale: (locale: Locale) => void }`; throw a clear error outside the provider. Setter calls changeLocale before updating state; no reload, App key or remount. Memoize the context value and setter.
Use a layout effect keyed by locale to call syncLocaleDocument on mount/updates, keeping lang/title synchronized before paint without persisting detection. Runtime already applies the same sync immediately during switching. Locale and Theme providers have no mutual dependency.
`LocaleSwitcher` uses useLocale and useTranslation: `<select className="locale-switcher" value={locale} aria-label={t('language.label')}>`; onChange validate value as en/zh-CN, call setLocale. Options use values en/zh-CN and catalog keys language.english/language.chinese. Native keyboard/focus behavior stays intact; mount beside theme button in header-meta-actions. No CSS file change in this scope; inspect narrow layouts manually.
No I18nextProvider is needed: initReactI18next registers this single instance for useTranslation; LocaleProvider supplies preference state rather than another translation instance. An explicit I18nextProvider would only be needed for independently scoped instances.

## main.tsx wiring sketch

Before: `createRoot(...).render(<ErrorBoundary><ThemeProvider><App /></ThemeProvider></ErrorBoundary>)`.
After (new imports: resolveInitialLocale, initI18n/translate, LocaleProvider):
```tsx
const locale = resolveInitialLocale()
await initI18n(locale)
createRoot(document.getElementById('root')!).render(
  <ErrorBoundary><ThemeProvider><LocaleProvider initialLocale={locale}>
    <App />
  </LocaleProvider></ThemeProvider></ErrorBoundary>,
)
```
Use an `async function bootstrap(): Promise<void>` called with `void bootstrap()` around this sketch to avoid introducing top-level-await build-target requirements. Keep ErrorBoundary outside both providers; replace its heading/reload/unknown-message lookups with translate while retaining the exact three original English strings as final fallback if translation throws/returns its key. Keep raw Error.message precedence, reload handler and logging. No hook inside the class; fallback reads current runtime locale when rendered, including first-render failures.
Keep index.html's English lang/title as no-JS fallback; runtime synchronizes both at startup and on selection. No HTML edit is needed.

## Tests, implementation order and verification

`src/i18n.test.ts` uses node:test and node:assert/strict: normalize aliases/case/underscores, zh-Hans suffixes, English/unsupported/Traditional/empty fallback; detection order, skipped unsupported/Traditional entries, empty/all-unsupported fallback; fake storage with each canonical value, null/missing/invalid/alias values and throwing getItem; recursively flattened en↔zh key parity, string-only nonempty trimmed leaves, and matching interpolation names. Assert exact seeded English templates and representative substitutions. Imports exercise JSON attributes directly; no React rendering is claimed. Hub typecheck excludes test files.

1. Add pure locale APIs and nested catalogs/resources; add root-level test coverage.
2. Add the singleton runtime with cached initialization and shared document sync; preserve type-only imports and strict noUnusedLocals/noUnusedParameters throughout.
3. Add LocaleContext and LocaleSwitcher, wire bootstrap/outer ErrorBoundary, then Header's seeded lookups/control; retain all existing navigation values, counts and handlers.
4. Review scope/key parity and exact English output; no extraction/compiler/checker tooling or unseeded namespace additions in this phase.
5. Future implementer verification from repository root: `cd ui/hub`, `node --experimental-strip-types --test src/i18n.test.ts`, `pnpm test`, `pnpm run typecheck`, `pnpm run build`. These commands are documented only; none run for this design task.
6. Manual verification: switch both ways without losing drafts/project/split panes; reload persisted selection; test blocked storage and ordered browser preferences; inspect document lang/title, theme independence, status/theme titles, keyboard selection and narrow Header layout; force a render error in each locale and confirm original English remains the last resort.
