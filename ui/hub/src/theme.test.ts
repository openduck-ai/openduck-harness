import assert from 'node:assert/strict'
import test from 'node:test'
import { getSystemTheme, resolveTheme } from './theme.ts'

test('resolveTheme returns direct preference for light or dark', () => {
  assert.equal(resolveTheme('light'), 'light')
  assert.equal(resolveTheme('dark'), 'dark')
})

test('resolveTheme resolves system to fallback when matchMedia is absent', () => {
  assert.equal(resolveTheme('system'), 'dark')
  assert.equal(getSystemTheme(), 'dark')
})
