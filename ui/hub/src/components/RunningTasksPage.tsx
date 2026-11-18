import { useTranslation } from 'react-i18next'
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type {
  HarnessActiveJob,
  HarnessHistorySummary,
  HarnessRunResponse,
  Project,
} from '@aaif/goose-hub-core'
import {
  Activity,
  Ban,
  Check,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Clock,
  FolderOpen,
  Loader2,
  RefreshCw,
  Search,
  Square,
  X,
} from 'lucide-react'
import { createApi, normalizeHistoryResponse } from '../api.ts'
import { formatDateTime, formatDurationMs } from '../i18n/format.ts'
import {
  filterTaskEntries,
  historyStartedLabel,
  historyStatusLabel,
  mergeAndSortTasks,
  paginateTaskEntries,
  runningActivityText,
  runningElapsedLabel,
  runningProjectLabel,
  runningStatusLabel,
  runningStepLabel,
  taskProjectOptions,
  type TaskListEntry,
  type UnifiedStatusFilter,
} from '../runningTasks.ts'

const LazyTaskTrajectoryViewer = lazy(() =>
  import('./TaskTrajectoryViewer.tsx').then(m => ({ default: m.TaskTrajectoryViewer })),
)

interface RunningTasksPageProps {
  api: ReturnType<typeof createApi>
  jobs: HarnessActiveJob[]
  projects: Project[]
  loaded: boolean
  isConnected: boolean
  error: string
  now: number
  refreshing: boolean
  stoppingJobId: string | null
  allSessionsState?: Record<string, { isPrompting?: boolean; statusLine?: string | null }>
  onRefresh: () => void
  onOpenProject: (project: Project) => void
  onOpenProjectChat?: (project: Project) => void
  onOpenJobLog?: (project: Project, job: HarnessActiveJob) => void
  onOpenHistoryLog?: (project: Project, item: HarnessHistorySummary) => void
  onStopJob: (job: HarnessActiveJob) => void
  onStopChat?: (slug: string) => void
}

type InlineInspectTarget =
  | { kind: 'job'; jobId: string; projectSlug?: string | null }
  | { kind: 'history'; fileName: string; projectSlug?: string | null; id: string }

