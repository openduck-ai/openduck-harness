# Subtask 3: index-lock lifecycle design and safety boundary

Design only; no source changes. References below are to the inspected working tree,
not HEAD. Unless qualified, line references mean
`crates/openduck/src/control/git.rs` (1,972 lines). Subtasks 1–2 remain complete;
no client changes, timeout-constant changes, or redesign of commit-response notices.

## 1. Decision: distinguish the achievable fix from the absolute guarantee

**The complete requested guarantee is not implementable with the current shared
index, arbitrary external Git writers, and the stated portable/minimal constraints.**
Implement the cancellation/budget improvements and fail-closed safety below, but do
not describe those improvements as fully completing H1/H3/H5/H7/H8. In particular,
do not replace the existing age gate with unconditional removal after child exit.

Three counterexamples explain this boundary:

1. Observe no lock, spawn server Git; external Git wins `index.lock` creation;
   server Git fails because the lock exists. Removing the resulting lock as
   “ours” destroys the external writer's exclusion. Before/after snapshots, a
   server operation ID, and the process-local mutex cannot distinguish this from
   an orphan left by our own child. The mutex currently covers only this process
   (lines 18–43); `run_git` records no lock-creation provenance (376–411).
2. An empty, old, unchanged lock is compatible with a live writer waiting for a
   hook, an invisible Windows writer on WSL, or an unreadable process. Neither
   the lock bytes nor mtime prove the writer died. The existing detector ignores
   unreadable entries (87–104), requires exactly `git` (102), checks only cwd
   (112–118), and returns false outside Linux (120–123).
3. Even a dev/inode/mtime recheck immediately before `remove_file` is two separate
   filesystem operations. Another actor can replace the directory entry between
   them. The retry loop at 148–161 widens the existing race; adding a recheck
   narrows it but does not make unlink conditional on file identity. Renaming
   to quarantine also moves whatever currently occupies the name, and is not
   a solution to deleting/moving an active replacement lock.

Consequently “automatically remove every abandoned unknown lock” and “never remove
any possibly live external lock” cannot both be unconditional acceptance criteria.
An unavailable detector must refuse removal. Filesystem permission errors, server
SIGKILL, runtime destruction, and a live descendant escaping containment also rule
out an unconditional eventual-unlink promise. Record these as explicit limits,
not swallowed cleanup errors or a false claim that polling proves ownership.

This plan specifies a concrete safety-first implementation, its conditional owned
cleanup path, and the additional decision needed for the stronger guarantee.
The final section states exactly which acceptance criteria remain blocked.

## 2. Existing call graph and required coverage

- `stage`/`stage_locked`: 903–916; `unstage`: 918–948;
  `discard`: 950–964; `commit`: 1016–1065. Each public function takes the current
  input-path mutex; each inner function calls ensure/recovery separately.
- `discard_one` (966–1013) runs clean/restore and must receive the same context.
- `commit_locked` runs add at 1033, commit at 1049, and best-effort log at
  1057–1064. Preserve `commit_response_from_log` (1067–1082).
- `TaskAutoCommit::finish` delegates to public `commit` at 1364; keep that route.
  Its observational status helper is at 1287–1301, public status at 710–729.
- `git_tokio` (334–347) preserves `git_command()` security settings, strips Git
  directory/index environment overrides, and enables kill-on-drop. It does not
  invoke the process-group setup in `subprocess.rs:68–81`.
- Current timeout reaping (370–374) ignores kill/wait errors. Cleanup runs only
  on timeout/completion (398–410), not a wait-error early return at 399 or drop.
- Worktree add/remove (1478–1522) are additional mutation paths. Their target
  index is different from the source index; do not “cover” them by unlinking the
  source lock. No whole-module guarantee is valid while ignoring these paths.

Scope the initial implementation to the four Web Hub mutation operations, their
TaskAutoCommit reuse, and making observational commands lockless. Worktree target
ownership/containment is a separately enumerated gap, not silently covered by
`writes_git_index`, whose first-argument list (350–357) omits worktree.

## 3. Concrete API and ownership model

Add private `control/git/index_lock.rs`, imported from `git.rs`. No public API
change, dependency change, or HTTP response-schema change. Sketch signatures:

```rust
struct RepoWriteContext {
    worktree: PathBuf,
    git_dir: PathBuf,
    index_lock: PathBuf,
    mutex: Arc<tokio::sync::Mutex<()>>,
}

struct FileIdentity { /* platform identity, size, modified time */ }
struct LockSnapshot { identity: FileIdentity, observed_at: std::time::SystemTime }

enum LockProvenance {
    Unknown,
    ObservedOwned(OwnedLockReceipt),
}

struct OwnedLockReceipt { /* private: retained file handle + owned child identity */ }
enum WriterEvidence { Live, NoVisibleWriter, Unavailable }
enum RemovalOutcome { Removed, AlreadyAbsent }

async fn resolve_write_context(path: &Path) -> Result<RepoWriteContext>;
fn snapshot_lock(path: &Path) -> Result<Option<LockSnapshot>>;
fn inspect_writers(context: &RepoWriteContext) -> WriterEvidence;
fn observe_owned_lock(context: &RepoWriteContext, child: &OwnedGitChild)
    -> Result<Option<OwnedLockReceipt>>;
async fn recover_stale_index_lock(context: &RepoWriteContext) -> Result<()>;
async fn cleanup_owned_index_lock(
    context: &RepoWriteContext,
    receipt: &OwnedLockReceipt,
    termination: &ConfirmedTreeExit,
) -> Result<RemovalOutcome>;
```

`OwnedGitChild` owns the actual Child, its process/container identity, and pipe
readers. `ConfirmedTreeExit` has a private constructor; killing a parent or merely
requesting kill does not construct it. `OwnedLockReceipt` is not constructed from
mtime, absence-before-spawn, command arguments, stderr, or the operation UUID.
Use enums for unavailable evidence; avoid optional booleans.

**Actual provenance acquisition:** on Linux only, while a tracked child is live,
inspect its `/proc/<pid>/fd` handles. Match an open regular-file handle to the
resolved lock by device/inode, retain a duplicate/read handle when permitted,
and bind the receipt to the still-owned child/process incarnation. Include
contained descendants if the process-tree backend can establish membership.
A fixture can do the same with a child deliberately holding the lock open.

This is positive evidence that the owned tree held this inode; it is not evidence
that every future file appearing at the pathname is owned. If the process exits
before observation, permission denies observation, or identity changes, provenance
stays Unknown. Polling can miss an entire lock lifetime. It therefore enables a
conditional H1 fix, not the requested universal H1 guarantee. macOS/Windows have
no positive receipt backend in this minimal design; return unavailable, never
synthesize ownership. A fabricated receipt constructor is allowed only in private
unit-test fixtures and cannot justify a production guarantee.

At entry, resolve context, acquire its index-keyed mutex, recover any pre-existing
lock, and only then spawn a mutation. Call sites become:

```rust
async fn stage_locked(context: &RepoWriteContext, paths: &[String], cancel: &Cancellation)
    -> Result<GitStatusResponse>;
// Same context/cancellation parameters for unstage_locked, discard_locked,
// discard_one, and commit_locked.
async fn run_git_write(context: &RepoWriteContext, label: &str, args: &[&str],
    timeout: Duration, cancel: &Cancellation) -> Result<GitOutput>;
```

Keep `run_git` for observational commands, with no implicit unlink behavior.
`run_git_write` explicitly owns lifecycle/provenance. Remove the cleanup decisions
based on `writes_git_index` from generic `run_git`; a subcommand classifier cannot
prove either the affected index or creation ownership. Before each child, check
for an intervening unknown lock; never adopt it into an operation's receipt.

## 4. Policy by outcome

