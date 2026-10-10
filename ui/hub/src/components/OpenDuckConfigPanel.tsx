import { useEffect, useState } from 'react'
import type { AppConfigDocument, ProxyItem, createApi } from '@aaif/goose-hub-core'
import { Check, Edit2, Trash2, X } from 'lucide-react'
import {
  draftFromResponse,
  addProvider,
  buildPutBody,
  fastModelLabel,
  secondaryModelLabel,
  findMatchingProxyId,
  validateProxyId,
  validateProxyUrl,
  type ProviderConfigDraft,
} from '../providerConfig'
import {
  JUDGE_MODES,
  JUDGE_POINT_LABELS,
  MODE_OPTIONS,
  configLabel,
  draftsFromDocument,
  formatScalar,
  harnessDraftUnchanged,
  harnessUpdateForSave,
  judgeUpdateFromDraft,
  parseScalarInput,
  validateNewConfigKey,
  type ConfigDrafts,
} from '../configEditor'

type ConfigApi = Pick<
  ReturnType<typeof createApi>,
  | 'getAppConfig'
  | 'upsertAppConfig'
  | 'removeAppConfig'
  | 'updateHarnessConfig'
  | 'updateJudgeConfig'
  | 'removeJudgeConfig'
  | 'getProvidersConfig'
  | 'updateProvidersConfig'
  | 'getProxiesConfig'
  | 'updateProxiesConfig'
>

interface OpenDuckConfigPanelProps {
  api: ConfigApi
  isConnected: boolean
}

const KNOWN_JUDGE_POINTS = new Set(['turn.drift', 'turn.completion', 'context.forget', 'tool.risk'])

function messageFrom(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback
}

