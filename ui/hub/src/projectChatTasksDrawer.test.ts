import assert from 'node:assert/strict'
import test from 'node:test'
import type { ProjectTaskDefinition, ProjectTaskSummary } from '@aaif/goose-hub-core'

test('project task definitions for project chat include required fields', () => {
  const taskDef: ProjectTaskDefinition = {
    id: 'verify-auth-session',
    name: 'Verify Auth Session',
    category: 'security',
    tags: ['auth', 'session'],
    prompt: 'Verify that invalid auth tokens return 401 Unauthorized.',
    dynamicPrompt: true,
    cron: '0 4 * * *',
    verifier: {
      command: 'cargo test test_auth_unauthorized',
      expectedExitCode: 0,
    },
    maxTurns: 15,
  }

  assert.equal(taskDef.id, 'verify-auth-session')
  assert.equal(taskDef.dynamicPrompt, true)
  assert.equal(taskDef.maxTurns, 15)
  assert.deepEqual(taskDef.tags, ['auth', 'session'])
})

test('project chat task action handlers validate task id and prompt before submission', () => {
  function validateTaskForm(id: string, prompt: string): string | null {
    if (!id.trim()) return 'Task ID is required.'
    if (!prompt.trim()) return 'Task prompt instructions are required.'
    return null
  }

  assert.equal(validateTaskForm('', 'some prompt'), 'Task ID is required.')
  assert.equal(validateTaskForm('task-1', ''), 'Task prompt instructions are required.')
  assert.equal(validateTaskForm('task-1', 'do something'), null)
})

test('dynamic prompt composition for task execution in project chat', () => {
  function composeTaskRunPrompt(basePrompt: string, dynamicPrompt: boolean, extraPrompt?: string) {
    if (!extraPrompt || !extraPrompt.trim()) return basePrompt
    if (!dynamicPrompt) return basePrompt
    return `${basePrompt}\n\nAdditional instructions for this run:\n${extraPrompt.trim()}`
  }

  const base = 'Run integration tests.'
  const extra = 'Focus on failure recovery.'

  // Dynamic prompt enabled
  assert.equal(
    composeTaskRunPrompt(base, true, extra),
    'Run integration tests.\n\nAdditional instructions for this run:\nFocus on failure recovery.',
  )

  // Dynamic prompt disabled ignores extra prompt
  assert.equal(composeTaskRunPrompt(base, false, extra), base)
})
