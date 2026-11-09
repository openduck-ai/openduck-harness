# Web Hub i18n inventory: English and Simplified Chinese

Recon date: 2026-11-04. Repository: `/mnt/e/openduck-harness`. Scope: `ui/hub`; implementation proposals only. This document is the only repository file created by recon. No Hub source, dependencies, tests, or shared packages were changed; no build/test was run. Line numbers refer to this checkout and will move during implementation.

English remains the default/fallback catalog. Browser preference detection can select Simplified Chinese on first visit; an explicit selection takes priority. Implementation is local/exploratory unless later designated an upstream contribution; a future upstream PR must follow the Ready-issue workflow in `AGENTS.md`.

## 1. Framework & tooling conclusions

| Item | Verified conclusion |
| --- | --- |
| Package | `ui/hub/package.json`: `@aaif/goose-hub`, private, version `0.1.0`, `type: module` |
| React | Manifest `react`/`react-dom` `^19.2.4`; pnpm overrides both to `^19.2.8`; installed/resolved from Hub: **19.2.8** |
| Vite | Manifest `^6.3.5`; Hub resolves **6.4.3** from `ui/hub/node_modules/vite`. Hoisted `ui/node_modules/vite` is **8.2.1**: do not mistake that for Hub's Vite |
| TypeScript | Manifest `~5.9.3`, installed **5.9.3**; plugin-react manifest `^4.7.0` |
| Installed i18n | `ui/node_modules/react-intl/package.json`: **10.1.0**, ESM, React 19 peer; `intl-messageformat` **11.2.0**; `ui/node_modules/@formatjs/` includes CLI and ICU parser |
| Workspace | `ui/pnpm-workspace.yaml`: `sdk`, `desktop`, `hub`, `hub-core`, `mobile`, `goose-binary/*`; `nodeLinker: hoisted`, `preferOffline: true`; lockfile `ui/pnpm-lock.yaml` |
| Workspace caveat | `ui/package.json` has an older npm `workspaces` array including `text` and omitting Hub/hub-core. Use pnpm YAML as the actual workspace definition |
| Offline availability | React-intl and FormatJS files already exist through Desktop. Declare Hub's dependency explicitly; hoisted presence alone is not a dependency declaration or a guarantee that every pnpm store artifact is cached |
| Scripts | `dev: vite`; `build: tsc -b && vite build`; `typecheck: tsc --noEmit`; `test: node --experimental-strip-types --test src/*.test.ts` |
| Runtime observed | Node **v22.22.3**. No Vitest, jsdom, or TSX transform in Hub's test script |
| Routing | `urlNav.ts` uses `URLSearchParams` and browser history, with `page`, `project`, `tab`, `split`. No react-router. Preserve URL keys/values |

### TypeScript configuration

`ui/hub/tsconfig.json` includes `src` and **excludes `src/**/*.test.ts`**. Thus typecheck/build do not typecheck the Node tests.

| Option | Value | i18n consequence |
| --- | --- | --- |
| `target` / `lib` | ES2022 / ES2022, DOM, DOM.Iterable | Intl APIs available to typed application code; browser/Node runtime still supplies locale data |
| `module` / `moduleResolution` | ESNext / bundler | Vite supports the existing extensionless component imports; native Node requires explicit executable file specifiers |
| `resolveJsonModule` | true | JSON catalogs are typed by TS, but this alone does not make bare JSON imports work in Node ESM |
| `allowImportingTsExtensions` | true | Existing imports such as `./projectTasks.ts` are allowed; retain explicit `.ts` in Node-tested helper chains |
| `noEmit` | true | TypeScript checks; Vite emits bundles. Do not design a separate tsc-emitted helper runtime |
| `isolatedModules` / `moduleDetection` | true / force | Keep types as `import type`; avoid features requiring cross-module emit analysis or unsupported Node type-strip transforms |
| `jsx` / `strict` | react-jsx / true | `.tsx` remains browser-bundled; context values and catalog descriptors need proper types |
| `noUnusedLocals` / `noUnusedParameters` | true / true | Stray `useIntl`, `intl`, descriptor imports, or unused callback parameters break checks |
| Other checks | `noFallthroughCasesInSwitch`, `noUncheckedSideEffectImports`, `skipLibCheck` true | Keep branch behavior and CSS imports intact |

Actual import convention is **mixed**: `main.tsx` uses `./App` and `./contexts/ThemeContext`; `App.tsx` uses extensionless lazy imports; `.ts` helpers/tests generally use explicit `.ts`; components also explicitly import `.ts` and sometimes `.tsx` (`AgentLogTranscript.tsx:20` imports `./MarkdownPreview.tsx`). Do not normalize the entire project. New Node-facing code should use `.ts` specifiers and never import a context/component barrel that transitively loads `.tsx`.

## 2. Existing i18n state

Hub has no i18n package declaration, i18n directory, `IntlProvider`, `useIntl`, or `formatMessage` usage. It has English JSX, helper-generated presentation strings, raw status displays, and browser-default date/number calls.

**Correction to supplied context:** Hub already has hardcoded Chinese. `taskPdfExport.ts:573–674` and `857–910` contain bilingual report headings/labels, and `taskPdfExport.ts:306` hardcodes `<html lang="zh-CN">`. `ProjectChatTasksDrawer.tsx:532` also has a Chinese comment, which is not UI copy. Test fixtures contain Chinese user content and must remain untouched.

### Reusable Desktop pattern

Read in full: root `I18N.md` and `ui/desktop/src/i18n/index.ts`. Also inspected renderer/provider wiring, test wrapper, catalogs, npm scripts, and check/compile/validation scripts.

| Part | Desktop behavior | Hub decision |
| --- | --- | --- |
| React API | `defineMessages` descriptors with stable IDs and English `defaultMessage`; `useIntl().formatMessage(...)` in components | Reuse API and ICU conventions as-is |
| Provider | `renderer.tsx:30–47` loads messages, then wraps the app with `IntlProvider`, `locale`, `defaultLocale="en"`, and `messages` | New Hub provider/context; keep above App and all lazy Suspense/fallback content |
| Source catalog shape | `messages/en.json`, `zh-CN.json`, etc.: `{"id":{"defaultMessage":"..."}}` | Reuse translator-facing shape; normalize to `Record<string,string>` for runtime |
| Runtime loading | `loadMessages('en')` returns `{}` for source defaults; otherwise dynamic `import(./compiled/${locale}.json)`; compiler generates runtime strings/AST | Do not pass the descriptor-shaped source JSON directly as `IntlProvider.messages`. Hub needs its own explicit two-catalog loader/normalizer |
| Locale resolution | Config `window.appConfig.get('GOOSE_LOCALE')`, then walks `navigator.languages`, then en; normalizes underscores, Chinese aliases, base language and validates regional formatting tags | Reuse pure normalization logic, adapted to only en and zh-CN; no Electron `window.appConfig` dependency |
| Chinese variants | Simplified aliases → zh-CN; Traditional aliases → zh-TW | Hub has no zh-TW catalog. Explicitly map zh/zh-CN/zh-Hans/zh-SG/zh-MY to zh-CN; skip zh-Hant/zh-TW/zh-HK/zh-MO and continue preferences, then en |
| Outside React | Exported `currentLocale` / `currentMessageLocale`, computed once at module initialization | Reuse concept only; new mutable/snapshotted runtime accessor must update on Hub switching |
| Formatting | Docs prescribe `intl.formatDate`, `formatNumber`, `formatRelativeTime`; utility code also uses native Intl with `currentLocale` | Reuse formatting principles; Hub helpers should call the current shared `IntlShape` accessor |
| Extraction | FormatJS extracts `src/**/*.{ts,tsx}` into en.json; English defaults live in source | Reuse extraction conventions, scoped to Hub; catalog check must catch source-default drift |
| Checks | `i18n-check.js` compares extraction with committed en; `i18n-validate-locale.js` checks key parity and ICU arguments; compile script invokes FormatJS | Reuse algorithms; adapt paths/module syntax and declare tooling dependencies |
| Tests | Desktop `IntlTestWrapper` is TSX and assumes a renderer test environment | Cannot reuse in Hub's Node-only tests; add pure `.ts` locale/catalog/runtime tests |

