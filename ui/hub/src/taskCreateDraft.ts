import type { ScheduleKind, SubtaskSpec } from '@aaif/goose-hub-core'

/** Prefix for in-progress "create project task" drafts, scoped per project. */
export const TASK_DRAFT_STORAGE_PREFIX = 'openduck.taskCreateDraft.'

export const TASK_DRAFT_VERSION = 1

/** Drafts older than this are dropped instead of restored. */
export const TASK_DRAFT_TTL_MS = 30 * 24 * 60 * 60 * 1000

/**
 * A timestamp far ahead of the local clock means the payload was tampered with or the clock
 * moved backwards; either way the TTL below could never expire it, so it is dropped.
 */
export const TASK_DRAFT_MAX_FUTURE_SKEW_MS = 5 * 60 * 1000

export const TASK_CREATE_SECTIONS = [
  'basics',
  'prompt',
  'schedule',
  'verifier',
  'subtasks',
] as const
export type TaskCreateSection = (typeof TASK_CREATE_SECTIONS)[number]

export type TaskVerifierType = 'none' | 'command' | 'diff'
export const TASK_VERIFIER_TYPES: readonly TaskVerifierType[] = ['none', 'command', 'diff']

export const SCHEDULE_KINDS: readonly ScheduleKind[] = [
  'none',
  'hourly',
  'daily',
  'weekdays',
  'weekly',
]

export interface TaskCreateDraftForm {
  id: string
  name: string
  category: string
  tagsInput: string
  prompt: string
  maxTurns: number
  timeoutSeconds: number
  dynamicPrompt: boolean
  subtasks: SubtaskSpec[]
  provider: string
  model: string
  secondaryProvider: string
  secondaryModel: string
}

export interface TaskCreateDraftSchedule {
  kind: ScheduleKind
  hour: number
  minute: number
  weekday: number
}

export interface TaskCreateDraftVerifier {
  type: TaskVerifierType
  command: string
  expectedStdout: string
  expectedExitCode: number
  filePath: string
  expectedContent: string
}

/** Remembers which section was being edited and which ones were left expanded. */
export interface TaskCreateDraftUi {
  section: TaskCreateSection
  expanded: Record<TaskCreateSection, boolean>
}

export interface TaskCreateDraftData {
  ui: TaskCreateDraftUi
  form: TaskCreateDraftForm
  schedule: TaskCreateDraftSchedule
  verifier: TaskCreateDraftVerifier
}

/** Persisted envelope; the stored `projectId` guards against restoring into the wrong project. */
export interface TaskCreateDraft extends TaskCreateDraftData {
  version: number
  projectId: string
  updatedAt: string
}

/**
 * Attachments (`File[]`), mention popovers and chat transcripts are intentionally not persisted:
 * file handles cannot survive a reload and chat history lives on the server.
 */
export type TaskDraftStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>

export function defaultTaskCreateDraftUi(): TaskCreateDraftUi {
  return {
    section: 'basics',
    expanded: { basics: true, prompt: true, schedule: true, verifier: true, subtasks: true },
  }
}

export function defaultTaskCreateDraftForm(): TaskCreateDraftForm {
  return {
    id: '',
    name: '',
    category: '',
    tagsInput: '',
    prompt: '',
    maxTurns: 25,
    timeoutSeconds: 300,
    dynamicPrompt: false,
    subtasks: [],
    provider: '',
    model: '',
    secondaryProvider: '',
    secondaryModel: '',
  }
}

export function defaultTaskCreateDraftSchedule(): TaskCreateDraftSchedule {
  return { kind: 'none', hour: 9, minute: 0, weekday: 0 }
}

export function defaultTaskCreateDraftVerifier(): TaskCreateDraftVerifier {
  return {
    type: 'none',
    command: '',
    expectedStdout: '',
    expectedExitCode: 0,
    filePath: '',
    expectedContent: '',
  }
}

export function defaultTaskCreateDraftData(): TaskCreateDraftData {
  return {
    ui: defaultTaskCreateDraftUi(),
    form: defaultTaskCreateDraftForm(),
    schedule: defaultTaskCreateDraftSchedule(),
    verifier: defaultTaskCreateDraftVerifier(),
  }
}

export function taskDraftStorageKey(projectId: string): string {
  return `${TASK_DRAFT_STORAGE_PREFIX}${projectId}`
}

export interface TaskDraftStoreOptions {
  /** Injecting storage keeps the store testable without a DOM. */
  storage?: TaskDraftStorage | null
  /** Injected clock for TTL checks. */
  now?: Date
}

function resolveStorage(storage: TaskDraftStorage | null | undefined): TaskDraftStorage | null {
  if (storage !== undefined) return storage
  try {
    return (globalThis as { localStorage?: TaskDraftStorage }).localStorage ?? null
  } catch {
    return null
  }
}

function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {}
}

