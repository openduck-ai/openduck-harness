import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  filterRunningTasks,
  filterTaskEntries,
  historyStatusLabel,
  mergeAndSortTasks,
  normalizeHistoryStatusKind,
  paginateTaskEntries,
  runningActivityText,
  runningElapsedLabel,
  runningProjectLabel,
  runningProjectOptions,
  runningStatusLabel,
  runningStepLabel,
  runningTaskTitle,
  sortRunningTasks,
  taskProjectOptions,
  type HarnessActiveJob,
  type HarnessHistorySummary,
  type RunningTaskFields,
} from './runningTasks.ts'

function job(overrides: Partial<RunningTaskFields> = {}): RunningTaskFields {
  return {
    jobId: 'job-1',
    taskId: 'task-1',
    jobType: 'task',
    description: 'Fix the harness page',
    startedAt: '2026-10-26T10:00:00.000Z',
    currentStatus: 'running',
    provider: 'gemini',
    model: 'gemini-3.8-flash',
    projectSlug: 'openduck',
    ...overrides,
  }
}

test('runningTaskTitle prefers the description', () => {
  assert.equal(runningTaskTitle(job()), 'Fix the harness page')
  assert.equal(runningTaskTitle(job({ description: '  ' })), 'task-1')
})

test('runningProjectLabel falls back to the slug and labels harness jobs', () => {
  assert.equal(runningProjectLabel('openduck', { openduck: 'OpenDuck' }), 'OpenDuck')
  assert.equal(runningProjectLabel('missing', {}), 'missing')
  assert.equal(runningProjectLabel(null, {}), 'Harness')
})

test('running activity describes the live phase', () => {
  assert.equal(runningStatusLabel('queued'), 'Queued')
  assert.equal(runningActivityText(job({ currentStatus: 'queued' })), 'Waiting for a free slot')
  assert.equal(runningActivityText(job()), 'Running')
  assert.equal(
    runningActivityText(job({ activity: { phase: 'thinking' } })),
    'Thinking…',
  )
  assert.equal(
    runningActivityText(
      job({
        activity: {
          phase: 'tool',
          toolName: 'developer__shell',
          toolArguments: { command: 'cargo test -p openduck' },
        },
      }),
    ),
    'Run cargo test -p openduck',
  )
  assert.equal(
    runningActivityText(job({ activity: { phase: 'waiting', detail: 'Interrupted' } })),
    'Interrupted',
  )
  assert.equal(runningStepLabel(0), undefined)
  assert.equal(runningStepLabel(1), '1 step')
  assert.equal(runningStepLabel(4), '4 steps')
})

test('runningElapsedLabel measures from the start time', () => {
  const now = Date.parse('2026-10-26T10:02:05.000Z')
  assert.equal(runningElapsedLabel('2026-10-26T10:00:00.000Z', now), '2m 5s')
  assert.equal(runningElapsedLabel('not-a-date', now), '<1s')
})

test('filterRunningTasks matches project, model, and current activity', () => {
  const jobs = [
    job(),
    job({
      jobId: 'job-2',
      taskId: 'task-2',
      description: 'Write the notes',
      projectSlug: 'easy-cut',
      model: 'other-model',
      startedAt: '2026-10-26T11:00:00.000Z',
      activity: { phase: 'thinking' },
    }),
    job({
      jobId: 'job-3',
      description: 'Global eval',
      projectSlug: null,
      startedAt: '2026-10-26T09:00:00.000Z',
    }),
  ]
  const titles = { openduck: 'OpenDuck', 'easy-cut': 'Easy Cut' }

  const thinking = filterRunningTasks(jobs, { query: 'thinking', projectTitles: titles })
  assert.deepEqual(thinking.map(item => item.jobId), ['job-2'])

  const byProject = filterRunningTasks(jobs, { projectSlug: 'openduck', projectTitles: titles })
  assert.deepEqual(byProject.map(item => item.jobId), ['job-1'])

  const sorted = sortRunningTasks(jobs)
  assert.deepEqual(sorted.map(item => item.jobId), ['job-2', 'job-1', 'job-3'])

  assert.deepEqual(runningProjectOptions(jobs, titles), [
    { slug: 'easy-cut', label: 'Easy Cut' },
    { slug: 'openduck', label: 'OpenDuck' },
  ])
})