Desktop supports en, es, fr, de, it, pt, id, ms, vi, hi, ja, ko, ru, tr, zh-CN, zh-TW. Its UI language setting reloads the renderer, as `I18N.md` explains. Hub must apply changes immediately, so do not copy Desktop's startup-only exports/reload behavior. Existing translated catalogs are Desktop-feature-specific, not a ready-made Hub translation set. Reuse terminology after review, not Desktop message IDs indiscriminately. Desktop check scripts use CommonJS `require`; Hub's package is ESM, so a copied `.js` script must be converted to ESM or renamed `.cjs`.

## 3. Switcher mount point

Recommended: a labeled native select or compact menu in `Header`'s `header-meta-actions`, immediately adjacent to the theme toggle. Offer **English** and **简体中文** using stable values `en` and `zh-CN`; localize its accessible name. This remains reachable across projects, running tasks, TUI agents, and settings. Optional: a Language row next to Appearance & Theme in Server settings, using the same context setter.

Relevant existing JSX, verbatim with source line numbers:

```tsx
// ui/hub/src/components/Header.tsx:91–102
91       <div className="header-meta-actions">
92         <button
93           type="button"
94           className="icon-btn theme-toggle-btn"
95           onClick={toggleTheme}
96           title={`Theme: ${themePreference} (showing ${resolvedTheme}). Click to switch to ${
97             resolvedTheme === 'dark' ? 'light' : 'dark'
98           } mode.`}
99           aria-label={`Toggle theme (currently ${resolvedTheme})`}
100        >
101          {resolvedTheme === 'dark' ? <Sun size={15} /> : <Moon size={15} />}
102        </button>
```

```tsx
// ui/hub/src/components/ServerSettings.tsx:200–205
200          <div className="panel">
201            <h3>Appearance & Theme</h3>
202            <p className="muted text-sm">
203              Choose your interface color scheme. You can switch between Light, Dark, or System mode.
204            </p>
205            <div className="theme-options-grid">
```

Header's remaining controls are the server-status pill (`104–114`) and New Project (`116–119`). Preserve responsive space and keyboard focus; a visible or visually-hidden label must identify the language control independently of its selected value. Server settings already obtains theme state/setters; it can obtain locale state the same way.

## 4. State/persistence design

| Concern | Proposed design |
| --- | --- |
| Persistent key | `goose-hub-locale`, following `theme.ts`'s `THEME_STORAGE_KEY = 'goose-hub-theme'` |
| Values | Only `en` / `zh-CN` for explicit choices; validate untrusted stored values |
| Detection | Valid stored preference → first supported entry in `navigator.languages` → `en`. Unsupported/corrupt storage must not block browser detection. Use a guarded `navigator.language` fallback if the languages array is unavailable |
| Regional tags | Match en-* to en catalog; preserve a validated canonical English regional tag for formatting during browser detection if desired. Explicit English selection uses en. Never feed invalid BCP 47 strings to Intl |
| Storage failures | Mirror ThemeProvider's try/catch for storage reads/writes; update in-memory state even if persistence fails |
| Immediate application | React LocaleContext owns selected locale; `IntlProvider locale/messages` updates cause renders, without reload or a locale-based App key/remount. Preserve drafts, active project, split panes, sessions, and task state |
| Catalog loading | With only two locales, eagerly load small initial catalogs for atomic switching; if dynamic loading is chosen, load/normalize first, then commit locale+messages together and ignore stale responses |
| Outside React | A `.ts` runtime module with `getIntl()` / `translate(descriptor, values)` and a live locale accessor. Update the same runtime before committing the new provider props. No `.tsx`, DOM requirement, or captured initial locale in this import chain |
| Recomputing copy | Do not leave translated labels in module-level arrays or locale-insensitive `useMemo`. Derive labels during render or from descriptors. Recompute helper presentations on context updates |
| Existing error/toast state | Storing already-formatted English in state prevents immediate switching of an open notice. Store descriptor+values for owned messages or re-resolve them on locale changes; preserve raw external errors |
| HTML language | Set `document.documentElement.lang` on initial resolution and each change, in the provider effect; document-independent helpers remain Node-safe |
| Title | `index.html:2` currently has `<title>OpenDuck</title>` and lang=en. Keep the English no-JS fallback; synchronize `document.title` from an `app.documentTitle` message on every locale change. DefaultMessage remains exactly `OpenDuck`; the brand may stay identical in zh-CN |
| Error boundary | Keep main's outer boundary; supply a tiny non-hook runtime `translate` accessor initialized before rendering. A failed render can use this accessor without depending on a failed provider/hook. Error fallback uses current selected locale on render; initialize runtime early enough for first-render failures |

`ThemeContext.tsx` is the reusable persistence example, not a file to change for locale state. Locale and theme preferences should be independent. Cross-tab storage-event synchronization is optional, not required for initial scope.

## 5. Comprehensive user-visible-string inventory

### Coverage and counting method

Walked **all 91 files** under `ui/hub/src`: **63 production TS/TSX/declaration files (30,333 LOC)**, **27 `.test.ts` files (6,033 LOC)**, plus **styles.css (13,603 LOC)**. Total including tests and CSS: **49,969 LOC**. The rough “36k / 90 files” context therefore depends on exclusions; actual counts above use this checkout.

Estimates below count visible source occurrences, including repeated buttons, distinct ternary branches, fallback errors, accessibility text, and helper-generated phrases. They are **not deduplicated catalog-key counts**. Dynamic user content, protocol values, CSS classes, keyboard-event constants, paths, logs, and parser tokens are excluded. JSX fragments may become one complete ICU message; PDF HTML templates contain several messages inside one template literal. Estimates were adjusted for these cases after literal/JSX parsing and grep inspection.

**Planning total: approximately 2,080 user-visible string occurrences across 48 existing source files**, plus about 15–20 shared-schedule messages exposed from hub-core. Allow roughly ±20% during extraction; do not treat this as a completed message catalog. The following table lists every production file with owned or generated visible copy; zero-copy files appear in the review-only table below.

### Per-file inventory

Paths below are relative to `ui/hub/src`. Formatting line lists include native locale calls and decimal formatting; manual/helper cases are expanded afterward.

