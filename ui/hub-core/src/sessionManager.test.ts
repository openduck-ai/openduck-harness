import test from 'node:test'
import assert from 'node:assert/strict'
import { projectSessionRegistry } from './sessionManager.ts'

test('projectSessionRegistry isolates sessions by projectId', () => {
  const session1 = projectSessionRegistry.getOrCreate({
    baseUrl: 'http://localhost:3000',
    secretKey: 'test',
    projectId: 'project-alpha',
    cwd: '/path/to/alpha',
    client: 'goose-hub',
  })

  const session2 = projectSessionRegistry.getOrCreate({
    baseUrl: 'http://localhost:3000',
    secretKey: 'test',
    projectId: 'project-beta',
    cwd: '/path/to/beta',
    client: 'goose-hub',
  })

  assert.notEqual(session1, session2)
  assert.equal(session1.getState().projectId, 'project-alpha')
  assert.equal(session2.getState().projectId, 'project-beta')

  const all = projectSessionRegistry.getAllStates()
  assert.ok('project-alpha' in all)
  assert.ok('project-beta' in all)
  assert.equal(all['project-alpha']?.cwd, '/path/to/alpha')
  assert.equal(all['project-beta']?.cwd, '/path/to/beta')
})

test('restored session id is selected before its transcript is loaded', () => {
  const session = projectSessionRegistry.getOrCreate({
    baseUrl: 'http://localhost:3000',
    secretKey: 'test',
    projectId: 'project-restore',
    cwd: '/path/to/restore',
    client: 'goose-hub',
    sessionId: 'sess-last',
  })

  const state = session.getState()
  assert.equal(state.sessionId, 'sess-last')
  assert.equal(state.messages.length, 0)
  assert.equal(state.historyLoaded, false)
  assert.equal(state.statusLine, 'Loading session…')
})

test('hydrate does nothing when the project has no session yet', async () => {
  const session = projectSessionRegistry.getOrCreate({
    baseUrl: 'http://localhost:3000',
    secretKey: 'test',
    projectId: 'project-hydrate-empty',
    cwd: '/path/to/empty',
    client: 'goose-hub',
  })

  await session.hydrate()
  const state = session.getState()
  assert.equal(state.sessionId, null)
  assert.equal(state.messages.length, 0)
  assert.equal(state.historyLoaded, true)
  assert.equal(state.error, null)
})

test('projectSessionRegistry notifies subscribers on state changes', () => {
  let callCount = 0
  const unsubscribe = projectSessionRegistry.subscribe(() => {
    callCount += 1
  })

  const session = projectSessionRegistry.getOrCreate({
    baseUrl: 'http://localhost:3000',
    secretKey: 'test',
    projectId: 'project-gamma',
    cwd: '/path/to/gamma',
    client: 'goose-hub',
  })

  assert.ok(callCount >= 1)
  const countBefore = callCount
  session.clearPermissions()
  assert.ok(callCount > countBefore)

  unsubscribe()
})

test('cancelPrompt updates statusLine, clears permissions, and sends cancel', async () => {
  let cancelCalledWith: any = null
  const session = projectSessionRegistry.getOrCreate({
    baseUrl: 'http://localhost:3000',
    secretKey: 'test',
    projectId: 'project-cancel-test',
    cwd: '/path/to/cancel',
    client: 'goose-hub',
    sessionId: 'sess-to-cancel',
  })

  // Simulate active prompting state with a mock client
  session.isPrompting = true
  session.statusLine = 'Generating…'
  session.client = {
    cancel: async (params: any) => {
      cancelCalledWith = params
    },
  } as any

  let permissionCancelled = false
  session.permissionResolvers.set('perm-1', response => {
    if (response.outcome.outcome === 'cancelled') {
      permissionCancelled = true
    }
  })
  session.pendingPermission = {
    key: 'perm-1',
    request: { sessionId: 'sess-to-cancel' } as any,
  }

  await session.cancelPrompt()

  assert.equal(session.statusLine, 'Stopping…')
  assert.equal(session.pendingPermission, null)
  assert.equal(permissionCancelled, true)
  assert.deepEqual(cancelCalledWith, { sessionId: 'sess-to-cancel' })
})

test('projectSessionRegistry.cancelPrompt delegates to target session', async () => {
  let cancelled = false
  const session = projectSessionRegistry.getOrCreate({
    baseUrl: 'http://localhost:3000',
    secretKey: 'test',
    projectId: 'project-registry-cancel',
    cwd: '/path/to/reg',
    client: 'goose-hub',
    sessionId: 'sess-reg',
  })
  session.isPrompting = true
  session.client = {
    cancel: async () => {
      cancelled = true
    },
  } as any

  await projectSessionRegistry.cancelPrompt('project-registry-cancel')
  assert.equal(cancelled, true)
  assert.equal(session.statusLine, 'Stopping…')
})
