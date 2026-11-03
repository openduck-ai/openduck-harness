# Subtask 2: Git commit timeout implementation plan

1. **Keep scope limited to timeout budgets, override precedence, and post-commit success reporting.**

   Reviewed the current working-tree implementations of `crates/openduck/src/control/git.rs`, `crates/openduck/src/control/http.rs`, `ui/hub-core/src/rest.ts`, `ui/hub-core/src/rest.test.ts`, and `ui/hub/src/components/ProjectGit.tsx`.

   Implement only the changes below. Preserve existing uncommitted changes. Do not change index-lock detection, removal, recovery, repository serialization, subprocess cleanup, or process-liveness checks; those belong to subtask 3. This is a local design plan, not an upstream PR implementation requiring a Ready issue.

2. **Fix the commit budget invariant and the misleading comments.**

   In `crates/openduck/src/control/git.rs`, retain these server constants unchanged:

   ```rust
   const ADD_TIMEOUT: Duration = Duration::from_secs(60);
   const COMMIT_TIMEOUT: Duration = Duration::from_secs(120);
   const MUTATE_TIMEOUT: Duration = Duration::from_secs(90);
   const READ_TIMEOUT: Duration = Duration::from_secs(30);
   const MESSAGE_TIMEOUT: Duration = Duration::from_secs(90);
   ```

   Replace the comment immediately above them with:

   ```rust
   // Core commit subprocess budgets total 240s: ensure-repo(30s) + add(60s) +
   // commit(120s) + post-commit log(30s). The Hub commit request allows 300s
   // (ui/hub-core/src/rest.ts), leaving 60s beyond that subtotal. These budgets
   // do not bound repository-lock waiting or separate lock-maintenance work.
   ```

   In `ui/hub-core/src/rest.ts`, change only `GIT_COMMIT_TIMEOUT_MS` from `240_000` to `300_000`. Keep `DEFAULT_REQUEST_TIMEOUT_MS`, `GIT_COMMIT_MESSAGE_TIMEOUT_MS`, `GIT_MUTATE_TIMEOUT_MS`, and `GIT_READ_TIMEOUT_MS` unchanged for this focused fix. Replace the git-budget comment with:

   ```ts
   // Git routes use dedicated request budgets. The core commit subprocesses
   // total 240s (ensure-repo 30 + add 60 + commit 120 + log 30); allow 300s
   // for that subtotal plus response overhead. This is not a bound on lock waiting.
   export const GIT_COMMIT_TIMEOUT_MS = 300_000
   ```

   The required invariant is `300_000 > (30 + 60 + 120 + 30) * 1_000`, with 60 seconds of headroom. Increasing the client alone preserves server phase limits and avoids the existing zero-margin race. Do not shorten server subprocess budgets merely to fit the old client deadline.

   Precision about the current tree: 240 seconds is the requested **core subprocess subtotal**, not a hard whole-request maximum. `with_repo_write_lock` can wait, and existing lock-maintenance helpers invoke additional reads. A finite client timeout cannot guarantee it always outlasts those paths. Record this qualification; do not redesign those helpers in this subtask. Likewise, do not describe unchanged read/message tiers as strictly exceeding all composed server routes.

3. **Close H6 with one explicit ordering rule in `rest.ts`.**

   Dedicated project git-route budgets win over every generic timeout override. Ignore `options.timeoutMs`, `openduck.api.timeoutMs`, and `VITE_API_TIMEOUT_MS` for these routes, including values of `0` and values larger than the tier. This deliberately chooses a fixed budget over floor semantics; there is only one git policy to test.

   Match project git routes using the pathname, before generic substring exemptions. That also prevents a project slug such as `runner` from accidentally matching `/run` and disabling the git timeout. Strip the query string for classification. Leave the original non-git routing and override code in its existing order.

   Code sketch inside `createApi`:

   ```ts
   function gitRouteTimeoutMs(route: string): number {
     if (route.endsWith('/git/commit')) return GIT_COMMIT_TIMEOUT_MS
     if (route.endsWith('/git/commit-message')) return GIT_COMMIT_MESSAGE_TIMEOUT_MS
     if (['/git/stage', '/git/unstage', '/git/discard'].some(r => route.endsWith(r))) {
       return GIT_MUTATE_TIMEOUT_MS
     }
     return GIT_READ_TIMEOUT_MS
   }

   function requestTimeoutMs(path: string, method = 'GET'): number | null {
     const route = path.toLowerCase()
     const pathname = route.split('?')[0]
     if (/^\/api\/v1\/projects\/[^/]+\/git\//.test(pathname)) {
       return gitRouteTimeoutMs(pathname)
     }

     // Keep the existing network-status, TUI, long-running-route, and
     // history/report branches here, unchanged and in their existing order.

     if (options.timeoutMs !== undefined) {
       return options.timeoutMs === 0 ? null : options.timeoutMs
     }
     const defaultMs = resolveConfiguredTimeout(
       'openduck.api.timeoutMs',
       'VITE_API_TIMEOUT_MS',
       DEFAULT_REQUEST_TIMEOUT_MS,
     )
     return defaultMs === 0 ? null : defaultMs
   }
   ```

   The placeholder comment above describes retained code, not a proposed new source comment. Remove the old `const fallbackMs = gitRouteTimeoutMs(route)` at the generic branch. The existing `resolveConfiguredTimeout` and `fetchWithTimeout` need no behavior changes. Non-git behavior remains: special-route policies first; otherwise caller option, then localStorage, then environment, then the 12-second default. `0` continues to disable ordinary request timeouts.

