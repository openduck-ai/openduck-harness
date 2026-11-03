# Task: Fix UI Hub Git tab commit timeout + index.lock residue

Worktree: /mnt/e/openduck-harness. Baseline HEAD a38080d. Codex design: .agent/plan-subtask3-lock.md (480 lines).

## Progress
1. Recon — DONE
2. Timeout fix — DONE
3. Lock cleanup + stale-lock recovery — IN PROGRESS (this subtask)
4. Verify + regression tests — pending

## Scope decision for subtask 3 (MINE, overrides codex's over-scoped plan)
Codex recommended a full ownership/containment redesign and declared the absolute
guarantee unreachable ("do not label subtask 3 satisfied"). I reject that scope; it
is not needed for the user's bug and would be a material design change. Adopted
pragmatic scope:
 (a) provenance-based cleanup: locks created by OUR killed/timed-out child are
     removed regardless of mtime age (fixes H1, the actual user-facing bug);
 (b) unknown pre-existing lock: keep the conservative age+live-process+content rule,
     refuse removal with an actionable error, but make the message name the exact
     lock path and the reclaim steps;
 (c) reuse ONE rev-parse discovery per guarded operation to (1) validate the repo,
     (2) get toplevel, (3) get absolute index.lock path, (4) provide the stale
     snapshot — removes the extra 2x30s rev-parse per cleanup (H10) and keeps the
     documented 240s/300s invariant;
 (d) TOCTOU-narrowing: re-verify the file identity immediately before unlink inside
     the retry loop (mtime+len+dev+ino) and abort removal if it changed (H5);
 (e) Drop guard: on cancellation, a detached cleanup task removes ONLY a lock whose
     snapshot identity it captured before the child started (H2), never an unknown
     lock.

## Verified facts
- git 2.43.0; rustc 1.96.1, edition 2021, rust-version 1.94.1.
- `git rev-parse --is-inside-work-tree --path-format=absolute --show-toplevel
  --git-path index.lock --absolute-git-dir` works and prints one line per flag in
  that order; in a subdir it returns toplevel + absolute lock; in a linked worktree
  it returns /repo/.git/worktrees/wtN/index.lock. Without --path-format=absolute,
  --git-path returns a RELATIVE path (../.git/index.lock) - must use the flag.
- Empirical: index.lock lives only while a writer is actually writing; a 4s
  pre-commit hook and a 200-file add did not leave an observable lock in my
  sampling window. Therefore tests must CREATE the lock file directly rather than
  racing a real git writer.
- `crates/openduck` dev-deps have tokio but NOT futures.
- run_git: l.376-412. cleanup calls at 402-404 and 408-410 (writes_git_index).
- cleanup_orphaned_index_lock l.414-437: skips age<10s (H1 root cause).
- recover_stale_index_lock l.125-146: bails on non-file / fresh / live process.
- Guarded fns: stage_locked 907, unstage_locked 922, discard_locked 954,
  commit_locked 1020. discard_one 966-1013 called only from discard_locked.
- ensure_repo 511-523 (one rev-parse). repo_toplevel 45-61. index_lock_path 63-83.
- Tests mod ends at 1972 incl. commit_core_phase_budget_is_240_seconds.

## Verification commands
- cargo fmt; cargo check -p openduck --lib
- cargo test -p openduck --lib control::git
- cargo test -p openduck --test git_index_lock
- cargo clippy --all-targets -- -D warnings

## Attempt log for subtask 3 (12:4x-13:1x)
- codex session codex_1793639511765 produced .agent/plan-subtask3-lock.md (480 lines). It is
  OVER-SCOPED: declares the absolute guarantee unreachable, demands an index_lock.rs module with
  /proc/<pid>/fd ownership receipts, process-tree containment, cancellation supervisor. REJECTED
  as a material design change. Superseded by .agent/contract-subtask3.md (mine, authoritative).
- grok_1793641586464 / lockA-grok / agy_1793641716200: all three stalled in "Working.../Generating..."
  with no file edits. The composer is not accepting prompts (input composer did not become ready).
  => TUI delegation is flaky right now; retry with a FRESH session id.

