import type { HarnessHistorySummary, ProjectTaskSummary } from '@aaif/goose-hub-core'

export function historyTaskTitle(
  item: { taskId: string; taskName?: string | null },
  taskNames?: Record<string, string>,
): string {
  const fromItem = item.taskName?.trim()
  if (fromItem) return fromItem
  const fromMap = taskNames?.[item.taskId]?.trim()
  if (fromMap) return fromMap
  return item.taskId
}

export type ProjectTaskSort = 'activity' | 'created'

export function filterAndSortProjectTasks(
  tasks: ProjectTaskSummary[],
  options: {
    query?: string
    tag?: string
    sort?: ProjectTaskSort
    latestByTaskId?: ReadonlyMap<string, HarnessHistorySummary>
    runningJobStartedAt?: ReadonlyMap<string, string>
  } = {},
): ProjectTaskSummary[] {
  const query = options.query?.toLowerCase() ?? ''
  const tag = options.tag ?? ''
  const sort = options.sort ?? 'activity'

  return tasks
    .filter(task => {
      const taskTags = Array.isArray(task.tags) ? task.tags : []
      if (tag && !taskTags.includes(tag)) return false
      if (!query) return true
      return (
        task.id.toLowerCase().includes(query) ||
        (task.name && task.name.toLowerCase().includes(query)) ||
        (task.category && task.category.toLowerCase().includes(query)) ||
        taskTags.some(t => t.toLowerCase().includes(query)) ||
        Boolean(task.promptPreview && task.promptPreview.toLowerCase().includes(query))
      )
    })
    .sort((a, b) => {
      const runningStartedA = options.runningJobStartedAt?.get(a.id)
      const runningStartedB = options.runningJobStartedAt?.get(b.id)
      const isRunningA = Boolean(runningStartedA)
      const isRunningB = Boolean(runningStartedB)

      // Running tasks pin first in both sort modes, most recently started first
      if (isRunningA && !isRunningB) return -1
      if (!isRunningA && isRunningB) return 1
      if (isRunningA && isRunningB) {
        const timeA = Date.parse(runningStartedA!) || 0
        const timeB = Date.parse(runningStartedB!) || 0
        if (timeA !== timeB) return timeB - timeA
        return a.id.localeCompare(b.id)
      }

      if (sort === 'created') {
        const byCreated = compareCreatedAtDesc(a.createdAt, b.createdAt)
        if (byCreated !== 0) return byCreated
        return a.id.localeCompare(b.id)
      }

      // sort === 'activity'
      // Last activity is startedAt + durationMs of latestRunForTask, or createdAt when the task has no run
      const latestA = options.latestByTaskId?.get(a.id)
      const latestB = options.latestByTaskId?.get(b.id)

      const actA = latestA?.startedAt
        ? (Date.parse(latestA.startedAt) || 0) + (latestA.durationMs || 0)
        : a.createdAt
          ? (Date.parse(a.createdAt) || 0)
          : 0

      const actB = latestB?.startedAt
        ? (Date.parse(latestB.startedAt) || 0) + (latestB.durationMs || 0)
        : b.createdAt
          ? (Date.parse(b.createdAt) || 0)
          : 0

      if (actA !== actB) return actB - actA
      return a.id.localeCompare(b.id)
    })
}

function compareCreatedAtDesc(a?: string, b?: string): number {
  if (a === b) return 0
  if (!a) return 1
  if (!b) return -1
  return b.localeCompare(a)
}

export function latestRunForTask(
  history: readonly HarnessHistorySummary[],
  taskId: string,
): HarnessHistorySummary | undefined {
  let latest: HarnessHistorySummary | undefined
  for (const item of history) {
    if (item.taskId !== taskId) continue
    if (!latest) {
      latest = item
      continue
    }
    const itemStarted = item.startedAt ?? ''
    const latestStarted = latest.startedAt ?? ''
    if (itemStarted > latestStarted) {
      latest = item
    } else if (itemStarted === latestStarted) {
      if (item.id > latest.id) {
        latest = item
      }
    }
  }
  return latest
}

