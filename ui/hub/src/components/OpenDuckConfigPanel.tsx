import { ConfigValidationError, type ConfigMessageDescriptor } from '../configEditor.ts'
import type { translate } from '../i18n/index.ts'
import { useTranslation } from 'react-i18next'
import { useEffect, useRef, useState } from 'react'
import type { AppConfigDocument, ProxyItem, TuiAgentProxyDocument, createApi } from '@aaif/goose-hub-core'
import { Check, Edit2, Trash2, X } from 'lucide-react'
import {
  CUSTOM_MODEL_OPTION,
  getLegacyProviderServerNotice,
  addProvider,
  applyProviderInventoryIfCurrent,
  buildPutBody,
  draftFromResponse,
  envLockDescription,
  fastModelLabel,
  filterModelOptions,
  findMatchingProxyId,
  identicalRoleTargetWarning,
  inventoryStatusLabel,
  modelPriceLabel,
  markProviderRefreshFailed,
  markProviderRefreshing,
  modelChoiceState,
  nextRefreshGeneration,
  refreshGuardIsCurrent,
  removeProvider,
  roleProviderOptions,
  secondaryModelLabel,
  selectRoleModel,
  selectRoleProvider,
  updateProviderRow,
  validateProxyIdMessage as validateProxyId,
  validateProxyUrlMessage as validateProxyUrl,
  type ProviderConfigDraft,
  type ProviderRole,
  type ProviderRowDraft,
} from '../providerConfig'
import {
  getJudgeModes,
  getJudgePointLabels,
  getModeOptions,
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
import { validateProxyUrlMessage } from '../providerConfig'

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
  | 'refreshProviderModels'
  | 'getProxiesConfig'
  | 'updateProxiesConfig'
  | 'getTuiAgentProxies'
  | 'updateTuiAgentProxies'
>

type TuiProxyMode = 'inherit' | 'direct' | 'url'

interface TuiProxyDraftRow {
  agent: string
  mode: TuiProxyMode
  url: string
}

const DEFAULT_TUI_PROXY_AGENTS = ['default', 'agy', 'grok', 'codex', 'claude']

function draftsFromTuiProxies(doc: TuiAgentProxyDocument | null): TuiProxyDraftRow[] {
  const agents = doc?.agents?.length
    ? doc.agents
    : DEFAULT_TUI_PROXY_AGENTS.map((agent) => ({ agent, httpsProxy: null }))
  return agents.map((row) => {
    const value = row.httpsProxy?.trim() ?? ''
    if (!value) return { agent: row.agent, mode: 'inherit', url: '' }
    if (value === 'direct') return { agent: row.agent, mode: 'direct', url: '' }
    return { agent: row.agent, mode: 'url', url: value }
  })
}

interface OpenDuckConfigPanelProps {
  api: ConfigApi
  isConnected: boolean
}

const KNOWN_JUDGE_POINTS = new Set(['turn.drift', 'turn.completion', 'context.forget', 'tool.risk'])

type ConfigMessage = string | ConfigMessageDescriptor

function renderConfigMessage(message: ConfigMessage, t: typeof translate): string {
  return typeof message === 'string' ? message : t(message.key, message.values?.(t))
}

function messageFrom(error: unknown, fallback: ConfigMessage): ConfigMessage
function messageFrom(error: unknown, fallback: ConfigMessage): ConfigMessage {
  if (error instanceof ConfigValidationError) return { key: error.key, values: error.values }
  return error instanceof Error && error.message ? error.message : fallback
}

function RoleModelEditor({
  draft,
  role,
  saving,
  query,
  onQuery,
  onModel,
}: {
  draft: ProviderConfigDraft
  role: ProviderRole
  saving: boolean
  query: string
  onQuery: (value: string) => void
  onModel: (model: string | null) => void
}) {
  const { t } = useTranslation()
  const providerName = role === 'major' ? draft.majorProvider : draft.secondaryProvider
  const fromEnv =
    role === 'major'
      ? draft.majorModelFromEnv || draft.modelFromEnv
      : draft.secondaryProviderModelFromEnv || draft.secondaryModelFromEnv
  const explicit = role === 'major' ? draft.majorModel : draft.secondaryProviderModel
  const fileValue = role === 'major' ? draft.majorModelFileValue : draft.secondaryModelFileValue
  const effectiveProvider =
    role === 'major' ? draft.majorModelEffectiveProvider : draft.secondaryModelEffectiveProvider
  const storedEffective =
    role === 'major' ? draft.majorModelEffective : draft.secondaryModelEffective
  const effective = providerName && effectiveProvider === providerName ? storedEffective : null
  const row = draft.providers.find((provider) => provider.name === providerName)
  const [enteringCustom, setEnteringCustom] = useState(false)
  useEffect(() => {
    setEnteringCustom(false)
  }, [providerName])

  if (!providerName) {
    return <p className="form-hint">{t('config.panel.selectProviderFirst')}</p>
  }
  if (fromEnv) {
    return (
      <div>
        <input value={effective ?? ''} disabled readOnly />
        <div className="config-env-note">{envLockDescription(fileValue, effective)}</div>
      </div>
    )
  }

  const choice = modelChoiceState({
    models: row?.models,
    defaultModel: row?.model ?? '',
    explicitModel: explicit,
    effectiveModel: effective,
  })
  if (choice.freeText) {
    return (
      <div>
        <input
          value={explicit ?? ''}
          disabled={saving}
          placeholder={
            row?.model.trim()
              ? t('config.panel.inheritModelPlaceholder', { model: row.model.trim() })
              : t('config.panel.modelId')
          }
          onChange={(event) => onModel(event.target.value === '' ? null : event.target.value)}
          style={{ width: '100%' }}
        />
        {row?.models && row.models.length === 0 && (
          <div className="form-hint">{t('config.panel.noModels')}</div>
        )}
        {row?.lastRefreshError && (
          <div className="form-hint">
            {renderConfigMessage(row.lastRefreshError, t)} {t('config.panel.retryModelHint')}
          </div>
        )}
        {!row?.model.trim() && !explicit?.trim() && (
          <div className="form-hint">{t('config.panel.noDefaultModel')}</div>
        )}
      </div>
    )
  }

  const visible = choice.searchable
    ? filterModelOptions(choice.options, query, choice.selectValue)
    : choice.options
  return (
    <div>
      {choice.searchable && (
        <input
          value={query}
          disabled={saving}
          placeholder={t('config.panel.searchModels')}
          onChange={(event) => onQuery(event.target.value)}
          style={{ width: '100%', marginBottom: '0.35rem' }}
        />
      )}
      <select
        value={choice.selectValue}
        disabled={saving}
        onChange={(event) => {
          const value = event.target.value
          if (value === CUSTOM_MODEL_OPTION) {
            setEnteringCustom(true)
            return
          }
          setEnteringCustom(false)
          onModel(value === '' ? null : value)
        }}
      >
        {!row?.model.trim() && <option value="">{t('config.panel.selectModel')}</option>}
        {visible.map((option) => (
          <option key={option.inherit ? '__inherit__' : option.value} value={option.value}>
            {option.label}
          </option>
        ))}
        <option value={CUSTOM_MODEL_OPTION}>{t('config.panel.enterModelId')}</option>
      </select>
      {enteringCustom && (
        <input
          value={explicit ?? ''}
          disabled={saving}
          placeholder={t('config.panel.modelId')}
          onChange={(event) => onModel(event.target.value === '' ? null : event.target.value)}
          style={{ width: '100%', marginTop: '0.35rem' }}
        />
      )}
    </div>
  )
}

function ProviderSettingsTable({
  draft,
  proxies,
  saving,
  onDraft,
  onRefresh,
}: {
  draft: ProviderConfigDraft
  proxies: ProxyItem[]
  saving: boolean
  onDraft: (next: ProviderConfigDraft) => void
  onRefresh: (providerId: string) => void
}) {
  const { t } = useTranslation()
  const legacy = draft.selectionVersion !== 2
  return (
    <div className="provider-table-wrapper" style={{ overflowX: 'auto', marginTop: '1rem' }}>
      <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left' }}>
        <thead>
          <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
            <th style={{ padding: '0.5rem' }}>{t('config.panel.name')}</th>
            <th style={{ padding: '0.5rem' }}>{t('config.panel.enabled')}</th>
            <th style={{ padding: '0.5rem' }}>{t('config.panel.defaultModel')}</th>
            <th style={{ padding: '0.5rem' }}>{t('config.panel.advanced')}</th>
            <th style={{ padding: '0.5rem' }}>{t('config.panel.proxy')}</th>
            <th style={{ padding: '0.5rem' }}>{t('config.panel.models')}</th>
            <th style={{ padding: '0.5rem', width: '2.5rem' }}></th>
          </tr>
        </thead>
        <tbody>
          {draft.providers.length === 0 ? (
            <tr>
              <td colSpan={7} style={{ padding: '1rem', textAlign: 'center' }} className="muted">
                {t('config.panel.noProviders')}
              </td>
            </tr>
          ) : (
            draft.providers.map((row, idx) => (
              <ProviderSettingsRow
                key={row.name}
                draft={draft}
                row={row}
                index={idx}
                proxies={proxies}
                saving={saving}
                legacy={legacy}
                onDraft={onDraft}
                onRefresh={onRefresh}
              />
            ))
          )}
        </tbody>
      </table>
    </div>
  )
}

function ProviderSettingsRow({
  draft,
  row,
  index,
  proxies,
  saving,
  legacy,
  onDraft,
  onRefresh,
}: {
  draft: ProviderConfigDraft
  row: ProviderRowDraft
  index: number
  proxies: ProxyItem[]
  saving: boolean
  legacy: boolean
  onDraft: (next: ProviderConfigDraft) => void
  onRefresh: (providerId: string) => void
}) {
  const { t } = useTranslation()
  const isMajorRow = (draft.majorFromEnv ? draft.majorFileValue : draft.majorProvider) === row.name
  const isSecondaryRow =
    (draft.secondaryFromEnv ? draft.secondaryFileValue : draft.secondaryProvider) === row.name
  const modelLocked =
    (draft.modelFromEnv && isMajorRow) || (draft.secondaryModelFromEnv && isSecondaryRow)
  const fastModelLocked = draft.fastModelFromEnv
  const matchedProxyId = findMatchingProxyId(row.proxy, proxies)
  const isCustomProxy = Boolean(row.proxy.trim() && !matchedProxyId)
  const selectVal = matchedProxyId ? matchedProxyId : isCustomProxy ? '__custom__' : ''
  const statusLabel = inventoryStatusLabel(row.inventoryStatus)
  const shownModel = (
    modelLocked
      ? ((isMajorRow ? row.modelEnvValue : row.secondaryModelEnvValue) ?? row.model)
      : row.model
  ).trim()
  const shownModelPrice = modelPriceLabel(row.models?.find((model) => model.id === shownModel))
  const refreshDisabled = saving || legacy || row.supportsRefresh === false || row.refreshing
  const refreshLabel = row.refreshing
    ? t('config.panel.refreshing')
    : row.lastRefreshError
      ? t('config.panel.retryRefresh')
      : t('config.panel.refreshModels')

  return (
    <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
      <td style={{ padding: '0.5rem', fontWeight: 600 }}>{row.name}</td>
      <td style={{ padding: '0.5rem' }}>
        <input
          type="checkbox"
          checked={row.enabled}
          disabled={saving}
          onChange={(event) =>
            onDraft(updateProviderRow(draft, index, { enabled: event.target.checked }))
          }
        />
      </td>
      <td style={{ padding: '0.5rem' }}>
        <input
          value={
            modelLocked
              ? ((isMajorRow ? row.modelEnvValue : row.secondaryModelEnvValue) ?? row.model)
              : row.model
          }
          disabled={modelLocked || saving}
          onChange={(event) =>
            onDraft(updateProviderRow(draft, index, { model: event.target.value }))
          }
          placeholder={t('config.panel.optionalLowercase')}
          style={{ width: '100%', minWidth: '8rem' }}
        />
        {modelLocked && (
          <div className="config-env-note">
            {envLockDescription(
              row.fileModel,
              (isMajorRow ? row.modelEnvValue : row.secondaryModelEnvValue) ?? row.model,
            )}
          </div>
        )}
        {shownModelPrice && (
          <div className="form-hint" style={{ fontSize: '0.75rem', marginTop: '0.2rem' }}>
            {shownModelPrice}
          </div>
        )}
      </td>
      <td style={{ padding: '0.5rem' }}>
        <details>
          <summary>{t('config.panel.advanced')}</summary>
          <label style={{ display: 'block', marginTop: '0.4rem' }}>
            {t('config.panel.fastModel')}
            <input
              value={fastModelLocked ? (row.fastModelEnvValue ?? row.fastModel) : row.fastModel}
              disabled={fastModelLocked || saving}
              onChange={(event) =>
                onDraft(updateProviderRow(draft, index, { fastModel: event.target.value }))
              }
              placeholder={
                row.suggestedFastModel
                  ? t('config.panel.suggestedModel', { model: row.suggestedFastModel })
                  : t('config.panel.usesModel')
              }
              style={{ width: '100%', minWidth: '8rem' }}
            />
            <div className="form-hint" style={{ fontSize: '0.75rem', marginTop: '0.2rem' }}>
              {fastModelLabel(row.fastModel, row.model)}
            </div>
            {fastModelLocked && (
              <div className="config-env-note">
                {envLockDescription(row.fileFastModel, row.fastModelEnvValue ?? row.fastModel)}
              </div>
            )}
          </label>
          <label style={{ display: 'block', marginTop: '0.4rem' }}>
            {t('config.panel.secondaryModel')}
            <input
              value={row.secondaryModel}
              disabled={saving}
              onChange={(event) =>
                onDraft(updateProviderRow(draft, index, { secondaryModel: event.target.value }))
              }
              placeholder={t('config.panel.noSecondaryModel')}
              style={{ width: '100%', minWidth: '8rem' }}
            />
            <div className="form-hint" style={{ fontSize: '0.75rem', marginTop: '0.2rem' }}>
              {secondaryModelLabel(row.secondaryModel)}
            </div>
          </label>
        </details>
      </td>
      <td style={{ padding: '0.5rem' }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: '0.25rem' }}>
          <select
            value={selectVal}
            disabled={saving}
            style={{ width: '100%', minWidth: '9.5rem' }}
            onChange={(event) => {
              const val = event.target.value
              if (val === '') onDraft(updateProviderRow(draft, index, { proxy: '' }))
              else if (val === '__custom__') {
                onDraft(
                  updateProviderRow(draft, index, { proxy: isCustomProxy ? row.proxy : 'http://' }),
                )
              } else onDraft(updateProviderRow(draft, index, { proxy: val }))
            }}
          >
            <option value="">{t('config.panel.directOption')}</option>
            {proxies.map((proxy) => (
              <option key={proxy.id} value={proxy.id}>
                {proxy.id} ({proxy.url})
              </option>
            ))}
            <option value="__custom__">{t('config.panel.customUrl')}</option>
          </select>
          {selectVal === '__custom__' && (
            <input
              value={row.proxy}
              disabled={saving}
              onChange={(event) =>
                onDraft(updateProviderRow(draft, index, { proxy: event.target.value }))
              }
              placeholder="http://127.0.0.1:8080"
              style={{ width: '100%', minWidth: '9.5rem', marginTop: '0.2rem' }}
            />
          )}
          <div className="form-hint" style={{ fontSize: '0.75rem', marginTop: '0.15rem' }}>
            {row.proxy.trim() ? (
              matchedProxyId ? (
                <span
                  title={t('config.panel.resolvedProxy', {
                    url: String(proxies.find((proxy) => proxy.id === matchedProxyId)?.url),
                  })}
                >
                  {t('config.panel.usingProxy', { id: matchedProxyId })}
                </span>
              ) : (
                <span>{t('config.panel.customProxy')}</span>
              )
            ) : (
              t('config.panel.direct')
            )}
          </div>
        </div>
      </td>
      <td style={{ padding: '0.5rem' }}>
        <button
          type="button"
          className="btn-secondary"
          disabled={refreshDisabled}
          title={
            legacy
              ? t('config.panel.refreshNeedsNewServer')
              : (row.lastRefreshError ? renderConfigMessage(row.lastRefreshError, t) : t('config.panel.refreshModelList'))
          }
          onClick={() => onRefresh(row.name)}
        >
          {refreshLabel}
        </button>
        {row.models && row.models.length === 0 && (
          <div className="form-hint">{t('config.panel.noModels')}</div>
        )}
        {statusLabel && <div className="form-hint">{statusLabel}</div>}
        {row.lastRefreshError && <div className="form-hint">{renderConfigMessage(row.lastRefreshError, t)}</div>}
      </td>
      <td style={{ padding: '0.5rem' }}>
        {row.name !== draft.majorProvider && row.name !== draft.secondaryProvider && (
          <button
            type="button"
            className="btn-icon"
            title={t('config.panel.removeItem', { name: row.name })}
            disabled={saving}
            onClick={() => onDraft(removeProvider(draft, row.name))}
          >
            <Trash2 size={16} />
          </button>
        )}
      </td>
    </tr>
  )
}

