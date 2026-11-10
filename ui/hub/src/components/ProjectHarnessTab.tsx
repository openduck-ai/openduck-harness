import { Trans, useTranslation } from 'react-i18next'
import { formatDateTime, formatNumber, formatTime } from '../i18n/format.ts'
import { useEffect, useRef, useState, useCallback, useMemo } from 'react'
import {
  AlertCircle,
  BarChart3,
  ChevronDown,
  CircleDot,
  Clock,
  Disc,
  FastForward,
  History,
  Loader2,
  MoreHorizontal,
  Paperclip,
  Pencil,
  Play,
  RotateCcw,
  Settings,
  Trash2,
  X,
} from 'lucide-react'
import {
  defaultSchedule,
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
  type HarnessPagination,
  type HarnessProjectEvent,
  type HarnessReportSummary,
  type HarnessRunResponse,
  type Project,
  type ProjectHarnessConfig,
  type ProjectProvidersConfigResponse,
  type ProvidersConfigResponse,
  type ProjectTaskDefinition,
  type ProjectTaskEstimateResponse,
  type ProjectTaskSummary,
  type TaskDraftMessage,
  type ScheduleKind,
} from '@aaif/goose-hub-core'
import { inspectProjectHarnessJob } from '../api.ts'
import { projectModelChoices, projectProviderChoices } from '../projectProviderOptions.ts'
import { CopyLinkButton } from './CopyLinkButton'
import {
  isValidProjectFilePath,
  liveSubscriptionTarget,
  logPagingFields,
  logPagingKey,
  logTargetKey,
  requestStillCurrent,
  resolvedLogPaging,
  type HubDestination,
} from '../urlNav'
import { escapeClosesTaskEditor } from '../taskEditorDismiss.ts'
import { defaultTaskCreateDraftData, type TaskCreateSection } from '../taskCreateDraft.ts'
import { createTaskDraftAutosave, prepareTaskCreator } from '../taskCreateDraftFlow.ts'
import {
  describeDraftRestored,
  shouldPersistTaskCreatorDraft,
  taskCreatorDataToDraft,
  taskCreatorDefaults,
  toggleTaskCreatorSection,
  type TaskCreatorState,
} from '../taskCreator.ts'
import { SubtaskEditor, TaskDraftChat } from './TaskDraftPanel.tsx'
import {
  applyTaskDraft,
  blankSubtask,
  calculateTaskMaxTurnsFromSubtasks,
  recalculateSubtasksTurns,
  shouldReplaceGeneratedTaskId,
  subtaskSaveErrorDescriptor as subtaskSaveError,
  subtasksForSave,
  taskFormToDraft,
} from '../taskDraft.ts'
import { extraTurnsForContinue, isContinuableStatus, isHistoryFollowUpStatus } from '../taskContinuation.ts'
import { buildAgentLog, liveLineText } from '../taskTrajectory.ts'
import {
  fetchWithRetry,
  filterAndSortProjectTasks,
  formatDurationMs,
  historyTaskTitle,
  latestRunForTask,
  presentLastRun,
  projectTaskListEmpty,
  taskProviderLines,
  type ProjectTaskSort,
} from '../projectTasks.ts'
import {
  isAcceptedTaskAttachment,
  listTaskAttachmentPaths,
  TASK_ATTACHMENT_ACCEPT,
  uploadAttachmentsAndComposePrompt,
  workspacePathsForAttachments,
} from '../taskAttachments.ts'
import { HarnessHistoryTable } from './HarnessHistoryTable.tsx'
import { normalizeHistoryResponse } from '../api.ts'
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

type PageMessage = string | {
  key: `harness.page.${string}` | `harness.draft.helper.${string}`
  values?: Record<string, string | number>
}

function pageMessage(key: `harness.page.${string}`, values?: Record<string, string | number>): PageMessage {
  return { key, values }
}

interface ProjectHarnessTabProps {
  api: ReturnType<typeof createApi>
  projects?: Project[]
  selectedProjectSlug: string
  onSelectProject?: (slug: string) => void
  onInspectTrajectory?: (run: HarnessRunResponse) => void
  onNavigateToFiles?: (path: string) => void
  destination?: HubDestination | null
  shareUrl?: string
  onHubNavigate?: (
    update: { tab?: 'harness' | 'files' | null; destination?: HubDestination | null },
    replace?: boolean,
  ) => void
}

function savedAgentLogPageSize(): number {
  try {
    const saved = localStorage.getItem('openduck.agentLog.pageSize')
    if (saved !== null) {
      const parsed = parseInt(saved, 10)
      if (!Number.isNaN(parsed) && [25, 50, 100, 0].includes(parsed)) return parsed
    }
  } catch {
    // Ignore
  }
  return 50
}

type ProjectSubTab = 'tasks' | 'config' | 'reports' | 'cassettes' | 'history' | 'trajectory'
type LogOrigin = 'tasks' | 'history' | 'reports' | 'config' | 'cassettes'
type TaskMentionField = 'prompt' | 'draft'

interface TaskMentionPopoverState {
  isOpen: boolean
  query: string
  mentionStart: number
  selectedIndex: number
  field: TaskMentionField | null
}

const closedTaskMentionPopover: TaskMentionPopoverState = {
  isOpen: false,
  query: '',
  mentionStart: -1,
  selectedIndex: 0,
  field: null,
}

function normalizeHarnessConfig(raw: Partial<ProjectHarnessConfig> | any): ProjectHarnessConfig {
  return {
    version: raw?.version || '1.0',
    policy: {
      provider: raw?.policy?.provider || undefined,
      model: raw?.policy?.model || undefined,
      secondaryProvider: raw?.policy?.secondaryProvider || raw?.policy?.secondary_provider || undefined,
      secondaryModel: raw?.policy?.secondaryModel || raw?.policy?.secondary_model || undefined,
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
      autoCommit: raw?.execution?.autoCommit ?? raw?.execution?.auto_commit ?? true,
      maxTurns: raw?.execution?.maxTurns ?? raw?.execution?.max_turns ?? 25,
      timeoutSeconds: raw?.execution?.timeoutSeconds ?? raw?.execution?.timeout_seconds ?? 300,
      concurrency: raw?.execution?.concurrency ?? 4,
    },
    maxTurnsLimit: raw?.maxTurnsLimit ?? raw?.max_turns_limit ?? 500,
    paths: {
      tasksDir: raw?.paths?.tasksDir || raw?.paths?.tasks_dir || '.goose/tasks',
      resultsDir: raw?.paths?.resultsDir || raw?.paths?.results_dir || '.goose/harness_results',
      cassettesDir: raw?.paths?.cassettesDir || raw?.paths?.cassettes_dir || '.goose/cassettes',
    },
    providers: raw?.providers || {},
  }
}