4. **Close H9 in `git.rs::commit_locked`: successful commit remains successful when metadata fails.**

   Keep `ensure_repo`, validation, staging, and `git commit` failures fatal. Only after `output.success` is confirmed, treat the final `git log` as best-effort. Keep its existing `READ_TIMEOUT`; do not add another subprocess or retry.

   On a log spawn error, timeout, nonzero exit, empty output, or malformed metadata, return:

   ```rust
   GitCommitResponse {
       sha: String::new(),
       subject: message.lines().next().unwrap_or_default().to_string(),
   }
   ```

   Here `message` is the already validated, trimmed commit message. For `"  feat: save work\n\nDetails  "`, the exact fallback JSON is `{"sha":"","subject":"feat: save work"}`. Empty SHA means the commit succeeded but its revision metadata is unavailable. Preserve the existing response schema and string field types. Do not infer a SHA from human-readable `git commit` output.

   Extract a small private conversion helper so error and malformed-output behavior can be tested without altering PATH, waiting 30 seconds, or adding a general command-injection framework:

   ```rust
   fn commit_response_from_log(line: Result<String>, message: &str) -> GitCommitResponse {
       if let Ok(line) = line {
           if let Some((sha, subject)) = line.trim().split_once('\u{1f}') {
               if !sha.is_empty() && !subject.is_empty() {
                   return GitCommitResponse {
                       sha: sha.to_string(),
                       subject: subject.to_string(),
                   };
               }
           }
       }
       GitCommitResponse {
           sha: String::new(),
           subject: message.lines().next().unwrap_or_default().to_string(),
       }
   }
   ```

   Replace the final log parsing/error block in `commit_locked` with:

   ```rust
   let line = run_git_ok(
       path,
       "git log",
       &["log", "-1", "--format=%H%x1f%s"],
       READ_TIMEOUT,
   )
   .await;
   Ok(commit_response_from_log(line, &message))
   ```

   In particular, there must be no `?` after this log read and no `bail!` for unavailable metadata after successful `git commit`. Do not change `run_git` or `run_git_ok`: their errors remain meaningful to other callers.

5. **Make the fallback UX exact in `ProjectGit.tsx::commitChanges`.**

   Replace the unconditional notice template with:

   ```ts
   const result = await api.commitGitChanges(slug, message, targetPaths)
   const sha = result.sha.trim()
   const subject = result.subject.trim()
   setNotice(
     sha
       ? `Committed ${sha.slice(0, 7)}${subject ? ` — ${subject}` : ''}`
       : subject
         ? `Committed — ${subject}`
         : 'Committed successfully',
   )
   ```

   Exact examples: normal metadata gives `Committed abc1234 — feat: save work`; fallback metadata gives `Committed — feat: save work`; both fields empty gives `Committed successfully`; SHA only gives `Committed abc1234`. Never render `Committed  — ` or a trailing separator.

   Preserve message clearing, selection clearing, and `runMutation`'s subsequent `load()` refresh. The fallback is a success notice, not a mutation error and not an instruction to retry the commit. `load()` currently catches its own refresh failures; do not change that flow in this subtask. A refresh failure can still display its existing load error independently of the completed mutation.

6. **Leave `http.rs::git_commit` and the route unchanged.**

   The handler already awaits `super::git::commit` and serializes `GitCommitResponse`. The inspected routing has no commit-specific 12-second deadline. `run_git` already uses asynchronous subprocess waiting and per-command `tokio::time::timeout`.

   No route deadline, background-job API, `spawn_blocking`, detached task, or new outer `tokio::time::timeout` is required for this timeout fix. An outer cancellation deadline would introduce separate mutation-cancellation semantics. The original 12-second error comes from the REST client's `fetchWithTimeout` timer. Existing synchronous small file operations are not the cause being fixed here.

