import { useState, useEffect, useCallback, useMemo } from 'react'
import type { createApi, ManagedTuiSession, SessionOperation, AgentProfile, TuiAccount, QuotaSnapshot, ProxyItem } from '../api'
import {
  Terminal,
  Activity,
  History,
  ListOrdered,
  Cpu,
  CreditCard,
  RefreshCw,
  Shield,
  AlertCircle,
  Plus,
  Play,
  Globe,
} from 'lucide-react'

interface TuiAgentsPageProps {
  api: ReturnType<typeof createApi>
}

type TabType = 'overview' | 'running' | 'history' | 'operations' | 'agents' | 'quotas'

export function TuiAgentsPage({ api }: TuiAgentsPageProps) {
  const [activeTab, setActiveTab] = useState<TabType>('running')
  const [sessions, setSessions] = useState<ManagedTuiSession[]>([])
  const [operations, setOperations] = useState<SessionOperation[]>([])
  const [agents, setAgents] = useState<AgentProfile[]>([])
  const [accounts, setAccounts] = useState<TuiAccount[]>([])
  const [quotas, setQuotas] = useState<Record<string, QuotaSnapshot[]>>({})
  const [proxies, setProxies] = useState<ProxyItem[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Dialog & Inspection states
  const [inspectSession, setInspectSession] = useState<ManagedTuiSession | null>(null)
  const [inspectScreen, setInspectScreen] = useState<string>('')
  const [promptModalSession, setPromptModalSession] = useState<ManagedTuiSession | null>(null)
  const [promptText, setPromptText] = useState('')
  const [promptSending, setPromptSending] = useState(false)
  const [keysModalSession, setKeysModalSession] = useState<ManagedTuiSession | null>(null)
  const [keysInput, setKeysInput] = useState('')
  const [transferModalSession, setTransferModalSession] = useState<ManagedTuiSession | null>(null)
  const [transferRunId, setTransferRunId] = useState('')
  const [editProfileModal, setEditProfileModal] = useState<AgentProfile | null>(null)
  const [newAccountModal, setNewAccountModal] = useState(false)
  const [newAccountProvider, setNewAccountProvider] = useState('')
  const [newAccountName, setNewAccountName] = useState('')

  // New session modal states
  const [newSessionModal, setNewSessionModal] = useState(false)
  const [newSessionAgent, setNewSessionAgent] = useState('agy')
  const [newSessionCustomAgent, setNewSessionCustomAgent] = useState('')
  const [newSessionName, setNewSessionName] = useState('')
  const [newSessionCwd, setNewSessionCwd] = useState('')
  const [newSessionModel, setNewSessionModel] = useState('')
  const [newSessionEffort, setNewSessionEffort] = useState('')
  const [newSessionProxyMode, setNewSessionProxyMode] = useState('inherit')
  const [newSessionProxyId, setNewSessionProxyId] = useState('')
  const [newSessionInitialPrompt, setNewSessionInitialPrompt] = useState('')
  const [newSessionAutoApprove, setNewSessionAutoApprove] = useState(true)
  const [newSessionStarting, setNewSessionStarting] = useState(false)

  const loadData = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [sessRes, opsRes, agRes, accRes, proxiesRes] = await Promise.all([
        api.listManagedTuiSessions({ limit: 200 }),
        api.listManagedTuiOperations({ limit: 100 }),
        api.listTuiAgents(),
        api.listTuiAccounts(),
        typeof (api as any).getProxiesConfig === 'function'
          ? (api as any).getProxiesConfig().catch(() => ({ proxies: [] }))
          : Promise.resolve({ proxies: [] }),
      ])
      setSessions(sessRes.sessions || [])
      setOperations(opsRes.operations || [])
      setAgents(agRes.agents || [])
      setAccounts(accRes.accounts || [])
      setProxies(proxiesRes.proxies || [])

      // Load quotas for accounts
      const quotasMap: Record<string, QuotaSnapshot[]> = {}
      for (const acc of accRes.accounts || []) {
        try {
          const qRes = await api.listTuiQuotas(acc.id)
          quotasMap[acc.id] = qRes.quotas || []
        } catch (_) {}
      }
      setQuotas(quotasMap)
    } catch (e: any) {
      setError(e.message || 'Failed to load TUI agents data')
    } finally {
      setLoading(false)
    }
  }, [api])

  useEffect(() => {
    loadData()
    const timer = setInterval(() => {
      loadData()
    }, 10000)
    return () => clearInterval(timer)
  }, [loadData])

  // Split sessions into running/active and historical
  const runningSessions = useMemo(() => {
    return sessions.filter(
      s => s.lifecycleState === 'active' || s.lifecycleState === 'starting' || s.lifecycleState === 'retained' || s.lifecycleState === 'reclaim_pending',
    )
  }, [sessions])

  const historicalSessions = useMemo(() => {
    return sessions.filter(
      s => s.lifecycleState === 'reclaimed' || s.lifecycleState === 'stopping' || s.lifecycleState === 'cleanup_failed',
    )
  }, [sessions])

  // Metrics for overview
  const metrics = useMemo(() => {
    const working = runningSessions.filter(s => s.tuiPaneState === 'working').length
    const waiting = runningSessions.filter(s => s.tuiPaneState.includes('waiting') || s.tuiPaneState.includes('approval')).length
    const idle = runningSessions.filter(s => s.tuiPaneState === 'idle_prompt' || s.tuiPaneState === 'ready').length
    const retained = runningSessions.filter(s => s.lifecycleState === 'retained' || s.lifecycleState === 'reclaim_pending').length
    return {
      activeTotal: runningSessions.length,
      working,
      waiting,
      idle,
      retained,
      historicalTotal: historicalSessions.length,
      agentsCount: agents.length,
      accountsCount: accounts.length,
    }
  }, [runningSessions, historicalSessions, agents, accounts])

  const handleInterrupt = async (id: string) => {
    try {
      await api.interruptManagedTuiSession(id)
      loadData()
    } catch (e: any) {
      alert(`Interrupt failed: ${e.message}`)
    }
  }

  const handleStop = async (id: string) => {
    if (!confirm('Are you sure you want to stop this session?')) return
    try {
      await api.stopManagedTuiSession(id, 'manual_stop')
      loadData()
    } catch (e: any) {
      alert(`Stop failed: ${e.message}`)
    }
  }

  const handleExtend = async (id: string) => {
    try {
      await api.extendManagedTuiSession(id, 30)
      loadData()
    } catch (e: any) {
      alert(`Extend failed: ${e.message}`)
    }
  }

  const handleTakeover = async (id: string) => {
    try {
      await api.takeoverManagedTuiSession(id)
      loadData()
    } catch (e: any) {
      alert(`Takeover failed: ${e.message}`)
    }
  }

  const handleTransfer = async () => {
    if (!transferModalSession || !transferRunId.trim()) return
    try {
      await api.transferManagedTuiSession(transferModalSession.id, transferRunId.trim())
      setTransferModalSession(null)
      setTransferRunId('')
      loadData()
    } catch (e: any) {
      alert(`Transfer failed: ${e.message}`)
    }
  }

  const handleSendPrompt = async () => {
    if (!promptModalSession || !promptText.trim()) return
    setPromptSending(true)
    try {
      await api.promptTuiSession(promptModalSession.sessionName, { prompt: promptText.trim(), wait: false })
      setPromptModalSession(null)
      setPromptText('')
      loadData()
    } catch (e: any) {
      alert(`Send prompt failed: ${e.message}`)
    } finally {
      setPromptSending(false)
    }
  }

  const handleSendKeys = async () => {
    if (!keysModalSession || !keysInput.trim()) return
    try {
      const keys = keysInput.trim().split(/\s+/)
      await api.keysTuiSession(keysModalSession.sessionName, { keys })
      setKeysModalSession(null)
      setKeysInput('')
      loadData()
    } catch (e: any) {
      alert(`Send keys failed: ${e.message}`)
    }
  }

  const handleOpenScreen = async (session: ManagedTuiSession) => {
    setInspectSession(session)
    setInspectScreen('Loading terminal screen...')
    try {
      const res = await api.getManagedTuiSession(session.id)
      setInspectScreen(res.screen || 'No screen output available')
    } catch (e: any) {
      setInspectScreen(`Error reading terminal: ${e.message}`)
    }
  }

  const handleSaveProfile = async (profile: AgentProfile) => {
    try {
      await api.upsertTuiAgent(profile)
      setEditProfileModal(null)
      loadData()
    } catch (e: any) {
      alert(`Save profile failed: ${e.message}`)
    }
  }

  const handleCreateAccount = async () => {
    if (!newAccountProvider.trim() || !newAccountName.trim()) return
    try {
      await api.upsertTuiAccount({
        provider: newAccountProvider.trim(),
        accountName: newAccountName.trim(),
        status: 'active',
      })
      setNewAccountModal(false)
      setNewAccountProvider('')
      setNewAccountName('')
      loadData()
    } catch (e: any) {
      alert(`Create account failed: ${e.message}`)
    }
  }

  const handleRefreshQuotas = async (accountId: string) => {
    try {
      await api.refreshTuiQuotas(accountId)
      loadData()
    } catch (e: any) {
      alert(`Refresh quotas failed: ${e.message}`)
    }
  }

  const resolveProxyDisplay = (proxyMode?: string, proxyId?: string, effectiveProxy?: string) => {
    if (effectiveProxy) {
      const match = proxies.find(p => p.id === effectiveProxy || p.url === effectiveProxy)
      return match ? `${match.id} (${match.url})` : effectiveProxy
    }
    if (proxyMode === 'direct') return 'Direct (No Proxy)'
    if (proxyMode === 'custom') return proxyId ? `Custom (${proxyId})` : 'Custom URL'
    if (proxyMode === 'named' && proxyId) {
      const match = proxies.find(p => p.id === proxyId)
      return match ? `${match.id} (${match.url})` : `Named: ${proxyId}`
    }
    return 'Inherit (Global/Default)'
  }

  const handleStartSession = async () => {
    const agentKind = newSessionAgent === 'custom' ? newSessionCustomAgent.trim() : newSessionAgent
    if (!agentKind) {
      alert('Please specify an agent to start')
      return
    }
    setNewSessionStarting(true)
    try {
      const payload: any = {
        agent: agentKind,
        autoApprove: newSessionAutoApprove,
      }
      if (newSessionName.trim()) payload.sessionName = newSessionName.trim()
      if (newSessionCwd.trim()) payload.cwd = newSessionCwd.trim()
      if (newSessionModel.trim()) payload.model = newSessionModel.trim()
      if (newSessionEffort.trim()) payload.effort = newSessionEffort.trim()
      if (newSessionInitialPrompt.trim()) payload.initialPrompt = newSessionInitialPrompt.trim()

      if (newSessionProxyMode === 'direct') {
        payload.proxyMode = 'direct'
      } else if (newSessionProxyMode === 'named' && newSessionProxyId.trim()) {
        payload.proxyMode = 'named'
        payload.proxyId = newSessionProxyId.trim()
      } else if (newSessionProxyMode === 'custom' && newSessionProxyId.trim()) {
        payload.proxyMode = 'custom'
        payload.proxyId = newSessionProxyId.trim()
        payload.httpsProxy = newSessionProxyId.trim()
      } else {
        payload.proxyMode = 'inherit'
      }

      await (api as any).startManagedTuiSession(payload)
      setNewSessionModal(false)
      setNewSessionName('')
      setNewSessionCwd('')
      setNewSessionModel('')
      setNewSessionEffort('')
      setNewSessionInitialPrompt('')
      loadData()
    } catch (e: any) {
      alert(`Start session failed: ${e.message}`)
    } finally {
      setNewSessionStarting(false)
    }
  }

  return (
    <div className="tui-agents-page" style={{ padding: '24px', maxWidth: '1400px', margin: '0 auto' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
        <div>
          <h2 style={{ margin: 0, display: 'flex', alignItems: 'center', gap: '8px', fontSize: '22px' }}>
            <Terminal size={24} />
            <span>TUI Agent Unified Management</span>
          </h2>
          <p style={{ margin: '4px 0 0 0', color: 'var(--text-muted, #888)', fontSize: '14px' }}>
            Track interactive agent CLI sessions, attribution, quotas, operations ledger, and automated lifecycle.
          </p>
        </div>
        <div style={{ display: 'flex', gap: '8px' }}>
          <button
            type="button"
            onClick={() => setNewSessionModal(true)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '6px',
              padding: '8px 14px',
              borderRadius: '6px',
              border: 'none',
              background: '#0066cc',
              color: '#fff',
              cursor: 'pointer',
              fontWeight: 500,
            }}
          >
            <Plus size={14} />
            <span>Launch Agent</span>
          </button>
          <button
            type="button"
            onClick={loadData}
            disabled={loading}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '6px',
              padding: '8px 14px',
              borderRadius: '6px',
              border: '1px solid var(--border-color, #ccc)',
              background: 'var(--bg-card, #fff)',
              cursor: 'pointer',
            }}
          >
            <RefreshCw size={14} className={loading ? 'spin' : ''} />
            <span>Refresh</span>
          </button>
        </div>
      </div>

      {error && (
        <div style={{ padding: '12px 16px', marginBottom: '16px', background: '#ffebee', color: '#c62828', borderRadius: '6px', display: 'flex', alignItems: 'center', gap: '8px' }}>
          <AlertCircle size={18} />
          <span>{error}</span>
        </div>
      )}

      {/* Navigation Tabs */}
      <div style={{ display: 'flex', gap: '4px', borderBottom: '1px solid var(--border-color, #e0e0e0)', marginBottom: '20px' }}>
        {[
          { key: 'running', label: `Running Sessions (${runningSessions.length})`, icon: Activity },
          { key: 'overview', label: 'Overview', icon: Shield },
          { key: 'history', label: `Session History (${historicalSessions.length})`, icon: History },
          { key: 'operations', label: `Operations Ledger (${operations.length})`, icon: ListOrdered },
          { key: 'agents', label: `Agent Profiles (${agents.length})`, icon: Cpu },
          { key: 'quotas', label: `Accounts & Quotas (${accounts.length})`, icon: CreditCard },
        ].map(tab => {
          const Icon = tab.icon
          const isActive = activeTab === tab.key
          return (
            <button
              key={tab.key}
              type="button"
              onClick={() => setActiveTab(tab.key as TabType)}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
                padding: '10px 18px',
                border: 'none',
                borderBottom: isActive ? '2px solid var(--primary, #0066cc)' : '2px solid transparent',
                background: 'transparent',
                fontWeight: isActive ? 600 : 400,
                color: isActive ? 'var(--primary, #0066cc)' : 'inherit',
                cursor: 'pointer',
              }}
            >
              <Icon size={16} />
              <span>{tab.label}</span>
            </button>
          )
        })}
      </div>

      {/* Tab: Overview */}
      {activeTab === 'overview' && (
        <div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: '16px', marginBottom: '24px' }}>
            <div style={{ padding: '16px', borderRadius: '8px', background: 'var(--bg-card, #f9f9f9)', border: '1px solid var(--border-color, #eee)' }}>
              <div style={{ fontSize: '13px', color: 'var(--text-muted, #777)' }}>Active Sessions</div>
              <div style={{ fontSize: '28px', fontWeight: 700, margin: '4px 0' }}>{metrics.activeTotal}</div>
              <div style={{ fontSize: '12px', color: '#2e7d32' }}>{metrics.working} working, {metrics.idle} idle</div>
            </div>
            <div style={{ padding: '16px', borderRadius: '8px', background: 'var(--bg-card, #f9f9f9)', border: '1px solid var(--border-color, #eee)' }}>
              <div style={{ fontSize: '13px', color: 'var(--text-muted, #777)' }}>Waiting Approval</div>
              <div style={{ fontSize: '28px', fontWeight: 700, margin: '4px 0', color: metrics.waiting > 0 ? '#ed6c02' : 'inherit' }}>
                {metrics.waiting}
              </div>
              <div style={{ fontSize: '12px', color: 'var(--text-muted, #777)' }}>Needs keys / user confirmation</div>
            </div>
            <div style={{ padding: '16px', borderRadius: '8px', background: 'var(--bg-card, #f9f9f9)', border: '1px solid var(--border-color, #eee)' }}>
              <div style={{ fontSize: '13px', color: 'var(--text-muted, #777)' }}>Retained / Pending Reclaim</div>
              <div style={{ fontSize: '28px', fontWeight: 700, margin: '4px 0', color: metrics.retained > 0 ? '#0288d1' : 'inherit' }}>
                {metrics.retained}
              </div>
              <div style={{ fontSize: '12px', color: 'var(--text-muted, #777)' }}>10m grace period after run end</div>
            </div>
            <div style={{ padding: '16px', borderRadius: '8px', background: 'var(--bg-card, #f9f9f9)', border: '1px solid var(--border-color, #eee)' }}>
              <div style={{ fontSize: '13px', color: 'var(--text-muted, #777)' }}>Configured Agents & Accounts</div>
              <div style={{ fontSize: '28px', fontWeight: 700, margin: '4px 0' }}>{metrics.agentsCount} / {metrics.accountsCount}</div>
              <div style={{ fontSize: '12px', color: 'var(--text-muted, #777)' }}>Profiles & Provider pools</div>
            </div>
          </div>

          <div style={{ padding: '20px', borderRadius: '8px', background: 'var(--bg-card, #f9f9f9)', border: '1px solid var(--border-color, #eee)' }}>
            <h3 style={{ margin: '0 0 12px 0', fontSize: '16px' }}>Lifecycle & Reclamation Policy Summary</h3>
            <ul style={{ margin: 0, paddingLeft: '20px', lineHeight: '1.8', fontSize: '14px', color: 'var(--text-muted, #555)' }}>
              <li><strong>Run Stop / Cancel:</strong> Immediately interrupts the assigned TUI session and terminates tmux pane within 30 seconds.</li>
              <li><strong>Run Success:</strong> Preserves idle session for 10 minutes for review or follow-up before auto-reclaiming.</li>
              <li><strong>Run Failure:</strong> Freezes static terminal snapshot and retains for 10 minutes for inspection.</li>
              <li><strong>Manual / Idle Sessions:</strong> Reclaimed automatically after 30 minutes of inactivity unless extended.</li>
              <li><strong>Ownership:</strong> Sessions are bound to immutable task runs, preventing rogue turns after run completion.</li>
            </ul>
          </div>
        </div>
      )}

      {/* Tab: Running Sessions */}
      {activeTab === 'running' && (
        <div>
          {runningSessions.length === 0 ? (
            <div style={{ padding: '40px', textAlign: 'center', color: 'var(--text-muted, #888)' }}>
              <Terminal size={48} style={{ opacity: 0.3, marginBottom: '12px' }} />
              <div>No active TUI agent sessions currently running.</div>
            </div>
          ) : (
            <div style={{ overflowX: 'auto' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '13px' }}>
                <thead>
                  <tr style={{ borderBottom: '2px solid var(--border-color, #eee)', color: 'var(--text-muted, #777)' }}>
                    <th style={{ padding: '10px 12px' }}>Session / Agent</th>
                    <th style={{ padding: '10px 12px' }}>Attribution</th>
                    <th style={{ padding: '10px 12px' }}>Model & Proxy</th>
                    <th style={{ padding: '10px 12px' }}>State</th>
                    <th style={{ padding: '10px 12px' }}>Last Op / Activity</th>
                    <th style={{ padding: '10px 12px' }}>Reclaim Countdown</th>
                    <th style={{ padding: '10px 12px' }}>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {runningSessions.map(s => {
                    const isWorking = s.tuiPaneState === 'working'
                    const isWaiting = s.tuiPaneState.includes('waiting') || s.tuiPaneState.includes('approval')
                    return (
                      <tr key={s.id} style={{ borderBottom: '1px solid var(--border-color, #f0f0f0)' }}>
                        <td style={{ padding: '12px' }}>
                          <div style={{ fontWeight: 600 }}>{s.sessionName}</div>
                          <div style={{ fontSize: '11px', color: 'var(--text-muted, #888)' }}>
                            CLI: <span style={{ textTransform: 'uppercase' }}>{s.agentKind}</span> (id: {s.id.slice(0, 10)}...)
                          </div>
                        </td>
                        <td style={{ padding: '12px' }}>
                          <div>Project: {s.createdByProjectId || 'Global'}</div>
                          <div style={{ fontSize: '11px', color: 'var(--text-muted, #888)' }}>
                            Owner Run: {s.ownerRunId ? `${s.ownerRunId.slice(0, 14)}...` : 'Manual'} ({s.ownershipMode})
                          </div>
                        </td>
                        <td style={{ padding: '12px' }}>
                          <div>{s.model || 'Default'}</div>
                          <div style={{ fontSize: '11px', color: 'var(--text-muted, #888)' }}>
                            Proxy: {s.effectiveProxy || s.proxyMode}
                          </div>
                        </td>
                        <td style={{ padding: '12px' }}>
                          <span
                            style={{
                              display: 'inline-block',
                              padding: '2px 8px',
                              borderRadius: '4px',
                              fontSize: '11px',
                              fontWeight: 600,
                              background: isWorking ? '#e8f5e9' : isWaiting ? '#fff3e0' : '#e3f2fd',
                              color: isWorking ? '#2e7d32' : isWaiting ? '#e65100' : '#0277bd',
                            }}
                          >
                            {s.tuiPaneState} ({s.lifecycleState})
                          </span>
                        </td>
                        <td style={{ padding: '12px' }}>
                          <div style={{ fontSize: '12px' }}>{s.lastOperationAt ? new Date(s.lastOperationAt).toLocaleTimeString() : 'None'}</div>
                          <div style={{ fontSize: '11px', color: 'var(--text-muted, #888)' }}>
                            Input: {s.lastInputAt ? new Date(s.lastInputAt).toLocaleTimeString() : 'N/A'}
                          </div>
                        </td>
                        <td style={{ padding: '12px' }}>
                          {s.reclaimAfter ? (
                            <span style={{ color: '#d32f2f', fontWeight: 500 }}>
                              {new Date(s.reclaimAfter).toLocaleTimeString()}
                            </span>
                          ) : (
                            <span style={{ color: 'var(--text-muted, #888)' }}>30m idle sweep</span>
                          )}
                        </td>
                        <td style={{ padding: '12px' }}>
                          <div style={{ display: 'flex', gap: '6px', flexWrap: 'wrap' }}>
                            <button
                              type="button"
                              onClick={() => handleOpenScreen(s)}
                              title="View Screen"
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              Screen
                            </button>
                            <button
                              type="button"
                              onClick={() => setPromptModalSession(s)}
                              title="Send Prompt"
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              Prompt
                            </button>
                            <button
                              type="button"
                              onClick={() => setKeysModalSession(s)}
                              title="Send Keys"
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              Keys
                            </button>
                            <button
                              type="button"
                              onClick={() => handleExtend(s.id)}
                              title="Extend 30m"
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              +30m
                            </button>
                            <button
                              type="button"
                              onClick={() => handleInterrupt(s.id)}
                              title="Interrupt (C-c)"
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ff9800', color: '#e65100' }}
                            >
                              Interrupt
                            </button>
                            <button
                              type="button"
                              onClick={() => handleTakeover(s.id)}
                              title="Manual Takeover"
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              Takeover
                            </button>
                            <button
                              type="button"
                              onClick={() => {
                                setTransferModalSession(s)
                                setTransferRunId('')
                              }}
                              title="Transfer to Run"
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              Transfer
                            </button>
                            <button
                              type="button"
                              onClick={() => handleStop(s.id)}
                              title="Stop Session"
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #f44336', color: '#d32f2f' }}
                            >
                              Stop
                            </button>
                          </div>
                        </td>
                      </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* Tab: Session History */}
      {activeTab === 'history' && (
        <div>
          {historicalSessions.length === 0 ? (
            <div style={{ padding: '40px', textAlign: 'center', color: 'var(--text-muted, #888)' }}>
              <History size={48} style={{ opacity: 0.3, marginBottom: '12px' }} />
              <div>No terminated or reclaimed sessions recorded.</div>
            </div>
          ) : (
            <div style={{ overflowX: 'auto' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '13px' }}>
                <thead>
                  <tr style={{ borderBottom: '2px solid var(--border-color, #eee)', color: 'var(--text-muted, #777)' }}>
                    <th style={{ padding: '10px 12px' }}>Session</th>
                    <th style={{ padding: '10px 12px' }}>Agent</th>
                    <th style={{ padding: '10px 12px' }}>Project / Run</th>
                    <th style={{ padding: '10px 12px' }}>Created At</th>
                    <th style={{ padding: '10px 12px' }}>Reclaimed At</th>
                    <th style={{ padding: '10px 12px' }}>Stop Reason</th>
                    <th style={{ padding: '10px 12px' }}>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {historicalSessions.map(s => (
                    <tr key={s.id} style={{ borderBottom: '1px solid var(--border-color, #f0f0f0)' }}>
                      <td style={{ padding: '12px', fontWeight: 600 }}>{s.sessionName}</td>
                      <td style={{ padding: '12px', textTransform: 'uppercase' }}>{s.agentKind}</td>
                      <td style={{ padding: '12px' }}>
                        <div>{s.createdByProjectId || 'Global'}</div>
                        <div style={{ fontSize: '11px', color: 'var(--text-muted, #888)' }}>{s.createdByRunId || 'Manual'}</div>
                      </td>
                      <td style={{ padding: '12px' }}>{new Date(s.createdAt).toLocaleString()}</td>
                      <td style={{ padding: '12px' }}>{s.reclaimedAt ? new Date(s.reclaimedAt).toLocaleString() : s.stoppedAt ? new Date(s.stoppedAt).toLocaleString() : 'N/A'}</td>
                      <td style={{ padding: '12px' }}>
                        <span style={{ padding: '2px 6px', borderRadius: '4px', background: '#f5f5f5', fontSize: '11px' }}>
                          {s.stopIntent || s.lifecycleState}
                        </span>
                      </td>
                      <td style={{ padding: '12px' }}>
                        <button
                          type="button"
                          onClick={() => handleOpenScreen(s)}
                          style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                        >
                          View Snapshot
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* Tab: Operations Ledger */}
      {activeTab === 'operations' && (
        <div>
          {operations.length === 0 ? (
            <div style={{ padding: '40px', textAlign: 'center', color: 'var(--text-muted, #888)' }}>
              <ListOrdered size={48} style={{ opacity: 0.3, marginBottom: '12px' }} />
              <div>No operations recorded yet.</div>
            </div>
          ) : (
            <div style={{ overflowX: 'auto' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '13px' }}>
                <thead>
                  <tr style={{ borderBottom: '2px solid var(--border-color, #eee)', color: 'var(--text-muted, #777)' }}>
                    <th style={{ padding: '10px 12px' }}>Time</th>
                    <th style={{ padding: '10px 12px' }}>Type</th>
                    <th style={{ padding: '10px 12px' }}>Session</th>
                    <th style={{ padding: '10px 12px' }}>Operator</th>
                    <th style={{ padding: '10px 12px' }}>Run / Task</th>
                    <th style={{ padding: '10px 12px' }}>Status</th>
                    <th style={{ padding: '10px 12px' }}>Duration</th>
                    <th style={{ padding: '10px 12px' }}>Summary</th>
                  </tr>
                </thead>
                <tbody>
                  {operations.map(op => {
                    const isSuccess = op.status === 'succeeded'
                    const isFailed = op.status === 'failed'
                    return (
                      <tr key={op.id} style={{ borderBottom: '1px solid var(--border-color, #f0f0f0)' }}>
                        <td style={{ padding: '10px 12px', whiteSpace: 'nowrap' }}>{new Date(op.startedAt).toLocaleTimeString()}</td>
                        <td style={{ padding: '10px 12px' }}>
                          <span style={{ fontWeight: 600, textTransform: 'uppercase', fontSize: '11px' }}>{op.operationType}</span>
                        </td>
                        <td style={{ padding: '10px 12px' }}>{op.sessionName}</td>
                        <td style={{ padding: '10px 12px' }}>{op.operator}</td>
                        <td style={{ padding: '10px 12px', fontSize: '11px', color: 'var(--text-muted, #888)' }}>
                          {op.runId ? `${op.runId.slice(0, 12)}...` : 'N/A'}
                        </td>
                        <td style={{ padding: '10px 12px' }}>
                          <span
                            style={{
                              display: 'inline-block',
                              padding: '2px 6px',
                              borderRadius: '4px',
                              fontSize: '11px',
                              background: isSuccess ? '#e8f5e9' : isFailed ? '#ffebee' : '#fff3e0',
                              color: isSuccess ? '#2e7d32' : isFailed ? '#c62828' : '#e65100',
                            }}
                          >
                            {op.status}
                          </span>
                        </td>
                        <td style={{ padding: '10px 12px' }}>{op.durationMs !== undefined ? `${op.durationMs}ms` : '-'}</td>
                        <td style={{ padding: '10px 12px', maxWidth: '300px', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {op.summary || '-'}
                        </td>
                      </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* Tab: Agent Profiles */}
      {activeTab === 'agents' && (
        <div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(320px, 1fr))', gap: '16px' }}>
            {Array.from(new Set(['agy', 'grok', 'codex', 'claude', ...agents.map(a => a.agentKind)])).map(kind => {
              const profile = agents.find(p => p.agentKind === kind)
              return (
                <div
                  key={kind}
                  style={{
                    padding: '20px',
                    borderRadius: '8px',
                    background: 'var(--bg-card, #f9f9f9)',
                    border: '1px solid var(--border-color, #eee)',
                  }}
                >
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '12px' }}>
                    <h3 style={{ margin: 0, textTransform: 'uppercase', fontSize: '16px', display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <Cpu size={18} />
                      <span>{kind}</span>
                    </h3>
                    <button
                      type="button"
                      onClick={() =>
                        setEditProfileModal(
                          profile || {
                            id: '',
                            agentKind: kind,
                            defaultModel: '',
                            defaultEffort: '',
                            accountId: '',
                            proxyMode: 'inherit',
                            proxyId: '',
                            createdAt: '',
                            updatedAt: '',
                          },
                        )
                      }
                      style={{ padding: '4px 10px', fontSize: '12px', borderRadius: '4px', border: '1px solid #ccc', cursor: 'pointer' }}
                    >
                      Configure
                    </button>
                  </div>
                  <div style={{ fontSize: '13px', lineHeight: '1.8' }}>
                    <div><strong>Default Model:</strong> {profile?.defaultModel || 'CLI Default'}</div>
                    <div><strong>Default Effort:</strong> {profile?.defaultEffort || 'Default'}</div>
                    <div>
                      <strong>Network Proxy:</strong>{' '}
                      <span style={{ fontFamily: 'monospace' }}>
                        {resolveProxyDisplay(profile?.proxyMode, profile?.proxyId)}
                      </span>
                    </div>
                    <div><strong>Account Binding:</strong> {profile?.accountId || 'None (System)'}</div>
                    <div><strong>Max Concurrency:</strong> {profile?.maxConcurrency || 'Unlimited'}</div>
                    <div><strong>Idle Timeout:</strong> {profile?.idleTimeoutMins ? `${profile.idleTimeoutMins} mins` : '30 mins (default)'}</div>
                  </div>
                </div>
              )
            })}
          </div>
        </div>
      )}

      {/* Tab: Accounts & Quotas */}
      {activeTab === 'quotas' && (
        <div>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px' }}>
            <h3 style={{ margin: 0, fontSize: '16px' }}>Provider Accounts & Quota Pools</h3>
            <button
              type="button"
              onClick={() => setNewAccountModal(true)}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                padding: '6px 12px',
                borderRadius: '4px',
                border: '1px solid #0066cc',
                background: '#0066cc',
                color: '#fff',
                fontSize: '13px',
                cursor: 'pointer',
              }}
            >
              <Plus size={14} />
              <span>Add Account</span>
            </button>
          </div>

          {accounts.length === 0 ? (
            <div style={{ padding: '40px', textAlign: 'center', color: 'var(--text-muted, #888)' }}>
              <CreditCard size={48} style={{ opacity: 0.3, marginBottom: '12px' }} />
              <div>No provider accounts configured yet.</div>
            </div>
          ) : (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(360px, 1fr))', gap: '16px' }}>
              {accounts.map(acc => {
                const accQuotas = quotas[acc.id] || []
                return (
                  <div
                    key={acc.id}
                    style={{
                      padding: '20px',
                      borderRadius: '8px',
                      background: 'var(--bg-card, #f9f9f9)',
                      border: '1px solid var(--border-color, #eee)',
                    }}
                  >
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '12px' }}>
                      <div>
                        <h4 style={{ margin: 0, fontSize: '15px' }}>{acc.accountName}</h4>
                        <div style={{ fontSize: '12px', color: 'var(--text-muted, #888)' }}>Provider: {acc.provider}</div>
                      </div>
                      <button
                        type="button"
                        onClick={() => handleRefreshQuotas(acc.id)}
                        style={{ padding: '4px 8px', fontSize: '11px', borderRadius: '4px', border: '1px solid #ccc', cursor: 'pointer' }}
                      >
                        Refresh Quota
                      </button>
                    </div>

                    {accQuotas.length === 0 ? (
                      <div style={{ fontSize: '12px', color: 'var(--text-muted, #888)', fontStyle: 'italic' }}>
                        No quota snapshots recorded.
                      </div>
                    ) : (
                      <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
                        {accQuotas.map(q => (
                          <div key={q.id} style={{ padding: '8px', background: '#fff', borderRadius: '6px', border: '1px solid #eee' }}>
                            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '12px', marginBottom: '4px' }}>
                              <span style={{ fontWeight: 600 }}>{q.quotaType} quota ({q.unit})</span>
                              <span>{q.remainingPercent !== undefined ? `${q.remainingPercent.toFixed(1)}% left` : 'Unknown'}</span>
                            </div>
                            <div style={{ height: '6px', width: '100%', background: '#eee', borderRadius: '3px', overflow: 'hidden' }}>
                              <div
                                style={{
                                  height: '100%',
                                  width: `${Math.min(100, Math.max(0, q.remainingPercent ?? 100))}%`,
                                  background: (q.remainingPercent ?? 100) > 20 ? '#4caf50' : '#f44336',
                                }}
                              />
                            </div>
                            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '10px', color: '#888', marginTop: '4px' }}>
                              <span>Used: {q.usedAmount}</span>
                              <span>Observed: {new Date(q.observedAt).toLocaleTimeString()}</span>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          )}
        </div>
      )}

      {/* Screen Inspector Modal */}
      {inspectSession && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,0.6)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
        >
          <div
            style={{
              width: '90%',
              maxWidth: '900px',
              maxHeight: '85vh',
              background: '#1e1e1e',
              color: '#fff',
              borderRadius: '8px',
              display: 'flex',
              flexDirection: 'column',
              boxShadow: '0 8px 32px rgba(0,0,0,0.4)',
            }}
          >
            <div style={{ padding: '14px 18px', borderBottom: '1px solid #333', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div style={{ fontWeight: 600 }}>
                Terminal Capture: {inspectSession.sessionName} ({inspectSession.agentKind})
              </div>
              <button
                type="button"
                onClick={() => setInspectSession(null)}
                style={{ background: 'transparent', border: 'none', color: '#aaa', cursor: 'pointer', fontSize: '18px' }}
              >
                ✕
              </button>
            </div>
            <pre
              style={{
                flex: 1,
                padding: '16px',
                margin: 0,
                overflowY: 'auto',
                fontFamily: 'monospace',
                fontSize: '12px',
                lineHeight: '1.4',
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-all',
              }}
            >
              {inspectScreen}
            </pre>
            <div style={{ padding: '12px 18px', borderTop: '1px solid #333', display: 'flex', justifyContent: 'flex-end' }}>
              <button
                type="button"
                onClick={() => setInspectSession(null)}
                style={{ padding: '6px 14px', borderRadius: '4px', border: '1px solid #555', background: '#333', color: '#fff', cursor: 'pointer' }}
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Send Prompt Modal */}
      {promptModalSession && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
        >
          <div style={{ width: '90%', maxWidth: '500px', background: '#fff', borderRadius: '8px', padding: '20px', boxShadow: '0 8px 32px rgba(0,0,0,0.2)' }}>
            <h3 style={{ margin: '0 0 12px 0' }}>Send Prompt to {promptModalSession.sessionName}</h3>
            <textarea
              rows={4}
              value={promptText}
              onChange={e => setPromptText(e.target.value)}
              placeholder="Enter message or prompt to send to the interactive agent..."
              style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
            />
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '16px' }}>
              <button
                type="button"
                onClick={() => setPromptModalSession(null)}
                style={{ padding: '6px 12px', borderRadius: '4px', border: '1px solid #ccc', background: '#fff', cursor: 'pointer' }}
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleSendPrompt}
                disabled={promptSending || !promptText.trim()}
                style={{ padding: '6px 14px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer' }}
              >
                {promptSending ? 'Submitting...' : 'Send Prompt'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Send Keys Modal */}
      {keysModalSession && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
        >
          <div style={{ width: '90%', maxWidth: '400px', background: '#fff', borderRadius: '8px', padding: '20px', boxShadow: '0 8px 32px rgba(0,0,0,0.2)' }}>
            <h3 style={{ margin: '0 0 12px 0' }}>Send Keys to {keysModalSession.sessionName}</h3>
            <p style={{ margin: '0 0 8px 0', fontSize: '13px', color: '#666' }}>
              Common keys: Enter, y, n, Escape, Down, Up, Space
            </p>
            <input
              type="text"
              value={keysInput}
              onChange={e => setKeysInput(e.target.value)}
              placeholder="e.g. Enter or y Enter"
              style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
            />
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '16px' }}>
              <button
                type="button"
                onClick={() => setKeysModalSession(null)}
                style={{ padding: '6px 12px', borderRadius: '4px', border: '1px solid #ccc', background: '#fff', cursor: 'pointer' }}
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleSendKeys}
                disabled={!keysInput.trim()}
                style={{ padding: '6px 14px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer' }}
              >
                Send Keys
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Transfer Ownership Modal */}
      {transferModalSession && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
        >
          <div style={{ width: '90%', maxWidth: '450px', background: '#fff', borderRadius: '8px', padding: '20px', boxShadow: '0 8px 32px rgba(0,0,0,0.2)' }}>
            <h3 style={{ margin: '0 0 12px 0' }}>Transfer Session Ownership</h3>
            <p style={{ margin: '0 0 8px 0', fontSize: '13px', color: '#666' }}>
              Assign session <strong>{transferModalSession.sessionName}</strong> to a target Task Run ID.
            </p>
            <input
              type="text"
              value={transferRunId}
              onChange={e => setTransferRunId(e.target.value)}
              placeholder="Target Run ID (e.g. task_run_12345)"
              style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
            />
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '16px' }}>
              <button
                type="button"
                onClick={() => setTransferModalSession(null)}
                style={{ padding: '6px 12px', borderRadius: '4px', border: '1px solid #ccc', background: '#fff', cursor: 'pointer' }}
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleTransfer}
                disabled={!transferRunId.trim()}
                style={{ padding: '6px 14px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer' }}
              >
                Transfer
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Edit Profile Modal */}
      {editProfileModal && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
        >
          <div style={{ width: '90%', maxWidth: '480px', background: '#fff', borderRadius: '8px', padding: '20px', boxShadow: '0 8px 32px rgba(0,0,0,0.2)' }}>
            <h3 style={{ margin: '0 0 16px 0' }}>Configure Profile: {editProfileModal.agentKind.toUpperCase()}</h3>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', fontSize: '13px' }}>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Default Model</label>
                <input
                  type="text"
                  value={editProfileModal.defaultModel || ''}
                  onChange={e => setEditProfileModal({ ...editProfileModal, defaultModel: e.target.value })}
                  placeholder="e.g. claude-3-7-sonnet or gpt-4o"
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                />
              </div>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Default Effort</label>
                <input
                  type="text"
                  value={editProfileModal.defaultEffort || ''}
                  onChange={e => setEditProfileModal({ ...editProfileModal, defaultEffort: e.target.value })}
                  placeholder="e.g. high / medium / low"
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                />
              </div>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Proxy Mode</label>
                <select
                  value={editProfileModal.proxyMode || 'inherit'}
                  onChange={e => {
                    const nextMode = e.target.value
                    setEditProfileModal({
                      ...editProfileModal,
                      proxyMode: nextMode,
                      proxyId: nextMode === 'named' ? (proxies[0]?.id || '') : nextMode === 'custom' ? (editProfileModal.proxyId || '') : '',
                    })
                  }}
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                >
                  <option value="inherit">Inherit (Global/Default)</option>
                  <option value="direct">Direct (No Proxy)</option>
                  <option value="named">Named Proxy (Configured)</option>
                  <option value="custom">Custom Proxy URL</option>
                </select>
              </div>
              {editProfileModal.proxyMode === 'named' && (
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Configured Proxy</label>
                  {proxies.length > 0 ? (
                    <select
                      value={editProfileModal.proxyId || ''}
                      onChange={e => setEditProfileModal({ ...editProfileModal, proxyId: e.target.value })}
                      style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                    >
                      <option value="">-- Select a proxy --</option>
                      {proxies.map(p => (
                        <option key={p.id} value={p.id}>
                          {p.id} ({p.url}){p.description ? ` - ${p.description}` : ''}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <input
                      type="text"
                      value={editProfileModal.proxyId || ''}
                      onChange={e => setEditProfileModal({ ...editProfileModal, proxyId: e.target.value })}
                      placeholder="e.g. clash"
                      style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                    />
                  )}
                  {proxies.length === 0 && (
                    <small style={{ color: 'var(--text-muted, #888)', display: 'block', marginTop: '4px' }}>
                      No named proxies in Settings → Network Proxies. You can enter a Proxy ID or switch to Custom URL.
                    </small>
                  )}
                </div>
              )}
              {editProfileModal.proxyMode === 'custom' && (
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Custom Proxy URL</label>
                  <input
                    type="text"
                    value={editProfileModal.proxyId || ''}
                    onChange={e => setEditProfileModal({ ...editProfileModal, proxyId: e.target.value })}
                    placeholder="http://127.0.0.1:7890 or socks5://127.0.0.1:1088"
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                  <small style={{ color: 'var(--text-muted, #888)', display: 'block', marginTop: '4px' }}>
                    Directly overrides HTTP_PROXY, HTTPS_PROXY, and ALL_PROXY for this agent.
                  </small>
                </div>
              )}
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Idle Timeout (minutes)</label>
                <input
                  type="number"
                  value={editProfileModal.idleTimeoutMins || ''}
                  onChange={e => setEditProfileModal({ ...editProfileModal, idleTimeoutMins: Number(e.target.value) || undefined })}
                  placeholder="Default 30"
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                />
              </div>
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '20px' }}>
              <button
                type="button"
                onClick={() => setEditProfileModal(null)}
                style={{ padding: '6px 12px', borderRadius: '4px', border: '1px solid #ccc', background: '#fff', cursor: 'pointer' }}
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => handleSaveProfile(editProfileModal)}
                style={{ padding: '6px 14px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer' }}
              >
                Save Profile
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Launch New Session Modal */}
      {newSessionModal && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
        >
          <div style={{ width: '90%', maxWidth: '520px', background: '#fff', borderRadius: '8px', padding: '20px', boxShadow: '0 8px 32px rgba(0,0,0,0.2)' }}>
            <h3 style={{ margin: '0 0 16px 0', display: 'flex', alignItems: 'center', gap: '8px' }}>
              <Play size={18} />
              <span>Launch Persistent TUI Agent Session</span>
            </h3>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', fontSize: '13px' }}>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Agent CLI</label>
                <select
                  value={newSessionAgent}
                  onChange={e => setNewSessionAgent(e.target.value)}
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                >
                  <option value="agy">agy (Google Antigravity)</option>
                  <option value="grok">grok (xAI Grok Build)</option>
                  <option value="codex">codex (OpenAI Codex)</option>
                  <option value="claude">claude (Claude Code)</option>
                  <option value="custom">custom (Custom CLI)</option>
                </select>
              </div>

              {newSessionAgent === 'custom' && (
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Custom CLI Command</label>
                  <input
                    type="text"
                    value={newSessionCustomAgent}
                    onChange={e => setNewSessionCustomAgent(e.target.value)}
                    placeholder="e.g. cursor-cli or custom_agent"
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                </div>
              )}

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Session Name (Optional)</label>
                  <input
                    type="text"
                    value={newSessionName}
                    onChange={e => setNewSessionName(e.target.value)}
                    placeholder="Auto-generated if blank"
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                </div>
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Working Directory</label>
                  <input
                    type="text"
                    value={newSessionCwd}
                    onChange={e => setNewSessionCwd(e.target.value)}
                    placeholder="Defaults to workspace"
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                </div>
              </div>

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Model Override</label>
                  <input
                    type="text"
                    value={newSessionModel}
                    onChange={e => setNewSessionModel(e.target.value)}
                    placeholder="Defaults to agent profile"
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                </div>
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Effort</label>
                  <input
                    type="text"
                    value={newSessionEffort}
                    onChange={e => setNewSessionEffort(e.target.value)}
                    placeholder="low / medium / high"
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                </div>
              </div>

              {/* Proxy Setting for Session */}
              <div style={{ background: '#f8fafc', padding: '12px', borderRadius: '6px', border: '1px solid #e2e8f0' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '6px', marginBottom: '8px', fontWeight: 600 }}>
                  <Globe size={16} />
                  <span>Network Proxy Setting</span>
                </div>
                <div style={{ display: 'grid', gridTemplateColumns: newSessionProxyMode === 'named' || newSessionProxyMode === 'custom' ? '1fr 1fr' : '1fr', gap: '10px' }}>
                  <div>
                    <label style={{ display: 'block', marginBottom: '4px', fontSize: '12px', color: '#64748b' }}>Proxy Mode</label>
                    <select
                      value={newSessionProxyMode}
                      onChange={e => {
                        const m = e.target.value
                        setNewSessionProxyMode(m)
                        if (m === 'named' && proxies.length > 0 && !newSessionProxyId) {
                          setNewSessionProxyId(proxies[0].id)
                        } else if (m === 'inherit' || m === 'direct') {
                          setNewSessionProxyId('')
                        }
                      }}
                      style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                    >
                      <option value="inherit">Inherit (From Agent Profile)</option>
                      <option value="direct">Direct (No Proxy)</option>
                      <option value="named">Named Proxy</option>
                      <option value="custom">Custom URL</option>
                    </select>
                  </div>

                  {newSessionProxyMode === 'named' && (
                    <div>
                      <label style={{ display: 'block', marginBottom: '4px', fontSize: '12px', color: '#64748b' }}>Configured Proxy</label>
                      {proxies.length > 0 ? (
                        <select
                          value={newSessionProxyId}
                          onChange={e => setNewSessionProxyId(e.target.value)}
                          style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                        >
                          <option value="">-- Select Proxy --</option>
                          {proxies.map(p => (
                            <option key={p.id} value={p.id}>
                              {p.id} ({p.url})
                            </option>
                          ))}
                        </select>
                      ) : (
                        <input
                          type="text"
                          value={newSessionProxyId}
                          onChange={e => setNewSessionProxyId(e.target.value)}
                          placeholder="e.g. clash"
                          style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                        />
                      )}
                    </div>
                  )}

                  {newSessionProxyMode === 'custom' && (
                    <div>
                      <label style={{ display: 'block', marginBottom: '4px', fontSize: '12px', color: '#64748b' }}>Proxy URL</label>
                      <input
                        type="text"
                        value={newSessionProxyId}
                        onChange={e => setNewSessionProxyId(e.target.value)}
                        placeholder="http://127.0.0.1:7890"
                        style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                      />
                    </div>
                  )}
                </div>
              </div>

              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Initial Prompt (Optional)</label>
                <textarea
                  value={newSessionInitialPrompt}
                  onChange={e => setNewSessionInitialPrompt(e.target.value)}
                  placeholder="Task or question to execute immediately..."
                  rows={3}
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box', fontFamily: 'inherit', resize: 'vertical' }}
                />
              </div>

              <div>
                <label style={{ display: 'flex', alignItems: 'center', gap: '6px', cursor: 'pointer' }}>
                  <input
                    type="checkbox"
                    checked={newSessionAutoApprove}
                    onChange={e => setNewSessionAutoApprove(e.target.checked)}
                  />
                  <span>Auto-approve tool execution and permission prompts</span>
                </label>
              </div>
            </div>

            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '20px' }}>
              <button
                type="button"
                onClick={() => setNewSessionModal(false)}
                disabled={newSessionStarting}
                style={{ padding: '6px 12px', borderRadius: '4px', border: '1px solid #ccc', background: '#fff', cursor: 'pointer' }}
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleStartSession}
                disabled={newSessionStarting}
                style={{ padding: '6px 16px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer', fontWeight: 500 }}
              >
                {newSessionStarting ? 'Starting…' : 'Start Session'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Add Account Modal */}
      {newAccountModal && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
        >
          <div style={{ width: '90%', maxWidth: '420px', background: '#fff', borderRadius: '8px', padding: '20px', boxShadow: '0 8px 32px rgba(0,0,0,0.2)' }}>
            <h3 style={{ margin: '0 0 16px 0' }}>Add Provider Account</h3>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', fontSize: '13px' }}>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Provider</label>
                <input
                  type="text"
                  value={newAccountProvider}
                  onChange={e => setNewAccountProvider(e.target.value)}
                  placeholder="e.g. anthropic, openai, google"
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                />
              </div>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>Account Identifier / Name</label>
                <input
                  type="text"
                  value={newAccountName}
                  onChange={e => setNewAccountName(e.target.value)}
                  placeholder="e.g. Work-Anthropic-1"
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                />
              </div>
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '20px' }}>
              <button
                type="button"
                onClick={() => setNewAccountModal(false)}
                style={{ padding: '6px 12px', borderRadius: '4px', border: '1px solid #ccc', background: '#fff', cursor: 'pointer' }}
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleCreateAccount}
                disabled={!newAccountProvider.trim() || !newAccountName.trim()}
                style={{ padding: '6px 14px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer' }}
              >
                Create Account
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
