import assert from 'node:assert/strict'
import test from 'node:test'
import type { HarnessHistorySummary, ProjectTaskSummary } from '@aaif/goose-hub-core'
import {
  fetchWithRetry,
  filterAndSortProjectTasks,
  filterHistoryItems,
  firstLine,
  formatDurationMs,
  formatRelativeTime,
  historyTaskTitle,
  latestRunForTask,
  presentHistoryRow,
  presentLastRun,
  projectTaskListEmpty,
  runDetailLine,
} from './projectTasks.ts'

function task(
  id: string,
  createdAt: string,
  extra: Partial<ProjectTaskSummary> = {},
): ProjectTaskSummary {
  return {
    name: id,
    tags: [],
    filePath: `${id}.yaml`,
    hasVerifier: false,
    ...extra,
    id,
    createdAt,
  }
}

function historySummary(
  id: string,
  taskId: string,
  startedAt: string,
  extra: Partial<HarnessHistorySummary> = {},
): HarnessHistorySummary {
  return {
    id,
    fileName: `${id}.json`,
    taskId,
    jobType: extra.jobType ?? 'task',
    status: extra.status ?? 'Success',
    stepCount: extra.stepCount ?? 1,
    toolCallsCount: extra.toolCallsCount ?? 0,
    durationMs: extra.durationMs ?? 1000,
    startedAt,
    ...extra,
  }
}

const mixedById = [
  task('aaa', '2024-01-01T00:00:00.000Z', { name: 'Oldest', tags: ['alpha'], category: 'demo' }),
  task('zzz', '2024-06-15T12:00:00.000Z', { name: 'Middle', tags: ['beta'], category: 'demo' }),
  task('mmm', '2024-12-31T23:59:59.000Z', { name: 'Newest', tags: ['alpha', 'beta'], category: 'ops' }),
]

test('filterAndSortProjectTasks orders newest created first when sort: created', () => {
  const ordered = filterAndSortProjectTasks(mixedById, { sort: 'created' })
  assert.deepEqual(
    ordered.map(item => item.id),
    ['mmm', 'zzz', 'aaa'],
  )
})

test('filterAndSortProjectTasks keeps newest-created-first after search filter with sort: created', () => {
  const ordered = filterAndSortProjectTasks(mixedById, { query: 'demo', sort: 'created' })
  assert.deepEqual(
    ordered.map(item => item.id),
    ['zzz', 'aaa'],
  )
})

test('filterAndSortProjectTasks matches promptPreview substring', () => {
  const tasksWithPrompt = [
    task('p1', '2024-01-01T00:00:00.000Z', { promptPreview: 'Review pull request diff and suggest fixes' }),
    task('p2', '2024-01-02T00:00:00.000Z', { promptPreview: 'Generate integration test report' }),
  ]
  const ordered = filterAndSortProjectTasks(tasksWithPrompt, { query: 'pull request' })
  assert.deepEqual(ordered.map(item => item.id), ['p1'])
})

test('filterAndSortProjectTasks keeps newest-created-first after tag filter with sort: created', () => {
  const ordered = filterAndSortProjectTasks(mixedById, { tag: 'alpha', sort: 'created' })
  assert.deepEqual(
    ordered.map(item => item.id),
    ['mmm', 'aaa'],
  )
})

test('filterAndSortProjectTasks defaults to last activity sort', () => {
  // aaa has newest run, zzz has older run, mmm has no run (falls back to createdAt)
  const latestByTaskId = new Map<string, HarnessHistorySummary>([
    ['aaa', historySummary('run-aaa', 'aaa', '2025-05-01T00:00:00.000Z', { durationMs: 10000 })],
    ['zzz', historySummary('run-zzz', 'zzz', '2025-02-01T00:00:00.000Z', { durationMs: 5000 })],
  ])

  const ordered = filterAndSortProjectTasks(mixedById, { latestByTaskId })
  assert.deepEqual(
    ordered.map(item => item.id),
    ['aaa', 'zzz', 'mmm'],
  )
})

