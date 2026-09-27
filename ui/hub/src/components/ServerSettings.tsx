import { useState } from 'react'
import type { createApi } from '@aaif/goose-hub-core'
import { Sun, Moon, Laptop } from 'lucide-react'
import { useTheme } from '../contexts/ThemeContext'
import { ProjectRootsEditor } from './ProjectRootsEditor'
import { useProjectRoots } from './useProjectRoots'

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
  const { themePreference, setThemePreference, resolvedTheme } = useTheme()
  const projectRoots = useProjectRoots(api)
  const [inputUrl, setInputUrl] = useState(baseUrl)
  const [inputSecret, setInputSecret] = useState(secret)
  const [testing, setTesting] = useState(false)
  const [pingLatency, setPingLatency] = useState<number | null>(null)
  const [testResult, setTestResult] = useState<{ success: boolean; message: string } | null>(null)
  const [savedSuccess, setSavedSuccess] = useState(false)

  const handleSave = () => {
    onSaveConnection(inputUrl, inputSecret)
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
        message: `Successfully connected to Goose server (${latency}ms latency).`,
      })
    } catch (cause) {
      setTestResult({
        success: false,
        message: cause instanceof Error ? cause.message : 'Connection test failed',
      })
    } finally {
      setTesting(false)
    }
  }

  return (
    <div className="management-page-container">
      <div className="management-page-header">
        <div>
          <p className="eyebrow">SYSTEM CONFIGURATION</p>
          <h2>Server & Settings</h2>
          <p className="muted">
            Configure connection endpoints, appearance preferences, security credentials, and view system diagnostics.
          </p>
        </div>
      </div>

      <div className="columns settings-layout">
        <div className="panel-column">
          <div className="panel form-panel">
            <h3>Goose Server Connection</h3>
            <p className="muted">
              Specify the address and authentication key for your running Goose background daemon.
            </p>

            <div className="settings-field-group">
              <label>
                Server URL
                <input
                  value={inputUrl}
                  onChange={e => setInputUrl(e.target.value)}
                  placeholder="Leave empty for local proxy (current origin)"
                />
                <small className="muted">Default: empty (uses Vite proxy / current origin)</small>
              </label>

              <label>
                Secret Key
                <input
                  type="password"
                  value={inputSecret}
                  onChange={e => setInputSecret(e.target.value)}
                  placeholder="Optional (if configured on server via GOOSE_SERVER__SECRET_KEY)"
                />
              </label>

              {savedSuccess && <div className="toast-inline">✓ Connection settings saved!</div>}

              <div className="settings-actions-row">
                <button type="button" className="btn-primary" onClick={handleSave}>
                  Save Connection
                </button>
                <button
                  type="button"
                  className="secondary"
                  disabled={testing}
                  onClick={() => void handleTestConnection()}
                >
                  {testing ? 'Testing…' : '⚡ Test Connection / Ping'}
                </button>
              </div>

              {testResult && (
                <div className={`connection-test-result ${testResult.success ? 'success' : 'failed'}`}>
                  {testResult.success ? '✓ ' : '✕ '} {testResult.message}
                </div>
              )}
            </div>
          </div>

          <div className="panel">
            <h3>Project Root Directories</h3>
            <p className="muted">
              New projects can only be registered as a folder inside one of these roots. Add the
              parent directories where your projects live.
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
            <h3>Appearance & Theme</h3>
            <p className="muted text-sm">
              Choose your interface color scheme. You can switch between Light, Dark, or System mode.
            </p>
            <div className="theme-options-grid">
              <button
                type="button"
                className={`theme-option-card ${themePreference === 'system' ? 'active' : ''}`}
                onClick={() => setThemePreference('system')}
              >
                <Laptop size={20} className="theme-option-icon" />
                <div className="theme-option-text">
                  <strong>System</strong>
                  <span className="muted text-xs">Auto ({resolvedTheme})</span>
                </div>
              </button>
              <button
                type="button"
                className={`theme-option-card ${themePreference === 'light' ? 'active' : ''}`}
                onClick={() => setThemePreference('light')}
              >
                <Sun size={20} className="theme-option-icon" />
                <div className="theme-option-text">
                  <strong>Light</strong>
                  <span className="muted text-xs">Daylight theme</span>
                </div>
              </button>
              <button
                type="button"
                className={`theme-option-card ${themePreference === 'dark' ? 'active' : ''}`}
                onClick={() => setThemePreference('dark')}
              >
                <Moon size={20} className="theme-option-icon" />
                <div className="theme-option-text">
                  <strong>Dark</strong>
                  <span className="muted text-xs">Obsidian dark</span>
                </div>
              </button>
            </div>
          </div>
        </div>

        <div className="panel-column">
          <div className="panel">
            <h3>System Status & Diagnostics</h3>
            <div className="diagnostics-grid">
              <div className="diagnostic-item">
                <span className="muted text-sm">Server Health</span>
                <div className="diagnostic-value">
                  <span className={`status-dot ${isConnected ? 'online' : 'offline'}`} />
                  <strong>{isConnected ? 'Connected & Ready' : 'Disconnected'}</strong>
                </div>
              </div>

              <div className="diagnostic-item">
                <span className="muted text-sm">Ping Latency</span>
                <div className="diagnostic-value">
                  <strong>{pingLatency !== null ? `${pingLatency} ms` : isConnected ? 'Active' : '—'}</strong>
                </div>
              </div>

              <div className="diagnostic-item">
                <span className="muted text-sm">Registered Projects</span>
                <div className="diagnostic-value">
                  <strong>{totalProjects}</strong>
                </div>
              </div>

              <div className="diagnostic-item">
                <span className="muted text-sm">Active Live Sessions</span>
                <div className="diagnostic-value">
                  <strong>{activeSessionsCount}</strong>
                </div>
              </div>
            </div>
          </div>

          <div className="panel">
            <h3>About OpenDuck Hub</h3>
            <div className="about-details">
              <p className="muted text-sm">
                OpenDuck Hub is a project-first enterprise agent management interface for Goose. It
                enables managing projects, monitoring scheduled automation recipes, executing shell
                commands, and coordinating parallel agent sessions.
              </p>
              <div className="tags">
                <span>UI v0.1.0</span>
                <span>ACP Protocol v0.19</span>
                <span>Vite + React 19</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

