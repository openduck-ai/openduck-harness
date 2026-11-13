import assert from 'node:assert/strict'
import test from 'node:test'
import { commitAgentRefresh, loadGlobalQuotas, loadMissingAgentQuotas, mergeAgentQuotas, quotaView, runQuotaRefresh, quotasForAgent, type QuotaRefreshClient } from './tuiQuotaView.ts'

const NOW = Date.parse('2026-01-01T00:10:00Z')
const snap = (over: Record<string, unknown> = {}) => ({
  id: 'q1', poolId: 'p', agentKind: 'grok', quotaType: 'weekly', unit: 'percent',
  usedAmount: 0, source: 'cli', status: 'valid', observedAt: '2026-01-01T00:09:00Z', ...over,
}) as any

test('unknown percent renders neutral, never ok or exhausted', () => {
  const v = quotaView(snap({ status: 'unknown' }), NOW)
  assert.equal(v.tone, 'unknown')
  assert.equal(v.remainingPercent, null)
  assert.equal(quotaView(snap({ remainingPercent: undefined, status: 'valid' }), NOW).tone, 'unknown')
})

test('percent, exhausted, low and reset time', () => {
  assert.equal(quotaView(snap({ remainingPercent: 80 }), NOW).tone, 'ok')
  assert.equal(quotaView(snap({ remainingPercent: 10 }), NOW).tone, 'low')
  assert.equal(quotaView(snap({ remainingPercent: 0, status: 'exhausted' }), NOW).tone, 'exhausted')
  assert.equal(quotaView(snap({ resetAt: '2026-01-02T00:00:00Z' }), NOW).resetAt, '2026-01-02T00:00:00Z')
  assert.equal(quotaView(snap(), NOW).resetAt, null)
})

test('freshness from observedAt, explicit field, or unknown', () => {
  assert.equal(quotaView(snap(), NOW).freshness, 'fresh')
  assert.equal(quotaView(snap({ observedAt: '2026-01-01T00:00:00Z' }), NOW).freshness, 'stale')
  assert.equal(quotaView(snap({ freshness: 'stale' }), NOW).freshness, 'stale')
  assert.equal(quotaView(snap({ observedAt: 'bogus' }), NOW).freshness, 'unknown')
})

test('quotasForAgent matches case-insensitively', () => {
  assert.equal(quotasForAgent([snap(), snap({ agentKind: 'codex' })], 'GROK').length, 1)
})

test('per-agent refresh success replaces only that agent snapshots', async () => {
  const before = [snap({ remainingPercent: 10 }), snap({ id: 'c', agentKind: 'codex', remainingPercent: 50 })]
  const client = {
    refreshTuiAgentQuotas: async (id: string) => {
      assert.equal(id, 'grok')
      return { quotas: [snap({ id: 'q2', remainingPercent: 90 })] }
    },
  } as unknown as QuotaRefreshClient
  const res = await runQuotaRefresh(client, before, 'grok')
  assert.equal(res.error, null)
  assert.deepEqual(res.quotas.map(q => [q.id, q.remainingPercent]), [['c', 50], ['q2', 90]])
})

test('refresh failure preserves prior snapshots and reports error', async () => {
  const before = [snap({ remainingPercent: 42 })]
  const client = {
    refreshTuiAgentQuotas: async () => { throw new Error('boom') },
  } as unknown as QuotaRefreshClient
  const a = await runQuotaRefresh(client, before, 'grok')
  assert.equal(a.quotas, before)
  assert.equal(a.error, 'boom')
})

test('refresh with an empty quota list keeps the previous observation', async () => {
  const before = [snap({ remainingPercent: 40 })]
  const client = {
    refreshTuiAgentQuotas: async () => ({ quotas: [] }),
  } as unknown as QuotaRefreshClient
  const res = await runQuotaRefresh(client, before, 'grok')
  assert.equal(res.quotas, before)
  assert.equal(res.error, null)
})

test('refresh that returns the same observation does not replace snapshots', async () => {
  const before = [snap({ remainingPercent: 40 })]
  const client = {
    refreshTuiAgentQuotas: async () => ({ quotas: [snap({ remainingPercent: 40 })] }),
  } as unknown as QuotaRefreshClient
  const res = await runQuotaRefresh(client, before, 'grok')
  assert.equal(res.quotas, before)
  assert.equal(res.error, null)
})