function historyItem(overrides: Partial<HarnessHistorySummary> = {}): HarnessHistorySummary {
  return {
    id: 'run-1',
    fileName: '20261026_093000_run-1.json',
    taskId: 'task-hist-1',
    taskName: 'Historical task',
    jobType: 'task',
    status: 'success',
    stepCount: 6,
    toolCallsCount: 4,
    durationMs: 42000,
    startedAt: '2026-10-26T09:30:00.000Z',
    projectSlug: 'openduck',
    providerStats: {
      totalCalls: 4,
      totalInputTokens: 1000,
      totalOutputTokens: 200,
      totalLatencyMs: 3000,
      errorCount: 0,
      fallbackCount: 0,
      byProvider: {
        anthropic: { calls: 4, inputTokens: 1000, outputTokens: 200, totalLatencyMs: 3000, errors: 0 },
      },
      byModel: {
        'claude-sonnet': { calls: 4, inputTokens: 1000, outputTokens: 200, totalLatencyMs: 3000, errors: 0 },
      },
    } as HarnessHistorySummary['providerStats'],
    ...overrides,
  }
}

test('mergeAndSortTasks merges active and historical tasks sorted by startedAt desc and deduplicates', () => {
  const activeJobs: HarnessActiveJob[] = [
    job({
      jobId: 'job-running-older',
      taskId: 'task-1',
      description: 'Older running task',
      startedAt: '2026-10-26T10:00:00.000Z',
      projectSlug: 'openduck',
    }),
    job({
      jobId: 'job-running-newest',
      taskId: 'task-2',
      description: 'Newest running task',
      startedAt: '2026-10-26T12:00:00.000Z',
      projectSlug: 'easy-cut',
    }),
  ]

  const history: HarnessHistorySummary[] = [
    // Duplicate of active job-running-older by id: should be dropped
    historyItem({
      id: 'job-running-older',
      fileName: 'dup.json',
      taskId: 'task-1',
      status: 'running',
      startedAt: '2026-10-26T10:00:00.000Z',
    }),
    // Historical task between the two active jobs
    historyItem({
      id: 'hist-mid',
      fileName: '20261026_110000_hist-mid.json',
      taskId: 'task-mid',
      taskName: 'Mid completed task',
      status: 'success',
      startedAt: '2026-10-26T11:00:00.000Z',
      projectSlug: 'openduck',
    }),
    // Oldest historical task
    historyItem({
      id: 'hist-old',
      fileName: '20261026_080000_hist-old.json',
      taskId: 'task-old',
      taskName: 'Old failed task',
      status: 'failure',
      error: 'Build failed',
      startedAt: '2026-10-26T08:00:00.000Z',
      projectSlug: null,
    }),
  ]

  const merged = mergeAndSortTasks(activeJobs, history)
  assert.deepEqual(
    merged.map(entry => `${entry.kind}:${entry.kind === 'active' ? entry.jobId : entry.id}`),
    [
      'active:job-running-newest',
      'history:hist-mid',
      'active:job-running-older',
      'history:hist-old',
    ],
  )

  const titles = { openduck: 'OpenDuck', 'easy-cut': 'Easy Cut' }
  assert.deepEqual(taskProjectOptions(merged, titles), [
    { slug: 'easy-cut', label: 'Easy Cut' },
    { slug: 'openduck', label: 'OpenDuck' },
  ])

  const onlyActive = filterTaskEntries(merged, { status: 'active', projectTitles: titles })
  assert.deepEqual(
    onlyActive.map(e => (e.kind === 'active' ? e.jobId : e.id)),
    ['job-running-newest', 'job-running-older'],
  )

  const onlyFailed = filterTaskEntries(merged, { status: 'failure', projectTitles: titles })
  assert.deepEqual(
    onlyFailed.map(e => (e.kind === 'active' ? e.jobId : e.id)),
    ['hist-old'],
  )

  const byErrorQuery = filterTaskEntries(merged, { query: 'build failed', projectTitles: titles })
  assert.deepEqual(
    byErrorQuery.map(e => (e.kind === 'active' ? e.jobId : e.id)),
    ['hist-old'],
  )

  assert.equal(normalizeHistoryStatusKind('error'), 'failure')
  assert.equal(normalizeHistoryStatusKind('timeout'), 'timeout')
  assert.equal(historyStatusLabel('success'), 'Success')
})

test('paginateTaskEntries slices entries and respects totalOverride', () => {
  const items = Array.from({ length: 25 }, (_, i) => ({ id: i + 1 }))
  const page1 = paginateTaskEntries(items, 1, 10, 42)
  assert.equal(page1.items.length, 10)
  assert.equal(page1.items[0].id, 1)
  assert.deepEqual(page1.pagination, {
    page: 1,
    limit: 10,
    total: 42,
    totalPages: 5,
  })

  const page2 = paginateTaskEntries(items, 2, 10)
  assert.equal(page2.items.length, 10)
  assert.equal(page2.items[0].id, 11)
  assert.deepEqual(page2.pagination, {
    page: 2,
    limit: 10,
    total: 25,
    totalPages: 3,
  })

  const clamped = paginateTaskEntries(items, 99, 10)
  assert.equal(clamped.items.length, 5)
  assert.equal(clamped.pagination.page, 3)
})