7. **Add focused Rust assertions in `git.rs`'s existing `#[cfg(test)]` module.**

   Keep the existing real-repository rejection and successful round-trip tests. Add these deterministic helper tests:

   ```rust
   #[test]
   fn commit_metadata_success_preserves_git_values() {
       let response = commit_response_from_log(
           Ok("abc1234def\u{1f}actual git subject\n".to_string()),
           "requested subject",
       );
       assert_eq!(response.sha, "abc1234def");
       assert_eq!(response.subject, "actual git subject");
   }

   #[test]
   fn commit_metadata_errors_preserve_success_response() {
       for error in [
           anyhow!("git log timed out"),
           anyhow!("git is not available: spawn failed"),
           anyhow!("fatal: cannot read revision"),
       ] {
           let response = commit_response_from_log(
               Err(error),
               "feat: save work\n\nDetails",
           );
           assert_eq!(response.sha, "");
           assert_eq!(response.subject, "feat: save work");
           assert_eq!(
               serde_json::to_value(&response).unwrap(),
               serde_json::json!({"sha": "", "subject": "feat: save work"}),
           );
       }
   }

   #[test]
   fn commit_metadata_missing_or_malformed_uses_fallback() {
       for line in ["", "\n", "abc1234", "\u{1f}subject", "abc1234\u{1f}"] {
           let response = commit_response_from_log(
               Ok(line.to_string()),
               "feat: save work\n\nDetails",
           );
           assert_eq!(response.sha, "", "input: {line:?}");
           assert_eq!(response.subject, "feat: save work", "input: {line:?}");
       }
   }

   #[test]
   fn commit_core_phase_budget_is_240_seconds() {
       assert_eq!(
           READ_TIMEOUT + ADD_TIMEOUT + COMMIT_TIMEOUT + READ_TIMEOUT,
           Duration::from_secs(240),
       );
   }
   ```

   These assert preservation of real metadata, exact fallback serialization for each metadata error category, safe handling of unusable output, and inclusion of both reads in the composed budget. The helper tests exercise conversion of `run_git_ok`'s `Result`; they do not pretend to simulate an actual failed subprocess. Review the call site to confirm it passes the unpropagated log result into this helper only after commit success.

   Strengthen `commit_stage_and_commit_roundtrip` to use selected paths directly, exercising both add and commit on the path whose budgets are composed:

   ```rust
   // After initializing/configuring the repository and writing tracked.txt:
   let result = commit(
       dir.path(),
       "test: add tracked file\n\nDetails",
       &["tracked.txt".to_string()],
   )
   .await
   .unwrap();
   assert!(!result.sha.is_empty());
   assert_eq!(result.subject, "test: add tracked file");
   assert_eq!(
       run_git_ok(dir.path(), "git rev-parse", &["rev-parse", "HEAD"], READ_TIMEOUT)
           .await
           .unwrap()
           .trim(),
       result.sha,
   );
   ```

   Retain a staged-files/empty-path round trip as well, either keeping the existing test and adding a second one or expanding it with another commit. Keep `commit_reports_git_rejection_from_stderr` asserting `unwrap_err()` and `"git commit failed"`: failure before a landed commit must not become fallback success. Do not add lock-recovery tests in this subtask.

