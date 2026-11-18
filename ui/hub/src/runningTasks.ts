import { translate } from './i18n/index.ts'
import type {
  HarnessActiveJob,
  HarnessHistorySummary,
  HarnessJobActivity,
  HarnessRunStatus,
} from '@aaif/goose-hub-core'
import { formatDurationMs, formatRelativeTime } from './i18n/format.ts'
import { describeToolCall } from './taskTrajectory.ts'

export interface RunningTaskFields {
  jobId: string
  taskId: string
  jobType: string
  description: string
  startedAt: string
  currentStatus: string
  provider?: string | null
  model?: string | null
  projectSlug?: string | null
  stepCount?: number | null
  activity?: HarnessJobActivity | null
}

export function runningTaskTitle(job: Pick<RunningTaskFields, 'description' | 'taskId'>): string {
  const name = job.description.trim()
  return name || job.taskId
}

export function runningProjectLabel(
  slug: string | null | undefined,
  titles: Readonly<Record<string, string>>,
): string {
  if (!slug) return translate('runningTasks.helper.harness')
  const title = titles[slug]?.trim()
  return title || slug
}

export function runningStatusKind(status: string): 'queued' | 'running' {
  return status.trim().toLowerCase() === 'queued' ? 'queued' : 'running'
}

export function runningStatusLabel(status: string): string {
  return runningStatusKind(status) === 'queued'
    ? translate('runningTasks.helper.queued')
    : translate('runningTasks.helper.running')
}

export function runningElapsedLabel(startedAt: string, now: number): string {
  const started = Date.parse(startedAt)
  const elapsed = formatDurationMs(Math.max(0, now - (Number.isNaN(started) ? now : started)))
  return elapsed
}

export function runningStepLabel(stepCount: number | null | undefined): string | undefined {
  if (typeof stepCount !== 'number' || stepCount <= 0) return undefined
  return translate(
    stepCount === 1 ? 'runningTasks.helper.steps_one' : 'runningTasks.helper.steps_other',
    { count: stepCount },
  )
}

export function runningActivityText(job: Pick<RunningTaskFields, 'currentStatus' | 'activity'>): string {
  if (runningStatusKind(job.currentStatus) === 'queued') {
    return translate('runningTasks.helper.waitingForSlot')
  }
  const activity = job.activity
  if (!activity) return translate('runningTasks.helper.running')
  switch (activity.phase) {
    case 'thinking':
      return translate('runningTasks.helper.thinking')
    case 'starting':
      return translate('runningTasks.helper.starting')
    case 'answer':
      return translate('runningTasks.helper.answer')
    case 'waiting': {
      const detail = activity.detail?.trim()
      return detail || translate('runningTasks.helper.waiting')
    }
    case 'tool': {
      if (!activity.toolName) return translate('runningTasks.helper.tool')
      const summary = describeToolCall(activity.toolName, activity.toolArguments ?? undefined)
      return summary || activity.toolName
    }
    default:
      return translate('runningTasks.helper.running')
  }
}

export function sortRunningTasks<T extends Pick<RunningTaskFields, 'startedAt' | 'jobId'>>(
  jobs: readonly T[],
): T[] {
  return [...jobs].sort((a, b) => {
    const byStart = b.startedAt.localeCompare(a.startedAt)
    if (byStart !== 0) return byStart
    return b.jobId.localeCompare(a.jobId)
  })
}

export function filterRunningTasks<T extends RunningTaskFields>(
  jobs: readonly T[],
  options: {
    query?: string
    projectSlug?: string
    projectTitles?: Readonly<Record<string, string>>
  } = {},
): T[] {
  const query = options.query?.trim().toLowerCase() ?? ''
  const projectSlug = options.projectSlug ?? ''
  const titles = options.projectTitles ?? {}
  const matched = jobs.filter(job => {
    if (projectSlug && (job.projectSlug ?? '') !== projectSlug) return false
    if (!query) return true
    const haystack = [
      runningTaskTitle(job),
      job.taskId,
      job.projectSlug ?? '',
      runningProjectLabel(job.projectSlug, titles),
      job.provider ?? '',
      job.model ?? '',
      job.jobType,
      job.currentStatus,
      runningActivityText(job),
    ]
      .join('\n')
      .toLowerCase()
    return haystack.includes(query)
  })
  return sortRunningTasks(matched)
}

