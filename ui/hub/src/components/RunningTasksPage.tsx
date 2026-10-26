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
  onRefresh: () => void
  onOpenProject: (project: Project) => void
  onStopJob: (job: HarnessActiveJob) => void
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
  onRefresh,
  onOpenProject,
  onStopJob,
}: RunningTasksPageProps) {
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
    [jobs, projectTitles],
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
    [jobs, query, activeProjectFilter, projectTitles],
  )

  const projectCount = new Set(jobs.map(job => job.projectSlug).filter(Boolean)).size
  const projectPhrase =
    projectCount === 0 ? '' : projectCount === 1 ? ' in 1 project' : ` across ${projectCount} projects`
  const summary = jobs.length === 0 ? 'Nothing is running' : `${jobs.length} in progress${projectPhrase}`

  return (
    <div className="running-tasks-page">
      <div className="running-tasks-header">
        <div>
          <p className="eyebrow">Project tasks</p>
          <h2>Running tasks</h2>
          <p className="muted">{summary}. Status refreshes every few seconds.</p>
        </div>
      </div>

      <div className="section-toolbar">
        <div className="toolbar-left">
          <div className="search-box">
            <Search size={16} className="search-icon" />
            <input
              type="text"
              className="search-input"
              placeholder="Search by task, project, model, or current action…"
              value={query}
              onChange={event => setQuery(event.target.value)}
              aria-label="Search running tasks"
            />
            {query && (
              <button
                type="button"
                className="clear-search-btn"
                onClick={() => setQuery('')}
                title="Clear search"
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
                aria-label="Filter by project"
              >
                <option value="">All projects</option>
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
            <span>Refresh</span>
          </button>
        </div>
      </div>

      {error && <div className="error-banner">{error}</div>}

      {!loaded && jobs.length === 0 ? (
        <div className="empty-state-box">
          <div className="empty-state-icon-box">
            <Activity size={26} />
          </div>
          <h3>{isConnected ? 'Loading running tasks…' : 'Server offline'}</h3>
          <p className="muted">
            {isConnected
              ? 'Checking which project tasks are in progress.'
              : 'Running tasks show up here once OpenDuck is connected.'}
          </p>
        </div>
      ) : jobs.length === 0 ? (
        <div className="empty-state-box">
          <div className="empty-state-icon-box">
            <Activity size={26} />
          </div>
          <h3>No tasks are running</h3>
          <p className="muted">
            Project tasks that are queued or in progress appear here with their current step.
          </p>
        </div>
      ) : visibleJobs.length === 0 ? (
        <div className="empty-state-box">
          <h3>No matching tasks</h3>
          <p className="muted">Nothing running matches this search.</p>
        </div>
      ) : (
        <ul className="running-task-list">
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
                      <span>Open project</span>
                    </button>
                  )}
                  <button
                    type="button"
                    className="btn-action stop-btn"
                    onClick={() => onStopJob(job)}
                    disabled={stopping}
                    title={kind === 'queued' ? 'Cancel queued task' : 'Stop this running task'}
                  >
                    <Square size={13} />
                    <span>{stopping ? 'Stopping…' : kind === 'queued' ? 'Cancel' : 'Stop'}</span>
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
