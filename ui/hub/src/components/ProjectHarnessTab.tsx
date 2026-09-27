import { useEffect, useRef, useState, useCallback, useMemo } from 'react'
import {
  Activity,
  AlertCircle,
  CircleDot,
  Clock,
  Eye,
  EyeOff,
  FastForward,
  Loader2,
  Paperclip,
  Pencil,
  Play,
  RotateCcw,
  SlidersHorizontal,
  Square,
  Trash2,
  X,
} from 'lucide-react'
import {
  defaultSchedule,
  describeTaskSchedule,
  parseFriendlySchedule,
  parseTimeInput,
  scheduleToCron,
  timeInputValue,
  WEEKDAYS,
  type createApi,
  type FriendlySchedule,
  type HarnessActiveJob,
  type HarnessCassetteSummary,
  type HarnessEvaluationReport,
  type HarnessHistorySummary,
  type HarnessJobLiveEvent,
  type HarnessProjectEvent,
  type HarnessReportSummary,
  type HarnessRunResponse,
  type Project,
  type ProjectHarnessConfig,
  type ProjectTaskDefinition,
  type ProjectTaskEstimateResponse,
  type ProjectTaskSummary,
  type ScheduleKind,
} from '@aaif/goose-hub-core'
import { inspectProjectHarnessJob } from '../api.ts'
import { extraTurnsForContinue, isContinuableStatus } from '../taskContinuation.ts'
import { isLiveTrajectoryVisible } from '../taskTrajectory.ts'
import { fetchWithRetry, filterAndSortProjectTasks, historyTaskTitle } from '../projectTasks.ts'
import {
  isAcceptedTaskAttachment,
  listTaskAttachmentPaths,
  TASK_ATTACHMENT_ACCEPT,
  uploadAttachmentsAndComposePrompt,
  workspacePathsForAttachments,
} from '../taskAttachments.ts'
import { HarnessHistoryTable } from './HarnessHistoryTable.tsx'
import { TaskTrajectoryViewer } from './TaskTrajectoryViewer.tsx'
import { MarkdownPreview } from './MarkdownPreview.tsx'
import { SkeletonBox } from './Skeletons'
import { MentionPopover, type MentionPopoverHandle } from './MentionPopover.tsx'
import {
  detectMentionTrigger,
  applyMentionInsertion,
  insertMentionText,
  scanProjectFiles,
  searchProjectFiles,
  fetchAgentMentions,
  shouldIgnoreMentionTrigger,
  debounce,
  mentionItemsFromAttachmentPaths,
  mergeMentionItems,
  type MentionDisplayItem,
  type DismissedMention,
} from '../mention.ts'

interface ProjectHarnessTabProps {
  api: ReturnType<typeof createApi>
  projects?: Project[]
  selectedProjectSlug: string
  onSelectProject?: (slug: string) => void
  onInspectTrajectory?: (run: HarnessRunResponse) => void
  onNavigateToFiles?: (path: string) => void
}

type ProjectSubTab = 'tasks' | 'config' | 'reports' | 'cassettes' | 'history' | 'trajectory'

const MIN_SUBTASK_TURNS = 8

const SYNTHESIZED_PHASE_LABELS = [
  {
    title: 'Phase 1: Discovery / DTO',
    description: 'Locate modules and define DTOs, traits, and interfaces',
  },
  {
    title: 'Phase 2: Core implementation',
    description: 'Implement backend engine, services, and business logic',
  },
  {
    title: 'Phase 3: Integration / tests',
    description: 'Build, run tests, and verify end-to-end behavior',
  },
] as const

/** Matches Rust `split_synthesized_phase_turns`: 2:3:2 split with a floor of 8. */
function splitSynthesizedPhaseTurns(parentMaxTurns: number): number[] {
  const parent = Math.max(Number(parentMaxTurns) || 0, MIN_SUBTASK_TURNS)
  const p1 = Math.max(MIN_SUBTASK_TURNS, Math.floor((parent * 2) / 7))
  const p3 = Math.max(MIN_SUBTASK_TURNS, Math.floor((parent * 2) / 7))
  const p2 = Math.max(MIN_SUBTASK_TURNS, parent - p1 - p3)
  return [p1, p2, p3]
}

function phaseTurnsFromTask(
  task: Pick<ProjectTaskDefinition, 'maxTurns' | 'phaseMaxTurns'>,
): number[] {
  const split = splitSynthesizedPhaseTurns(task.maxTurns || 25)
  const overrides = task.phaseMaxTurns
  if (!overrides || overrides.length === 0) return split
  return split.map((fallback, i) => {
    const value = overrides[i]
    return typeof value === 'number' && value > 0 ? value : fallback
  })
}

function InspectRunningJobButton({
  inspectingThis,
  loading = false,
  onClick,
}: {
  inspectingThis: boolean
  loading?: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      className={`btn-action view-btn ${inspectingThis ? 'is-pressed' : ''}`}
      onClick={onClick}
      disabled={loading}
      aria-pressed={inspectingThis}
      title={
        inspectingThis
          ? 'Hide the live agent log of this running task'
          : 'Inspect the live agent log of this running task'
      }
    >
      {loading ? (
        <>
          <Loader2 size={13} className="spinning" />
          <span>Loading…</span>
        </>
      ) : (
        <>
          {inspectingThis ? <EyeOff size={13} /> : <Eye size={13} />}
          <span>{inspectingThis ? 'Hide' : 'Inspect'}</span>
        </>
      )}
    </button>
  )
}

function normalizeHarnessConfig(raw: Partial<ProjectHarnessConfig> | any): ProjectHarnessConfig {
  return {
    version: raw?.version || '1.0',
    policy: {
      provider: raw?.policy?.provider || undefined,
      model: raw?.policy?.model || undefined,
      systemPrompt: raw?.policy?.systemPrompt || raw?.policy?.system_prompt || undefined,
    },
    sandbox: {
      kind: raw?.sandbox?.kind || 'local',
      containerImage: raw?.sandbox?.containerImage || raw?.sandbox?.container_image || undefined,
      setupCommands: Array.isArray(raw?.sandbox?.setupCommands)
        ? raw.sandbox.setupCommands
        : Array.isArray(raw?.sandbox?.setup_commands)
        ? raw.sandbox.setup_commands
        : [],
      envVars: raw?.sandbox?.envVars || raw?.sandbox?.env_vars || {},
    },
    execution: {
      maxTurns: raw?.execution?.maxTurns ?? raw?.execution?.max_turns ?? 25,
      timeoutSeconds: raw?.execution?.timeoutSeconds ?? raw?.execution?.timeout_seconds ?? 300,
      concurrency: raw?.execution?.concurrency ?? 4,
    },
    paths: {
      tasksDir: raw?.paths?.tasksDir || raw?.paths?.tasks_dir || '.goose/tasks',
      resultsDir: raw?.paths?.resultsDir || raw?.paths?.results_dir || '.goose/harness_results',
      cassettesDir: raw?.paths?.cassettesDir || raw?.paths?.cassettes_dir || '.goose/cassettes',
    },
  }
}

