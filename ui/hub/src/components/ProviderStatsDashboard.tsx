import { useState, useMemo } from 'react'
import type { HarnessRunResponse, ProviderCallRecord, TaskProviderStats } from '@aaif/goose-hub-core'
import {
  Zap,
  CheckCircle2,
  AlertCircle,
  Clock,
  RotateCcw,
  Sparkles,
  Search,
  ArrowRight,
  Layers,
  Bot,
  DollarSign,
} from 'lucide-react'

interface ProviderStatsDashboardProps {
  run: HarnessRunResponse
  providerStats: TaskProviderStats
  onJumpToStep?: (stepIdx: number) => void
}

interface FlattenedCallRecord extends ProviderCallRecord {
  stepIndex: number
  callIndexInStep: number
}

type StatusFilter = 'all' | 'success' | 'failed' | 'retried'

export function ProviderStatsDashboard({
  run,
  providerStats,
  onJumpToStep,
}: ProviderStatsDashboardProps) {
  const [searchQuery, setSearchQuery] = useState('')
  const [selectedProvider, setSelectedProvider] = useState<string>('all')
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all')
  const [expandedCallId, setExpandedCallId] = useState<string | null>(null)

  // Flatten all calls across steps
  const allCalls = useMemo<FlattenedCallRecord[]>(() => {
    const steps = run.trajectory?.steps ?? []
    const calls: FlattenedCallRecord[] = []

    steps.forEach(step => {
      const stepCalls = step.providerCalls ?? []
      stepCalls.forEach((call, cIdx) => {
        calls.push({
          ...call,
          stepIndex: step.stepIndex,
          callIndexInStep: cIdx,
        })
      })
    })

    return calls
  }, [run.trajectory?.steps])

  const providersList = useMemo(() => {
    return Object.keys(providerStats.byProvider || {})
  }, [providerStats.byProvider])

  // Filtered calls ledger
  const filteredCalls = useMemo(() => {
    return allCalls.filter(call => {
      if (selectedProvider !== 'all' && call.provider !== selectedProvider) {
        return false
      }
      if (statusFilter === 'success' && !call.success) {
        return false
      }
      if (statusFilter === 'failed' && call.success) {
        return false
      }
      if (
        statusFilter === 'retried' &&
        (call.retryAttempt === undefined || call.retryAttempt === null || call.retryAttempt === 0) &&
        !call.retryReason
      ) {
        return false
      }
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase()
        const matchProvider = call.provider.toLowerCase().includes(q)
        const matchModel = call.model?.toLowerCase().includes(q)
        const matchReason = call.retryReason?.toLowerCase().includes(q)
        const matchError = call.error?.toLowerCase().includes(q)
        return matchProvider || matchModel || matchReason || matchError
      }
      return true
    })
  }, [allCalls, selectedProvider, statusFilter, searchQuery])

  const successRate =
    providerStats.totalCalls > 0
      ? (providerStats.successfulCalls / providerStats.totalCalls) * 100
      : 100

  const toggleCallExpanded = (id: string) => {
    setExpandedCallId(prev => (prev === id ? null : id))
  }

  return (
    <div className="provider-stats-dashboard">
      {/* 1. Header Banner */}
      <div className="stats-dashboard-header">
        <div className="stats-header-titles">
          <div className="stats-header-badge">
            <Zap size={14} className="text-accent" />
            <span>Telemetry & Performance</span>
          </div>
          <h3>LLM Provider Call Statistics</h3>
          <p className="stats-header-desc">
            Aggregated metrics for model invocations, latency, tokens, retries, and errors across task{' '}
            <code>{run.taskId}</code>.
          </p>
        </div>
      </div>

      {/* 2. KPI Summary Grid */}
      <div className="provider-kpi-grid">
        <div className="provider-kpi-card">
          <div className="kpi-label-row">
            <span className="kpi-label">Total Invocations</span>
            <Bot size={15} className="kpi-icon" />
          </div>
          <div className="kpi-value">{providerStats.totalCalls}</div>
          <div className="kpi-subtext">
            <span className="kpi-ok">{providerStats.successfulCalls} succeeded</span>
            {providerStats.failedCalls > 0 && (
              <span className="kpi-err"> • {providerStats.failedCalls} failed</span>
            )}
          </div>
        </div>

        <div className="provider-kpi-card">
          <div className="kpi-label-row">
            <span className="kpi-label">Success Rate</span>
            {successRate >= 95 ? (
              <CheckCircle2 size={15} className="kpi-icon text-success" />
            ) : (
              <AlertCircle size={15} className="kpi-icon text-warn" />
            )}
          </div>
          <div className="kpi-value font-mono">{successRate.toFixed(1)}%</div>
          <div className="kpi-progress-bar">
            <div
              className={`kpi-progress-fill ${successRate < 90 ? 'warn' : 'ok'}`}
              style={{ width: `${Math.min(100, Math.max(0, successRate))}%` }}
            />
          </div>
        </div>

        <div className="provider-kpi-card">
          <div className="kpi-label-row">
            <span className="kpi-label">Retries & Failovers</span>
            <RotateCcw size={15} className="kpi-icon" />
          </div>
          <div className="kpi-value">{providerStats.totalRetries}</div>
          <div className="kpi-subtext">
            {providerStats.totalRetries === 0
              ? 'Zero retry overhead'
              : `${providerStats.totalRetries} retry attempts logged`}
          </div>
        </div>

        <div className="provider-kpi-card">
          <div className="kpi-label-row">
            <span className="kpi-label">Avg Call Latency</span>
            <Clock size={15} className="kpi-icon" />
          </div>
          <div className="kpi-value font-mono">
            {Math.round(providerStats.avgDurationMs).toLocaleString()}{' '}
            <span className="kpi-unit">ms</span>
          </div>
          <div className="kpi-subtext">
            {(providerStats.totalDurationMs / 1000).toFixed(2)}s total completion time
          </div>
        </div>

        <div className="provider-kpi-card">
          <div className="kpi-label-row">
            <span className="kpi-label">Total LLM Tokens</span>
            <Sparkles size={15} className="kpi-icon" />
          </div>
          <div className="kpi-value font-mono token-highlight">
            {providerStats.totalTokens.toLocaleString()}
          </div>
          <div className="kpi-subtext">
            ↑ {providerStats.inputTokens.toLocaleString()} prompt • ↓{' '}
            {providerStats.outputTokens.toLocaleString()} output
          </div>
        </div>

        {providerStats.totalCost !== undefined && providerStats.totalCost !== null && providerStats.totalCost > 0 && (
          <div className="provider-kpi-card">
            <div className="kpi-label-row">
              <span className="kpi-label">Estimated Cost</span>
              <DollarSign size={15} className="kpi-icon" />
            </div>
            <div className="kpi-value font-mono">${providerStats.totalCost.toFixed(4)}</div>
            <div className="kpi-subtext">Across all model calls</div>
          </div>
        )}
      </div>

      {/* 3. Provider & Model Breakdowns */}
      <div className="provider-breakdown-section">
        <h4 className="section-title">
          <Layers size={15} /> Breakdown by Provider
        </h4>
        <div className="provider-cards-grid">
          {Object.entries(providerStats.byProvider || {}).map(([pName, pStat]) => {
            const pSuccessRate =
              pStat.totalCalls > 0 ? (pStat.successfulCalls / pStat.totalCalls) * 100 : 100
            return (
              <div key={pName} className="provider-summary-card">
                <div className="p-card-top">
                  <div className="p-card-title">
                    <span className="provider-dot" />
                    <strong>{pName}</strong>
                  </div>
                  <span className="p-card-calls-badge">{pStat.totalCalls} calls</span>
                </div>

                <div className="p-card-metrics-grid">
                  <div className="p-metric-cell">
                    <span className="p-metric-label">Success Rate</span>
                    <span className={`p-metric-value ${pStat.failedCalls > 0 ? 'text-warn' : 'text-success'}`}>
                      {pSuccessRate.toFixed(1)}%
                    </span>
                  </div>
                  <div className="p-metric-cell">
                    <span className="p-metric-label">Avg Latency</span>
                    <span className="p-metric-value">{Math.round(pStat.avgDurationMs)} ms</span>
                  </div>
                  <div className="p-metric-cell">
                    <span className="p-metric-label">Tokens</span>
                    <span className="p-metric-value">{pStat.totalTokens.toLocaleString()}</span>
                  </div>
                  <div className="p-metric-cell">
                    <span className="p-metric-label">Retries</span>
                    <span className="p-metric-value">{pStat.retries}</span>
                  </div>
                </div>

                {pStat.errors && pStat.errors.length > 0 && (
                  <div className="p-card-errors">
                    <span className="p-errors-title">Encountered Errors:</span>
                    <ul className="p-errors-list">
                      {pStat.errors.map((err, eIdx) => (
                        <li key={eIdx}>{err}</li>
                      ))}
                    </ul>
                  </div>
                )}
              </div>
            )
          })}
        </div>
      </div>

      {/* Model Breakdown if available */}
      {providerStats.byModel && Object.keys(providerStats.byModel).length > 0 && (
        <div className="provider-breakdown-section">
          <h4 className="section-title">
            <Bot size={15} /> Breakdown by Model
          </h4>
          <div className="model-table-container">
            <table className="model-summary-table">
              <thead>
                <tr>
                  <th>Provider / Model</th>
                  <th>Calls</th>
                  <th>Success</th>
                  <th>Retries</th>
                  <th>Avg Latency</th>
                  <th>Tokens (Prompt / Output)</th>
                  <th>Total Tokens</th>
                  {providerStats.totalCost ? <th>Cost</th> : null}
                </tr>
              </thead>
              <tbody>
                {Object.entries(providerStats.byModel).map(([mKey, mStat]) => (
                  <tr key={mKey}>
                    <td>
                      <div className="model-name-cell">
                        <span className="provider-tag">{mStat.provider}</span>
                        <code className="model-code">{mStat.model || mKey}</code>
                      </div>
                    </td>
                    <td>
                      <strong>{mStat.totalCalls}</strong>
                    </td>
                    <td>
                      <span className={`status-pill ${mStat.failedCalls > 0 ? 'pill-warn' : 'pill-ok'}`}>
                        {mStat.successfulCalls} / {mStat.totalCalls}
                      </span>
                    </td>
                    <td>{mStat.retries > 0 ? <span className="text-warn">{mStat.retries}</span> : 0}</td>
                    <td className="font-mono">{Math.round(mStat.avgDurationMs)} ms</td>
                    <td className="font-mono text-muted">
                      {mStat.inputTokens.toLocaleString()} / {mStat.outputTokens.toLocaleString()}
                    </td>
                    <td className="font-mono font-bold">{mStat.totalTokens.toLocaleString()}</td>
                    {providerStats.totalCost ? (
                      <td className="font-mono">
                        {mStat.totalCost !== undefined && mStat.totalCost !== null
                          ? `$${mStat.totalCost.toFixed(4)}`
                          : '—'}
                      </td>
                    ) : null}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* 4. Chronological Provider Calls Ledger */}
      <div className="calls-ledger-section">
        <div className="ledger-header">
          <div className="ledger-title-group">
            <h4 className="section-title">
              <Sparkles size={15} /> Chronological Provider Invocations ({allCalls.length})
            </h4>
            <span className="ledger-subtitle">
              Detailed step-by-step breakdown of every model request, retry, and latency measurement
            </span>
          </div>

          <div className="ledger-filters-toolbar">
            <div className="ledger-search-box">
              <Search size={14} className="search-icon" />
              <input
                type="text"
                placeholder="Search provider, model, retry reason, error..."
                value={searchQuery}
                onChange={e => setSearchQuery(e.target.value)}
                className="ledger-search-input"
              />
            </div>

            {providersList.length > 1 && (
              <select
                className="ledger-filter-select"
                value={selectedProvider}
                onChange={e => setSelectedProvider(e.target.value)}
                aria-label="Filter by provider"
              >
                <option value="all">All Providers</option>
                {providersList.map(p => (
                  <option key={p} value={p}>
                    {p}
                  </option>
                ))}
              </select>
            )}

            <div className="ledger-status-tabs">
              {(['all', 'success', 'failed', 'retried'] as StatusFilter[]).map(tab => (
                <button
                  key={tab}
                  type="button"
                  className={`ledger-tab-btn ${statusFilter === tab ? 'active' : ''}`}
                  onClick={() => setStatusFilter(tab)}
                >
                  {tab === 'all'
                    ? 'All'
                    : tab === 'success'
                      ? 'Success'
                      : tab === 'failed'
                        ? 'Failed'
                        : 'Retries'}
                </button>
              ))}
            </div>
          </div>
        </div>

        {filteredCalls.length === 0 ? (
          <div className="empty-ledger-card">
            {allCalls.length === 0 ? (
              <p>No individual provider call records were captured for this run.</p>
            ) : (
              <p>No provider calls match the active filter or search criteria.</p>
            )}
          </div>
        ) : (
          <div className="ledger-table-wrapper">
            <table className="ledger-table">
              <thead>
                <tr>
                  <th>Step</th>
                  <th>Attempt</th>
                  <th>Provider & Model</th>
                  <th>Status</th>
                  <th>Latency</th>
                  <th>Tokens</th>
                  <th>Retry Details / Reason</th>
                  <th>Action</th>
                </tr>
              </thead>
              <tbody>
                {filteredCalls.map((call, idx) => {
                  const callKey = `${call.stepIndex}-${call.callIndexInStep}-${idx}`
                  const isExpanded = expandedCallId === callKey
                  return (
                    <tr
                      key={callKey}
                      className={`ledger-row ${!call.success ? 'row-failed' : ''} ${
                        call.retryAttempt ? 'row-retried' : ''
                      }`}
                    >
                      <td>
                        {onJumpToStep ? (
                          <button
                            type="button"
                            className="step-jump-btn"
                            onClick={() => onJumpToStep(call.stepIndex)}
                            title={`Inspect Step #${call.stepIndex}`}
                          >
                            Step #{call.stepIndex}
                            <ArrowRight size={11} />
                          </button>
                        ) : (
                          <span className="font-mono">#{call.stepIndex}</span>
                        )}
                      </td>
                      <td>
                        <span className="attempt-badge">
                          #{call.callIndexInStep + 1}
                          {call.retryAttempt !== undefined && call.retryAttempt !== null && call.retryAttempt > 0 && (
                            <span className="retry-pill">Retry {call.retryAttempt}</span>
                          )}
                          {call.isSecondary && <span className="secondary-pill">Secondary</span>}
                        </span>
                      </td>
                      <td>
                        <div className="provider-model-col">
                          <span className="p-badge">{call.provider}</span>
                          {call.model && <code className="m-badge">{call.model}</code>}
                        </div>
                      </td>
                      <td>
                        <span className={`status-pill ${call.success ? 'pill-ok' : 'pill-err'}`}>
                          {call.success ? '✓ Ok' : '✕ Fail'}
                        </span>
                      </td>
                      <td className="font-mono">{call.latencyMs.toLocaleString()} ms</td>
                      <td className="font-mono">
                        {call.tokenUsage ? (
                          <span
                            title={`Input: ${call.tokenUsage.inputTokens?.toLocaleString() ?? 0} | Output: ${call.tokenUsage.outputTokens?.toLocaleString() ?? 0}`}
                          >
                            {(call.tokenUsage.totalTokens ?? (call.tokenUsage.inputTokens ?? 0) + (call.tokenUsage.outputTokens ?? 0)).toLocaleString()}
                          </span>
                        ) : (
                          '—'
                        )}
                      </td>
                      <td>
                        <div className="retry-reason-col">
                          {call.retryReason && (
                            <span className="reason-text" title={call.retryReason}>
                              {call.retryReason}
                            </span>
                          )}
                          {call.error && (
                            <span className="error-text" title={call.error}>
                              ⚠️ {call.error}
                            </span>
                          )}
                          {!call.retryReason && !call.error && <span className="text-muted">—</span>}
                        </div>
                      </td>
                      <td>
                        {(call.error || call.retryReason) && (
                          <button
                            type="button"
                            className="btn-tiny"
                            onClick={() => toggleCallExpanded(callKey)}
                          >
                            {isExpanded ? 'Hide' : 'Details'}
                          </button>
                        )}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  )
}