| Situation | Required action |
| --- | --- |
| Own timeout, positive receipt, tree exit confirmed | Remove the matching orphan without the 10s age test; finish cleanup before releasing serialization. Preserve `git add timed out` / `git commit timed out`. |
| Own ordinary nonzero exit, positive receipt | Same cleanup; preserve original Git stderr/fallback as primary error. |
| Own successful exit, matching stray lock | Same cleanup, but only with the same provenance/termination checks. Commit remains successful after Git reports success; cleanup failure becomes a security/error log and retained recovery state, not a false “commit failed” response. |
| Spawn fails before a child exists | No owned cleanup; preserve spawn error. |
| Child wait/drain fails | Reap/contain the tree, then apply the same owned cleanup policy; no early `?` bypass. |
| Future cancelled/dropped | Signal the supervisor; supervisor terminates/reaps its child and completes cleanup independently of caller lifetime. |
| Fresh unknown lock (<10s) | Refuse mutation, retain lock, return helpful busy/ownership-unverified error. |
| Aged unknown lock (>=10s) | Age permits investigation only. Without positive abandonment evidence plus exclusion, refuse removal. Empty contents and no visible process are insufficient. |
| Detection unavailable, unreadable processes, WSL cross-OS uncertainty | Refuse unknown removal; positive owned receipt is usable only if its backend can also prove tree termination and namespace exclusion. |
| Symlink/non-regular lock | Refuse, do not follow/unlink it. |
| Metadata/identity/clock error | Refuse with path and reason; only NotFound means absence. |
| Unlink fails after retries | Retain recovery state and report/log real OS error; do not silently claim cleanup succeeded. |

Current recovery treats all symlink_metadata errors as absence (127–129), and
cleanup silently discards path/metadata/removal errors (415–436). Correct those
behaviors. On failure append cleanup refusal to the original message only when
needed; keep the original phase/stderr first. HTTP already turns errors into
400 with JSON `error` text (`control/http.rs:149–153`); git_commit uses that at
1345–1353. No HTTP code change is needed.

Suggested refusal text (retain the existing tested prefix):

> Another git operation is in progress, or ownership of the index lock could not
> be verified: <path>. Automatic removal was refused (<reason>). Retry after
> other Git operations finish. If this persists, stop Git in terminals, editors,
> other servers, and Windows tools when using WSL; verify the lock is abandoned
> before removing this exact file.

Distinguish `lock is fresh`, `live writer detected`, `process inspection unavailable`,
`lock identity changed`, and `filesystem error`. Do not promise that simply waiting
10 seconds will authorize unknown-lock removal.

## 5. Safety predicate, process evidence, and unlink boundary

For an unknown lock, neither size zero nor index-like bytes encode an owner PID.
Do not parse the first integer in index data as a PID; do not write an ownership
marker into Git's index data. Bytes may be empty or a partly/fully written index.
No content-based branch authorizes deletion.

Replace bool process detection with tri-state evidence. Linux diagnostic scan:

- Enumerate processes without silently flattening errors; disappearance is normal
  only if exit can be established. EACCES/EPERM or an incomplete scan => Unavailable.
- Inspect executable/comm/cmdline, including `git`, `git.exe`, and `git-*` helpers;
  comm alone is neither identity nor proof that a writer is absent.
- Parse NUL-separated argv, not shell text. Interpret repeated `-C dir` / `-Cdir`
  in order relative to cwd, and `--git-dir value` / `--git-dir=value`,
  `--work-tree` forms, and index overrides from readable environment.
- Match canonical worktree, administrative Git directory and actual index resource,
  including linked worktrees and commands launched outside the worktree. A cwd
  comparison is a positive conservative veto, not a complete location resolver.
- Hooks/helpers with non-Git names, wrappers, inherited descriptors, other UIDs,
  namespaces, remote filesystems, and Windows writers can defeat negative scans.
  On macOS/Windows return Unavailable until a separately tested native backend
  exists. On WSL a negative Linux scan never proves cross-OS absence.
- `Live` vetoes removal. `NoVisibleWriter` is diagnostic, not `ProvenAbandoned`.
  An unrelated lockless `git log` need not block a *positively owned* recovery,
  but relaxing that veto cannot make an unknown lock safe to delete.

For positive owned provenance, sample the final identity after confirmed tree
termination. Unix identity includes dev+ino, length, mtime (subsecond), and
optionally ctime; hold an open file handle to prevent inode reuse. On Windows use
volume/file identity only if supported by the selected toolchain/native backend;
size+mtime alone is not a substitute. Missing identity support refuses removal.
The mtime sampled during writing is allowed to change before the final snapshot;
never confuse expected child writes with pathname replacement.

Checked retry algorithm (replace `remove_index_lock`, 148–161):

1. Require authorized provenance and established exclusion; absent lock => success.
2. For every attempt, including retries, lstat and compare the full final snapshot
   and regular-file type. Replacement/type/mtime change => immediate refusal.
3. With no await between comparison and unlink, remove the checked pathname.
4. NotFound => idempotent success. Retry only transient sharing/permission errors
   using existing 10 attempts and 100ms delay, revalidating at the next attempt.
   Return the actual OS error/path on exhaustion; an identity mismatch is not
   retried against a new snapshot.
