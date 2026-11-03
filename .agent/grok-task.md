# Rust task — fix Git commit timeout metadata handling (subtask 2 of 4)

Read these first for context:
- `.agent/plan-subtask2-timeout.md` (sections 2, 4, 6, 7 are yours)
- `.agent/notes-plan-task-fix-git-tab-commit-timeout-lock.md`

Do NOT touch TypeScript files (another agent owns those).
THE ONLY FILE YOU MAY EDIT: `crates/openduck/src/control/git.rs`

## Context
The UI Hub "Git" tab commit was aborted at 12s by the REST client while the server runs
several sequential git subprocesses. Separately, a post-commit `git log` failure turned a
LANDED commit into an error response. This subtask fixes the timeout/metadata half only.
Index-lock cleanup and stale-lock recovery are subtask 3 — do not touch that.

## Change 1 — budget comment only (do NOT change the constants)
Replace the stale comment above the budget constants (currently says
"A full commit costs at most add(60s) + commit(120s) + post-commit log read(30s) = 210s")
with an accurate one, 3-4 lines:
core commit subprocess subtotal = ensure-repo READ_TIMEOUT(30s) + ADD_TIMEOUT(60s)
+ COMMIT_TIMEOUT(120s) + post-commit log READ_TIMEOUT(30s) = 240s; the Hub client allows
300s for `/git/commit` (ui/hub-core/src/rest.ts), 60s headroom; these per-subprocess budgets
do not bound repo write mutex waiting or git-side lock waiting.

## Change 2 — make the post-commit metadata read non-fatal
In `commit_locked` (~line 1019) the final block does `run_git_ok(...).await?` on
`git log -1 --format=%H%x1f%s`, splits on `\u{1f}`, and
`bail!("Commit succeeded but the new revision could not be read")` when sha is empty.

Add a private helper in this file:

```rust
fn commit_response_from_log(line: Result<String>, message: &str) -> GitCommitResponse
```

Behavior: if `line` is Ok and `line.trim().split_once('\u{1f}')` yields a sha and subject
that are BOTH non-empty, return those real git values. Otherwise return
`GitCommitResponse { sha: String::new(), subject: <first non-empty line of message> }`,
where `message` is the already validated+trimmed commit message passed into `commit_locked`.
Never invent a SHA from human-readable output. Keep the `GitCommitResponse` schema.

Call site becomes:

```rust
let line = run_git_ok(path, "git log", &["log", "-1", "--format=%H%x1f%s"], READ_TIMEOUT).await;
Ok(commit_response_from_log(line, &message))
```

Do not change `run_git` / `run_git_ok`.
Do not change the genuine commit-rejection path: a failed `git commit` must still return an
error (`git commit failed` / stderr), and must NOT become fallback success.

## Change 3 — tests in the existing `#[cfg(test)]` module of git.rs
- `commit_metadata_success_preserves_git_values`
- `commit_metadata_errors_preserve_success_response` — timeout-style error, spawn-style
  error, git-fatal error; assert `sha == ""` and `subject ==` first message line, plus exact
  serde_json `{"sha":"","subject":"feat: save work"}` for a message with a body
- `commit_metadata_missing_or_malformed_uses_fallback` — inputs `""`, `"\n"`, `"abc1234"`,
  `"\u{1f}subject"`, `"abc1234\u{1f}"`; assert `sha == ""` and `subject ==` first message line
- `commit_core_phase_budget_is_240_seconds` asserting
  `READ_TIMEOUT + ADD_TIMEOUT + COMMIT_TIMEOUT + READ_TIMEOUT == Duration::from_secs(240)`
- strengthen existing `commit_stage_and_commit_roundtrip` to commit a specific path
  (e.g. `&["tracked.txt".to_string()]`) so both `git add -- <path>` and `git commit -- <path>`
  run; assert subject == message first line and returned sha == `git rev-parse HEAD`.
  Keep a staged-files / empty-paths round trip too so both paths are covered.
- keep `commit_reports_git_rejection_from_stderr` asserting an error

## Style (project rules)
Self-documenting code; no comments restating code; `anyhow::Result`; no new logging;
no overly defensive code; minimal scoped diff; do not reformat unrelated code.

## Verify and report
```
cargo fmt
cargo check -p openduck --lib
cargo test -p openduck --lib control::git::tests
```
`run_git_timeout_error_names_the_phase` is known-flaky in full-module runs (Duration::ZERO
race) — re-run it alone if it fails.

Do NOT run clippy. Do NOT commit. Report the diff summary and test output.
