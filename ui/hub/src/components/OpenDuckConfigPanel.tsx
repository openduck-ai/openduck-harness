import { useEffect, useState } from 'react'
import type { createApi } from '@aaif/goose-hub-core'
import type { AppConfigDocument } from '@aaif/goose-hub-core'
import { Trash2 } from 'lucide-react'
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
      return
    }
    let cancelled = false
    setLoading(true)
    setError('')
    void api
      .getAppConfig()
      .then(next => {
        if (cancelled) return
        applyDocument(next)
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

  const saveDefaults = async () => {
    if (!doc || !drafts) return
    setSaving('defaults')
    setError('')
    setNotice('')
    try {
      let latest = doc
      const provider = drafts.provider.trim()
      const model = drafts.model.trim()
      const mode = drafts.mode.trim()
      if (!doc.provider.fromEnv && provider !== (doc.provider.value ?? '')) {
        latest = provider
          ? await api.upsertAppConfig('GOOSE_PROVIDER', provider)
          : await api.removeAppConfig('GOOSE_PROVIDER')
      }
      if (!latest.model.fromEnv && model !== (latest.model.value ?? '')) {
        latest = model
          ? await api.upsertAppConfig('GOOSE_MODEL', model)
          : await api.removeAppConfig('GOOSE_MODEL')
      }
      if (!latest.mode.fromEnv && mode !== (latest.mode.value ?? '')) {
        latest = mode
          ? await api.upsertAppConfig('GOOSE_MODE', mode)
          : await api.removeAppConfig('GOOSE_MODE')
      }
      applyDocument(latest)
      setNotice(latest === doc ? 'No model changes to save.' : 'Model settings saved.')
    } catch (cause) {
      setError(messageFrom(cause, 'Failed to save model settings'))
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
      ) : drafts && doc ? (
        <>
          <section className="config-section">
            <h4>Model</h4>
            <div className="config-form-grid">
              <label>
                Provider
                {doc.provider.fromEnv && <span className="config-env-note">From environment</span>}
                <input
                  value={drafts.provider}
                  disabled={doc.provider.fromEnv || saving === 'defaults'}
                  onChange={event => setDrafts({ ...drafts, provider: event.target.value })}
                  placeholder="openai"
                />
              </label>
              <label>
                Model
                {doc.model.fromEnv && <span className="config-env-note">From environment</span>}
                <input
                  value={drafts.model}
                  disabled={doc.model.fromEnv || saving === 'defaults'}
                  onChange={event => setDrafts({ ...drafts, model: event.target.value })}
                  placeholder="gpt-4o"
                />
              </label>
              <label>
                Tool mode
                {doc.mode.fromEnv && <span className="config-env-note">From environment</span>}
                <select
                  value={drafts.mode}
                  disabled={doc.mode.fromEnv || saving === 'defaults'}
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
              <button type="button" className="btn-primary" disabled={saving === 'defaults'} onClick={() => void saveDefaults()}>
                {saving === 'defaults' ? 'Saving…' : 'Save model settings'}
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