export function ProjectHarnessTab({
  api,
  projects: _projects = [],
  selectedProjectSlug,
  onSelectProject: _onSelectProject,
  onInspectTrajectory: _onInspectTrajectory,
  onNavigateToFiles,
  destination = null,
  shareUrl,
  onHubNavigate,
}: ProjectHarnessTabProps) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language
  const renderMessage = (message: PageMessage | null, language = locale): string =>
    !message ? '' : typeof message === 'string' ? message : t(message.key, { ...message.values, lng: language })
  const weekdays = [
    t('harness.page.schedule.days.sunday'),
    t('harness.page.schedule.days.monday'),
    t('harness.page.schedule.days.tuesday'),
    t('harness.page.schedule.days.wednesday'),
    t('harness.page.schedule.days.thursday'),
    t('harness.page.schedule.days.friday'),
    t('harness.page.schedule.days.saturday'),
  ]
  const describeLocalizedTaskSchedule = (cron?: string | null, paused = false): string => {
    if (!cron?.trim()) return ''
    const parsed = parseFriendlySchedule(cron)
    const time = formatTime(new Date(1970, 0, 1, parsed.hour, parsed.minute), { hour: 'numeric', minute: '2-digit' })
    let description: string
    switch (parsed.kind) {
      case 'none': description = t('harness.page.schedule.description.manual'); break
      case 'hourly': description = t('harness.page.schedule.hourly'); break
      case 'daily': description = t('harness.page.schedule.description.daily', { time }); break
      case 'weekdays': description = t('harness.page.schedule.description.weekdays', { time }); break
      case 'weekly': description = t('harness.page.schedule.description.weekly', { day: weekdays[parsed.weekday] ?? weekdays[0], time }); break
    }
    return paused ? t('harness.page.schedule.description.paused', { description }) : description
  }
  const [subTab, setSubTab] = useState<ProjectSubTab>(() => {
    if (destination?.kind === 'history') return 'history'
    if (destination?.kind === 'log') return 'trajectory'
    return 'tasks'
  })
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<PageMessage>('')
  const [successMsg, setSuccessMsg] = useState<PageMessage>('')

  // Project data
  const [config, setConfig] = useState<ProjectHarnessConfig | null>(null)
  const [providerCatalog, setProviderCatalog] = useState<ProjectProvidersConfigResponse | null>(null)
  const [globalProviders, setGlobalProviders] = useState<ProvidersConfigResponse | null>(null)
  const maxTurnsLimit = Math.max(1, config?.maxTurnsLimit || 500)
  const [tasks, setTasks] = useState<ProjectTaskSummary[]>([])
  const [reports, setReports] = useState<HarnessReportSummary[]>([])
  const [cassettes, setCassettes] = useState<HarnessCassetteSummary[]>([])
  const [activeJobs, setActiveJobs] = useState<HarnessActiveJob[]>([])
  const [history, setHistory] = useState<HarnessHistorySummary[]>([])
  const [historyPage, setHistoryPage] = useState<number>(1)
  const [historyPageSize, setHistoryPageSize] = useState<number>(() => {
    try {
      const saved = localStorage.getItem('openduck.history.pageSize')
      if (saved) {
        const parsed = parseInt(saved, 10)
        if ([10, 20, 50, 100].includes(parsed)) return parsed
      }
    } catch {}
    return 20
  })
  const [historyPagination, setHistoryPagination] = useState<HarnessPagination | null>(null)
  const [selectedHistoryId, setSelectedHistoryId] = useState<string | null>(null)
  const [selectedHistoryFileName, setSelectedHistoryFileName] = useState<string | null>(null)

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
  const [draftMessages, setDraftMessages] = useState<TaskDraftMessage[]>([])
  const [draftInput, setDraftInput] = useState('')
  const [drafting, setDrafting] = useState(false)
  const [taskAttachments, setTaskAttachments] = useState<File[]>([])
  const [existingAttachmentMentions, setExistingAttachmentMentions] = useState<
    MentionDisplayItem[]
  >([])
  const [savingTask, setSavingTask] = useState(false)
  const [taskCreatorSection, setTaskCreatorSection] = useState<TaskCreateSection>('basics')
  const [taskCreatorExpanded, setTaskCreatorExpanded] = useState<
    Record<TaskCreateSection, boolean>
  >(() => defaultTaskCreateDraftData().ui.expanded)
  const [taskCreatorRestoredFrom, setTaskCreatorRestoredFrom] = useState<Date | null>(null)
  const taskAttachInputRef = useRef<HTMLInputElement | null>(null)

  // Task mention autocomplete state
  const [taskMentionItems, setTaskMentionItems] = useState<MentionDisplayItem[]>([])
  const [loadingTaskMentions, setLoadingTaskMentions] = useState(false)
  const [taskMentionPopover, setTaskMentionPopover] = useState<TaskMentionPopoverState>(
    closedTaskMentionPopover,
  )
  const taskMentionPopoverRef = useRef<MentionPopoverHandle | null>(null)
  const draftMentionPopoverRef = useRef<MentionPopoverHandle | null>(null)
  const taskPromptTextareaRef = useRef<HTMLTextAreaElement | null>(null)
  const draftTextareaRef = useRef<HTMLTextAreaElement | null>(null)
  const taskEditorRef = useRef<HTMLDivElement | null>(null)
  const imeEscapeUntilRef = useRef(0)
  const dismissedTaskMentionRef = useRef<DismissedMention | null>(null)
  const dismissedTaskMentionFieldRef = useRef<TaskMentionField | null>(null)
  const lastDismissedTaskTimeRef = useRef<number>(0)

  // Running states
  const [runningTaskId, setRunningTaskId] = useState<string | null>(null)
  const [stoppingTaskId, setStoppingTaskId] = useState<string | null>(null)
  const [runningEval, setRunningEval] = useState(false)
  const [savingConfig, setSavingConfig] = useState(false)
  const [filterTag, setFilterTag] = useState('')
  const [filterQuery, setFilterQuery] = useState('')
  const [taskSort, setTaskSort] = useState<ProjectTaskSort>('activity')
  const [historyTaskFilter, setHistoryTaskFilter] = useState<string>('')
  const [openMenuTaskId, setOpenMenuTaskId] = useState<string | null>(null)
  const [moreNavOpen, setMoreNavOpen] = useState(false)
  const [logOrigin, setLogOrigin] = useState<LogOrigin | null>(
    destination?.kind === 'log' ? 'history' : null,
  )
  const [logRetry, setLogRetry] = useState(0)
  const [logError, setLogError] = useState<PageMessage>('')
  const [logPageLoading, setLogPageLoading] = useState(false)
  const sectionIntentRef = useRef<ProjectSubTab | null>(null)
  const logRequestRef = useRef(0)
  const logPagingGenRef = useRef(0)
  const appliedPagingKeyRef = useRef<string | null>(null)
  const suppressPagingFetchRef = useRef(false)
  const slugRef = useRef(selectedProjectSlug)
  const targetKeyRef = useRef('')
  const destinationRef = useRef(destination)
  const appliedKindRef = useRef<HubDestination['kind'] | 'none' | null>(null)
  slugRef.current = selectedProjectSlug
  targetKeyRef.current = logTargetKey(destination)
  destinationRef.current = destination
  const [savedScrollY, setSavedScrollY] = useState(0)
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    if (activeJobs.length === 0) return
    const interval = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(interval)
  }, [activeJobs.length])

  useEffect(() => {
    if (!openMenuTaskId) return
    const handlePointerDown = (e: PointerEvent) => {
      const target = e.target as HTMLElement | null
      if (target?.closest('.task-menu-container')) return
      setOpenMenuTaskId(null)
    }
    window.addEventListener('pointerdown', handlePointerDown)
    return () => window.removeEventListener('pointerdown', handlePointerDown)
  }, [openMenuTaskId])

  useEffect(() => {
    if (!moreNavOpen) return
    const handlePointerDown = (e: PointerEvent) => {
      const target = e.target as HTMLElement | null
      if (target?.closest('.subnav-more-container')) return
      setMoreNavOpen(false)
    }
    window.addEventListener('pointerdown', handlePointerDown)
    return () => window.removeEventListener('pointerdown', handlePointerDown)
  }, [moreNavOpen])
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

  const applyLogSlice = useCallback(async (
    slug: string,
    dest: Extract<HubDestination, { kind: 'log' }>,
    gen: number,
    requested?: { page: number; limit: number } | null,
  ) => {
    const paging = requested === undefined
      ? resolvedLogPaging(dest, savedAgentLogPageSize())
      : requested === null
        ? undefined
        : requested
    const target = dest.target
    if (target.kind === 'history') {
      const fileName = target.fileName
      const run = await api.getProjectHistoryDetail(slug, fileName, paging)
      if (logPagingGenRef.current !== gen || slugRef.current !== slug) return
      if (logTargetKey(destinationRef.current) !== `history:${fileName}`) return
      setActiveTrajectoryView(run)
      setSelectedHistoryFileName(fileName)
      setSelectedHistoryId(current => current ?? fileName)
      setError('')
      setLogError('')
      return
    }
    const jobId = target.jobId
    const detail = await inspectProjectHarnessJob(slug, jobId, paging)
    if (logPagingGenRef.current !== gen || slugRef.current !== slug) return
    if (logTargetKey(destinationRef.current) !== `job:${jobId}`) return
    if (!detail.snapshot) {
      setActiveTrajectoryView(null)
      setLogError('deepLink.logUnavailable')
      return
    }
    setActiveTrajectoryView(detail.snapshot)
    setSelectedHistoryFileName(null)
    setError('')
    setLogError('')
    if (detail.live) startLiveInspect(jobId, detail.job.taskId)
  }, [api, startLiveInspect])

  // Continue task modal and turn selection state
  const [continueModalTask, setContinueModalTask] = useState<{
    taskId: string
    runId?: string
    taskName?: string
    previousSteps: number
    progressSummary?: string
    resumeSubtaskId?: string
    resumeSubtaskTitle?: string
    failureReason?: string
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
    error: PageMessage
  } | null>(null)

  // Loaded sections registry to prevent redundant fetches
  const [loadedSections, setLoadedSections] = useState<Record<string, boolean>>({})
  const [sectionLoading, setSectionLoading] = useState<string | null>(null)
  const [historyError, setHistoryError] = useState<PageMessage | null>(null)
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
          const histRes = await fetchWithRetry(
            () =>
              api.listProjectHistory(slug, {
                page: historyPage,
                limit: historyPageSize,
              }),
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
            const norm = normalizeHistoryResponse(histRes)
            setHistory(norm.items || [])
            setHistoryPagination(norm.pagination ?? null)
            setLoadedSections(prev => ({ ...prev, history: true }))
            setHistoryError(null)
            setHistoryRetryAttempt(0)
          }
        } catch (err) {
          if (reqId === historyRequestIdRef.current) {
            const msg = err instanceof Error ? err.message : pageMessage('harness.page.errors.historyLoad')
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
    [api, historyPage, historyPageSize],
  )

  const handleHistoryPageChange = useCallback(
    (newPage: number) => {
      setHistoryPage(newPage)
      if (selectedProjectSlug) {
        void (async () => {
          setLoadingHistory(true)
          try {
            const histRes = await api.listProjectHistory(selectedProjectSlug, {
              page: newPage,
              limit: historyPageSize,
            })
            const norm = normalizeHistoryResponse(histRes)
            setHistory(norm.items || [])
            setHistoryPagination(norm.pagination ?? null)
          } catch (err) {
            setHistoryError(err instanceof Error ? err.message : pageMessage('harness.page.errors.historyLoad'))
          } finally {
            setLoadingHistory(false)
          }
        })()
      }
    },
    [api, selectedProjectSlug, historyPageSize],
  )

  const handleHistoryPageSizeChange = useCallback(
    (newSize: number) => {
      setHistoryPageSize(newSize)
      setHistoryPage(1)
      try {
        localStorage.setItem('openduck.history.pageSize', String(newSize))
      } catch {}
      if (selectedProjectSlug) {
        void (async () => {
          setLoadingHistory(true)
          try {
            const histRes = await api.listProjectHistory(selectedProjectSlug, {
              page: 1,
              limit: newSize,
            })
            const norm = normalizeHistoryResponse(histRes)
            setHistory(norm.items || [])
            setHistoryPagination(norm.pagination ?? null)
          } catch (err) {
            setHistoryError(err instanceof Error ? err.message : pageMessage('harness.page.errors.historyLoad'))
          } finally {
            setLoadingHistory(false)
          }
        })()
      }
    },
    [api, selectedProjectSlug],
  )

  const handleRetryHistory = useCallback(() => {
    if (!selectedProjectSlug) return
    setError(prev => (renderMessage(prev, 'en').toLowerCase().includes('history') ? '' : prev))
    void loadHistory(selectedProjectSlug, false, true)
  }, [selectedProjectSlug, loadHistory, locale])

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
          setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.tasksLoad'))
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
        if (!silent) setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.reportsLoad'))
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
        if (!silent) setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.cassettesLoad'))
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
        if (!silent) setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.configLoad'))
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
      setHistoryPage(1)
      setHistoryPagination(null)
      setHistoryError(null)
      setHistoryRetryAttempt(0)
      setLoadingHistory(true)
      setActiveTrajectoryView(null)
      setLoadingTrajectory(false)
      setLoadingHistoryId(null)
      setProviderCatalog(null)
      setGlobalProviders(null)
      void loadSubTabData(selectedProjectSlug, subTab, true)
    } else {
      void loadSubTabData(selectedProjectSlug, subTab, false)
    }
  }, [selectedProjectSlug, subTab])

  useEffect(() => {
    if (!selectedProjectSlug || subTab !== 'config') return
    let cancelled = false
    void Promise.all([
      api.getProjectProvidersConfig(selectedProjectSlug).catch(() => null),
      api.getProvidersConfig().catch(() => null),
    ]).then(([catalog, providers]) => {
      if (cancelled) return
      setProviderCatalog(catalog)
      setGlobalProviders(providers)
    })
    return () => {
      cancelled = true
    }
  }, [api, selectedProjectSlug, subTab])

  const providerChoices = useMemo(
    () =>
      projectProviderChoices({
        availableProviders: providerCatalog?.availableProviders,
        globalProviderNames: globalProviders
          ? globalProviders.providers.map(provider => provider.name)
          : undefined,
        current: [
          config?.policy?.provider,
          config?.policy?.secondaryProvider,
          taskForm.provider,
          taskForm.secondaryProvider,
        ],
      }),
    [
      providerCatalog,
      globalProviders,
      config?.policy?.provider,
      config?.policy?.secondaryProvider,
      taskForm.provider,
      taskForm.secondaryProvider,
    ],
  )

  const modelChoicesFor = (providerName: string | undefined, current: string | undefined) => {
    const provider = providerName?.trim() || ''
    const globalModels = globalProviders?.providers
      .find(row => row.name === provider)
      ?.models?.map(model => model.id)
    return projectModelChoices({
      provider,
      providerModels: providerCatalog?.providerModels,
      globalModels,
      current,
    })
  }

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
          const isRunning = ['running', 'queued'].includes(event.status.toLowerCase())
          setTasks(prev =>
            prev.map(task =>
              task.id === event.taskId ? { ...task, currentlyRunning: isRunning } : task,
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

  const handleSelectHistory = async (item: HarnessHistorySummary, origin: LogOrigin = 'history') => {
    if (!selectedProjectSlug) return
    sectionIntentRef.current = null
    setLogOrigin(origin)
    setSavedScrollY(window.scrollY)
    setSelectedHistoryId(item.id)
    setSelectedHistoryFileName(item.fileName)
    setSubTab('trajectory')
    setLogError('')
    setError('')
    if (onHubNavigate) {
      if (logTargetKey(destination) !== `history:${item.fileName}`) {
        setLoadingHistoryId(item.id)
        setLoadingTrajectory(true)
      }
      onHubNavigate({
        tab: 'harness',
        destination: {
          kind: 'log',
          target: { kind: 'history', fileName: item.fileName },
          file: null,
          logPage: null,
          logSize: null,
        },
      })
      return
    }
    stopLiveInspect()
    setLoadingHistoryId(item.id)
    setLoadingTrajectory(true)
    try {
      const paging = resolvedLogPaging(null, savedAgentLogPageSize())
      const run = await api.getProjectHistoryDetail(selectedProjectSlug, item.fileName, paging)
      setActiveTrajectoryView(run)
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.historicalRun'))
    } finally {
      setLoadingHistoryId(null)
      setLoadingTrajectory(false)
    }
  }

  const handleInspectRunningJob = async (job: HarnessActiveJob, origin?: LogOrigin) => {
    if (!selectedProjectSlug) return
    const effectiveOrigin: LogOrigin =
      origin ||
      ((['tasks', 'history', 'reports', 'config', 'cassettes'].includes(subTab)
        ? subTab
        : 'tasks') as LogOrigin)
    sectionIntentRef.current = null
    setLogOrigin(effectiveOrigin)
    setSavedScrollY(window.scrollY)
    setSubTab('trajectory')
    setLogError('')
    setError('')
    if (onHubNavigate) {
      if (logTargetKey(destination) !== `job:${job.jobId}`) setLoadingTrajectory(true)
      onHubNavigate({
        tab: 'harness',
        destination: {
          kind: 'log',
          target: { kind: 'job', jobId: job.jobId },
          file: null,
          logPage: null,
          logSize: null,
        },
      })
      return
    }

    startLiveInspect(job.jobId, job.taskId)
    setLoadingTrajectory(true)
    try {
      const paging = resolvedLogPaging(null, savedAgentLogPageSize())
      const detail = await inspectProjectHarnessJob(selectedProjectSlug, job.jobId, paging)
      if (inspectingJobIdRef.current !== job.jobId) return
      if (detail.snapshot) {
        setActiveTrajectoryView(detail.snapshot)
      }
    } catch (err) {
      if (inspectingJobIdRef.current !== job.jobId) return
      stopLiveInspect()
      if (subTab === 'trajectory') setSubTab(effectiveOrigin)
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.inspect'))
    } finally {
      setLoadingTrajectory(false)
    }
  }

  const handleBackFromTrajectory = () => {
    const targetTab = logOrigin || 'history'
    setLogOrigin(null)
    stopLiveInspect()
    setActiveTrajectoryView(null)
    setLoadingTrajectory(false)
    setLogError('')
    if (targetTab === 'history') {
      sectionIntentRef.current = null
      setSubTab('history')
      onHubNavigate?.({ tab: 'harness', destination: { kind: 'history' } })
    } else {
      sectionIntentRef.current = targetTab
      setSubTab(targetTab)
      onHubNavigate?.({ tab: 'harness', destination: null })
    }
    setTimeout(() => {
      window.scrollTo({ top: savedScrollY, behavior: 'instant' })
    }, 0)
  }

  const handleStopTask = async (taskId: string) => {
    if (!selectedProjectSlug) return
    setStoppingTaskId(taskId)
    setError('')
    try {
      await api.stopProjectTask(selectedProjectSlug, taskId)
      setSuccessMsg(pageMessage('harness.page.notices.stopped', { taskId: taskId }))
      if (runningTaskId === taskId) {
        setRunningTaskId(null)
      }
      await loadTasksAndJobs(selectedProjectSlug, true)
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.stop'))
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
      setSuccessMsg(pageMessage('harness.page.notices.stopped', { taskId: job.taskId }))
      if (runningTaskId === job.taskId) {
        setRunningTaskId(null)
      }
      if (inspectingJobId === job.jobId) {
        stopLiveInspect()
      }
      await loadTasksAndJobs(selectedProjectSlug, true)
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.stop'))
    } finally {
      setStoppingTaskId(null)
    }
  }

  const activeLogKey = logTargetKey(destination)
  const liveJobId = liveSubscriptionTarget(destination)

  useEffect(() => {
    if (!selectedProjectSlug) return
    const dest = destination
    const kind = dest?.kind ?? 'none'
    const previousKind = appliedKindRef.current
    appliedKindRef.current = kind
    if (dest?.kind === 'log') {
      const targetKey = logTargetKey(dest)
      const request = { id: ++logRequestRef.current, projectSlug: selectedProjectSlug, targetKey }
      const gen = ++logPagingGenRef.current
      appliedPagingKeyRef.current = logPagingKey(dest)
      setSubTab('trajectory')
      setLogOrigin(current => current ?? 'history')
      setLoadingTrajectory(true)
      setLogPageLoading(false)
      setLogError('')
      const sameTarget = () =>
        requestStillCurrent(request, {
          id: logRequestRef.current,
          projectSlug: slugRef.current,
          targetKey: targetKeyRef.current,
        })
      const target = dest.target
      void (async () => {
        try {
          if (target.kind === 'history') {
            stopLiveInspect()
            setSelectedHistoryFileName(target.fileName)
          } else {
            stopLiveInspect()
          }
          await applyLogSlice(selectedProjectSlug, dest, gen)
        } catch (err) {
          if (!sameTarget() || logPagingGenRef.current !== gen) return
          stopLiveInspect()
          setActiveTrajectoryView(null)
          setLogError(err instanceof Error && err.message ? err.message : 'deepLink.logUnavailable')
        } finally {
          if (sameTarget()) {
            setLoadingTrajectory(false)
            setLoadingHistoryId(null)
          }
        }
      })()
      return
    }

    if (dest?.kind === 'history') {
      logRequestRef.current += 1
      logPagingGenRef.current += 1
      stopLiveInspect()
      setActiveTrajectoryView(null)
      setLoadingTrajectory(false)
      setLoadingHistoryId(null)
      setLogError('')
      setSubTab('history')
      return
    }

    const intent = sectionIntentRef.current
    sectionIntentRef.current = null
    if (previousKind !== 'log' && previousKind !== 'history' && !intent) return

    logRequestRef.current += 1
    logPagingGenRef.current += 1
    stopLiveInspect()
    setActiveTrajectoryView(null)
    setLoadingTrajectory(false)
    setLoadingHistoryId(null)
    setLogError('')
    if (intent && intent !== 'trajectory') {
      setSubTab(intent)
      return
    }
    setSubTab(current => (current === 'trajectory' || current === 'history' ? 'tasks' : current))
  }, [selectedProjectSlug, activeLogKey, destination?.kind, logRetry, api, applyLogSlice, startLiveInspect, stopLiveInspect])

  const pagingKey = logPagingKey(destination)

  useEffect(() => {
    if (!selectedProjectSlug || destination?.kind !== 'log') return
    if (suppressPagingFetchRef.current) {
      suppressPagingFetchRef.current = false
      appliedPagingKeyRef.current = pagingKey
      return
    }
    if (appliedPagingKeyRef.current === null || appliedPagingKeyRef.current === pagingKey) return
    const dest = destination
    const slug = selectedProjectSlug
    const gen = ++logPagingGenRef.current
    appliedPagingKeyRef.current = pagingKey
    setLogPageLoading(true)
    void (async () => {
      try {
        await applyLogSlice(slug, dest, gen)
      } catch (err) {
        if (logPagingGenRef.current !== gen || slugRef.current !== slug) return
        setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.historicalRun'))
      } finally {
        if (logPagingGenRef.current === gen) setLogPageLoading(false)
      }
    })()
  }, [selectedProjectSlug, pagingKey, destination, applyLogSlice])

  useEffect(() => {
    if (liveJobId) return
    stopLiveInspect()
  }, [liveJobId, stopLiveInspect])

  // 2. Real-time SSE Stream for Live Job Inspection
  useEffect(() => {
    if (!selectedProjectSlug || !inspectingJobId || inspectingJobId !== liveJobId) return

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
        } else if (event.type === 'partial_update') {
          setActiveTrajectoryView(prev =>
            prev
              ? {
                  ...prev,
                  partialThinking: event.partialThinking,
                  partialText: event.partialText,
                  durationMs: event.durationMs,
                }
              : prev,
          )
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
              partialThinking: null,
              partialText: null,
              stepCount: existingSteps.length,
              toolCallsCount: toolCalls,
              durationMs: event.durationMs || prev.durationMs,
              providerStats: event.providerStats ?? prev.providerStats,
              trajectory: {
                ...prev.trajectory,
                providerStats: event.providerStats ?? prev.trajectory.providerStats,
                steps: existingSteps,
              },
            }
          })
        } else if (event.type === 'finished') {
          setActiveTrajectoryView(prev =>
            prev
              ? {
                  ...prev,
                  partialThinking: null,
                  partialText: null,
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
        const jobId = inspectingJobId
        stopLiveInspect()
        try {
          if (liveSubscriptionTarget(destinationRef.current) !== jobId) return
          if (slugRef.current !== selectedProjectSlug) return
          const historyRes = await api.listProjectHistory(selectedProjectSlug)
          const historyList = normalizeHistoryResponse(historyRes).items
          const latest = historyList.find(item => !taskId || item.taskId === taskId)
          if (liveSubscriptionTarget(destinationRef.current) !== jobId) return
          if (latest) {
            const paging = resolvedLogPaging(destinationRef.current, savedAgentLogPageSize())
            const run = await api.getProjectHistoryDetail(selectedProjectSlug, latest.fileName, paging)
            if (liveSubscriptionTarget(destinationRef.current) !== jobId) return
            setActiveTrajectoryView(run)
            setSelectedHistoryId(latest.id)
            setSelectedHistoryFileName(latest.fileName)
          }
        } catch {
          // Retain last live snapshot if history is not yet available
        }
        if (slugRef.current === selectedProjectSlug) {
          void loadTasksAndJobs(selectedProjectSlug, true)
        }
      },
    )

    return () => {
      unsubscribe()
    }
  }, [selectedProjectSlug, inspectingJobId, inspectingTaskId, liveJobId, api])

  const newTaskFormBase = (): ProjectTaskDefinition => ({
    id: `task-${Date.now().toString().slice(-6)}`,
    name: '',
    category: '',
    tags: [],
    prompt: '',
    maxTurns: config?.execution.maxTurns || 25,
    timeoutSeconds: config?.execution.timeoutSeconds || 300,
    cron: '',
    schedulePaused: false,
  })

  const collectTaskCreatorState = (): TaskCreatorState => ({
    taskForm,
    taskTagsInput,
    schedule,
    verifierType,
    verifierCommand,
    verifierExpectedStdout,
    verifierExpectedExitCode,
    verifierFilePath,
    verifierExpectedContent,
    section: taskCreatorSection,
    expanded: taskCreatorExpanded,
  })

  const applyTaskCreatorState = (state: TaskCreatorState) => {
    setTaskForm(state.taskForm)
    setTaskTagsInput(state.taskTagsInput)
    setSchedule({ ...state.schedule })
    setVerifierType(state.verifierType)
    setVerifierCommand(state.verifierCommand)
    setVerifierExpectedStdout(state.verifierExpectedStdout)
    setVerifierExpectedExitCode(state.verifierExpectedExitCode)
    setVerifierFilePath(state.verifierFilePath)
    setVerifierExpectedContent(state.verifierExpectedContent)
    setTaskCreatorSection(state.section)
    setTaskCreatorExpanded({ ...state.expanded })
  }

  const resetTaskCreatorTransient = () => {
    setEstimateInfo(null)
    setEstimateFeedback('')
    setDraftMessages([])
    setDraftInput('')
    setDrafting(false)
    setTaskAttachments([])
    setExistingAttachmentMentions([])
    setTaskMentionPopover(closedTaskMentionPopover)
  }

  const handleOpenCreateTask = () => {
    const { state, restoredAt } = prepareTaskCreator({
      projectId: selectedProjectSlug,
      base: newTaskFormBase(),
      schedule: { ...defaultSchedule },
    })
    setEditingTaskId(null)
    applyTaskCreatorState(state)
    setTaskCreatorRestoredFrom(restoredAt)
    resetTaskCreatorTransient()
    setIsEditingTask(true)
  }

  const handleSendDraft = async (textOverride?: string) => {
    if (!selectedProjectSlug || drafting) return
    const text = (textOverride ?? draftInput).trim()
    if (!text) return
    const nextMessages: TaskDraftMessage[] = [...draftMessages, { role: 'user', content: text }]
    setDraftMessages(nextMessages)
    setDraftInput('')
    setTaskMentionPopover(prev =>
      prev.field === 'draft' ? { ...prev, isOpen: false } : prev,
    )
    setDrafting(true)
    setError('')
    try {
      const res = await api.draftProjectTask(selectedProjectSlug, {
        messages: nextMessages,
        current: taskFormToDraft(taskForm, taskTagsInput),
      })
      setDraftMessages([...nextMessages, { role: 'assistant', content: res.reply }])
      if (res.draft) {
        const replaceId = shouldReplaceGeneratedTaskId(taskForm.id, Boolean(editingTaskId))
        setTaskForm(prev => applyTaskDraft(prev, res.draft!, { replaceId }))
        setTaskTagsInput((res.draft.tags ?? []).join(', '))
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.draft'))
    } finally {
      setDrafting(false)
    }
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
  }, [api, selectedProjectSlug, locale])

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
  }, [api, isEditingTask, selectedProjectSlug, taskForm.id, locale])

  const pendingAttachmentMentions = useMemo(
    () =>
      mentionItemsFromAttachmentPaths(
        workspacePathsForAttachments(
          taskAttachments.map(file => file.name),
          taskForm.id,
          existingAttachmentMentions.map(item => item.relativePath),
        ),
      ),
    [taskAttachments, taskForm.id, existingAttachmentMentions, locale],
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
    (text: string, cursorPos: number, field: TaskMentionField) => {
      const trigger = detectMentionTrigger(text, cursorPos)
      if (!trigger) {
        dismissedTaskMentionRef.current = null
        dismissedTaskMentionFieldRef.current = null
        setTaskMentionPopover(prev => (prev.isOpen ? { ...prev, isOpen: false } : prev))
        return
      }

      const sameDismissedField = dismissedTaskMentionFieldRef.current === field
      if (sameDismissedField && shouldIgnoreMentionTrigger(trigger, dismissedTaskMentionRef.current)) {
        return
      }

      if (sameDismissedField && Date.now() - lastDismissedTaskTimeRef.current < 200) {
        return
      }

      dismissedTaskMentionRef.current = null
      dismissedTaskMentionFieldRef.current = null

      if (taskMentionItems.length === 0 && !loadingTaskMentions) {
        void loadTaskMentionSources()
      }

      setTaskMentionPopover({
        isOpen: true,
        query: trigger.query,
        mentionStart: trigger.mentionStart,
        selectedIndex: 0,
        field,
      })
    },
    [taskMentionItems.length, loadingTaskMentions, loadTaskMentionSources],
  )

  const debouncedTaskCursorMentionUpdate = useMemo(
    () =>
      debounce((text: string, pos: number, field: TaskMentionField) => {
        updateTaskMentionState(text, pos, field)
      }, 50),
    [updateTaskMentionState],
  )

  const handleTaskMentionClose = useCallback(() => {
    setTaskMentionPopover(prev => {
      if (prev.isOpen) {
        dismissedTaskMentionRef.current = {
          mentionStart: prev.mentionStart,
          query: prev.query,
        }
        dismissedTaskMentionFieldRef.current = prev.field
        lastDismissedTaskTimeRef.current = Date.now()
      }
      return { ...prev, isOpen: false }
    })
  }, [])

  const handleTaskPromptChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = e.target.value
    const cursorPos = e.target.selectionStart ?? val.length
    setTaskForm({ ...taskForm, prompt: val })
    updateTaskMentionState(val, cursorPos, 'prompt')
  }

  const handleDraftInput = (value: string, cursor: number) => {
    setDraftInput(value)
    updateTaskMentionState(value, cursor, 'draft')
  }

  const handleTaskMentionSelect = (item: MentionDisplayItem) => {
    dismissedTaskMentionRef.current = null
    dismissedTaskMentionFieldRef.current = null
    const field = taskMentionPopover.field
    const source =
      field === 'draft'
        ? (draftTextareaRef.current?.value ?? draftInput)
        : (taskPromptTextareaRef.current?.value ?? taskForm.prompt)
    const { newText, newCursorPos } = applyMentionInsertion(
      source,
      taskMentionPopover.mentionStart,
      taskMentionPopover.query.length,
      item.insertText,
    )
    if (field === 'draft') {
      applyDraftText(newText, newCursorPos)
      return
    }
    applyPromptText(newText, newCursorPos)
  }

  const handleMentionMenuKeyDown = (
    e: React.KeyboardEvent<HTMLTextAreaElement>,
    field: TaskMentionField,
  ): boolean => {
    const popoverRef = field === 'draft' ? draftMentionPopoverRef : taskMentionPopoverRef
    if (!taskMentionPopover.isOpen || taskMentionPopover.field !== field || !popoverRef.current) {
      return false
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      const displayItems = popoverRef.current.getDisplayItems()
      const maxIndex = Math.max(0, displayItems.length - 1)
      setTaskMentionPopover(prev => ({
        ...prev,
        selectedIndex: Math.min(prev.selectedIndex + 1, maxIndex),
      }))
      return true
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      setTaskMentionPopover(prev => ({
        ...prev,
        selectedIndex: Math.max(prev.selectedIndex - 1, 0),
      }))
      return true
    }
    if (e.key === 'Enter' || e.key === 'Tab') {
      const displayItems = popoverRef.current.getDisplayItems()
      if (displayItems.length > 0) {
        e.preventDefault()
        e.stopPropagation()
        popoverRef.current.selectItem(taskMentionPopover.selectedIndex)
        return true
      }
      return false
    }
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      dismissedTaskMentionRef.current = {
        mentionStart: taskMentionPopover.mentionStart,
        query: taskMentionPopover.query,
      }
      dismissedTaskMentionFieldRef.current = field
      lastDismissedTaskTimeRef.current = Date.now()
      setTaskMentionPopover(prev => ({ ...prev, isOpen: false }))
      return true
    }
    return false
  }

  const handleTaskPromptKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    handleMentionMenuKeyDown(e, 'prompt')
  }

  const isCreatingTask = isEditingTask && !editingTaskId

  const taskCreatorDraftInput = useMemo(
    () => taskCreatorDataToDraft(collectTaskCreatorState()),
    [
      taskForm,
      taskTagsInput,
      schedule,
      verifierType,
      verifierCommand,
      verifierExpectedStdout,
      verifierExpectedExitCode,
      verifierFilePath,
      verifierExpectedContent,
      taskCreatorSection,
      taskCreatorExpanded,
    ],
  )

  const taskDraftAutosave = useMemo(
    () => createTaskDraftAutosave({ projectId: selectedProjectSlug }),
    [selectedProjectSlug],
  )

  /* The ESC handler runs from a window listener registered in an effect, so it closes over a stale
     render. Reading the newest snapshot from a ref keeps a collapse from persisting old keystrokes. */
  const taskCreatorDraftInputRef = useRef(taskCreatorDraftInput)
  taskCreatorDraftInputRef.current = taskCreatorDraftInput

  const flushTaskCreatorDraftImmediately = () => {
    taskDraftAutosave.update(taskCreatorDraftInputRef.current)
    taskDraftAutosave.flush()
  }

  const clearTaskCreatorDraftImmediately = () => {
    taskDraftAutosave.clear()
  }

  const taskDraftCreatingRef = useRef(isCreatingTask)
  taskDraftCreatingRef.current = isCreatingTask

  /* Keyed on the autosave instance: when the project changes, the cleanup flushes the queued write to
     the project the keystrokes belong to instead of dropping it. The instance already holds the newest
     snapshot, so this never writes across projects. */
  useEffect(() => {
    const autosave = taskDraftAutosave
    return () => {
      autosave.dispose({ flush: taskDraftCreatingRef.current })
    }
  }, [taskDraftAutosave])

  useEffect(() => {
    if (!shouldPersistTaskCreatorDraft({ isCreatingTask, saving: savingTask })) return
    /* persistTaskDraft clears the entry for a blank form, so erasing every field drops the draft
       instead of leaving stale content to restore on the next visit. */
    taskDraftAutosave.update(taskCreatorDraftInput)
  }, [isCreatingTask, savingTask, taskCreatorDraftInput, taskDraftAutosave])

  const closeTaskCreator = (options: { discardDraft: boolean }) => {
    if (savingTask) return
    const isCreating = isEditingTask && !editingTaskId
    if (isCreating) {
      /* Flush the pending debounce so collapsing the panel keeps the latest keystrokes; an empty
         form clears the draft instead of leaving a blank one behind. */
      if (options.discardDraft) {
        clearTaskCreatorDraftImmediately()
      } else {
        flushTaskCreatorDraftImmediately()
      }
    }
    resetTaskCreatorTransient()
    setTaskCreatorRestoredFrom(null)
    setIsEditingTask(false)
  }

  const handleToggleTaskCreatorSection = (section: TaskCreateSection) => {
    setTaskCreatorSection(section)
    setTaskCreatorExpanded(prev => toggleTaskCreatorSection(prev, section))
  }

  const handleDiscardTaskCreatorDraft = () => {
    if (savingTask) return
    clearTaskCreatorDraftImmediately()
    const state = taskCreatorDefaults(newTaskFormBase(), { ...defaultSchedule })
    applyTaskCreatorState(state)
    resetTaskCreatorTransient()
    setTaskCreatorRestoredFrom(null)
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
      setDraftMessages([])
      setDraftInput('')
      setDrafting(false)
      setTaskAttachments([])
      setExistingAttachmentMentions([])
      setTaskMentionPopover(closedTaskMentionPopover)
      setTaskCreatorRestoredFrom(null)
      /* Editing an existing task starts from a clean layout rather than the collapsed sections the
         user left the creator in. */
      setTaskCreatorSection('basics')
      setTaskCreatorExpanded(defaultTaskCreateDraftData().ui.expanded)
      setIsEditingTask(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.taskDetail'))
    }
  }

  const handleEstimateBudget = async (feedbackOverride?: string) => {
    if (!selectedProjectSlug) return
    if (!taskForm.prompt.trim()) {
      setError(pageMessage('harness.page.errors.promptForBudget'))
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
        subtasks: recalculateSubtasksTurns(prev.subtasks, res.suggestedMaxTurns, maxTurnsLimit),
      }))
      setSuccessMsg(
        feedbackToSend
          ? pageMessage('harness.page.budget.refined', { turns: res.suggestedMaxTurns, seconds: res.suggestedTimeoutSeconds, complexity: res.complexity })
          : pageMessage('harness.page.budget.estimated', { turns: res.suggestedMaxTurns, seconds: res.suggestedTimeoutSeconds, complexity: res.complexity }),
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.estimate'))
    } finally {
      setEstimatingBudget(false)
    }
  }

  const handleAdjustBudgetMultiplier = (multiplier: number) => {
    const currentTurns = taskForm.maxTurns || 25
    const newTurns = Math.max(1, Math.min(maxTurnsLimit, Math.round(currentTurns * multiplier)))
    const currentTimeout = taskForm.timeoutSeconds || 300
    const newTimeout = Math.max(
      10,
      Math.min(7200, Math.round(currentTimeout * (multiplier > 1 ? multiplier * 0.9 : multiplier * 1.1))),
    )
    setTaskForm(prev => ({
      ...prev,
      maxTurns: newTurns,
      timeoutSeconds: newTimeout,
      subtasks: recalculateSubtasksTurns(prev.subtasks, newTurns, maxTurnsLimit),
    }))
    setSuccessMsg(pageMessage('harness.page.budget.adjusted', { turns: newTurns, seconds: newTimeout }))
  }

  const handleAttachFiles = (e: React.ChangeEvent<HTMLInputElement>) => {
    const selected = Array.from(e.target.files ?? [])
    const accepted = selected.filter(file => isAcceptedTaskAttachment(file.name))
    if (accepted.length === 0) {
      e.target.value = ''
      if (selected.length > 0) {
        setError(pageMessage('harness.page.errors.attachmentType'))
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

  const placeTextareaCursor = (node: HTMLTextAreaElement, newText: string, newCursorPos: number) => {
    node.value = newText
    node.focus()
    setTimeout(() => {
      node.setSelectionRange(newCursorPos, newCursorPos)
    }, 0)
  }

  const applyPromptText = (newText: string, newCursorPos: number) => {
    setTaskForm(prev => ({ ...prev, prompt: newText }))
    setTaskMentionPopover(prev => ({ ...prev, isOpen: false }))
    if (taskPromptTextareaRef.current) {
      placeTextareaCursor(taskPromptTextareaRef.current, newText, newCursorPos)
    }
  }

  const applyDraftText = (newText: string, newCursorPos: number) => {
    setDraftInput(newText)
    setTaskMentionPopover(prev => ({ ...prev, isOpen: false }))
    if (draftTextareaRef.current) {
      placeTextareaCursor(draftTextareaRef.current, newText, newCursorPos)
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
      setError(pageMessage('harness.page.errors.taskId'))
      return
    }
    if (!taskForm.prompt.trim()) {
      setError(pageMessage('harness.page.errors.prompt'))
      return
    }
    const subtaskError = subtaskSaveError(taskForm.subtasks)
    if (subtaskError) {
      setError(subtaskError)
      return
    }

    const tags = taskTagsInput
      .split(',')
      .map(task => task.trim())
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
      const payload: ProjectTaskDefinition = {
        ...taskForm,
        prompt,
        tags,
        verifier: verifierSpec,
        cron,
        schedulePaused: Boolean(cron) && Boolean(taskForm.schedulePaused),
        subtasks: subtasksForSave(taskForm.subtasks),
        provider: taskForm.provider?.trim() || undefined,
        model: taskForm.model?.trim() || undefined,
        secondaryProvider: taskForm.secondaryProvider?.trim() || undefined,
        secondaryModel: taskForm.secondaryModel?.trim() || undefined,
      }

      if (editingTaskId) {
        await api.updateProjectTask(selectedProjectSlug, editingTaskId, payload)
        setSuccessMsg(pageMessage('harness.page.notices.updated', { taskId: taskForm.id }))
      } else {
        await api.createProjectTask(selectedProjectSlug, payload)
        setSuccessMsg(pageMessage('harness.page.notices.created', { taskId: taskForm.id }))
      }
      /* A collapsed create draft survives editing an existing task, so only a successful create
         clears it. */
      if (isCreatingTask) clearTaskCreatorDraftImmediately()
      setTaskCreatorRestoredFrom(null)
      setTaskAttachments([])
      setExistingAttachmentMentions([])
      setIsEditingTask(false)
      void loadTasksAndJobs(selectedProjectSlug, true)
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.saveTask'))
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
          ? pageMessage('harness.page.notices.scheduleResumed', { taskId: task.id })
          : pageMessage('harness.page.notices.schedulePaused', { taskId: task.id }),
      )
      await loadTasksAndJobs(selectedProjectSlug)
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.schedule'))
    }
  }

  const handleDeleteTask = async (taskId: string, taskName?: string) => {
    if (!selectedProjectSlug) return
    const displayName = taskName?.trim() || taskId
    if (!confirm(t('harness.page.tasks.deleteConfirm', { name: displayName }))) return
    setError('')
    setSuccessMsg('')
    try {
      await api.deleteProjectTask(selectedProjectSlug, taskId)
      setSuccessMsg(pageMessage('harness.page.notices.deleted', { name: displayName }))
      await loadTasksAndJobs(selectedProjectSlug)
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.deleteTask'))
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
      setSuccessMsg(pageMessage('harness.page.notices.completed', { taskId: taskId, status: res.status }))
      await loadTasksAndJobs(selectedProjectSlug, true)
      const historyRes = await api.listProjectHistory(selectedProjectSlug, {
        page: 1,
        limit: historyPageSize,
      })
      const norm = normalizeHistoryResponse(historyRes)
      setHistory(norm.items)
      setHistoryPagination(norm.pagination ?? null)
      setHistoryPage(1)
      const historyList = norm.items
      const isThisLogOpen =
        subTab === 'trajectory' &&
        (inspectingJobId !== null || activeTrajectoryView?.taskId === taskId)
      if (isThisLogOpen) {
        const latest = latestRunForTask(historyList, taskId)
        if (latest) {
          setSelectedHistoryId(latest.id)
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.runTask', { taskId: taskId }))
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
              error: err instanceof Error ? err.message : pageMessage('harness.page.errors.taskPrompt'),
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
    resumeSubtaskId?: string,
    resumeSubtaskTitle?: string,
    failureReason?: string,
  ) => {
    setContinueModalTask({
      taskId,
      runId,
      taskName: taskName || tasks.find(task => task.id === taskId)?.name || taskId,
      previousSteps,
      progressSummary,
      resumeSubtaskId,
      resumeSubtaskTitle,
      failureReason,
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

  const handleSaveConfig = async () => {
    if (!selectedProjectSlug || !config) return
    setSavingConfig(true)
    setError('')
    setSuccessMsg('')
    try {
      const saved = await api.updateProjectHarnessConfig(selectedProjectSlug, config)
      setConfig(normalizeHarnessConfig(saved))
      setSuccessMsg(pageMessage('harness.page.notices.configSaved'))
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.saveConfig'))
    } finally {
      setSavingConfig(false)
    }
  }

  const latestByTaskId = useMemo(() => {
    const map = new Map<string, HarnessHistorySummary>()
    for (const task of tasks) {
      const latest = latestRunForTask(history, task.id)
      if (latest) {
        map.set(task.id, latest)
      }
    }
    return map
  }, [tasks, history])

  const runningJobStartedAt = useMemo(() => {
    const map = new Map<string, string>()
    for (const job of activeJobs) {
      if (job.taskId && job.startedAt) {
        map.set(job.taskId, job.startedAt)
      }
    }
    return map
  }, [activeJobs])

  const filteredTasks = useMemo(
    () =>
      filterAndSortProjectTasks(tasks, {
        query: filterQuery,
        tag: filterTag,
        sort: taskSort,
        latestByTaskId,
        runningJobStartedAt,
      }),
    [tasks, filterQuery, filterTag, taskSort, latestByTaskId, runningJobStartedAt],
  )

  const taskNamesById = useMemo(
    () => Object.fromEntries(tasks.map(task => [task.id, task.name])),
    [tasks],
  )

  const allTags = Array.from(new Set(tasks.flatMap(task => (Array.isArray(task.tags) ? task.tags : []))))

  const handleRunProjectEval = async () => {
    if (!selectedProjectSlug) return
    setRunningEval(true)
    setError('')
    setSuccessMsg('')
    try {
      const taskIds = filteredTasks.map(task => task.id)
      const report = await api.runProjectHarnessEval(selectedProjectSlug, { taskIds })
      setSelectedReportDetail(report)
      setSuccessMsg(
        pageMessage('harness.page.evaluation.completed', { rate: formatNumber((report.metrics.passRate * 100), { minimumFractionDigits: 1, maximumFractionDigits: 1, useGrouping: false }), passed: report.metrics.passedTasks, total: report.metrics.totalTasks }),
      )
      await loadReports(selectedProjectSlug, true)
    } catch (err) {
      setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.evaluation'))
    } finally {
      setRunningEval(false)
    }
  }

  useEffect(() => {
    if (!dynamicRun) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setDynamicRun(null)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [dynamicRun])

  useEffect(() => {
    if (!isEditingTask) return
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target
      const editor = taskEditorRef.current
      const targetInsideEditor = target instanceof Node && editor ? editor.contains(target) : false
      if (
        !escapeClosesTaskEditor({
          key: event.key,
          defaultPrevented: event.defaultPrevented,
          isComposing: event.isComposing || event.keyCode === 229,
          imeLocked: Date.now() < imeEscapeUntilRef.current,
          mentionOpen: taskMentionPopover.isOpen,
          saving: savingTask,
          targetInsideEditor,
        })
      ) {
        return
      }
      closeTaskCreator({ discardDraft: false })
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [isEditingTask, savingTask, taskMentionPopover.isOpen])

  return (
    <div className="project-harness-container">
      {subTab !== 'trajectory' && (
        <div className="project-subnav-bar">
          <div className="project-subnav-tabs">
            <button
              type="button"
              className={`project-subnav-btn ${subTab === 'tasks' ? 'active' : ''}`}
              onClick={() => {
                sectionIntentRef.current = 'tasks'
                setSubTab('tasks')
                setMoreNavOpen(false)
                onHubNavigate?.({ tab: 'harness', destination: null })
              }}
            >
              {t('harness.page.nav.tasks', { count: tasks.length })}
            </button>
            <button
              type="button"
              className={`project-subnav-btn ${subTab === 'history' ? 'active' : ''}`}
              onClick={() => {
                sectionIntentRef.current = null
                setSubTab('history')
                setMoreNavOpen(false)
                onHubNavigate?.({ tab: 'harness', destination: { kind: 'history' } })
              }}
            >
              {t('harness.page.nav.history', { countLabel: loadedSections.history ? `(${history.length})` : '' })}
            </button>
            <div className="subnav-more-container">
              <button
                type="button"
                className={`project-subnav-btn subnav-more-btn ${['reports', 'config', 'cassettes'].includes(subTab) ? 'active' : ''}`}
                onClick={() => setMoreNavOpen(prev => !prev)}
                aria-expanded={moreNavOpen}
              >
                <span>{t('harness.page.nav.more')}</span>
                <ChevronDown size={13} />
              </button>
              {moreNavOpen && (
                <div className="subnav-more-menu" role="menu">
                  <button
                    type="button"
                    className={`subnav-more-item ${subTab === 'reports' ? 'active' : ''}`}
                    role="menuitem"
                    onClick={() => {
                      sectionIntentRef.current = 'reports'
                      setSubTab('reports')
                      setMoreNavOpen(false)
                      onHubNavigate?.({ tab: 'harness', destination: null })
                    }}
                  >
                    <BarChart3 size={14} />
                    <span>{t('harness.page.nav.reports', { countLabel: loadedSections.reports ? `(${reports.length})` : '' })}</span>
                  </button>
                  <button
                    type="button"
                    className={`subnav-more-item ${subTab === 'config' ? 'active' : ''}`}
                    role="menuitem"
                    onClick={() => {
                      sectionIntentRef.current = 'config'
                      setSubTab('config')
                      setMoreNavOpen(false)
                      onHubNavigate?.({ tab: 'harness', destination: null })
                    }}
                  >
                    <Settings size={14} />
                    <span>{t('harness.page.nav.config')}</span>
                  </button>
                  <button
                    type="button"
                    className={`subnav-more-item ${subTab === 'cassettes' ? 'active' : ''}`}
                    role="menuitem"
                    onClick={() => {
                      sectionIntentRef.current = 'cassettes'
                      setSubTab('cassettes')
                      setMoreNavOpen(false)
                      onHubNavigate?.({ tab: 'harness', destination: null })
                    }}
                  >
                    <Disc size={14} />
                    <span>{t('harness.page.nav.cassettes', { countLabel: loadedSections.cassettes ? `(${cassettes.length})` : '' })}</span>
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {error && <div className="error-banner">{renderMessage(error)}</div>}
      {successMsg && <div className="success-banner">{renderMessage(successMsg)}</div>}

      {(() => {
        const visibleActiveJobs = activeJobs.filter(
          job => !(subTab === 'trajectory' && inspectingJobId === job.jobId),
        )
        if (visibleActiveJobs.length === 0) return null

        return (
          <div className="active-jobs-banner sticky-active-jobs">
            <div className="active-jobs-list">
              {visibleActiveJobs.map(job => {
                const matchedTask = tasks.find(task => task.id === job.taskId)
                const title = matchedTask?.name || job.description || job.taskId
                const elapsed = formatDurationMs(
                  Math.max(0, now - (Date.parse(job.startedAt) || now)),
                )
                const isInspected =
                  liveInspect && inspectingJobId === job.jobId && Boolean(activeTrajectoryView)
                const actionText = isInspected
                  ? liveLineText(
                      buildAgentLog(activeTrajectoryView),
                      true,
                      String(activeTrajectoryView?.status || 'running'),
                    )
                  : undefined

                return (
                  <div
                    key={job.jobId}
                    id={`active-job-${job.jobId}`}
                    className="active-job-item active-job-row-interactive"
                    role="button"
                    tabIndex={0}
                    onClick={() => void handleInspectRunningJob(job)}
                    onKeyDown={e => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault()
                        void handleInspectRunningJob(job)
                      }
                    }}
                    title={t('harness.page.logs.openLive')}
                  >
                    <span className="live-dot" />
                    <span className="active-job-title">{title}</span>
                    <span className="active-job-elapsed">
                      {job.currentStatus === 'queued' ? t('harness.page.jobs.queued', { elapsed: elapsed }) : elapsed}
                    </span>
                    {job.model && <span className="active-job-model">{job.model}</span>}
                    {actionText && <span className="active-job-action">{actionText}</span>}
                    <button
                      type="button"
                      className="btn-action stop-btn active-job-stop"
                      onClick={e => {
                        e.stopPropagation()
                        void handleStopRunningJob(job)
                      }}
                      disabled={stoppingTaskId === job.taskId}
                      title={job.currentStatus === 'queued' ? t('harness.page.jobs.cancelQueued') : t('harness.page.jobs.stopTitle')}
                    >
                      {stoppingTaskId === job.taskId
                        ? t('harness.page.jobs.stopping')
                        : job.currentStatus === 'queued' ? t('harness.page.actions.cancel') : t('harness.page.actions.stop')}
                    </button>
                  </div>
                )
              })}
            </div>
          </div>
        )
      })()}

      {/* ================= SUBTAB: TASKS ================= */}
      {subTab === 'tasks' && (
        <div className="project-tasks-view">
          <div className="tasks-header-bar">
            <div className="tasks-search-row">
              <input
                type="text"
                className="form-input search-input"
                placeholder={t('harness.page.search.placeholder')}
                value={filterQuery}
                onChange={e => setFilterQuery(e.target.value)}
              />
              {allTags.length > 0 && (
                <div className="tags-filter-bar">
                  <span className="chips-label">{t('harness.page.search.tag')}</span>
                  <button
                    type="button"
                    className={`chip-btn ${filterTag === '' ? 'selected' : ''}`}
                    onClick={() => setFilterTag('')}
                  >
                    {t('harness.page.search.all')}</button>
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
              <div className="tasks-sort-control">
                <select
                  className="form-input form-select-sm task-sort-select"
                  value={taskSort}
                  onChange={e => setTaskSort(e.target.value as ProjectTaskSort)}
                  aria-label={t('harness.page.search.sort')}
                >
                  <option value="activity">{t('harness.page.search.activity')}</option>
                  <option value="created">{t('harness.page.search.created')}</option>
                </select>
              </div>
              <button
                type="button"
                className="btn-secondary btn-sm run-eval-btn"
                onClick={handleRunProjectEval}
                disabled={runningEval || filteredTasks.length === 0}
              >
                {runningEval ? t('harness.page.evaluation.running') : t('harness.page.evaluation.runFiltered', { count: filteredTasks.length })}
              </button>
            </div>

            <div className="tasks-actions-row">
              {!isEditingTask && (
                <button type="button" className="btn-primary" onClick={handleOpenCreateTask}>
                  {t('harness.page.actions.create')}</button>
              )}
              <button
                type="button"
                className="btn-secondary"
                onClick={() => {
                  if (selectedProjectSlug) {
                    void loadSubTabData(selectedProjectSlug, subTab, true)
                  }
                }}
                disabled={loading || Boolean(sectionLoading) || runningEval || runningTaskId !== null}
                title={t('harness.page.actions.refreshTitle')}
              >
                <RotateCcw size={13} />
                <span>{t('harness.page.actions.refresh')}</span>
              </button>
            </div>
          </div>

          {/* Inline task creator */}
          {isEditingTask && (
            <section
              ref={taskEditorRef}
              className="inline-task-creator"
              aria-label={editingTaskId ? t('harness.page.editor.label') : t('harness.page.editor.createLabel')}
              onCompositionStart={() => {
                imeEscapeUntilRef.current = Number.POSITIVE_INFINITY
              }}
              onCompositionEnd={() => {
                imeEscapeUntilRef.current = Date.now() + 150
              }}
            >
              <div className="inline-task-creator-header">
                <h3 className="section-title inline-task-creator-title">
                  {editingTaskId ? (taskForm.name || editingTaskId) : t('harness.page.editor.title')}
                </h3>
                <div className="inline-task-creator-actions">
                  {isCreatingTask && (
                    <button
                      type="button"
                      className="btn-secondary btn-sm"
                      onClick={handleDiscardTaskCreatorDraft}
                      disabled={savingTask}
                    >
                      {t('harness.page.editor.discard')}</button>
                  )}
                  <button
                    type="button"
                    className="btn-secondary btn-sm"
                    onClick={() => closeTaskCreator({ discardDraft: false })}
                    disabled={savingTask}
                  >
                    {t('harness.page.editor.collapse')}</button>
                </div>
              </div>
              {isCreatingTask && taskCreatorRestoredFrom && (
                <div className="inline-task-creator-banner" role="status">
                  <span>{describeDraftRestored(taskCreatorRestoredFrom)}</span>
                  <div className="inline-task-creator-banner-actions">
                    <button
                      type="button"
                      className="btn-secondary btn-sm"
                      onClick={() => setTaskCreatorRestoredFrom(null)}
                    >
                      {t('harness.page.editor.keepEditing')}</button>
                    <button
                      type="button"
                      className="btn-secondary btn-sm"
                      onClick={handleDiscardTaskCreatorDraft}
                      disabled={savingTask}
                    >
                      {t('harness.page.editor.startOver')}</button>
                  </div>
                </div>
              )}
              <div className="form-grid">
                <TaskDraftChat
                  messages={draftMessages}
                  input={draftInput}
                  drafting={drafting}
                  onInput={handleDraftInput}
                  onSend={() => void handleSendDraft()}
                  textareaRef={draftTextareaRef}
                  mentionOpen={taskMentionPopover.isOpen && taskMentionPopover.field === 'draft'}
                  onMentionKeyDown={event => handleMentionMenuKeyDown(event, 'draft')}
                  onCursorActivity={(value, cursor) =>
                    debouncedTaskCursorMentionUpdate(value, cursor, 'draft')
                  }
                  mentionPopover={
                    <MentionPopover
                      ref={draftMentionPopoverRef}
                      isOpen={taskMentionPopover.isOpen && taskMentionPopover.field === 'draft'}
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
                      placement="below"
                    />
                  }
                />
                <div className="inline-task-section">
                  <button
                    type="button"
                    className="inline-task-section-header"
                    onClick={() => handleToggleTaskCreatorSection('basics')}
                    aria-expanded={taskCreatorExpanded.basics}
                  >
                    <ChevronDown
                      size={14}
                      className={taskCreatorExpanded.basics ? 'section-chevron expanded' : 'section-chevron'}
                    />
                    <span>{t('harness.page.editor.basics')}</span>
                  </button>
                  {taskCreatorExpanded.basics && (
                    <div className="inline-task-section-body">
                      <div className="form-row-2">
                        <div className="form-group">
                          <label className="form-label">{t('harness.page.editor.taskId')}</label>
                          <input
                            type="text"
                            className="form-input"
                            value={taskForm.id}
                            onChange={e => setTaskForm({ ...taskForm, id: e.target.value })}
                            placeholder={t('harness.page.editor.idPlaceholder')}
                            disabled={!!editingTaskId}
                          />
                        </div>
                        <div className="form-group">
                          <label className="form-label">{t('harness.page.editor.taskName')}</label>
                          <input
                            type="text"
                            className="form-input"
                            value={taskForm.name || ''}
                            onChange={e => setTaskForm({ ...taskForm, name: e.target.value })}
                            placeholder={t('harness.page.editor.namePlaceholder')}
                          />
                        </div>
                      </div>

                      <div className="form-row-2">
                        <div className="form-group">
                          <label className="form-label">{t('harness.page.editor.category')}</label>
                          <input
                            type="text"
                            className="form-input"
                            value={taskForm.category || ''}
                            onChange={e => setTaskForm({ ...taskForm, category: e.target.value })}
                            placeholder={t('harness.page.editor.categoryPlaceholder')}
                          />
                        </div>
                        <div className="form-group">
                          <label className="form-label">{t('harness.page.editor.tags')}</label>
                          <input
                            type="text"
                            className="form-input"
                            value={taskTagsInput}
                            onChange={e => setTaskTagsInput(e.target.value)}
                            placeholder={t('harness.page.editor.tagsPlaceholder')}
                          />
                        </div>
                      </div>

                      <p className="form-hint">{t('harness.page.editor.providerHelp')}</p>
                      <div className="form-row-2">
                        <div className="form-group">
                          <label className="form-label">{t('harness.page.editor.provider')}</label>
                          <select
                            className="form-input"
                            value={taskForm.provider || ''}
                            onChange={e =>
                              setTaskForm({
                                ...taskForm,
                                provider: e.target.value || undefined,
                                model: undefined,
                              })
                            }
                          >
                            <option value="">{t('harness.page.editor.useProject')}</option>
                            {providerChoices.map(name => (
                              <option key={name} value={name}>
                                {name}
                              </option>
                            ))}
                          </select>
                        </div>
                        <div className="form-group">
                          <label className="form-label">{t('harness.page.editor.model')}</label>
                          <input
                            type="text"
                            list="task-major-model-options"
                            className="form-input"
                            value={taskForm.model || ''}
                            onChange={e =>
                              setTaskForm({ ...taskForm, model: e.target.value || undefined })
                            }
                            placeholder={t('harness.page.editor.modelPlaceholder')}
                          />
                          <datalist id="task-major-model-options">
                            {modelChoicesFor(
                              taskForm.provider ||
                                config?.policy?.provider ||
                                providerCatalog?.globalMajorProvider,
                              taskForm.model,
                            ).map(model => (
                              <option key={model} value={model} />
                            ))}
                          </datalist>
                        </div>
                      </div>
                      <div className="form-row-2">
                        <div className="form-group">
                          <label className="form-label">{t('harness.page.editor.fallbackProvider')}</label>
                          <select
                            className="form-input"
                            value={taskForm.secondaryProvider || ''}
                            onChange={e =>
                              setTaskForm({
                                ...taskForm,
                                secondaryProvider: e.target.value || undefined,
                                secondaryModel: undefined,
                              })
                            }
                          >
                            <option value="">{t('harness.page.editor.useProject')}</option>
                            {providerChoices.map(name => (
                              <option key={name} value={name}>
                                {name}
                              </option>
                            ))}
                          </select>
                        </div>
                        <div className="form-group">
                          <label className="form-label">{t('harness.page.editor.fallbackModel')}</label>
                          <input
                            type="text"
                            list="task-fallback-model-options"
                            className="form-input"
                            value={taskForm.secondaryModel || ''}
                            onChange={e =>
                              setTaskForm({
                                ...taskForm,
                                secondaryModel: e.target.value || undefined,
                              })
                            }
                            placeholder={t('harness.page.editor.modelPlaceholder')}
                          />
                          <datalist id="task-fallback-model-options">
                            {modelChoicesFor(
                              taskForm.secondaryProvider ||
                                config?.policy?.secondaryProvider ||
                                providerCatalog?.globalSecondaryProvider,
                              taskForm.secondaryModel,
                            ).map(model => (
                              <option key={model} value={model} />
                            ))}
                          </datalist>
                        </div>
                      </div>
                    </div>
                  )}
                </div>

                <div className="inline-task-section">
                  <button
                    type="button"
                    className="inline-task-section-header"
                    onClick={() => handleToggleTaskCreatorSection('prompt')}
                    aria-expanded={taskCreatorExpanded.prompt}
                  >
                    <ChevronDown
                      size={14}
                      className={taskCreatorExpanded.prompt ? 'section-chevron expanded' : 'section-chevron'}
                    />
                    <span>{t('harness.page.editor.instructions')}</span>
                  </button>
                  {taskCreatorExpanded.prompt && (
                    <div className="inline-task-section-body">
                      <div className="form-group mention-input-group" style={{ position: 'relative' }}>
                        <label className="form-label">{t('harness.page.editor.prompt')}</label>
                        <MentionPopover
                          ref={taskMentionPopoverRef}
                          isOpen={taskMentionPopover.isOpen && taskMentionPopover.field === 'prompt'}
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
                            const cursorPos = e.currentTarget.selectionStart ?? e.currentTarget.value.length
                            debouncedTaskCursorMentionUpdate(e.currentTarget.value, cursorPos, 'prompt')
                          }}
                          onClick={e => {
                            const cursorPos = e.currentTarget.selectionStart ?? e.currentTarget.value.length
                            debouncedTaskCursorMentionUpdate(e.currentTarget.value, cursorPos, 'prompt')
                          }}
                          onKeyDown={handleTaskPromptKeyDown}
                          placeholder={t('harness.page.editor.promptPlaceholder')}
                        />
                        <button
                          type="button"
                          className="btn-secondary btn-sm task-draft-split"
                          onClick={() =>
                            void handleSendDraft(
                              `Break this into subtasks if it is a long-running task:\n\n${taskForm.prompt}`,
                            )
                          }
                          disabled={drafting || !taskForm.prompt.trim()}
                        >
                          {drafting ? t('harness.page.editor.splitting') : t('harness.page.editor.split')}
                        </button>
                      </div>

                      <div className="form-group task-attach-row">
                        <label className="form-label">{t('harness.page.attachments.label')}</label>
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
                            <Paperclip size={16} />{' '}{t('harness.page.attachments.add')}</button>
                          <span className="form-hint">
                            {t('harness.page.attachments.help')}</span>
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
                                  title={t('harness.page.attachments.mention', { path: item.relativePath })}
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
                                  title={t('harness.page.attachments.mention', { path: item.relativePath })}
                                  onClick={() => handleInsertAttachmentMention(item.relativePath)}
                                >
                                  {item.name}
                                </button>
                                <button
                                  type="button"
                                  aria-label={t('harness.page.attachments.remove', { name: item.name })}
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
                    </div>
                  )}
                </div>

                {/* Verifier Settings */}
                <div className="inline-task-section">
                  <button
                    type="button"
                    className="inline-task-section-header"
                    onClick={() => handleToggleTaskCreatorSection('verifier')}
                    aria-expanded={taskCreatorExpanded.verifier}
                  >
                    <ChevronDown
                      size={14}
                      className={taskCreatorExpanded.verifier ? 'section-chevron expanded' : 'section-chevron'}
                    />
                    <span>{t('harness.page.verifier.heading')}</span>
                  </button>
                  {taskCreatorExpanded.verifier && (
                    <div className="inline-task-section-body">
                      <div className="verifier-section">
                        <label className="form-label">{t('harness.page.verifier.label')}</label>
                        <div className="verifier-type-toggle">
                          <label>
                            <input
                              type="radio"
                              name="verifierType"
                              checked={verifierType === 'none'}
                              onChange={() => setVerifierType('none')}
                            />{' '}
                            {t('harness.page.verifier.manual')}</label>
                          <label>
                            <input
                              type="radio"
                              name="verifierType"
                              checked={verifierType === 'command'}
                              onChange={() => setVerifierType('command')}
                            />{' '}
                            {t('harness.page.verifier.command')}</label>
                          <label>
                            <input
                              type="radio"
                              name="verifierType"
                              checked={verifierType === 'diff'}
                              onChange={() => setVerifierType('diff')}
                            />{' '}
                            {t('harness.page.verifier.diff')}</label>
                        </div>

                        {verifierType === 'command' && (
                          <div className="verifier-subform">
                            <div className="form-group">
                              <label className="form-label">{t('harness.page.verifier.testCommand')}</label>
                              <input
                                type="text"
                                className="form-input"
                                value={verifierCommand}
                                onChange={e => setVerifierCommand(e.target.value)}
                                placeholder={t('harness.page.verifier.commandPlaceholder')}
                              />
                            </div>
                            <div className="form-row-2">
                              <div className="form-group">
                                <label className="form-label">{t('harness.page.verifier.stdout')}</label>
                                <input
                                  type="text"
                                  className="form-input"
                                  value={verifierExpectedStdout}
                                  onChange={e => setVerifierExpectedStdout(e.target.value)}
                                  placeholder={t('harness.page.verifier.stdoutPlaceholder')}
                                />
                              </div>
                              <div className="form-group">
                                <label className="form-label">{t('harness.page.verifier.exitCode')}</label>
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
                              <label className="form-label">{t('harness.page.verifier.filePath')}</label>
                              <input
                                type="text"
                                className="form-input"
                                value={verifierFilePath}
                                onChange={e => setVerifierFilePath(e.target.value)}
                                placeholder={t('harness.page.verifier.pathPlaceholder')}
                              />
                            </div>
                            <div className="form-group">
                              <label className="form-label">{t('harness.page.verifier.content')}</label>
                              <textarea
                                className="form-textarea"
                                rows={3}
                                value={verifierExpectedContent}
                                onChange={e => setVerifierExpectedContent(e.target.value)}
                                placeholder={t('harness.page.verifier.contentPlaceholder')}
                              />
                            </div>
                          </div>
                  )}
                      </div>
                    </div>
                )}
                </div>

                <div className="inline-task-section">
                  <button
                    type="button"
                    className="inline-task-section-header"
                    onClick={() => handleToggleTaskCreatorSection('subtasks')}
                    aria-expanded={taskCreatorExpanded.subtasks}
                  >
                    <ChevronDown
                      size={14}
                      className={taskCreatorExpanded.subtasks ? 'section-chevron expanded' : 'section-chevron'}
                    />
                    <span>{t('harness.page.budget.heading')}</span>
                  </button>
                  {taskCreatorExpanded.subtasks && (
                    <div className="inline-task-section-body">
                      <div className="budget-section">
                        <div className="budget-header-flex">
                          <label className="form-label" style={{ marginBottom: 0 }}>
                            {t('harness.page.budget.label')}</label>
                          <div className="budget-header-actions">
                            <button
                              type="button"
                              className="btn btn-xs btn-outline budget-estimate-btn"
                              onClick={() => handleEstimateBudget()}
                              disabled={estimatingBudget || !taskForm.prompt.trim()}
                              title={t('harness.page.budget.analyze')}
                            >
                              {estimatingBudget
                                ? t('harness.page.budget.estimating')
                                : estimateInfo
                                ? t('harness.page.budget.reestimate')
                                : t('harness.page.budget.estimate')}
                            </button>
                          </div>
                        </div>
                        {estimateInfo && (
                          <div className={`budget-estimate-badge complexity-${estimateInfo.complexity.toLowerCase()}`}>
                            <div className="budget-badge-top">
                              <span className="complexity-tag">{estimateInfo.complexity.toUpperCase()}</span>
                              <span className="budget-source-tag">
                                {estimateInfo.source.includes('model') ? t('harness.page.budget.model') : t('harness.page.budget.heuristic')}
                              </span>
                              <span className="budget-turns-tag">
                                {t('harness.page.budget.summary', { turns: estimateInfo.suggestedMaxTurns, seconds: estimateInfo.suggestedTimeoutSeconds })}
                              </span>
                            </div>
                            <div className="budget-reasoning">{estimateInfo.reasoning}</div>

                            <div className="budget-correction-card">
                              <div className="budget-correction-header">
                                <span className="budget-correction-title">
                                  {t('harness.page.budget.correct')}</span>
                                <div className="budget-quick-adjust-group">
                                  <span className="quick-adjust-label">{t('harness.page.budget.quickAdjust')}</span>
                                  <button
                                    type="button"
                                    className="quick-adjust-btn"
                                    onClick={() => handleAdjustBudgetMultiplier(0.75)}
                                    title={t('harness.page.budget.decrease')}
                                  >
                                    −25%
                                  </button>
                                  <button
                                    type="button"
                                    className="quick-adjust-btn"
                                    onClick={() => handleAdjustBudgetMultiplier(1.25)}
                                    title={t('harness.page.budget.increaseQuarter')}
                                  >
                                    +25%
                                  </button>
                                  <button
                                    type="button"
                                    className="quick-adjust-btn"
                                    onClick={() => handleAdjustBudgetMultiplier(1.5)}
                                    title={t('harness.page.budget.increaseHalf')}
                                  >
                                    +50%
                                  </button>
                                  <button
                                    type="button"
                                    className="quick-adjust-btn"
                                    onClick={() => handleAdjustBudgetMultiplier(2.0)}
                                    title={t('harness.page.budget.doubleTitle')}
                                  >
                                    {t('harness.page.budget.double')}</button>
                                </div>
                              </div>
                              <div className="budget-feedback-row">
                                <input
                                  type="text"
                                  className="form-input budget-feedback-input"
                                  value={estimateFeedback}
                                  onChange={e => setEstimateFeedback(e.target.value)}
                                  placeholder={t('harness.page.budget.feedbackPlaceholder')}
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
                                  title={t('harness.page.budget.refineTitle')}
                                >
                                  {estimatingBudget ? t('harness.page.budget.calibrating') : t('harness.page.budget.applyFeedback')}
                                </button>
                              </div>
                            </div>
                          </div>
                  )}
                      </div>

                  <div className="form-row-2">
                    <div className="form-group">
                      <label className="form-label">{t('harness.page.budget.maxTurns')}</label>
                      <input
                        type="number"
                        min={1}
                        max={maxTurnsLimit}
                        className="form-input"
                        value={taskForm.maxTurns || 25}
                        onChange={e => {
                          const maxTurns = Math.max(1, Math.min(maxTurnsLimit, Number(e.target.value) || 25))
                          setTaskForm(prev => ({
                            ...prev,
                            maxTurns,
                            subtasks: recalculateSubtasksTurns(prev.subtasks, maxTurns, maxTurnsLimit),
                          }))
                        }}
                      />
                    </div>
                    <div className="form-group">
                      <label className="form-label">{t('harness.page.budget.timeout')}</label>
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
                          {t('harness.page.dynamic.enable')}</div>
                        <div style={{ fontSize: '0.75rem', opacity: 0.75, marginTop: '0.15rem', lineHeight: '1.35' }}>
                          {t('harness.page.dynamic.help')}</div>
                      </div>
                    </label>
                  </div>

                  <SubtaskEditor
                    subtasks={taskForm.subtasks ?? []}
                    maxTurnsLimit={maxTurnsLimit}
                    onChange={next => {
                      const newMaxTurns = calculateTaskMaxTurnsFromSubtasks(next, taskForm.maxTurns, maxTurnsLimit)
                      setTaskForm(prev => ({
                        ...prev,
                        subtasks: next.length ? next : undefined,
                        maxTurns: next.length ? newMaxTurns : prev.maxTurns,
                      }))
                    }}
                    onAdd={() => {
                      const currentSubtasks = taskForm.subtasks ?? []
                      if (currentSubtasks.length === 0) {
                        const firstSubtask = blankSubtask(0, taskForm.maxTurns || 25)
                        setTaskForm(prev => ({
                          ...prev,
                          subtasks: [firstSubtask],
                        }))
                      } else if (currentSubtasks.length === 1) {
                        const totalTurns = taskForm.maxTurns || 25
                        const subtask2 = blankSubtask(1, 1)
                        const recalculated = recalculateSubtasksTurns([currentSubtasks[0], subtask2], totalTurns, maxTurnsLimit)
                        setTaskForm(prev => ({
                          ...prev,
                          subtasks: recalculated,
                        }))
                      } else {
                        const avgTurns = Math.max(1, Math.round((taskForm.maxTurns || 25) / (currentSubtasks.length + 1)))
                        const newSubtask = blankSubtask(currentSubtasks.length, avgTurns)
                        const nextSubtasks = [...currentSubtasks, newSubtask]
                        const newMaxTurns = calculateTaskMaxTurnsFromSubtasks(nextSubtasks, taskForm.maxTurns, maxTurnsLimit)
                        setTaskForm(prev => ({
                          ...prev,
                          subtasks: nextSubtasks,
                          maxTurns: newMaxTurns,
                        }))
                      }
                    }}
                  />
                    </div>
                )}
                </div>

                <div className="inline-task-section">
                  <button
                    type="button"
                    className="inline-task-section-header"
                    onClick={() => handleToggleTaskCreatorSection('schedule')}
                    aria-expanded={taskCreatorExpanded.schedule}
                  >
                    <ChevronDown
                      size={14}
                      className={taskCreatorExpanded.schedule ? 'section-chevron expanded' : 'section-chevron'}
                    />
                    <span>{t('harness.page.schedule.heading')}</span>
                  </button>
                  {taskCreatorExpanded.schedule && (
                    <div className="inline-task-section-body">
                      <div className="form-group">
                        <label className="form-label">{t('harness.page.schedule.automatic')}</label>
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
                          <option value="none">{t('harness.page.schedule.manualOption')}</option>
                          <option value="hourly">{t('harness.page.schedule.hourly')}</option>
                          <option value="daily">{t('harness.page.schedule.dailyOption')}</option>
                          <option value="weekdays">{t('harness.page.schedule.weekdaysOption')}</option>
                          <option value="weekly">{t('harness.page.schedule.weeklyOption')}</option>
                        </select>
                        {schedule.kind !== 'none' && schedule.kind !== 'hourly' && (
                          <div className="schedule-fields">
                            {schedule.kind === 'weekly' && (
                              <label className="form-group">
                                <span className="form-label">{t('harness.page.schedule.dayLabel')}</span>
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
                                      {weekdays[index]}
                                    </option>
                                  ))}
                                </select>
                              </label>
                            )}
                            <label className="form-group">
                              <span className="form-label">{t('harness.page.schedule.timeLabel')}</span>
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
                    ? t('harness.page.editor.saving')
                    : t('harness.page.editor.save', { taskId: taskForm.id })}
                </button>
                <button
                  type="button"
                  className="btn-secondary"
                  onClick={() => closeTaskCreator({ discardDraft: true })}
                  disabled={savingTask}
                >
                  {t('harness.page.actions.cancel')}</button>
              </div>
              </div>
            </section>
          )}

          {/* Tasks List */}
          {(() => {
            const emptyState = projectTaskListEmpty({
              taskCount: tasks.length,
              matchCount: filteredTasks.length,
            })
            if (emptyState === 'no-tasks') {
              /* The creator is inline above this slot, so an empty-state CTA would duplicate it. */
              if (isEditingTask) return null
              return (
                <div className="empty-tasks-card">
                  <p className="empty-text">{t('harness.page.tasks.empty')}</p>
                  <button
                    type="button"
                    className="btn-primary"
                    onClick={handleOpenCreateTask}
                  >
                    {t('harness.page.tasks.createFirst')}</button>
                </div>
              )
            }
            if (emptyState === 'no-match') {
              return (
                <div className="empty-tasks-card">
                  <p className="empty-text">{t('harness.page.tasks.noMatch')}</p>
                  <button
                    type="button"
                    className="btn-secondary"
                    onClick={() => {
                      setFilterQuery('')
                      setFilterTag('')
                    }}
                  >
                    {t('harness.page.tasks.clearFilters')}</button>
                </div>
              )
            }
            return (
              <div className="tasks-grid">
                {filteredTasks.map(task => {
                  const runningJob = activeJobs.find(job => job.taskId === task.id)
                  const runningSnapshot =
                    runningJob && inspectingJobId === runningJob.jobId && liveInspect
                      ? activeTrajectoryView
                      : undefined
                  const stepCount = runningSnapshot
                    ? (runningSnapshot.stepCount ?? runningSnapshot.trajectory?.steps?.length)
                    : undefined
                  const runningInfo = runningJob
                    ? {
                        jobId: runningJob.jobId,
                        startedAt: runningJob.startedAt,
                        currentStatus: runningJob.currentStatus,
                        stepCount,
                      }
                    : undefined
                  const latestRun = latestByTaskId.get(task.id)

                  const lastRunPres = presentLastRun({
                    historyLoaded: Boolean(loadedSections.history),
                    historyError: Boolean(historyError),
                    latest: latestRun,
                    running: runningInfo,
                    now,
                  })

                  return (
                    <div key={task.id} className="task-card">
                      <div className="task-card-header">
                        <div>
                          <h4 className="task-card-title">{task.name}</h4>
                          <code className="task-card-id">{task.id}</code>
                        </div>
                        <div className="task-badges">
                          {task.category && <span className="badge category-badge">{task.category}</span>}
                          {task.hasVerifier && (
                            <span className="badge verifier-badge">{t('harness.page.tasks.verifier')}</span>
                          )}
                          {task.cron && (
                            task.schedulePaused ? (
                              <span className="badge schedule-paused-badge">{t('harness.page.tasks.autoRunOff')}</span>
                            ) : (
                              <span className="badge schedule-badge">{t('harness.page.tasks.autoRuns')}</span>
                            )
                          )}
                          {task.dynamicPrompt && (
                            <span className="badge dynamic-prompt-badge">{t('harness.page.tasks.dynamicPrompt')}</span>
                          )}
                        </div>
                      </div>

                      {task.tags.length > 0 && (
                        <div className="task-card-tags">
                          {task.tags.map(tag => (
                            <span key={tag} className="tag-pill">
                              #{tag}
                            </span>
                          ))}
                        </div>
                      )}

                      {task.promptPreview && (
                        <div className="task-card-prompt-preview">{task.promptPreview}</div>
                      )}

                      {(() => {
                        const providers = taskProviderLines(task)
                        if (!providers.primary && !providers.fallback) return null
                        return (
                          <div className="task-card-providers">
                            {providers.primary && <span>{providers.primary}</span>}
                            {providers.fallback && (
                              <span>{t('harness.page.tasks.fallback', { name: providers.fallback })}</span>
                            )}
                          </div>
                        )
                      })()}

                      <div className="task-card-status">
                        {lastRunPres.kind === 'loading' && (
                          <div className="task-card-status-skeleton" aria-label={t('harness.page.tasks.loadingStatus')} />
                        )}
                        {lastRunPres.kind === 'unavailable' && (
                          <div className="task-card-status-error">
                            <span>{t('harness.page.tasks.lastRunError')}</span>
                            <button
                              type="button"
                              className="btn-retry-inline"
                              onClick={() => void handleRetryHistory()}
                            >
                              {t('harness.page.actions.retry')}</button>
                          </div>
                        )}
                        {lastRunPres.kind === 'none' && (
                          <div className="task-card-status-empty">
                            <span>{t('harness.page.tasks.noRuns')}</span>
                          </div>
                        )}
                        {lastRunPres.kind === 'running' && (
                          <div
                            className="task-card-status-running"
                            role="button"
                            tabIndex={0}
                            onClick={() => {
                              const el = document.getElementById(`active-job-${lastRunPres.jobId}`)
                              if (el) {
                                el.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
                                el.focus()
                              }
                            }}
                            onKeyDown={e => {
                              if (e.key === 'Enter' || e.key === ' ') {
                                e.preventDefault()
                                const el = document.getElementById(`active-job-${lastRunPres.jobId}`)
                                if (el) {
                                  el.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
                                  el.focus()
                                }
                              }
                            }}
                            title={t('harness.page.tasks.focusJob')}
                          >
                            <span className="task-status-text">{lastRunPres.text}</span>
                          </div>
                        )}
                        {(lastRunPres.kind === 'success' || lastRunPres.kind === 'terminal') && (
                          <button
                            type="button"
                            className={`task-card-status-btn status-${lastRunPres.kind}`}
                            onClick={() => {
                              const summary = history.find(h => h.id === lastRunPres.runId)
                              if (summary) {
                                void handleSelectHistory(summary, 'tasks')
                              }
                            }}
                            title={t('harness.page.logs.openRun')}
                          >
                            <div className="task-status-line">{lastRunPres.text}</div>
                            {lastRunPres.detail && (
                              <div className="task-status-detail">{lastRunPres.detail}</div>
                            )}
                          </button>
                        )}
                      </div>

                      {task.cron && (
                        <div className="task-card-schedule">
                          <span>{describeLocalizedTaskSchedule(task.cron, task.schedulePaused)}</span>
                          {!task.schedulePaused && task.nextRunAt && (
                            <span>{t('harness.page.tasks.nextRun', { date: formatDateTime(new Date(task.nextRunAt)) })}</span>
                          )}
                        </div>
                      )}

                      {(() => {
                        const latest = latestRunForTask(history, task.id)
                        const canContinue = Boolean(
                          latest &&
                            (latest.continuable || isContinuableStatus(String(latest.status))),
                        )
                        const isTaskBusy =
                          runningTaskId === task.id || activeJobs.some(job => job.taskId === task.id)
                        const isMenuOpen = openMenuTaskId === task.id

                        return (
                          <div className="task-card-actions">
                            <div className="task-actions-group task-actions-primary">
                              {canContinue && latest ? (
                                <>
                                  <button
                                    type="button"
                                    className="btn-action continue-btn continue-btn-primary"
                                    onClick={() =>
                                      handleOpenContinueModal(
                                        task.id,
                                        latest.stepCount || 25,
                                        task.name,
                                        latest.id,
                                        latest.progressSummary ?? undefined,
                                        latest.resumeSubtaskId ?? undefined,
                                        latest.resumeSubtaskTitle ?? undefined,
                                        latest.error ?? undefined,
                                      )
                                    }
                                    disabled={isTaskBusy}
                                    title={t('harness.page.continue.open')}
                                  >
                                    <FastForward size={13} fill="currentColor" />
                                    <span>
                                      {latest.resumeSubtaskTitle
                                        ? t('harness.page.continue.resume', { title: latest.resumeSubtaskTitle })
                                        : t('harness.page.continue.action')}
                                    </span>
                                  </button>
                                  {isHistoryFollowUpStatus(String(latest.status)) && (
                                    <button
                                      type="button"
                                      className="btn-action continue-btn"
                                      onClick={() => void handleSelectHistory(latest, 'tasks')}
                                      disabled={isTaskBusy}
                                      title={t('harness.page.continue.followUpTitle')}
                                    >
                                      <FastForward size={13} />
                                      <span>{t('harness.page.continue.followUp')}</span>
                                    </button>
                                  )}
                                  <button
                                    type="button"
                                    className="btn-action run-btn run-btn-outline"
                                    onClick={() => {
                                      if (task.dynamicPrompt) {
                                        void openDynamicRun(task, false)
                                      } else {
                                        void handleRunTask(task.id, false)
                                      }
                                    }}
                                    disabled={isTaskBusy}
                                    title={
                                      task.dynamicPrompt
                                        ? t('harness.page.run.reviewFresh')
                                        : t('harness.page.run.fresh')
                                    }
                                  >
                                    <Play size={13} />
                                    <span>{t('harness.page.actions.run')}</span>
                                  </button>
                                </>
                              ) : (
                                <>
                                {latest && isHistoryFollowUpStatus(String(latest.status)) && (
                                  <button
                                    type="button"
                                    className="btn-action continue-btn"
                                    onClick={() => void handleSelectHistory(latest, 'tasks')}
                                    disabled={isTaskBusy}
                                    title={t('harness.page.continue.followUpTitle')}
                                  >
                                    <FastForward size={13} />
                                    <span>{t('harness.page.continue.followUp')}</span>
                                  </button>
                                )}
                                <button
                                  type="button"
                                  className="btn-action run-btn run-btn-filled"
                                  onClick={() => {
                                    if (task.dynamicPrompt) {
                                      void openDynamicRun(task, false)
                                    } else {
                                      void handleRunTask(task.id, false)
                                    }
                                  }}
                                  disabled={isTaskBusy}
                                  title={
                                    isTaskBusy
                                      ? t('harness.page.run.busy')
                                      : task.dynamicPrompt
                                        ? t('harness.page.run.reviewExtra')
                                        : t('harness.page.run.title')
                                  }
                                >
                                  {isTaskBusy ? (
                                    <>
                                      <Loader2 size={13} className="spinning" />
                                      <span>{t('harness.page.run.running')}</span>
                                    </>
                                  ) : (
                                    <>
                                      <Play size={13} fill="currentColor" />
                                      <span>{t('harness.page.actions.run')}</span>
                                    </>
                                  )}
                                </button>
                                </>
                              )}
                            </div>

                            <div className="task-actions-group task-actions-secondary">
                              <div className="task-menu-container">
                                <button
                                  type="button"
                                  className={`btn-action task-menu-btn ${isMenuOpen ? 'is-active' : ''}`}
                                  onClick={e => {
                                    e.stopPropagation()
                                    setOpenMenuTaskId(isMenuOpen ? null : task.id)
                                  }}
                                  title={t('harness.page.actions.taskMenu')}
                                  aria-label={t('harness.page.actions.moreTasks')}
                                  aria-expanded={isMenuOpen}
                                >
                                  <MoreHorizontal size={15} />
                                </button>
                                {isMenuOpen && (
                                  <div
                                    className="task-overflow-menu"
                                    role="menu"
                                    onClick={e => e.stopPropagation()}
                                  >
                                    <button
                                      type="button"
                                      className="task-overflow-item"
                                      role="menuitem"
                                      onClick={() => {
                                        setOpenMenuTaskId(null)
                                        if (task.dynamicPrompt) {
                                          void openDynamicRun(task, true)
                                        } else {
                                          void handleRunTask(task.id, true)
                                        }
                                      }}
                                      disabled={isTaskBusy}
                                      title={t('harness.page.record.title')}
                                    >
                                      <CircleDot size={13} />
                                      <span>{t('harness.page.actions.record')}</span>
                                    </button>
                                    <button
                                      type="button"
                                      className="task-overflow-item"
                                      role="menuitem"
                                      onClick={() => {
                                        setOpenMenuTaskId(null)
                                        handleOpenEditTask(task.id)
                                      }}
                                    >
                                      <Pencil size={13} />
                                      <span>{t('harness.page.actions.edit')}</span>
                                    </button>
                                    {task.cron && (
                                      <button
                                        type="button"
                                        className="task-overflow-item"
                                        role="menuitem"
                                        onClick={() => {
                                          setOpenMenuTaskId(null)
                                          void handleToggleSchedulePause(task)
                                        }}
                                      >
                                        <Clock size={13} />
                                        <span>
                                          {task.schedulePaused ? t('harness.page.schedule.resume') : t('harness.page.schedule.pause')}
                                        </span>
                                      </button>
                                    )}
                                    <button
                                      type="button"
                                      className="task-overflow-item"
                                      role="menuitem"
                                      onClick={() => {
                                        setOpenMenuTaskId(null)
                                        setHistoryTaskFilter(task.id)
                                        sectionIntentRef.current = null
                                        setSubTab('history')
                                        onHubNavigate?.({ tab: 'harness', destination: { kind: 'history' } })
                                      }}
                                    >
                                      <History size={13} />
                                      <span>{t('harness.page.actions.allRuns')}</span>
                                    </button>
                                    <div className="task-overflow-divider" />
                                    <button
                                      type="button"
                                      className="task-overflow-item task-overflow-item-danger"
                                      role="menuitem"
                                      onClick={() => {
                                        setOpenMenuTaskId(null)
                                        void handleDeleteTask(task.id, task.name)
                                      }}
                                    >
                                      <Trash2 size={13} />
                                      <span>{t('harness.page.actions.delete')}</span>
                                    </button>
                                  </div>
                                )}
                              </div>
                            </div>
                          </div>
                        )
                      })()}
                </div>
              )
            })}
          </div>
        )
      })()}
        </div>
      )}

      {/* ================= SUBTAB: CONFIG ================= */}
      {subTab === 'config' && (
        sectionLoading === 'config' && !config ? (
          <div className="harness-card project-config-card">
            <h3 className="section-title">{t('harness.page.config.heading')}</h3>
            <div className="tab-skeleton-container mt-3">
              <SkeletonBox height="40px" borderRadius="var(--radius-sm)" />
              <SkeletonBox height="180px" className="mt-3" borderRadius="var(--radius-md)" />
            </div>
          </div>
        ) : config ? (
          <div className="harness-card project-config-card">
            <h3 className="section-title">{t('harness.page.config.heading')}</h3>
          <p className="section-desc">
            <Trans t={t} i18nKey="harness.page.config.storage" values={{ path: '.goose/harness.yaml' }} components={{ path: <code /> }} />
          </p>

          <div className="form-grid">
            <h4 className="sub-section-title">{t('harness.page.config.policy')}</h4>
            <div className="form-row-2">
              <div className="form-group">
                <label className="form-label">{t('harness.page.config.provider')}</label>
                <select
                  className="form-input"
                  value={config.policy?.provider || ''}
                  onChange={e =>
                    setConfig({
                      ...config,
                      policy: { ...(config.policy || {}), provider: e.target.value || undefined },
                    })
                  }
                >
                  <option value="">{t('harness.page.config.useGlobal')}</option>
                  {providerChoices.map(name => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              </div>
              <div className="form-group">
                <label className="form-label">{t('harness.page.config.model')}</label>
                <input
                  type="text"
                  list="harness-major-model-options"
                  className="form-input"
                  value={config.policy?.model || ''}
                  onChange={e =>
                    setConfig({
                      ...config,
                      policy: { ...(config.policy || {}), model: e.target.value || undefined },
                    })
                  }
                  placeholder={t('harness.page.config.modelPlaceholder')}
                />
                <datalist id="harness-major-model-options">
                  {modelChoicesFor(
                    config.policy?.provider || providerCatalog?.globalMajorProvider,
                    config.policy?.model,
                  ).map(model => (
                    <option key={model} value={model} />
                  ))}
                </datalist>
              </div>
            </div>

            <div className="form-row-2">
              <div className="form-group">
                <label className="form-label">{t('harness.page.config.secondaryProvider')}</label>
                <select
                  className="form-input"
                  value={config.policy?.secondaryProvider || ''}
                  onChange={e =>
                    setConfig({
                      ...config,
                      policy: { ...(config.policy || {}), secondaryProvider: e.target.value || undefined },
                    })
                  }
                >
                  <option value="">{t('harness.page.config.noSecondary')}</option>
                  {providerChoices.map(name => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              </div>
              <div className="form-group">
                <label className="form-label">{t('harness.page.config.secondaryModel')}</label>
                <input
                  type="text"
                  list="harness-secondary-model-options"
                  className="form-input"
                  value={config.policy?.secondaryModel || ''}
                  onChange={e =>
                    setConfig({
                      ...config,
                      policy: { ...(config.policy || {}), secondaryModel: e.target.value || undefined },
                    })
                  }
                  placeholder={t('harness.page.config.secondaryModelPlaceholder')}
                />
                <datalist id="harness-secondary-model-options">
                  {modelChoicesFor(
                    config.policy?.secondaryProvider || providerCatalog?.globalSecondaryProvider,
                    config.policy?.secondaryModel,
                  ).map(model => (
                    <option key={model} value={model} />
                  ))}
                </datalist>
              </div>
            </div>

            <div className="form-group">
              <label className="form-label">{t('harness.page.config.systemPrompt')}</label>
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
                placeholder={t('harness.page.config.promptPlaceholder')}
              />
            </div>

            <h4 className="sub-section-title">{t('harness.page.config.environment')}</h4>
            <div className="form-row-2">
              <div className="form-group">
                <label className="form-label">{t('harness.page.config.sandboxKind')}</label>
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
                  <option value="local">{t('harness.page.config.local')}</option>
                  <option value="worktree">{t('harness.page.config.worktree')}</option>
                  <option value="container">{t('harness.page.config.container')}</option>
                </select>
                {config.sandbox?.kind === 'worktree' && (
                  <div style={{ marginTop: 4, fontSize: '0.8rem', color: 'var(--text-muted, #888)' }}>
                    {t('harness.page.config.worktreeHelp')}</div>
                )}
              </div>
              <div className="form-group">
                <label className="form-label">{t('harness.page.config.image')}</label>
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
                  placeholder={t('harness.page.config.imagePlaceholder')}
                  disabled={config.sandbox?.kind !== 'container'}
                />
              </div>
            </div>

            <div className="form-group">
              <label className="form-label">{t('harness.page.config.setup')}</label>
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
                placeholder={t('harness.page.config.setupPlaceholder')}
              />
            </div>

            <h4 className="sub-section-title">{t('harness.page.config.limits')}</h4>
            <div className="form-group">
              <label className="form-label">
                <input
                  type="checkbox"
                  checked={config.execution.autoCommit ?? true}
                  onChange={e => setConfig({
                    ...config,
                    execution: { ...config.execution, autoCommit: e.target.checked },
                  })}
                />{' '}
                {t('harness.page.config.autoCommit')}</label>
              <span className="form-hint">
                {t('harness.page.config.autoCommitHelp')}</span>
            </div>
            <div className="form-row-3">
              <div className="form-group">
                <label className="form-label">{t('harness.page.budget.maxTurns')}</label>
                <input
                  type="number"
                  min={1}
                  max={maxTurnsLimit}
                  className="form-input"
                  value={config.execution?.maxTurns ?? 25}
                  onChange={e =>
                    setConfig({
                      ...config,
                      execution: {
                        ...(config.execution || { timeoutSeconds: 300, concurrency: 4 }),
                        maxTurns: Math.max(1, Math.min(maxTurnsLimit, Number(e.target.value) || 25)),
                      },
                    })
                  }
                />
                <span className="form-hint">
                  {t('harness.page.config.turnsHelp', { limit: maxTurnsLimit })}
                </span>
              </div>
              <div className="form-group">
                <label className="form-label">{t('harness.page.budget.timeout')}</label>
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
                <label className="form-label">{t('harness.page.config.concurrency')}</label>
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
                <label className="form-label">{t('harness.page.config.tasksDir')}</label>
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
                <label className="form-label">{t('harness.page.config.resultsDir')}</label>
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
                <label className="form-label">{t('harness.page.config.cassettesDir')}</label>
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
                {savingConfig ? t('harness.page.config.saving') : t('harness.page.config.save')}
              </button>
            </div>
          </div>
        </div>
      ) : null)}

      {/* ================= SUBTAB: REPORTS ================= */}
      {subTab === 'reports' && (
        <div className="harness-card project-reports-card">
          <h3 className="section-title">{t('harness.page.reports.heading')}</h3>
          <p className="section-desc">
            <Trans t={t} i18nKey="harness.page.reports.storage" values={{ path: config?.paths.resultsDir || '.goose/harness_results' }} components={{ path: <code /> }} />
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
            <p className="empty-text">{t('harness.page.reports.empty')}</p>
          ) : (
            <div className="reports-table-wrap">
              <table className="harness-table">
                <thead>
                  <tr>
                    <th>{t('harness.page.reports.id')}</th>
                    <th>{t('harness.page.reports.suite')}</th>
                    <th>{t('harness.page.reports.passRate')}</th>
                    <th>{t('harness.page.reports.passedTotal')}</th>
                    <th>{t('harness.page.reports.averageDuration')}</th>
                    <th>{t('harness.page.reports.toolCalls')}</th>
                    <th>{t('harness.page.reports.action')}</th>
                  </tr>
                </thead>
                <tbody>
                  {reports.map(r => (
                    <tr key={r.id}>
                      <td>
                        <strong>{r.id}</strong>
                        <div className="sub-timestamp">{formatDateTime(new Date(r.timestamp))}</div>
                      </td>
                      <td>{r.suiteName}</td>
                      <td>
                        <span
                          className={`pass-rate-badge ${r.passRate >= 0.8 ? 'good' : r.passRate >= 0.5 ? 'warn' : 'bad'}`}
                        >
                          {formatNumber((r.passRate * 100), { minimumFractionDigits: 1, maximumFractionDigits: 1, useGrouping: false })}%
                        </span>
                      </td>
                      <td>
                        {r.passedTasks} / {r.totalTasks}
                      </td>
                      <td>{t('harness.page.reports.seconds', { seconds: formatNumber((r.avgDurationMs / 1000), { minimumFractionDigits: 2, maximumFractionDigits: 2, useGrouping: false }) })}</td>
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
                              setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.reportDetail'))
                            }
                          }}
                        >
                          {t('harness.page.reports.detail')}</button>
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
                {t('harness.page.reports.summary', { suite: selectedReportDetail.suiteName, rate: formatNumber((selectedReportDetail.metrics.passRate * 100), { minimumFractionDigits: 1, maximumFractionDigits: 1, useGrouping: false }) })}
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
                        {tr.passed ? t('harness.page.reports.passed') : t('harness.page.reports.failed')}
                      </span>
                    </div>
                    <div className="result-meta">
                      <span>{t('harness.page.reports.steps', { count: tr.stepCount })}</span>
                      <span>{t('harness.page.reports.calls', { count: tr.toolCallsCount })}</span>
                      <span>{t('harness.page.reports.duration', { seconds: formatNumber((tr.durationMs / 1000), { minimumFractionDigits: 2, maximumFractionDigits: 2, useGrouping: false }) })}</span>
                    </div>
                    {tr.verification && (
                      <div className="result-logs">
                        <pre>{tr.verification.stdout || tr.verification.stderr || t('harness.page.reports.noLogs')}</pre>
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
          <h3 className="section-title">{t('harness.page.cassettes.heading')}</h3>
          <p className="section-desc">
            <Trans t={t} i18nKey="harness.page.cassettes.storage" values={{ path: config?.paths.cassettesDir || '.goose/cassettes' }} components={{ path: <code /> }} />
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
            <p className="empty-text">{t('harness.page.cassettes.empty')}</p>
          ) : (
            <div className="cassettes-grid">
              {cassettes.map(c => (
                <div key={c.fileName} className="cassette-card">
                  <div className="cassette-header">
                    <h4>{c.name}</h4>
                    <span className="badge frame-badge">{t('harness.page.cassettes.frames', { count: c.frameCount })}</span>
                  </div>
                  <div className="cassette-meta">
                    <div>{t('harness.page.cassettes.file', { name: c.fileName })}</div>
                    <div>{t('harness.page.cassettes.recorded', { date: formatDateTime(new Date(c.createdAt)) })}</div>
                    {c.problemStatement && (
                      <div className="cassette-prompt">{t('harness.page.cassettes.prompt', { prompt: c.problemStatement })}</div>
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
                <span>{t('harness.page.history.heading')}</span>
                {isHistoryLoading && (
                  <span
                    title={
                      historyRetryAttempt > 0
                        ? t('harness.page.history.retrying', { attempt: historyRetryAttempt })
                        : t('harness.page.history.loading')
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
                    title={t('harness.page.history.retryTitle')}
                    style={{ padding: '2px 8px', fontSize: '0.75rem', display: 'inline-flex', alignItems: 'center', gap: '4px' }}
                  >
                    <RotateCcw size={12} />
                    <span>{t('harness.page.actions.retry')}</span>
                  </button>
                )}
              </h3>
              <p className="section-desc">
                {t('harness.page.history.help')}</p>
            </div>
            {shareUrl && <CopyLinkButton url={shareUrl} />}
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
                <span>{t('harness.page.history.refreshError', { error: renderMessage(historyError) })}</span>
              </div>
              <button
                type="button"
                className="btn-secondary mini-btn"
                onClick={() => void handleRetryHistory()}
                style={{ display: 'inline-flex', alignItems: 'center', gap: '0.35rem' }}
              >
                <RotateCcw size={12} />
                <span>{t('harness.page.actions.retry')}</span>
              </button>
            </div>
          )}

          {isHistoryLoading && history.length === 0 ? (
            <div className="history-loading-spinner-box">
              <Loader2 size={28} className="spinning" />
              <span>
                {historyRetryAttempt > 0
                  ? t('harness.page.history.loadingRetry', { attempt: historyRetryAttempt })
                  : t('harness.page.history.loading')}
              </span>
            </div>
          ) : historyError && history.length === 0 ? (
            <div className="history-loading-spinner-box history-error-box">
              <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                <AlertCircle size={20} className="history-error-icon" />
                <span className="history-error-title">{t('harness.page.history.failed')}</span>
              </div>
              <p style={{ margin: 0, fontSize: '0.875rem', color: 'var(--text-secondary)', maxWidth: '420px' }}>
                {renderMessage(historyError)}
              </p>
              <button
                type="button"
                className="btn-secondary"
                onClick={() => void handleRetryHistory()}
                style={{ display: 'inline-flex', alignItems: 'center', gap: '0.4rem', marginTop: '0.5rem' }}
              >
                <RotateCcw size={14} />
                <span>{t('harness.page.actions.retry')}</span>
              </button>
            </div>
          ) : (
            <HarnessHistoryTable
              items={history}
              selectedId={selectedHistoryId}
              loadingRunId={loadingHistoryId}
              taskNames={taskNamesById}
              tasks={tasks}
              initialTaskId={historyTaskFilter}
              now={now}
              pagination={historyPagination}
              onPageChange={handleHistoryPageChange}
              onPageSizeChange={handleHistoryPageSizeChange}
              isLoading={isHistoryLoading}
              emptyText={t('harness.page.history.empty')}
              onInspect={item => void handleSelectHistory(item, 'history')}
              onContinue={item =>
                handleOpenContinueModal(
                  item.taskId,
                  item.stepCount || 25,
                  historyTaskTitle(item, taskNamesById),
                  item.id,
                  item.progressSummary ?? undefined,
                  item.resumeSubtaskId ?? undefined,
                  item.resumeSubtaskTitle ?? undefined,
                  item.error ?? undefined,
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
                {t('harness.page.logs.loading')}</strong>
              <span style={{ fontSize: '0.875rem', color: 'var(--text-secondary)' }}>
                {t('harness.page.logs.fetching')}</span>
            </div>
          </div>
        ) : activeTrajectoryView ? (
          <TaskTrajectoryViewer
            run={activeTrajectoryView}
            api={api}
            projectSlug={selectedProjectSlug}
            maxTurnsLimit={maxTurnsLimit}
            onNavigateToFiles={onNavigateToFiles}
            previewFilePath={destination?.kind === 'log' ? destination.file : null}
            onPreviewFilePath={path => {
              if (destination?.kind !== 'log') return
              if (path && !isValidProjectFilePath(path)) return
              onHubNavigate?.({
                tab: 'harness',
                destination: { ...destination, file: path },
              })
            }}
            shareUrl={shareUrl}
            linkedLogPage={destination?.kind === 'log' ? destination.logPage : null}
            linkedLogSize={destination?.kind === 'log' ? destination.logSize : null}
            pageLoading={logPageLoading}
            onResumeLiveFollow={() => {
              const dest = destinationRef.current
              if (dest?.kind !== 'log' || dest.logPage == null) return
              suppressPagingFetchRef.current = true
              onHubNavigate?.({
                tab: 'harness',
                destination: {
                  ...dest,
                  logPage: null,
                  logSize: dest.logSize === 50 ? null : dest.logSize,
                },
              })
            }}
            liveInspect={liveInspect}
            inspectingJobId={inspectingJobId}
            selectedHistoryId={selectedHistoryId}
            selectedHistoryFileName={selectedHistoryFileName}
            runningTaskId={runningTaskId}
            stoppingTaskId={stoppingTaskId}
            activeJobs={activeJobs}
            onStopTask={handleStopTask}
            onStopJob={handleStopRunningJob}
            onBackToTasks={handleBackFromTrajectory}
            backLabel={logOrigin ? t('harness.page.logs.back', { origin: ({ tasks: t('harness.page.logs.origin.tasks'), history: t('harness.page.logs.origin.history'), reports: t('harness.page.logs.origin.reports'), config: t('harness.page.logs.origin.config'), cassettes: t('harness.page.logs.origin.cassettes') })[logOrigin] }) : t('harness.page.logs.backTasks')}
            onPageChange={async (page: number, pageSize: number) => {
              if (!selectedProjectSlug) return
              const dest = destinationRef.current
              const fields = logPagingFields(page, pageSize, liveInspect && page === 1)
              if (dest?.kind === 'log' && onHubNavigate) {
                const next: HubDestination = { ...dest, ...fields }
                if (logPagingKey(next) !== logPagingKey(dest)) suppressPagingFetchRef.current = true
                const gen = ++logPagingGenRef.current
                onHubNavigate({ tab: 'harness', destination: next })
                const requested = pageSize > 0 ? { page, limit: pageSize } : null
                try {
                  await applyLogSlice(selectedProjectSlug, next, gen, requested)
                } catch (err) {
                  if (logPagingGenRef.current !== gen) return
                  setError(err instanceof Error ? err.message : pageMessage('harness.page.errors.historicalRun'))
                }
                return
              }
              const paging = pageSize > 0 ? { page, limit: pageSize } : undefined
              if (selectedHistoryFileName) {
                const run = await api.getProjectHistoryDetail(
                  selectedProjectSlug,
                  selectedHistoryFileName,
                  paging,
                )
                setActiveTrajectoryView(run)
              } else if (inspectingJobId) {
                const detail = await inspectProjectHarnessJob(
                  selectedProjectSlug,
                  inspectingJobId,
                  paging,
                )
                if (detail.snapshot) setActiveTrajectoryView(detail.snapshot)
              }
            }}
            onContinueTask={async (taskId, extraTurns, extraPrompt) => {
              await handleRunTask(
                taskId,
                false,
                undefined,
                selectedHistoryId ?? undefined,
                extraTurns,
                extraPrompt,
              )
            }}
            onOpenContinueModal={handleOpenContinueModal}
          />
        ) : logError ? (
          <div className="harness-card">
            <h3 className="section-title">{t('deepLink.logUnavailable')}</h3>
            <p className="section-desc">
              {logError === 'deepLink.logUnavailable' ? t('deepLink.logUnavailableHint') : renderMessage(logError)}
            </p>
            <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
              <button type="button" className="btn-secondary" onClick={() => setLogRetry(count => count + 1)}>
                {t('common.retry')}
              </button>
              <button type="button" className="btn-secondary" onClick={handleBackFromTrajectory}>
                {t('deepLink.backToHistory')}
              </button>
              {shareUrl && <CopyLinkButton url={shareUrl} />}
            </div>
          </div>
        ) : (
          <div className="harness-card">
            <p className="empty-text">{t('harness.page.logs.empty')}</p>
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
                      {t('harness.page.continue.heading')}</h3>
                    <span className="form-hint">
                      {t('harness.page.continue.help')}</span>
                  </div>
                </div>
                <button type="button" className="close-btn" onClick={handleCloseContinueModal}>
                  ✕
                </button>
              </div>

              <div className="continue-modal-info-card">
                <div className="continue-modal-info-row">
                  <span className="info-label">{t('harness.page.continue.taskId')}</span>
                  <strong className="info-value code-font">{continueModalTask.taskId}</strong>
                </div>
                {continueModalTask.taskName && (
                  <div className="continue-modal-info-row">
                    <span className="info-label">{t('harness.page.continue.taskName')}</span>
                    <span className="info-value">{continueModalTask.taskName}</span>
                  </div>
                )}
                <div className="continue-modal-info-row">
                  <span className="info-label">{t('harness.page.continue.previousRun')}</span>
                  <span className="info-value">
                    <span className="status-badge cancelled" style={{ marginRight: 6 }}>
                      {t('harness.page.continue.incomplete')}</span>
                    <Trans t={t} i18nKey="harness.page.continue.stopped" values={{ count: continueModalTask.previousSteps }} components={{ steps: <strong /> }} />
                    {continueModalTask.runId ? (
                      <>
                        {' '}
                        <Trans t={t} i18nKey="harness.page.continue.runId" values={{ runId: continueModalTask.runId }} components={{ id: <code /> }} />
                      </>
                    ) : null}
                  </span>
                </div>
                {continueModalTask.failureReason && (
                  <div className="continue-modal-info-row" style={{ color: '#f87171' }}>
                    <span className="info-label" style={{ color: '#f87171' }}>{t('harness.page.continue.failure')}</span>
                    <span className="info-value" style={{ fontWeight: 500 }}>{continueModalTask.failureReason}</span>
                  </div>
                )}
                {continueModalTask.resumeSubtaskTitle && (
                  <div className="continue-modal-info-row" style={{ background: 'rgba(37, 99, 235, 0.12)', padding: '6px 10px', borderRadius: 6, border: '1px solid rgba(59, 130, 246, 0.3)' }}>
                    <span className="info-label" style={{ color: '#60a5fa' }}>{t('harness.page.continue.resumeSubtask')}</span>
                    <span className="info-value" style={{ fontWeight: 600, color: '#93c5fd' }}>
                      {continueModalTask.resumeSubtaskTitle} {continueModalTask.resumeSubtaskId ? `(${continueModalTask.resumeSubtaskId})` : ''}
                    </span>
                  </div>
                )}
                {continueModalTask.progressSummary && (
                  <div className="continuation-summary-panel">
                    <MarkdownPreview content={continueModalTask.progressSummary} />
                  </div>
                )}
              </div>

              <div className="continue-mode-selector">
                <label className="form-label">{t('harness.page.continue.budgetMode')}</label>
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
                    {t('harness.page.continue.additionalMode')}</label>
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
                    {t('harness.page.continue.totalMode')}</label>
                </div>
              </div>

              <div className="continue-turns-section">
                <label className="form-label">
                  {continueTurnsMode === 'additional'
                    ? t('harness.page.continue.additionalLabel')
                    : t('harness.page.continue.totalLabel')}
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
                          {t('harness.page.continue.presetAdditional', { count: n })}
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
                            {t('harness.page.continue.presetTotal', { total, extra })}
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
                <div className="preview-label">{t('harness.page.continue.calculation')}</div>
                <div className="preview-calc">
                  <span>
                    <Trans t={t} i18nKey="harness.page.continue.previousSteps" values={{ count: continueModalTask.previousSteps }} components={{ steps: <strong /> }} />
                  </span>
                  <span className="calc-arrow">→</span>
                  <span>
                    {t('harness.page.continue.target')}{' '}
                    <strong className="calc-highlight">
                      {continueTurnsMode === 'additional'
                        ? continueModalTask.previousSteps + Math.max(1, continueTurnsInput)
                        : Math.max(1, continueTurnsInput)}
                    </strong>
                  </span>
                  <span className="calc-diff">
                    (
                    {continueTurnsMode === 'additional'
                      ? t('harness.page.continue.addTurns', { turns: Math.max(1, continueTurnsInput) })
                      : t('harness.page.continue.turnsDifference', { sign: Math.max(1, continueTurnsInput) >= continueModalTask.previousSteps
                            ? '+'
                            : '', turns: Math.max(1, continueTurnsInput) - continueModalTask.previousSteps })}
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
                  <span>{t('harness.page.continue.record')}</span>
                </label>
              </div>

              <div className="modal-actions-row">
                <button type="button" className="btn-secondary" onClick={handleCloseContinueModal}>
                  {t('harness.page.actions.cancel')}</button>
                <button
                  type="button"
                  className="btn-primary continue-confirm-btn"
                  onClick={handleConfirmContinueTask}
                  disabled={runningTaskId === continueModalTask.taskId}
                >
                  {runningTaskId === continueModalTask.taskId ? (
                    t('harness.page.continue.launching')
                  ) : continueModalTask.resumeSubtaskTitle ? (
                    t('harness.page.continue.confirmSubtask', { title: continueModalTask.resumeSubtaskTitle, turns: continueTurnsMode === 'additional'
                        ? Math.max(1, continueTurnsInput)
                        : Math.max(1, continueTurnsInput - continueModalTask.previousSteps) })
                  ) : (
                    t('harness.page.continue.confirmTask', { turns: continueTurnsMode === 'additional'
                        ? continueModalTask.previousSteps + Math.max(1, continueTurnsInput)
                        : Math.max(1, continueTurnsInput) })
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
                      {dynamicRun.record ? t('harness.page.dynamic.record') : t('harness.page.dynamic.run')}
                    </h3>
                    <span className="form-hint">
                      {t('harness.page.dynamic.taskHint', { name: dynamicRun.taskName })}
                    </span>
                  </div>
                </div>
                <button type="button" className="close-btn" onClick={() => setDynamicRun(null)}>
                  ✕
                </button>
              </div>

              <label className="form-label" htmlFor="dynamic-prompt-current">
                {t('harness.page.dynamic.current')}</label>
              {dynamicRun.loadingPrompt ? (
                <p className="muted">{t('harness.page.dynamic.loading')}</p>
              ) : dynamicRun.error ? (
                <p className="dynamic-prompt-error">{renderMessage(dynamicRun.error)}</p>
              ) : (
                <pre id="dynamic-prompt-current" className="dynamic-prompt-current">
                  {dynamicRun.prompt}
                </pre>
              )}

              <label className="form-label" htmlFor="dynamic-prompt-extra">
                {t('harness.page.dynamic.additional')}</label>
              <textarea
                id="dynamic-prompt-extra"
                className="form-textarea"
                rows={5}
                autoFocus
                value={dynamicRun.extra}
                placeholder={t('harness.page.dynamic.placeholder')}
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
                  {dynamicRun.record ? t('harness.page.actions.record') : t('harness.page.actions.run')}
                </button>
                <button type="button" className="btn-secondary" onClick={() => setDynamicRun(null)}>
                  {t('harness.page.actions.cancel')}</button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
