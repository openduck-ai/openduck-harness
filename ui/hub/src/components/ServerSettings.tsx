import { useTranslation } from 'react-i18next'
import { useState } from 'react'
import type { createApi } from '@aaif/goose-hub-core'
import { Sun, Moon, Laptop } from 'lucide-react'
import { useTheme } from '../contexts/ThemeContext'
import { OpenDuckConfigPanel } from './OpenDuckConfigPanel'
import { ProjectRootsEditor } from './ProjectRootsEditor'
import { useProjectRoots } from './useProjectRoots'
import { MemoryMonitorPanel } from './MemoryMonitorPanel'
import { NetworkStatusPanel } from './NetworkStatusPanel'

interface ServerSettingsProps {
  baseUrl: string
  secret: string
  onSaveConnection: (url: string, secret: string) => void
  isConnected: boolean
  api: ReturnType<typeof createApi>
  totalProjects: number
  activeSessionsCount: number
}

export function ServerSettings({
  baseUrl,
  secret,
  onSaveConnection,
  isConnected,
  api,
  totalProjects,
  activeSessionsCount,
}: ServerSettingsProps) {
  const { t } = useTranslation()
  const { themePreference, setThemePreference, resolvedTheme } = useTheme()
  const projectRoots = useProjectRoots(api)
  const [inputUrl, setInputUrl] = useState(baseUrl)
  const [inputSecret, setInputSecret] = useState(secret)
  const [requestTimeoutSecs, setRequestTimeoutSecs] = useState<number>(() => {
    try {
      const stored = localStorage.getItem('openduck.api.timeoutMs')
      if (stored) {
        const val = parseInt(stored, 10)
        if (!Number.isNaN(val) && val >= 0) return Math.round(val / 1000)
      }
    } catch {}
    return 12
  })
  const [historyTimeoutSecs, setHistoryTimeoutSecs] = useState<number>(() => {
    try {
      const stored = localStorage.getItem('openduck.harness.historyTimeoutMs')
      if (stored) {
        const val = parseInt(stored, 10)
        if (!Number.isNaN(val) && val >= 0) return Math.round(val / 1000)
      }
    } catch {}
    return 60
  })
  const [testing, setTesting] = useState(false)
  const [pingLatency, setPingLatency] = useState<number | null>(null)
  const [testResult, setTestResult] = useState<{ success: boolean; error?: string } | null>(null)
  const [savedSuccess, setSavedSuccess] = useState(false)

  const handleSave = () => {
    onSaveConnection(inputUrl, inputSecret)
    try {
      localStorage.setItem('openduck.api.timeoutMs', String(requestTimeoutSecs * 1000))
      localStorage.setItem('openduck.harness.historyTimeoutMs', String(historyTimeoutSecs * 1000))
    } catch {}
    setSavedSuccess(true)
    setTimeout(() => setSavedSuccess(false), 3000)
  }

  const handleTestConnection = async () => {
    setTesting(true)
    setTestResult(null)
    setPingLatency(null)
    const startTime = performance.now()
    try {
      await api.listProjects()
      const latency = Math.round(performance.now() - startTime)
      setPingLatency(latency)
      setTestResult({
        success: true,
      })
    } catch (cause) {
      setTestResult({
        success: false,
        error: cause instanceof Error ? cause.message : undefined,
      })
    } finally {
      setTesting(false)
    }
  }

  return (
    <div className="management-page-container">
      <div className="management-page-header">
        <div>
          <p className="eyebrow">{t('settings.server.eyebrow')}</p>
          <h2>{t('settings.server.title')}</h2>
          <p className="muted">
            {t('settings.server.description')}
          </p>
        </div>
      </div>

      <div className="columns settings-layout">
        <div className="panel-column">
          <div className="panel form-panel">
            <h3>{t('settings.server.connectionTitle')}</h3>
            <p className="muted">
              {t('settings.server.connectionHint')}
            </p>

            <div className="settings-field-group">
              <label>
                {t('settings.server.serverUrl')}
                <input
                  value={inputUrl}
                  onChange={e => setInputUrl(e.target.value)}
                  placeholder={t('settings.server.serverUrlPlaceholder')}
                />
                <small className="muted">{t('settings.server.serverUrlDefault')}</small>
              </label>

              <label>
                {t('settings.server.secretKey')}
                <input
                  type="password"
                  value={inputSecret}
                  onChange={e => setInputSecret(e.target.value)}
                  placeholder={t('settings.server.secretPlaceholder')}
                />
              </label>

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '0.75rem', marginTop: '0.25rem' }}>
                <label>
                  {t('settings.server.requestTimeout')}
                  <input
                    type="number"
                    min={0}
                    max={300}
                    value={requestTimeoutSecs}
                    onChange={e => setRequestTimeoutSecs(Math.max(0, parseInt(e.target.value, 10) || 0))}
                    placeholder={t('settings.server.requestTimeoutPlaceholder')}
                  />
                  <small className="muted">{t('settings.server.requestTimeoutHint')}</small>
                </label>
                <label>
                  {t('settings.server.historyTimeout')}
                  <input
                    type="number"
                    min={0}
                    max={600}
                    value={historyTimeoutSecs}
                    onChange={e => setHistoryTimeoutSecs(Math.max(0, parseInt(e.target.value, 10) || 0))}
                    placeholder={t('settings.server.historyTimeoutPlaceholder')}
                  />
                  <small className="muted">{t('settings.server.historyTimeoutHint')}</small>
                </label>
              </div>

              {savedSuccess && <div className="toast-inline">{t('settings.server.saved')}</div>}

              <div className="settings-actions-row">
                <button type="button" className="btn-primary" onClick={handleSave}>
                  {t('settings.server.save')}
                </button>
                <button
                  type="button"
                  className="secondary"
                  disabled={testing}
                  onClick={() => void handleTestConnection()}
                >
                  {testing ? t('settings.server.testing') : t('settings.server.test')}
                </button>
              </div>

              {testResult && (
                <div className={`connection-test-result ${testResult.success ? 'success' : 'failed'}`}>
                  {testResult.success ? '✓ ' : '✕ '} {testResult.success
                    ? t('settings.server.testSuccess', { latency: pingLatency ?? 0 })
                    : testResult.error ?? t('settings.server.testFailed')}
                </div>
              )}
            </div>
          </div>

          <div className="panel">
            <h3>{t('settings.server.rootsTitle')}</h3>
            <p className="muted">
              {t('settings.server.rootsHint')}
            </p>
            <ProjectRootsEditor
              roots={projectRoots.roots}
              loading={projectRoots.loading}
              error={projectRoots.error}
              clearError={projectRoots.clearError}
              onAdd={projectRoots.addRoot}
              onRemove={projectRoots.removeRoot}
            />
          </div>

          <div className="panel">
            <h3>{t('settings.server.appearance')}</h3>
            <p className="muted text-sm">
              {t('settings.server.appearanceHint')}
            </p>
            <div className="theme-options-grid">
              <button
                type="button"
                className={`theme-option-card ${themePreference === 'system' ? 'active' : ''}`}
                onClick={() => setThemePreference('system')}
              >
                <Laptop size={20} className="theme-option-icon" />
                <div className="theme-option-text">
                  <strong>{t('settings.server.system')}</strong>
                  <span className="muted text-xs">{t('settings.server.autoTheme', { theme: resolvedTheme === 'light' ? t('settings.server.themeLight') : t('settings.server.themeDark') })}</span>
                </div>
              </button>
              <button
                type="button"
                className={`theme-option-card ${themePreference === 'light' ? 'active' : ''}`}
                onClick={() => setThemePreference('light')}
              >
                <Sun size={20} className="theme-option-icon" />
                <div className="theme-option-text">
                  <strong>{t('settings.server.light')}</strong>
                  <span className="muted text-xs">{t('settings.server.daylight')}</span>
                </div>
              </button>
              <button
                type="button"
                className={`theme-option-card ${themePreference === 'dark' ? 'active' : ''}`}
                onClick={() => setThemePreference('dark')}
              >
                <Moon size={20} className="theme-option-icon" />
                <div className="theme-option-text">
                  <strong>{t('settings.server.dark')}</strong>
                  <span className="muted text-xs">{t('settings.server.obsidian')}</span>
                </div>
              </button>
            </div>
          </div>
        </div>

        <div className="panel-column">
          <div className="panel">
            <h3>{t('settings.server.diagnostics')}</h3>
            <div className="diagnostics-grid">
              <div className="diagnostic-item">
                <span className="muted text-sm">{t('settings.server.health')}</span>
                <div className="diagnostic-value">
                  <span className={`status-dot ${isConnected ? 'online' : 'offline'}`} />
                  <strong>{isConnected ? t('settings.server.connected') : t('settings.server.disconnected')}</strong>
                </div>
              </div>

              <div className="diagnostic-item">
                <span className="muted text-sm">{t('settings.server.latencyLabel')}</span>
                <div className="diagnostic-value">
                  <strong>{pingLatency !== null ? t('settings.server.latency', { latency: pingLatency }) : isConnected ? t('settings.server.active') : '—'}</strong>
                </div>
              </div>

              <div className="diagnostic-item">
                <span className="muted text-sm">{t('settings.server.projects')}</span>
                <div className="diagnostic-value">
                  <strong>{totalProjects}</strong>
                </div>
              </div>

              <div className="diagnostic-item">
                <span className="muted text-sm">{t('settings.server.sessions')}</span>
                <div className="diagnostic-value">
                  <strong>{activeSessionsCount}</strong>
                </div>
              </div>
            </div>
          </div>

          <div className="panel">
            <h3>{t('settings.server.about')}</h3>
            <div className="about-details">
              <p className="muted text-sm">
                {t('settings.server.aboutDescription')}
              </p>
              <div className="tags">
                <span>{t('settings.server.uiVersion')}</span>
                <span>{t('settings.server.acpVersion')}</span>
                <span>{t('settings.server.stack')}</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <NetworkStatusPanel api={api} isConnected={isConnected} />
      <MemoryMonitorPanel api={api} isConnected={isConnected} />
      <OpenDuckConfigPanel api={api} isConnected={isConnected} />
    </div>
  )
}