export function OpenDuckConfigPanel({ api, isConnected }: OpenDuckConfigPanelProps) {
  const [doc, setDoc] = useState<AppConfigDocument | null>(null)
  const [drafts, setDrafts] = useState<ConfigDrafts | null>(null)
  const [providersDraft, setProvidersDraft] = useState<ProviderConfigDraft | null>(null)
  const [proxiesDraft, setProxiesDraft] = useState<ProxyItem[]>([])
  const [newProxyId, setNewProxyId] = useState('')
  const [newProxyUrl, setNewProxyUrl] = useState('')
  const [newProxyDesc, setNewProxyDesc] = useState('')
  const [proxyFormError, setProxyFormError] = useState('')
  const [editingProxyId, setEditingProxyId] = useState<string | null>(null)
  const [editProxyUrl, setEditProxyUrl] = useState('')
  const [editProxyDesc, setEditProxyDesc] = useState('')
  const [newProviderSelect, setNewProviderSelect] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [saving, setSaving] = useState<string | null>(null)
  const [newKey, setNewKey] = useState('')
  const [newValue, setNewValue] = useState('')
  const [newPointId, setNewPointId] = useState('')

  const applyDocument = (next: AppConfigDocument) => {
    setDoc(next)
    setDrafts(draftsFromDocument(next))
  }

  useEffect(() => {
    if (!isConnected) {
      setDoc(null)
      setDrafts(null)
      setProvidersDraft(null)
      setProxiesDraft([])
      return
    }
    let cancelled = false
    setLoading(true)
    setError('')
    void Promise.all([
      api.getAppConfig(),
      api.getProvidersConfig(),
      api.getProxiesConfig ? api.getProxiesConfig() : Promise.resolve(null),
    ])
      .then(([nextConfig, nextProviders, nextProxies]) => {
        if (cancelled) return
        applyDocument(nextConfig)
        setProvidersDraft(draftFromResponse(nextProviders))
        const loadedProxies =
          nextProxies && nextProxies.proxies
            ? nextProxies.proxies
            : nextProviders.proxies ?? []
        setProxiesDraft(loadedProxies)
        if (nextProviders.availableProviders.length > 0) {
          setNewProviderSelect(nextProviders.availableProviders[0] ?? '')
        }
      })
      .catch(cause => {
        if (cancelled) return
        setError(messageFrom(cause, 'Failed to load OpenDuck configuration'))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [api, isConnected])

  const saveProviders = async () => {
    if (!providersDraft) return
    setSaving('providers')
    setError('')
    setNotice('')
    try {
      const putBody = buildPutBody(providersDraft)
      const saved = await api.updateProvidersConfig(putBody)
      setProvidersDraft(draftFromResponse(saved))
      if (saved.proxies) {
        setProxiesDraft(saved.proxies)
      }
      if (saved.availableProviders.length > 0) {
        setNewProviderSelect(saved.availableProviders[0] ?? '')
      }
      setNotice('Provider settings saved.')
    } catch (cause) {
      setError(messageFrom(cause, 'Failed to save provider settings'))
    } finally {
      setSaving(null)
    }
  }

  const saveProxies = async () => {
    setSaving('proxies')
    setError('')
    setNotice('')
    try {
      const saved = await api.updateProxiesConfig({ proxies: proxiesDraft })
      setProxiesDraft(saved.proxies)
      if (providersDraft) {
        setProvidersDraft({ ...providersDraft, proxies: saved.proxies })
      }
      setNotice('Network proxy settings saved.')
    } catch (cause) {
      setError(messageFrom(cause, 'Failed to save network proxies'))
    } finally {
      setSaving(null)
    }
  }

  const handleAddProxy = () => {
    setProxyFormError('')
    const idErr = validateProxyId(newProxyId, proxiesDraft)
    if (idErr) {
      setProxyFormError(idErr)
      return
    }
    const urlErr = validateProxyUrl(newProxyUrl)
    if (urlErr) {
      setProxyFormError(urlErr)
      return
    }
    const newItem: ProxyItem = {
      id: newProxyId.trim(),
      url: newProxyUrl.trim(),
      description: newProxyDesc.trim() || null,
    }
    const nextProxies = [...proxiesDraft, newItem]
    setProxiesDraft(nextProxies)
    if (providersDraft) {
      setProvidersDraft({ ...providersDraft, proxies: nextProxies })
    }
    setNewProxyId('')
    setNewProxyUrl('')
    setNewProxyDesc('')
  }

  const handleDeleteProxy = (id: string) => {
    const nextProxies = proxiesDraft.filter(p => p.id !== id)
    setProxiesDraft(nextProxies)
    if (providersDraft) {
      setProvidersDraft({ ...providersDraft, proxies: nextProxies })
    }
    if (editingProxyId === id) {
      setEditingProxyId(null)
    }
  }

  const handleStartEditProxy = (proxy: ProxyItem) => {
    setEditingProxyId(proxy.id)
    setEditProxyUrl(proxy.url)
    setEditProxyDesc(proxy.description ?? '')
    setProxyFormError('')
  }

  const handleSaveEditProxy = (id: string) => {
    const urlErr = validateProxyUrl(editProxyUrl)
    if (urlErr) {
      setProxyFormError(urlErr)
      return
    }
    const nextProxies = proxiesDraft.map(p =>
      p.id === id
        ? { ...p, url: editProxyUrl.trim(), description: editProxyDesc.trim() || null }
        : p,
    )
    setProxiesDraft(nextProxies)
    if (providersDraft) {
      setProvidersDraft({ ...providersDraft, proxies: nextProxies })
    }
    setEditingProxyId(null)
    setProxyFormError('')
  }

  const saveMode = async () => {
    if (!doc || !drafts) return
    setSaving('mode')
    setError('')
    setNotice('')
    try {
      let latest = doc
      const mode = drafts.mode.trim()
      if (!latest.mode.fromEnv && mode !== (latest.mode.value ?? '')) {
        latest = mode
          ? await api.upsertAppConfig('GOOSE_MODE', mode)
          : await api.removeAppConfig('GOOSE_MODE')
      }
      applyDocument(latest)
      setNotice('Tool mode saved.')
    } catch (cause) {
      setError(messageFrom(cause, 'Failed to save tool mode'))
    } finally {
      setSaving(null)
    }
  }

  const saveHarness = async () => {
    if (!doc || !drafts) return
    setSaving('harness')
    setError('')
    setNotice('')
    try {
      if (harnessDraftUnchanged(doc, drafts.harness)) {
        setNotice('No harness changes to save.')
        return
      }
      const saved = await api.updateHarnessConfig(harnessUpdateForSave(doc, drafts.harness))
      applyDocument(saved)
      setNotice('Harness settings saved.')
    } catch (cause) {
      setError(messageFrom(cause, 'Failed to save harness settings'))
    } finally {
      setSaving(null)
    }
  }

  const saveJudge = async () => {
    if (!drafts) return
    setSaving('judge')
    setError('')
    setNotice('')
    try {
      const saved = await api.updateJudgeConfig(judgeUpdateFromDraft(drafts.judge))
      applyDocument(saved)
      setNotice('Judge settings saved.')
    } catch (cause) {
      setError(messageFrom(cause, 'Failed to save judge settings'))
    } finally {
      setSaving(null)
    }
  }

  const removeJudge = async () => {
    setSaving('judge-remove')
    setError('')
    setNotice('')
    try {
      const saved = await api.removeJudgeConfig()
      applyDocument(saved)
      setNotice('Judge block removed. Built-in defaults apply.')
    } catch (cause) {
      setError(messageFrom(cause, 'Failed to remove the judge block'))
    } finally {
      setSaving(null)
    }
  }

  const saveEntry = async (key: string) => {
    if (!doc || !drafts) return
    const entry = doc.entries.find(item => item.key === key)
    if (!entry) return
    setSaving(key)
    setError('')
    setNotice('')
    try {
      const value = parseScalarInput(drafts.entries[key] ?? '', entry.value)
      const saved = await api.upsertAppConfig(key, value)
      applyDocument(saved)
      setNotice(`Saved ${configLabel(key)}.`)
    } catch (cause) {
      setError(messageFrom(cause, `Failed to save ${key}`))
    } finally {
      setSaving(null)
    }
  }

  const removeEntry = async (key: string) => {
    setSaving(`remove:${key}`)
    setError('')
    setNotice('')
    try {
      const saved = await api.removeAppConfig(key)
      applyDocument(saved)
      setNotice(`Removed ${configLabel(key)}.`)
    } catch (cause) {
      setError(messageFrom(cause, `Failed to remove ${key}`))
    } finally {
      setSaving(null)
    }
  }

  const addEntry = async () => {
    setError('')
    setNotice('')
    try {
      const key = validateNewConfigKey(newKey)
      setSaving('add')
      const saved = await api.upsertAppConfig(key, newValue)
      applyDocument(saved)
      setNewKey('')
      setNewValue('')
      setNotice(`Added ${configLabel(key)}.`)
    } catch (cause) {
      setError(messageFrom(cause, 'Failed to add setting'))
    } finally {
      setSaving(null)
    }
  }

  const addJudgePoint = () => {
    if (!drafts) return
    const id = newPointId.trim()
    if (!id) {
      setError('Enter a decision point id')
      return
    }
    if (drafts.judge.points.some(point => point.id === id)) {
      setError(`Decision point ${id} is already listed`)
      return
    }
    setError('')
    setDrafts({
      ...drafts,
      judge: { ...drafts.judge, points: [...drafts.judge.points, { id, mode: 'off' }] },
    })
    setNewPointId('')
  }

  if (!isConnected) {
    return (
      <div className="panel openduck-config-panel">
        <h3>OpenDuck configuration</h3>
        <p className="muted">Connect to the OpenDuck server to edit its configuration.</p>
      </div>
    )
  }

  return (
    <div className="panel openduck-config-panel">
      <h3>OpenDuck configuration</h3>
      <p className="muted">
        Provider, tool mode, harness guardrails, judge, and other values from the OpenDuck config
        file. API keys stay in the system keyring or secrets.yaml.
      </p>
      {error && <div className="error config-banner">{error}</div>}
      {notice && <div className="toast-inline config-banner">{notice}</div>}
      {loading && !drafts ? (
        <p className="muted">Loading configuration…</p>
      ) : drafts && doc && providersDraft ? (
        <>
          <section className="config-section">
            <h4>Providers</h4>
            <p className="form-hint">
              Configure primary and secondary fallback providers and models.
            </p>

            <div className="config-form-grid" style={{ marginBottom: '1rem' }}>
              <label>
                Major provider
                {providersDraft.majorFromEnv && <span className="config-env-note">From environment</span>}
                <select
                  value={providersDraft.majorProvider ?? ''}
                  disabled={providersDraft.majorFromEnv || saving === 'providers'}
                  onChange={e => {
                    const val = e.target.value || null
                    setProvidersDraft({ ...providersDraft, majorProvider: val })
                  }}
                >
                  <option value="">Unset (clear)</option>
                  {providersDraft.providers
                    .filter(p => p.enabled && p.model.trim())
                    .map(p => (
                      <option key={p.name} value={p.name}>
                        {p.name}
                      </option>
                    ))}
                  {providersDraft.majorProvider &&
                    !providersDraft.providers.some(
                      p => p.enabled && p.model.trim() && p.name === providersDraft.majorProvider,
                    ) && (
                      <option value={providersDraft.majorProvider}>
                        {providersDraft.majorProvider}
                      </option>
                    )}
                </select>
              </label>

              <label>
                Secondary provider (fallback)
                {providersDraft.secondaryFromEnv && <span className="config-env-note">From environment</span>}
                <select
                  value={providersDraft.secondaryProvider ?? ''}
                  disabled={providersDraft.secondaryFromEnv || saving === 'providers'}
                  onChange={e => {
                    const val = e.target.value || null
                    setProvidersDraft({ ...providersDraft, secondaryProvider: val })
                  }}
                >
                  <option value="">None</option>
                  {providersDraft.providers
                    .filter(p => p.enabled && p.model.trim() && p.name !== providersDraft.majorProvider)
                    .map(p => (
                      <option key={p.name} value={p.name}>
                        {p.name}
                      </option>
                    ))}
                  {providersDraft.secondaryProvider &&
                    !providersDraft.providers.some(
                      p =>
                        p.enabled &&
                        p.model.trim() &&
                        p.name !== providersDraft.majorProvider &&
                        p.name === providersDraft.secondaryProvider,
                    ) && (
                      <option value={providersDraft.secondaryProvider}>
                        {providersDraft.secondaryProvider}
                      </option>
                    )}
                </select>
              </label>
            </div>

            <div className="provider-table-wrapper" style={{ overflowX: 'auto', marginTop: '1rem' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left' }}>
                <thead>
                  <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    <th style={{ padding: '0.5rem' }}>Name</th>
                    <th style={{ padding: '0.5rem' }}>Enabled</th>
                    <th style={{ padding: '0.5rem' }}>Model</th>
                    <th style={{ padding: '0.5rem' }}>Fast model</th>
                    <th style={{ padding: '0.5rem' }}>Secondary model</th>
                    <th style={{ padding: '0.5rem' }}>Proxy</th>
                    <th style={{ padding: '0.5rem', width: '2.5rem' }}></th>
                  </tr>
                </thead>
                <tbody>
                  {providersDraft.providers.length === 0 ? (
                    <tr>
                      <td colSpan={7} style={{ padding: '1rem', textAlign: 'center' }} className="muted">
                        No providers configured. Add one below.
                      </td>
                    </tr>
                  ) : (
                    providersDraft.providers.map((row, idx) => {
                      const isMajorRow =
                        (providersDraft.majorFromEnv
                          ? providersDraft.majorFileValue
                          : providersDraft.majorProvider) === row.name
                      const isSecondaryRow =
                        (providersDraft.secondaryFromEnv
                          ? providersDraft.secondaryFileValue
                          : providersDraft.secondaryProvider) === row.name
                      const modelLocked =
                        (providersDraft.modelFromEnv && isMajorRow) ||
                        (providersDraft.secondaryModelFromEnv && isSecondaryRow)
                      const fastModelLocked = providersDraft.fastModelFromEnv

                      return (
                        <tr key={row.name} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                          <td style={{ padding: '0.5rem', fontWeight: 600 }}>{row.name}</td>
                          <td style={{ padding: '0.5rem' }}>
                            <input
                              type="checkbox"
                              checked={row.enabled}
                              disabled={saving === 'providers'}
                              onChange={e => {
                                const next = [...providersDraft.providers]
                                next[idx] = { ...row, enabled: e.target.checked }
                                setProvidersDraft({ ...providersDraft, providers: next })
                              }}
                            />
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            <input
                              value={
                                modelLocked
                                  ? ((isMajorRow ? row.modelEnvValue : row.secondaryModelEnvValue) ??
                                    row.model)
                                  : row.model
                              }
                              disabled={modelLocked || saving === 'providers'}
                              onChange={e => {
                                const next = [...providersDraft.providers]
                                next[idx] = { ...row, model: e.target.value }
                                setProvidersDraft({ ...providersDraft, providers: next })
                              }}
                              placeholder="e.g. gpt-4o"
                              style={{ width: '100%', minWidth: '8rem' }}
                            />
                            {modelLocked && <div className="config-env-note">From env</div>}
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            <input
                              value={
                                fastModelLocked
                                  ? (row.fastModelEnvValue ?? row.fastModel)
                                  : row.fastModel
                              }
                              disabled={fastModelLocked || saving === 'providers'}
                              onChange={e => {
                                const next = [...providersDraft.providers]
                                next[idx] = { ...row, fastModel: e.target.value }
                                setProvidersDraft({ ...providersDraft, providers: next })
                              }}
                              placeholder={
                                row.suggestedFastModel
                                  ? `Suggested: ${row.suggestedFastModel}`
                                  : 'uses model'
                              }
                              style={{ width: '100%', minWidth: '8rem' }}
                            />
                            <div className="form-hint" style={{ fontSize: '0.75rem', marginTop: '0.2rem' }}>
                              {fastModelLabel(row.fastModel, row.model)}
                            </div>
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            <input
                              value={row.secondaryModel}
                              disabled={saving === 'providers'}
                              onChange={e => {
                                const next = [...providersDraft.providers]
                                next[idx] = { ...row, secondaryModel: e.target.value }
                                setProvidersDraft({ ...providersDraft, providers: next })
                              }}
                              placeholder="no secondary model"
                              style={{ width: '100%', minWidth: '8rem' }}
                            />
                            <div className="form-hint" style={{ fontSize: '0.75rem', marginTop: '0.2rem' }}>
                              {secondaryModelLabel(row.secondaryModel)}
                            </div>
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            {(() => {
                              const matchedProxyId = findMatchingProxyId(row.proxy, proxiesDraft)
                              const isCustom = Boolean(row.proxy.trim() && !matchedProxyId)
                              const selectVal = matchedProxyId ? matchedProxyId : isCustom ? '__custom__' : ''

                              return (
                                <div style={{ display: 'flex', flexDirection: 'column', gap: '0.25rem' }}>
                                  <select
                                    value={selectVal}
                                    disabled={saving === 'providers'}
                                    style={{ width: '100%', minWidth: '9.5rem' }}
                                    onChange={e => {
                                      const val = e.target.value
                                      const next = [...providersDraft.providers]
                                      if (val === '') {
                                        next[idx] = { ...row, proxy: '' }
                                      } else if (val === '__custom__') {
                                        const customVal = isCustom ? row.proxy : 'http://'
                                        next[idx] = { ...row, proxy: customVal }
                                      } else {
                                        next[idx] = { ...row, proxy: val }
                                      }
                                      setProvidersDraft({ ...providersDraft, providers: next })
                                    }}
                                  >
                                    <option value="">None (direct)</option>
                                    {proxiesDraft.map(p => (
                                      <option key={p.id} value={p.id}>
                                        {p.id} ({p.url})
                                      </option>
                                    ))}
                                    <option value="__custom__">Custom URL…</option>
                                  </select>

                                  {selectVal === '__custom__' && (
                                    <input
                                      value={row.proxy}
                                      disabled={saving === 'providers'}
                                      onChange={e => {
                                        const next = [...providersDraft.providers]
                                        next[idx] = { ...row, proxy: e.target.value }
                                        setProvidersDraft({ ...providersDraft, providers: next })
                                      }}
                                      placeholder="http://127.0.0.1:8080"
                                      style={{ width: '100%', minWidth: '9.5rem', marginTop: '0.2rem' }}
                                    />
                                  )}

                                  <div className="form-hint" style={{ fontSize: '0.75rem', marginTop: '0.15rem' }}>
                                    {row.proxy.trim() ? (
                                      matchedProxyId ? (
                                        <span title={`Resolved to ${proxiesDraft.find(p => p.id === matchedProxyId)?.url}`}>
                                          Using {matchedProxyId}
                                        </span>
                                      ) : (
                                        <span>Custom proxy</span>
                                      )
                                    ) : (
                                      'direct'
                                    )}
                                  </div>
                                </div>
                              )
                            })()}
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            {row.name !== providersDraft.majorProvider &&
                              row.name !== providersDraft.secondaryProvider && (
                                <button
                                  type="button"
                                  className="btn-icon"
                                  title={`Remove ${row.name}`}
                                  disabled={saving === 'providers'}
                                  onClick={() => {
                                    const next = providersDraft.providers.filter((_, i) => i !== idx)
                                    const available = [
                                      ...providersDraft.availableProviders,
                                      row.name,
                                    ].sort()
                                    setProvidersDraft({
                                      ...providersDraft,
                                      providers: next,
                                      availableProviders: available,
                                    })
                                  }}
                                >
                                  <Trash2 size={16} />
                                </button>
                              )}
                          </td>
                        </tr>
                      )
                    })
                  )}
                </tbody>
              </table>
            </div>

            <div className="config-add-row" style={{ marginTop: '1rem' }}>
              <select
                value={newProviderSelect}
                disabled={providersDraft.availableProviders.length === 0 || saving === 'providers'}
                onChange={e => setNewProviderSelect(e.target.value)}
              >
                {providersDraft.availableProviders.length === 0 ? (
                  <option value="">No more providers in registry</option>
                ) : (
                  providersDraft.availableProviders.map(p => (
                    <option key={p} value={p}>
                      {p}
                    </option>
                  ))
                )}
              </select>
              <div></div>
              <button
                type="button"
                className="btn-secondary"
                disabled={
                  providersDraft.availableProviders.length === 0 ||
                  !newProviderSelect ||
                  saving === 'providers'
                }
                onClick={() => {
                  if (!newProviderSelect) return
                  const next = addProvider(providersDraft, newProviderSelect)
                  setProvidersDraft(next)
                  setNewProviderSelect(next.availableProviders[0] ?? '')
                }}
              >
                Add provider
              </button>
            </div>

            <div className="settings-actions-row" style={{ marginTop: '1rem' }}>
              <button
                type="button"
                className="btn-primary"
                disabled={saving === 'providers'}
                onClick={() => void saveProviders()}
              >
                {saving === 'providers' ? 'Saving…' : 'Save provider settings'}
              </button>
            </div>
          </section>

          <section className="config-section">
            <h4>Network Proxies</h4>
            <p className="form-hint">
              Pre-configure reusable HTTP, HTTPS, or SOCKS5 proxies to easily assign to model providers.
            </p>

            <div className="provider-table-wrapper" style={{ overflowX: 'auto', marginTop: '0.75rem' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left' }}>
                <thead>
                  <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    <th style={{ padding: '0.5rem' }}>Proxy ID</th>
                    <th style={{ padding: '0.5rem' }}>Proxy URL</th>
                    <th style={{ padding: '0.5rem' }}>Description</th>
                    <th style={{ padding: '0.5rem' }}>Used By</th>
                    <th style={{ padding: '0.5rem', width: '5.5rem' }}>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {proxiesDraft.length === 0 ? (
                    <tr>
                      <td colSpan={5} style={{ padding: '1rem', textAlign: 'center' }} className="muted">
                        No proxies configured yet. Add one below to use with providers.
                      </td>
                    </tr>
                  ) : (
                    proxiesDraft.map(p => {
                      const isEditing = editingProxyId === p.id
                      const providersUsing = (providersDraft?.providers ?? [])
                        .filter(row => row.proxy.trim() === p.id || row.proxy.trim() === p.url)
                        .map(row => row.name)

                      return (
                        <tr key={p.id} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                          <td style={{ padding: '0.5rem', fontWeight: 600 }}>
                            <span className="badge" style={{ fontSize: '0.75rem', fontFamily: 'monospace' }}>
                              {p.id}
                            </span>
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            {isEditing ? (
                              <input
                                value={editProxyUrl}
                                onChange={e => setEditProxyUrl(e.target.value)}
                                placeholder="e.g. http://127.0.0.1:7890"
                                style={{ width: '100%', minWidth: '12rem' }}
                              />
                            ) : (
                              <span style={{ fontFamily: 'monospace', fontSize: '0.85rem' }}>{p.url}</span>
                            )}
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            {isEditing ? (
                              <input
                                value={editProxyDesc}
                                onChange={e => setEditProxyDesc(e.target.value)}
                                placeholder="Optional description"
                                style={{ width: '100%', minWidth: '10rem' }}
                              />
                            ) : (
                              <span className="muted" style={{ fontSize: '0.85rem' }}>
                                {p.description || '—'}
                              </span>
                            )}
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            {providersUsing.length > 0 ? (
                              <div style={{ display: 'flex', gap: '0.25rem', flexWrap: 'wrap' }}>
                                {providersUsing.map(pName => (
                                  <span key={pName} className="badge" style={{ fontSize: '0.7rem' }}>
                                    {pName}
                                  </span>
                                ))}
                              </div>
                            ) : (
                              <span className="muted" style={{ fontSize: '0.8rem' }}>none</span>
                            )}
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            <div style={{ display: 'flex', gap: '0.35rem' }}>
                              {isEditing ? (
                                <>
                                  <button
                                    type="button"
                                    className="btn-icon"
                                    title="Save proxy"
                                    onClick={() => handleSaveEditProxy(p.id)}
                                  >
                                    <Check size={16} />
                                  </button>
                                  <button
                                    type="button"
                                    className="btn-icon"
                                    title="Cancel"
                                    onClick={() => {
                                      setEditingProxyId(null)
                                      setProxyFormError('')
                                    }}
                                  >
                                    <X size={16} />
                                  </button>
                                </>
                              ) : (
                                <>
                                  <button
                                    type="button"
                                    className="btn-icon"
                                    title="Edit proxy"
                                    disabled={saving === 'proxies'}
                                    onClick={() => handleStartEditProxy(p)}
                                  >
                                    <Edit2 size={15} />
                                  </button>
                                  <button
                                    type="button"
                                    className="btn-icon"
                                    title={`Delete proxy ${p.id}`}
                                    disabled={saving === 'proxies'}
                                    onClick={() => handleDeleteProxy(p.id)}
                                  >
                                    <Trash2 size={16} />
                                  </button>
                                </>
                              )}
                            </div>
                          </td>
                        </tr>
                      )
                    })
                  )}
                </tbody>
              </table>
            </div>

            <div
              style={{
                marginTop: '1rem',
                padding: '0.85rem',
                background: 'var(--color-bg-secondary, rgba(255,255,255,0.03))',
                borderRadius: '6px',
                border: '1px solid var(--border-subtle)',
              }}
            >
              <div style={{ fontWeight: 600, fontSize: '0.85rem', marginBottom: '0.5rem' }}>
                Add New Proxy
              </div>
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr)) auto',
                  gap: '0.6rem',
                  alignItems: 'start',
                }}
              >
                <div>
                  <input
                    value={newProxyId}
                    placeholder="ID (e.g. clash)"
                    onChange={e => {
                      setNewProxyId(e.target.value)
                      setProxyFormError('')
                    }}
                    disabled={saving === 'proxies'}
                    style={{ width: '100%' }}
                  />
                  <div className="form-hint" style={{ fontSize: '0.7rem' }}>Identifier</div>
                </div>
                <div>
                  <input
                    value={newProxyUrl}
                    placeholder="URL (e.g. http://127.0.0.1:7890)"
                    onChange={e => {
                      setNewProxyUrl(e.target.value)
                      setProxyFormError('')
                    }}
                    disabled={saving === 'proxies'}
                    style={{ width: '100%' }}
                  />
                  <div className="form-hint" style={{ fontSize: '0.7rem' }}>http://, https://, socks5://</div>
                </div>
                <div>
                  <input
                    value={newProxyDesc}
                    placeholder="Description (optional)"
                    onChange={e => setNewProxyDesc(e.target.value)}
                    disabled={saving === 'proxies'}
                    style={{ width: '100%' }}
                  />
                  <div className="form-hint" style={{ fontSize: '0.7rem' }}>Optional label</div>
                </div>
                <div>
                  <button
                    type="button"
                    className="secondary"
                    onClick={handleAddProxy}
                    disabled={saving === 'proxies' || !newProxyId.trim() || !newProxyUrl.trim()}
                    style={{ whiteSpace: 'nowrap', marginTop: '1px' }}
                  >
                    Add Proxy
                  </button>
                </div>
              </div>
              {proxyFormError && (
                <div style={{ color: 'var(--color-danger, #ef4444)', fontSize: '0.8rem', marginTop: '0.4rem' }}>
                  {proxyFormError}
                </div>
              )}
            </div>

            <div className="settings-actions-row" style={{ marginTop: '1rem' }}>
              <button
                type="button"
                className="btn-primary"
                disabled={saving === 'proxies'}
                onClick={() => void saveProxies()}
              >
                {saving === 'proxies' ? 'Saving…' : 'Save proxies'}
              </button>
            </div>
          </section>

          <section className="config-section">
            <h4>Tool mode</h4>
            <div className="config-form-grid">
              <label>
                Mode
                {doc.mode.fromEnv && <span className="config-env-note">From environment</span>}
                <select
                  value={drafts.mode}
                  disabled={doc.mode.fromEnv || saving === 'mode'}
                  onChange={event => setDrafts({ ...drafts, mode: event.target.value })}
                >
                  {MODE_OPTIONS.map(option => (
                    <option key={option.value || 'unset'} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                  {drafts.mode && !MODE_OPTIONS.some(option => option.value === drafts.mode) && (
                    <option value={drafts.mode}>{drafts.mode}</option>
                  )}
                </select>
              </label>
            </div>
            <div className="settings-actions-row">
              <button
                type="button"
                className="btn-primary"
                disabled={saving === 'mode'}
                onClick={() => void saveMode()}
              >
                {saving === 'mode' ? 'Saving…' : 'Save tool mode'}
              </button>
            </div>
          </section>

          <section className="config-section">
            <h4>Harness</h4>
            <p className="form-hint">
              Global defaults for harness runs. A project harness file can override the turn budget.
              Leave a number blank to use the built-in default.
            </p>
            <div className="config-form-grid">
              <label>
                Max turns
                {doc.harness.maxTurns.fromEnv && <span className="config-env-note">From environment</span>}
                <input
                  inputMode="numeric"
                  value={drafts.harness.maxTurns}
                  disabled={doc.harness.maxTurns.fromEnv || saving === 'harness'}
                  placeholder="25"
                  onChange={event =>
                    setDrafts({ ...drafts, harness: { ...drafts.harness, maxTurns: event.target.value } })
                  }
                />
                <span className="form-hint">Unset uses 25 when a project omits its own budget.</span>
              </label>
              <label>
                Stagnation threshold
                {doc.harness.stagnationThreshold.fromEnv && (
                  <span className="config-env-note">From environment</span>
                )}
                <input
                  inputMode="numeric"
                  value={drafts.harness.stagnationThreshold}
                  disabled={doc.harness.stagnationThreshold.fromEnv || saving === 'harness'}
                  placeholder="6"
                  onChange={event =>
                    setDrafts({
                      ...drafts,
                      harness: { ...drafts.harness, stagnationThreshold: event.target.value },
                    })
                  }
                />
                <span className="form-hint">Unset uses 6.</span>
              </label>
              <label>
                Exploration budget
                {doc.harness.explorationBudget.fromEnv && (
                  <span className="config-env-note">From environment</span>
                )}
                <input
                  inputMode="numeric"
                  value={drafts.harness.explorationBudget}
                  disabled={doc.harness.explorationBudget.fromEnv || saving === 'harness'}
                  placeholder="Optional"
                  onChange={event =>
                    setDrafts({
                      ...drafts,
                      harness: { ...drafts.harness, explorationBudget: event.target.value },
                    })
                  }
                />
              </label>
              <label>
                Review interval
                {doc.harness.periodicReviewInterval.fromEnv && (
                  <span className="config-env-note">From environment</span>
                )}
                <input
                  inputMode="numeric"
                  value={drafts.harness.periodicReviewInterval}
                  disabled={doc.harness.periodicReviewInterval.fromEnv || saving === 'harness'}
                  placeholder="Optional"
                  onChange={event =>
                    setDrafts({
                      ...drafts,
                      harness: { ...drafts.harness, periodicReviewInterval: event.target.value },
                    })
                  }
                />
              </label>
              <label>
                Circuit breaker
                {doc.harness.circuitBreakerEnabled.fromEnv && (
                  <span className="config-env-note">From environment</span>
                )}
                <select
                  value={drafts.harness.circuitBreaker}
                  disabled={doc.harness.circuitBreakerEnabled.fromEnv || saving === 'harness'}
                  onChange={event =>
                    setDrafts({
                      ...drafts,
                      harness: {
                        ...drafts.harness,
                        circuitBreaker: event.target.value as ConfigDrafts['harness']['circuitBreaker'],
                      },
                    })
                  }
                >
                  <option value="default">Default (on)</option>
                  <option value="on">On</option>
                  <option value="off">Off</option>
                </select>
              </label>
            </div>
            <div className="settings-actions-row">
              <button type="button" className="btn-primary" disabled={saving === 'harness'} onClick={() => void saveHarness()}>
                {saving === 'harness' ? 'Saving…' : 'Save harness settings'}
              </button>
            </div>
          </section>

          <section className="config-section">
            <h4>Judge</h4>
            <p className="form-hint">
              {doc.judge.configured
                ? 'This judge block is stored in the OpenDuck config file. The judge runs only when a decision point is shadow or active.'
                : 'No judge block is stored yet. These are the built-in defaults. Save to write them into the config file.'}
            </p>
            <div className="config-form-grid">
              <label>
                Provider
                <input
                  value={drafts.judge.provider}
                  disabled={saving === 'judge'}
                  onChange={event =>
                    setDrafts({ ...drafts, judge: { ...drafts.judge, provider: event.target.value } })
                  }
                />
              </label>
              <label>
                Endpoint
                <input
                  value={drafts.judge.endpoint}
                  disabled={saving === 'judge'}
                  onChange={event =>
                    setDrafts({ ...drafts, judge: { ...drafts.judge, endpoint: event.target.value } })
                  }
                />
              </label>
              <label>
                Timeout (ms)
                <input
                  inputMode="numeric"
                  value={drafts.judge.timeoutMs}
                  disabled={saving === 'judge'}
                  onChange={event =>
                    setDrafts({ ...drafts, judge: { ...drafts.judge, timeoutMs: event.target.value } })
                  }
                />
              </label>
              <label className="checkbox-row config-checkbox">
                <input
                  type="checkbox"
                  checked={drafts.judge.fallbackOnError}
                  disabled={saving === 'judge'}
                  onChange={event =>
                    setDrafts({
                      ...drafts,
                      judge: { ...drafts.judge, fallbackOnError: event.target.checked },
                    })
                  }
                />
                Continue when the judge service errors
              </label>
            </div>
            <div className="config-entry-list">
              {drafts.judge.points.map(point => (
                <div className="config-point-row" key={point.id}>
                  <label>
                    {JUDGE_POINT_LABELS[point.id] ?? point.id}
                    <input className="readonly-input" value={point.id} readOnly />
                  </label>
                  <label>
                    Mode
                    <select
                      value={point.mode}
                      disabled={saving === 'judge'}
                      onChange={event =>
                        setDrafts({
                          ...drafts,
                          judge: {
                            ...drafts.judge,
                            points: drafts.judge.points.map(item =>
                              item.id === point.id ? { ...item, mode: event.target.value } : item,
                            ),
                          },
                        })
                      }
                    >
                      {JUDGE_MODES.map(option => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                  </label>
                  {KNOWN_JUDGE_POINTS.has(point.id) ? (
                    <span />
                  ) : (
                    <button
                      type="button"
                      className="icon-btn"
                      aria-label={`Remove ${point.id}`}
                      onClick={() =>
                        setDrafts({
                          ...drafts,
                          judge: {
                            ...drafts.judge,
                            points: drafts.judge.points.filter(item => item.id !== point.id),
                          },
                        })
                      }
                    >
                      <Trash2 size={16} />
                    </button>
                  )}
                </div>
              ))}
            </div>
            <div className="config-add-row">
              <label>
                Decision point
                <input
                  value={newPointId}
                  placeholder="custom.point"
                  onChange={event => setNewPointId(event.target.value)}
                />
              </label>
              <div className="config-entry-actions">
                <button type="button" className="secondary" onClick={addJudgePoint}>
                  Add point
                </button>
              </div>
            </div>
            <div className="settings-actions-row">
              <button type="button" className="btn-primary" disabled={saving === 'judge'} onClick={() => void saveJudge()}>
                {saving === 'judge' ? 'Saving…' : 'Save judge settings'}
              </button>
              {doc.judge.configured && (
                <button
                  type="button"
                  className="danger"
                  disabled={saving === 'judge-remove'}
                  onClick={() => void removeJudge()}
                >
                  {saving === 'judge-remove' ? 'Removing…' : 'Remove judge block'}
                </button>
              )}
            </div>
          </section>

          <section className="config-section">
            <h4>Other settings</h4>
            <p className="form-hint">Scalar values in the config file. Extensions stay in the extensions block.</p>
            {doc.entries.length === 0 ? (
              <p className="muted">No extra settings yet.</p>
            ) : (
              <div className="config-entry-list">
                {doc.entries.map(entry => (
                  <div className="config-entry-row" key={entry.key}>
                    <label>
                      {configLabel(entry.key)}
                      {entry.fromEnv && <span className="config-env-note">From environment</span>}
                      <input className="readonly-input" value={entry.key} readOnly />
                    </label>
                    {typeof entry.value === 'boolean' ? (
                      <label>
                        Value
                        <select
                          value={drafts.entries[entry.key] ?? formatScalar(entry.value)}
                          disabled={entry.fromEnv || saving === entry.key}
                          onChange={event =>
                            setDrafts({
                              ...drafts,
                              entries: { ...drafts.entries, [entry.key]: event.target.value },
                            })
                          }
                        >
                          <option value="true">true</option>
                          <option value="false">false</option>
                        </select>
                      </label>
                    ) : (
                      <label>
                        Value
                        <input
                          value={drafts.entries[entry.key] ?? ''}
                          disabled={entry.fromEnv || saving === entry.key}
                          onChange={event =>
                            setDrafts({
                              ...drafts,
                              entries: { ...drafts.entries, [entry.key]: event.target.value },
                            })
                          }
                        />
                      </label>
                    )}
                    <div className="config-entry-actions">
                      <button
                        type="button"
                        className="secondary"
                        disabled={entry.fromEnv || saving === entry.key}
                        onClick={() => void saveEntry(entry.key)}
                      >
                        {saving === entry.key ? 'Saving…' : 'Save'}
                      </button>
                      <button
                        type="button"
                        className="icon-btn"
                        aria-label={`Remove ${entry.key}`}
                        disabled={entry.fromEnv || saving === `remove:${entry.key}`}
                        onClick={() => void removeEntry(entry.key)}
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
            <div className="config-add-row">
              <label>
                Name
                <input
                  value={newKey}
                  placeholder="OLLAMA_HOST"
                  onChange={event => setNewKey(event.target.value)}
                />
              </label>
              <label>
                Value
                <input
                  value={newValue}
                  placeholder="http://127.0.0.1:11434"
                  onChange={event => setNewValue(event.target.value)}
                />
              </label>
              <div className="config-entry-actions">
                <button type="button" className="secondary" disabled={saving === 'add'} onClick={() => void addEntry()}>
                  {saving === 'add' ? 'Adding…' : 'Add setting'}
                </button>
              </div>
            </div>
          </section>
        </>
      ) : null}
    </div>
  )
}