5. Never chase replacements after successful removal. A subsequent legitimate
   external writer may create a new lock; that is not our orphan.

**Critical qualification:** step 3 is race-reduced, not a TOCTOU-safe conditional
unlink against arbitrary actors. No atomic compare-and-unlink facility is supplied
by this code or its portable std API. Keep the exclusion requirement explicit:
all participating writers must honor the extant lock, and no actor may unlink/
replace it behind the owner. Unknown-lock recovery has no such exclusion proof
and therefore cannot use this algorithm automatically. Do not ship a test that
injects replacement only before lstat and call that proof of atomicity.

## 6. One discovery subprocess, zero cleanup subprocesses (H10)

Today `ensure_repo` spends one READ_TIMEOUT (511–523); recovery adds index lookup
(63–83, 125–137), and orphan cleanup repeats lookup plus toplevel (414–433).
Replace ensure+discovery in the four guarded operations with one `rev-parse`:

```text
rev-parse --is-inside-work-tree --path-format=absolute --show-toplevel \
          --absolute-git-dir --git-path index.lock
```

Validate `true`, absolute paths and expected output shape; reject ambiguous output
rather than trimming it into a different path. Verify supported Git versions and
output ordering with real repositories before implementation acceptance; do not
add a second discovery fallback. Preserve `Not a git repository` for invalid repos.
Test spaces, Unicode, nested project directories, linked worktrees and gitfiles.
Embedded-newline discovery output needs either an unambiguous parser or explicit
refusal; do not silently reinterpret it. No hand-built `path/.git/index.lock`.

Canonicalize the existing parent directory and join the exact lock basename; key
`repo_write_mutex` by that resource, not the user-supplied subdirectory. This
coordinates project aliases and TaskAutoCommit against the same index. Linked
worktrees retain distinct per-worktree indexes; common-dir mutation is separate.
Discovery occurs before the mutex and starts no writer; all ownership checks occur
again after acquiring it. Context stays owned by the supervisor until cleanup ends.

Reuse context for stage's add, unstage's restore, every discard_one call, and
commit's add/commit. Cleanup, cancellation, and recovery do no rev-parse/toplevel
subprocesses. Keep `repo_toplevel` only if other callers still need it; remove the
boxed recursion workaround when no longer needed. Do not re-resolve context in
`run_git_write` or Drop.

No timeout constant or client budget changes. Discovery replaces the existing
30s ensure-repo phase, so the documented core subtotal remains
30 + 60 + 120 + 30 = 240s, with the existing 300s client allowance (165–173).
Do not retain ensure_repo and add another 30s discovery phase: that would make
270s before cleanup. Repo admission, tree termination, filesystem I/O/retries and
response overhead remain outside the stated core-subprocess subtotal; neither
240s nor 300s becomes a hard whole-operation bound. No new claim that all queues
and cleanup fit inside the 60s margin. Repeated cleanups no longer inflate the
subprocess subtotal. Other route budgets remain outside this design.

## 7. Cancellation and child containment

Use one detached supervisor per public mutation, rather than spawning async unlink
from Child::drop. Add `WriteOperation` (owned Stage/Unstage/Discard/Commit arguments)
and `execute_write_operation(path: PathBuf, operation: WriteOperation)`.
Public signatures stay unchanged; clone their borrowed arguments before spawn.

The request owns a small cancellation lease backed by `tokio::sync::watch<bool>`;
its Drop sends true synchronously. The spawned task owns context, Child, provenance,
recovery state and the write guard. Awaiting its JoinHandle obtains the result;
dropping that handle does not request task abort. The lease disarms after normal
completion. Record/report a supervisor panic; do not unwrap JoinError.

- Before acquiring the mutex, select cancellation against admission. A cancelled
  queued request starts no mutation. Check cancellation again before each child.
- Inside `run_git_write`, select cancellation/deadline against pipe drains and
  child wait while retaining ownership of Child outside the selected future.
  Do not select/drop the entire `*_locked` future once a child may be alive.
- Spawn and arm lifecycle tracking without an intervening await. Every completion,
  timeout, wait failure and cancellation funnels through one finalizer.
