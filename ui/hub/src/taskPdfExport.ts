import type { HarnessRunResponse } from '@aaif/goose-hub-core'
import {
  extractFinalAnswer,
  calculateTrajectoryMetrics,
  extractCompletedSubtasks,
  type CompletedSubtaskInfo,
} from './taskTrajectory.ts'

export interface TaskPdfData {
  taskId: string
  taskName: string
  projectSlug: string
  status: string
  policyName: string
  startedAt?: string | null
  completedAt?: string | null
  durationMs: number
  durationFormatted: string
  totalSteps: number
  toolCallsCount: number
  totalTokens: number
  inputTokens: number
  outputTokens: number
  finalAnswer: string | null
  subtasks: CompletedSubtaskInfo[]
  error?: string | null
  exportDate: string
}

export interface ExportPdfOptions {
  taskName?: string
  projectSlug?: string
  method?: 'print' | 'download' | 'preview'
}

/**
 * Formats a millisecond duration into a clean, human-readable string.
 */
export function formatDuration(ms: number): string {
  if (ms <= 0) return '0s'
  if (ms < 1000) return `${ms}ms`
  const seconds = ms / 1000
  if (seconds < 60) {
    return `${seconds.toFixed(2)}s`
  }
  const mins = Math.floor(seconds / 60)
  const remainingSecs = Math.round(seconds % 60)
  return `${mins}m ${remainingSecs}s`
}

/**
 * Escapes HTML characters to prevent XSS.
 */
export function escapeHtml(str: unknown): string {
  return String(str ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;')
}

/**
 * Converts inline Markdown syntax (bold, italic, code, links) to safe HTML.
 */
export function formatInlineMarkdown(text: string): string {
  let s = escapeHtml(text)
  // Inline code: `code`
  s = s.replace(/`([^`]+)`/g, '<code>$1</code>')
  // Bold: **text** or __text__
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  s = s.replace(/__([^_]+)__/g, '<strong>$1</strong>')
  // Italic: *text* or _text_
  s = s.replace(/\*([^*]+)\*/g, '<em>$1</em>')
  s = s.replace(/_([^_]+)_/g, '<em>$1</em>')
  // Links: [label](url)
  s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>')
  return s
}

/**
 * Lightweight, robust Markdown to HTML converter for report rendering.
 */
export function markdownToHtml(md: string): string {
  if (!md || !md.trim()) return ''
  const lines = md.split(/\r?\n/)
  const out: string[] = []
  let inCodeBlock = false
  let codeBuffer: string[] = []
  let inList: 'ul' | 'ol' | null = null
  let inTable = false

  for (let i = 0; i < lines.length; i++) {
    const rawLine = lines[i]
    const trimmed = rawLine.trim()

    // 1. Fenced Code Block toggle
    if (trimmed.startsWith('```')) {
      if (inCodeBlock) {
        out.push(`<pre><code>${escapeHtml(codeBuffer.join('\n'))}</code></pre>`)
        codeBuffer = []
        inCodeBlock = false
      } else {
        if (inList) {
          out.push(inList === 'ul' ? '</ul>' : '</ol>')
          inList = null
        }
        if (inTable) {
          out.push('</tbody></table>')
          inTable = false
        }
        inCodeBlock = true
      }
      continue
    }

    if (inCodeBlock) {
      codeBuffer.push(rawLine)
      continue
    }

    // 2. Table rows: | Col 1 | Col 2 |
    if (trimmed.startsWith('|') && trimmed.endsWith('|')) {
      const isDelimiter = /^\|(\s*[-:]+[-| :]*)\|$/.test(trimmed)
      if (isDelimiter) {
        continue
      }
      const cells = trimmed
        .slice(1, -1)
        .split('|')
        .map(c => c.trim())

      if (!inTable) {
        if (inList) {
          out.push(inList === 'ul' ? '</ul>' : '</ol>')
          inList = null
        }
        inTable = true
        out.push('<table class="pdf-table"><thead><tr>')
        out.push(cells.map(c => `<th>${formatInlineMarkdown(c)}</th>`).join(''))
        out.push('</tr></thead><tbody>')
      } else {
        out.push('<tr>')
        out.push(cells.map(c => `<td>${formatInlineMarkdown(c)}</td>`).join(''))
        out.push('</tr>')
      }
      continue
    } else if (inTable) {
      out.push('</tbody></table>')
      inTable = false
    }

    // 3. Horizontal rules
    if (/^(\*{3,}|-{3,}|_{3,})$/.test(trimmed)) {
      if (inList) {
        out.push(inList === 'ul' ? '</ul>' : '</ol>')
        inList = null
      }
      out.push('<hr class="pdf-divider" />')
      continue
    }

    // 4. Headings: #, ##, ###, ####
    const headingMatch = trimmed.match(/^(#{1,6})\s+(.*)$/)
    if (headingMatch) {
      if (inList) {
        out.push(inList === 'ul' ? '</ul>' : '</ol>')
        inList = null
      }
      const level = headingMatch[1].length
      out.push(`<h${level}>${formatInlineMarkdown(headingMatch[2])}</h${level}>`)
      continue
    }

    // 5. Blockquotes: > quote
    const quoteMatch = trimmed.match(/^>\s?(.*)$/)
    if (quoteMatch) {
      if (inList) {
        out.push(inList === 'ul' ? '</ul>' : '</ol>')
        inList = null
      }
      out.push(`<blockquote>${formatInlineMarkdown(quoteMatch[1])}</blockquote>`)
      continue
    }

    // 6. Unordered lists: - item, * item
    const ulMatch = trimmed.match(/^[-*+]\s+(.*)$/)
    if (ulMatch) {
      if (inList !== 'ul') {
        if (inList === 'ol') out.push('</ol>')
        out.push('<ul>')
        inList = 'ul'
      }
      out.push(`<li>${formatInlineMarkdown(ulMatch[1])}</li>`)
      continue
    }

    // 7. Ordered lists: 1. item
    const olMatch = trimmed.match(/^\d+\.\s+(.*)$/)
    if (olMatch) {
      if (inList !== 'ol') {
        if (inList === 'ul') out.push('</ul>')
        out.push('<ol>')
        inList = 'ol'
      }
      out.push(`<li>${formatInlineMarkdown(olMatch[1])}</li>`)
      continue
    }

    // If we were in a list and this is not a list item
    if (inList) {
      out.push(inList === 'ul' ? '</ul>' : '</ol>')
      inList = null
    }

    // 8. Blank line
    if (!trimmed) {
      continue
    }

    // 9. Regular Paragraph
    out.push(`<p>${formatInlineMarkdown(trimmed)}</p>`)
  }

  if (inCodeBlock) {
    out.push(`<pre><code>${escapeHtml(codeBuffer.join('\n'))}</code></pre>`)
  }
  if (inList) {
    out.push(inList === 'ul' ? '</ul>' : '</ol>')
  }
  if (inTable) {
    out.push('</tbody></table>')
  }

  return out.join('\n')
}