export function ProjectHarnessTab({
  api,
  projects = [],
  selectedProjectSlug,
  onSelectProject,
  onInspectTrajectory,
  onNavigateToFiles,
}: ProjectHarnessTabProps) {
  const [subTab, setSubTab] = useState<ProjectSubTab>('tasks')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [successMsg, setSuccessMsg] = useState('')

  // Project data
  const [config, setConfig] = useState<ProjectHarnessConfig | null>(null)
  const [tasks, setTasks] = useState<ProjectTaskSummary[]>([])
  const [reports, setReports] = useState<HarnessReportSummary[]>([])
  const [cassettes, setCassettes] = useState<HarnessCassetteSummary[]>([])
  const [activeJobs, setActiveJobs] = useState<HarnessActiveJob[]>([])
  const [history, setHistory] = useState<HarnessHistorySummary[]>([])
  const [selectedHistoryId, setSelectedHistoryId] = useState<string | null>(null)

  // Task editing / creation
  const [isEditingTask, setIsEditingTask] = useState(false)
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null)
  const [taskForm, setTaskForm] = useState<ProjectTaskDefinition>({
    id: '',
    name: '',
    category: '',
    tags: [],
    prompt: '',
    maxTurns: 25,
    timeoutSeconds: 300,
    autoDecompose: true,
    phaseMaxTurns: splitSynthesizedPhaseTurns(25),
  })
  const [taskTagsInput, setTaskTagsInput] = useState('')
  const [schedule, setSchedule] = useState<FriendlySchedule>(defaultSchedule)
  const [verifierType, setVerifierType] = useState<'none' | 'command' | 'diff'>('none')
  const [verifierCommand, setVerifierCommand] = useState('')
  const [verifierExpectedStdout, setVerifierExpectedStdout] = useState('')
  const [verifierExpectedExitCode, setVerifierExpectedExitCode] = useState(0)
  const [verifierFilePath, setVerifierFilePath] = useState('')
  const [verifierExpectedContent, setVerifierExpectedContent] = useState('')
  const [estimatingBudget, setEstimatingBudget] = useState(false)
  const [estimateInfo, setEstimateInfo] = useState<ProjectTaskEstimateResponse | null>(null)
  const [estimateFeedback, setEstimateFeedback] = useState('')
  const [taskAttachments, setTaskAttachments] = useState<File[]>([])
  const [existingAttachmentMentions, setExistingAttachmentMentions] = useState<
    MentionDisplayItem[]
  >([])
  const [savingTask, setSavingTask] = useState(false)
  const taskAttachInputRef = useRef<HTMLInputElement | null>(null)

  // Task mention autocomplete state
  const [taskMentionItems, setTaskMentionItems] = useState<MentionDisplayItem[]>([])
  const [loadingTaskMentions, setLoadingTaskMentions] = useState(false)
  const [taskMentionPopover, setTaskMentionPopover] = useState<{
    isOpen: boolean
    query: string
    mentionStart: number
    selectedIndex: number
  }>({
    isOpen: false,
    query: '',
    mentionStart: -1,
    selectedIndex: 0,
  })
  const taskMentionPopoverRef = useRef<MentionPopoverHandle | null>(null)
  const taskPromptTextareaRef = useRef<HTMLTextAreaElement | null>(null)
  const dismissedTaskMentionRef = useRef<DismissedMention | null>(null)
  const lastDismissedTaskTimeRef = useRef<number>(0)

  // Running states
  const [runningTaskId, setRunningTaskId] = useState<string | null>(null)
  const [stoppingTaskId, setStoppingTaskId] = useState<string | null>(null)
  const [runningEval, setRunningEval] = useState(false)
  const [savingConfig, setSavingConfig] = useState(false)
  const [filterTag, setFilterTag] = useState('')
  const [filterQuery, setFilterQuery] = useState('')
  const [selectedReportDetail, setSelectedReportDetail] = useState<HarnessEvaluationReport | null>(null)
  const [activeTrajectoryView, setActiveTrajectoryView] = useState<HarnessRunResponse | null>(null)
  const [loadingTrajectory, setLoadingTrajectory] = useState(false)
  const [loadingHistoryId, setLoadingHistoryId] = useState<string | null>(null)
  const [loadingHistory, setLoadingHistory] = useState(false)
  const [inspectingJobId, setInspectingJobId] = useState<string | null>(null)
  const [inspectingTaskId, setInspectingTaskId] = useState<string | null>(null)
  const [liveInspect, setLiveInspect] = useState(false)
  const inspectingJobIdRef = useRef<string | null>(null)
  const requestIdRef = useRef<number>(0)

  inspectingJobIdRef.current = inspectingJobId

  const startLiveInspect = useCallback((jobId: string, taskId: string) => {
    inspectingJobIdRef.current = jobId
    setInspectingJobId(jobId)
    setInspectingTaskId(taskId)
    setLiveInspect(true)
  }, [])

  const stopLiveInspect = useCallback(() => {
    inspectingJobIdRef.current = null
    setInspectingJobId(null)
    setInspectingTaskId(null)
    setLiveInspect(false)
  }, [])

  // Continue task modal and turn selection state
  const [continueModalTask, setContinueModalTask] = useState<{
    taskId: string
    runId?: string
    taskName?: string
    previousSteps: number
    progressSummary?: string
  } | null>(null)
  const [continueTurnsMode, setContinueTurnsMode] = useState<'additional' | 'total'>('additional')
  const [continueTurnsInput, setContinueTurnsInput] = useState<number>(25)
  const [continueRecord, setContinueRecord] = useState<boolean>(false)
  const [dynamicRun, setDynamicRun] = useState<{
    taskId: string
    taskName: string
    record: boolean
    prompt: string
    extra: string
    loadingPrompt: boolean
    error: string
  } | null>(null)
  const selectedProject = projects.find(p => p.slug === selectedProjectSlug)



  // Loaded sections registry to prevent redundant fetches
  const [loadedSections, setLoadedSections] = useState<Record<string, boolean>>({})
  const [sectionLoading, setSectionLoading] = useState<string | null>(null)
  const [historyError, setHistoryError] = useState<string | null>(null)
  const [historyRetryAttempt, setHistoryRetryAttempt] = useState<number>(0)
  const isHistoryLoading =
    loadingHistory || sectionLoading === 'history' || (!loadedSections.history && !historyError)

  // In-flight request trackers to deduplicate concurrent/StrictMode requests
  const inFlightTasksRef = useRef<Promise<void> | null>(null)
  const inFlightHistoryRef = useRef<Promise<void> | null>(null)
  const inFlightReportsRef = useRef<Promise<void> | null>(null)
  const inFlightCassettesRef = useRef<Promise<void> | null>(null)
  const inFlightConfigRef = useRef<Promise<void> | null>(null)
  const lastLoadedSlugRef = useRef<string | null>(null)
  const historyRequestIdRef = useRef<number>(0)

  // 1. History loader with automatic retries
  const loadHistory = useCallback(
    async (
      slug: string,
      silent = false,
      force = false,
      maxRetries = 2,
    ): Promise<void> => {
      if (!slug) return
      if (!force && inFlightHistoryRef.current) {
        return inFlightHistoryRef.current
      }

      const reqId = ++historyRequestIdRef.current
      if (!silent) setSectionLoading('history')
      setLoadingHistory(true)
      setHistoryError(null)
      setHistoryRetryAttempt(0)

      const promise = (async () => {
        try {
          const hist = await fetchWithRetry(
            () => api.listProjectHistory(slug),
            {
              maxRetries,
              delayMs: 1000,
              onRetry: attempt => {
                if (reqId === historyRequestIdRef.current) {
                  setHistoryRetryAttempt(attempt)
                }
              },
            },
          )
          if (reqId === historyRequestIdRef.current) {
            setHistory(hist || [])
            setLoadedSections(prev => ({ ...prev, history: true }))
            setHistoryError(null)
            setHistoryRetryAttempt(0)
          }
        } catch (err) {
          if (reqId === historyRequestIdRef.current) {
            const msg = err instanceof Error ? err.message : 'Failed to load task history'
            setHistoryError(msg)
            setHistoryRetryAttempt(0)
            if (!silent) setError(msg)
          }
        } finally {
          inFlightHistoryRef.current = null
          if (reqId === historyRequestIdRef.current) {
            setLoadingHistory(false)
            setSectionLoading(prev => (prev === 'history' ? null : prev))
          }
        }
      })()

      inFlightHistoryRef.current = promise
      return promise
    },
    [api],
  )

  const handleRetryHistory = useCallback(() => {
    if (!selectedProjectSlug) return
    setError(prev => (prev.toLowerCase().includes('history') ? '' : prev))
    void loadHistory(selectedProjectSlug, false, true)
  }, [selectedProjectSlug, loadHistory])

  // 2. Lightweight tasks & active jobs loader (serves default view, fast with no heavy disk scanning)
  const pendingTasksRefreshRef = useRef(false)

  const loadTasksAndJobs = async (slug: string, silent = false, _force = false): Promise<void> => {
    if (!slug) return
    if (inFlightTasksRef.current) {
      pendingTasksRefreshRef.current = true
      return inFlightTasksRef.current
    }

    const currentReqId = ++requestIdRef.current
    if (!silent) {
      setLoading(true)
      setError('')
    }

    const promise = (async () => {
      try {
        const [tasksRes, jobsRes, configRes] = await Promise.all([
          api.listProjectTasks(slug),
          api.listProjectActiveJobs(slug),
          config ? Promise.resolve(config) : api.getProjectHarnessConfig(slug).catch(() => null),
        ])

        if (currentReqId === requestIdRef.current) {
          setTasks(tasksRes || [])
          setActiveJobs(jobsRes || [])
          if (configRes) {
            setConfig(normalizeHarnessConfig(configRes))
          }
          setLoadedSections(prev => ({ ...prev, tasks: true, config: Boolean(configRes) }))
        }

        // Fetch recent task history in background with automatic retries
        void loadHistory(slug, true, true)
      } catch (err) {
        if (currentReqId === requestIdRef.current && !silent) {
          setError(err instanceof Error ? err.message : 'Failed to load project tasks')
        }
      } finally {
        inFlightTasksRef.current = null
        if (currentReqId === requestIdRef.current) {
          setLoading(false)
        }
        if (pendingTasksRefreshRef.current) {
          pendingTasksRefreshRef.current = false
          void loadTasksAndJobs(slug, true)
        }
      }
    })()

    inFlightTasksRef.current = promise
    return promise
  }

  // 3. On-demand Reports loader
  const loadReports = async (slug: string, silent = false, force = false): Promise<void> => {
    if (!slug) return
    if (!force && inFlightReportsRef.current) {
      return inFlightReportsRef.current
    }
    if (!silent) setSectionLoading('reports')

    const promise = (async () => {
      try {
        const reps = await api.listProjectReports(slug)
        setReports(reps || [])
        setLoadedSections(prev => ({ ...prev, reports: true }))
      } catch (err) {
        if (!silent) setError(err instanceof Error ? err.message : 'Failed to load evaluation reports')
      } finally {
        inFlightReportsRef.current = null
        setSectionLoading(prev => (prev === 'reports' ? null : prev))
      }
    })()

    inFlightReportsRef.current = promise
    return promise
  }

  // 4. On-demand Cassettes loader
  const loadCassettes = async (slug: string, silent = false, force = false): Promise<void> => {
    if (!slug) return
    if (!force && inFlightCassettesRef.current) {
      return inFlightCassettesRef.current
    }
    if (!silent) setSectionLoading('cassettes')

    const promise = (async () => {
      try {
        const cass = await api.listProjectCassettes(slug)
        setCassettes(cass || [])
        setLoadedSections(prev => ({ ...prev, cassettes: true }))
      } catch (err) {
        if (!silent) setError(err instanceof Error ? err.message : 'Failed to load recorded cassettes')
      } finally {
        inFlightCassettesRef.current = null
        setSectionLoading(prev => (prev === 'cassettes' ? null : prev))
      }
    })()

    inFlightCassettesRef.current = promise
    return promise
  }

  // 5. On-demand Config loader
  const loadConfig = async (slug: string, silent = false, force = false): Promise<void> => {
    if (!slug) return
    if (!force && inFlightConfigRef.current) {
      return inFlightConfigRef.current
    }
    if (!silent) setSectionLoading('config')

    const promise = (async () => {
      try {
        const cfg = await api.getProjectHarnessConfig(slug)
        setConfig(normalizeHarnessConfig(cfg))
        setLoadedSections(prev => ({ ...prev, config: true }))
      } catch (err) {
        if (!silent) setError(err instanceof Error ? err.message : 'Failed to load harness config')
      } finally {
        inFlightConfigRef.current = null
        setSectionLoading(prev => (prev === 'config' ? null : prev))
      }
    })()

    inFlightConfigRef.current = promise
    return promise
  }

  // 6. Loader for the current active subtab
  const loadSubTabData = async (slug: string, tab: ProjectSubTab, force = false, silent = false) => {
    if (!slug) return
    switch (tab) {
      case 'tasks':
        if (force || !loadedSections.tasks) {
          await loadTasksAndJobs(slug, silent, force)
        }
        break
      case 'history':
        if (force || !loadedSections.history) {
          await loadHistory(slug, silent, force)
        }
        break
      case 'reports':
        if (force || !loadedSections.reports) {
          await loadReports(slug, silent, force)
        }
        break
      case 'cassettes':
        if (force || !loadedSections.cassettes) {
          await loadCassettes(slug, silent, force)
        }
        break
      case 'config':
        if (force || !loadedSections.config) {
          await loadConfig(slug, silent, force)
        }
        break
    }
  }

  // Single effect to load data when project or subtab changes
  useEffect(() => {
    if (!selectedProjectSlug) return

    // If switching to a different project, reset loaded sections
    if (lastLoadedSlugRef.current !== selectedProjectSlug) {
      lastLoadedSlugRef.current = selectedProjectSlug
      historyRequestIdRef.current++
      setLoadedSections({})
      setHistory([])
      setHistoryError(null)
      setHistoryRetryAttempt(0)
      setLoadingHistory(true)
      setActiveTrajectoryView(null)
      setLoadingTrajectory(false)
      setLoadingHistoryId(null)
      void loadSubTabData(selectedProjectSlug, subTab, true)
    } else {
      void loadSubTabData(selectedProjectSlug, subTab, false)
    }
  }, [selectedProjectSlug, subTab])

  // 1. Real-time SSE Stream for Project Harness Events
  useEffect(() => {
    if (!selectedProjectSlug) return

    // Low-frequency fallback poll (45s): only poll active subtab data!
    const fallbackInterval = setInterval(() => {
      if (document.hidden) return
      void loadTasksAndJobs(selectedProjectSlug, true)
      if (subTab !== 'tasks') {
        void loadSubTabData(selectedProjectSlug, subTab, true, true)
      }
    }, 45000)

    const unsubscribe = api.subscribeProjectEvents(
      selectedProjectSlug,
      (event: HarnessProjectEvent) => {
        if (event.type === 'task_status_changed') {
          const isRunning = event.status.toLowerCase() === 'running'
          setTasks(prev =>
            prev.map(t =>
              t.id === event.taskId ? { ...t, currentlyRunning: isRunning } : t,
            ),
          )
          if (!isRunning) {
            setActiveJobs(prev =>
              prev.filter(
                j =>
                  j.taskId !== event.taskId &&
                  (!event.jobId || j.jobId !== event.jobId),
              ),
            )
            setRunningTaskId(prev => (prev === event.taskId ? null : prev))
          } else if (event.jobId) {
            startLiveInspect(event.jobId, event.taskId)
            setSubTab('trajectory')
          }
          void loadTasksAndJobs(selectedProjectSlug, true)
          if (subTab === 'history') {
            void loadHistory(selectedProjectSlug, true)
          }
        } else if (event.type === 'task_upserted' || event.type === 'task_deleted') {
          void loadTasksAndJobs(selectedProjectSlug, true)
        } else if (event.type === 'report_generated') {
          void loadReports(selectedProjectSlug, true)
        } else if (event.type === 'overview_invalidated') {
          void loadSubTabData(selectedProjectSlug, subTab, true, true)
        }
      },
    )

    return () => {
      clearInterval(fallbackInterval)
      unsubscribe()
    }
  }, [selectedProjectSlug, subTab, api])

  const handleSelectHistory = async (item: HarnessHistorySummary) => {
    if (!selectedProjectSlug) return
    setSelectedHistoryId(item.id)
    setLoadingHistoryId(item.id)
    setLoadingTrajectory(true)
    setSubTab('trajectory')
    stopLiveInspect()
    setError('')
    try {
      const run = await api.getProjectHistoryDetail(selectedProjectSlug, item.fileName)
      setActiveTrajectoryView(run)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load historical run')
    } finally {
      setLoadingHistoryId(null)
      setLoadingTrajectory(false)
    }
  }

  const handleInspectRunningJob = async (job: HarnessActiveJob) => {
    if (!selectedProjectSlug) return

    if (isLiveTrajectoryVisible(job.jobId, inspectingJobId, liveInspect, subTab === 'trajectory')) {
      stopLiveInspect()
      setSubTab('tasks')
      return
    }

    startLiveInspect(job.jobId, job.taskId)
    setLoadingTrajectory(true)
    setSubTab('trajectory')
    setError('')
    try {
      const detail = await inspectProjectHarnessJob(selectedProjectSlug, job.jobId)
      if (inspectingJobIdRef.current !== job.jobId) return
      if (detail.snapshot) {
        setActiveTrajectoryView(detail.snapshot)
      }
    } catch (err) {
      if (inspectingJobIdRef.current !== job.jobId) return
      stopLiveInspect()
      if (subTab === 'trajectory') setSubTab('tasks')
      setError(err instanceof Error ? err.message : 'Failed to inspect running task')
    } finally {
      setLoadingTrajectory(false)
    }
  }

  const handleStopTask = async (taskId: string) => {
    if (!selectedProjectSlug) return
    setStoppingTaskId(taskId)
    setError('')
    try {
      await api.stopProjectTask(selectedProjectSlug, taskId)
      setSuccessMsg(`Task "${taskId}" stopped successfully`)
      if (runningTaskId === taskId) {
        setRunningTaskId(null)
      }
      await loadTasksAndJobs(selectedProjectSlug, true)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to stop task')
    } finally {
      setStoppingTaskId(null)
    }
  }

  const handleStopRunningJob = async (job: HarnessActiveJob) => {
    if (!selectedProjectSlug) return
    setStoppingTaskId(job.taskId)
    setError('')
    try {
      await api.stopProjectHarnessJob(selectedProjectSlug, job.jobId)
      setSuccessMsg(`Task "${job.taskId}" stopped successfully`)
      if (runningTaskId === job.taskId) {
        setRunningTaskId(null)
      }
      if (inspectingJobId === job.jobId) {
        stopLiveInspect()
      }
      await loadTasksAndJobs(selectedProjectSlug, true)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to stop task')
    } finally {
      setStoppingTaskId(null)
    }
  }

  // 2. Real-time SSE Stream for Live Job Inspection
  useEffect(() => {
    if (!selectedProjectSlug || !inspectingJobId) return

    const unsubscribe = api.subscribeJobLiveStream(
      selectedProjectSlug,
      inspectingJobId,
      (event: HarnessJobLiveEvent) => {
        if (event.type === 'snapshot') {
          setActiveTrajectoryView(event.snapshot)
          if (String(event.snapshot.status).toLowerCase() !== 'running') {
            stopLiveInspect()
            void loadTasksAndJobs(selectedProjectSlug, true)
          }
        } else if (event.type === 'step_update') {
          setActiveTrajectoryView(prev => {
            if (!prev) return prev
            const existingSteps = [...prev.trajectory.steps]
            if (event.stepIndex < existingSteps.length) {
              existingSteps[event.stepIndex] = event.step
            } else {
              existingSteps.push(event.step)
            }
            const toolCalls = existingSteps.reduce(
              (acc, s) =>
                acc + (Array.isArray(s.toolResults) ? s.toolResults.length : 0),
              0,
            )
            return {
              ...prev,
              stepCount: existingSteps.length,
              toolCallsCount: toolCalls,
              durationMs: event.durationMs || prev.durationMs,
              trajectory: {
                ...prev.trajectory,
                steps: existingSteps,
              },
            }
          })
        } else if (event.type === 'finished') {
          setActiveTrajectoryView(prev =>
            prev
              ? {
                  ...prev,
                  status: event.status,
                  durationMs: event.durationMs,
                  finalAnswer: event.finalAnswer ?? prev.finalAnswer,
                }
              : prev,
          )
          stopLiveInspect()
          void loadTasksAndJobs(selectedProjectSlug, true)
        }
      },
      () => {
        stopLiveInspect()
        void loadTasksAndJobs(selectedProjectSlug, true)
      },
      async () => {
        const taskId = inspectingTaskId
        stopLiveInspect()
        try {
          const historyList = await api.listProjectHistory(selectedProjectSlug)
          const latest = historyList.find(item => !taskId || item.taskId === taskId)
          if (latest) {
            const run = await api.getProjectHistoryDetail(selectedProjectSlug, latest.fileName)
            setActiveTrajectoryView(run)
            setSelectedHistoryId(latest.id)
          }
        } catch {
          // Retain last live snapshot if history is not yet available
        }
        void loadTasksAndJobs(selectedProjectSlug, true)
      },
    )

    return () => {
      unsubscribe()
    }
  }, [selectedProjectSlug, inspectingJobId, inspectingTaskId, api])

  const handleOpenCreateTask = () => {
    setEditingTaskId(null)
    setTaskForm({
      id: `task-${Date.now().toString().slice(-6)}`,
      name: '',
      category: '',
      tags: [],
      prompt: '',
      maxTurns: config?.execution.maxTurns || 25,
      timeoutSeconds: config?.execution.timeoutSeconds || 300,
      cron: '',
      schedulePaused: false,
      autoDecompose: true,
      phaseMaxTurns: splitSynthesizedPhaseTurns(config?.execution.maxTurns || 25),
    })
    setSchedule({ ...defaultSchedule })
    setTaskTagsInput('')
    setVerifierType('none')
    setVerifierCommand('')
    setVerifierExpectedStdout('')
    setVerifierExpectedExitCode(0)
    setVerifierFilePath('')
    setVerifierExpectedContent('')
    setEstimateInfo(null)
    setEstimateFeedback('')
    setTaskAttachments([])
    setExistingAttachmentMentions([])
    setIsEditingTask(true)
  }

  const loadTaskMentionSources = useCallback(async () => {
    if (!selectedProjectSlug) return
    setLoadingTaskMentions(true)
    try {
      const fileItems = await scanProjectFiles(api, selectedProjectSlug)
      let agentItems: MentionDisplayItem[] = []
      try {
        const projectDetail = await api.getProject(selectedProjectSlug, { lazy: true })
        if (projectDetail?.project?.path) {
          agentItems = await fetchAgentMentions('', '', projectDetail.project.path)
        }
      } catch {
        // fileItems will still be populated
      }
      setTaskMentionItems([...agentItems, ...fileItems])
    } catch (err) {
      console.debug('Failed to load task mention sources:', err)
    } finally {
      setLoadingTaskMentions(false)
    }
  }, [api, selectedProjectSlug])

  useEffect(() => {
    if (isEditingTask) {
      void loadTaskMentionSources()
    }
  }, [isEditingTask, loadTaskMentionSources])

  useEffect(() => {
    if (!isEditingTask || !selectedProjectSlug || !taskForm.id.trim()) {
      setExistingAttachmentMentions([])
      return
    }

    let cancelled = false
    void (async () => {
      const paths = await listTaskAttachmentPaths(
        path => api.listFiles(selectedProjectSlug, path),
        taskForm.id,
      )
      if (cancelled) return
      setExistingAttachmentMentions(mentionItemsFromAttachmentPaths(paths))
    })()

    return () => {
      cancelled = true
    }
  }, [api, isEditingTask, selectedProjectSlug, taskForm.id])

  const pendingAttachmentMentions = useMemo(
    () =>
      mentionItemsFromAttachmentPaths(
        workspacePathsForAttachments(
          taskAttachments.map(file => file.name),
          taskForm.id,
          existingAttachmentMentions.map(item => item.relativePath),
        ),
      ),
    [taskAttachments, taskForm.id, existingAttachmentMentions],
  )

  const [serverSearchTaskItems, setServerSearchTaskItems] = useState<MentionDisplayItem[]>([])
  const [isSearchingTaskServer, setIsSearchingTaskServer] = useState(false)

  useEffect(() => {
    const trimmed = taskMentionPopover.query.trim()
    if (!taskMentionPopover.isOpen || !trimmed || !selectedProjectSlug) {
      setServerSearchTaskItems([])
      return
    }

    let cancelled = false
    setIsSearchingTaskServer(true)

    const timer = setTimeout(async () => {
      try {
        const results = await searchProjectFiles(api, selectedProjectSlug, trimmed)
        if (!cancelled) {
          setServerSearchTaskItems(results)
        }
      } catch (err) {
        console.debug('Failed to search task mention files on server:', err)
      } finally {
        if (!cancelled) {
          setIsSearchingTaskServer(false)
        }
      }
    }, 120)

    return () => {
      cancelled = true
      clearTimeout(timer)
      setIsSearchingTaskServer(false)
    }
  }, [api, selectedProjectSlug, taskMentionPopover.isOpen, taskMentionPopover.query])

  const combinedTaskMentionItems = useMemo(
    () =>
      mergeMentionItems([
        serverSearchTaskItems,
        pendingAttachmentMentions,
        existingAttachmentMentions,
        taskMentionItems,
      ]),
    [serverSearchTaskItems, pendingAttachmentMentions, existingAttachmentMentions, taskMentionItems],
  )

  const updateTaskMentionState = useCallback(
    (text: string, cursorPos: number) => {
      const trigger = detectMentionTrigger(text, cursorPos)
      if (!trigger) {
        dismissedTaskMentionRef.current = null
        setTaskMentionPopover(prev => (prev.isOpen ? { ...prev, isOpen: false } : prev))
        return
      }

      if (shouldIgnoreMentionTrigger(trigger, dismissedTaskMentionRef.current)) {
        return
      }

      if (Date.now() - lastDismissedTaskTimeRef.current < 200) {
        return
      }

      dismissedTaskMentionRef.current = null

      if (taskMentionItems.length === 0 && !loadingTaskMentions) {
        void loadTaskMentionSources()
      }

      setTaskMentionPopover({
        isOpen: true,
        query: trigger.query,
        mentionStart: trigger.mentionStart,
        selectedIndex: 0,
      })
    },
    [taskMentionItems.length, loadingTaskMentions, loadTaskMentionSources],
  )

  const debouncedTaskCursorMentionUpdate = useMemo(
    () => debounce((text: string, pos: number) => updateTaskMentionState(text, pos), 50),
    [updateTaskMentionState],
  )

  const handleTaskMentionClose = useCallback(() => {
    setTaskMentionPopover(prev => {
      if (prev.isOpen) {
        dismissedTaskMentionRef.current = {
          mentionStart: prev.mentionStart,
          query: prev.query,
        }
        lastDismissedTaskTimeRef.current = Date.now()
      }
      return { ...prev, isOpen: false }
    })
  }, [])

  const handleTaskPromptChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = e.target.value
    const cursorPos = e.target.selectionStart ?? val.length
    setTaskForm({ ...taskForm, prompt: val })
    updateTaskMentionState(val, cursorPos)
  }

  const handleTaskMentionSelect = (item: MentionDisplayItem) => {
    dismissedTaskMentionRef.current = null
    const { newText, newCursorPos } = applyMentionInsertion(
      taskForm.prompt,
      taskMentionPopover.mentionStart,
      taskMentionPopover.query.length,
      item.insertText,
    )
    applyPromptText(newText, newCursorPos)
  }

  const handleTaskPromptKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (taskMentionPopover.isOpen && taskMentionPopoverRef.current) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        const displayItems = taskMentionPopoverRef.current.getDisplayItems()
        const maxIndex = Math.max(0, displayItems.length - 1)
        setTaskMentionPopover(prev => ({
          ...prev,
          selectedIndex: Math.min(prev.selectedIndex + 1, maxIndex),
        }))
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setTaskMentionPopover(prev => ({
          ...prev,
          selectedIndex: Math.max(prev.selectedIndex - 1, 0),
        }))
        return
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        const displayItems = taskMentionPopoverRef.current.getDisplayItems()
        if (displayItems.length > 0) {
          e.preventDefault()
          taskMentionPopoverRef.current.selectItem(taskMentionPopover.selectedIndex)
          return
        }
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        dismissedTaskMentionRef.current = {
          mentionStart: taskMentionPopover.mentionStart,
          query: taskMentionPopover.query,
        }
        lastDismissedTaskTimeRef.current = Date.now()
        setTaskMentionPopover(prev => ({ ...prev, isOpen: false }))
        return
      }
    }
  }

  const handleOpenEditTask = async (taskId: string) => {
    if (!selectedProjectSlug) return
    setError('')
    try {
      const detail = await api.getProjectTask(selectedProjectSlug, taskId)
      setEditingTaskId(taskId)
      setTaskForm(detail)
      setSchedule(parseFriendlySchedule(detail.cron))
      setTaskTagsInput(detail.tags ? detail.tags.join(', ') : '')
      if (detail.verifier) {
        if ('command' in detail.verifier) {
          setVerifierType('command')
          setVerifierCommand(detail.verifier.command)
          setVerifierExpectedStdout(detail.verifier.expectedStdout || '')
          setVerifierExpectedExitCode(detail.verifier.expectedExitCode ?? 0)
        } else if ('filePath' in detail.verifier) {
          setVerifierType('diff')
          setVerifierFilePath(detail.verifier.filePath)
          setVerifierExpectedContent(detail.verifier.expectedContent)
        }
      } else {
        setVerifierType('none')
      }
      setEstimateInfo(null)
      setEstimateFeedback('')
      setTaskAttachments([])
      setExistingAttachmentMentions([])
      setIsEditingTask(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to fetch task detail')
    }
  }

  const handleEstimateBudget = async (feedbackOverride?: string) => {
    if (!selectedProjectSlug) return
    if (!taskForm.prompt.trim()) {
      setError('Please enter a task prompt first to estimate budget')
      return
    }
    setEstimatingBudget(true)
    setError('')
    try {
      const feedbackToSend = (feedbackOverride !== undefined ? feedbackOverride : estimateFeedback).trim()
      const res = await api.estimateProjectTask(selectedProjectSlug, {
        prompt: taskForm.prompt,
        category: taskForm.category || undefined,
        feedback: feedbackToSend || undefined,
        previousEstimate: estimateInfo || undefined,
      })
      setEstimateInfo(res)
      setTaskForm(prev => ({
        ...prev,
        maxTurns: res.suggestedMaxTurns,
        timeoutSeconds: res.suggestedTimeoutSeconds,
        phaseMaxTurns: splitSynthesizedPhaseTurns(res.suggestedMaxTurns),
      }))
      setSuccessMsg(
        feedbackToSend
          ? `AI Budget refined with feedback: ~${res.suggestedMaxTurns} turns, ${res.suggestedTimeoutSeconds}s (${res.complexity} complexity)`
          : `AI Budget estimated: ~${res.suggestedMaxTurns} turns, ${res.suggestedTimeoutSeconds}s (${res.complexity} complexity)`,
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to estimate task budget')
    } finally {
      setEstimatingBudget(false)
    }
  }

  const handleAdjustBudgetMultiplier = (multiplier: number) => {
    const currentTurns = taskForm.maxTurns || 25
    const newTurns = Math.max(1, Math.min(500, Math.round(currentTurns * multiplier)))
    const currentTimeout = taskForm.timeoutSeconds || 300
    const newTimeout = Math.max(
      10,
      Math.min(7200, Math.round(currentTimeout * (multiplier > 1 ? multiplier * 0.9 : multiplier * 1.1))),
    )
    setTaskForm(prev => ({
      ...prev,
      maxTurns: newTurns,
      timeoutSeconds: newTimeout,
      phaseMaxTurns: splitSynthesizedPhaseTurns(newTurns),
    }))
    setSuccessMsg(`Budget manually adjusted to ${newTurns} turns (${newTimeout}s)`)
  }

  const handleAttachFiles = (e: React.ChangeEvent<HTMLInputElement>) => {
    const selected = Array.from(e.target.files ?? [])
    const accepted = selected.filter(file => isAcceptedTaskAttachment(file.name))
    if (accepted.length === 0) {
      e.target.value = ''
      if (selected.length > 0) {
        setError('Attach Excel, PDF, or image files only')
      }
      return
    }
    setError('')
    setTaskAttachments(prev => [...prev, ...accepted])
    e.target.value = ''
  }

  const handleRemoveAttachment = (index: number) => {
    setTaskAttachments(prev => prev.filter((_, i) => i !== index))
  }

  const applyPromptText = (newText: string, newCursorPos: number) => {
    setTaskForm(prev => ({ ...prev, prompt: newText }))
    setTaskMentionPopover(prev => ({ ...prev, isOpen: false }))
    if (taskPromptTextareaRef.current) {
      taskPromptTextareaRef.current.value = newText
      taskPromptTextareaRef.current.focus()
      setTimeout(() => {
        if (taskPromptTextareaRef.current) {
          taskPromptTextareaRef.current.setSelectionRange(newCursorPos, newCursorPos)
        }
      }, 0)
    }
  }

  const handleInsertAttachmentMention = (path: string) => {
    dismissedTaskMentionRef.current = null
    const cursorPos =
      taskPromptTextareaRef.current?.selectionStart ?? taskForm.prompt.length
    const trigger = detectMentionTrigger(taskForm.prompt, cursorPos)
    const { newText, newCursorPos } = insertMentionText(
      taskForm.prompt,
      cursorPos,
      `@${path} `,
      trigger,
    )
    applyPromptText(newText, newCursorPos)
  }

  const handleSaveTask = async () => {
    if (!selectedProjectSlug) return
    if (!taskForm.id.trim()) {
      setError('Task ID is required')
      return
    }
    if (!taskForm.prompt.trim()) {
      setError('Task prompt is required')
      return
    }

    const tags = taskTagsInput
      .split(',')
      .map(t => t.trim())
      .filter(Boolean)

    let verifierSpec = undefined
    if (verifierType === 'command') {
      verifierSpec = {
        type: 'command' as const,
        command: verifierCommand,
        expectedExitCode: verifierExpectedExitCode,
        expectedStdout: verifierExpectedStdout || undefined,
      }
    } else if (verifierType === 'diff') {
      verifierSpec = {
        type: 'diff' as const,
        filePath: verifierFilePath,
        expectedContent: verifierExpectedContent,
      }
    }

    setError('')
    setSuccessMsg('')
    setSavingTask(true)
    try {
      let prompt = taskForm.prompt
      if (taskAttachments.length > 0) {
        const files = await Promise.all(
          taskAttachments.map(async file => ({
            name: file.name,
            bytes: new Uint8Array(await file.arrayBuffer()),
          })),
        )
        prompt = await uploadAttachmentsAndComposePrompt({
          prompt: taskForm.prompt,
          taskId: taskForm.id,
          files,
          existingPaths: existingAttachmentMentions.map(item => item.relativePath),
          writeFileBytes: (path, content) =>
            api.writeFileBytes(selectedProjectSlug, path, content),
        })
      }

      const cron = scheduleToCron(schedule)
      const hasExplicitSubtasks = (taskForm.subtasks?.length || 0) >= 2
      const payload: ProjectTaskDefinition = {
        ...taskForm,
        prompt,
        tags,
        verifier: verifierSpec,
        cron,
        schedulePaused: Boolean(cron) && Boolean(taskForm.schedulePaused),
        phaseMaxTurns:
          taskForm.autoDecompose === false || hasExplicitSubtasks
            ? undefined
            : phaseTurnsFromTask(taskForm),
      }

      if (editingTaskId) {
        await api.updateProjectTask(selectedProjectSlug, editingTaskId, payload)
        setSuccessMsg(`Task ${taskForm.id} updated successfully`)
      } else {
        await api.createProjectTask(selectedProjectSlug, payload)
        setSuccessMsg(`Task ${taskForm.id} created successfully`)
      }
      setTaskAttachments([])
      setIsEditingTask(false)
      void loadTasksAndJobs(selectedProjectSlug, true)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save task')
    } finally {
      setSavingTask(false)
    }
  }

  const handleToggleSchedulePause = async (task: ProjectTaskSummary) => {
    if (!selectedProjectSlug || !task.cron) return
    setError('')
    setSuccessMsg('')
    try {
      await api.patchProjectTaskSchedule(selectedProjectSlug, task.id, {
        paused: !task.schedulePaused,
      })
      setSuccessMsg(
        task.schedulePaused
          ? `Task ${task.id} will run on its schedule again`
          : `Task ${task.id} will stay saved, but won't run by itself until you turn auto-run back on`,
      )
      await loadTasksAndJobs(selectedProjectSlug)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update task schedule')
    }
  }

  const handleDeleteTask = async (taskId: string) => {
    if (!selectedProjectSlug) return
    if (!confirm(`Are you sure you want to delete task "${taskId}"?`)) return
    setError('')
    setSuccessMsg('')
    try {
      await api.deleteProjectTask(selectedProjectSlug, taskId)
      setSuccessMsg(`Task ${taskId} deleted`)
      await loadTasksAndJobs(selectedProjectSlug)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete task')
    }
  }

  const handleRunTask = async (
    taskId: string,
    record = false,
    customMaxTurns?: number,
    continueFromRunId?: string,
    extraTurns?: number,
    extraPrompt?: string,
  ) => {
    if (!selectedProjectSlug) return
    setRunningTaskId(taskId)
    setError('')
    setSuccessMsg('')
    try {
      const res = await api.runProjectTask(selectedProjectSlug, taskId, {
        record,
        maxTurns: continueFromRunId ? undefined : customMaxTurns,
        continueFromRunId,
        extraTurns: continueFromRunId ? extraTurns ?? 25 : extraTurns,
        extraPrompt: extraPrompt?.trim() || undefined,
      })
      setActiveTrajectoryView(res)
      stopLiveInspect()
      if (onInspectTrajectory) {
        onInspectTrajectory(res)
      }
      setSuccessMsg(`Task ${taskId} completed with status: ${res.status}`)
      await loadTasksAndJobs(selectedProjectSlug, true)
      const historyList = await api.listProjectHistory(selectedProjectSlug)
      setHistory(historyList)
      const latest = historyList.find(item => item.taskId === taskId)
      if (latest) {
        setSelectedHistoryId(latest.id)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : `Failed to run task ${taskId}`)
    } finally {
      setRunningTaskId(null)
    }
  }

  const openDynamicRun = async (task: ProjectTaskSummary, record: boolean) => {
    if (!selectedProjectSlug) return
    setDynamicRun({
      taskId: task.id,
      taskName: task.name,
      record,
      prompt: '',
      extra: '',
      loadingPrompt: true,
      error: '',
    })
    try {
      const detail = await api.getProjectTask(selectedProjectSlug, task.id)
      setDynamicRun(prev =>
        prev && prev.taskId === task.id
          ? { ...prev, prompt: detail.prompt, loadingPrompt: false }
          : prev,
      )
    } catch (err) {
      setDynamicRun(prev =>
        prev && prev.taskId === task.id
          ? {
              ...prev,
              loadingPrompt: false,
              error: err instanceof Error ? err.message : 'Failed to load task prompt',
            }
          : prev,
      )
    }
  }

  const confirmDynamicRun = async () => {
    if (!dynamicRun || dynamicRun.loadingPrompt || dynamicRun.error) return
    const { taskId, record, extra } = dynamicRun
    setDynamicRun(null)
    await handleRunTask(taskId, record, undefined, undefined, undefined, extra)
  }

  const handleOpenContinueModal = (
    taskId: string,
    previousSteps = 25,
    taskName?: string,
    runId?: string,
    progressSummary?: string,
  ) => {
    setContinueModalTask({
      taskId,
      runId,
      taskName: taskName || tasks.find(t => t.id === taskId)?.name || taskId,
      previousSteps,
      progressSummary,
    })
    setContinueTurnsMode('additional')
    setContinueTurnsInput(25)
    setContinueRecord(false)
  }

  const handleCloseContinueModal = () => {
    setContinueModalTask(null)
  }

  const handleConfirmContinueTask = async () => {
    if (!continueModalTask) return
    const extraTurns = extraTurnsForContinue({
      previousSteps: continueModalTask.previousSteps,
      mode: continueTurnsMode,
      value: continueTurnsInput,
    })

    const taskId = continueModalTask.taskId
    const runId = continueModalTask.runId
    const record = continueRecord
    handleCloseContinueModal()
    await handleRunTask(taskId, record, undefined, runId, extraTurns)
  }

  const handleRunProjectEval = async () => {
    if (!selectedProjectSlug) return
    setRunningEval(true)
    setError('')
    setSuccessMsg('')
    try {
      const report = await api.runProjectHarnessEval(selectedProjectSlug, {})
      setSelectedReportDetail(report)
      setSuccessMsg(
        `Project eval completed! Pass rate: ${(report.metrics.passRate * 100).toFixed(1)}% (${report.metrics.passedTasks}/${report.metrics.totalTasks})`,
      )
      await loadReports(selectedProjectSlug, true)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Project evaluation failed')
    } finally {
      setRunningEval(false)
    }
  }

  const handleSaveConfig = async () => {
    if (!selectedProjectSlug || !config) return
    setSavingConfig(true)
    setError('')
    setSuccessMsg('')
    try {
      const saved = await api.updateProjectHarnessConfig(selectedProjectSlug, config)
      setConfig(saved)
      setSuccessMsg('Project harness configuration saved to .goose/harness.yaml')
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save harness configuration')
    } finally {
      setSavingConfig(false)
    }
  }

  const filteredTasks = filterAndSortProjectTasks(tasks, {
    query: filterQuery,
    tag: filterTag,
  })

  const taskNamesById = useMemo(
    () => Object.fromEntries(tasks.map(t => [t.id, t.name])),
    [tasks],
  )

  const allTags = Array.from(new Set(tasks.flatMap(t => (Array.isArray(t.tags) ? t.tags : []))))

  useEffect(() => {
    if (!dynamicRun) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setDynamicRun(null)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [dynamicRun])

  return (
    <div className="project-harness-container">
      {/* Project Header / Selector Bar */}
      <div className="harness-card project-selector-card">
        <div className="project-selector-header">
          <div className="project-selector-info">
            <span className="project-label">PROJECT HARNESS:</span>
            {projects.length > 1 && onSelectProject ? (
              <select
                className="form-input project-select-dropdown"
                value={selectedProjectSlug}
                onChange={e => onSelectProject(e.target.value)}
              >
                {projects.map(p => (
                  <option key={p.slug} value={p.slug}>
                    {p.title || p.slug} ({p.path})
                  </option>
                ))}
              </select>
            ) : (
              <span className="project-current-name">
                <strong>{selectedProject?.title || selectedProjectSlug}</strong>
                {selectedProject?.path && <span className="project-path-sub"> ({selectedProject.path})</span>}
              </span>
            )}
            {selectedProject && (
              <span className={`status-tag status-${selectedProject.status}`}>
                {selectedProject.status}
              </span>
            )}
          </div>
          <div className="project-selector-actions">
            <button
              type="button"
              className="btn-secondary"
              onClick={() => {
                if (selectedProjectSlug) {
                  void loadSubTabData(selectedProjectSlug, subTab, true)
                }
              }}
              disabled={loading || Boolean(sectionLoading) || runningEval || runningTaskId !== null}
            >
              🔄 Refresh
            </button>
          </div>
        </div>

        <div className="project-subnav-tabs">
          <button
            type="button"
            className={`project-subnav-btn ${subTab === 'tasks' ? 'active' : ''}`}
            onClick={() => setSubTab('tasks')}
          >
            📋 Project Tasks ({tasks.length})
          </button>
          <button
            type="button"
            className={`project-subnav-btn ${subTab === 'history' ? 'active' : ''}`}
            onClick={() => setSubTab('history')}
          >
            📜 Task History {loadedSections.history ? `(${history.length})` : ''}
          </button>
          <button
            type="button"
            className={`project-subnav-btn ${subTab === 'reports' ? 'active' : ''}`}
            onClick={() => setSubTab('reports')}
          >
            📊 Eval Reports {loadedSections.reports ? `(${reports.length})` : ''}
          </button>
          <button
            type="button"
            className={`project-subnav-btn ${subTab === 'config' ? 'active' : ''}`}
            onClick={() => setSubTab('config')}
          >
            ⚙️ Harness Config
          </button>
          <button
            type="button"
            className={`project-subnav-btn ${subTab === 'cassettes' ? 'active' : ''}`}
            onClick={() => setSubTab('cassettes')}
          >
            📼 Cassettes {loadedSections.cassettes ? `(${cassettes.length})` : ''}
          </button>
          {(activeTrajectoryView || loadingTrajectory) && (
            <button
              type="button"
              className={`project-subnav-btn ${subTab === 'trajectory' ? 'active' : ''}`}
              onClick={() => setSubTab('trajectory')}
            >
              {loadingTrajectory && !activeTrajectoryView ? (
                <span style={{ display: 'inline-flex', alignItems: 'center', gap: '0.4rem' }}>
                  <Loader2 size={13} className="spinning text-accent" />
                  <span>Loading Agent Log…</span>
                </span>
              ) : (
                <span style={{ display: 'inline-flex', alignItems: 'center', gap: '0.4rem' }}>
                  <span>🧠 {liveInspect ? 'Live' : 'Last Run'} Agent Log ({activeTrajectoryView?.taskId ?? '…'})</span>
                  {loadingTrajectory && (
                    <Loader2 size={13} className="spinning text-accent" />
                  )}
                </span>
              )}
            </button>
          )}
        </div>
      </div>

      {error && <div className="error-banner">{error}</div>}
      {successMsg && <div className="success-banner">{successMsg}</div>}

      {activeJobs.length > 0 && (
        <div className="active-jobs-banner">
          <div className="active-jobs-header">
            <span className="live-dot" />
            <strong>Currently Running Project Tasks ({activeJobs.length})</strong>
          </div>
          <div className="active-jobs-list">
            {activeJobs.map(job => (
              <div key={job.jobId} className="active-job-item">
                <div className="active-job-row">
                  <div className="active-job-meta">
                    <span className="job-type-pill">{job.jobType.toUpperCase()}</span>
                    <span className="job-task-name">{job.taskId}</span>
                    <span className="job-time">
                      Started {new Date(job.startedAt).toLocaleTimeString()}
                    </span>
                  </div>
                  <div style={{ display: 'flex', gap: '0.4rem', alignItems: 'center' }}>
                    <InspectRunningJobButton
                      inspectingThis={isLiveTrajectoryVisible(
                        job.jobId,
                        inspectingJobId,
                        liveInspect,
                        subTab === 'trajectory',
                      )}
                      loading={inspectingJobId === job.jobId && loadingTrajectory}
                      onClick={() => void handleInspectRunningJob(job)}
                    />
                    <button
                      type="button"
                      className="btn-action stop-btn"
                      onClick={() => void handleStopRunningJob(job)}
                      disabled={stoppingTaskId === job.taskId}
                      title="Stop this running task"
                    >
                      {stoppingTaskId === job.taskId ? '⏳ Stopping…' : '⏹ Stop'}
                    </button>
                  </div>
                </div>
                <div className="active-job-desc">{job.description}</div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* ================= SUBTAB: TASKS ================= */}
      {subTab === 'tasks' && (
        <div className="project-tasks-view">
          <div className="tasks-header-bar">
            <div className="tasks-search-row">
              <input
                type="text"
                className="form-input search-input"
                placeholder="Search tasks by ID, name, category..."
                value={filterQuery}
                onChange={e => setFilterQuery(e.target.value)}
              />
              {allTags.length > 0 && (
                <div className="tags-filter-bar">
                  <span className="chips-label">Tag:</span>
                  <button
                    type="button"
                    className={`chip-btn ${filterTag === '' ? 'selected' : ''}`}
                    onClick={() => setFilterTag('')}
                  >
                    All
                  </button>
                  {allTags.map(tag => (
                    <button
                      key={tag}
                      type="button"
                      className={`chip-btn ${filterTag === tag ? 'selected' : ''}`}
                      onClick={() => setFilterTag(tag)}
                    >
                      {tag}
                    </button>
                  ))}
                </div>
              )}
            </div>

            <div className="tasks-actions-row">
              <button
                type="button"
                className="btn-secondary"
                onClick={() => setSubTab('history')}
              >
                📜 Task History ({history.length})
              </button>
              <button
                type="button"
                className="btn-primary"
                onClick={handleOpenCreateTask}
              >
                ➕ New Project Task
              </button>
              <button
                type="button"
                className="btn-secondary"
                onClick={handleRunProjectEval}
                disabled={runningEval || tasks.length === 0}
              >
                {runningEval ? '⏳ Running Eval...' : '🚀 Run Batch Eval'}
              </button>
            </div>
          </div>

          {/* Task Editor Modal / Section */}
          {isEditingTask && (
            <div className="harness-card task-editor-card">
              <h3 className="section-title">
                {editingTaskId ? `✏️ Edit Task: ${editingTaskId}` : '➕ Create New Project Task'}
              </h3>
              <div className="form-grid">
                <div className="form-row-2">
                  <div className="form-group">
                    <label className="form-label">Task ID (Filename Slug)</label>
                    <input
                      type="text"
                      className="form-input"
                      value={taskForm.id}
                      onChange={e => setTaskForm({ ...taskForm, id: e.target.value })}
                      placeholder="e.g. fetch-polymarket-data"
                      disabled={!!editingTaskId}
                    />
                  </div>
                  <div className="form-group">
                    <label className="form-label">Task Display Name</label>
                    <input
                      type="text"
                      className="form-input"
                      value={taskForm.name || ''}
                      onChange={e => setTaskForm({ ...taskForm, name: e.target.value })}
                      placeholder="e.g. Fetch Polymarket Live Markets"
                    />
                  </div>
                </div>

                <div className="form-row-2">
                  <div className="form-group">
                    <label className="form-label">Category / Suite</label>
                    <input
                      type="text"
                      className="form-input"
                      value={taskForm.category || ''}
                      onChange={e => setTaskForm({ ...taskForm, category: e.target.value })}
                      placeholder="e.g. live-data-api or web3"
                    />
                  </div>
                  <div className="form-group">
                    <label className="form-label">Tags (Comma Separated)</label>
                    <input
                      type="text"
                      className="form-input"
                      value={taskTagsInput}
                      onChange={e => setTaskTagsInput(e.target.value)}
                      placeholder="e.g. python, api, regression"
                    />
                  </div>
                </div>

                <div className="form-group mention-input-group" style={{ position: 'relative' }}>
                  <label className="form-label">Task Instructions / Prompt</label>
                  <MentionPopover
                    ref={taskMentionPopoverRef}
                    isOpen={taskMentionPopover.isOpen}
                    onClose={handleTaskMentionClose}
                    onSelect={handleTaskMentionSelect}
                    position={{ x: 0, y: 0 }}
                    query={taskMentionPopover.query}
                    selectedIndex={taskMentionPopover.selectedIndex}
                    onSelectedIndexChange={index =>
                      setTaskMentionPopover(prev => ({ ...prev, selectedIndex: index }))
                    }
                    items={combinedTaskMentionItems}
                    loading={loadingTaskMentions || isSearchingTaskServer}
                  />
                  <textarea
                    ref={taskPromptTextareaRef}
                    className="form-textarea"
                    rows={4}
                    value={taskForm.prompt}
                    onChange={handleTaskPromptChange}
                    onKeyUp={e => {
                      if (
                        e.key === 'Escape' ||
                        e.key === 'ArrowUp' ||
                        e.key === 'ArrowDown' ||
                        e.key === 'Enter' ||
                        e.key === 'Tab' ||
                        e.key === 'Shift' ||
                        e.key === 'Control' ||
                        e.key === 'Alt' ||
                        e.key === 'Meta'
                      ) {
                        return
                      }
                      const cursorPos = e.currentTarget.selectionStart ?? taskForm.prompt.length
                      debouncedTaskCursorMentionUpdate(taskForm.prompt, cursorPos)
                    }}
                    onClick={e => {
                      const cursorPos = e.currentTarget.selectionStart ?? taskForm.prompt.length
                      debouncedTaskCursorMentionUpdate(taskForm.prompt, cursorPos)
                    }}
                    onKeyDown={handleTaskPromptKeyDown}
                    placeholder="Instructions for the agent to execute in the project repository workspace (type @ to mention attached files, repo files, rules, or agents)..."
                  />
                </div>

                <div className="form-group task-attach-row">
                  <label className="form-label">Attach files (Excel, PDF, images)</label>
                  <input
                    ref={taskAttachInputRef}
                    type="file"
                    className="task-attach-input"
                    accept={TASK_ATTACHMENT_ACCEPT}
                    multiple
                    onChange={handleAttachFiles}
                  />
                  <div className="task-attach-controls">
                    <button
                      type="button"
                      className="btn-secondary"
                      onClick={() => taskAttachInputRef.current?.click()}
                      disabled={savingTask}
                    >
                      <Paperclip size={16} /> Attach files
                    </button>
                    <span className="form-hint">
                      Files are saved into the project workspace. Type @ or click a file name to mention it in the prompt.
                    </span>
                  </div>
                  {(existingAttachmentMentions.length > 0 || taskAttachments.length > 0) && (
                    <ul className="task-attach-list">
                      {existingAttachmentMentions.map(item => (
                        <li
                          key={`existing-${item.relativePath}`}
                          className="task-attach-chip existing"
                        >
                          <button
                            type="button"
                            className="task-attach-mention"
                            title={`Mention @${item.relativePath} in the prompt`}
                            onClick={() => handleInsertAttachmentMention(item.relativePath)}
                          >
                            {item.name}
                          </button>
                        </li>
                      ))}
                      {pendingAttachmentMentions.map((item, index) => (
                        <li key={`pending-${item.relativePath}`} className="task-attach-chip">
                          <button
                            type="button"
                            className="task-attach-mention"
                            title={`Mention @${item.relativePath} in the prompt`}
                            onClick={() => handleInsertAttachmentMention(item.relativePath)}
                          >
                            {item.name}
                          </button>
                          <button
                            type="button"
                            aria-label={`Remove ${item.name}`}
                            onClick={() => handleRemoveAttachment(index)}
                            disabled={savingTask}
                          >
                            <X size={14} />
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>

                {/* Verifier Settings */}
                <div className="verifier-section">
                  <label className="form-label">Automated Evaluation Verifier</label>
                  <div className="verifier-type-toggle">
                    <label>
                      <input
                        type="radio"
                        name="verifierType"
                        checked={verifierType === 'none'}
                        onChange={() => setVerifierType('none')}
                      />{' '}
                      None (Manual)
                    </label>
                    <label>
                      <input
                        type="radio"
                        name="verifierType"
                        checked={verifierType === 'command'}
                        onChange={() => setVerifierType('command')}
                      />{' '}
                      Command Assertion (Shell)
                    </label>
                    <label>
                      <input
                        type="radio"
                        name="verifierType"
                        checked={verifierType === 'diff'}
                        onChange={() => setVerifierType('diff')}
                      />{' '}
                      File Content Diff (Patch)
                    </label>
                  </div>

                  {verifierType === 'command' && (
                    <div className="verifier-subform">
                      <div className="form-group">
                        <label className="form-label">Test Command</label>
                        <input
                          type="text"
                          className="form-input"
                          value={verifierCommand}
                          onChange={e => setVerifierCommand(e.target.value)}
                          placeholder={"e.g. python3 -c 'import json; assert len(json.load(open(\"markets.json\"))) >= 5; print(\"PASS\")'"}
                        />
                      </div>
                      <div className="form-row-2">
                        <div className="form-group">
                          <label className="form-label">Expected Stdout Substring</label>
                          <input
                            type="text"
                            className="form-input"
                            value={verifierExpectedStdout}
                            onChange={e => setVerifierExpectedStdout(e.target.value)}
                            placeholder="e.g. PASS"
                          />
                        </div>
                        <div className="form-group">
                          <label className="form-label">Expected Exit Code</label>
                          <input
                            type="number"
                            className="form-input"
                            value={verifierExpectedExitCode}
                            onChange={e => setVerifierExpectedExitCode(Number(e.target.value) || 0)}
                          />
                        </div>
                      </div>
                    </div>
                  )}

                  {verifierType === 'diff' && (
                    <div className="verifier-subform">
                      <div className="form-group">
                        <label className="form-label">Target File Path</label>
                        <input
                          type="text"
                          className="form-input"
                          value={verifierFilePath}
                          onChange={e => setVerifierFilePath(e.target.value)}
                          placeholder="e.g. src/index.ts or markets.json"
                        />
                      </div>
                      <div className="form-group">
                        <label className="form-label">Expected Content Contains</label>
                        <textarea
                          className="form-textarea"
                          rows={3}
                          value={verifierExpectedContent}
                          onChange={e => setVerifierExpectedContent(e.target.value)}
                          placeholder="Expected snippet or text in the file"
                        />
                      </div>
                    </div>
                  )}
                </div>

                <div className="budget-section">
                  <div className="budget-header-flex">
                    <label className="form-label" style={{ marginBottom: 0 }}>
                      Execution Budget
                    </label>
                    <div className="budget-header-actions">
                      <button
                        type="button"
                        className="btn btn-xs btn-outline budget-estimate-btn"
                        onClick={() => handleEstimateBudget()}
                        disabled={estimatingBudget || !taskForm.prompt.trim()}
                        title="Analyze prompt using AI to estimate max turns and timeout"
                      >
                        {estimatingBudget
                          ? '⏳ Estimating...'
                          : estimateInfo
                          ? '🔄 Re-estimate'
                          : '🪄 AI Estimate Budget'}
                      </button>
                    </div>
                  </div>
                  {estimateInfo && (
                    <div className={`budget-estimate-badge complexity-${estimateInfo.complexity.toLowerCase()}`}>
                      <div className="budget-badge-top">
                        <span className="complexity-tag">{estimateInfo.complexity.toUpperCase()}</span>
                        <span className="budget-source-tag">
                          {estimateInfo.source.includes('model') ? '⚡ AI Model' : '📋 Heuristic'}
                        </span>
                        <span className="budget-turns-tag">
                          ~{estimateInfo.suggestedMaxTurns} turns ({estimateInfo.suggestedTimeoutSeconds}s)
                        </span>
                      </div>
                      <div className="budget-reasoning">{estimateInfo.reasoning}</div>

                      <div className="budget-correction-card">
                        <div className="budget-correction-header">
                          <span className="budget-correction-title">
                            💬 Comment & Correct Estimation
                          </span>
                          <div className="budget-quick-adjust-group">
                            <span className="quick-adjust-label">Quick adjust:</span>
                            <button
                              type="button"
                              className="quick-adjust-btn"
                              onClick={() => handleAdjustBudgetMultiplier(0.75)}
                              title="Decrease max turns by 25%"
                            >
                              −25%
                            </button>
                            <button
                              type="button"
                              className="quick-adjust-btn"
                              onClick={() => handleAdjustBudgetMultiplier(1.25)}
                              title="Increase max turns by 25%"
                            >
                              +25%
                            </button>
                            <button
                              type="button"
                              className="quick-adjust-btn"
                              onClick={() => handleAdjustBudgetMultiplier(1.5)}
                              title="Increase max turns by 50%"
                            >
                              +50%
                            </button>
                            <button
                              type="button"
                              className="quick-adjust-btn"
                              onClick={() => handleAdjustBudgetMultiplier(2.0)}
                              title="Double max turns (2x)"
                            >
                              2x
                            </button>
                          </div>
                        </div>
                        <div className="budget-feedback-row">
                          <input
                            type="text"
                            className="form-input budget-feedback-input"
                            value={estimateFeedback}
                            onChange={e => setEstimateFeedback(e.target.value)}
                            placeholder="Comment or correction (e.g. 'Single file change, reduce turns', 'Slow tests, need 100 turns')..."
                            onKeyDown={e => {
                              if (e.key === 'Enter' && !estimatingBudget && estimateFeedback.trim()) {
                                e.preventDefault()
                                handleEstimateBudget()
                              }
                            }}
                          />
                          <button
                            type="button"
                            className="btn btn-xs btn-primary budget-refine-btn"
                            onClick={() => handleEstimateBudget()}
                            disabled={estimatingBudget || !estimateFeedback.trim()}
                            title="Recalibrate budget with your comments/feedback"
                          >
                            {estimatingBudget ? '⏳ Calibrating...' : '✨ Apply Feedback'}
                          </button>
                        </div>
                      </div>
                    </div>
                  )}
                </div>

                <div className="form-row-2">
                  <div className="form-group">
                    <label className="form-label">Max Turns</label>
                    <input
                      type="number"
                      min={1}
                      max={500}
                      className="form-input"
                      value={taskForm.maxTurns || 25}
                      onChange={e => {
                        const maxTurns = Number(e.target.value) || 25
                        setTaskForm({
                          ...taskForm,
                          maxTurns,
                          phaseMaxTurns: splitSynthesizedPhaseTurns(maxTurns),
                        })
                      }}
                    />
                  </div>
                  <div className="form-group">
                    <label className="form-label">Timeout (Seconds)</label>
                    <input
                      type="number"
                      min={10}
                      max={7200}
                      className="form-input"
                      value={taskForm.timeoutSeconds || 300}
                      onChange={e =>
                        setTaskForm({
                          ...taskForm,
                          timeoutSeconds: Number(e.target.value) || 300,
                        })
                      }
                    />
                  </div>
                </div>

                <div className="form-group" style={{ marginTop: '0.25rem', marginBottom: '1rem' }}>
                  <label style={{ display: 'flex', alignItems: 'flex-start', gap: '0.65rem', cursor: 'pointer' }}>
                    <input
                      type="checkbox"
                      style={{ marginTop: '0.25rem' }}
                      checked={taskForm.autoDecompose !== false}
                      onChange={e =>
                        setTaskForm({
                          ...taskForm,
                          autoDecompose: e.target.checked,
                          phaseMaxTurns: e.target.checked
                            ? phaseTurnsFromTask(taskForm)
                            : undefined,
                        })
                      }
                    />
                    <div>
                      <div style={{ fontWeight: 600, fontSize: '0.875rem' }}>
                        Auto-Decompose Task into Sequential Subtasks (Recommended)
                      </div>
                      <div style={{ fontSize: '0.75rem', opacity: 0.75, marginTop: '0.15rem', lineHeight: '1.35' }}>
                        In Step 1, OpenDuck analyzes the prompt, splits complex tasks into discrete phases (Discovery &amp; DTO &rarr; Core Service &rarr; Integration/Verification), and executes them in sequential order within the shared sandbox workspace.
                      </div>
                    </div>
                  </label>
                </div>

                <div className="form-group" style={{ marginTop: '0.25rem', marginBottom: '1rem' }}>
                  <label style={{ display: 'flex', alignItems: 'flex-start', gap: '0.65rem', cursor: 'pointer' }}>
                    <input
                      type="checkbox"
                      style={{ marginTop: '0.25rem' }}
                      checked={Boolean(taskForm.dynamicPrompt)}
                      onChange={e =>
                        setTaskForm({
                          ...taskForm,
                          dynamicPrompt: e.target.checked,
                        })
                      }
                    />
                    <div>
                      <div style={{ fontWeight: 600, fontSize: '0.875rem' }}>
                        Accept a dynamic prompt before each run
                      </div>
                      <div style={{ fontSize: '0.75rem', opacity: 0.75, marginTop: '0.15rem', lineHeight: '1.35' }}>
                        Run and Record open a dialog with this prompt plus an extra-instructions field.
                        Those instructions apply to that run only. In file review, a selected passage can
                        be sent as the extra prompt from a context menu.
                      </div>
                    </div>
                  </label>
                </div>

                {taskForm.autoDecompose !== false && (taskForm.subtasks?.length || 0) >= 2 && (
                  <div className="form-group" style={{ marginBottom: '1rem' }}>
                    <label className="form-label">Per-phase Max Turns</label>
                    <div style={{ fontSize: '0.75rem', opacity: 0.75, marginBottom: '0.5rem' }}>
                      Explicit subtasks from this task YAML. Each phase runs sequentially in the shared sandbox.
                    </div>
                    <div className="form-row-3">
                      {taskForm.subtasks!.map((subtask, index) => (
                        <div className="form-group" key={subtask.id || index}>
                          <label className="form-label" title={subtask.description}>
                            {subtask.title || `Phase ${index + 1}`}
                          </label>
                          <input
                            type="number"
                            min={1}
                            max={500}
                            className="form-input"
                            value={subtask.maxTurns || 8}
                            onChange={e => {
                              const next = (taskForm.subtasks || []).map((item, i) =>
                                i === index
                                  ? { ...item, maxTurns: Math.max(1, Number(e.target.value) || 1) }
                                  : item,
                              )
                              setTaskForm({ ...taskForm, subtasks: next })
                            }}
                          />
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {taskForm.autoDecompose !== false && (taskForm.subtasks?.length || 0) < 2 && (
                  <div className="form-group" style={{ marginBottom: '1rem' }}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', gap: '0.75rem' }}>
                      <label className="form-label">Per-phase Max Turns</label>
                      <button
                        type="button"
                        className="btn btn-xs btn-secondary"
                        onClick={() =>
                          setTaskForm({
                            ...taskForm,
                            phaseMaxTurns: splitSynthesizedPhaseTurns(taskForm.maxTurns || 25),
                          })
                        }
                        title="Re-split the parent Max Turns across the three synthesized phases (2:3:2)"
                      >
                        Reset to split of Max Turns
                      </button>
                    </div>
                    <div style={{ fontSize: '0.75rem', opacity: 0.75, margin: '0.15rem 0 0.5rem', lineHeight: '1.35' }}>
                      Auto-synthesis splits parent Max Turns ({taskForm.maxTurns || 25}) across three phases using a 2:3:2 ratio (discovery : implementation : verification). Override any phase here.
                    </div>
                    <div className="form-row-3">
                      {SYNTHESIZED_PHASE_LABELS.map((phase, index) => (
                        <div className="form-group" key={phase.title}>
                          <label className="form-label" title={phase.description}>
                            {phase.title}
                          </label>
                          <input
                            type="number"
                            min={1}
                            max={500}
                            className="form-input"
                            value={phaseTurnsFromTask(taskForm)[index]}
                            onChange={e => {
                              const next = [...phaseTurnsFromTask(taskForm)]
                              next[index] = Math.max(1, Number(e.target.value) || 1)
                              setTaskForm({ ...taskForm, phaseMaxTurns: next })
                            }}
                          />
                          <div style={{ fontSize: '0.7rem', opacity: 0.65, marginTop: '0.2rem' }}>
                            {phase.description}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                <div className="form-group">
                  <label className="form-label">Run automatically</label>
                  <select
                    className="form-input"
                    value={schedule.kind}
                    onChange={e =>
                      setSchedule({
                        ...schedule,
                        kind: e.target.value as ScheduleKind,
                      })
                    }
                  >
                    <option value="none">No, only when I click Run</option>
                    <option value="hourly">Every hour</option>
                    <option value="daily">Every day</option>
                    <option value="weekdays">Weekdays (Monday–Friday)</option>
                    <option value="weekly">Once a week</option>
                  </select>
                  {schedule.kind !== 'none' && schedule.kind !== 'hourly' && (
                    <div className="schedule-fields">
                      {schedule.kind === 'weekly' && (
                        <label className="form-group">
                          <span className="form-label">On</span>
                          <select
                            className="form-input"
                            value={schedule.weekday}
                            onChange={e =>
                              setSchedule({
                                ...schedule,
                                weekday: Number(e.target.value),
                              })
                            }
                          >
                            {WEEKDAYS.map((day, index) => (
                              <option key={day} value={index}>
                                {day}
                              </option>
                            ))}
                          </select>
                        </label>
                      )}
                      <label className="form-group">
                        <span className="form-label">At</span>
                        <input
                          type="time"
                          className="form-input"
                          value={timeInputValue(schedule.hour, schedule.minute)}
                          onChange={e =>
                            setSchedule({
                              ...schedule,
                              ...parseTimeInput(e.target.value),
                            })
                          }
                        />
                      </label>
                    </div>
                  )}
                </div>

                <div className="editor-button-row">
                  <button
                    type="button"
                    className="btn-primary"
                    onClick={handleSaveTask}
                    disabled={savingTask}
                  >
                    {savingTask
                      ? 'Saving…'
                      : `💾 Save Task (.goose/tasks/${taskForm.id}.yaml)`}
                  </button>
                  <button
                    type="button"
                    className="btn-secondary"
                    onClick={() => {
                      setTaskAttachments([])
                      setExistingAttachmentMentions([])
                      setIsEditingTask(false)
                    }}
                    disabled={savingTask}
                  >
                    Cancel
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* Tasks List */}
          {filteredTasks.length === 0 ? (
            <div className="empty-tasks-card">
              <p className="empty-text">No tasks found in this project.</p>
              <button
                type="button"
                className="btn-primary"
                onClick={handleOpenCreateTask}
              >
                Create your first Task
              </button>
            </div>
          ) : (
            <div className="tasks-grid">
              {filteredTasks.map(t => (
                <div key={t.id} className="task-card">
                  <div className="task-card-header">
                    <div>
                      <h4 className="task-card-title">{t.name}</h4>
                      <code className="task-card-id">{t.id}</code>
                    </div>
                    <div className="task-badges">
                      {t.category && <span className="badge category-badge">{t.category}</span>}
                      {t.hasVerifier ? (
                        <span className="badge verifier-badge">✓ Verifier</span>
                      ) : (
                        <span className="badge no-verifier-badge">No Verifier</span>
                      )}
                      {t.cron && (
                        t.schedulePaused ? (
                          <span className="badge schedule-paused-badge">Auto-run off</span>
                        ) : (
                          <span className="badge schedule-badge">Auto-runs</span>
                        )
                      )}
                      {t.dynamicPrompt && (
                        <span className="badge dynamic-prompt-badge">Dynamic prompt</span>
                      )}
                    </div>
                  </div>

                  {t.tags.length > 0 && (
                    <div className="task-card-tags">
                      {t.tags.map(tag => (
                        <span key={tag} className="tag-pill">
                          #{tag}
                        </span>
                      ))}
                    </div>
                  )}

                  {t.cron && (
                    <div className="task-card-schedule">
                      <span>{describeTaskSchedule(t.cron, t.schedulePaused)}</span>
                      {!t.schedulePaused && t.nextRunAt && (
                        <span>Next run {new Date(t.nextRunAt).toLocaleString()}</span>
                      )}
                    </div>
                  )}

                  <div className="task-card-path">
                    <span>📄 {t.filePath}</span>
                  </div>

                  <div className="task-card-actions">
                    <div className="task-actions-group task-actions-primary">
                      <button
                        type="button"
                        className="btn-action run-btn"
                        onClick={() => {
                          if (t.dynamicPrompt) {
                            void openDynamicRun(t, false)
                          } else {
                            void handleRunTask(t.id, false)
                          }
                        }}
                        disabled={runningTaskId === t.id || activeJobs.some(job => job.taskId === t.id)}
                        title={
                          runningTaskId === t.id || activeJobs.some(job => job.taskId === t.id)
                            ? 'Task is currently running'
                            : t.dynamicPrompt
                              ? 'Review the current prompt and add extra instructions for this run'
                              : 'Run this task'
                        }
                      >
                        {runningTaskId === t.id || activeJobs.some(job => job.taskId === t.id) ? (
                          <>
                            <Loader2 size={13} className="spinning" />
                            <span>Running...</span>
                          </>
                        ) : (
                          <>
                            <Play size={13} fill="currentColor" />
                            <span>Run</span>
                          </>
                        )}
                      </button>
                      {(() => {
                        const latest = history.find(h => h.taskId === t.id)
                        const canContinue =
                          latest &&
                          (latest.continuable || isContinuableStatus(String(latest.status)))
                        if (!canContinue || !latest) return null
                        const prevSteps = latest.stepCount || 25
                        const isTaskBusy =
                          runningTaskId === t.id || activeJobs.some(job => job.taskId === t.id)
                        return (
                          <div className="continue-btn-group">
                            <button
                              type="button"
                              className="btn-action continue-btn"
                              onClick={() => handleRunTask(t.id, false, undefined, latest.id, 25)}
                              disabled={isTaskBusy}
                              title="New run seeded with compacted progress from the last incomplete run (+25 turns)"
                            >
                              <FastForward size={13} fill="currentColor" />
                              <span>Continue (+25)</span>
                            </button>
                            <button
                              type="button"
                              className="btn-action continue-more-btn"
                              onClick={() =>
                                handleOpenContinueModal(t.id, prevSteps, t.name, latest.id)
                              }
                              disabled={isTaskBusy}
                              title="Continue task with specified extra turns..."
                              aria-label="Configure turns and continue"
                            >
                              <SlidersHorizontal size={13} />
                            </button>
                          </div>
                        )
                      })()}
                      <button
                        type="button"
                        className="btn-action record-btn"
                        onClick={() => {
                          if (t.dynamicPrompt) {
                            void openDynamicRun(t, true)
                          } else {
                            void handleRunTask(t.id, true)
                          }
                        }}
                        disabled={runningTaskId === t.id}
                        title={
                          t.dynamicPrompt
                            ? 'Review the current prompt, add extra instructions, and record a cassette'
                            : 'Run and record cassette for offline deterministic replay'
                        }
                      >
                        <CircleDot size={13} />
                        <span>Record</span>
                      </button>
                      {(() => {
                        const runningJob = activeJobs.find(job => job.taskId === t.id)
                        const isRunning = runningTaskId === t.id || Boolean(runningJob)
                        return (
                          <>
                            {runningJob && (
                              <button
                                type="button"
                                className="btn-action view-btn"
                                onClick={() => void handleInspectRunningJob(runningJob)}
                                disabled={inspectingJobId === runningJob.jobId && loadingTrajectory}
                                title="Inspect the live agent log of this running task"
                              >
                                {inspectingJobId === runningJob.jobId && loadingTrajectory ? (
                                  <>
                                    <Loader2 size={13} className="spinning" />
                                    <span>Loading…</span>
                                  </>
                                ) : (
                                  <>
                                    <Eye size={13} />
                                    <span>Inspect</span>
                                  </>
                                )}
                              </button>
                            )}
                            {isRunning && (
                              <button
                                type="button"
                                className="btn-action stop-btn"
                                onClick={() => {
                                  if (runningJob) {
                                    void handleStopRunningJob(runningJob)
                                  } else {
                                    void handleStopTask(t.id)
                                  }
                                }}
                                disabled={stoppingTaskId === t.id}
                                title="Stop this running task"
                              >
                                {stoppingTaskId === t.id ? (
                                  <>
                                    <Loader2 size={13} className="spinning" />
                                    <span>Stopping…</span>
                                  </>
                                ) : (
                                  <>
                                    <Square size={12} fill="currentColor" />
                                    <span>Stop</span>
                                  </>
                                )}
                              </button>
                            )}
                            {!runningJob && activeTrajectoryView?.taskId === t.id && (
                              <button
                                type="button"
                                className="btn-action view-btn trace-btn"
                                onClick={() => setSubTab('trajectory')}
                                title="View last execution trajectory"
                              >
                                <Activity size={13} />
                                <span>Trace</span>
                              </button>
                            )}
                          </>
                        )
                      })()}
                    </div>

                    <div className="task-actions-group task-actions-secondary">
                      {t.cron && (
                        <button
                          type="button"
                          className={`btn-action pause-btn ${t.schedulePaused ? 'is-paused' : 'is-active'}`}
                          onClick={() => void handleToggleSchedulePause(t)}
                          title={
                            t.schedulePaused
                              ? 'Start running this task on its schedule again'
                              : 'Keep the schedule, but stop running it automatically'
                          }
                        >
                          <Clock size={13} />
                          <span>{t.schedulePaused ? 'Auto-run off' : 'Auto-runs'}</span>
                        </button>
                      )}
                      <button
                        type="button"
                        className="btn-action edit-btn"
                        onClick={() => handleOpenEditTask(t.id)}
                        title="Edit task"
                      >
                        <Pencil size={13} />
                        <span>Edit</span>
                      </button>
                      <button
                        type="button"
                        className="btn-action delete-btn"
                        onClick={() => handleDeleteTask(t.id)}
                        title="Delete task"
                        aria-label="Delete task"
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}

          <div className="harness-card history-card recent-history-card">
            <div className="history-card-header">
              <div>
                <h3 className="section-title" style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                  <span>📜 Recent Task History</span>
                  {isHistoryLoading && (
                    <span
                      title={
                        historyRetryAttempt > 0
                          ? `Retrying recent task history (attempt ${historyRetryAttempt}/2)…`
                          : 'Loading recent task history…'
                      }
                      style={{ display: 'inline-flex', alignItems: 'center' }}
                    >
                      <Loader2 size={15} className="spinning text-accent" />
                    </span>
                  )}
                  {historyError && !isHistoryLoading && (
                    <button
                      type="button"
                      className="btn-action retry-btn"
                      onClick={() => void handleRetryHistory()}
                      title="Retry loading recent task history"
                      style={{ padding: '2px 8px', fontSize: '0.75rem', display: 'inline-flex', alignItems: 'center', gap: '4px' }}
                    >
                      <RotateCcw size={12} />
                      <span>Retry</span>
                    </button>
                  )}
                </h3>
                <p className="section-desc">
                  Completed and failed harness runs for this project. Open a run to inspect its trajectory.
                </p>
              </div>
              {history.length > 5 && (
                <button type="button" className="btn-secondary" onClick={() => setSubTab('history')}>
                  View all {history.length} runs
                </button>
              )}
            </div>

            {historyError && history.length > 0 && !isHistoryLoading && (
              <div
                className="history-error-banner"
                style={{
                  marginBottom: '1rem',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  gap: '0.75rem',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                  <AlertCircle size={16} />
                  <span>Failed to refresh recent task history: {historyError}</span>
                </div>
                <button
                  type="button"
                  className="btn-secondary mini-btn"
                  onClick={() => void handleRetryHistory()}
                  style={{ display: 'inline-flex', alignItems: 'center', gap: '0.35rem' }}
                >
                  <RotateCcw size={12} />
                  <span>Retry</span>
                </button>
              </div>
            )}

            {isHistoryLoading && history.length === 0 ? (
              <div className="history-loading-spinner-box">
                <Loader2 size={24} className="spinning" />
                <span>
                  {historyRetryAttempt > 0
                    ? `Loading recent task history… (retry ${historyRetryAttempt}/2)`
                    : 'Loading recent task history…'}
                </span>
              </div>
            ) : historyError && history.length === 0 ? (
              <div className="history-loading-spinner-box history-error-box">
                <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                  <AlertCircle size={20} className="history-error-icon" />
                  <span className="history-error-title">Failed to load recent task history</span>
                </div>
                <p style={{ margin: 0, fontSize: '0.875rem', color: 'var(--text-secondary)', maxWidth: '420px' }}>
                  {historyError}
                </p>
                <button
                  type="button"
                  className="btn-secondary"
                  onClick={() => void handleRetryHistory()}
                  style={{ display: 'inline-flex', alignItems: 'center', gap: '0.4rem', marginTop: '0.5rem' }}
                >
                  <RotateCcw size={14} />
                  <span>Retry</span>
                </button>
              </div>
            ) : (
              <HarnessHistoryTable
                items={history.slice(0, 5)}
                selectedId={selectedHistoryId}
                loadingRunId={loadingHistoryId}
                taskNames={taskNamesById}
                emptyText="No historical project task runs yet. Run a task above to populate this list."
                onInspect={item => void handleSelectHistory(item)}
                onContinue={item =>
                  handleOpenContinueModal(
                    item.taskId,
                    item.stepCount || 25,
                    historyTaskTitle(item, taskNamesById),
                    item.id,
                    item.progressSummary ?? undefined,
                  )
                }
              />
            )}
          </div>
        </div>
      )}

      {/* ================= SUBTAB: CONFIG ================= */}
      {subTab === 'config' && (
        sectionLoading === 'config' && !config ? (
          <div className="harness-card project-config-card">
            <h3 className="section-title">⚙️ Project Harness Configuration</h3>
            <div className="tab-skeleton-container mt-3">
              <SkeletonBox height="40px" borderRadius="var(--radius-sm)" />
              <SkeletonBox height="180px" className="mt-3" borderRadius="var(--radius-md)" />
            </div>
          </div>
        ) : config ? (
          <div className="harness-card project-config-card">
            <h3 className="section-title">⚙️ Project Harness Configuration</h3>
          <p className="section-desc">
            Project-level execution defaults stored in <code>.goose/harness.yaml</code>.
          </p>

          <div className="form-grid">
            <h4 className="sub-section-title">🤖 Model & Agent Policy Override</h4>
            <div className="form-row-2">
              <div className="form-group">
                <label className="form-label">Default Provider Override</label>
                <input
                  type="text"
                  className="form-input"
                  value={config.policy?.provider || ''}
                  onChange={e =>
                    setConfig({
                      ...config,
                      policy: { ...(config.policy || {}), provider: e.target.value || undefined },
                    })
                  }
                  placeholder="e.g. anthropic or openai (blank to use global default)"
                />
              </div>
              <div className="form-group">
                <label className="form-label">Default Model Override</label>
                <input
                  type="text"
                  className="form-input"
                  value={config.policy?.model || ''}
                  onChange={e =>
                    setConfig({
                      ...config,
                      policy: { ...(config.policy || {}), model: e.target.value || undefined },
                    })
                  }
                  placeholder="e.g. claude-3-7-sonnet (blank to use global default)"
                />
              </div>
            </div>

            <div className="form-group">
              <label className="form-label">Project Custom System Prompt</label>
              <textarea
                className="form-textarea"
                rows={3}
                value={config.policy?.systemPrompt || ''}
                onChange={e =>
                  setConfig({
                    ...config,
                    policy: { ...(config.policy || {}), systemPrompt: e.target.value || undefined },
                  })
                }
                placeholder="Custom agent prompt instructions tailored to this repository..."
              />
            </div>

            <h4 className="sub-section-title">📦 Sandbox & Execution Environment</h4>
            <div className="form-row-2">
              <div className="form-group">
                <label className="form-label">Sandbox Kind</label>
                <select
                  className="form-input"
                  value={config.sandbox?.kind || 'local'}
                  onChange={e =>
                    setConfig({
                      ...config,
                      sandbox: { ...(config.sandbox || { setupCommands: [], envVars: {} }), kind: e.target.value },
                    })
                  }
                >
                  <option value="local">Local (Host Process Sandbox)</option>
                  <option value="container">Container (Docker / OCI)</option>
                </select>
              </div>
              <div className="form-group">
                <label className="form-label">Container Image (When Container Sandbox)</label>
                <input
                  type="text"
                  className="form-input"
                  value={config.sandbox?.containerImage || ''}
                  onChange={e =>
                    setConfig({
                      ...config,
                      sandbox: { ...(config.sandbox || { kind: 'local', setupCommands: [], envVars: {} }), containerImage: e.target.value || undefined },
                    })
                  }
                  placeholder="e.g. ubuntu:24.04 or rust:latest"
                />
              </div>
            </div>

            <div className="form-group">
              <label className="form-label">Setup Commands (One per line)</label>
              <textarea
                className="form-textarea"
                rows={3}
                value={(config.sandbox?.setupCommands || []).join('\n')}
                onChange={e =>
                  setConfig({
                    ...config,
                    sandbox: {
                      ...(config.sandbox || { kind: 'local', envVars: {} }),
                      setupCommands: e.target.value.split('\n').filter(Boolean),
                    },
                  })
                }
                placeholder="e.g. source bin/activate-hermit || true"
              />
            </div>

            <h4 className="sub-section-title">⏱️ Execution Limits & Paths</h4>
            <div className="form-row-3">
              <div className="form-group">
                <label className="form-label">Max Turns</label>
                <input
                  type="number"
                  min={1}
                  max={500}
                  className="form-input"
                  value={config.execution?.maxTurns ?? 25}
                  onChange={e =>
                    setConfig({
                      ...config,
                      execution: {
                        ...(config.execution || { timeoutSeconds: 300, concurrency: 4 }),
                        maxTurns: Number(e.target.value) || 25,
                      },
                    })
                  }
                />
              </div>
              <div className="form-group">
                <label className="form-label">Timeout (Seconds)</label>
                <input
                  type="number"
                  min={10}
                  max={7200}
                  className="form-input"
                  value={config.execution?.timeoutSeconds ?? 300}
                  onChange={e =>
                    setConfig({
                      ...config,
                      execution: {
                        ...(config.execution || { maxTurns: 25, concurrency: 4 }),
                        timeoutSeconds: Number(e.target.value) || 300,
                      },
                    })
                  }
                />
              </div>
              <div className="form-group">
                <label className="form-label">Eval Concurrency</label>
                <input
                  type="number"
                  min={1}
                  max={16}
                  className="form-input"
                  value={config.execution?.concurrency ?? 4}
                  onChange={e =>
                    setConfig({
                      ...config,
                      execution: {
                        ...(config.execution || { maxTurns: 25, timeoutSeconds: 300 }),
                        concurrency: Number(e.target.value) || 4,
                      },
                    })
                  }
                />
              </div>
            </div>

            <div className="form-row-3">
              <div className="form-group">
                <label className="form-label">Tasks Directory</label>
                <input
                  type="text"
                  className="form-input"
                  value={config.paths?.tasksDir || '.goose/tasks'}
                  onChange={e =>
                    setConfig({
                      ...config,
                      paths: { ...(config.paths || { resultsDir: '.goose/harness_results', cassettesDir: '.goose/cassettes' }), tasksDir: e.target.value },
                    })
                  }
                />
              </div>
              <div className="form-group">
                <label className="form-label">Results Directory</label>
                <input
                  type="text"
                  className="form-input"
                  value={config.paths?.resultsDir || '.goose/harness_results'}
                  onChange={e =>
                    setConfig({
                      ...config,
                      paths: { ...(config.paths || { tasksDir: '.goose/tasks', cassettesDir: '.goose/cassettes' }), resultsDir: e.target.value },
                    })
                  }
                />
              </div>
              <div className="form-group">
                <label className="form-label">Cassettes Directory</label>
                <input
                  type="text"
                  className="form-input"
                  value={config.paths?.cassettesDir || '.goose/cassettes'}
                  onChange={e =>
                    setConfig({
                      ...config,
                      paths: { ...(config.paths || { tasksDir: '.goose/tasks', resultsDir: '.goose/harness_results' }), cassettesDir: e.target.value },
                    })
                  }
                />
              </div>
            </div>

            <div className="config-save-row">
              <button
                type="button"
                className="btn-primary"
                onClick={handleSaveConfig}
                disabled={savingConfig}
              >
                {savingConfig ? '⏳ Saving...' : '💾 Save Project Configuration'}
              </button>
            </div>
          </div>
        </div>
      ) : null)}

      {/* ================= SUBTAB: REPORTS ================= */}
      {subTab === 'reports' && (
        <div className="harness-card project-reports-card">
          <h3 className="section-title">📊 Project Evaluation Reports</h3>
          <p className="section-desc">
            Benchmark run histories stored in <code>{config?.paths.resultsDir || '.goose/harness_results'}</code>.
          </p>

          {sectionLoading === 'reports' && !loadedSections.reports ? (
            <div className="tab-skeleton-container" style={{ padding: '1rem 0' }}>
              <SkeletonBox height="40px" borderRadius="var(--radius-sm)" />
              <div className="mt-3" style={{ display: 'flex', flexDirection: 'column', gap: '0.6rem' }}>
                {Array.from({ length: 3 }).map((_, i) => (
                  <SkeletonBox key={i} height="48px" borderRadius="var(--radius-sm)" />
                ))}
              </div>
            </div>
          ) : reports.length === 0 ? (
            <p className="empty-text">No evaluation reports generated for this project yet.</p>
          ) : (
            <div className="reports-table-wrap">
              <table className="harness-table">
                <thead>
                  <tr>
                    <th>Report ID / Timestamp</th>
                    <th>Suite</th>
                    <th>Pass Rate</th>
                    <th>Passed / Total</th>
                    <th>Avg Duration</th>
                    <th>Tool Calls</th>
                    <th>Action</th>
                  </tr>
                </thead>
                <tbody>
                  {reports.map(r => (
                    <tr key={r.id}>
                      <td>
                        <strong>{r.id}</strong>
                        <div className="sub-timestamp">{new Date(r.timestamp).toLocaleString()}</div>
                      </td>
                      <td>{r.suiteName}</td>
                      <td>
                        <span
                          className={`pass-rate-badge ${r.passRate >= 0.8 ? 'good' : r.passRate >= 0.5 ? 'warn' : 'bad'}`}
                        >
                          {(r.passRate * 100).toFixed(1)}%
                        </span>
                      </td>
                      <td>
                        {r.passedTasks} / {r.totalTasks}
                      </td>
                      <td>{(r.avgDurationMs / 1000).toFixed(2)}s</td>
                      <td>{r.totalToolCalls}</td>
                      <td>
                        <button
                          type="button"
                          className="btn-secondary btn-sm"
                          onClick={async () => {
                            if (!selectedProjectSlug) return
                            try {
                              const rep = await api.getProjectReport(selectedProjectSlug, r.id)
                              setSelectedReportDetail(rep)
                            } catch (err) {
                              setError(err instanceof Error ? err.message : 'Failed to load report')
                            }
                          }}
                        >
                          View Detail
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {selectedReportDetail && (
            <div className="report-detail-modal">
              <h4 className="sub-section-title">
                Report: {selectedReportDetail.suiteName} (
                {(selectedReportDetail.metrics.passRate * 100).toFixed(1)}% Passed)
              </h4>
              <div className="task-results-list">
                {selectedReportDetail.taskResults.map(tr => (
                  <div
                    key={tr.taskId}
                    className={`task-result-item ${tr.passed ? 'passed' : 'failed'}`}
                  >
                    <div className="result-header">
                      <strong>{tr.taskId}</strong>
                      <span className={`result-tag ${tr.passed ? 'tag-pass' : 'tag-fail'}`}>
                        {tr.passed ? 'PASSED' : 'FAILED'}
                      </span>
                    </div>
                    <div className="result-meta">
                      <span>Steps: {tr.stepCount}</span>
                      <span>Tool Calls: {tr.toolCallsCount}</span>
                      <span>Duration: {(tr.durationMs / 1000).toFixed(2)}s</span>
                    </div>
                    {tr.verification && (
                      <div className="result-logs">
                        <pre>{tr.verification.stdout || tr.verification.stderr || 'No logs'}</pre>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {/* ================= SUBTAB: CASSETTES ================= */}
      {subTab === 'cassettes' && (
        <div className="harness-card project-cassettes-card">
          <h3 className="section-title">📼 Project Replay Cassettes</h3>
          <p className="section-desc">
            Recorded deterministic cassettes in <code>{config?.paths.cassettesDir || '.goose/cassettes'}</code>.
          </p>

          {sectionLoading === 'cassettes' && !loadedSections.cassettes ? (
            <div className="tab-skeleton-container" style={{ padding: '1rem 0' }}>
              <div className="cassettes-grid">
                {Array.from({ length: 3 }).map((_, i) => (
                  <SkeletonBox key={i} height="110px" borderRadius="var(--radius-md)" />
                ))}
              </div>
            </div>
          ) : cassettes.length === 0 ? (
            <p className="empty-text">No recorded cassettes found in this project.</p>
          ) : (
            <div className="cassettes-grid">
              {cassettes.map(c => (
                <div key={c.fileName} className="cassette-card">
                  <div className="cassette-header">
                    <h4>{c.name}</h4>
                    <span className="badge frame-badge">{c.frameCount} frames</span>
                  </div>
                  <div className="cassette-meta">
                    <div>File: {c.fileName}</div>
                    <div>Recorded: {new Date(c.createdAt).toLocaleString()}</div>
                    {c.problemStatement && (
                      <div className="cassette-prompt">Prompt: {c.problemStatement}</div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* ================= SUBTAB: HISTORY ================= */}
      {subTab === 'history' && (
        <div className="harness-card history-card">
          <div className="history-card-header">
            <div>
              <h3 className="section-title" style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                <span>📜 Project Task History</span>
                {isHistoryLoading && (
                  <span
                    title={
                      historyRetryAttempt > 0
                        ? `Retrying project task history (attempt ${historyRetryAttempt}/2)…`
                        : 'Loading project task history…'
                    }
                    style={{ display: 'inline-flex', alignItems: 'center' }}
                  >
                    <Loader2 size={15} className="spinning text-accent" />
                  </span>
                )}
                {historyError && !isHistoryLoading && (
                  <button
                    type="button"
                    className="btn-action retry-btn"
                    onClick={() => void handleRetryHistory()}
                    title="Retry loading project task history"
                    style={{ padding: '2px 8px', fontSize: '0.75rem', display: 'inline-flex', alignItems: 'center', gap: '4px' }}
                  >
                    <RotateCcw size={12} />
                    <span>Retry</span>
                  </button>
                )}
              </h3>
              <p className="section-desc">
                All previous project harness executions. Inspect a run to reload its full trajectory.
              </p>
            </div>
          </div>

          {historyError && history.length > 0 && !isHistoryLoading && (
            <div
              className="history-error-banner"
              style={{
                marginBottom: '1rem',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: '0.75rem',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                <AlertCircle size={16} />
                <span>Failed to refresh project task history: {historyError}</span>
              </div>
              <button
                type="button"
                className="btn-secondary mini-btn"
                onClick={() => void handleRetryHistory()}
                style={{ display: 'inline-flex', alignItems: 'center', gap: '0.35rem' }}
              >
                <RotateCcw size={12} />
                <span>Retry</span>
              </button>
            </div>
          )}

          {isHistoryLoading && history.length === 0 ? (
            <div className="history-loading-spinner-box">
              <Loader2 size={28} className="spinning" />
              <span>
                {historyRetryAttempt > 0
                  ? `Loading project task history… (retry ${historyRetryAttempt}/2)`
                  : 'Loading project task history…'}
              </span>
            </div>
          ) : historyError && history.length === 0 ? (
            <div className="history-loading-spinner-box history-error-box">
              <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                <AlertCircle size={20} className="history-error-icon" />
                <span className="history-error-title">Failed to load project task history</span>
              </div>
              <p style={{ margin: 0, fontSize: '0.875rem', color: 'var(--text-secondary)', maxWidth: '420px' }}>
                {historyError}
              </p>
              <button
                type="button"
                className="btn-secondary"
                onClick={() => void handleRetryHistory()}
                style={{ display: 'inline-flex', alignItems: 'center', gap: '0.4rem', marginTop: '0.5rem' }}
              >
                <RotateCcw size={14} />
                <span>Retry</span>
              </button>
            </div>
          ) : (
            <HarnessHistoryTable
              items={history}
              selectedId={selectedHistoryId}
              loadingRunId={loadingHistoryId}
              taskNames={taskNamesById}
              emptyText="No historical project task runs yet. Run a task to populate this list."
              onInspect={item => void handleSelectHistory(item)}
              onContinue={item =>
                handleOpenContinueModal(
                  item.taskId,
                  item.stepCount || 25,
                  historyTaskTitle(item, taskNamesById),
                  item.id,
                  item.progressSummary ?? undefined,
                )
              }
            />
          )}
        </div>
      )}

      {/* ================= SUBTAB: TRAJECTORY ================= */}
      {subTab === 'trajectory' && (
        loadingTrajectory ? (
          <div className="harness-card trajectory-loading-card">
            <Loader2 size={36} className="spinning" />
            <div className="trajectory-loading-text">
              <strong style={{ display: 'block', fontSize: '1.05rem', color: 'var(--text-primary)', marginBottom: '0.25rem' }}>
                Loading Agent Log…
              </strong>
              <span style={{ fontSize: '0.875rem', color: 'var(--text-secondary)' }}>
                Fetching execution trajectory and step details
              </span>
            </div>
          </div>
        ) : activeTrajectoryView ? (
          <TaskTrajectoryViewer
            run={activeTrajectoryView}
            api={api}
            projectSlug={selectedProjectSlug}
            onNavigateToFiles={onNavigateToFiles}
            liveInspect={liveInspect}
            inspectingJobId={inspectingJobId}
            selectedHistoryId={selectedHistoryId}
            runningTaskId={runningTaskId}
            stoppingTaskId={stoppingTaskId}
            activeJobs={activeJobs}
            onStopTask={handleStopTask}
            onStopJob={handleStopRunningJob}
            onBackToTasks={() => setSubTab('tasks')}
            onContinueTask={async (taskId, extraTurns) => {
              await handleRunTask(
                taskId,
                false,
                undefined,
                selectedHistoryId ?? undefined,
                extraTurns,
              )
            }}
            onOpenContinueModal={handleOpenContinueModal}
          />
        ) : (
          <div className="harness-card">
            <p className="empty-text">No agent log selected. Select a task run from history to view its log.</p>
          </div>
        )
      )}

      {/* ================= MODAL: CONTINUE CANCELLED TASK WITH SPECIFIED TURNS ================= */}
      {continueModalTask && (
        <div className="modal-backdrop" onClick={handleCloseContinueModal}>
          <div className="glass-modal continue-modal" onClick={e => e.stopPropagation()}>
            <div className="form-panel">
              <div className="modal-header-row">
                <div className="modal-header-title">
                  <div className="modal-icon-badge continue-icon-badge">⏩</div>
                  <div>
                    <h3 className="section-title" style={{ margin: 0 }}>
                      Continue Unfinished Task
                    </h3>
                    <span className="form-hint">
                      New run seeded with the saved progress summary from the previous run
                    </span>
                  </div>
                </div>
                <button type="button" className="close-btn" onClick={handleCloseContinueModal}>
                  ✕
                </button>
              </div>

              <div className="continue-modal-info-card">
                <div className="continue-modal-info-row">
                  <span className="info-label">Task ID:</span>
                  <strong className="info-value code-font">{continueModalTask.taskId}</strong>
                </div>
                {continueModalTask.taskName && (
                  <div className="continue-modal-info-row">
                    <span className="info-label">Task Name:</span>
                    <span className="info-value">{continueModalTask.taskName}</span>
                  </div>
                )}
                <div className="continue-modal-info-row">
                  <span className="info-label">Previous Run:</span>
                  <span className="info-value">
                    <span className="status-badge cancelled" style={{ marginRight: 6 }}>
                      INCOMPLETE
                    </span>
                    Stopped after <strong>{continueModalTask.previousSteps}</strong> steps
                    {continueModalTask.runId ? (
                      <>
                        {' '}
                        • Run <code>{continueModalTask.runId}</code>
                      </>
                    ) : null}
                  </span>
                </div>
                {continueModalTask.progressSummary && (
                  <div className="continuation-summary-panel">
                    <MarkdownPreview content={continueModalTask.progressSummary} />
                  </div>
                )}
              </div>

              <div className="continue-mode-selector">
                <label className="form-label">Turn Budget Mode</label>
                <div className="verifier-type-toggle">
                  <label>
                    <input
                      type="radio"
                      name="continueMode"
                      checked={continueTurnsMode === 'additional'}
                      onChange={() => {
                        setContinueTurnsMode('additional')
                        setContinueTurnsInput(25)
                      }}
                    />{' '}
                    Add Additional Turns (+N)
                  </label>
                  <label>
                    <input
                      type="radio"
                      name="continueMode"
                      checked={continueTurnsMode === 'total'}
                      onChange={() => {
                        setContinueTurnsMode('total')
                        setContinueTurnsInput(continueModalTask.previousSteps + 25)
                      }}
                    />{' '}
                    Set Total Max Turns (Total = N)
                  </label>
                </div>
              </div>

              <div className="continue-turns-section">
                <label className="form-label">
                  {continueTurnsMode === 'additional'
                    ? 'Additional Turns to Add'
                    : 'Total Max Turn Limit'}
                </label>

                <div className="continue-preset-chips">
                  {continueTurnsMode === 'additional' ? (
                    <>
                      {[10, 25, 50, 100].map(n => (
                        <button
                          key={n}
                          type="button"
                          className={`preset-chip ${continueTurnsInput === n ? 'active' : ''}`}
                          onClick={() => setContinueTurnsInput(n)}
                        >
                          +{n} Turns
                        </button>
                      ))}
                    </>
                  ) : (
                    <>
                      {[10, 25, 50, 100].map(extra => {
                        const total = continueModalTask.previousSteps + extra
                        return (
                          <button
                            key={total}
                            type="button"
                            className={`preset-chip ${
                              continueTurnsInput === total ? 'active' : ''
                            }`}
                            onClick={() => setContinueTurnsInput(total)}
                          >
                            {total} Turns (+{extra})
                          </button>
                        )
                      })}
                    </>
                  )}
                </div>

                <div className="continue-custom-stepper-row">
                  <button
                    type="button"
                    className="btn-stepper"
                    onClick={() => setContinueTurnsInput(Math.max(1, continueTurnsInput - 5))}
                  >
                    -5
                  </button>
                  <button
                    type="button"
                    className="btn-stepper"
                    onClick={() => setContinueTurnsInput(Math.max(1, continueTurnsInput - 1))}
                  >
                    -1
                  </button>
                  <input
                    type="number"
                    min={1}
                    max={1000}
                    className="form-input continue-turns-number-input"
                    value={continueTurnsInput}
                    onChange={e =>
                      setContinueTurnsInput(Math.max(1, parseInt(e.target.value, 10) || 1))
                    }
                  />
                  <button
                    type="button"
                    className="btn-stepper"
                    onClick={() => setContinueTurnsInput(continueTurnsInput + 1)}
                  >
                    +1
                  </button>
                  <button
                    type="button"
                    className="btn-stepper"
                    onClick={() => setContinueTurnsInput(continueTurnsInput + 5)}
                  >
                    +5
                  </button>
                </div>
              </div>

              <div className="continue-budget-preview-box">
                <div className="preview-label">Budget Calculation</div>
                <div className="preview-calc">
                  <span>
                    Previous: <strong>{continueModalTask.previousSteps}</strong> steps
                  </span>
                  <span className="calc-arrow">→</span>
                  <span>
                    Target Max Turns:{' '}
                    <strong className="calc-highlight">
                      {continueTurnsMode === 'additional'
                        ? continueModalTask.previousSteps + Math.max(1, continueTurnsInput)
                        : Math.max(1, continueTurnsInput)}
                    </strong>
                  </span>
                  <span className="calc-diff">
                    (
                    {continueTurnsMode === 'additional'
                      ? `+${Math.max(1, continueTurnsInput)} turns`
                      : `${
                          Math.max(1, continueTurnsInput) >= continueModalTask.previousSteps
                            ? '+'
                            : ''
                        }${Math.max(1, continueTurnsInput) - continueModalTask.previousSteps} turns`}
                    )
                  </span>
                </div>
              </div>

              <div className="continue-options-row">
                <label className="checkbox-row">
                  <input
                    type="checkbox"
                    checked={continueRecord}
                    onChange={e => setContinueRecord(e.target.checked)}
                  />
                  <span>📼 Record cassette for offline deterministic replay</span>
                </label>
              </div>

              <div className="modal-actions-row">
                <button type="button" className="btn-secondary" onClick={handleCloseContinueModal}>
                  Cancel
                </button>
                <button
                  type="button"
                  className="btn-primary continue-confirm-btn"
                  onClick={handleConfirmContinueTask}
                  disabled={runningTaskId === continueModalTask.taskId}
                >
                  {runningTaskId === continueModalTask.taskId ? (
                    '⏳ Launching…'
                  ) : (
                    `⏩ Continue Task (${
                      continueTurnsMode === 'additional'
                        ? continueModalTask.previousSteps + Math.max(1, continueTurnsInput)
                        : Math.max(1, continueTurnsInput)
                    } Turns)`
                  )}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {dynamicRun && (
        <div className="modal-backdrop" onClick={() => setDynamicRun(null)}>
          <div
            className="glass-modal dynamic-prompt-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="dynamic-prompt-title"
            onClick={e => e.stopPropagation()}
          >
            <div className="form-panel">
              <div className="modal-header-row">
                <div className="modal-header-title">
                  <div className="modal-icon-badge dynamic-prompt-icon-badge">✎</div>
                  <div>
                    <h3 id="dynamic-prompt-title" className="section-title" style={{ margin: 0 }}>
                      {dynamicRun.record ? 'Record with extra prompt' : 'Run with extra prompt'}
                    </h3>
                    <span className="form-hint">
                      {dynamicRun.taskName} · extra instructions are not saved to the task
                    </span>
                  </div>
                </div>
                <button type="button" className="close-btn" onClick={() => setDynamicRun(null)}>
                  ✕
                </button>
              </div>

              <label className="form-label" htmlFor="dynamic-prompt-current">
                Current prompt
              </label>
              {dynamicRun.loadingPrompt ? (
                <p className="muted">Loading prompt…</p>
              ) : dynamicRun.error ? (
                <p className="dynamic-prompt-error">{dynamicRun.error}</p>
              ) : (
                <pre id="dynamic-prompt-current" className="dynamic-prompt-current">
                  {dynamicRun.prompt}
                </pre>
              )}

              <label className="form-label" htmlFor="dynamic-prompt-extra">
                Additional prompt
              </label>
              <textarea
                id="dynamic-prompt-extra"
                className="form-textarea"
                rows={5}
                autoFocus
                value={dynamicRun.extra}
                placeholder="Optional instructions for this run"
                onChange={e =>
                  setDynamicRun(prev => (prev ? { ...prev, extra: e.target.value } : prev))
                }
              />

              <div className="editor-button-row" style={{ marginTop: '1rem' }}>
                <button
                  type="button"
                  className="btn-primary"
                  disabled={dynamicRun.loadingPrompt || Boolean(dynamicRun.error) || runningTaskId === dynamicRun.taskId}
                  onClick={() => void confirmDynamicRun()}
                >
                  {dynamicRun.record ? 'Record' : 'Run'}
                </button>
                <button type="button" className="btn-secondary" onClick={() => setDynamicRun(null)}>
                  Cancel
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
