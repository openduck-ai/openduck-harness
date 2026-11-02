import assert from 'node:assert/strict'
import test from 'node:test'
import type { ProjectTaskDefinition } from '@aaif/goose-hub-core'
import {
  createTaskDraftAutosave,
  prepareTaskCreator,
  type TaskDraftTimer,
} from './taskCreateDraftFlow.ts'
import {
  defaultTaskCreateDraftData,
  loadTaskDraft,
  saveTaskDraft,
  TASK_DRAFT_STORAGE_PREFIX,
  taskDraftStorageKey,
  type TaskCreateDraftData,
  type TaskDraftStorage,
} from './taskCreateDraft.ts'
import type { TaskCreatorState } from './taskCreator.ts'

function memoryStorage(): TaskDraftStorage & { entries: Map<string, string> } {
  const entries = new Map<string, string>()
  return {
    entries,
    getItem: key => entries.get(key) ?? null,
    setItem: (key, value) => {
      entries.set(key, value)
    },
    removeItem: key => {
      entries.delete(key)
    },
  }
}

function controlledTimer() {
  let nextHandle = 1
  const queued = new Map<number, () => void>()
  const timer: TaskDraftTimer = {
    setTimeout: callback => {
      const handle = nextHandle++
      queued.set(handle, callback)
      return handle
    },
    clearTimeout: handle => {
      queued.delete(handle as number)
    },
  }
  return {
    timer,
    pending: () => queued.size,
    runQueued: () => {
      const callbacks = [...queued.values()]
      queued.clear()
      for (const callback of callbacks) callback()
    },
  }
}

const baseTask = (): ProjectTaskDefinition => ({
  id: 'task-123456',
  name: '',
  category: '',
  tags: [],
  prompt: '',
  maxTurns: 25,
  timeoutSeconds: 300,
})
const baseSchedule = { kind: 'none' as const, hour: 9, minute: 0, weekday: 0 }

function draftWith(overrides: Partial<TaskCreateDraftData['form']> = {}): TaskCreateDraftData {
  const base = defaultTaskCreateDraftData()
  return { ...base, form: { ...base.form, ...overrides } }
}

test('prepareTaskCreator starts from defaults when the project has no draft', () => {
  const storage = memoryStorage()
  const result = prepareTaskCreator({
    projectId: 'proj-alpha',
    base: baseTask(),
    schedule: baseSchedule,
    storage,
  })
  assert.equal(result.restoredAt, null)
  assert.equal(result.state.taskForm.prompt, '')
  assert.equal(result.state.verifierType, 'none')
  assert.equal(result.state.section, 'basics')
})

test('prepareTaskCreator restores a saved draft and reports when it was saved', () => {
  const storage = memoryStorage()
  const savedAt = new Date('2026-01-01T10:00:00Z')
  saveTaskDraft(
    'proj-alpha',
    draftWith({ name: 'Disk pressure check', prompt: 'Check the disk usage report' }),
    { storage, now: savedAt },
  )

  const result = prepareTaskCreator({
    projectId: 'proj-alpha',
    base: baseTask(),
    schedule: baseSchedule,
    storage,
    now: new Date(savedAt.getTime() + 60_000),
  })
  assert.equal(result.state.taskForm.name, 'Disk pressure check')
  assert.equal(result.state.taskForm.prompt, 'Check the disk usage report')
  assert.deepEqual(result.restoredAt, savedAt)
})

test('prepareTaskCreator ignores an untouched draft so no banner appears', () => {
  const storage = memoryStorage()
  saveTaskDraft('proj-alpha', draftWith(), { storage })

  const result = prepareTaskCreator({
    projectId: 'proj-alpha',
    base: baseTask(),
    schedule: baseSchedule,
    storage,
  })
  assert.equal(result.restoredAt, null)
  assert.equal(result.state.taskForm.prompt, '')
})

test('prepareTaskCreator skips storage when there is no project yet', () => {
  const storage = memoryStorage()
  saveTaskDraft('proj-alpha', draftWith({ prompt: 'hello' }), { storage })

  const result = prepareTaskCreator({
    projectId: null,
    base: baseTask(),
    schedule: baseSchedule,
    storage,
  })
  assert.equal(result.restoredAt, null)
  assert.equal(result.state.taskForm.prompt, '')
})

