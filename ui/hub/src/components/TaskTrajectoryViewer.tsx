import { useState, useMemo, useEffect, useRef, useCallback, Suspense } from 'react'
import type {
  HarnessRunResponse,
  HarnessActiveJob,
  createApi,
} from '@aaif/goose-hub-core'
import {
  parseAgentAction,
  extractFinalAnswer,
  formatToolArguments,
  describeToolCall,
  extractLlmTelemetry,
  filterTrajectorySteps,
  calculateTrajectoryMetrics,
  downloadTrajectoryJson,
  buildAgentLog,
  filterAgentLogTurns,
  parseToolResults,
  type TrajectoryFilterType,
  type TrajectoryViewMode,
  type AgentLogTurn,
  type AgentLogToolCall,
  type HarnessJudgmentRecord,
  evaluateAutoScrollState,
  formatPromptMessageText,
} from '../taskTrajectory.ts'
import { isContinuableStatus } from '../taskContinuation.ts'
import { MarkdownPreview } from './MarkdownPreview.tsx'
import { FilePreviewModal } from './FilePreviewModal.tsx'
import { formatSelectionExtraPrompt } from '../dynamicPrompt.ts'
import {
  DynamicPromptRunNotice,
  DynamicPromptSelectionMenu,
  useDynamicPromptTasks,
  useSelectionTaskRun,
} from './DynamicPromptSelectionMenu.tsx'
import {
  extractMentionedFiles,
  getFileIcon,
  resolveProjectAssetPath,
} from '../pathUtils.ts'
import {
  Search,
  SlidersHorizontal,
  ChevronLeft,
  ChevronRight,
  Download,
  Copy,
  Check,
  Zap,
  Clock,
  Bot,
  Brain,
  MessageSquare,
  MessagesSquare,
  FileCode,
  Sparkles,
  Filter,
  Layers,
  User,
  Wrench,
  CheckCircle2,
  AlertCircle,
  ChevronDown,
  ChevronRight as ChevronRightIcon,
  Terminal,
  ArrowDownToLine,
  Scale,
} from 'lucide-react'

interface TaskTrajectoryViewerProps {
  run: HarnessRunResponse
  api?: ReturnType<typeof createApi>
  projectSlug?: string
  onNavigateToFiles?: (path: string) => void
  liveInspect?: boolean
  inspectingJobId?: string | null
  selectedHistoryId?: string | null
  runningTaskId?: string | null
  stoppingTaskId?: string | null
  activeJobs?: HarnessActiveJob[]
  onStopTask?: (taskId: string) => Promise<void>
  onStopJob?: (job: HarnessActiveJob) => Promise<void>
  onBackToTasks: () => void
  onContinueTask?: (taskId: string, extraTurns: number) => Promise<void>
  onOpenContinueModal?: (
    taskId: string,
    previousSteps: number,
    taskName?: string,
    runId?: string,
    progressSummary?: string,
  ) => void
}

type TelemetryTab = 'prompt' | 'response' | 'thinking' | 'tokens' | 'raw'
type MsgRoleFilter = 'all' | 'user' | 'assistant' | 'tool' | 'system'

