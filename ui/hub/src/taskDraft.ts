import type {
  ProjectTaskDefinition,
  SubtaskExpectedOutput,
  SubtaskSpec,
  TaskDraft,
} from '@aaif/goose-hub-core'

export function shouldReplaceGeneratedTaskId(id: string, editing: boolean): boolean {
  return !editing && /^task-\d{6}$/.test(id)
}

export function applyTaskDraft(
  form: ProjectTaskDefinition,
  draft: TaskDraft,
  options: { replaceId: boolean },
): ProjectTaskDefinition {
  const subtasks =
    draft.largeTask && draft.subtasks.length >= 2 ? draft.subtasks : undefined
  return {
    ...form,
    id: options.replaceId && draft.id ? draft.id : form.id,
    name: draft.name || form.name,
    category: draft.category || form.category,
    tags: draft.tags ?? form.tags,
    prompt: draft.prompt || form.prompt,
    maxTurns: draft.maxTurns || form.maxTurns,
    timeoutSeconds: draft.timeoutSeconds || form.timeoutSeconds,
    subtasks,
  }
}

/** Sends the open form back with the next chat turn so the draft can revise it. */
export function taskFormToDraft(
  form: ProjectTaskDefinition,
  tagsInput: string,
): TaskDraft | undefined {
  const subtasks = form.subtasks ?? []
  if (!form.prompt.trim() && subtasks.length < 2) return undefined
  return {
    id: form.id,
    name: form.name?.trim() || form.id,
    category: form.category,
    tags: tagsInput
      .split(',')
      .map(tag => tag.trim())
      .filter(Boolean),
    prompt: form.prompt,
    maxTurns: form.maxTurns || 25,
    timeoutSeconds: form.timeoutSeconds || 300,
    largeTask: subtasks.length >= 2,
    complexity: subtasks.length >= 2 ? 'complex' : 'medium',
    reasoning: '',
    subtasks,
  }
}

export function blankExpectedOutput(): SubtaskExpectedOutput {
  return { description: '', kind: 'note' }
}

export function blankSubtask(index: number, defaultTurns: number = 8): SubtaskSpec {
  return {
    id: `subtask-${index + 1}`,
    title: '',
    description: '',
    maxTurns: defaultTurns,
    expectedOutputs: [blankExpectedOutput()],
  }
}

export function moveSubtask(subtasks: SubtaskSpec[], index: number, delta: number): SubtaskSpec[] {
  const next = index + delta
  if (next < 0 || next >= subtasks.length) return subtasks
  const copy = subtasks.slice()
  const [item] = copy.splice(index, 1)
  copy.splice(next, 0, item)
  return copy
}

export function subtasksForSave(subtasks: SubtaskSpec[] | undefined): SubtaskSpec[] | undefined {
  if (!subtasks?.length) return undefined
  const cleaned = subtasks
    .map((subtask, index) => {
      const expectedOutputs = (subtask.expectedOutputs ?? [])
        .map(output => ({
          description: output.description.trim(),
          kind: output.kind || 'note',
          command: emptyToUndefined(output.command),
          expectedExitCode: output.kind === 'command' ? output.expectedExitCode ?? 0 : undefined,
          expectedStdout: emptyToUndefined(output.expectedStdout),
          filePath: emptyToUndefined(output.filePath),
          expectedContent: emptyToUndefined(output.expectedContent),
        }))
        .filter(output => output.description || output.command || output.filePath)
      const targetFiles = (subtask.targetFiles ?? []).map(file => file.trim()).filter(Boolean)
      return {
        id: subtask.id.trim() || `subtask-${index + 1}`,
        title: subtask.title.trim(),
        description: subtask.description.trim(),
        targetFiles: targetFiles.length ? targetFiles : undefined,
        maxTurns: subtask.maxTurns,
        explorationBudget: subtask.explorationBudget,
        expectedOutputs: expectedOutputs.length ? expectedOutputs : undefined,
      }
    })
    .filter(subtask => subtask.title || subtask.description)
    .map((subtask, index) => ({
      ...subtask,
      title: subtask.title || `Subtask ${index + 1}`,
    }))
  return cleaned.length ? cleaned : undefined
}

export function subtaskSaveError(subtasks: SubtaskSpec[] | undefined): string | null {
  for (const [index, subtask] of (subtasks ?? []).entries()) {
    for (const output of subtask.expectedOutputs ?? []) {
      if (output.kind === 'file' && !output.filePath?.trim()) {
        return `Subtask ${index + 1} has a file check without a path`
      }
      if (output.kind === 'command' && !output.command?.trim()) {
        return `Subtask ${index + 1} has a command check without a command`
      }
    }
  }
  return null
}

function emptyToUndefined(value: string | undefined): string | undefined {
  const trimmed = value?.trim()
  return trimmed ? trimmed : undefined
}

/**
 * Calculates the total task max turns from subtasks.
 * When subtasks are present, task max turns is the sum of all subtask max turns.
 */
export function calculateTaskMaxTurnsFromSubtasks(
  subtasks: SubtaskSpec[] | undefined,
  currentTaskMaxTurns?: number,
  maxTurnsLimit: number = 500,
): number {
  if (!subtasks || subtasks.length === 0) {
    return Math.max(1, Math.min(maxTurnsLimit, currentTaskMaxTurns || 25))
  }
  const sum = subtasks.reduce((acc, subtask) => {
    const turns =
      typeof subtask.maxTurns === 'number' && subtask.maxTurns > 0
        ? Math.round(subtask.maxTurns)
        : 8
    return acc + turns
  }, 0)
  return Math.max(1, Math.min(maxTurnsLimit, sum))
}

/**
 * Re-calculates each subtask's maxTurns proportionally when the parent task's max turns changes.
 * Guarantees that:
 * 1. Each subtask gets at least 1 turn.
 * 2. Total turns across subtasks matches totalTurns (clamped to [count, maxTurnsLimit]).
 * 3. Proportions between existing subtasks are preserved.
 */
export function recalculateSubtasksTurns(
  subtasks: SubtaskSpec[] | undefined,
  totalTurns: number,
  maxTurnsLimit: number = 500,
): SubtaskSpec[] | undefined {
  if (!subtasks || subtasks.length === 0) return undefined

  const count = subtasks.length
  const clampedTotal = Math.max(count, Math.min(maxTurnsLimit, Math.round(totalTurns) || count))

  if (count === 1) {
    return [{ ...subtasks[0], maxTurns: clampedTotal }]
  }

  const currentTurns = subtasks.map(s =>
    typeof s.maxTurns === 'number' && s.maxTurns > 0 ? s.maxTurns : 8,
  )
  const currentSum = currentTurns.reduce((acc, t) => acc + t, 0)

  const quotas = subtasks.map((_, i) =>
    currentSum > 0 ? (clampedTotal * currentTurns[i]) / currentSum : clampedTotal / count,
  )

  const baseTurns = quotas.map(q => Math.max(1, Math.floor(q)))
  const allocated = baseTurns.reduce((acc, t) => acc + t, 0)
  const remainder = clampedTotal - allocated

  if (remainder > 0) {
    const fractionalParts = quotas.map((q, idx) => ({
      idx,
      frac: q - Math.floor(q),
    }))
    // Sort descending by fractional part, breaking ties by index
    fractionalParts.sort((a, b) => b.frac - a.frac || a.idx - b.idx)

    for (let i = 0; i < remainder; i++) {
      baseTurns[fractionalParts[i % count].idx] += 1
    }
  }

  return subtasks.map((subtask, i) => ({
    ...subtask,
    maxTurns: baseTurns[i],
  }))
}
