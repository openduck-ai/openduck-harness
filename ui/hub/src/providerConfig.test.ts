import assert from 'node:assert/strict'
import test from 'node:test'
import type { ProvidersConfigResponse } from '@aaif/goose-hub-core'
import {
  addProvider,
  buildPutBody,
  draftFromResponse,
  fastModelLabel,
  findMatchingProxyId,
  normalizeFastModel,
  normalizeProxy,
  normalizeSecondaryModel,
  resolveProxyLabel,
  secondaryModelLabel,
  validateDraft,
  validateProxyId,
  validateProxyUrl,
} from './providerConfig.ts'

const sampleResponse: ProvidersConfigResponse = {
  majorProvider: { value: 'openai', fileValue: 'openai', fromEnv: false },
  secondaryProvider: { value: null, fileValue: null, fromEnv: false },
  modelFromEnv: false,
  fastModelFromEnv: false,
  secondaryModelFromEnv: false,
  availableProviders: ['anthropic', 'google', 'ollama'],
  providers: [
    {
      name: 'openai',
      enabled: true,
      configured: true,
      model: 'gpt-4o',
      modelEnvValue: null,
      fastModel: null,
      fastModelEnvValue: null,
      secondaryModel: null,
      secondaryModelEnvValue: null,
      resolvedFastModel: 'gpt-4o',
      fastModelUsesMain: true,
      suggestedFastModel: 'gpt-4o-mini',
    },
  ],
}

test('blank fast model normalizes to absent', () => {
  assert.equal(normalizeFastModel('', 'gpt-4o'), null)
  assert.equal(normalizeFastModel('   ', 'gpt-4o'), null)
  assert.equal(normalizeFastModel(null, 'gpt-4o'), null)
})

test('fast model equal to the entry model normalizes to absent', () => {
  assert.equal(normalizeFastModel('gpt-4o', 'gpt-4o'), null)
  assert.equal(normalizeFastModel(' gpt-4o ', 'gpt-4o'), null)
  assert.equal(normalizeFastModel('gpt-4o-mini', 'gpt-4o'), 'gpt-4o-mini')
})

test('blank secondary model normalizes to absent', () => {
  assert.equal(normalizeSecondaryModel(''), null)
  assert.equal(normalizeSecondaryModel('   '), null)
  assert.equal(normalizeSecondaryModel(null), null)
  assert.equal(normalizeSecondaryModel('claude-3-opus'), 'claude-3-opus')
})

test('resolved label is "uses model" when fast model is absent', () => {
  assert.equal(fastModelLabel(null, 'gpt-4o'), 'uses model')
  assert.equal(fastModelLabel('', 'gpt-4o'), 'uses model')
  assert.equal(fastModelLabel('gpt-4o', 'gpt-4o'), 'uses model')
  assert.equal(fastModelLabel('gpt-4o-mini', 'gpt-4o'), 'gpt-4o-mini')
})

test('label is "no secondary model" when secondary model is absent', () => {
  assert.equal(secondaryModelLabel(null), 'no secondary model')
  assert.equal(secondaryModelLabel(''), 'no secondary model')
  assert.equal(secondaryModelLabel('gpt-4.1'), 'gpt-4.1')
})

test('secondary equal to major is rejected', () => {
  const draft = draftFromResponse(sampleResponse)
  draft.secondaryProvider = 'openai'
  assert.throws(() => validateDraft(draft), /cannot equal major provider/)
})

test('majorProvider: null is the explicit clear; omitting the key is rejected', () => {
  const draft = draftFromResponse(sampleResponse)
  draft.majorProvider = null
  const putBody = buildPutBody(draft)
  assert.equal(putBody.majorProvider, null)
  assert.ok('majorProvider' in putBody)
})

test('disabled or not-yet-configured entry cannot be selected after the PUT is applied', () => {
  const draft = draftFromResponse(sampleResponse)
  draft.providers[0]!.enabled = false
  assert.throws(() => validateDraft(draft), /is disabled/)
})

test('empty model on a selected provider is rejected', () => {
  const draft = draftFromResponse(sampleResponse)
  draft.providers[0]!.model = '   '
  assert.throws(() => validateDraft(draft), /empty model/)
})

