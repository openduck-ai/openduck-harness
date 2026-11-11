import { useState, useEffect, useCallback, useMemo } from 'react'
import { Trans, useTranslation } from 'react-i18next'
import type { createApi, ManagedTuiSession, SessionOperation, AgentProfile, TuiAccount, QuotaSnapshot, ProxyItem } from '../api'
import { formatDateTime, formatTime, formatNumber } from '../i18n/format.ts'
import { validateProxyUrlMessage } from '../providerConfig'
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

type ErrorText = { kind: 'raw'; message: string } | { kind: 'fallback'; key: string }
type ScreenText =
  | { kind: 'raw'; text: string }
  | { kind: 'fallback'; key: string; values?: { message: string } }

type TabType = 'overview' | 'running' | 'history' | 'operations' | 'agents' | 'quotas'

export function TuiAgentsPage({ api }: TuiAgentsPageProps) {
  const { t } = useTranslation()
  const [activeTab, setActiveTab] = useState<TabType>('running')
  const [sessions, setSessions] = useState<ManagedTuiSession[]>([])
  const [operations, setOperations] = useState<SessionOperation[]>([])
  const [agents, setAgents] = useState<AgentProfile[]>([])
  const [accounts, setAccounts] = useState<TuiAccount[]>([])
  const [quotas, setQuotas] = useState<Record<string, QuotaSnapshot[]>>({})
  const [proxies, setProxies] = useState<ProxyItem[]>([])
  const [fileProxies, setFileProxies] = useState<Record<string, string>>({})
  const [editFileProxyMode, setEditFileProxyMode] = useState<'inherit' | 'direct' | 'url'>('inherit')
  const [editFileProxyUrl, setEditFileProxyUrl] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<ErrorText | null>(null)

  // Dialog & Inspection states
  const [inspectSession, setInspectSession] = useState<ManagedTuiSession | null>(null)
  const [inspectScreen, setInspectScreen] = useState<ScreenText>({ kind: 'raw', text: '' })
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
      const [sessRes, opsRes, agRes, accRes, proxiesRes, fileProxyRes] = await Promise.all([
        api.listManagedTuiSessions({ limit: 200 }),
        api.listManagedTuiOperations({ limit: 100 }),
        api.listTuiAgents(),
        api.listTuiAccounts(),
        typeof (api as any).getProxiesConfig === 'function'
          ? (api as any).getProxiesConfig().catch(() => ({ proxies: [] }))
          : Promise.resolve({ proxies: [] }),
        typeof (api as any).getTuiAgentProxies === 'function'
          ? (api as any).getTuiAgentProxies().catch(() => ({ agents: [] }))
          : Promise.resolve({ agents: [] }),
      ])
      setSessions(sessRes.sessions || [])
      setOperations(opsRes.operations || [])
      setAgents(agRes.agents || [])
      setAccounts(accRes.accounts || [])
      setProxies(proxiesRes.proxies || [])
      const nextFileProxies: Record<string, string> = {}
      for (const row of fileProxyRes.agents || []) {
        if (row?.agent && row.httpsProxy) nextFileProxies[row.agent] = row.httpsProxy
      }
      setFileProxies(nextFileProxies)

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
      setError(e.message ? { kind: 'raw', message: e.message } : { kind: 'fallback', key: 'tuiAgents.page.loadFailed' })
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
      alert(t('tuiAgents.page.interruptFailed', { message: e.message }))
    }
  }

  const handleStop = async (id: string) => {
    if (!confirm(t('tuiAgents.page.confirmStopSession'))) return
    try {
      await api.stopManagedTuiSession(id, 'manual_stop')
      loadData()
    } catch (e: any) {
      alert(t('tuiAgents.page.stopFailed', { message: e.message }))
    }
  }

  const handleExtend = async (id: string) => {
    try {
      await api.extendManagedTuiSession(id, 30)
      loadData()
    } catch (e: any) {
      alert(t('tuiAgents.page.extendFailed', { message: e.message }))
    }
  }

  const handleTakeover = async (id: string) => {
    try {
      await api.takeoverManagedTuiSession(id)
      loadData()
    } catch (e: any) {
      alert(t('tuiAgents.page.takeoverFailed', { message: e.message }))
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
      alert(t('tuiAgents.page.transferFailed', { message: e.message }))
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
      alert(t('tuiAgents.page.sendPromptFailed', { message: e.message }))
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
      alert(t('tuiAgents.page.sendKeysFailed', { message: e.message }))
    }
  }

  const handleOpenScreen = async (session: ManagedTuiSession) => {
    setInspectSession(session)
    setInspectScreen({ kind: 'fallback', key: 'tuiAgents.page.loadingTerminalScreen' })
    try {
      const res = await api.getManagedTuiSession(session.id)
      setInspectScreen(res.screen ? { kind: 'raw', text: res.screen } : { kind: 'fallback', key: 'tuiAgents.page.noScreenOutput' })
    } catch (e: any) {
      setInspectScreen({ kind: 'fallback', key: 'tuiAgents.page.errorReadingTerminal', values: { message: e.message } })
    }
  }

  const openProfileEditor = (profile: AgentProfile) => {
    const stored = fileProxies[profile.agentKind]
    if (!stored) {
      setEditFileProxyMode('inherit')
      setEditFileProxyUrl('')
    } else if (stored === 'direct') {
      setEditFileProxyMode('direct')
      setEditFileProxyUrl('')
    } else {
      setEditFileProxyMode('url')
      setEditFileProxyUrl(stored)
    }
    setEditProfileModal(profile)
  }

  const fileProxyLabel = (agentKind: string) => {
    const stored = fileProxies[agentKind]
    if (!stored) return t('config.panel.tuiAgentProxyInherit')
    if (stored === 'direct') return t('config.panel.tuiAgentProxyDirect')
    return stored
  }

  const handleSaveProfile = async (profile: AgentProfile) => {
    const httpsProxy =
      editFileProxyMode === 'direct' ? 'direct' : editFileProxyMode === 'url' ? editFileProxyUrl.trim() : null
    if (editFileProxyMode === 'url') {
      const invalid = validateProxyUrlMessage(httpsProxy || '')
      if (invalid) {
        alert(t(invalid.key))
        return
      }
    }
    try {
      await api.upsertTuiAgent(profile)
      if (typeof (api as any).updateTuiAgentProxies === 'function') {
        await (api as any).updateTuiAgentProxies({
          agents: [{ agent: profile.agentKind, httpsProxy }],
        })
      }
      setEditProfileModal(null)
      loadData()
    } catch (e: any) {
      alert(t('tuiAgents.page.saveProfileFailed', { message: e.message }))
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
      alert(t('tuiAgents.page.createAccountFailed', { message: e.message }))
    }
  }

  const handleRefreshQuotas = async (accountId: string) => {
    try {
      await api.refreshTuiQuotas(accountId)
      loadData()
    } catch (e: any) {
      alert(t('tuiAgents.page.refreshQuotasFailed', { message: e.message }))
    }
  }

  const resolveProxyDisplay = (proxyMode?: string, proxyId?: string, effectiveProxy?: string) => {
    if (effectiveProxy) {
      const match = proxies.find(p => p.id === effectiveProxy || p.url === effectiveProxy)
      return match ? `${match.id} (${match.url})` : effectiveProxy
    }
    if (proxyMode === 'direct') return t('tuiAgents.page.proxyDirect')
    if (proxyMode === 'custom') return proxyId ? t('tuiAgents.page.proxyCustomWithId', { id: proxyId }) : t('tuiAgents.page.proxyCustom')
    if (proxyMode === 'named' && proxyId) {
      const match = proxies.find(p => p.id === proxyId)
      return match ? `${match.id} (${match.url})` : t('tuiAgents.page.proxyNamedWithId', { id: proxyId })
    }
    return t('tuiAgents.page.proxyInherit')
  }

  const handleStartSession = async () => {
    const agentKind = newSessionAgent === 'custom' ? newSessionCustomAgent.trim() : newSessionAgent
    if (!agentKind) {
      alert(t('tuiAgents.page.specifyAgentAlert'))
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
      alert(t('tuiAgents.page.startSessionFailed', { message: e.message }))
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
            <span>{t('tuiAgents.page.headerTitle')}</span>
          </h2>
          <p style={{ margin: '4px 0 0 0', color: 'var(--text-muted, #888)', fontSize: '14px' }}>
            {t('tuiAgents.page.headerSubtitle')}
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
            <span>{t('tuiAgents.page.launchAgentButton')}</span>
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
            <span>{t('tuiAgents.page.refreshButton')}</span>
          </button>
        </div>
      </div>

      {error && (
        <div style={{ padding: '12px 16px', marginBottom: '16px', background: '#ffebee', color: '#c62828', borderRadius: '6px', display: 'flex', alignItems: 'center', gap: '8px' }}>
          <AlertCircle size={18} />
          <span>{error.kind === 'raw' ? error.message : t(error.key)}</span>
        </div>
      )}

      {/* Navigation Tabs */}
      <div style={{ display: 'flex', gap: '4px', borderBottom: '1px solid var(--border-color, #e0e0e0)', marginBottom: '20px' }}>
        {[
          { key: 'running', label: t('tuiAgents.page.tabRunningSessions', { count: runningSessions.length }), icon: Activity },
          { key: 'overview', label: t('tuiAgents.page.tabOverview'), icon: Shield },
          { key: 'history', label: t('tuiAgents.page.tabSessionHistory', { count: historicalSessions.length }), icon: History },
          { key: 'operations', label: t('tuiAgents.page.tabOperationsLedger', { count: operations.length }), icon: ListOrdered },
          { key: 'agents', label: t('tuiAgents.page.tabAgentProfiles', { count: agents.length }), icon: Cpu },
          { key: 'quotas', label: t('tuiAgents.page.tabAccountsQuotas', { count: accounts.length }), icon: CreditCard },
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
              <div style={{ fontSize: '13px', color: 'var(--text-muted, #777)' }}>{t('tuiAgents.page.activeSessions')}</div>
              <div style={{ fontSize: '28px', fontWeight: 700, margin: '4px 0' }}>{metrics.activeTotal}</div>
              <div style={{ fontSize: '12px', color: '#2e7d32' }}>{t('tuiAgents.page.activeWorkingIdle', { working: metrics.working, idle: metrics.idle })}</div>
            </div>
            <div style={{ padding: '16px', borderRadius: '8px', background: 'var(--bg-card, #f9f9f9)', border: '1px solid var(--border-color, #eee)' }}>
              <div style={{ fontSize: '13px', color: 'var(--text-muted, #777)' }}>{t('tuiAgents.page.waitingApproval')}</div>
              <div style={{ fontSize: '28px', fontWeight: 700, margin: '4px 0', color: metrics.waiting > 0 ? '#ed6c02' : 'inherit' }}>
                {metrics.waiting}
              </div>
              <div style={{ fontSize: '12px', color: 'var(--text-muted, #777)' }}>{t('tuiAgents.page.needsKeysConfirmation')}</div>
            </div>
            <div style={{ padding: '16px', borderRadius: '8px', background: 'var(--bg-card, #f9f9f9)', border: '1px solid var(--border-color, #eee)' }}>
              <div style={{ fontSize: '13px', color: 'var(--text-muted, #777)' }}>{t('tuiAgents.page.retainedPendingReclaim')}</div>
              <div style={{ fontSize: '28px', fontWeight: 700, margin: '4px 0', color: metrics.retained > 0 ? '#0288d1' : 'inherit' }}>
                {metrics.retained}
              </div>
              <div style={{ fontSize: '12px', color: 'var(--text-muted, #777)' }}>{t('tuiAgents.page.gracePeriodNotice')}</div>
            </div>
            <div style={{ padding: '16px', borderRadius: '8px', background: 'var(--bg-card, #f9f9f9)', border: '1px solid var(--border-color, #eee)' }}>
              <div style={{ fontSize: '13px', color: 'var(--text-muted, #777)' }}>{t('tuiAgents.page.configuredAgentsAccounts')}</div>
              <div style={{ fontSize: '28px', fontWeight: 700, margin: '4px 0' }}>{metrics.agentsCount} / {metrics.accountsCount}</div>
              <div style={{ fontSize: '12px', color: 'var(--text-muted, #777)' }}>{t('tuiAgents.page.profilesProviderPools')}</div>
            </div>
          </div>

          <div style={{ padding: '20px', borderRadius: '8px', background: 'var(--bg-card, #f9f9f9)', border: '1px solid var(--border-color, #eee)' }}>
            <h3 style={{ margin: '0 0 12px 0', fontSize: '16px' }}>{t('tuiAgents.page.policySummaryTitle')}</h3>
            <ul style={{ margin: 0, paddingLeft: '20px', lineHeight: '1.8', fontSize: '14px', color: 'var(--text-muted, #555)' }}>
              <li><strong>{t('tuiAgents.page.policyRunStopLabel')}</strong> {t('tuiAgents.page.policyRunStopDesc')}</li>
              <li><strong>{t('tuiAgents.page.policyRunSuccessLabel')}</strong> {t('tuiAgents.page.policyRunSuccessDesc')}</li>
              <li><strong>{t('tuiAgents.page.policyRunFailureLabel')}</strong> {t('tuiAgents.page.policyRunFailureDesc')}</li>
              <li><strong>{t('tuiAgents.page.policyManualIdleLabel')}</strong> {t('tuiAgents.page.policyManualIdleDesc')}</li>
              <li><strong>{t('tuiAgents.page.policyOwnershipLabel')}</strong> {t('tuiAgents.page.policyOwnershipDesc')}</li>
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
              <div>{t('tuiAgents.page.noRunningSessions')}</div>
            </div>
          ) : (
            <div style={{ overflowX: 'auto' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '13px' }}>
                <thead>
                  <tr style={{ borderBottom: '2px solid var(--border-color, #eee)', color: 'var(--text-muted, #777)' }}>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thSessionAgent')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thAttribution')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thModelProxy')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thState')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thLastOpActivity')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thReclaimCountdown')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thActions')}</th>
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
                            {t('tuiAgents.page.cliLabel')} <span style={{ textTransform: 'uppercase' }}>{s.agentKind}</span> {t('tuiAgents.page.sessionShortId', { id: s.id.slice(0, 10) })}
                          </div>
                        </td>
                        <td style={{ padding: '12px' }}>
                          <div>{t('tuiAgents.page.projectAttribution', { project: s.createdByProjectId || t('tuiAgents.page.global') })}</div>
                          <div style={{ fontSize: '11px', color: 'var(--text-muted, #888)' }}>
                            {t('tuiAgents.page.ownerRunAttribution', { run: s.ownerRunId ? `${s.ownerRunId.slice(0, 14)}...` : t('tuiAgents.page.manual'), mode: s.ownershipMode })}
                          </div>
                        </td>
                        <td style={{ padding: '12px' }}>
                          <div>{s.model || t('tuiAgents.page.default')}</div>
                          <div style={{ fontSize: '11px', color: 'var(--text-muted, #888)' }}>
                            {t('tuiAgents.page.proxyLabel', { proxy: s.effectiveProxy || s.proxyMode })}
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
                          <div style={{ fontSize: '12px' }}>{s.lastOperationAt ? formatTime(s.lastOperationAt) : t('tuiAgents.page.none')}</div>
                          <div style={{ fontSize: '11px', color: 'var(--text-muted, #888)' }}>
                            {t('tuiAgents.page.lastInput', { time: s.lastInputAt ? formatTime(s.lastInputAt) : t('tuiAgents.page.notAvailable') })}
                          </div>
                        </td>
                        <td style={{ padding: '12px' }}>
                          {s.reclaimAfter ? (
                            <span style={{ color: '#d32f2f', fontWeight: 500 }}>
                              {formatTime(s.reclaimAfter)}
                            </span>
                          ) : (
                            <span style={{ color: 'var(--text-muted, #888)' }}>{t('tuiAgents.page.idleSweep')}</span>
                          )}
                        </td>
                        <td style={{ padding: '12px' }}>
                          <div style={{ display: 'flex', gap: '6px', flexWrap: 'wrap' }}>
                            <button
                              type="button"
                              onClick={() => handleOpenScreen(s)}
                              title={t('tuiAgents.page.actionViewScreen')}
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              {t('tuiAgents.page.btnScreen')}
                            </button>
                            <button
                              type="button"
                              onClick={() => setPromptModalSession(s)}
                              title={t('tuiAgents.page.actionSendPrompt')}
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              {t('tuiAgents.page.btnPrompt')}
                            </button>
                            <button
                              type="button"
                              onClick={() => setKeysModalSession(s)}
                              title={t('tuiAgents.page.actionSendKeys')}
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              {t('tuiAgents.page.btnKeys')}
                            </button>
                            <button
                              type="button"
                              onClick={() => handleExtend(s.id)}
                              title={t('tuiAgents.page.actionExtend30m')}
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              {t('tuiAgents.page.btnExtend30m')}
                            </button>
                            <button
                              type="button"
                              onClick={() => handleInterrupt(s.id)}
                              title={t('tuiAgents.page.actionInterrupt')}
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ff9800', color: '#e65100' }}
                            >
                              {t('tuiAgents.page.btnInterrupt')}
                            </button>
                            <button
                              type="button"
                              onClick={() => handleTakeover(s.id)}
                              title={t('tuiAgents.page.actionTakeover')}
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              {t('tuiAgents.page.btnTakeover')}
                            </button>
                            <button
                              type="button"
                              onClick={() => {
                                setTransferModalSession(s)
                                setTransferRunId('')
                              }}
                              title={t('tuiAgents.page.actionTransfer')}
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #ccc' }}
                            >
                              {t('tuiAgents.page.btnTransfer')}
                            </button>
                            <button
                              type="button"
                              onClick={() => handleStop(s.id)}
                              title={t('tuiAgents.page.actionStop')}
                              style={{ padding: '4px 8px', fontSize: '11px', cursor: 'pointer', borderRadius: '4px', border: '1px solid #f44336', color: '#d32f2f' }}
                            >
                              {t('tuiAgents.page.btnStop')}
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
              <div>{t('tuiAgents.page.noHistorySessions')}</div>
            </div>
          ) : (
            <div style={{ overflowX: 'auto' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '13px' }}>
                <thead>
                  <tr style={{ borderBottom: '2px solid var(--border-color, #eee)', color: 'var(--text-muted, #777)' }}>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thSession')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thAgent')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thProjectRun')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thCreatedAt')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thReclaimedAt')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thStopReason')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thActions')}</th>
                  </tr>
                </thead>
                <tbody>
                  {historicalSessions.map(s => (
                    <tr key={s.id} style={{ borderBottom: '1px solid var(--border-color, #f0f0f0)' }}>
                      <td style={{ padding: '12px', fontWeight: 600 }}>{s.sessionName}</td>
                      <td style={{ padding: '12px', textTransform: 'uppercase' }}>{s.agentKind}</td>
                      <td style={{ padding: '12px' }}>
                        <div>{s.createdByProjectId || t('tuiAgents.page.global')}</div>
                        <div style={{ fontSize: '11px', color: 'var(--text-muted, #888)' }}>{s.createdByRunId || t('tuiAgents.page.manual')}</div>
                      </td>
                      <td style={{ padding: '12px' }}>{formatDateTime(s.createdAt)}</td>
                      <td style={{ padding: '12px' }}>{s.reclaimedAt ? formatDateTime(s.reclaimedAt) : s.stoppedAt ? formatDateTime(s.stoppedAt) : t('tuiAgents.page.notAvailable')}</td>
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
                          {t('tuiAgents.page.btnViewSnapshot')}
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
              <div>{t('tuiAgents.page.noOperations')}</div>
            </div>
          ) : (
            <div style={{ overflowX: 'auto' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '13px' }}>
                <thead>
                  <tr style={{ borderBottom: '2px solid var(--border-color, #eee)', color: 'var(--text-muted, #777)' }}>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thTime')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thType')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thSession')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thOperator')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thRunTask')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thStatus')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thDuration')}</th>
                    <th style={{ padding: '10px 12px' }}>{t('tuiAgents.page.thSummary')}</th>
                  </tr>
                </thead>
                <tbody>
                  {operations.map(op => {
                    const isSuccess = op.status === 'succeeded'
                    const isFailed = op.status === 'failed'
                    return (
                      <tr key={op.id} style={{ borderBottom: '1px solid var(--border-color, #f0f0f0)' }}>
                        <td style={{ padding: '10px 12px', whiteSpace: 'nowrap' }}>{formatTime(op.startedAt)}</td>
                        <td style={{ padding: '10px 12px' }}>
                          <span style={{ fontWeight: 600, textTransform: 'uppercase', fontSize: '11px' }}>{op.operationType}</span>
                        </td>
                        <td style={{ padding: '10px 12px' }}>{op.sessionName}</td>
                        <td style={{ padding: '10px 12px' }}>{op.operator}</td>
                        <td style={{ padding: '10px 12px', fontSize: '11px', color: 'var(--text-muted, #888)' }}>
                          {op.runId ? `${op.runId.slice(0, 12)}...` : t('tuiAgents.page.notAvailable')}
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
                        openProfileEditor(
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
                      {t('tuiAgents.page.btnConfigure')}
                    </button>
                  </div>
                  <div style={{ fontSize: '13px', lineHeight: '1.8' }}>
                    <div><strong>{t('tuiAgents.page.labelDefaultModel')}</strong> {profile?.defaultModel || t('tuiAgents.page.cliDefault')}</div>
                    <div><strong>{t('tuiAgents.page.labelDefaultEffort')}</strong> {profile?.defaultEffort || t('tuiAgents.page.default')}</div>
                    <div>
                      <strong>{t('tuiAgents.page.labelFileProxy')}</strong>{' '}
                      <span style={{ fontFamily: 'monospace' }}>{fileProxyLabel(kind)}</span>
                    </div>
                    <div>
                      <strong>{t('tuiAgents.page.labelNetworkProxy')}</strong>{' '}
                      <span style={{ fontFamily: 'monospace' }}>
                        {resolveProxyDisplay(profile?.proxyMode, profile?.proxyId)}
                      </span>
                    </div>
                    <div><strong>{t('tuiAgents.page.labelAccountBinding')}</strong> {profile?.accountId || t('tuiAgents.page.noneSystem')}</div>
                    <div><strong>{t('tuiAgents.page.labelMaxConcurrency')}</strong> {profile?.maxConcurrency || t('tuiAgents.page.unlimited')}</div>
                    <div><strong>{t('tuiAgents.page.labelIdleTimeout')}</strong> {profile?.idleTimeoutMins ? t('tuiAgents.page.timeoutMins', { mins: profile.idleTimeoutMins }) : t('tuiAgents.page.timeoutDefault')}</div>
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
            <h3 style={{ margin: 0, fontSize: '16px' }}>{t('tuiAgents.page.providerAccountsTitle')}</h3>
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
              <span>{t('tuiAgents.page.btnAddAccount')}</span>
            </button>
          </div>

          {accounts.length === 0 ? (
            <div style={{ padding: '40px', textAlign: 'center', color: 'var(--text-muted, #888)' }}>
              <CreditCard size={48} style={{ opacity: 0.3, marginBottom: '12px' }} />
              <div>{t('tuiAgents.page.noAccountsConfigured')}</div>
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
                        <div style={{ fontSize: '12px', color: 'var(--text-muted, #888)' }}>{t('tuiAgents.page.providerLabel', { provider: acc.provider })}</div>
                      </div>
                      <button
                        type="button"
                        onClick={() => handleRefreshQuotas(acc.id)}
                        style={{ padding: '4px 8px', fontSize: '11px', borderRadius: '4px', border: '1px solid #ccc', cursor: 'pointer' }}
                      >
                        {t('tuiAgents.page.btnRefreshQuota')}
                      </button>
                    </div>

                    {accQuotas.length === 0 ? (
                      <div style={{ fontSize: '12px', color: 'var(--text-muted, #888)', fontStyle: 'italic' }}>
                        {t('tuiAgents.page.noQuotaSnapshots')}
                      </div>
                    ) : (
                      <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
                        {accQuotas.map(q => (
                          <div key={q.id} style={{ padding: '8px', background: '#fff', borderRadius: '6px', border: '1px solid #eee' }}>
                            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '12px', marginBottom: '4px' }}>
                              <span style={{ fontWeight: 600 }}>{t('tuiAgents.page.quotaHeading', { type: q.quotaType, unit: q.unit })}</span>
                              <span>{q.remainingPercent !== undefined ? t('tuiAgents.page.percentLeft', { percent: formatNumber(q.remainingPercent, { minimumFractionDigits: 1, maximumFractionDigits: 1 }) }) : t('tuiAgents.page.unknown')}</span>
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
                              <span>{t('tuiAgents.page.usedAmount', { amount: q.usedAmount })}</span>
                              <span>{t('tuiAgents.page.observedTime', { time: formatTime(q.observedAt) })}</span>
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
                {t('tuiAgents.page.terminalCaptureTitle', { name: inspectSession.sessionName, agent: inspectSession.agentKind })}
              </div>
              <button
                type="button"
                onClick={() => setInspectSession(null)}
                aria-label={t('tuiAgents.page.btnClose')}
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
              {inspectScreen.kind === 'raw' ? inspectScreen.text : t(inspectScreen.key, inspectScreen.values)}
            </pre>
            <div style={{ padding: '12px 18px', borderTop: '1px solid #333', display: 'flex', justifyContent: 'flex-end' }}>
              <button
                type="button"
                onClick={() => setInspectSession(null)}
                style={{ padding: '6px 14px', borderRadius: '4px', border: '1px solid #555', background: '#333', color: '#fff', cursor: 'pointer' }}
              >
                {t('tuiAgents.page.btnClose')}
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
            <h3 style={{ margin: '0 0 12px 0' }}>{t('tuiAgents.page.sendPromptTitle', { name: promptModalSession.sessionName })}</h3>
            <textarea
              rows={4}
              value={promptText}
              onChange={e => setPromptText(e.target.value)}
              placeholder={t('tuiAgents.page.promptPlaceholder')}
              style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
            />
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '16px' }}>
              <button
                type="button"
                onClick={() => setPromptModalSession(null)}
                style={{ padding: '6px 12px', borderRadius: '4px', border: '1px solid #ccc', background: '#fff', cursor: 'pointer' }}
              >
                {t('tuiAgents.page.btnCancel')}
              </button>
              <button
                type="button"
                onClick={handleSendPrompt}
                disabled={promptSending || !promptText.trim()}
                style={{ padding: '6px 14px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer' }}
              >
                {promptSending ? t('tuiAgents.page.btnSubmitting') : t('tuiAgents.page.btnSendPrompt')}
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
            <h3 style={{ margin: '0 0 12px 0' }}>{t('tuiAgents.page.sendKeysTitle', { name: keysModalSession.sessionName })}</h3>
            <p style={{ margin: '0 0 8px 0', fontSize: '13px', color: '#666' }}>
              {t('tuiAgents.page.commonKeysHint')}
            </p>
            <input
              type="text"
              value={keysInput}
              onChange={e => setKeysInput(e.target.value)}
              placeholder={t('tuiAgents.page.keysPlaceholder')}
              style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
            />
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '16px' }}>
              <button
                type="button"
                onClick={() => setKeysModalSession(null)}
                style={{ padding: '6px 12px', borderRadius: '4px', border: '1px solid #ccc', background: '#fff', cursor: 'pointer' }}
              >
                {t('tuiAgents.page.btnCancel')}
              </button>
              <button
                type="button"
                onClick={handleSendKeys}
                disabled={!keysInput.trim()}
                style={{ padding: '6px 14px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer' }}
              >
                {t('tuiAgents.page.btnSendKeys')}
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
            <h3 style={{ margin: '0 0 12px 0' }}>{t('tuiAgents.page.transferTitle')}</h3>
            <p style={{ margin: '0 0 8px 0', fontSize: '13px', color: '#666' }}>
              <Trans i18nKey="tuiAgents.page.transferAssign" values={{ name: transferModalSession.sessionName }} components={{ strong: <strong /> }} />
            </p>
            <input
              type="text"
              value={transferRunId}
              onChange={e => setTransferRunId(e.target.value)}
              placeholder={t('tuiAgents.page.targetRunIdPlaceholder')}
              style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
            />
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '16px' }}>
              <button
                type="button"
                onClick={() => setTransferModalSession(null)}
                style={{ padding: '6px 12px', borderRadius: '4px', border: '1px solid #ccc', background: '#fff', cursor: 'pointer' }}
              >
                {t('tuiAgents.page.btnCancel')}
              </button>
              <button
                type="button"
                onClick={handleTransfer}
                disabled={!transferRunId.trim()}
                style={{ padding: '6px 14px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer' }}
              >
                {t('tuiAgents.page.btnTransfer')}
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
            <h3 style={{ margin: '0 0 16px 0' }}>{t('tuiAgents.page.configureProfileTitle', { kind: editProfileModal.agentKind.toUpperCase() })}</h3>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', fontSize: '13px' }}>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formDefaultModel')}</label>
                <input
                  type="text"
                  value={editProfileModal.defaultModel || ''}
                  onChange={e => setEditProfileModal({ ...editProfileModal, defaultModel: e.target.value })}
                  placeholder={t('tuiAgents.page.modelPlaceholder')}
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                />
              </div>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formDefaultEffort')}</label>
                <input
                  type="text"
                  value={editProfileModal.defaultEffort || ''}
                  onChange={e => setEditProfileModal({ ...editProfileModal, defaultEffort: e.target.value })}
                  placeholder={t('tuiAgents.page.effortPlaceholder')}
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                />
              </div>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formFileProxy')}</label>
                <select
                  value={editFileProxyMode}
                  onChange={e => {
                    const mode = e.target.value as 'inherit' | 'direct' | 'url'
                    setEditFileProxyMode(mode)
                    if (mode !== 'url') setEditFileProxyUrl('')
                  }}
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                >
                  <option value="inherit">{t('config.panel.tuiAgentProxyInherit')}</option>
                  <option value="direct">{t('config.panel.tuiAgentProxyDirect')}</option>
                  <option value="url">{t('config.panel.tuiAgentProxyUrl')}</option>
                </select>
                {editFileProxyMode === 'url' && (
                  <input
                    type="text"
                    value={editFileProxyUrl}
                    onChange={e => setEditFileProxyUrl(e.target.value)}
                    placeholder={t('config.panel.tuiAgentProxyUrlPlaceholder')}
                    style={{ width: '100%', marginTop: '8px', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                )}
                <small style={{ color: 'var(--text-muted, #888)', display: 'block', marginTop: '4px' }}>
                  {t('tuiAgents.page.fileProxyHint')}
                </small>
              </div>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formProxyMode')}</label>
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
                  <option value="inherit">{t('tuiAgents.page.optProxyInherit')}</option>
                  <option value="direct">{t('tuiAgents.page.optProxyDirect')}</option>
                  <option value="named">{t('tuiAgents.page.optProxyNamed')}</option>
                  <option value="custom">{t('tuiAgents.page.optProxyCustom')}</option>
                </select>
              </div>
              {editProfileModal.proxyMode === 'named' && (
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formConfiguredProxy')}</label>
                  {proxies.length > 0 ? (
                    <select
                      value={editProfileModal.proxyId || ''}
                      onChange={e => setEditProfileModal({ ...editProfileModal, proxyId: e.target.value })}
                      style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                    >
                      <option value="">{t('tuiAgents.page.optSelectProxy')}</option>
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
                      placeholder={t('tuiAgents.page.proxyIdPlaceholder')}
                      style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                    />
                  )}
                  {proxies.length === 0 && (
                    <small style={{ color: 'var(--text-muted, #888)', display: 'block', marginTop: '4px' }}>
                      {t('tuiAgents.page.noNamedProxiesHint')}
                    </small>
                  )}
                </div>
              )}
              {editProfileModal.proxyMode === 'custom' && (
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formCustomProxyUrl')}</label>
                  <input
                    type="text"
                    value={editProfileModal.proxyId || ''}
                    onChange={e => setEditProfileModal({ ...editProfileModal, proxyId: e.target.value })}
                    placeholder={t('tuiAgents.page.customProxyPlaceholder')}
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                  <small style={{ color: 'var(--text-muted, #888)', display: 'block', marginTop: '4px' }}>
                    {t('tuiAgents.page.customProxyHint')}
                  </small>
                </div>
              )}
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formIdleTimeout')}</label>
                <input
                  type="number"
                  value={editProfileModal.idleTimeoutMins || ''}
                  onChange={e => setEditProfileModal({ ...editProfileModal, idleTimeoutMins: Number(e.target.value) || undefined })}
                  placeholder={t('tuiAgents.page.idleTimeoutPlaceholder')}
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
                {t('tuiAgents.page.btnCancel')}
              </button>
              <button
                type="button"
                onClick={() => handleSaveProfile(editProfileModal)}
                style={{ padding: '6px 14px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer' }}
              >
                {t('tuiAgents.page.btnSaveProfile')}
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
              <span>{t('tuiAgents.page.launchSessionTitle')}</span>
            </h3>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', fontSize: '13px' }}>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formAgentCli')}</label>
                <select
                  value={newSessionAgent}
                  onChange={e => setNewSessionAgent(e.target.value)}
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                >
                  <option value="agy">{t('tuiAgents.page.optAgentAgy')}</option>
                  <option value="grok">{t('tuiAgents.page.optAgentGrok')}</option>
                  <option value="codex">{t('tuiAgents.page.optAgentCodex')}</option>
                  <option value="claude">{t('tuiAgents.page.optAgentClaude')}</option>
                  <option value="custom">{t('tuiAgents.page.optAgentCustom')}</option>
                </select>
              </div>

              {newSessionAgent === 'custom' && (
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formCustomCliCommand')}</label>
                  <input
                    type="text"
                    value={newSessionCustomAgent}
                    onChange={e => setNewSessionCustomAgent(e.target.value)}
                    placeholder={t('tuiAgents.page.customCliPlaceholder')}
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                </div>
              )}

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formSessionNameOptional')}</label>
                  <input
                    type="text"
                    value={newSessionName}
                    onChange={e => setNewSessionName(e.target.value)}
                    placeholder={t('tuiAgents.page.sessionNamePlaceholder')}
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                </div>
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formWorkingDirectory')}</label>
                  <input
                    type="text"
                    value={newSessionCwd}
                    onChange={e => setNewSessionCwd(e.target.value)}
                    placeholder={t('tuiAgents.page.workingDirPlaceholder')}
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                </div>
              </div>

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formModelOverride')}</label>
                  <input
                    type="text"
                    value={newSessionModel}
                    onChange={e => setNewSessionModel(e.target.value)}
                    placeholder={t('tuiAgents.page.modelOverridePlaceholder')}
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                </div>
                <div>
                  <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formEffort')}</label>
                  <input
                    type="text"
                    value={newSessionEffort}
                    onChange={e => setNewSessionEffort(e.target.value)}
                    placeholder={t('tuiAgents.page.effortLevelsPlaceholder')}
                    style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                  />
                </div>
              </div>

              {/* Proxy Setting for Session */}
              <div style={{ background: '#f8fafc', padding: '12px', borderRadius: '6px', border: '1px solid #e2e8f0' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '6px', marginBottom: '8px', fontWeight: 600 }}>
                  <Globe size={16} />
                  <span>{t('tuiAgents.page.networkProxySetting')}</span>
                </div>
                <div style={{ display: 'grid', gridTemplateColumns: newSessionProxyMode === 'named' || newSessionProxyMode === 'custom' ? '1fr 1fr' : '1fr', gap: '10px' }}>
                  <div>
                    <label style={{ display: 'block', marginBottom: '4px', fontSize: '12px', color: '#64748b' }}>{t('tuiAgents.page.formProxyMode')}</label>
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
                      <option value="inherit">{t('tuiAgents.page.optProxyInheritFromProfile')}</option>
                      <option value="direct">{t('tuiAgents.page.optProxyDirect')}</option>
                      <option value="named">{t('tuiAgents.page.optProxyNamedShort')}</option>
                      <option value="custom">{t('tuiAgents.page.optProxyCustomShort')}</option>
                    </select>
                  </div>

                  {newSessionProxyMode === 'named' && (
                    <div>
                      <label style={{ display: 'block', marginBottom: '4px', fontSize: '12px', color: '#64748b' }}>{t('tuiAgents.page.formConfiguredProxy')}</label>
                      {proxies.length > 0 ? (
                        <select
                          value={newSessionProxyId}
                          onChange={e => setNewSessionProxyId(e.target.value)}
                          style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                        >
                          <option value="">{t('tuiAgents.page.optSelectProxyShort')}</option>
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
                          placeholder={t('tuiAgents.page.proxyIdPlaceholder')}
                          style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                        />
                      )}
                    </div>
                  )}

                  {newSessionProxyMode === 'custom' && (
                    <div>
                      <label style={{ display: 'block', marginBottom: '4px', fontSize: '12px', color: '#64748b' }}>{t('tuiAgents.page.formProxyUrl')}</label>
                      <input
                        type="text"
                        value={newSessionProxyId}
                        onChange={e => setNewSessionProxyId(e.target.value)}
                        placeholder={t('tuiAgents.page.proxyUrlPlaceholder')}
                        style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                      />
                    </div>
                  )}
                </div>
              </div>

              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formInitialPromptOptional')}</label>
                <textarea
                  value={newSessionInitialPrompt}
                  onChange={e => setNewSessionInitialPrompt(e.target.value)}
                  placeholder={t('tuiAgents.page.initialPromptPlaceholder')}
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
                  <span>{t('tuiAgents.page.autoApproveLabel')}</span>
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
                {t('tuiAgents.page.btnCancel')}
              </button>
              <button
                type="button"
                onClick={handleStartSession}
                disabled={newSessionStarting}
                style={{ padding: '6px 16px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer', fontWeight: 500 }}
              >
                {newSessionStarting ? t('tuiAgents.page.btnStarting') : t('tuiAgents.page.btnStartSession')}
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
            <h3 style={{ margin: '0 0 16px 0' }}>{t('tuiAgents.page.addAccountTitle')}</h3>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', fontSize: '13px' }}>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formProvider')}</label>
                <input
                  type="text"
                  value={newAccountProvider}
                  onChange={e => setNewAccountProvider(e.target.value)}
                  placeholder={t('tuiAgents.page.providerPlaceholder')}
                  style={{ width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid #ccc', boxSizing: 'border-box' }}
                />
              </div>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontWeight: 500 }}>{t('tuiAgents.page.formAccountIdentifierName')}</label>
                <input
                  type="text"
                  value={newAccountName}
                  onChange={e => setNewAccountName(e.target.value)}
                  placeholder={t('tuiAgents.page.accountNamePlaceholder')}
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
                {t('tuiAgents.page.btnCancel')}
              </button>
              <button
                type="button"
                onClick={handleCreateAccount}
                disabled={!newAccountProvider.trim() || !newAccountName.trim()}
                style={{ padding: '6px 14px', borderRadius: '4px', border: 'none', background: '#0066cc', color: '#fff', cursor: 'pointer' }}
              >
                {t('tuiAgents.page.btnCreateAccount')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
