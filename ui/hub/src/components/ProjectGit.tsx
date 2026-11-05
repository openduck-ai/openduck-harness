import { useTranslation } from 'react-i18next'
import { useCallback, useEffect, useMemo, useState } from 'react'
import type { createApi, GitFileChange, GitLogResponse, GitShowResponse } from '@aaif/goose-hub-core'
import {
  GitBranch,
  RefreshCw,
  Sparkles,
  RotateCcw,
  Plus,
  Minus,
  Check,
  FileText,
} from 'lucide-react'
import { GitGraph } from './GitGraph'
import { gitDiffLineClass, parseGitDiff, type ParsedDiffFile } from '../gitDiff'

function diffFileBadge(status: ParsedDiffFile['status']): string {
  switch (status) {
    case 'added':
      return 'A'
    case 'deleted':
      return 'D'
    case 'renamed':
      return 'R'
    case 'modified':
    default:
      return 'M'
  }
}

interface GitDiffViewProps {
  diff: string
  empty: string
  truncated?: boolean
  showFileList?: boolean
}

function GitDiffView({
  diff,
  empty,
  truncated = false,
  showFileList = true,
}: GitDiffViewProps) {
  const { t } = useTranslation()
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null)
  const parsedFiles = useMemo(() => parseGitDiff(diff), [diff])

  useEffect(() => {
    if (selectedFilePath && !parsedFiles.some(f => f.path === selectedFilePath)) {
      setSelectedFilePath(null)
    }
  }, [parsedFiles, selectedFilePath])

  if (!diff) {
    return <pre className="git-diff">{empty}</pre>
  }

  if (parsedFiles.length === 0) {
    return (
      <div className="git-diff-container">
        <pre className="git-diff">
          {diff.split('\n').map((line, index) => (
            <span key={index} className={`git-diff-line ${gitDiffLineClass(line)}`}>
              {line || ' '}
            </span>
          ))}
        </pre>
        {truncated && (
          <div className="git-diff-truncated-note">{t('git.page.diffTruncated')}</div>
        )}
      </div>
    )
  }

  const totalAdditions = parsedFiles.reduce((acc, f) => acc + f.additions, 0)
  const totalDeletions = parsedFiles.reduce((acc, f) => acc + f.deletions, 0)
  const shouldShowFileList = showFileList && parsedFiles.length >= 1
  const displayedFiles = selectedFilePath
    ? parsedFiles.filter(f => f.path === selectedFilePath)
    : parsedFiles

  return (
    <div className="git-diff-container">
      {shouldShowFileList && (
        <div className="git-diff-files-panel">
          <div className="git-diff-files-header">
            <div className="git-diff-files-title">
              <FileText size={14} />
              <span>{t('git.page.updatedFiles', { count: parsedFiles.length })}</span>
            </div>
            <div className="git-diff-totals">
              {totalAdditions > 0 && <span className="stat-add">+{totalAdditions}</span>}
              {totalDeletions > 0 && <span className="stat-del">-{totalDeletions}</span>}
            </div>
          </div>

          <div className="git-diff-file-list">
            {parsedFiles.length > 1 && (
              <button
                type="button"
                className={`git-diff-all-files-btn ${selectedFilePath === null ? 'active' : ''}`}
                onClick={() => setSelectedFilePath(null)}
              >
                {t('git.page.allFiles', { count: parsedFiles.length })}
              </button>
            )}
            {parsedFiles.map(file => {
              const isSelected = selectedFilePath === file.path
              return (
                <button
                  key={file.path}
                  type="button"
                  className={`git-diff-file-row ${file.status}${isSelected ? ' selected' : ''}`}
                  onClick={() => setSelectedFilePath(isSelected ? null : file.path)}
                  title={file.path}
                >
                  <span className={`git-file-badge ${file.status}`}>
                    {diffFileBadge(file.status)}
                  </span>
                  <span className="git-diff-file-path">{file.path}</span>
                  <span className="git-diff-file-stats">
                    {file.additions > 0 && <span className="stat-add">+{file.additions}</span>}
                    {file.deletions > 0 && <span className="stat-del">-{file.deletions}</span>}
                  </span>
                </button>
              )
            })}
          </div>
        </div>
      )}

      <div className="git-diff-files-content">
        {displayedFiles.map(file => (
          <div key={file.path} className="git-diff-file-block">
            <div className="git-diff-file-banner">
              <span className={`git-file-badge ${file.status}`}>
                {diffFileBadge(file.status)}
              </span>
              <span className="git-diff-file-banner-path">
                {file.path}
                {file.oldPath ? ` ← ${file.oldPath}` : ''}
              </span>
              <span className="git-diff-file-stats">
                {file.additions > 0 && <span className="stat-add">+{file.additions}</span>}
                {file.deletions > 0 && <span className="stat-del">-{file.deletions}</span>}
              </span>
            </div>
            <pre className="git-diff git-diff-file-body">
              {file.lines.map((line, index) => (
                <span key={index} className={`git-diff-line ${gitDiffLineClass(line)}`}>
                  {line || ' '}
                </span>
              ))}
            </pre>
          </div>
        ))}
      </div>

      {truncated && (
        <div className="git-diff-truncated-note">{t('git.page.diffTruncated')}</div>
      )}
    </div>
  )
}

