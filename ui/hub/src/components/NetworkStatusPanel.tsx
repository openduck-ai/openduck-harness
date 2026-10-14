import { useEffect, useRef, useState } from 'react'
import type { createApi, NetworkProbe, NetworkStatus } from '@aaif/goose-hub-core'
import { Activity, RefreshCw } from 'lucide-react'

interface Props {
  api: ReturnType<typeof createApi>
  isConnected: boolean
}

function ProbeValue({ probe }: { probe: NetworkProbe }) {
  return (
    <div className={`network-probe network-probe-${probe.status}`} title={probe.detail}>
      <span className={`status-dot ${probe.status === 'reachable' ? 'online' : 'offline'}`} />
      <span>
        <strong>{probe.latencyMs === null ? probe.status : `${probe.latencyMs.toFixed(1)} ms`}</strong>
        <small className="muted">{probe.detail}</small>
      </span>
    </div>
  )
}

function LatencyHistory({ history, provider }: { history: NetworkStatus[]; provider: string }) {
  const samples = history.map(sample => ({
    time: sample.checkedAt,
    probe: sample.endpoints.find(endpoint => endpoint.provider === provider)?.providerHttp,
  }))
  const maximum = Math.max(1, ...samples.map(sample => sample.probe?.latencyMs ?? 0))
  const failures = samples.filter(sample => sample.probe && sample.probe.status !== 'reachable').length
  return (
    <div className="network-history">
      <div className="network-history-bars" aria-label={`${provider} recent HTTP latency samples`}>
        {samples.map(sample => (
          <span
            key={sample.time}
            className={sample.probe?.status === 'reachable' ? 'network-history-ok' : 'network-history-failed'}
            style={{ height: `${Math.max(8, ((sample.probe?.latencyMs ?? maximum) / maximum) * 100)}%` }}
            title={`${new Date(sample.time).toLocaleTimeString()}: ${sample.probe?.detail ?? 'No sample'}${sample.probe?.latencyMs != null ? ` · ${sample.probe.latencyMs.toFixed(1)} ms` : ''}`}
          />
        ))}
      </div>
      <small className="muted">{samples.length} recent samples · {failures} connection failures</small>
    </div>
  )
}

export function NetworkStatusPanel({ api, isConnected }: Props) {
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
          setError(cause instanceof Error ? cause.message : 'Network monitoring failed')
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
    <section className="panel network-panel" aria-labelledby="network-status-title">
      <div className="network-panel-heading">
        <h3 id="network-status-title"><Activity size={18} /> Network Status</h3>
        <button
          type="button"
          className="secondary"
          disabled={loading || !isConnected}
          onClick={() => setRefresh(value => value + 1)}
        >
          <RefreshCw size={14} /> {loading ? 'Checking…' : 'Refresh'}
        </button>
      </div>
      <p className="muted text-sm">Live checks from the OpenDuck server to provider endpoints.</p>
      <label className="network-auto-refresh">
        <input type="checkbox" checked={autoRefresh} onChange={event => setAutoRefresh(event.target.checked)} />
        Refresh every 30 seconds while this page is visible
      </label>
      {!isConnected && <p role="status" className="muted">Connect to the server to monitor its network. {report && 'Showing the last successful sample.'}</p>}
      {error && <p role="alert" className="connection-test-result failed">{error} {report && 'Showing the last successful sample.'}</p>}
      {loading && !report && <p role="status" className="muted">Checking DNS, ping, and proxy connections…</p>}
      {report && <>
        <div className="network-summary" aria-live="polite">
          <strong>{healthy}/{report.endpoints.length} provider routes reachable</strong>
          <span className="muted text-sm">Checked {new Date(report.checkedAt).toLocaleString()}</span>
        </div>
        <div className="network-table-scroll">
          <table className="network-table">
            <thead><tr><th>Provider / route</th><th>DNS</th><th>ICMP ping</th><th>Direct TCP</th><th>Direct HTTP</th><th>Provider HTTP</th></tr></thead>
            <tbody>{report.endpoints.map(endpoint => (
              <tr key={endpoint.provider}>
                <td>
                  <strong>{endpoint.provider}</strong>
                  <small className="muted network-url">{endpoint.url}</small>
                  <small className="muted network-url">{endpoint.proxy ? `Proxy: ${endpoint.proxy}` : 'System proxy settings'}</small>
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
        <p className="muted text-sm">HTTP latency includes DNS, connection, TLS, and response headers. Any HTTP response confirms reachability; it does not verify API credentials or subscriptions. ICMP may be blocked even when HTTP works.</p>
        <h4>Proxies</h4>
        {report.proxies.length === 0 ? <p className="muted text-sm">No named, provider, or environment proxies configured.</p> : (
          <div className="network-table-scroll">
            <table className="network-table">
              <thead><tr><th>Source / address</th><th>Proxy TCP</th><th>HTTP through proxy</th></tr></thead>
              <tbody>{report.proxies.map(proxy => (
                <tr key={proxy.source}>
                  <td><strong>{proxy.source}</strong><small className="muted network-url">{proxy.url}</small></td>
                  <td><ProbeValue probe={proxy.tcp} /></td>
                  <td><ProbeValue probe={proxy.tunnel} /><small className="muted network-url">Target: {proxy.target}</small></td>
                </tr>
              ))}</tbody>
            </table>
          </div>
        )}
        {report.noProxy && <p className="muted text-sm network-url">NO_PROXY bypass: {report.noProxy}</p>}
        <p className="muted text-sm">Samples are shared for 30 seconds. Recent history is retained only while this panel is open. Proxy credentials are hidden.</p>
      </>}
    </section>
  )
}