test('loadGlobalQuotas preserves previous snapshots and reports error on failure', async () => {
  const prev = [snap({ id: 'c', agentKind: 'codex' })]
  const res = await loadGlobalQuotas({ getTuiQuotas: async () => { throw new Error('boom') } }, prev)
  assert.equal(res.quotas, prev)
  assert.equal(res.error, 'boom')
})

test('loadGlobalQuotas returns fresh snapshots on success', async () => {
  const next = [snap({ id: 'n', agentKind: 'claude' })]
  const res = await loadGlobalQuotas({ getTuiQuotas: async () => ({ quotas: next }) }, [snap({ id: 'c', agentKind: 'codex' })])
  assert.deepEqual(res, { quotas: next, error: null })
})

test('loadGlobalQuotas keeps previous snapshots when the response has no quotas', async () => {
  const prev = [snap({ id: 'c', agentKind: 'codex', remainingPercent: 40 })]
  const empty = await loadGlobalQuotas({ getTuiQuotas: async () => ({ quotas: [] }) }, prev)
  assert.equal(empty.quotas, prev)
  assert.equal(empty.error, null)
  const missing = await loadGlobalQuotas({ getTuiQuotas: async () => ({}) }, prev)
  assert.equal(missing.quotas, prev)
  assert.equal(missing.error, null)
})

test('mergeAgentQuotas keeps previous snapshots when refresh returns none', () => {
  const cur = [snap({ id: 'c', agentKind: 'codex' }), snap({ id: 'l', agentKind: 'claude' })]
  assert.equal(mergeAgentQuotas(cur, 'codex', []), cur)
})

test('commitAgentRefresh leaves a newer list in place when refresh did not change', () => {
  const baseline = [snap({ remainingPercent: 10 })]
  const latest = [snap({ remainingPercent: 10 }), snap({ id: 'c', agentKind: 'codex', remainingPercent: 50 })]
  assert.equal(commitAgentRefresh(latest, baseline, baseline, 'grok'), latest)
})

test('commitAgentRefresh applies only the refreshed agent onto the latest list', () => {
  const baseline = [snap({ remainingPercent: 10 }), snap({ id: 'c', agentKind: 'codex', remainingPercent: 50 })]
  const refreshed = mergeAgentQuotas(baseline, 'grok', [snap({ id: 'q2', remainingPercent: 90 })])
  const latest = [snap({ id: 'c', agentKind: 'codex', remainingPercent: 55 }), snap({ id: 'other', agentKind: 'claude', remainingPercent: 70 })]
  const res = commitAgentRefresh(latest, baseline, refreshed, 'grok')
  assert.deepEqual(res.map(q => [q.id, q.agentKind, q.remainingPercent]), [
    ['c', 'codex', 55],
    ['other', 'claude', 70],
    ['q2', 'grok', 90],
  ])
})

test('loadMissingAgentQuotas skips agents that already have snapshots', async () => {
  let calls = 0
  const current = [snap({ agentKind: 'grok', remainingPercent: 20 })]
  const res = await loadMissingAgentQuotas({
    getTuiAgentQuotas: async () => {
      calls += 1
      return { quotas: [] }
    },
  }, current, ['grok', 'GROK'])
  assert.equal(calls, 0)
  assert.equal(res.quotas, current)
  assert.equal(res.error, null)
})

test('loadMissingAgentQuotas merges a missing agent and keeps snapshots on empty or error', async () => {
  const current = [snap({ agentKind: 'codex', remainingPercent: 50 })]
  const loaded = await loadMissingAgentQuotas({
    getTuiAgentQuotas: async (id) => {
      assert.equal(id, 'grok')
      return { quotas: [snap({ id: 'g', agentKind: 'grok', remainingPercent: 80 })] }
    },
  }, current, ['codex', 'grok'])
  assert.equal(loaded.error, null)
  assert.deepEqual(loaded.quotas.map(q => [q.id, q.agentKind]), [['q1', 'codex'], ['g', 'grok']])

  const empty = await loadMissingAgentQuotas({
    getTuiAgentQuotas: async () => ({ quotas: [] }),
  }, current, ['grok'])
  assert.equal(empty.quotas, current)
  assert.equal(empty.error, null)

  const failed = await loadMissingAgentQuotas({
    getTuiAgentQuotas: async () => { throw new Error('detail down') },
  }, current, ['grok', 'agy'])
  assert.equal(failed.quotas, current)
  assert.equal(failed.error, 'detail down')
})