test('filterAndSortProjectTasks pins running jobs first in most recently started order', () => {
  const runningJobStartedAt = new Map<string, string>([
    ['zzz', '2025-06-01T10:00:00.000Z'], // started earlier
    ['aaa', '2025-06-01T12:00:00.000Z'], // started most recently
  ])

  const ordered = filterAndSortProjectTasks(mixedById, {
    sort: 'created',
    runningJobStartedAt,
  })
  // aaa and zzz pinned first, aaa started more recently so it comes first
  assert.deepEqual(
    ordered.map(item => item.id),
    ['aaa', 'zzz', 'mmm'],
  )
})

test('filterAndSortProjectTasks query matches tags as substring', () => {
  const ordered = filterAndSortProjectTasks(mixedById, { query: 'alph', sort: 'created' })
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
  ], { sort: 'created' })
  assert.deepEqual(
    ordered.map(item => item.id),
    ['mu', 'alpha', 'zeta'],
  )
})

test('latestRunForTask picks greatest startedAt regardless of array order and tie-breaks by id', () => {
  const runs = [
    historySummary('run-1', 'task-a', '2024-01-01T10:00:00.000Z'),
    historySummary('run-3', 'task-a', '2024-01-03T10:00:00.000Z'),
    historySummary('run-2', 'task-a', '2024-01-02T10:00:00.000Z'),
    historySummary('eval-1', 'project-demo', '2024-01-04T10:00:00.000Z'), // different taskId
  ]

  const latest = latestRunForTask(runs, 'task-a')
  assert.equal(latest?.id, 'run-3')

  // Tie on startedAt breaks on id
  const tieRuns = [
    historySummary('run-alpha', 'task-b', '2024-01-01T00:00:00.000Z'),
    historySummary('run-beta', 'task-b', '2024-01-01T00:00:00.000Z'),
  ]
  const tieWinner = latestRunForTask(tieRuns, 'task-b')
  assert.equal(tieWinner?.id, 'run-beta')
})

test('formatDurationMs formats elapsed and duration correctly with zero components omitted', () => {
  assert.equal(formatDurationMs(500), '<1s')
  assert.equal(formatDurationMs(999), '<1s')
  assert.equal(formatDurationMs(1000), '1s')
  assert.equal(formatDurationMs(2140), '2s')
  assert.equal(formatDurationMs(45000), '45s')
  assert.equal(formatDurationMs(120000), '2m')
  assert.equal(formatDurationMs(134000), '2m 14s')
  assert.equal(formatDurationMs(3600000), '1h')
  assert.equal(formatDurationMs(3780000), '1h 3m')
})

test('formatRelativeTime produces expected relative text', () => {
  const now = Date.parse('2024-10-07T12:00:00.000Z')
  assert.equal(formatRelativeTime('', now), '')
  assert.equal(formatRelativeTime('invalid-date', now), '')
  assert.equal(formatRelativeTime('2024-10-07T11:59:30.000Z', now), 'just now') // 30s ago
  assert.equal(formatRelativeTime('2024-10-07T11:57:00.000Z', now), '3m ago') // 3m ago
  assert.equal(formatRelativeTime('2024-10-07T07:00:00.000Z', now), '5h ago') // 5h ago
  assert.equal(formatRelativeTime('2024-10-05T12:00:00.000Z', now), '2d ago') // 2d ago
  assert.equal(formatRelativeTime('2024-09-01T12:00:00.000Z', now), 'Sep 1')
})

test('firstLine trims, takes text before first newline, and caps at max chars', () => {
  assert.equal(firstLine(null), undefined)
  assert.equal(firstLine(''), undefined)
  assert.equal(firstLine('  \n  '), undefined)
  assert.equal(firstLine('First line\nSecond line'), 'First line')
  const longText = 'a'.repeat(150)
  assert.equal(firstLine(longText), `${'a'.repeat(140)}…`)
})

