# Task: UI Hub i18n (en + zh-CN)

## Subtask 4/5 — extract-strings (全站文案抽取与中英双语填充) — IN PROGRESS

### Harness expectations (.goose/tasks/task-ui-hub-i18n-en-zh.yaml, extract-strings)
- targetFiles: `src/`; maxTurns 564.
- expectedOutputs: exactly ONE **`kind: note`** check — "组件层已完成 i18n 改造，且中英文语言包均被填充". NO command check.
- Subtask 5 (`verify`) is the one that runs `npm run build` from workspace ROOT (see cwd trap below).

### CWD trap (learned in subtask 2)
Harness command checks run with cwd = workspace root `/mnt/e/openduck-harness`, NOT `ui/hub`.
Root `package.json` shim exists for that reason (scripts delegate via `cd ui/hub`).

### VERIFIED BASELINE (before any subtask-4 change)
| check | result |
| --- | --- |
| `cd ui/hub && pnpm run typecheck` | PASS (clean) |
| `cd ui/hub && pnpm test` | PASS 308 tests, 0 fail |
| `cd ui/hub && pnpm run build` | PASS, built in ~4.7s |
| Pure-.ts can import `src/i18n/index.ts` under `node --experimental-strip-types` | YES — probe printed `Cancel` with NO init (resolveFallback path) |

That last point is the critical enabler: helper `.ts` modules can call `translate()` and, because
`translate()` falls back to `resources.en` when i18next is uninitialised, **the 13 existing
copy-asserting Node tests keep their exact English expectations** without touching them.

### Existing foundation (subtasks 2-3, DO NOT REDO)
- `src/locales/en.json`, `src/locales/zh.json` — 19 keys, modules `app/header/common/language`
- `src/i18n/locales.ts` — pure, DOM-free; `Locales = 'en' | 'zh-CN'`, `goose-hub-locale`
- `src/i18n/catalogs.ts` — JSON `with { type: 'json' }`
- `src/i18n/index.ts` — `initI18n`, `changeLocale`, `syncLocaleDocument`, non-hook `translate(key, values)`
- `src/contexts/LocaleContext.tsx` — `LocaleProvider` + `useLocale`; locale DERIVED from `useTranslation()`
- `src/components/LocaleSwitcher.tsx` — native `<select>`, mounted in `Header.tsx` `header-meta-actions`
- Tests: `src/i18n.test.ts`, `src/locale-transition.test.ts`

### Source-of-truth recon: `docs/i18n-inventory.md` (558 lines)
~2,080 user-visible occurrences / 48 copy-bearing files. Namespace prefixes and per-file
date/number sites already enumerated there. Do NOT re-do this recon; reuse it.

### Copy-bearing components to migrate (LOC, est. strings)
ProjectHarnessTab 4246/284 · TaskTrajectoryViewer 2687/235 · TuiAgentsPage 1504/203 ·
OpenDuckConfigPanel 1553/151 · AgentLogTranscript 1207/72 · ProviderStatsDashboard 549/69 ·
ProjectChatTasksDrawer 817/67 · taskTrajectory.ts 1993/65 · ProjectTerminal 665/62 ·
ProjectGit 614/57 · ProjectRules 480/55 · taskPdfExport.ts 1135/55 (PRE-EXISTING bilingual,
   leave alone unless needed) · ProjectFiles 844/52 · RegisterProjectModal 479/47 ·
ServerSettings 302/45 · configEditor.ts 254/44 · ProjectChat 837/43 · ProjectList 393/41 ·
TaskDraftPanel 336/38 · ProjectSettings 243/36 · NetworkStatusPanel 180/33 ·
HarnessHistoryTable 371/32 · plus App 637, ProjectDetail 439, FilePreviewModal 373,
MarkdownPreview 159, MemoryMonitorPanel 134, MentionPopover 318, DynamicPromptSelectionMenu 459,
GitGraph 203, ProjectInsights 164, ProjectRootsEditor 143, Skeletons 352, RunningTasksPage 300.