- Cancellation after a child starts means terminate/reap, then cleanup; do not
  proceed to commit after cancellation during the preceding add phase.
- Cancellation during cleanup does not cancel cleanup. A cancellation lease cannot
  interrupt the finalizer. No repository mutex is reacquired in the finalizer;
  it already owns the original guard. Keep that guard until finalization ends.
- The detached task already has the absolute lock path and any observed receipt;
  it needs no asynchronous discovery after caller cancellation. If no receipt was
  observed, cancellation does not magically create provenance.
- Set `GIT_OPTIONAL_LOCKS=0` in this module's command setup so observational status
  (710–717 and 1287–1301) cannot opportunistically create index locks outside this
  lifecycle. Required mutation locks remain Git's responsibility. Preserve
  `git_command()` and its fsmonitor/bare-repo security flags (`subprocess.rs:34–44`).

For Unix, use a separate owned process group, terminate its contained descendants,
and wait for the direct child plus verified group quiescence before authorizing
cleanup. Reuse the established process-group pattern in `subprocess.rs:68–73`,
without changing the global subprocess helper. A group kill alone does not prove
all descendants died or that a hook could not escape; failed proof retains lock.
Windows requires a kill-on-close Job Object with assignment before the child can
spawn descendants; `kill_on_drop` and `taskkill` polling are insufficient proof.
The existing winapi feature list (`Cargo.toml:240–243`) has no job-object feature;
this plan does not silently expand dependencies/features to invent that backend.
Until a reviewed backend exists, Windows tree proof is unavailable.

Termination failure must yield a refusal/pending-recovery record, not unbounded
mutex ownership or an unlink under a possibly live tree. Keep the Child under a
separate reaper owner if wait fails; retain its resource state so later writes
refuse admission while that owned writer may still be alive. Successful reaping
can clear admission state. Permanent OS denial still needs operator intervention.
Graceful server shutdown must drain supervisors before runtime teardown; request
cancellation tests do not establish crash or runtime-shutdown guarantees.

## 8. Concrete tests and acceptance limits

Prefer new `crates/openduck/tests/git_index_lock.rs` for public real-repo behavior.
Keep private provenance, race-injection, child timeouts and discovery instrumentation
in the new module's unit tests; do not export internals just to test them.
`futures` is already a normal dependency (`Cargo.toml:98`), not an explicit
**dev-dependency** (257–274); tests can use it without a dependency change. Tokio
process/sync/time features are available at 101, tempfile at 174, and Unix libc at
254–255. Rust edition is 2021 (`Cargo.toml` workspace root:10).

