export type ContinueTurnsMode = 'additional' | 'total'

export function isContinuableStatus(status: string | undefined | null): boolean {
  const normalized = String(status ?? '').toLowerCase()
  return normalized === 'cancelled' || normalized === 'failure' || normalized === 'timeout'
}

export interface ContinueTurnCalculationOptions {
  previousSteps: number
  mode: ContinueTurnsMode
  value: number
}

/**
 * Calculates the target max_turns for continuing a cancelled task.
 * - 'additional': adds the specified number of turns to previousSteps.
 * - 'total': sets the absolute total turn limit (at least 1).
 */
export function calculateContinueTurns(options: ContinueTurnCalculationOptions): number {
  const { previousSteps, mode, value } = options
  const safePrev = Math.max(0, Math.floor(previousSteps))
  const safeValue = Math.max(1, Math.floor(value || 1))

  if (mode === 'additional') {
    return safePrev + safeValue
  }

  return Math.max(1, safeValue)
}

/** Extra turns applied to the in-progress subtask of a continued run. */
export function extraTurnsForContinue(options: ContinueTurnCalculationOptions): number {
  const { previousSteps, mode, value } = options
  const safePrev = Math.max(0, Math.floor(previousSteps))
  const safeValue = Math.max(1, Math.floor(value || 1))
  if (mode === 'additional') {
    return safeValue
  }
  return Math.max(1, safeValue - safePrev)
}

/**
 * Returns human-friendly text describing the turn budget adjustment.
 */
export function formatContinueBudgetSummary(
  previousSteps: number,
  targetMaxTurns: number,
): {
  previousSteps: number
  targetMaxTurns: number
  diffText: string
} {
  const diff = targetMaxTurns - previousSteps
  const diffText = diff >= 0 ? `+${diff} turns` : `${diff} turns`
  return {
    previousSteps,
    targetMaxTurns,
    diffText,
  }
}