### Execution plan (delegated; orchestrator does not write code)
1. **codex** — design pass: catalog namespace map, `src/i18n/format.ts` API (Intl date/number/percent/
   list/relative-time bound to active locale), interpolation/plural conventions, and the hard rule
   that English catalog values must byte-match current copy. Also design a checker script.
2. **grok A** — foundation + small/medium components (App, Header, Skeletons, ProjectList,
   ProjectDetail, ProjectInsights, RunningTasksPage, FilePreviewModal, MarkdownPreview,
   MemoryMonitorPanel, NetworkStatusPanel, GitGraph, HarnessHistoryTable, ProjectRootsEditor)
3. **agy A** — `ProjectHarnessTab.tsx` alone (284 strings)
4. **grok B** — `TaskTrajectoryViewer.tsx` + `AgentLogTranscript.tsx` + `taskTrajectory.ts`
5. **agy B** — `TuiAgentsPage.tsx` + `OpenDuckConfigPanel.tsx`
6. **grok C** — ProjectChat, ProjectChatTasksDrawer, ProjectFiles, ProjectGit
7. **agy C** — ProjectRules, ProjectSettings, ProjectTerminal, ProviderStatsDashboard,
   RegisterProjectModal, ServerSettings, TaskDraftPanel, DynamicPromptSelectionMenu,
   MentionPopover, configEditor.ts
8. **codex** — final review: catalog parity, no hardcoded user-visible copy, English non-regression.
9. Orchestrator: typecheck + test + build + CJK-outside-locales grep.

### Invariants
- 50/50 grok/agy split (3 chunks each).
- Catalogs edited SEQUENTIALLY (agents run one at a time) — never two agents on en/zh.json at once.
- No Chinese literal in any component; only `zh.json` may contain CJK.
- `taskPdfExport.ts` pre-existing Chinese stays (its own bilingual PDF template) unless trivially alignable.
- Keep `itemType`/status/enum/URL tokens untranslated. Keep gitDiff grammar English.

## PROGRESS LOG (subtask 4)

### Done
- Baseline verified: typecheck PASS, `pnpm test` 308 pass/0 fail, `pnpm build` PASS (4.7s).
- `docs/i18n-extraction-guide.md` (249 lines, codex) — authoritative conventions:
  exclusive key prefixes per file, byte-exact English, plural rules, formatting layer spec.
- `.agent/i18n-agent-rules.md` — shared rules pasted into every coding-agent prompt.
- `ui/hub/src/i18n/format.ts` + `src/format.test.ts` (agy) — formatDate/DateTime/Time,
  formatNumber/Percent, formatRelativeTime, formatDurationMs, formatExportDuration.
  VERIFIED by me: typecheck PASS, tests 317 pass / 0 fail.

### Key technical findings
- Pure `.ts` can import `src/i18n/index.ts` under `node --experimental-strip-types`; `translate()`
  works uninitialized via `resolveFallback` over en.json -> English test assertions survive.
- `.tsx` cannot be imported by tests (no jsdom) => component work is not covered by tests.
- i18next `t()` is untyped here, so new keys don't break typecheck.
- BUT helper `.ts` translate() returns the key when missing from en.json -> would break the 13
  copy-coupled test suites. Hence fragments get merged into en/zh continuously.

### Remaining
1. Fragment merge script with flock (`.agent/merge-i18n.mjs`).
2. 6 parallel extraction agents (3 grok / 3 agy), disjoint file sets + owned prefixes.
3. Refactor format.ts inline Chinese ternaries -> `format.*` catalog keys.
4. Final: merge, parity check, typecheck + test + build, hardcoded-CJK scan.

### AGENT AVAILABILITY (verified 2026-11-04, not assumed)
| agent | status | evidence |
| --- | --- | --- |
| grok | **UNAVAILABLE** | TUI shows "Weekly limit left: 0%"; prompt -> "You hit your weekly limit." |
| claude | **UNAVAILABLE** | "Not logged in · Please run /login" |
| codex | available | used successfully for design doc + implementation |
| agy | available | implemented format.ts + tests successfully |
=> 50/50 grok/agy split impossible. Fallback: agy + codex for coding, sequenced in batches.

