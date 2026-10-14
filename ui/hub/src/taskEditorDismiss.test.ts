import assert from 'node:assert/strict'
import test from 'node:test'
import { escapeClosesTaskEditor, type EscapeCloseInput } from './taskEditorDismiss.ts'

const base: EscapeCloseInput = {
  key: 'Escape',
  defaultPrevented: false,
  isComposing: false,
  imeLocked: false,
  mentionOpen: false,
  saving: false,
  targetInsideEditor: false,
}

test('Escape outside the task editor closes it', () => {
  assert.equal(escapeClosesTaskEditor(base), true)
})

test('Escape inside the editor does not close it', () => {
  assert.equal(escapeClosesTaskEditor({ ...base, targetInsideEditor: true }), false)
})

test('Escape dismisses an open mention menu without closing the editor', () => {
  assert.equal(escapeClosesTaskEditor({ ...base, mentionOpen: true }), false)
  assert.equal(escapeClosesTaskEditor({ ...base, defaultPrevented: true }), false)
})

test('Escape during IME composition does not close the editor', () => {
  assert.equal(escapeClosesTaskEditor({ ...base, isComposing: true }), false)
  assert.equal(escapeClosesTaskEditor({ ...base, imeLocked: true }), false)
})

test('Escape does not close the editor while a task is saving or for other keys', () => {
  assert.equal(escapeClosesTaskEditor({ ...base, saving: true }), false)
  assert.equal(escapeClosesTaskEditor({ ...base, key: 'Enter' }), false)
})