export function formatDurationMs(ms: number): string {
  if (ms < 1000) return '<1s'
  const totalSeconds = Math.round(ms / 1000)
  if (totalSeconds < 1) return '<1s'
  const hours = Math.floor(totalSeconds / 3600)
  const remainingSeconds = totalSeconds % 3600
  const minutes = Math.floor(remainingSeconds / 60)
  const seconds = remainingSeconds % 60

  const parts: string[] = []
  if (hours > 0) parts.push(`${hours}h`)
  if (minutes > 0) parts.push(`${minutes}m`)
  if (seconds > 0) parts.push(`${seconds}s`)
  return parts.join(' ')
}

export function formatRelativeTime(iso: string | null | undefined, now: number): string {
  if (!iso) return ''
  const timestamp = Date.parse(iso)
  if (Number.isNaN(timestamp)) return ''
  const diffMs = Math.max(0, now - timestamp)
  const diffSec = Math.floor(diffMs / 1000)

  if (diffSec < 45) return 'just now'
  if (diffSec < 60 * 60) {
    const mins = Math.max(1, Math.floor(diffSec / 60))
    return `${mins}m ago`
  }
  if (diffSec < 24 * 3600) {
    const hours = Math.floor(diffSec / 3600)
    return `${hours}h ago`
  }
  if (diffSec < 7 * 86400) {
    const days = Math.floor(diffSec / 86400)
    return `${days}d ago`
  }
  const date = new Date(timestamp)
  return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' })
}

export function firstLine(text: string | null | undefined, max = 140): string | undefined {
  if (!text) return undefined
  const trimmed = text.trim()
  if (!trimmed) return undefined
  const first = trimmed.split(/\r?\n/)[0].trim()
  if (!first) return undefined
  if (first.length > max) {
    return `${first.slice(0, max)}…`
  }
  return first
}

export function runDetailLine(item: {
  error?: string | null
  finalAnswer?: string | null
}): string | undefined {
  const err = firstLine(item.error)
  if (err) return err
  return firstLine(item.finalAnswer)
}

export type LastRunPresentation =
  | { kind: 'loading' }
  | { kind: 'unavailable' }
  | { kind: 'none' }
  | { kind: 'running'; text: string; jobId: string }
  | { kind: 'success'; text: string; detail?: string; runId: string }
  | { kind: 'terminal'; text: string; detail?: string; runId: string }

export function presentLastRun(input: {
  historyLoaded: boolean
  historyError: boolean
  latest: HarnessHistorySummary | undefined
  running: {
    jobId: string
    startedAt: string
    stepCount?: number
    currentStatus?: string
  } | undefined
  now: number
}): LastRunPresentation {
  if (input.running) {
    const elapsed = formatDurationMs(Math.max(0, input.now - Date.parse(input.running.startedAt)))
    const stepPart =
      input.running.stepCount !== undefined ? ` · step ${input.running.stepCount}` : ''
    return {
      kind: 'running',
      text: input.running.currentStatus === 'queued'
        ? `Queued · ${elapsed}`
        : `Running · ${elapsed}${stepPart}`,
      jobId: input.running.jobId,
    }
  }

  if (!input.historyLoaded) {
    if (input.historyError && !input.latest) {
      return { kind: 'unavailable' }
    }
    return { kind: 'loading' }
  }

  if (!input.latest) {
    if (input.historyError) {
      return { kind: 'unavailable' }
    }
    return { kind: 'none' }
  }

  const status = String(input.latest.status).toLowerCase()
  const detail = runDetailLine(input.latest)
  const runId = input.latest.id

  if (status === 'success') {
    const text = `Success · ${formatDurationMs(input.latest.durationMs ?? 0)}`
    return {
      kind: 'success',
      text,
      detail,
      runId,
    }
  }

  if (status === 'skipped') {
    const text = `Skipped · ${formatRelativeTime(input.latest.startedAt, input.now)}`
    return {
      kind: 'terminal',
      text,
      detail,
      runId,
    }
  }

  const steps = input.latest.stepCount ?? 0
  const stepsText = `${steps} steps`
  const relTime = formatRelativeTime(input.latest.startedAt, input.now)

  let statusWord: string
  if (status === 'failure') {
    statusWord = 'Failed'
  } else if (status === 'timeout') {
    statusWord = 'Timed out'
  } else if (status === 'cancelled') {
    statusWord = 'Cancelled'
  } else {
    statusWord = status.charAt(0).toUpperCase() + status.slice(1)
  }

  const text = `${statusWord} · ${relTime} · ${stepsText}`
  return {
    kind: 'terminal',
    text,
    detail,
    runId,
  }
}

