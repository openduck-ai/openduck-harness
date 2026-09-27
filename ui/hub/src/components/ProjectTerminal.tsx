import { useState, useMemo } from 'react'
import type { KeyboardEvent } from 'react'
import type { createApi, ExecCommandOutput } from '@aaif/goose-hub-core'

interface ProjectTerminalProps {
  api: ReturnType<typeof createApi>
  slug: string
  rootPath: string
  language?: string
}

export function ProjectTerminal({
  api,
  slug,
  rootPath,
  language,
}: ProjectTerminalProps) {
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

  const clear = () => {
    setHistory([])
  }

  return (
    <div className="panel terminal-panel">
      <div className="terminal-header">
        <div className="terminal-info">
          <h3>Terminal</h3>
          <span className="terminal-cwd">{rootPath}</span>
        </div>
        <div className="terminal-actions">
          <button type="button" className="secondary mini-btn" onClick={clear}>
            Clear Output
          </button>
        </div>
      </div>

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
    </div>
  )
}