test('autosave coalesces rapid edits into a single write after the debounce', () => {
  const storage = memoryStorage()
  const { timer, pending, runQueued } = controlledTimer()
  const autosave = createTaskDraftAutosave({ projectId: 'proj-alpha', storage, debounceMs: 600, timer })

  autosave.update(draftWith({ prompt: 'a' }))
  autosave.update(draftWith({ prompt: 'ab' }))
  autosave.update(draftWith({ prompt: 'abc' }))

  assert.equal(pending(), 1)
  assert.equal(storage.entries.size, 0)
  assert.equal(autosave.hasPendingWrite(), true)

  runQueued()
  assert.equal(storage.entries.size, 1)
  assert.equal(loadTaskDraft('proj-alpha', { storage })?.form.prompt, 'abc')
  assert.equal(autosave.hasPendingWrite(), false)
})

test('autosave clears the stored draft when the user erases every field', () => {
  const storage = memoryStorage()
  const { timer, runQueued } = controlledTimer()
  const autosave = createTaskDraftAutosave({ projectId: 'proj-alpha', storage, timer })

  autosave.update(draftWith({ prompt: 'typed something' }))
  runQueued()
  assert.ok(loadTaskDraft('proj-alpha', { storage }))

  autosave.update(draftWith())
  runQueued()
  assert.equal(loadTaskDraft('proj-alpha', { storage }), null)
  assert.equal(storage.entries.size, 0)
})

test('flush persists the newest snapshot without waiting for the debounce', () => {
  const storage = memoryStorage()
  const { timer, pending } = controlledTimer()
  const autosave = createTaskDraftAutosave({ projectId: 'proj-alpha', storage, timer })

  autosave.update(draftWith({ prompt: 'half typed' }))
  autosave.flush()

  assert.equal(pending(), 0)
  assert.equal(loadTaskDraft('proj-alpha', { storage })?.form.prompt, 'half typed')
})

test('clear drops the stored draft and any queued write so it cannot come back', () => {
  const storage = memoryStorage()
  const { timer, pending, runQueued } = controlledTimer()
  const autosave = createTaskDraftAutosave({ projectId: 'proj-alpha', storage, timer })

  autosave.update(draftWith({ prompt: 'queued' }))
  autosave.clear()

  assert.equal(pending(), 0)
  runQueued()
  assert.equal(loadTaskDraft('proj-alpha', { storage }), null)
  assert.equal([...storage.entries.keys()].some(key => key.startsWith(TASK_DRAFT_STORAGE_PREFIX)), false)
})

test('dispose with flush keeps the last keystrokes, dispose without flush discards them', () => {
  const storage = memoryStorage()
  const { timer } = controlledTimer()

  const flushed = createTaskDraftAutosave({ projectId: 'proj-alpha', storage, timer })
  flushed.update(draftWith({ prompt: 'survives' }))
  flushed.dispose({ flush: true })
  assert.equal(loadTaskDraft('proj-alpha', { storage })?.form.prompt, 'survives')

  const discarded = createTaskDraftAutosave({ projectId: 'proj-beta', storage, timer })
  discarded.update(draftWith({ prompt: 'dropped' }))
  discarded.dispose()
  assert.equal(loadTaskDraft('proj-beta', { storage }), null)
})

test('a project-scoped autosave never writes without a project id', () => {
  const storage = memoryStorage()
  const { timer } = controlledTimer()
  const autosave = createTaskDraftAutosave({ projectId: null, storage, timer })

  autosave.update(draftWith({ prompt: 'orphan' }))
  autosave.flush()

  assert.equal(storage.entries.size, 0)
  assert.equal(autosave.hasPendingWrite(), false)
})

test('drafts stay isolated per project so one project never restores another', () => {
  const storage = memoryStorage()
  const { timer, runQueued } = controlledTimer()
  const alpha = createTaskDraftAutosave({ projectId: 'proj-alpha', storage, timer })
  const beta = createTaskDraftAutosave({ projectId: 'proj-beta', storage, timer })

  alpha.update(draftWith({ prompt: 'alpha draft' }))
  beta.update(draftWith({ prompt: 'beta draft' }))
  runQueued()

  assert.equal(loadTaskDraft('proj-alpha', { storage })?.form.prompt, 'alpha draft')
  assert.equal(loadTaskDraft('proj-beta', { storage })?.form.prompt, 'beta draft')
  assert.equal(loadTaskDraft('proj-gamma', { storage }), null)
})