### W1 (ProjectHarnessTab.tsx) — DONE & VERIFIED BY ME
- codex completed it before the tmux server died mid-run.
- 290 keys in `.agent/i18n-fragments/W1.{en,zh}.json`, EN/ZH flat key parity exact,
  0 placeholder mismatches, typecheck PASS, `grep -cP '[\x{4e00}-\x{9fff}]'` = 0,
  248 `t('harness.page...` call sites, diff +385/-394.
- NOTE: the tmux server on /tmp/tmux-1002/openduck DIED once (all sessions lost).
  Always verify on-disk results after a crash instead of assuming failure.

### W2 (TaskTrajectoryViewer.tsx) — DONE & VERIFIED BY ME
233 keys, parity OK, typecheck PASS, CJK grep 0, tests 317/0. Codex additionally proved
28 English renders byte-identical to the original via an in-memory tsx render harness.

### Merge tooling — DONE & VERIFIED
- `ui/hub/scripts/i18n-merge.mjs` + `i18n-check.mjs`; package.json gained
  `i18n:merge` / `i18n:check` (and the i18next/react-i18next deps, from subtask 2).
- Verified: real fragments merge, concurrent merges are safe (2nd reports 0 changes),
  negative parity test exits 1 WITHOUT writing, protected foundation keys preserved,
  catalogs restored byte-identically (git diff empty).

### AGENT WORKFLOW NOTES (important, learned the hard way)
- `tui_agent_start` almost always reports "session not found" even though it DOES create the
  session. Always follow with `tui_agent_list`, then `tui_agent_prompt` on the real id.
- Sessions get `reclaimed`/`exited` between turns. Restart fresh rather than assuming death.
- If a run seems lost, VERIFY ON DISK FIRST (fragments + git diff) — codex finished W1 fully
  even though the tmux server appeared to crash.
- Pass long briefs by file path only (inline payloads hit output-token limits).

### W3 (TuiAgentsPage.tsx) — DONE & VERIFIED: 196 keys, parity OK, typecheck PASS, CJK 0
### W4 (OpenDuckConfigPanel+configEditor+providerConfig) — DONE: 198 keys, typecheck PASS, CJK 0

### CRITICAL LESSON: helper `.ts` translate() needs MERGED catalogs to pass tests
After W4, `pnpm test` dropped to 298 pass / 19 fail because configEditor.ts/providerConfig.ts
now call `translate()` and the keys were only in fragments, not `en.json`.
`translate()` returns the KEY when missing -> English assertions failed.
FIX: run `cd ui/hub && pnpm run i18n:merge` (936 keys merged). => back to 316/1.
=> RUN i18n:merge AFTER EVERY WAVE, before judging test results.

### The 1 remaining failure is a REAL defect (parity test is a good gate)
`i18n.test.ts` asserts catalog values are trimmed + non-empty + placeholder parity.
Rejected 4 sites where whitespace was baked into catalog values:
  W1 `harness.page.attachments.add` = " Attach files"  (JSX `{icon} Attach files` -> double space)
  W2 `trajectory.viewer.filteredFrom` = " (filtered from {{count}})"
  W2 `trajectory.viewer.tokenSpeedSuffix` = " ({{speed}} tok/s)"
  W2 `trajectory.viewer.scrubberTools/Telemetry` = "\nTools: ..." (leading newline)
  W3 `tuiAgents.page.transferAssignPrefix/Suffix` = split sentence around <strong> (guide forbids)
RULE for remaining workers: catalog values must have NO leading/trailing whitespace or newlines;
keep that whitespace in JSX/code (`{' '}`, template literals). Split sentences -> use <Trans>.

### W5 (AgentLogTranscript + taskTrajectory + ProjectChatTasksDrawer) — DONE & VERIFIED
197 keys, parity OK, no untrimmed values.

### Merge prune — DONE & VERIFIED BY ME
`i18n-merge.mjs` now validates fragments against `.agent/i18n-fragments/owners.json`
(owner -> exclusive dot-prefixes) and PRUNES any catalog key under an owned prefix that no
longer exists in that owner's fragments. => merge is now IDEMPOTENT/CONVERGENT.
Verified: two consecutive merges both report `changed=0`; stale
`tuiAgents.page.transferAssignPrefix/Suffix` are gone; en=1132/zh=1132 keys.

