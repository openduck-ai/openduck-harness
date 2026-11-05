import { ConfigValidationError, type ConfigMessageDescriptor } from './configEditor.ts'
import { translate } from './i18n/index.ts'
import type {
  ProviderConfigPutBody,
  ProviderEntryPutItemV2,
  ProviderInventoryStatus,
  ProviderModelOption,
  ProvidersConfigResponse,
  ProxyItem,
  RoleModelField,
  RoleModelSource,
} from '@aaif/goose-hub-core'

export type ProviderRole = 'major' | 'secondary'

export const MODEL_SEARCH_MIN_COUNT = 8

export const CUSTOM_MODEL_OPTION = '__enter_model_id__'

export function getLegacyProviderServerNotice(): string {
  return translate('config.providers.legacyServerNotice')
}

export const IDENTICAL_ROLE_TARGET_WARNING = 'Identical targets cause duplicate invocations'

export interface ProviderRowDraft {
  name: string
  enabled: boolean
  configured: boolean
  model: string
  fastModel: string
  secondaryModel: string
  proxy: string
  modelEnvValue: string | null
  fastModelEnvValue: string | null
  secondaryModelEnvValue: string | null
  resolvedFastModel: string
  fastModelUsesMain: boolean
  suggestedFastModel: string | null
  fileModel: string
  fileFastModel: string | null
  fileSecondaryModel: string | null
  fileProxy: string | null
  models: ProviderModelOption[] | null
  inventoryStatus: ProviderInventoryStatus | null
  supportsRefresh: boolean | null
  refreshing: boolean
  lastUpdatedAt: string | null
  lastRefreshError: string | ConfigMessageDescriptor | null
}

export interface ProviderConfigDraft {
  selectionVersion: 2 | null
  majorProvider: string | null
  secondaryProvider: string | null
  majorModel: string | null
  secondaryProviderModel: string | null
  majorFileValue: string | null
  majorFromEnv: boolean
  secondaryFileValue: string | null
  secondaryFromEnv: boolean
  majorModelFileValue: string | null
  majorModelEffective: string | null
  majorModelFromEnv: boolean
  majorModelSource: RoleModelSource | null
  majorModelEffectiveProvider: string | null
  secondaryModelFileValue: string | null
  secondaryModelEffective: string | null
  secondaryProviderModelFromEnv: boolean
  secondaryModelSource: RoleModelSource | null
  secondaryModelEffectiveProvider: string | null
  modelFromEnv: boolean
  fastModelFromEnv: boolean
  secondaryModelFromEnv: boolean
  availableProviders: string[]
  providers: ProviderRowDraft[]
  proxies: ProxyItem[]
  modelDraftsByProvider: Record<ProviderRole, Record<string, string | null>>
  dirty: boolean
}

export interface ProviderSelectOption {
  name: string
  eligible: boolean
}

export interface ModelChoiceOption {
  value: string
  label: string
  recommended: boolean
  custom: boolean
  inherit: boolean
}

export interface ModelChoiceState {
  freeText: boolean
  searchable: boolean
  options: ModelChoiceOption[]
  selectValue: string
}

export interface ProviderRefreshGuard {
  providerId: string
  generation: number
}

export interface ResolvedRoleTarget {
  provider: string | null
  model: string | null
}

export function normalizeFastModel(
  fastModel: string | null | undefined,
  model: string,
): string | null {
  if (!fastModel) return null
  const trimmed = fastModel.trim()
  if (!trimmed || trimmed === model.trim()) return null
  return trimmed
}

export function normalizeSecondaryModel(secondaryModel: string | null | undefined): string | null {
  if (!secondaryModel) return null
  const trimmed = secondaryModel.trim()
  if (!trimmed) return null
  return trimmed
}

export function normalizeProxy(proxy: string | null | undefined): string | null {
  if (!proxy) return null
  const trimmed = proxy.trim()
  return trimmed ? trimmed : null
}

