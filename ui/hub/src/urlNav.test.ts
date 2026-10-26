import { test } from 'node:test'
import assert from 'node:assert/strict'
import { parseUrlNav, formatUrlNavQuery } from './urlNav.ts'

test('parseUrlNav returns defaults for empty search string', () => {
  const state = parseUrlNav('')
  assert.deepEqual(state, {
    page: 'projects',
    project: null,
    tab: null,
    split: null,
  })
})

test('parseUrlNav parses settings page correctly', () => {
  const state = parseUrlNav('?page=settings')
  assert.deepEqual(state, {
    page: 'settings',
    project: null,
    tab: null,
    split: null,
  })
})

test('parseUrlNav parses project and subtab accurately', () => {
  const state = parseUrlNav('?project=openduck&tab=chat')
  assert.deepEqual(state, {
    page: 'projects',
    project: 'openduck',
    tab: 'chat',
    split: null,
  })
})

test('parseUrlNav ignores invalid subtab values', () => {
  const state = parseUrlNav('?project=openduck&tab=unknown_tab')
  assert.deepEqual(state, {
    page: 'projects',
    project: 'openduck',
    tab: null,
    split: null,
  })
})

test('parseUrlNav extracts split project parameter', () => {
  const state = parseUrlNav('?project=proj-a&tab=files&split=proj-b')
  assert.deepEqual(state, {
    page: 'projects',
    project: 'proj-a',
    tab: 'files',
    split: 'proj-b',
  })
})

test('parseUrlNav parses the running tasks page', () => {
  const state = parseUrlNav('?page=running')
  assert.deepEqual(state, {
    page: 'running',
    project: null,
    tab: null,
    split: null,
  })
})

test('formatUrlNavQuery formats settings correctly', () => {
  const query = formatUrlNavQuery({ page: 'settings' })
  assert.equal(query, '?page=settings')
})

test('formatUrlNavQuery formats the running tasks page', () => {
  const query = formatUrlNavQuery({ page: 'running' })
  assert.equal(query, '?page=running')
})

test('formatUrlNavQuery formats project and tab query', () => {
  const query = formatUrlNavQuery({
    project: 'my-project',
    tab: 'git',
  })
  assert.equal(query, '?project=my-project&tab=git')
})

test('formatUrlNavQuery includes split parameter when provided', () => {
  const query = formatUrlNavQuery({
    project: 'main-app',
    tab: 'terminal',
    split: 'side-app',
  })
  assert.equal(query, '?project=main-app&tab=terminal&split=side-app')
})

test('formatUrlNavQuery returns empty string for default all projects', () => {
  const query = formatUrlNavQuery({
    page: 'projects',
    project: null,
  })
  assert.equal(query, '')
})
