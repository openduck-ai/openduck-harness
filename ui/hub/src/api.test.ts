import assert from 'node:assert/strict'
import test from 'node:test'
import { apiUrl, createApi } from './api.ts'

test('apiUrl joins server and route without duplicate slashes', () => {
  assert.equal(apiUrl(' https://host.example/ ', 'api/v1/projects'), 'https://host.example/api/v1/projects')
  assert.equal(apiUrl('http://localhost:3000', '/api/v1/projects/acme%2Fapi'), 'http://localhost:3000/api/v1/projects/acme%2Fapi')
})

test('REST client sends auth and JSON request headers', async () => {
  const originalFetch = globalThis.fetch
  let request: Request | undefined
  globalThis.fetch = async (input, init) => {
    request = new Request(input, init)
    return new Response(JSON.stringify({ projects: [] }), { status: 200, headers: { 'content-type': 'application/json' } })
  }
  try {
    await createApi('http://host', 'secret').listProjects()
    assert.equal(request?.url, 'http://host/api/v1/projects')
    assert.equal(request?.headers.get('X-Secret-Key'), 'secret')
    assert.equal(request?.headers.get('Accept'), 'application/json')
  } finally {
    globalThis.fetch = originalFetch
  }
})

test('REST client encodes project slugs', async () => {
  const originalFetch = globalThis.fetch
  let url = ''
  globalThis.fetch = async input => { url = String(input); return new Response(JSON.stringify({ project: {} }), { status: 200 }) }
  try {
    await createApi('http://host').getProject('a project')
    assert.equal(url, 'http://host/api/v1/projects/a%20project')
    await createApi('http://host').getProject('a project', { lazy: true })
    assert.equal(url, 'http://host/api/v1/projects/a%20project?lazy=true')
    await createApi('http://host').getProjectInsight('a project')
    assert.equal(url, 'http://host/api/v1/projects/a%20project/insight')
  } finally {
    globalThis.fetch = originalFetch
  }
})

test('createApi always exposes inspect methods for running jobs', async () => {
  const originalFetch = globalThis.fetch
  const urls: string[] = []
  globalThis.fetch = async input => {
    urls.push(String(input))
    return new Response(JSON.stringify({ job: {}, live: true, snapshot: null }), { status: 200 })
  }
  try {
    const api = createApi('http://host', 'secret')
    assert.equal(typeof api.inspectProjectHarnessJob, 'function')
    assert.equal(typeof api.inspectHarnessJob, 'function')
    await api.inspectProjectHarnessJob('my-proj', 'job_1')
    await api.inspectHarnessJob('job_1')
    assert.equal(urls[0], 'http://host/api/v1/projects/my-proj/harness/jobs/job_1')
    assert.equal(urls[1], 'http://host/api/v1/harness/jobs/job_1')

    await api.inspectProjectHarnessJob('my-proj', 'job_1', { page: 2, limit: 50 })
    await api.inspectHarnessJob('job_1', { page: 3, limit: 25 })
    assert.equal(urls[2], 'http://host/api/v1/projects/my-proj/harness/jobs/job_1?page=2&limit=50')
    assert.equal(urls[3], 'http://host/api/v1/harness/jobs/job_1?page=3&limit=25')
  } finally {
    globalThis.fetch = originalFetch
  }
})