export function validateProxyUrlMessage(url: string): ConfigMessageDescriptor | null {
  const trimmed = url.trim()
  if (!trimmed) return { key: 'config.providers.emptyProxyUrl' }
  if (
    !trimmed.startsWith('http://') &&
    !trimmed.startsWith('https://') &&
    !trimmed.startsWith('socks5://') &&
    !trimmed.startsWith('socks5h://')
  ) {
    return { key: 'config.providers.proxyUrlScheme' }
  }
  return null
}

export function validateProxyIdMessage(
  id: string,
  existingProxies: ProxyItem[],
  currentId?: string,
): ConfigMessageDescriptor | null {
  const trimmed = id.trim()
  if (!trimmed) return { key: 'config.providers.emptyProxyId' }
  if (/[^a-zA-Z0-9_-]/.test(trimmed)) {
    return { key: 'config.providers.proxyIdCharacters' }
  }
  if (existingProxies.some((p) => p.id === trimmed && p.id !== currentId)) {
    return { key: 'config.providers.duplicateProxyId', values: () => ({ id: trimmed }) }
  }
  return null
}

export function validateProxyUrl(url: string): string | null {
  const message = validateProxyUrlMessage(url)
  return message ? translate(message.key, message.values?.(translate)) : null
}

export function validateProxyId(id: string, existingProxies: ProxyItem[], currentId?: string): string | null {
  const message = validateProxyIdMessage(id, existingProxies, currentId)
  return message ? translate(message.key, message.values?.(translate)) : null
}

export function resolveProxyLabel(
  proxyValue: string | null | undefined,
  proxies: ProxyItem[],
): string {
  if (!proxyValue) return translate('config.providers.noProxy')
  const trimmed = proxyValue.trim()
  if (!trimmed) return translate('config.providers.noProxy')
  const match = proxies.find((p) => p.id === trimmed || p.url === trimmed)
  if (match) {
    return `${match.id} (${match.url})`
  }
  return trimmed
}

export function findMatchingProxyId(
  proxyValue: string | null | undefined,
  proxies: ProxyItem[],
): string | null {
  if (!proxyValue) return null
  const trimmed = proxyValue.trim()
  if (!trimmed) return null
  const match = proxies.find((p) => p.id === trimmed || p.url === trimmed)
  return match ? match.id : null
}

export function fastModelLabel(fastModel: string | null | undefined, model: string): string {
  const norm = normalizeFastModel(fastModel, model)
  return norm ? norm : translate('config.providers.usesModel')
}

export function secondaryModelLabel(secondaryModel: string | null | undefined): string {
  const norm = normalizeSecondaryModel(secondaryModel)
  return norm ? norm : translate('config.providers.noSecondaryModel')
}

export function envLockDescription(
  fileValue: string | null,
  effectiveValue: string | null,
): string {
  return translate('config.providers.envLock', {
    file: fileValue ?? translate('config.providers.unset'),
    effective: effectiveValue ?? translate('config.providers.unset'),
  })
}

export function inventoryStatusLabel(
  status: ProviderInventoryStatus | null | undefined,
): string | null {
  switch (status) {
    case 'stale':
      return translate('config.providers.staleModels')
    case 'neverFetched':
      return translate('config.providers.modelsNotFetched')
    case 'unsupported':
      return translate('config.providers.refreshUnsupported')
    default:
      return null
  }
}

export function supportsRoleModelSelection(draft: ProviderConfigDraft): boolean {
  return draft.selectionVersion === 2
}

function emptyModelBuffers(): Record<ProviderRole, Record<string, string | null>> {
  return { major: {}, secondary: {} }
}

function copyModels(
  models: ProviderModelOption[] | null | undefined,
): ProviderModelOption[] | null {
  if (!models) return null
  return models.map((model) => ({
    id: model.id,
    name: model.name,
    ...(model.recommended ? { recommended: true } : {}),
  }))
}

