import assert from 'node:assert/strict'
import test from 'node:test'
import { layoutGitGraph } from './gitGraph.ts'

test('lays out a linear history in a single column', () => {
  const rows = layoutGitGraph([
    { sha: 'C', parents: ['B'] },
    { sha: 'B', parents: ['A'] },
    { sha: 'A', parents: [] },
  ])
  assert.deepEqual(
    rows.map(row => row.column),
    [0, 0, 0],
  )
})

test('lays out a merge as a diamond', () => {
  const rows = layoutGitGraph([
    { sha: 'D', parents: ['B', 'C'] },
    { sha: 'B', parents: ['A'] },
    { sha: 'C', parents: ['A'] },
    { sha: 'A', parents: [] },
  ])
  assert.equal(rows[0].column, 0)
  assert.equal(rows[1].column, 0)
  assert.equal(rows[2].column, 1)
  assert.equal(rows[3].column, 0)
  assert.ok(rows[0].connections.some(lane => lane.from === 0 && lane.to === 1))
})