export function runningProjectOptions(
  jobs: readonly Pick<RunningTaskFields, 'projectSlug'>[],
  titles: Readonly<Record<string, string>>,
): Array<{ slug: string; label: string }> {
  const slugs = new Set<string>()
  for (const job of jobs) {
    if (job.projectSlug) slugs.add(job.projectSlug)
  }
  return [...slugs]
    .map(slug => ({ slug, label: runningProjectLabel(slug, titles) }))
    .sort((a, b) => a.label.localeCompare(b.label) || a.slug.localeCompare(b.slug))
}

export type { HarnessActiveJob, HarnessHistorySummary }

export type HistoryStatusKind = 'success' | 'failure' | 'timeout' | 'cancelled' | 'skipped'
export type UnifiedStatusFilter = '' | 'active' | HistoryStatusKind

export type TaskListEntry =
  | {
      kind: 'active'
      key: string
      jobId: string
      taskId: string
      title: string
      startedAt: string
      projectSlug: string | null
      statusKind: 'running' | 'queued'
      status: string
      provider?: string | null
      model?: string | null
      jobType: string
      stepCount?: number | null
      activity?: HarnessJobActivity | null
      job: HarnessActiveJob
    }
  | {
      kind: 'history'
      key: string
      id: string
      fileName: string
      taskId: string
      title: string
      startedAt: string
      projectSlug: string | null
      statusKind: HistoryStatusKind
      status: HarnessRunStatus
      provider?: string | null
      model?: string | null
      jobType: string
      stepCount: number
      durationMs: number
      error?: string | null
      progressSummary?: string | null
      history: HarnessHistorySummary
    }

export function normalizeHistoryStatusKind(status: string): HistoryStatusKind {
  switch (status.trim().toLowerCase()) {
    case 'success':
      return 'success'
    case 'failure':
    case 'failed':
    case 'error':
      return 'failure'
    case 'timeout':
      return 'timeout'
    case 'skipped':
      return 'skipped'
    case 'cancelled':
    case 'canceled':
    default:
      return 'cancelled'
  }
}

export function historyStatusLabel(status: string): string {
  switch (normalizeHistoryStatusKind(status)) {
    case 'success':
      return translate('harness.history.success')
    case 'failure':
      return translate('harness.history.failed')
    case 'timeout':
      return translate('harness.history.timedOut')
    case 'skipped':
      return translate('harness.history.skipped')
    case 'cancelled':
      return translate('harness.history.cancelled')
  }
}

export function historyStartedLabel(startedAt: string, now: number): string {
  const parsed = Date.parse(startedAt)
  if (Number.isNaN(parsed)) return startedAt
  return formatRelativeTime(startedAt, now)
}

export function compareStartedAtDesc(aStartedAt: string, bStartedAt: string, aKey: string, bKey: string): number {
  const aTime = Date.parse(aStartedAt)
  const bTime = Date.parse(bStartedAt)
  if (!Number.isNaN(aTime) && !Number.isNaN(bTime) && aTime !== bTime) {
    return bTime - aTime
  }
  const byStart = bStartedAt.localeCompare(aStartedAt)
  if (byStart !== 0) return byStart
  return bKey.localeCompare(aKey)
}