function rowFromDetail(provider: ProvidersConfigResponse['providers'][number]): ProviderRowDraft {
  return {
    name: provider.name,
    enabled: provider.enabled,
    configured: provider.configured,
    model: provider.model,
    fastModel: provider.fastModel ?? '',
    secondaryModel: provider.secondaryModel ?? '',
    proxy: provider.proxy ?? '',
    modelEnvValue: provider.modelEnvValue,
    fastModelEnvValue: provider.fastModelEnvValue,
    secondaryModelEnvValue: provider.secondaryModelEnvValue,
    resolvedFastModel: provider.resolvedFastModel,
    fastModelUsesMain: provider.fastModelUsesMain,
    suggestedFastModel: provider.suggestedFastModel,
    fileModel: provider.model,
    fileFastModel: provider.fastModel,
    fileSecondaryModel: provider.secondaryModel,
    fileProxy: provider.proxy ?? null,
    models: copyModels(provider.models),
    inventoryStatus: provider.inventoryStatus ?? null,
    supportsRefresh: provider.supportsRefresh ?? null,
    refreshing: provider.refreshing ?? false,
    lastUpdatedAt: provider.lastUpdatedAt ?? null,
    lastRefreshError: provider.lastRefreshError ?? null,
  }
}

function roleModelDraft(field: RoleModelField | undefined): {
  explicit: string | null
  fileValue: string | null
  effective: string | null
  fromEnv: boolean
  source: RoleModelSource | null
} {
  return {
    explicit: field?.fileValue ?? null,
    fileValue: field?.fileValue ?? null,
    effective: field?.value ?? null,
    fromEnv: field?.fromEnv ?? false,
    source: field?.source ?? null,
  }
}

export function draftFromResponse(resp: ProvidersConfigResponse): ProviderConfigDraft {
  const majorModel = roleModelDraft(resp.majorModel)
  const secondaryModel = roleModelDraft(resp.secondaryProviderModel)
  return {
    selectionVersion: resp.selectionVersion === 2 ? 2 : null,
    majorProvider: resp.majorProvider.fromEnv
      ? resp.majorProvider.value
      : resp.majorProvider.fileValue,
    secondaryProvider: resp.secondaryProvider.fromEnv
      ? resp.secondaryProvider.value
      : resp.secondaryProvider.fileValue,
    majorModel: majorModel.explicit,
    secondaryProviderModel: secondaryModel.explicit,
    majorFileValue: resp.majorProvider.fileValue,
    majorFromEnv: resp.majorProvider.fromEnv,
    secondaryFileValue: resp.secondaryProvider.fileValue,
    secondaryFromEnv: resp.secondaryProvider.fromEnv,
    majorModelFileValue: majorModel.fileValue,
    majorModelEffective: majorModel.effective,
    majorModelFromEnv: majorModel.fromEnv,
    majorModelSource: majorModel.source,
    majorModelEffectiveProvider: resp.majorProvider.value,
    secondaryModelFileValue: secondaryModel.fileValue,
    secondaryModelEffective: secondaryModel.effective,
    secondaryProviderModelFromEnv: secondaryModel.fromEnv,
    secondaryModelSource: secondaryModel.source,
    secondaryModelEffectiveProvider: resp.secondaryProvider.value,
    modelFromEnv: resp.modelFromEnv,
    fastModelFromEnv: resp.fastModelFromEnv,
    secondaryModelFromEnv: resp.secondaryModelFromEnv,
    availableProviders: [...resp.availableProviders],
    proxies: resp.proxies
      ? resp.proxies.map((p) => ({ id: p.id, url: p.url, description: p.description ?? null }))
      : [],
    providers: resp.providers.map(rowFromDetail),
    modelDraftsByProvider: emptyModelBuffers(),
    dirty: false,
  }
}

