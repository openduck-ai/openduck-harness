import { useState } from 'react'
import type { HarnessHistorySummary } from '@aaif/goose-hub-core'
import { Copy, Check, Loader2 } from 'lucide-react'
import { historyTaskTitle } from '../projectTasks.ts'
import { isContinuableStatus } from '../taskContinuation.ts'

interface HarnessHistoryTableProps {
  items: HarnessHistorySummary[]
  selectedId?: string | null
  emptyText: string
  showProject?: boolean
  taskNames?: Record<string, string>
  onInspect: (item: HarnessHistorySummary) => void
  onContinue?: (item: HarnessHistorySummary) => void
  loadingRunId?: string | null
}

export function HarnessHistoryTable({
  items,
  selectedId,
  emptyText,
  showProject = false,
  taskNames,
  onInspect,
  onContinue,
  loadingRunId,
}: HarnessHistoryTableProps) {
  const [copiedId, setCopiedId] = useState<string | null>(null)

  const handleCopyId = (id: string, e: React.MouseEvent) => {
    e.stopPropagation()
    void navigator.clipboard.writeText(id)
    setCopiedId(id)
    setTimeout(() => setCopiedId(null), 2000)
  }

  if (items.length === 0) {
    return <p className="empty-text">{emptyText}</p>
  }

  return (
    <div className="history-table-container">
      <table className="history-table">
        <thead>
          <tr>
            <th>Task</th>
            {showProject && <th>Project</th>}
            <th>Type</th>
            <th>Status</th>
            <th>Steps</th>
            <th>Tools</th>
            <th>Duration</th>
            <th>Started At</th>
            <th>Action</th>
          </tr>
        </thead>
        <tbody>
          {items.map(item => {
            const title = historyTaskTitle(item, taskNames)
            return (
              <tr key={item.id} className={selectedId === item.id ? 'active-row' : ''}>
                <td>
                  <div className="history-task-cell">
                    <div className="history-task-header">
                      <strong
                        className="history-task-name"
                        title={title !== item.taskId ? `Task ID: ${item.taskId}` : undefined}
                      >
                        {title}
                      </strong>
                      {item.recordedCassettePath && (
                        <span className="task-dataset-tag" style={{ marginLeft: 6 }}>
                          cassette
                        </span>
                      )}
                    </div>
                    {title !== item.taskId && (
                      <code className="history-task-id" title={`Task ID: ${item.taskId}`}>
                        {item.taskId}
                      </code>
                    )}
                    <div className="history-run-id-row">
                      <code className="history-run-id" title={`Run ID: ${item.id}`}>
                        {item.id}
                      </code>
                      <button
                        type="button"
                        className="history-copy-btn"
                        onClick={e => handleCopyId(item.id, e)}
                        title={copiedId === item.id ? 'Copied Run ID!' : 'Copy Run ID'}
                        aria-label="Copy Run ID"
                      >
                        {copiedId === item.id ? (
                          <Check size={12} className="text-success" />
                        ) : (
                          <Copy size={12} />
                        )}
                      </button>
                    </div>
                    {item.error && <div className="task-error-text">{item.error}</div>}
                  </div>
                </td>
                {showProject && <td>{item.projectSlug || '—'}</td>}
                <td>
                  <span className="job-type-pill">{item.jobType.toUpperCase()}</span>
                </td>
                <td>
                  <span className={`status-badge ${String(item.status).toLowerCase()}`}>
                    {String(item.status).toUpperCase()}
                  </span>
                </td>
                <td>{item.stepCount}</td>
                <td>{item.toolCallsCount}</td>
                <td>{(item.durationMs / 1000).toFixed(2)}s</td>
                <td>{new Date(item.startedAt).toLocaleString()}</td>
                <td>
                  <div style={{ display: 'flex', gap: '0.4rem', alignItems: 'center' }}>
                    <button
                      type="button"
                      className="btn-small"
                      onClick={() => onInspect(item)}
                      disabled={loadingRunId === item.id}
                      style={{ display: 'inline-flex', alignItems: 'center', gap: '0.35rem' }}
                    >
                      {loadingRunId === item.id ? (
                        <>
                          <Loader2 size={12} className="spinning" />
                          <span>Loading Log…</span>
                        </>
                      ) : (
                        'View Agent Log'
                      )}
                    </button>
                    {onContinue &&
                      (item.continuable || isContinuableStatus(String(item.status))) && (
                        <button
                          type="button"
                          className="btn-small continue-btn"
                          onClick={() => onContinue(item)}
                          title="Start a new run seeded with compacted progress from this run"
                        >
                          ⏩ Continue
                        </button>
                      )}
                  </div>
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}