function text(value: unknown, fallback: string): string {
  return typeof value === 'string' ? value : fallback
}

function int(value: unknown, fallback: number, min: number, max: number): number {
  const parsed = typeof value === 'number' ? value : Number(value)
  if (!Number.isFinite(parsed)) return fallback
  return Math.min(max, Math.max(min, Math.round(parsed)))
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback
}

function oneOf<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return typeof value === 'string' && (allowed as readonly string[]).includes(value)
    ? (value as T)
    : fallback
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined
}

function optionalText(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined
}

function expectedOutputs(value: unknown): SubtaskSpec['expectedOutputs'] {
  if (!Array.isArray(value)) return undefined
  const outputs = value
    .filter(entry => typeof entry === 'object' && entry !== null)
    .map(entry => {
      const record = asRecord(entry)
      return {
        description: text(record.description, ''),
        kind: text(record.kind, 'note'),
        command: optionalText(record.command),
        expectedExitCode: optionalNumber(record.expectedExitCode),
        expectedStdout: optionalText(record.expectedStdout),
        filePath: optionalText(record.filePath),
        expectedContent: optionalText(record.expectedContent),
      }
    })
  return outputs.length > 0 ? outputs : undefined
}

function subtasks(value: unknown): SubtaskSpec[] {
  if (!Array.isArray(value)) return []
  return value
    .filter(entry => typeof entry === 'object' && entry !== null)
    .map((entry, index) => {
      const record = asRecord(entry)
      const targetFiles = Array.isArray(record.targetFiles)
        ? record.targetFiles.filter((file): file is string => typeof file === 'string')
        : []
      return {
        id: text(record.id, `subtask-${index + 1}`),
        title: text(record.title, ''),
        description: text(record.description, ''),
        targetFiles: targetFiles.length > 0 ? targetFiles : undefined,
        maxTurns: optionalNumber(record.maxTurns),
        explorationBudget: optionalNumber(record.explorationBudget),
        expectedOutputs: expectedOutputs(record.expectedOutputs),
      }
    })
}

function sanitizeUi(value: unknown): TaskCreateDraftUi {
  const record = asRecord(value)
  const fallback = defaultTaskCreateDraftUi()
  const storedExpanded = asRecord(record.expanded)
  const expanded = { ...fallback.expanded }
  for (const section of TASK_CREATE_SECTIONS) {
    expanded[section] = bool(storedExpanded[section], fallback.expanded[section])
  }
  return {
    section: oneOf(record.section, TASK_CREATE_SECTIONS, fallback.section),
    expanded,
  }
}

function sanitizeForm(value: unknown): TaskCreateDraftForm {
  const record = asRecord(value)
  const fallback = defaultTaskCreateDraftForm()
  return {
    id: text(record.id, fallback.id),
    name: text(record.name, fallback.name),
    category: text(record.category, fallback.category),
    tagsInput: text(record.tagsInput, fallback.tagsInput),
    prompt: text(record.prompt, fallback.prompt),
    maxTurns: int(record.maxTurns, fallback.maxTurns, 1, 1000),
    timeoutSeconds: int(record.timeoutSeconds, fallback.timeoutSeconds, 1, 86400),
    dynamicPrompt: bool(record.dynamicPrompt, fallback.dynamicPrompt),
    subtasks: subtasks(record.subtasks),
    provider: text(record.provider, fallback.provider),
    model: text(record.model, fallback.model),
    secondaryProvider: text(record.secondaryProvider, fallback.secondaryProvider),
    secondaryModel: text(record.secondaryModel, fallback.secondaryModel),
  }
}

function sanitizeSchedule(value: unknown): TaskCreateDraftSchedule {
  const record = asRecord(value)
  const fallback = defaultTaskCreateDraftSchedule()
  return {
    kind: oneOf(record.kind, SCHEDULE_KINDS, fallback.kind),
    hour: int(record.hour, fallback.hour, 0, 23),
    minute: int(record.minute, fallback.minute, 0, 59),
    weekday: int(record.weekday, fallback.weekday, 0, 6),
  }
}

function sanitizeVerifier(value: unknown): TaskCreateDraftVerifier {
  const record = asRecord(value)
  const fallback = defaultTaskCreateDraftVerifier()
  return {
    type: oneOf(record.type, TASK_VERIFIER_TYPES, fallback.type),
    command: text(record.command, fallback.command),
    expectedStdout: text(record.expectedStdout, fallback.expectedStdout),
    expectedExitCode: int(record.expectedExitCode, fallback.expectedExitCode, -1, 255),
    filePath: text(record.filePath, fallback.filePath),
    expectedContent: text(record.expectedContent, fallback.expectedContent),
  }
}

/** Age of the stored draft in milliseconds, or null when the timestamp is unusable. */
function draftAgeMs(updatedAt: unknown, now: Date | undefined): number | null {
  if (typeof updatedAt !== 'string') return null
  const savedAt = Date.parse(updatedAt)
  if (!Number.isFinite(savedAt)) return null
  return (now ?? new Date()).getTime() - savedAt
}