function blankRow(name: string): ProviderRowDraft {
  return {
    name,
    enabled: true,
    configured: false,
    model: '',
    fastModel: '',
    secondaryModel: '',
    proxy: '',
    modelEnvValue: null,
    fastModelEnvValue: null,
    secondaryModelEnvValue: null,
    resolvedFastModel: '',
    fastModelUsesMain: true,
    suggestedFastModel: null,
    fileModel: '',
    fileFastModel: null,
    fileSecondaryModel: null,
    fileProxy: null,
    models: null,
    inventoryStatus: null,
    supportsRefresh: null,
    refreshing: false,
    lastUpdatedAt: null,
    lastRefreshError: null,
  }
}

export function addProvider(draft: ProviderConfigDraft, name: string): ProviderConfigDraft {
  if (!name || draft.providers.some((p) => p.name === name)) {
    return draft
  }
  return {
    ...draft,
    availableProviders: draft.availableProviders.filter((p) => p !== name),
    providers: [...draft.providers, blankRow(name)],
    dirty: true,
  }
}

export function removeProvider(draft: ProviderConfigDraft, name: string): ProviderConfigDraft {
  if (name === draft.majorProvider || name === draft.secondaryProvider) return draft
  if (!draft.providers.some((p) => p.name === name)) return draft
  return {
    ...draft,
    providers: draft.providers.filter((p) => p.name !== name),
    availableProviders: [...draft.availableProviders, name].sort(),
    dirty: true,
  }
}

export function updateProviderRow(
  draft: ProviderConfigDraft,
  index: number,
  patch: Partial<
    Pick<ProviderRowDraft, 'enabled' | 'model' | 'fastModel' | 'secondaryModel' | 'proxy'>
  >,
): ProviderConfigDraft {
  const current = draft.providers[index]
  if (!current) return draft
  return {
    ...draft,
    providers: draft.providers.map((row, rowIndex) =>
      rowIndex === index ? { ...row, ...patch } : row,
    ),
    dirty: true,
  }
}

function roleProvider(draft: ProviderConfigDraft, role: ProviderRole): string | null {
  const name = role === 'major' ? draft.majorProvider : draft.secondaryProvider
  return name?.trim() ? name : null
}

function roleExplicitModel(draft: ProviderConfigDraft, role: ProviderRole): string | null {
  return role === 'major' ? draft.majorModel : draft.secondaryProviderModel
}

function writeRole(
  draft: ProviderConfigDraft,
  role: ProviderRole,
  provider: string | null,
  model: string | null,
  buffers: Record<string, string | null>,
): ProviderConfigDraft {
  const modelDraftsByProvider = { ...draft.modelDraftsByProvider, [role]: buffers }
  if (role === 'major') {
    return {
      ...draft,
      majorProvider: provider,
      majorModel: model,
      modelDraftsByProvider,
      dirty: true,
    }
  }
  return {
    ...draft,
    secondaryProvider: provider,
    secondaryProviderModel: model,
    modelDraftsByProvider,
    dirty: true,
  }
}

export function selectRoleProvider(
  draft: ProviderConfigDraft,
  role: ProviderRole,
  nextProvider: string | null,
): ProviderConfigDraft {
  if (role === 'major' ? draft.majorFromEnv : draft.secondaryFromEnv) return draft
  const currentProvider = roleProvider(draft, role)
  const buffers = { ...draft.modelDraftsByProvider[role] }
  if (currentProvider) buffers[currentProvider] = roleExplicitModel(draft, role)
  const trimmed = nextProvider?.trim() || null
  let nextModel: string | null = null
  if (trimmed && Object.prototype.hasOwnProperty.call(buffers, trimmed)) {
    nextModel = buffers[trimmed] ?? null
  }
  return writeRole(draft, role, trimmed, nextModel, buffers)
}

export function selectRoleModel(
  draft: ProviderConfigDraft,
  role: ProviderRole,
  model: string | null,
): ProviderConfigDraft {
  if (roleModelLocked(draft, role)) return draft
  const provider = roleProvider(draft, role)
  if (!provider) return writeRole(draft, role, null, null, draft.modelDraftsByProvider[role])
  const buffers = { ...draft.modelDraftsByProvider[role], [provider]: model }
  return writeRole(draft, role, provider, model, buffers)
}

