import type {
  ProviderConfigPutBody,
  ProvidersConfigResponse,
} from '@aaif/goose-hub-core'

export interface ProviderRowDraft {
  name: string
  enabled: boolean
  configured: boolean
  model: string
  fastModel: string
  secondaryModel: string
  modelEnvValue: string | null
  fastModelEnvValue: string | null
  secondaryModelEnvValue: string | null
  resolvedFastModel: string
  fastModelUsesMain: boolean
  suggestedFastModel: string | null
  fileModel: string
  fileFastModel: string | null
  fileSecondaryModel: string | null
}

export interface ProviderConfigDraft {
  majorProvider: string | null
  secondaryProvider: string | null
  majorFileValue: string | null
  majorFromEnv: boolean
  secondaryFileValue: string | null
  secondaryFromEnv: boolean
  modelFromEnv: boolean
  fastModelFromEnv: boolean
  secondaryModelFromEnv: boolean
  availableProviders: string[]
  providers: ProviderRowDraft[]
}

export function normalizeFastModel(fastModel: string | null | undefined, model: string): string | null {
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

export function fastModelLabel(fastModel: string | null | undefined, model: string): string {
  const norm = normalizeFastModel(fastModel, model)
  return norm ? norm : 'uses model'
}

export function secondaryModelLabel(secondaryModel: string | null | undefined): string {
  const norm = normalizeSecondaryModel(secondaryModel)
  return norm ? norm : 'no secondary model'
}

export function draftFromResponse(resp: ProvidersConfigResponse): ProviderConfigDraft {
  return {
    majorProvider: resp.majorProvider.fromEnv ? resp.majorProvider.value : resp.majorProvider.fileValue,
    secondaryProvider: resp.secondaryProvider.fromEnv ? resp.secondaryProvider.value : resp.secondaryProvider.fileValue,
    majorFileValue: resp.majorProvider.fileValue,
    majorFromEnv: resp.majorProvider.fromEnv,
    secondaryFileValue: resp.secondaryProvider.fileValue,
    secondaryFromEnv: resp.secondaryProvider.fromEnv,
    modelFromEnv: resp.modelFromEnv,
    fastModelFromEnv: resp.fastModelFromEnv,
    secondaryModelFromEnv: resp.secondaryModelFromEnv,
    availableProviders: [...resp.availableProviders],
    providers: resp.providers.map(p => ({
      name: p.name,
      enabled: p.enabled,
      configured: p.configured,
      model: p.model,
      fastModel: p.fastModel ?? '',
      secondaryModel: p.secondaryModel ?? '',
      modelEnvValue: p.modelEnvValue,
      fastModelEnvValue: p.fastModelEnvValue,
      secondaryModelEnvValue: p.secondaryModelEnvValue,
      resolvedFastModel: p.resolvedFastModel,
      fastModelUsesMain: p.fastModelUsesMain,
      suggestedFastModel: p.suggestedFastModel,
      fileModel: p.model,
      fileFastModel: p.fastModel,
      fileSecondaryModel: p.secondaryModel,
    })),
  }
}

export function addProvider(draft: ProviderConfigDraft, name: string): ProviderConfigDraft {
  if (!name || draft.providers.some(p => p.name === name)) {
    return draft
  }
  const newRow: ProviderRowDraft = {
    name,
    enabled: true,
    configured: false,
    model: '',
    fastModel: '',
    secondaryModel: '',
    modelEnvValue: null,
    fastModelEnvValue: null,
    secondaryModelEnvValue: null,
    resolvedFastModel: '',
    fastModelUsesMain: true,
    suggestedFastModel: null,
    fileModel: '',
    fileFastModel: null,
    fileSecondaryModel: null,
  }
  return {
    ...draft,
    availableProviders: draft.availableProviders.filter(p => p !== name),
    providers: [...draft.providers, newRow],
  }
}

export function validateDraft(draft: ProviderConfigDraft): void {
  const major = draft.majorFromEnv
    ? (draft.majorFileValue ? draft.majorFileValue.trim() : null)
    : (draft.majorProvider ? draft.majorProvider.trim() : null)

  const secondary = draft.secondaryFromEnv
    ? (draft.secondaryFileValue ? draft.secondaryFileValue.trim() : null)
    : (draft.secondaryProvider ? draft.secondaryProvider.trim() : null)

  if (major && secondary && major === secondary) {
    throw new Error('Secondary provider cannot equal major provider')
  }

  if (major) {
    const entry = draft.providers.find(p => p.name === major)
    if (!entry) {
      throw new Error(`Selected major provider ${major} not found`)
    }
    if (!entry.enabled) {
      throw new Error(`Selected major provider ${major} is disabled`)
    }
    if (!entry.model.trim()) {
      throw new Error(`Selected major provider ${major} has an empty model`)
    }
  }

  if (secondary) {
    const entry = draft.providers.find(p => p.name === secondary)
    if (!entry) {
      throw new Error(`Selected secondary provider ${secondary} not found`)
    }
    if (!entry.enabled) {
      throw new Error(`Selected secondary provider ${secondary} is disabled`)
    }
    if (!entry.model.trim()) {
      throw new Error(`Selected secondary provider ${secondary} has an empty model`)
    }
  }
}

export function buildPutBody(draft: ProviderConfigDraft): ProviderConfigPutBody {
  validateDraft(draft)

  const majorProvider = draft.majorFromEnv
    ? (draft.majorFileValue ?? null)
    : (draft.majorProvider ? draft.majorProvider.trim() : null)

  const secondaryProvider = draft.secondaryFromEnv
    ? (draft.secondaryFileValue ?? null)
    : (draft.secondaryProvider ? draft.secondaryProvider.trim() : null)

  const providers = draft.providers.map(p => {
    const isMajorRow = (draft.majorFromEnv ? draft.majorFileValue : draft.majorProvider) === p.name
    const isSecondaryRow = (draft.secondaryFromEnv ? draft.secondaryFileValue : draft.secondaryProvider) === p.name

    let model = p.model.trim()
    if (draft.modelFromEnv && isMajorRow) {
      model = p.fileModel.trim()
    } else if (draft.secondaryModelFromEnv && isSecondaryRow) {
      model = p.fileModel.trim()
    }

    let fastModel: string | null = normalizeFastModel(p.fastModel, model)
    if (draft.fastModelFromEnv) {
      fastModel = p.fileFastModel ?? null
    }

    let secondaryModel: string | null = normalizeSecondaryModel(p.secondaryModel)

    return {
      name: p.name.trim(),
      enabled: p.enabled,
      model,
      fastModel,
      secondaryModel,
    }
  })

  return {
    majorProvider,
    secondaryProvider,
    providers,
  }
}