8. **Replace weak timeout checks with actual timer-budget assertions in `ui/hub-core/src/rest.test.ts`.**

   Import `DEFAULT_REQUEST_TIMEOUT_MS` alongside the existing git constants. Replace the old `>= 210_000` commit assertion with:

   ```ts
   assert.equal(GIT_COMMIT_TIMEOUT_MS, 300_000)
   assert.ok(GIT_COMMIT_TIMEOUT_MS > (30 + 60 + 120 + 30) * 1_000)
   assert.equal(GIT_COMMIT_TIMEOUT_MS - 240_000, 60_000)
   assert.equal(GIT_COMMIT_MESSAGE_TIMEOUT_MS, 90_000)
   assert.equal(GIT_MUTATE_TIMEOUT_MS, 120_000)
   assert.equal(GIT_READ_TIMEOUT_MS, 30_000)
   assert.equal(DEFAULT_REQUEST_TIMEOUT_MS, 12_000)
   ```

   Rename the existing “tiers exceed server-side budgets” test to avoid claiming all unchanged tiers exceed their composed routes. Signal presence alone does not prove a request gets the right duration. Capture the scheduled delay with the node:test mock API while returning an immediate mocked fetch response, so each timer is cleared normally and no test waits minutes:

   ```ts
   test('git routes schedule their dedicated timeout tiers', async t => {
     const scheduled: number[] = []
     const originalSetTimeout = globalThis.setTimeout
     t.mock.method(globalThis, 'setTimeout', (...args: Parameters<typeof setTimeout>) => {
       scheduled.push(Number(args[1]))
       return originalSetTimeout(...args)
     })
     t.mock.method(globalThis, 'fetch', async () =>
       new Response('{}', {
         status: 200,
         headers: { 'content-type': 'application/json' },
       }),
     )
     const api = createApi('http://host', 'secret')
     await api.commitGitChanges('my-proj', 'feat: thing', [])
     await api.generateGitCommitMessage('my-proj', [])
     await api.stageGitFiles('my-proj', ['a.txt'])
     await api.unstageGitFiles('my-proj', ['a.txt'])
     await api.discardGitFiles('my-proj', ['a.txt'])
     await api.getGitStatus('my-proj')
     await api.getGitLog('my-proj', 150)
     await api.commitGitChanges('runner', 'feat: thing', [])
     assert.deepEqual(scheduled, [
       300_000, 90_000, 120_000, 120_000, 120_000, 30_000, 30_000, 300_000,
     ])
   })
   ```

   `getGitLog` supplies query parameters; this assertion verifies pathname classification. `runner` verifies that git classification precedes the broad `/run` exemption. Continue existing URL and signal checks where useful.

   Replace “timeout options still override git route timeouts,” which encodes the H6 bug. Use the same timer/fetch instrumentation and independently test each configuration case below, restoring all globals and environment variables in `finally` or through test cleanup. Run these global-mutating cases serially.

   | Configuration supplied for the case | Commit timer | Status timer | Ordinary `listProjects()` timer |
   | --- | ---: | ---: | ---: |
   | No overrides | 300,000 | 30,000 | 12,000 |
   | `options.timeoutMs = 12_000` | 300,000 | 30,000 | 12,000 |
   | `options.timeoutMs = 0` | 300,000 | 30,000 | none |
   | `options.timeoutMs = 600_000` | 300,000 | 30,000 | 600,000 |
   | localStorage `openduck.api.timeoutMs = '12_000'` expressed as `'12000'` | 300,000 | 30,000 | 12,000 |
   | localStorage key set to `'0'` | 300,000 | 30,000 | none |
   | localStorage key set to `'600000'` | 300,000 | 30,000 | 600,000 |
   | `process.env.VITE_API_TIMEOUT_MS = '12000'` | 300,000 | 30,000 | 12,000 |
   | environment key set to `'0'` | 300,000 | 30,000 | none |
   | environment key set to `'600000'` | 300,000 | 30,000 | 600,000 |

   For each row, call commit, status, and listProjects in order and assert the complete timer array. Exact representative assertions:

   ```ts
   assert.deepEqual(scheduled, [300_000, 30_000, 12_000]) // shrinking override
   assert.deepEqual(scheduled, [300_000, 30_000]) // zero affects only ordinary route
   assert.deepEqual(scheduled, [300_000, 30_000, 600_000]) // large override
   ```

   Test localStorage through the same `window.localStorage.getItem` interface production reads. Save the original `window` property descriptor, install a configurable test property, and restore the descriptor (or delete the property if previously absent). Save and restore `process.env.VITE_API_TIMEOUT_MS`, deleting it when originally absent. Clear higher-priority sources when isolating lower-priority cases.

   Add an ordinary-route precedence assertion with all sources populated:

   ```ts
   // localStorage = '23000', environment = '34000'
   await createApi('http://host', '', { timeoutMs: 45_000 }).listProjects()
   await createApi('http://host').listProjects()
   // Then remove the stored override before the third call.
   await createApi('http://host').listProjects()
   assert.deepEqual(scheduled, [45_000, 23_000, 34_000])
   ```

   This proves the git fix does not reorder non-git configuration precedence. Preserve the existing history-specific override, zero-disable, network-status, and long-running-route tests. Do not export request-timeout internals solely for testing.

9. **Verify only when implementation/build/test work is authorized; this request writes the plan only.**

   During implementation, run `cargo fmt` after Rust changes. When authorized to build/test, run the targeted Rust control/git tests, `ui/hub-core` tests, and Hub typecheck using the repository environment. Suggested commands:

   ```bash
   source bin/activate-hermit
   cargo fmt
   cargo test -p openduck control::git::tests
   pnpm --dir ui/hub-core test
   pnpm --dir ui/hub-core typecheck
   pnpm --dir ui/hub typecheck
   ```

   Manually verify the four exact notice strings from step 5 with controlled commit responses, and verify the fallback response clears the composer and refreshes status/history without retrying the commit. The existing Hub test script targets `src/*.test.ts`, so do not assume it automatically executes a newly added nested component test or introduce a new UI test framework for this change.

   Review the final diff for the four intended implementation files (`git.rs`, `rest.ts`, `rest.test.ts`, `ProjectGit.tsx`), with `http.rs` unchanged by this subtask. Preserve other working-tree modifications. Do not run a build, tests, recipe execution, or formatting while merely writing this plan; no source edits are part of the present request.