## NEXT ACTION
Spawn ONE fresh coding agent (grok preferred, agy fallback) with a SHORT prompt (long prompts seem
to make these CLIs hang) that points at .agent/contract-subtask3.md and does chunk A only.
Git diff of git.rs currently shows ONLY prior subtask-2 changes (522 insertions vs HEAD are the
pre-existing replan of subtask1/2 work); no repo_layout yet. Verify with:
 grep -c repo_layout crates/openduck/src/control/git.rs  # must be >0 after chunk A

## Chunk A DONE (agy session chunkA2, verified by me)
git.rs now has: RepoLayout{toplevel,git_dir,index_lock}, repo_layout() (ONE rev-parse with
--is-inside-work-tree --path-format=absolute --show-toplevel --git-path index.lock
--absolute-git-dir), parse_repo_layout() (4 lines, line0=="true", all absolute),
LockIdentity{len,modified,dev_ino} + lock_identity/identity_from_metadata/device_and_inode
(unix dev+ino, None elsewhere), reclaim_stale_index_lock(layout), index_lock_in_progress(),
remove_observed_index_lock(path, expected), remove_created_index_lock(path, before:Option<..>),
unlink_index_lock, remove_index_lock(path), OwnedLockGuard{lock_path,before,armed} + arm/disarm.
recover_stale_index_lock REMOVED. 4 call sites now do repo_layout+reclaim (l.1081/1096/1128/1194).
Budget comment updated to "repo discovery READ_TIMEOUT(30s) + ADD(60) + COMMIT(120) + log(30) = 240s".
VERIFIED: `git rev-parse --is-inside-work-tree --path-format=absolute --show-toplevel
--git-path index.lock --absolute-git-dir` prints exactly 4 lines (true,toplevel,lock,gitdir) on
git 2.43.0; exit 128 + "fatal: not a git repository" outside a repo (mapped to "Not a git
repository"). cargo check -p openduck --lib OK; cargo test -p openduck --lib control::git => 18
passed 0 failed (incl. fresh_index_lock_blocks_commit_without_removal and
stale_index_lock_is_recovered_before_commit, which the agent added).

## Chunk B IN PROGRESS (agy_1793643536601), task file .agent/chunkb-task.md
B1: delete INDEX_WRITING_GIT_SUBCOMMANDS/writes_git_index/cleanup_orphaned_index_lock; add
run_git_mutating(path, lock_path, label,args,timeout) + run_git_mutating_ok; snapshot
lock_identity BEFORE spawn; remove_created_index_lock after timeout reap AND after normal exit;
route stage/unstage/discard_one/commit(add&commit) through it. B2: arm OwnedLockGuard pre-spawn,
disarm after explicit cleanup; drop the #[allow(dead_code)]s. B3: parse_repo_layout unit tests +
crates/openduck/tests/git_index_lock.rs integration tests (reclaim aged lock via FileTimes
backdating, fresh lock refused with actionable message, no-lock-after-commit, non-repo message).

## Milestone: TUI delegation note
grok/agy `tui_agent` calls kept failing with "input composer did not become ready". Workaround that
WORKS: start the session with tui_agent_start (it returns even when the prompt fails), then
`tmux -L openduck send-keys -t <session> -l "<text>"` + `send-keys Enter`. Use /tmp/kick.sh <session>.

## Chunk B DONE + VERIFIED BY ME (grok session chunkB2-grok)
grok initially hit "exit 101" on `cargo test -p openduck --lib control::git` because RepoLayout
lacked Debug (the test asserted on it); it added `#[derive(Debug)]`... actually the fix landed as
`#[allow(dead_code)]` on git_dir plus Debug where needed; after its 2nd pass everything compiled.
Final verified state of crates/openduck/src/control/git.rs (~2100 lines):
- RepoLayout{l.85}, #[allow(dead_code)] on git_dir (only remaining allow).
- repo_layout l.133 (ONE rev-parse, 4 lines, maps to "Not a git repository"), parse_repo_layout l.153.
- LockIdentity l.174, lock_identity l.181, identity_from_metadata l.186, device_and_inode l.192
  (cfg(unix) dev+ino, None elsewhere).
- reclaim_stale_index_lock l.205, index_lock_in_progress l.229 (actionable msg),
  remove_observed_index_lock l.236, remove_created_index_lock l.216,
  unlink_index_lock l.240, remove_index_lock l.336.
- OwnedLockGuard l.260 + arm/disarm l.266-278 + Drop l.280-292 (sync unlink, diff-vs-before).
- GitChildOutcome/run_git_timed l.510-547 (shared spawn/drain/timeout), run_git l.549,
  run_git_mutating l.553 (snapshot before spawn, explicit remove_created_index_lock on BOTH
  timed-out and normal paths, guard.disarm), run_git_mutating_ok l.573.
- REMOVED: recover_stale_index_lock, cleanup_orphaned_index_lock, writes_git_index,
  INDEX_WRITING_GIT_SUBCOMMANDS, index_lock_path (grep confirms 0 hits).
- run_git_mutating wired into stage add l.1065, unstage restore l.1104, discard_one l.1148/1159/1168/
  1191, commit add l.1219 + commit l.1235. discard_one now takes &RepoLayout.

### Verification I ran myself (all green)
- cargo fmt --check                     -> clean
- cargo check -p openduck --lib          -> ok
- cargo test -p openduck --lib control::git -> 20 passed, 0 failed
  (incl. parse_repo_layout_accepts/rejects, fresh_index_lock_blocks_commit_without_removal,
   stale_index_lock_is_recovered_before_commit, commit_core_phase_budget_is_240_seconds)
- cargo test -p openduck --test git_index_lock -> 4 passed, 0 failed
  (non_repo_reports_not_a_git_repository, fresh_lock_blocks_commit_with_actionable_error,
   aged_lock_is_reclaimed_before_commit, fresh_lock_left_by_failed_commit_is_cleaned_up)
- cargo clippy -p openduck --all-targets -- -D warnings -> clean (only the pre-existing
  openduck-cli duplicate-main-target warning, unrelated)

## SUBTASK 3 = COMPLETE. Next: subtask 4 (final verification).
Remaining for subtask 4: full `cargo test -p openduck --lib`, ui/hub-core pnpm test +
typecheck, ui/hub pnpm test + typecheck (already green from subtask 2, re-run to confirm no drift),
and summarize cleanup timing + stale-lock safety conditions in the final answer.

## Final polish DONE (grok polish-grok; agy polish3-agy stalled)
- Removed unused `git_dir` field + its `#[allow(dead_code)]` + stale comment from RepoLayout;
  parse_repo_layout still validates lines[3] absolute via a local var but no longer stores it.
  => zero #[allow(dead_code)] left in git.rs.
- run_git_mutating's duplicated if/else collapsed to: guard.disarm(); let _ =
  remove_created_index_lock(lock_path, before).await; finish_git_run(outcome?)

### FINAL VERIFICATION (after polish, run by me)
- cargo fmt --check                      -> FMT OK
- cargo test -p openduck --lib control::git -> 20 passed, 0 failed
- cargo test -p openduck --test git_index_lock -> 4 passed, 0 failed
- cargo clippy -p openduck --all-targets -- -D warnings -> clean
git.rs is 2190 lines (+747/-59 vs HEAD). Test file crates/openduck/tests/git_index_lock.rs is new
(untracked, 3658 bytes).

## SUBTASK 3 COMPLETE. Ready for subtask 4 (final verification + summary).

## Regression tests: added + corrected (after polish)
Added the MISSING H1 regression test (this was the real bug and had no coverage):
- `control::git::tests::timed_out_git_child_lock_is_reclaimed` (unix-only, in-crate):
  seeds repo with pending change, writes .git/hooks/pre-commit that `touch`es the
  index lock then `sleep 2`, calls private `run_git_mutating(...)` with a 300ms
  timeout so the child is killed right AFTER the hook created the lock, asserts
  error mentions "timed out" and `!layout.index_lock.exists()`.
- NEGATIVE CONTROL verified (this is what proves it guards the fix): I patched
  `remove_created_index_lock` to `return Ok(())` early -> the test FAILED at
  `assert!(!layout.index_lock.exists())`; restored -> passes. So the test genuinely
  fails without the provenance-cleanup fix. /tmp/git.rs.bak was the restore point.
- Integration test names were misleading (they never created a lock), renamed:
  fresh_lock_left_by_failed_commit_is_cleaned_up -> `commits_leave_no_lock_behind`;
  fresh_lock_blocks_commit_with_actionable_error ->
  `fresh_lock_blocks_commit_and_names_lock_path`. fn names only, no logic.

## FINAL VERIFICATION (authoritative, latest run)
- cargo fmt --check                                  -> FMT OK
- cargo test -p openduck --lib control::git          -> 21 passed, 0 failed
- cargo test -p openduck --test git_index_lock       -> 4 passed, 0 failed
- cargo clippy -p openduck --all-targets -- -D warnings -> clean

## Tooling notes
- `agy` stalls repeatedly (queued prompts never execute) -> fall back to `grok` (reliable).
- kick helper: /tmp/kick.sh <session> "<prompt>" (tmux send literal then Enter).

## SUBTASK 3: COMPLETE ✅
Final state verified: fmt OK; lib control::git 21 passed; git_index_lock 4 passed;
task_auto_commit 3 passed; clippy -D warnings clean (only pre-existing
openduck-cli Cargo.toml duplicate-target warning, not from this change).
Unrelated pre-existing fmt-only diffs also present in tests/debug_dump.rs and
tests/provider_model_selection_test.rs (NOT mine, not touched by this task).
Sessions killed, temp files removed. Proceed to subtask 4.

## SUBTASK 4 (verification) — attempt log
### Green so far
- cargo fmt --check                                  -> OK
- cargo test -p openduck --lib control::git          -> 21 passed, 0 failed
- cargo test -p openduck --test git_index_lock       -> 4 passed, 0 failed
- ui/hub-core: npm test -> 42 passed; npm run typecheck -> OK
- ui/hub:      npm test -> 271 passed; npm run typecheck -> OK
### Problem found
- cargo test -p openduck --lib (FULL) -> 2152 passed, 34 FAILED.
  Failing modules: acp::tests, agents::prompt_manager, agents::platform_extensions,
  agents::state_machine::compaction_lifecycle, config::migrations, config::providers,
  providers::*, rules, scheduler, sources.
  ZERO failures in control::*. Worktree also carries an UNRELATED in-progress task
  (.agent/notes-plan-provider-model-decouple-full.md) with modified
  config/providers.rs, config/migrations.rs, acp/server/config.rs,
  providers/inventory/mod.rs, control/app_config.rs -> those failures are its fallout.
  Must PROVE this before claiming verification.
### Missing coverage to add
1. Cancellation: drop the commit future mid-flight -> OwnedLockGuard::drop must unlink the lock.
2. Failed commit (hook exit 1 / git rejects) -> no lock, repo still usable.
3. Post-failure recovery: fresh lock rejection then, after aging/removal, commit succeeds
   (proves one failure does not permanently wedge the repo).

### ROOT-CAUSED the 34 lib failures: NOT MINE (proven)
Experiment: `git stash push -- crates/openduck/src/control/git.rs` (only my file reverted
to HEAD, rest of the other task's changes left in place) then re-ran the full lib suite:
  baseline 34 failing / mine 34 failing -> `diff /tmp/baseline_fails.txt /tmp/real_fails.txt`
  => IDENTICAL FAILURE SETS. Zero regressions attributable to this task.
Failures live in config::*, providers::*, acp::*, agents::prompt_manager,
agents::state_machine::compaction, control::app_config, control::harness::tests,
rules/scheduler/sources -> fallout of the concurrent provider-model-decouple task
(modified config/providers.rs, config/migrations.rs, acp/server/config.rs,
providers/inventory/mod.rs, control/app_config.rs).
control::git itself: 21/21 passing. git.rs restored byte-identical (diff -q OK).

### SUBTASK 4 progress — tests added & verified
NEW: crates/openduck/tests/git_index_lock.rs grew 130 -> 214 lines (+88/-2, by grok).
Helpers added: install_pre_commit_hook(root, script) (unix perms 0o755).
Tests added (both #[cfg(unix)]):
  - cancelled_commit_removes_lock_it_created: pre-commit hook touches index.lock then sleeps 5s;
    outer tokio timeout 500ms cancels the future mid-hook; then polls up to 2s for absence.
    Proves OwnedLockGuard::drop sync-unlink cleans up on browser-abort cancellation.
  - failed_commit_leaves_no_lock_and_repo_stays_usable: hook touches lock then exit 1;
    commit Err, no lock, hook removed, second commit Ok with non-empty sha => repo not wedged.
RESULTS: cargo test -p openduck --test git_index_lock -> 6 passed, 0 failed,
         repeated 3x consecutively: 1.44s / 0.53s / 0.54s -> all 6 passed. No flake.
  cargo fmt --check -> FMT OK
  cargo clippy -p openduck --lib --tests -- -D warnings -> clean (Finished, no warnings)
Grok initially flagged a real test-design hazard (non-executable hook / 500ms too tight);
final code handles it with 0o755 perms in the helper and a 2s post-cancel poll window.

### 50/50 coding split status
- grok: git_index_lock.rs two tests (DONE, verified above; session stopped due to cargo
  lock contention from concurrent builds).
- agy: git_commit_recovery.rs (cancelled commit releases repo write mutex) IN FLIGHT.
NOTE: concurrent `cargo test` runs deadlock on the target-dir lock -> run cargo serially.

### BLOCKER: agy unusable this run
- agy_1793654603257: stuck in "Generating..." forever even after approving (known stall).
- agy_1793654907105: fresh session shows "You are currently not signed in. Select login
  method: Google OAuth / Google Cloud project" -> cannot run any prompt.
=> agy has NO usable auth right now. Falling back to grok for the second test,
   recording that here so the 50/50 intent is documented even though only one coder
   was actually available.

## SUBTASK 4: COMPLETE ✅
### Final verification (all run serially after stopping all coder sessions)
- cargo fmt --check                                              -> FMT OK
- cargo test -p openduck --test git_index_lock        x2 runs     -> 6 passed, 0 failed each
    (incl. new cancelled_commit_removes_lock_it_created,
           failed_commit_leaves_no_lock_and_repo_stays_usable)
- cargo test -p openduck --test git_commit_recovery   x2 runs     -> 1 passed, 0 failed each
    (new cancelled_commit_releases_repo_write_lock)
- cargo test -p openduck --lib control::git                      -> 21 passed, 0 failed
- cargo clippy -p openduck --lib --tests -- -D warnings          -> clean (Finished, no warnings)
- ui/hub-core npm test                                           -> 42 tests, 42 pass, 0 fail
- ui/hub      npm test                                           -> 271 tests, 271 pass, 0 fail
- ui/hub-core npm run typecheck / ui/hub npm run typecheck        -> both exit 0
### Regression proof for the 34 full-lib failures
Identical failure set with git.rs reverted to HEAD => none attributable to this task.
They are fallout of the concurrent provider-model-decouple task.
### Files delivered by this task
- crates/openduck/src/control/git.rs            (timeout budgets, commit metadata
   fallback, provenance lock cleanup, stale-lock reclaim, TOCTOU re-check, Drop guard)
- crates/openduck/src/control/http.rs           (fmt-only)
- ui/hub-core/src/rest.ts + rest.test.ts        (per-route git timeouts)
- ui/hub/src/components/ProjectGit.tsx          (commit notice handles empty sha)
- crates/openduck/tests/git_index_lock.rs       (+88, 4 -> 6 tests)
- crates/openduck/tests/git_commit_recovery.rs  (new, 1 test)
### Cleanup
All coder sessions stopped (grok x2, agy x2). No stray cargo/git processes.
