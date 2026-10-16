import {
  apiUrl,
  createApi as createCoreApi,
  readErrorMessage,
} from '@aaif/goose-hub-core/rest'
import type {
  HarnessJobInspect,
  TrajectoryPagingOptions,
} from '@aaif/goose-hub-core'

export { apiUrl } from '@aaif/goose-hub-core/rest'
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

export function createApi(baseUrl: string, secret = '') {
  const api = createCoreApi(baseUrl, secret)
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
  return api
}

