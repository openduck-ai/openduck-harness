import { useEffect, useRef, useState } from 'react'
import type { createApi, MemoryLog, MemoryLogEntry } from '@aaif/goose-hub-core'
import { RefreshCw, ScrollText } from 'lucide-react'

interface Props {
  api: ReturnType<typeof createApi>
  isConnected: boolean
}

function entryDetails(entry: MemoryLogEntry): string | null {
  const parts: string[] = []
  if (entry.availableMib != null && entry.limitMib != null) {
    parts.push(`${entry.availableMib} / ${entry.limitMib} MiB`)
  }
  if (entry.processRssMib != null) parts.push(`RSS ${entry.processRssMib} MiB`)
  if (entry.cgroupLimited) parts.push('cgroup')
  if (entry.pausedTasks != null) parts.push(`${entry.pausedTasks} tasks paused`)
  if (entry.stoppedSchedules != null) parts.push(`${entry.stoppedSchedules} schedules stopped`)
  return parts.length > 0 ? parts.join(' · ') : null
}

export function MemoryMonitorPanel({ api, isConnected }: Props) {
  const [log, setLog] = useState<MemoryLog | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [autoRefresh, setAutoRefresh] = useState(true)
  const [refresh, setRefresh] = useState(0)
  const controller = useRef<AbortController | null>(null)

  useEffect(() => {
    setLog(null)
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
        const next = await api.getMemoryLog(100, request.signal)
        if (disposed) return
        setLog(next)
        setError(null)
      } catch (cause) {
        if (!disposed && !request.signal.aborted) {
          setError(cause instanceof Error ? cause.message : 'Memory monitor log failed')
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

  const entries = log ? [...log.entries].reverse() : []

  return (
    <section className="panel memory-log-panel" aria-labelledby="memory-log-title">
      <div className="network-panel-heading">
        <h3 id="memory-log-title"><ScrollText size={18} /> Memory Monitor</h3>
        <button
          type="button"
          className="secondary"
          disabled={loading || !isConnected}
          onClick={() => setRefresh(value => value + 1)}
        >
          <RefreshCw size={14} /> {loading ? 'Loading…' : 'Refresh'}
        </button>
      </div>
      <p className="muted text-sm">Recent samples, pauses, recoveries, and alert emails from the harness memory monitor.</p>
      <label className="network-auto-refresh">
        <input type="checkbox" checked={autoRefresh} onChange={event => setAutoRefresh(event.target.checked)} />
        Refresh every 30 seconds while this page is visible
      </label>
      {!isConnected && <p role="status" className="muted">Connect to the server to read its memory monitor log. {log && 'Showing the last successful read.'}</p>}
      {error && <p role="alert" className="connection-test-result failed">{error} {log && 'Showing the last successful read.'}</p>}
      {loading && !log && <p role="status" className="muted">Reading the memory monitor log…</p>}
      {log && <>
        <p className={`memory-log-state ${log.blocked ? 'paused' : 'open'}`} aria-live="polite">
          <span className="status-dot" />
          {log.blocked ? 'Tasks paused until memory recovers' : 'Accepting new tasks'}
        </p>
        <p className="muted text-sm memory-log-path">{log.path}</p>
        {entries.length === 0 ? (
          <p className="muted text-sm">No memory monitor events yet. They appear after openduck serve starts the monitor.</p>
        ) : (
          <ol className="memory-log-list" aria-label="Recent memory monitor events">
            {entries.map((entry, index) => {
              const details = entryDetails(entry)
              return (
                <li key={`${entry.at}-${entry.event}-${index}`} className={`memory-log-entry memory-log-level-${entry.level}`}>
                  <time dateTime={entry.at}>{new Date(entry.at).toLocaleString()}</time>
                  <span className="memory-log-event">{entry.event}</span>
                  <span>
                    {entry.message}
                    {details && <small className="muted">{details}</small>}
                  </span>
                </li>
              )
            })}
          </ol>
        )}
        <p className="muted text-sm">{entries.length} recent events, newest first. The log keeps the latest 500 lines.</p>
      </>}
    </section>
  )
}
