import { useState, useMemo, useEffect } from 'react'
import type { HarnessHistorySummary } from '@aaif/goose-hub-core'
import { Check, X, FastForward, Loader2 } from 'lucide-react'
import {
  filterHistoryItems,
  historyTaskTitle,
  presentHistoryRow,
} from '../projectTasks.ts'
import { isContinuableStatus } from '../taskContinuation.ts'

interface HarnessHistoryTableProps {
  items: HarnessHistorySummary[]
  selectedId?: string | null
  emptyText: string
  showProject?: boolean
  taskNames?: Record<string, string>
  tasks?: Array<{ id: string; name: string }>
  initialTaskId?: string
  now?: number
  onInspect: (item: HarnessHistorySummary) => void
  onContinue?: (item: HarnessHistorySummary) => void
  loadingRunId?: string | null
}

const STATUS_OPTIONS = [
  { label: 'All', value: '' },
  { label: 'Success', value: 'success' },
  { label: 'Failed', value: 'failure' },
  { label: 'Timed out', value: 'timeout' },
  { label: 'Cancelled', value: 'cancelled' },
  { label: 'Skipped', value: 'skipped' },
]

export function HarnessHistoryTable({
  items,
  selectedId,
  emptyText,
  showProject = false,
  taskNames,
  tasks = [],
  initialTaskId = '',
  now,
  onInspect,
  onContinue,
  loadingRunId,
}: HarnessHistoryTableProps) {
  const [statusFilter, setStatusFilter] = useState('')
  const [taskFilter, setTaskFilter] = useState(initialTaskId)
  const [jobTypeFilter, setJobTypeFilter] = useState('')

  useEffect(() => {
    if (initialTaskId !== undefined) {
      setTaskFilter(initialTaskId)
    }
  }, [initialTaskId])

  const allTaskIds = useMemo(() => {
    const idSet = new Set<string>()
    for (const t of tasks) {
      idSet.add(t.id)
    }
    for (const item of items) {
      idSet.add(item.taskId)
    }
    return Array.from(idSet).sort()
  }, [tasks, items])

  const jobTypes = useMemo(() => {
    const types = new Set<string>()
    for (const item of items) {
      if (item.jobType) {
        types.add(item.jobType)
      }
    }
    return Array.from(types).sort()
  }, [items])

  const filteredItems = useMemo(
    () =>
      filterHistoryItems(items, {
        status: statusFilter,
        taskId: taskFilter,
        jobType: jobTypeFilter,
      }),
    [items, statusFilter, taskFilter, jobTypeFilter],
  )

  const hasActiveFilters = Boolean(statusFilter || taskFilter || jobTypeFilter)

  if (items.length === 0) {
    return <p className="empty-text">{emptyText}</p>
  }

  return (
    <div className="history-list-container">
      <div className="history-filters-bar">
        <div className="history-filter-group">
          <label className="history-filter-label">Status:</label>
          <select
            className="form-input form-select-sm history-filter-select"
            value={statusFilter}
            onChange={e => setStatusFilter(e.target.value)}
            aria-label="Filter by status"
          >
            {STATUS_OPTIONS.map(opt => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </div>

        <div className="history-filter-group">
          <label className="history-filter-label">Task:</label>
          <select
            className="form-input form-select-sm history-filter-select"
            value={taskFilter}
            onChange={e => setTaskFilter(e.target.value)}
            aria-label="Filter by task"
          >
            <option value="">All Tasks</option>
            {allTaskIds.map(id => (
              <option key={id} value={id}>
                {historyTaskTitle({ taskId: id }, taskNames)}
              </option>
            ))}
          </select>
        </div>

        {jobTypes.length > 1 && (
          <div className="history-filter-group">
            <label className="history-filter-label">Type:</label>
            <select
              className="form-input form-select-sm history-filter-select"
              value={jobTypeFilter}
              onChange={e => setJobTypeFilter(e.target.value)}
              aria-label="Filter by job type"
            >
              <option value="">All Types</option>
              {jobTypes.map(jt => (
                <option key={jt} value={jt}>
                  {jt}
                </option>
              ))}
            </select>
          </div>
        )}

        {hasActiveFilters && (
          <button
            type="button"
            className="btn-link history-clear-filters-btn"
            onClick={() => {
              setStatusFilter('')
              setTaskFilter('')
              setJobTypeFilter('')
            }}
          >
            Clear filters
          </button>
        )}
      </div>

      {filteredItems.length === 0 ? (
        <div className="empty-history-filter">
          <p className="empty-text">No runs match your filter criteria.</p>
          <button
            type="button"
            className="btn-secondary"
            onClick={() => {
              setStatusFilter('')
              setTaskFilter('')
              setJobTypeFilter('')
            }}
          >
            Clear filters
          </button>
        </div>
      ) : (
        <div className="history-list">
          {filteredItems.map(item => {
            const row = presentHistoryRow(item, taskNames, now ?? Date.now())
            const canContinue = Boolean(
              onContinue &&
                (item.continuable || isContinuableStatus(String(item.status))),
            )

            return (
              <div
                key={item.id}
                className={`history-list-row ${selectedId === item.id ? 'active-row' : ''}`}
                role="button"
                tabIndex={0}
                onClick={() => onInspect(item)}
                onKeyDown={e => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault()
                    onInspect(item)
                  }
                }}
                title={`Run ID: ${item.id}`}
              >
                <div className="history-row-main">
                  <div className="history-row-header">
                    <span className={`history-status-icon status-${row.status}`}>
                      {loadingRunId === item.id ? (
                        <Loader2 size={14} className="spinning" />
                      ) : row.status === 'success' ? (
                        <Check size={14} />
                      ) : row.status === 'failure' ? (
                        <X size={14} />
                      ) : row.status === 'skipped' ? (
                        <span className="skip-icon">○</span>
                      ) : (
                        <span className="dot-icon">●</span>
                      )}
                    </span>
                    <span className="history-row-title">{row.title}</span>
                    {showProject && item.projectSlug && (
                      <span className="history-row-project">{item.projectSlug}</span>
                    )}
                    <div className="history-row-metrics">
                      <span className="history-row-relative">{row.relative}</span>
                      <span className="history-row-duration">{row.duration}</span>
                      <span className="history-row-steps">{row.steps} steps</span>
                    </div>
                    {canContinue && (
                      <button
                        type="button"
                        className="btn-action history-continue-btn"
                        onClick={e => {
                          e.stopPropagation()
                          onContinue?.(item)
                        }}
                        title="Continue this run"
                      >
                        <FastForward size={13} fill="currentColor" />
                        <span>Continue</span>
                      </button>
                    )}
                  </div>
                  {row.detail && (
                    <div className="history-row-detail">{row.detail}</div>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
