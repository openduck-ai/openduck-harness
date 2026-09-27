import assert from 'node:assert/strict'
import test from 'node:test'
import {
  calculateContinueTurns,
  formatContinueBudgetSummary,
} from './taskContinuation.ts'
import { createApi } from './api.ts'

test('calculateContinueTurns calculates additional turns correctly', () => {
  assert.equal(
    calculateContinueTurns({
      previousSteps: 25,
      mode: 'additional',
      value: 10,
    }),
    35,
  )

  assert.equal(
    calculateContinueTurns({
      previousSteps: 25,
      mode: 'additional',
      value: 25,
    }),
    50,
  )

  assert.equal(
    calculateContinueTurns({
      previousSteps: 25,
      mode: 'additional',
      value: 50,
    }),
    75,
  )

  assert.equal(
    calculateContinueTurns({
      previousSteps: 25,
      mode: 'additional',
      value: 100,
    }),
    125,
  )
})

test('calculateContinueTurns handles total turns mode', () => {
  assert.equal(
    calculateContinueTurns({
      previousSteps: 25,
      mode: 'total',
      value: 50,
    }),
    50,
  )

  assert.equal(
    calculateContinueTurns({
      previousSteps: 25,
      mode: 'total',
      value: 100,
    }),
    100,
  )
})

test('calculateContinueTurns clamps invalid/zero/negative values', () => {
  assert.equal(
    calculateContinueTurns({
      previousSteps: 20,
      mode: 'additional',
      value: 0,
    }),
    21,
  )

  assert.equal(
    calculateContinueTurns({
      previousSteps: 20,
      mode: 'total',
      value: -5,
    }),
    1,
  )
})

test('isContinuableStatus covers cancelled, failure, and timeout', async () => {
  const { isContinuableStatus } = await import('./taskContinuation.ts')
  assert.equal(isContinuableStatus('cancelled'), true)
  assert.equal(isContinuableStatus('Failure'), true)
  assert.equal(isContinuableStatus('timeout'), true)
  assert.equal(isContinuableStatus('success'), false)
})

test('extraTurnsForContinue uses +N for additional and remainder for total', async () => {
  const { extraTurnsForContinue } = await import('./taskContinuation.ts')
  assert.equal(
    extraTurnsForContinue({ previousSteps: 20, mode: 'additional', value: 25 }),
    25,
  )
  assert.equal(extraTurnsForContinue({ previousSteps: 20, mode: 'total', value: 45 }), 25)
})

test('formatContinueBudgetSummary formats diff and target turns', () => {
  const summary1 = formatContinueBudgetSummary(25, 50)
  assert.equal(summary1.previousSteps, 25)
  assert.equal(summary1.targetMaxTurns, 50)
  assert.equal(summary1.diffText, '+25 turns')

  const summary2 = formatContinueBudgetSummary(30, 20)
  assert.equal(summary2.diffText, '-10 turns')
})

test('runProjectTask sends specified maxTurns and record payload', async () => {
  const originalFetch = globalThis.fetch
  let requestUrl = ''
  let requestBody: any = null

  globalThis.fetch = async (input, init) => {
    requestUrl = String(input)
    if (init?.body) {
      requestBody = JSON.parse(String(init.body))
    }
    return new Response(
      JSON.stringify({
        taskId: 'test-task',
        status: 'Success',
        stepCount: 12,
        toolCallsCount: 5,
        durationMs: 4500,
        finalAnswer: 'Finished successfully',
        trajectory: {
          sessionId: 'test-session',
          taskId: 'test-task',
          policyName: 'agent',
          startedAt: new Date().toISOString(),
          completedAt: new Date().toISOString(),
          success: true,
          steps: [],
        },
      }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    )
  }

  try {
    const api = createApi('http://localhost:3000', 'test-secret')
    const res = await api.runProjectTask('my-project', 'test-task', {
      maxTurns: 45,
      record: true,
      continueFromRunId: '20260902_115311_task_task-413704_01a061f7',
      extraTurns: 25,
    })

    assert.equal(
      requestUrl,
      'http://localhost:3000/api/v1/projects/my-project/harness/tasks/test-task/run',
    )
    assert.equal(requestBody?.maxTurns, 45)
    assert.equal(requestBody?.record, true)
    assert.equal(
      requestBody?.continueFromRunId,
      '20260902_115311_task_task-413704_01a061f7',
    )
    assert.equal(requestBody?.extraTurns, 25)
    assert.equal(res.status, 'Success')
  } finally {
    globalThis.fetch = originalFetch
  }
})