test('inline create lifecycle: type, crash, reopen, restore, submit, reopen clean', () => {
  const storage = memoryStorage()
  const { timer, runQueued } = controlledTimer()
  const projectId = 'proj-alpha'
  const key = taskDraftStorageKey(projectId)

  const autosave = createTaskDraftAutosave({ projectId, storage, timer })
  autosave.update(
    draftWith({ name: 'Nightly audit', prompt: 'Audit the nightly build', tagsInput: 'ci, audit' }),
  )
  autosave.dispose({ flush: true })

  const reopened = prepareTaskCreator({ projectId, base: baseTask(), schedule: baseSchedule, storage })
  assert.notEqual(reopened.restoredAt, null)
  assert.equal(reopened.state.taskForm.name, 'Nightly audit')
  assert.equal(reopened.state.taskForm.prompt, 'Audit the nightly build')
  assert.equal(reopened.state.taskTagsInput, 'ci, audit')

  const resubmitted = createTaskDraftAutosave({ projectId, storage, timer })
  resubmitted.update(draftWith({ prompt: 'Audit the nightly build' }))
  resubmitted.clear()
  runQueued()
  assert.equal(storage.entries.has(key), false)

  const afterSubmit = prepareTaskCreator({ projectId, base: baseTask(), schedule: baseSchedule, storage })
  assert.equal(afterSubmit.restoredAt, null)
  assert.equal(afterSubmit.state.taskForm.name, '')
  assert.equal(afterSubmit.state.taskForm.prompt, '')
})

test('restored state keeps the section and collapse the user had open', () => {
  const storage = memoryStorage()
  const savedAt = new Date('2026-01-01T10:00:00Z')
  const draft = draftWith({ prompt: 'deep in the verifier section' })
  draft.ui = {
    section: 'verifier',
    expanded: { basics: false, prompt: true, schedule: true, verifier: true, subtasks: false },
  }
  saveTaskDraft('proj-alpha', draft, { storage, now: savedAt })

  const result = prepareTaskCreator({
    projectId: 'proj-alpha',
    base: baseTask(),
    schedule: baseSchedule,
    storage,
    now: new Date(savedAt.getTime() + 60_000),
  })
  assert.equal(result.state.section, 'verifier')
  assert.equal(result.state.expanded.basics, false)
  assert.equal(result.state.expanded.subtasks, false)
})

test('reopening a restored draft does not reset the age shown in the banner', () => {
  const storage = memoryStorage()
  const { timer, runQueued } = controlledTimer()
  const savedAt = new Date(Date.now() - 20 * 60_000)

  saveTaskDraft(
    'proj-alpha',
    draftWith({ name: 'Disk pressure check', prompt: 'Check the disk usage report' }),
    { storage, now: savedAt },
  )

  const reopened = prepareTaskCreator({
    projectId: 'proj-alpha',
    base: baseTask(),
    schedule: baseSchedule,
    storage,
  })
  assert.deepEqual(reopened.restoredAt, savedAt)

  /* Opening the panel re-runs the autosave effect with content identical to the stored draft. */
  const autosave = createTaskDraftAutosave({ projectId: 'proj-alpha', storage, timer })
  autosave.update(draftWith({ name: 'Disk pressure check', prompt: 'Check the disk usage report' }))
  runQueued()

  const again = prepareTaskCreator({
    projectId: 'proj-alpha',
    base: baseTask(),
    schedule: baseSchedule,
    storage,
  })
  assert.deepEqual(again.restoredAt, savedAt)
})

test('a session that never types leaves a stored draft intact when it closes', () => {
  const storage = memoryStorage()
  const { timer } = controlledTimer()

  const creating = createTaskDraftAutosave({ projectId: 'proj-alpha', storage, timer })
  creating.update(draftWith({ prompt: 'draft from the create panel' }))
  creating.flush()

  /* Editing an existing task opens the same panel, but its edits are not a create draft. */
  const editing = createTaskDraftAutosave({ projectId: 'proj-alpha', storage, timer })
  editing.dispose({ flush: true })

  assert.equal(loadTaskDraft('proj-alpha', { storage })?.form.prompt, 'draft from the create panel')
})

test('a stale draft past the TTL is not restored', () => {
  const storage = memoryStorage()
  saveTaskDraft('proj-alpha', draftWith({ prompt: 'ancient' }), {
    storage,
    now: new Date('2025-01-01T00:00:00Z'),
  })

  const result = prepareTaskCreator({
    projectId: 'proj-alpha',
    base: baseTask(),
    schedule: baseSchedule,
    storage,
    now: new Date('2026-01-01T00:00:00Z'),
  })
  assert.equal(result.restoredAt, null)
  assert.equal((result.state as TaskCreatorState).taskForm.prompt, '')
})
