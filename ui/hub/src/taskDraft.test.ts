import assert from 'node:assert/strict'
import test from 'node:test'
import type { ProjectTaskDefinition, TaskDraft } from '@aaif/goose-hub-core'
import {
  applyTaskDraft,
  shouldReplaceGeneratedTaskId,
  subtaskSaveError,
  subtasksForSave,
} from './taskDraft.ts'

const form: ProjectTaskDefinition = {
  id: 'task-123456',
  name: '',
  category: '',
  tags: [],
  prompt: '',
  maxTurns: 25,
  timeoutSeconds: 300,
}

const draft: TaskDraft = {
  id: 'task-gps-notifications',
  name: 'GPS notifications',
  category: 'feature',
  tags: ['gps'],
  prompt: 'Implement GPS notifications',
  maxTurns: 80,
  timeoutSeconds: 1800,
  largeTask: true,
  complexity: 'complex',
  reasoning: 'Several modules.',
  subtasks: [
    {
      id: 'task-gps-notifications-1',
      title: 'Define types',
      description: 'Add src/dto.rs',
      maxTurns: 24,
      targetFiles: ['src/dto.rs'],
      expectedOutputs: [
        { description: 'src/dto.rs exists', kind: 'file', filePath: 'src/dto.rs' },
      ],
    },
    {
      id: 'task-gps-notifications-2',
      title: 'Verify',
      description: 'Run cargo test',
      maxTurns: 16,
      expectedOutputs: [
        { description: 'cargo test passes', kind: 'command', command: 'cargo test' },
      ],
    },
  ],
}

test('a generated id is replaced when the chat drafts a new task', () => {
  assert.equal(shouldReplaceGeneratedTaskId('task-123456', false), true)
  assert.equal(shouldReplaceGeneratedTaskId('task-123456', true), false)
  assert.equal(shouldReplaceGeneratedTaskId('my-task', false), false)
  const next = applyTaskDraft(form, draft, { replaceId: true })
  assert.equal(next.id, 'task-gps-notifications')
  assert.equal(next.name, 'GPS notifications')
  assert.equal(next.prompt, 'Implement GPS notifications')
  assert.equal(next.maxTurns, 80)
  assert.equal(next.subtasks?.length, 2)
  assert.equal(next.subtasks?.[0].expectedOutputs?.[0].filePath, 'src/dto.rs')
})

test('editing an existing task keeps its id and a focused draft clears subtasks', () => {
  const next = applyTaskDraft(
    { ...form, id: 'task-kept', subtasks: draft.subtasks },
    { ...draft, largeTask: false, subtasks: [] },
    { replaceId: false },
  )
  assert.equal(next.id, 'task-kept')
  assert.equal(next.subtasks, undefined)
})

test('save drops blank subtasks and incomplete check fields', () => {
  const saved = subtasksForSave([
    {
      id: '  ',
      title: '  ',
      description: '   ',
      expectedOutputs: [{ description: '   ', kind: 'note' }],
    },
    {
      id: 'step-2',
      title: 'Write the file',
      description: 'Create it',
      targetFiles: [' src/a.rs ', ''],
      expectedOutputs: [
        { description: '  file exists  ', kind: 'file', filePath: ' src/a.rs ' },
        { description: '', kind: 'note' },
      ],
    },
  ])
  assert.equal(saved?.length, 1)
  assert.equal(saved?.[0].id, 'step-2')
  assert.deepEqual(saved?.[0].targetFiles, ['src/a.rs'])
  assert.equal(saved?.[0].expectedOutputs?.length, 1)
  assert.equal(saved?.[0].expectedOutputs?.[0].filePath, 'src/a.rs')
})

test('a file or command check needs its target before save', () => {
  assert.equal(
    subtaskSaveError([
      {
        id: 's1',
        title: 'One',
        description: 'Do it',
        expectedOutputs: [{ description: 'file', kind: 'file' }],
      },
    ]),
    'Subtask 1 has a file check without a path',
  )
  assert.equal(
    subtaskSaveError([
      {
        id: 's1',
        title: 'One',
        description: 'Do it',
        expectedOutputs: [{ description: 'cmd', kind: 'command' }],
      },
    ]),
    'Subtask 1 has a command check without a command',
  )
  assert.equal(subtaskSaveError(undefined), null)
})
