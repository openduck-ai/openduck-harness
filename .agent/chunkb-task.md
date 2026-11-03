# CHUNK B task (implement in crates/openduck/src/control/git.rs + new integration test)

Chunk A is already implemented and compiles (repo_layout, RepoLayout, reclaim_stale_index_lock,
LockIdentity/lock_identity, remove_observed_index_lock, remove_created_index_lock, OwnedLockGuard,
unlink_index_lock, remove_index_lock(path)). Read `.agent/contract-subtask3.md` section 2 and 6.

## B1. Provenance-aware mutating runner
Delete `INDEX_WRITING_GIT_SUBCOMMANDS`, `writes_git_index`, and `cleanup_orphaned_index_lock`
entirely. Add:

```rust
async fn run_git_mutating(
    path: &Path,
    lock_path: &Path,
    label: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<GitOutput>
```

It snapshots `remove_created_index_lock`'s "before" value BEFORE spawning:
`let before = lock_identity(lock_path);`
Then, after the child finishes or is reaped:
`let _ = remove_created_index_lock(lock_path, before).await;`
i.e. cleanup runs on BOTH the timeout path (after `reap_timed_out_git`) and the normal path
(success or non-zero exit). Keep `run_git` exactly as-is otherwise (it must stop calling
`cleanup_orphaned_index_lock`). `run_git_mutating` must reuse the same spawn/drain/timeout body as
`run_git`; factor the shared part into a private helper rather than duplicating it if that is
cleaner, but do NOT change `run_git`'s signature (it has many callers).

Route these call sites through `run_git_mutating(..., &layout.index_lock, ...)`:
- stage_locked: `git add` (MUTATE_TIMEOUT) — replace `run_git_ok` with `run_git_mutating` + the
  existing success check (factor a `run_git_mutating_ok` twin of `run_git_ok` that does
  `run_git_mutating` then the same `!output.success` bail/`Ok(stdout)` logic).
- unstage_locked: the `git restore` call.
- discard_locked -> discard_one: `git clean` / `git restore` calls (thread `&RepoLayout` into
  `discard_one`).
- commit_locked: `git add` (ADD_TIMEOUT) and `git commit` (COMMIT_TIMEOUT). The post-commit
  `git log` stays a plain `run_git_ok` (read-only).

## B2. OwnedLockGuard wiring (cancellation)
Arm an `OwnedLockGuard` in `run_git_mutating` immediately after `before` is captured and before
spawn: `let guard = OwnedLockGuard::arm(lock_path.to_path_buf(), before);`
- On the normal return path: `guard.disarm();` then run `remove_created_index_lock` (so errors are
  reportable) — or simply let the async cleanup run and disarm right before it.
- Do NOT disarm on the timeout path before the explicit cleanup; disarm after cleanup.
Remove the `#[allow(dead_code)]` attributes chunk A added on the guard/created-remover once they
are used.

## B3. Tests
Add to the existing `mod tests` in git.rs (pure/parse/identity logic):
- `parse_repo_layout_accepts_git_output` and `parse_repo_layout_rejects_bad_output` (wrong line
  count, "false", relative paths).
- keep existing `fresh_index_lock_blocks_commit_without_removal` and
  `stale_index_lock_is_recovered_before_commit` passing.

Add integration file `crates/openduck/tests/git_index_lock.rs` (real git, tempfile):
1. `fresh_lock_left_by_failed_commit_is_cleaned_up`: init repo, create a `.git/index.lock` with
   OLD mtime won't be needed here — instead assert that after a *successful* commit no lock remains
   and that `commit` works twice in a row.
2. `aged_lock_is_reclaimed_before_commit`: init repo, create `.git/index.lock`, backdate its mtime
   by 60s using `std::fs::File::set_times` / `FileTimes` (Rust 1.75+), then
   `openduck::control::git::commit(...)` succeeds and the lock file is gone.
3. `fresh_lock_blocks_commit_with_actionable_error`: create a brand-new `.git/index.lock`, call
   `commit`, assert the error string contains the absolute lock path and "delete that file".
4. `non_repo_reports_not_a_git_repository`: a plain temp dir -> commit() errors containing
   "Not a git repository".
Use `openduck::control::git::{commit, stage}` and plain `std::process::Command` git setup like
`crates/openduck/tests/task_auto_commit.rs` does (set `user.email`/`user.name` locally).

## Acceptance
cargo fmt; cargo check -p openduck --lib; cargo test -p openduck --lib control::git;
cargo test -p openduck --test git_index_lock; cargo clippy --all-targets -- -D warnings.
No changes outside crates/openduck/src/control/git.rs and crates/openduck/tests/git_index_lock.rs.
