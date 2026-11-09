import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  absoluteHubUrl,
  clearDestinationForScopeChange,
  closePreview,
  formatUrlNavQuery,
  liveSubscriptionTarget,
  logPagingKey,
  logTargetKey,
  parseUrlNav,
  resolvedLogPaging,
  requestStillCurrent,
  retainLogUnderFile,
  type UrlNavState,
} from './urlNav.ts'

const defaults = {
  page: 'projects' as const,
  project: null,
  tab: null,
  split: null,
  destination: null,
}

test('parseUrlNav returns defaults for empty search string', () => {
  assert.deepEqual(parseUrlNav(''), defaults)
})

test('parseUrlNav parses settings page correctly', () => {
  assert.deepEqual(parseUrlNav('?page=settings'), { ...defaults, page: 'settings' })
})

test('parseUrlNav parses tui-agents page correctly', () => {
  assert.deepEqual(parseUrlNav('?page=tui-agents'), { ...defaults, page: 'tui-agents' })
})

test('parseUrlNav parses project and subtab accurately', () => {
  assert.deepEqual(parseUrlNav('?project=openduck&tab=chat'), {
    ...defaults,
    project: 'openduck',
    tab: 'chat',
  })
})

test('parseUrlNav ignores invalid subtab values', () => {
  assert.deepEqual(parseUrlNav('?project=openduck&tab=unknown_tab'), {
    ...defaults,
    project: 'openduck',
  })
})

test('parseUrlNav extracts split project parameter', () => {
  assert.deepEqual(parseUrlNav('?project=proj-a&tab=files&split=proj-b'), {
    ...defaults,
    project: 'proj-a',
    tab: 'files',
    split: 'proj-b',
  })
})

test('parseUrlNav parses the running tasks page', () => {
  assert.deepEqual(parseUrlNav('?page=running'), { ...defaults, page: 'running' })
})

test('formatUrlNavQuery formats settings correctly', () => {
  assert.equal(formatUrlNavQuery({ page: 'settings' }), '?page=settings')
})

test('formatUrlNavQuery formats the running tasks page', () => {
  assert.equal(formatUrlNavQuery({ page: 'running' }), '?page=running')
})

test('formatUrlNavQuery formats project and tab query', () => {
  assert.equal(formatUrlNavQuery({ project: 'my-project', tab: 'git' }), '?project=my-project&tab=git')
})

test('formatUrlNavQuery includes split parameter when provided', () => {
  assert.equal(
    formatUrlNavQuery({ project: 'main-app', tab: 'terminal', split: 'side-app' }),
    '?project=main-app&tab=terminal&split=side-app',
  )
})

test('formatUrlNavQuery returns empty string for default all projects', () => {
  assert.equal(formatUrlNavQuery({ page: 'projects', project: null }), '')
})

function roundTrip(search: string): UrlNavState {
  const state = parseUrlNav(search)
  return parseUrlNav(formatUrlNavQuery(state))
}

test('deep link destinations round-trip through the canonical query', () => {
  const cases: Array<[string, UrlNavState['destination']]> = [
    ['?project=my-project&tab=harness&view=history', { kind: 'history' }],
    [
      '?project=my-project&tab=harness&view=log&history=run.json',
      { kind: 'log', target: { kind: 'history', fileName: 'run.json' }, file: null, logPage: null, logSize: null },
    ],
    [
      '?project=my-project&tab=harness&view=log&job=job-id',
      { kind: 'log', target: { kind: 'job', jobId: 'job-id' }, file: null, logPage: null, logSize: null },
    ],
    [
      '?project=my-project&tab=files&file=docs/report.md',
      { kind: 'file', path: 'docs/report.md' },
    ],
    [
      '?project=my-project&tab=harness&view=log&history=run.json&file=docs/report.md',
      { kind: 'log', target: { kind: 'history', fileName: 'run.json' }, file: 'docs/report.md', logPage: null, logSize: null },
    ],
    [
      '?project=my-project&tab=harness&view=log&job=job-id&file=docs/report.md',
      { kind: 'log', target: { kind: 'job', jobId: 'job-id' }, file: 'docs/report.md', logPage: null, logSize: null },
    ],
  ]

  for (const [search, destination] of cases) {
    const state = parseUrlNav(search)
    assert.deepEqual(state.destination, destination, search)
    assert.deepEqual(parseUrlNav(formatUrlNavQuery(state)), state)
    assert.deepEqual(roundTrip(search), state)
  }
})

test('format keeps a split project beside a primary-pane destination', () => {
  const state = parseUrlNav('?project=my-project&tab=harness&view=history&split=other')
  assert.equal(state.split, 'other')
  assert.deepEqual(state.destination, { kind: 'history' })
  assert.equal(
    formatUrlNavQuery(state),
    '?project=my-project&tab=harness&view=history&split=other',
  )
})

