import assert from 'node:assert/strict'
import test from 'node:test'
import {
  clearTaskDraft,
  defaultTaskCreateDraftData,
  isMeaningfulTaskDraft,
  loadTaskDraft,
  persistTaskDraft,
  saveTaskDraft,
  TASK_DRAFT_MAX_FUTURE_SKEW_MS,
  TASK_DRAFT_STORAGE_PREFIX,
  TASK_DRAFT_TTL_MS,
  TASK_DRAFT_VERSION,
  taskDraftStorageKey,
  type TaskCreateDraftData,
  type TaskDraftStorage,
} from './taskCreateDraft.ts'

function memoryStorage(initial: Record<string, string> = {}) {
  const entries = new Map(Object.entries(initial))
  const storage: TaskDraftStorage & { entries: Map<string, string> } = {
    entries,
    getItem: key => entries.get(key) ?? null,
    setItem: (key, value) => {
      entries.set(key, value)
    },
    removeItem: key => {
      entries.delete(key)
    },
  }
  return storage
}

function draftData(overrides: Partial<TaskCreateDraftData['form']> = {}): TaskCreateDraftData {
  const base = defaultTaskCreateDraftData()
  return {
    ...base,
    ui: { section: 'prompt', expanded: { ...base.ui.expanded, verifier: false } },
    form: {
      ...base.form,
      name: 'Nightly audit',
      prompt: 'Review the diff and summarise it.',
      ...overrides,
    },
    schedule: { kind: 'daily', hour: 2, minute: 30, weekday: 0 },
    verifier: {
      type: 'command',
      command: 'pnpm test',
      expectedStdout: 'pass',
      expectedExitCode: 0,
      filePath: '',
      expectedContent: '',
    },
  }
}

test('saveTaskDraft round-trips form fields, ui state and timestamp under the project key', () => {
  const storage = memoryStorage()
  const now = new Date('2024-05-01T10:00:00.000Z')

  const saved = saveTaskDraft('proj-alpha', draftData(), { storage, now })

  assert.ok(saved)
  assert.equal(saved.updatedAt, now.toISOString())
  assert.equal(storage.getItem(taskDraftStorageKey('proj-alpha')) !== null, true)

  const loaded = loadTaskDraft('proj-alpha', { storage, now })
  assert.ok(loaded)
  assert.equal(loaded.version, TASK_DRAFT_VERSION)
  assert.equal(loaded.projectId, 'proj-alpha')
  assert.equal(loaded.form.name, 'Nightly audit')
  assert.equal(loaded.form.prompt, 'Review the diff and summarise it.')
  assert.equal(loaded.ui.section, 'prompt')
  assert.equal(loaded.ui.expanded.verifier, false)
  assert.deepEqual(loaded.schedule, { kind: 'daily', hour: 2, minute: 30, weekday: 0 })
  assert.equal(loaded.verifier.command, 'pnpm test')
})

test('drafts are scoped per project and do not leak across projects', () => {
  const storage = memoryStorage()

  saveTaskDraft('proj-alpha', draftData({ name: 'Alpha task' }), { storage })
  saveTaskDraft('proj-beta', draftData({ name: 'Beta task' }), { storage })

  assert.equal(loadTaskDraft('proj-alpha', { storage })?.form.name, 'Alpha task')
  assert.equal(loadTaskDraft('proj-beta', { storage })?.form.name, 'Beta task')
  assert.equal(loadTaskDraft('proj-gamma', { storage }), null)
  assert.equal(storage.entries.size, 2)
})

test('clearTaskDraft removes the stored draft', () => {
  const storage = memoryStorage()
  saveTaskDraft('proj-alpha', draftData(), { storage })

  clearTaskDraft('proj-alpha', { storage })

  assert.equal(storage.getItem(taskDraftStorageKey('proj-alpha')), null)
  assert.equal(loadTaskDraft('proj-alpha', { storage }), null)
})

test('loadTaskDraft degrades safely on corrupt JSON and clears the entry', () => {
  const storage = memoryStorage({ [`${TASK_DRAFT_STORAGE_PREFIX}proj-alpha`]: '{not json' })

  assert.equal(loadTaskDraft('proj-alpha', { storage }), null)
  assert.equal(storage.entries.size, 0)
})