export function roleProviderOptions(
  draft: ProviderConfigDraft,
  role: ProviderRole,
): ProviderSelectOption[] {
  const selected = roleProvider(draft, role)
  const options = draft.providers
    .filter((provider) => provider.enabled && provider.configured)
    .map((provider) => ({ name: provider.name, eligible: true }))
  if (selected && !options.some((option) => option.name === selected)) {
    options.push({ name: selected, eligible: false })
  }
  return options
}

export function sortModelInventory(models: readonly ProviderModelOption[]): ProviderModelOption[] {
  return models
    .map((model, index) => ({ model, index }))
    .sort((left, right) => {
      const leftRank = left.model.recommended ? 0 : 1
      const rightRank = right.model.recommended ? 0 : 1
      if (leftRank !== rightRank) return leftRank - rightRank
      return left.index - right.index
    })
    .map((item) => item.model)
}

export function usesFreeTextModelInput(models: ProviderModelOption[] | null | undefined): boolean {
  return !models || models.length === 0
}

export function roleModelSelectValue(
  explicitModel: string | null,
  effectiveModel: string | null,
  defaultModel: string,
): string {
  if (explicitModel && explicitModel.trim()) return explicitModel.trim()
  if (defaultModel.trim()) return ''
  if (effectiveModel && effectiveModel.trim()) return effectiveModel.trim()
  return ''
}

export function modelChoiceState(input: {
  models: ProviderModelOption[] | null | undefined
  defaultModel: string
  explicitModel: string | null
  effectiveModel: string | null
}): ModelChoiceState {
  const defaultTrimmed = input.defaultModel.trim()
  const inventory = input.models ?? null
  const options: ModelChoiceOption[] = []
  if (defaultTrimmed) {
    options.push({
      value: '',
      label: translate('config.providers.inheritDefault', { model: defaultTrimmed }),
      recommended: false,
      custom: false,
      inherit: true,
    })
  }
  const seen = new Set<string>()
  for (const model of sortModelInventory(inventory ?? [])) {
    if (seen.has(model.id)) continue
    seen.add(model.id)
    const recommended = model.recommended === true
    options.push({
      value: model.id,
      label: recommended
        ? translate('config.providers.recommendedModel', { name: model.name })
        : model.name,
      recommended,
      custom: false,
      inherit: false,
    })
  }
  const extras = [input.explicitModel, input.effectiveModel]
  for (const raw of extras) {
    const id = raw?.trim()
    if (!id || seen.has(id)) continue
    if (!input.explicitModel?.trim() && id === defaultTrimmed) continue
    seen.add(id)
    options.push({
      value: id,
      label: translate('config.providers.customModel', { id }),
      recommended: false,
      custom: true,
      inherit: false,
    })
  }
  return {
    freeText: usesFreeTextModelInput(inventory),
    searchable: (inventory?.length ?? 0) >= MODEL_SEARCH_MIN_COUNT,
    options,
    selectValue: roleModelSelectValue(
      input.explicitModel,
      input.effectiveModel,
      input.defaultModel,
    ),
  }
}

export function filterModelOptions(
  options: readonly ModelChoiceOption[],
  query: string,
  selectedValue?: string,
): ModelChoiceOption[] {
  const needle = query.trim().toLowerCase()
  if (!needle) return [...options]
  return options.filter(
    (option) =>
      option.inherit ||
      option.value === selectedValue ||
      option.label.toLowerCase().includes(needle) ||
      option.value.toLowerCase().includes(needle),
  )
}

function submittedProvider(draft: ProviderConfigDraft, role: ProviderRole): string | null {
  const fromEnv = role === 'major' ? draft.majorFromEnv : draft.secondaryFromEnv
  const fileValue = role === 'major' ? draft.majorFileValue : draft.secondaryFileValue
  const raw = fromEnv ? fileValue : roleProvider(draft, role)
  const trimmed = raw?.trim()
  return trimmed ? trimmed : null
}