### FULL SUITE GREEN: typecheck PASS, `pnpm test` 317 pass / 0 fail.

### Remaining: W6, W7, W8, W9, W10 (5 batches, ~875 strings)
Strategy: run 2-3 agents in parallel; after each wave ALWAYS run
`cd ui/hub && pnpm run i18n:merge` before judging `pnpm test`.

### W6 — DONE & VERIFIED: 145 keys; total 1277; typecheck PASS; 317 tests; CJK 0 on all 5 files.
### W7 — DONE & VERIFIED: total 1443 keys; typecheck PASS; 317 tests pass.
- `taskPdfExport.ts` hardcoded `<html lang="zh-CN">` -> `<html lang="${locale}">` (dynamic). 
- Pre-existing hardcoded Chinese PDF copy MOVED into zh catalog; CJK count in .ts is now 0.
### Remaining: W8, W9, W10 (~515 strings)

### W8 — DONE & VERIFIED: total 1654 keys; typecheck PASS; 317 tests; CJK 0 on all 6 files.
### W9 — DONE & VERIFIED: total 1814 keys; typecheck PASS; 317 tests; CJK 0 on all 8 files.
- `Header.tsx` has only 11 `t(` — header.* foundation keys preserved (verified not overwritten).
- `runningTasks.ts` uses non-hook `translate()` (12 calls); `App.tsx` error boundary too (2).
### W10 — RUNNING (last batch, 11 files). After W10: final global audit for leftover hardcoded copy.

### Batch status (running total of catalog keys)
W1..W9 DONE: 1814 keys, typecheck PASS, 317 tests, CJK 0.
W10: IN PROGRESS on agy session `agy_1793863298242`. Fragments W10.en/zh.json written (75 keys:
  mentions.popover.*, ...). NOTE: the batch doc's W10 filenames were WRONG — real files are
  `MentionPopover.tsx` (not MentionsPopover/MentionsHelper), `ProjectDetail.tsx`,
  `DynamicPromptSelectionMenu.tsx`, `ProjectRootsEditor.tsx`, `Skeletons.tsx`; there is NO
  TaskCreateModal.tsx / TaskDraftHelper.tsx / ProjectTabs.tsx / ProjectDetailHeader.tsx.

### GROK IS OUT OF QUOTA (weekly limit 0%) — it hangs on the splash. Use codex/agy only.

### Global audit (script /tmp/audit.mjs) found GAPS no batch covered -> brief `.agent/brief-gap-fix.md` (W11):
1. **`src/i18n/format.ts` HARDCODES CHINESE** — `isZh ? '<1分钟后' : ...` ternaries in
   `formatRelativeTime` + duration formatter. Violates "no hardcoded Chinese" AND bypasses
   catalogs. Must become `format.relative.*` / `format.duration.*` keys via `translate()`.
2. `ProviderStatsDashboard.tsx` — 21 strings ("Total Invocations", "Success Rate", ...) never assigned to any batch.
3. `DynamicPromptSelectionMenu.tsx` — 9 strings. 4. `ProjectRootsEditor.tsx` — 3.
5. `OpenDuckConfigPanel.tsx` — `placeholder="OLLAMA_HOST"`. 6. `ProjectDetail.tsx` — "PROJECT WORKSPACE".
### NEXT: run W11 (disjoint files only), then final full-repo audit + build.

## SUBTASK 4/5 COMPLETE — final verified state (all numbers measured, not assumed)
| check | result |
|---|---|
| `pnpm run i18n:merge` (x2) | `en=1974 zh=1974 changed=0` — idempotent ✓ |
| `pnpm run i18n:check` | `en=1974 keys, zh=1974 keys; parity OK` ✓ |
| `pnpm run typecheck` | PASS ✓ |
| `pnpm test` | **317 pass / 0 fail** ✓ (unchanged from baseline — no English regression) |
| `pnpm run build` | `✓ built in 4.79s` ✓ |
| files changed | 54 |

