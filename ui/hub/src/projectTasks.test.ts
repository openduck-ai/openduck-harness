import assert from 'node:assert/strict'
import test from 'node:test'
import type { ProjectTaskSummary } from '@aaif/goose-hub-core'
import { fetchWithRetry, filterAndSortProjectTasks, historyTaskTitle } from './projectTasks.ts'

function task(
  id: string,
  createdAt: string,
  extra: Partial<ProjectTaskSummary> = {},
): ProjectTaskSummary {
  return {
    id,
    name: extra.name ?? id,
    tags: extra.tags ?? [],
    filePath: extra.filePath ?? `${id}.yaml`,
    hasVerifier: extra.hasVerifier ?? false,
    createdAt,
    ...extra,
    id,
    createdAt,
  }
}

const mixedById = [
  task('aaa', '2024-01-01T00:00:00.000Z', { name: 'Oldest', tags: ['alpha'], category: 'demo' }),
  task('zzz', '2024-06-15T12:00:00.000Z', { name: 'Middle', tags: ['beta'], category: 'demo' }),
  task('mmm', '2024-12-31T23:59:59.000Z', { name: 'Newest', tags: ['alpha', 'beta'], category: 'ops' }),
]

test('filterAndSortProjectTasks orders newest created first even when ids sort the other way', () => {
  const ordered = filterAndSortProjectTasks(mixedById)
  assert.deepEqual(
    ordered.map(item => item.id),
    ['mmm', 'zzz', 'aaa'],
  )
})

test('filterAndSortProjectTasks keeps newest-created-first after search filter', () => {
  const ordered = filterAndSortProjectTasks(mixedById, { query: 'demo' })
  assert.deepEqual(
    ordered.map(item => item.id),
    ['zzz', 'aaa'],
  )
})

test('filterAndSortProjectTasks keeps newest-created-first after tag filter', () => {
  const ordered = filterAndSortProjectTasks(mixedById, { tag: 'alpha' })
  assert.deepEqual(
    ordered.map(item => item.id),
    ['mmm', 'aaa'],
  )
})

test('historyTaskTitle prefers the API task name, then the live task map, then the id', () => {
  assert.equal(
    historyTaskTitle({ taskId: 'task-1', taskName: ' Ship GPS alerts ' }),
    'Ship GPS alerts',
  )
  assert.equal(
    historyTaskTitle({ taskId: 'task-1' }, { 'task-1': 'Live title' }),
    'Live title',
  )
  assert.equal(historyTaskTitle({ taskId: 'task-1', taskName: '   ' }), 'task-1')
  assert.equal(historyTaskTitle({ taskId: 'task-1' }), 'task-1')
})

test('filterAndSortProjectTasks uses id as a stable key when created dates match', () => {
  const ordered = filterAndSortProjectTasks([
    task('zeta', '2024-05-01T00:00:00.000Z'),
    task('alpha', '2024-05-01T00:00:00.000Z'),
    task('mu', '2025-01-01T00:00:00.000Z'),
  ])
  assert.deepEqual(
    ordered.map(item => item.id),
    ['mu', 'alpha', 'zeta'],
  )
})

test('fetchWithRetry returns result on first attempt without retrying', async () => {
  let callCount = 0
  const result = await fetchWithRetry(async () => {
    callCount++
    return 'success'
  })
  assert.equal(result, 'success')
  assert.equal(callCount, 1)
})

test('fetchWithRetry retries on failure and succeeds on subsequent attempt', async () => {
  let callCount = 0
  const sleepDelays: number[] = []
  const retryAttempts: number[] = []

  const result = await fetchWithRetry(
    async () => {
      callCount++
      if (callCount < 3) {
        throw new Error(`Attempt ${callCount} failed`)
      }
      return 'recovered'
    },
    {
      maxRetries: 3,
      delayMs: 100,
      backoffFactor: 2,
      onRetry: (attempt: number) => retryAttempts.push(attempt),
      sleep: async (ms: number) => {
        sleepDelays.push(ms)
      },
    },
  )

  assert.equal(result, 'recovered')
  assert.equal(callCount, 3)
  assert.deepEqual(retryAttempts, [1, 2])
  assert.deepEqual(sleepDelays, [100, 200])
})

test('fetchWithRetry throws when maxRetries is exceeded', async () => {
  let callCount = 0
  const retryAttempts: number[] = []

  await assert.rejects(
    () =>
      fetchWithRetry(
        async () => {
          callCount++
          throw new Error('Persistent failure')
        },
        {
          maxRetries: 2,
          delayMs: 50,
          onRetry: (attempt: number) => retryAttempts.push(attempt),
          sleep: async () => {},
        },
      ),
    {
      message: 'Persistent failure',
    },
  )

  assert.equal(callCount, 3)
  assert.deepEqual(retryAttempts, [1, 2])
})
