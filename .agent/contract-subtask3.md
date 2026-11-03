# Subtask 3 implementation contract (final, overrides .agent/plan-subtask3-lock.md)

Scope decision: the codex plan over-scoped (it declares the guarantee unreachable and
proposes a full ownership/containment redesign). Rejected as a material design change.
This contract is the authoritative scope: pragmatic, minimal, in `crates/openduck/src/control/git.rs` only.

## Root cause of the residue bug (already verified)
`cleanup_orphaned_index_lock` refuses any lock younger than `STALE_LOCK_AGE` (10s).
A lock created by OUR OWN git child that we just killed on timeout has mtime in the
last few milliseconds, so it is never removed. The next operation then hits
`recover_stale_index_lock`, sees age < 10s, and bails with "Another git operation is
in progress" — permanently (nothing ever retries). That is the user-facing permanent freeze.

## Required changes

### 1. Single discovery per guarded operation (removes H10's extra rev-parse)
Add `struct RepoLayout { toplevel: PathBuf, git_dir: PathBuf, index_lock: PathBuf }` and

```rust
async fn repo_layout(path: &Path) -> Result<RepoLayout>
```

It runs exactly one subprocess:

```
git rev-parse --is-inside-work-tree --path-format=absolute --show-toplevel --git-path index.lock --absolute-git-dir
```

Verified on git 2.43.0: prints 5 lines in that order (true, toplevel, lock, gitdir).
Parse strictly: 5 lines; line0 == "true" else `bail!("Not a git repository")` (KEEP this
string, tests assert it); lines 1-3 must be absolute; no embedded '\n' handling needed since
we split by lines but reject empty. If any check fails -> `bail!("Not a git repository")`.

Then replace `ensure_repo(path).await?` + `recover_stale_index_lock(path).await?` at the four
guarded entry points with a single `let layout = repo_layout(path).await?;` followed by
`reclaim_stale_index_lock(&layout).await?`. Keep `ensure_repo` unchanged for the other callers
(status, log, diff, show, worktree*, message) — it is still used at l.711,742,817,854,1254,1308,
1463,1484,1507,1526. Do NOT delete `repo_toplevel`; it is used elsewhere. Add `#[allow(dead_code)]`
only if a helper becomes unused, but prefer deleting truly unused code.

### 2. Provenance for own-child locks (fixes H1, the real bug)
The runner must know whether the lock existed BEFORE the child was spawned.

- `run_git` gains provenance awareness without changing its public signature being used
  everywhere: keep `run_git(path,label,args,timeout)` for read-only commands, and add

```rust
async fn run_git_mutating(
    path: &Path,
    lock_path: &Path,        // resolved once by the caller from RepoLayout
    label: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<GitOutput>
```

`run_git_mutating` snapshots `LockSnapshot::capture(lock_path)` (None if absent) BEFORE spawn.
On timeout: reap the child, then if the lock's current identity differs from the pre-spawn
snapshot (i.e. the lock was created or replaced by the child we just killed) -> remove it
(`lock_snapshot_changed` + `remove_created_index_lock`). On ordinary success/failure, do the
same check (a killed writer is not the only source; a failed `git commit` can leave the lock
only if it was killed). Never remove when the current identity EQUALS the pre-spawn snapshot
(that lock was not ours).

Then DELETE `cleanup_orphaned_index_lock` and the `writes_git_index`/`INDEX_WRITING_GIT_SUBCOMMANDS`
classifier entirely, and route each mutation call through `run_git_mutating`:
- commit_locked: `git add` (l.1033, ADD_TIMEOUT) and `git commit` (l.1049, COMMIT_TIMEOUT)
- stage_locked: `git add` (l.914, MUTATE_TIMEOUT)
- unstage_locked: `git restore` (l.946)
- discard_one: `git clean`/`git restore` (l.978, 982, 990, 1012) -> thread `&layout` through
  `discard_locked` -> `discard_one`.

### 3. Stale-lock reclamation policy (unknown pre-existing lock)
`reclaim_stale_index_lock(layout: &RepoLayout) -> Result<()>`

- Resolve via `symlink_metadata(layout.index_lock)`; NotFound -> Ok(()).
- Not a regular file -> bail with the existing "git index lock path is not a regular file: {}".
- If age >= STALE_LOCK_AGE **and** no live git process covers the repo -> remove with
  identity re-check (section 4). Live check uses `has_live_git_process_in(&layout.toplevel)`.