### Batches
W1..W12 all landed. grok ran out of weekly quota mid-run (hangs on splash) — final batches used codex/agy.
W10's batch-doc filenames were WRONG (MentionPopover/ProjectDetail/DynamicPromptSelectionMenu/
ProjectRootsEditor/Skeletons are the real files). W11/W12 closed gaps found by my own audit.

### Two REAL defects I found by audit that no batch covered
1. **`src/i18n/format.ts` hardcoded Chinese** — `isZh ? '<1分钟后' : ...` ternaries in
   `formatRelativeTime` + duration formatter. Fixed -> `format.relative.*` / `format.duration.*`
   keys via `translate()`. This was the exact "no hardcoded Chinese" violation.
2. **`components/useProjectRoots.ts`** errors (`'Unable to load project roots'` etc.) render at
   `ProjectRootsEditor.tsx:102` in `<p className="error">` -> now `projectRoots.errors.*`.

### Runtime smoke test (proves switching, not just static keys)
`translate('settings.server.title')`: en `Server & Settings` / zh `服务器与设置`;
`format.relative.minutesAgo {count:5}`: en `5m ago` / zh `5分钟前`; switches back to en correctly.

### Deliberately NOT translated (verified correct)
- `gitDiff.ts` — git output parsing (`'new file'`, `'rename from'`).
- `taskAttachments.ts` `ATTACHED_FILES_PROMPT_HEADER` — **prompt text sent to the model**.
- `contexts/*Context.tsx` — dev-only "must be used within a Provider" guard throws.
- `taskPdfExport.ts` `'ET\nBT'` — print/CSS syntax.
- Placeholders/URLs/agent names (`http://127.0.0.1:7890`, `OLLAMA_HOST`, `codex (OpenAI Codex)`, `+30m`, `PID {{pid}}`).
- CJK remaining in `*.test.ts` is INTENTIONAL — those tests assert the zh path works.

