# Task plan: per-repository Git actions

## Scope
Implement explicit pull, commit, and push for a selected nested repository without auto-committing, with correct root targeting, clear errors, coverage, and checks.

## Implementation
- Added POST `/api/v1/projects/{slug}/git/repositories/action` with pull/commit/push actions, explicit message requirement for commit, path validation, and resolution to a discovered repository inside project root. Uses selected repository root and write lock for Git commands; commit reuses established staging semantics; pull/push return Git command failures. Push initializes tracking for the configured default remote branch if no upstream is set.
- Added backend unit tests for nested-vs-parent targeting, path validation, no remote, unavailable remote/dirty pull, empty commit/message validation, successful pull/push/commit, and push rejection. Added HTTP auth/routing test coverage.
- Added Hub Core types/request method, per-row Pull/Commit/Push actions, explicit commit dialog, feedback handling/locales, and tests ensuring clicked row path is passed (main composer uses selected repository). Corrected success feedback to not label pull/push as commits.

## Verification
- `git diff --check`: passed.
- `cargo fmt --check`: passed.
- `cargo test -p openduck --lib control::git`: 34 passed. Initial test failure was missing upstream configuration in test fixture; adjusted push behavior/test, rerun passed.
- `cd ui/hub-core && pnpm test && pnpm run typecheck`: 49 tests passed; typecheck passed.
- `cd ui/hub && pnpm test && pnpm run typecheck && pnpm run i18n:check`: 373 tests passed; typecheck passed; locale parity 2106/2106.
- Codex static reviews flagged UI targeting and success label issues; fixed and added focused tests.
- UI production build passed as reported by coding agent; current changes do not include generated `tsconfig.tsbuildinfo`.

## Notes
- One combined check initially returned 101 because `repository_action_push_and_pull_success` expected push success before any branch upstream. This was addressed; final 34-test rerun passed.
- No unrelated modified files remain in status. Existing discovery/status work stays in the changeset.