test('createApi exposes TUI model catalog and switch methods', async () => {
  const originalFetch = globalThis.fetch
  const calls: Request[] = []
  globalThis.fetch = async (input, init) => {
    calls.push(new Request(input, init))
    const url = String(input)
    if (url.includes('/models')) {
      return new Response(JSON.stringify({ catalog: { agent: 'grok', models: [{ id: 'grok-4', displayName: 'Grok 4' }] } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      })
    }
    return new Response(JSON.stringify({ success: true, activeModel: 'grok-4' }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  try {
    const api = createApi('http://host', 'secret')
    const cat1 = await api.getTuiAgentModels('grok')
    assert.equal(cat1.agent, 'grok')
    assert.equal(cat1.models[0].id, 'grok-4')
    assert.equal(calls[0].url, 'http://host/api/v1/tui-agents/grok/models')
    assert.equal(calls[0].method, 'GET')
    assert.equal(calls[0].headers.get('X-Secret-Key'), 'secret')

    const cat2 = await api.getTuiAgentModels('grok', { refresh: true })
    assert.equal(calls[1].url, 'http://host/api/v1/tui-agents/grok/models?refresh=true')

    await api.refreshTuiAgentModels('grok')
    assert.equal(calls[2].url, 'http://host/api/v1/tui-agents/grok/models/refresh')
    assert.equal(calls[2].method, 'POST')

    const switchRes = await api.switchTuiSessionModel('s1', 'grok-4')
    assert.equal(calls[3].url, 'http://host/api/v1/tui-sessions/s1/model')
    assert.equal(calls[3].method, 'POST')
    assert.deepEqual(await calls[3].json(), { model: 'grok-4' })
    assert.equal(switchRes.success, true)
    assert.equal(switchRes.activeModel, 'grok-4')

    const projectSwitchRes = await api.switchProjectTuiSessionModel('my-proj', 's1', 'grok-4')
    assert.equal(calls[4].url, 'http://host/api/v1/projects/my-proj/tui-sessions/s1/model')
    assert.equal(calls[4].method, 'POST')
    assert.deepEqual(await calls[4].json(), { model: 'grok-4' })
    assert.equal(projectSwitchRes.success, true)
    assert.equal(projectSwitchRes.activeModel, 'grok-4')
  } finally {
    globalThis.fetch = originalFetch
  }
})

const quotaSnapshot = {
  id: 'q1',
  poolId: 'pool',
  accountId: null,
  agentKind: 'grok',
  modelPattern: null,
  quotaType: 'weekly',
  unit: 'percent',
  usedAmount: 1.5,
  limitAmount: null,
  remainingPercent: null,
  windowStart: null,
  windowEnd: null,
  resetAt: null,
  source: 'cli_status',
  status: 'unknown',
  observedAt: '2026-01-01T00:00:00Z',
}

test('tui quota client lists all and per-agent snapshots and refreshes one agent', async () => {
  const originalFetch = globalThis.fetch
  const calls: Request[] = []
  globalThis.fetch = async (input, init) => {
    const request = new Request(input, init)
    calls.push(request)
    const url = new URL(request.url)
    if (request.method === 'POST' && url.pathname.endsWith('/quotas/refresh')) {
      return new Response(JSON.stringify({ success: true, quotas: [{ ...quotaSnapshot, remainingPercent: 80 }] }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      })
    }
    return new Response(JSON.stringify({ quotas: [quotaSnapshot] }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  try {
    const api = createApi('http://host', 'secret')
    const agentId = 'codex/team+1'

    const all = await api.getTuiQuotas()
    assert.equal(all.success, undefined)
    assert.equal(all.quotas.length, 1)
    assert.equal(all.quotas[0].accountId, null)
    assert.equal(all.quotas[0].agentKind, 'grok')
    assert.equal(all.quotas[0].modelPattern, null)
    assert.equal(all.quotas[0].limitAmount, null)
    assert.equal(all.quotas[0].remainingPercent, null)
    assert.equal(all.quotas[0].windowStart, null)
    assert.equal(all.quotas[0].windowEnd, null)
    assert.equal(all.quotas[0].resetAt, null)
    assert.equal(calls[0].method, 'GET')
    assert.equal(calls[0].url, 'http://host/api/v1/tui-quotas')
    assert.equal(calls[0].headers.get('X-Secret-Key'), 'secret')
    assert.equal(calls[0].headers.get('Accept'), 'application/json')

    const legacy = await api.listAllTuiQuotas()
    assert.equal(legacy.success, undefined)
    assert.equal(legacy.quotas[0].id, 'q1')
    assert.equal(calls[1].method, 'GET')
    assert.equal(calls[1].url, 'http://host/api/v1/tui-quotas')

    const agent = await api.getTuiAgentQuotas(agentId)
    assert.equal(agent.success, undefined)
    assert.equal(agent.quotas[0].remainingPercent, null)
    assert.equal(calls[2].method, 'GET')
    assert.equal(calls[2].url, `http://host/api/v1/tui-agents/${encodeURIComponent(agentId)}/quotas`)

    const refreshed = await api.refreshTuiAgentQuotas(agentId)
    assert.equal(refreshed.success, true)
    assert.equal(refreshed.quotas[0].remainingPercent, 80)
    assert.equal(calls[3].method, 'POST')
    assert.equal(calls[3].url, `http://host/api/v1/tui-agents/${encodeURIComponent(agentId)}/quotas/refresh`)
    assert.equal(calls[3].headers.get('X-Secret-Key'), 'secret')
    assert.equal(calls[3].headers.get('Content-Type'), 'application/json')
    assert.equal(await calls[3].text(), '')

    const accountId = 'acc/id+1'
    const accountList = await api.listTuiQuotas(accountId)
    assert.equal(accountList.quotas[0].id, 'q1')
    assert.equal(calls[4].method, 'GET')
    assert.equal(calls[4].url, `http://host/api/v1/tui-accounts/${encodeURIComponent(accountId)}/quotas`)

    const accountRefresh = await api.refreshTuiQuotas(accountId)
    assert.equal(accountRefresh.success, true)
    assert.equal(accountRefresh.quotas[0].remainingPercent, 80)
    assert.equal(calls[5].method, 'POST')
    assert.equal(calls[5].url, `http://host/api/v1/tui-accounts/${encodeURIComponent(accountId)}/quotas/refresh`)

    assert.equal(
      calls.some((call) => call.method === 'POST' && new URL(call.url).pathname === '/api/v1/tui-quotas'),
      false,
    )
  } finally {
    globalThis.fetch = originalFetch
  }
})

test('tui quota client surfaces HTTP errors for list and refresh', async () => {
  const originalFetch = globalThis.fetch
  const calls: Request[] = []
  globalThis.fetch = async (input, init) => {
    const request = new Request(input, init)
    calls.push(request)
    const url = new URL(request.url)
    if (request.method === 'GET' && url.pathname === '/api/v1/tui-quotas') {
      return new Response(JSON.stringify({ error: 'quota store unavailable' }), { status: 400 })
    }
    if (request.method === 'GET' && url.pathname.endsWith('/quotas')) {
      return new Response(null, { status: 404 })
    }
    if (request.method === 'POST' && url.pathname.endsWith('/quotas/refresh')) {
      return new Response('plain failure', { status: 502 })
    }
    return new Response('unexpected', { status: 500 })
  }
  try {
    const api = createApi('http://host', 'secret')
    await assert.rejects(() => api.getTuiQuotas(), (err: unknown) => {
      assert.ok(err instanceof Error)
      assert.equal(err.message, 'quota store unavailable')
      return true
    })
    await assert.rejects(() => api.getTuiAgentQuotas('agy/custom agent'), (err: unknown) => {
      assert.ok(err instanceof Error)
      assert.equal(err.message, 'Request failed (404)')
      return true
    })
    await assert.rejects(() => api.refreshTuiAgentQuotas('agy/custom agent'), (err: unknown) => {
      assert.ok(err instanceof Error)
      assert.equal(err.message, 'plain failure')
      return true
    })
    await assert.rejects(() => api.listAllTuiQuotas(), /quota store unavailable/)
    await assert.rejects(() => api.refreshTuiQuotas('acc 1'), /plain failure/)

    assert.equal(calls[0].method, 'GET')
    assert.equal(calls[0].url, 'http://host/api/v1/tui-quotas')
    assert.equal(calls[1].method, 'GET')
    assert.equal(calls[1].url, `http://host/api/v1/tui-agents/${encodeURIComponent('agy/custom agent')}/quotas`)
    assert.equal(calls[2].method, 'POST')
    assert.equal(
      calls[2].url,
      `http://host/api/v1/tui-agents/${encodeURIComponent('agy/custom agent')}/quotas/refresh`,
    )
    assert.equal(calls[3].method, 'GET')
    assert.equal(calls[4].method, 'POST')
    assert.equal(calls[4].url, `http://host/api/v1/tui-accounts/${encodeURIComponent('acc 1')}/quotas/refresh`)
    assert.equal(
      calls.some((call) => call.method === 'POST' && new URL(call.url).pathname === '/api/v1/tui-quotas'),
      false,
    )
  } finally {
    globalThis.fetch = originalFetch
  }
})