export function projectTaskListEmpty(input: {
  taskCount: number
  matchCount: number
}): 'no-tasks' | 'no-match' | 'list' {
  if (input.taskCount === 0) return 'no-tasks'
  if (input.matchCount === 0) return 'no-match'
  return 'list'
}

export function filterHistoryItems(
  items: readonly HarnessHistorySummary[],
  filter: { status?: string; taskId?: string; jobType?: string },
): HarnessHistorySummary[] {
  const statusFilter = filter.status?.toLowerCase() ?? ''
  const taskIdFilter = filter.taskId ?? ''
  const jobTypeFilter = filter.jobType ?? ''

  return items.filter(item => {
    if (taskIdFilter && item.taskId !== taskIdFilter) return false
    if (jobTypeFilter && item.jobType !== jobTypeFilter) return false
    if (statusFilter && statusFilter !== 'all') {
      const itemStatus = String(item.status).toLowerCase()
      if (itemStatus !== statusFilter) return false
    }
    return true
  })
}

export function presentHistoryRow(
  item: HarnessHistorySummary,
  taskNames: Record<string, string> | undefined,
  now: number,
): {
  title: string
  status: 'success' | 'failure' | 'timeout' | 'cancelled' | 'skipped' | 'running' | 'other'
  relative: string
  duration: string
  steps: number
  detail?: string
} {
  const title = historyTaskTitle(item, taskNames)
  const rawStatus = String(item.status).toLowerCase()
  let status: 'success' | 'failure' | 'timeout' | 'cancelled' | 'skipped' | 'running' | 'other'
  if (
    rawStatus === 'success' ||
    rawStatus === 'failure' ||
    rawStatus === 'timeout' ||
    rawStatus === 'cancelled' ||
    rawStatus === 'skipped' ||
    rawStatus === 'running'
  ) {
    status = rawStatus
  } else {
    status = 'other'
  }

  const relative = formatRelativeTime(item.startedAt, now)
  const duration =
    status === 'skipped' && (!item.durationMs || item.durationMs === 0)
      ? '—'
      : formatDurationMs(item.durationMs ?? 0)
  const steps = item.stepCount ?? 0
  const detail = runDetailLine(item)

  return {
    title,
    status,
    relative,
    duration,
    steps,
    detail,
  }
}

export interface RetryOptions {
  maxRetries?: number
  delayMs?: number
  backoffFactor?: number
  onRetry?: (attempt: number, error: unknown) => void
  sleep?: (ms: number) => Promise<void>
}

export async function fetchWithRetry<T>(
  fn: () => Promise<T>,
  options: RetryOptions = {},
): Promise<T> {
  const {
    maxRetries = 2,
    delayMs = 1000,
    backoffFactor = 1.5,
    onRetry,
    sleep = ms => new Promise(resolve => setTimeout(resolve, ms)),
  } = options

  let attempt = 0
  while (true) {
    try {
      return await fn()
    } catch (err) {
      if (attempt >= maxRetries) {
        throw err
      }
      attempt++
      if (onRetry) {
        onRetry(attempt, err)
      }
      const waitTime = Math.round(delayMs * Math.pow(backoffFactor, attempt - 1))
      await sleep(waitTime)
    }
  }
}
