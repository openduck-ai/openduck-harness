import { useState, useEffect } from 'react'
import type { ProjectDetail, ProjectInsight, SessionSummary, JobRun, createApi } from '@aaif/goose-hub-core'
import { TabSkeleton } from './Skeletons'

interface ProjectInsightsProps {
  detail: ProjectDetail
  api?: ReturnType<typeof createApi>
}

export function ProjectInsights({ detail, api }: ProjectInsightsProps) {
  const [loading, setLoading] = useState<boolean>(() => {
    return !detail.insight?.git?.isRepo && (!detail.insight?.entries || detail.insight.entries.length === 0)
  })
  const [insight, setInsight] = useState<ProjectInsight>(detail.insight)
  const [sessions, setSessions] = useState<SessionSummary[]>(detail.recents?.sessions ?? [])
  const [jobRuns, setJobRuns] = useState<JobRun[]>(detail.recents?.jobRuns ?? [])
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (detail.insight?.git?.isRepo || (detail.insight?.entries && detail.insight.entries.length > 0)) {
      setInsight(detail.insight)
      setSessions(detail.recents?.sessions ?? [])
      setJobRuns(detail.recents?.jobRuns ?? [])
      setLoading(false)
      return
    }

    if (!api) {
      setLoading(false)
      return
    }

    let isMounted = true
    setLoading(true)
    setError(null)

    api.getProjectInsight(detail.project.slug)
      .then(res => {
        if (!isMounted) return
        setInsight(res.insight)
        if (res.recents?.sessions) setSessions(res.recents.sessions)
        if (res.recents?.jobRuns) setJobRuns(res.recents.jobRuns)
      })
      .catch(err => {
        if (!isMounted) return
        setError(err instanceof Error ? err.message : 'Failed to load project insights')
      })
      .finally(() => {
        if (isMounted) setLoading(false)
      })

    return () => {
      isMounted = false
    }
  }, [api, detail.project.slug, detail.insight, detail.recents])

  if (loading) {
    return (
      <div className="project-insights-container">
        <TabSkeleton tab="insights" />
      </div>
    )
  }

  const git = insight.git

  return (
    <div className="project-insights-container">
      {error && (
        <div className="alert alert-error mb-4" style={{ padding: '0.75rem 1rem', marginBottom: '1rem' }}>
          {error}
        </div>
      )}
      <div className="columns">
        <div className="panel">
          <h3>Directory & Git Insight</h3>
          <p className="path-display">{detail.project.path}</p>
          {!insight.exists ? (
            <p className="error">Directory is currently unavailable or does not exist.</p>
          ) : (
            <>
              <div className="insight-meta">
                <span className="meta-item">
                  📁 {insight.entryCount} entries
                  {insight.truncated ? ' (showing first 200)' : ''}
                </span>
                {git?.isRepo && (
                  <span className="meta-item git-item">
                    🌿 Branch: <strong>{git.branch ?? 'detached'}</strong>
                    <span className={`git-status-tag ${git.dirty ? 'dirty' : 'clean'}`}>
                      {git.dirty ? '● Uncommitted changes' : '✓ Clean'}
                    </span>
                    {typeof git.changed === 'number' && git.changed > 0 && (
                      <span>({git.changed} changed files)</span>
                    )}
                  </span>
                )}
              </div>

              {git?.lastCommit && (
                <div className="commit-box">
                  <small className="muted">Latest commit:</small>
                  <div className="commit-content">
                    <code>{git.lastCommit.sha}</code> — <span>{git.lastCommit.subject}</span>
                  </div>
                </div>
              )}

              <h4 className="entries-heading">Directory Structure</h4>
              <ul className="entries-list">
                {insight.entries.map(entry => (
                  <li key={entry.name} className="entry-item">
                    <span className="entry-icon">{entry.kind === 'dir' ? '📁' : '📄'}</span>
                    <span className="entry-name">{entry.name}</span>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>

        <div className="panel-column">
          <div className="panel">
            <h3>Recent Agent Sessions</h3>
            {sessions.length === 0 ? (
              <p className="muted">No sessions in this project yet.</p>
            ) : (
              <ul className="entries-list">
                {sessions.map(session => (
                  <li key={session.id} className="entry-item">
                    <span className="badge">{session.sessionType || 'chat'}</span>
                    <div className="entry-details">
                      <strong>{session.name || session.id.slice(0, 12)}</strong>
                      <small className="muted">ID: {session.id.slice(0, 8)}</small>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>

          <div className="panel">
            <h3>Recent Scheduled Job Runs</h3>
            {jobRuns.length === 0 ? (
              <p className="muted">No job runs executed yet.</p>
            ) : (
              <ul className="entries-list">
                {jobRuns.map(run => (
                  <li key={run.id} className="entry-item">
                    <span className={`status ${run.status}`}>{run.status}</span>
                    <div className="entry-details">
                      <strong>{run.jobId}</strong>
                      <small className="muted">Run ID: {run.id.slice(0, 8)}</small>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
