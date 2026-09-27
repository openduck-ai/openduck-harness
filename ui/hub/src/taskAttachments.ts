export const TASK_ATTACHMENT_DIR = '.goose/task-attachments'

export const TASK_ATTACHMENT_ACCEPT =
  '.xlsx,.xls,.xlsm,.xlsb,.pdf,.png,.jpg,.jpeg,.gif,.webp,.bmp,application/pdf,application/vnd.ms-excel,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,image/*'

const ACCEPTED_EXTENSIONS = new Set([
  'xlsx',
  'xls',
  'xlsm',
  'xlsb',
  'pdf',
  'png',
  'jpg',
  'jpeg',
  'gif',
  'webp',
  'bmp',
])

export function isAcceptedTaskAttachment(fileName: string): boolean {
  const base = fileName.split(/[/\\]/).pop() ?? fileName
  const dot = base.lastIndexOf('.')
  if (dot <= 0) return false
  return ACCEPTED_EXTENSIONS.has(base.slice(dot + 1).toLowerCase())
}

export function sanitizeAttachmentFileName(name: string): string {
  const base = name.split(/[/\\]/).pop()?.trim() ?? ''
  const cleaned = base.replace(/[^\w.\-]+/g, '_').replace(/^\.+/, '')
  return cleaned || 'attachment'
}

export function workspacePathForAttachment(fileName: string, taskId: string): string {
  const safeTask = sanitizeAttachmentFileName(taskId)
  const safeName = sanitizeAttachmentFileName(fileName)
  return `${TASK_ATTACHMENT_DIR}/${safeTask}/${safeName}`
}

export function taskAttachmentDir(taskId: string): string {
  return `${TASK_ATTACHMENT_DIR}/${sanitizeAttachmentFileName(taskId)}`
}

export function workspacePathsForAttachments(
  fileNames: string[],
  taskId: string,
  existingPaths: string[] = [],
): string[] {
  const used = new Set(existingPaths.filter(Boolean))
  return fileNames.map(name => {
    const initial = workspacePathForAttachment(name, taskId)
    if (!used.has(initial)) {
      used.add(initial)
      return initial
    }
    const slash = initial.lastIndexOf('/')
    const file = initial.slice(slash + 1)
    const dir = initial.slice(0, slash + 1)
    const dot = file.lastIndexOf('.')
    const stem = dot > 0 ? file.slice(0, dot) : file
    const ext = dot > 0 ? file.slice(dot) : ''
    let n = 2
    let candidate = `${dir}${stem}-${n}${ext}`
    while (used.has(candidate)) {
      n += 1
      candidate = `${dir}${stem}-${n}${ext}`
    }
    used.add(candidate)
    return candidate
  })
}

export const ATTACHED_FILES_PROMPT_HEADER =
  'Attached files (use these workspace files when completing the task):'

export function composePromptWithAttachments(prompt: string, attachmentPaths: string[]): string {
  const paths = attachmentPaths.map(p => p.trim()).filter(Boolean)
  if (paths.length === 0) return prompt

  const missing = paths.filter(path => !prompt.includes(`@${path}`))
  if (missing.length === 0) return prompt

  const listing = missing.map(path => `- @${path}`).join('\n')
  const trimmed = prompt.trimEnd()
  if (!trimmed) return `${ATTACHED_FILES_PROMPT_HEADER}\n${listing}`
  if (trimmed.includes(ATTACHED_FILES_PROMPT_HEADER)) {
    return `${trimmed}\n${listing}`
  }
  return `${trimmed}\n\n${ATTACHED_FILES_PROMPT_HEADER}\n${listing}`
}

export async function listTaskAttachmentPaths(
  listFiles: (path: string) => Promise<{
    entries?: Array<{ name: string; path: string; kind: string }>
  }>,
  taskId: string,
): Promise<string[]> {
  if (!taskId.trim()) return []
  try {
    const response = await listFiles(taskAttachmentDir(taskId))
    return (response.entries || [])
      .filter(entry => entry.kind !== 'dir')
      .map(entry => entry.path.replace(/^\/+/, ''))
      .filter(Boolean)
  } catch {
    return []
  }
}

export async function uploadAttachmentsAndComposePrompt(options: {
  prompt: string
  taskId: string
  files: Array<{ name: string; bytes: Uint8Array }>
  writeFileBytes: (path: string, content: Uint8Array) => Promise<unknown>
  existingPaths?: string[]
}): Promise<string> {
  const { prompt, taskId, files, writeFileBytes, existingPaths = [] } = options
  if (files.length === 0) return prompt
  const paths = workspacePathsForAttachments(
    files.map(file => file.name),
    taskId,
    existingPaths,
  )
  for (let i = 0; i < files.length; i++) {
    await writeFileBytes(paths[i], files[i].bytes)
  }
  return composePromptWithAttachments(prompt, paths)
}
