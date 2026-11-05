import { useTranslation } from 'react-i18next'
import { useState, useMemo, useEffect } from 'react'
import type { HarnessHistorySummary, HarnessPagination } from '@aaif/goose-hub-core'
import {
  Check,
  X,
  FastForward,
  Loader2,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
} from 'lucide-react'
import {
  filterHistoryItems,
  historyTaskTitle,
  presentHistoryRow,
} from '../projectTasks.ts'
import { isContinuableStatus, isHistoryFollowUpStatus } from '../taskContinuation.ts'

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
  pagination?: HarnessPagination | null
  onPageChange?: (page: number) => void
  onPageSizeChange?: (pageSize: number) => void
  isLoading?: boolean
}

function getStatusOptions(t: ReturnType<typeof useTranslation>['t']) {
  return [
    { label: t('harness.history.all'), value: '' },
    { label: t('harness.history.success'), value: 'success' },
    { label: t('harness.history.failed'), value: 'failure' },
    { label: t('harness.history.timedOut'), value: 'timeout' },
    { label: t('harness.history.cancelled'), value: 'cancelled' },
    { label: t('harness.history.skipped'), value: 'skipped' },
  ]
}

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
  pagination,
  onPageChange,
  onPageSizeChange,
  isLoading = false,
}: HarnessHistoryTableProps) {
  const { t } = useTranslation()
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
          <label className="history-filter-label">{t('harness.history.status')}</label>
          <select
            className="form-input form-select-sm history-filter-select"
            value={statusFilter}
            onChange={e => setStatusFilter(e.target.value)}
            aria-label={t('harness.history.filterStatus')}
          >
            {getStatusOptions(t).map(opt => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </div>

        <div className="history-filter-group">
          <label className="history-filter-label">{t('harness.history.task')}</label>
          <select
            className="form-input form-select-sm history-filter-select"
            value={taskFilter}
            onChange={e => setTaskFilter(e.target.value)}
            aria-label={t('harness.history.filterTask')}
          >
            <option value="">{t('harness.history.allTasks')}</option>
            {allTaskIds.map(id => (
              <option key={id} value={id}>
                {historyTaskTitle({ taskId: id }, taskNames)}
              </option>
            ))}
          </select>
        </div>

        {jobTypes.length > 1 && (
          <div className="history-filter-group">
            <label className="history-filter-label">{t('harness.history.type')}</label>
            <select
              className="form-input form-select-sm history-filter-select"
              value={jobTypeFilter}
              onChange={e => setJobTypeFilter(e.target.value)}
              aria-label={t('harness.history.filterType')}
            >
              <option value="">{t('harness.history.allTypes')}</option>
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
            {t('harness.history.clearFilters')}
          </button>
        )}
      </div>

      {filteredItems.length === 0 ? (
        <div className="empty-history-filter">
          <p className="empty-text">{t('harness.history.noMatches')}</p>
          <button
            type="button"
            className="btn-secondary"
            onClick={() => {
              setStatusFilter('')
              setTaskFilter('')
              setJobTypeFilter('')
            }}
          >
            {t('harness.history.clearFilters')}
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
                title={t('harness.history.runId', { id: item.id })}
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
                      <span className="history-row-steps">{t('harness.history.steps', { count: row.steps })}</span>
                    </div>
                    {canContinue && (
                      <button
                        type="button"
                        className="btn-action history-continue-btn"
                        onClick={e => {
                          e.stopPropagation()
                          onContinue?.(item)
                        }}
                        title={t('harness.history.continueTitle')}
                      >
                        <FastForward size={13} fill="currentColor" />
                        <span>
                          {item.resumeSubtaskTitle
                            ? t('harness.history.resume', { title: item.resumeSubtaskTitle })
                            : t('harness.history.continue')}
                        </span>
                      </button>
                    )}
                    {isHistoryFollowUpStatus(String(item.status)) && (
                      <button
                        type="button"
                        className="btn-action history-continue-btn"
                        onClick={e => {
                          e.stopPropagation()
                          onInspect(item)
                        }}
                        title={t('harness.history.followUpTitle')}
                      >
                        <FastForward size={13} />
                        <span>{t('harness.history.followUp')}</span>
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

      {pagination && (
        <div
          className="history-pagination-bar"
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '0.75rem 0.25rem 0.25rem',
            marginTop: '0.75rem',
            borderTop: '1px solid var(--border-subtle)',
            flexWrap: 'wrap',
            gap: '0.75rem',
          }}
        >
          <div style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>
            {t('harness.history.showing', { start: pagination.total === 0 ? 0 : (pagination.page - 1) * pagination.limit + 1, end: Math.min(pagination.page * pagination.limit, pagination.total), total: pagination.total })}
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}>
            {onPageSizeChange && (
              <div style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', fontSize: '0.85rem' }}>
                <span style={{ color: 'var(--text-secondary)' }}>{t('harness.history.pageSize')}</span>
                <select
                  className="form-input form-select-sm"
                  value={pagination.limit}
                  onChange={e => onPageSizeChange(Number(e.target.value))}
                  style={{ width: 'auto', padding: '2px 6px' }}
                >
                  <option value="10">10</option>
                  <option value="20">20</option>
                  <option value="50">50</option>
                  <option value="100">100</option>
                </select>
              </div>
            )}
            <div style={{ display: 'flex', alignItems: 'center', gap: '0.35rem' }}>
              <button
                type="button"
                className="btn-secondary mini-btn"
                disabled={pagination.page <= 1 || isLoading}
                onClick={() => onPageChange?.(1)}
                title={t('harness.history.firstPage')}
                style={{ padding: '3px 6px', display: 'inline-flex', alignItems: 'center' }}
              >
                <ChevronsLeft size={14} />
              </button>
              <button
                type="button"
                className="btn-secondary mini-btn"
                disabled={pagination.page <= 1 || isLoading}
                onClick={() => onPageChange?.(pagination.page - 1)}
                title={t('harness.history.previousPage')}
                style={{ padding: '3px 6px', display: 'inline-flex', alignItems: 'center' }}
              >
                <ChevronLeft size={14} />
              </button>
              <span style={{ fontSize: '0.85rem', padding: '0 4px', color: 'var(--text-primary)' }}>
                {t('harness.history.page', { page: pagination.page, total: Math.max(1, pagination.totalPages) })}
              </span>
              <button
                type="button"
                className="btn-secondary mini-btn"
                disabled={pagination.page >= pagination.totalPages || isLoading}
                onClick={() => onPageChange?.(pagination.page + 1)}
                title={t('harness.history.nextPage')}
                style={{ padding: '3px 6px', display: 'inline-flex', alignItems: 'center' }}
              >
                <ChevronRight size={14} />
              </button>
              <button
                type="button"
                className="btn-secondary mini-btn"
                disabled={pagination.page >= pagination.totalPages || isLoading}
                onClick={() => onPageChange?.(pagination.totalPages)}
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