/**
 * Extracts normalized task basic info and conclusions into a structured data object.
 */
export function buildTaskPdfData(
  run: HarnessRunResponse,
  options?: ExportPdfOptions,
): TaskPdfData {
  const steps = run.trajectory?.steps || []
  const metrics = calculateTrajectoryMetrics(run)
  const finalAnswer = extractFinalAnswer(run)
  const subtasks = extractCompletedSubtasks(run)

  const taskId = run.taskId || 'unknown-task'
  const taskName =
    options?.taskName ||
    (typeof (run.trajectory as unknown as Record<string, unknown>)?.taskName === 'string'
      ? String((run.trajectory as unknown as Record<string, unknown>).taskName)
      : taskId)
  const projectSlug = options?.projectSlug || run.projectSlug || 'default'
  const status = String(run.status || 'unknown').toUpperCase()
  const policyName = run.trajectory?.policyName || 'agent'
  const durationMs = run.durationMs || metrics.durationMs || 0
  const durationFormatted = formatDuration(durationMs)
  const totalSteps = metrics.totalSteps || run.stepCount || steps.length
  const toolCallsCount = metrics.toolCallsCount || run.toolCallsCount || 0

  const totalTokens = metrics.totalTokens || 0
  const inputTokens = metrics.inputTokens || 0
  const outputTokens = metrics.outputTokens || 0

  const startedAt = run.trajectory?.startedAt || null
  const completedAt = run.trajectory?.completedAt || null

  let error: string | null = null
  if (typeof (run as unknown as Record<string, unknown>).error === 'string') {
    error = String((run as unknown as Record<string, unknown>).error)
  }

  return {
    taskId,
    taskName,
    projectSlug,
    status,
    policyName,
    startedAt,
    completedAt,
    durationMs,
    durationFormatted,
    totalSteps,
    toolCallsCount,
    totalTokens,
    inputTokens,
    outputTokens,
    finalAnswer,
    subtasks,
    error,
    exportDate: new Date().toLocaleString(),
  }
}

