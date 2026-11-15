import type { AppConfigDocument } from '@aaif/goose-hub-core'
import { ConfigValidationError } from './configEditor.ts'
import type { ProviderConfigDraft } from './providerConfig.ts'

export const ADAPTIVE_CONFIG_KEYS = {
  mode: 'ADAPTIVE_MODE',
  regularModel: 'ADAPTIVE_REGULAR_MODEL',
  economyModel: 'ADAPTIVE_ECONOMY_MODEL',
  baseProvider: 'ADAPTIVE_BASE_PROVIDER',
  timezone: 'ADAPTIVE_TIMEZONE',
  offPeak: 'ADAPTIVE_OFF_PEAK',
  probeRatio: 'ADAPTIVE_PROBE_RATIO',
  cooldownSeconds: 'ADAPTIVE_COOLDOWN_SECONDS',
} as const

export type AdaptiveRoutingMode = 'auto' | 'regular' | 'economy'

export interface AdaptiveRoutingDraft {
  mode: AdaptiveRoutingMode
  regularModel: string
  economyModel: string
  baseProvider: string
  timezone: string
  offPeak: string
  probeRatio: string
  cooldownSeconds: string
  fromEnvKeys: ReadonlySet<string>
}

export interface AdaptiveConfigUpdateItem {
  key: string
  value: string | null
}

function readEntryString(doc: AppConfigDocument, key: string, fallback = ''): string {
  const entry = doc.entries.find((item) => item.key === key)
  if (!entry || entry.value === null || entry.value === undefined) return fallback
  return String(entry.value)
}

export function normalizeAdaptiveMode(raw: string): AdaptiveRoutingMode {
  const normalized = raw.trim().toLowerCase()
  if (normalized === 'regular' || normalized === 'fixed_regular' || normalized === 'fixed-regular') {
    return 'regular'
  }
  if (normalized === 'economy' || normalized === 'fixed_economy' || normalized === 'fixed-economy') {
    return 'economy'
  }
  return 'auto'
}

export function adaptiveDraftFromDocument(doc: AppConfigDocument): AdaptiveRoutingDraft {
  const fromEnvKeys = new Set<string>()
  for (const key of Object.values(ADAPTIVE_CONFIG_KEYS)) {
    if (doc.entries.find((item) => item.key === key)?.fromEnv) {
      fromEnvKeys.add(key)
    }
  }
  return {
    mode: normalizeAdaptiveMode(readEntryString(doc, ADAPTIVE_CONFIG_KEYS.mode, 'auto')),
    regularModel: readEntryString(doc, ADAPTIVE_CONFIG_KEYS.regularModel, ''),
    economyModel: readEntryString(doc, ADAPTIVE_CONFIG_KEYS.economyModel, ''),
    baseProvider: readEntryString(doc, ADAPTIVE_CONFIG_KEYS.baseProvider, ''),
    timezone: readEntryString(doc, ADAPTIVE_CONFIG_KEYS.timezone, 'Asia/Shanghai'),
    offPeak: readEntryString(doc, ADAPTIVE_CONFIG_KEYS.offPeak, '01:00-07:00'),
    probeRatio: readEntryString(doc, ADAPTIVE_CONFIG_KEYS.probeRatio, '0.10'),
    cooldownSeconds: readEntryString(doc, ADAPTIVE_CONFIG_KEYS.cooldownSeconds, '300'),
    fromEnvKeys,
  }
}

const TIME_WINDOW_RE = /^([01]\d|2[0-3]):[0-5]\d-([01]\d|2[0-3]):[0-5]\d$/

