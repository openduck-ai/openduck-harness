import { isProjectSubTab, type ProjectSubTab } from './lazyTabs.ts'

export type NavPage = 'projects' | 'running' | 'tui-agents' | 'settings'

/** Historical and live logs are mutually exclusive. */
export type LogTarget =
  | { kind: 'history'; fileName: string }
  | { kind: 'job'; jobId: string }

/** Turn counts the Agent Log pager offers. Zero means every turn. */
export const AGENT_LOG_PAGE_SIZES = [25, 50, 100, 0] as const

export const DEFAULT_AGENT_LOG_PAGE_SIZE = 50

/**
 * Nested destination for the primary project pane.
 * Split panes do not carry their own destination.
 * `logPage` is 1-based and null on the first page. `logSize` is null when the
 * saved page size should be used. Neither field changes which run is open.
 */
export type HubDestination =
  | { kind: 'history' }
  | {
      kind: 'log'
      target: LogTarget
      file: string | null
      logPage: number | null
      logSize: number | null
    }
  | { kind: 'file'; path: string }

export interface UrlNavState {
  page: NavPage
  project: string | null
  tab: ProjectSubTab | null
  split: string | null
  destination: HubDestination | null
}

export function isValidHistoryFileName(value: string): boolean {
  return isValidBareName(value)
}

export function isValidJobId(value: string): boolean {
  return isValidBareName(value)
}

/**
 * Project-relative file path. Rejects empty names, traversal, and forms the
 * file API would rewrite (leading slash, `.` segments, backslashes). Does not
 * trim or rewrite an accepted path.
 */
export function isValidProjectFilePath(value: string): boolean {
  if (!value || value !== value.trim()) return false
  if (value.includes('\0') || value.includes('\\') || value.includes(':')) return false
  if (value.startsWith('/') || value.endsWith('/')) return false
  const parts = value.split('/')
  if (parts.some(part => !part || part === '.' || part === '..')) return false
  return true
}

function isValidBareName(value: string): boolean {
  if (!value || value !== value.trim()) return false
  if (value.includes('\0') || value.includes('/') || value.includes('\\')) return false
  if (value === '.' || value === '..') return false
  return true
}

export function parseUrlNav(search: string): UrlNavState {
  const cleanSearch = search.startsWith('?') ? search.slice(1) : search
  const params = new URLSearchParams(cleanSearch)

  const pageParam = params.get('page')
  const page: NavPage =
    pageParam === 'settings'
      ? 'settings'
      : pageParam === 'running'
        ? 'running'
        : pageParam === 'tui-agents'
          ? 'tui-agents'
          : 'projects'

  const rawProject = params.get('project')?.trim() || null
  const project = rawProject || null

  const rawTab = params.get('tab')
  const tab: ProjectSubTab | null = isProjectSubTab(rawTab) ? rawTab : null

  const rawSplit = params.get('split')?.trim() || null
  const split = rawSplit || null

  return {
    page: project ? 'projects' : page,
    project,
    tab,
    split,
    destination: project ? parseDestination(params, tab) : null,
  }
}

function parseDestination(params: URLSearchParams, tab: ProjectSubTab | null): HubDestination | null {
  const file = validFileParam(params.get('file'))

  if (tab === 'files') {
    return file ? { kind: 'file', path: file } : null
  }

  if (tab !== 'harness') return null

  const view = params.get('view')
  if (view === 'history') return { kind: 'history' }

  if (view !== 'log') return null

  const history = validBareParam(params.get('history'))
  const job = validBareParam(params.get('job'))
  if ((history && job) || (!history && !job)) return { kind: 'history' }

  const paging = parseLogPaging(params)
  return {
    kind: 'log',
    target: history
      ? { kind: 'history', fileName: history }
      : { kind: 'job', jobId: job as string },
    file,
    logPage: paging.logPage,
    logSize: paging.logSize,
  }
}

function parseLogPaging(params: URLSearchParams): { logPage: number | null; logSize: number | null } {
  const logSize = parseLogSize(params.get('logSize'))
  const logPage = logSize === 0 ? null : parseLogPage(params.get('logPage'))
  return { logPage, logSize }
}