test('env-locked provider selectors are submitted as fileValue; locked model fields submit the row file model, fastModel, or secondaryModel', () => {
  const lockedResp: ProvidersConfigResponse = {
    ...sampleResponse,
    majorProvider: { value: 'env-provider', fileValue: 'openai', fromEnv: true },
    secondaryProvider: { value: 'env-secondary', fileValue: 'anthropic', fromEnv: true },
    modelFromEnv: true,
    fastModelFromEnv: true,
    secondaryModelFromEnv: true,
    providers: [
      {
        name: 'openai',
        enabled: true,
        configured: true,
        model: 'file-model-openai',
        modelEnvValue: 'env-model',
        fastModel: 'file-fast-openai',
        fastModelEnvValue: 'env-fast-model',
        secondaryModel: null,
        secondaryModelEnvValue: null,
        resolvedFastModel: 'env-fast-model',
        fastModelUsesMain: false,
        suggestedFastModel: null,
      },
      {
        name: 'anthropic',
        enabled: true,
        configured: true,
        model: 'file-model-anthropic',
        modelEnvValue: null,
        fastModel: null,
        fastModelEnvValue: 'env-fast-model',
        secondaryModel: 'file-sec-model',
        secondaryModelEnvValue: 'env-sec-model',
        resolvedFastModel: 'env-fast-model',
        fastModelUsesMain: false,
        suggestedFastModel: null,
      },
    ],
  }

  const draft = draftFromResponse(lockedResp)
  // Even if user edits draft UI inputs
  draft.majorProvider = 'different-provider'
  draft.secondaryProvider = 'different-secondary'
  draft.providers[0]!.model = 'user-typed-model'
  draft.providers[0]!.fastModel = 'user-typed-fast'
  draft.providers[1]!.model = 'user-typed-sec-model'

  const putBody = buildPutBody(draft)
  // Selectors must submit fileValue
  assert.equal(putBody.majorProvider, 'openai')
  assert.equal(putBody.secondaryProvider, 'anthropic')

  // Major model must submit fileModel
  const openaiRow = putBody.providers.find(p => p.name === 'openai')!
  assert.equal(openaiRow.model, 'file-model-openai')
  // Fast model locked submits fileFastModel
  assert.equal(openaiRow.fastModel, 'file-fast-openai')

  // Secondary model locked submits fileModel
  const anthropicRow = putBody.providers.find(p => p.name === 'anthropic')!
  assert.equal(anthropicRow.model, 'file-model-anthropic')
})

test('a locked fast model whose file value is absent submits null or "", not the env string', () => {
  const lockedResp: ProvidersConfigResponse = {
    ...sampleResponse,
    fastModelFromEnv: true,
    providers: [
      {
        name: 'openai',
        enabled: true,
        configured: true,
        model: 'gpt-4o',
        modelEnvValue: null,
        fastModel: null,
        fastModelEnvValue: 'env-fast-model',
        secondaryModel: null,
        secondaryModelEnvValue: null,
        resolvedFastModel: 'env-fast-model',
        fastModelUsesMain: false,
        suggestedFastModel: null,
      },
    ],
  }
  const draft = draftFromResponse(lockedResp)
  const putBody = buildPutBody(draft)
  const openaiRow = putBody.providers.find(p => p.name === 'openai')!
  assert.equal(openaiRow.fastModel, null)
})

test('adding a provider appends a name from availableProviders that is not already in the map', () => {
  const draft = draftFromResponse(sampleResponse)
  assert.equal(draft.providers.length, 1)
  assert.ok(draft.availableProviders.includes('anthropic'))

  const nextDraft = addProvider(draft, 'anthropic')
  assert.equal(nextDraft.providers.length, 2)
  assert.equal(nextDraft.providers[1]?.name, 'anthropic')
  assert.equal(nextDraft.providers[1]?.enabled, true)
  assert.equal(nextDraft.providers[1]?.model, '')
  assert.ok(!nextDraft.availableProviders.includes('anthropic'))
})