test('reserved characters and unicode round-trip in project, history, and file values', () => {
  const state: UrlNavState = {
    page: 'projects',
    project: 'proj ect',
    tab: 'harness',
    split: null,
    destination: {
      kind: 'log',
      target: { kind: 'history', fileName: 'run #1+&.json' },
      file: 'docs/报告 draft.md',
      logPage: null,
      logSize: null,
    },
  }
  const query = formatUrlNavQuery(state)
  assert.match(query, /project=proj\+ect/)
  assert.match(query, /history=run\+%231%2B%26\.json|history=run\+%231%2B%26.json/)
  assert.deepEqual(parseUrlNav(query), state)
})

test('a log link with both history and job falls back to task history', () => {
  const state = parseUrlNav(
    '?project=my-project&tab=harness&view=log&history=run.json&job=job-id&file=docs/a.md',
  )
  assert.deepEqual(state.destination, { kind: 'history' })
  assert.equal(formatUrlNavQuery(state), '?project=my-project&tab=harness&view=history')
})

test('a log link with neither history nor job falls back to task history', () => {
  const state = parseUrlNav('?project=my-project&tab=harness&view=log')
  assert.deepEqual(state.destination, { kind: 'history' })
  assert.equal(formatUrlNavQuery(state), '?project=my-project&tab=harness&view=history')
})

test('unsupported views and incompatible destinations are dropped', () => {
  assert.equal(parseUrlNav('?project=my-project&tab=harness&view=nope&history=run.json').destination, null)
  assert.equal(
    formatUrlNavQuery(parseUrlNav('?project=my-project&tab=harness&view=nope&history=run.json')),
    '?project=my-project&tab=harness',
  )
  assert.equal(parseUrlNav('?project=my-project&tab=chat&file=docs/a.md').destination, null)
  assert.equal(parseUrlNav('?tab=harness&view=history').destination, null)
  assert.equal(parseUrlNav('?project=my-project&tab=files&file=../secret').destination, null)
  assert.equal(parseUrlNav('?project=my-project&tab=files&file=').destination, null)
  assert.equal(parseUrlNav('?project=my-project&tab=harness&view=log&history=../run.json').destination?.kind, 'history')
  assert.equal(
    formatUrlNavQuery(parseUrlNav('?project=my-project&tab=files&file=../secret&split=side')),
    '?project=my-project&tab=files&split=side',
  )
})

test('file preview keeps the underlying log and scope changes clear it', () => {
  const log = parseUrlNav('?project=my-project&tab=harness&view=log&history=run.json&split=side')
  const withFile = retainLogUnderFile(log, 'docs/report.md')
  assert.equal(withFile.destination?.kind, 'log')
  assert.equal(
    formatUrlNavQuery(withFile),
    '?project=my-project&tab=harness&view=log&history=run.json&file=docs%2Freport.md&split=side',
  )

  const closed = closePreview(withFile)
  assert.deepEqual(closed.destination, log.destination)
  assert.equal(closed.split, 'side')

  const switchedTab = clearDestinationForScopeChange(withFile, { tab: 'git' })
  assert.equal(switchedTab.destination, null)
  assert.equal(switchedTab.tab, 'git')
  assert.equal(switchedTab.split, 'side')

  const switchedProject = clearDestinationForScopeChange(withFile, { project: 'other' })
  assert.equal(switchedProject.destination, null)
  assert.equal(switchedProject.project, 'other')
  assert.equal(switchedProject.split, 'side')

  const files = parseUrlNav('?project=my-project&tab=files&file=docs/report.md')
  assert.equal(closePreview(files).destination, null)
})

test('agent log paging round-trips without using the top-level page param', () => {
  const paged = parseUrlNav(
    '?project=my-project&tab=harness&view=log&history=run.json&logPage=3&logSize=25',
  )
  assert.deepEqual(paged.destination, {
    kind: 'log',
    target: { kind: 'history', fileName: 'run.json' },
    file: null,
    logPage: 3,
    logSize: 25,
  })
  assert.equal(
    formatUrlNavQuery(paged),
    '?project=my-project&tab=harness&view=log&history=run.json&logPage=3&logSize=25',
  )
  assert.equal(paged.page, 'projects')

  const laterDefaultSize = parseUrlNav('?project=my-project&tab=harness&view=log&job=job-id&logPage=2')
  assert.equal(formatUrlNavQuery(laterDefaultSize), '?project=my-project&tab=harness&view=log&job=job-id&logPage=2&logSize=50')

  const firstPage = parseUrlNav(
    '?project=my-project&tab=harness&view=log&history=run.json&logPage=1&logSize=50',
  )
  assert.equal(
    formatUrlNavQuery(firstPage),
    '?project=my-project&tab=harness&view=log&history=run.json&logPage=1&logSize=50',
  )
  assert.equal(firstPage.destination?.kind === 'log' && firstPage.destination.logPage, 1)
  assert.equal(firstPage.destination?.kind === 'log' && firstPage.destination.logSize, null)

  const omitted = parseUrlNav('?project=my-project&tab=harness&view=log&history=run.json')
  assert.equal(formatUrlNavQuery(omitted), '?project=my-project&tab=harness&view=log&history=run.json')
  assert.equal(omitted.destination?.kind === 'log' && omitted.destination.logPage, null)

  const allTurns = parseUrlNav(
    '?project=my-project&tab=harness&view=log&history=run.json&logSize=0&logPage=4',
  )
  assert.equal(formatUrlNavQuery(allTurns), '?project=my-project&tab=harness&view=log&history=run.json&logSize=0')
  assert.equal(allTurns.destination?.kind === 'log' && allTurns.destination.logPage, null)
  assert.equal(allTurns.destination?.kind === 'log' && allTurns.destination.logSize, 0)

  const invalid = parseUrlNav(
    '?project=my-project&tab=harness&view=log&history=run.json&logPage=0&logSize=20&logSize=050',
  )
  assert.equal(formatUrlNavQuery(invalid), '?project=my-project&tab=harness&view=log&history=run.json')

  const withFile = parseUrlNav(
    '?project=my-project&tab=harness&view=log&history=run.json&logPage=2&logSize=100&file=docs/a.md&split=side',
  )
  assert.equal(
    formatUrlNavQuery(withFile),
    '?project=my-project&tab=harness&view=log&history=run.json&logPage=2&logSize=100&file=docs%2Fa.md&split=side',
  )

  const conflict = parseUrlNav(
    '?project=my-project&tab=harness&view=log&history=run.json&job=job-id&logPage=2&logSize=25',
  )
  assert.deepEqual(conflict.destination, { kind: 'history' })
  assert.equal(formatUrlNavQuery(conflict), '?project=my-project&tab=harness&view=history')
})