export function RunningTasksPage({
  api,
  jobs,
  projects,
  loaded,
  isConnected,
  error,
  now,
  refreshing,
  stoppingJobId,
  allSessionsState,
  onRefresh,
  onOpenProject,
  onOpenProjectChat,
  onOpenJobLog,
  onOpenHistoryLog,
  onStopJob,
  onStopChat,
}: RunningTasksPageProps) {
  const { t, i18n } = useTranslation()
  const [query, setQuery] = useState('')
  const [projectFilter, setProjectFilter] = useState('')
  const [statusFilter, setStatusFilter] = useState<UnifiedStatusFilter>('')
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(20)

  const [historyItems, setHistoryItems] = useState<HarnessHistorySummary[]>([])
  const [historyTotal, setHistoryTotal] = useState(0)
  const [historyLoaded, setHistoryLoaded] = useState(false)
  const [historyRefreshing, setHistoryRefreshing] = useState(false)
  const [historyError, setHistoryError] = useState('')
  const historyReqRef = useRef(0)
  const prevJobsCountRef = useRef(jobs.length)

  const [inlineTarget, setInlineTarget] = useState<InlineInspectTarget | null>(null)
  const [inlineRun, setInlineRun] = useState<HarnessRunResponse | null>(null)
  const [inlineLoading, setInlineLoading] = useState(false)
  const [inlineError, setInlineError] = useState('')
  const inlineReqRef = useRef(0)

  const hasClientFilter = Boolean(query.trim() || statusFilter)
  const activeProjectFilter = projects.some(p => p.slug === projectFilter) ? projectFilter : ''
  const isSingleProjectServerPaged = Boolean(activeProjectFilter) && !hasClientFilter

  const loadHistory = useCallback(
    async (silent = false) => {
      if (!isConnected) return
      const reqId = ++historyReqRef.current
      if (!silent) setHistoryRefreshing(true)

      try {
        if (isSingleProjectServerPaged) {
          const res = await api.listProjectHistory(activeProjectFilter, {
            page,
            limit: pageSize,
          })
          if (reqId !== historyReqRef.current) return
          const norm = normalizeHistoryResponse(res)
          const enriched = norm.items.map(item =>
            !item.projectSlug ? { ...item, projectSlug: activeProjectFilter } : item,
          )
          setHistoryItems(enriched)
          setHistoryTotal(norm.pagination?.total ?? enriched.length)
          setHistoryError('')
          return
        }

        const fetchLimit = hasClientFilter ? 100 : page * pageSize
        const requests: Array<{ slug: string | null; promise: Promise<unknown> }> = [
          { slug: null, promise: api.listHarnessHistory({ page: 1, limit: fetchLimit }) },
          ...projects.map(project => ({
            slug: project.slug,
            promise: api.listProjectHistory(project.slug, { page: 1, limit: fetchLimit }),
          })),
        ]

        const results = await Promise.allSettled(requests.map(r => r.promise))
        if (reqId !== historyReqRef.current) return

        const collected: HarnessHistorySummary[] = []
        let totalSum = 0
        let anySuccess = false
        for (let i = 0; i < results.length; i += 1) {
          const result = results[i]
          if (result.status !== 'fulfilled') continue
          anySuccess = true
          const slug = requests[i].slug
          const norm = normalizeHistoryResponse(result.value as Parameters<typeof normalizeHistoryResponse>[0])
          totalSum += norm.pagination?.total ?? norm.items.length
          for (const item of norm.items) {
            collected.push(
              slug && !item.projectSlug ? { ...item, projectSlug: slug } : item,
            )
          }
        }

        if (anySuccess) {
          setHistoryItems(collected)
          setHistoryTotal(Math.max(totalSum, collected.length))
          setHistoryError('')
        } else {
          setHistoryError(t('runningTasks.page.historyLoadError'))
        }
      } catch (cause) {
        if (reqId !== historyReqRef.current) return
        setHistoryError(cause instanceof Error ? cause.message : t('runningTasks.page.historyLoadError'))
      } finally {
        if (reqId === historyReqRef.current) {
          setHistoryLoaded(true)
          setHistoryRefreshing(false)
        }
      }
    },
    [
      api,
      isConnected,
      projects,
      activeProjectFilter,
      isSingleProjectServerPaged,
      hasClientFilter,
      page,
      pageSize,
      t,
    ],
  )

  useEffect(() => {
    if (!isConnected) return
    void loadHistory(false)
  }, [isConnected, loadHistory])

  useEffect(() => {
    const prev = prevJobsCountRef.current
    prevJobsCountRef.current = jobs.length
    if (isConnected && jobs.length < prev) {
      void loadHistory(true)
    }
  }, [isConnected, jobs.length, loadHistory])

  const projectTitles = useMemo(() => {
    const titles: Record<string, string> = {}
    for (const project of projects) {
      titles[project.slug] = project.title || project.slug
    }
    return titles
  }, [projects])

  const allEntries = useMemo(
    () => mergeAndSortTasks(isSingleProjectServerPaged && page > 1 ? [] : jobs, historyItems),
    [jobs, historyItems, isSingleProjectServerPaged, page],
  )

  const projectOptions = useMemo(
    () =>
      taskProjectOptions(
        [
          ...projects.map(p => ({ projectSlug: p.slug })),
          ...allEntries,
        ],
        projectTitles,
      ),
    [projects, allEntries, projectTitles, i18n.language],
  )

  const visibleEntries = useMemo(
    () =>
      filterTaskEntries(allEntries, {
        query,
        projectSlug: activeProjectFilter,
        status: statusFilter,
        projectTitles,
      }),
    [allEntries, query, activeProjectFilter, statusFilter, projectTitles, i18n.language],
  )

  const { items: pagedEntries, pagination } = useMemo(() => {
    if (isSingleProjectServerPaged) {
      const projectJobsCount = jobs.filter(j => (j.projectSlug ?? '') === activeProjectFilter).length
      const total = projectJobsCount + historyTotal
      const totalPages = Math.max(1, Math.ceil(total / pageSize))
      return {
        items: visibleEntries.slice(0, pageSize),
        pagination: {
          page: Math.min(page, totalPages),
          limit: pageSize,
          total,
          totalPages,
        },
      }
    }
    const totalOverride = hasClientFilter
      ? visibleEntries.length
      : jobs.length + historyTotal
    return paginateTaskEntries(visibleEntries, page, pageSize, totalOverride)
  }, [
    isSingleProjectServerPaged,
    jobs,
    activeProjectFilter,
    historyTotal,
    pageSize,
    visibleEntries,
    page,
    hasClientFilter,
  ])

  const activeChats = useMemo(() => {
    if (!allSessionsState) return []
    if (statusFilter && statusFilter !== 'active') return []
    return Object.entries(allSessionsState)
      .filter(([, s]) => s?.isPrompting)
      .map(([slug, s]) => {
        const project = projects.find(item => item.slug === slug)
        return {
          slug,
          project,
          title: project?.title || slug,
          statusLine: s?.statusLine || t('runningTasks.page.generating'),
        }
      })
      .filter(item => {
        if (activeProjectFilter && item.slug !== activeProjectFilter) return false
        if (!query.trim()) return true
        const q = query.toLowerCase()
        return (
          item.slug.toLowerCase().includes(q) ||
          item.title.toLowerCase().includes(q) ||
          item.statusLine.toLowerCase().includes(q)
        )
      })
  }, [allSessionsState, projects, activeProjectFilter, statusFilter, query, i18n.language, t])

  const runningCount = jobs.length + activeChats.length
  const historyCount = historyTotal || allEntries.filter(entry => entry.kind === 'history').length
  const totalCount = runningCount + historyCount
  const projectCount = new Set([
    ...allEntries.map(entry => entry.projectSlug),
    ...activeChats.map(c => c.slug),
  ].filter(Boolean)).size

  const summary = useMemo(() => {
    if (totalCount === 0) return t('runningTasks.page.summaryEmpty')
    if (runningCount > 0 && historyCount > 0) {
      if (projectCount === 0) {
        return t('runningTasks.page.summaryAllNoProjects', {
          running: runningCount,
          history: historyCount,
        })
      }
      return t(
        projectCount === 1 ? 'runningTasks.page.summaryAll_one' : 'runningTasks.page.summaryAll_other',
        { running: runningCount, history: historyCount, projects: projectCount },
      )
    }
    if (runningCount > 0) {
      if (projectCount === 0) {
        return t('runningTasks.page.summaryNoProjects', { count: runningCount })
      }
      return t(
        projectCount === 1 ? 'runningTasks.page.summary_one' : 'runningTasks.page.summary_other',
        { count: runningCount, projects: projectCount },
      )
    }
    if (projectCount === 0) {
      return t('runningTasks.page.summaryHistoryNoProjects', { history: historyCount })
    }
    return t(
      projectCount === 1
        ? 'runningTasks.page.summaryHistoryOnly_one'
        : 'runningTasks.page.summaryHistoryOnly_other',
      { history: historyCount, projects: projectCount },
    )
  }, [totalCount, runningCount, historyCount, projectCount, t])

  const fetchInlineRun = useCallback(
    async (target: InlineInspectTarget, page = 1, pageSize = 50) => {
      const reqId = ++inlineReqRef.current
      setInlineLoading(true)
      setInlineError('')
      const paging = pageSize > 0 ? { page, limit: pageSize } : undefined
      try {
        if (target.kind === 'job') {
          const detail = target.projectSlug
            ? await api.inspectProjectHarnessJob(target.projectSlug, target.jobId, paging)
            : await api.inspectHarnessJob(target.jobId, paging)
          if (reqId !== inlineReqRef.current) return
          if (detail.snapshot) {
            setInlineRun(detail.snapshot)
          } else {
            setInlineError(t('runningTasks.page.logLoadError'))
          }
        } else {
          const run = target.projectSlug
            ? await api.getProjectHistoryDetail(target.projectSlug, target.fileName, paging)
            : await api.getHarnessHistoryDetail(target.fileName, paging)
          if (reqId !== inlineReqRef.current) return
          setInlineRun(run)
        }
      } catch (cause) {
        if (reqId !== inlineReqRef.current) return
        setInlineError(cause instanceof Error ? cause.message : t('runningTasks.page.logLoadError'))
      } finally {
        if (reqId === inlineReqRef.current) {
          setInlineLoading(false)
        }
      }
    },
    [api, t],
  )

  const handleOpenTaskLog = (entry: TaskListEntry) => {
    const project = entry.projectSlug
      ? projects.find(item => item.slug === entry.projectSlug)
      : undefined

    if (entry.kind === 'active') {
      if (project && onOpenJobLog) {
        onOpenJobLog(project, entry.job)
        return
      }
      const target: InlineInspectTarget = {
        kind: 'job',
        jobId: entry.jobId,
        projectSlug: entry.projectSlug,
      }
      setInlineTarget(target)
      setInlineRun(null)
      void fetchInlineRun(target)
      return
    }

    if (project && onOpenHistoryLog) {
      onOpenHistoryLog(project, entry.history)
      return
    }
    const target: InlineInspectTarget = {
      kind: 'history',
      fileName: entry.fileName,
      projectSlug: entry.projectSlug,
      id: entry.id,
    }
    setInlineTarget(target)
    setInlineRun(null)
    void fetchInlineRun(target)
  }

  if (inlineTarget) {
    return (
      <div className="running-tasks-page">
        {inlineError && (
          <div className="error-banner">
            {inlineError}
            <button
              type="button"
              className="secondary"
              style={{ marginLeft: '0.75rem' }}
              onClick={() => {
                setInlineTarget(null)
                setInlineRun(null)
                setInlineError('')
              }}
            >
              {t('runningTasks.page.backToTasks')}
            </button>
          </div>
        )}
        {inlineLoading && !inlineRun ? (
          <div className="empty-state-box">
            <Loader2 size={28} className="spinning" />
            <h3>{t('runningTasks.page.loadingLog')}</h3>
          </div>
        ) : inlineRun ? (
          <Suspense
            fallback={
              <div className="empty-state-box">
                <Loader2 size={28} className="spinning" />
                <h3>{t('runningTasks.page.loadingLog')}</h3>
              </div>
            }
          >
            <LazyTaskTrajectoryViewer
              run={inlineRun}
              api={api}
              projectSlug={inlineTarget.projectSlug ?? undefined}
              liveInspect={inlineTarget.kind === 'job'}
              inspectingJobId={inlineTarget.kind === 'job' ? inlineTarget.jobId : null}
              selectedHistoryId={inlineTarget.kind === 'history' ? inlineTarget.id : null}
              selectedHistoryFileName={inlineTarget.kind === 'history' ? inlineTarget.fileName : null}
              activeJobs={jobs}
              onBackToTasks={() => {
                setInlineTarget(null)
                setInlineRun(null)
                setInlineError('')
              }}
              backLabel={t('runningTasks.page.backToTasks')}
              onPageChange={async (page, pageSize) => {
                await fetchInlineRun(inlineTarget, page, pageSize)
              }}
            />
          </Suspense>
        ) : null}
      </div>
    )
  }

  const isLoadingInitial = (!loaded || !historyLoaded) && totalCount === 0
  const isRefreshing = refreshing || historyRefreshing
  const combinedError = error || historyError

  return (
    <div className="running-tasks-page">
      <div className="running-tasks-header">
        <div>
          <p className="eyebrow">{t('runningTasks.page.eyebrow')}</p>
          <h2>{t('runningTasks.page.title')}</h2>
          <p className="muted">{t('runningTasks.page.statusRefresh', { summary })}</p>
        </div>
      </div>

      <div className="section-toolbar">
        <div className="toolbar-left">
          <div className="search-box">
            <Search size={16} className="search-icon" />
            <input
              type="text"
              className="search-input"
              placeholder={t('runningTasks.page.searchPlaceholder')}
              value={query}
              onChange={event => {
                setQuery(event.target.value)
                setPage(1)
              }}
              aria-label={t('runningTasks.page.searchLabel')}
            />
            {query && (
              <button
                type="button"
                className="clear-search-btn"
                onClick={() => {
                  setQuery('')
                  setPage(1)
                }}
                title={t('runningTasks.page.clearSearch')}
              >
                <X size={14} />
              </button>
            )}
          </div>
          {projectOptions.length > 1 && (
            <div className="filter-group">
              <select
                className="filter-select"
                value={activeProjectFilter}
                onChange={event => {
                  setProjectFilter(event.target.value)
                  setPage(1)
                }}
                aria-label={t('runningTasks.page.filterProject')}
              >
                <option value="">{t('runningTasks.page.allProjects')}</option>
                {projectOptions.map(option => (
                  <option key={option.slug} value={option.slug}>
                    {option.label}
                  </option>
                ))}
              </select>
            </div>
          )}
          <div className="filter-group">
            <select
              className="filter-select"
              value={statusFilter}
              onChange={event => {
                setStatusFilter(event.target.value as UnifiedStatusFilter)
                setPage(1)
              }}
              aria-label={t('runningTasks.page.filterStatus')}
            >
              <option value="">{t('runningTasks.page.allStatuses')}</option>
              <option value="active">{t('runningTasks.page.statusActive')}</option>
              <option value="success">{t('harness.history.success')}</option>
              <option value="failure">{t('harness.history.failed')}</option>
              <option value="timeout">{t('harness.history.timedOut')}</option>
              <option value="cancelled">{t('harness.history.cancelled')}</option>
              <option value="skipped">{t('harness.history.skipped')}</option>
            </select>
          </div>
        </div>
        <div className="toolbar-right">
          <button
            type="button"
            className="secondary refresh-btn"
            onClick={() => {
              onRefresh()
              void loadHistory(false)
            }}
            disabled={isRefreshing || !isConnected}
          >
            <RefreshCw size={15} className={isRefreshing ? 'spinning' : ''} />
            <span>{t('runningTasks.page.refresh')}</span>
          </button>
        </div>
      </div>

      {combinedError && <div className="error-banner">{combinedError}</div>}

      {isLoadingInitial ? (
        <div className="empty-state-box">
          <div className="empty-state-icon-box">
            <Activity size={26} />
          </div>
          <h3>{isConnected ? t('runningTasks.page.loading') : t('runningTasks.page.offline')}</h3>
          <p className="muted">
            {isConnected
              ? t('runningTasks.page.checking')
              : t('runningTasks.page.connectHint')}
          </p>
        </div>
      ) : totalCount === 0 ? (
        <div className="empty-state-box">
          <div className="empty-state-icon-box">
            <Activity size={26} />
          </div>
          <h3>{t('runningTasks.page.empty')}</h3>
          <p className="muted">
            {t('runningTasks.page.emptyHint')}
          </p>
        </div>
      ) : visibleEntries.length === 0 && activeChats.length === 0 ? (
        <div className="empty-state-box">
          <h3>{t('runningTasks.page.noMatches')}</h3>
          <p className="muted">{t('runningTasks.page.noMatchesHint')}</p>
        </div>
      ) : (
        <ul className="running-task-list">
          {page === 1 && activeChats.map(chat => (
            <li
              key={`chat-${chat.slug}`}
              className={`running-task-card chat-running-card ${chat.project ? 'is-clickable' : ''}`}
              role={chat.project ? 'button' : undefined}
              tabIndex={chat.project ? 0 : undefined}
              onClick={() => {
                if (!chat.project) return
                if (onOpenProjectChat) {
                  onOpenProjectChat(chat.project)
                } else {
                  onOpenProject(chat.project)
                }
              }}
              onKeyDown={event => {
                if (!chat.project) return
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault()
                  if (onOpenProjectChat) {
                    onOpenProjectChat(chat.project)
                  } else {
                    onOpenProject(chat.project)
                  }
                }
              }}
            >
              <div className="running-task-main">
                <div className="running-task-title-row">
                  <span className="live-dot" />
                  <span className="running-status is-running">{t('runningTasks.page.chatting')}</span>
                  <h3 className="running-task-title">{t('runningTasks.page.projectChat', { title: chat.title })}</h3>
                </div>
                <p className="running-task-activity">{chat.statusLine}</p>
                <div className="running-task-meta">
                  <span>{chat.title}</span>
                  <span>{t('runningTasks.page.interactiveSession')}</span>
                </div>
              </div>
              <div className="running-task-actions">
                {chat.project && (
                  <button
                    type="button"
                    className="secondary"
                    onClick={event => {
                      event.stopPropagation()
                      onOpenProject(chat.project!)
                    }}
                  >
                    <FolderOpen size={15} />
                    <span>{t('runningTasks.page.openProject')}</span>
                  </button>
                )}
                <button
                  type="button"
                  className="btn-action stop-btn"
                  onClick={event => {
                    event.stopPropagation()
                    onStopChat?.(chat.slug)
                  }}
                  title={t('runningTasks.page.stopChat')}
                >
                  <Square size={13} />
                  <span>{t('runningTasks.page.stop')}</span>
                </button>
              </div>
            </li>
          ))}

          {pagedEntries.map(entry => {
            const project = entry.projectSlug
              ? projects.find(item => item.slug === entry.projectSlug)
              : undefined
            const stepLabel = runningStepLabel(entry.stepCount)
            const modelLabel = [entry.provider, entry.model].filter(Boolean).join(' · ')
            const startedExact = formatDateTime(entry.startedAt)

            if (entry.kind === 'active') {
              const stopping = stoppingJobId === entry.jobId
              return (
                <li
                  key={entry.key}
                  className="running-task-card is-clickable"
                  role="button"
                  tabIndex={0}
                  onClick={() => handleOpenTaskLog(entry)}
                  onKeyDown={event => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault()
                      handleOpenTaskLog(entry)
                    }
                  }}
                >
                  <div className="running-task-main">
                    <div className="running-task-title-row">
                      <span className={entry.statusKind === 'queued' ? 'queued-dot' : 'live-dot'} />
                      <span className={`running-status is-${entry.statusKind}`}>
                        {runningStatusLabel(entry.status)}
                      </span>
                      <h3 className="running-task-title">{entry.title}</h3>
                    </div>
                    {(entry.activity || entry.statusKind === 'queued') && (
                      <p className="running-task-activity">{runningActivityText(entry.job)}</p>
                    )}
                    <div className="running-task-meta">
                      <span>{runningProjectLabel(entry.projectSlug, projectTitles)}</span>
                      <span title={startedExact}>{runningElapsedLabel(entry.startedAt, now)}</span>
                      {stepLabel && <span>{stepLabel}</span>}
                      {modelLabel && <span>{modelLabel}</span>}
                      {entry.jobType && entry.jobType !== 'task' && <span>{entry.jobType}</span>}
                    </div>
                  </div>
                  <div className="running-task-actions">
                    {project && (
                      <button
                        type="button"
                        className="secondary"
                        onClick={event => {
                          event.stopPropagation()
                          onOpenProject(project)
                        }}
                      >
                        <FolderOpen size={15} />
                        <span>{t('runningTasks.page.openProject')}</span>
                      </button>
                    )}
                    <button
                      type="button"
                      className="btn-action stop-btn"
                      onClick={event => {
                        event.stopPropagation()
                        onStopJob(entry.job)
                      }}
                      disabled={stopping}
                      title={
                        entry.statusKind === 'queued'
                          ? t('runningTasks.page.cancelQueued')
                          : t('runningTasks.page.stopTask')
                      }
                    >
                      <Square size={13} />
                      <span>
                        {stopping
                          ? t('runningTasks.page.stopping')
                          : entry.statusKind === 'queued'
                            ? t('runningTasks.page.cancel')
                            : t('runningTasks.page.stop')}
                      </span>
                    </button>
                    <span className="running-task-log-hint" aria-hidden="true">
                      <ChevronRight size={16} />
                    </span>
                  </div>
                </li>
              )
            }

            const detailLine = entry.error?.trim() || entry.progressSummary?.trim() || ''
            return (
              <li
                key={entry.key}
                className="running-task-card is-historical is-clickable"
                role="button"
                tabIndex={0}
                onClick={() => handleOpenTaskLog(entry)}
                onKeyDown={event => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault()
                    handleOpenTaskLog(entry)
                  }
                }}
              >
                <div className="running-task-main">
                  <div className="running-task-title-row">
                    <span className={`history-status-icon is-${entry.statusKind}`} aria-hidden="true">
                      {entry.statusKind === 'success' ? (
                        <Check size={12} />
                      ) : entry.statusKind === 'failure' ? (
                        <X size={12} />
                      ) : entry.statusKind === 'timeout' ? (
                        <Clock size={12} />
                      ) : (
                        <Ban size={12} />
                      )}
                    </span>
                    <span className={`running-status is-${entry.statusKind}`}>
                      {historyStatusLabel(entry.status)}
                    </span>
                    <h3 className="running-task-title">{entry.title}</h3>
                  </div>
                  {detailLine && (
                    <p className="running-task-activity is-muted">{detailLine}</p>
                  )}
                  <div className="running-task-meta">
                    <span>{runningProjectLabel(entry.projectSlug, projectTitles)}</span>
                    <span title={startedExact}>{historyStartedLabel(entry.startedAt, now)}</span>
                    <span>{formatDurationMs(entry.durationMs)}</span>
                    {stepLabel && <span>{stepLabel}</span>}
                    {modelLabel && <span>{modelLabel}</span>}
                    {entry.jobType && entry.jobType !== 'task' && <span>{entry.jobType}</span>}
                  </div>
                </div>
                <div className="running-task-actions">
                  {project && (
                    <button
                      type="button"
                      className="secondary"
                      onClick={event => {
                        event.stopPropagation()
                        onOpenProject(project)
                      }}
                    >
                      <FolderOpen size={15} />
                      <span>{t('runningTasks.page.openProject')}</span>
                    </button>
                  )}
                  <span className="running-task-log-hint">
                    <span>{t('runningTasks.page.viewLog')}</span>
                    <ChevronRight size={15} />
                  </span>
                </div>
              </li>
            )
          })}
        </ul>
      )}

      {pagination.total > 0 && (
        <div
          className="history-pagination-bar"
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '0.75rem 0.25rem 0.25rem',
            marginTop: '0.5rem',
            borderTop: '1px solid var(--border-subtle)',
            flexWrap: 'wrap',
            gap: '0.75rem',
          }}
        >
          <div style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>
            {t('harness.history.showing', {
              start: (pagination.page - 1) * pagination.limit + 1,
              end: Math.min(pagination.page * pagination.limit, pagination.total),
              total: pagination.total,
            })}
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', fontSize: '0.85rem' }}>
              <span style={{ color: 'var(--text-secondary)' }}>{t('harness.history.pageSize')}</span>
              <select
                className="form-input form-select-sm"
                value={pageSize}
                onChange={event => {
                  setPageSize(Number(event.target.value))
                  setPage(1)
                }}
                style={{ width: 'auto', padding: '2px 6px' }}
              >
                <option value="10">10</option>
                <option value="20">20</option>
                <option value="50">50</option>
                <option value="100">100</option>
              </select>
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: '0.35rem' }}>
              <button
                type="button"
                className="btn-secondary mini-btn"
                disabled={pagination.page <= 1 || isRefreshing}
                onClick={() => setPage(1)}
                title={t('harness.history.firstPage')}
                style={{ padding: '3px 6px', display: 'inline-flex', alignItems: 'center' }}
              >
                <ChevronsLeft size={14} />
              </button>
              <button
                type="button"
                className="btn-secondary mini-btn"
                disabled={pagination.page <= 1 || isRefreshing}
                onClick={() => setPage(p => Math.max(1, p - 1))}
                title={t('harness.history.previousPage')}
                style={{ padding: '3px 6px', display: 'inline-flex', alignItems: 'center' }}
              >
                <ChevronLeft size={14} />
              </button>
              <span style={{ fontSize: '0.85rem', padding: '0 4px', color: 'var(--text-primary)' }}>
                {t('harness.history.page', {
                  page: pagination.page,
                  total: Math.max(1, pagination.totalPages),
                })}
              </span>
              <button
                type="button"
                className="btn-secondary mini-btn"
                disabled={pagination.page >= pagination.totalPages || isRefreshing}
                onClick={() => setPage(p => Math.min(pagination.totalPages, p + 1))}
                title={t('harness.history.nextPage')}
                style={{ padding: '3px 6px', display: 'inline-flex', alignItems: 'center' }}
              >
                <ChevronRight size={14} />
              </button>
              <button
                type="button"
                className="btn-secondary mini-btn"
                disabled={pagination.page >= pagination.totalPages || isRefreshing}
                onClick={() => setPage(pagination.totalPages)}
                title={t('harness.history.lastPage')}
                style={{ padding: '3px 6px', display: 'inline-flex', alignItems: 'center' }}
              >
                <ChevronsRight size={14} />
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
