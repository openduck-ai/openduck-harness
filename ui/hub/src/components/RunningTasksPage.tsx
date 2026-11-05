import { useTranslation } from 'react-i18next'
import { useMemo, useState } from 'react'
import type { HarnessActiveJob, Project } from '@aaif/goose-hub-core'
import { Activity, FolderOpen, RefreshCw, Search, Square, X } from 'lucide-react'
import {
  filterRunningTasks,
  runningActivityText,
  runningElapsedLabel,
  runningProjectLabel,
  runningProjectOptions,
  runningStatusKind,
  runningStatusLabel,
  runningStepLabel,
  runningTaskTitle,
} from '../runningTasks.ts'

interface RunningTasksPageProps {
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
  onStopJob: (job: HarnessActiveJob) => void
  onStopChat?: (slug: string) => void
}

export function RunningTasksPage({
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
  onStopJob,
  onStopChat,
}: RunningTasksPageProps) {
  const { t, i18n } = useTranslation()
  const [query, setQuery] = useState('')
  const [projectFilter, setProjectFilter] = useState('')

  const projectTitles = useMemo(() => {
    const titles: Record<string, string> = {}
    for (const project of projects) {
      titles[project.slug] = project.title || project.slug
    }
    return titles
  }, [projects])

  const projectOptions = useMemo(
    () => runningProjectOptions(jobs, projectTitles),
    [jobs, projectTitles, i18n.language],
  )
  const activeProjectFilter = projectOptions.some(option => option.slug === projectFilter)
    ? projectFilter
    : ''

  const visibleJobs = useMemo(
    () =>
      filterRunningTasks(jobs, {
        query,
        projectSlug: activeProjectFilter,
        projectTitles,
      }),
    [jobs, query, activeProjectFilter, projectTitles, i18n.language],
  )

  const activeChats = useMemo(() => {
    if (!allSessionsState) return []
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
  }, [allSessionsState, projects, activeProjectFilter, query, i18n.language])

  const totalCount = jobs.length + activeChats.length
  const projectCount = new Set([
    ...jobs.map(job => job.projectSlug),
    ...activeChats.map(c => c.slug),
  ].filter(Boolean)).size
  const summary = totalCount === 0
    ? t('runningTasks.page.summaryEmpty')
    : projectCount === 0
      ? t('runningTasks.page.summaryNoProjects', { count: totalCount })
      : t(projectCount === 1 ? 'runningTasks.page.summary_one' : 'runningTasks.page.summary_other', { count: totalCount, projects: projectCount })

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
              onChange={event => setQuery(event.target.value)}
              aria-label={t('runningTasks.page.searchLabel')}
            />
            {query && (
              <button
                type="button"
                className="clear-search-btn"
                onClick={() => setQuery('')}
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
                onChange={event => setProjectFilter(event.target.value)}
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
        </div>
        <div className="toolbar-right">
          <button
            type="button"
            className="secondary refresh-btn"
            onClick={onRefresh}
            disabled={refreshing || !isConnected}
          >
            <RefreshCw size={15} className={refreshing ? 'spinning' : ''} />
            <span>{t('runningTasks.page.refresh')}</span>
          </button>
        </div>
      </div>

      {error && <div className="error-banner">{error}</div>}

      {!loaded && jobs.length === 0 ? (
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
      ) : visibleJobs.length === 0 && activeChats.length === 0 ? (
        <div className="empty-state-box">
          <h3>{t('runningTasks.page.noMatches')}</h3>
          <p className="muted">{t('runningTasks.page.noMatchesHint')}</p>
        </div>
      ) : (
        <ul className="running-task-list">
          {activeChats.map(chat => (
            <li key={`chat-${chat.slug}`} className="running-task-card chat-running-card">
              <div className="running-task-main">
                <div className="running-task-title-row">
                  <span className="live-dot" />
                  <span className="running-status is-live">{t('runningTasks.page.chatting')}</span>
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
                    onClick={() => onOpenProject(chat.project!)}
                  >
                    <FolderOpen size={15} />
                    <span>{t('runningTasks.page.openProject')}</span>
                  </button>
                )}
                <button
                  type="button"
                  className="btn-action stop-btn"
                  onClick={() => onStopChat?.(chat.slug)}
                  title={t('runningTasks.page.stopChat')}
                >
                  <Square size={13} />
                  <span>{t('runningTasks.page.stop')}</span>
                </button>
              </div>
            </li>
          ))}
          {visibleJobs.map(job => {
            const project = job.projectSlug
              ? projects.find(item => item.slug === job.projectSlug)
              : undefined
            const kind = runningStatusKind(job.currentStatus)
            const stepLabel = runningStepLabel(job.stepCount)
            const modelLabel = [job.provider, job.model].filter(Boolean).join(' · ')
            const stopping = stoppingJobId === job.jobId
            return (
              <li key={job.jobId} className="running-task-card">
                <div className="running-task-main">
                  <div className="running-task-title-row">
                    <span className={kind === 'queued' ? 'queued-dot' : 'live-dot'} />
                    <span className={`running-status is-${kind}`}>
                      {runningStatusLabel(job.currentStatus)}
                    </span>
                    <h3 className="running-task-title">{runningTaskTitle(job)}</h3>
                  </div>
                  {(job.activity || kind === 'queued') && (
                    <p className="running-task-activity">{runningActivityText(job)}</p>
                  )}
                  <div className="running-task-meta">
                    <span>{runningProjectLabel(job.projectSlug, projectTitles)}</span>
                    <span>{runningElapsedLabel(job.startedAt, now)}</span>
                    {stepLabel && <span>{stepLabel}</span>}
                    {modelLabel && <span>{modelLabel}</span>}
                    {job.jobType && job.jobType !== 'task' && <span>{job.jobType}</span>}
                  </div>
                </div>
                <div className="running-task-actions">
                  {project && (
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => onOpenProject(project)}
                    >
                      <FolderOpen size={15} />
                      <span>{t('runningTasks.page.openProject')}</span>
                    </button>
                  )}
                  <button
                    type="button"
                    className="btn-action stop-btn"
                    onClick={() => onStopJob(job)}
                    disabled={stopping}
                    title={kind === 'queued' ? t('runningTasks.page.cancelQueued') : t('runningTasks.page.stopTask')}
                  >
                    <Square size={13} />
                    <span>{stopping ? t('runningTasks.page.stopping') : kind === 'queued' ? t('runningTasks.page.cancel') : t('runningTasks.page.stop')}</span>
                  </button>
                </div>
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