| file | LOC | ~# user-visible strings | categories (nav / button / label / placeholder / empty-state / error / toast / aria / title / tooltip) | date-number formatting calls | proposed namespace prefix |
| --- | ---: | ---: | --- | --- | --- |
| `components/ProjectHarnessTab.tsx` | 4246 | ~284 | nav / button / label / placeholder / empty-state / error / toast / aria / title / tooltip | L1870 toFixed, L3062 toLocaleString, L3605 toLocaleString, L3612 toFixed, L3618 toFixed, L3648 toFixed, L3665 toFixed, L3708 toLocaleString | `harness.*` |
| `components/TaskTrajectoryViewer.tsx` | 2687 | ~235 | nav / button / label / placeholder / empty-state / aria / title / tooltip | L932 toFixed, L963 toLocaleString, L969 toLocaleString, L970 toLocaleString, L1752 toLocaleTimeString, L1788 toLocaleString, L2010 toLocaleTimeString, L2017 toLocaleString, L2479 toLocaleString, L2485 toLocaleString, L2491 toLocaleString, L2574 toLocaleString, L2581 toLocaleString, L2587 toLocaleString, L2596 toLocaleString, L2604 toFixed | `trajectory.*` |
| `components/TuiAgentsPage.tsx` | 1504 | ~203 | nav / button / label / placeholder / empty-state / error / aria / title / tooltip | L524 toLocaleTimeString, L526 toLocaleTimeString, L532 toLocaleTimeString, L650 toLocaleString, L651 toLocaleString/toLocaleString, L704 toLocaleTimeString, L873 toFixed, L886 toLocaleTimeString | `tuiAgents.*` |
| `components/OpenDuckConfigPanel.tsx` | 1553 | ~151 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | — | `config.*` |
| `components/AgentLogTranscript.tsx` | 1207 | ~72 | button / label / placeholder / empty-state / aria / title / tooltip | L826 toLocaleString, L1151 toFixed, L1153 toFixed, L1155 toFixed | `trajectory.log.*` |
| `components/ProviderStatsDashboard.tsx` | 549 | ~69 | nav / button / label / empty-state / title / tooltip | L150 toFixed, L178 toLocaleString, L184 toFixed, L194 toLocaleString, L197 toLocaleString, L198 toLocaleString, L208 toFixed, L237 toFixed, L246 toLocaleString, L310 toLocaleString/toLocaleString, L312 toLocaleString, L316 toFixed, L459 toLocaleString, L505 toLocaleString/toLocaleString, L507 toLocaleString | `providers.stats.*` |
| `components/ProjectChatTasksDrawer.tsx` | 817 | ~67 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | L744 toLocaleString, L749 toLocaleString | `chat.tasks.*` |
| `taskTrajectory.ts` | 1993 | ~65 | label / empty-state / error / title | L1485 toFixed | `trajectory.*` |
| `components/ProjectTerminal.tsx` | 665 | ~62 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | L100 toLocaleTimeString | `terminal.*` |
| `components/ProjectGit.tsx` | 614 | ~57 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | — | `git.*` |
| `components/ProjectRules.tsx` | 480 | ~55 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | — | `rules.*` |
| `taskPdfExport.ts` | 1135 | ~55 | label / empty-state / error / title | L44 toFixed, L294 toLocaleString, L621 toLocaleString/toLocaleString/toLocaleString, L880 toLocaleString/toLocaleString/toLocaleString | `export.pdf.*` |
| `components/ProjectFiles.tsx` | 844 | ~52 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | L32 toFixed, L33 toFixed, L798 toLocaleDateString | `files.*` |
| `components/RegisterProjectModal.tsx` | 479 | ~47 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | — | `projectRegistration.*` |
| `components/ServerSettings.tsx` | 302 | ~45 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | — | `settings.*` |
| `configEditor.ts` | 254 | ~44 | label / error | — | `config.editor.*` |
| `components/ProjectChat.tsx` | 837 | ~43 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | — | `chat.*` |
| `components/ProjectList.tsx` | 393 | ~41 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | L288 toLocaleDateString, L367 toLocaleString | `projectList.*` |
| `components/TaskDraftPanel.tsx` | 336 | ~38 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | — | `harness.draft.*` |
| `components/ProjectSettings.tsx` | 243 | ~36 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | — | `projectSettings.*` |
| `components/NetworkStatusPanel.tsx` | 180 | ~33 | button / label / empty-state / error / aria / title / tooltip | L15 toFixed, L37 toLocaleTimeString/toFixed, L137 toLocaleString | `network.*` |
| `components/HarnessHistoryTable.tsx` | 371 | ~32 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | — | `harness.history.*` |
| `components/RunningTasksPage.tsx` | 300 | ~31 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | — | `runningTasks.*` |
| `providerConfig.ts` | 809 | ~30 | label / error / tooltip | — | `config.providers.*` |
| `components/FilePreviewModal.tsx` | 373 | ~26 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | L43 toFixed, L44 toFixed | `files.preview.*` |
| `projectTasks.ts` | 384 | ~24 | label / error / tooltip | L155 toLocaleDateString | `harness.tasks.*` |
| `components/MentionPopover.tsx` | 318 | ~19 | label / empty-state / aria / title | — | `mentions.*` |
| `components/ProjectDetail.tsx` | 439 | ~19 | nav / label / empty-state / title / tooltip | — | `projectDetail.*` |
| `components/MemoryMonitorPanel.tsx` | 134 | ~17 | button / label / empty-state / error / aria | L119 toLocaleString | `memory.*` |
| `components/DynamicPromptSelectionMenu.tsx` | 459 | ~16 | button / label / empty-state / error / toast / aria / title | — | `dynamicPrompt.*` |
| `components/ProjectInsights.tsx` | 164 | ~16 | button / label / placeholder / empty-state / error / toast / aria / title / tooltip | — | `insights.*` |
| `App.tsx` | 637 | ~13 | nav / button / error / aria / title / tooltip | — | `app.*` |
| `components/Header.tsx` | 124 | ~13 | nav / button / label / aria / title / tooltip | — | `header.*` |
| `runningTasks.ts` | 134 | ~11 | label | duration helper L43; steps L49 | `runningTasks.*` |
| `components/ProjectRootsEditor.tsx` | 143 | ~10 | button / label / placeholder / empty-state / error | — | `projectRoots.*` |
| `lazyTabs.ts` | 66 | ~8 | nav / label | — | `projectDetail.tabs.*` |
| `mention.ts` | 620 | ~7 | label | — | `mentions.*` |
| `components/Skeletons.tsx` | 352 | ~6 | label | — | `skeletons.*` |
| `components/MarkdownPreview.tsx` | 159 | ~4 | empty-state / title / tooltip | — | `markdown.*` |
| `components/useProjectRoots.ts` | 96 | ~4 | error | — | `projectRoots.*` |
| `taskCreator.ts` | 143 | ~4 | toast | manual relative age L134,135,137,139 | `harness.draft.*` |
| `components/GitGraph.tsx` | 203 | ~3 | label / empty-state | L31 toLocaleString | `git.graph.*` |
| `main.tsx` | 57 | ~3 | button / error / title | — | `app.errorBoundary.*` |
| `rules.ts` | 138 | ~3 | error | — | `rules.*` |
| `taskDraft.ts` | 206 | ~3 | label / error | — | `harness.draft.*` |
| `taskContinuation.ts` | 66 | ~2 | label | manual signed turns L60 | `harness.continue.*` |
| `dynamicPrompt.ts` | 42 | ~1 | label | — | `dynamicPrompt.*` |
| `taskAttachments.ts` | 127 | ~1 | label | — | `attachments.*` |

### Reproducible grep evidence for the 25 largest copy inventories

`G` is the **actual number of source lines matched by the single rg pattern below**, not a claim that every matching line is translatable. Counts include comments, code literals and duplicates; they miss lowercase/interpolated-only copy and multiline template internals. The estimates in the main table additionally use a TypeScript AST walk over string/template literals and JSX text, plus manual review. Keeping G separate prevents CSS or parser strings from inflating the message estimate.

