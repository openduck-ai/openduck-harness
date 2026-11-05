import { translate } from './i18n/index.ts'
import type { HarnessActiveJob, HarnessJobActivity } from '@aaif/goose-hub-core'
import { formatDurationMs } from './i18n/format.ts'
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

export type { HarnessActiveJob }
