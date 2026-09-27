export interface DynamicPromptTaskOption {
  id: string
  name: string
}

export function dynamicPromptTasks(
  tasks: Array<{ id: string; name?: string | null; dynamicPrompt?: boolean }>,
): DynamicPromptTaskOption[] {
  return tasks
    .filter(task => task.dynamicPrompt)
    .map(task => ({
      id: task.id,
      name: task.name?.trim() || task.id,
    }))
    .sort((a, b) => a.name.localeCompare(b.name) || a.id.localeCompare(b.id))
}

/** Selected file text becomes the one-run extra prompt, with its source path. */
export function formatSelectionExtraPrompt(filePath: string, selectedText: string): string {
  const text = selectedText.trim()
  const path = filePath.trim()
  if (!text) return ''
  if (!path) return text
  return `Selected excerpt from \`${path}\`:\n\n${text}`
}

export function clampMenuPosition(
  x: number,
  y: number,
  viewportWidth: number,
  viewportHeight: number,
): { x: number; y: number } {
  const menuWidth = 320
  const menuHeight = 280
  const margin = 8
  const left = Math.min(Math.max(margin, x), Math.max(margin, viewportWidth - menuWidth - margin))
  const top = Math.min(
    Math.max(margin, y + 6),
    Math.max(margin, viewportHeight - menuHeight - margin),
  )
  return { x: left, y: top }
}