test('loadTaskDraft drops drafts with a stale version or a mismatched project id', () => {
  const storage = memoryStorage()
  const now = new Date('2024-05-01T10:00:00.000Z')
  saveTaskDraft('proj-alpha', draftData(), { storage, now })

  const key = taskDraftStorageKey('proj-alpha')
  const stored = JSON.parse(storage.getItem(key) as string)

  storage.setItem(key, JSON.stringify({ ...stored, version: TASK_DRAFT_VERSION + 1 }))
  assert.equal(loadTaskDraft('proj-alpha', { storage, now }), null)
  assert.equal(storage.entries.size, 0)

  saveTaskDraft('proj-alpha', draftData(), { storage, now })
  storage.setItem(key, JSON.stringify({ ...stored, projectId: 'proj-beta' }))
  assert.equal(loadTaskDraft('proj-alpha', { storage, now }), null)
  assert.equal(storage.entries.size, 0)
})

test('loadTaskDraft expires drafts older than the TTL', () => {
  const storage = memoryStorage()
  const savedAt = new Date('2024-05-01T10:00:00.000Z')
  saveTaskDraft('proj-alpha', draftData(), { storage, now: savedAt })

  const justInsideTtl = new Date(savedAt.getTime() + TASK_DRAFT_TTL_MS - 1000)
  const restored = loadTaskDraft('proj-alpha', { storage, now: justInsideTtl })
  assert.equal(restored?.form.name, 'Nightly audit')

  const pastTtl = new Date(savedAt.getTime() + TASK_DRAFT_TTL_MS + 1000)
  assert.equal(loadTaskDraft('proj-alpha', { storage, now: pastTtl }), null)
  assert.equal(storage.entries.size, 0)
})

test('loadTaskDraft drops drafts stamped far in the future instead of keeping them forever', () => {
  const storage = memoryStorage()
  const savedAt = new Date('2024-05-01T10:00:00.000Z')
  saveTaskDraft('proj-alpha', draftData(), { storage, now: savedAt })

  const skewedClock = new Date(savedAt.getTime() - TASK_DRAFT_MAX_FUTURE_SKEW_MS - 1000)
  assert.equal(loadTaskDraft('proj-alpha', { storage, now: skewedClock }), null)
  assert.equal(storage.entries.size, 0)
})

test('loadTaskDraft tolerates a timestamp slightly ahead of the local clock', () => {
  const storage = memoryStorage()
  const savedAt = new Date('2024-05-01T10:00:00.000Z')
  saveTaskDraft('proj-alpha', draftData(), { storage, now: savedAt })

  const slightlyBehind = new Date(savedAt.getTime() - 60_000)
  const restored = loadTaskDraft('proj-alpha', { storage, now: slightlyBehind })
  assert.equal(restored?.form.name, 'Nightly audit')
})

test('loadTaskDraft clears an entry whose timestamp cannot be parsed', () => {
  const storage = memoryStorage()
  saveTaskDraft('proj-alpha', draftData(), { storage })
  const key = taskDraftStorageKey('proj-alpha')
  const stored = JSON.parse(storage.getItem(key) as string)
  storage.setItem(key, JSON.stringify({ ...stored, updatedAt: 'not-a-date' }))

  assert.equal(loadTaskDraft('proj-alpha', { storage }), null)
  assert.equal(storage.entries.size, 0)
})