export function OpenDuckConfigPanel({ api, isConnected }: OpenDuckConfigPanelProps) {
  const { t } = useTranslation()
  const [doc, setDoc] = useState<AppConfigDocument | null>(null)
  const [drafts, setDrafts] = useState<ConfigDrafts | null>(null)
  const [providersDraft, setProvidersDraft] = useState<ProviderConfigDraft | null>(null)
  const [proxiesDraft, setProxiesDraft] = useState<ProxyItem[]>([])
  const [tuiProxyDraft, setTuiProxyDraft] = useState<TuiProxyDraftRow[]>(() =>
    draftsFromTuiProxies(null),
  )
  const [tuiProxyUnavailable, setTuiProxyUnavailable] = useState(false)
  const [newProxyId, setNewProxyId] = useState('')
  const [newProxyUrl, setNewProxyUrl] = useState('')
  const [newProxyDesc, setNewProxyDesc] = useState('')
  const [proxyFormError, setProxyFormError] = useState<ConfigMessage>('')
  const [editingProxyId, setEditingProxyId] = useState<string | null>(null)
  const [editProxyUrl, setEditProxyUrl] = useState('')
  const [editProxyDesc, setEditProxyDesc] = useState('')
  const [newProviderSelect, setNewProviderSelect] = useState('')
  const [modelQuery, setModelQuery] = useState<{ major: string; secondary: string }>({
    major: '',
    secondary: '',
  })
  const refreshGenerations = useRef<Record<string, number>>({})
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<ConfigMessage>('')
  const [notice, setNotice] = useState<ConfigMessage>('')
  const [saving, setSaving] = useState<string | null>(null)
  const [newKey, setNewKey] = useState('')
  const [newValue, setNewValue] = useState('')
  const [newPointId, setNewPointId] = useState('')

  const renderMessage = (message: ConfigMessage) => renderConfigMessage(message, t)

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
      setTuiProxyDraft(draftsFromTuiProxies(null))
      setTuiProxyUnavailable(false)
      return
    }
    let cancelled = false
    setLoading(true)
    setError('')
    void Promise.all([
      api.getAppConfig(),
      api.getProvidersConfig(),
      api.getProxiesConfig ? api.getProxiesConfig() : Promise.resolve(null),
      api.getTuiAgentProxies().catch(() => null),
    ])
      .then(([nextConfig, nextProviders, nextProxies, nextTuiProxies]) => {
        if (cancelled) return
        applyDocument(nextConfig)
        setProvidersDraft(draftFromResponse(nextProviders))
        const loadedProxies =
          nextProxies && nextProxies.proxies ? nextProxies.proxies : (nextProviders.proxies ?? [])
        setProxiesDraft(loadedProxies)
        setTuiProxyUnavailable(nextTuiProxies == null)
        setTuiProxyDraft(draftsFromTuiProxies(nextTuiProxies))
        if (nextProviders.availableProviders.length > 0) {
          setNewProviderSelect(nextProviders.availableProviders[0] ?? '')
        }
      })
      .catch((cause) => {
        if (cancelled) return
        setError(messageFrom(cause, { key: 'config.panel.loadFailed' }))
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
      setNotice({ key: 'config.panel.providersSaved' })
    } catch (cause) {
      setError(messageFrom(cause, { key: 'config.panel.saveProvidersFailed' }))
    } finally {
      setSaving(null)
    }
  }

  const refreshModels = async (providerId: string) => {
    if (!providersDraft || providersDraft.selectionVersion !== 2) return
    const started = nextRefreshGeneration(refreshGenerations.current, providerId)
    refreshGenerations.current = started.generations
    const guard = started.guard
    setProvidersDraft((current) =>
      current ? markProviderRefreshing(current, providerId, true) : current,
    )
    try {
      const plan = await api.refreshProviderModels(providerId)
      const latest = await api.getProvidersConfig()
      if (!refreshGuardIsCurrent(refreshGenerations.current, guard)) return
      setProvidersDraft((current) =>
        current
          ? applyProviderInventoryIfCurrent(current, refreshGenerations.current, guard, latest)
          : current,
      )
      const skip = plan.skipped.find((item) => item.providerId === providerId)
      if (!skip || !refreshGuardIsCurrent(refreshGenerations.current, guard)) return
      setProvidersDraft((current) =>
        current && refreshGuardIsCurrent(refreshGenerations.current, guard)
          ? markProviderRefreshFailed(current, providerId, skip.reason)
          : current,
      )
    } catch (cause) {
      if (!refreshGuardIsCurrent(refreshGenerations.current, guard)) return
      const message = messageFrom(cause, { key: 'config.panel.refreshFailed' })
      setProvidersDraft((current) =>
        current && refreshGuardIsCurrent(refreshGenerations.current, guard)
          ? markProviderRefreshFailed(current, providerId, message)
          : current,
      )
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
      setNotice({ key: 'config.panel.proxiesSaved' })
    } catch (cause) {
      setError(messageFrom(cause, { key: 'config.panel.saveProxiesFailed' }))
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
    const nextProxies = proxiesDraft.filter((p) => p.id !== id)
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
    const nextProxies = proxiesDraft.map((p) =>
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
      setNotice({ key: 'config.panel.modeSaved' })
    } catch (cause) {
      setError(messageFrom(cause, { key: 'config.panel.saveModeFailed' }))
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
        setNotice({ key: 'config.panel.harnessUnchanged' })
        return
      }
      const saved = await api.updateHarnessConfig(harnessUpdateForSave(doc, drafts.harness))
      applyDocument(saved)
      setNotice({ key: 'config.panel.harnessSaved' })
    } catch (cause) {
      setError(messageFrom(cause, { key: 'config.panel.saveHarnessFailed' }))
    } finally {
      setSaving(null)
    }
  }

  const saveTuiProxies = async () => {
    setSaving('tui-proxies')
    setError('')
    setNotice('')
    for (const row of tuiProxyDraft) {
      if (row.mode !== 'url') continue
      const invalid = validateProxyUrlMessage(row.url)
      if (invalid) {
        setError(invalid)
        setSaving(null)
        return
      }
    }
    try {
      const saved = await api.updateTuiAgentProxies({
        agents: tuiProxyDraft.map((row) => ({
          agent: row.agent,
          httpsProxy: row.mode === 'direct' ? 'direct' : row.mode === 'url' ? row.url.trim() : null,
        })),
      })
      setTuiProxyUnavailable(false)
      setTuiProxyDraft(draftsFromTuiProxies(saved))
      setNotice({ key: 'config.panel.tuiAgentProxiesSaved' })
    } catch (cause) {
      setError(messageFrom(cause, { key: 'config.panel.saveTuiAgentProxiesFailed' }))
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
      setNotice({ key: 'config.panel.judgeSaved' })
    } catch (cause) {
      setError(messageFrom(cause, { key: 'config.panel.saveJudgeFailed' }))
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
      setNotice({ key: 'config.panel.judgeRemoved' })
    } catch (cause) {
      setError(messageFrom(cause, { key: 'config.panel.removeJudgeFailed' }))
    } finally {
      setSaving(null)
    }
  }

  const saveEntry = async (key: string) => {
    if (!doc || !drafts) return
    const entry = doc.entries.find((item) => item.key === key)
    if (!entry) return
    setSaving(key)
    setError('')
    setNotice('')
    try {
      const value = parseScalarInput(drafts.entries[key] ?? '', entry.value)
      const saved = await api.upsertAppConfig(key, value)
      applyDocument(saved)
      setNotice({ key: 'config.panel.settingSaved', values: () => ({ label: configLabel(key) }) })
    } catch (cause) {
      setError(
        messageFrom(cause, { key: 'config.panel.saveSettingFailed', values: () => ({ key }) }),
      )
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
      setNotice({ key: 'config.panel.settingRemoved', values: () => ({ label: configLabel(key) }) })
    } catch (cause) {
      setError(
        messageFrom(cause, { key: 'config.panel.removeSettingFailed', values: () => ({ key }) }),
      )
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
      setNotice({ key: 'config.panel.settingAdded', values: () => ({ label: configLabel(key) }) })
    } catch (cause) {
      setError(messageFrom(cause, { key: 'config.panel.addSettingFailed' }))
    } finally {
      setSaving(null)
    }
  }

  const addJudgePoint = () => {
    if (!drafts) return
    const id = newPointId.trim()
    if (!id) {
      setError({ key: 'config.panel.enterPointId' })
      return
    }
    if (drafts.judge.points.some((point) => point.id === id)) {
      setError({ key: 'config.panel.pointAlreadyListed', values: () => ({ id }) })
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
      <div
        id="settings-openduck"
        data-settings-section="settings-openduck"
        className="panel openduck-config-panel"
      >
        <h3>{t('config.panel.title')}</h3>
        <p className="muted">{t('config.panel.connectHint')}</p>
      </div>
    )
  }

  return (
    <div id="settings-openduck" className="panel openduck-config-panel">
      <h3>{t('config.panel.title')}</h3>
      <p className="muted">{t('config.panel.description')}</p>
      {error && <div className="error config-banner">{renderMessage(error)}</div>}
      {notice && <div className="toast-inline config-banner">{renderMessage(notice)}</div>}
      {loading && !drafts ? (
        <p className="muted">{t('config.panel.loading')}</p>
      ) : drafts && doc && providersDraft ? (
        <>
          <section
            id="settings-providers"
            data-settings-section="settings-providers"
            className="config-section"
          >
            <h4>{t('config.panel.providers')}</h4>
            <p className="form-hint">{t('config.panel.providersHint')}</p>
            {providersDraft.selectionVersion !== 2 && (
              <p className="form-hint">{getLegacyProviderServerNotice()}</p>
            )}

            <div className="config-form-grid" style={{ marginBottom: '1rem' }}>
              <label>
                {t('config.panel.majorProvider')}
                {providersDraft.majorFromEnv && (
                  <span className="config-env-note">
                    {envLockDescription(
                      providersDraft.majorFileValue,
                      providersDraft.majorProvider,
                    )}
                  </span>
                )}
                <select
                  value={providersDraft.majorProvider ?? ''}
                  disabled={providersDraft.majorFromEnv || saving === 'providers'}
                  onChange={(event) => {
                    const value = event.target.value || null
                    setModelQuery((current) => ({ ...current, major: '' }))
                    setProvidersDraft((current) =>
                      current ? selectRoleProvider(current, 'major', value) : current,
                    )
                  }}
                >
                  <option value="">{t('config.panel.unsetClear')}</option>
                  {roleProviderOptions(providersDraft, 'major').map((option) => (
                    <option key={option.name} value={option.name}>
                      {option.eligible
                        ? option.name
                        : t('config.panel.unavailableProvider', { name: option.name })}
                    </option>
                  ))}
                </select>
              </label>

              {providersDraft.selectionVersion === 2 && (
                <label>
                  {t('config.panel.majorModel')}
                  <RoleModelEditor
                    draft={providersDraft}
                    role="major"
                    saving={saving === 'providers'}
                    query={modelQuery.major}
                    onQuery={(value) => setModelQuery((current) => ({ ...current, major: value }))}
                    onModel={(model) =>
                      setProvidersDraft((current) =>
                        current ? selectRoleModel(current, 'major', model) : current,
                      )
                    }
                  />
                </label>
              )}

              <label>
                {t('config.panel.secondaryProvider')}
                {providersDraft.secondaryFromEnv && (
                  <span className="config-env-note">
                    {envLockDescription(
                      providersDraft.secondaryFileValue,
                      providersDraft.secondaryProvider,
                    )}
                  </span>
                )}
                <select
                  value={providersDraft.secondaryProvider ?? ''}
                  disabled={providersDraft.secondaryFromEnv || saving === 'providers'}
                  onChange={(event) => {
                    const value = event.target.value || null
                    setModelQuery((current) => ({ ...current, secondary: '' }))
                    setProvidersDraft((current) =>
                      current ? selectRoleProvider(current, 'secondary', value) : current,
                    )
                  }}
                >
                  <option value="">{t('config.panel.none')}</option>
                  {roleProviderOptions(providersDraft, 'secondary').map((option) => (
                    <option key={option.name} value={option.name}>
                      {option.eligible
                        ? option.name
                        : t('config.panel.unavailableProvider', { name: option.name })}
                    </option>
                  ))}
                </select>
              </label>

              {providersDraft.selectionVersion === 2 && (
                <label>
                  {t('config.panel.secondaryModel')}
                  <RoleModelEditor
                    draft={providersDraft}
                    role="secondary"
                    saving={saving === 'providers'}
                    query={modelQuery.secondary}
                    onQuery={(value) =>
                      setModelQuery((current) => ({ ...current, secondary: value }))
                    }
                    onModel={(model) =>
                      setProvidersDraft((current) =>
                        current ? selectRoleModel(current, 'secondary', model) : current,
                      )
                    }
                  />
                </label>
              )}
            </div>
            {identicalRoleTargetWarning(providersDraft) && (
              <p className="form-hint" role="status">
                {identicalRoleTargetWarning(providersDraft)}
              </p>
            )}

            <ProviderSettingsTable
              draft={providersDraft}
              proxies={proxiesDraft}
              saving={saving === 'providers'}
              onDraft={setProvidersDraft}
              onRefresh={(providerId) => {
                void refreshModels(providerId)
              }}
            />

            <div className="config-add-row" style={{ marginTop: '1rem' }}>
              <select
                value={newProviderSelect}
                disabled={providersDraft.availableProviders.length === 0 || saving === 'providers'}
                onChange={(e) => setNewProviderSelect(e.target.value)}
              >
                {providersDraft.availableProviders.length === 0 ? (
                  <option value="">{t('config.panel.noMoreProviders')}</option>
                ) : (
                  providersDraft.availableProviders.map((p) => (
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
                {t('config.panel.addProvider')}
              </button>
            </div>

            <div className="settings-actions-row" style={{ marginTop: '1rem' }}>
              <button
                type="button"
                className="btn-primary"
                disabled={saving === 'providers' || !providersDraft.dirty}
                onClick={() => void saveProviders()}
              >
                {saving === 'providers'
                  ? t('config.panel.saving')
                  : t('config.panel.saveProviders')}
              </button>
            </div>
          </section>

          <section
            id="settings-proxies"
            data-settings-section="settings-proxies"
            className="config-section"
          >
            <h4>{t('config.panel.networkProxies')}</h4>
            <p className="form-hint">{t('config.panel.proxiesHint')}</p>

            <div
              className="provider-table-wrapper"
              style={{ overflowX: 'auto', marginTop: '0.75rem' }}
            >
              <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left' }}>
                <thead>
                  <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    <th style={{ padding: '0.5rem' }}>{t('config.panel.proxyId')}</th>
                    <th style={{ padding: '0.5rem' }}>{t('config.panel.proxyUrl')}</th>
                    <th style={{ padding: '0.5rem' }}>{t('config.panel.descriptionLabel')}</th>
                    <th style={{ padding: '0.5rem' }}>{t('config.panel.usedBy')}</th>
                    <th style={{ padding: '0.5rem', width: '5.5rem' }}>
                      {t('config.panel.actions')}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {proxiesDraft.length === 0 ? (
                    <tr>
                      <td
                        colSpan={5}
                        style={{ padding: '1rem', textAlign: 'center' }}
                        className="muted"
                      >
                        {t('config.panel.noProxies')}
                      </td>
                    </tr>
                  ) : (
                    proxiesDraft.map((p) => {
                      const isEditing = editingProxyId === p.id
                      const providersUsing = (providersDraft?.providers ?? [])
                        .filter((row) => row.proxy.trim() === p.id || row.proxy.trim() === p.url)
                        .map((row) => row.name)

                      return (
                        <tr key={p.id} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                          <td style={{ padding: '0.5rem', fontWeight: 600 }}>
                            <span
                              className="badge"
                              style={{ fontSize: '0.75rem', fontFamily: 'monospace' }}
                            >
                              {p.id}
                            </span>
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            {isEditing ? (
                              <input
                                value={editProxyUrl}
                                onChange={(e) => setEditProxyUrl(e.target.value)}
                                placeholder={t('config.panel.proxyUrlExample')}
                                style={{ width: '100%', minWidth: '12rem' }}
                              />
                            ) : (
                              <span style={{ fontFamily: 'monospace', fontSize: '0.85rem' }}>
                                {p.url}
                              </span>
                            )}
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            {isEditing ? (
                              <input
                                value={editProxyDesc}
                                onChange={(e) => setEditProxyDesc(e.target.value)}
                                placeholder={t('config.panel.optionalDescription')}
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
                                {providersUsing.map((pName) => (
                                  <span
                                    key={pName}
                                    className="badge"
                                    style={{ fontSize: '0.7rem' }}
                                  >
                                    {pName}
                                  </span>
                                ))}
                              </div>
                            ) : (
                              <span className="muted" style={{ fontSize: '0.8rem' }}>
                                {t('config.panel.noneLowercase')}
                              </span>
                            )}
                          </td>
                          <td style={{ padding: '0.5rem' }}>
                            <div style={{ display: 'flex', gap: '0.35rem' }}>
                              {isEditing ? (
                                <>
                                  <button
                                    type="button"
                                    className="btn-icon"
                                    title={t('config.panel.saveProxy')}
                                    onClick={() => handleSaveEditProxy(p.id)}
                                  >
                                    <Check size={16} />
                                  </button>
                                  <button
                                    type="button"
                                    className="btn-icon"
                                    title={t('config.panel.cancel')}
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
                                    title={t('config.panel.editProxy')}
                                    disabled={saving === 'proxies'}
                                    onClick={() => handleStartEditProxy(p)}
                                  >
                                    <Edit2 size={15} />
                                  </button>
                                  <button
                                    type="button"
                                    className="btn-icon"
                                    title={t('config.panel.deleteProxy', { id: p.id })}
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
                {t('config.panel.addNewProxy')}
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
                    placeholder={t('config.panel.proxyIdExample')}
                    onChange={(e) => {
                      setNewProxyId(e.target.value)
                      setProxyFormError('')
                    }}
                    disabled={saving === 'proxies'}
                    style={{ width: '100%' }}
                  />
                  <div className="form-hint" style={{ fontSize: '0.7rem' }}>
                    {t('config.panel.identifier')}
                  </div>
                </div>
                <div>
                  <input
                    value={newProxyUrl}
                    placeholder={t('config.panel.proxyUrlPlaceholder')}
                    onChange={(e) => {
                      setNewProxyUrl(e.target.value)
                      setProxyFormError('')
                    }}
                    disabled={saving === 'proxies'}
                    style={{ width: '100%' }}
                  />
                  <div className="form-hint" style={{ fontSize: '0.7rem' }}>
                    http://, https://, socks5://
                  </div>
                </div>
                <div>
                  <input
                    value={newProxyDesc}
                    placeholder={t('config.panel.descriptionOptional')}
                    onChange={(e) => setNewProxyDesc(e.target.value)}
                    disabled={saving === 'proxies'}
                    style={{ width: '100%' }}
                  />
                  <div className="form-hint" style={{ fontSize: '0.7rem' }}>
                    {t('config.panel.optionalLabel')}
                  </div>
                </div>
                <div>
                  <button
                    type="button"
                    className="secondary"
                    onClick={handleAddProxy}
                    disabled={saving === 'proxies' || !newProxyId.trim() || !newProxyUrl.trim()}
                    style={{ whiteSpace: 'nowrap', marginTop: '1px' }}
                  >
                    {t('config.panel.addProxy')}
                  </button>
                </div>
              </div>
              {proxyFormError && (
                <div
                  style={{
                    color: 'var(--color-danger, #ef4444)',
                    fontSize: '0.8rem',
                    marginTop: '0.4rem',
                  }}
                >
                  {renderMessage(proxyFormError)}
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
                {saving === 'proxies' ? t('config.panel.saving') : t('config.panel.saveProxies')}
              </button>
            </div>
          </section>

          <section
            id="settings-tool-mode"
            data-settings-section="settings-tool-mode"
            className="config-section"
          >
            <h4>{t('config.panel.toolMode')}</h4>
            <div className="config-form-grid">
              <label>
                {t('config.panel.mode')}
                {doc.mode.fromEnv && (
                  <span className="config-env-note">{t('config.panel.fromEnvironment')}</span>
                )}
                <select
                  value={drafts.mode}
                  disabled={doc.mode.fromEnv || saving === 'mode'}
                  onChange={(event) => setDrafts({ ...drafts, mode: event.target.value })}
                >
                  {getModeOptions().map((option) => (
                    <option key={option.value || 'unset'} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                  {drafts.mode &&
                    !getModeOptions().some((option) => option.value === drafts.mode) && (
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
                {saving === 'mode' ? t('config.panel.saving') : t('config.panel.saveToolMode')}
              </button>
            </div>
          </section>

          <section
            id="settings-harness"
            data-settings-section="settings-harness"
            className="config-section"
          >
            <h4>{t('config.panel.harness')}</h4>
            <p className="form-hint">{t('config.panel.harnessHint')}</p>
            <div className="config-form-grid">
              <label>
                {t('config.panel.maxTurns')}
                {doc.harness.maxTurns.fromEnv && (
                  <span className="config-env-note">{t('config.panel.fromEnvironment')}</span>
                )}
                <input
                  inputMode="numeric"
                  value={drafts.harness.maxTurns}
                  disabled={doc.harness.maxTurns.fromEnv || saving === 'harness'}
                  placeholder="25"
                  onChange={(event) =>
                    setDrafts({
                      ...drafts,
                      harness: { ...drafts.harness, maxTurns: event.target.value },
                    })
                  }
                />
                <span className="form-hint">{t('config.panel.maxTurnsHint')}</span>
              </label>
              <label>
                {t('config.panel.stagnationThreshold')}
                {doc.harness.stagnationThreshold.fromEnv && (
                  <span className="config-env-note">{t('config.panel.fromEnvironment')}</span>
                )}
                <input
                  inputMode="numeric"
                  value={drafts.harness.stagnationThreshold}
                  disabled={doc.harness.stagnationThreshold.fromEnv || saving === 'harness'}
                  placeholder="6"
                  onChange={(event) =>
                    setDrafts({
                      ...drafts,
                      harness: { ...drafts.harness, stagnationThreshold: event.target.value },
                    })
                  }
                />
                <span className="form-hint">{t('config.panel.stagnationHint')}</span>
              </label>
              <label>
                {t('config.panel.explorationBudget')}
                {doc.harness.explorationBudget.fromEnv && (
                  <span className="config-env-note">{t('config.panel.fromEnvironment')}</span>
                )}
                <input
                  inputMode="numeric"
                  value={drafts.harness.explorationBudget}
                  disabled={doc.harness.explorationBudget.fromEnv || saving === 'harness'}
                  placeholder={t('config.panel.optional')}
                  onChange={(event) =>
                    setDrafts({
                      ...drafts,
                      harness: { ...drafts.harness, explorationBudget: event.target.value },
                    })
                  }
                />
              </label>
              <label>
                {t('config.panel.reviewInterval')}
                {doc.harness.periodicReviewInterval.fromEnv && (
                  <span className="config-env-note">{t('config.panel.fromEnvironment')}</span>
                )}
                <input
                  inputMode="numeric"
                  value={drafts.harness.periodicReviewInterval}
                  disabled={doc.harness.periodicReviewInterval.fromEnv || saving === 'harness'}
                  placeholder={t('config.panel.optional')}
                  onChange={(event) =>
                    setDrafts({
                      ...drafts,
                      harness: { ...drafts.harness, periodicReviewInterval: event.target.value },
                    })
                  }
                />
              </label>
              <label>
                {t('config.panel.circuitBreaker')}
                {doc.harness.circuitBreakerEnabled.fromEnv && (
                  <span className="config-env-note">{t('config.panel.fromEnvironment')}</span>
                )}
                <select
                  value={drafts.harness.circuitBreaker}
                  disabled={doc.harness.circuitBreakerEnabled.fromEnv || saving === 'harness'}
                  onChange={(event) =>
                    setDrafts({
                      ...drafts,
                      harness: {
                        ...drafts.harness,
                        circuitBreaker: event.target
                          .value as ConfigDrafts['harness']['circuitBreaker'],
                      },
                    })
                  }
                >
                  <option value="default">{t('config.panel.defaultOn')}</option>
                  <option value="on">{t('config.panel.on')}</option>
                  <option value="off">{t('config.panel.off')}</option>
                </select>
              </label>
            </div>
            <div className="settings-actions-row">
              <button
                type="button"
                className="btn-primary"
                disabled={saving === 'harness'}
                onClick={() => void saveHarness()}
              >
                {saving === 'harness' ? t('config.panel.saving') : t('config.panel.saveHarness')}
              </button>
            </div>
          </section>

          <section
            id="settings-tui-proxies"
            data-settings-section="settings-tui-proxies"
            className="config-section"
          >
            <h4>{t('config.panel.tuiAgentProxies')}</h4>
            <p className="form-hint">{t('config.panel.tuiAgentProxiesHint')}</p>
            {tuiProxyUnavailable && (
              <p className="form-hint">{t('config.panel.tuiAgentProxiesUnavailable')}</p>
            )}
            <div className="config-form-grid">
              {tuiProxyDraft.map((row, index) => (
                <label key={row.agent}>
                  {row.agent}
                  <select
                    value={row.mode}
                    disabled={saving === 'tui-proxies'}
                    onChange={(event) => {
                      const mode = event.target.value as TuiProxyMode
                      setTuiProxyDraft((current) =>
                        current.map((item, itemIndex) =>
                          itemIndex === index ? { ...item, mode } : item,
                        ),
                      )
                    }}
                  >
                    <option value="inherit">{t('config.panel.tuiAgentProxyInherit')}</option>
                    <option value="direct">{t('config.panel.tuiAgentProxyDirect')}</option>
                    <option value="url">{t('config.panel.tuiAgentProxyUrl')}</option>
                  </select>
                  {row.mode === 'url' && (
                    <input
                      value={row.url}
                      disabled={saving === 'tui-proxies'}
                      placeholder={t('config.panel.tuiAgentProxyUrlPlaceholder')}
                      onChange={(event) => {
                        const url = event.target.value
                        setTuiProxyDraft((current) =>
                          current.map((item, itemIndex) =>
                            itemIndex === index ? { ...item, url } : item,
                          ),
                        )
                      }}
                    />
                  )}
                </label>
              ))}
            </div>
            <div className="settings-actions-row">
              <button
                type="button"
                className="btn-primary"
                disabled={saving === 'tui-proxies'}
                onClick={() => void saveTuiProxies()}
              >
                {saving === 'tui-proxies'
                  ? t('config.panel.saving')
                  : t('config.panel.saveTuiAgentProxies')}
              </button>
            </div>
          </section>

          <section
            id="settings-judge"
            data-settings-section="settings-judge"
            className="config-section"
          >
            <h4>{t('config.panel.judge')}</h4>
            <p className="form-hint">
              {doc.judge.configured
                ? t('config.panel.judgeConfiguredHint')
                : t('config.panel.judgeDefaultHint')}
            </p>
            <div className="config-form-grid">
              <label>
                {t('config.panel.provider')}
                <input
                  value={drafts.judge.provider}
                  disabled={saving === 'judge'}
                  onChange={(event) =>
                    setDrafts({
                      ...drafts,
                      judge: { ...drafts.judge, provider: event.target.value },
                    })
                  }
                />
              </label>
              <label>
                {t('config.panel.endpoint')}
                <input
                  value={drafts.judge.endpoint}
                  disabled={saving === 'judge'}
                  onChange={(event) =>
                    setDrafts({
                      ...drafts,
                      judge: { ...drafts.judge, endpoint: event.target.value },
                    })
                  }
                />
              </label>
              <label>
                {t('config.panel.timeoutMs')}
                <input
                  inputMode="numeric"
                  value={drafts.judge.timeoutMs}
                  disabled={saving === 'judge'}
                  onChange={(event) =>
                    setDrafts({
                      ...drafts,
                      judge: { ...drafts.judge, timeoutMs: event.target.value },
                    })
                  }
                />
              </label>
              <label className="checkbox-row config-checkbox">
                <input
                  type="checkbox"
                  checked={drafts.judge.fallbackOnError}
                  disabled={saving === 'judge'}
                  onChange={(event) =>
                    setDrafts({
                      ...drafts,
                      judge: { ...drafts.judge, fallbackOnError: event.target.checked },
                    })
                  }
                />
                {t('config.panel.continueOnJudgeError')}
              </label>
            </div>
            <div className="config-entry-list">
              {drafts.judge.points.map((point) => (
                <div className="config-point-row" key={point.id}>
                  <label>
                    {getJudgePointLabels()[point.id] ?? point.id}
                    <input className="readonly-input" value={point.id} readOnly />
                  </label>
                  <label>
                    {t('config.panel.mode')}
                    <select
                      value={point.mode}
                      disabled={saving === 'judge'}
                      onChange={(event) =>
                        setDrafts({
                          ...drafts,
                          judge: {
                            ...drafts.judge,
                            points: drafts.judge.points.map((item) =>
                              item.id === point.id ? { ...item, mode: event.target.value } : item,
                            ),
                          },
                        })
                      }
                    >
                      {getJudgeModes().map((option) => (
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
                      aria-label={t('config.panel.removeItem', { name: point.id })}
                      onClick={() =>
                        setDrafts({
                          ...drafts,
                          judge: {
                            ...drafts.judge,
                            points: drafts.judge.points.filter((item) => item.id !== point.id),
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
                {t('config.panel.decisionPoint')}
                <input
                  value={newPointId}
                  placeholder="custom.point"
                  onChange={(event) => setNewPointId(event.target.value)}
                />
              </label>
              <div className="config-entry-actions">
                <button type="button" className="secondary" onClick={addJudgePoint}>
                  {t('config.panel.addPoint')}
                </button>
              </div>
            </div>
            <div className="settings-actions-row">
              <button
                type="button"
                className="btn-primary"
                disabled={saving === 'judge'}
                onClick={() => void saveJudge()}
              >
                {saving === 'judge' ? t('config.panel.saving') : t('config.panel.saveJudge')}
              </button>
              {doc.judge.configured && (
                <button
                  type="button"
                  className="danger"
                  disabled={saving === 'judge-remove'}
                  onClick={() => void removeJudge()}
                >
                  {saving === 'judge-remove'
                    ? t('config.panel.removing')
                    : t('config.panel.removeJudge')}
                </button>
              )}
            </div>
          </section>

          <section
            id="settings-other"
            data-settings-section="settings-other"
            className="config-section"
          >
            <h4>{t('config.panel.otherSettings')}</h4>
            <p className="form-hint">{t('config.panel.scalarSettingsHint')}</p>
            {doc.entries.length === 0 ? (
              <p className="muted">{t('config.panel.noExtraSettings')}</p>
            ) : (
              <div className="config-entry-list">
                {doc.entries.map((entry) => (
                  <div className="config-entry-row" key={entry.key}>
                    <label>
                      {configLabel(entry.key)}
                      {entry.fromEnv && (
                        <span className="config-env-note">{t('config.panel.fromEnvironment')}</span>
                      )}
                      <input className="readonly-input" value={entry.key} readOnly />
                    </label>
                    {typeof entry.value === 'boolean' ? (
                      <label>
                        {t('config.panel.value')}
                        <select
                          value={drafts.entries[entry.key] ?? formatScalar(entry.value)}
                          disabled={entry.fromEnv || saving === entry.key}
                          onChange={(event) =>
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
                        {t('config.panel.value')}
                        <input
                          value={drafts.entries[entry.key] ?? ''}
                          disabled={entry.fromEnv || saving === entry.key}
                          onChange={(event) =>
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
                        {saving === entry.key ? t('config.panel.saving') : t('config.panel.save')}
                      </button>
                      <button
                        type="button"
                        className="icon-btn"
                        aria-label={t('config.panel.removeItem', { name: entry.key })}
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
                {t('config.panel.name')}
                <input
                  value={newKey}
                  placeholder={t('config.stats.ollamaHostPlaceholder')}
                  onChange={(event) => setNewKey(event.target.value)}
                />
              </label>
              <label>
                {t('config.panel.value')}
                <input
                  value={newValue}
                  placeholder="http://127.0.0.1:11434"
                  onChange={(event) => setNewValue(event.target.value)}
                />
              </label>
              <div className="config-entry-actions">
                <button
                  type="button"
                  className="secondary"
                  disabled={saving === 'add'}
                  onClick={() => void addEntry()}
                >
                  {saving === 'add' ? t('config.panel.adding') : t('config.panel.addSetting')}
                </button>
              </div>
            </div>
          </section>
        </>
      ) : null}
    </div>
  )
}
