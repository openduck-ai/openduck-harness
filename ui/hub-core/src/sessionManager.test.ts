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