test('runDetailLine prefers non-empty error, then finalAnswer, otherwise undefined', () => {
  assert.equal(
    runDetailLine({ error: 'Crash error\nStack trace', finalAnswer: 'All done' }),
    'Crash error',
  )
  assert.equal(
    runDetailLine({ error: null, finalAnswer: 'Policy step 4 failed for task foo' }),
    'Policy step 4 failed for task foo',
  )
  assert.equal(runDetailLine({ error: null, finalAnswer: null }), undefined)
})

test('presentLastRun covers all cases correctly', () => {
  const now = Date.parse('2024-10-07T12:00:00.000Z')

  // Running without step
  const runningNoStep = presentLastRun({
    historyLoaded: true,
    historyError: false,
    latest: undefined,
    running: { jobId: 'job-1', startedAt: '2024-10-07T11:57:46.000Z' }, // 2m 14s ago
    now,
  })
  assert.deepEqual(runningNoStep, {
    kind: 'running',
    text: 'Running · 2m 14s',
    jobId: 'job-1',
  })

  // Running with step
  const runningWithStep = presentLastRun({
    historyLoaded: true,
    historyError: false,
    latest: undefined,
    running: { jobId: 'job-1', startedAt: '2024-10-07T11:57:46.000Z', stepCount: 4 },
    now,
  })
  assert.deepEqual(runningWithStep, {
    kind: 'running',
    text: 'Running · 2m 14s · step 4',
    jobId: 'job-1',
  })

  // Loading
  const loading = presentLastRun({
    historyLoaded: false,
    historyError: false,
    latest: undefined,
    running: undefined,
    now,
  })
  assert.deepEqual(loading, { kind: 'loading' })

  // Unavailable (history error and no row)
  const unavailable = presentLastRun({
    historyLoaded: true,
    historyError: true,
    latest: undefined,
    running: undefined,
    now,
  })
  assert.deepEqual(unavailable, { kind: 'unavailable' })

  // None (loaded and no rows)
  const none = presentLastRun({
    historyLoaded: true,
    historyError: false,
    latest: undefined,
    running: undefined,
    now,
  })
  assert.deepEqual(none, { kind: 'none' })

  // Success
  const successRun = historySummary('run-s', 't1', '2024-10-07T11:00:00.000Z', {
    status: 'Success',
    durationMs: 134000,
    finalAnswer: 'Finished successfully',
  })
  const successPres = presentLastRun({
    historyLoaded: true,
    historyError: false,
    latest: successRun,
    running: undefined,
    now,
  })
  assert.deepEqual(successPres, {
    kind: 'success',
    text: 'Success · 2m 14s',
    detail: 'Finished successfully',
    runId: 'run-s',
  })

  // Failure (policy step failure, error is null, finalAnswer is set)
  const failRun = historySummary('run-f', 't1', '2024-10-07T11:57:00.000Z', {
    status: 'Failure',
    stepCount: 12,
    error: null,
    finalAnswer: "Policy step 4 failed for task 'fetch-markets'",
  })
  const failPres = presentLastRun({
    historyLoaded: true,
    historyError: false,
    latest: failRun,
    running: undefined,
    now,
  })
  assert.deepEqual(failPres, {
    kind: 'terminal',
    text: 'Failed · 3m ago · 12 steps',
    detail: "Policy step 4 failed for task 'fetch-markets'",
    runId: 'run-f',
  })

  // User stop (both error and finalAnswer set to "Task stopped by user")
  const stopRun = historySummary('run-stop', 't1', '2024-10-07T11:57:00.000Z', {
    status: 'Cancelled',
    stepCount: 5,
    error: 'Task stopped by user',
    finalAnswer: 'Task stopped by user',
  })
  const stopPres = presentLastRun({
    historyLoaded: true,
    historyError: false,
    latest: stopRun,
    running: undefined,
    now,
  })
  assert.deepEqual(stopPres, {
    kind: 'terminal',
    text: 'Cancelled · 3m ago · 5 steps',
    detail: 'Task stopped by user',
    runId: 'run-stop',
  })

  // Skipped
  const skipRun = historySummary('run-skip', 't1', '2024-10-05T12:00:00.000Z', {
    status: 'Skipped',
    stepCount: 0,
    durationMs: 0,
    error: 'Skipped: previous run still in progress',
    finalAnswer: 'Skipped: previous run still in progress',
  })
  const skipPres = presentLastRun({
    historyLoaded: true,
    historyError: false,
    latest: skipRun,
    running: undefined,
    now,
  })
  assert.deepEqual(skipPres, {
    kind: 'terminal',
    text: 'Skipped · 2d ago',
    detail: 'Skipped: previous run still in progress',
    runId: 'run-skip',
  })

  // Timeout (optional case)
  const timeoutRun = historySummary('run-to', 't1', '2024-10-07T11:57:00.000Z', {
    status: 'Timeout',
    stepCount: 10,
    error: 'Execution timed out',
  })
  const timeoutPres = presentLastRun({
    historyLoaded: true,
    historyError: false,
    latest: timeoutRun,
    running: undefined,
    now,
  })
  assert.deepEqual(timeoutPres, {
    kind: 'terminal',
    text: 'Timed out · 3m ago · 10 steps',
    detail: 'Execution timed out',
    runId: 'run-to',
  })
})