test('an empty provider list still renders the add control from availableProviders', () => {
  const emptyResp: ProvidersConfigResponse = {
    majorProvider: { value: null, fileValue: null, fromEnv: false },
    secondaryProvider: { value: null, fileValue: null, fromEnv: false },
    modelFromEnv: false,
    fastModelFromEnv: false,
    secondaryModelFromEnv: false,
    availableProviders: ['openai', 'anthropic', 'google'],
    providers: [],
  }
  const draft = draftFromResponse(emptyResp)
  assert.equal(draft.providers.length, 0)
  assert.equal(draft.availableProviders.length, 3)

  const withFirst = addProvider(draft, 'openai')
  assert.equal(withFirst.providers.length, 1)
  assert.equal(withFirst.providers[0]?.name, 'openai')
})

test('blank proxy normalizes to absent', () => {
  assert.equal(normalizeProxy(''), null)
  assert.equal(normalizeProxy('   '), null)
  assert.equal(normalizeProxy(null), null)
  assert.equal(normalizeProxy('http://127.0.0.1:2087'), 'http://127.0.0.1:2087')
  assert.equal(normalizeProxy('  socks5://127.0.0.1:1088  '), 'socks5://127.0.0.1:1088')
})

test('proxy is preserved in buildPutBody', () => {
  const resp: ProvidersConfigResponse = {
    ...sampleResponse,
    providers: [
      {
        ...sampleResponse.providers[0]!,
        proxy: 'http://127.0.0.1:2087',
      },
    ],
  }
  const draft = draftFromResponse(resp)
  assert.equal(draft.providers[0]?.proxy, 'http://127.0.0.1:2087')

  const putBody = buildPutBody(draft)
  assert.equal(putBody.providers[0]?.proxy, 'http://127.0.0.1:2087')
})

test('proxies list is loaded in draftFromResponse', () => {
  const resp: ProvidersConfigResponse = {
    ...sampleResponse,
    proxies: [
      { id: 'clash', url: 'http://127.0.0.1:7890', description: 'Clash proxy' },
    ],
  }
  const draft = draftFromResponse(resp)
  assert.equal(draft.proxies.length, 1)
  assert.equal(draft.proxies[0]?.id, 'clash')
  assert.equal(draft.proxies[0]?.url, 'http://127.0.0.1:7890')
})

test('validateProxyUrl checks protocol and non-empty', () => {
  assert.equal(validateProxyUrl(''), 'Proxy URL cannot be empty')
  assert.equal(validateProxyUrl('ftp://localhost'), 'Proxy URL must start with http://, https://, or socks5://')
  assert.equal(validateProxyUrl('http://127.0.0.1:7890'), null)
  assert.equal(validateProxyUrl('https://proxy.corp:8443'), null)
  assert.equal(validateProxyUrl('socks5://127.0.0.1:1088'), null)
})

test('validateProxyId checks characters and uniqueness', () => {
  const existing = [{ id: 'clash', url: 'http://127.0.0.1:7890' }]
  assert.equal(validateProxyId('', existing), 'Proxy ID cannot be empty')
  assert.equal(validateProxyId('clash/bad', existing), 'Proxy ID can only contain letters, numbers, underscores, and dashes')
  assert.equal(validateProxyId('clash', existing), 'Proxy ID "clash" already exists')
  assert.equal(validateProxyId('clash', existing, 'clash'), null)
  assert.equal(validateProxyId('socks_proxy-1', existing), null)
})

test('resolveProxyLabel and findMatchingProxyId resolve aliases', () => {
  const proxies = [
    { id: 'clash', url: 'http://127.0.0.1:7890' },
    { id: 'socks', url: 'socks5://127.0.0.1:1088' },
  ]
  assert.equal(resolveProxyLabel(null, proxies), 'no proxy')
  assert.equal(resolveProxyLabel('', proxies), 'no proxy')
  assert.equal(resolveProxyLabel('clash', proxies), 'clash (http://127.0.0.1:7890)')
  assert.equal(resolveProxyLabel('http://127.0.0.1:7890', proxies), 'clash (http://127.0.0.1:7890)')
  assert.equal(resolveProxyLabel('http://custom:1234', proxies), 'http://custom:1234')

  assert.equal(findMatchingProxyId('clash', proxies), 'clash')
  assert.equal(findMatchingProxyId('http://127.0.0.1:7890', proxies), 'clash')
  assert.equal(findMatchingProxyId('http://custom:1234', proxies), null)
})

