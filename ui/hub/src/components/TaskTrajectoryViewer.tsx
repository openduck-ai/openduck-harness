import { useState, useMemo, useEffect, useLayoutEffect, useRef, useCallback, Suspense } from 'react'
import { Trans, useTranslation } from 'react-i18next'
import { formatNumber, formatTime } from '../i18n/format.ts'
import type {
  HarnessRunResponse,
  HarnessActiveJob,
  createApi,
} from '@aaif/goose-hub-core'
import {
  parseAgentAction,
  extractFinalAnswer,
  extractFailureReason,
  formatToolArguments,
  extractLlmTelemetry,
  filterTrajectorySteps,
  calculateTrajectoryMetrics,
  downloadTrajectoryJson,
  buildAgentLog,
  filterAgentLogTurns,
  paginateAgentLogTurns,
  parseToolResults,
  extractCompletedSubtasks,
  type AgentLogPagination,
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
import { exportTaskPdf } from '../taskPdfExport.ts'
import {
  Search,
  SlidersHorizontal,
  ChevronLeft,
  ChevronRight,
  ChevronDown,
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
  FileText,
  Printer,
  ExternalLink,
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
  selectedHistoryFileName?: string | null
  runningTaskId?: string | null
  stoppingTaskId?: string | null
  activeJobs?: HarnessActiveJob[]
  onStopTask?: (taskId: string) => Promise<void>
  onStopJob?: (job: HarnessActiveJob) => Promise<void>
  onBackToTasks: () => void
  backLabel?: string
  onContinueTask?: (taskId: string, extraTurns: number, extraPrompt?: string) => Promise<void>
  maxTurnsLimit?: number
  onPageChange?: (page: number, pageSize: number) => Promise<void>
  onOpenContinueModal?: (
    taskId: string,
    previousSteps: number,
    taskName?: string,
    runId?: string,
    progressSummary?: string,
    resumeSubtaskId?: string,
    resumeSubtaskTitle?: string,
    failureReason?: string,
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
  selectedHistoryFileName,
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
  onPageChange,
}: TaskTrajectoryViewerProps) {
  const { t, i18n } = useTranslation()
  const statusLabel = (status: string) => {
    switch (status.toLowerCase()) {
      case 'success':
        return t('trajectory.viewer.statusSuccess')
      case 'failure':
        return t('trajectory.viewer.statusFailure')
      case 'timeout':
        return t('trajectory.viewer.statusTimeout')
      case 'error':
        return t('trajectory.viewer.statusError')
      case 'running':
        return t('trajectory.viewer.statusRunning')
      case 'cancelled':
        return t('trajectory.viewer.statusCancelled')
      case 'skipped':
        return t('trajectory.viewer.statusSkipped')
      default:
        return status.toUpperCase()
    }
  }
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

  // PDF Export dropdown state
  const [exportMenuOpen, setExportMenuOpen] = useState(false)
  const exportMenuRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!exportMenuOpen) return
    const handleClickOutside = (e: MouseEvent) => {
      if (exportMenuRef.current && !exportMenuRef.current.contains(e.target as Node)) {
        setExportMenuOpen(false)
      }
    }
    document.addEventListener('pointerdown', handleClickOutside)
    return () => document.removeEventListener('pointerdown', handleClickOutside)
  }, [exportMenuOpen])

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
  const agentLog = useMemo(() => buildAgentLog(run), [run, i18n.language])
  const completedSubtasks = useMemo(() => extractCompletedSubtasks(run), [run, i18n.language])
  const filteredLogTurns = useMemo(
    () => filterAgentLogTurns(agentLog, filterType, searchQuery),
    [agentLog, filterType, searchQuery, i18n.language],
  )
  const [logPage, setLogPage] = useState(() => run.pagination?.page ?? 1)
  const [logPageSize, setLogPageSize] = useState<number>(() => {
    if (run.pagination?.limit) {
      return run.pagination.limit
    }
    try {
      const saved = localStorage.getItem('openduck.agentLog.pageSize')
      if (saved !== null) {
        const parsed = parseInt(saved, 10)
        if (!Number.isNaN(parsed) && [25, 50, 100, 0].includes(parsed)) {
          return parsed
        }
      }
    } catch {
      // Ignore
    }
    return 50
  })
  const [loadingPage, setLoadingPage] = useState(false)

  useEffect(() => {
    if (run.pagination) {
      setLogPage(run.pagination.page)
      setLogPageSize(run.pagination.limit)
    }
  }, [run.pagination?.page, run.pagination?.limit])

  const logPagination: AgentLogPagination = useMemo(() => {
    if (run.pagination) {
      const page = run.pagination.page
      const pageSize = run.pagination.limit
      const totalTurns = run.pagination.total
      const totalPages = Math.max(1, run.pagination.totalPages)
      const startIndex = (page - 1) * pageSize
      const endIndex = Math.min(page * pageSize, totalTurns)
      return {
        page,
        pageSize,
        totalTurns,
        totalPages,
        startIndex,
        endIndex,
      }
    }
    return paginateAgentLogTurns(filteredLogTurns, logPage, logPageSize).pagination
  }, [run.pagination, filteredLogTurns, logPage, logPageSize])

  const pagedLogTurns = useMemo(() => {
    if (run.pagination) {
      return filteredLogTurns
    }
    return paginateAgentLogTurns(filteredLogTurns, logPage, logPageSize).pagedTurns
  }, [run.pagination, filteredLogTurns, logPage, logPageSize])

  useEffect(() => {
    if (liveInspect && autoScrollRef.current) {
      if (logPagination.totalPages !== logPage) {
        setLogPage(logPagination.totalPages)
      }
    } else if (logPage > logPagination.totalPages) {
      setLogPage(logPagination.totalPages)
    }
  }, [liveInspect, logPagination.totalPages, logPage])

  useEffect(() => {
    if (!run.pagination) {
      setLogPage(1)
    }
  }, [filterType, searchQuery, run.pagination])

  const handleLogPageSizeChange = useCallback(async (newSize: number) => {
    setLogPageSize(newSize)
    setLogPage(1)
    try {
      localStorage.setItem('openduck.agentLog.pageSize', String(newSize))
    } catch {
      // Ignore
    }
    if (onPageChange) {
      setLoadingPage(true)
      try {
        await onPageChange(1, newSize)
      } finally {
        setLoadingPage(false)
      }
    }
  }, [onPageChange])

  const handleLogPageChange = useCallback(async (target: number) => {
    const clamped = Math.min(Math.max(1, target), logPagination.totalPages)
    setLogPage(clamped)
    if (liveInspect && clamped < logPagination.totalPages) {
      autoScrollRef.current = false
      setAutoScroll(false)
      userPagedAwayRef.current = true
    }
    if (agentLogTranscriptRef.current) {
      agentLogTranscriptRef.current.scrollTop = 0
    }
    if (onPageChange && (run.pagination || logPageSize > 0)) {
      setLoadingPage(true)
      try {
        await onPageChange(clamped, logPageSize)
      } finally {
        setLoadingPage(false)
      }
    }
  }, [logPagination.totalPages, liveInspect, onPageChange, run.pagination, logPageSize])

  const logScrollKey = useMemo(() => agentLogScrollKey(pagedLogTurns), [pagedLogTurns])
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
    [steps, filterType, searchQuery, i18n.language],
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
    setLogPage(logPagination.totalPages)
    pinLogToBottom(true)
  }, [logPagination.totalPages, pinLogToBottom])

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

  const handleExportPdf = useCallback(
    async (method: 'print' | 'download' | 'preview' = 'print') => {
      let targetRun = run
      if (
        run.pagination &&
        api &&
        effectiveProjectSlug &&
        (selectedHistoryFileName || selectedHistoryId)
      ) {
        try {
          targetRun = await api.getProjectHistoryDetail(
            effectiveProjectSlug,
            selectedHistoryFileName || selectedHistoryId!,
          )
        } catch {
          // Fall back to current page
        }
      }
      exportTaskPdf(targetRun, {
        taskName: selectedHistoryFileName || undefined,
        projectSlug: effectiveProjectSlug,
        method,
      })
    },
    [run, api, effectiveProjectSlug, selectedHistoryFileName, selectedHistoryId],
  )

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
  const failureReason = useMemo(() => extractFailureReason(run), [run, i18n.language])
  const resumeSubtaskId = run.continuation?.resumeSubtaskId
  const resumeSubtask = useMemo(() => {
    if (!resumeSubtaskId || !Array.isArray(run.continuation?.subtaskPlan)) return undefined
    return run.continuation.subtaskPlan.find(s => s.id === resumeSubtaskId)
  }, [resumeSubtaskId, run.continuation?.subtaskPlan])
  const resumeSubtaskTitle = resumeSubtask?.title || resumeSubtaskId
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
              {liveInspect
                ? t(viewMode === 'log' ? 'trajectory.viewer.liveLogTitle' : 'trajectory.viewer.liveTrajectoryTitle')
                : t(viewMode === 'log' ? 'trajectory.viewer.agentLogTitle' : 'trajectory.viewer.executionTrajectoryTitle')}{' '}
              <span className="task-id-highlight">{run.taskId}</span>
              {liveInspect && (
                <span className="live-badge">
                  <span className="live-dot" /> {t('trajectory.viewer.liveRunning')}
                </span>
              )}
            </h3>
            <div className="trajectory-meta-row">
              {selectedHistoryId && !liveInspect && (
                <span className="meta-pill run-id-pill">
                  {t('trajectory.viewer.runId')} <code>{selectedHistoryId}</code>
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
                    title={t('trajectory.viewer.copyRunId')}
                    aria-label={t('trajectory.viewer.copyRunId')}
                  >
                    {copiedRunId ? <Check size={11} /> : <Copy size={11} />}
                  </button>
                </span>
              )}
              {liveInspect && inspectingJobId && (
                <span className="meta-pill">
                  {t('trajectory.viewer.jobId')} <code>{inspectingJobId}</code>
                </span>
              )}
              <span className="meta-pill">
                {t('trajectory.viewer.policy')} <code>{run.trajectory?.policyName || 'agent'}</code>
              </span>
              <span
                className={`status-badge ${liveInspect ? 'running' : String(run.status).toLowerCase()}`}
              >
                {statusLabel(liveInspect ? 'running' : String(run.status))}
              </span>
              {run.recordedCassettePath && (
                <span className="meta-pill cassette-pill" title={run.recordedCassettePath}>
                  {t('trajectory.viewer.cassetteRecorded')}
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
                title={t('trajectory.viewer.stopTaskHint')}
              >
                {stoppingTaskId ? t('trajectory.viewer.stopping') : t('trajectory.viewer.stopTask')}
              </button>
            )}
            <button
              type="button"
              className="btn-secondary export-btn"
              onClick={async () => {
                if (
                  run.pagination &&
                  api &&
                  effectiveProjectSlug &&
                  (selectedHistoryFileName || selectedHistoryId)
                ) {
                  try {
                    const full = await api.getProjectHistoryDetail(
                      effectiveProjectSlug,
                      selectedHistoryFileName || selectedHistoryId!,
                    )
                    downloadTrajectoryJson(full)
                    return
                  } catch {
                    // Fall back to current page
                  }
                }
                downloadTrajectoryJson(run)
              }}
              title={t('trajectory.viewer.exportJsonHint')}
            >
              <Download size={14} /> {t('trajectory.viewer.exportJson')}
            </button>
            <div className="export-pdf-dropdown-wrapper" ref={exportMenuRef}>
              <button
                type="button"
                className="btn-secondary export-btn"
                onClick={() => setExportMenuOpen(prev => !prev)}
                title={t('trajectory.viewer.exportPdfHint')}
              >
                <FileText size={14} /> {t('trajectory.viewer.exportPdf')} <ChevronDown size={12} className="export-dropdown-icon" />
              </button>
              {exportMenuOpen && (
                <div className="export-pdf-dropdown-menu">
                  <button
                    type="button"
                    className="export-dropdown-item"
                    onClick={async () => {
                      setExportMenuOpen(false)
                      await handleExportPdf('print')
                    }}
                    title={t('trajectory.viewer.printPdfHint')}
                  >
                    <Printer size={14} />
                    <div className="export-item-text">
                      <span className="export-item-title">{t('trajectory.viewer.printPdf')}</span>
                      <span className="export-item-desc">{t('trajectory.viewer.vectorPdf')}</span>
                    </div>
                  </button>
                  <button
                    type="button"
                    className="export-dropdown-item"
                    onClick={async () => {
                      setExportMenuOpen(false)
                      await handleExportPdf('download')
                    }}
                    title={t('trajectory.viewer.downloadPdfHint')}
                  >
                    <Download size={14} />
                    <div className="export-item-text">
                      <span className="export-item-title">{t('trajectory.viewer.downloadPdf')}</span>
                      <span className="export-item-desc">{t('trajectory.viewer.directPdf')}</span>
                    </div>
                  </button>
                  <button
                    type="button"
                    className="export-dropdown-item"
                    onClick={async () => {
                      setExportMenuOpen(false)
                      await handleExportPdf('preview')
                    }}
                    title={t('trajectory.viewer.previewHint')}
                  >
                    <ExternalLink size={14} />
                    <div className="export-item-text">
                      <span className="export-item-title">{t('trajectory.viewer.previewTab')}</span>
                      <span className="export-item-desc">{t('trajectory.viewer.previewDescription')}</span>
                    </div>
                  </button>
                </div>
              )}
            </div>
            <button
              type="button"
              className="btn-secondary"
              onClick={onBackToTasks}
              title={t('trajectory.viewer.backHint')}
            >
              ← {backLabel || t('trajectory.viewer.back')}
            </button>
          </div>
        </div>

        {/* Aggregate Metrics Bar */}
        <div className="trajectory-metrics-strip">
          <div className="metric-item">
            <span className="metric-label">{t('trajectory.viewer.totalSteps')}</span>
            <span className="metric-value">{metrics.totalSteps}</span>
          </div>
          <div className="metric-item">
            <span className="metric-label">{t('trajectory.viewer.toolCalls')}</span>
            <span className="metric-value">{metrics.toolCallsCount}</span>
          </div>
          <div className="metric-item">
            <span className="metric-label">{t('trajectory.viewer.totalDuration')}</span>
            <span className="metric-value">{t('trajectory.viewer.durationSeconds', { seconds: formatNumber(metrics.durationMs / 1000, { minimumFractionDigits: 2, maximumFractionDigits: 2, useGrouping: false }) })}</span>
          </div>
          {metrics.providerStats && metrics.providerStats.totalCalls > 0 && (
            <div
              className={`metric-item clickable-metric ${viewMode === 'providers' ? 'active' : ''}`}
              onClick={() => setViewMode('providers')}
              title={t('trajectory.viewer.providerStatsHint')}
              role="button"
              tabIndex={0}
            >
              <span className="metric-label">{t('trajectory.viewer.providerCalls')}</span>
              <div className="metric-value provider-calls-val">
                <span>{metrics.providerStats.totalCalls}</span>
                {metrics.providerStats.failedCalls > 0 && (
                  <span className="metric-badge err" title={t('trajectory.viewer.failedCallsHint', { count: metrics.providerStats.failedCalls })}>
                    {t('trajectory.viewer.failedCallsCount', { count: metrics.providerStats.failedCalls })}
                  </span>
                )}
                {metrics.providerStats.totalRetries > 0 && (
                  <span className="metric-badge warn" title={t('trajectory.viewer.retriesHint', { count: metrics.providerStats.totalRetries })}>
                    {t('trajectory.viewer.retryCount', { count: metrics.providerStats.totalRetries })}
                  </span>
                )}
              </div>
            </div>
          )}
          {metrics.totalTokens > 0 && (
            <>
              <div className="metric-item">
                <span className="metric-label">{t('trajectory.viewer.totalTokens')}</span>
                <span className="metric-value token-total">
                  {formatNumber(metrics.totalTokens)}
                </span>
              </div>
              <div className="metric-item token-breakdown-item">
                <span className="metric-label">{t('trajectory.viewer.tokenBreakdown')}</span>
                <span className="metric-subtext">
                  {t('trajectory.viewer.tokenBreakdownValue', { input: formatNumber(metrics.inputTokens), output: formatNumber(metrics.outputTokens) })}
                </span>
              </div>
            </>
          )}
          {metrics.providerStats && Object.keys(metrics.providerStats.byProvider).length > 0 && (
            <div className="metric-item provider-breakdown-item">
              <span className="metric-label">{t('trajectory.viewer.providers')}</span>
              <div className="tool-freq-chips">
                {Object.entries(metrics.providerStats.byProvider).map(([pName, pStat]) => (
                  <button
                    key={pName}
                    type="button"
                    className="tool-freq-chip provider-chip"
                    onClick={() => setViewMode('providers')}
                    title={t('trajectory.viewer.providerSummary', { provider: pName, calls: pStat.totalCalls, successful: pStat.successfulCalls, failed: pStat.failedCalls, retries: pStat.retries, duration: Math.round(pStat.avgDurationMs) })}
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
              <span className="metric-label">{t('trajectory.viewer.toolsUsed')}</span>
              <div className="tool-freq-chips">
                {Object.entries(metrics.toolUsageFrequencies).map(([name, count]) => (
                  <span key={name} className="tool-freq-chip" title={t('trajectory.viewer.toolSummary', { name, count })}>
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
            <h4>{t('trajectory.viewer.incompleteRun')}</h4>
            {failureReason && (
              <p style={{ marginTop: 4, marginBottom: 8, color: '#f87171', fontWeight: 500 }}>
                <strong>{t('trajectory.viewer.failureReason')}</strong> {failureReason}
              </p>
            )}
            {resumeSubtaskTitle && (
              <p style={{ marginTop: 2, marginBottom: 6, color: '#93c5fd' }}>
                {t('trajectory.viewer.resumeTarget')} <strong style={{ color: '#60a5fa' }}>{resumeSubtaskTitle}</strong> {resumeSubtaskId ? `(${resumeSubtaskId})` : ''}
              </p>
            )}
            <p>
              <Trans t={t} i18nKey="trajectory.viewer.continuationDescription" values={{ steps: metrics.totalSteps }} components={{ strong: <strong /> }} />
            </p>
            {run.continuation?.compactedSummary && (
              <div className="continuation-summary-panel">
                <MarkdownPreview content={run.continuation.compactedSummary} />
              </div>
            )}
          </div>
          <div className="cancelled-warning-actions">
            {resumeSubtaskTitle && (
              <button
                type="button"
                className="btn-action continue-btn continue-btn-highlight"
                onClick={() => onContinueTask(run.taskId, 25)}
                disabled={runningTaskId === run.taskId}
                title={t('trajectory.viewer.resumeHint', { title: resumeSubtaskTitle })}
                style={{
                  fontWeight: 600,
                  background: '#2563eb',
                  color: '#fff',
                  padding: '6px 14px',
                }}
              >
                {t('trajectory.viewer.resumeSubtask', { title: resumeSubtaskTitle })}
              </button>
            )}
            {[10, 25, 50, 100].map(n => (
              <button
                key={n}
                type="button"
                className="btn-action continue-btn"
                onClick={() => onContinueTask(run.taskId, n)}
                disabled={runningTaskId === run.taskId}
                title={t('trajectory.viewer.extraTaskTurnsHint', { count: n })}
              >
                {t('trajectory.viewer.extraTurns', { count: n })}
              </button>
            ))}
            <div className="banner-turns-control">
              <label htmlFor="banner-extra-turns-nav">{t('trajectory.viewer.extra')}</label>
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
                title={t('trajectory.viewer.extraTurnsHint', { count: bannerCustomTurns })}
              >
                {t('trajectory.viewer.runExtra', { count: bannerCustomTurns })}
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
                  resumeSubtaskId ?? undefined,
                  resumeSubtaskTitle ?? undefined,
                  failureReason ?? undefined,
                )
              }
              disabled={runningTaskId === run.taskId}
              title={t('trajectory.viewer.customTurnsHint')}
            >
              {t('trajectory.viewer.specifiedTurns')}
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
                <h4 className="prompt-inspector-title">{t('trajectory.viewer.promptRules')}</h4>
                <span className="prompt-inspector-subtitle">
                  {t('trajectory.viewer.promptRulesDescription')}
                </span>
              </div>
            </div>
            <button
              type="button"
              className="btn-secondary prompt-inspector-toggle-btn"
              onClick={() => setShowPromptInspector(!showPromptInspector)}
            >
              {showPromptInspector ? t('trajectory.viewer.hidePromptRules') : t('trajectory.viewer.inspectPromptRules')}
            </button>
          </div>

          {showPromptInspector && (
            <div className="prompt-inspector-body">
              {activeRules && activeRules.length > 0 && (
                <div className="prompt-rules-section">
                  <h5 className="prompt-rules-title">{t('trajectory.viewer.activeRules', { count: activeRules.length })}</h5>
                  <div className="prompt-rules-chips">
                    {activeRules.map((rule, rIdx) => (
                      <div
                        key={rIdx}
                        className={`prompt-rule-chip ${rule.global ? 'global' : 'project'}`}
                      >
                        <span className="rule-badge-type">
                          {rule.global ? t('trajectory.viewer.globalRule') : t('trajectory.viewer.projectRule')}
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
                        {t('trajectory.viewer.markdownPreview')}
                      </button>
                      <button
                        type="button"
                        className={`prompt-tab-btn ${promptInspectorMode === 'raw' ? 'active' : ''}`}
                        onClick={() => setPromptInspectorMode('raw')}
                      >
                        {t('trajectory.viewer.rawMonospace')}
                      </button>
                    </div>
                    <button
                      type="button"
                      className={`prompt-copy-btn ${copiedSystemPrompt ? 'copied' : ''}`}
                      onClick={() => handleCopySystemPrompt(systemPrompt)}
                      title={t('trajectory.viewer.copyFullSystemPrompt')}
                    >
                      {copiedSystemPrompt ? t('trajectory.viewer.copiedPrompt') : t('trajectory.viewer.copyPrompt')}
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
              <h4 className="final-answer-title">
                {String(run.status).toLowerCase() === 'success' || liveInspect
                  ? t('trajectory.viewer.finalAgentAnswer')
                  : t('trajectory.viewer.failureReport')}
              </h4>
              <span
                className={`status-badge ${
                  liveInspect ? 'running' : String(run.status).toLowerCase()
                }`}
              >
                {statusLabel(liveInspect ? 'running' : String(run.status))}
              </span>
            </div>
            <div className="final-answer-actions">
              <div className="final-answer-tabs">
                <button
                  type="button"
                  className={`final-answer-tab-btn ${finalAnswerMode === 'markdown' ? 'active' : ''}`}
                  onClick={() => setFinalAnswerMode('markdown')}
                  title={t('trajectory.viewer.markdownHint')}
                >
                  {t('trajectory.viewer.markdown')}
                </button>
                <button
                  type="button"
                  className={`final-answer-tab-btn ${finalAnswerMode === 'raw' ? 'active' : ''}`}
                  onClick={() => setFinalAnswerMode('raw')}
                  title={t('trajectory.viewer.rawTextHint')}
                >
                  {t('trajectory.viewer.rawText')}
                </button>
              </div>
              <button
                type="button"
                className={`final-answer-copy-btn ${copiedFinalAnswer ? 'copied' : ''}`}
                onClick={() => handleCopyFinalAnswer(finalAnswer)}
                title={t('trajectory.viewer.copyFinalAnswer')}
              >
                {copiedFinalAnswer ? t('trajectory.viewer.copied') : t('trajectory.viewer.copy')}
              </button>
              <button
                type="button"
                className="final-answer-copy-btn export-pdf-btn"
                onClick={() => void handleExportPdf('print')}
                title={t('trajectory.viewer.exportConclusionHint')}
              >
                <FileText size={12} /> {t('trajectory.viewer.exportPdf')}
              </button>
            </div>
          </div>

          {/* Mentioned Files Toolbar */}
          {mentionedFiles.length > 0 && (
            <div className="final-answer-mentioned-files">
              <div className="mentioned-files-label">
                <span>{t('trajectory.viewer.mentionedFiles', { count: mentionedFiles.length })}</span>
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
                      title={t('trajectory.viewer.previewFileHint', { file })}
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
              void runWithSelection(task, formatSelectionExtraPrompt(t('trajectory.viewer.selectionSource'), text))
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
              title={t('trajectory.viewer.agentLogHint')}
            >
              <MessagesSquare size={14} /> {t('trajectory.viewer.agentLog')}
            </button>
            <button
              type="button"
              className={`view-mode-btn ${viewMode === 'split' ? 'active' : ''}`}
              onClick={() => setViewMode('split')}
              title={t('trajectory.viewer.splitHint')}
            >
              <Layers size={14} /> {t('trajectory.viewer.splitInspector')}
            </button>
            <button
              type="button"
              className={`view-mode-btn ${viewMode === 'stream' ? 'active' : ''}`}
              onClick={() => setViewMode('stream')}
              title={t('trajectory.viewer.streamHint')}
            >
              <SlidersHorizontal size={14} /> {t('trajectory.viewer.streamTimeline')}
            </button>
            <button
              type="button"
              className={`view-mode-btn ${viewMode === 'providers' ? 'active' : ''}`}
              onClick={() => setViewMode('providers')}
              title={t('trajectory.viewer.providerCallsHint')}
            >
              <Zap size={14} /> {t('trajectory.viewer.providerStats')}{metrics.providerStats?.totalCalls ? ` (${metrics.providerStats.totalCalls})` : ''}
            </button>
          </div>

          {viewMode === 'log' ? (
            <>
              <label className="agent-log-filter">
                <Filter size={14} aria-hidden="true" />
                <select
                  aria-label={t('trajectory.viewer.filterLog')}
                  value={filterType}
                  onChange={event => setFilterType(event.target.value as TrajectoryFilterType)}
                >
                  <option value="all">{t('trajectory.viewer.filterAll', { count: steps.length })}</option>
                  <option value="tools">{t('trajectory.viewer.filterTools', { count: toolStepCount })}</option>
                  <option value="final_answer">{t('trajectory.viewer.filterAnswer', { count: answerStepCount })}</option>
                  <option value="yield">{t('trajectory.viewer.filterStops', { count: stopStepCount })}</option>
                  <option value="telemetry">{t('trajectory.viewer.filterTelemetry', { count: telemetryStepCount })}</option>
                </select>
              </label>
              <label className="agent-log-pagesize" title={t('trajectory.viewer.turnsPerPage')}>
                <Layers size={14} aria-hidden="true" />
                <select
                  aria-label={t('trajectory.viewer.turnsPerPage')}
                  value={logPageSize}
                  onChange={e => handleLogPageSizeChange(Number(e.target.value))}
                >
                  <option value={25}>{t('trajectory.viewer.pageSize25')}</option>
                  <option value={50}>{t('trajectory.viewer.pageSize50')}</option>
                  <option value={100}>{t('trajectory.viewer.pageSize100')}</option>
                  <option value={0}>{t('trajectory.viewer.allTurns')}</option>
                </select>
              </label>
            </>
          ) : (
            <div className="filter-pills-group">
              <button
                type="button"
                className={`filter-pill ${filterType === 'all' ? 'active' : ''}`}
                onClick={() => setFilterType('all')}
              >
                {t('trajectory.viewer.filterAll', { count: steps.length })}
              </button>
              <button
                type="button"
                className={`filter-pill ${filterType === 'tools' ? 'active' : ''}`}
                onClick={() => setFilterType('tools')}
              >
                {t('trajectory.viewer.filterToolsIcon', { count: toolStepCount })}
              </button>
              <button
                type="button"
                className={`filter-pill ${filterType === 'final_answer' ? 'active' : ''}`}
                onClick={() => setFilterType('final_answer')}
              >
                {t('trajectory.viewer.filterAnswerIcon', { count: answerStepCount })}
              </button>
              <button
                type="button"
                className={`filter-pill ${filterType === 'yield' ? 'active' : ''}`}
                onClick={() => setFilterType('yield')}
              >
                {t('trajectory.viewer.filterStopsIcon', { count: stopStepCount })}
              </button>
              <button
                type="button"
                className={`filter-pill ${filterType === 'telemetry' ? 'active' : ''}`}
                onClick={() => setFilterType('telemetry')}
              >
                {t('trajectory.viewer.filterTelemetryIcon', { count: telemetryStepCount })}
              </button>
            </div>
          )}

          <div className="trajectory-search-wrapper">
            <Search size={14} className="search-icon" />
            <input
              type="text"
              placeholder={t('trajectory.viewer.searchPlaceholder')}
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              className="trajectory-search-input"
            />
            {searchQuery && (
              <button
                type="button"
                className="search-clear-btn"
                onClick={() => setSearchQuery('')}
                title={t('trajectory.viewer.clearSearch')}
              >
                ✕
              </button>
            )}
          </div>
        </div>

        {viewMode === 'log' ? (
          <div className="toolbar-bottom-row agent-log-toolbar-bottom">
            <div className="stepper-controls">
              <button
                type="button"
                className="stepper-btn"
                onClick={() => handleLogPageChange(1)}
                disabled={loadingPage || logPage <= 1}
                title={t('trajectory.viewer.firstPage')}
              >
                {t('trajectory.viewer.first')}
              </button>
              <button
                type="button"
                className="stepper-btn"
                onClick={() => handleLogPageChange(logPage - 1)}
                disabled={loadingPage || logPage <= 1}
                title={t('trajectory.viewer.previousPage')}
              >
                <ChevronLeft size={14} /> {t('trajectory.viewer.previous')}
              </button>
              <span className="stepper-count-indicator">
                <Trans t={t} i18nKey="trajectory.viewer.pageIndicator" values={{ page: logPage, total: logPagination.totalPages }} components={{ current: <strong />, total: <strong /> }} />
                {loadingPage && <span className="stepper-loading-tag"> …</span>}
              </span>
              <button
                type="button"
                className="stepper-btn"
                onClick={() => handleLogPageChange(logPage + 1)}
                disabled={loadingPage || logPage >= logPagination.totalPages}
                title={t('trajectory.viewer.nextPage')}
              >
                {t('trajectory.viewer.next')} <ChevronRight size={14} />
              </button>
              <button
                type="button"
                className="stepper-btn"
                onClick={() => handleLogPageChange(logPagination.totalPages)}
                disabled={loadingPage || logPage >= logPagination.totalPages}
                title={t('trajectory.viewer.lastPage')}
              >
                {t('trajectory.viewer.last')}
              </button>
            </div>
            <div className="agent-log-pagination-summary">
              {logPagination.totalTurns > 0 ? (
                <>
                  <Trans t={t} i18nKey="trajectory.viewer.turnRange" values={{ start: logPagination.startIndex + 1, end: logPagination.endIndex, total: logPagination.totalTurns }} components={{ range: <strong />, total: <strong /> }} />
                  {logPagination.totalTurns < agentLog.length && (
                    <span className="agent-log-filtered-tag"> {t('trajectory.viewer.filteredFrom', { count: agentLog.length })}</span>
                  )}
                </>
              ) : (
                <span>{t('trajectory.viewer.zeroTurns')}</span>
              )}
            </div>
          </div>
        ) : (
        <>
        {/* Stepper + Global LLM Toggle Bar */}
        <div className="toolbar-bottom-row">
          <div className="stepper-controls">
            <button
              type="button"
              className="stepper-btn"
              onClick={() => setSelectedStepIdx(prev => Math.max(0, prev - 1))}
              disabled={selectedStepIdx <= 0 || steps.length === 0}
              title={t('trajectory.viewer.previousStepHint')}
            >
              <ChevronLeft size={14} /> {t('trajectory.viewer.previous')}
            </button>
            <span className="stepper-count-indicator">
              <Trans t={t} i18nKey="trajectory.viewer.stepIndicator" values={{ step: steps.length > 0 ? selectedStepIdx + 1 : 0, total: steps.length }} components={{ current: <strong />, total: <strong /> }} />
              {filteredSteps.length !== steps.length && (
                <span className="filtered-hint">{t('trajectory.viewer.matches', { count: filteredSteps.length })}</span>
              )}
            </span>
            <button
              type="button"
              className="stepper-btn"
              onClick={() => setSelectedStepIdx(prev => Math.min(steps.length - 1, prev + 1))}
              disabled={selectedStepIdx >= steps.length - 1 || steps.length === 0}
              title={t('trajectory.viewer.nextStepHint')}
            >
              {t('trajectory.viewer.next')} <ChevronRight size={14} />
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
                title={t('trajectory.viewer.jumpAnswerHint')}
              >
                {t('trajectory.viewer.jumpAnswer')}
              </button>
            )}
          </div>

          <div className="toolbar-options-right">
            <label
              className="trajectory-llm-toggle-label"
              title={t('trajectory.viewer.expandTelemetryHint')}
            >
              <input
                type="checkbox"
                className="trajectory-checkbox"
                checked={showAllLlmTelemetry}
                onChange={e => setShowAllLlmTelemetry(e.target.checked)}
              />
              <span className="trajectory-toggle-title">
                {t('trajectory.viewer.expandTelemetry')}
              </span>
            </label>
            <span className="keyboard-hint" title={t('trajectory.viewer.shortcutsHint')}>
              <Trans t={t} i18nKey="trajectory.viewer.shortcuts" components={{ j: <kbd />, k: <kbd /> }} />
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
                  title={t('trajectory.viewer.scrubberTitle', {
                    step: step.stepIndex,
                    duration: step.durationMs,
                    tools: action.toolCalls
                      ? `\n${t('trajectory.viewer.scrubberTools', {
                          tools: action.toolCalls.map(tool => tool.name).join(', '),
                        })}`
                      : '',
                    telemetry: hasTelemetry ? `\n${t('trajectory.viewer.scrubberTelemetry')}` : '',
                  })}
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
              ? t('trajectory.viewer.waitingSteps')
              : t('trajectory.viewer.noSteps')}
          </p>
        </div>
      )}

      {showAgentLog && (
        <AgentLogTranscript
          turns={pagedLogTurns}
          allTurns={filteredLogTurns}
          completedSubtasks={completedSubtasks}
          pagination={logPagination}
          onPageChange={handleLogPageChange}
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
            void runWithSelection(task, formatSelectionExtraPrompt(t('trajectory.viewer.selectionSource'), text))
          }}
          finalAnswer={finalAnswer}
          failureReason={failureReason}
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
                  resumeSubtaskId: resumeSubtaskId ?? undefined,
                  resumeSubtaskTitle: resumeSubtaskTitle ?? undefined,
                  failureReason: failureReason ?? undefined,
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
                            resumeSubtaskId ?? undefined,
                            resumeSubtaskTitle ?? undefined,
                            failureReason ?? undefined,
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
              <span className="sidebar-title">{t('trajectory.viewer.trajectorySteps')}</span>
              <span className="sidebar-counter">
                {t('trajectory.viewer.stepCount', { count: filteredSteps.length, total: steps.length })}
              </span>
            </div>

            <div className="sidebar-steps-list">
              {filteredSteps.length === 0 ? (
                <div className="sidebar-empty-hint">{t('trajectory.viewer.noSearchSteps')}</div>
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
                        <span className="step-num-badge">{t('trajectory.viewer.stepNumber', { step: step.stepIndex })}</span>
                        <span className="step-time-badge">
                          {formatTime(new Date(step.timestamp), {
                            hour: '2-digit',
                            minute: '2-digit',
                            second: '2-digit',
                          })}
                        </span>
                      </div>

                      <div className="sidebar-step-action-desc">
                        {isFinal && (
                          <span className="step-action-tag final-answer">{t('trajectory.viewer.finalAnswer')}</span>
                        )}
                        {parsedAction.kind === 'call_tools' && (
                          <div className="sidebar-tools-list">
                            {toolCalls.slice(0, 3).map((t, tIdx) => (
                              <span key={tIdx} className="sidebar-tool-badge">
                                🛠️ {t.name}
                              </span>
                            ))}
                            {toolCalls.length > 3 && (
                              <span className="sidebar-tool-more">{t('trajectory.viewer.moreTools', { count: toolCalls.length - 3 })}</span>
                            )}
                          </div>
                        )}
                        {parsedAction.kind === 'yield_control' && (
                          <span className="step-action-tag yield-control">{t('trajectory.viewer.yieldControl')}</span>
                        )}
                        {parsedAction.kind === 'request_input' && (
                          <span className="step-action-tag request-input">{t('trajectory.viewer.requestInput')}</span>
                        )}
                      </div>

                      <div className="sidebar-step-footer">
                        <span className="step-duration-text">{t('trajectory.viewer.stepDuration', { duration: step.durationMs })}</span>
                        {telemetry?.totalTokens && (
                          <span className="step-tokens-text">
                            {t('trajectory.viewer.compactTokens', { tokens: formatNumber(telemetry.totalTokens) })}
                          </span>
                        )}
                        {telemetry && (
                          <span className="sidebar-telemetry-indicator" title={t('trajectory.viewer.telemetryAvailable')}>
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
            <div className="empty-card">{t('trajectory.viewer.noFilterSteps')}</div>
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
  const { t } = useTranslation()
  const roleLabel = (role: string) => {
    switch (role) {
      case 'user':
        return t('trajectory.viewer.roleUser')
      case 'assistant':
        return t('trajectory.viewer.roleAssistant')
      case 'tool':
        return t('trajectory.viewer.roleTool')
      case 'system':
        return t('trajectory.viewer.roleSystem')
      default:
        return role.toUpperCase()
    }
  }
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
          <span className="step-index-badge">{t('trajectory.viewer.stepNumber', { step: step.stepIndex })}</span>
          <span className="step-time">
            <Clock size={12} /> {t('trajectory.viewer.stepTime', { time: formatTime(new Date(step.timestamp)), duration: step.durationMs })}
          </span>
        </div>

        <div className="step-header-right">
          {telemetry?.totalTokens && (
            <span className="step-token-badge">
              {t('trajectory.viewer.tokensValue', { tokens: formatNumber(telemetry.totalTokens) })}
              {telemetry.tokensPerSec ? ` ${t('trajectory.viewer.tokenSpeedSuffix', { speed: telemetry.tokensPerSec })}` : ''}
            </span>
          )}
          <button
            type="button"
            className="step-toggle-btn"
            onClick={onToggleRawExpanded}
            title={t('trajectory.viewer.rawActionHint')}
          >
            {isRawExpanded ? t('trajectory.viewer.hideRawAction') : t('trajectory.viewer.rawAction')}
          </button>
        </div>
      </div>

      <div className="step-body">
        {/* ================= 1. AGENT ACTION BOX ================= */}
        <div className="step-action-box">
          <div className="step-action-header-row">
            <h5>{t('trajectory.viewer.agentAction')}</h5>
            <div style={{ display: 'flex', gap: '0.4rem', alignItems: 'center' }}>
              {parsedAction.kind === 'final_answer' && (
                <span className="step-action-badge final-answer">{t('trajectory.viewer.finalAnswer')}</span>
              )}
              {parsedAction.kind === 'call_tools' && (
                <span className="step-action-badge call-tools">
                  {t('trajectory.viewer.actionCalls', { count: parsedAction.toolCalls?.length ?? 0 })}
                </span>
              )}
              {parsedAction.kind === 'yield_control' && (
                <span className="step-action-badge yield-control">{t('trajectory.viewer.yieldControl')}</span>
              )}
              {parsedAction.kind === 'request_input' && (
                <span className="step-action-badge request-input">{t('trajectory.viewer.requestInput')}</span>
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
                            title={t('trajectory.viewer.copyArguments')}
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
                  <strong>{t('trajectory.viewer.reason')}</strong> {parsedAction.yieldReason || t('trajectory.viewer.none')}
                </div>
              )}
              {parsedAction.kind === 'request_input' && (
                <div className="step-input-box">
                  <strong>{t('trajectory.viewer.prompt')}</strong> {parsedAction.prompt || t('trajectory.viewer.none')}
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
                  <MessageSquare size={13} /> {t('trajectory.viewer.fullPrompt', { count: messages.length })}
                </button>
                <button
                  type="button"
                  className={`telemetry-tab-btn ${activeTelemetryTab === 'response' ? 'active' : ''}`}
                  onClick={() => setActiveTelemetryTab('response')}
                >
                  <Bot size={13} /> {t('trajectory.viewer.rawResponse')}
                </button>
                {telemetry?.thinking && (
                  <button
                    type="button"
                    className={`telemetry-tab-btn ${activeTelemetryTab === 'thinking' ? 'active' : ''}`}
                    onClick={() => setActiveTelemetryTab('thinking')}
                  >
                    <Sparkles size={13} /> {t('trajectory.viewer.reasoning')}
                  </button>
                )}
                {telemetry?.totalTokens && (
                  <button
                    type="button"
                    className={`telemetry-tab-btn ${activeTelemetryTab === 'tokens' ? 'active' : ''}`}
                    onClick={() => setActiveTelemetryTab('tokens')}
                  >
                    <Zap size={13} /> {t('trajectory.viewer.tokensLatency')}
                  </button>
                )}
                <button
                  type="button"
                  className={`telemetry-tab-btn ${activeTelemetryTab === 'raw' ? 'active' : ''}`}
                  onClick={() => setActiveTelemetryTab('raw')}
                >
                  <FileCode size={13} /> {t('trajectory.viewer.rawPayloads')}
                </button>
                {telemetry?.providerCalls && telemetry.providerCalls.length > 0 && (
                  <button
                    type="button"
                    className={`telemetry-tab-btn ${activeTelemetryTab === 'provider_calls' ? 'active' : ''}`}
                    onClick={() => setActiveTelemetryTab('provider_calls')}
                  >
                    <Zap size={13} /> {t('trajectory.viewer.telemetryProviderCalls', { count: telemetry.providerCalls.length })}
                  </button>
                )}
              </div>

              <div className="telemetry-tabs-right">
                {activeTelemetryTab === 'prompt' && (
                  <button
                    type="button"
                    className="step-copy-sub-btn"
                    onClick={() => onCopyPrompt(step.llmRequest)}
                    title={t('trajectory.viewer.copyPromptPayload')}
                  >
                    {copiedPrompt ? t('trajectory.viewer.copied') : t('trajectory.viewer.copyPrompt')}
                  </button>
                )}
                {activeTelemetryTab === 'response' && (
                  <button
                    type="button"
                    className="step-copy-sub-btn"
                    onClick={() => onCopyResponse(step.llmResponse)}
                    title={t('trajectory.viewer.copyResponsePayload')}
                  >
                    {copiedResponse ? t('trajectory.viewer.copied') : t('trajectory.viewer.copyResponse')}
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
                      <Brain size={13} /> {t('trajectory.viewer.systemInstructions', { count: telemetry.systemPrompt.length })}
                    </summary>
                    <div className="system-prompt-actions">
                      <button
                        type="button"
                        className="mini-copy-btn"
                        onClick={() =>
                          onCopyMessageText(`sys-${stepIdx}`, telemetry.systemPrompt || '')
                        }
                        title={t('trajectory.viewer.copySystemPrompt')}
                      >
                        {copiedMsgIdx === `sys-${stepIdx}` ? t('trajectory.viewer.copied') : t('trajectory.viewer.copy')}
                      </button>
                    </div>
                    <pre className="step-prompt-system-pre">{telemetry.systemPrompt}</pre>
                  </details>
                )}

                {/* Messages header & role filters */}
                <div className="prompt-messages-toolbar">
                  <div className="msg-role-filters">
                    <span className="toolbar-label">
                      <Filter size={12} /> {t('trajectory.viewer.filterRole')}
                    </span>
                    {(['all', 'user', 'assistant', 'tool', 'system'] as MsgRoleFilter[]).map(role => (
                      <button
                        key={role}
                        type="button"
                        className={`msg-filter-btn ${msgRoleFilter === role ? 'active' : ''}`}
                        onClick={() => setMsgRoleFilter(role)}
                      >
                        {role === 'all'
                          ? t('trajectory.viewer.allMessages', { count: messages.length })
                          : t('trajectory.viewer.roleCount', { role: roleLabel(role), count: messages.filter(m => m.role === role).length })}
                      </button>
                    ))}
                  </div>

                  <div className="prompt-messages-actions">
                    <div className="prompt-render-mode-tabs">
                      <button
                        type="button"
                        className={`render-mode-tab ${promptRenderMode === 'monospace' ? 'active' : ''}`}
                        onClick={() => setPromptRenderMode('monospace')}
                        title={t('trajectory.viewer.monospaceHint')}
                      >
                        {t('trajectory.viewer.monospace')}
                      </button>
                      <button
                        type="button"
                        className={`render-mode-tab ${promptRenderMode === 'markdown' ? 'active' : ''}`}
                        onClick={() => setPromptRenderMode('markdown')}
                        title={t('trajectory.viewer.renderMarkdownHint')}
                      >
                        {t('trajectory.viewer.markdown')}
                      </button>
                    </div>

                    <button
                      type="button"
                      className="expand-all-btn"
                      onClick={() => onToggleExpandAllMessages(messages.length)}
                      title={t('trajectory.viewer.expandAllHint')}
                    >
                      {allMessagesExpanded ? t('trajectory.viewer.collapseAll') : t('trajectory.viewer.expandAll')}
                    </button>
                  </div>
                </div>

                {/* Message cards */}
                {filteredMessages.length === 0 ? (
                  <div className="empty-subcard">{t('trajectory.viewer.noRoleMessages')}</div>
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
                                {roleIcon} {roleLabel(m.role)}
                              </span>
                              <span className="prompt-message-index">{t('trajectory.viewer.turnNumber', { turn: m.index })}</span>
                              {m.text && (
                                <span className="msg-length-badge">
                                  {t('trajectory.viewer.shortCharacters', { count: m.text.length })}
                                </span>
                              )}
                              {m.toolCalls && (
                                <span className="msg-tool-call-count">
                                  {t('trajectory.viewer.messageToolCalls', { count: m.toolCalls.length })}
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
                                title={t('trajectory.viewer.copyMessage')}
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
                                  <span className="thinking-title">{t('trajectory.viewer.thinkingTrace')}</span>
                                  <pre className="thinking-content">{m.thinking}</pre>
                                </div>
                              )}

                              {/* Tool calls inside message */}
                              {m.toolCalls && m.toolCalls.length > 0 && (
                                <div className="prompt-msg-tool-calls">
                                  {m.toolCalls.map((tc, tcIdx) => (
                                    <div key={tcIdx} className="prompt-msg-tool-call">
                                      <span className="prompt-tool-badge">
                                        {t('trajectory.viewer.toolRequest')} <code>{tc.name}</code>
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
                      <h6>{t('trajectory.viewer.completionText')}</h6>
                      <span className="subpart-meta">
                        {t('trajectory.viewer.characters', { count: telemetry.completionText.length })}
                      </span>
                    </div>
                    <pre className="llm-response-pre">{telemetry.completionText}</pre>
                  </div>
                )}

                {telemetry?.modelToolCalls && telemetry.modelToolCalls.length > 0 && (
                  <div className="response-subpart">
                    <div className="response-subpart-header">
                      <h6>{t('trajectory.viewer.generatedCalls')}</h6>
                      <span className="subpart-meta">
                        {t('trajectory.viewer.generatedCallCount', { count: telemetry.modelToolCalls.length })}
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
                      <h6>{t('trajectory.viewer.assistantReply')}</h6>
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
                    <Sparkles size={14} /> {t('trajectory.viewer.chainOfThought')}
                  </div>
                  <button
                    type="button"
                    className="mini-copy-btn"
                    onClick={() =>
                      onCopyMessageText(`think-${stepIdx}`, telemetry.thinking || '')
                    }
                    title={t('trajectory.viewer.copyThinkingHint')}
                  >
                    {copiedMsgIdx === `think-${stepIdx}` ? t('trajectory.viewer.copied') : t('trajectory.viewer.copyThinking')}
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
                    <span className="t-label">{t('trajectory.viewer.promptTokens')}</span>
                    <span className="t-val input">
                      {telemetry.inputTokens == null ? '—' : formatNumber(telemetry.inputTokens)}
                    </span>
                  </div>
                  <div className="token-metric-card">
                    <span className="t-label">{t('trajectory.viewer.completionTokens')}</span>
                    <span className="t-val output">
                      {telemetry.outputTokens == null ? '—' : formatNumber(telemetry.outputTokens)}
                    </span>
                  </div>
                  <div className="token-metric-card">
                    <span className="t-label">{t('trajectory.viewer.stepTokens')}</span>
                    <span className="t-val total">
                      {telemetry.totalTokens == null ? '—' : formatNumber(telemetry.totalTokens)}
                    </span>
                  </div>
                  <div className="token-metric-card">
                    <span className="t-label">{t('trajectory.viewer.turnLatency')}</span>
                    <span className="t-val latency">{t('trajectory.viewer.latencyValue', { duration: telemetry.durationMs })}</span>
                  </div>
                  {telemetry.tokensPerSec !== undefined && (
                    <div className="token-metric-card">
                      <span className="t-label">{t('trajectory.viewer.generationSpeed')}</span>
                      <span className="t-val speed">{t('trajectory.viewer.generationSpeedValue', { speed: telemetry.tokensPerSec })}</span>
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
                      <h6>{t('trajectory.viewer.requestPayload')}</h6>
                      <button
                        type="button"
                        className="mini-copy-btn"
                        onClick={() => onCopyPrompt(step.llmRequest)}
                      >
                        {copiedPrompt ? t('trajectory.viewer.copied') : t('trajectory.viewer.copyRequest')}
                      </button>
                    </div>
                    <pre className="json-pre">{JSON.stringify(step.llmRequest, null, 2)}</pre>
                  </div>

                  <div className="raw-json-col">
                    <div className="raw-col-header">
                      <h6>{t('trajectory.viewer.responsePayload')}</h6>
                      <button
                        type="button"
                        className="mini-copy-btn"
                        onClick={() => onCopyResponse(step.llmResponse)}
                      >
                        {copiedResponse ? t('trajectory.viewer.copied') : t('trajectory.viewer.copyResponse')}
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
                            {t('trajectory.viewer.callNumber', { call: cIdx + 1 })}
                            {call.retryAttempt !== undefined && call.retryAttempt !== null && call.retryAttempt > 0 && (
                              <span className="call-retry-pill">{t('trajectory.viewer.retryNumber', { attempt: call.retryAttempt })}</span>
                            )}
                            {call.isSecondary && <span className="call-secondary-pill">{t('trajectory.viewer.secondaryFailover')}</span>}
                          </span>
                          <span className="call-provider-tag">
                            <strong>{call.provider}</strong>
                            {call.model && <code className="call-model-tag">{call.model}</code>}
                          </span>
                        </div>
                        <span className={`call-status-badge ${call.success ? 'ok' : 'err'}`}>
                          {call.success ? t('trajectory.viewer.success') : t('trajectory.viewer.failed')}
                        </span>
                      </div>

                      <div className="provider-call-metrics-row">
                        <div className="call-mini-metric">
                          <span className="mini-label">{t('trajectory.viewer.latency')}</span>
                          <span className="mini-val font-mono">{t('trajectory.viewer.providerLatencyValue', { duration: formatNumber(call.latencyMs) })}</span>
                        </div>
                        {call.tokenUsage && (
                          <>
                            <div className="call-mini-metric">
                              <span className="mini-label">{t('trajectory.viewer.inputTokens')}</span>
                              <span className="mini-val font-mono">
                                {formatNumber(call.tokenUsage.inputTokens ?? 0)}
                              </span>
                            </div>
                            <div className="call-mini-metric">
                              <span className="mini-label">{t('trajectory.viewer.outputTokens')}</span>
                              <span className="mini-val font-mono">
                                {formatNumber(call.tokenUsage.outputTokens ?? 0)}
                              </span>
                            </div>
                            <div className="call-mini-metric">
                              <span className="mini-label">{t('trajectory.viewer.totalTokens')}</span>
                              <span className="mini-val font-mono total">
                                {formatNumber(
                                  call.tokenUsage.totalTokens ??
                                  (call.tokenUsage.inputTokens ?? 0) + (call.tokenUsage.outputTokens ?? 0),
                                )}
                              </span>
                            </div>
                          </>
                        )}
                        {call.cost !== undefined && call.cost !== null && (
                          <div className="call-mini-metric">
                            <span className="mini-label">{t('trajectory.viewer.estimatedCost')}</span>
                            <span className="mini-val font-mono">${formatNumber(call.cost, { minimumFractionDigits: 5, maximumFractionDigits: 5, useGrouping: false })}</span>
                          </div>
                        )}
                      </div>

                      {call.retryReason && (
                        <div className="call-retry-reason-box">
                          <strong>{t('trajectory.viewer.retryReason')}</strong> {call.retryReason}
                        </div>
                      )}

                      {call.error && (
                        <div className="call-error-box">
                          <strong>{t('trajectory.viewer.errorLabel')}</strong> {call.error}
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
              <h5>{t('trajectory.viewer.toolResults')}</h5>
              <button
                type="button"
                className="mini-copy-btn"
                onClick={() =>
                  onCopyMessageText(
                    `results-${stepIdx}`,
                    JSON.stringify(step.toolResults, null, 2),
                  )
                }
                title={t('trajectory.viewer.copyResultsHint')}
              >
                {copiedMsgIdx === `results-${stepIdx}` ? t('trajectory.viewer.copied') : t('trajectory.viewer.copyResults')}
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
                      {result.isError ? t('trajectory.viewer.toolError') : t('trajectory.viewer.toolOk')}
                    </span>
                    {result.output && (
                      <button
                        type="button"
                        className="mini-copy-btn"
                        onClick={() => onCopyMessageText(`result-${stepIdx}-${rIdx}`, result.output || '')}
                        title={t('trajectory.viewer.copyToolOutput')}
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
