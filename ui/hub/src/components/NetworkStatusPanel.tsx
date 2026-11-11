import { formatNumber, formatTime, formatDateTime } from '../i18n/format.ts'
import { useTranslation } from 'react-i18next'
import { useEffect, useRef, useState } from 'react'
import type { createApi, NetworkProbe, NetworkStatus } from '@aaif/goose-hub-core'
import { Activity, RefreshCw } from 'lucide-react'

interface Props {
  api: ReturnType<typeof createApi>
  isConnected: boolean
}

function ProbeValue({ probe }: { probe: NetworkProbe }) {
  const { t } = useTranslation()
  return (
    <div className={`network-probe network-probe-${probe.status}`} title={probe.detail}>
      <span className={`status-dot ${probe.status === 'reachable' ? 'online' : 'offline'}`} />
      <span>
        <strong>{probe.latencyMs === null ? probe.status : t('network.panel.latency', { latency: formatNumber(probe.latencyMs, { minimumFractionDigits: 1, maximumFractionDigits: 1, useGrouping: false }) })}</strong>
        <small className="muted">{probe.detail}</small>
      </span>
    </div>
  )
}

function LatencyHistory({ history, provider }: { history: NetworkStatus[]; provider: string }) {
  const { t } = useTranslation()
  const samples = history.map(sample => ({
    time: sample.checkedAt,
    probe: sample.endpoints.find(endpoint => endpoint.provider === provider)?.providerHttp,
  }))
  const maximum = Math.max(1, ...samples.map(sample => sample.probe?.latencyMs ?? 0))
  const failures = samples.filter(sample => sample.probe && sample.probe.status !== 'reachable').length
  return (
    <div className="network-history">
      <div className="network-history-bars" aria-label={t('network.panel.historyLabel', { provider })}>
        {samples.map(sample => (
          <span
            key={sample.time}
            className={sample.probe?.status === 'reachable' ? 'network-history-ok' : 'network-history-failed'}
            style={{ height: `${Math.max(8, ((sample.probe?.latencyMs ?? maximum) / maximum) * 100)}%` }}
            title={`${formatTime(sample.time)}: ${sample.probe?.detail ?? t('network.panel.noSample')}${sample.probe?.latencyMs != null ? ` · ${t('network.panel.latency', { latency: formatNumber(sample.probe.latencyMs, { minimumFractionDigits: 1, maximumFractionDigits: 1, useGrouping: false }) })}` : ''}`}
          />
        ))}
      </div>
      <small className="muted">{t('network.panel.historySummary', { samples: samples.length, failures })}</small>
    </div>
  )
}

