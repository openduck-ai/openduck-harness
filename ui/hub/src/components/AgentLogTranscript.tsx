import { useEffect, useRef, useState, type RefObject } from 'react'
import type { createApi } from '@aaif/goose-hub-core'
import {
  describeToolCall,
  formatPromptMessageText,
  presentToolCall,
  type AgentLogToolCall,
  type AgentLogTurn,
  type HarnessJudgmentRecord,
} from '../taskTrajectory.ts'
import { gitDiffLineClass } from '../gitDiff.ts'
import { getFileIcon } from '../pathUtils.ts'
import { MarkdownPreview } from './MarkdownPreview.tsx'
import {
  DynamicPromptRunNotice,
  DynamicPromptSelectionMenu,
  type DynamicPromptTaskOption,
  type SelectionRunNotice,
} from './DynamicPromptSelectionMenu.tsx'
import {
  AlertCircle,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Loader2,
} from 'lucide-react'

const CLAMP_LINES = 12
const CONTINUE_TURN_CHOICES = [10, 25, 50] as const

export interface AgentLogContinueRequest {
  busy: boolean
  summary?: string
  onContinue: (extraTurns: number, extraPrompt: string) => Promise<void> | void
  onSpecifiedTurns?: () => void
}

interface AgentLogTranscriptProps {
  turns: AgentLogTurn[]
  live: boolean
  status: string
  sessionKey: string
  copiedId: string | null
  onCopyText: (id: string, text: string) => void
  onOpenFile: (path: string) => void
  transcriptRef: RefObject<HTMLDivElement | null>
  endRef: RefObject<HTMLDivElement | null>
  onScroll: () => void
  showLatest: boolean
  onLatest: () => void
  mentionedFiles: string[]
  resolveImageUrl?: (src: string) => string
  dynamicTasks: DynamicPromptTaskOption[]
  selectionNotice: SelectionRunNotice | null
  selectionBusy: boolean
  selectionEnabled: boolean
  api?: ReturnType<typeof createApi> | null
  projectSlug?: string | null
  onSelectDynamicTask: (task: DynamicPromptTaskOption, text: string) => void
  continueRequest?: AgentLogContinueRequest
}

function isUnifiedDiff(text: string): boolean {
  return text.includes('diff --git') || /^@@\s/m.test(text)
}

function isToolResultMessage(message: { role: string; toolResults?: unknown[] }): boolean {
  return message.role === 'tool' || Boolean(message.toolResults && message.toolResults.length > 0)
}

function thoughtLineCount(thinking: string): number {
  const count = thinking.split('\n').filter(line => line.trim().length > 0).length
  return Math.max(1, count)
}

function firstErrorLine(output: string | undefined): string | undefined {
  if (!output) return undefined
  const line = output.split('\n').find(item => item.trim().length > 0)?.trim()
  if (!line) return undefined
  return line.length > 140 ? `${line.slice(0, 139)}…` : line
}

function statusWord(status: string): string {
  const raw = status.trim()
  if (!raw) return 'Done'
  return raw.charAt(0).toUpperCase() + raw.slice(1).toLowerCase()
}

function liveLineText(turns: AgentLogTurn[], live: boolean, status: string): string {
  if (!live) return statusWord(status)
  for (let index = turns.length - 1; index >= 0; index--) {
    const turn = turns[index]
    if (turn.role !== 'assistant') continue
    const newest = turn.toolCalls?.[turn.toolCalls.length - 1]
    if (newest) {
      const summary = describeToolCall(newest.name, newest.arguments)
      if (summary) return summary
    }
    return 'Thinking…'
  }
  return 'Thinking…'
}

function splitSummary(summary: string, path?: string): { before: string; path?: string; after: string } {
  if (!path) return { before: summary, after: '' }
  const at = summary.indexOf(path)
  if (at < 0) return { before: summary, after: '' }
  return {
    before: summary.slice(0, at),
    path,
    after: summary.slice(at + path.length),
  }
}

