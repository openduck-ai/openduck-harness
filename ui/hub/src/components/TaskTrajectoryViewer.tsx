import { useState, useMemo, useEffect, useLayoutEffect, useRef, useCallback, Suspense } from 'react'
import type {
  HarnessRunResponse,
  HarnessActiveJob,
  createApi,
} from '@aaif/goose-hub-core'
import {
  parseAgentAction,
  extractFinalAnswer,
  formatToolArguments,
  extractLlmTelemetry,
  filterTrajectorySteps,
  calculateTrajectoryMetrics,
  downloadTrajectoryJson,
  buildAgentLog,
  filterAgentLogTurns,
  parseToolResults,
  type TrajectoryFilterType,
  type TrajectoryViewMode,
  agentLogScrollKey,
  evaluateAutoScrollState,
  dockRevealDelta,
} from '../taskTrajectory.ts'
import { isContinuableStatus, isHistoryFollowUpStatus } from '../taskContinuation.ts'
import { MarkdownPreview } from './MarkdownPreview.tsx'
import { AgentLogTranscript } from './AgentLogTranscript.tsx'
import { FilePreviewModal } from './FilePreviewModal.tsx'
import { ProviderStatsDashboard } from './ProviderStatsDashboard.tsx'
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
  CheckCircle2,
  AlertCircle,
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
  backLabel?: string
  onContinueTask?: (taskId: string, extraTurns: number, extraPrompt?: string) => Promise<void>
  maxTurnsLimit?: number
  onOpenContinueModal?: (
    taskId: string,
    previousSteps: number,
    taskName?: string,
    runId?: string,
    progressSummary?: string,
  ) => void
}

