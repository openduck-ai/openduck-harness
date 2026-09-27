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
  } finally {
    globalThis.fetch = originalFetch
  }
})
