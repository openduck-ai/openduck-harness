import { Trans, useTranslation } from 'react-i18next'
import { formatNumber } from '../i18n/format.ts'
import { useState, useMemo } from 'react'
import type {
  HarnessRunResponse,
  ProviderCallRecord,
  TaskProviderStats,
} from '@aaif/goose-hub-core'
import { computeTaskProviderStats } from '../taskTrajectory.ts'
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
  providerStats?: TaskProviderStats | null
  onJumpToStep?: (stepIdx: number) => void
}

interface FlattenedCallRecord extends ProviderCallRecord {
  stepIndex: number
  callIndexInStep: number
}

type StatusFilter = 'all' | 'success' | 'failed' | 'retried'

export function ProviderStatsDashboard({
  run,
  providerStats: inputProviderStats,
  onJumpToStep,
}: ProviderStatsDashboardProps) {
  const { t } = useTranslation()
  const providerStats = useMemo(
    () => inputProviderStats ?? computeTaskProviderStats(run),
    [inputProviderStats, run],
  )
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
        (call.retryAttempt === undefined ||
          call.retryAttempt === null ||
          call.retryAttempt === 0) &&
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
            <span>{t('config.stats.telemetry')}</span>
          </div>
          <h3>{t('config.stats.title')}</h3>
          <p className="stats-header-desc">
            <Trans
              t={t}
              i18nKey="config.stats.description"
              values={{ taskId: run.taskId }}
              components={{ code: <code /> }}
            />
          </p>
        </div>
      </div>

      {/* 2. KPI Summary Grid */}
      <div className="provider-kpi-grid">
        <div className="provider-kpi-card">
          <div className="kpi-label-row">
            <span className="kpi-label">{t('config.stats.totalInvocations')}</span>
            <Bot size={15} className="kpi-icon" />
          </div>
          <div className="kpi-value">{providerStats.totalCalls}</div>
          <div className="kpi-subtext">
            <span className="kpi-ok">
              {t('config.stats.succeeded', {
                count: providerStats.successfulCalls,
              })}
            </span>
            {providerStats.failedCalls > 0 && (
              <span className="kpi-err">
                {' • '}
                {t('config.stats.failedCount', {
                  count: providerStats.failedCalls,
                })}
              </span>
            )}
          </div>
        </div>

        <div className="provider-kpi-card">
          <div className="kpi-label-row">
            <span className="kpi-label">{t('config.stats.successRate')}</span>
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
            <span className="kpi-label">{t('config.stats.retriesFailovers')}</span>
            <RotateCcw size={15} className="kpi-icon" />
          </div>
          <div className="kpi-value">{providerStats.totalRetries}</div>
          <div className="kpi-subtext">
            {providerStats.totalRetries === 0
              ? t('config.stats.zeroRetry')
              : t('config.stats.retryAttempts', {
                  count: providerStats.totalRetries,
                })}
          </div>
        </div>

        <div className="provider-kpi-card">
          <div className="kpi-label-row">
            <span className="kpi-label">{t('config.stats.avgCallLatency')}</span>
            <Clock size={15} className="kpi-icon" />
          </div>
          <div className="kpi-value font-mono">
            {formatNumber(Math.round(providerStats.avgDurationMs))}{' '}
            <span className="kpi-unit">{t('config.stats.millisecondUnit')}</span>
          </div>
          <div className="kpi-subtext">
            {providerStats.avgWaitMs !== undefined && providerStats.avgWaitMs !== null
              ? t('config.stats.timing', {
                  wait: Math.round(providerStats.avgWaitMs),
                  receive: Math.round(providerStats.avgReceiveMs || 0),
                })
              : t('config.stats.completionTime', {
                  seconds: (providerStats.totalDurationMs / 1000).toFixed(2),
                })}
          </div>
        </div>

        <div className="provider-kpi-card">
          <div className="kpi-label-row">
            <span className="kpi-label">{t('config.stats.totalLlmTokens')}</span>
            <Sparkles size={15} className="kpi-icon" />
          </div>
          <div className="kpi-value font-mono token-highlight">
            {formatNumber(providerStats.totalTokens)}
          </div>
          <div className="kpi-subtext">
            {t('config.stats.tokenSummary', {
              input: formatNumber(providerStats.inputTokens),
              output: formatNumber(providerStats.outputTokens),
            })}
          </div>
        </div>

        {providerStats.totalCost !== undefined &&
          providerStats.totalCost !== null &&
          providerStats.totalCost > 0 && (
            <div className="provider-kpi-card">
              <div className="kpi-label-row">
                <span className="kpi-label">{t('config.stats.estimatedCost')}</span>
                <DollarSign size={15} className="kpi-icon" />
              </div>
              <div className="kpi-value font-mono">${providerStats.totalCost.toFixed(4)}</div>
              <div className="kpi-subtext">{t('config.stats.acrossCalls')}</div>
            </div>
          )}
      </div>

      {/* 3. Provider & Model Breakdowns */}
      <div className="provider-breakdown-section">
        <h4 className="section-title">
          <Layers size={15} /> {t('config.stats.byProvider')}
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
                  <span className="p-card-calls-badge">
                    {t('config.stats.callCount', { count: pStat.totalCalls })}
                  </span>
                </div>

                <div className="p-card-metrics-grid">
                  <div className="p-metric-cell">
                    <span className="p-metric-label">{t('config.stats.successRate')}</span>
                    <span
                      className={`p-metric-value ${pStat.failedCalls > 0 ? 'text-warn' : 'text-success'}`}
                    >
                      {pSuccessRate.toFixed(1)}%
                    </span>
                  </div>
                  <div className="p-metric-cell">
                    <span className="p-metric-label">{t('config.stats.avgLatency')}</span>
                    <span className="p-metric-value">
                      {t('config.stats.milliseconds', { ms: Math.round(pStat.avgDurationMs) })}
                    </span>
                  </div>
                  <div className="p-metric-cell">
                    <span className="p-metric-label">{t('config.stats.tokens')}</span>
                    <span className="p-metric-value">{formatNumber(pStat.totalTokens)}</span>
                  </div>
                  <div className="p-metric-cell">
                    <span className="p-metric-label">{t('config.stats.retries')}</span>
                    <span className="p-metric-value">{pStat.retries}</span>
                  </div>
                </div>

                {pStat.errors && pStat.errors.length > 0 && (
                  <div className="p-card-errors">
                    <span className="p-errors-title">{t('config.stats.errors')}</span>
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
            <Bot size={15} /> {t('config.stats.byModel')}
          </h4>
          <div className="model-table-container">
            <table className="model-summary-table">
              <thead>
                <tr>
                  <th>{t('config.stats.providerModel')}</th>
                  <th>{t('config.stats.calls')}</th>
                  <th>{t('config.stats.success')}</th>
                  <th>{t('config.stats.retries')}</th>
                  <th>{t('config.stats.avgLatency')}</th>
                  <th>{t('config.stats.tokenBreakdown')}</th>
                  <th>{t('config.stats.totalTokens')}</th>
                  {providerStats.totalCost ? <th>{t('config.stats.cost')}</th> : null}
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
                      <span
                        className={`status-pill ${mStat.failedCalls > 0 ? 'pill-warn' : 'pill-ok'}`}
                      >
                        {mStat.successfulCalls} / {mStat.totalCalls}
                      </span>
                    </td>
                    <td>
                      {mStat.retries > 0 ? <span className="text-warn">{mStat.retries}</span> : 0}
                    </td>
                    <td className="font-mono">
                      {t('config.stats.milliseconds', { ms: Math.round(mStat.avgDurationMs) })}
                    </td>
                    <td className="font-mono text-muted">
                      {formatNumber(mStat.inputTokens)} / {formatNumber(mStat.outputTokens)}
                    </td>
                    <td className="font-mono font-bold">{formatNumber(mStat.totalTokens)}</td>
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
              <Sparkles size={15} /> {t('config.stats.ledgerTitle', { count: allCalls.length })}
            </h4>
            <span className="ledger-subtitle">{t('config.stats.ledgerSubtitle')}</span>
          </div>

          <div className="ledger-filters-toolbar">
            <div className="ledger-search-box">
              <Search size={14} className="search-icon" />
              <input
                type="text"
                placeholder={t('config.stats.search')}
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
                aria-label={t('config.stats.filterProvider')}
              >
                <option value="all">{t('config.stats.allProviders')}</option>
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
                    ? t('config.stats.all')
                    : tab === 'success'
                      ? t('config.stats.success')
                      : tab === 'failed'
                        ? t('config.stats.failed')
                        : t('config.stats.retries')}
                </button>
              ))}
            </div>
          </div>
        </div>

        {filteredCalls.length === 0 ? (
          <div className="empty-ledger-card">
            {allCalls.length === 0 ? (
              <p>{t('config.stats.noCalls')}</p>
            ) : (
              <p>{t('config.stats.noMatches')}</p>
            )}
          </div>
        ) : (
          <div className="ledger-table-wrapper">
            <table className="ledger-table">
              <thead>
                <tr>
                  <th>{t('config.stats.step')}</th>
                  <th>{t('config.stats.attempt')}</th>
                  <th>{t('config.stats.providerAndModel')}</th>
                  <th>{t('config.stats.status')}</th>
                  <th>{t('config.stats.latency')}</th>
                  <th>{t('config.stats.networkWaterfall')}</th>
                  <th>{t('config.stats.tokens')}</th>
                  <th>{t('config.stats.retryDetails')}</th>
                  <th>{t('config.stats.action')}</th>
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
                            title={t('config.stats.inspectStep', {
                              step: call.stepIndex,
                            })}
                          >
                            {t('config.stats.stepNumber', {
                              step: call.stepIndex,
                            })}
                            <ArrowRight size={11} />
                          </button>
                        ) : (
                          <span className="font-mono">#{call.stepIndex}</span>
                        )}
                      </td>
                      <td>
                        <span className="attempt-badge">
                          #{call.callIndexInStep + 1}
                          {call.retryAttempt !== undefined &&
                            call.retryAttempt !== null &&
                            call.retryAttempt > 0 && (
                              <span className="retry-pill">
                                {t('config.stats.retryNumber', {
                                  count: call.retryAttempt,
                                })}
                              </span>
                            )}
                          {call.isSecondary && (
                            <span className="secondary-pill">{t('config.stats.secondary')}</span>
                          )}
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
                          {call.success ? t('config.stats.ok') : t('config.stats.fail')}
                        </span>
                      </td>
                      <td className="font-mono">
                        {t('config.stats.milliseconds', { ms: formatNumber(call.latencyMs) })}
                      </td>
                      <td>
                        {call.networkTiming ? (
                          (() => {
                            const total = call.networkTiming.totalMs || call.latencyMs || 1
                            const connPct = Math.round(
                              ((call.networkTiming.connectionMs || 0) / total) * 100,
                            )
                            const sendPct = Math.max(
                              1,
                              Math.round(((call.networkTiming.sendMs || 0) / total) * 100),
                            )
                            const waitPct = Math.round(
                              ((call.networkTiming.waitMs || 0) / total) * 100,
                            )
                            const recvPct = Math.round(
                              ((call.networkTiming.receiveMs || 0) / total) * 100,
                            )
                            return (
                              <div className="network-waterfall-cell">
                                <div
                                  className="network-waterfall-bar"
                                  title={t('config.stats.waterfallTooltip', {
                                    connect: call.networkTiming.connectionMs,
                                    send: call.networkTiming.sendMs,
                                    wait: call.networkTiming.waitMs,
                                    receive: call.networkTiming.receiveMs,
                                  })}
                                >
                                  {call.networkTiming.connectionMs > 0 && (
                                    <div
                                      className="waterfall-seg seg-connect"
                                      style={{ width: `${connPct}%` }}
                                    />
                                  )}
                                  <div
                                    className="waterfall-seg seg-send"
                                    style={{ width: `${sendPct}%` }}
                                  />
                                  <div
                                    className="waterfall-seg seg-wait"
                                    style={{ width: `${waitPct}%` }}
                                  />
                                  {call.networkTiming.receiveMs > 0 && (
                                    <div
                                      className="waterfall-seg seg-receive"
                                      style={{ width: `${recvPct}%` }}
                                    />
                                  )}
                                </div>
                                <div className="waterfall-legend-inline">
                                  <span
                                    className="legend-item-inline"
                                    title={t('config.stats.ttfbTitle')}
                                  >
                                    <span className="legend-dot wait" />
                                    {t('config.stats.ttfb', {
                                      ms: call.networkTiming.waitMs,
                                    })}
                                  </span>
                                  {call.networkTiming.receiveMs > 0 && (
                                    <span
                                      className="legend-item-inline"
                                      title={t('config.stats.streamTitle')}
                                    >
                                      <span className="legend-dot receive" />
                                      {t('config.stats.stream', {
                                        ms: call.networkTiming.receiveMs,
                                      })}
                                    </span>
                                  )}
                                </div>
                              </div>
                            )
                          })()
                        ) : (
                          <span className="text-muted">—</span>
                        )}
                      </td>
                      <td className="font-mono">
                        {call.tokenUsage ? (
                          <span
                            title={t('config.stats.tokenTooltip', {
                              input: formatNumber(call.tokenUsage.inputTokens ?? 0),
                              output: formatNumber(call.tokenUsage.outputTokens ?? 0),
                            })}
                          >
                            {formatNumber(
                              call.tokenUsage.totalTokens ??
                                (call.tokenUsage.inputTokens ?? 0) +
                                  (call.tokenUsage.outputTokens ?? 0),
                            )}
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
                          {!call.retryReason && !call.error && (
                            <span className="text-muted">—</span>
                          )}
                        </div>
                      </td>
                      <td>
                        {(call.error || call.retryReason) && (
                          <button
                            type="button"
                            className="btn-tiny"
                            onClick={() => toggleCallExpanded(callKey)}
                          >
                            {isExpanded ? t('config.stats.hide') : t('config.stats.details')}
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