export function mergeAndSortTasks(
  jobs: readonly HarnessActiveJob[],
  history: readonly HarnessHistorySummary[],
): TaskListEntry[] {
  const activeJobIds = new Set<string>()
  const activeTaskKeys = new Set<string>()
  const entries: TaskListEntry[] = []

  for (const job of jobs) {
    activeJobIds.add(job.jobId)
    activeTaskKeys.add(`${job.projectSlug ?? ''}::${job.taskId}::${job.startedAt}`)
    entries.push({
      kind: 'active',
      key: `job:${job.jobId}`,
      jobId: job.jobId,
      taskId: job.taskId,
      title: runningTaskTitle(job),
      startedAt: job.startedAt,
      projectSlug: job.projectSlug ?? null,
      statusKind: runningStatusKind(job.currentStatus),
      status: job.currentStatus,
      provider: job.provider,
      model: job.model,
      jobType: job.jobType,
      stepCount: job.stepCount,
      activity: job.activity,
      job,
    })
  }

  const seenHistoryKeys = new Set<string>()
  for (const item of history) {
    if (activeJobIds.has(item.id)) continue
    if (
      item.status === 'running' &&
      activeTaskKeys.has(`${item.projectSlug ?? ''}::${item.taskId}::${item.startedAt}`)
    ) {
      continue
    }
    const dedupKey = `${item.projectSlug ?? ''}::${item.fileName || item.id}`
    if (seenHistoryKeys.has(dedupKey)) continue
    seenHistoryKeys.add(dedupKey)

    entries.push({
      kind: 'history',
      key: `history:${dedupKey}`,
      id: item.id,
      fileName: item.fileName,
      taskId: item.taskId,
      title: item.taskName?.trim() || item.taskId,
      startedAt: item.startedAt,
      projectSlug: item.projectSlug ?? null,
      statusKind: normalizeHistoryStatusKind(item.status),
      status: item.status,
      provider: Object.keys(item.providerStats?.byProvider ?? {})[0] ?? null,
      model: Object.keys(item.providerStats?.byModel ?? {})[0] ?? null,
      jobType: item.jobType,
      stepCount: item.stepCount,
      durationMs: item.durationMs,
      error: item.error,
      progressSummary: item.progressSummary,
      history: item,
    })
  }

  return entries.sort((a, b) => compareStartedAtDesc(a.startedAt, b.startedAt, a.key, b.key))
}

export function filterTaskEntries(
  entries: readonly TaskListEntry[],
  options: {
    query?: string
    projectSlug?: string
    status?: UnifiedStatusFilter
    projectTitles?: Readonly<Record<string, string>>
  } = {},
): TaskListEntry[] {
  const query = options.query?.trim().toLowerCase() ?? ''
  const projectSlug = options.projectSlug ?? ''
  const status = options.status ?? ''
  const titles = options.projectTitles ?? {}

  return entries.filter(entry => {
    if (projectSlug && (entry.projectSlug ?? '') !== projectSlug) return false
    if (status === 'active' && entry.kind !== 'active') return false
    if (status && status !== 'active' && (entry.kind !== 'history' || entry.statusKind !== status)) {
      return false
    }
    if (!query) return true

    const haystack = [
      entry.title,
      entry.taskId,
      entry.projectSlug ?? '',
      runningProjectLabel(entry.projectSlug, titles),
      entry.provider ?? '',
      entry.model ?? '',
      entry.jobType,
      entry.status,
      entry.kind === 'active'
        ? runningActivityText(entry.job)
        : [entry.error ?? '', entry.progressSummary ?? '', entry.fileName].join('\n'),
    ]
      .join('\n')
      .toLowerCase()
    return haystack.includes(query)
  })
}

export function taskProjectOptions(
  entries: readonly Pick<TaskListEntry, 'projectSlug'>[],
  titles: Readonly<Record<string, string>>,
): Array<{ slug: string; label: string }> {
  return runningProjectOptions(entries, titles)
}

export interface TaskListPagination {
  page: number
  limit: number
  total: number
  totalPages: number
}

export function paginateTaskEntries<T>(
  entries: readonly T[],
  page: number,
  limit: number,
  totalOverride?: number,
): { items: T[]; pagination: TaskListPagination } {
  const safeLimit = Math.max(1, Math.floor(limit) || 20)
  const total = Math.max(entries.length, totalOverride ?? entries.length)
  const totalPages = Math.max(1, Math.ceil(total / safeLimit))
  const safePage = Math.min(Math.max(1, Math.floor(page) || 1), totalPages)
  const start = (safePage - 1) * safeLimit
  return {
    items: entries.slice(start, start + safeLimit),
    pagination: {
      page: safePage,
      limit: safeLimit,
      total,
      totalPages,
    },
  }
}
