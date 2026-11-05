import { formatDateTime } from '../i18n/format.ts'
import type { TFunction } from 'i18next'
import { useTranslation } from 'react-i18next'
import { useEffect, useRef, useState } from 'react'
import type { createApi, MemoryLog, MemoryLogEntry } from '@aaif/goose-hub-core'
import { RefreshCw, ScrollText } from 'lucide-react'

interface Props {
  api: ReturnType<typeof createApi>
  isConnected: boolean
}

function entryDetails(entry: MemoryLogEntry, t: TFunction): string | null {
  const parts: string[] = []
  if (entry.availableMib != null && entry.limitMib != null) {
    parts.push(t('memory.panel.capacity', { available: entry.availableMib, limit: entry.limitMib }))
  }
  if (entry.processRssMib != null) parts.push(t('memory.panel.rss', { value: entry.processRssMib }))
  if (entry.cgroupLimited) parts.push('cgroup')
  if (entry.pausedTasks != null) parts.push(t('memory.panel.pausedTasks', { count: entry.pausedTasks }))
  if (entry.stoppedSchedules != null) parts.push(t('memory.panel.stoppedSchedules', { count: entry.stoppedSchedules }))
  return parts.length > 0 ? parts.join(' · ') : null
}

export function MemoryMonitorPanel({ api, isConnected }: Props) {
  const { t } = useTranslation()
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
          setError(cause instanceof Error ? cause.message : 'memory.panel.error')
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
        <h3 id="memory-log-title"><ScrollText size={18} /> {t('memory.panel.title')}</h3>
        <button
          type="button"
          className="secondary"
          disabled={loading || !isConnected}
          onClick={() => setRefresh(value => value + 1)}
        >
          <RefreshCw size={14} /> {loading ? t('memory.panel.loading') : t('memory.panel.refresh')}
        </button>
      </div>
      <p className="muted text-sm">{t('memory.panel.description')}</p>
      <label className="network-auto-refresh">
        <input type="checkbox" checked={autoRefresh} onChange={event => setAutoRefresh(event.target.checked)} />
        {t('memory.panel.autoRefresh')}
      </label>
      {!isConnected && <p role="status" className="muted">{t('memory.panel.connect')}{' '}{log && t('memory.panel.lastRead')}</p>}
      {error && <p role="alert" className="connection-test-result failed">{error === 'memory.panel.error' ? t('memory.panel.error') : error} {log && t('memory.panel.lastRead')}</p>}
      {loading && !log && <p role="status" className="muted">{t('memory.panel.reading')}</p>}
      {log && <>
        <p className={`memory-log-state ${log.blocked ? 'paused' : 'open'}`} aria-live="polite">
          <span className="status-dot" />
          {log.blocked ? t('memory.panel.paused') : t('memory.panel.accepting')}
        </p>
        <p className="muted text-sm memory-log-path">{log.path}</p>
        {entries.length === 0 ? (
          <p className="muted text-sm">{t('memory.panel.empty')}</p>
        ) : (
          <ol className="memory-log-list" aria-label={t('memory.panel.events')}>
            {entries.map((entry, index) => {
              const details = entryDetails(entry, t)
              return (
                <li key={`${entry.at}-${entry.event}-${index}`} className={`memory-log-entry memory-log-level-${entry.level}`}>
                  <time dateTime={entry.at}>{formatDateTime(entry.at)}</time>
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
        <p className="muted text-sm">{t('memory.panel.summary', { count: entries.length })}</p>
      </>}
    </section>
  )
}
