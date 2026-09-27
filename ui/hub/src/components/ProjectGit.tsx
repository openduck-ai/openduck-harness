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
          <div className="git-diff-truncated-note">Diff is truncated because it exceeds maximum display size.</div>
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
              <span>Updated Files ({parsedFiles.length})</span>
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
                All files ({parsedFiles.length})
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
        <div className="git-diff-truncated-note">Diff is truncated because it exceeds maximum display size.</div>
      )}
    </div>
  )
}

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
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')

  const load = useCallback(async () => {
    setLoading(true)
    setError('')
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
      setError(cause instanceof Error ? cause.message : 'Unable to load git data')
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
        setError(cause instanceof Error ? cause.message : 'Unable to load diff')
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
        setError(cause instanceof Error ? cause.message : 'Unable to load commit')
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
    setError('')
    setNotice('')
    try {
      await work()
      await load()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Git operation failed')
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
        `Discard local changes in ${targetPaths.length} file${targetPaths.length === 1 ? '' : 's'}? This cannot be undone.`,
      )
    ) {
      return
    }
    void runMutation(() => api.discardGitFiles(slug, targetPaths))
  }

  const generateMessage = async () => {
    setGenerating(true)
    setError('')
    try {
      const result = await api.generateGitCommitMessage(slug, targetPaths)
      setMessage(result.message)
      setMessageSource(result.source)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Unable to generate commit message')
    } finally {
      setGenerating(false)
    }
  }

  const commitChanges = async () => {
    if (!message.trim()) return
    await runMutation(async () => {
      const result = await api.commitGitChanges(slug, message, targetPaths)
      setNotice(`Committed ${result.sha.slice(0, 7)} — ${result.subject}`)
      setMessage('')
      setMessageSource('')
      setSelectedSha(null)
      setSelectedPaths(new Set())
    })
  }

  if (error && !status && !history) {
    return (
      <div className="panel git-panel">
        <p className="error">{error}</p>
        <button type="button" className="secondary mini-btn" onClick={() => void load()}>
          Retry
        </button>
      </div>
    )
  }

  const branchLabel = status?.detached
    ? 'detached HEAD'
    : status?.branch ?? history?.branch ?? 'unknown'
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
            {dirtyCount > 0 ? `${dirtyCount} changed` : 'Clean'}
          </span>
        </div>
        <button
          type="button"
          className="secondary mini-btn"
          onClick={() => void load()}
          disabled={loading || mutating}
        >
          <RefreshCw size={13} />
          Refresh
        </button>
      </div>

      {error && <p className="error">{error}</p>}
      {notice && <p className="git-notice">{notice}</p>}

      <div className="git-body">
        <div className="git-history-pane">
          <button
            type="button"
            className={`git-working-tree ${selectedSha === null ? 'selected' : ''}`}
            onClick={() => setSelectedSha(null)}
          >
            Working tree
            <small>{dirtyCount > 0 ? `${dirtyCount} changes` : 'No local changes'}</small>
          </button>
          {loading && !history ? (
            <p className="muted">Loading history…</p>
          ) : (
            <GitGraph
              commits={history?.commits ?? []}
              selectedSha={selectedSha}
              onSelect={sha => setSelectedSha(sha)}
            />
          )}
          {history?.truncated && (
            <p className="muted git-truncated">Showing the most recent 150 commits.</p>
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
                empty="No diff for this commit."
              />
            </div>
          ) : (
            <>
              <div className="git-changes-header">
                <h3>Changes</h3>
                <div className="git-changes-actions">
                  <button type="button" className="secondary mini-btn" onClick={selectAll} disabled={files.length === 0}>
                    Select all
                  </button>
                  <button type="button" className="secondary mini-btn" onClick={selectNone} disabled={selectedPaths.size === 0}>
                    Clear
                  </button>
                  <button
                    type="button"
                    className="secondary mini-btn"
                    onClick={stageSelected}
                    disabled={mutating || targetPaths.length === 0}
                  >
                    <Plus size={13} />
                    Stage
                  </button>
                  <button
                    type="button"
                    className="secondary mini-btn"
                    onClick={unstageSelected}
                    disabled={mutating || targetPaths.length === 0}
                  >
                    <Minus size={13} />
                    Unstage
                  </button>
                  <button
                    type="button"
                    className="danger mini-btn"
                    onClick={discardSelected}
                    disabled={mutating || targetPaths.length === 0}
                  >
                    <RotateCcw size={13} />
                    Discard
                  </button>
                </div>
              </div>

              {files.length === 0 ? (
                <p className="muted">Working tree is clean.</p>
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
                        {file.staged && <span className="git-staged-tag">staged</span>}
                        {file.untracked && <span className="git-untracked-tag">untracked</span>}
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
                    ? 'No textual diff for this file.'
                    : 'Select a file to view its diff.'
                }
              />

              <div className="git-commit-composer">
                <div className="git-commit-composer-header">
                  <h3>Commit</h3>
                  <button
                    type="button"
                    className="secondary mini-btn"
                    onClick={() => void generateMessage()}
                    disabled={generating || files.length === 0}
                  >
                    <Sparkles size={13} />
                    {generating ? 'Generating…' : 'Generate message'}
                  </button>
                </div>
                <textarea
                  className="git-commit-input"
                  placeholder="Commit message"
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
                      ? 'Generated from the configured model.'
                      : 'Suggested from changed file names.'}
                  </small>
                )}
                <button
                  type="button"
                  className="mini-btn"
                  onClick={() => void commitChanges()}
                  disabled={mutating || !canCommit}
                >
                  <Check size={13} />
                  {mutating ? 'Committing…' : targetPaths.length > 0 ? `Commit ${targetPaths.length} file${targetPaths.length === 1 ? '' : 's'}` : 'Commit staged files'}
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  )
}
