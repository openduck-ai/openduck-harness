import assert from 'node:assert/strict'
import test from 'node:test'
import {
  composePromptWithAttachments,
  isAcceptedTaskAttachment,
  listTaskAttachmentPaths,
  TASK_ATTACHMENT_ACCEPT,
  uploadAttachmentsAndComposePrompt,
  workspacePathForAttachment,
  workspacePathsForAttachments,
} from './taskAttachments.ts'

test('composePromptWithAttachments keeps original instructions and references each attached file', () => {
  const prompt = 'Summarize the quarterly numbers and extract tables from the spec.'
  const paths = [
    '.goose/task-attachments/task-42/numbers.xlsx',
    '.goose/task-attachments/task-42/spec.pdf',
    '.goose/task-attachments/task-42/chart.png',
    '.goose/task-attachments/task-42/photo.jpg',
  ]

  const composed = composePromptWithAttachments(prompt, paths)

  assert.ok(composed.includes(prompt), 'composed prompt must keep the original instructions')
  for (const path of paths) {
    assert.ok(composed.includes(path), `composed prompt must reference ${path}`)
  }
})

test('composePromptWithAttachments returns the original prompt when there are no attachments', () => {
  const prompt = 'Just implement the feature.'
  assert.equal(composePromptWithAttachments(prompt, []), prompt)
  assert.equal(composePromptWithAttachments(prompt, ['', '  ']), prompt)
})

test('workspacePathForAttachment stores uploads under a task-scoped workspace path', () => {
  const path = workspacePathForAttachment('Q1 Report.xlsx', 'task-99')
  assert.ok(!path.startsWith('/'), 'path must be workspace-relative')
  assert.ok(path.includes('task-99'))
  assert.ok(path.endsWith('Q1_Report.xlsx') || path.includes('Q1'))
  assert.match(path, /\.xlsx$/i)
})

test('workspacePathsForAttachments disambiguates duplicate file names', () => {
  const paths = workspacePathsForAttachments(['spec.pdf', 'spec.pdf'], 'task-1')
  assert.equal(paths.length, 2)
  assert.notEqual(paths[0], paths[1])
  assert.ok(paths.every(p => p.includes('spec') && p.endsWith('.pdf')))
})

test('workspacePathsForAttachments avoids colliding with existing task attachments', () => {
  const existing = ['.goose/task-attachments/task-1/spec.pdf']
  const paths = workspacePathsForAttachments(['spec.pdf'], 'task-1', existing)
  assert.equal(paths.length, 1)
  assert.notEqual(paths[0], existing[0])
  assert.ok(paths[0].endsWith('.pdf'))
})

test('composePromptWithAttachments skips files already mentioned in the prompt', () => {
  const existingPath = '.goose/task-attachments/task-42/spec.pdf'
  const newPath = '.goose/task-attachments/task-42/chart.png'
  const prompt = `Summarize @${existingPath} and the screenshot.`
  const composed = composePromptWithAttachments(prompt, [existingPath, newPath])
  assert.equal(composed.includes(`@${existingPath}`), true)
  assert.equal(composed.includes(newPath), true)
  assert.equal((composed.match(new RegExp(existingPath.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g')) || []).length, 1)
})

test('composePromptWithAttachments returns the original prompt when every file is already mentioned', () => {
  const path = '.goose/task-attachments/task-42/spec.pdf'
  const prompt = `Use @${path} when answering.`
  assert.equal(composePromptWithAttachments(prompt, [path]), prompt)
})

test('listTaskAttachmentPaths returns workspace files for a task and ignores missing folders', async () => {
  const listed = await listTaskAttachmentPaths(async path => {
    assert.equal(path, '.goose/task-attachments/task-9')
    return {
      entries: [
        { name: 'spec.pdf', path: `${path}/spec.pdf`, kind: 'file' },
        { name: 'nested', path: `${path}/nested`, kind: 'dir' },
      ],
    }
  }, 'task-9')
  assert.deepEqual(listed, ['.goose/task-attachments/task-9/spec.pdf'])

  const missing = await listTaskAttachmentPaths(async () => {
    throw new Error('not found')
  }, 'task-9')
  assert.deepEqual(missing, [])
})

test('uploadAttachmentsAndComposePrompt writes original bytes then includes each path in the prompt', async () => {
  const written: { path: string; bytes: number[] }[] = []
  const prompt = 'Summarize the spreadsheet, cite the PDF, and describe the screenshot.'
  const xlsx = new Uint8Array([0x50, 0x4b, 0x03, 0x04, 0x00])
  const pdf = new Uint8Array([0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x34, 0x80])
  const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])

  const composed = await uploadAttachmentsAndComposePrompt({
    prompt,
    taskId: 'task-7',
    files: [
      { name: 'numbers.xlsx', bytes: xlsx },
      { name: 'spec.pdf', bytes: pdf },
      { name: 'chart.png', bytes: png },
    ],
    writeFileBytes: async (path, bytes) => {
      written.push({ path, bytes: Array.from(bytes) })
    },
  })

  assert.ok(composed.includes(prompt))
  assert.equal(written.length, 3)
  assert.deepEqual(written[0].bytes, Array.from(xlsx))
  assert.deepEqual(written[1].bytes, Array.from(pdf))
  assert.deepEqual(written[2].bytes, Array.from(png))
  assert.equal(written[2].bytes[0], 0x89)
  for (const entry of written) {
    assert.ok(composed.includes(entry.path), `prompt must reference uploaded path ${entry.path}`)
  }
})

test('accepted attachment types include Excel, PDF, and images', () => {
  const accept = TASK_ATTACHMENT_ACCEPT.toLowerCase()
  assert.ok(accept.includes('.xlsx'))
  assert.ok(accept.includes('.pdf'))
  assert.ok(accept.includes('image') || accept.includes('.png'))
  assert.equal(isAcceptedTaskAttachment('budget.xlsx'), true)
  assert.equal(isAcceptedTaskAttachment('spec.pdf'), true)
  assert.equal(isAcceptedTaskAttachment('chart.PNG'), true)
  assert.equal(isAcceptedTaskAttachment('photo.jpg'), true)
  assert.equal(isAcceptedTaskAttachment('notes.txt'), false)
})
