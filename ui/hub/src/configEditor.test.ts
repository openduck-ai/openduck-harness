import assert from 'node:assert/strict'
import test from 'node:test'
import type { AppConfigDocument } from '@aaif/goose-hub-core'
import {
  circuitValue,
  draftsFromDocument,
  harnessDraftUnchanged,
  harnessUpdateForSave,
  harnessUpdateFromDraft,
  judgeUpdateFromDraft,
  parseOptionalCount,
  parseScalarInput,
  validateNewConfigKey,
} from './configEditor.ts'

const document: AppConfigDocument = {
  provider: { value: 'openai', fromEnv: false },
  model: { value: 'gpt-4o', fromEnv: false },
  mode: { value: 'auto', fromEnv: false },
  harness: {
    maxTurns: { value: 40, fromEnv: false },
    stagnationThreshold: { value: null, fromEnv: false },
    explorationBudget: { value: null, fromEnv: true },
    periodicReviewInterval: { value: 10, fromEnv: false },
    circuitBreakerEnabled: { value: null, fromEnv: false },
  },
  judge: {
    configured: false,
    provider: 'laya',
    endpoint: 'http://localhost:8732/api/laya',
    timeoutMs: 350,
    fallbackOnError: true,
    points: [
      { id: 'turn.drift', mode: 'shadow' },
      { id: 'tool.risk', mode: 'active' },
    ],
  },
  entries: [{ key: 'OLLAMA_HOST', value: 'http://127.0.0.1:11434', fromEnv: false }],
}

test('parseOptionalCount accepts blanks and whole numbers', () => {
  assert.equal(parseOptionalCount('  ', 'Max turns'), null)
  assert.equal(parseOptionalCount('25', 'Max turns'), 25)
  assert.throws(() => parseOptionalCount('0', 'Max turns'), /at least 1/)
  assert.throws(() => parseOptionalCount('1.5', 'Max turns'), /whole number/)
})

test('parseScalarInput keeps the previous value type', () => {
  assert.equal(parseScalarInput('false', true), false)
  assert.equal(parseScalarInput('12', 3), 12)
  assert.equal(parseScalarInput('12', '3'), '12')
  assert.throws(() => parseScalarInput('nope', 3), /number/)
})

test('validateNewConfigKey rejects secrets and blank names', () => {
  assert.equal(validateNewConfigKey(' OLLAMA_HOST '), 'OLLAMA_HOST')
  assert.throws(() => validateNewConfigKey('OPENAI_API_KEY'), /API keys/)
  assert.throws(() => validateNewConfigKey(''), /setting name/)
})

test('harness draft skips env-owned fields when checking for edits', () => {
  const drafts = draftsFromDocument(document)
  assert.equal(harnessDraftUnchanged(document, drafts.harness), true)
  drafts.harness.explorationBudget = '0'
  assert.equal(harnessDraftUnchanged(document, drafts.harness), true)
  assert.equal(harnessUpdateForSave(document, drafts.harness).explorationBudget, null)
  drafts.harness.explorationBudget = '99'
  drafts.harness.maxTurns = '50'
  assert.equal(harnessDraftUnchanged(document, drafts.harness), false)
  assert.deepEqual(harnessUpdateFromDraft(drafts.harness), {
    maxTurns: 50,
    stagnationThreshold: null,
    explorationBudget: 99,
    periodicReviewInterval: 10,
    circuitBreakerEnabled: null,
  })
})

test('circuit value maps the three choices', () => {
  assert.equal(circuitValue('default'), null)
  assert.equal(circuitValue('on'), true)
  assert.equal(circuitValue('off'), false)
})

test('judge draft requires provider, endpoint, and timeout', () => {
  const drafts = draftsFromDocument(document)
  const update = judgeUpdateFromDraft(drafts.judge)
  assert.equal(update.provider, 'laya')
  assert.equal(update.timeoutMs, 350)
  assert.throws(() => judgeUpdateFromDraft({ ...drafts.judge, endpoint: '  ' }), /endpoint/)
})
