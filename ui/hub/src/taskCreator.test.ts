import assert from 'node:assert/strict'
import test from 'node:test'
import type { ProjectTaskDefinition } from '@aaif/goose-hub-core'
import {
  TASK_DRAFT_SAVE_DEBOUNCE_MS,
  describeDraftRestored,
  shouldPersistTaskCreatorDraft,
  taskCreatorDataToDraft,
  taskCreatorDefaults,
  taskCreatorStateFromDraft,
  toggleTaskCreatorSection,
} from './taskCreator.ts'
import { defaultTaskCreateDraftData } from './taskCreateDraft.ts'

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

test('a fresh creator starts on the basics section with every section expanded', () => {
  const state = taskCreatorDefaults(baseTask(), baseSchedule)
  assert.equal(state.section, defaultTaskCreateDraftData().ui.section)
  assert.deepEqual(state.expanded, defaultTaskCreateDraftData().ui.expanded)
  assert.equal(state.taskTagsInput, '')
  assert.equal(state.verifierType, 'none')
  assert.equal(state.taskForm.subtasks, undefined)
})

test('round-trips every persisted field through the draft shape', () => {
  const data = defaultTaskCreateDraftData()
  const state = taskCreatorStateFromDraft(data, { base: baseTask(), schedule: baseSchedule })

  const restored = taskCreatorDataToDraft(state)
  assert.deepEqual(restored.form.subtasks, [])
  assert.equal(restored.form.name, '')
  assert.equal(restored.form.maxTurns, data.form.maxTurns)
  assert.equal(restored.verifier.type, 'none')
  assert.deepEqual(restored.schedule, {
    kind: data.schedule.kind,
    hour: data.schedule.hour,
    minute: data.schedule.minute,
    weekday: data.schedule.weekday,
  })
})

test('provider fields survive a create draft', () => {
  const data = defaultTaskCreateDraftData()
  data.form.provider = 'gemini'
  data.form.model = 'gemini-3.8-flash'
  data.form.secondaryProvider = 'openai'
  data.form.secondaryModel = 'gpt-4.1'

  const state = taskCreatorStateFromDraft(data, { base: baseTask(), schedule: baseSchedule })
  assert.equal(state.taskForm.provider, 'gemini')
  assert.equal(state.taskForm.secondaryModel, 'gpt-4.1')

  const restored = taskCreatorDataToDraft(state)
  assert.equal(restored.form.provider, 'gemini')
  assert.equal(restored.form.model, 'gemini-3.8-flash')
  assert.equal(restored.form.secondaryProvider, 'openai')
  assert.equal(restored.form.secondaryModel, 'gpt-4.1')
})

test('restoring a draft replaces the freshly generated id so the task is not created twice', () => {
  const data = defaultTaskCreateDraftData()
  data.form.id = 'fetch-markets'
  data.form.name = 'Fetch Markets'

  const state = taskCreatorStateFromDraft(data, { base: baseTask(), schedule: baseSchedule })
  assert.equal(state.taskForm.id, 'fetch-markets')
  assert.equal(state.taskForm.name, 'Fetch Markets')
})

test('restoring keeps subtasks only when the draft has some', () => {
  const data = defaultTaskCreateDraftData()
  data.form.subtasks = [
    { title: 'Collect', description: 'Fetch the feed', maxTurns: 5 },
  ]
  const state = taskCreatorStateFromDraft(data, { base: baseTask(), schedule: baseSchedule })
  assert.equal(state.taskForm.subtasks?.length, 1)
})

test('draft ui state survives the trip back into component state', () => {
  const data = defaultTaskCreateDraftData()
  data.ui.section = 'verifier'
  data.ui.expanded = {
    basics: false,
    prompt: true,
    schedule: false,
    verifier: true,
    subtasks: false,
  }
  const state = taskCreatorStateFromDraft(data, { base: baseTask(), schedule: baseSchedule })
  assert.equal(state.section, 'verifier')
  assert.deepEqual(state.expanded, data.ui.expanded)
})

test('toggling a section flips only that flag and leaves the rest untouched', () => {
  const expanded = defaultTaskCreateDraftData().ui.expanded
  const once = toggleTaskCreatorSection(expanded, 'prompt')
  assert.equal(once.prompt, !expanded.prompt)
  assert.equal(once.basics, expanded.basics)
  assert.equal(toggleTaskCreatorSection(once, 'prompt').prompt, expanded.prompt)
})

test('drafts are only written while creating and not mid-save', () => {
  assert.equal(shouldPersistTaskCreatorDraft({ isCreatingTask: true, saving: false }), true)
  assert.equal(shouldPersistTaskCreatorDraft({ isCreatingTask: false, saving: false }), false)
  assert.equal(shouldPersistTaskCreatorDraft({ isCreatingTask: true, saving: true }), false)
})

test('the debounce window is short enough to survive an immediate reload', () => {
  assert.equal(TASK_DRAFT_SAVE_DEBOUNCE_MS, 600)
})

test('restore copy describes the age of the draft', () => {
  const now = new Date('2026-01-01T12:00:00Z')
  const at = (msAgo: number) => new Date(now.getTime() - msAgo)

  assert.equal(describeDraftRestored(at(5_000), now), 'Restored your unsubmitted draft from a moment ago')
  assert.equal(
    describeDraftRestored(at(60_000), now),
    'Restored your unsubmitted draft from 1 minute ago',
  )
  assert.equal(
    describeDraftRestored(at(5 * 60_000), now),
    'Restored your unsubmitted draft from 5 minutes ago',
  )
  assert.equal(
    describeDraftRestored(at(2 * 3_600_000), now),
    'Restored your unsubmitted draft from 2 hours ago',
  )
  assert.equal(
    describeDraftRestored(at(3 * 86_400_000), now),
    'Restored your unsubmitted draft from 3 days ago',
  )
})

test('a draft timestamped ahead of the clock never reads as negative age', () => {
  const now = new Date('2026-01-01T12:00:00Z')
  const future = new Date(now.getTime() + 10 * 60_000)
  assert.equal(describeDraftRestored(future, now), 'Restored your unsubmitted draft from a moment ago')
})