| Test name | Setup and assertions | Placement |
| --- | --- | --- |
| `owned_fresh_lock_removed_after_timeout` | Contained child creates/holds exact lock open, signals readiness, observer obtains a real receipt, child refreshes mtime immediately before forced timeout. Reap tree, remove lock despite age <10s, preserve phase error, next mutation succeeds. | Private lifecycle test; Linux backend. |
| `owned_lock_removed_after_nonzero_exit` | Receipt-bearing fixture exits nonzero with stderr; same cleanup, original stderr stays first. | Private lifecycle test. |
| `successful_child_stray_owned_lock_is_removed` | Fixture exits zero leaving observed lock; remove it; successful result remains successful. | Private lifecycle test. |
| `unobserved_child_lock_is_not_assumed_owned` | Child exits before receipt collection; lock remains Unknown and is refused. Captures the remaining universal-H1 gap. | Unit test. |
| `external_writer_wins_after_empty_preflight` | Synchronize external lock creation between preflight and server spawn; server fails, external inode/bytes remain intact. | Real repo integration plus private scheduling barrier. |
| `fresh_unknown_lock_refused_with_path` | Manually write bytes in temp repo; public stage/commit refuses with existing busy prefix, exact path/reason, unchanged bytes. | Integration; all platforms. |
| `aged_unknown_lock_without_provenance_is_refused` | Backdate empty and index-like files using FileTimes, no visible writer; neither is automatically removed. This deliberately revises the unsafe old expectation. | Integration; all platforms. |
| `aged_proven_abandoned_lock_removed_under_exclusion` | Receipt-backed, terminated fixture in exclusively controlled namespace; backdate lock, allow unrelated lockless process; remove only matching inode. Tests the authorized predicate, not arbitrary unknown recovery. | Private fixture test. |
| `live_external_lock_never_removed` | Real external commit hook waits on barrier while holding index lock; backdate lock; server refuses, external writer finishes successfully after barrier. | Unix real-repo integration. |
| `writer_scan_resolves_git_C_and_git_dir` | argv fixtures for separated/attached/repeated -C, --git-dir forms, helper comm, cwd elsewhere, environment overrides, aliases and worktrees. Live/Unavailable veto deletion. | Private parser/evidence tests; Linux real-process supplement. |
| `unavailable_detection_never_authorizes_unlink` | Inject EACCES, incomplete /proc, non-Linux and WSL uncertainty; aged/fresh lock survives. | Unit, native platform integration when available. |
| `replacement_before_retry_is_refused` | Initial unlink fails transiently; replace lock at retry barrier, including same length/mtime with different inode. Next attempt refuses; replacement bytes survive. | Unit with real files and deterministic barrier. |
| `changed_lock_after_scan_is_refused` | Replace after evidence collection but before final lstat; no unlink of replacement. Explicitly does not prove atomic compare-and-unlink. | Unit. |
| `cancelled_mutation_finalizes_before_next_writer` | Poll operation to fixture-ready barrier; drop Box::pin future / timeout outer request, assert cancellation delivered, owned tree exits, observed lock disappears, queued next mutation proceeds only after cleanup. | Private supervisor test; integration variant with actual hook. |
| `cancelled_cleanup_continues` | Drop caller while retry loop is waiting; supervisor remains alive, completes authorized cleanup, releases guard. | Unit. |
| `cancelled_before_admission_starts_no_child` | Hold index mutex, enqueue operation, cancel request, then release mutex; child count stays zero. | Unit. |
| `cleanup_has_zero_discovery_commands` | Per-operation command recorder: one discovery, add/commit/log as applicable; zero discovery during finalizer, timeout, cancellation or retries. | Unit; real Git Trace2 fixture can supplement. |
| `subdirectory_aliases_share_index_mutex` | Root and subdirectory operations resolve same key; serialization barrier proves no overlap. Linked worktree has its own correct lock. | Real-repo integration plus private barrier. |
| `observational_status_does_not_take_index_lock` | Refresh-needed repo, foreign lock present; status and TaskAutoCommit capture do not remove it or create a competing optional lock. | Integration. |

Manually writing `.git/index.lock` is valid for unknown-lock tests, but never
establishes server-child ownership. Use a private child fixture that creates a
file with create_new, keeps an open descriptor, signals readiness and waits.
A real Git hook gives public-path coverage: verify actual index.lock existence
and observed handle rather than assuming every hook/Git version holds it. Kill
and reap fixture descendants in test teardown; never leak a long-running hook.
Tests must use short injected lifecycle deadlines, not alter production constants.

For cancellation, `tokio::time::timeout` around the public operation is sufficient
once the readiness barrier is crossed. For explicit poll/drop use a scoped owned
`Box::pin` and `futures::poll!`; dropping only the Pin reference created by
`tokio::pin!` does not necessarily drop the underlying future. Assert eventual
cleanup with a bounded notification/deadline, not an arbitrary sleep.

Retain existing phase-error test (1808–1816), stderr test (1818–1825), fresh-lock
prefix test (1860–1875), and commit roundtrip (1877 onward). The existing aged
unknown recovery test (1848–1858) encodes the unsafe policy and must be rewritten
as the two distinct aged tests above; silently preserving it would reintroduce H8.
Retain `tests/task_auto_commit.rs:43–83` staged-change preservation/attribution and
`tests/git_command_security.rs:22–84` bare-repo/fsmonitor protections.

The requested “aged unknown removed when no writer is found” test cannot safely
pass under the strict cross-platform contract. Neither a lockless-process fixture
nor a successful /proc scan proves the absence of an invisible writer. Likewise,
there is no valid test claiming elimination of the final lstat/unlink race without
a stronger exclusion/atomic mechanism. Keep these as explicitly unmet criteria,
not ignored tests marked passing through an injected fictional proof backend.

Future implementation verification (not executed for this design-only task):

```bash
source bin/activate-hermit
cargo fmt
cargo test -p openduck --test git_index_lock
cargo test -p openduck --test git_command_security
cargo test -p openduck --test task_auto_commit
cargo test -p openduck control::git
cargo clippy --all-targets -- -D warnings
```