function roleModelLocked(draft: ProviderConfigDraft, role: ProviderRole): boolean {
  if (role === 'major') return draft.majorModelFromEnv || draft.modelFromEnv
  return draft.secondaryProviderModelFromEnv || draft.secondaryModelFromEnv
}

function submittedRoleModel(draft: ProviderConfigDraft, role: ProviderRole): string | null {
  if (!submittedProvider(draft, role)) return null
  if (roleModelLocked(draft, role)) {
    const fileValue = role === 'major' ? draft.majorModelFileValue : draft.secondaryModelFileValue
    return fileValue && fileValue.length > 0 ? fileValue : null
  }
  return roleExplicitModel(draft, role)
}

function roleLabel(role: ProviderRole): string {
  return role === 'major'
    ? 'config.providers.majorRole'
    : 'config.providers.secondaryRole'
}

function assertProviderSelectable(
  draft: ProviderConfigDraft,
  role: ProviderRole,
  name: string,
): ProviderRowDraft {
  const entry = draft.providers.find((provider) => provider.name === name)
  const label = roleLabel(role)
  if (!entry) throw new ConfigValidationError('config.providers.providerNotFound', t => ({ role: t(label), name }))
  if (!entry.enabled)
    throw new ConfigValidationError('config.providers.providerDisabled', t => ({ role: t(label), name }))
  if (!entry.configured)
    throw new ConfigValidationError('config.providers.providerNotConfigured', t => ({ role: t(label), name }))
  return entry
}

function assertRoleResolves(
  draft: ProviderConfigDraft,
  role: ProviderRole,
  name: string,
  entry: ProviderRowDraft,
): void {
  const Label =
    role === 'major'
      ? 'config.providers.majorRoleTitle'
      : 'config.providers.secondaryRoleTitle'
  const explicit = submittedRoleModel(draft, role)
  if (explicit !== null && !roleModelLocked(draft, role)) {
    if (explicit.trim() === '' || explicit !== explicit.trim()) {
      throw new ConfigValidationError('config.providers.blankModel', t => ({ role: t(Label) }))
    }
    return
  }
  if (explicit !== null) return
  if (entry.model.trim()) return
  const effective = role === 'major' ? draft.majorModelEffective : draft.secondaryModelEffective
  const effectiveProvider =
    role === 'major' ? draft.majorModelEffectiveProvider : draft.secondaryModelEffectiveProvider
  if (roleModelLocked(draft, role) && effective?.trim()) return
  const loadedFile = role === 'major' ? draft.majorModelFileValue : draft.secondaryModelFileValue
  // A cleared explicit choice must not keep resolving through the previous effective model.
  if (
    roleExplicitModel(draft, role) === loadedFile &&
    effectiveProvider === name &&
    effective?.trim()
  )
    return
  throw new ConfigValidationError('config.providers.roleModelRequired', t => ({ role: t(Label) }))
}

export function validateDraft(draft: ProviderConfigDraft): void {
  for (const role of ['major', 'secondary'] as const) {
    const name = submittedProvider(draft, role)
    const label = roleLabel(role)
    const Label =
      role === 'major'
        ? 'config.providers.majorRoleTitle'
        : 'config.providers.secondaryRoleTitle'
    if (!name) {
      if (
        draft.selectionVersion === 2 &&
        !roleModelLocked(draft, role) &&
        roleExplicitModel(draft, role) !== null
      ) {
        throw new ConfigValidationError('config.providers.modelWithoutProvider', t => ({ roleTitle: t(Label), role: t(label) }))
      }
      continue
    }
    const entry = assertProviderSelectable(draft, role, name)
    if (draft.selectionVersion === 2) {
      assertRoleResolves(draft, role, name, entry)
    } else if (!entry.model.trim()) {
      throw new ConfigValidationError('config.providers.emptyProviderModel', t => ({ role: t(label), name }))
    }
  }
}