/** Keeps anything that is not a well-formed draft payload from reaching the form. */
function sanitizeData(value: unknown): TaskCreateDraftData {
  const record = asRecord(value)
  return {
    ui: sanitizeUi(record.ui),
    form: sanitizeForm(record.form),
    schedule: sanitizeSchedule(record.schedule),
    verifier: sanitizeVerifier(record.verifier),
  }
}

function sameDraftContent(a: TaskCreateDraftData, b: TaskCreateDraftData): boolean {
  return JSON.stringify(sanitizeData(a)) === JSON.stringify(sanitizeData(b))
}

/** True when the draft carries user input worth restoring. */
export function isMeaningfulTaskDraft(data: TaskCreateDraftData): boolean {
  const { form, schedule, verifier } = data
  return (
    form.name.trim() !== '' ||
    form.category.trim() !== '' ||
    form.tagsInput.trim() !== '' ||
    form.prompt.trim() !== '' ||
    form.dynamicPrompt ||
    form.provider.trim() !== '' ||
    form.model.trim() !== '' ||
    form.secondaryProvider.trim() !== '' ||
    form.secondaryModel.trim() !== '' ||
    form.subtasks.some(
      subtask => subtask.title.trim() !== '' || subtask.description.trim() !== '',
    ) ||
    schedule.kind !== 'none' ||
    verifier.type !== 'none' ||
    verifier.command.trim() !== '' ||
    verifier.expectedStdout.trim() !== '' ||
    verifier.filePath.trim() !== '' ||
    verifier.expectedContent.trim() !== ''
  )
}

/**
 * Debounced saves call this: a form the user never touched would otherwise leave an empty
 * draft behind and show a bogus "restored your draft" banner on the next visit.
 */
export function persistTaskDraft(
  projectId: string,
  data: TaskCreateDraftData,
  options: TaskDraftStoreOptions = {},
): TaskCreateDraft | null {
  if (!isMeaningfulTaskDraft(data)) {
    clearTaskDraft(projectId, options)
    return null
  }
  return saveTaskDraft(projectId, data, options)
}

export function saveTaskDraft(
  projectId: string,
  data: TaskCreateDraftData,
  options: TaskDraftStoreOptions = {},
): TaskCreateDraft | null {
  if (!projectId.trim()) return null
  const storage = resolveStorage(options.storage)
  if (!storage) return null

  const content = sanitizeData(data)
  /* Re-entering the creator rewrites an unchanged draft; keeping the original timestamp stops the
     "restored from a moment ago" banner from claiming fresher content than the user actually typed. */
  const existing = loadTaskDraft(projectId, options)
  const keepsTimestamp = existing !== null && sameDraftContent(content, existing)
  const draft: TaskCreateDraft = {
    ...content,
    version: TASK_DRAFT_VERSION,
    projectId,
    updatedAt:
      keepsTimestamp && existing ? existing.updatedAt : (options.now ?? new Date()).toISOString(),
  }

  try {
    storage.setItem(taskDraftStorageKey(projectId), JSON.stringify(draft))
  } catch {
    // Storage can be disabled or full (private browsing); drafts are best effort.
    return null
  }
  return draft
}

export function loadTaskDraft(
  projectId: string,
  options: TaskDraftStoreOptions = {},
): TaskCreateDraft | null {
  if (!projectId.trim()) return null
  const storage = resolveStorage(options.storage)
  if (!storage) return null

  let raw: string | null
  try {
    raw = storage.getItem(taskDraftStorageKey(projectId))
  } catch {
    return null
  }
  if (!raw) return null

  let parsed: Record<string, unknown>
  try {
    parsed = asRecord(JSON.parse(raw))
  } catch {
    clearTaskDraft(projectId, options)
    return null
  }

  const updatedAt = parsed.updatedAt
  const age = draftAgeMs(updatedAt, options.now)
  if (
    parsed.version !== TASK_DRAFT_VERSION ||
    parsed.projectId !== projectId ||
    age === null ||
    age > TASK_DRAFT_TTL_MS ||
    age < -TASK_DRAFT_MAX_FUTURE_SKEW_MS
  ) {
    clearTaskDraft(projectId, options)
    return null
  }

  return {
    ...sanitizeData(parsed),
    version: TASK_DRAFT_VERSION,
    projectId,
    updatedAt: updatedAt as string,
  }
}

export function clearTaskDraft(projectId: string, options: TaskDraftStoreOptions = {}): void {
  if (!projectId.trim()) return
  const storage = resolveStorage(options.storage)
  if (!storage) return
  try {
    storage.removeItem(taskDraftStorageKey(projectId))
  } catch {
    // Nothing to do: the draft is already unreachable.
  }
}