/**
 * Generates a standalone HTML document styled for high-resolution vector PDF printing.
 */
export function generateTaskPdfHtml(data: TaskPdfData): string {
  const statusClass = `status-${data.status.toLowerCase()}`
  const finalAnswerHtml = data.finalAnswer ? markdownToHtml(data.finalAnswer) : ''

  return `<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <title>Task-${escapeHtml(data.taskId)}-Report</title>
  <style>
    @page {
      size: A4;
      margin: 14mm 14mm 16mm 14mm;
    }
    @media print {
      body {
        -webkit-print-color-adjust: exact !important;
        print-color-adjust: exact !important;
      }
      .no-print {
        display: none !important;
      }
      .page-break-inside-avoid {
        break-inside: avoid !important;
        page-break-inside: avoid !important;
      }
    }
    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }
    body {
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Noto Sans SC", sans-serif;
      color: #0f172a;
      background: #ffffff;
      line-height: 1.5;
      font-size: 13px;
      padding: 24px;
      max-width: 860px;
      margin: 0 auto;
    }
    .report-header {
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
      border-bottom: 2px solid #0284c7;
      padding-bottom: 14px;
      margin-bottom: 20px;
    }
    .report-title-group h1 {
      font-size: 20px;
      font-weight: 700;
      color: #0f172a;
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .report-title-group .report-subtitle {
      font-size: 12px;
      color: #64748b;
      margin-top: 3px;
    }
    .status-badge {
      display: inline-block;
      padding: 4px 10px;
      border-radius: 9999px;
      font-size: 12px;
      font-weight: 700;
      letter-spacing: 0.04em;
      text-transform: uppercase;
    }
    .status-success { background: #ecfdf5; color: #047857; border: 1px solid #a7f3d0; }
    .status-failure, .status-failed, .status-error { background: #fef2f2; color: #b91c1c; border: 1px solid #fecaca; }
    .status-cancelled, .status-timeout { background: #fffbeb; color: #b45309; border: 1px solid #fde68a; }
    .status-running { background: #eff6ff; color: #1d4ed8; border: 1px solid #bfdbfe; }
    .status-unknown, .status-skipped { background: #f1f5f9; color: #475569; border: 1px solid #cbd5e1; }

    .section-title {
      font-size: 13px;
      font-weight: 700;
      color: #1e293b;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      margin: 22px 0 10px 0;
      padding-bottom: 5px;
      border-bottom: 1px solid #e2e8f0;
      display: flex;
      align-items: center;
      gap: 6px;
    }

    .info-grid {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 10px;
      margin-bottom: 18px;
    }
    .info-cell {
      background: #f8fafc;
      border: 1px solid #e2e8f0;
      border-radius: 6px;
      padding: 8px 12px;
      break-inside: avoid;
    }
    .info-label {
      font-size: 11px;
      color: #64748b;
      text-transform: uppercase;
      font-weight: 600;
      letter-spacing: 0.04em;
      margin-bottom: 2px;
    }
    .info-value {
      font-size: 13px;
      font-weight: 600;
      color: #0f172a;
      word-break: break-all;
    }
    .info-value code {
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 12px;
      background: #e2e8f0;
      padding: 1px 4px;
      border-radius: 3px;
    }

    .conclusion-box {
      background: #f8fafc;
      border: 1px solid #cbd5e1;
      border-left: 4px solid #0284c7;
      border-radius: 6px;
      padding: 14px 18px;
      margin-bottom: 20px;
      font-size: 13px;
      line-height: 1.6;
      break-inside: avoid;
    }
    .conclusion-content {
      color: #1e293b;
    }
    .conclusion-content h1, .conclusion-content h2, .conclusion-content h3 {
      font-size: 14px;
      font-weight: 700;
      margin: 12px 0 6px 0;
      color: #0f172a;
    }
    .conclusion-content p {
      margin-bottom: 8px;
    }
    .conclusion-content ul, .conclusion-content ol {
      margin: 6px 0 10px 20px;
    }
    .conclusion-content li {
      margin-bottom: 3px;
    }
    .conclusion-content pre {
      background: #0f172a;
      color: #f8fafc;
      padding: 10px 12px;
      border-radius: 6px;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 12px;
      overflow-x: auto;
      white-space: pre-wrap;
      margin: 8px 0;
    }
    .conclusion-content code {
      background: #e2e8f0;
      color: #0f172a;
      padding: 1px 4px;
      border-radius: 3px;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 12px;
    }
    .conclusion-content blockquote {
      border-left: 3px solid #94a3b8;
      padding-left: 10px;
      color: #475569;
      font-style: italic;
      margin: 8px 0;
    }
    .pdf-table {
      width: 100%;
      border-collapse: collapse;
      margin: 10px 0;
    }
    .pdf-table th, .pdf-table td {
      border: 1px solid #cbd5e1;
      padding: 6px 10px;
      text-align: left;
      font-size: 12px;
    }
    .pdf-table th {
      background: #e2e8f0;
      font-weight: 600;
    }
    .pdf-divider {
      border: none;
      border-top: 1px solid #e2e8f0;
      margin: 14px 0;
    }

    .subtask-card {
      background: #ffffff;
      border: 1px solid #e2e8f0;
      border-radius: 6px;
      padding: 12px 14px;
      margin-bottom: 10px;
      break-inside: avoid;
    }
    .subtask-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 6px;
    }
    .subtask-title-group {
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .subtask-id {
      background: #e0f2fe;
      color: #0369a1;
      font-size: 11px;
      font-weight: 700;
      padding: 1px 6px;
      border-radius: 4px;
      font-family: monospace;
    }
    .subtask-title {
      font-weight: 600;
      font-size: 13px;
      color: #0f172a;
    }
    .subtask-conclusion {
      font-size: 12px;
      color: #334155;
      background: #f8fafc;
      border-radius: 4px;
      padding: 8px 10px;
      margin-top: 6px;
      line-height: 1.5;
    }

    .error-card {
      background: #fef2f2;
      border: 1px solid #fecaca;
      border-left: 4px solid #ef4444;
      border-radius: 6px;
      padding: 12px 16px;
      margin-bottom: 18px;
      color: #991b1b;
      font-size: 13px;
      break-inside: avoid;
    }

    .report-footer {
      margin-top: 24px;
      padding-top: 10px;
      border-top: 1px solid #e2e8f0;
      display: flex;
      justify-content: space-between;
      font-size: 11px;
      color: #94a3b8;
    }
  </style>
</head>
<body>
  <div class="report-header">
    <div class="report-title-group">
      <h1>📋 OpenDuck Task Report / 任务执行报告</h1>
      <div class="report-subtitle">
        Project: <strong>${escapeHtml(data.projectSlug)}</strong> &bull; Generated: ${escapeHtml(data.exportDate)}
      </div>
    </div>
    <div>
      <span class="status-badge ${statusClass}">${escapeHtml(data.status)}</span>
    </div>
  </div>

  <div class="section-title">
    <span>基本信息 (Basic Information)</span>
  </div>
  <div class="info-grid">
    <div class="info-cell">
      <div class="info-label">任务 ID (Task ID)</div>
      <div class="info-value"><code>${escapeHtml(data.taskId)}</code></div>
    </div>
    <div class="info-cell">
      <div class="info-label">任务名称 (Task Name)</div>
      <div class="info-value">${escapeHtml(data.taskName)}</div>
    </div>
    <div class="info-cell">
      <div class="info-label">所属项目 (Project)</div>
      <div class="info-value">${escapeHtml(data.projectSlug)}</div>
    </div>
    <div class="info-cell">
      <div class="info-label">执行策略 (Policy)</div>
      <div class="info-value"><code>${escapeHtml(data.policyName)}</code></div>
    </div>
    <div class="info-cell">
      <div class="info-label">总执行耗时 (Duration)</div>
      <div class="info-value">${escapeHtml(data.durationFormatted)}</div>
    </div>
    <div class="info-cell">
      <div class="info-label">步骤数 / 工具调用 (Steps / Tools)</div>
      <div class="info-value">${data.totalSteps} 步 / ${data.toolCallsCount} 次调用</div>
    </div>
    <div class="info-cell">
      <div class="info-label">开始时间 (Started At)</div>
      <div class="info-value">${data.startedAt ? escapeHtml(data.startedAt) : '—'}</div>
    </div>
    <div class="info-cell">
      <div class="info-label">完成时间 (Completed At)</div>
      <div class="info-value">${data.completedAt ? escapeHtml(data.completedAt) : '—'}</div>
    </div>
    <div class="info-cell">
      <div class="info-label">Token 消耗 (Tokens)</div>
      <div class="info-value">${data.totalTokens > 0 ? `${data.totalTokens.toLocaleString()} (↑${data.inputTokens.toLocaleString()} ↓${data.outputTokens.toLocaleString()})` : '—'}</div>
    </div>
  </div>

  ${
    data.error
      ? `
  <div class="section-title">
    <span>异常 / 错误信息 (Error Details)</span>
  </div>
  <div class="error-card">
    <strong>Error:</strong> ${escapeHtml(data.error)}
  </div>
  `
      : ''
  }

  <div class="section-title">
    <span>任务结论 / 最终结果 (Task Conclusion)</span>
  </div>
  <div class="conclusion-box">
    ${
      data.finalAnswer
        ? `<div class="conclusion-content">${finalAnswerHtml}</div>`
        : `<div class="conclusion-content" style="color:#64748b; font-style:italic;">(无记录的最终回答 / No final answer recorded)</div>`
    }
  </div>

  ${
    data.subtasks && data.subtasks.length > 0
      ? `
  <div class="section-title">
    <span>子任务结论与成果 (Subtask Outcomes & Conclusions)</span>
  </div>
  <div class="subtasks-container">
    ${data.subtasks
      .map(
        st => `
    <div class="subtask-card">
      <div class="subtask-header">
        <div class="subtask-title-group">
          <span class="subtask-id">${escapeHtml(st.subtaskId)}</span>
          <span class="subtask-title">${escapeHtml(st.title)}</span>
        </div>
        <span class="status-badge status-${escapeHtml(String(st.status).toLowerCase())}">${escapeHtml(String(st.status).toUpperCase())}</span>
      </div>
      ${
        st.stepsTaken !== undefined || st.stepCount !== undefined
          ? `<div style="font-size:11px; color:#64748b; margin-bottom:4px;">Execution steps: ${st.stepsTaken ?? st.stepCount}</div>`
          : ''
      }
      ${
        st.summary
          ? `<div class="subtask-conclusion">${markdownToHtml(st.summary)}</div>`
          : ''
      }
    </div>
    `,
      )
      .join('')}
  </div>
  `
      : ''
  }

  <div class="report-footer">
    <div>OpenDuck Agent Harness &bull; Project Execution Report</div>
    <div>Document ID: <code>${escapeHtml(data.taskId)}</code></div>
  </div>
</body>
</html>`
}