function legacyProviderItems(draft: ProviderConfigDraft): ProviderConfigPutBody {
  const providers = draft.providers.map((row) => {
    const isMajorRow =
      (draft.majorFromEnv ? draft.majorFileValue : draft.majorProvider) === row.name
    const isSecondaryRow =
      (draft.secondaryFromEnv ? draft.secondaryFileValue : draft.secondaryProvider) === row.name
    let model = row.model.trim()
    if (draft.modelFromEnv && isMajorRow) {
      model = row.fileModel.trim()
    } else if (draft.secondaryModelFromEnv && isSecondaryRow) {
      model = row.fileModel.trim()
    }
    let fastModel: string | null = normalizeFastModel(row.fastModel, model)
    if (draft.fastModelFromEnv) {
      fastModel = row.fileFastModel ?? null
    }
    return {
      name: row.name.trim(),
      enabled: row.enabled,
      model,
      fastModel,
      secondaryModel: normalizeSecondaryModel(row.secondaryModel),
      proxy: normalizeProxy(row.proxy),
    }
  })
  return {
    majorProvider: submittedProvider(draft, 'major'),
    secondaryProvider: submittedProvider(draft, 'secondary'),
    providers,
  }
}

function defaultModelForV2(draft: ProviderConfigDraft, row: ProviderRowDraft): string | undefined {
  const isMajorRow = (draft.majorFromEnv ? draft.majorFileValue : draft.majorProvider) === row.name
  const isSecondaryRow =
    (draft.secondaryFromEnv ? draft.secondaryFileValue : draft.secondaryProvider) === row.name
  let model = row.model.trim()
  if ((draft.modelFromEnv && isMajorRow) || (draft.secondaryModelFromEnv && isSecondaryRow)) {
    model = row.fileModel.trim()
  }
  if (model === row.fileModel.trim()) return undefined
  return model
}

function v2ProviderItem(draft: ProviderConfigDraft, row: ProviderRowDraft): ProviderEntryPutItemV2 {
  const keptOrCleared = defaultModelForV2(draft, row)
  const modelForFast = keptOrCleared ?? row.fileModel.trim()
  let fastModel: string | null = normalizeFastModel(row.fastModel, modelForFast)
  if (draft.fastModelFromEnv) fastModel = row.fileFastModel ?? null
  const item: ProviderEntryPutItemV2 = {
    name: row.name.trim(),
    enabled: row.enabled,
    fastModel,
    secondaryModel: normalizeSecondaryModel(row.secondaryModel),
    proxy: normalizeProxy(row.proxy),
  }
  if (keptOrCleared !== undefined) item.model = keptOrCleared
  return item
}

export function buildPutBody(draft: ProviderConfigDraft): ProviderConfigPutBody {
  validateDraft(draft)
  if (draft.selectionVersion !== 2) return legacyProviderItems(draft)
  return {
    selectionVersion: 2,
    majorProvider: submittedProvider(draft, 'major'),
    majorModel: submittedRoleModel(draft, 'major'),
    secondaryProvider: submittedProvider(draft, 'secondary'),
    secondaryProviderModel: submittedRoleModel(draft, 'secondary'),
    providers: draft.providers.map((row) => v2ProviderItem(draft, row)),
  }
}

export function resolvedRoleTarget(
  draft: ProviderConfigDraft,
  role: ProviderRole,
): ResolvedRoleTarget {
  const provider = roleProvider(draft, role)
  if (!provider) return { provider: null, model: null }
  const row = draft.providers.find((item) => item.name === provider)
  if (draft.selectionVersion !== 2) {
    if (!row) return { provider, model: null }
    const envLocked = role === 'major' ? draft.modelFromEnv : draft.secondaryModelFromEnv
    const shown = envLocked
      ? ((role === 'major' ? row.modelEnvValue : row.secondaryModelEnvValue) ?? row.model)
      : row.model
    const trimmed = shown.trim()
    return { provider, model: trimmed || null }
  }
  if (roleModelLocked(draft, role)) {
    const effective = role === 'major' ? draft.majorModelEffective : draft.secondaryModelEffective
    return { provider, model: effective?.trim() || null }
  }
  const explicit = roleExplicitModel(draft, role)
  if (explicit && explicit.trim() && explicit === explicit.trim()) {
    return { provider, model: explicit.trim() }
  }
  const fallback = row?.model.trim() || null
  if (fallback) return { provider, model: fallback }
  const effective = role === 'major' ? draft.majorModelEffective : draft.secondaryModelEffective
  const effectiveProvider =
    role === 'major' ? draft.majorModelEffectiveProvider : draft.secondaryModelEffectiveProvider
  if (effectiveProvider === provider && effective?.trim()) {
    return { provider, model: effective.trim() }
  }
  return { provider, model: null }
}

