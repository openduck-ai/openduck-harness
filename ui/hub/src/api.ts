import {
  apiUrl,
  createApi as createCoreApi,
  readErrorMessage,
} from '@aaif/goose-hub-core/rest'
import type {
  ApiOptions,
  HarnessJobInspect,
  TrajectoryPagingOptions,
  TuiAgentModelCatalog,
} from '@aaif/goose-hub-core'

export { apiUrl, normalizeHistoryResponse } from '@aaif/goose-hub-core/rest'
export type {
  ApiOptions,
  ProxyItem,
  TuiAgentModelCatalog,
  TuiCatalogModelEntry,
  SwitchTuiSessionModelResult,
} from '@aaif/goose-hub-core'
export type { Project, ProjectDetail, ProjectInput, ProjectKind } from '@aaif/goose-hub-core/types'

function formatPagingQuery(paging?: TrajectoryPagingOptions): string {
  if (!paging) return ''
  const params = new URLSearchParams()
  if (paging.page !== undefined) params.set('page', String(paging.page))
  if (paging.limit !== undefined) params.set('limit', String(paging.limit))
  if (paging.pageSize !== undefined) params.set('pageSize', String(paging.pageSize))
  if (paging.offset !== undefined) params.set('offset', String(paging.offset))
  const qs = params.toString()
  return qs ? `?${qs}` : ''
}

async function getJson<T>(baseUrl: string, secret: string, path: string): Promise<T> {
  const headers = new Headers({ Accept: 'application/json' })
  if (secret) headers.set('X-Secret-Key', secret)
  const response = await fetch(apiUrl(baseUrl, path), { headers })
  if (!response.ok) throw new Error(await readErrorMessage(response))
  return response.json() as Promise<T>
}

function hubConnection(): { baseUrl: string; secret: string } {
  if (typeof window === 'undefined') {
    return { baseUrl: '', secret: '' }
  }
  const stored = window.localStorage.getItem('goose-hub-url')?.trim() ?? ''
  return {
    baseUrl: stored || window.location.origin,
    secret: window.localStorage.getItem('goose-hub-secret') ?? '',
  }
}

export function inspectHarnessJob(
  jobId: string,
  paging?: TrajectoryPagingOptions,
): Promise<HarnessJobInspect> {
  const { baseUrl, secret } = hubConnection()
  return getJson<HarnessJobInspect>(
    baseUrl,
    secret,
    `/api/v1/harness/jobs/${encodeURIComponent(jobId)}${formatPagingQuery(paging)}`,
  )
}

export function inspectProjectHarnessJob(
  slug: string,
  jobId: string,
  paging?: TrajectoryPagingOptions,
): Promise<HarnessJobInspect> {
  const { baseUrl, secret } = hubConnection()
  return getJson<HarnessJobInspect>(
    baseUrl,
    secret,
    `/api/v1/projects/${encodeURIComponent(slug)}/harness/jobs/${encodeURIComponent(jobId)}${formatPagingQuery(paging)}`,
  )
}