export function validateAdaptiveDraft(draft: AdaptiveRoutingDraft): void {
  if (!draft.regularModel.trim()) {
    throw new ConfigValidationError('config.adaptive.regularModelRequired')
  }
  if (!draft.economyModel.trim()) {
    throw new ConfigValidationError('config.adaptive.economyModelRequired')
  }

  const probeRatio = Number(draft.probeRatio.trim())
  if (!draft.probeRatio.trim() || Number.isNaN(probeRatio) || probeRatio < 0 || probeRatio > 1) {
    throw new ConfigValidationError('config.adaptive.invalidProbeRatio')
  }

  const cooldown = Number(draft.cooldownSeconds.trim())
  if (
    !draft.cooldownSeconds.trim() ||
    !Number.isInteger(cooldown) ||
    cooldown < 0
  ) {
    throw new ConfigValidationError('config.adaptive.invalidCooldown')
  }

  const windows = draft.offPeak
    .split(',')
    .map((w) => w.trim())
    .filter(Boolean)
  if (windows.length === 0 || windows.some((w) => !TIME_WINDOW_RE.test(w))) {
    throw new ConfigValidationError('config.adaptive.invalidOffPeak')
  }
}

export function adaptiveEntriesToUpsert(
  draft: AdaptiveRoutingDraft,
  doc: AppConfigDocument,
): AdaptiveConfigUpdateItem[] {
  const desired: Array<[string, string | null]> = [
    [ADAPTIVE_CONFIG_KEYS.mode, draft.mode],
    [ADAPTIVE_CONFIG_KEYS.regularModel, draft.regularModel.trim() || null],
    [ADAPTIVE_CONFIG_KEYS.economyModel, draft.economyModel.trim() || null],
    [ADAPTIVE_CONFIG_KEYS.baseProvider, draft.baseProvider.trim() || null],
    [ADAPTIVE_CONFIG_KEYS.timezone, draft.timezone.trim() || 'Asia/Shanghai'],
    [ADAPTIVE_CONFIG_KEYS.offPeak, draft.offPeak.trim() || '01:00-07:00'],
    [ADAPTIVE_CONFIG_KEYS.probeRatio, draft.probeRatio.trim() || '0.10'],
    [ADAPTIVE_CONFIG_KEYS.cooldownSeconds, draft.cooldownSeconds.trim() || '300'],
  ]

  const updates: AdaptiveConfigUpdateItem[] = []
  for (const [key, nextVal] of desired) {
    if (draft.fromEnvKeys.has(key)) continue
    const existing = doc.entries.find((item) => item.key === key)
    const prevVal =
      existing && existing.value !== null && existing.value !== undefined
        ? String(existing.value)
        : null
    if (nextVal !== prevVal) {
      updates.push({ key, value: nextVal })
    }
  }
  return updates
}

export interface AdaptiveCandidateOption {
  value: string
  label: string
  provider: string
}

export function adaptiveCandidateModelOptions(
  providersDraft: ProviderConfigDraft | null,
): AdaptiveCandidateOption[] {
  if (!providersDraft) return []
  const seen = new Set<string>()
  const options: AdaptiveCandidateOption[] = []

  for (const row of providersDraft.providers) {
    if (row.name === 'adaptive') continue
    const add = (modelId: string, modelName?: string | null) => {
      const id = modelId.trim()
      if (!id) return
      const value = `${row.name}/${id}`
      if (seen.has(value)) return
      seen.add(value)
      const display = modelName?.trim() && modelName.trim() !== id ? `${row.name}/${modelName.trim()}` : value
      options.push({ value, label: display, provider: row.name })
    }

    if (row.model) {
      add(row.model)
    }
    for (const model of row.models ?? []) {
      add(model.id, model.name)
    }
  }

  return options
}

export function adaptiveBaseProviderOptions(
  providersDraft: ProviderConfigDraft | null,
): string[] {
  if (!providersDraft) return []
  const names = new Set<string>()
  for (const row of providersDraft.providers) {
    if (row.name !== 'adaptive') names.add(row.name)
  }
  for (const name of providersDraft.availableProviders) {
    if (name !== 'adaptive') names.add(name)
  }
  return Array.from(names).sort()
}