export function identicalRoleTargetWarning(draft: ProviderConfigDraft): string | null {
  const major = resolvedRoleTarget(draft, 'major')
  const secondary = resolvedRoleTarget(draft, 'secondary')
  if (!major.provider || !secondary.provider || !major.model || !secondary.model) return null
  if (major.provider === secondary.provider && major.model === secondary.model) {
    return translate('config.providers.identicalRoleTargetWarning')
  }
  return null
}

function mergeInventory(
  row: ProviderRowDraft,
  fresh: ProvidersConfigResponse['providers'][number],
): ProviderRowDraft {
  return {
    ...row,
    models: copyModels(fresh.models),
    inventoryStatus: fresh.inventoryStatus ?? null,
    supportsRefresh: fresh.supportsRefresh ?? null,
    refreshing: fresh.refreshing ?? false,
    lastUpdatedAt: fresh.lastUpdatedAt ?? null,
    lastRefreshError: fresh.lastRefreshError ?? null,
  }
}

// Refresh results update only the named provider's inventory. Role selections and
// every other row stay as the user left them, including after a provider switch.
export function applyProviderInventory(
  draft: ProviderConfigDraft,
  providerId: string,
  response: ProvidersConfigResponse,
): ProviderConfigDraft {
  const fresh = response.providers.find((provider) => provider.name === providerId)
  if (!fresh) return draft
  return {
    ...draft,
    providers: draft.providers.map((row) =>
      row.name === providerId ? mergeInventory(row, fresh) : row,
    ),
  }
}

export function nextRefreshGeneration(
  generations: Readonly<Record<string, number>>,
  providerId: string,
): { generations: Record<string, number>; guard: ProviderRefreshGuard } {
  const generation = (generations[providerId] ?? 0) + 1
  return {
    generations: { ...generations, [providerId]: generation },
    guard: { providerId, generation },
  }
}

export function refreshGuardIsCurrent(
  generations: Readonly<Record<string, number>>,
  guard: ProviderRefreshGuard,
): boolean {
  return generations[guard.providerId] === guard.generation
}

export function applyProviderInventoryIfCurrent(
  draft: ProviderConfigDraft,
  generations: Readonly<Record<string, number>>,
  guard: ProviderRefreshGuard,
  response: ProvidersConfigResponse,
): ProviderConfigDraft {
  if (!refreshGuardIsCurrent(generations, guard)) return draft
  return applyProviderInventory(draft, guard.providerId, response)
}

export function markProviderRefreshing(
  draft: ProviderConfigDraft,
  providerId: string,
  refreshing: boolean,
): ProviderConfigDraft {
  return {
    ...draft,
    providers: draft.providers.map((row) =>
      row.name === providerId
        ? { ...row, refreshing, lastRefreshError: refreshing ? null : row.lastRefreshError }
        : row,
    ),
  }
}

export function markProviderRefreshFailed(
  draft: ProviderConfigDraft,
  providerId: string,
  message: string | ConfigMessageDescriptor,
): ProviderConfigDraft {
  return {
    ...draft,
    providers: draft.providers.map((row) =>
      row.name === providerId ? { ...row, refreshing: false, lastRefreshError: message } : row,
    ),
  }
}
