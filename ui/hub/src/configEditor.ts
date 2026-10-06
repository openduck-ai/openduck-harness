import type {
  AppConfigDocument,
  ConfigScalar,
  HarnessSettingsUpdate,
  JudgeSettingsUpdate,
} from '@aaif/goose-hub-core'

export const MODE_OPTIONS = [
  { value: '', label: 'Not set' },
  { value: 'auto', label: 'Auto — tools run without asking' },
  { value: 'approve', label: 'Approve — ask before every tool' },
  { value: 'smart_approve', label: 'Smart approve — ask for sensitive tools' },
  { value: 'chat', label: 'Chat — no tools' },
] as const

export const JUDGE_MODES = [
  { value: 'off', label: 'Off' },
  { value: 'shadow', label: 'Shadow' },
  { value: 'active', label: 'Active' },
] as const

export const JUDGE_POINT_LABELS: Record<string, string> = {
  'turn.drift': 'Drift',
  'turn.completion': 'Completion',
  'context.forget': 'Context forget',
  'tool.risk': 'Tool risk',
}

const CONFIG_LABELS: Record<string, string> = {
  GOOSE_TEMPERATURE: 'Temperature',
  OPENDUCK_TEMPERATURE: 'Temperature',
  GOOSE_FAST_MODEL: 'Fast model',
  OPENDUCK_FAST_MODEL: 'Fast model',
  GOOSE_TELEMETRY_ENABLED: 'Telemetry',
  OPENDUCK_TELEMETRY_ENABLED: 'Telemetry',
  OPENAI_HOST: 'OpenAI host',
  OPENAI_BASE_PATH: 'OpenAI base path',
  ANTHROPIC_HOST: 'Anthropic host',
  OLLAMA_HOST: 'Ollama host',
  GOOSE_MAX_TURNS: 'Max turns',
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
  const known = CONFIG_LABELS[key]
  if (known) return known
  return key
    .split('_')
    .filter(Boolean)
    .map(word => word.charAt(0) + word.slice(1).toLowerCase())
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
      throw new Error('Enter a number')
    }
    const value = Number(trimmed)
    if (!Number.isFinite(value)) throw new Error('Enter a number')
    return value
  }
  return raw
}

export function parseOptionalCount(raw: string, label: string): number | null {
  const trimmed = raw.trim()
  if (!trimmed) return null
  if (!/^\d+$/.test(trimmed)) {
    throw new Error(`${label} must be a whole number`)
  }
  const value = Number(trimmed)
  if (!Number.isInteger(value) || value < 1) {
    throw new Error(`${label} must be at least 1`)
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
  if (!trimmed) throw new Error('Enter a setting name')
  if (trimmed.length > 128 || !/^[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(trimmed)) {
    throw new Error('Use letters, numbers, underscores, dots, or hyphens')
  }
  if (isSecretKey(trimmed)) throw new Error('API keys and tokens are not edited here')
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
      points: doc.judge.points.map(point => ({ id: point.id, mode: point.mode })),
    },
    entries,
  }
}

export function harnessUpdateFromDraft(draft: HarnessDraft): HarnessSettingsUpdate {
  return {
    maxTurns: parseOptionalCount(draft.maxTurns, 'Max turns'),
    stagnationThreshold: parseOptionalCount(draft.stagnationThreshold, 'Stagnation threshold'),
    explorationBudget: parseOptionalCount(draft.explorationBudget, 'Exploration budget'),
    periodicReviewInterval: parseOptionalCount(draft.periodicReviewInterval, 'Review interval'),
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
      : parseOptionalCount(draft.maxTurns, 'Max turns'),
    stagnationThreshold: current.stagnationThreshold.fromEnv
      ? current.stagnationThreshold.value
      : parseOptionalCount(draft.stagnationThreshold, 'Stagnation threshold'),
    explorationBudget: current.explorationBudget.fromEnv
      ? current.explorationBudget.value
      : parseOptionalCount(draft.explorationBudget, 'Exploration budget'),
    periodicReviewInterval: current.periodicReviewInterval.fromEnv
      ? current.periodicReviewInterval.value
      : parseOptionalCount(draft.periodicReviewInterval, 'Review interval'),
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
  if (!provider) throw new Error('Enter a judge provider')
  if (!endpoint) throw new Error('Enter a judge endpoint')
  const timeout = parseOptionalCount(draft.timeoutMs, 'Judge timeout')
  if (timeout == null) throw new Error('Enter a judge timeout in milliseconds')
  const seen = new Set<string>()
  const points = draft.points.map(point => {
    const id = point.id.trim()
    if (!id || !/^[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(id)) {
      throw new Error('Decision point ids use letters, numbers, dots, or hyphens')
    }
    if (seen.has(id)) throw new Error(`Decision point ${id} is listed twice`)
    seen.add(id)
    if (point.mode !== 'off' && point.mode !== 'shadow' && point.mode !== 'active') {
      throw new Error('Judge point mode must be off, shadow, or active')
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
