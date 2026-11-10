import assert from 'node:assert/strict'
import test from 'node:test'
import type { ProvidersConfigResponse } from '@aaif/goose-hub-core'
import {
  IDENTICAL_ROLE_TARGET_WARNING,
  addProvider,
  applyProviderInventoryIfCurrent,
  buildPutBody,
  draftFromResponse,
  fastModelLabel,
  filterModelOptions,
  findMatchingProxyId,
  identicalRoleTargetWarning,
  formatUsdPerMillion,
  modelChoiceState,
  modelPriceLabel,
  nextRefreshGeneration,
  normalizeFastModel,
  normalizeProxy,
  normalizeSecondaryModel,
  refreshGuardIsCurrent,
  resolveProxyLabel,
  roleProviderOptions,
  secondaryModelLabel,
  selectRoleModel,
  selectRoleProvider,
  updateProviderRow,
  usesFreeTextModelInput,
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

test('same provider can fill both roles', () => {
  const draft = selectRoleProvider(draftFromResponse(sampleResponse), 'secondary', 'openai')
  assert.doesNotThrow(() => validateDraft(draft))
  const putBody = buildPutBody(draft)
  assert.equal(putBody.majorProvider, 'openai')
  assert.equal(putBody.secondaryProvider, 'openai')
  assert.equal('selectionVersion' in putBody, false)
  assert.equal(identicalRoleTargetWarning(draft), IDENTICAL_ROLE_TARGET_WARNING)
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

function providerDetail(
  name: string,
  model: string,
  extra: Partial<ProvidersConfigResponse['providers'][number]> = {},
): ProvidersConfigResponse['providers'][number] {
  return {
    name,
    enabled: true,
    configured: true,
    model,
    modelEnvValue: null,
    fastModel: null,
    fastModelEnvValue: null,
    secondaryModel: null,
    secondaryModelEnvValue: null,
    resolvedFastModel: model,
    fastModelUsesMain: true,
    suggestedFastModel: null,
    ...extra,
  }
}

const arkInventory = [
  { id: 'glm-4.6', name: 'GLM 4.6' },
  { id: 'deepseek-v3', name: 'DeepSeek V3', recommended: true },
  { id: 'm1', name: 'M1' },
  { id: 'm2', name: 'M2' },
  { id: 'm3', name: 'M3' },
  { id: 'm4', name: 'M4' },
  { id: 'm5', name: 'M5' },
  { id: 'm6', name: 'M6' },
]

const v2Response: ProvidersConfigResponse = {
  selectionVersion: 2,
  majorProvider: { value: 'ark', fileValue: 'ark', fromEnv: false },
  secondaryProvider: { value: 'ark', fileValue: 'ark', fromEnv: false },
  majorModel: { value: 'deepseek-v3', fileValue: 'deepseek-v3', fromEnv: false, source: 'role' },
  secondaryProviderModel: { value: 'glm-4.6', fileValue: 'glm-4.6', fromEnv: false, source: 'role' },
  modelFromEnv: false,
  fastModelFromEnv: false,
  secondaryModelFromEnv: false,
  availableProviders: ['openai'],
  providers: [
    providerDetail('ark', '', {
      models: arkInventory,
      inventoryStatus: 'ready',
      supportsRefresh: true,
      refreshing: false,
    }),
    providerDetail('openai', 'gpt-4o', { models: [{ id: 'gpt-4o', name: 'GPT-4o' }] }),
  ],
}

test('provider candidates are enabled and configured and include the major provider', () => {
  const draft = draftFromResponse({
    ...v2Response,
    providers: [
      providerDetail('ark', ''),
      providerDetail('openai', '', { configured: false }),
      providerDetail('offline', 'gpt-4o', { enabled: false }),
    ],
  })
  const secondary = roleProviderOptions(draft, 'secondary').map(option => option.name)
  assert.deepEqual(secondary, ['ark'])
  assert.ok(secondary.includes(draft.majorProvider ?? ''))
})

test('v2 save sends both role models and omits an unchanged default model', () => {
  const draft = draftFromResponse(v2Response)
  assert.equal(draft.dirty, false)
  assert.equal(draft.majorModel, 'deepseek-v3')
  const putBody = buildPutBody(draft)
  assert.equal(putBody.selectionVersion, 2)
  if (putBody.selectionVersion !== 2) return
  assert.equal(putBody.majorProvider, 'ark')
  assert.equal(putBody.majorModel, 'deepseek-v3')
  assert.equal(putBody.secondaryProvider, 'ark')
  assert.equal(putBody.secondaryProviderModel, 'glm-4.6')
  assert.equal(identicalRoleTargetWarning(draft), null)
  const ark = putBody.providers.find(provider => provider.name === 'ark')
  assert.ok(ark)
  assert.equal('model' in ark, false)
  assert.equal(JSON.stringify(putBody).includes('"model":null'), false)
})

test('different role models on one provider do not warn', () => {
  const draft = draftFromResponse(v2Response)
  assert.equal(identicalRoleTargetWarning(draft), null)
  const same = selectRoleModel(draft, 'secondary', 'deepseek-v3')
  assert.equal(identicalRoleTargetWarning(same), IDENTICAL_ROLE_TARGET_WARNING)
})

test('v2 empty string clears a default model', () => {
  const draft = updateProviderRow(draftFromResponse({
    ...v2Response,
    providers: [providerDetail('openai', 'gpt-4o'), providerDetail('ark', '')],
  }), 0, { model: '' })
  const putBody = buildPutBody(selectRoleProvider(draft, 'secondary', null))
  if (putBody.selectionVersion !== 2) throw new Error('expected v2')
  assert.equal(putBody.providers.find(provider => provider.name === 'openai')?.model, '')
})

test('clearing a role provider forces a null role model', () => {
  const cleared = selectRoleProvider(draftFromResponse(v2Response), 'secondary', null)
  assert.equal(cleared.secondaryProvider, null)
  assert.equal(cleared.secondaryProviderModel, null)
  const putBody = buildPutBody(cleared)
  if (putBody.selectionVersion !== 2) throw new Error('expected v2')
  assert.equal(putBody.secondaryProvider, null)
  assert.equal(putBody.secondaryProviderModel, null)
  cleared.secondaryProviderModel = 'glm-4.6'
  assert.throws(() => validateDraft(cleared), /model must be null when secondary provider is null/)
})

test('inherit default submits null and a missing default requires an explicit model', () => {
  const inherited = selectRoleModel(draftFromResponse(v2Response), 'major', null)
  assert.doesNotThrow(() => validateDraft({ ...inherited, majorProvider: 'openai', majorModel: null }))
  const inheritBody = buildPutBody({ ...inherited, majorProvider: 'openai', majorModel: null })
  if (inheritBody.selectionVersion !== 2) throw new Error('expected v2')
  assert.equal(inheritBody.majorModel, null)

  const missing = selectRoleModel(draftFromResponse(v2Response), 'major', null)
  assert.throws(() => validateDraft(missing), /Major role must resolve to a non-empty model/)
  assert.doesNotThrow(() => validateDraft(selectRoleModel(missing, 'major', 'custom-id')))
})

test('switching provider clears the model draft and restores a buffered choice', () => {
  const draft = draftFromResponse(v2Response)
  const switched = selectRoleProvider(draft, 'major', 'openai')
  assert.equal(switched.majorProvider, 'openai')
  assert.equal(switched.majorModel, null)
  const back = selectRoleProvider(switched, 'major', 'ark')
  assert.equal(back.majorModel, 'deepseek-v3')
  assert.equal(back.dirty, true)
  assert.equal(draftFromResponse(v2Response).dirty, false)
})

test('model options include catalog prices in USD per million tokens', () => {
  assert.equal(formatUsdPerMillion(0.95), '$0.95')
  assert.equal(formatUsdPerMillion(4), '$4')
  assert.equal(formatUsdPerMillion(0.035), '$0.035')
  assert.equal(
    modelPriceLabel({ inputCost: 0.95, outputCost: 4 }),
    '$0.95 in / $4 out per 1M tokens',
  )
  assert.equal(modelPriceLabel({ inputCost: null, outputCost: null }), null)

  const choice = modelChoiceState({
    models: [
      { id: 'kimi-k2.6', name: 'Kimi K2.6', inputCost: 0.95, outputCost: 4, recommended: true },
      { id: 'local-model', name: 'Local Model' },
    ],
    defaultModel: 'kimi-k2.6',
    explicitModel: null,
    effectiveModel: 'kimi-k2.6',
  })
  const kimi = choice.options.find((option) => option.value === 'kimi-k2.6')
  assert.match(kimi?.label ?? '', /Kimi K2\.6/)
  assert.match(kimi?.label ?? '', /\$0\.95 in \/ \$4 out per 1M tokens/)
  assert.match(choice.options[0]?.label ?? '', /\$0\.95 in \/ \$4 out per 1M tokens/)
  const local = choice.options.find((option) => option.value === 'local-model')
  assert.equal(local?.label, 'Local Model')
})

test('an effective model missing from inventory is labeled custom / not listed', () => {
  const choice = modelChoiceState({
    models: [{ id: 'glm-4.6', name: 'GLM 4.6' }],
    defaultModel: '',
    explicitModel: null,
    effectiveModel: 'my-custom',
  })
  const custom = choice.options.find(option => option.value === 'my-custom')
  assert.equal(custom?.custom, true)
  assert.match(custom?.label ?? '', /custom \/ not listed/)
  assert.equal(choice.selectValue, 'my-custom')
  assert.equal(choice.freeText, false)
})

test('recommended models sort first and are not auto-selected', () => {
  const loaded = draftFromResponse({
    ...v2Response,
    majorModel: { value: 'gpt-4o', fileValue: null, fromEnv: false, source: 'providerDefault' },
    majorProvider: { value: 'openai', fileValue: 'openai', fromEnv: false },
  })
  assert.equal(loaded.majorModel, null)
  const choice = modelChoiceState({
    models: arkInventory,
    defaultModel: 'gpt-4o',
    explicitModel: null,
    effectiveModel: 'gpt-4o',
  })
  const listed = choice.options.filter(option => !option.inherit)
  assert.equal(listed[0]?.value, 'deepseek-v3')
  assert.equal(listed[0]?.recommended, true)
  assert.equal(choice.selectValue, '')
  assert.match(choice.options[0]?.label ?? '', /inherit default \(gpt-4o\)/)
})

test('long model lists are searchable and empty inventories fall back to free text', () => {
  const longList = modelChoiceState({
    models: arkInventory,
    defaultModel: '',
    explicitModel: 'deepseek-v3',
    effectiveModel: 'deepseek-v3',
  })
  assert.equal(longList.searchable, true)
  const filtered = filterModelOptions(longList.options, 'glm', longList.selectValue)
  assert.ok(filtered.some(option => option.value === 'glm-4.6'))
  assert.equal(filtered.some(option => option.value === 'm1'), false)
  assert.ok(filtered.some(option => option.value === 'deepseek-v3'))

  const shortList = modelChoiceState({
    models: arkInventory.slice(0, 2),
    defaultModel: '',
    explicitModel: null,
    effectiveModel: null,
  })
  assert.equal(shortList.searchable, false)
  assert.equal(usesFreeTextModelInput(null), true)
  assert.equal(usesFreeTextModelInput([]), true)
  assert.equal(usesFreeTextModelInput([{ id: 'gpt-4o', name: 'GPT-4o' }]), false)
})

test('env-locked role models submit the file value', () => {
  const draft = draftFromResponse({
    selectionVersion: 2,
    majorProvider: { value: 'env-provider', fileValue: 'openai', fromEnv: true },
    secondaryProvider: { value: 'env-secondary', fileValue: null, fromEnv: true },
    majorModel: { value: 'env-model', fileValue: 'file-model', fromEnv: true, source: 'environment' },
    secondaryProviderModel: { value: 'env-secondary-model', fileValue: null, fromEnv: true, source: 'environment' },
    modelFromEnv: true,
    fastModelFromEnv: false,
    secondaryModelFromEnv: true,
    availableProviders: [],
    providers: [providerDetail('openai', 'file-default', { modelEnvValue: 'env-model' })],
  })
  draft.majorModel = 'user-typed'
  draft.majorProvider = 'user-provider'
  const putBody = buildPutBody(draft)
  if (putBody.selectionVersion !== 2) throw new Error('expected v2')
  assert.equal(putBody.majorProvider, 'openai')
  assert.equal(putBody.majorModel, 'file-model')
  assert.equal(putBody.secondaryProvider, null)
  assert.equal(putBody.secondaryProviderModel, null)
})

test('a selected v2 provider must be enabled and configured even with an empty default model', () => {
  const draft = draftFromResponse(v2Response)
  draft.providers[0]!.enabled = false
  assert.throws(() => validateDraft(draft), /is disabled/)
  draft.providers[0]!.enabled = true
  draft.providers[0]!.configured = false
  assert.throws(() => validateDraft(draft), /is not configured/)
})

test('padded role models are rejected', () => {
  const draft = selectRoleModel(draftFromResponse(v2Response), 'major', ' deepseek-v3 ')
  assert.throws(() => validateDraft(draft), /cannot be blank or padded/)
})

test('refresh inventory applies only to the requested provider and ignores a stale generation', () => {
  const draft = selectRoleProvider(draftFromResponse(v2Response), 'major', 'openai')
  draft.providers[1]!.model = 'user-typed-default'
  const first = nextRefreshGeneration({}, 'ark')
  const second = nextRefreshGeneration(first.generations, 'ark')
  assert.equal(refreshGuardIsCurrent(second.generations, first.guard), false)
  const stale = applyProviderInventoryIfCurrent(draft, second.generations, first.guard, {
    ...v2Response,
    providers: [
      providerDetail('ark', 'server-reset', { models: [{ id: 'new-model', name: 'New' }] }),
      providerDetail('openai', 'server-reset'),
    ],
  })
  assert.equal(stale, draft)
  const applied = applyProviderInventoryIfCurrent(draft, second.generations, second.guard, {
    ...v2Response,
    majorProvider: { value: 'ark', fileValue: 'ark', fromEnv: false },
    providers: [
      providerDetail('ark', 'server-reset', {
        models: [{ id: 'new-model', name: 'New' }],
        inventoryStatus: 'stale',
        lastRefreshError: null,
      }),
      providerDetail('openai', 'server-reset'),
    ],
  })
  assert.equal(applied.majorProvider, 'openai')
  assert.equal(applied.majorModel, null)
  assert.equal(applied.providers[1]?.model, 'user-typed-default')
  assert.equal(applied.providers[0]?.model, '')
  assert.equal(applied.providers[0]?.models?.[0]?.id, 'new-model')
  assert.equal(applied.providers[0]?.inventoryStatus, 'stale')
  assert.equal(applied.dirty, true)
})