test('log paging stays on the same run and resolves the requested slice', () => {
  const first = parseUrlNav('?project=p&tab=harness&view=log&history=run.json').destination
  const paged = parseUrlNav(
    '?project=p&tab=harness&view=log&history=run.json&logPage=4&logSize=25',
  ).destination
  assert.equal(logTargetKey(first), logTargetKey(paged))
  assert.equal(logTargetKey(first), 'history:run.json')
  assert.notEqual(logPagingKey(first), logPagingKey(paged))

  const kept = retainLogUnderFile(
    parseUrlNav('?project=p&tab=harness&view=log&job=job-a&logPage=2&logSize=100'),
    'docs/a.md',
  )
  assert.equal(kept.destination?.kind === 'log' && kept.destination.logPage, 2)
  assert.equal(kept.destination?.kind === 'log' && kept.destination.logSize, 100)
  assert.equal(closePreview(kept).destination?.kind === 'log' && closePreview(kept).destination.logPage, 2)

  assert.deepEqual(resolvedLogPaging(first, 100), { page: 1, limit: 100 })
  assert.equal(
    resolvedLogPaging(parseUrlNav('?project=p&tab=harness&view=log&history=run.json&logSize=0').destination, 100),
    undefined,
  )
  assert.deepEqual(
    resolvedLogPaging(parseUrlNav('?project=p&tab=harness&view=log&history=run.json&logPage=3').destination, 100),
    { page: 3, limit: 50 },
  )
  assert.deepEqual(
    resolvedLogPaging(
      parseUrlNav('?project=p&tab=harness&view=log&history=run.json&logPage=3&logSize=25').destination,
      100,
    ),
    { page: 3, limit: 25 },
  )
  assert.deepEqual(
    resolvedLogPaging(parseUrlNav('?project=p&tab=harness&view=log&history=run.json&logSize=100').destination, 50),
    { page: 1, limit: 100 },
  )
  assert.deepEqual(resolvedLogPaging(null, 0), undefined)
})

test('stale detail responses and live subscriptions follow the latest target', () => {
  const latest = { id: 2, projectSlug: 'next', targetKey: 'job:b' }
  assert.equal(requestStillCurrent({ id: 1, projectSlug: 'next', targetKey: 'job:b' }, latest), false)
  assert.equal(requestStillCurrent({ id: 2, projectSlug: 'old', targetKey: 'job:b' }, latest), false)
  assert.equal(requestStillCurrent({ id: 2, projectSlug: 'next', targetKey: 'history:run.json' }, latest), false)
  assert.equal(requestStillCurrent(latest, latest), true)

  const live = parseUrlNav('?project=my-project&tab=harness&view=log&job=job-a').destination
  assert.equal(liveSubscriptionTarget(live), 'job-a')
  assert.equal(logTargetKey(live), 'job:job-a')
  const left = clearDestinationForScopeChange(
    parseUrlNav('?project=my-project&tab=harness&view=log&job=job-a'),
    { project: 'other' },
  )
  assert.equal(liveSubscriptionTarget(left.destination), null)
})

test('absolute hub urls use the deployment pathname and omit secrets', () => {
  const url = absoluteHubUrl(
    {
      page: 'projects',
      project: 'my-project',
      tab: 'harness',
      split: null,
      destination: { kind: 'history' },
    },
    { origin: 'https://hub.example', pathname: '/hub/' },
  )
  assert.equal(url, 'https://hub.example/hub/?project=my-project&tab=harness&view=history')
  assert.equal(url.includes('secret'), false)
})
