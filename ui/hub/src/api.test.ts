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