export function AgentLogTranscript({
  turns,
  live,
  status,
  sessionKey,
  copiedId,
  onCopyText,
  onOpenFile,
  transcriptRef,
  endRef,
  onScroll,
  showLatest,
  onLatest,
  mentionedFiles,
  resolveImageUrl,
  dynamicTasks,
  selectionNotice,
  selectionBusy,
  selectionEnabled,
  api,
  projectSlug,
  onSelectDynamicTask,
  continueRequest,
}: AgentLogTranscriptProps) {
  const finalProseRef = useRef<HTMLDivElement>(null)
  const [thinkingOpen, setThinkingOpen] = useState<Record<string, boolean>>({})
  const [detailsOpen, setDetailsOpen] = useState<Record<string, boolean>>({})
  const [toolsOpen, setToolsOpen] = useState<Record<string, boolean>>({})

  let lastFinalId: string | undefined
  let lastAssistantIndex = -1
  turns.forEach((turn, index) => {
    if (turn.kind === 'final_answer') lastFinalId = turn.id
    if (turn.role === 'assistant') lastAssistantIndex = index
  })

  return (
    <div className="agent-log-panel">
      <div className="agent-transcript-wrap">
        <div className="agent-transcript" ref={transcriptRef} onScroll={onScroll}>
          {turns.length === 0 ? (
            <div className="empty-card">No log entries match filter / search criteria.</div>
          ) : (
            turns.map((turn, index) =>
              turn.role === 'user' ? (
                <UserTurn
                  key={turn.id}
                  turn={turn}
                  onOpenFile={onOpenFile}
                  resolveImageUrl={resolveImageUrl}
                />
              ) : (
                <AssistantTurn
                  key={turn.id}
                  turn={turn}
                  isLiveStep={live && index === lastAssistantIndex}
                  isFinal={turn.id === lastFinalId}
                  thinkingOpen={turn.id in thinkingOpen ? thinkingOpen[turn.id] : live && index === lastAssistantIndex}
                  onToggleThinking={() =>
                    setThinkingOpen(prev => ({
                      ...prev,
                      [turn.id]: !(turn.id in prev ? prev[turn.id] : live && index === lastAssistantIndex),
                    }))
                  }
                  detailsOpen={detailsOpen[turn.id] ?? false}
                  onToggleDetails={() =>
                    setDetailsOpen(prev => ({ ...prev, [turn.id]: !prev[turn.id] }))
                  }
                  toolsOpen={toolsOpen}
                  onToggleTool={(toolKey, failed) =>
                    setToolsOpen(prev => ({
                      ...prev,
                      [toolKey]: !(toolKey in prev ? prev[toolKey] : failed),
                    }))
                  }
                  copiedId={copiedId}
                  onCopyText={onCopyText}
                  onOpenFile={onOpenFile}
                  resolveImageUrl={resolveImageUrl}
                  mentionedFiles={mentionedFiles}
                  finalProseRef={finalProseRef}
                  dynamicTasks={dynamicTasks}
                  selectionNotice={selectionNotice}
                  selectionBusy={selectionBusy}
                  selectionEnabled={selectionEnabled}
                  api={api}
                  projectSlug={projectSlug}
                  onSelectDynamicTask={onSelectDynamicTask}
                />
              ),
            )
          )}
          <div ref={endRef} />
        </div>
        {showLatest && (
          <button type="button" className="agent-latest" onClick={onLatest}>
            ↓ Latest
          </button>
        )}
      </div>
      <AgentLogDock
        turns={turns}
        live={live}
        status={status}
        sessionKey={sessionKey}
        continueRequest={continueRequest}
      />
    </div>
  )
}

function UserTurn({
  turn,
  onOpenFile,
  resolveImageUrl,
}: {
  turn: AgentLogTurn
  onOpenFile: (path: string) => void
  resolveImageUrl?: (src: string) => string
}) {
  if (!turn.text) return null
  return (
    <section className="agent-user">
      <div className="agent-user-label">You</div>
      <MarkdownPreview content={turn.text} onOpenFile={onOpenFile} resolveImageUrl={resolveImageUrl} />
    </section>
  )
}

