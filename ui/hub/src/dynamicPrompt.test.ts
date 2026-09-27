import assert from 'node:assert/strict'
import test from 'node:test'
import {
  clampMenuPosition,
  dynamicPromptTasks,
  formatSelectionExtraPrompt,
} from './dynamicPrompt.ts'

test('dynamicPromptTasks keeps only opted-in tasks and sorts by name', () => {
  const tasks = dynamicPromptTasks([
    { id: 'task-b', name: 'Beta', dynamicPrompt: true },
    { id: 'task-a', name: 'Alpha' },
    { id: 'task-c', name: '  ', dynamicPrompt: true },
    { id: 'task-d', name: 'Delta', dynamicPrompt: false },
  ])
  assert.deepEqual(tasks, [
    { id: 'task-b', name: 'Beta' },
    { id: 'task-c', name: 'task-c' },
  ])
})

test('formatSelectionExtraPrompt labels the source file and drops empty selections', () => {
  assert.equal(formatSelectionExtraPrompt('src/lib.rs', '   '), '')
  assert.equal(formatSelectionExtraPrompt('  ', 'fn main() {}'), 'fn main() {}')
  assert.equal(
    formatSelectionExtraPrompt(' src/lib.rs ', ' fn main() {} '),
    'Selected excerpt from `src/lib.rs`:\n\nfn main() {}',
  )
})

test('clampMenuPosition keeps the menu inside the viewport', () => {
  assert.deepEqual(clampMenuPosition(10, 20, 800, 600), { x: 10, y: 26 })
  assert.deepEqual(clampMenuPosition(900, 700, 800, 600), { x: 472, y: 312 })
  assert.deepEqual(clampMenuPosition(-40, -40, 200, 120), { x: 8, y: 8 })
})
