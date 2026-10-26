import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  filterRunningTasks,
  runningActivityText,
  runningElapsedLabel,
  runningProjectLabel,
  runningProjectOptions,
  runningStatusLabel,
  runningStepLabel,
  runningTaskTitle,
  sortRunningTasks,
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
