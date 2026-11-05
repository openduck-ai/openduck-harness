import { translate } from './i18n/index.ts'
import type {
  AppConfigDocument,
  ConfigScalar,
  HarnessSettingsUpdate,
  JudgeSettingsUpdate,
} from '@aaif/goose-hub-core'

export interface ConfigMessageDescriptor {
  key: string
  values?: (t: typeof translate) => Record<string, string>
}

export class ConfigValidationError extends Error implements ConfigMessageDescriptor {
  readonly key: string
  readonly values?: (t: typeof translate) => Record<string, string>

  constructor(key: string, values?: (t: typeof translate) => Record<string, string>) {
    super()
    this.key = key
    this.values = values
  }

  override get message(): string {
    return translate(this.key, this.values?.(translate))
  }
}

export function getModeOptions() {
  return [
    { value: '', label: translate('config.editor.modeNotSet') },
    { value: 'auto', label: translate('config.editor.modeAuto') },
    { value: 'approve', label: translate('config.editor.modeApprove') },
    { value: 'smart_approve', label: translate('config.editor.modeSmartApprove') },
    { value: 'chat', label: translate('config.editor.modeChat') },
  ] as const
}

export function getJudgeModes() {
  return [
    { value: 'off', label: translate('config.editor.judgeOff') },
    { value: 'shadow', label: translate('config.editor.judgeShadow') },
    { value: 'active', label: translate('config.editor.judgeActive') },
  ] as const
}

export function getJudgePointLabels(): Record<string, string> {
  return {
    'turn.drift': translate('config.editor.drift'),
    'turn.completion': translate('config.editor.completion'),
    'context.forget': translate('config.editor.contextForget'),
    'tool.risk': translate('config.editor.toolRisk'),
  }
}

function getConfigLabels(): Record<string, string> {
  return {
    GOOSE_TEMPERATURE: translate('config.editor.temperature'),
    OPENDUCK_TEMPERATURE: translate('config.editor.temperature'),
    GOOSE_FAST_MODEL: translate('config.editor.fastModel'),
    OPENDUCK_FAST_MODEL: translate('config.editor.fastModel'),
    GOOSE_TELEMETRY_ENABLED: translate('config.editor.telemetry'),
    OPENDUCK_TELEMETRY_ENABLED: translate('config.editor.telemetry'),
    OPENAI_HOST: translate('config.editor.openaiHost'),
    OPENAI_BASE_PATH: translate('config.editor.openaiBasePath'),
    ANTHROPIC_HOST: translate('config.editor.anthropicHost'),
    OLLAMA_HOST: translate('config.editor.ollamaHost'),
    GOOSE_MAX_TURNS: translate('config.editor.maxTurns'),
  }
}

export interface HarnessDraft {
  maxTurns: string
  stagnationThreshold: string
  explorationBudget: string
  periodicReviewInterval: string
  circuitBreaker: 'default' | 'on' | 'off'
}

export interface JudgeDraft {
  provider: string
  endpoint: string
  timeoutMs: string
  fallbackOnError: boolean
  points: { id: string; mode: string }[]
}

export interface ConfigDrafts {
  provider: string
  model: string
  mode: string
  harness: HarnessDraft
  judge: JudgeDraft
  entries: Record<string, string>
}

export function configLabel(key: string): string {
  const known = getConfigLabels()[key]
  if (known) return known
  return key
    .split('_')
    .filter(Boolean)
    .map((word) => word.charAt(0) + word.slice(1).toLowerCase())
    .join(' ')
}

export function formatScalar(value: ConfigScalar): string {
  return String(value)
}

export function parseScalarInput(raw: string, previous: ConfigScalar): ConfigScalar {
  if (typeof previous === 'boolean') {
    return raw === 'true'
  }
  if (typeof previous === 'number') {
    const trimmed = raw.trim()
    if (!/^-?\d+(\.\d+)?$/.test(trimmed)) {
      throw new ConfigValidationError('config.editor.enterNumber')
    }
    const value = Number(trimmed)
    if (!Number.isFinite(value)) throw new ConfigValidationError('config.editor.enterNumber')
    return value
  }
  return raw
}

export function parseOptionalCount(raw: string, label: string | { key: string }): number | null {
  const trimmed = raw.trim()
  if (!trimmed) return null
  if (!/^\d+$/.test(trimmed)) {
    throw new ConfigValidationError('config.editor.wholeNumber', t => ({ label: typeof label === 'string' ? label : t(label.key) }))
  }
  const value = Number(trimmed)
  if (!Number.isInteger(value) || value < 1) {
    throw new ConfigValidationError('config.editor.minimumCount', t => ({ label: typeof label === 'string' ? label : t(label.key) }))
  }
  return value
}

export function circuitChoice(value: boolean | null): HarnessDraft['circuitBreaker'] {
  if (value === true) return 'on'
  if (value === false) return 'off'
  return 'default'
}

export function circuitValue(choice: HarnessDraft['circuitBreaker']): boolean | null {
  if (choice === 'on') return true
  if (choice === 'off') return false
  return null
}

export function isSecretKey(key: string): boolean {
  const upper = key.toUpperCase()
  return (
    upper.includes('_KEY') ||
    upper.includes('_TOKEN') ||
    upper.includes('_SECRET') ||
    upper.includes('PASSWORD') ||
    upper.includes('CREDENTIAL') ||
    upper.includes('AUTHORIZATION')
  )
}

