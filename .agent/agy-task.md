# TypeScript task — fix Git commit timeout budgets + precedence (subtask 2 of 4)

Read these first for context:
- `.agent/plan-subtask2-timeout.md` (sections 2, 3, 5, 8 are yours)
- `.agent/notes-plan-task-fix-git-tab-commit-timeout-lock.md`

Do NOT touch any Rust files (another agent owns `crates/openduck`).
FILES YOU OWN (only these three):
- `ui/hub-core/src/rest.ts`
- `ui/hub-core/src/rest.test.ts`
- `ui/hub/src/components/ProjectGit.tsx`

## Context
The UI Hub "Git" tab commit was aborted at 12s by the REST client, while the server runs
several sequential git subprocesses (core subtotal 240s). Two extra holes: generic timeout
overrides could still shrink git routes back to 12s; and a post-commit metadata read failure
turned a landed commit into an error that the UI rendered as "Committed  — ".

## Change 1 — `ui/hub-core/src/rest.ts`: budgets
- `DEFAULT_REQUEST_TIMEOUT_MS` stays `12_000`.
- `GIT_COMMIT_MESSAGE_TIMEOUT_MS` (90_000), `GIT_MUTATE_TIMEOUT_MS` (120_000),
  `GIT_READ_TIMEOUT_MS` (30_000) stay unchanged.
- `GIT_COMMIT_TIMEOUT_MS`: `240_000` -> `300_000`.
- Replace the git-budget comment with an accurate one: the core commit subprocesses total
  240s (ensure-repo 30 + add 60 + commit 120 + post-commit log 30); 300s covers that plus
  response overhead; it is not a bound on git-side lock waiting.

## Change 2 — `ui/hub-core/src/rest.ts`: close the override-precedence hole
Today `requestTimeoutMs(path, method)` consults `options.timeoutMs`, localStorage
`openduck.api.timeoutMs` and `VITE_API_TIMEOUT_MS` in ways that can shrink git routes back
to 12s, and the broad substring exemptions (`/run`, `/eval`, ...) can misfire on a project slug.

Required rule: classify PROJECT GIT ROUTES FIRST, by pathname (strip the query string), and
return the dedicated git tier. For git routes the tiers are authoritative — they win over
`options.timeoutMs`, localStorage and env, INCLUDING values of `0` and values larger than the
tier. Everything else keeps its existing ordering and existing zero-disables-timeout behavior.

Shape inside `createApi`:

```ts
function gitRouteTimeoutMs(route: string): number {
  if (route.endsWith('/git/commit')) return GIT_COMMIT_TIMEOUT_MS
  if (route.endsWith('/git/commit-message')) return GIT_COMMIT_MESSAGE_TIMEOUT_MS
  if (['/git/stage', '/git/unstage', '/git/discard'].some(r => route.endsWith(r))) {
    return GIT_MUTATE_TIMEOUT_MS
  }
  return GIT_READ_TIMEOUT_MS
}
```

and at the top of `requestTimeoutMs`:

```ts
const route = path.toLowerCase()
const pathname = route.split('?')[0]
if (/^\/api\/v1\/projects\/[^/]+\/git\//.test(pathname)) return gitRouteTimeoutMs(pathname)
```

then the existing network-status / tui-session / long-running-route / history-report branches
unchanged, then the existing generic override handling unchanged. A git route must never fall
through to the raw 12s default and never return `null`.

## Change 3 — `ui/hub/src/components/ProjectGit.tsx`: commit notice
`commitChanges` must never render "Committed  — ". A landed commit with unavailable metadata
now returns `sha: ""`. Requirements:
- trim both `sha` and `subject`;
- sha + subject -> `Committed <sha first 7> — <subject>`;
- sha only -> `Committed <sha first 7>`;
- subject only -> `Committed — <subject>`;
- neither -> `Committed successfully`.
Keep clearing message, message source, selected sha and selected paths, and keep the existing
`runMutation` refresh behavior. This is a SUCCESS notice — never an error, never a retry prompt.

## Change 4 — `ui/hub-core/src/rest.test.ts`: real assertions
- Import `DEFAULT_REQUEST_TIMEOUT_MS` along with the git constants.
- Replace the weak `>= 210_000` style assertions with exact values:
  `GIT_COMMIT_TIMEOUT_MS === 300_000`, `> (30 + 60 + 120 + 30) * 1000`,
  `GIT_COMMIT_MESSAGE_TIMEOUT_MS === 90_000`, `GIT_MUTATE_TIMEOUT_MS === 120_000`,
  `GIT_READ_TIMEOUT_MS === 30_000`, `DEFAULT_REQUEST_TIMEOUT_MS === 12_000`.
  Rename the test so it no longer claims every tier exceeds its composed route.
- Replace the existing test named `timeout options still override git route timeouts` — it
  encodes the bug. Add timer-instrumented tests that prove the ACTUAL scheduled delay using
  node:test mock APIs (`t.mock.method(globalThis, 'setTimeout', ...)` capturing the delay and a
  mocked `fetch` returning an immediate JSON `Response`, so timers clear and nothing waits minutes).
  Cover:
  * git tiers: commit 300_000, commit-message 90_000, stage/unstage/discard 120_000,
    status/log 30_000 — use `getGitLog` with query params to prove pathname classification;
  * a project slug containing "run" (e.g. `commitGitChanges('runner', ...)`) still gets 300_000,
    proving git classification precedes the `/run` exemption;
  * a precedence matrix over git commit + git status + an ordinary route (e.g. `listProjects()`):
    no overrides; `options.timeoutMs` of 12_000 / 0 / 600_000; localStorage
    `openduck.api.timeoutMs` of '12000' / '0' / '600000'; `process.env.VITE_API_TIMEOUT_MS` of
    '12000' / '0' / '600000'. Git routes always get their tier; the ordinary route follows the
    existing override rules (0 => no timer).
  Restore all globals/env/localStorage in `finally` and run global-mutating cases serially.
  For localStorage go through the same `window.localStorage.getItem` interface production reads —
  save the original property descriptor and restore or delete it.

## Style
Match surrounding code (the file uses no semicolons — follow it). No comments that restate code.
Keep the diff minimal and scoped.

## Verify and report
```
cd ui/hub-core && pnpm test
cd ui/hub-core && pnpm run typecheck   # or the package's tsc script if that name differs
cd ui/hub && pnpm run typecheck        # if it exists
```
Do NOT commit. Report the files changed, key decisions, and test output.