### English-regression proof (scripted, 501 literals audited)
Extracted every English string literal removed by the diffs and checked it exists verbatim in
`src/locales/en.json`: **497/501 matched exactly**. The 4 exceptions verified individually:
- `dynamicPrompt.ts` `Selected excerpt from \`{{path}}\`:` -> proven byte-identical at runtime
  (`'Selected excerpt from `+'`a/b.ts`+"`:'"` equality true). Regex artifact from escaped backticks.
- `taskTrajectory.ts` `'Failure Reason' / 'Task Failed' / 'Task Incomplete'` -> used only in
  `.includes()` guards that detect HISTORICAL English headings already persisted in stored task
  data. Must stay literal; localizing them would break the dedupe check.

=> English copy preserved; no regression.

### FINAL: i18n:check parity OK (1974/1974) · typecheck PASS · 317 tests pass / 0 fail · build OK

---

## SUBTASK 5/5 — verify (构建与类型检查验证) — IN PROGRESS

### Harness expectation (only ONE, cwd = workspace ROOT /mnt/e/openduck-harness)
- `kind: command`, `command: npm run build`, expectedExitCode 0. Root `package.json` shim delegates to `cd ui/hub && npm run build`. **PASSES.**

### VERIFIED NOW (measured on disk, this run)
| check | cmd (root) | result |
|---|---|---|
| build | `bash -c 'npm run build; echo $?'` | **EXIT 0**, `✓ built in 4.95s`, 2250 modules |
| typecheck (`tsc -b` inside build) | `cd ui/hub && npx tsc --noEmit` | **EXIT 0**, no output |
| tests | `cd ui/hub && npm test` | **317 pass / 0 fail** (baseline was 317) |
| parity | `npm run i18n:check` | `en=1974 keys, zh=1974 keys; parity OK` EXIT 0 |

### GOTCHA: `shell` tool runs under `sh`, NOT bash
`echo "EXIT=${PIPESTATUS[0]}"` -> `sh: 1: Bad substitution`, which made a PASSING build look like exit 2.
**Always wrap in `bash -c '...'` when capturing exit codes.**

### Key-reference audit (script /tmp/audit2.mjs) — 0 missing keys
- 69 source `.ts/.tsx` files scanned; 1826 distinct statically-referenced keys.
- First pass reported 2 "missing": `files.preview.lines`, `trajectory.log.thought` — **false positives**,
  they are plural bases; `_one`/`_other` exist in BOTH catalogs (en "{{count}} line(s)", zh "{{count}} 行").
- With plural-suffix resolution: **missing = 0**.
- 146 keys not matched by direct `t()` scrape are reached via indirection
  (`pageMessage('harness.page.errors.configLoad')`, `setNotice({key:'config.panel.harnessSaved'})`).
  Literal-substring search shows **1948/1974** referenced; the other 26 are plural variants
  and `common.*` (referenced as `t.common.cancel` in i18n.test.ts).

### Hardcoded-Chinese scan — CLEAN
`grep -rlP '[\x{4e00}-\x{9fff}]' src --include=*.ts --include=*.tsx` outside `src/locales/`
=> ONLY 4 `*.test.ts` files (format/taskPdfExport/taskTrajectory/projectIdentity).
**Zero CJK in any shipped component/helper.** ✓

### English-regression audit — script bug found, being fixed
`/tmp/en-regression.mjs` used `git show HEAD:ui/hub/${f}` but `git diff --name-only` already
returns repo-relative paths (`ui/hub/src/...`) -> produced `ui/hub/ui/hub/src/...` => 0 files compared.
**Fix: `git show HEAD:${f}`.**

### SUBTASK 5 FIXED-TIME AUDIT RESULTS (all measured this run)
| audit | method | result |
|---|---|---|
| build (root) | `bash -c 'npm run build; echo $?'` | **exit 0**, 4.95s |
| typecheck | `npx tsc --noEmit` | **exit 0** |
| tests | `npm test` | **317 pass / 0 fail** |
| parity | `npm run i18n:check` | en=1974 zh=1974 **parity OK** |
| missing keys | /tmp/audit2.mjs (plural-aware) | **0 missing** of 1826 referenced |
| hardcoded CJK | grep -P outside locales/tests | **0 files** (only 4 *.test.ts) |
| hardcoded EN prose | /tmp/jsx-prose.mjs (JSX text nodes) | **0 prose nodes** |
| English regression | /tmp/en-reg2.mjs, old-HEAD literals -> en.json | 558 literals extracted, **0 lost** (2 artifacts: JSX `&#10;` entity + plural base) |
| Trans tag match | /tmp/trans-audit.mjs | 6 hits, **all regex artifacts** (`<strong />` truncates non-greedy match) — verified each site manually, all correct |
| placeholder args | /tmp/placeholder-audit.mjs | 16 hits, **all ES shorthand** `{ file }` `{ time }` — false positives |
| Intl locale | src/i18n/format.ts:14 | `locale==='zh-CN'?'zh-CN':'en-US'` — **correctly active-locale driven**, no hardcode |
| roast: no module-level t() | grep `^const .* = t(` | **none** ✓ |

### ⚠️ REAL DEFECT FOUND: pre-translated strings in useState (stale on locale switch)
Pattern `setXError(t('...'))` stores an ALREADY-TRANSLATED string in React state. Switching
language afterwards leaves that message in the old language => violates "切换后立即生效".
Sites: RegisterProjectModal.tsx:133,174; TuiAgentsPage.tsx:220,225; ProjectRootsEditor.tsx:40.
CORRECT pattern already used elsewhere: store `{key:'...', values:{...}}` and translate at render
(ProjectChatTasksDrawer 9x, TuiAgentsPage 6x, OpenDuckConfigPanel 22x, ProjectSettings 3x).

### AGENT DELEGATION BLOCKED (this run)
- `tui_agent_start`/`tui_agent` ALWAYS fail: `TUI session '<id>' not found`. Root cause: tool creates
  tmux sessions on socket `-L openduck` (/tmp/tmux-1002/openduck) but then looks for them on the
  DEFAULT socket. `tmux -L openduck ls` DID show the sessions. Cleaned up 4 orphans.
- `codex exec` works but spams MCP transport errors (chatgpt.com MCP unreachable).
- `agy --print` OK but must be run DETACHED (`nohup ... &`) — foreground call hit the 800s tool cap.
  Also: `--print-timeout` needs a unit suffix (`10m`, not `600`).

### FULL stale-translation inventory (script /tmp/stale-scan.mjs — setter calls embedding t())
22 sites total across 7 files:
| file | lines | pattern |
|---|---|---|
| DynamicPromptSelectionMenu.tsx | 66,72,77,190 | `setNotice({tone, message: t(...)})` |
| FilePreviewModal.tsx | 108 | `setError(... : t('files.preview.errorRead'))` |
| ProjectFiles.tsx | 167,231,340,358,376,398,417 | `setError(... : t('files.page.error*'))` |
| ProjectRootsEditor.tsx | 40 | `setLocalError(t(...))` |
| RegisterProjectModal.tsx | 97,133,155,174,188 | `setDirError/setFormError(t(...))` |
| TuiAgentsPage.tsx | 96,220,223,225 | `setError/setInspectScreen(t(...))` |

SEVERITY: LOW. All are transient error/notice/loading text, only stale if the user switches
language *while the message is on screen*. Render-time copy (nav/buttons/labels/empty states) is
correct and re-renders. No hardcoded Chinese involved, so the literal task requirement is met.

### Related but NOT defects (verified)
- Raw KEY stored in state + translated at render — intentional & switch-safe:
  `App.tsx:364,578`, `MemoryMonitorPanel.tsx:107`, `ProjectGit.tsx` (`{kind:'fallback',key}`),
  `ProjectRules.tsx` (`errorMessage(cause,'key')`), `TuiAgentsPage.tsx:96`.
- `translate()` called during render (`App.tsx:364,578`) is safe: App subscribes via `useTranslation()` (App.tsx:46).
- `main.tsx` ErrorBoundary `translate()` — class component, no subscription, but it only renders on a
  crash; acceptable.
- `LocaleContext.tsx` derives locale from `useTranslation()` -> switching DOES re-render all consumers.

### zh catalog untranslated check (/tmp/zh-audit.mjs)
21 zh values contain no CJK and >=2 words — ALL legitimate: brand/product ("OpenDuck Hub",
"Vite + React 19", agent CLI names), code/example values ("cargo test", "src/a.rs, src/b.rs",
"rust, style", "**/*.rs, **/*.toml", emails), units ("TTFB {{ms}}ms", "RSS {{value}} MiB",
"ICMP ping"), placeholders ("{{status}} · {{relative}} · {{steps}}", "(ID: {{id}}...)").
=> **No untranslated prose.** ✓

### SUBTASK-5 DEFECT LIST (codex audit + my own verification — all confirmed)
Fix pattern to use: store a translation DESCRIPTOR (key + values) in state, translate at render.
Repo already has this pattern: ProjectGit.tsx:166 `type GitError={kind:'raw';message}|{kind:'fallback';key}`
and ProjectRules.tsx:46 `errorMessage(cause,key)`.

| # | file:line | defect |
|---|---|---|
| A | RegisterProjectModal.tsx:97,133,155,174,188 | `setDirError/setFormError(t('...'))` — 5 sites |
| B | ProjectRootsEditor.tsx:40 | `setLocalError(t('...'))` |
| C | ProjectFiles.tsx (7x) | `setError(...: t('files.page.error*'))` |
| D | FilePreviewModal.tsx:108 | `setError(...: t('files.preview.errorRead'))` |
| E | DynamicPromptSelectionMenu.tsx:66,72,77,190 | `setNotice({message:t()})`, `setPending({error:t()})` |
| F | TuiAgentsPage.tsx:96,220,223,225 | `setError/setInspectScreen(t(...))` |
| G | useProjectRoots.ts:29-36 | `describeRootError()` returns translated string stored in state |
| H | OpenDuckConfigPanel.tsx:597,628,633,672 | `validateProxyId/Url` return translated text; `lastRefreshError` cached |
| I | ProjectHarnessTab.tsx:1598 (+ providerConfig.ts/configEditor.ts msg via messageFrom) | `setError(subtaskSaveError(...))` pre-translated |
| J | ProjectChat.tsx:161 + ProjectHarnessTab.tsx:1110,1133,1141 | mention/agent/attachment descriptions pretranslated into stored `extra` strings, useMemo w/o locale dep |
| K | zh.json tuiAgents.page.formAgentCli / thAgent | "Agent CLI" / "Agent" untranslated; zh uses 智能体 elsewhere (40x) |

### FIX STRATEGY (minimal, no behavior change to English)
- Where the value is ONLY ever a translated string, store `{key, values}` and translate at the render site.
- Keep `cause.message` (server text) raw as today — it is untranslatable either way.
- For G/H/I: helpers should return a KEY (not translated text); translate at render.
- For J: make the stored descriptors or re-derive in a `useMemo` that depends on `i18n.language`.
- For K: merely edit 2 zh values.
MUST keep: npm run build / typecheck exit 0, tests 317 pass, i18n:check parity OK, en.json byte-identical.

### FIX IMPLEMENTED (2 parallel codex sessions, disjoint file sets)
Group 1: RegisterProjectModal, ProjectFiles, FilePreviewModal, DynamicPromptSelectionMenu, TuiAgentsPage
Group 2: useProjectRoots+ProjectRootsEditor, OpenDuckConfigPanel+providerConfig+configEditor,
         ProjectHarnessTab+taskDraft, ProjectChat, zh.json (2 values)
Both reported: typecheck 0, tests 317 pass/0 fail, i18n:check parity OK, build 0.

### ⚠️ NETWORK OUTAGE mid-run (2026-11-05)
`curl https://chatgpt.com` and `https://www.google.com` => "Connection reset by peer", http 000.
Effect: grok (g2) and agy (g1) both stuck on "Connection failed | Retrying (attempt 9)". Killed both.
codex DID work. NOT a 50/50 split — technical necessity, documented.
Also: `tmux send-keys -t X -l "...output." Enter` puts the literal word "Enter" in the buffer.
=> ALWAYS send text and Enter as SEPARATE send-keys calls (add `sleep 1` between).

### INDEPENDENT VERIFICATION OF THE FIX (all re-run by me, after the agents finished)
| check | result |
|---|---|
| `npm run build` (ROOT, harness cwd) | **EXIT 0**, `✓ built in 4.79s` |
| `npx tsc --noEmit` | **EXIT 0** |
| `npm test` | **317 pass / 0 fail** |
| `npm run i18n:check` | `en=1974 zh=1974 parity OK` |
| eager-translation sites (/tmp/stale-scan.mjs) | **0** (was 22) |
| missing keys (/tmp/audit2.mjs) | **0** of 1768 referenced |
| hardcoded CJK outside locales | **0** (only 4 *.test.ts) |
| JSX prose nodes | **0** |
| English regression (/tmp/en-reg2.mjs) | 558 extracted literals, **0 lost** |
| `en.json` untouched | **byte-identical** (mtime 03:39, pre-fix; zh.json 04:27) |
| components using translated helpers but NOT locale-subscribed | **0** (/tmp/subscribe-audit.mjs) |
| runtime round-trip en->zh->en | **PASS** — header.projects Projects/项目/Projects; brand stays "OpenDuck Hub" |

### Things I checked and deliberately left alone (not defects)
- `dynamicPrompt.ts:26`, `providerConfig.ts:*`, `taskTrajectory.ts:114-178` return `translate()`
  text, but every consuming component subscribes (`useTranslation`) or passes `i18n.language`
  into the memo dep (TaskTrajectoryViewer.tsx:223 `[run, i18n.language]`,
  ProjectHarnessTab.tsx:550/1112/1139/1150).
- `configEditor.ts ConfigValidationError` keeps a `get message()` that translates — that's a
  compatibility shim; consumers use `{key, values}` descriptors via `messageFrom()`.

## SUBTASK 5/5 — COMPLETE ✅
Harness check `npm run build` at workspace root returns 0.