export function NetworkStatusPanel({ api, isConnected }: Props) {
  const { t } = useTranslation()
  const [report, setReport] = useState<NetworkStatus | null>(null)
  const [history, setHistory] = useState<NetworkStatus[]>([])
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [autoRefresh, setAutoRefresh] = useState(true)
  const [refresh, setRefresh] = useState(0)
  const controller = useRef<AbortController | null>(null)

  useEffect(() => {
    setReport(null)
    setHistory([])
    setError(null)
  }, [api])

  useEffect(() => {
    if (!isConnected) {
      setLoading(false)
      return
    }
    let disposed = false
    let running = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const load = async () => {
      if (document.hidden || running) return
      running = true
      const request = new AbortController()
      controller.current = request
      setLoading(true)
      try {
        const next = await api.getNetworkStatus(request.signal)
        if (disposed) return
        setReport(next)
        setError(null)
        setHistory(previous => previous.at(-1)?.checkedAt === next.checkedAt
          ? previous
          : [...previous, next].slice(-20))
      } catch (cause) {
        if (!disposed && !request.signal.aborted) {
          setError(cause instanceof Error ? cause.message : 'network.panel.error')
        }
      } finally {
        running = false
        if (!disposed) {
          setLoading(false)
          if (autoRefresh) timer = setTimeout(() => void load(), 30_000)
        }
      }
    }
    const onVisible = () => {
      if (!document.hidden && autoRefresh) {
        clearTimeout(timer)
        void load()
      }
    }
    void load()
    document.addEventListener('visibilitychange', onVisible)
    return () => {
      disposed = true
      clearTimeout(timer)
      controller.current?.abort()
      document.removeEventListener('visibilitychange', onVisible)
    }
  }, [api, isConnected, autoRefresh, refresh])

  const healthy = report?.endpoints.filter(endpoint => endpoint.providerHttp.status === 'reachable').length ?? 0

  return (
    <section
      id="settings-network"
      data-settings-section="settings-network"
      className="panel network-panel"
      aria-labelledby="network-status-title"
    >
      <div className="network-panel-heading">
        <h3 id="network-status-title"><Activity size={18} /> {t('network.panel.title')}</h3>
        <button
          type="button"
          className="secondary"
          disabled={loading || !isConnected}
          onClick={() => setRefresh(value => value + 1)}
        >
          <RefreshCw size={14} /> {loading ? t('network.panel.checking') : t('network.panel.refresh')}
        </button>
      </div>
      <p className="muted text-sm">{t('network.panel.description')}</p>
      <label className="network-auto-refresh">
        <input type="checkbox" checked={autoRefresh} onChange={event => setAutoRefresh(event.target.checked)} />
        {t('network.panel.autoRefresh')}
      </label>
      {!isConnected && <p role="status" className="muted">{t('network.panel.connect')}{' '}{report && t('network.panel.lastSample')}</p>}
      {error && <p role="alert" className="connection-test-result failed">{error === 'network.panel.error' ? t('network.panel.error') : error} {report && t('network.panel.lastSample')}</p>}
      {loading && !report && <p role="status" className="muted">{t('network.panel.checkingConnections')}</p>}
      {report && <>
        <div className="network-summary" aria-live="polite">
          <strong>{t('network.panel.reachable', { healthy, total: report.endpoints.length })}</strong>
          <span className="muted text-sm">{t('network.panel.checked', { time: formatDateTime(report.checkedAt) })}</span>
        </div>
        <div className="network-table-scroll">
          <table className="network-table">
            <thead><tr><th>{t('network.panel.providerRoute')}</th><th>{t('network.panel.dns')}</th><th>{t('network.panel.ping')}</th><th>{t('network.panel.tcp')}</th><th>{t('network.panel.directHttp')}</th><th>{t('network.panel.providerHttp')}</th></tr></thead>
            <tbody>{report.endpoints.map(endpoint => (
              <tr key={endpoint.provider}>
                <td>
                  <strong>{endpoint.provider}</strong>
                  <small className="muted network-url">{endpoint.url}</small>
                  <small className="muted network-url">{endpoint.proxy ? t('network.panel.proxy', { proxy: endpoint.proxy }) : t('network.panel.systemProxy')}</small>
                  <LatencyHistory history={history} provider={endpoint.provider} />
                </td>
                <td><ProbeValue probe={endpoint.dns} /><small className="muted network-url">{endpoint.addresses.join(', ')}</small></td>
                <td><ProbeValue probe={endpoint.ping} /></td>
                <td><ProbeValue probe={endpoint.tcp} /></td>
                <td><ProbeValue probe={endpoint.directHttp} /></td>
                <td><ProbeValue probe={endpoint.providerHttp} /></td>
              </tr>
            ))}</tbody>
          </table>
        </div>
        <p className="muted text-sm">{t('network.panel.latencyHint')}</p>
        <h4>{t('network.panel.proxies')}</h4>
        {report.proxies.length === 0 ? <p className="muted text-sm">{t('network.panel.noProxies')}</p> : (
          <div className="network-table-scroll">
            <table className="network-table">
              <thead><tr><th>{t('network.panel.sourceAddress')}</th><th>{t('network.panel.proxyTcp')}</th><th>{t('network.panel.proxyHttp')}</th></tr></thead>
              <tbody>{report.proxies.map(proxy => (
                <tr key={proxy.source}>
                  <td><strong>{proxy.source}</strong><small className="muted network-url">{proxy.url}</small></td>
                  <td><ProbeValue probe={proxy.tcp} /></td>
                  <td><ProbeValue probe={proxy.tunnel} /><small className="muted network-url">{t('network.panel.target', { target: proxy.target })}</small></td>
                </tr>
              ))}</tbody>
            </table>
          </div>
        )}
        {report.noProxy && <p className="muted text-sm network-url">{t('network.panel.noProxy', { value: report.noProxy })}</p>}
        <p className="muted text-sm">{t('network.panel.retentionHint')}</p>
      </>}
    </section>
  )
}
