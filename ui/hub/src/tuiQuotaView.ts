import type { QuotaSnapshot } from './api'

export const DEFAULT_STALE_AFTER_SECONDS = 300

export type QuotaTone = 'ok' | 'low' | 'exhausted' | 'unknown'
export type QuotaFreshness = 'fresh' | 'stale' | 'unknown'

export type QuotaSnapshotInput = QuotaSnapshot & {
  freshness?: string
  staleAfterSeconds?: number
}

export interface QuotaView {
  tone: QuotaTone
  freshness: QuotaFreshness
  remainingPercent: number | null
  resetAt: string | null
}

export interface QuotaRefreshClient {
  refreshTuiAgentQuotas(agentId: string): Promise<{ quotas?: QuotaSnapshot[] }>
}

const LOW_PERCENT = 20

function finitePercent(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? Math.min(100, Math.max(0, value)) : null
}

export function quotaFreshness(q: QuotaSnapshotInput, now: number = Date.now()): QuotaFreshness {
  if (q.freshness === 'fresh' || q.freshness === 'stale') return q.freshness
  const observed = Date.parse(q.observedAt)
  if (Number.isNaN(observed)) return 'unknown'
  const staleAfter = q.staleAfterSeconds ?? DEFAULT_STALE_AFTER_SECONDS
  return now - observed > staleAfter * 1000 ? 'stale' : 'fresh'
}

export function quotaView(q: QuotaSnapshotInput, now: number = Date.now()): QuotaView {
  const status = (q.status || '').toLowerCase()
  const percent = finitePercent(q.remainingPercent)
  let tone: QuotaTone
  if (status === 'exhausted') tone = 'exhausted'
  else if (percent === null) tone = 'unknown'
  else tone = percent <= LOW_PERCENT ? 'low' : 'ok'
  return {
    tone,
    freshness: quotaFreshness(q, now),
    remainingPercent: percent,
    resetAt: q.resetAt || null,
  }
}

export function quotasForAgent(quotas: QuotaSnapshot[], agentKind: string): QuotaSnapshot[] {
  const kind = agentKind.toLowerCase()
  return quotas.filter(q => q.agentKind?.toLowerCase() === kind)
}

export function mergeAgentQuotas(
  current: QuotaSnapshot[],
  agentKind: string,
  next: QuotaSnapshot[],
): QuotaSnapshot[] {
  if (next.length === 0) return current
  return [...current.filter(q => q.agentKind?.toLowerCase() !== agentKind.toLowerCase()), ...next]
}

function observationKey(q: QuotaSnapshot): string {
  return JSON.stringify({
    id: q.id,
    poolId: q.poolId,
    accountId: q.accountId ?? null,
    agentKind: (q.agentKind ?? '').toLowerCase(),
    modelPattern: q.modelPattern ?? null,
    quotaType: q.quotaType,
    unit: q.unit,
    usedAmount: q.usedAmount,
    limitAmount: q.limitAmount ?? null,
    remainingPercent: q.remainingPercent ?? null,
    windowStart: q.windowStart ?? null,
    windowEnd: q.windowEnd ?? null,
    resetAt: q.resetAt ?? null,
    source: q.source,
    status: q.status,
    observedAt: q.observedAt,
  })
}

export function sameAgentObservations(current: QuotaSnapshot[], agentKind: string, next: QuotaSnapshot[]): boolean {
  const prev = quotasForAgent(current, agentKind)
  if (prev.length !== next.length) return false
  const prevKeys = prev.map(observationKey).sort()
  const nextKeys = next.map(observationKey).sort()
  return prevKeys.every((key, index) => key === nextKeys[index])
}

export function commitAgentRefresh(
  latest: QuotaSnapshot[],
  baseline: QuotaSnapshot[],
  refreshed: QuotaSnapshot[],
  agentKind: string,
): QuotaSnapshot[] {
  if (refreshed === baseline) return latest
  return mergeAgentQuotas(latest, agentKind, quotasForAgent(refreshed, agentKind))
}

function errorMessage(e: unknown): string {
  return e instanceof Error && e.message ? e.message : String(e)
}

function applyQuotaList(previous: QuotaSnapshot[], incoming: QuotaSnapshot[] | undefined): QuotaSnapshot[] {
  if (!incoming || incoming.length === 0) return previous
  return incoming
}

export interface QuotaRefreshResult {
  quotas: QuotaSnapshot[]
  error: string | null
}

export async function runQuotaRefresh(
  client: QuotaRefreshClient,
  current: QuotaSnapshot[],
  agentKind: string,
): Promise<QuotaRefreshResult> {
  try {
    const res = await client.refreshTuiAgentQuotas(agentKind)
    const next = res.quotas ?? []
    if (next.length === 0 || sameAgentObservations(current, agentKind, next)) {
      return { quotas: current, error: null }
    }
    return { quotas: mergeAgentQuotas(current, agentKind, next), error: null }
  } catch (e) {
    return { quotas: current, error: errorMessage(e) }
  }
}

export interface QuotaLoadClient {
  getTuiQuotas(): Promise<{ quotas?: QuotaSnapshot[] }>
}

export async function loadGlobalQuotas(
  client: QuotaLoadClient,
  previous: QuotaSnapshot[],
): Promise<QuotaRefreshResult> {
  try {
    const res = await client.getTuiQuotas()
    return { quotas: applyQuotaList(previous, res.quotas), error: null }
  } catch (e) {
    return { quotas: previous, error: errorMessage(e) }
  }
}

export interface QuotaAgentClient {
  getTuiAgentQuotas(agentId: string): Promise<{ quotas?: QuotaSnapshot[] }>
}

export async function loadMissingAgentQuotas(
  client: QuotaAgentClient,
  current: QuotaSnapshot[],
  agentIds: string[],
): Promise<QuotaRefreshResult> {
  const missing: string[] = []
  const seen = new Set<string>()
  for (const id of agentIds) {
    const key = id.trim().toLowerCase()
    if (!key || seen.has(key)) continue
    seen.add(key)
    if (quotasForAgent(current, id).length === 0) missing.push(id)
  }
  if (missing.length === 0) return { quotas: current, error: null }

  let quotas = current
  const errors: string[] = []
  for (const id of missing) {
    try {
      const res = await client.getTuiAgentQuotas(id)
      const next = res.quotas ?? []
      if (next.length > 0) quotas = mergeAgentQuotas(quotas, id, next)
    } catch (e) {
      errors.push(errorMessage(e))
    }
  }
  const error = [...new Set(errors)].join('; ')
  return { quotas, error: error || null }
}
