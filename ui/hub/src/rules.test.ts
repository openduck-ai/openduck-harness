import assert from 'node:assert/strict'
import test from 'node:test'
import type { SourceEntry } from '@openduck/sdk'
import {
  buildRuleProperties,
  draftFromSource,
  draftsEqual,
  emptyRuleDraft,
  joinCommaList,
  parseRuleProperties,
  splitCommaList,
  validateRuleName,
} from './rules.ts'

test('splitCommaList trims and drops empty items', () => {
  assert.deepEqual(splitCommaList(' **/*.rs,  **/*.toml , '), ['**/*.rs', '**/*.toml'])
  assert.deepEqual(splitCommaList(''), [])
})

test('joinCommaList round-trips lists', () => {
  assert.equal(joinCommaList(['rust', 'style']), 'rust, style')
})

test('parseRuleProperties reads camelCase and snake_case flags', () => {
  const camel = parseRuleProperties({
    globs: ['**/*.rs'],
    alwaysApply: false,
    tags: 'rust, style',
    order: 4,
    icon: '🦀',
  })
  assert.deepEqual(camel.globs, ['**/*.rs'])
  assert.equal(camel.alwaysApply, false)
  assert.deepEqual(camel.tags, ['rust', 'style'])
  assert.equal(camel.order, 4)
  assert.deepEqual(camel.extra, { icon: '🦀' })

  const snake = parseRuleProperties({ always_apply: true, globs: 'src/**/*.ts' })
  assert.equal(snake.alwaysApply, true)
  assert.deepEqual(snake.globs, ['src/**/*.ts'])
})

test('draftFromSource defaults alwaysApply when globs are empty', () => {
  const source: SourceEntry = {
    type: 'rule',
    name: 'safety',
    description: 'Do not leak secrets',
    content: 'Never print API keys.',
    path: '/tmp/.agents/rules/safety.md',
    global: true,
    writable: true,
    properties: {},
  }
  const draft = draftFromSource(source)
  assert.equal(draft.alwaysApply, true)
  assert.equal(draft.writable, true)
  assert.equal(draft.globsText, '')
  assert.deepEqual(buildRuleProperties(draft).globs, [])
  assert.equal(buildRuleProperties(draft).alwaysApply, true)
})

test('empty drafts start writable and always applied', () => {
  const draft = emptyRuleDraft(false)
  assert.equal(draft.path, null)
  assert.equal(draft.global, false)
  assert.equal(draft.alwaysApply, true)
  assert.equal(draft.writable, true)
  assert.equal(draftsEqual(draft, emptyRuleDraft(false)), true)
  assert.equal(draftsEqual(draft, { ...draft, name: 'x' }), false)
})

test('buildRuleProperties parses comma-separated globs and tags on save', () => {
  const properties = buildRuleProperties({
    ...emptyRuleDraft(false),
    globsText: '**/*.rs, **/*.toml, ',
    tagsText: 'rust, style',
  })
  assert.deepEqual(properties.globs, ['**/*.rs', '**/*.toml'])
  assert.deepEqual(properties.tags, ['rust', 'style'])
})

test('validateRuleName rejects empty, long, and path-like names', () => {
  assert.equal(validateRuleName(' rust-style '), null)
  assert.match(validateRuleName(''), /required/)
  assert.match(validateRuleName('a'.repeat(81)), /80/)
  assert.match(validateRuleName('foo/bar'), /path separators/)
})