export function validateNewConfigKey(key: string): string {
  const trimmed = key.trim()
  if (!trimmed) throw new ConfigValidationError('config.editor.enterSettingName')
  if (trimmed.length > 128 || !/^[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(trimmed)) {
    throw new ConfigValidationError('config.editor.settingNameCharacters')
  }
  if (isSecretKey(trimmed)) throw new ConfigValidationError('config.editor.protectedSecrets')
  return trimmed
}

export function draftsFromDocument(doc: AppConfigDocument): ConfigDrafts {
  const entries: Record<string, string> = {}
  for (const entry of doc.entries) {
    entries[entry.key] = formatScalar(entry.value)
  }
  return {
    provider: doc.provider.value ?? '',
    model: doc.model.value ?? '',
    mode: doc.mode.value ?? '',
    harness: {
      maxTurns: doc.harness.maxTurns.value == null ? '' : String(doc.harness.maxTurns.value),
      stagnationThreshold:
        doc.harness.stagnationThreshold.value == null
          ? ''
          : String(doc.harness.stagnationThreshold.value),
      explorationBudget:
        doc.harness.explorationBudget.value == null
          ? ''
          : String(doc.harness.explorationBudget.value),
      periodicReviewInterval:
        doc.harness.periodicReviewInterval.value == null
          ? ''
          : String(doc.harness.periodicReviewInterval.value),
      circuitBreaker: circuitChoice(doc.harness.circuitBreakerEnabled.value),
    },
    judge: {
      provider: doc.judge.provider,
      endpoint: doc.judge.endpoint,
      timeoutMs: String(doc.judge.timeoutMs),
      fallbackOnError: doc.judge.fallbackOnError,
      points: doc.judge.points.map((point) => ({ id: point.id, mode: point.mode })),
    },
    entries,
  }
}

export function harnessUpdateFromDraft(draft: HarnessDraft): HarnessSettingsUpdate {
  return {
    maxTurns: parseOptionalCount(draft.maxTurns, { key: 'config.editor.maxTurns' }),
    stagnationThreshold: parseOptionalCount(
      draft.stagnationThreshold,
      { key: 'config.editor.stagnationThreshold' },
    ),
    explorationBudget: parseOptionalCount(
      draft.explorationBudget,
      { key: 'config.editor.explorationBudget' },
    ),
    periodicReviewInterval: parseOptionalCount(
      draft.periodicReviewInterval,
      { key: 'config.editor.reviewInterval' },
    ),
    circuitBreakerEnabled: circuitValue(draft.circuitBreaker),
  }
}

export function harnessUpdateForSave(
  doc: AppConfigDocument,
  draft: HarnessDraft,
): HarnessSettingsUpdate {
  const current = doc.harness
  return {
    maxTurns: current.maxTurns.fromEnv
      ? current.maxTurns.value
      : parseOptionalCount(draft.maxTurns, { key: 'config.editor.maxTurns' }),
    stagnationThreshold: current.stagnationThreshold.fromEnv
      ? current.stagnationThreshold.value
      : parseOptionalCount(
          draft.stagnationThreshold,
          { key: 'config.editor.stagnationThreshold' },
        ),
    explorationBudget: current.explorationBudget.fromEnv
      ? current.explorationBudget.value
      : parseOptionalCount(draft.explorationBudget, { key: 'config.editor.explorationBudget' }),
    periodicReviewInterval: current.periodicReviewInterval.fromEnv
      ? current.periodicReviewInterval.value
      : parseOptionalCount(draft.periodicReviewInterval, { key: 'config.editor.reviewInterval' }),
    circuitBreakerEnabled: current.circuitBreakerEnabled.fromEnv
      ? current.circuitBreakerEnabled.value
      : circuitValue(draft.circuitBreaker),
  }
}

export function harnessDraftUnchanged(doc: AppConfigDocument, draft: HarnessDraft): boolean {
  const next = harnessUpdateForSave(doc, draft)
  const current = doc.harness
  return (
    current.maxTurns.value === next.maxTurns &&
    current.stagnationThreshold.value === next.stagnationThreshold &&
    current.explorationBudget.value === next.explorationBudget &&
    current.periodicReviewInterval.value === next.periodicReviewInterval &&
    current.circuitBreakerEnabled.value === next.circuitBreakerEnabled
  )
}

export function judgeUpdateFromDraft(draft: JudgeDraft): JudgeSettingsUpdate {
  const provider = draft.provider.trim()
  const endpoint = draft.endpoint.trim()
  if (!provider) throw new ConfigValidationError('config.editor.enterJudgeProvider')
  if (!endpoint) throw new ConfigValidationError('config.editor.enterJudgeEndpoint')
  const timeout = parseOptionalCount(draft.timeoutMs, { key: 'config.editor.judgeTimeout' })
  if (timeout == null) throw new ConfigValidationError('config.editor.enterJudgeTimeout')
  const seen = new Set<string>()
  const points = draft.points.map((point) => {
    const id = point.id.trim()
    if (!id || !/^[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(id)) {
      throw new ConfigValidationError('config.editor.pointIdCharacters')
    }
    if (seen.has(id)) throw new ConfigValidationError('config.editor.duplicatePoint', () => ({ id }))
    seen.add(id)
    if (point.mode !== 'off' && point.mode !== 'shadow' && point.mode !== 'active') {
      throw new ConfigValidationError('config.editor.invalidJudgeMode')
    }
    return { id, mode: point.mode }
  })
  return {
    provider,
    endpoint,
    timeoutMs: timeout,
    fallbackOnError: draft.fallbackOnError,
    points,
  }
}