function parseLogPage(value: string | null): number | null {
  if (value === null || !/^[1-9]\d*$/.test(value)) return null
  const page = Number(value)
  if (!Number.isSafeInteger(page) || page < 1) return null
  return page
}

function parseLogSize(value: string | null): number | null {
  if (value === null || !/^(0|25|50|100)$/.test(value)) return null
  const size = Number(value)
  if (size === DEFAULT_AGENT_LOG_PAGE_SIZE) return null
  return size
}

function validFileParam(value: string | null): string | null {
  if (value === null || !isValidProjectFilePath(value)) return null
  return value
}

function validBareParam(value: string | null): string | null {
  if (value === null || !isValidBareName(value)) return null
  return value
}

export function formatUrlNavQuery(state: Partial<UrlNavState>): string {
  const params = new URLSearchParams()

  if (state.project) {
    params.set('project', state.project)
    if (state.tab) params.set('tab', state.tab)
    appendDestination(params, state.tab ?? null, state.destination ?? null)
    if (state.split) params.set('split', state.split)
  } else if (state.page === 'settings' || state.page === 'running' || state.page === 'tui-agents') {
    params.set('page', state.page)
  }

  const query = params.toString()
  return query ? `?${query}` : ''
}

function appendDestination(
  params: URLSearchParams,
  tab: ProjectSubTab | null,
  destination: HubDestination | null,
): void {
  if (!destination) return
  if (destination.kind === 'file') {
    if (tab === 'files' && isValidProjectFilePath(destination.path)) {
      params.set('file', destination.path)
    }
    return
  }
  if (tab !== 'harness') return
  if (destination.kind === 'history') {
    params.set('view', 'history')
    return
  }
  const target = destination.target
  const historyOk = target.kind === 'history' && isValidHistoryFileName(target.fileName)
  const jobOk = target.kind === 'job' && isValidJobId(target.jobId)
  if (!historyOk && !jobOk) {
    params.set('view', 'history')
    return
  }
  params.set('view', 'log')
  if (historyOk && target.kind === 'history') params.set('history', target.fileName)
  if (jobOk && target.kind === 'job') params.set('job', target.jobId)
  appendLogPaging(params, destination.logPage, destination.logSize)
  if (destination.file && isValidProjectFilePath(destination.file)) {
    params.set('file', destination.file)
  }
}

function appendLogPaging(
  params: URLSearchParams,
  logPage: number | null,
  logSize: number | null,
): void {
  if (logSize === 0) {
    params.set('logSize', '0')
    return
  }
  if (logPage != null && logPage >= 1) {
    params.set('logPage', String(logPage))
    const size = logSize === 25 || logSize === 100 || logSize === 50
      ? logSize
      : DEFAULT_AGENT_LOG_PAGE_SIZE
    params.set('logSize', String(size))
    return
  }
  if (logSize === 25 || logSize === 100) params.set('logSize', String(logSize))
}

/**
 * Page fields written when the user changes the Agent Log pager.
 * Page 1 is omitted unless `pinFirstPage` is set, which keeps a live log
 * on the first page instead of following the latest turn.
 */
export function logPagingFields(
  page: number,
  pageSize: number,
  pinFirstPage = false,
): { logPage: number | null; logSize: number | null } {
  if (!Number.isInteger(pageSize) || !AGENT_LOG_PAGE_SIZES.includes(pageSize as (typeof AGENT_LOG_PAGE_SIZES)[number])) {
    return { logPage: null, logSize: null }
  }
  if (pageSize === 0) return { logPage: null, logSize: 0 }
  const logPage = Number.isInteger(page) && page > 1
    ? page
    : pinFirstPage && page === 1
      ? 1
      : null
  if (pageSize === DEFAULT_AGENT_LOG_PAGE_SIZE) return { logPage, logSize: logPage != null ? pageSize : null }
  return { logPage, logSize: pageSize }
}

/**
 * Query sent to history detail and job inspect.
 * An omitted size uses `savedPageSize`. A page after the first without a size
 * uses the default size so the slice does not follow a local preference.
 * Size 0 loads every turn and sends no paging query.
 */
