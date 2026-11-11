import { projectModelChoices, projectProviderChoices } from './projectProviderOptions.ts'

export interface ChatModelChoice {
  value: string
  label: string
}

export interface ChatProviderModelView {
  provider: string
  model: string
  providers: ChatModelChoice[]
  models: ChatModelChoice[]
}

export interface ChatProviderCatalogRow {
  name: string
  enabled: boolean
  configured: boolean
  model?: string | null
  models?: readonly { id: string; name?: string | null }[] | null
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object'
}

function readChoices(options: unknown): ChatModelChoice[] {
  if (!Array.isArray(options)) return []
  const choices: ChatModelChoice[] = []
  for (const option of options) {
    if (!isRecord(option)) continue
    if (typeof option.value === 'string' && option.value) {
      const name = typeof option.name === 'string' && option.name.trim() ? option.name.trim() : option.value
      choices.push({ value: option.value, label: name })
      continue
    }
    if (Array.isArray(option.options)) choices.push(...readChoices(option.options))
  }
  return choices
}

function readSessionSelect(configOptions: unknown, id: string): { currentValue: string; choices: ChatModelChoice[] } | null {
  if (!Array.isArray(configOptions)) return null
  for (const option of configOptions) {
    if (!isRecord(option) || option.id !== id || option.type !== 'select') continue
    const currentValue = typeof option.currentValue === 'string' ? option.currentValue : ''
    return { currentValue, choices: readChoices(option.options) }
  }
  return null
}

function withCurrent(choices: readonly ChatModelChoice[], current: string): ChatModelChoice[] {
  const trimmed = current.trim()
  if (!trimmed || choices.some(choice => choice.value === trimmed)) return [...choices]
  return [{ value: trimmed, label: trimmed }, ...choices]
}

function mergeChoices(primary: readonly ChatModelChoice[], extra: readonly ChatModelChoice[]): ChatModelChoice[] {
  const seen = new Set<string>()
  const result: ChatModelChoice[] = []
  for (const choice of [...primary, ...extra]) {
    const value = choice.value.trim()
    if (!value || seen.has(value)) continue
    seen.add(value)
    result.push({ value, label: choice.label.trim() || value })
  }
  return result
}

function catalogModelLabels(
  provider: string,
  globalProviders: readonly ChatProviderCatalogRow[] | null | undefined,
): Map<string, string> {
  const labels = new Map<string, string>()
  const row = globalProviders?.find(item => item.name === provider)
  for (const model of row?.models ?? []) {
    const id = model.id?.trim()
    if (!id || labels.has(id)) continue
    labels.set(id, model.name?.trim() || id)
  }
  return labels
}

export function chatProviderModelView(input: {
  configOptions?: unknown
  availableProviders?: readonly string[] | null
  providerModels?: Readonly<Record<string, readonly string[] | undefined>> | null
  globalProviders?: readonly ChatProviderCatalogRow[] | null
  fallbackProvider?: string | null
  fallbackModel?: string | null
  /** Provider the user just picked, before the session menu catches up. */
  providerOverride?: string | null
}): ChatProviderModelView {
  const sessionProvider = readSessionSelect(input.configOptions, 'provider')
  const sessionModel = readSessionSelect(input.configOptions, 'model')
  const fallbackProvider = input.fallbackProvider?.trim() || ''
  const fallbackModel = input.fallbackModel?.trim() || ''
  const override = input.providerOverride?.trim() || ''
  const provider = (override || sessionProvider?.currentValue || fallbackProvider).trim()
  const providerChanged = Boolean(override && override !== (sessionProvider?.currentValue || fallbackProvider))

  const catalogProviders = projectProviderChoices({
    availableProviders: input.availableProviders,
    globalProviderNames: input.globalProviders
      ? input.globalProviders.filter(row => row.enabled && row.configured).map(row => row.name)
      : undefined,
    current: [provider, fallbackProvider],
  }).map(name => ({ value: name, label: name }))

  const providers = sessionProvider
    ? withCurrent(sessionProvider.choices, provider)
    : withCurrent(catalogProviders, provider)

  const configuredModel =
    input.globalProviders?.find(row => row.name === provider)?.model?.trim() ||
    input.providerModels?.[provider]?.find(model => model.trim())?.trim() ||
    ''
  const model = (
    sessionModel && !providerChanged
      ? sessionModel.currentValue
      : provider === fallbackProvider
        ? fallbackModel || configuredModel
        : configuredModel
  ).trim()

  const labels = catalogModelLabels(provider, input.globalProviders)
  const catalogModels = projectModelChoices({
    provider,
    providerModels: input.providerModels,
    globalModels: input.globalProviders?.find(row => row.name === provider)?.models?.map(model => model.id),
    current: model,
  }).map(id => ({ value: id, label: labels.get(id) || id }))

  const models = sessionModel && !providerChanged
    ? mergeChoices(withCurrent(sessionModel.choices, model), catalogModels)
    : withCurrent(catalogModels, model)

  return { provider, model, providers, models }
}