Update the Git phase of `openduck-self-test.yaml:163–172` with public busy-lock and
cancel/retry scenarios; once build/test work is authorized, rebuild and execute
`openduck run --recipe openduck-self-test.yaml` per repository rules. Platform
claims require native macOS/Windows/WSL validation, not Linux cfg simulation alone.

## 9. File ownership and two sequential implementation chunks

These are implementation assignments only; this planning task spawns no agents.
Both chunks add no crates/dependencies and use anyhow::Result. They can be authored
independently against the API above, then applied **A first, B second**. B consumes
A's module API; the chunks do not edit the same existing source file.

**Agent A — lock evidence and checked-removal module:**

- Add only `crates/openduck/src/control/git/index_lock.rs`; keep its private unit
  tests in that file. Add FileIdentity/LockSnapshot/LockProvenance/receipt types,
  tri-state writer inspection, Linux argv/evidence parsing and owned-handle
  observation, fail-closed preflight, authorized checked-retry removal.
- Keep platform proof unavailable where unsupported; do not make the fallback
  return “no writer.” No discovery subprocess in this module. Receive all paths,
  child identities and termination proof through its API.
- No changes to git.rs, Cargo.toml, Cargo.lock, HTTP, client, or recipe. A's file
  is intentionally unwired until B; A alone makes no behavioral guarantee.

**Agent B — operation supervisor, discovery and public regressions:**

- Modify only `crates/openduck/src/control/git.rs`: import A's module; add
  resolve_write_context, WriteOperation, cancellation lease, supervisor,
  OwnedGitChild finalization; key repo_write_mutex by index resource; replace
  guarded ensure/recovery with one discovery; pass context through the four
  locked helpers and discard_one; add run_git_write and lockless read setup.
- Remove obsolete index_lock_path/has_live_git_process_in/process_covers_root/
  cleanup_orphaned_index_lock/remove_index_lock implementations after replacing
  all references. Modify reap_timed_out_git into shared confirmed finalization;
  remove writes_git_index if unused. Keep generic run_git safe on read errors.
- Retain unrelated public Git functions and completed subtask-2 behavior. Add
  supervisor/command-count tests to existing git.rs tests; replace unsafe aged
  recovery expectation. Add `crates/openduck/tests/git_index_lock.rs` and extend
  only the Git section of `openduck-self-test.yaml`.
- Preserve all existing uncommitted changes. No http.rs/subprocess.rs/Cargo/UI
  changes are required by these two chunks. Run formatting and the agreed checks
  after integrating A+B, when implementation verification is authorized.

## 10. Decision required before claiming full completion

A+B eliminate cancellation bypass for supervised Web Hub operations and repeated
cleanup discovery; they prevent guessed ownership and fail-open foreign deletion.
Positive receipt plus confirmed tree exit fixes fresh-orphan cleanup **where that
receipt/exclusion can be established**. They do not guarantee that every Git-created
lock is observed, safely reclaim arbitrary old foreign locks, or provide portable
atomic conditional unlink. They can refuse subsequent mutations instead of risking
another writer's index; that intentionally does not meet “never blocks itself
forever” under permanent uncertainty or OS failure.

To meet the stronger goal, revise the agreed design in one of these concrete ways:

1. Establish an exclusive managed-repository boundary covering every writer, then
   use supervisor lifecycle plus owned receipts and controlled namespace recovery.
   A server-only mutex or advisory lock that terminal Git ignores is insufficient;
   external Git must be excluded or explicitly participate.
2. Introduce a reviewed isolated-index transaction architecture: server creates a
   shared index reservation with create_new, runs Git on a private index, and
   publishes under that reservation with crash recovery and platform containment.
   This supplies stronger creation ownership but changes commit/hook/partial-index
   semantics and still cannot identify arbitrary pre-existing unknown owners.
   It is a material redesign, not a minimal helper refactor.
3. Accept a documented best-effort recovery policy based on age, stable identity
   and process scans. That can improve availability, but explicitly weakens the
   “never delete a possibly live external lock” requirement and is not recommended
   under the safety instruction in this task.

This document chooses the conservative safety boundary, not option 3. Do not label
subtask 3 fully satisfied until the ownership/exclusion decision resolves these
incompatible acceptance requirements. No timeout/client work needs to be reopened.
