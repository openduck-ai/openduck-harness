import test from 'node:test'
import assert from 'node:assert/strict'
import type { HarnessRunResponse } from '@aaif/goose-hub-core'
import {
  formatDuration,
  escapeHtml,
  formatInlineMarkdown,
  markdownToHtml,
  buildTaskPdfData,
  generateTaskPdfHtml,
  generateTaskPdfBinary,
  exportTaskPdf,
} from './taskPdfExport.ts'

test('formatDuration formats milliseconds into human-readable strings', () => {
  assert.equal(formatDuration(0), '0s')
  assert.equal(formatDuration(450), '450ms')
  assert.equal(formatDuration(1250), '1.25s')
  assert.equal(formatDuration(65000), '1m 5s')
  assert.equal(formatDuration(125000), '2m 5s')
})

test('escapeHtml prevents XSS characters', () => {
  assert.equal(escapeHtml('<script>alert("xss")</script>'), '&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;')
  assert.equal(escapeHtml("Tom & Jerry's"), 'Tom &amp; Jerry&#039;s')
})

test('formatInlineMarkdown converts inline markdown', () => {
  assert.equal(formatInlineMarkdown('Use `git status` command'), 'Use <code>git status</code> command')
  assert.equal(formatInlineMarkdown('This is **bold** and *italic*'), 'This is <strong>bold</strong> and <em>italic</em>')
  assert.equal(
    formatInlineMarkdown('[OpenDuck](https://openduck.ai)'),
    '<a href="https://openduck.ai" target="_blank" rel="noopener noreferrer">OpenDuck</a>',
  )
})

test('markdownToHtml converts headings, lists, code blocks, tables and quotes', () => {
  const md = [
    '# Conclusion Title',
    'Task finished cleanly.',
    '## Key Results',
    '- Verified unit tests',
    '- Deployed migration',
    '1. Step one',
    '2. Step two',
    '> Important notice',
    '```bash',
    'cargo test',
    '```',
    '| Feature | Status |',
    '|---|---|',
    '| Export | Ready |',
  ].join('\n')

  const html = markdownToHtml(md)
  assert.ok(html.includes('<h1>Conclusion Title</h1>'))
  assert.ok(html.includes('<p>Task finished cleanly.</p>'))
  assert.ok(html.includes('<h2>Key Results</h2>'))
  assert.ok(html.includes('<ul>'))
  assert.ok(html.includes('<li>Verified unit tests</li>'))
  assert.ok(html.includes('<ol>'))
  assert.ok(html.includes('<li>Step one</li>'))
  assert.ok(html.includes('<blockquote>Important notice</blockquote>'))
  assert.ok(html.includes('<pre><code>cargo test</code></pre>'))
  assert.ok(html.includes('<table class="pdf-table">'))
  assert.ok(html.includes('<th>Feature</th>'))
  assert.ok(html.includes('<td>Ready</td>'))
})

test('buildTaskPdfData extracts task basic info and conclusion', () => {
  const run: HarnessRunResponse = {
    taskId: 'task-build-index',
    status: 'success',
    stepCount: 12,
    toolCallsCount: 5,
    durationMs: 4200,
    finalAnswer: 'All index operations were completed.',
    projectSlug: 'analytics-svc',
    trajectory: {
      taskId: 'task-build-index',
      policyName: 'agent-fifo',
      startedAt: '2026-10-26T04:00:00Z',
      completedAt: '2026-10-26T04:00:04Z',
      steps: [
        {
          stepNumber: 1,
          agentAction: { kind: 'CallTools', toolCalls: [] },
          promptMessages: [],
          tokenUsage: { inputTokens: 500, outputTokens: 120, totalTokens: 620 },
        },
      ],
    },
    continuation: {
      taskId: 'task-build-index',
      createdAt: '2026-10-26T04:00:04Z',
      stopReason: 'completed',
      compactedSummary: 'Summary of steps',
      completedSubtasks: [
        {
          subtaskId: 'st-1',
          title: 'Build table schema',
          status: 'success',
          modifiedFiles: ['schema.sql'],
          summary: 'Schema applied successfully.',
          stepCount: 4,
        },
      ],
      subtaskPlan: [],
    },
  }

  const data = buildTaskPdfData(run, { taskName: 'Index Construction' })

  assert.equal(data.taskId, 'task-build-index')
  assert.equal(data.taskName, 'Index Construction')
  assert.equal(data.projectSlug, 'analytics-svc')
  assert.equal(data.status, 'SUCCESS')
  assert.equal(data.policyName, 'agent-fifo')
  assert.equal(data.durationMs, 4200)
  assert.equal(data.durationFormatted, '4.20s')
  assert.equal(data.totalSteps, 12)
  assert.equal(data.toolCallsCount, 5)
  assert.equal(data.finalAnswer, 'All index operations were completed.')
  assert.equal(data.subtasks.length, 1)
  assert.equal(data.subtasks[0].subtaskId, 'st-1')
  assert.equal(data.subtasks[0].summary, 'Schema applied successfully.')
  assert.equal(data.totalTokens, 620)
})