function AssistantTurn({
  turn,
  isLiveStep,
  isFinal,
  thinkingOpen,
  onToggleThinking,
  detailsOpen,
  onToggleDetails,
  toolsOpen,
  onToggleTool,
  copiedId,
  onCopyText,
  onOpenFile,
  resolveImageUrl,
  mentionedFiles,
  finalProseRef,
  dynamicTasks,
  selectionNotice,
  selectionBusy,
  selectionEnabled,
  api,
  projectSlug,
  onSelectDynamicTask,
}: {
  turn: AgentLogTurn
  isLiveStep: boolean
  isFinal: boolean
  thinkingOpen: boolean
  onToggleThinking: () => void
  detailsOpen: boolean
  onToggleDetails: () => void
  toolsOpen: Record<string, boolean>
  onToggleTool: (toolKey: string, failed: boolean) => void
  copiedId: string | null
  onCopyText: (id: string, text: string) => void
  onOpenFile: (path: string) => void
  resolveImageUrl?: (src: string) => string
  mentionedFiles: string[]
  finalProseRef: RefObject<HTMLDivElement | null>
  dynamicTasks: DynamicPromptTaskOption[]
  selectionNotice: SelectionRunNotice | null
  selectionBusy: boolean
  selectionEnabled: boolean
  api?: ReturnType<typeof createApi> | null
  projectSlug?: string | null
  onSelectDynamicTask: (task: DynamicPromptTaskOption, text: string) => void
}) {
  const detailMessages = (turn.promptMessages ?? []).filter(message => !isToolResultMessage(message))
  const triggered = (turn.judgments ?? []).filter(judgment => judgment.verdict?.triggered)
  const hasDetails =
    detailMessages.length > 0 ||
    (turn.judgments?.length ?? 0) > 0 ||
    (turn.providerCalls?.length ?? 0) > 0
  const thoughtLines = turn.thinking ? thoughtLineCount(turn.thinking) : 0

  return (
    <section className={`agent-step${isLiveStep ? ' is-live' : ''}`} data-log-step={turn.stepIndex}>
      {turn.thinking && (
        <div className={`agent-thinking${thinkingOpen ? ' open' : ''}`}>
          <button type="button" className="agent-thinking-toggle" onClick={onToggleThinking} aria-expanded={thinkingOpen}>
            <span>
              Thought · {thoughtLines} {thoughtLines === 1 ? 'line' : 'lines'}
            </span>
            {thinkingOpen ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          </button>
          {thinkingOpen && <div className="agent-thinking-body">{turn.thinking}</div>}
        </div>
      )}

      {turn.text && (
        <div className="agent-final-block" ref={isFinal ? finalProseRef : undefined}>
          <div className={`agent-prose${isFinal ? ' is-final' : ''}${turn.kind === 'error' ? ' is-error' : ''}`}>
            <MarkdownPreview content={turn.text} onOpenFile={onOpenFile} resolveImageUrl={resolveImageUrl} />
          </div>
          {isFinal && mentionedFiles.length > 0 && (
            <div className="final-answer-mentioned-files">
              <div className="mentioned-files-label">
                <span>Mentioned files ({mentionedFiles.length})</span>
              </div>
              <div className="mentioned-files-chips">
                {mentionedFiles.map(file => {
                  const fileName = file.split('/').pop() || file
                  return (
                    <button
                      key={file}
                      type="button"
                      className="mentioned-file-chip"
                      onClick={() => onOpenFile(file)}
                      title={`Preview ${file}`}
                    >
                      <span className="file-chip-icon">{getFileIcon(file)}</span>
                      <span className="file-chip-name">{fileName}</span>
                    </button>
                  )
                })}
              </div>
            </div>
          )}
          {isFinal && <DynamicPromptRunNotice notice={selectionNotice} />}
          {isFinal && (
            <DynamicPromptSelectionMenu
              containerRef={finalProseRef}
              tasks={dynamicTasks}
              enabled={selectionEnabled}
              busy={selectionBusy}
              api={api}
              projectSlug={projectSlug}
              onSelect={onSelectDynamicTask}
            />
          )}
        </div>
      )}

      {turn.toolCalls && turn.toolCalls.length > 0 && (
        <div className="agent-tool-list">
          {turn.toolCalls.map((tool, toolIndex) => {
            const toolKey = `${turn.id}-${toolIndex}`
            const failed = Boolean(tool.isError)
            const open = toolKey in toolsOpen ? toolsOpen[toolKey] : failed
            return (
              <ToolRow
                key={tool.id ?? toolKey}
                tool={tool}
                open={open}
                onToggle={() => onToggleTool(toolKey, failed)}
                copiedId={copiedId}
                onCopyText={onCopyText}
                onOpenFile={onOpenFile}
                copyPrefix={toolKey}
              />
            )
          })}
        </div>
      )}

      {hasDetails && (
        <div className={`agent-details${detailsOpen ? ' open' : ''}`}>
          <button type="button" className="agent-details-toggle" onClick={onToggleDetails} aria-expanded={detailsOpen}>
            <span>Details</span>
            {triggered.map(judgment => (
              <span key={judgment.point} className="agent-verdict-chip" title={judgment.point}>
                <span className="agent-amber-dot" />
                {judgment.point}
              </span>
            ))}
            {detailsOpen ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          </button>
          {detailsOpen && (
            <div className="agent-details-body">
              {detailMessages.map((message, messageIndex) => (
                <div key={message.index ?? messageIndex} className={`agent-log-prompt-msg role-${message.role}`}>
                  <div className="agent-log-prompt-msg-role">{message.role}</div>
                  <pre className="agent-log-prompt-msg-text">{formatPromptMessageText(message)}</pre>
                </div>
              ))}
              {turn.judgments?.map((judgment, index) => (
                <AgentLogJudgmentDetailCard key={`${judgment.point}-${index}`} judgment={judgment} />
              ))}
              {turn.providerCalls && turn.providerCalls.length > 0 && (
                <ul className="agent-provider-list">
                  {turn.providerCalls.map((call, index) => (
                    <li key={`${call.provider}-${call.model}-${index}`}>
                      <span>{call.provider}</span>
                      {call.model && <span>{call.model}</span>}
                      <span>{call.latencyMs.toLocaleString()} ms</span>
                      <span>{call.success ? 'ok' : call.error || 'failed'}</span>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </div>
      )}
    </section>
  )
}

function ToolRow({
  tool,
  open,
  onToggle,
  copiedId,
  onCopyText,
  onOpenFile,
  copyPrefix,
}: {
  tool: AgentLogToolCall
  open: boolean
  onToggle: () => void
  copiedId: string | null
  onCopyText: (id: string, text: string) => void
  onOpenFile: (path: string) => void
  copyPrefix: string
}) {
  const presented = presentToolCall(tool)
  const failed = Boolean(tool.isError)
  const pending = tool.output === undefined && !failed
  const summary = splitSummary(presented.summary, presented.path)
  const summaryPath = summary.path
  const errorLine = failed ? firstErrorLine(tool.output) : undefined
  const argsAreDiff = Boolean(presented.argsText && isUnifiedDiff(presented.argsText))
  const outputIsDiff = Boolean(presented.outputText && isUnifiedDiff(presented.outputText))
  const outputSameAsArgs = presented.outputText !== undefined && presented.outputText === presented.argsText

  return (
    <div className={`agent-tool-row${open ? ' open' : ''}${failed ? ' is-error' : ''}`}>
      <button
        type="button"
        className="agent-tool-toggle"
        onClick={onToggle}
        aria-expanded={open}
        title={presented.summary}
      >
        {failed && <AlertCircle size={14} className="agent-tool-icon is-error" />}
        {pending && <Loader2 size={14} className="agent-tool-icon spinning" />}
        {!failed && !pending && <Check size={14} className="agent-tool-icon is-ok" />}
        <span className="agent-tool-summary">
          {summary.before}
          {summaryPath && (
            <span
              className="agent-tool-path"
              role="link"
              tabIndex={0}
              onClick={event => {
                event.stopPropagation()
                onOpenFile(summaryPath)
              }}
              onKeyDown={event => {
                if (event.key !== 'Enter' && event.key !== ' ') return
                event.preventDefault()
                event.stopPropagation()
                onOpenFile(summaryPath)
              }}
            >
              {summaryPath}
            </span>
          )}
          {summary.after}
        </span>
        {errorLine && <span className="agent-tool-error">{errorLine}</span>}
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
      </button>
      {open && (
        <div className="agent-tool-body">
          <div className="agent-tool-raw-name">{tool.name}</div>
          {presented.argsText && (
            <ClampedBlock
              label="Arguments"
              text={presented.argsText}
              diff={argsAreDiff}
              copyId={`${copyPrefix}-args`}
              copiedId={copiedId}
              onCopy={onCopyText}
            />
          )}
          {presented.outputText && !outputSameAsArgs && (
            <ClampedBlock
              label={failed ? 'Error output' : outputIsDiff ? 'Diff' : 'Output'}
              text={presented.outputText}
              diff={outputIsDiff}
              copyId={`${copyPrefix}-out`}
              copiedId={copiedId}
              onCopy={onCopyText}
            />
          )}
        </div>
      )}
    </div>
  )
}

function ClampedBlock({
  label,
  text,
  diff = false,
  copyId,
  copiedId,
  onCopy,
}: {
  label: string
  text: string
  diff?: boolean
  copyId: string
  copiedId: string | null
  onCopy: (id: string, text: string) => void
}) {
  const [expanded, setExpanded] = useState(false)
  const lines = text.split('\n')
  const clamped = !expanded && lines.length > CLAMP_LINES
  const shown = clamped ? lines.slice(0, CLAMP_LINES).join('\n') : text

  return (
    <div className="agent-tool-section">
      <div className="agent-tool-section-head">
        <span>{label}</span>
        <button type="button" className="mini-copy-btn" onClick={() => onCopy(copyId, text)} title={`Copy ${label.toLowerCase()}`}>
          {copiedId === copyId ? <Check size={11} /> : <Copy size={11} />}
        </button>
      </div>
      {diff ? (
        <pre className="agent-tool-pre agent-tool-diff">
          {shown.split('\n').map((line, index) => (
            <span key={index} className={`git-diff-line ${gitDiffLineClass(line)}`}>
              {line}
            </span>
          ))}
        </pre>
      ) : (
        <pre className="agent-tool-pre">{shown}</pre>
      )}
      {lines.length > CLAMP_LINES && (
        <button type="button" className="agent-show-more" onClick={() => setExpanded(value => !value)}>
          {expanded ? 'Show less' : 'Show more'}
        </button>
      )}
    </div>
  )
}

function AgentLogDock({
  turns,
  live,
  status,
  sessionKey,
  continueRequest,
}: {
  turns: AgentLogTurn[]
  live: boolean
  status: string
  sessionKey: string
  continueRequest?: AgentLogContinueRequest
}) {
  const [draft, setDraft] = useState('')
  const [extraTurns, setExtraTurns] = useState<number>(25)
  const [noteOpen, setNoteOpen] = useState(true)

  useEffect(() => {
    setDraft('')
    setNoteOpen(true)
  }, [sessionKey])

  if (continueRequest && !live) {
    const summary = continueRequest.summary?.trim()
    const send = () => {
      const text = draft.trim()
      if (!text || continueRequest.busy) return
      void continueRequest.onContinue(extraTurns, text)
    }
    return (
      <div className="agent-log-dock">
        {summary && noteOpen && (
          <div className="agent-continue-note">
            <div className="agent-continue-note-head">
              <p>This run stopped before completion. A follow-up starts a new run from the saved summary.</p>
              <button type="button" className="agent-note-dismiss" onClick={() => setNoteOpen(false)}>
                Dismiss
              </button>
            </div>
            <div className="continuation-summary-panel">
              <MarkdownPreview content={summary} />
            </div>
          </div>
        )}
        <form
          className="agent-composer"
          onSubmit={event => {
            event.preventDefault()
            send()
          }}
        >
          <textarea
            value={draft}
            onChange={event => setDraft(event.target.value)}
            placeholder="Send a follow-up"
            rows={2}
            disabled={continueRequest.busy}
            onKeyDown={event => {
              if (event.key !== 'Enter' || event.shiftKey || event.nativeEvent.isComposing) return
              event.preventDefault()
              send()
            }}
          />
          <div className="agent-composer-actions">
            <div className="agent-turn-choices" role="group" aria-label="Extra turns">
              {CONTINUE_TURN_CHOICES.map(choice => (
                <button
                  key={choice}
                  type="button"
                  className={choice === extraTurns ? 'active' : ''}
                  onClick={() => setExtraTurns(choice)}
                  title={`Continue with +${choice} turns`}
                >
                  {choice}
                </button>
              ))}
            </div>
            {continueRequest.onSpecifiedTurns && (
              <button
                type="button"
                className="agent-specified-btn"
                onClick={continueRequest.onSpecifiedTurns}
                disabled={continueRequest.busy}
              >
                Specified turns
              </button>
            )}
            <button type="submit" className="agent-continue-btn" disabled={continueRequest.busy || draft.trim().length === 0}>
              Continue
            </button>
          </div>
        </form>
      </div>
    )
  }

  return (
    <div className="agent-log-dock">
      <div className="agent-live-line" role="status">
        {live && <span className="stream-dot" />}
        <span>{liveLineText(turns, live, status)}</span>
      </div>
    </div>
  )
}

function AgentLogJudgmentDetailCard({ judgment }: { judgment: HarnessJudgmentRecord }) {
  const triggered = Boolean(judgment.verdict?.triggered)
  const verdictKind = judgment.verdict?.kind || 'unknown'
  const scoreVal = judgment.verdict?.value !== undefined ? judgment.verdict.value.toFixed(4) : undefined
  const thresholdVal =
    judgment.verdict?.threshold !== undefined ? judgment.verdict.threshold.toFixed(2) : undefined
  const confPercent =
    judgment.confidence !== undefined ? `${(judgment.confidence * 100).toFixed(1)}%` : undefined

  return (
    <div className={`agent-log-judgment-card ${triggered ? 'is-triggered' : 'is-ok'}`}>
      <div className="judgment-card-header">
        <div className="judgment-card-point">
          <span className="judgment-point-name">{judgment.point}</span>
          <span className={`judgment-mode-badge mode-${judgment.mode}`}>{judgment.mode}</span>
          <span className="judgment-latency-badge">{judgment.latencyMs} ms</span>
        </div>
        <span className={`judgment-verdict-badge ${triggered ? 'triggered' : 'ok'}`}>
          {triggered ? 'Triggered' : 'Passed'}
        </span>
      </div>
      <div className="judgment-card-metrics">
        <div className="judgment-metric">
          <span className="metric-label">Type:</span>
          <span className="metric-val">{judgment.questionType || verdictKind}</span>
        </div>
        {scoreVal !== undefined && (
          <div className="judgment-metric">
            <span className="metric-label">Score / Prob:</span>
            <span className="metric-val font-mono">{scoreVal}</span>
          </div>
        )}
        {thresholdVal !== undefined && (
          <div className="judgment-metric">
            <span className="metric-label">Threshold:</span>
            <span className="metric-val font-mono">{thresholdVal}</span>
          </div>
        )}
        {confPercent && (
          <div className="judgment-metric">
            <span className="metric-label">Confidence:</span>
            <span className="metric-val font-mono">{confPercent}</span>
          </div>
        )}
        {judgment.verdict?.selected && (
          <div className="judgment-metric">
            <span className="metric-label">Choice:</span>
            <span className="metric-val font-mono">{judgment.verdict.selected}</span>
          </div>
        )}
      </div>
      {judgment.stateDigest && (
        <div className="judgment-card-digest">
          <span className="digest-label">State Digest:</span>
          <pre className="digest-pre">{judgment.stateDigest}</pre>
        </div>
      )}
    </div>
  )
}