```bash
rg -n '(^|[^A-Za-z])(title|aria-label|placeholder|alt)=|>[[:space:]]*[A-Za-z][^<{]*[A-Za-z]|[\x27"`][A-Z][a-z]+[ A-Za-z]+' ui/hub/src/components/ProjectHarnessTab.tsx
```

| file | Actual grep matching lines (G) | Reviewed ~visible occurrences |
| --- | ---: | ---: |
| `components/ProjectHarnessTab.tsx` | 320 | ~284 |
| `components/TaskTrajectoryViewer.tsx` | 263 | ~235 |
| `components/TuiAgentsPage.tsx` | 208 | ~203 |
| `components/OpenDuckConfigPanel.tsx` | 167 | ~151 |
| `components/AgentLogTranscript.tsx` | 107 | ~72 |
| `components/ProviderStatsDashboard.tsx` | 62 | ~69 |
| `components/ProjectChatTasksDrawer.tsx` | 93 | ~67 |
| `taskTrajectory.ts` | 60 | ~65 |
| `components/ProjectTerminal.tsx` | 61 | ~62 |
| `components/ProjectGit.tsx` | 60 | ~57 |
| `components/ProjectRules.tsx` | 55 | ~55 |
| `taskPdfExport.ts` | 22 | ~55 |
| `components/ProjectFiles.tsx` | 64 | ~52 |
| `components/RegisterProjectModal.tsx` | 63 | ~47 |
| `components/ServerSettings.tsx` | 42 | ~45 |
| `configEditor.ts` | 43 | ~44 |
| `components/ProjectChat.tsx` | 68 | ~43 |
| `components/ProjectList.tsx` | 55 | ~41 |
| `components/TaskDraftPanel.tsx` | 55 | ~38 |
| `components/ProjectSettings.tsx` | 40 | ~36 |
| `components/NetworkStatusPanel.tsx` | 28 | ~33 |
| `components/HarnessHistoryTable.tsx` | 40 | ~32 |
| `components/RunningTasksPage.tsx` | 42 | ~31 |
| `providerConfig.ts` | 31 | ~30 |
| `components/FilePreviewModal.tsx` | 34 | ~26 |

### Grouped remainder

The 25 rows above account for ~1,873 occurrences. The remaining **23 copy-bearing source files** contribute ~207; their individual LOC and estimates are all retained in the inventory table. Shell, diagnostics, file/Markdown preview, mention UI, roots, skeletons, validation, draft restore, relative-time/duration, continuation, and prompt metadata are included rather than being hidden in a generic “utilities” bucket.

### Source files reviewed with no owned translatable copy

| file | LOC | Why review-only / do not translate |
| --- | ---: | --- |
| `acp.ts` | 186 | Protocol/client transport identifiers; backend content passes through. |
| `api.ts` | 325 | HTTP method/path/query tokens and propagated server errors, no owned English fallback. |
| `contexts/ThemeContext.tsx` | 84 | Theme persistence/state; the hook misuse Error is developer-only, not product copy. |
| `fileRefresh.ts` | 116 | File/watch/cache data operations. |
| `gitDiff.ts` | 147 | Git diff grammar (`new file`, `rename from`, etc.) and status/class tokens. Translating breaks parsing. |
| `gitGraph.ts` | 71 | Lane geometry only; visible copy is in components/GitGraph.tsx. |
| `markdown.ts` | 6 | Markdown filename detection only. |
| `pathUtils.ts` | 263 | Path/file-type classification and emoji icons; no English prose labels. |
| `projectIdentity.ts` | 57 | User-derived title casing and slug/path validation predicates, no owned message. |
| `taskCreateDraft.ts` | 427 | Draft persistence, defaults, normalization, schema and storage key; user-provided copy passes through. |
| `taskCreateDraftFlow.ts` | 139 | Draft lifecycle orchestration; no user-facing English literals. |
| `taskEditorDismiss.ts` | 17 | Keyboard/IME behavior; Escape is a KeyboardEvent.key token. |
| `theme.ts` | 32 | Theme values/storage key/document mutations, no display text. |
| `urlNav.ts` | 80 | Stable query navigation tokens; label translation lives in lazyTabs/components. |
| `vite-env.d.ts` | 1 | Vite declaration reference. |
| `styles.css` | 13603 | CSS and decorative generated punctuation, no English generated prose. Review layout for Chinese/selector fit; no copy migration required. |

### Helper and shared-boundary details

There is no `ui/hub/src/skeletons.ts` in this checkout: skeleton copy is in `components/Skeletons.tsx`, and Node-tested tab labels are in `lazyTabs.ts` (`skeletons.test.ts` imports that helper).

| Boundary | Specific finding / implementation instruction |
| --- | --- |
| `taskTrajectory.ts` | Conclusion headings/reasons, tool-purpose text, tool-call summaries, TUI states, error wrappers, transcript status fallbacks. Do not translate tool names, action kinds, field names, generated identity keys, or raw agent text. Existing code detects English heading/reason text (`245–248`); separate semantic status detection from localized display to prevent behavior regressions |
| `taskDraft.ts:109,118,121` | Generated `Subtask {n}` title and validation failures. Generated titles can be persisted: do not rewrite existing user/persisted titles on locale changes |
| `configEditor.ts:8–40` | Module-level option/label arrays need descriptors or locale-aware getters. Config keys and enum values stay stable |
| `providerConfig.ts` | Proxy validation, model fallback labels, stale-model hints and environment source tooltip; do not translate provider IDs, model IDs, URLs, or serialized request values |
| `projectTasks.ts` / `runningTasks.ts` | Status, steps, durations, relative activity. Preserve filter/search behavior when presentation becomes localized; stable status values remain English protocol values |
| `mention.ts` / `MentionPopover.tsx` | Translate Attached file, Rule source, Agent/Task wrappers and visible type badges. Keep `itemType` discriminants (`Attachment`, `Agent`, `Rule`, `Directory`, `File`, `Task`) and ranking map keys unchanged. Badge text currently participates in a CSS class in MentionPopover: use stable type tokens for class names |
| `dynamicPrompt.ts:24` | `Selected excerpt from ...` is agent-facing prompt metadata also shown to users; preserve source path and selected text. Consider keeping serialized prompt scaffolding English while localizing its UI presentation to avoid changing model instructions |
| `taskAttachments.ts:72–85` | Header is both visible prompt scaffolding and an `includes(ATTACHED_FILES_PROMPT_HEADER)` deduplication sentinel. Do not blindly translate this constant: keep serialized header stable or support both historical/localized headers when deduplicating |
| `taskContinuation.ts:60` | Signed turn-count summary is UI copy, even though the prose detector misses interpolated-leading templates |
| `taskPdfExport.ts` | Both printable HTML and directly generated PDF need labels, report title, footer/page number, dates/numbers and language metadata. Preserve markdown/user content and escaping. Existing bilingual copy is current English-mode behavior: preserve it byte-for-byte initially; converting en exports to English-only is a separate copy change |
| Imported Hub-core schedules | `ProjectHarnessTab.tsx:23–28` imports `describeTaskSchedule`, `timeInputValue`, `WEEKDAYS`; renders descriptions at **3060**, weekdays at **2830–2833**, time-input serialization at **2843**. These leak English from outside Hub |
| Shared schedule origins | `ui/hub-core/src/schedule.ts:10–18` weekday names; **112–116** `formatClock` uses browser-default `toLocaleTimeString([], ...)`; **172–185** manual descriptions; **188–193** paused suffix. New Hub-local i18n schedule adapter should consume pure schedule data and format through Hub intl. Preserve shared parser/cron/time-input APIs; do not add React/Hub i18n dependency to hub-core |
| Other external strings | Server/provider errors, network probe details, memory event summaries, rule/source descriptions, task names, terminal output, agent answers and model/provider names are external/user content. Localize owned wrappers and known statuses; retain raw content verbatim. Shared-package copy beyond the schedule leak needs separate ownership review |

### Every date / number / relative-time formatting site

The table below records **72 matching source lines** (some contain multiple calls) and contains **all production `toLocaleString`, `toLocaleDateString`, `toLocaleTimeString`, `Intl.*`, and `toFixed` matches** from an exhaustive rg scan. No existing production `Intl.*` construction was found in Hub. `toFixed` is included because several visible percentages/costs otherwise escape a locale scan. Multiple calls on one source line are indicated by the snippet; migrate each call. Paths are relative to `ui/hub/src`.

Use `intl.formatDate(date, options)` for dates/times, `intl.formatNumber(value, options)` for numbers/precision/costs/percentages, and `intl.formatRelativeTime(value, unit, options)` for relative time. Outside React use the same current IntlShape accessor. Preserve intended units, precision, timezones, and thresholds. Plain JSON/PDF encodings and calculations must remain locale-neutral.

| file:line | Existing call / purpose | Migration |
| --- | --- | --- |
| `taskTrajectory.ts:1485` | Calculation rounding of tokens/sec, not a display formatter | Keep calculation numeric; localize its eventual display |
| `taskPdfExport.ts:44` | return ${seconds.toFixed(2)}s | formatNumber |
| `taskPdfExport.ts:294` | exportDate: new Date().toLocaleString(), | formatDate with date + time fields |
| `taskPdfExport.ts:621` | <div class="info-value">${data.totalTokens > 0 ? ${data.totalTokens.toLocaleString()} (↑${data.inputTokens.toLocaleString()} ↓${data.outputTokens.toLocaleString()}) : '—'}</div> | formatNumber |
| `taskPdfExport.ts:880` | text: Tokens (Token 消耗):       ${data.totalTokens.toLocaleString()} (Prompt: ${data.inputTokens.toLocaleString()}, Output: ${data.outputTokens.toLocaleString()}), | formatNumber |
| `projectTasks.ts:155` | return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' }) | formatDate |
| `components/GitGraph.tsx:31` | return date.toLocaleString() | formatDate |
| `components/FilePreviewModal.tsx:43` | if (bytes < 1024 * 1024) return ${(bytes / 1024).toFixed(1)} KB | formatNumber |
| `components/FilePreviewModal.tsx:44` | return ${(bytes / (1024 * 1024)).toFixed(1)} MB | formatNumber |
| `components/ProjectFiles.tsx:32` | if (bytes < 1024 * 1024) return ${(bytes / 1024).toFixed(1)} KB | formatNumber |
| `components/ProjectFiles.tsx:33` | return ${(bytes / (1024 * 1024)).toFixed(1)} MB | formatNumber |
| `components/ProjectFiles.tsx:798` | {entry.modifiedAt ? new Date(entry.modifiedAt).toLocaleDateString() : '—'} | formatDate |
| `components/TaskTrajectoryViewer.tsx:932` | <span className="metric-value">{(metrics.durationMs / 1000).toFixed(2)}s</span> | formatNumber |
| `components/TaskTrajectoryViewer.tsx:963` | {metrics.totalTokens.toLocaleString()} | formatNumber |
| `components/TaskTrajectoryViewer.tsx:969` | ↑ {metrics.inputTokens.toLocaleString()} prompt • ↓{' '} | formatNumber |
| `components/TaskTrajectoryViewer.tsx:970` | {metrics.outputTokens.toLocaleString()} output | formatNumber |
| `components/TaskTrajectoryViewer.tsx:1752` | {new Date(step.timestamp).toLocaleTimeString([], { | formatDate |
| `components/TaskTrajectoryViewer.tsx:1788` | ⚡ {telemetry.totalTokens.toLocaleString()} tok | formatNumber |
| `components/TaskTrajectoryViewer.tsx:2010` | <Clock size={12} /> {new Date(step.timestamp).toLocaleTimeString()} ({step.durationMs} ms) | formatDate |
| `components/TaskTrajectoryViewer.tsx:2017` | ⚡ {telemetry.totalTokens.toLocaleString()} tokens | formatNumber |
| `components/TaskTrajectoryViewer.tsx:2479` | {telemetry.inputTokens?.toLocaleString() ?? '—'} | formatNumber |
| `components/TaskTrajectoryViewer.tsx:2485` | {telemetry.outputTokens?.toLocaleString() ?? '—'} | formatNumber |
| `components/TaskTrajectoryViewer.tsx:2491` | {telemetry.totalTokens?.toLocaleString() ?? '—'} | formatNumber |
| `components/TaskTrajectoryViewer.tsx:2574` | <span className="mini-val font-mono">{call.latencyMs.toLocaleString()} ms</span> | formatNumber |
| `components/TaskTrajectoryViewer.tsx:2581` | {(call.tokenUsage.inputTokens ?? 0).toLocaleString()} | formatNumber |
| `components/TaskTrajectoryViewer.tsx:2587` | {(call.tokenUsage.outputTokens ?? 0).toLocaleString()} | formatNumber |
| `components/TaskTrajectoryViewer.tsx:2596` | ).toLocaleString()} | formatNumber |
| `components/TaskTrajectoryViewer.tsx:2604` | <span className="mini-val font-mono">${call.cost.toFixed(5)}</span> | formatNumber |
| `components/ProjectList.tsx:288` | {new Date(project.lastActivityAt).toLocaleDateString()} | formatDate |
| `components/ProjectList.tsx:367` | ? new Date(project.lastActivityAt).toLocaleString() | formatDate |
| `components/AgentLogTranscript.tsx:826` | <span>{call.latencyMs.toLocaleString()} ms</span> | formatNumber |
| `components/AgentLogTranscript.tsx:1151` | const scoreVal = judgment.verdict?.value !== undefined ? judgment.verdict.value.toFixed(4) : undefined | formatNumber |
| `components/AgentLogTranscript.tsx:1153` | judgment.verdict?.threshold !== undefined ? judgment.verdict.threshold.toFixed(2) : undefined | formatNumber |
| `components/AgentLogTranscript.tsx:1155` | judgment.confidence !== undefined ? ${(judgment.confidence * 100).toFixed(1)}% : undefined | formatNumber |
| `components/ProjectTerminal.tsx:100` | const timestamp = new Date().toLocaleTimeString() | formatDate |
| `components/ProviderStatsDashboard.tsx:150` | <div className="kpi-value font-mono">{successRate.toFixed(1)}%</div> | formatNumber |
| `components/ProviderStatsDashboard.tsx:178` | {Math.round(providerStats.avgDurationMs).toLocaleString()}{' '} | formatNumber |
| `components/ProviderStatsDashboard.tsx:184` | : ${(providerStats.totalDurationMs / 1000).toFixed(2)}s total completion time} | formatNumber |
| `components/ProviderStatsDashboard.tsx:194` | {providerStats.totalTokens.toLocaleString()} | formatNumber |
| `components/ProviderStatsDashboard.tsx:197` | ↑ {providerStats.inputTokens.toLocaleString()} prompt • ↓{' '} | formatNumber |
| `components/ProviderStatsDashboard.tsx:198` | {providerStats.outputTokens.toLocaleString()} output | formatNumber |
| `components/ProviderStatsDashboard.tsx:208` | <div className="kpi-value font-mono">${providerStats.totalCost.toFixed(4)}</div> | formatNumber |
| `components/ProviderStatsDashboard.tsx:237` | {pSuccessRate.toFixed(1)}% | formatNumber |
| `components/ProviderStatsDashboard.tsx:246` | <span className="p-metric-value">{pStat.totalTokens.toLocaleString()}</span> | formatNumber |
| `components/ProviderStatsDashboard.tsx:310` | {mStat.inputTokens.toLocaleString()} / {mStat.outputTokens.toLocaleString()} | formatNumber |
| `components/ProviderStatsDashboard.tsx:312` | <td className="font-mono font-bold">{mStat.totalTokens.toLocaleString()}</td> | formatNumber |
| `components/ProviderStatsDashboard.tsx:316` | ? $${mStat.totalCost.toFixed(4)} | formatNumber |
| `components/ProviderStatsDashboard.tsx:459` | <td className="font-mono">{call.latencyMs.toLocaleString()} ms</td> | formatNumber |
| `components/ProviderStatsDashboard.tsx:505` | title={Input: ${call.tokenUsage.inputTokens?.toLocaleString() ?? 0} \| Output: ${call.tokenUsage.outputTokens?.toLocaleString() ?? 0}} | formatNumber |
| `components/ProviderStatsDashboard.tsx:507` | {(call.tokenUsage.totalTokens ?? (call.tokenUsage.inputTokens ?? 0) + (call.tokenUsage.outputTokens ?? 0)).toLocaleString()} | formatNumber |
| `components/NetworkStatusPanel.tsx:15` | <strong>{probe.latencyMs === null ? probe.status : ${probe.latencyMs.toFixed(1)} ms}</strong> | formatNumber |
| `components/NetworkStatusPanel.tsx:37` | title={${new Date(sample.time).toLocaleTimeString()}: ${sample.probe?.detail ?? 'No sample'}${sample.probe?.latencyMs != null ?  · ${sample.probe.latencyMs.toFixed(1)} ms : ''}} | formatDate + formatNumber + localized tooltip |
| `components/NetworkStatusPanel.tsx:137` | <span className="muted text-sm">Checked {new Date(report.checkedAt).toLocaleString()}</span> | formatDate |
| `components/ProjectChatTasksDrawer.tsx:744` | <span>{new Date(task.nextRunAt).toLocaleString()}</span> | formatDate |
| `components/ProjectChatTasksDrawer.tsx:749` | <span>{new Date(task.createdAt).toLocaleString()}</span> | formatDate |
| `components/ProjectHarnessTab.tsx:1870` | Project eval completed! Pass rate: ${(report.metrics.passRate * 100).toFixed(1)}% (${report.metrics.passedTasks}/${report.metrics.totalTasks}), | formatNumber |
| `components/ProjectHarnessTab.tsx:3062` | <span>Next run {new Date(t.nextRunAt).toLocaleString()}</span> | formatDate |
| `components/ProjectHarnessTab.tsx:3605` | <div className="sub-timestamp">{new Date(r.timestamp).toLocaleString()}</div> | formatDate |
| `components/ProjectHarnessTab.tsx:3612` | {(r.passRate * 100).toFixed(1)}% | formatNumber |
| `components/ProjectHarnessTab.tsx:3618` | <td>{(r.avgDurationMs / 1000).toFixed(2)}s</td> | formatNumber |
| `components/ProjectHarnessTab.tsx:3648` | {(selectedReportDetail.metrics.passRate * 100).toFixed(1)}% Passed) | formatNumber |
| `components/ProjectHarnessTab.tsx:3665` | <span>Duration: {(tr.durationMs / 1000).toFixed(2)}s</span> | formatNumber |
| `components/ProjectHarnessTab.tsx:3708` | <div>Recorded: {new Date(c.createdAt).toLocaleString()}</div> | formatDate |
| `components/TuiAgentsPage.tsx:524` | <div style={{ fontSize: '12px' }}>{s.lastOperationAt ? new Date(s.lastOperationAt).toLocaleTimeString() : 'None'}</div> | formatDate |
| `components/TuiAgentsPage.tsx:526` | Input: {s.lastInputAt ? new Date(s.lastInputAt).toLocaleTimeString() : 'N/A'} | formatDate |
| `components/TuiAgentsPage.tsx:532` | {new Date(s.reclaimAfter).toLocaleTimeString()} | formatDate |
| `components/TuiAgentsPage.tsx:650` | <td style={{ padding: '12px' }}>{new Date(s.createdAt).toLocaleString()}</td> | formatDate |
| `components/TuiAgentsPage.tsx:651` | <td style={{ padding: '12px' }}>{s.reclaimedAt ? new Date(s.reclaimedAt).toLocaleString() : s.stoppedAt ? new Date(s.stoppedAt).toLocaleString() : 'N/A'}</td> | formatDate |
| `components/TuiAgentsPage.tsx:704` | <td style={{ padding: '10px 12px', whiteSpace: 'nowrap' }}>{new Date(op.startedAt).toLocaleTimeString()}</td> | formatDate |
| `components/TuiAgentsPage.tsx:873` | <span>{q.remainingPercent !== undefined ? ${q.remainingPercent.toFixed(1)}% left : 'Unknown'}</span> | formatNumber |
| `components/TuiAgentsPage.tsx:886` | <span>Observed: {new Date(q.observedAt).toLocaleTimeString()}</span> | formatDate |
| `components/MemoryMonitorPanel.tsx:119` | <time dateTime={entry.at}>{new Date(entry.at).toLocaleString()}</time> | formatDate |

Manual formatting definitions and all existing helper invocations:

| file:line | Formatting / affected calls | Migration detail |
| --- | --- | --- |
| `projectTasks.ts:118,119,121,128,129,130` | `formatDurationMs`, `<1s`, compact h/m/s | Localized duration units + formatNumber; preserve existing rounding and omitted-zero behavior |
| `projectTasks.ts:134,141,144,148,152,155` | `formatRelativeTime`: just now, Nm/Nh/Nd ago, older-date fallback | formatRelativeTime for localized output, formatDate for old dates; preserve UTC date fallback and threshold behavior |
| `projectTasks.ts:200,231,241,252,330,334` | All calls to duration/relative helpers in task/history presentation | Shared runtime formatter; recompute with selected locale |
| `runningTasks.ts:43` | runningElapsedLabel → formatDurationMs | Same duration formatter |
| `runningTasks.ts:49` | singular/plural step labels | ICU plural + formatted count |
| `components/ProjectHarnessTab.tsx:2009` | formatDurationMs for live elapsed time | Same duration formatter |
| `taskCreator.ts:134,135,137,139` | Restored draft a moment/minute/hour/day ago, manual plural suffixes | formatRelativeTime and localized surrounding message; preserve en phrasing via explicit en defaults if Intl differs |
| `taskPdfExport.ts:39,40,41,44,48` | formatDuration: ms, fractional seconds, minutes+seconds | formatNumber + translated unit message; keep existing precision |
| `taskPdfExport.ts:260` | formatDuration invocation for export data | Same export-locale formatter |
| `components/FilePreviewModal.tsx:40,42,43,44` | formatBytes: B/KB/MB and decimal precision | formatNumber + unit label; preserve binary conversion factors |
| `components/FilePreviewModal.tsx:200,339` | formatBytes invocations | Same helper |
| `components/ProjectFiles.tsx:29,31,32,33` | Duplicate formatBytes implementation | Share a Node-testable localized display helper |
| `components/ProjectFiles.tsx:610,796` | formatBytes invocations | Same helper |
| `components/GitGraph.tsx:28,31,182` | formatTime definition/native call/invocation | formatDate with date and time |
| `taskContinuation.ts:60` | `+{n} turns` / negative count | ICU plural + number; preserve explicit plus sign |
| `taskPdfExport.ts:612–617,873,876` | Started/completed timestamps emitted raw from ISO strings | Deliberate display decision: formatDate for reader-facing dates; retain raw values in JSON/data |
| `taskPdfExport.ts:858` | export-date fallback `new Date().toISOString()` | Human footer should use selected export locale; ISO remains valid for machine metadata |
| `ui/hub-core/src/schedule.ts:112–116` | Shared formatClock native time call | Hub-local adapter uses formatDate; keep numeric `HH:mm` for `<input type="time">` |
| `components/ProjectHarnessTab.tsx:2830,3060` | Shared weekday labels and describeTaskSchedule | Localize weekday names and schedule sentence in Hub adapter, using stable weekday indices |

**English regression constraint:** native Intl short relative time often produces `3 min. ago`, not current `3m ago`; medium date presets may also differ. Exercise Intl formatting, but preserve English message templates and explicit options where needed. There is no guarantee that blindly replacing helper bodies with Intl presets preserves English. Add exact English assertions before approving any output change.

Raw JSX counts (project totals, pagination, steps, concurrency, turns, milliseconds) do not appear as formatting *calls*. They also need formatNumber/ICU review while extracting messages. Do not apply locale formatting to numeric `<input>` values, CSS widths, cron fields, JSON exports, hex/PDF object offsets, IDs, parser text, or API payloads. `taskPdfExport.ts:838,1027` padStart calls encode PDF structure, not visible localized numbers.

## 6. Key-naming convention

Use stable feature namespaces and semantic names; do not use English copy as a key or use array indexes as IDs.

| Prefix | Examples / ownership |
| --- | --- |
| `app.*` | documentTitle, connectionError, errorBoundary.reload |
| `header.*` | projects, running, settings, themeToggleLabel, localeLabel |
| `projectList.*`, `projectDetail.*` | list empty states, filters, tabs, workspace metadata |
| `harness.*` | task editor, budgets, scheduling, verification, eval, cassettes, history |
| `trajectory.*` | execution inspection, transcript, final answer, telemetry |
| `config.*` | providers, proxy, tool mode, judge, validation |
| `tuiAgents.*` | sessions, profiles, quotas, lifecycle actions |
| `settings.*`, `projectSettings.*` | server appearance/connection vs project settings |
| `chat.*`, `files.*`, `git.*`, `rules.*`, `mentions.*` | feature-local UI and helper descriptors |
| `export.pdf.*` | HTML/direct-PDF report labels, page footer, export titles |
| `common.*` | Truly identical semantics/copy: cancel, save, retry, close; do not merge different capitalization or punctuation |

**Hard rule: English `defaultMessage` must byte-for-byte preserve current copy.** Preserve capitalization, Unicode ellipsis vs three dots, curly/straight quotes, punctuation, emoji, spaces, line breaks, units and plural behavior. Replace dynamic values with named ICU placeholders while preserving the resulting English text for the same inputs. For JSX indentation, use the actual current rendered whitespace; do not copy indentation into catalog values. Keep explicit tests for existing English output. Existing bilingual PDF text is part of this baseline.

Use complete messages for interpolated sentences, plurals and rich text, rather than concatenating translated fragments. ICU braces/apostrophes and rich-text tags require escaping/review; interpolation values remain user/backend content. Do not translate slug, URL-page/tab, enum, provider/config keys or machine-readable identifiers.

## 7. Affected-files list

### (a) New files proposed

| New path | Role |
| --- | --- |
| `ui/hub/src/i18n/locale.ts` | Pure supported-locales, normalization/detection/storage-key utilities; no React or browser globals at import time |
| `ui/hub/src/i18n/catalogs.ts` | JSON imports + descriptor-catalog to runtime-message normalization; explicit en/zh-CN mapping |
| `ui/hub/src/i18n/index.ts` | Non-hook runtime Intl instance/getIntl/translate and descriptors API; never re-export TSX context/hook from this barrel |
| `ui/hub/src/i18n/messages/en.json` | Canonical extracted source descriptors |
| `ui/hub/src/i18n/messages/zh-CN.json` | Complete Simplified Chinese catalog, same keys and ICU arguments |
| `ui/hub/src/contexts/LocaleContext.tsx` | LocaleProvider, persistence and immediate IntlProvider updates; lang/title synchronization |
| `ui/hub/src/hooks/useLocale.ts` | Hook reading context; React-only import path, separate from Node helper runtime |
| `ui/hub/src/components/LocaleSwitcher.tsx` | Shared accessible Header/settings control |
| `ui/hub/src/i18n/schedule.ts` | Hub-local localized schedule/weekday adapter over shared pure schedule parsing |
| `ui/hub/scripts/i18n-check.mjs` | ESM extraction-drift + parity + ICU validation checker |
| `ui/hub/src/locale.test.ts` | Node-discovered locale normalization/detection/storage/runtime/catalog tests; root src placement matches `src/*.test.ts` glob |

A separate descriptors `.ts` module or shared number/duration utility may be useful when a feature otherwise exposes React to helpers; keep it Node-importable. Compiled catalogs are optional with only two languages, not a prerequisite.

### (b) Existing files to modify

The per-file copy inventory above is the complete **48-file production source modification list**, including all **31 existing components with copy**, App, main, hook and helper modules. Each row names its namespace and migration surface. In addition:

| Existing path | Change |
| --- | --- |
| `ui/hub/package.json` | Declare react-intl, required extraction/parser dev tooling, and i18n check/extract scripts; preserve existing test command |
| `ui/pnpm-lock.yaml` | Keep importer/dependency metadata consistent using pnpm; no manual dependency-only drift |
| `ui/hub/index.html` | Review/update locale/title fallback metadata as needed; preserve lang=en and current English title while runtime takes ownership |
| `ui/hub/src/main.tsx` | Already counted: provider wiring and outer ErrorBoundary copy |
| `ui/hub/src/App.tsx` | Already counted: shell/fallback/action copy and contextual render refresh |
| `ui/hub/src/components/Header.tsx` | Already counted: switcher mount, nav/status/theme aria/title |
| `ui/hub/src/components/ServerSettings.tsx` | Already counted: settings copy, optional second switcher |

**Baseline existing-file scope: 51 files** = 48 source files + package.json + pnpm lockfile + index.html. This is an implementation planning count, not files changed by recon. Existing tests listed next are compatibility constraints; modifying their English expected strings should not be necessary. Optional layout fixes in styles.css and any justified test changes expand this count.

### Existing Node tests coupled to user-visible copy

| Existing test | Copy assertion / implication |
| --- | --- |
| `src/projectTasks.test.ts:178–198` and presentation tests later in file | Exact compact duration and relative time (`<1s`, `3m ago`, `Sep 1`), success/failed/running presentation strings. Preserve English and UTC fallback |
| `src/runningTasks.test.ts:36–75` | Harness fallback, status/activity/step labels and elapsed text |
| `src/taskTrajectory.test.ts:188–209,318–406,418+` | Conclusion headings, Failure Reason markdown, tool summaries and TUI state labels; many strings are exact behavioral expectations |
| `src/taskDraft.test.ts:106–129` | Exact subtask validation errors |
| `src/taskCreator.test.ts:104–124` | Exact draft-restored moment/minute/hour/day messages including singular/plural |
| `src/taskContinuation.test.ts:113–120` | Exact `+25 turns` / `-10 turns` |
| `src/configEditor.test.ts:42–58,90` | Regex checks of number, minimum, whole-number, protected-key, name and endpoint validation messages |
| `src/providerConfig.test.ts:76–85,113–116,280–305,428–436,451–482,533–544` | Exact fallback labels and proxy errors; regex checks of validation/inherited-model/custom-option labels |
| `src/rules.test.ts:82–86` | Regex checks of required/path/length validation |
| `src/skeletons.test.ts:45–49` | Exact getTabLabel English labels from lazyTabs.ts; no import of Skeletons.tsx |
| `src/dynamicPrompt.test.ts:22–28` | Exact English selected-excerpt prompt scaffold |
| `src/taskPdfExport.test.ts:19–24,118–130,164–173` and later PDF tests | Exact duration/report title and rendered content checks; preserve export copy and user content |
| `src/projectChatTasksDrawer.test.ts:27–56` | Asserts English validation and Additional instructions prompt text in **test-local duplicated functions**. Does not import or exercise actual TSX component behavior; a passing test is not component i18n coverage |

These **13 files assert copy or formatting**. Other tests assert user data, paths, protocol/discriminant strings or markup rather than owned UI text. In particular, `mention.test.ts` asserts itemType and insertion paths (keep those stable), `taskAttachments.test.ts` asserts original prompt/attachment inclusion rather than a localized header, and `taskCreateDraft.test.ts` / `taskCreateDraftFlow.test.ts` assert persisted user drafts and state, not translated notices. Do not translate test fixture strings.

For locale-sensitive helper tests, initialize explicit English in setup and restore runtime state after each locale mutation; do not let the machine's navigator/storage determine expected text. Add Chinese behavior assertions in the new pure test file. Context/TSX rendering tests require separate future tooling; do not smuggle `.tsx` into Node's existing import graph.

### Catalog format recommendation

Recommend **JSON catalogs**, retaining Desktop's `{"id":{"defaultMessage":"..."}}` translator shape and extraction workflow. In browser-facing `.ts`, statically import with **`with { type: 'json' }`** so the same modules are supported by native Node 22 ESM and Vite; normalize descriptors to strings for IntlProvider/createIntl. Verify this path with the existing commands in phase 2. For the checker, use ESM `createRequire(import.meta.url)` to read JSON or plain fs+JSON.parse. Do not import `node:module` in browser runtime code.

The older `assert { type: 'json' }` import syntax should not be chosen for the observed Node 22 runtime; use current import attributes (`with`). `resolveJsonModule` only solves TS typing. `.ts` catalog modules with `export default` are a viable Node-importable alternative, but would require adapting Desktop's JSON extraction/check/translation tools; they offer less reuse here. A pure helper test that transitively imports the catalogs should verify native JSON loading explicitly.

### (c) Review-only / do-not-touch

The zero-copy table in section 5 lists every production file with no copy migration. `ThemeContext.tsx`/`theme.ts` are reference patterns; `styles.css` needs a visual/layout review only unless overflow requires a targeted fix. Keep gitDiff grammar, pathUtils classifications, projectIdentity/user-title logic, URL/query values, draft schemas and keyboard handling stable.

Do not change `ui/desktop/**`, root `I18N.md`, or `ui/hub-core/**` just to reuse i18n. Shared schedule parsing remains unchanged; adapt its display at the Hub boundary. Backend/Rust, SDK generated types, terminal output, raw task prompts/answers and external provider/server messages are outside this copy-migration scope. Do not recreate any Desktop generated API code. The pre-existing untracked `.agent/notes-plan-task-ui-hub-i18n-en-zh.md` is unrelated and was left untouched.

## 8. Phased implementation plan

| Phase | Concrete work | Exit / verification |
| --- | --- | --- |
| 1. Recon (this document) | Verify configuration, Desktop reuse, copy surfaces, test coupling and formatting sites | Document only; source remains unchanged. For upstream implementation, confirm Ready issue and agreed design before coding |
| 2. Infrastructure | Declare deps; pure locale utils/catalog loader/runtime; LocaleContext/useLocale; en/zh catalogs; checker; root-level locale.test.ts | `cd ui/hub && pnpm run typecheck`; `pnpm test`; proposed `pnpm run i18n:check`; `pnpm build`. Specifically prove helpers load under native Node with JSON attributes and no TSX |
| 3. Core shell + navigation | main/ErrorBoundary, App, Header switcher, settings, project list/detail, lazy tab labels and skeletons; document lang/title | Same checks; manually switch both directions, reload, unsupported storage, browser-language order, lazy fallbacks, keyboard-only access and split pane preservation |
| 4. Heavy pages + helpers | Harness/trajectory/log/config/TUI first, then chat/tasks/files/git/rules/terminal/insights/diagnostics; all helper messages; schedules and HTML/direct-PDF exports | Same checks; checker has no missing/extra keys; exact English helper assertions pass; exercise Chinese with all namespaces, active tasks, restore notices, errors and open dialogs |
| 5. QA | Full catalog/ICU audit, aria/title/tooltips, date/number precision/timezones, truncation, exports, raw-content boundaries and offline refresh | `cd ui/hub && pnpm run typecheck`; `pnpm test`; `pnpm build`; `pnpm run i18n:check`; documented manual en/zh smoke pass including persisted locale |

Proposed checker command: `cd ui/hub && node scripts/i18n-check.mjs` (wire as `i18n:check`). It should extract current source defaults to a temporary file, reject duplicate IDs with conflicting defaults, compare extracted English with committed en.json, require identical en/zh-CN key sets and nonempty messages, parse every ICU message, compare argument/tag sets recursively, validate plural/select branches (`other` required; language-specific plural categories may differ), and fail on parse errors. Do not require English and Chinese text to differ for brands, acronyms or technical values. Reject stale/unreferenced keys after extraction; report file/key diagnostics. Declare the CLI/parser directly when using them instead of relying on Desktop's transitive tools.

No commands in this plan were run as builds/tests during recon. Future implementation should run the listed Hub checks. Rust cargo builds/tests/clippy are not needed for this documentation-only change; the repository's cargo-format rule concerns Rust edits, of which there are none.

## 9. Risks & gotchas

| Risk | Required handling |
| --- | --- |
| Unused imports/parameters | TS noUnusedLocals/noUnusedParameters makes staged extraction fail if unused intl/descriptors remain |
| Provider placement | Locale/IntlProvider must wrap App above lazy Suspense boundaries and translated skeleton/fallback content; avoid App remount on language change |
| Outer class ErrorBoundary | main.tsx:31–42 has English failure/reload text; boundary wraps ThemeProvider at 50–56. Use a preinitialized non-hook accessor, not useIntl in the class. Raw Error.message may still be external English |
| Error after provider failure | Accessor must work without a mounted React provider and fall back safely to original English descriptors |
| Startup-only values | Desktop currentLocale is immutable at module load. Hub's runtime accessor and any cached labels must update; never capture initial locale in arrays, effects or event closures |
| Stale string state | Existing error/toast/draft banners and ProjectTerminal timestamp strings can remain in their old locale. Prefer semantic data/descriptors and store timestamps as instants for rendering |
| Formatting changes | Browser-default locale calls are inconsistent today. Preserve precision, UTC where specified, and English compact wording; formatRelativeTime presets alone can fail exact regression tests |
| document.title / language | Initial index.html is en/OpenDuck; update root lang and title on selection. Export HTML has a separate hardcoded zh-CN lang to handle |
| Accessible copy | Extract aria-label, title, alt and hidden labels, theme tooltip values and icon-only action names, not just text nodes |
| Helpers outside React | `.ts` message/error factories need the shared non-hook formatter. Importing LocaleContext/useLocale through their runtime barrel breaks Node tests |
| Node tests | Glob is only `src/*.test.ts`; nested new tests will silently not run. Typecheck excludes tests. No jsdom/Vitest/TSX imports are available |
| ESM/JSON | Use explicit .ts helper imports, current JSON import attributes or checker-side createRequire; avoid Node-only modules in browser code and CommonJS require in Hub .js scripts |
| ICU copy fidelity | Source braces, apostrophes, markup tags and multiline markdown need careful escaping. Never translate user-provided prompts, paths, command examples' machine tokens or source snippets |
| Sentinel/parser coupling | Attached-files header and transcript-heading detection currently inspect English text; translating those without semantic separation can duplicate prompt sections or failure reports |
| Persisted generated copy | Generated subtask names/drafts may survive locale switches; treat saved text as data instead of retroactively translating it |
| Shared schedule leakage | WEEKDAYS and describeTaskSchedule are English in hub-core; localize via Hub adapter. A src-only string scan misses this visible surface |
| CSS coupling | Mention badge CSS uses typeBadgeText; separate stable classes from translated text. Check compact Header actions, grids/tables, dialogs and CJK wrapping |
| PDF | Both standalone HTML and direct PDF embed bilingual copy; preserve escapeHtml/markdown processing, font/CJK encoding and page layout. Do not localize structural PDF numbers/offsets or JSON export data |
| Async locales | If catalogs are lazy-loaded, commit a matching locale/messages pair atomically and prevent older requests overwriting later selections |
| Dependency resolution | Hoisted Vite 8 differs from Hub-resolved Vite 6; use Hub scripts. Hoisted react-intl availability must still be reflected in Hub package and lockfile |

## Recon commands used

Commands below are read-only and rerunnable from repository root, except the final document-writing step. Temporary scan data/scripts were kept under `/tmp`; they are not project deliverables. Every src file was read by a temporary TypeScript parser scan (including tests); visible JSX/literal candidates were then inspected along with targeted source ranges. No network lookup was needed: this inventory describes checked-in code and local installed packages.

```bash
pwd
cat AGENTS.md
rg --files -g AGENTS.md -g I18N.md -g package.json -g tsconfig.json -g pnpm-workspace.yaml
cat I18N.md
cat ui/desktop/src/i18n/index.ts
cat ui/hub/package.json ui/hub/tsconfig.json ui/pnpm-workspace.yaml
cat ui/package.json
rg --files ui/hub/src
find ui/hub/src -type f -print0 | xargs -0 wc -l
cat ui/hub/src/main.tsx ui/hub/src/App.tsx ui/hub/index.html
cat ui/hub/src/contexts/ThemeContext.tsx ui/hub/src/theme.ts
nl -ba ui/hub/src/components/Header.tsx
nl -ba ui/hub/src/components/ServerSettings.tsx
rg -n 'i18n|react-intl|useIntl|formatMessage' ui/hub/src ui/hub/package.json
rg -n '[\p{Han}]' ui/hub/src --glob '!*.test.ts'
cat ui/node_modules/react-intl/package.json
cat ui/node_modules/intl-messageformat/package.json
cat ui/hub/node_modules/vite/package.json
cat ui/node_modules/react/package.json ui/node_modules/react-dom/package.json
cat ui/node_modules/typescript/package.json
ls ui/node_modules/@formatjs
node --version
rg -n 'i18n:|IntlProvider|loadMessages' ui/desktop/package.json ui/desktop/src/renderer.tsx
cat ui/desktop/src/i18n/test-utils.tsx
head -25 ui/desktop/src/i18n/messages/zh-CN.json
cat ui/desktop/scripts/i18n-check.js
cat ui/desktop/scripts/i18n-compile.js
cat ui/desktop/scripts/i18n-validate-locale.js
rg -n 'toLocale(String|DateString|TimeString)|Intl\.|toFixed\(' ui/hub/src --glob '!*.test.ts'
rg -n 'padStart|toPrecision|getHours|getMinutes|getDate|getMonth|getFullYear|ago|just now|yesterday|format[A-Z]' ui/hub/src --glob '!*.test.ts' --glob '!styles.css'
rg -n 'en-US|en-GB|zh-CN|lang=|document.title' ui/hub/src ui/hub/index.html --glob '!*.test.ts'
rg -n 'assert\.|test\(' ui/hub/src/*.test.ts
rg -n 'WEEKDAYS|describeTaskSchedule|formatClock|timeInputValue' ui/hub-core/src/schedule.ts ui/hub/src/components/ProjectHarnessTab.tsx
rg -n 'content:\s*[^;]+' ui/hub/src/styles.css
rg -n 'from .*\.tsx|from .*\.ts' ui/hub/src
cat ui/hub/src/{taskDraft,taskTrajectory,pathUtils,gitDiff,providerConfig,rules,projectTasks,runningTasks}.ts
cat ui/hub/src/{taskCreateDraft,taskCreateDraftFlow,taskCreator,taskAttachments,taskPdfExport,markdown,configEditor,dynamicPrompt,mention,projectIdentity,gitGraph,lazyTabs,urlNav,taskContinuation}.ts
git status --short
git diff -- ui/hub/src
```

Reproduce the exact grep evidence counts (the same pattern was invoked once per inventory file; these counts are matching lines, not message keys):

```bash
rg -c '(^|[^A-Za-z])(title|aria-label|placeholder|alt)=|>[[:space:]]*[A-Za-z][^<{]*[A-Za-z]|[\x27"`][A-Z][a-z]+[ A-Za-z]+' ui/hub/src --glob '*.tsx' --glob '*.ts' --glob '!*.test.ts'
```

