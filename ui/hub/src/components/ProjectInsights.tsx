import { Trans, useTranslation } from 'react-i18next'
import { useState, useEffect } from 'react'
import type { ProjectDetail, ProjectInsight, SessionSummary, JobRun, createApi } from '@aaif/goose-hub-core'
import { TabSkeleton } from './Skeletons'

interface ProjectInsightsProps {
  detail: ProjectDetail
  api?: ReturnType<typeof createApi>
}

export function ProjectInsights({ detail, api }: ProjectInsightsProps) {
  const { t } = useTranslation()
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
        setError(err instanceof Error ? err.message : 'insights.page.error')
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
          {error === 'insights.page.error' ? t('insights.page.error') : error}
        </div>
      )}
      <div className="columns">
        <div className="panel">
          <h3>{t('insights.page.title')}</h3>
          <p className="path-display">{detail.project.path}</p>
          {!insight.exists ? (
            <p className="error">{t('insights.page.unavailable')}</p>
          ) : (
            <>
              <div className="insight-meta">
                <span className="meta-item">
                  {t('insights.page.entries', { count: insight.entryCount })}
                  {insight.truncated ? ` ${t('insights.page.truncated')}` : ''}
                </span>
                {git?.isRepo && (
                  <span className="meta-item git-item">
                    <Trans t={t} i18nKey="insights.page.branch" values={{ branch: git.branch ?? t('insights.page.detached') }} components={{ strong: <strong /> }} />
                    <span className={`git-status-tag ${git.dirty ? 'dirty' : 'clean'}`}>
                      {git.dirty ? t('insights.page.dirty') : t('insights.page.clean')}
                    </span>
                    {typeof git.changed === 'number' && git.changed > 0 && (
                      <span>{t('insights.page.changedFiles', { count: git.changed })}</span>
                    )}
                  </span>
                )}
              </div>

              {git?.lastCommit && (
                <div className="commit-box">
                  <small className="muted">{t('insights.page.latestCommit')}</small>
                  <div className="commit-content">
                    <code>{git.lastCommit.sha}</code> — <span>{git.lastCommit.subject}</span>
                  </div>
                </div>
              )}

              <h4 className="entries-heading">{t('insights.page.structure')}</h4>
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
            <h3>{t('insights.page.sessions')}</h3>
            {sessions.length === 0 ? (
              <p className="muted">{t('insights.page.noSessions')}</p>
            ) : (
              <ul className="entries-list">
                {sessions.map(session => (
                  <li key={session.id} className="entry-item">
                    <span className="badge">{session.sessionType || t('insights.page.chat')}</span>
                    <div className="entry-details">
                      <strong>{session.name || session.id.slice(0, 12)}</strong>
                      <small className="muted">{t('insights.page.sessionId', { id: session.id.slice(0, 8) })}</small>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>

          <div className="panel">
            <h3>{t('insights.page.scheduledRuns')}</h3>
            {jobRuns.length === 0 ? (
              <p className="muted">{t('insights.page.noRuns')}</p>
            ) : (
              <ul className="entries-list">
                {jobRuns.map(run => (
                  <li key={run.id} className="entry-item">
                    <span className={`status ${run.status}`}>{run.status}</span>
                    <div className="entry-details">
                      <strong>{run.jobId}</strong>
                      <small className="muted">{t('insights.page.runId', { id: run.id.slice(0, 8) })}</small>
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