type TelemetryTab = 'prompt' | 'response' | 'thinking' | 'tokens' | 'raw' | 'provider_calls'
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
  backLabel,
  onContinueTask,
  onOpenContinueModal,
  maxTurnsLimit = 500,
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
  const [copiedRunId, setCopiedRunId] = useState(false)
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
  const programmaticGenerationRef = useRef(0)
  const userExplicitlyDisabledRef = useRef(false)
  const userPagedAwayRef = useRef(false)
  const userScrollIntentRef = useRef(false)
  const userScrollIntentTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const scrollbarDragRef = useRef(false)
  const liveFollowKey = liveInspect ? `${inspectingJobId ?? ''}:${run.taskId}` : ''
  const liveFollowKeyRef = useRef(liveFollowKey)
  const [autoScroll, setAutoScroll] = useState(true)
  const autoScrollRef = useRef(true)
  autoScrollRef.current = autoScroll

  const steps = useMemo(() => run.trajectory?.steps ?? [], [run.trajectory?.steps])
  const metrics = useMemo(() => calculateTrajectoryMetrics(run), [run])
  const agentLog = useMemo(() => buildAgentLog(run), [run])
  const filteredLogTurns = useMemo(
    () => filterAgentLogTurns(agentLog, filterType, searchQuery),
    [agentLog, filterType, searchQuery],
  )
  const logScrollKey = useMemo(() => agentLogScrollKey(filteredLogTurns), [filteredLogTurns])
  const showAgentLog = viewMode === 'log' && (steps.length > 0 || filteredLogTurns.length > 0)
  if (liveFollowKeyRef.current !== liveFollowKey) {
    liveFollowKeyRef.current = liveFollowKey
    if (liveInspect) {
      userPagedAwayRef.current = false
      userExplicitlyDisabledRef.current = false
      autoScrollRef.current = true
      if (!autoScroll) setAutoScroll(true)
    }
  }

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

  const pinLogToBottom = useCallback((revealDock: boolean) => {
    const el = agentLogTranscriptRef.current
    if (!el) {
      agentLogEndRef.current?.scrollIntoView({ behavior: 'auto', block: 'end' })
      return
    }
    // An upward read suppresses the pin until the gesture settles.
    if (userScrollIntentRef.current) return

    isProgrammaticScrollRef.current = true
    const generation = ++programmaticGenerationRef.current
    if (programmaticScrollTimerRef.current) {
      clearTimeout(programmaticScrollTimerRef.current)
    }
    const pinInner = () => {
      el.scrollTop = el.scrollHeight
    }
    pinInner()
    if (revealDock && !userPagedAwayRef.current) {
      const panel = el.closest('.agent-log-panel')
      const dock = panel?.querySelector('.agent-log-dock')
      const target = dock instanceof HTMLElement ? dock : panel
      if (target instanceof HTMLElement) {
        const rect = target.getBoundingClientRect()
        const delta = dockRevealDelta(rect.bottom, window.innerHeight)
        if (delta > 0) {
          const beforeY = window.scrollY
          const beforeTop = rect.top
          window.scrollBy(0, delta)
          const next = target.getBoundingClientRect()
          // A stuck panel does not move with the page. Undo a scroll that
          // left the dock below the fold so later tokens do not walk the document.
          if (
            window.scrollY !== beforeY &&
            Math.abs(next.top - beforeTop) < 1 &&
            dockRevealDelta(next.bottom, window.innerHeight) > 0
          ) {
            window.scrollTo(0, beforeY)
          }
        }
      }
      pinInner()
    }
    programmaticScrollTimerRef.current = setTimeout(() => {
      if (programmaticGenerationRef.current === generation) {
        isProgrammaticScrollRef.current = false
      }
    }, 32)
  }, [])

  const settleTranscriptFollow = useCallback(() => {
    const node = agentLogTranscriptRef.current
    if (!node) return
    const distanceFromBottom = node.scrollHeight - node.scrollTop - node.clientHeight
    const evaluation = evaluateAutoScrollState({
      distanceFromBottom,
      currentAutoScroll: autoScrollRef.current,
      userExplicitlyDisabled: userExplicitlyDisabledRef.current,
    })
    if (evaluation.autoScroll && !autoScrollRef.current) {
      userPagedAwayRef.current = false
    }
    if (evaluation.autoScroll !== autoScrollRef.current) {
      autoScrollRef.current = evaluation.autoScroll
      setAutoScroll(evaluation.autoScroll)
    }
    if (evaluation.autoScroll) pinLogToBottom(true)
  }, [pinLogToBottom])

  // The latest line lives at the bottom of the log panel. Keep that edge on
  // screen while the run is live, including thinking text that grows in place.
  useLayoutEffect(() => {
    if (!liveInspect || viewMode !== 'log' || !autoScroll) return
    const el = agentLogTranscriptRef.current
    if (!el) return

    const pin = () => {
      if (!autoScrollRef.current) return
      pinLogToBottom(true)
    }
    pin()

    const content = el.querySelector(':scope > .agent-transcript-content')
    if (!content || typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(() => pin())
    observer.observe(content)
    return () => observer.disconnect()
  }, [liveInspect, viewMode, autoScroll, logScrollKey, pinLogToBottom])

  useEffect(() => {
    if (!liveInspect) return
    let lastY = window.scrollY
    const onWindowScroll = () => {
      const nextY = window.scrollY
      if (!isProgrammaticScrollRef.current && nextY < lastY - 48) {
        userPagedAwayRef.current = true
      }
      lastY = nextY
    }
    window.addEventListener('scroll', onWindowScroll, { passive: true })
    return () => window.removeEventListener('scroll', onWindowScroll)
  }, [liveInspect, inspectingJobId, run.taskId])

  useEffect(() => {
    const el = agentLogTranscriptRef.current
    if (!el || !showAgentLog) return

    let accumUp = 0
    const armGesture = () => {
      userScrollIntentRef.current = true
      if (userScrollIntentTimerRef.current) clearTimeout(userScrollIntentTimerRef.current)
      userScrollIntentTimerRef.current = setTimeout(() => {
        userScrollIntentRef.current = false
        accumUp = 0
        if (!scrollbarDragRef.current) settleTranscriptFollow()
      }, 200)
    }
    const wheelPixels = (event: WheelEvent) => {
      if (event.deltaMode === WheelEvent.DOM_DELTA_LINE) return event.deltaY * 16
      if (event.deltaMode === WheelEvent.DOM_DELTA_PAGE) return event.deltaY * el.clientHeight
      return event.deltaY
    }
    const onWheel = (event: WheelEvent) => {
      const dy = wheelPixels(event)
      if (dy < 0) {
        accumUp += -dy
        armGesture()
        const distance = el.scrollHeight - el.scrollTop - el.clientHeight + accumUp
        if (distance > 60 && autoScrollRef.current) {
          autoScrollRef.current = false
          setAutoScroll(false)
        }
        return
      }
      accumUp = 0
      if (dy > 0 && !autoScrollRef.current) armGesture()
    }
    let touchStartY = 0
    const onTouchStart = (event: TouchEvent) => {
      touchStartY = event.touches[0]?.clientY ?? 0
    }
    const onTouchMove = (event: TouchEvent) => {
      const y = event.touches[0]?.clientY ?? touchStartY
      if (y > touchStartY + 4) armGesture()
    }
    const onPointerDown = (event: PointerEvent) => {
      const scrollbar = event.offsetX > el.clientWidth || event.offsetY > el.clientHeight
      if (!scrollbar) return
      scrollbarDragRef.current = true
      userScrollIntentRef.current = true
    }
    const onPointerUp = () => {
      if (!scrollbarDragRef.current) return
      scrollbarDragRef.current = false
      userScrollIntentRef.current = false
      settleTranscriptFollow()
    }

    el.addEventListener('wheel', onWheel, { passive: true })
    el.addEventListener('touchstart', onTouchStart, { passive: true })
    el.addEventListener('touchmove', onTouchMove, { passive: true })
    el.addEventListener('pointerdown', onPointerDown)
    window.addEventListener('pointerup', onPointerUp)
    window.addEventListener('pointercancel', onPointerUp)
    return () => {
      el.removeEventListener('wheel', onWheel)
      el.removeEventListener('touchstart', onTouchStart)
      el.removeEventListener('touchmove', onTouchMove)
      el.removeEventListener('pointerdown', onPointerDown)
      window.removeEventListener('pointerup', onPointerUp)
      window.removeEventListener('pointercancel', onPointerUp)
      if (userScrollIntentTimerRef.current) clearTimeout(userScrollIntentTimerRef.current)
    }
  }, [showAgentLog, settleTranscriptFollow])

  // Detect user scroll in the log transcript container
  const handleLogScroll = useCallback(() => {
    const el = agentLogTranscriptRef.current
    if (!el || isProgrammaticScrollRef.current) return

    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight
    const evaluation = evaluateAutoScrollState({
      distanceFromBottom,
      currentAutoScroll: autoScrollRef.current,
      userExplicitlyDisabled: userExplicitlyDisabledRef.current,
    })

    if (evaluation.autoScroll && !autoScrollRef.current) {
      userPagedAwayRef.current = false
    }
    if (evaluation.autoScroll !== autoScrollRef.current) {
      autoScrollRef.current = evaluation.autoScroll
      setAutoScroll(evaluation.autoScroll)
    }
  }, [])

  // Floating resume button click handler
  const handleResumeAutoScroll = useCallback(() => {
    userExplicitlyDisabledRef.current = false
    userPagedAwayRef.current = false
    autoScrollRef.current = true
    setAutoScroll(true)
    pinLogToBottom(true)
  }, [pinLogToBottom])

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
      const typing = Boolean(
        target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable),
      )
      const inSearch = target?.classList.contains('trajectory-search-input') ?? false
      if (e.key === 'Escape' && searchQuery && (inSearch || !typing)) {
        e.preventDefault()
        setSearchQuery('')
        return
      }
      if (typing || viewMode === 'log') {
        return
      }

      if (e.key === 'j' || e.key === 'ArrowDown' || e.key === 'ArrowRight') {
        e.preventDefault()
        setSelectedStepIdx(prev => Math.min(steps.length - 1, prev + 1))
      } else if (e.key === 'k' || e.key === 'ArrowUp' || e.key === 'ArrowLeft') {
        e.preventDefault()
        setSelectedStepIdx(prev => Math.max(0, prev - 1))
      } else if (e.key === 'f') {
        const finalIdx = steps.findIndex(s => parseAgentAction(s.agentAction).kind === 'final_answer')
        if (finalIdx !== -1) {
          e.preventDefault()
          setSelectedStepIdx(finalIdx)
        }
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [steps, searchQuery, viewMode])

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
    autoScrollRef.current = false
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

  const toolStepCount = steps.filter(step => parseAgentAction(step.agentAction).kind === 'call_tools').length
  const answerStepCount = steps.filter(step => parseAgentAction(step.agentAction).kind === 'final_answer').length
  const stopStepCount = steps.filter(step => {
    const kind = parseAgentAction(step.agentAction).kind
    return kind === 'yield_control' || kind === 'request_input'
  }).length
  const telemetryStepCount = steps.filter(step => Boolean(step.llmRequest || step.llmResponse || step.tokenUsage)).length

  const finalAnswer = extractFinalAnswer(run)
  const mentionedFiles = useMemo(() => {
    return finalAnswer ? extractMentionedFiles(finalAnswer) : []
  }, [finalAnswer])
  const systemPrompt = run.systemPrompt || run.trajectory?.systemPrompt
  const activeRules = run.activeRules || run.trajectory?.activeRules

  return (
    <div className={`trajectory-viewer-container${viewMode === 'log' ? ' is-log-mode' : ''}`}>
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
                <span className="meta-pill run-id-pill">
                  Run ID: <code>{selectedHistoryId}</code>
                  <button
                    type="button"
                    className="history-copy-btn run-id-copy-btn"
                    onClick={async e => {
                      e.stopPropagation()
                      try {
                        await navigator.clipboard.writeText(selectedHistoryId)
                        setCopiedRunId(true)
                        setTimeout(() => setCopiedRunId(false), 2000)
                      } catch {
                        // ignore clipboard error
                      }
                    }}
                    title="Copy Run ID"
                    aria-label="Copy Run ID"
                  >
                    {copiedRunId ? <Check size={11} /> : <Copy size={11} />}
                  </button>
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
              title="Return to origin screen"
            >
              ← {backLabel || 'Back to tasks'}
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
          {metrics.providerStats && metrics.providerStats.totalCalls > 0 && (
            <div
              className={`metric-item clickable-metric ${viewMode === 'providers' ? 'active' : ''}`}
              onClick={() => setViewMode('providers')}
              title="Click to view LLM provider call statistics and ledger"
              role="button"
              tabIndex={0}
            >
              <span className="metric-label">Provider Calls</span>
              <div className="metric-value provider-calls-val">
                <span>{metrics.providerStats.totalCalls}</span>
                {metrics.providerStats.failedCalls > 0 && (
                  <span className="metric-badge err" title={`${metrics.providerStats.failedCalls} failed calls`}>
                    {metrics.providerStats.failedCalls} failed
                  </span>
                )}
                {metrics.providerStats.totalRetries > 0 && (
                  <span className="metric-badge warn" title={`${metrics.providerStats.totalRetries} retries`}>
                    {metrics.providerStats.totalRetries} retry
                  </span>
                )}
              </div>
            </div>
          )}
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
          {metrics.providerStats && Object.keys(metrics.providerStats.byProvider).length > 0 && (
            <div className="metric-item provider-breakdown-item">
              <span className="metric-label">Providers</span>
              <div className="tool-freq-chips">
                {Object.entries(metrics.providerStats.byProvider).map(([pName, pStat]) => (
                  <button
                    key={pName}
                    type="button"
                    className="tool-freq-chip provider-chip"
                    onClick={() => setViewMode('providers')}
                    title={`${pName}: ${pStat.totalCalls} calls (${pStat.successfulCalls} ok, ${pStat.failedCalls} fail, ${pStat.retries} retries), avg ${Math.round(pStat.avgDurationMs)}ms`}
                  >
                    <code>{pName}</code> ×{pStat.totalCalls}
                    {pStat.failedCalls > 0 && <span className="chip-badge err">!{pStat.failedCalls}</span>}
                    {pStat.retries > 0 && <span className="chip-badge warn">↺{pStat.retries}</span>}
                  </button>
                ))}
              </div>
            </div>
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
      {viewMode !== 'log' && isContinuableStatus(String(run.status)) && !liveInspect && onContinueTask && onOpenContinueModal && (
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
                max={maxTurnsLimit}
                className="banner-turns-input"
                value={bannerCustomTurns}
                onChange={e =>
                  setBannerCustomTurns(
                    Math.max(1, Math.min(maxTurnsLimit, parseInt(e.target.value, 10) || 1)),
                  )
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
      {viewMode !== 'log' && (systemPrompt || (activeRules && activeRules.length > 0)) && (
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

      {/* Final Answer Banner — Split and Stream. Log mode shows the answer in the transcript. */}
      {viewMode !== 'log' && finalAnswer && (
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
            <button
              type="button"
              className={`view-mode-btn ${viewMode === 'providers' ? 'active' : ''}`}
              onClick={() => setViewMode('providers')}
              title="LLM Provider Calls & Performance Statistics"
            >
              <Zap size={14} /> Provider Stats{metrics.providerStats?.totalCalls ? ` (${metrics.providerStats.totalCalls})` : ''}
            </button>
          </div>

          {viewMode === 'log' ? (
            <label className="agent-log-filter">
              <Filter size={14} aria-hidden="true" />
              <select
                aria-label="Filter log"
                value={filterType}
                onChange={event => setFilterType(event.target.value as TrajectoryFilterType)}
              >
                <option value="all">All ({steps.length})</option>
                <option value="tools">Tools ({toolStepCount})</option>
                <option value="final_answer">Answer ({answerStepCount})</option>
                <option value="yield">Stops ({stopStepCount})</option>
                <option value="telemetry">Telemetry ({telemetryStepCount})</option>
              </select>
            </label>
          ) : (
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
                🛠️ Tools ({toolStepCount})
              </button>
              <button
                type="button"
                className={`filter-pill ${filterType === 'final_answer' ? 'active' : ''}`}
                onClick={() => setFilterType('final_answer')}
              >
                🏁 Answer ({answerStepCount})
              </button>
              <button
                type="button"
                className={`filter-pill ${filterType === 'yield' ? 'active' : ''}`}
                onClick={() => setFilterType('yield')}
              >
                ⏸️ Stops ({stopStepCount})
              </button>
              <button
                type="button"
                className={`filter-pill ${filterType === 'telemetry' ? 'active' : ''}`}
                onClick={() => setFilterType('telemetry')}
              >
                🤖 Telemetry ({telemetryStepCount})
              </button>
            </div>
          )}

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

        {viewMode !== 'log' && (
        <>
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
        </>
        )}
      </div>

      {/* ================= EMPTY STATE ================= */}
      {steps.length === 0 && !showAgentLog && (
        <div className="empty-trajectory-card">
          <p className="empty-text">
            {liveInspect
              ? 'Waiting for the first agent step… this view refreshes automatically while the task is running.'
              : 'No trajectory steps recorded for this run.'}
          </p>
        </div>
      )}

      {showAgentLog && (
        <AgentLogTranscript
          turns={filteredLogTurns}
          live={liveInspect}
          status={String(run.status)}
          sessionKey={`${run.taskId}:${selectedHistoryId ?? ''}:${String(run.status)}:${steps.length}`}
          copiedId={copiedMsgIdx}
          onCopyText={handleCopyMessageText}
          onOpenFile={setPreviewFilePath}
          transcriptRef={agentLogTranscriptRef}
          endRef={agentLogEndRef}
          onScroll={handleLogScroll}
          showLatest={liveInspect && !autoScroll}
          onLatest={handleResumeAutoScroll}
          mentionedFiles={mentionedFiles}
          resolveImageUrl={src =>
            api && effectiveProjectSlug
              ? api.getFileBytesUrl(effectiveProjectSlug, resolveProjectAssetPath('', src))
              : src
          }
          dynamicTasks={dynamicTasks}
          selectionNotice={selectionRunNotice}
          selectionBusy={selectionRunBusy}
          selectionEnabled={Boolean(api && effectiveProjectSlug)}
          api={api}
          projectSlug={effectiveProjectSlug}
          onSelectDynamicTask={(task, text) => {
            void runWithSelection(task, formatSelectionExtraPrompt('Final Agent Answer', text))
          }}
          continueRequest={
            onContinueTask &&
            !liveInspect &&
            (isContinuableStatus(String(run.status)) || isHistoryFollowUpStatus(String(run.status)))
              ? {
                  busy: runningTaskId === run.taskId,
                  mode: isHistoryFollowUpStatus(String(run.status)) ? 'history' : 'summary',
                  summary: isHistoryFollowUpStatus(String(run.status))
                    ? undefined
                    : run.continuation?.compactedSummary,
                  onContinue: (extraTurns, extraPrompt) => onContinueTask(run.taskId, extraTurns, extraPrompt),
                  onSpecifiedTurns:
                    isHistoryFollowUpStatus(String(run.status)) || !onOpenContinueModal
                      ? undefined
                      : () =>
                          onOpenContinueModal(
                            run.taskId,
                            metrics.totalSteps,
                            undefined,
                            selectedHistoryId ?? undefined,
                            run.continuation?.compactedSummary ?? undefined,
                          ),
                }
              : undefined
          }
        />
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

      {/* ================= VIEW MODE 4: PROVIDER CALL STATS DASHBOARD ================= */}
      {viewMode === 'providers' && (
        <ProviderStatsDashboard
          run={run}
          providerStats={metrics.providerStats}
          onJumpToStep={stepIdx => {
            setSelectedStepIdx(stepIdx)
            setViewMode('split')
          }}
        />
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
                {telemetry?.providerCalls && telemetry.providerCalls.length > 0 && (
                  <button
                    type="button"
                    className={`telemetry-tab-btn ${activeTelemetryTab === 'provider_calls' ? 'active' : ''}`}
                    onClick={() => setActiveTelemetryTab('provider_calls')}
                  >
                    <Zap size={13} /> Provider Calls ({telemetry.providerCalls.length})
                  </button>
                )}
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

            {/* TAB PANEL 6: PROVIDER CALL ATTEMPTS */}
            {activeTelemetryTab === 'provider_calls' && telemetry?.providerCalls && (
              <div className="telemetry-panel provider-calls-panel">
                <div className="step-provider-calls-list">
                  {telemetry.providerCalls.map((call, cIdx) => (
                    <div
                      key={`call-${cIdx}-${call.provider}`}
                      className={`step-provider-call-card ${call.success ? 'success' : 'failed'}`}
                    >
                      <div className="provider-call-card-header">
                        <div className="call-title-group">
                          <span className="call-attempt-badge">
                            Call #{cIdx + 1}
                            {call.retryAttempt !== undefined && call.retryAttempt !== null && call.retryAttempt > 0 && (
                              <span className="call-retry-pill">Retry #{call.retryAttempt}</span>
                            )}
                            {call.isSecondary && <span className="call-secondary-pill">Secondary Failover</span>}
                          </span>
                          <span className="call-provider-tag">
                            <strong>{call.provider}</strong>
                            {call.model && <code className="call-model-tag">{call.model}</code>}
                          </span>
                        </div>
                        <span className={`call-status-badge ${call.success ? 'ok' : 'err'}`}>
                          {call.success ? '✓ Success' : '✕ Failed'}
                        </span>
                      </div>

                      <div className="provider-call-metrics-row">
                        <div className="call-mini-metric">
                          <span className="mini-label">Latency</span>
                          <span className="mini-val font-mono">{call.latencyMs.toLocaleString()} ms</span>
                        </div>
                        {call.tokenUsage && (
                          <>
                            <div className="call-mini-metric">
                              <span className="mini-label">Input Tokens</span>
                              <span className="mini-val font-mono">
                                {(call.tokenUsage.inputTokens ?? 0).toLocaleString()}
                              </span>
                            </div>
                            <div className="call-mini-metric">
                              <span className="mini-label">Output Tokens</span>
                              <span className="mini-val font-mono">
                                {(call.tokenUsage.outputTokens ?? 0).toLocaleString()}
                              </span>
                            </div>
                            <div className="call-mini-metric">
                              <span className="mini-label">Total Tokens</span>
                              <span className="mini-val font-mono total">
                                {(
                                  call.tokenUsage.totalTokens ??
                                  (call.tokenUsage.inputTokens ?? 0) + (call.tokenUsage.outputTokens ?? 0)
                                ).toLocaleString()}
                              </span>
                            </div>
                          </>
                        )}
                        {call.cost !== undefined && call.cost !== null && (
                          <div className="call-mini-metric">
                            <span className="mini-label">Est. Cost</span>
                            <span className="mini-val font-mono">${call.cost.toFixed(5)}</span>
                          </div>
                        )}
                      </div>

                      {call.retryReason && (
                        <div className="call-retry-reason-box">
                          <strong>Retry Reason:</strong> {call.retryReason}
                        </div>
                      )}

                      {call.error && (
                        <div className="call-error-box">
                          <strong>Error:</strong> {call.error}
                        </div>
                      )}
                    </div>
                  ))}
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
