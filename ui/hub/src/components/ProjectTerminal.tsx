import { useState, useMemo, useEffect, useCallback } from 'react'
import type { KeyboardEvent } from 'react'
import type {
  createApi,
  ExecCommandOutput,
  TuiSessionSummary,
  AgentCliKind,
} from '@aaif/goose-hub-core'

interface ProjectTerminalProps {
  api: ReturnType<typeof createApi>
  slug: string
  rootPath: string
  language?: string
}

type TerminalMode = 'shell' | 'tui_agents'

export function ProjectTerminal({
  api,
  slug,
  rootPath,
  language,
}: ProjectTerminalProps) {
  const [mode, setMode] = useState<TerminalMode>('shell')

  // --- Shell Runner State ---
  const [command, setCommand] = useState('')
  const [history, setHistory] = useState<
    Array<{
      cmd: string
      output?: ExecCommandOutput
      error?: string
      timestamp: string
    }>
  >([])
  const [historyIndex, setHistoryIndex] = useState(-1)
  const [savedCommandHistory, setSavedCommandHistory] = useState<string[]>([])
  const [running, setRunning] = useState(false)

  // --- TUI Agents State ---
  const [tuiSessions, setTuiSessions] = useState<TuiSessionSummary[]>([])
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null)
  const [activeScreen, setActiveScreen] = useState<string>('')
  const [activeResponse, setActiveResponse] = useState<string>('')
  const [viewMode, setViewMode] = useState<'screen' | 'response'>('screen')
  const [tuiPrompt, setTuiPrompt] = useState('')
  const [tuiBusy, setTuiBusy] = useState(false)
  const [autoRefresh, setAutoRefresh] = useState(true)
  const [copiedAttach, setCopiedAttach] = useState(false)

  const quickPresets = useMemo(() => {
    const common = ['git status', 'git diff', 'ls -la']
    const lang = (language ?? '').toLowerCase()
    if (lang.includes('rust')) {
      return [...common, 'cargo check', 'cargo test']
    }
    if (
      lang.includes('node') ||
      lang.includes('typescript') ||
      lang.includes('javascript') ||
      lang.includes('react')
    ) {
      return [...common, 'npm test', 'npm run build']
    }
    if (lang.includes('python')) {
      return [...common, 'pytest', 'python3 -m unittest']
    }
    if (lang.includes('go')) {
      return [...common, 'go test ./...', 'go build']
    }
    return common
  }, [language])

  const execute = async (cmdToRun: string) => {
    const trimmed = cmdToRun.trim()
    if (!trimmed || running) return

    setRunning(true)
    setSavedCommandHistory(prev => [trimmed, ...prev.filter(c => c !== trimmed)])
    setHistoryIndex(-1)
    setCommand('')

    const timestamp = new Date().toLocaleTimeString()
    try {
      const output = await api.execCommand(slug, { command: trimmed })
      setHistory(prev => [...prev, { cmd: trimmed, output, timestamp }])
    } catch (cause) {
      setHistory(prev => [
        ...prev,
        {
          cmd: trimmed,
          error: cause instanceof Error ? cause.message : 'Execution failed',
          timestamp,
        },
      ])
    } finally {
      setRunning(false)
    }
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      if (savedCommandHistory.length === 0) return
      const nextIndex = Math.min(historyIndex + 1, savedCommandHistory.length - 1)
      setHistoryIndex(nextIndex)
      setCommand(savedCommandHistory[nextIndex] ?? '')
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      const nextIndex = historyIndex - 1
      if (nextIndex < 0) {
        setHistoryIndex(-1)
        setCommand('')
      } else {
        setHistoryIndex(nextIndex)
        setCommand(savedCommandHistory[nextIndex] ?? '')
      }
    }
  }

  // --- TUI Session Management ---
  const fetchTuiSessions = useCallback(async () => {
    try {
      const sessions = await api.listTuiSessions(slug, true)
      setTuiSessions(sessions)
      if (sessions.length > 0 && (!activeSessionId || !sessions.some(s => s.sessionId === activeSessionId))) {
        setActiveSessionId(sessions[0].sessionId)
      }
    } catch (err) {
      console.warn('Failed to list TUI sessions:', err)
    }
  }, [api, slug, activeSessionId])

  const pollActiveScreen = useCallback(async () => {
    if (!activeSessionId) return
    try {
      const turn = await api.readTuiSession(slug, activeSessionId, 120, false)
      setActiveScreen(turn.screenSnapshot)
      if (turn.response) {
        setActiveResponse(turn.response)
      }
    } catch (err) {
      console.warn('Failed to read TUI screen:', err)
    }
  }, [api, slug, activeSessionId])

  useEffect(() => {
    if (mode === 'tui_agents') {
      void fetchTuiSessions()
    }
  }, [mode, fetchTuiSessions])

  useEffect(() => {
    if (mode !== 'tui_agents' || !activeSessionId || !autoRefresh) return
    void pollActiveScreen()
    const timer = setInterval(() => {
      void pollActiveScreen()
    }, 1500)
    return () => clearInterval(timer)
  }, [mode, activeSessionId, autoRefresh, pollActiveScreen])

  const handleStartAgent = async (agent: AgentCliKind) => {
    setTuiBusy(true)
    try {
      const res = await api.startTuiSession(slug, {
        agent,
        autoApprove: true,
      })
      setActiveSessionId(res.sessionId)
      setActiveScreen(res.initialScreen)
      await fetchTuiSessions()
    } catch (err) {
      alert(`Failed to start ${agent} session: ${err instanceof Error ? err.message : String(err)}`)
    } finally {
      setTuiBusy(false)
    }
  }

  const handleSendPrompt = async () => {
    const trimmed = tuiPrompt.trim()
    if (!trimmed || !activeSessionId || tuiBusy) return

    setTuiBusy(true)
    setTuiPrompt('')
    try {
      const turn = await api.promptTuiSession(slug, activeSessionId, {
        prompt: trimmed,
        wait: true,
      })
      setActiveScreen(turn.screenSnapshot)
      if (turn.response) {
        setActiveResponse(turn.response)
        setViewMode('response')
      }
      await fetchTuiSessions()
    } catch (err) {
      alert(`Prompt error: ${err instanceof Error ? err.message : String(err)}`)
    } finally {
      setTuiBusy(false)
    }
  }

  const handleSendKeys = async (keys: string[]) => {
    if (!activeSessionId || tuiBusy) return
    try {
      await api.keysTuiSession(slug, activeSessionId, keys)
      setTimeout(() => void pollActiveScreen(), 300)
    } catch (err) {
      console.error('Keys error:', err)
    }
  }

  const handleStopActiveSession = async () => {
    if (!activeSessionId) return
    if (!confirm(`Are you sure you want to stop session ${activeSessionId}?`)) return

    try {
      await api.stopTuiSession(slug, activeSessionId)
      setActiveSessionId(null)
      setActiveScreen('')
      setActiveResponse('')
      await fetchTuiSessions()
    } catch (err) {
      alert(`Failed to stop session: ${err instanceof Error ? err.message : String(err)}`)
    }
  }

  const copyAttachCommand = () => {
    const session = tuiSessions.find(s => s.sessionId === activeSessionId)
    if (!session) return
    navigator.clipboard.writeText(session.attachCommand).then(() => {
      setCopiedAttach(true)
      setTimeout(() => setCopiedAttach(false), 2000)
    })
  }

  const activeSession = tuiSessions.find(s => s.sessionId === activeSessionId)

  return (
    <div className="panel terminal-panel">
      <div className="terminal-header">
        <div className="terminal-info">
          <h3>Terminal & Agent TUIs</h3>
          <span className="terminal-cwd">{rootPath}</span>
        </div>
        <div className="terminal-actions" style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
          <div style={{ display: 'flex', background: 'var(--bg-chip)', borderRadius: '6px', padding: '2px' }}>
            <button
              type="button"
              className={`mini-btn ${mode === 'shell' ? 'primary' : 'secondary'}`}
              style={{ borderRadius: '4px', border: 'none' }}
              onClick={() => setMode('shell')}
            >
              Shell Runner
            </button>
            <button
              type="button"
              className={`mini-btn ${mode === 'tui_agents' ? 'primary' : 'secondary'}`}
              style={{ borderRadius: '4px', border: 'none' }}
              onClick={() => setMode('tui_agents')}
            >
              Interactive TUIs (tmux)
            </button>
          </div>
          {mode === 'shell' && (
            <button type="button" className="secondary mini-btn" onClick={() => setHistory([])}>
              Clear Output
            </button>
          )}
        </div>
      </div>

      {mode === 'shell' ? (
        <>
          <div className="quick-presets">
            <span className="presets-label">Quick actions:</span>
            {quickPresets.map(preset => (
              <button
                key={preset}
                type="button"
                className="preset-chip"
                disabled={running}
                onClick={() => void execute(preset)}
              >
                {preset}
              </button>
            ))}
          </div>

          <div className="terminal-console">
            {history.length === 0 && (
              <p className="terminal-empty">
                Terminal ready. Run commands in project root or click quick presets above.
              </p>
            )}
            {history.map((entry, idx) => {
              const isSuccess = entry.output?.success ?? false
              const exitCode = entry.output?.exitCode
              const duration = entry.output?.durationMs

              return (
                <div key={idx} className="terminal-entry">
                  <div className="terminal-entry-meta">
                    <span className="terminal-prompt">$ {entry.cmd}</span>
                    <div className="terminal-badges">
                      {duration !== undefined && <span className="duration-tag">{duration}ms</span>}
                      {exitCode !== undefined && (
                        <span className={`exit-badge ${isSuccess ? 'exit-ok' : 'exit-err'}`}>
                          {isSuccess ? '0' : `exit ${exitCode}`}
                        </span>
                      )}
                      <span className="time-tag">{entry.timestamp}</span>
                    </div>
                  </div>
                  {entry.output?.stdout && (
                    <pre className="terminal-stdout">{entry.output.stdout}</pre>
                  )}
                  {entry.output?.stderr && (
                    <pre className="terminal-stderr">{entry.output.stderr}</pre>
                  )}
                  {entry.error && <pre className="terminal-stderr">Error: {entry.error}</pre>}
                </div>
              )
            })}
            {running && (
              <div className="terminal-running">
                <span className="badge working">⚡ Executing command…</span>
              </div>
            )}
          </div>

          <form
            className="terminal-input-bar"
            onSubmit={e => {
              e.preventDefault()
              void execute(command)
            }}
          >
            <span className="terminal-input-prompt">$</span>
            <input
              autoFocus
              className="terminal-input"
              value={command}
              onChange={e => setCommand(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Type shell command (e.g. git status, cargo test) and press Enter…"
              disabled={running}
            />
            <button type="submit" disabled={!command.trim() || running}>
              {running ? 'Running…' : 'Run'}
            </button>
          </form>
        </>
      ) : (
        /* --- Interactive Agent TUI Sessions Mode --- */
        <div style={{ display: 'flex', flexDirection: 'column', gap: '0.75rem' }}>
          {/* Launch bar */}
          <div className="quick-presets" style={{ alignItems: 'center' }}>
            <span className="presets-label">Launch persistent agent:</span>
            <button
              type="button"
              className="preset-chip"
              disabled={tuiBusy}
              onClick={() => void handleStartAgent('agy')}
              style={{ fontWeight: 600, color: '#38bdf8' }}
            >
              ⚡ agy (Antigravity)
            </button>
            <button
              type="button"
              className="preset-chip"
              disabled={tuiBusy}
              onClick={() => void handleStartAgent('grok')}
              style={{ fontWeight: 600, color: '#fbbf24' }}
            >
              ⚡ grok (Grok Build)
            </button>
            <button
              type="button"
              className="preset-chip"
              disabled={tuiBusy}
              onClick={() => void handleStartAgent('codex')}
              style={{ fontWeight: 600, color: '#34d399' }}
            >
              ⚡ codex (OpenAI)
            </button>
            <button
              type="button"
              className="preset-chip"
              disabled={tuiBusy}
              onClick={() => void handleStartAgent('claude')}
              style={{ fontWeight: 600, color: '#f472b6' }}
            >
              ⚡ claude (Claude Code)
            </button>
          </div>

          {/* Sessions selector tabs */}
          {tuiSessions.length > 0 && (
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', borderBottom: '1px solid var(--border-default)', paddingBottom: '0.5rem', flexWrap: 'wrap', gap: '0.5rem' }}>
              <div style={{ display: 'flex', gap: '0.35rem', overflowX: 'auto', alignItems: 'center' }}>
                <span style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>Sessions:</span>
                {tuiSessions.map(sess => {
                  const isActive = sess.sessionId === activeSessionId
                  const status = sess.state.status
                  const statusColor =
                    status === 'working' ? '#38bdf8' :
                    status === 'waiting_approval' ? '#fbbf24' :
                    status === 'idle_prompt' ? '#34d399' : '#94a3b8'

                  return (
                    <button
                      key={sess.sessionId}
                      type="button"
                      className={`preset-chip ${isActive ? 'active' : ''}`}
                      onClick={() => setActiveSessionId(sess.sessionId)}
                      style={{
                        borderColor: isActive ? 'var(--border-focus)' : undefined,
                        background: isActive ? 'var(--bg-chip-hover)' : undefined,
                        display: 'flex',
                        alignItems: 'center',
                        gap: '0.35rem',
                      }}
                    >
                      <span style={{ width: 8, height: 8, borderRadius: '50%', background: statusColor, display: 'inline-block' }} />
                      <span>{sess.kind}: {sess.sessionId.slice(-8)}</span>
                      {sess.pid && <span style={{ opacity: 0.6, fontSize: '0.7rem' }}>PID {sess.pid}</span>}
                    </button>
                  )
                })}
              </div>

              {activeSession && (
                <div style={{ display: 'flex', gap: '0.4rem', alignItems: 'center' }}>
                  <button
                    type="button"
                    className="preset-chip"
                    onClick={copyAttachCommand}
                    title="Copy command to attach live via tmux in your terminal"
                    style={{ fontSize: '0.72rem' }}
                  >
                    {copiedAttach ? '✓ Copied attach command' : `📋 ${activeSession.attachCommand}`}
                  </button>
                  <button
                    type="button"
                    className="preset-chip"
                    onClick={() => setAutoRefresh(!autoRefresh)}
                    style={{ opacity: autoRefresh ? 1 : 0.6 }}
                  >
                    {autoRefresh ? '🔄 Auto-sync: ON' : '⏸ Auto-sync: OFF'}
                  </button>
                  <button
                    type="button"
                    className="preset-chip"
                    onClick={() => void handleStopActiveSession()}
                    style={{ color: '#f87171' }}
                  >
                    Stop
                  </button>
                </div>
              )}
            </div>
          )}

          {/* View toggle (Screen vs Response) */}
          {activeResponse && (
            <div style={{ display: 'flex', gap: '0.5rem', marginBottom: '-0.25rem' }}>
              <button
                type="button"
                className={`mini-btn ${viewMode === 'screen' ? 'primary' : 'secondary'}`}
                onClick={() => setViewMode('screen')}
              >
                TUI Screen Grid
              </button>
              <button
                type="button"
                className={`mini-btn ${viewMode === 'response' ? 'primary' : 'secondary'}`}
                onClick={() => setViewMode('response')}
              >
                Lossless Extracted Response
              </button>
            </div>
          )}

          {/* Terminal Console Viewport */}
          <div className="terminal-console" style={{ position: 'relative', minHeight: 400, maxHeight: 560, overflowY: 'auto' }}>
            {!activeSessionId ? (
              <p className="terminal-empty">
                No active agent TUI sessions. Launch agy, grok, codex, or claude above to start a persistent warm tmux session.
              </p>
            ) : viewMode === 'response' && activeResponse ? (
              <div style={{ padding: '0.75rem', color: 'var(--terminal-text)', fontFamily: 'var(--font-mono)', fontSize: '0.85rem', whiteSpace: 'pre-wrap', lineHeight: 1.5 }}>
                {activeResponse}
              </div>
            ) : (
              <pre
                className="terminal-stdout"
                style={{ margin: 0, padding: '0.75rem', fontFamily: 'var(--font-mono)', fontSize: '0.82rem', whiteSpace: 'pre', overflowX: 'auto', lineHeight: 1.35 }}
              >
                {activeScreen || '(Loading terminal screen grid…)'}
              </pre>
            )}

            {tuiBusy && (
              <div className="terminal-running" style={{ position: 'absolute', bottom: 10, right: 10 }}>
                <span className="badge working">⚡ Injecting prompt & waiting for turn…</span>
              </div>
            )}
          </div>

          {/* Quick Keys Control Bar */}
          {activeSessionId && (
            <div style={{ display: 'flex', gap: '0.35rem', flexWrap: 'wrap', alignItems: 'center' }}>
              <span className="presets-label">Keystrokes:</span>
              <button type="button" className="preset-chip" onClick={() => void handleSendKeys(['Escape'])}>
                Esc
              </button>
              <button type="button" className="preset-chip" onClick={() => void handleSendKeys(['Enter'])}>
                Enter ↵
              </button>
              <button type="button" className="preset-chip" onClick={() => void handleSendKeys(['y', 'Enter'])} style={{ color: '#34d399' }}>
                y (Approve)
              </button>
              <button type="button" className="preset-chip" onClick={() => void handleSendKeys(['n', 'Enter'])} style={{ color: '#f87171' }}>
                n (Deny)
              </button>
              <button type="button" className="preset-chip" onClick={() => void handleSendKeys(['C-c'])}>
                Ctrl+C
              </button>
              <button type="button" className="preset-chip" onClick={() => void handleSendKeys(['/compact', 'Enter'])}>
                /compact
              </button>
              <button type="button" className="preset-chip" onClick={() => void handleSendKeys(['/status', 'Enter'])}>
                /status
              </button>
              <button type="button" className="preset-chip" onClick={() => void handleSendKeys(['/plan', 'Enter'])}>
                /plan
              </button>
            </div>
          )}

          {/* Prompt composer */}
          {activeSessionId && (
            <form
              className="terminal-input-bar"
              onSubmit={e => {
                e.preventDefault()
                void handleSendPrompt()
              }}
            >
              <span className="terminal-input-prompt" style={{ color: '#38bdf8' }}>&gt;</span>
              <input
                className="terminal-input"
                value={tuiPrompt}
                onChange={e => setTuiPrompt(e.target.value)}
                placeholder="Type prompt or slash command (/plan, /compact, /status) for agent TUI..."
                disabled={tuiBusy}
              />
              <button type="submit" disabled={!tuiPrompt.trim() || tuiBusy}>
                {tuiBusy ? 'Waiting…' : 'Send Prompt'}
              </button>
            </form>
          )}
        </div>
      )}
    </div>
  )
}