/**
 * Sanitizes plain text for standard PDF Type 1 Helvetica font.
 */
function sanitizePdfString(str: string): string {
  if (!str) return ''
  return str.replace(/[\\()]/g, '\\$&').replace(/[^\x20-\x7E\r\n\t]/g, '?')
}

/**
 * Word wraps plain text to fit into line width limits.
 */
function wrapLines(text: string, maxChars = 75): string[] {
  const result: string[] = []
  const paragraphs = text.split(/\r?\n/)
  for (const para of paragraphs) {
    if (!para.trim()) {
      result.push('')
      continue
    }
    const words = para.split(/\s+/)
    let currentLine = ''
    for (const w of words) {
      if ((currentLine + ' ' + w).trim().length <= maxChars) {
        currentLine = (currentLine + ' ' + w).trim()
      } else {
        if (currentLine) result.push(currentLine)
        currentLine = w
      }
    }
    if (currentLine) result.push(currentLine)
  }
  return result
}

/**
 * Generates a valid multi-page PDF 1.4 binary file with basic info and conclusion.
 */
export function generateTaskPdfBinary(data: TaskPdfData): Uint8Array {
  interface PdfLineItem {
    text: string
    bold?: boolean
    size?: number
    leading?: number
  }

  const lines: PdfLineItem[] = []
  lines.push({ text: '==========================================================================', bold: false, size: 8 })
  lines.push({ text: 'OPENDUCK AGENT TASK EXECUTION REPORT', bold: true, size: 16, leading: 22 })
  lines.push({ text: `Exported at: ${data.exportDate || new Date().toISOString()}`, bold: false, size: 9, leading: 14 })
  lines.push({ text: '==========================================================================', bold: false, size: 8, leading: 16 })

  // Basic info section
  lines.push({ text: 'TASK BASIC INFORMATION', bold: true, size: 12, leading: 16 })
  lines.push({ text: `Task ID:           ${data.taskId}`, bold: false, size: 10 })
  if (data.taskName && data.taskName !== data.taskId) {
    lines.push({ text: `Task Name:         ${data.taskName}`, bold: false, size: 10 })
  }
  lines.push({ text: `Project:           ${data.projectSlug || 'default'}`, bold: false, size: 10 })
  lines.push({ text: `Status:            ${data.status.toUpperCase()}`, bold: true, size: 10 })
  lines.push({ text: `Policy:            ${data.policyName || 'agent'}`, bold: false, size: 10 })
  lines.push({ text: `Duration:          ${data.durationFormatted}`, bold: false, size: 10 })
  lines.push({ text: `Total Steps:       ${data.totalSteps}`, bold: false, size: 10 })
  lines.push({ text: `Tool Calls:        ${data.toolCallsCount}`, bold: false, size: 10 })
  if (data.startedAt) {
    lines.push({ text: `Started At:        ${data.startedAt}`, bold: false, size: 10 })
  }
  if (data.completedAt) {
    lines.push({ text: `Completed At:      ${data.completedAt}`, bold: false, size: 10 })
  }
  if (data.totalTokens > 0) {
    lines.push({
      text: `Total Tokens:      ${data.totalTokens.toLocaleString()} (Prompt: ${data.inputTokens.toLocaleString()}, Output: ${data.outputTokens.toLocaleString()})`,
      bold: false,
      size: 10,
    })
  }
  lines.push({ text: '', bold: false, size: 10, leading: 10 })

  // Error section
  if (data.error) {
    lines.push({ text: 'EXECUTION ERROR', bold: true, size: 12, leading: 16 })
    for (const l of wrapLines(data.error, 75)) {
      lines.push({ text: `  ${l}`, bold: false, size: 10 })
    }
    lines.push({ text: '', bold: false, size: 10, leading: 10 })
  }

  // Conclusion section
  lines.push({ text: 'TASK CONCLUSION / FINAL ANSWER', bold: true, size: 12, leading: 16 })
  if (data.finalAnswer && data.finalAnswer.trim()) {
    for (const l of wrapLines(data.finalAnswer, 75)) {
      lines.push({ text: l, bold: false, size: 10 })
    }
  } else {
    lines.push({ text: '(No final answer recorded)', bold: false, size: 10 })
  }
  lines.push({ text: '', bold: false, size: 10, leading: 10 })

  // Subtask outcomes
  if (data.subtasks && data.subtasks.length > 0) {
    lines.push({ text: 'SUBTASK OUTCOMES AND CONCLUSIONS', bold: true, size: 12, leading: 16 })
    for (const st of data.subtasks) {
      lines.push({ text: `[${st.subtaskId}] ${st.title} - Status: ${String(st.status).toUpperCase()}`, bold: true, size: 10 })
      if (st.summary) {
        for (const l of wrapLines(st.summary, 72)) {
          lines.push({ text: `    ${l}`, bold: false, size: 9 })
        }
      }
      lines.push({ text: '', bold: false, size: 8, leading: 6 })
    }
  }

  // PDF binary structure builder
  const objects: string[] = []
  function addObj(content: string): number {
    objects.push(content)
    return objects.length
  }

  // Obj 1: Catalog
  addObj('<< /Type /Catalog /Pages 2 0 R >>')
  // Obj 2: Pages (placeholder)
  const pagesObjIndex = addObj('')
  // Obj 3: Regular Font
  const fontRegularId = addObj('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>')
  // Obj 4: Bold Font
  const fontBoldId = addObj('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>')

  const pageIds: number[] = []
  const linesPerPage = 42
  const totalPages = Math.max(1, Math.ceil(lines.length / linesPerPage))

  for (let p = 0; p < totalPages; p++) {
    const pageLines = lines.slice(p * linesPerPage, (p + 1) * linesPerPage)
    let stream = 'BT\n'
    let y = 780
    for (const item of pageLines) {
      const font = item.bold ? '/F2' : '/F1'
      const size = item.size || 10
      stream += `${font} ${size} Tf\n`
      stream += `50 ${y} Td\n`
      stream += `(${sanitizePdfString(item.text)}) Tj\n`
      stream += 'ET\nBT\n'
      y -= item.leading || 14
    }
    // Footer
    stream += '/F1 9 Tf\n'
    stream += '50 35 Td\n'
    stream += `(OpenDuck Harness - Page ${p + 1} of ${totalPages}) Tj\n`
    stream += 'ET\n'

    const streamBytes = new TextEncoder().encode(stream)
    const streamId = addObj(`<< /Length ${streamBytes.length} >>\nstream\n${stream}endstream`)
    const pageId = addObj(
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595.28 841.89] /Contents ${streamId} 0 R /Resources << /Font << /F1 ${fontRegularId} 0 R /F2 ${fontBoldId} 0 R >> >> >>`,
    )
    pageIds.push(pageId)
  }

  // Update Pages dictionary
  objects[pagesObjIndex - 1] = `<< /Type /Pages /Kids [${pageIds.map(id => `${id} 0 R`).join(' ')}] /Count ${pageIds.length} >>`

  let out = '%PDF-1.4\n%\xe2\xe3\xcf\xd3\n'
  const offsets: number[] = [0]
  const encoder = new TextEncoder()

  for (let i = 0; i < objects.length; i++) {
    offsets.push(encoder.encode(out).length)
    out += `${i + 1} 0 obj\n${objects[i]}\nendobj\n`
  }

  const startxref = encoder.encode(out).length
  out += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`
  for (let i = 1; i <= objects.length; i++) {
    out += `${String(offsets[i]).padStart(10, '0')} 00000 n \n`
  }
  out += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${startxref}\n%%EOF\n`

  return encoder.encode(out)
}

/**
 * Triggers a direct browser download of the PDF binary file.
 */
export function downloadTaskPdfBlob(data: TaskPdfData): void {
  const bytes = generateTaskPdfBinary(data)
  if (typeof document === 'undefined' || typeof window === 'undefined') {
    return
  }
  const blob = new Blob([bytes as BlobPart], { type: 'application/pdf' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  const dateStr = new Date().toISOString().replace(/[:.]/g, '-')
  a.href = url
  a.download = `task-${data.taskId}-report-${dateStr}.pdf`
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  URL.revokeObjectURL(url)
}

/**
 * Opens the styled HTML report in a new tab for preview and printing.
 */
export function previewTaskPdfHtml(data: TaskPdfData): void {
  if (typeof window === 'undefined') {
    return
  }
  const html = generateTaskPdfHtml(data)
  const blob = new Blob([html], { type: 'text/html;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  window.open(url, '_blank')
}

/**
 * Prints the styled task PDF document using an isolated iframe.
 */
export function printTaskPdf(data: TaskPdfData): void {
  if (typeof document === 'undefined') {
    downloadTaskPdfBlob(data)
    return
  }

  const html = generateTaskPdfHtml(data)
  const iframe = document.createElement('iframe')
  iframe.style.position = 'fixed'
  iframe.style.right = '0'
  iframe.style.bottom = '0'
  iframe.style.width = '0'
  iframe.style.height = '0'
  iframe.style.border = '0'
  iframe.setAttribute('aria-hidden', 'true')
  document.body.appendChild(iframe)

  const doc = iframe.contentWindow?.document
  if (doc) {
    doc.open()
    doc.write(html)
    doc.close()

    setTimeout(() => {
      try {
        iframe.contentWindow?.focus()
        iframe.contentWindow?.print()
      } catch {
        // Fallback to direct PDF download
        downloadTaskPdfBlob(data)
      } finally {
        setTimeout(() => {
          if (iframe.parentNode) {
            iframe.parentNode.removeChild(iframe)
          }
        }, 1500)
      }
    }, 250)
    return
  }

  downloadTaskPdfBlob(data)
}

/**
 * Unified entry point to export task basic info and conclusion as PDF.
 * Supports:
 * - 'print' (default): opens browser Print Preview dialog (Save as PDF) with complete styles & formatting
 * - 'download': generates and downloads a direct .pdf file
 * - 'preview': opens the full styled report in a new tab
 */
export function exportTaskPdf(
  run: HarnessRunResponse,
  options?: ExportPdfOptions,
): void {
  const data = buildTaskPdfData(run, options)
  const method = options?.method ?? 'print'

  if (method === 'download') {
    downloadTaskPdfBlob(data)
  } else if (method === 'preview') {
    previewTaskPdfHtml(data)
  } else {
    printTaskPdf(data)
  }
}