async function postJson<T>(baseUrl: string, secret: string, path: string, body?: unknown): Promise<T> {
  const headers = new Headers({ Accept: 'application/json', 'Content-Type': 'application/json' })
  if (secret) headers.set('X-Secret-Key', secret)
  const response = await fetch(apiUrl(baseUrl, path), {
    method: 'POST',
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  if (!response.ok) throw new Error(await readErrorMessage(response))
  return response.json() as Promise<T>
}

async function patchJson<T>(baseUrl: string, secret: string, path: string, body?: unknown): Promise<T> {
  const headers = new Headers({ Accept: 'application/json', 'Content-Type': 'application/json' })
  if (secret) headers.set('X-Secret-Key', secret)
  const response = await fetch(apiUrl(baseUrl, path), {
    method: 'PATCH',
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  if (!response.ok) throw new Error(await readErrorMessage(response))
  return response.json() as Promise<T>
}

export interface ManagedTuiSession {
  id: string
  sessionName: string
  agentKind: string
  cwd: string
  model?: string
  activeModel?: string
  currentModel?: string
  effort?: string
  autoApprove: boolean
  proxyMode: string
  proxyId?: string
  effectiveProxy?: string
  accountId?: string
  createdByProjectId?: string
  createdByTaskId?: string
  createdByRunId?: string
  createdBySubtaskId?: string
  creationSource: string
  ownerRunId?: string
  ownershipMode: string
  generation: number
  lifecycleState: 'starting' | 'active' | 'retained' | 'reclaim_pending' | 'stopping' | 'reclaimed' | 'cleanup_failed'
  processState: 'alive' | 'exited' | 'unknown'
  tuiPaneState: string
  stopIntent?: string
  cleanupError?: string
  createdAt: string
  lastOperationAt?: string
  lastInputAt?: string
  lastProgressAt?: string
  lastObservedAt?: string
  idleSince?: string
  ownerTerminalAt?: string
  reclaimAfter?: string
  stoppedAt?: string
  reclaimedAt?: string
}

export interface SessionOperation {
  id: string
  sessionId: string
  sessionName: string
  operationType: string
  operator: string
  projectId?: string
  taskId?: string
  runId?: string
  subtaskId?: string
  summary?: string
  contentRef?: string
  status: string
  error?: string
  durationMs?: number
  startedAt: string
  finishedAt?: string
}

export interface AgentProfile {
  id: string
  agentKind: string
  defaultModel?: string
  defaultEffort?: string
  accountId?: string
  proxyMode: string
  proxyId?: string
  maxConcurrency?: number
  idleTimeoutMins?: number
  exitedRetentionMins?: number
  maxTurnTimeoutSecs?: number
  createdAt: string
  updatedAt: string
}

export interface TuiAccount {
  id: string
  provider: string
  accountName: string
  authRef?: string
  status: string
  createdAt: string
  updatedAt: string
}

export interface QuotaSnapshot {
  id: string
  poolId: string
  accountId?: string
  agentKind?: string
  modelPattern?: string
  quotaType: string
  unit: string
  usedAmount: number
  limitAmount?: number
  remainingPercent?: number
  windowStart?: string
  windowEnd?: string
  resetAt?: string
  source: string
  status: string
  observedAt: string
}

export function createApi(baseUrl: string, secret = '', options?: ApiOptions) {
  const api = createCoreApi(baseUrl, secret, options) as any
  api.inspectHarnessJob = (jobId: string, paging?: TrajectoryPagingOptions) =>
    getJson<HarnessJobInspect>(
      baseUrl,
      secret,
      `/api/v1/harness/jobs/${encodeURIComponent(jobId)}${formatPagingQuery(paging)}`,
    )
  api.inspectProjectHarnessJob = (
    slug: string,
    jobId: string,
    paging?: TrajectoryPagingOptions,
  ) =>
    getJson<HarnessJobInspect>(
      baseUrl,
      secret,
      `/api/v1/projects/${encodeURIComponent(slug)}/harness/jobs/${encodeURIComponent(jobId)}${formatPagingQuery(paging)}`,
    )

  api.listTuiAgents = () =>
    getJson<{ agents: AgentProfile[] }>(baseUrl, secret, '/api/v1/tui-agents')
  api.upsertTuiAgent = (profile: Partial<AgentProfile>) =>
    postJson<{ success: boolean; agent: AgentProfile }>(baseUrl, secret, '/api/v1/tui-agents', profile)
  api.getTuiAgent = (agentKind: string) =>
    getJson<{ agent: AgentProfile }>(baseUrl, secret, `/api/v1/tui-agents/${encodeURIComponent(agentKind)}`)
  api.patchTuiAgent = (agentKind: string, updates: Partial<AgentProfile>) =>
    patchJson<{ success: boolean; agent: AgentProfile }>(
      baseUrl,
      secret,
      `/api/v1/tui-agents/${encodeURIComponent(agentKind)}`,
      updates,
    )

  api.startManagedTuiSession = (data: {
    agent: string
    cwd?: string
    model?: string
    effort?: string
    autoApprove?: boolean
    resumeId?: string
    initialPrompt?: string
    sessionName?: string
    extraArgs?: string[]
    proxyMode?: string
    proxyId?: string
    httpsProxy?: string
    accountId?: string
    idleTimeoutMins?: number
    exitedRetentionMins?: number
    projectSlug?: string
    taskId?: string
    runId?: string
    subtaskId?: string
    creationSource?: string
  }) =>
    postJson<{ success: boolean; session: ManagedTuiSession; initialTurn?: any }>(
      baseUrl,
      secret,
      '/api/v1/tui-sessions',
      data,
    )

  api.listManagedTuiSessions = (query?: { projectId?: string; runId?: string; activeOnly?: boolean; limit?: number }) => {
    const params = new URLSearchParams()
    if (query?.projectId) params.set('projectId', query.projectId)
    if (query?.runId) params.set('runId', query.runId)
    if (query?.activeOnly !== undefined) params.set('activeOnly', String(query.activeOnly))
    if (query?.limit !== undefined) params.set('limit', String(query.limit))
    const qs = params.toString()
    return getJson<{ sessions: ManagedTuiSession[] }>(baseUrl, secret, `/api/v1/tui-sessions${qs ? `?${qs}` : ''}`)
  }
  api.getManagedTuiSession = (id: string) =>
    getJson<{ session: ManagedTuiSession; screen?: string; bindings?: any[] }>(
      baseUrl,
      secret,
      `/api/v1/tui-sessions/${encodeURIComponent(id)}`,
    )
  api.getManagedTuiSessionOperations = (id: string) =>
    getJson<{ operations: SessionOperation[] }>(
      baseUrl,
      secret,
      `/api/v1/tui-sessions/${encodeURIComponent(id)}/operations`,
    )
  api.listManagedTuiOperations = (query?: { sessionId?: string; runId?: string; projectId?: string; limit?: number; offset?: number }) => {
    const params = new URLSearchParams()
    if (query?.sessionId) params.set('sessionId', query.sessionId)
    if (query?.runId) params.set('runId', query.runId)
    if (query?.projectId) params.set('projectId', query.projectId)
    if (query?.limit !== undefined) params.set('limit', String(query.limit))
    if (query?.offset !== undefined) params.set('offset', String(query.offset))
    const qs = params.toString()
    return getJson<{ operations: SessionOperation[] }>(baseUrl, secret, `/api/v1/tui-operations${qs ? `?${qs}` : ''}`)
  }
  api.interruptManagedTuiSession = (id: string) =>
    postJson<{ success: boolean }>(baseUrl, secret, `/api/v1/tui-sessions/${encodeURIComponent(id)}/interrupt`)
  api.stopManagedTuiSession = (id: string, reason?: string) =>
    postJson<{ success: boolean }>(baseUrl, secret, `/api/v1/tui-sessions/${encodeURIComponent(id)}/stop`, { reason })
  api.takeoverManagedTuiSession = (id: string) =>
    postJson<{ success: boolean; session: ManagedTuiSession }>(baseUrl, secret, `/api/v1/tui-sessions/${encodeURIComponent(id)}/takeover`)
  api.transferManagedTuiSession = (id: string, targetRunId: string, projectId?: string, taskId?: string) =>
    postJson<{ success: boolean; session: ManagedTuiSession }>(
      baseUrl,
      secret,
      `/api/v1/tui-sessions/${encodeURIComponent(id)}/transfer`,
      { targetRunId, projectId, taskId },
    )
  api.extendManagedTuiSession = (id: string, additionalMins?: number) =>
    postJson<{ success: boolean; session: ManagedTuiSession }>(
      baseUrl,
      secret,
      `/api/v1/tui-sessions/${encodeURIComponent(id)}/extend`,
      { additionalMins },
    )

  api.listTuiAccounts = () =>
    getJson<{ accounts: TuiAccount[] }>(baseUrl, secret, '/api/v1/tui-accounts')
  api.upsertTuiAccount = (account: Partial<TuiAccount>) =>
    postJson<{ success: boolean; account: TuiAccount }>(baseUrl, secret, '/api/v1/tui-accounts', account)
  api.listTuiQuotas = (accountId: string) =>
    getJson<{ quotas: QuotaSnapshot[] }>(baseUrl, secret, `/api/v1/tui-accounts/${encodeURIComponent(accountId)}/quotas`)
  api.refreshTuiQuotas = (accountId: string) =>
    postJson<{ success: boolean; quotas: QuotaSnapshot[] }>(
      baseUrl,
      secret,
      `/api/v1/tui-accounts/${encodeURIComponent(accountId)}/quotas/refresh`,
    )

  api.getTuiAgentModels = async (agentKind: string, options?: { refresh?: boolean }): Promise<TuiAgentModelCatalog> => {
    const qs = options?.refresh ? '?refresh=true' : ''
    const res = await getJson<any>(
      baseUrl,
      secret,
      `/api/v1/tui-agents/${encodeURIComponent(agentKind)}/models${qs}`,
    )
    if (res && typeof res === 'object' && 'catalog' in res && res.catalog) {
      return res.catalog as TuiAgentModelCatalog
    }
    return res as TuiAgentModelCatalog
  }
  api.refreshTuiAgentModels = async (agentKind: string): Promise<TuiAgentModelCatalog> => {
    const res = await postJson<any>(
      baseUrl,
      secret,
      `/api/v1/tui-agents/${encodeURIComponent(agentKind)}/models/refresh`,
    )
    if (res && typeof res === 'object' && 'catalog' in res && res.catalog) {
      return res.catalog as TuiAgentModelCatalog
    }
    return res as TuiAgentModelCatalog
  }
  api.switchTuiSessionModel = (sessionId: string, model: string) =>
    postJson<{ success: boolean; activeModel?: string; session?: ManagedTuiSession; message?: string }>(
      baseUrl,
      secret,
      `/api/v1/tui-sessions/${encodeURIComponent(sessionId)}/model`,
      { model },
    )
  api.switchProjectTuiSessionModel = (slug: string, sessionId: string, model: string) =>
    postJson<{ success: boolean; activeModel?: string; session?: ManagedTuiSession; message?: string }>(
      baseUrl,
      secret,
      `/api/v1/projects/${encodeURIComponent(slug)}/tui-sessions/${encodeURIComponent(sessionId)}/model`,
      { model },
    )

  return api
}

