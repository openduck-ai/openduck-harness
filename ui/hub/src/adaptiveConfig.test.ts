import assert from 'node:assert/strict'
import test from 'node:test'
import type { AppConfigDocument } from '@aaif/goose-hub-core'
import {
  ADAPTIVE_CONFIG_KEYS,
  adaptiveBaseProviderOptions,
  adaptiveCandidateModelOptions,
  adaptiveDraftFromDocument,
  adaptiveEntriesToUpsert,
  normalizeAdaptiveMode,
  validateAdaptiveDraft,
} from './adaptiveConfig.ts'
import { ConfigValidationError } from './configEditor.ts'
import type { ProviderConfigDraft } from './providerConfig.ts'

function makeDoc(entries: Array<{ key: string; value: string | number | boolean | null; fromEnv?: boolean }>): AppConfigDocument {
  return {
    path: '/tmp/config.yaml',
    mode: { value: 'auto', fromEnv: false },
    entries: entries.map((e) => ({
      key: e.key,
      value: e.value,
      fromEnv: e.fromEnv ?? false,
      secret: false,
    })),
    harness: {} as AppConfigDocument['harness'],
    judge: {} as AppConfigDocument['judge'],
  }
}

test('adaptiveDraftFromDocument applies defaults and respects fromEnv locks', () => {
  const doc = makeDoc([
    { key: ADAPTIVE_CONFIG_KEYS.regularModel, value: 'ark/deepseek-v4.1-flash' },
    { key: ADAPTIVE_CONFIG_KEYS.economyModel, value: 'ark/deepseek-v4.1-flash-economy', fromEnv: true },
    { key: ADAPTIVE_CONFIG_KEYS.mode, value: 'fixed_economy' },
  ])

  const draft = adaptiveDraftFromDocument(doc)
  assert.equal(draft.mode, 'economy')
  assert.equal(draft.regularModel, 'ark/deepseek-v4.1-flash')
  assert.equal(draft.economyModel, 'ark/deepseek-v4.1-flash-economy')
  assert.equal(draft.timezone, 'Asia/Shanghai')
  assert.equal(draft.offPeak, '01:00-07:00')
  assert.equal(draft.probeRatio, '0.10')
  assert.equal(draft.cooldownSeconds, '300')
  assert.equal(draft.fromEnvKeys.has(ADAPTIVE_CONFIG_KEYS.economyModel), true)
  assert.equal(draft.fromEnvKeys.has(ADAPTIVE_CONFIG_KEYS.regularModel), false)
})

test('validateAdaptiveDraft rejects missing models, bad probe ratio, cooldown, and off-peak windows', () => {
  const base = adaptiveDraftFromDocument(makeDoc([]))

  assert.throws(() => validateAdaptiveDraft(base), ConfigValidationError)

  const valid = {
    ...base,
    regularModel: 'ark/deepseek-v4.1-flash',
    economyModel: 'ark/deepseek-v4.1-flash-economy',
  }
  validateAdaptiveDraft(valid)

  assert.throws(
    () => validateAdaptiveDraft({ ...valid, probeRatio: '1.5' }),
    ConfigValidationError,
  )
  assert.throws(
    () => validateAdaptiveDraft({ ...valid, cooldownSeconds: '-1' }),
    ConfigValidationError,
  )
  assert.throws(
    () => validateAdaptiveDraft({ ...valid, offPeak: '25:00-07:00' }),
    ConfigValidationError,
  )
  // Midnight-wrapping window is valid
  validateAdaptiveDraft({ ...valid, offPeak: '23:00-06:00, 12:30-13:30' })
})

test('adaptiveEntriesToUpsert skips fromEnv keys and only emits changed entries', () => {
  const doc = makeDoc([
    { key: ADAPTIVE_CONFIG_KEYS.mode, value: 'auto' },
    { key: ADAPTIVE_CONFIG_KEYS.regularModel, value: 'ark/deepseek-v4.1-flash' },
    { key: ADAPTIVE_CONFIG_KEYS.economyModel, value: 'ark/deepseek-v4.1-flash-economy', fromEnv: true },
  ])
  const draft = {
    ...adaptiveDraftFromDocument(doc),
    mode: normalizeAdaptiveMode('regular'),
    economyModel: 'should-be-ignored-because-from-env',
  }

  const updates = adaptiveEntriesToUpsert(draft, doc)
  assert.equal(updates.some((u) => u.key === ADAPTIVE_CONFIG_KEYS.economyModel), false)
  assert.equal(updates.some((u) => u.key === ADAPTIVE_CONFIG_KEYS.regularModel), false)
  assert.deepEqual(
    updates.find((u) => u.key === ADAPTIVE_CONFIG_KEYS.mode),
    { key: ADAPTIVE_CONFIG_KEYS.mode, value: 'regular' },
  )
})

test('adaptiveCandidateModelOptions and adaptiveBaseProviderOptions list configured providers and exclude adaptive', () => {
  const providersDraft = {
    availableProviders: ['openai', 'adaptive'],
    providers: [
      {
        name: 'ark',
        model: 'deepseek-v4.1-flash',
        models: [
          { id: 'deepseek-v4.1-flash', name: 'DeepSeek V4.1 Flash' },
          { id: 'deepseek-v4.1-flash-economy', name: null },
        ],
      },
      {
        name: 'adaptive',
        model: 'auto',
        models: [{ id: 'auto', name: 'auto' }],
      },
    ],
  } as unknown as ProviderConfigDraft

  const candidates = adaptiveCandidateModelOptions(providersDraft)
  assert.deepEqual(
    candidates.map((c) => c.value),
    ['ark/deepseek-v4.1-flash', 'ark/deepseek-v4.1-flash-economy'],
  )

  const baseProviders = adaptiveBaseProviderOptions(providersDraft)
  assert.deepEqual(baseProviders, ['ark', 'openai'])
})