export function resolvedLogPaging(
  destination: HubDestination | null,
  savedPageSize: number,
): { page: number; limit: number } | undefined {
  const saved = AGENT_LOG_PAGE_SIZES.includes(savedPageSize as (typeof AGENT_LOG_PAGE_SIZES)[number])
    ? savedPageSize
    : DEFAULT_AGENT_LOG_PAGE_SIZE
  if (destination?.kind !== 'log') {
    return saved > 0 ? { page: 1, limit: saved } : undefined
  }
  const explicit = destination.logSize
  const size = explicit === 0 || explicit === 25 || explicit === 100
    ? explicit
    : destination.logPage != null
      ? DEFAULT_AGENT_LOG_PAGE_SIZE
      : saved
  if (size === 0) return undefined
  return { page: destination.logPage ?? 1, limit: size }
}

export function syncUrlNav(state: Partial<UrlNavState>, replace = false): void {
  if (typeof window === 'undefined' || !window.history) return

  const query = formatUrlNavQuery(state)
  const newUrl = `${window.location.pathname}${query}${window.location.hash}`
  const currentUrl = `${window.location.pathname}${window.location.search}${window.location.hash}`
  if (newUrl === currentUrl) return

  if (replace) window.history.replaceState(null, '', newUrl)
  else window.history.pushState(null, '', newUrl)
}

export function absoluteHubUrl(
  state: Partial<UrlNavState>,
  location: { origin: string; pathname: string } = window.location,
): string {
  return `${location.origin}${location.pathname}${formatUrlNavQuery(state)}`
}

/** Keep the open log and attach or replace its file preview. */
export function retainLogUnderFile(state: UrlNavState, file: string): UrlNavState {
  if (state.destination?.kind !== 'log' || !isValidProjectFilePath(file)) return state
  return { ...state, destination: { ...state.destination, file } }
}

/** Drop only the file preview. A log underneath stays selected. */
export function closePreview(state: UrlNavState): UrlNavState {
  if (state.destination?.kind === 'log') {
    return { ...state, destination: { ...state.destination, file: null } }
  }
  if (state.destination?.kind === 'file') {
    return { ...state, destination: null }
  }
  return state
}

/**
 * Changing project, tab, or top-level page drops nested destinations.
 * Split is preserved for project changes that stay on the projects page.
 */
export function clearDestinationForScopeChange(
  state: UrlNavState,
  change: { project?: string | null; tab?: ProjectSubTab | null; page?: NavPage },
): UrlNavState {
  const project = change.project !== undefined ? change.project : state.project
  const tab = change.tab !== undefined ? change.tab : state.tab
  const page = change.page !== undefined ? change.page : state.page
  const scopeChanged =
    project !== state.project || tab !== state.tab || page !== state.page || !project
  return {
    page: project ? 'projects' : page,
    project,
    tab: project ? tab : null,
    split: project && page === 'projects' ? state.split : null,
    destination: scopeChanged ? null : state.destination,
  }
}

export interface NavRequest {
  id: number
  projectSlug: string
  targetKey: string
}

/** Late history and inspect responses must match the latest project and target. */
export function requestStillCurrent(request: NavRequest, current: NavRequest): boolean {
  return (
    request.id === current.id &&
    request.projectSlug === current.projectSlug &&
    request.targetKey === current.targetKey
  )
}

/** Job id whose live stream should be open, or null when navigation has left it. */
export function liveSubscriptionTarget(destination: HubDestination | null): string | null {
  if (destination?.kind === 'log' && destination.target.kind === 'job') {
    return destination.target.jobId
  }
  return null
}

export function logTargetKey(destination: HubDestination | null): string {
  if (destination?.kind !== 'log') return ''
  return destination.target.kind === 'history'
    ? `history:${destination.target.fileName}`
    : `job:${destination.target.jobId}`
}

/** Identity of the visible Agent Log slice. Page changes must not reload the run. */
export function logPagingKey(destination: HubDestination | null): string {
  if (destination?.kind !== 'log') return ''
  return `${destination.logPage ?? ''}:${destination.logSize ?? ''}`
}