test('projectTaskListEmpty differentiates empty project from filter miss', () => {
  assert.equal(projectTaskListEmpty({ taskCount: 0, matchCount: 0 }), 'no-tasks')
  assert.equal(projectTaskListEmpty({ taskCount: 5, matchCount: 0 }), 'no-match')
  assert.equal(projectTaskListEmpty({ taskCount: 5, matchCount: 3 }), 'list')
})

test('filterHistoryItems matches lowercase wire status and rejects mismatching labels', () => {
  const items = [
    historySummary('r1', 't1', '2024-01-01T00:00:00Z', { status: 'Success', jobType: 'task' }),
    historySummary('r2', 't1', '2024-01-01T00:00:00Z', { status: 'Failure', jobType: 'task' }),
    historySummary('r3', 't2', '2024-01-01T00:00:00Z', { status: 'Cancelled', jobType: 'eval' }),
    historySummary('r4', 't2', '2024-01-01T00:00:00Z', { status: 'Timeout', jobType: 'task' }),
  ]

  // Wire lowercase 'failure' matches 'Failure'
  const failed = filterHistoryItems(items, { status: 'failure' })
  assert.equal(failed.length, 1)
  assert.equal(failed[0].id, 'r2')

  // UI label 'failed' matches nothing
  const failedLabel = filterHistoryItems(items, { status: 'failed' })
  assert.equal(failedLabel.length, 0)

  // Filter by task
  const task2 = filterHistoryItems(items, { taskId: 't2' })
  assert.equal(task2.length, 2)

  // Filter by jobType
  const evals = filterHistoryItems(items, { jobType: 'eval' })
  assert.equal(evals.length, 1)
  assert.equal(evals[0].id, 'r3')
})

test('presentHistoryRow formats skipped duration as em-dash when duration is zero', () => {
  const now = Date.parse('2024-10-07T12:00:00.000Z')
  const skipped = historySummary('r-skip', 't1', '2024-10-05T12:00:00.000Z', {
    status: 'Skipped',
    durationMs: 0,
    error: 'Skipped: previous run still in progress',
  })
  const row = presentHistoryRow(skipped, { t1: 'My Task' }, now)
  assert.equal(row.title, 'My Task')
  assert.equal(row.duration, '—')
  assert.equal(row.status, 'skipped')
  assert.equal(row.detail, 'Skipped: previous run still in progress')
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
