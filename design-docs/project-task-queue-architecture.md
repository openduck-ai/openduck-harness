# Project Task Queue Architecture & Specification

| Metadata | Details |
|---|---|
| **Feature** | Project-Level Task Queue (FIFO Concurrency Control) |
| **Status** | Implemented & Active |
| **Modules** | `crates/openduck/src/control/project_harness.rs`, `crates/openduck/src/control/harness.rs`, `ui/hub/src/components/ProjectHarnessTab.tsx` |
| **Scope** | Enterprise Agent Harness, Web Hub, CLI Task Runner |

---

## 1. Background & Motivation

When an agent runs a task within a project harness, it routinely performs non-idempotent modifications to the project repository:
- Creating or checking out git branches / worktrees
- Modifying, creating, or deleting project files
- Executing compiler or test toolchains (e.g. `cargo test`, `npm test`, `pytest`)
- Writing local checkpoint, cassette, and trajectory files

### Why Strict Per-Project FIFO Serialization is Required
1. **Prevent Workspace Race Conditions**: If two tasks in the same project execute concurrently, their tool invocations (editing files, executing build scripts, unstashing git diffs) will collide and corrupt the workspace state.
2. **Project-Level Independence**: Tasks across **different** projects operate on distinct physical working directories and should run in parallel without contention.
3. **Graceful Queuing UX**: Rather than rejecting subsequent task runs with an immediate error, submitting another task for an active project should queue it automatically and execute it sequentially upon completion.

---

## 2. Architectural Design

```
+─────────────────────────────────────────────────────────────────────────────────────────+
│                                  HTTP / API Layer                                       │
│          POST /api/v1/projects/{slug}/harness/tasks/{task_id}/run                       │
+─────────────────────────────────────────────────────────────────────────────────────────+
                                            │
                                            ▼
                    +───────────────────────────────────────────────+
                    │             `launch_project_task`             │
                    │   - Validates project root & task store       │
                    │   - Generates unique harness `job_id`         │
                    +───────────────────────────────────────────────+
                                            │
                                            ▼
                    +───────────────────────────────────────────────+
                    │        `ProjectTaskQueueEntry::enqueue`       │
                    │   - Locks `PROJECT_TASK_QUEUES`               │
                    │   - Evaluates active queue for `slug`         │
                    +───────────────────────────────────────────────+
                                   /                 \
                     Queue is empty                   Queue has active/pending tasks
                                 /                     \
                                ▼                       ▼
                     Status: `running`               Status: `queued`
                     Send immediate wakeup           Hold `oneshot::Sender` in queue
                                \                       /
                                 \                     /
                                  ▼                   ▼
                    +───────────────────────────────────────────────+
                    │                 Tokio Worker                  │
                    │   `ready.await` (waits if status is queued)   │
                    │   Updates status to `running` upon wake       │
                    │   Executes `execute_project_task`             │
                    +───────────────────────────────────────────────+
                                            │
                                            ▼
                               [Worker Drops `queue_entry`]
                                            │
                                            ▼
                    +───────────────────────────────────────────────+
                    │           `Drop for ProjectTaskQueueEntry`    │
                    │   - Removes self from `slug` queue            │
                    │   - If index == 0: wakes up next in line      │
                    │   - Cleans up empty project queues            │
                    +───────────────────────────────────────────────+
```

---

## 3. Data Structures & Lifecycle

### 3.1 Global Queue Registry

In `crates/openduck/src/control/project_harness.rs`:

```rust
type ProjectTaskQueue = VecDeque<(String, Option<tokio::sync::oneshot::Sender<()>>)>;

static PROJECT_TASK_QUEUES: LazyLock<std::sync::Mutex<HashMap<String, ProjectTaskQueue>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));
```

- **Key (`String`)**: Project `slug` ensuring strict project-level isolation.
- **Value (`ProjectTaskQueue`)**: FIFO `VecDeque` of `(job_id, Option<Sender<()>>)`.
  - The currently executing task sits at `index == 0` with its sender consumed (`None`).
  - Pending tasks sit at `index >= 1` with their sender preserved (`Some(tx)`).

### 3.2 RAII Guard: `ProjectTaskQueueEntry`

```rust
struct ProjectTaskQueueEntry {
    slug: String,
    job_id: String,
}
```

By binding queue occupancy to the lifetime of `ProjectTaskQueueEntry`, OpenDuck guarantees:
- **Panic Safety**: If an agent worker panics or crashes, its `Drop` implementation executes.
- **Cancellation Safety**: When an HTTP client aborts or a user triggers `stop_project_task`, dropping the worker triggers `Drop` and wakes the next task immediately.
- **Mid-Queue Cancellation**: If a task still in `queued` status is cancelled (`index > 0`), it is cleanly spliced from the `VecDeque`, closing its channel without perturbing preceding or succeeding tasks.

---

## 4. State Transitions

```
[User triggers run]
        │
        ├─► [Queue empty] ───────► Status: `running` ──► [Executes immediately]
        │                                                     │
        └─► [Queue busy]  ───────► Status: `queued`           │
                                         │                    ▼
                                  (Awaits wakeup)      [Task finishes/fails/cancelled]
                                         │                    │
                                         ▼                    ▼
                                  Status: `running` ◄── [Queue Entry Dropped]
                                         │
                                         ▼
                                  [Begins execution]
```

### Event Broadcasting (SSE)
Every state change broadcasts a `HarnessProjectEvent::TaskStatusChanged` event through the project SSE stream:
- `status: "queued"` on initial enqueue.
- `status: "running"` once dequeued and execution starts.
- Terminal statuses (`"success"`, `"failure"`, `"timeout"`, `"cancelled"`) when execution concludes.

---

## 5. Memory Pressure & Conflict Handling

1. **Duplicate Identical Task Guard**:
   If a run request is received for a `task_id` that is **already running or queued** in the same project, `try_register_active_job` rejects it with:
   - `HTTP 409 Conflict`
   - `{"error": "Task ... is already running", "code": "task_already_running"}`
   This prevents accidental burst clicks or runaway schedulers from enqueuing identical duplicate runs.

2. **System Memory Gate**:
   Before a task is enqueued or starts running, OpenDuck checks `crate::memory_gate::ensure_open()`. If system RAM or swap exceeds safe operational limits:
   - Request is refused with `HTTP 503 Service Unavailable` (`code: "memory_pressure"`).
   - Scheduled tasks record a `skipped` history item to avoid infinite retry loops.

---

## 6. Frontend Presentation (Web Hub)

1. **Active Jobs Banner** (`ui/hub/src/components/ProjectHarnessTab.tsx`):
   - Shows active and queued jobs with real-time timers:
     - `Queued · 18s` (waiting)
     - `1m 24s` (running)
   - Provides a `Cancel` action for queued tasks, which invokes `/stop` to abort the job before execution starts.
2. **Task Cards & History**:
   - Task cards reflect the queued state and prevent conflicting actions while queued.
   - Once executed, the result is saved directly into the project's persistent history store.
