import type { FriendlySchedule, ProjectTaskDefinition } from '@aaif/goose-hub-core'
import {
  clearTaskDraft,
  isMeaningfulTaskDraft,
  loadTaskDraft,
  persistTaskDraft,
  type TaskCreateDraftData,
  type TaskDraftStorage,
} from './taskCreateDraft.ts'
import {
  TASK_DRAFT_SAVE_DEBOUNCE_MS,
  taskCreatorDefaults,
  taskCreatorStateFromDraft,
  type TaskCreatorState,
} from './taskCreator.ts'

export interface TaskDraftTimer {
  setTimeout(callback: () => void, delayMs: number): unknown
  clearTimeout(handle: unknown): void
}

const defaultTimer: TaskDraftTimer = {
  setTimeout: (callback, delayMs) => setTimeout(callback, delayMs),
  clearTimeout: handle => clearTimeout(handle as ReturnType<typeof setTimeout>),
}

export interface TaskCreateDraftRestore {
  state: TaskCreatorState
  /** When the restored draft was last saved, or `null` when the creator starts from defaults. */
  restoredAt: Date | null
}

/**
 * A saved draft only comes back when it carries real input; an untouched form would otherwise
 * reopen with a "restored your draft" banner for content the user never typed.
 */
export function prepareTaskCreator(options: {
  projectId: string | null | undefined
  base: ProjectTaskDefinition
  schedule: FriendlySchedule
  storage?: TaskDraftStorage | null
  now?: Date
}): TaskCreateDraftRestore {
  const seed = { base: options.base, schedule: options.schedule }
  const projectId = options.projectId ?? ''
  const saved = projectId.trim()
    ? loadTaskDraft(projectId, { storage: options.storage, now: options.now })
    : null
  if (!saved || !isMeaningfulTaskDraft(saved)) {
    return { state: taskCreatorDefaults(options.base, options.schedule), restoredAt: null }
  }
  return {
    state: taskCreatorStateFromDraft(saved, seed),
    restoredAt: new Date(saved.updatedAt),
  }
}

export interface TaskDraftAutosave {
  /** Records the newest snapshot; writes land once typing pauses. */
  update(data: TaskCreateDraftData): void
  /** Writes the newest snapshot now, skipping the remaining debounce. */
  flush(): void
  /** Drops the stored draft and any queued write. */
  clear(): void
  /** Releases the queued timer. `flush: true` keeps the latest keystrokes when the panel closes. */
  dispose(options?: { flush?: boolean }): void
  hasPendingWrite(): boolean
}

const noopAutosave: TaskDraftAutosave = {
  update: () => {},
  flush: () => {},
  clear: () => {},
  dispose: () => {},
  hasPendingWrite: () => false,
}

/**
 * Draft writes are debounced but never dropped silently: unmount and collapse both funnel through
 * `flush`, so a crash between keystrokes only loses the current typing pause.
 */
export function createTaskDraftAutosave(options: {
  projectId: string | null | undefined
  storage?: TaskDraftStorage | null
  debounceMs?: number
  timer?: TaskDraftTimer
}): TaskDraftAutosave {
  const projectId = options.projectId ?? ''
  if (!projectId.trim()) return noopAutosave

  const storage = options.storage
  const debounceMs = options.debounceMs ?? TASK_DRAFT_SAVE_DEBOUNCE_MS
  const timer = options.timer ?? defaultTimer
  let latest: TaskCreateDraftData | null = null
  let handle: unknown = null

  const cancel = () => {
    if (handle === null) return
    timer.clearTimeout(handle)
    handle = null
  }

  const writeLatest = () => {
    cancel()
    if (!latest) return
    persistTaskDraft(projectId, latest, { storage })
  }

  return {
    update: data => {
      latest = data
      cancel()
      handle = timer.setTimeout(() => {
        handle = null
        if (latest) persistTaskDraft(projectId, latest, { storage })
      }, debounceMs)
    },
    flush: () => {
      if (!latest) {
        cancel()
        return
      }
      writeLatest()
    },
    clear: () => {
      cancel()
      latest = null
      clearTaskDraft(projectId, { storage })
    },
    dispose: disposeOptions => {
      if (disposeOptions?.flush) {
        writeLatest()
        return
      }
      cancel()
    },
    hasPendingWrite: () => handle !== null,
  }
}
