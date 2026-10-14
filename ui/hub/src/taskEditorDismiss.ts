export interface EscapeCloseInput {
  key: string
  defaultPrevented: boolean
  isComposing: boolean
  imeLocked: boolean
  mentionOpen: boolean
  saving: boolean
  targetInsideEditor: boolean
}

/** Escape closes the task editor only from outside it, never while a mention menu or IME is active. */
export function escapeClosesTaskEditor(input: EscapeCloseInput): boolean {
  if (input.saving || input.key !== 'Escape') return false
  if (input.defaultPrevented || input.isComposing || input.imeLocked) return false
  if (input.mentionOpen || input.targetInsideEditor) return false
  return true
}
