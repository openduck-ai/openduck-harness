import type { ProjectTaskSummary } from '@aaif/goose-hub-core'

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

export function filterAndSortProjectTasks(
  tasks: ProjectTaskSummary[],
  options: { query?: string; tag?: string } = {},
): ProjectTaskSummary[] {
  const query = options.query?.toLowerCase() ?? ''
  const tag = options.tag ?? ''

  return tasks
    .filter(task => {
      const taskTags = Array.isArray(task.tags) ? task.tags : []
      if (tag && !taskTags.includes(tag)) return false
      if (!query) return true
      return (
        task.id.toLowerCase().includes(query) ||
        (task.name && task.name.toLowerCase().includes(query)) ||
        (task.category && task.category.toLowerCase().includes(query))
      )
    })
    .sort((a, b) => {
      const byCreated = compareCreatedAtDesc(a.createdAt, b.createdAt)
      if (byCreated !== 0) return byCreated
      return a.id.localeCompare(b.id)
    })
}

function compareCreatedAtDesc(a?: string, b?: string): number {
  if (a === b) return 0
  if (!a) return 1
  if (!b) return -1
  return b.localeCompare(a)
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
