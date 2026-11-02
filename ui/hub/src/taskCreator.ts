import type { FriendlySchedule, ProjectTaskDefinition } from '@aaif/goose-hub-core'
import {
  defaultTaskCreateDraftData,
  type TaskCreateDraftData,
  type TaskCreateSection,
  type TaskCreateDraftVerifier,
  type TaskVerifierType,
} from './taskCreateDraft.ts'

export interface TaskCreatorState {
  taskForm: ProjectTaskDefinition
  taskTagsInput: string
  schedule: FriendlySchedule
  verifierType: TaskVerifierType
  verifierCommand: string
  verifierExpectedStdout: string
  verifierExpectedExitCode: number
  verifierFilePath: string
  verifierExpectedContent: string
  section: TaskCreateSection
  expanded: Record<TaskCreateSection, boolean>
}

export function taskCreatorDataToDraft(state: TaskCreatorState): TaskCreateDraftData {
  const verifier: TaskCreateDraftVerifier = {
    type: state.verifierType,
    command: state.verifierCommand,
    expectedStdout: state.verifierExpectedStdout,
    expectedExitCode: state.verifierExpectedExitCode,
    filePath: state.verifierFilePath,
    expectedContent: state.verifierExpectedContent,
  }
  return {
    ui: { section: state.section, expanded: state.expanded },
    form: {
      id: state.taskForm.id,
      name: state.taskForm.name ?? '',
      category: state.taskForm.category ?? '',
      tagsInput: state.taskTagsInput,
      prompt: state.taskForm.prompt,
      maxTurns: state.taskForm.maxTurns ?? 25,
      timeoutSeconds: state.taskForm.timeoutSeconds ?? 300,
      dynamicPrompt: Boolean(state.taskForm.dynamicPrompt),
      subtasks: state.taskForm.subtasks ?? [],
    },
    schedule: {
      kind: state.schedule.kind,
      hour: state.schedule.hour,
      minute: state.schedule.minute,
      weekday: state.schedule.weekday,
    },
    verifier,
  }
}

export interface TaskCreatorDraftSeed {
  base: ProjectTaskDefinition
  schedule: FriendlySchedule
}

/**
 * Draft form rows and draft UI state for the inline creator. The draft owns the fields the user types,
 * so restoring it must override the freshly generated task id as well - otherwise a reload would leave
 * the id behind while the rest of the form returned, and saving would recreate the same task twice.
 */
export function taskCreatorStateFromDraft(
  draft: TaskCreateDraftData,
  seed: TaskCreatorDraftSeed,
): TaskCreatorState {
  const { form, schedule, verifier, ui } = draft
  return {
    taskForm: {
      ...seed.base,
      id: form.id,
      name: form.name,
      category: form.category,
      prompt: form.prompt,
      maxTurns: form.maxTurns,
      timeoutSeconds: form.timeoutSeconds,
      dynamicPrompt: form.dynamicPrompt,
      subtasks: form.subtasks.length > 0 ? form.subtasks : undefined,
    },
    taskTagsInput: form.tagsInput,
    schedule: { ...schedule },
    verifierType: verifier.type,
    verifierCommand: verifier.command,
    verifierExpectedStdout: verifier.expectedStdout,
    verifierExpectedExitCode: verifier.expectedExitCode,
    verifierFilePath: verifier.filePath,
    verifierExpectedContent: verifier.expectedContent,
    section: ui.section,
    expanded: ui.expanded,
  }
}

export function taskCreatorDefaults(
  base: ProjectTaskDefinition,
  schedule: FriendlySchedule,
): TaskCreatorState {
  const defaults = defaultTaskCreateDraftData()
  return {
    taskForm: { ...base, subtasks: undefined },
    taskTagsInput: '',
    schedule: { ...schedule },
    verifierType: 'none',
    verifierCommand: '',
    verifierExpectedStdout: '',
    verifierExpectedExitCode: defaults.verifier.expectedExitCode,
    verifierFilePath: '',
    verifierExpectedContent: '',
    section: defaults.ui.section,
    expanded: { ...defaults.ui.expanded },
  }
}

export function toggleTaskCreatorSection(
  expanded: Record<TaskCreateSection, boolean>,
  section: TaskCreateSection,
): Record<TaskCreateSection, boolean> {
  return { ...expanded, [section]: !expanded[section] }
}

/** Drafts are written while the user types but never for an untouched form (see persistTaskDraft). */
export function shouldPersistTaskCreatorDraft(options: {
  isCreatingTask: boolean
  saving: boolean
}): boolean {
  return options.isCreatingTask && !options.saving
}

export function describeDraftRestored(savedAt: Date, now: Date = new Date()): string {
  const ageMs = Math.max(0, now.getTime() - savedAt.getTime())
  const minutes = Math.floor(ageMs / 60000)
  if (minutes < 1) return 'Restored your unsubmitted draft from a moment ago'
  if (minutes < 60) return `Restored your unsubmitted draft from ${minutes} minute${minutes === 1 ? '' : 's'} ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `Restored your unsubmitted draft from ${hours} hour${hours === 1 ? '' : 's'} ago`
  const days = Math.floor(hours / 24)
  return `Restored your unsubmitted draft from ${days} day${days === 1 ? '' : 's'} ago`
}

/** Typing pauses longer than this flush the draft; short pauses avoid a write per keystroke. */
export const TASK_DRAFT_SAVE_DEBOUNCE_MS = 600
