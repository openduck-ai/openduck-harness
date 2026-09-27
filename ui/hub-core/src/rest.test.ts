import assert from 'node:assert/strict'
import test from 'node:test'
import { apiUrl, createApi, readErrorMessage } from './rest.ts'
import { defaultJobId } from './cron.ts'

test('apiUrl joins server and route without duplicate slashes', () => {
  assert.equal(apiUrl(' https://host.example/ ', 'api/v1/projects'), 'https://host.example/api/v1/projects')
  assert.equal(apiUrl('http://localhost:3000', '/api/v1/projects/acme%2Fapi'), 'http://localhost:3000/api/v1/projects/acme%2Fapi')
  assert.equal(apiUrl('1.13.3.104', '/api/v1/projects'), 'http://1.13.3.104/api/v1/projects')
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

test('defaults job ids to slug-name', () => {
  assert.equal(defaultJobId('acme-api', 'Nightly Test'), 'acme-api-nightly-test')
})

test('REST client unwraps JSON error bodies from failed harness runs', async () => {
  const originalFetch = globalThis.fetch
  globalThis.fetch = async () =>
    new Response(JSON.stringify({ error: 'Policy step 1 failed: Goose provider complete failed: Request failed: 401' }), {
      status: 500,
      headers: { 'content-type': 'application/json' },
    })
  try {
    await assert.rejects(
      () => createApi('http://host', 'secret').runProjectTask('zhanghu-keji', 'task-413704', {}),
      (err: unknown) => {
        assert.ok(err instanceof Error)
        assert.equal(
          err.message,
          'Policy step 1 failed: Goose provider complete failed: Request failed: 401',
        )
        return true
      },
    )
  } finally {
    globalThis.fetch = originalFetch
  }
})

test('readErrorMessage prefers JSON error field and falls back to raw text', async () => {
  const jsonError = await readErrorMessage(
    new Response(JSON.stringify({ error: 'Policy step 1 failed' }), { status: 500 }),
  )
  assert.equal(jsonError, 'Policy step 1 failed')

  const raw = await readErrorMessage(new Response('plain failure', { status: 502 }))
  assert.equal(raw, 'plain failure')

  const empty = await readErrorMessage(new Response(null, { status: 503 }))
  assert.equal(empty, 'Request failed (503)')
})

test('REST client calls file management and terminal endpoints', async () => {
  const originalFetch = globalThis.fetch
  const calls: { url: string; method: string; body?: string }[] = []
  globalThis.fetch = async (input, init) => {
    const req = new Request(input, init)
    const body = init?.body ? String(init.body) : undefined
    calls.push({ url: req.url, method: req.method, body })
    if (req.method === 'DELETE') {
      return new Response(null, { status: 204 })
    }
    return new Response(JSON.stringify({ success: true, currentPath: '', entries: [] }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }

  try {
    const api = createApi('http://host', 'secret')
    await api.listFiles('my-proj', 'src')
    await api.readFile('my-proj', 'src/main.rs')
    await api.writeFile('my-proj', 'src/main.rs', 'fn main() {}')
    await api.createFileOrDir('my-proj', { path: 'src/lib.rs', kind: 'file' })
    await api.deleteFile('my-proj', 'src/temp.txt')
    await api.renameFile('my-proj', { oldPath: 'a.txt', newPath: 'b.txt' })
    await api.execCommand('my-proj', { command: 'cargo check' })
    await api.getGitStatus('my-proj')
    await api.getGitLog('my-proj', 50)
    await api.getGitDiff('my-proj', 'src/main.rs', true)
    await api.getGitShow('my-proj', 'abc1234')
    await api.stageGitFiles('my-proj', ['src/main.rs'])
    await api.unstageGitFiles('my-proj', ['src/main.rs'])
    await api.discardGitFiles('my-proj', ['src/main.rs'])
    await api.commitGitChanges('my-proj', 'fix parser', ['src/main.rs'])
    await api.generateGitCommitMessage('my-proj', ['src/main.rs'])

    await api.readFileBytes('my-proj', 'assets/icon.png')

    assert.equal(calls[0].url, 'http://host/api/v1/projects/my-proj/files?path=src')
    assert.equal(calls[0].method, 'GET')

    assert.equal(calls[1].url, 'http://host/api/v1/projects/my-proj/files/content?path=src%2Fmain.rs')
    assert.equal(calls[1].method, 'GET')

    assert.equal(calls[2].url, 'http://host/api/v1/projects/my-proj/files/content')
    assert.equal(calls[2].method, 'PUT')
    assert.match(calls[2].body ?? '', /"fn main\(\) \{\}"/)

    assert.equal(calls[3].url, 'http://host/api/v1/projects/my-proj/files/create')
    assert.equal(calls[3].method, 'POST')

    assert.equal(calls[4].url, 'http://host/api/v1/projects/my-proj/files?path=src%2Ftemp.txt')
    assert.equal(calls[4].method, 'DELETE')

    assert.equal(calls[5].url, 'http://host/api/v1/projects/my-proj/files/rename')
    assert.equal(calls[5].method, 'POST')

    assert.equal(calls[6].url, 'http://host/api/v1/projects/my-proj/terminal/exec')
    assert.equal(calls[6].method, 'POST')
    assert.match(calls[6].body ?? '', /"cargo check"/)

    assert.equal(calls[7].url, 'http://host/api/v1/projects/my-proj/git/status')
    assert.equal(calls[7].method, 'GET')
    assert.equal(calls[8].url, 'http://host/api/v1/projects/my-proj/git/log?limit=50')
    assert.equal(calls[9].url, 'http://host/api/v1/projects/my-proj/git/diff?path=src%2Fmain.rs&staged=true')
    assert.equal(calls[10].url, 'http://host/api/v1/projects/my-proj/git/show?sha=abc1234')
    assert.equal(calls[11].url, 'http://host/api/v1/projects/my-proj/git/stage')
    assert.equal(calls[11].method, 'POST')
    assert.equal(calls[12].url, 'http://host/api/v1/projects/my-proj/git/unstage')
    assert.equal(calls[13].url, 'http://host/api/v1/projects/my-proj/git/discard')
    assert.equal(calls[14].url, 'http://host/api/v1/projects/my-proj/git/commit')
    assert.match(calls[14].body ?? '', /"fix parser"/)
    assert.equal(calls[15].url, 'http://host/api/v1/projects/my-proj/git/commit-message')
    assert.equal(calls[15].method, 'POST')
    assert.equal(calls[16].url, 'http://host/api/v1/projects/my-proj/files/bytes?path=assets%2Ficon.png')
    assert.equal(calls[16].method, 'GET')

    assert.equal(
      api.getFileBytesUrl('my-proj', 'assets/icon.png'),
      'http://host/api/v1/projects/my-proj/files/bytes?path=assets%2Ficon.png&token=secret',
    )
  } finally {
    globalThis.fetch = originalFetch
  }
})

test('REST client calls harness endpoints with correct routes and methods', async () => {
  const originalFetch = globalThis.fetch
  const calls: { url: string; method: string; body?: string }[] = []
  globalThis.fetch = async (input, init) => {
    const req = new Request(input, init)
    const body = init?.body ? String(init.body) : undefined
    calls.push({ url: req.url, method: req.method, body })
    return new Response(JSON.stringify({ availableDatasets: [], reports: [], cassettes: [] }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }

  try {
    const api = createApi('http://host', 'secret')
    await api.getHarnessOverview()
    await api.listActiveHarnessJobs()
    await api.inspectHarnessJob('job_1')
    await api.listHarnessHistory()
    await api.getHarnessHistoryDetail('run_1.json')
    await api.listHarnessReports()
    await api.getHarnessReport('eval_123')
    await api.listHarnessCassettes()
    await api.getHarnessCassette('cassette.json')
    await api.runHarnessEval({ dataset: 'test.yaml', concurrency: 2 })
    await api.runHarnessTask({ prompt: 'test prompt', maxTurns: 10 })
    await api.runHarnessReplay({ cassettePath: 'cassette.json' })
    await api.stopProjectTask('my-proj', 'task-1')
    await api.stopProjectHarnessJob('my-proj', 'job_1')
    await api.stopHarnessJob('job_2')
    await api.getProjectHarnessOverview('my-proj')
    await api.listProjectActiveJobs('my-proj')
    await api.inspectProjectHarnessJob('my-proj', 'job_1')
    await api.listProjectHistory('my-proj')
    await api.getProjectHistoryDetail('my-proj', 'run_1.json')

    assert.equal(calls[0].url, 'http://host/api/v1/harness/overview')
    assert.equal(calls[0].method, 'GET')

    assert.equal(calls[1].url, 'http://host/api/v1/harness/jobs')
    assert.equal(calls[1].method, 'GET')

    assert.equal(calls[2].url, 'http://host/api/v1/harness/jobs/job_1')
    assert.equal(calls[2].method, 'GET')

    assert.equal(calls[3].url, 'http://host/api/v1/harness/history')
    assert.equal(calls[3].method, 'GET')

    assert.equal(calls[4].url, 'http://host/api/v1/harness/history/run_1.json')
    assert.equal(calls[4].method, 'GET')

    assert.equal(calls[5].url, 'http://host/api/v1/harness/reports')
    assert.equal(calls[5].method, 'GET')

    assert.equal(calls[6].url, 'http://host/api/v1/harness/reports/eval_123')
    assert.equal(calls[6].method, 'GET')

    assert.equal(calls[7].url, 'http://host/api/v1/harness/cassettes')
    assert.equal(calls[7].method, 'GET')

    assert.equal(calls[8].url, 'http://host/api/v1/harness/cassettes/cassette.json')
    assert.equal(calls[8].method, 'GET')

    assert.equal(calls[9].url, 'http://host/api/v1/harness/eval')
    assert.equal(calls[9].method, 'POST')
    assert.match(calls[9].body ?? '', /"dataset":"test.yaml"/)

    assert.equal(calls[10].url, 'http://host/api/v1/harness/run')
    assert.equal(calls[10].method, 'POST')
    assert.match(calls[10].body ?? '', /"prompt":"test prompt"/)

    assert.equal(calls[11].url, 'http://host/api/v1/harness/replay')
    assert.equal(calls[11].method, 'POST')
    assert.match(calls[11].body ?? '', /"cassettePath":"cassette.json"/)

    assert.equal(calls[12].url, 'http://host/api/v1/projects/my-proj/harness/tasks/task-1/stop')
    assert.equal(calls[12].method, 'POST')

    assert.equal(calls[13].url, 'http://host/api/v1/projects/my-proj/harness/jobs/job_1/stop')
    assert.equal(calls[13].method, 'POST')

    assert.equal(calls[14].url, 'http://host/api/v1/harness/jobs/job_2/stop')
    assert.equal(calls[14].method, 'POST')

    assert.equal(calls[15].url, 'http://host/api/v1/projects/my-proj/harness/overview')
    assert.equal(calls[15].method, 'GET')

    assert.equal(calls[16].url, 'http://host/api/v1/projects/my-proj/harness/jobs')
    assert.equal(calls[16].method, 'GET')

    assert.equal(calls[17].url, 'http://host/api/v1/projects/my-proj/harness/jobs/job_1')
    assert.equal(calls[17].method, 'GET')

    assert.equal(calls[18].url, 'http://host/api/v1/projects/my-proj/harness/history')
    assert.equal(calls[18].method, 'GET')

    assert.equal(calls[19].url, 'http://host/api/v1/projects/my-proj/harness/history/run_1.json')
    assert.equal(calls[19].method, 'GET')
  } finally {
    globalThis.fetch = originalFetch
  }
})

test('REST client patches project task schedules', async () => {
  const originalFetch = globalThis.fetch
  let request: Request | undefined
  globalThis.fetch = async (input, init) => {
    request = new Request(input, init)
    return new Response(JSON.stringify({ id: 'nightly', cron: '0 2 * * *', schedulePaused: true }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  try {
    const updated = await createApi('http://host', 'secret').patchProjectTaskSchedule(
      'my-proj',
      'nightly',
      { paused: true },
    )
    assert.equal(request?.url, 'http://host/api/v1/projects/my-proj/harness/tasks/nightly/schedule')
    assert.equal(request?.method, 'PATCH')
    assert.equal(updated.schedulePaused, true)
  } finally {
    globalThis.fetch = originalFetch
  }
})

test('REST client sends extraPrompt when running a dynamic-prompt task', async () => {
  const originalFetch = globalThis.fetch
  let request: Request | undefined
  globalThis.fetch = async (input, init) => {
    request = new Request(input, init)
    return new Response(JSON.stringify({ status: 'success' }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  try {
    await createApi('http://host', 'secret').runProjectTask('my-proj', 'task-review', {
      extraPrompt: 'Look at the parser',
    })
    assert.equal(
      request?.url,
      'http://host/api/v1/projects/my-proj/harness/tasks/task-review/run',
    )
    assert.equal(request?.method, 'POST')
    assert.match(await request!.text(), /"extraPrompt":"Look at the parser"/)
  } finally {
    globalThis.fetch = originalFetch
  }
})

function bodyInitToBytes(body: BodyInit | undefined): Uint8Array {
  if (body instanceof Uint8Array) return body
  if (body instanceof ArrayBuffer) return new Uint8Array(body)
  throw new Error(`unexpected fetch body type: ${Object.prototype.toString.call(body)}`)
}

test('REST client writes original file bytes instead of a UTF-8 JSON string', async () => {
  const originalFetch = globalThis.fetch
  let request: Request | undefined
  let capturedBody: BodyInit | undefined
  globalThis.fetch = async (input, init) => {
    capturedBody = init?.body as BodyInit | undefined
    request = new Request(input, init)
    return new Response(JSON.stringify({ success: true, path: 'uploads/spec.pdf' }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  try {
    const pdfBytes = new Uint8Array([0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x34, 0x0a, 0x89, 0x00])
    const pngBytes = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
    const api = createApi('http://host', 'secret')

    await api.writeFileBytes('my-proj', 'uploads/spec.pdf', pdfBytes)
    assert.equal(
      request?.url,
      'http://host/api/v1/projects/my-proj/files/bytes?path=uploads%2Fspec.pdf',
    )
    assert.equal(request?.method, 'PUT')
    assert.equal(request?.headers.get('Content-Type'), 'application/octet-stream')
    assert.notEqual(request?.headers.get('Content-Type'), 'application/json')
    const sentPdf = bodyInitToBytes(capturedBody)
    assert.deepEqual(Array.from(sentPdf), Array.from(pdfBytes))
    const asText = new TextDecoder().decode(sentPdf)
    assert.equal(asText.startsWith('{'), false)
    assert.equal(asText.includes('"content"'), false)

    await api.writeFileBytes('my-proj', 'uploads/icon.png', pngBytes)
    assert.equal(
      request?.url,
      'http://host/api/v1/projects/my-proj/files/bytes?path=uploads%2Ficon.png',
    )
    const sentPng = bodyInitToBytes(capturedBody)
    assert.deepEqual(Array.from(sentPng), Array.from(pngBytes))
    assert.equal(sentPng[0], 0x89)
  } finally {
    globalThis.fetch = originalFetch
  }
})

test('REST client supports lazy getProject and getProjectInsight', async () => {
  const originalFetch = globalThis.fetch
  const urls: string[] = []
  globalThis.fetch = async input => {
    urls.push(String(input))
    return new Response(JSON.stringify({ project: {}, insight: { exists: true, entryCount: 0, entries: [], truncated: false } }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  try {
    const api = createApi('http://host', 'secret')
    await api.getProject('my-proj')
    await api.getProject('my-proj', { lazy: true })
    await api.getProjectInsight('my-proj')
    assert.equal(urls[0], 'http://host/api/v1/projects/my-proj')
    assert.equal(urls[1], 'http://host/api/v1/projects/my-proj?lazy=true')
    assert.equal(urls[2], 'http://host/api/v1/projects/my-proj/insight')
  } finally {
    globalThis.fetch = originalFetch
  }
})