test('generateTaskPdfHtml generates complete HTML document with metadata and conclusion', () => {
  const data = {
    taskId: 'task-7788',
    taskName: 'Data Pipeline',
    projectSlug: 'etl-core',
    status: 'SUCCESS',
    policyName: 'agent',
    startedAt: '2026-10-26T04:10:00Z',
    completedAt: '2026-10-26T04:10:30Z',
    durationMs: 30000,
    durationFormatted: '30.00s',
    totalSteps: 8,
    toolCallsCount: 4,
    totalTokens: 2500,
    inputTokens: 2000,
    outputTokens: 500,
    finalAnswer: 'Pipeline executed with 10,000 rows processed.',
    subtasks: [
      {
        subtaskId: 'st-101',
        title: 'Extract from S3',
        status: 'success',
        summary: 'Extracted 10k rows cleanly.',
      },
    ],
    error: null,
    exportDate: '2026-10-26 05:00:00',
  }

  const html = generateTaskPdfHtml(data)
  assert.ok(html.includes('Task-task-7788-Report'))
  assert.ok(html.includes('task-7788'))
  assert.ok(html.includes('Data Pipeline'))
  assert.ok(html.includes('etl-core'))
  assert.ok(html.includes('SUCCESS'))
  assert.ok(html.includes('30.00s'))
  assert.ok(html.includes('Pipeline executed with 10,000 rows processed.'))
  assert.ok(html.includes('st-101'))
  assert.ok(html.includes('Extract from S3'))
  assert.ok(html.includes('Extracted 10k rows cleanly.'))
})

test('generateTaskPdfBinary produces a valid PDF 1.4 binary', () => {
  const data = {
    taskId: 'task-bin-pdf',
    taskName: 'Binary PDF Test',
    projectSlug: 'test-project',
    status: 'SUCCESS',
    policyName: 'agent',
    startedAt: '2026-10-26T04:20:00Z',
    completedAt: '2026-10-26T04:20:10Z',
    durationMs: 10000,
    durationFormatted: '10.00s',
    totalSteps: 5,
    toolCallsCount: 2,
    totalTokens: 1200,
    inputTokens: 1000,
    outputTokens: 200,
    finalAnswer: 'Validation passed successfully without warnings.',
    subtasks: [
      {
        subtaskId: 'st-1',
        title: 'Run Lint',
        status: 'success',
        summary: 'Zero warnings.',
      },
    ],
    error: null,
    exportDate: '2026-10-26 05:00:00',
  }

  const bytes = generateTaskPdfBinary(data)
  const text = new TextDecoder('latin1').decode(bytes)

  assert.ok(text.startsWith('%PDF-1.4'))
  assert.ok(text.includes('/Type /Catalog'))
  assert.ok(text.includes('/Type /Pages'))
  assert.ok(text.includes('task-bin-pdf'))
  assert.ok(text.includes('Validation passed successfully without warnings.'))
  assert.ok(text.includes('startxref'))
  assert.ok(text.trim().endsWith('%%EOF'))
})

test('exportTaskPdf routes to appropriate method without throwing', () => {
  const run: HarnessRunResponse = {
    taskId: 'task-route-test',
    status: 'success',
    stepCount: 1,
    toolCallsCount: 0,
    durationMs: 500,
    finalAnswer: 'All done',
    trajectory: {
      taskId: 'task-route-test',
      policyName: 'agent',
      startedAt: '2026-10-26T04:00:00Z',
      steps: [],
    },
  }

  // In test environment, window/document are absent so it gracefully uses binary fallback
  assert.doesNotThrow(() => {
    exportTaskPdf(run, { method: 'download' })
  })
  assert.doesNotThrow(() => {
    exportTaskPdf(run, { method: 'preview' })
  })
  assert.doesNotThrow(() => {
    exportTaskPdf(run, { method: 'print' })
  })
})