test('loadTaskDraft coerces malformed field values instead of throwing', () => {
  const storage = memoryStorage()
  saveTaskDraft('proj-alpha', draftData(), {
    storage,
    now: new Date('2024-05-01T10:00:00.000Z'),
  })
  const key = taskDraftStorageKey('proj-alpha')
  const stored = JSON.parse(storage.getItem(key) as string)

  storage.setItem(
    key,
    JSON.stringify({
      ...stored,
      form: { ...stored.form, name: 42, maxTurns: '9999', subtasks: 'nope' },
      schedule: { kind: 'yearly', hour: -5, minute: 'abc', weekday: 99 },
      verifier: { type: 'sql', expectedExitCode: 'x' },
      ui: { section: 'unknown', expanded: { basics: 'yes' } },
    }),
  )

  const loaded = loadTaskDraft('proj-alpha', { storage, now: new Date('2024-05-01T10:00:00.000Z') })
  assert.ok(loaded)
  assert.equal(loaded.form.name, '')
  assert.equal(loaded.form.maxTurns, 1000)
  assert.deepEqual(loaded.form.subtasks, [])
  assert.equal(loaded.schedule.kind, 'none')
  assert.equal(loaded.schedule.hour, 0)
  assert.equal(loaded.schedule.minute, 0)
  assert.equal(loaded.schedule.weekday, 6)
  assert.equal(loaded.verifier.type, 'none')
  assert.equal(loaded.verifier.expectedExitCode, 0)
  assert.equal(loaded.ui.section, 'basics')
  assert.equal(loaded.ui.expanded.basics, true)
})

test('storage failures degrade to a no-op instead of throwing', () => {
  const thrower: TaskDraftStorage = {
    getItem: () => {
      throw new Error('blocked')
    },
    setItem: () => {
      throw new Error('quota exceeded')
    },
    removeItem: () => {
      throw new Error('blocked')
    },
  }

  assert.equal(saveTaskDraft('proj-alpha', draftData(), { storage: thrower }), null)
  assert.equal(loadTaskDraft('proj-alpha', { storage: thrower }), null)
  clearTaskDraft('proj-alpha', { storage: thrower })
  assert.equal(saveTaskDraft('proj-alpha', draftData(), { storage: null }), null)
})

test('blank project ids never touch storage', () => {
  const storage = memoryStorage()

  assert.equal(saveTaskDraft('   ', draftData(), { storage }), null)
  assert.equal(loadTaskDraft('   ', { storage }), null)
  clearTaskDraft('   ', { storage })
  assert.equal(storage.entries.size, 0)
})

test('isMeaningfulTaskDraft keeps a provider-only draft', () => {
  assert.equal(
    isMeaningfulTaskDraft({
      ...defaultTaskCreateDraftData(),
      form: { ...defaultTaskCreateDraftData().form, provider: 'gemini' },
    }),
    true,
  )
})

test('isMeaningfulTaskDraft ignores an untouched blank form', () => {
  assert.equal(isMeaningfulTaskDraft(defaultTaskCreateDraftData()), false)
  assert.equal(isMeaningfulTaskDraft(draftData()), true)
  assert.equal(
    isMeaningfulTaskDraft({
      ...defaultTaskCreateDraftData(),
      form: { ...defaultTaskCreateDraftData().form, prompt: 'Investigate flaky test' },
    }),
    true,
  )
})

test('persistTaskDraft stores meaningful input and clears a blank form', () => {
  const storage = memoryStorage()

  assert.ok(persistTaskDraft('proj-alpha', draftData(), { storage }))
  assert.equal(storage.entries.size, 1)

  const blank = persistTaskDraft('proj-alpha', defaultTaskCreateDraftData(), { storage })
  assert.equal(blank, null)
  assert.equal(storage.entries.size, 0)
})

test('rewriting unchanged content keeps the original timestamp so the banner stays honest', () => {
  const storage = memoryStorage()
  const first = new Date('2026-01-01T10:00:00Z')
  const later = new Date('2026-01-01T10:30:00Z')

  saveTaskDraft('proj-alpha', draftData(), { storage, now: first })
  const rewritten = saveTaskDraft('proj-alpha', draftData(), { storage, now: later })

  assert.equal(rewritten?.updatedAt, first.toISOString())
})

test('editing a field refreshes the timestamp', () => {
  const storage = memoryStorage()
  const first = new Date('2026-01-01T10:00:00Z')
  const later = new Date('2026-01-01T10:30:00Z')

  saveTaskDraft('proj-alpha', draftData(), { storage, now: first })
  const edited = saveTaskDraft('proj-alpha', draftData({ prompt: 'a new prompt' }), {
    storage,
    now: later,
  })

  assert.equal(edited?.updatedAt, later.toISOString())
  assert.equal(loadTaskDraft('proj-alpha', { storage, now: later })?.form.prompt, 'a new prompt')
})