type GitError = { kind: 'raw'; message: string } | { kind: 'fallback'; key: string }

interface ProjectGitProps {
  api: ReturnType<typeof createApi>
  slug: string
}

function statusBadge(file: GitFileChange): string {
  if (file.conflict) return 'U'
  if (file.untracked) return '?'
  if (file.indexStatus === 'A' || file.worktreeStatus === 'A') return 'A'
  if (file.indexStatus === 'D' || file.worktreeStatus === 'D') return 'D'
  if (file.indexStatus === 'R' || file.worktreeStatus === 'R') return 'R'
  return 'M'
}

function statusClass(file: GitFileChange): string {
  if (file.conflict) return 'conflict'
  if (file.untracked || file.indexStatus === 'A' || file.worktreeStatus === 'A') return 'added'
  if (file.indexStatus === 'D' || file.worktreeStatus === 'D') return 'deleted'
  if (file.indexStatus === 'R' || file.worktreeStatus === 'R') return 'renamed'
  return 'modified'
}

export function ProjectGit({ api, slug }: ProjectGitProps) {
  const { t } = useTranslation()
  const [status, setStatus] = useState<Awaited<ReturnType<typeof api.getGitStatus>> | null>(null)
  const [history, setHistory] = useState<GitLogResponse | null>(null)
  const [selectedSha, setSelectedSha] = useState<string | null>(null)
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set())
  const [activePath, setActivePath] = useState<string | null>(null)
  const [diff, setDiff] = useState('')
  const [diffTruncated, setDiffTruncated] = useState(false)
  const [show, setShow] = useState<GitShowResponse | null>(null)
  const [message, setMessage] = useState('')
  const [messageSource, setMessageSource] = useState('')
  const [loading, setLoading] = useState(false)
  const [mutating, setMutating] = useState(false)
  const [generating, setGenerating] = useState(false)
  const [error, setError] = useState<GitError | null>(null)
  const [notice, setNotice] = useState<{ sha: string; subject: string } | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [nextStatus, nextLog] = await Promise.all([
        api.getGitStatus(slug),
        api.getGitLog(slug, 150),
      ])
      setStatus(nextStatus)
      setHistory(nextLog)
      setSelectedPaths(current => {
        const valid = new Set(nextStatus.files.map(file => file.path))
        return new Set([...current].filter(path => valid.has(path)))
      })
    } catch (cause) {
      setStatus(null)
      setHistory(null)
      setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'git.page.loadFailed' })
    } finally {
      setLoading(false)
    }
  }, [api, slug])

  useEffect(() => {
    void load()
    setSelectedSha(null)
    setActivePath(null)
    setDiff('')
    setShow(null)
    setMessage('')
    setMessageSource('')
  }, [load])

  const files = status?.files ?? []
  const selectedFiles = useMemo(
    () => files.filter(file => selectedPaths.has(file.path)),
    [files, selectedPaths],
  )

  useEffect(() => {
    if (selectedSha) return
    if (activePath && !files.some(file => file.path === activePath)) {
      setActivePath(files[0]?.path ?? null)
    } else if (!activePath && files[0]) {
      setActivePath(files[0].path)
    }
  }, [files, activePath, selectedSha])

  useEffect(() => {
    if (!activePath || selectedSha) return
    const file = files.find(entry => entry.path === activePath)
    if (!file) {
      setDiff('')
      return
    }
    let cancelled = false
    void api
      .getGitDiff(slug, file.path, file.staged && !file.unstaged)
      .then(result => {
        if (cancelled) return
        setDiff(result.diff)
        setDiffTruncated(result.truncated)
      })
      .catch(cause => {
        if (cancelled) return
        setDiff('')
        setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'git.page.diffFailed' })
      })
    return () => {
      cancelled = true
    }
  }, [api, slug, activePath, files, selectedSha])

  useEffect(() => {
    if (!selectedSha) {
      setShow(null)
      return
    }
    let cancelled = false
    void api
      .getGitShow(slug, selectedSha)
      .then(result => {
        if (cancelled) return
        setShow(result)
      })
      .catch(cause => {
        if (cancelled) return
        setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'git.page.commitFailed' })
      })
    return () => {
      cancelled = true
    }
  }, [api, slug, selectedSha])

  const togglePath = (path: string) => {
    setSelectedPaths(current => {
      const next = new Set(current)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
    setActivePath(path)
  }

  const selectAll = () => setSelectedPaths(new Set(files.map(file => file.path)))
  const selectNone = () => setSelectedPaths(new Set())

  const runMutation = async (work: () => Promise<unknown>) => {
    if (mutating) return
    setMutating(true)
    setError(null)
    setNotice(null)
    try {
      await work()
      await load()
    } catch (cause) {
      setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'git.page.operationFailed' })
    } finally {
      setMutating(false)
    }
  }

  const targetPaths = selectedFiles.length > 0 ? selectedFiles.map(file => file.path) : []
  const hasStaged = files.some(file => file.staged)
  const canCommit =
    message.trim().length > 0 && (targetPaths.length > 0 || hasStaged)

  const stageSelected = () => {
    if (targetPaths.length === 0) return
    void runMutation(() => api.stageGitFiles(slug, targetPaths))
  }

  const unstageSelected = () => {
    if (targetPaths.length === 0) return
    void runMutation(() => api.unstageGitFiles(slug, targetPaths))
  }

  const discardSelected = () => {
    if (targetPaths.length === 0) return
    if (
      !confirm(
        t(targetPaths.length === 1 ? 'git.page.discardConfirm_one' : 'git.page.discardConfirm_other', { count: targetPaths.length }),
      )
    ) {
      return
    }
    void runMutation(() => api.discardGitFiles(slug, targetPaths))
  }

  const generateMessage = async () => {
    setGenerating(true)
    setError(null)
    try {
      const result = await api.generateGitCommitMessage(slug, targetPaths)
      setMessage(result.message)
      setMessageSource(result.source)
    } catch (cause) {
      setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'git.page.generateFailed' })
    } finally {
      setGenerating(false)
    }
  }

  const commitChanges = async () => {
    if (!message.trim()) return
    await runMutation(async () => {
      const result = await api.commitGitChanges(slug, message, targetPaths)
      const sha = result.sha.trim()
      const subject = result.subject.trim()
      setNotice({ sha: sha.slice(0, 7), subject })
      setMessage('')
      setMessageSource('')
      setSelectedSha(null)
      setSelectedPaths(new Set())
    })
  }

  if (error && !status && !history) {
    return (
      <div className="panel git-panel">
        <p className="error">{error.kind === 'raw' ? error.message : t(error.key)}</p>
        <button type="button" className="secondary mini-btn" onClick={() => void load()}>
          {t('git.page.retry')}
        </button>
      </div>
    )
  }

  const branchLabel = status?.detached
    ? t('git.page.detachedHead')
    : status?.branch ?? history?.branch ?? t('git.page.unknownBranch')
  const dirtyCount = files.length

  return (
    <div className="git-workspace">
      <div className="git-toolbar">
        <div className="git-toolbar-left">
          <span className="git-branch-chip">
            <GitBranch size={14} />
            <strong>{branchLabel}</strong>
          </span>
          {typeof status?.ahead === 'number' && status.ahead > 0 && (
            <span className="git-sync-chip">↑ {status.ahead}</span>
          )}
          {typeof status?.behind === 'number' && status.behind > 0 && (
            <span className="git-sync-chip behind">↓ {status.behind}</span>
          )}
          <span className={`git-status-tag ${dirtyCount > 0 ? 'dirty' : 'clean'}`}>
            {dirtyCount > 0 ? t('git.page.changedCount', { count: dirtyCount }) : t('git.page.clean')}
          </span>
        </div>
        <button
          type="button"
          className="secondary mini-btn"
          onClick={() => void load()}
          disabled={loading || mutating}
        >
          <RefreshCw size={13} />
          {t('git.page.refresh')}
        </button>
      </div>

      {error && <p className="error">{error.kind === 'raw' ? error.message : t(error.key)}</p>}
      {notice && <p className="git-notice">{notice.sha
        ? t('git.page.committedSha', { sha: notice.sha, subject: notice.subject ? ` — ${notice.subject}` : '' })
        : notice.subject
          ? t('git.page.committedSubject', { subject: notice.subject })
          : t('git.page.committedSuccessfully')}</p>}

      <div className="git-body">
        <div className="git-history-pane">
          <button
            type="button"
            className={`git-working-tree ${selectedSha === null ? 'selected' : ''}`}
            onClick={() => setSelectedSha(null)}
          >
            {t('git.page.workingTree')}
            <small>{dirtyCount > 0 ? t('git.page.changesCount', { count: dirtyCount }) : t('git.page.noLocalChanges')}</small>
          </button>
          {loading && !history ? (
            <p className="muted">{t('git.page.loadingHistory')}</p>
          ) : (
            <GitGraph
              commits={history?.commits ?? []}
              selectedSha={selectedSha}
              onSelect={sha => setSelectedSha(sha)}
            />
          )}
          {history?.truncated && (
            <p className="muted git-truncated">{t('git.page.recentCommits')}</p>
          )}
        </div>

        <div className="git-detail-pane">
          {selectedSha && show ? (
            <div className="git-commit-view">
              <div className="git-commit-header">
                <h3>{show.commit.subject}</h3>
                <div className="git-log-meta">
                  <code>{show.commit.shortSha}</code>
                  <span>{show.commit.authorName}</span>
                  <span>{show.commit.authorEmail}</span>
                </div>
              </div>
              {show.body && <pre className="git-commit-body">{show.body}</pre>}
              <GitDiffView
                diff={show.diff}
                truncated={show.truncated}
                showFileList={true}
                empty={t('git.page.noCommitDiff')}
              />
            </div>
          ) : (
            <>
              <div className="git-changes-header">
                <h3>{t('git.page.changes')}</h3>
                <div className="git-changes-actions">
                  <button type="button" className="secondary mini-btn" onClick={selectAll} disabled={files.length === 0}>
                    {t('git.page.selectAll')}
                  </button>
                  <button type="button" className="secondary mini-btn" onClick={selectNone} disabled={selectedPaths.size === 0}>
                    {t('git.page.clear')}
                  </button>
                  <button
                    type="button"
                    className="secondary mini-btn"
                    onClick={stageSelected}
                    disabled={mutating || targetPaths.length === 0}
                  >
                    <Plus size={13} />
                    {t('git.page.stage')}
                  </button>
                  <button
                    type="button"
                    className="secondary mini-btn"
                    onClick={unstageSelected}
                    disabled={mutating || targetPaths.length === 0}
                  >
                    <Minus size={13} />
                    {t('git.page.unstage')}
                  </button>
                  <button
                    type="button"
                    className="danger mini-btn"
                    onClick={discardSelected}
                    disabled={mutating || targetPaths.length === 0}
                  >
                    <RotateCcw size={13} />
                    {t('git.page.discard')}
                  </button>
                </div>
              </div>

              {files.length === 0 ? (
                <p className="muted">{t('git.page.cleanTree')}</p>
              ) : (
                <ul className="git-file-list">
                  {files.map(file => (
                    <li key={file.path}>
                      <div
                        className={`git-file-row ${statusClass(file)}${
                          activePath === file.path ? ' selected' : ''
                        }`}
                        role="button"
                        tabIndex={0}
                        onClick={() => setActivePath(file.path)}
                        onKeyDown={event => {
                          if (event.key === 'Enter' || event.key === ' ') {
                            event.preventDefault()
                            setActivePath(file.path)
                          }
                        }}
                      >
                        <input
                          type="checkbox"
                          checked={selectedPaths.has(file.path)}
                          onChange={() => togglePath(file.path)}
                          onClick={event => event.stopPropagation()}
                        />
                        <span className={`git-file-badge ${statusClass(file)}`}>{statusBadge(file)}</span>
                        <span className="git-file-path">
                          {file.path}
                          {file.originalPath ? ` ← ${file.originalPath}` : ''}
                        </span>
                        {file.staged && <span className="git-staged-tag">{t('git.page.staged')}</span>}
                        {file.untracked && <span className="git-untracked-tag">{t('git.page.untracked')}</span>}
                      </div>
                    </li>
                  ))}
                </ul>
              )}

              <GitDiffView
                diff={activePath ? diff : ''}
                truncated={diffTruncated}
                showFileList={false}
                empty={
                  activePath
                    ? t('git.page.noFileDiff')
                    : t('git.page.selectFile')
                }
              />

              <div className="git-commit-composer">
                <div className="git-commit-composer-header">
                  <h3>{t('git.page.commit')}</h3>
                  <button
                    type="button"
                    className="secondary mini-btn"
                    onClick={() => void generateMessage()}
                    disabled={generating || files.length === 0}
                  >
                    <Sparkles size={13} />
                    {generating ? t('git.page.generating') : t('git.page.generateMessage')}
                  </button>
                </div>
                <textarea
                  className="git-commit-input"
                  placeholder={t('git.page.commitMessage')}
                  value={message}
                  onChange={event => {
                    setMessage(event.target.value)
                    setMessageSource('')
                  }}
                  rows={4}
                />
                {messageSource && (
                  <small className="muted">
                    {messageSource === 'model'
                      ? t('git.page.generatedFromModel')
                      : t('git.page.suggestedFromFiles')}
                  </small>
                )}
                <button
                  type="button"
                  className="mini-btn"
                  onClick={() => void commitChanges()}
                  disabled={mutating || !canCommit}
                >
                  <Check size={13} />
                  {mutating ? t('git.page.committing') : targetPaths.length > 0 ? t(targetPaths.length === 1 ? 'git.page.commitFiles_one' : 'git.page.commitFiles_other', { count: targetPaths.length }) : t('git.page.commitStaged')}
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  )
}