- Otherwise (fresh, or a live git process exists) -> bail with an ACTIONABLE message that
  includes the absolute lock path and the manual reclaim steps:
  `"Another git operation is in progress in this repository (lock: {path}). If no git process is running, delete that file and retry."`
  This keeps safety (never delete a possibly-live lock) while unblocking the user.
- Replace the old `recover_stale_index_lock`, update all 4 callers.

### 4. TOCTOU narrowing (H5) + identity
`#[derive(Clone, Copy, PartialEq, Eq, Debug)] struct LockIdentity { len: u64, modified: Option<SystemTime>, dev_ino: Option<(u64,u64)> }`
with `fn lock_identity(path: &Path) -> Option<LockIdentity>` (None when absent) using
`symlink_metadata`; `dev_ino` from `std::os::unix::fs::MetadataExt` under `#[cfg(unix)]`, `None`
elsewhere. `remove_created_index_lock(path, expected_changed_from: Option<LockIdentity>)`:
inside the existing retry loop (LOCK_REMOVE_RETRIES x LOCK_RETRY_DELAY) re-read identity and
only `remove_file` when it still differs from the pre-spawn snapshot; if it changed again, abort
without deleting (another writer took over). Keep NotFound tolerant.

### 5. Cancellation / Drop guard (H2, best-effort, scoped)
Add a small guard used ONLY around the child-spawn region of `run_git_mutating`:

```rust
struct OwnedLockGuard { lock_path: PathBuf, before: Option<LockIdentity>, armed: bool }
```

`Drop` runs synchronously: if armed, it re-checks identity and, when the lock is new relative to
`before`, deletes it with `std::fs::remove_file` (sync call inside Drop is the only reliable way;
no async spawn, so no interaction with `with_repo_write_lock` and no deadlock). Disarm on normal
completion and on the timeout path (which cleans up explicitly so it can report errors).
Document why sync-in-Drop is chosen (dropping the future is the only signal; async work would be
cancelled).

### 6. Tests
Integration test file `crates/openduck/tests/git_index_lock.rs` (repo convention), using
`tempfile` + real `git init`, helpers similar to task_auto_commit.rs:
1. `fresh_lock_from_owned_timeout_is_removed`: pre-create no lock, run a mutating op with a
   fixture that leaves a fresh lock (simulate by creating `.git/index.lock` inside a
   `pre-commit` hook script that sleeps, then let the op time out with a tiny timeout — use the
   unit-level `run_git_mutating` via a `#[cfg(test)]`-visible path, or expose a small
   test-only wrapper). Assert lock gone afterwards.
2. `aged_unknown_lock_without_git_process_is_reclaimed`: create `.git/index.lock`, set its mtime
   20s in the past (use `filetime`? NOT available — instead use `std::fs::File::set_times`
   with `SystemTime` (Rust 1.75+, available on 1.96) or simply sleep-free approach: write the
   lock, then `touch`-equivalent via `set_times`). Then commit succeeds and lock is gone.
3. `fresh_unknown_lock_is_refused_with_actionable_error`: create a brand-new `.git/index.lock`,
   attempt commit, assert error mentions the lock path and "delete that file".
4. `stale_lock_identity_change_aborts_removal`: unit test over
   `remove_created_index_lock` where the identity is replaced between capture and removal ->
   returns Ok but leaves the file (refuses to delete an unknown replacement).
5. `non_repo_still_reports_not_a_git_repository`: regression for the error string.
6. `commit_stage_and_commit_roundtrip` in git.rs must keep passing (no lock left behind).

Prefer unit tests in the existing `mod tests` for pure logic (parse of repo_layout output,
identity comparison), and the integration file for filesystem behaviour.

## Acceptance
- `cargo fmt`; `cargo check -p openduck --lib`; `cargo test -p openduck --lib control::git`;
  `cargo test -p openduck --test git_index_lock`; `cargo clippy --all-targets -- -D warnings`.
- No changes to `ui/`, no timeout constant changes, no new dependencies.
- Update the 240s budget comment in git.rs if the discovery change alters the phase sum
  (it should keep 240s: discovery replaces ensure_repo's 30s READ_TIMEOUT phase).