export function TaskTrajectoryViewer({
  run,
  api,
  projectSlug,
  onNavigateToFiles,
  liveInspect = false,
  inspectingJobId,
  selectedHistoryId,
  runningTaskId,
  stoppingTaskId,
  activeJobs = [],
  onStopTask,
  onStopJob,
  onBackToTasks,
  onContinueTask,
  onOpenContinueModal,
}: TaskTrajectoryViewerProps) {
  const effectiveProjectSlug = projectSlug || run.projectSlug || undefined
  const finalAnswerRef = useRef<HTMLDivElement>(null)
  const dynamicTasks = useDynamicPromptTasks(api, effectiveProjectSlug)
  const {
    notice: selectionRunNotice,
    running: selectionRunBusy,
    runWithSelection,
  } = useSelectionTaskRun(api, effectiveProjectSlug)
  const [previewFilePath, setPreviewFilePath] = useState<string | null>(null)
  // Navigation & View state
  const [viewMode, setViewMode] = useState<TrajectoryViewMode>('log')
  const [selectedStepIdx, setSelectedStepIdx] = useState<number>(0)
  const [filterType, setFilterType] = useState<TrajectoryFilterType>('all')
  const [searchQuery, setSearchQuery] = useState('')

  // LLM Telemetry display state
  const [showAllLlmTelemetry, setShowAllLlmTelemetry] = useState(false)
  const [activeTelemetryTab, setActiveTelemetryTab] = useState<Record<number, TelemetryTab>>({})
  const [msgRoleFilter, setMsgRoleFilter] = useState<Record<number, MsgRoleFilter>>({})
  const [expandedMessages, setExpandedMessages] = useState<Record<number, Record<number, boolean>>>({})
  const [allMessagesExpanded, setAllMessagesExpanded] = useState<Record<number, boolean>>({})
  const [expandedStepRaw, setExpandedStepRaw] = useState<Record<number, boolean>>({})
  const [promptRenderMode, setPromptRenderMode] = useState<Record<number, 'markdown' | 'monospace'>>({})

  // Final Answer & System Prompt Inspector state
  const [showPromptInspector, setShowPromptInspector] = useState(false)
  const [promptInspectorMode, setPromptInspectorMode] = useState<'markdown' | 'raw'>('markdown')
  const [finalAnswerMode, setFinalAnswerMode] = useState<'markdown' | 'raw'>('markdown')
  const [copiedFinalAnswer, setCopiedFinalAnswer] = useState(false)
  const [copiedSystemPrompt, setCopiedSystemPrompt] = useState(false)
  const [copiedPromptStep, setCopiedPromptStep] = useState<number | null>(null)
  const [copiedResponseStep, setCopiedResponseStep] = useState<number | null>(null)
  const [copiedMsgIdx, setCopiedMsgIdx] = useState<string | null>(null)

  // Turn continuation custom count
  const [bannerCustomTurns, setBannerCustomTurns] = useState(25)

  const timelineContainerRef = useRef<HTMLDivElement>(null)
  const scrubberRailRef = useRef<HTMLDivElement>(null)
  const agentLogEndRef = useRef<HTMLDivElement>(null)
  const agentLogTranscriptRef = useRef<HTMLDivElement>(null)
  const isProgrammaticScrollRef = useRef(false)
  const programmaticScrollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const userExplicitlyDisabledRef = useRef(false)
  const [autoScroll, setAutoScroll] = useState(true)
  const [expandedPrompts, setExpandedPrompts] = useState<Record<string, boolean>>({})
  const [allPromptsExpanded, setAllPromptsExpanded] = useState(false)
  const [allThinkingExpanded, setAllThinkingExpanded] = useState(false)
  const [expandedThinking, setExpandedThinking] = useState<Record<string, boolean>>({})
  const [expandedLogTools, setExpandedLogTools] = useState<Record<string, boolean>>({})
  const [allJudgmentsExpanded, setAllJudgmentsExpanded] = useState(false)
  const [expandedJudgments, setExpandedJudgments] = useState<Record<string, boolean>>({})

  const steps = useMemo(() => run.trajectory?.steps ?? [], [run.trajectory?.steps])
  const metrics = useMemo(() => calculateTrajectoryMetrics(run), [run])
  const agentLog = useMemo(() => buildAgentLog(run), [run])
  const filteredLogTurns = useMemo(
    () => filterAgentLogTurns(agentLog, filterType, searchQuery),
    [agentLog, filterType, searchQuery],
  )

  // Filtered steps
  const filteredSteps = useMemo(
    () => filterTrajectorySteps(steps, filterType, searchQuery),
    [steps, filterType, searchQuery],
  )

  // Ensure selectedStepIdx is within bounds
  useEffect(() => {
    if (selectedStepIdx >= steps.length && steps.length > 0) {
      setSelectedStepIdx(steps.length - 1)
    }
  }, [steps.length, selectedStepIdx])

  // Default to the last step if liveInspect is active or new steps arrive
  useEffect(() => {
    if (liveInspect && steps.length > 0) {
      setSelectedStepIdx(steps.length - 1)
    }
  }, [liveInspect, steps.length])

  // Clean up programmatic scroll timer on unmount
  useEffect(() => {
    return () => {
      if (programmaticScrollTimerRef.current) {
        clearTimeout(programmaticScrollTimerRef.current)
      }
    }
  }, [])

  // Smoothly scroll the agent log container to the bottom
  const scrollToLogBottom = useCallback((behavior: ScrollBehavior = 'smooth') => {
    isProgrammaticScrollRef.current = true
    if (programmaticScrollTimerRef.current) {
      clearTimeout(programmaticScrollTimerRef.current)
    }

    if (agentLogTranscriptRef.current) {
      agentLogTranscriptRef.current.scrollTo({
        top: agentLogTranscriptRef.current.scrollHeight,
        behavior,
      })
    } else {
      agentLogEndRef.current?.scrollIntoView({ behavior, block: 'end' })
    }

    programmaticScrollTimerRef.current = setTimeout(() => {
      isProgrammaticScrollRef.current = false
    }, 600)
  }, [])

  // Auto-scroll when new steps or log turns arrive while live inspecting
  useEffect(() => {
    if (liveInspect && viewMode === 'log' && autoScroll) {
      scrollToLogBottom('smooth')
    }
  }, [liveInspect, viewMode, autoScroll, filteredLogTurns.length, steps.length, scrollToLogBottom])

  // Reset auto-scroll state when inspecting a new job or task
  useEffect(() => {
    if (liveInspect) {
      userExplicitlyDisabledRef.current = false
      setAutoScroll(true)
    }
  }, [liveInspect, inspectingJobId, run.taskId])

  // Detect user scroll in the log transcript container
  const handleLogScroll = useCallback(() => {
    const el = agentLogTranscriptRef.current
    if (!el || isProgrammaticScrollRef.current) return

    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight
    const evaluation = evaluateAutoScrollState({
      distanceFromBottom,
      currentAutoScroll: autoScroll,
      userExplicitlyDisabled: userExplicitlyDisabledRef.current,
    })

    if (evaluation.autoScroll !== autoScroll) {
      setAutoScroll(evaluation.autoScroll)
    }
  }, [autoScroll])

  // Explicit toggle button click handler
  const handleToggleAutoScroll = useCallback(() => {
    setAutoScroll(prev => {
      const next = !prev
      userExplicitlyDisabledRef.current = !next
      if (next) {
        setTimeout(() => {
          scrollToLogBottom('smooth')
        }, 50)
      }
      return next
    })
  }, [scrollToLogBottom])

  // Floating resume button click handler
  const handleResumeAutoScroll = useCallback(() => {
    userExplicitlyDisabledRef.current = false
    setAutoScroll(true)
    scrollToLogBottom('smooth')
  }, [scrollToLogBottom])

  // Scroll active pill into view in the scrubber rail
  useEffect(() => {
    if (scrubberRailRef.current) {
      const activePill = scrubberRailRef.current.querySelector(
        `[data-step-index="${selectedStepIdx}"]`,
      ) as HTMLElement | null
      if (activePill) {
        activePill.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'center' })
      }
    }
  }, [selectedStepIdx])

  // Keyboard navigation shortcuts (j/k or ArrowLeft/ArrowRight/ArrowUp/ArrowDown)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ignore if user is typing in an input or textarea
      const target = e.target as HTMLElement | null
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) {
        return
      }

      if (e.key === 'j' || e.key === 'ArrowDown' || e.key === 'ArrowRight') {
        e.preventDefault()
        setSelectedStepIdx(prev => Math.min(steps.length - 1, prev + 1))
      } else if (e.key === 'k' || e.key === 'ArrowUp' || e.key === 'ArrowLeft') {
        e.preventDefault()
        setSelectedStepIdx(prev => Math.max(0, prev - 1))
      } else if (e.key === 'f') {
        // Jump to final answer if present
        const finalIdx = steps.findIndex(s => parseAgentAction(s.agentAction).kind === 'final_answer')
        if (finalIdx !== -1) {
          e.preventDefault()
          setSelectedStepIdx(finalIdx)
        }
      } else if (e.key === 'Escape') {
        if (searchQuery) {
          e.preventDefault()
          setSearchQuery('')
        }
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [steps, searchQuery])

  // Copy handlers
  const handleCopyFinalAnswer = (text: string) => {
    void navigator.clipboard.writeText(text)
    setCopiedFinalAnswer(true)
    setTimeout(() => setCopiedFinalAnswer(false), 2000)
  }

  const handleCopySystemPrompt = (text: string) => {
    void navigator.clipboard.writeText(text)
    setCopiedSystemPrompt(true)
    setTimeout(() => setCopiedSystemPrompt(false), 2000)
  }

  const handleCopyPromptPayload = (stepIdx: number, req: unknown) => {
    const text = typeof req === 'string' ? req : JSON.stringify(req, null, 2)
    void navigator.clipboard.writeText(text)
    setCopiedPromptStep(stepIdx)
    setTimeout(() => setCopiedPromptStep(null), 2000)
  }

  const handleCopyResponsePayload = (stepIdx: number, res: unknown) => {
    const text = typeof res === 'string' ? res : JSON.stringify(res, null, 2)
    void navigator.clipboard.writeText(text)
    setCopiedResponseStep(stepIdx)
    setTimeout(() => setCopiedResponseStep(null), 2000)
  }

  const handleCopyMessageText = (id: string, text: string) => {
    void navigator.clipboard.writeText(text)
    setCopiedMsgIdx(id)
    setTimeout(() => setCopiedMsgIdx(null), 2000)
  }

  const handleToggleExpandAllMessages = (stepIdx: number, totalCount: number) => {
    const current = allMessagesExpanded[stepIdx] ?? false
    const next = !current
    setAllMessagesExpanded(prev => ({ ...prev, [stepIdx]: next }))

    const map: Record<number, boolean> = {}
    for (let i = 0; i < totalCount; i++) {
      map[i] = next
    }
    setExpandedMessages(prev => ({ ...prev, [stepIdx]: map }))
  }

  // Scroll to step in Stream view
  const scrollToStepInStream = (idx: number) => {
    setSelectedStepIdx(idx)
    if (timelineContainerRef.current) {
      const card = timelineContainerRef.current.querySelector(
        `[data-stream-step="${idx}"]`,
      ) as HTMLElement | null
      if (card) {
        card.scrollIntoView({ behavior: 'smooth', block: 'start' })
      }
    }
  }

  const scrollToLogStep = (idx: number) => {
    setSelectedStepIdx(idx)
    userExplicitlyDisabledRef.current = false
    setAutoScroll(false)
    const stepNumber = steps[idx]?.stepIndex
    const card = document.querySelector(`[data-log-step="${stepNumber}"]`) as HTMLElement | null
    if (card) {
      card.scrollIntoView({ behavior: 'smooth', block: 'start' })
    }
  }

  const jumpToStep = (idx: number) => {
    if (viewMode === 'stream') {
      scrollToStepInStream(idx)
    } else if (viewMode === 'log') {
      scrollToLogStep(idx)
    } else {
      setSelectedStepIdx(idx)
    }
  }

  const finalAnswer = extractFinalAnswer(run)
  const mentionedFiles = useMemo(() => {
    return finalAnswer ? extractMentionedFiles(finalAnswer) : []
  }, [finalAnswer])
  const systemPrompt = run.systemPrompt || run.trajectory?.systemPrompt
  const activeRules = run.activeRules || run.trajectory?.activeRules

  return (
    <div className="trajectory-viewer-container">
      {/* ================= HEADER & STATS BAR ================= */}
      <div className="trajectory-header-card">
        <div className="trajectory-header-main">
          <div className="trajectory-header-title-group">
            <h3 className="section-title">
              {viewMode === 'log' ? '🧠' : '🔍'}{' '}
              {liveInspect ? 'Live' : viewMode === 'log' ? 'Agent' : 'Execution'}{' '}
              {viewMode === 'log' ? 'Log' : 'Trajectory'}:{' '}
              <span className="task-id-highlight">{run.taskId}</span>
              {liveInspect && (
                <span className="live-badge">
                  <span className="live-dot" /> LIVE RUNNING
                </span>
              )}
            </h3>
            <div className="trajectory-meta-row">
              {selectedHistoryId && !liveInspect && (
                <span className="meta-pill">
                  Run ID: <code>{selectedHistoryId}</code>
                </span>
              )}
              {liveInspect && inspectingJobId && (
                <span className="meta-pill">
                  Job ID: <code>{inspectingJobId}</code>
                </span>
              )}
              <span className="meta-pill">
                Policy: <code>{run.trajectory?.policyName || 'agent'}</code>
              </span>
              <span
                className={`status-badge ${liveInspect ? 'running' : String(run.status).toLowerCase()}`}
              >
                {liveInspect ? 'RUNNING' : String(run.status).toUpperCase()}
              </span>
              {run.recordedCassettePath && (
                <span className="meta-pill cassette-pill" title={run.recordedCassettePath}>
                  📼 Cassette recorded
                </span>
              )}
            </div>
          </div>

          <div className="trajectory-header-actions">
            {liveInspect && onStopTask && (
              <button
                type="button"
                className="btn-action stop-btn"
                onClick={() => {
                  if (inspectingJobId && onStopJob) {
                    const job = activeJobs.find(j => j.jobId === inspectingJobId)
                    if (job) {
                      void onStopJob(job)
                      return
                    }
                  }
                  void onStopTask(run.taskId)
                }}
                disabled={stoppingTaskId !== null}
                title="Stop this running task"
              >
                {stoppingTaskId ? '⏳ Stopping…' : '⏹ Stop Task'}
              </button>
            )}
            <button
              type="button"
              className="btn-secondary export-btn"
              onClick={() => downloadTrajectoryJson(run)}
              title="Download complete trajectory log as JSON"
            >
              <Download size={14} /> Export JSON
            </button>
            <button
              type="button"
              className="btn-secondary"
              onClick={onBackToTasks}
              title="Return to task list"
            >
              ← Back to Tasks
            </button>
          </div>
        </div>

        {/* Aggregate Metrics Bar */}
        <div className="trajectory-metrics-strip">
          <div className="metric-item">
            <span className="metric-label">Total Steps</span>
            <span className="metric-value">{metrics.totalSteps}</span>
          </div>
          <div className="metric-item">
            <span className="metric-label">Tool Calls</span>
            <span className="metric-value">{metrics.toolCallsCount}</span>
          </div>
          <div className="metric-item">
            <span className="metric-label">Total Duration</span>
            <span className="metric-value">{(metrics.durationMs / 1000).toFixed(2)}s</span>
          </div>
          {metrics.totalTokens > 0 && (
            <>
              <div className="metric-item">
                <span className="metric-label">Total Tokens</span>
                <span className="metric-value token-total">
                  {metrics.totalTokens.toLocaleString()}
                </span>
              </div>
              <div className="metric-item token-breakdown-item">
                <span className="metric-label">Token Breakdown</span>
                <span className="metric-subtext">
                  ↑ {metrics.inputTokens.toLocaleString()} prompt • ↓{' '}
                  {metrics.outputTokens.toLocaleString()} output
                </span>
              </div>
            </>
          )}
          {Object.keys(metrics.toolUsageFrequencies).length > 0 && (
            <div className="metric-item tool-freq-item">
              <span className="metric-label">Tools Used</span>
              <div className="tool-freq-chips">
                {Object.entries(metrics.toolUsageFrequencies).map(([name, count]) => (
                  <span key={name} className="tool-freq-chip" title={`${name}: ${count} calls`}>
                    <code>{name}</code> ×{count}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Continuable Run Warning Banner */}
      {isContinuableStatus(String(run.status)) && !liveInspect && onContinueTask && onOpenContinueModal && (
        <div className="cancelled-warning-banner">
          <div className="cancelled-warning-content">
            <h4>⚠️ Incomplete run — continue from saved progress summary</h4>
            <p>
              This execution took {metrics.totalSteps} steps and stopped before completion.
              Continue starts a <strong>new</strong> run seeded with the summary below.
            </p>
            {run.continuation?.compactedSummary && (
              <div className="continuation-summary-panel">
                <MarkdownPreview content={run.continuation.compactedSummary} />
              </div>
            )}
          </div>
          <div className="cancelled-warning-actions">
            {[10, 25, 50, 100].map(n => (
              <button
                key={n}
                type="button"
                className="btn-action continue-btn"
                onClick={() => onContinueTask(run.taskId, n)}
                disabled={runningTaskId === run.taskId}
                title={`New run with +${n} extra turns on this task`}
              >
                ⏩ +{n} Turns
              </button>
            ))}
            <div className="banner-turns-control">
              <label htmlFor="banner-extra-turns-nav">Extra:</label>
              <input
                id="banner-extra-turns-nav"
                type="number"
                min={1}
                max={500}
                className="banner-turns-input"
                value={bannerCustomTurns}
                onChange={e =>
                  setBannerCustomTurns(Math.max(1, parseInt(e.target.value, 10) || 1))
                }
              />
              <button
                type="button"
                className="btn-action continue-btn"
                onClick={() => onContinueTask(run.taskId, Math.max(1, bannerCustomTurns))}
                disabled={runningTaskId === run.taskId}
                title={`New run with +${bannerCustomTurns} extra turns`}
              >
                ⏩ Run (+{bannerCustomTurns})
              </button>
            </div>
            <button
              type="button"
              className="btn-secondary"
              onClick={() =>
                onOpenContinueModal(
                  run.taskId,
                  metrics.totalSteps,
                  undefined,
                  selectedHistoryId ?? undefined,
                  run.continuation?.compactedSummary ?? undefined,
                )
              }
              disabled={runningTaskId === run.taskId}
              title="Configure custom turn budget and options"
            >
              ⚙️ Specified Turns...
            </button>
          </div>
        </div>
      )}

      {/* System Prompt & Active Rules Inspector (Collapsible Card) */}
      {(systemPrompt || (activeRules && activeRules.length > 0)) && (
        <div className="trajectory-prompt-inspector-card">
          <div className="prompt-inspector-header">
            <div className="prompt-inspector-title-group">
              <div className="prompt-inspector-icon">🧠</div>
              <div>
                <h4 className="prompt-inspector-title">Composed System Prompt & Active Rules</h4>
                <span className="prompt-inspector-subtitle">
                  Inspect the exact system instructions and active rules injected into this task
                </span>
              </div>
            </div>
            <button
              type="button"
              className="btn-secondary prompt-inspector-toggle-btn"
              onClick={() => setShowPromptInspector(!showPromptInspector)}
            >
              {showPromptInspector ? '▲ Hide Prompt & Rules' : '▼ Inspect Prompt & Rules'}
            </button>
          </div>

          {showPromptInspector && (
            <div className="prompt-inspector-body">
              {activeRules && activeRules.length > 0 && (
                <div className="prompt-rules-section">
                  <h5 className="prompt-rules-title">Active Rules Applied ({activeRules.length}):</h5>
                  <div className="prompt-rules-chips">
                    {activeRules.map((rule, rIdx) => (
                      <div
                        key={rIdx}
                        className={`prompt-rule-chip ${rule.global ? 'global' : 'project'}`}
                      >
                        <span className="rule-badge-type">
                          {rule.global ? '🌍 Global' : '📁 Project'}
                        </span>
                        <span className="rule-badge-name">{rule.name}</span>
                        {rule.description && (
                          <span className="rule-badge-desc" title={rule.description}>
                            – {rule.description}
                          </span>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {systemPrompt && (
                <div className="prompt-content-section">
                  <div className="prompt-content-toolbar">
                    <div className="prompt-mode-tabs">
                      <button
                        type="button"
                        className={`prompt-tab-btn ${promptInspectorMode === 'markdown' ? 'active' : ''}`}
                        onClick={() => setPromptInspectorMode('markdown')}
                      >
                        Markdown Preview
                      </button>
                      <button
                        type="button"
                        className={`prompt-tab-btn ${promptInspectorMode === 'raw' ? 'active' : ''}`}
                        onClick={() => setPromptInspectorMode('raw')}
                      >
                        Raw Monospace
                      </button>
                    </div>
                    <button
                      type="button"
                      className={`prompt-copy-btn ${copiedSystemPrompt ? 'copied' : ''}`}
                      onClick={() => handleCopySystemPrompt(systemPrompt)}
                      title="Copy full system prompt"
                    >
                      {copiedSystemPrompt ? '✓ Copied Prompt' : '📋 Copy Prompt'}
                    </button>
                  </div>

                  <div className="prompt-content-container">
                    {promptInspectorMode === 'markdown' ? (
                      <div className="prompt-markdown-view">
                        <Suspense fallback={<div className="skeleton-line" style={{ height: '2rem' }} />}>
                          <MarkdownPreview content={systemPrompt} />
                        </Suspense>
                      </div>
                    ) : (
                      <pre className="prompt-raw-pre">{systemPrompt}</pre>
                    )}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* Final Answer Banner (if present) */}
      {finalAnswer && (
        <div className="trajectory-final-answer">
          <div className="final-answer-header">
            <div className="final-answer-title-group">
              <h4 className="final-answer-title">🏁 Final Agent Answer</h4>
              <span
                className={`status-badge ${
                  liveInspect ? 'running' : String(run.status).toLowerCase()
                }`}
              >
                {liveInspect ? 'RUNNING' : String(run.status).toUpperCase()}
              </span>
            </div>
            <div className="final-answer-actions">
              <div className="final-answer-tabs">
                <button
                  type="button"
                  className={`final-answer-tab-btn ${finalAnswerMode === 'markdown' ? 'active' : ''}`}
                  onClick={() => setFinalAnswerMode('markdown')}
                  title="View formatted Markdown"
                >
                  Markdown
                </button>
                <button
                  type="button"
                  className={`final-answer-tab-btn ${finalAnswerMode === 'raw' ? 'active' : ''}`}
                  onClick={() => setFinalAnswerMode('raw')}
                  title="View raw monospace text"
                >
                  Raw Text
                </button>
              </div>
              <button
                type="button"
                className={`final-answer-copy-btn ${copiedFinalAnswer ? 'copied' : ''}`}
                onClick={() => handleCopyFinalAnswer(finalAnswer)}
                title="Copy final answer to clipboard"
              >
                {copiedFinalAnswer ? '✓ Copied' : '📋 Copy'}
              </button>
            </div>
          </div>

          {/* Mentioned Files Toolbar */}
          {mentionedFiles.length > 0 && (
            <div className="final-answer-mentioned-files">
              <div className="mentioned-files-label">
                <span>📎 Mentioned Files ({mentionedFiles.length}):</span>
              </div>
              <div className="mentioned-files-chips">
                {mentionedFiles.map((file) => {
                  const fileName = file.split('/').pop() || file
                  const icon = getFileIcon(file)
                  return (
                    <button
                      key={file}
                      type="button"
                      className="mentioned-file-chip"
                      onClick={() => setPreviewFilePath(file)}
                      title={`Click to preview file: ${file}`}
                    >
                      <span className="file-chip-icon">{icon}</span>
                      <span className="file-chip-name">{fileName}</span>
                    </button>
                  )
                })}
              </div>
            </div>
          )}

          <DynamicPromptRunNotice notice={selectionRunNotice} />
          <div className="final-answer-content" ref={finalAnswerRef}>
            {finalAnswerMode === 'markdown' ? (
              <div className="final-answer-markdown">
                <Suspense fallback={<div className="skeleton-line" style={{ height: '2rem' }} />}>
                  <MarkdownPreview
                    content={finalAnswer}
                    resolveImageUrl={(src) =>
                      api && effectiveProjectSlug
                        ? api.getFileBytesUrl(effectiveProjectSlug, resolveProjectAssetPath('', src))
                        : src
                    }
                    onOpenFile={(path) => setPreviewFilePath(path)}
                  />
                </Suspense>
              </div>
            ) : (
              <pre className="final-answer-raw-pre">{finalAnswer}</pre>
            )}
          </div>
          <DynamicPromptSelectionMenu
            containerRef={finalAnswerRef}
            tasks={dynamicTasks}
            enabled={Boolean(api && effectiveProjectSlug)}
            busy={selectionRunBusy}
            api={api}
            projectSlug={effectiveProjectSlug}
            onSelect={(task, text) => {
              void runWithSelection(task, formatSelectionExtraPrompt('Final Agent Answer', text))
            }}
          />
        </div>
      )}

      {/* ================= STICKY NAVIGATION TOOLBAR ================= */}
      <div className="trajectory-sticky-toolbar">
        {/* Left section: View mode + Filter pills + Search */}
        <div className="toolbar-top-row">
          <div className="view-mode-group">
            <button
              type="button"
              className={`view-mode-btn ${viewMode === 'log' ? 'active' : ''}`}
              onClick={() => setViewMode('log')}
              title="Agent conversation log — thinking, tools, and replies in order"
            >
              <MessagesSquare size={14} /> Agent Log
            </button>
            <button
              type="button"
              className={`view-mode-btn ${viewMode === 'split' ? 'active' : ''}`}
              onClick={() => setViewMode('split')}
              title="Split Inspector View (Master-Detail)"
            >
              <Layers size={14} /> Split Inspector
            </button>
            <button
              type="button"
              className={`view-mode-btn ${viewMode === 'stream' ? 'active' : ''}`}
              onClick={() => setViewMode('stream')}
              title="Stream Timeline View (Continuous Chronological Scroll)"
            >
              <SlidersHorizontal size={14} /> Stream Timeline
            </button>
          </div>

          <div className="filter-pills-group">
            <button
              type="button"
              className={`filter-pill ${filterType === 'all' ? 'active' : ''}`}
              onClick={() => setFilterType('all')}
            >
              All ({steps.length})
            </button>
            <button
              type="button"
              className={`filter-pill ${filterType === 'tools' ? 'active' : ''}`}
              onClick={() => setFilterType('tools')}
            >
              🛠️ Tools ({steps.filter(s => parseAgentAction(s.agentAction).kind === 'call_tools').length})
            </button>
            <button
              type="button"
              className={`filter-pill ${filterType === 'final_answer' ? 'active' : ''}`}
              onClick={() => setFilterType('final_answer')}
            >
              🏁 Answer ({steps.filter(s => parseAgentAction(s.agentAction).kind === 'final_answer').length})
            </button>
            <button
              type="button"
              className={`filter-pill ${filterType === 'yield' ? 'active' : ''}`}
              onClick={() => setFilterType('yield')}
            >
              ⏸️ Stops ({steps.filter(s => ['yield_control', 'request_input'].includes(parseAgentAction(s.agentAction).kind)).length})
            </button>
            <button
              type="button"
              className={`filter-pill ${filterType === 'telemetry' ? 'active' : ''}`}
              onClick={() => setFilterType('telemetry')}
            >
              🤖 Telemetry ({steps.filter(s => Boolean(s.llmRequest || s.llmResponse || s.tokenUsage)).length})
            </button>
          </div>

          <div className="trajectory-search-wrapper">
            <Search size={14} className="search-icon" />
            <input
              type="text"
              placeholder="Search steps, tools, prompts… (Esc to clear)"
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              className="trajectory-search-input"
            />
            {searchQuery && (
              <button
                type="button"
                className="search-clear-btn"
                onClick={() => setSearchQuery('')}
                title="Clear search"
              >
                ✕
              </button>
            )}
          </div>
        </div>

        {/* Stepper + Global LLM Toggle Bar */}
        <div className="toolbar-bottom-row">
          <div className="stepper-controls">
            <button
              type="button"
              className="stepper-btn"
              onClick={() => setSelectedStepIdx(prev => Math.max(0, prev - 1))}
              disabled={selectedStepIdx <= 0 || steps.length === 0}
              title="Previous step (Shortcut: k or ArrowLeft)"
            >
              <ChevronLeft size={14} /> Prev
            </button>
            <span className="stepper-count-indicator">
              Step <strong>{steps.length > 0 ? selectedStepIdx + 1 : 0}</strong> of{' '}
              <strong>{steps.length}</strong>
              {filteredSteps.length !== steps.length && (
                <span className="filtered-hint">({filteredSteps.length} matches)</span>
              )}
            </span>
            <button
              type="button"
              className="stepper-btn"
              onClick={() => setSelectedStepIdx(prev => Math.min(steps.length - 1, prev + 1))}
              disabled={selectedStepIdx >= steps.length - 1 || steps.length === 0}
              title="Next step (Shortcut: j or ArrowRight)"
            >
              Next <ChevronRight size={14} />
            </button>
            {finalAnswer && (
              <button
                type="button"
                className="stepper-jump-btn"
                onClick={() => {
                  const finalIdx = steps.findIndex(
                    s => parseAgentAction(s.agentAction).kind === 'final_answer',
                  )
                  if (finalIdx !== -1) {
                    jumpToStep(finalIdx)
                  }
                }}
                title="Jump directly to Final Answer step (Shortcut: f)"
              >
                🏁 Jump to Answer
              </button>
            )}
          </div>

          <div className="toolbar-options-right">
            {viewMode === 'log' && (
              <button
                type="button"
                className={`btn-autoscroll-toggle toolbar-autoscroll-btn ${autoScroll ? 'active' : ''}`}
                onClick={handleToggleAutoScroll}
                title={
                  autoScroll
                    ? 'Auto-scroll is ON: new messages scroll into view automatically. Click to pause auto-scroll.'
                    : 'Auto-scroll is OFF: scrolling is paused. Click to enable auto-scroll.'
                }
                aria-pressed={autoScroll}
              >
                <ArrowDownToLine size={13} />
                <span>Auto-scroll: {autoScroll ? 'ON' : 'OFF'}</span>
                {liveInspect && autoScroll && <span className="autoscroll-pulse-dot" />}
              </button>
            )}
            <label
              className="trajectory-llm-toggle-label"
              title="Expand complete prompt messages and raw LLM completions by default for all steps"
            >
              <input
                type="checkbox"
                className="trajectory-checkbox"
                checked={showAllLlmTelemetry}
                onChange={e => setShowAllLlmTelemetry(e.target.checked)}
              />
              <span className="trajectory-toggle-title">
                🤖 Expand Complete LLM Prompts & Responses
              </span>
            </label>
            <span className="keyboard-hint" title="Use keyboard shortcuts: J / K to move between steps, F for Final Answer">
              ⌨️ Shortcuts: <kbd>J</kbd> / <kbd>K</kbd> to step
            </span>
          </div>
        </div>

        {/* Step Scrubber Minimap Rail */}
        {steps.length > 0 && (
          <div className="trajectory-scrubber-rail" ref={scrubberRailRef}>
            {steps.map((step, idx) => {
              const action = parseAgentAction(step.agentAction)
              const isSelected = selectedStepIdx === idx
              const hasTelemetry = Boolean(step.llmRequest || step.llmResponse || step.tokenUsage)
              const toolCount = action.toolCalls?.length ?? 0

              let badgeIcon = '⚙️'
              let badgeClass = 'kind-raw'
              if (action.kind === 'final_answer') {
                badgeIcon = '🏁'
                badgeClass = 'kind-final'
              } else if (action.kind === 'call_tools') {
                badgeIcon = '🛠️'
                badgeClass = 'kind-tools'
              } else if (action.kind === 'yield_control') {
                badgeIcon = '⏸️'
                badgeClass = 'kind-yield'
              } else if (action.kind === 'request_input') {
                badgeIcon = '💬'
                badgeClass = 'kind-input'
              }

              // Dim if filtered out by search/filter
              const isFilteredIn = filteredSteps.some(fs => fs.originalIndex === idx)

              return (
                <button
                  key={idx}
                  type="button"
                  data-step-index={idx}
                  className={`scrubber-step-pill ${badgeClass} ${isSelected ? 'active' : ''} ${
                    !isFilteredIn ? 'dimmed' : ''
                  }`}
                  onClick={() => jumpToStep(idx)}
                  title={`Step #${step.stepIndex} (${step.durationMs}ms)${
                    action.toolCalls
                      ? `\nTools: ${action.toolCalls.map(t => t.name).join(', ')}`
                      : ''
                  }${hasTelemetry ? '\nLLM Telemetry available' : ''}`}
                >
                  <span className="scrubber-pill-num">#{step.stepIndex}</span>
                  <span className="scrubber-pill-icon">{badgeIcon}</span>
                  {toolCount > 1 && <span className="scrubber-pill-count">×{toolCount}</span>}
                </button>
              )
            })}
          </div>
        )}
      </div>

      {/* ================= EMPTY STATE ================= */}
      {steps.length === 0 && (
        <div className="empty-trajectory-card">
          <p className="empty-text">
            {liveInspect
              ? 'Waiting for the first agent step… this view refreshes automatically while the task is running.'
              : 'No trajectory steps recorded for this run.'}
          </p>
        </div>
      )}

      {/* ================= VIEW MODE 0: AGENT LOG (CHAT TRANSCRIPT) ================= */}
      {viewMode === 'log' && steps.length > 0 && (
        <div className="agent-log-panel">
          <div className="agent-log-header">
            <p className="agent-log-intro">
              Chat-style transcript of this task. Tool calls appear inline with a short summary of what ran;
              expand a card to inspect arguments and output.
            </p>
            <div className="agent-log-header-controls">
              <button
                type="button"
                className={`btn-autoscroll-toggle ${allPromptsExpanded || showAllLlmTelemetry ? 'active' : ''}`}
                onClick={() => setAllPromptsExpanded(prev => !prev)}
                title="Expand or collapse all prompt messages in the log"
              >
                <Terminal size={13} />
                <span>Prompts {allPromptsExpanded || showAllLlmTelemetry ? 'ON' : 'OFF'}</span>
              </button>
              <button
                type="button"
                className={`btn-autoscroll-toggle ${allThinkingExpanded || showAllLlmTelemetry ? 'active' : ''}`}
                onClick={() => setAllThinkingExpanded(prev => !prev)}
                title="Expand or collapse all thinking blocks in the log"
              >
                <Sparkles size={13} />
                <span>Thinking {allThinkingExpanded || showAllLlmTelemetry ? 'ON' : 'OFF'}</span>
              </button>
              <button
                type="button"
                className={`btn-autoscroll-toggle ${allJudgmentsExpanded || showAllLlmTelemetry ? 'active' : ''}`}
                onClick={() => setAllJudgmentsExpanded(prev => !prev)}
                title="Expand or collapse all Laya judgment cards in the log"
              >
                <Scale size={13} />
                <span>Judgments {allJudgmentsExpanded || showAllLlmTelemetry ? 'ON' : 'OFF'}</span>
              </button>
              <button
                type="button"
                className={`btn-autoscroll-toggle ${autoScroll ? 'active' : ''}`}
                onClick={handleToggleAutoScroll}
                title={
                  autoScroll
                    ? 'Auto-scroll is ON: new messages scroll into view automatically. Click to pause auto-scroll.'
                    : 'Auto-scroll is OFF: scrolling is locked. Click to enable auto-scroll.'
                }
                aria-pressed={autoScroll}
              >
                <ArrowDownToLine size={13} />
                <span>Auto-scroll {autoScroll ? 'ON' : 'OFF'}</span>
                {liveInspect && autoScroll && <span className="autoscroll-pulse-dot" />}
              </button>
            </div>
          </div>
          {filteredLogTurns.length === 0 ? (
            <div className="empty-card">No log entries match filter / search criteria.</div>
          ) : (
            <div
              className="agent-log-transcript"
              ref={agentLogTranscriptRef}
              onScroll={handleLogScroll}
            >
              {filteredLogTurns.map((turn, turnIdx) => {
                const isLatestStep = Boolean(liveInspect && turnIdx === filteredLogTurns.length - 1 && turn.role === 'assistant')
                const isPromptOpen = Boolean(
                  showAllLlmTelemetry ||
                  allPromptsExpanded ||
                  (expandedPrompts[turn.id] ?? false)
                )
                const isThinkingOpen = Boolean(
                  showAllLlmTelemetry ||
                  allThinkingExpanded ||
                  (expandedThinking[turn.id] ?? isLatestStep)
                )
                const isJudgmentOpen = Boolean(
                  showAllLlmTelemetry ||
                  allJudgmentsExpanded ||
                  (expandedJudgments[turn.id] ?? false)
                )
                return (
                  <AgentLogTurnCard
                    key={turn.id}
                    turn={turn}
                    isLatest={isLatestStep}
                    promptOpen={isPromptOpen}
                    onTogglePrompt={() =>
                      setExpandedPrompts(prev => ({
                        ...prev,
                        [turn.id]: !isPromptOpen,
                      }))
                    }
                    thinkingOpen={isThinkingOpen}
                    onToggleThinking={() =>
                      setExpandedThinking(prev => ({
                        ...prev,
                        [turn.id]: !isThinkingOpen,
                      }))
                    }
                    judgmentOpen={isJudgmentOpen}
                    onToggleJudgment={() =>
                      setExpandedJudgments(prev => ({
                        ...prev,
                        [turn.id]: !isJudgmentOpen,
                      }))
                    }
                    expandedTools={expandedLogTools}
                    onToggleTool={toolKey =>
                      setExpandedLogTools(prev => ({ ...prev, [toolKey]: !prev[toolKey] }))
                    }
                    copiedMsgIdx={copiedMsgIdx}
                    onCopyText={handleCopyMessageText}
                  />
                )
              })}
              {liveInspect && (
                <div className="status-live-banner agent-log-live-status">
                  <span className="stream-dot" />
                  <span className="status-live-text">Agent is working… log updates as each step finishes.</span>
                </div>
              )}
              <div ref={agentLogEndRef} />
            </div>
          )}
          {liveInspect && !autoScroll && (
            <button
              type="button"
              className="agent-log-floating-autoscroll-btn"
              onClick={handleResumeAutoScroll}
              title="Resume auto-scroll and jump to latest log step"
            >
              <ArrowDownToLine size={14} />
              <span>Resume Auto-scroll</span>
            </button>
          )}
        </div>
      )}

      {/* ================= VIEW MODE 1: SPLIT INSPECTOR (MASTER-DETAIL) ================= */}
      {viewMode === 'split' && steps.length > 0 && (
        <div className="trajectory-split-layout">
          {/* Left Step Navigator Rail */}
          <aside className="trajectory-step-sidebar">
            <div className="sidebar-header-row">
              <span className="sidebar-title">Trajectory Steps</span>
              <span className="sidebar-counter">
                {filteredSteps.length} of {steps.length}
              </span>
            </div>

            <div className="sidebar-steps-list">
              {filteredSteps.length === 0 ? (
                <div className="sidebar-empty-hint">No steps match filter / search query.</div>
              ) : (
                filteredSteps.map(({ step, originalIndex, parsedAction, telemetry }) => {
                  const isSelected = selectedStepIdx === originalIndex
                  const isFinal = parsedAction.kind === 'final_answer'
                  const toolCalls = parsedAction.toolCalls ?? []

                  return (
                    <div
                      key={originalIndex}
                      className={`sidebar-step-item ${isSelected ? 'selected' : ''} ${
                        isFinal ? 'is-final' : ''
                      }`}
                      onClick={() => setSelectedStepIdx(originalIndex)}
                    >
                      <div className="sidebar-step-top">
                        <span className="step-num-badge">Step #{step.stepIndex}</span>
                        <span className="step-time-badge">
                          {new Date(step.timestamp).toLocaleTimeString([], {
                            hour: '2-digit',
                            minute: '2-digit',
                            second: '2-digit',
                          })}
                        </span>
                      </div>

                      <div className="sidebar-step-action-desc">
                        {isFinal && (
                          <span className="step-action-tag final-answer">🏁 Final Answer</span>
                        )}
                        {parsedAction.kind === 'call_tools' && (
                          <div className="sidebar-tools-list">
                            {toolCalls.slice(0, 3).map((t, tIdx) => (
                              <span key={tIdx} className="sidebar-tool-badge">
                                🛠️ {t.name}
                              </span>
                            ))}
                            {toolCalls.length > 3 && (
                              <span className="sidebar-tool-more">+{toolCalls.length - 3} more</span>
                            )}
                          </div>
                        )}
                        {parsedAction.kind === 'yield_control' && (
                          <span className="step-action-tag yield-control">⏸️ Yield Control</span>
                        )}
                        {parsedAction.kind === 'request_input' && (
                          <span className="step-action-tag request-input">💬 Request Input</span>
                        )}
                      </div>

                      <div className="sidebar-step-footer">
                        <span className="step-duration-text">⏱️ {step.durationMs}ms</span>
                        {telemetry?.totalTokens && (
                          <span className="step-tokens-text">
                            ⚡ {telemetry.totalTokens.toLocaleString()} tok
                          </span>
                        )}
                        {telemetry && (
                          <span className="sidebar-telemetry-indicator" title="LLM Turn Telemetry Available">
                            🤖
                          </span>
                        )}
                      </div>
                    </div>
                  )
                })
              )}
            </div>
          </aside>

          {/* Right Step Detail Inspection Pane */}
          <main className="trajectory-inspector-pane">
            {steps[selectedStepIdx] && (
              <StepDetailCard
                step={steps[selectedStepIdx]}
                stepIdx={selectedStepIdx}
                showAllLlmTelemetry={showAllLlmTelemetry}
                activeTelemetryTab={activeTelemetryTab[selectedStepIdx] ?? 'prompt'}
                setActiveTelemetryTab={tab =>
                  setActiveTelemetryTab(prev => ({ ...prev, [selectedStepIdx]: tab }))
                }
                msgRoleFilter={msgRoleFilter[selectedStepIdx] ?? 'all'}
                setMsgRoleFilter={filter =>
                  setMsgRoleFilter(prev => ({ ...prev, [selectedStepIdx]: filter }))
                }
                expandedMessages={expandedMessages[selectedStepIdx] ?? {}}
                setExpandedMessage={(mIdx, val) =>
                  setExpandedMessages(prev => ({
                    ...prev,
                    [selectedStepIdx]: { ...(prev[selectedStepIdx] ?? {}), [mIdx]: val },
                  }))
                }
                allMessagesExpanded={allMessagesExpanded[selectedStepIdx] ?? false}
                onToggleExpandAllMessages={count =>
                  handleToggleExpandAllMessages(selectedStepIdx, count)
                }
                isRawExpanded={expandedStepRaw[selectedStepIdx] ?? false}
                onToggleRawExpanded={() =>
                  setExpandedStepRaw(prev => ({
                    ...prev,
                    [selectedStepIdx]: !prev[selectedStepIdx],
                  }))
                }
                promptRenderMode={promptRenderMode[selectedStepIdx] ?? 'monospace'}
                setPromptRenderMode={mode =>
                  setPromptRenderMode(prev => ({ ...prev, [selectedStepIdx]: mode }))
                }
                copiedPrompt={copiedPromptStep === selectedStepIdx}
                copiedResponse={copiedResponseStep === selectedStepIdx}
                copiedMsgIdx={copiedMsgIdx}
                onCopyPrompt={req => handleCopyPromptPayload(selectedStepIdx, req)}
                onCopyResponse={res => handleCopyResponsePayload(selectedStepIdx, res)}
                onCopyMessageText={handleCopyMessageText}
              />
            )}
          </main>
        </div>
      )}

      {/* ================= VIEW MODE 2: STREAM TIMELINE ================= */}
      {viewMode === 'stream' && steps.length > 0 && (
        <div className="trajectory-timeline" ref={timelineContainerRef}>
          {filteredSteps.length === 0 ? (
            <div className="empty-card">No steps match filter / search criteria.</div>
          ) : (
            filteredSteps.map(({ step, originalIndex }) => (
              <div
                key={originalIndex}
                data-stream-step={originalIndex}
                className={`trajectory-stream-step-wrapper ${
                  selectedStepIdx === originalIndex ? 'is-selected-stream-step' : ''
                }`}
              >
                <StepDetailCard
                  step={step}
                  stepIdx={originalIndex}
                  showAllLlmTelemetry={showAllLlmTelemetry}
                  activeTelemetryTab={activeTelemetryTab[originalIndex] ?? 'prompt'}
                  setActiveTelemetryTab={tab =>
                    setActiveTelemetryTab(prev => ({ ...prev, [originalIndex]: tab }))
                  }
                  msgRoleFilter={msgRoleFilter[originalIndex] ?? 'all'}
                  setMsgRoleFilter={filter =>
                    setMsgRoleFilter(prev => ({ ...prev, [originalIndex]: filter }))
                  }
                  expandedMessages={expandedMessages[originalIndex] ?? {}}
                  setExpandedMessage={(mIdx, val) =>
                    setExpandedMessages(prev => ({
                      ...prev,
                      [originalIndex]: { ...(prev[originalIndex] ?? {}), [mIdx]: val },
                    }))
                  }
                  allMessagesExpanded={allMessagesExpanded[originalIndex] ?? false}
                  onToggleExpandAllMessages={count =>
                    handleToggleExpandAllMessages(originalIndex, count)
                  }
                  isRawExpanded={expandedStepRaw[originalIndex] ?? false}
                  onToggleRawExpanded={() =>
                    setExpandedStepRaw(prev => ({
                      ...prev,
                      [originalIndex]: !prev[originalIndex],
                    }))
                  }
                  promptRenderMode={promptRenderMode[originalIndex] ?? 'monospace'}
                  setPromptRenderMode={mode =>
                    setPromptRenderMode(prev => ({ ...prev, [originalIndex]: mode }))
                  }
                  copiedPrompt={copiedPromptStep === originalIndex}
                  copiedResponse={copiedResponseStep === originalIndex}
                  copiedMsgIdx={copiedMsgIdx}
                  onCopyPrompt={req => handleCopyPromptPayload(originalIndex, req)}
                  onCopyResponse={res => handleCopyResponsePayload(originalIndex, res)}
                  onCopyMessageText={handleCopyMessageText}
                />
              </div>
            ))
          )}
        </div>
      )}

      {/* File Preview Modal */}
      {previewFilePath && api && effectiveProjectSlug && (
        <FilePreviewModal
          api={api}
          projectSlug={effectiveProjectSlug}
          filePath={previewFilePath}
          onClose={() => setPreviewFilePath(null)}
          onNavigateToFile={onNavigateToFiles}
        />
      )}
    </div>
  )
}

// =========================================================================
// STEP DETAIL CARD COMPONENT
// =========================================================================

interface StepDetailCardProps {
  step: HarnessRunResponse['trajectory']['steps'][0]
  stepIdx: number
  showAllLlmTelemetry: boolean
  activeTelemetryTab: TelemetryTab
  setActiveTelemetryTab: (tab: TelemetryTab) => void
  msgRoleFilter: MsgRoleFilter
  setMsgRoleFilter: (filter: MsgRoleFilter) => void
  expandedMessages: Record<number, boolean>
  setExpandedMessage: (mIdx: number, val: boolean) => void
  allMessagesExpanded: boolean
  onToggleExpandAllMessages: (count: number) => void
  isRawExpanded: boolean
  onToggleRawExpanded: () => void
  promptRenderMode: 'markdown' | 'monospace'
  setPromptRenderMode: (mode: 'markdown' | 'monospace') => void
  copiedPrompt: boolean
  copiedResponse: boolean
  copiedMsgIdx: string | null
  onCopyPrompt: (req: unknown) => void
  onCopyResponse: (res: unknown) => void
  onCopyMessageText: (id: string, text: string) => void
}

function StepDetailCard({
  step,
  stepIdx,
  showAllLlmTelemetry,
  activeTelemetryTab,
  setActiveTelemetryTab,
  msgRoleFilter,
  setMsgRoleFilter,
  expandedMessages,
  setExpandedMessage,
  allMessagesExpanded,
  onToggleExpandAllMessages,
  isRawExpanded,
  onToggleRawExpanded,
  promptRenderMode,
  setPromptRenderMode,
  copiedPrompt,
  copiedResponse,
  copiedMsgIdx,
  onCopyPrompt,
  onCopyResponse,
  onCopyMessageText,
}: StepDetailCardProps) {
  const parsedAction = parseAgentAction(step.agentAction)
  const telemetry = extractLlmTelemetry(step)
  const isFinalStep = parsedAction.kind === 'final_answer'
  const hasTelemetry = Boolean(telemetry)

  // Message filtering
  const messages = telemetry?.messages ?? []
  const filteredMessages = useMemo(() => {
    if (msgRoleFilter === 'all') return messages
    return messages.filter(m => m.role === msgRoleFilter)
  }, [messages, msgRoleFilter])

  return (
    <div className={`trajectory-step-card ${isFinalStep ? 'is-final-step' : ''}`}>
      {/* Step Header */}
      <div className="step-header">
        <div className="step-header-left">
          <span className="step-index-badge">Step #{step.stepIndex}</span>
          <span className="step-time">
            <Clock size={12} /> {new Date(step.timestamp).toLocaleTimeString()} ({step.durationMs} ms)
          </span>
        </div>

        <div className="step-header-right">
          {telemetry?.totalTokens && (
            <span className="step-token-badge">
              ⚡ {telemetry.totalTokens.toLocaleString()} tokens
              {telemetry.tokensPerSec ? ` (${telemetry.tokensPerSec} tok/s)` : ''}
            </span>
          )}
          <button
            type="button"
            className="step-toggle-btn"
            onClick={onToggleRawExpanded}
            title="Toggle raw JSON payload for agentAction"
          >
            {isRawExpanded ? 'Hide Raw Action JSON' : 'Raw Action JSON'}
          </button>
        </div>
      </div>

      <div className="step-body">
        {/* ================= 1. AGENT ACTION BOX ================= */}
        <div className="step-action-box">
          <div className="step-action-header-row">
            <h5>Agent Action:</h5>
            <div style={{ display: 'flex', gap: '0.4rem', alignItems: 'center' }}>
              {parsedAction.kind === 'final_answer' && (
                <span className="step-action-badge final-answer">🏁 Final Answer</span>
              )}
              {parsedAction.kind === 'call_tools' && (
                <span className="step-action-badge call-tools">
                  🛠️ Tool Calls ({parsedAction.toolCalls?.length ?? 0})
                </span>
              )}
              {parsedAction.kind === 'yield_control' && (
                <span className="step-action-badge yield-control">⏸️ Yield Control</span>
              )}
              {parsedAction.kind === 'request_input' && (
                <span className="step-action-badge request-input">💬 Request Input</span>
              )}
            </div>
          </div>

          {isRawExpanded ? (
            <pre className="json-pre">{JSON.stringify(step.agentAction, null, 2)}</pre>
          ) : (
            <>
              {parsedAction.kind === 'final_answer' && parsedAction.finalAnswer && (
                <div className="step-final-answer-card">
                  <Suspense fallback={<div className="skeleton-line" style={{ height: '2rem' }} />}>
                    <MarkdownPreview content={parsedAction.finalAnswer} />
                  </Suspense>
                </div>
              )}
              {parsedAction.kind === 'call_tools' && parsedAction.toolCalls && (
                <div className="step-tool-call-list">
                  {parsedAction.toolCalls.map((call, cIdx) => (
                    <div key={cIdx} className="step-tool-call-item">
                      <div className="step-tool-call-header">
                        <span className="step-tool-call-name">🛠️ {call.name}</span>
                        {call.arguments !== undefined && (
                          <button
                            type="button"
                            className="mini-copy-btn"
                            onClick={() =>
                              onCopyMessageText(
                                `tool-${stepIdx}-${cIdx}`,
                                formatToolArguments(call.arguments),
                              )
                            }
                            title="Copy arguments"
                          >
                            {copiedMsgIdx === `tool-${stepIdx}-${cIdx}` ? (
                              <Check size={11} className="text-success" />
                            ) : (
                              <Copy size={11} />
                            )}
                          </button>
                        )}
                      </div>
                      {call.arguments !== undefined && (
                        <pre className="step-tool-call-args">
                          {formatToolArguments(call.arguments)}
                        </pre>
                      )}
                    </div>
                  ))}
                </div>
              )}
              {parsedAction.kind === 'yield_control' && (
                <div className="step-yield-box">
                  <strong>Reason:</strong> {parsedAction.yieldReason || 'None'}
                </div>
              )}
              {parsedAction.kind === 'request_input' && (
                <div className="step-input-box">
                  <strong>Prompt:</strong> {parsedAction.prompt || 'None'}
                </div>
              )}
              {parsedAction.kind === 'raw' && (
                <pre className="json-pre">{JSON.stringify(step.agentAction, null, 2)}</pre>
              )}
            </>
          )}
        </div>

        {/* ================= 2. DEEP LLM TELEMETRY & RAW RESPONSES ================= */}
        {hasTelemetry && (
          <div className="step-llm-telemetry-box deep-inspector">
            {/* Telemetry Tabs Bar */}
            <div className="llm-telemetry-tab-bar">
              <div className="telemetry-tabs-left">
                <button
                  type="button"
                  className={`telemetry-tab-btn ${activeTelemetryTab === 'prompt' ? 'active' : ''}`}
                  onClick={() => setActiveTelemetryTab('prompt')}
                >
                  <MessageSquare size={13} /> Full Prompt Sent ({messages.length})
                </button>
                <button
                  type="button"
                  className={`telemetry-tab-btn ${activeTelemetryTab === 'response' ? 'active' : ''}`}
                  onClick={() => setActiveTelemetryTab('response')}
                >
                  <Bot size={13} /> Raw Model Response
                </button>
                {telemetry?.thinking && (
                  <button
                    type="button"
                    className={`telemetry-tab-btn ${activeTelemetryTab === 'thinking' ? 'active' : ''}`}
                    onClick={() => setActiveTelemetryTab('thinking')}
                  >
                    <Sparkles size={13} /> Reasoning / Thinking
                  </button>
                )}
                {telemetry?.totalTokens && (
                  <button
                    type="button"
                    className={`telemetry-tab-btn ${activeTelemetryTab === 'tokens' ? 'active' : ''}`}
                    onClick={() => setActiveTelemetryTab('tokens')}
                  >
                    <Zap size={13} /> Tokens & Latency
                  </button>
                )}
                <button
                  type="button"
                  className={`telemetry-tab-btn ${activeTelemetryTab === 'raw' ? 'active' : ''}`}
                  onClick={() => setActiveTelemetryTab('raw')}
                >
                  <FileCode size={13} /> Raw JSON Payloads
                </button>
              </div>

              <div className="telemetry-tabs-right">
                {activeTelemetryTab === 'prompt' && (
                  <button
                    type="button"
                    className="step-copy-sub-btn"
                    onClick={() => onCopyPrompt(step.llmRequest)}
                    title="Copy full prompt payload"
                  >
                    {copiedPrompt ? '✓ Copied' : '📋 Copy Prompt'}
                  </button>
                )}
                {activeTelemetryTab === 'response' && (
                  <button
                    type="button"
                    className="step-copy-sub-btn"
                    onClick={() => onCopyResponse(step.llmResponse)}
                    title="Copy raw response payload"
                  >
                    {copiedResponse ? '✓ Copied' : '📋 Copy Response'}
                  </button>
                )}
              </div>
            </div>

            {/* TAB PANEL 1: FULL PROMPT SENT */}
            {activeTelemetryTab === 'prompt' && (
              <div className="telemetry-panel prompt-panel">
                {/* System prompt details */}
                {telemetry?.systemPrompt && (
                  <details className="step-prompt-system-details" open={showAllLlmTelemetry}>
                    <summary className="step-prompt-system-summary">
                      <Brain size={13} /> Composed System Instructions ({telemetry.systemPrompt.length} chars)
                    </summary>
                    <div className="system-prompt-actions">
                      <button
                        type="button"
                        className="mini-copy-btn"
                        onClick={() =>
                          onCopyMessageText(`sys-${stepIdx}`, telemetry.systemPrompt || '')
                        }
                        title="Copy system prompt"
                      >
                        {copiedMsgIdx === `sys-${stepIdx}` ? '✓ Copied' : '📋 Copy'}
                      </button>
                    </div>
                    <pre className="step-prompt-system-pre">{telemetry.systemPrompt}</pre>
                  </details>
                )}

                {/* Messages header & role filters */}
                <div className="prompt-messages-toolbar">
                  <div className="msg-role-filters">
                    <span className="toolbar-label">
                      <Filter size={12} /> Filter Role:
                    </span>
                    {(['all', 'user', 'assistant', 'tool', 'system'] as MsgRoleFilter[]).map(role => (
                      <button
                        key={role}
                        type="button"
                        className={`msg-filter-btn ${msgRoleFilter === role ? 'active' : ''}`}
                        onClick={() => setMsgRoleFilter(role)}
                      >
                        {role === 'all'
                          ? `All (${messages.length})`
                          : `${role.toUpperCase()} (${messages.filter(m => m.role === role).length})`}
                      </button>
                    ))}
                  </div>

                  <div className="prompt-messages-actions">
                    <div className="prompt-render-mode-tabs">
                      <button
                        type="button"
                        className={`render-mode-tab ${promptRenderMode === 'monospace' ? 'active' : ''}`}
                        onClick={() => setPromptRenderMode('monospace')}
                        title="Monospace preformatted text"
                      >
                        Monospace
                      </button>
                      <button
                        type="button"
                        className={`render-mode-tab ${promptRenderMode === 'markdown' ? 'active' : ''}`}
                        onClick={() => setPromptRenderMode('markdown')}
                        title="Render markdown formatting"
                      >
                        Markdown
                      </button>
                    </div>

                    <button
                      type="button"
                      className="expand-all-btn"
                      onClick={() => onToggleExpandAllMessages(messages.length)}
                      title="Expand or collapse all message bodies"
                    >
                      {allMessagesExpanded ? 'Collapse All' : 'Expand All'}
                    </button>
                  </div>
                </div>

                {/* Message cards */}
                {filteredMessages.length === 0 ? (
                  <div className="empty-subcard">No messages match selected role filter.</div>
                ) : (
                  <div className="prompt-messages-list">
                    {filteredMessages.map((m, mIdx) => {
                      const msgId = `step-${stepIdx}-msg-${m.index}`
                      const isExpanded = showAllLlmTelemetry || (expandedMessages[m.index] ?? true)
                      const roleIcon =
                        m.role === 'system'
                          ? '🧠'
                          : m.role === 'assistant'
                          ? '🤖'
                          : m.role === 'tool'
                          ? '⚙️'
                          : '👤'

                      return (
                        <div key={mIdx} className={`prompt-message-card role-${m.role}`}>
                          <div
                            className="prompt-message-header"
                            onClick={() => setExpandedMessage(m.index, !isExpanded)}
                          >
                            <div className="prompt-message-header-left">
                              <span className={`prompt-role-pill ${m.role}`}>
                                {roleIcon} {m.role.toUpperCase()}
                              </span>
                              <span className="prompt-message-index">Turn #{m.index}</span>
                              {m.text && (
                                <span className="msg-length-badge">
                                  {m.text.length} chars
                                </span>
                              )}
                              {m.toolCalls && (
                                <span className="msg-tool-call-count">
                                  🛠️ {m.toolCalls.length} tool call(s)
                                </span>
                              )}
                            </div>

                            <div
                              className="prompt-message-header-right"
                              onClick={e => e.stopPropagation()}
                            >
                              <button
                                type="button"
                                className="mini-copy-btn"
                                onClick={() =>
                                  onCopyMessageText(
                                    msgId,
                                    m.text || JSON.stringify(m.content, null, 2),
                                  )
                                }
                                title="Copy message text"
                              >
                                {copiedMsgIdx === msgId ? (
                                  <Check size={12} className="text-success" />
                                ) : (
                                  <Copy size={12} />
                                )}
                              </button>
                              <button
                                type="button"
                                className="toggle-msg-collapse-btn"
                                onClick={() => setExpandedMessage(m.index, !isExpanded)}
                              >
                                {isExpanded ? '▲' : '▼'}
                              </button>
                            </div>
                          </div>

                          {isExpanded && (
                            <div className="prompt-message-body">
                              {/* Thinking block inside message */}
                              {m.thinking && (
                                <div className="prompt-msg-thinking-block">
                                  <span className="thinking-title">💭 Model Thinking Trace:</span>
                                  <pre className="thinking-content">{m.thinking}</pre>
                                </div>
                              )}

                              {/* Tool calls inside message */}
                              {m.toolCalls && m.toolCalls.length > 0 && (
                                <div className="prompt-msg-tool-calls">
                                  {m.toolCalls.map((tc, tcIdx) => (
                                    <div key={tcIdx} className="prompt-msg-tool-call">
                                      <span className="prompt-tool-badge">
                                        🛠️ Request: <code>{tc.name}</code>
                                      </span>
                                      {tc.arguments !== undefined && (
                                        <pre className="prompt-tool-args-pre">
                                          {formatToolArguments(tc.arguments)}
                                        </pre>
                                      )}
                                    </div>
                                  ))}
                                </div>
                              )}

                              {/* Message text */}
                              {m.text ? (
                                promptRenderMode === 'markdown' ? (
                                  <div className="prompt-msg-markdown">
                                    <Suspense fallback={<div className="skeleton-line" />}>
                                      <MarkdownPreview content={m.text} />
                                    </Suspense>
                                  </div>
                                ) : (
                                  <pre className="prompt-msg-text-pre">{m.text}</pre>
                                )
                              ) : (
                                <pre className="json-pre">{JSON.stringify(m.content, null, 2)}</pre>
                              )}
                            </div>
                          )}
                        </div>
                      )
                    })}
                  </div>
                )}
              </div>
            )}

            {/* TAB PANEL 2: RAW MODEL RESPONSE */}
            {activeTelemetryTab === 'response' && (
              <div className="telemetry-panel response-panel">
                {telemetry?.completionText && (
                  <div className="response-subpart">
                    <div className="response-subpart-header">
                      <h6>Model Completion Text:</h6>
                      <span className="subpart-meta">
                        {telemetry.completionText.length} characters
                      </span>
                    </div>
                    <pre className="llm-response-pre">{telemetry.completionText}</pre>
                  </div>
                )}

                {telemetry?.modelToolCalls && telemetry.modelToolCalls.length > 0 && (
                  <div className="response-subpart">
                    <div className="response-subpart-header">
                      <h6>Generated Tool Calls:</h6>
                      <span className="subpart-meta">
                        {telemetry.modelToolCalls.length} calls
                      </span>
                    </div>
                    <div className="response-tool-calls-list">
                      {telemetry.modelToolCalls.map((tc, tcIdx) => (
                        <div key={tcIdx} className="response-tool-call-item">
                          <span className="tool-call-badge">🛠️ {tc.name}</span>
                          {tc.arguments !== undefined && (
                            <pre className="tool-call-args-pre">
                              {formatToolArguments(tc.arguments)}
                            </pre>
                          )}
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {Boolean(telemetry?.reply) && (
                  <div className="response-subpart">
                    <div className="response-subpart-header">
                      <h6>Assistant Reply Object:</h6>
                    </div>
                    <pre className="json-pre">{JSON.stringify(telemetry?.reply, null, 2)}</pre>
                  </div>
                )}

                {!telemetry?.completionText && !telemetry?.modelToolCalls && !telemetry?.reply && (
                  <pre className="json-pre">{JSON.stringify(telemetry?.rawResponse, null, 2)}</pre>
                )}
              </div>
            )}

            {/* TAB PANEL 3: REASONING / THINKING */}
            {activeTelemetryTab === 'thinking' && telemetry?.thinking && (
              <div className="telemetry-panel thinking-panel">
                <div className="thinking-panel-header">
                  <div className="thinking-title">
                    <Sparkles size={14} /> Chain of Thought / Extended Thinking Trace
                  </div>
                  <button
                    type="button"
                    className="mini-copy-btn"
                    onClick={() =>
                      onCopyMessageText(`think-${stepIdx}`, telemetry.thinking || '')
                    }
                    title="Copy thinking trace"
                  >
                    {copiedMsgIdx === `think-${stepIdx}` ? '✓ Copied' : '📋 Copy Thinking'}
                  </button>
                </div>
                <pre className="deep-thinking-pre">{telemetry.thinking}</pre>
              </div>
            )}

            {/* TAB PANEL 4: TOKENS & LATENCY METRICS */}
            {activeTelemetryTab === 'tokens' && telemetry && (
              <div className="telemetry-panel tokens-panel">
                <div className="token-metrics-grid">
                  <div className="token-metric-card">
                    <span className="t-label">Prompt Input Tokens</span>
                    <span className="t-val input">
                      {telemetry.inputTokens?.toLocaleString() ?? '—'}
                    </span>
                  </div>
                  <div className="token-metric-card">
                    <span className="t-label">Completion Output Tokens</span>
                    <span className="t-val output">
                      {telemetry.outputTokens?.toLocaleString() ?? '—'}
                    </span>
                  </div>
                  <div className="token-metric-card">
                    <span className="t-label">Total Step Tokens</span>
                    <span className="t-val total">
                      {telemetry.totalTokens?.toLocaleString() ?? '—'}
                    </span>
                  </div>
                  <div className="token-metric-card">
                    <span className="t-label">Turn Latency</span>
                    <span className="t-val latency">{telemetry.durationMs} ms</span>
                  </div>
                  {telemetry.tokensPerSec !== undefined && (
                    <div className="token-metric-card">
                      <span className="t-label">Generation Speed</span>
                      <span className="t-val speed">{telemetry.tokensPerSec} tok/sec</span>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* TAB PANEL 5: RAW JSON PAYLOADS */}
            {activeTelemetryTab === 'raw' && (
              <div className="telemetry-panel raw-json-panel">
                <div className="raw-json-columns">
                  <div className="raw-json-col">
                    <div className="raw-col-header">
                      <h6>Request Payload (LLM Input):</h6>
                      <button
                        type="button"
                        className="mini-copy-btn"
                        onClick={() => onCopyPrompt(step.llmRequest)}
                      >
                        {copiedPrompt ? '✓ Copied' : '📋 Copy Request'}
                      </button>
                    </div>
                    <pre className="json-pre">{JSON.stringify(step.llmRequest, null, 2)}</pre>
                  </div>

                  <div className="raw-json-col">
                    <div className="raw-col-header">
                      <h6>Response Payload (LLM Output):</h6>
                      <button
                        type="button"
                        className="mini-copy-btn"
                        onClick={() => onCopyResponse(step.llmResponse)}
                      >
                        {copiedResponse ? '✓ Copied' : '📋 Copy Response'}
                      </button>
                    </div>
                    <pre className="json-pre">{JSON.stringify(step.llmResponse, null, 2)}</pre>
                  </div>
                </div>
              </div>
            )}
          </div>
        )}

        {/* ================= 3. TOOL EXECUTION RESULTS ================= */}
        {step.toolResults && (step.toolResults as unknown[]).length > 0 && (
          <div className="step-tool-results-box">
            <div className="step-tool-results-header">
              <h5>Tool Execution Results:</h5>
              <button
                type="button"
                className="mini-copy-btn"
                onClick={() =>
                  onCopyMessageText(
                    `results-${stepIdx}`,
                    JSON.stringify(step.toolResults, null, 2),
                  )
                }
                title="Copy tool execution results"
              >
                {copiedMsgIdx === `results-${stepIdx}` ? '✓ Copied' : '📋 Copy Results'}
              </button>
            </div>
            <div className="step-tool-result-list">
              {parseToolResults(step.toolResults).map((result, rIdx) => (
                <div
                  key={result.id ?? `${result.name}-${rIdx}`}
                  className={`step-tool-result-item ${result.isError ? 'is-error' : 'is-ok'}`}
                >
                  <div className="step-tool-result-header">
                    {result.isError ? (
                      <AlertCircle size={13} className="tool-status-icon failed" />
                    ) : (
                      <CheckCircle2 size={13} className="tool-status-icon success" />
                    )}
                    <span className="step-tool-call-name">{result.name}</span>
                    <span className={`tool-status-badge ${result.isError ? 'failed' : 'completed'}`}>
                      {result.isError ? 'error' : 'ok'}
                    </span>
                    {result.output && (
                      <button
                        type="button"
                        className="mini-copy-btn"
                        onClick={() => onCopyMessageText(`result-${stepIdx}-${rIdx}`, result.output || '')}
                        title="Copy tool output"
                      >
                        {copiedMsgIdx === `result-${stepIdx}-${rIdx}` ? (
                          <Check size={11} className="text-success" />
                        ) : (
                          <Copy size={11} />
                        )}
                      </button>
                    )}
                  </div>
                  {result.output && <pre className="step-tool-result-output">{result.output}</pre>}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

// =========================================================================
// AGENT LOG TURN (CHAT-STYLE TRANSCRIPT)
// =========================================================================

interface AgentLogTurnCardProps {
  turn: AgentLogTurn
  isLatest: boolean
  promptOpen: boolean
  onTogglePrompt: () => void
  thinkingOpen: boolean
  onToggleThinking: () => void
  judgmentOpen: boolean
  onToggleJudgment: () => void
  expandedTools: Record<string, boolean>
  onToggleTool: (toolKey: string) => void
  copiedMsgIdx: string | null
  onCopyText: (id: string, text: string) => void
}

function AgentLogTurnCard({
  turn,
  isLatest,
  promptOpen,
  onTogglePrompt,
  thinkingOpen,
  onToggleThinking,
  judgmentOpen,
  onToggleJudgment,
  expandedTools,
  onToggleTool,
  copiedMsgIdx,
  onCopyText,
}: AgentLogTurnCardProps) {
  const isUser = turn.role === 'user'
  const author =
    isUser
      ? 'Task Prompt'
      : turn.kind === 'final_answer'
        ? 'OpenDuck Agent · Final Answer'
        : turn.kind === 'yield'
          ? 'OpenDuck Agent · Stopped'
          : turn.kind === 'request_input'
            ? 'OpenDuck Agent · Needs Input'
            : turn.kind === 'error'
              ? 'OpenDuck Agent · Error'
              : 'OpenDuck Agent'

  return (
    <article
      className={`message-bubble-row agent-log-turn ${isUser ? 'user-row' : 'assistant-row'} kind-${turn.kind}`}
      data-log-step={turn.stepIndex}
    >
      <div className="message-avatar">
        {isUser ? <User size={16} /> : <Bot size={16} />}
      </div>

      <div className="message-content-box">
        <div className="message-meta-header">
          <span className="author-name">{author}</span>
          {turn.stepIndex !== undefined && (
            <span className="agent-log-step-pill">Step #{turn.stepIndex}</span>
          )}
          {turn.timestamp && (
            <span className="agent-log-time">
              {new Date(turn.timestamp).toLocaleTimeString([], {
                hour: '2-digit',
                minute: '2-digit',
                second: '2-digit',
              })}
            </span>
          )}
          {turn.durationMs !== undefined && turn.durationMs > 0 && (
            <span className="agent-log-duration">{turn.durationMs} ms</span>
          )}
          {turn.judgments && turn.judgments.length > 0 && (
            <span
              className={`agent-log-judgment-pill ${
                turn.judgments.some(j => j.verdict?.triggered) ? 'triggered' : 'passed'
              }`}
              title={turn.judgments.map(j => `${j.point}: ${j.mode}`).join(', ')}
            >
              <Scale size={11} />
              <span>Laya ({turn.judgments.length})</span>
            </span>
          )}
          {isLatest && (
            <span className="streaming-badge">
              <span className="stream-dot" /> Live
            </span>
          )}
        </div>

        {/* 1. PROMPT SECTION (for assistant turns with added prompt telemetry) */}
        {!isUser && (turn.promptMessages || turn.lastPromptText) && (
          <div className={`agent-log-prompt ${promptOpen ? 'open' : ''}`}>
            <button
              type="button"
              className="agent-log-prompt-toggle"
              onClick={onTogglePrompt}
              title="Inspect prompt added for this step"
            >
              <Terminal size={13} />
              <span>Prompt {turn.promptMessages ? `(${turn.promptMessages.length} msg${turn.promptMessages.length === 1 ? '' : 's'})` : ''}</span>
              {promptOpen ? <ChevronDown size={13} /> : <ChevronRightIcon size={13} />}
            </button>
            {promptOpen && (
              <div className="agent-log-prompt-body">
                {turn.promptMessages && turn.promptMessages.length > 0 ? (
                  <div className="agent-log-prompt-messages">
                    {turn.promptMessages.map((msg, mIdx) => {
                      const isToolResult = Boolean(
                        msg.role === 'tool' || (msg.toolResults && msg.toolResults.length > 0)
                      )
                      const roleLabel = isToolResult ? 'tool result' : msg.role
                      const formattedText = formatPromptMessageText(msg)
                      return (
                        <div
                          key={msg.index ?? mIdx}
                          className={`agent-log-prompt-msg role-${msg.role} ${isToolResult ? 'role-tool' : ''}`}
                        >
                          <div className="agent-log-prompt-msg-role">{roleLabel}</div>
                          <div className="agent-log-prompt-msg-content">
                            <pre className="agent-log-prompt-msg-text">{formattedText}</pre>
                          </div>
                        </div>
                      )
                    })}
                  </div>
                ) : (
                  <pre className="agent-log-prompt-msg-text">{turn.lastPromptText}</pre>
                )}
              </div>
            )}
          </div>
        )}

        {/* 2. THINKING SECTION (internal model reasoning) */}
        {turn.thinking && (
          <div className={`agent-log-thinking ${thinkingOpen ? 'open' : ''}`}>
            <button type="button" className="agent-log-thinking-toggle" onClick={onToggleThinking} title="Inspect internal reasoning">
              <Sparkles size={13} />
              <span>Thinking</span>
              {thinkingOpen ? <ChevronDown size={13} /> : <ChevronRightIcon size={13} />}
            </button>
            {thinkingOpen && <pre className="agent-log-thinking-body">{turn.thinking}</pre>}
          </div>
        )}

        {/* 3. JUDGMENTS SECTION (System 1 Decision Engine / Laya) */}
        {turn.judgments && turn.judgments.length > 0 && (
          <div className={`agent-log-judgments ${judgmentOpen ? 'open' : ''}`}>
            <button
              type="button"
              className="agent-log-judgments-toggle"
              onClick={onToggleJudgment}
              title="Inspect Laya System 1 decision engine evaluations"
            >
              <Scale size={13} />
              <span>
                Laya Decisions ({turn.judgments.length})
              </span>
              <span className="agent-log-judgment-summary-pills">
                {turn.judgments.map((j, idx) => {
                  const triggered = Boolean(j.verdict?.triggered)
                  return (
                    <span
                      key={idx}
                      className={`judgment-summary-pill ${triggered ? 'is-triggered' : 'is-ok'} mode-${j.mode}`}
                    >
                      {j.point} · {triggered ? '⚠️ Triggered' : '✓ OK'} ({j.latencyMs}ms)
                    </span>
                  )
                })}
              </span>
              {judgmentOpen ? <ChevronDown size={13} /> : <ChevronRightIcon size={13} />}
            </button>
            {judgmentOpen && (
              <div className="agent-log-judgments-body">
                {turn.judgments.map((j, idx) => (
                  <AgentLogJudgmentDetailCard key={idx} judgment={j} />
                ))}
              </div>
            )}
          </div>
        )}

        {turn.text && (
          <div className={`message-body-text ${turn.kind === 'error' ? 'is-error' : ''} ${turn.kind === 'final_answer' ? 'is-final' : ''}`}>
            {isUser ? (
              turn.text
            ) : (
              <Suspense fallback={<div className="skeleton-line" style={{ height: '1.5rem' }} />}>
                <MarkdownPreview content={turn.text} />
              </Suspense>
            )}
          </div>
        )}

        {turn.toolCalls && turn.toolCalls.length > 0 && (
          <div className="tool-calls-container">
            <div className="tool-calls-title">
              <Wrench size={13} />
              <span>Tools ({turn.toolCalls.length})</span>
            </div>
            <div className="tool-calls-list">
              {turn.toolCalls.map((tool, tIdx) => (
                <AgentLogToolCard
                  key={tool.id ?? `${tool.name}-${tIdx}`}
                  tool={tool}
                  expanded={expandedTools[`${turn.id}-${tIdx}`] ?? false}
                  onToggle={() => onToggleTool(`${turn.id}-${tIdx}`)}
                  copied={copiedMsgIdx === `${turn.id}-${tIdx}`}
                  onCopy={() =>
                    onCopyText(
                      `${turn.id}-${tIdx}`,
                      [
                        tool.name,
                        tool.arguments !== undefined ? formatToolArguments(tool.arguments) : '',
                        tool.output ?? '',
                      ]
                        .filter(Boolean)
                        .join('\n\n'),
                    )
                  }
                />
              ))}
            </div>
          </div>
        )}
      </div>
    </article>
  )
}

function AgentLogToolCard({
  tool,
  expanded,
  onToggle,
  copied,
  onCopy,
}: {
  tool: AgentLogToolCall
  expanded: boolean
  onToggle: () => void
  copied: boolean
  onCopy: () => void
}) {
  const pending = tool.output === undefined && !tool.isError
  const failed = Boolean(tool.isError)
  const description = describeToolCall(tool.name, tool.arguments)

  return (
    <div className={`tool-call-card ${expanded ? 'expanded' : ''} ${failed ? 'failed' : pending ? 'pending' : 'completed'}`}>
      <div className="tool-call-header" onClick={onToggle} role="button" tabIndex={0}>
        <div className="tool-header-left agent-log-tool-header-left">
          {failed && <AlertCircle size={14} className="tool-status-icon failed" />}
          {!failed && pending && <Terminal size={14} className="tool-status-icon running" />}
          {!failed && !pending && <CheckCircle2 size={14} className="tool-status-icon success" />}
          <div className="agent-log-tool-identity">
            <span className="tool-name">{tool.name}</span>
            {description && (
              <span className="agent-log-tool-desc" title={description}>
                {description}
              </span>
            )}
          </div>
        </div>
        <div className="tool-header-right">
          <span className={`tool-status-badge ${failed ? 'failed' : pending ? 'pending' : 'completed'}`}>
            {failed ? 'error' : pending ? 'running' : 'ok'}
          </span>
          {expanded ? <ChevronDown size={14} /> : <ChevronRightIcon size={14} />}
        </div>
      </div>
      {expanded && (
        <div className="tool-call-body agent-log-tool-body">
          {tool.arguments !== undefined && (
            <div className="agent-log-tool-section">
              <div className="agent-log-tool-section-header">
                <span className="tool-label">Arguments</span>
                <button type="button" className="mini-copy-btn" onClick={onCopy} title="Copy tool call">
                  {copied ? <Check size={11} className="text-success" /> : <Copy size={11} />}
                </button>
              </div>
              <pre className="step-tool-call-args">{formatToolArguments(tool.arguments)}</pre>
            </div>
          )}
          {tool.output !== undefined && (
            <div className="agent-log-tool-section">
              <span className="tool-label">{failed ? 'Error output' : 'Output'}</span>
              <pre className={`step-tool-result-output ${failed ? 'is-error' : ''}`}>{tool.output}</pre>
            </div>
          )}
          {pending && <div className="agent-log-tool-pending">Waiting for tool result…</div>}
        </div>
      )}
    </div>
  )
}

function AgentLogJudgmentDetailCard({ judgment }: { judgment: HarnessJudgmentRecord }) {
  const triggered = Boolean(judgment.verdict?.triggered)
  const verdictKind = judgment.verdict?.kind || 'unknown'
  const scoreVal =
    judgment.verdict?.value !== undefined ? judgment.verdict.value.toFixed(4) : undefined
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
          {triggered ? '⚠️ Triggered' : '✓ Passed'}
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
