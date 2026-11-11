import { useState, useEffect, useCallback, useMemo, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { Cpu, RefreshCw, AlertCircle, CheckCircle2, X } from 'lucide-react'
import type { createApi, TuiAgentModelCatalog } from '../api.ts'
import {
  resolveActiveModel,
  normalizeCatalog,
  performModelSwitch,
  performCatalogRefresh,
} from '../tuiModelPicker.ts'

export interface TuiModelPickerModalProps {
  session: {
    id: string
    sessionName?: string
    agentKind: string
    model?: string
    activeModel?: string
    currentModel?: string
    createdByProjectId?: string | null
  } | null
  projectSlug?: string
  isOpen: boolean
  onClose: () => void
  onModelSwitched?: (sessionId: string, newModel: string) => void
  api: ReturnType<typeof createApi>
}

export function TuiModelPickerModal({
  session,
  projectSlug,
  isOpen,
  onClose,
  onModelSwitched,
  api,
}: TuiModelPickerModalProps) {
  const { t } = useTranslation()

  const [catalog, setCatalog] = useState<TuiAgentModelCatalog | null>(null)
  const [loadingCatalog, setLoadingCatalog] = useState(false)
  const [isRefreshing, setIsRefreshing] = useState(false)
  const [isSwitching, setIsSwitching] = useState(false)
  const [catalogError, setCatalogError] = useState<string | null>(null)
  const [switchError, setSwitchError] = useState<string | null>(null)
  const [refreshSuccess, setRefreshSuccess] = useState<string | null>(null)

  const [selectedModel, setSelectedModel] = useState('')
  const [isCustomMode, setIsCustomMode] = useState(false)
  const [customModelText, setCustomModelText] = useState('')

  const currentActiveModel = useMemo(() => {
    return resolveActiveModel(session)
  }, [session])

  const catalogRef = useRef<TuiAgentModelCatalog | null>(catalog)
  catalogRef.current = catalog

  const agentKind = session?.agentKind
  const sessionId = session?.id
  const sessionIdentityRef = useRef<string | null>(sessionId ?? null)
  sessionIdentityRef.current = sessionId ?? null

  const fetchCatalog = useCallback(
    async (refresh = false) => {
      if (!agentKind) return
      const targetSessionId = sessionId
      if (refresh) {
        setIsRefreshing(true)
      } else {
        setLoadingCatalog(true)
      }
      setCatalogError(null)
      setRefreshSuccess(null)

      try {
        if (refresh) {
          const outcome = await performCatalogRefresh(
            api.refreshTuiAgentModels,
            agentKind,
            catalogRef.current,
          )
          if (sessionIdentityRef.current !== targetSessionId) return
          if (outcome.success) {
            setCatalog(outcome.catalog)
            setRefreshSuccess(t('tuiAgents.modelPicker.refreshSuccess'))
          } else {
            setCatalogError(outcome.error || 'Failed to refresh catalog')
          }
        } else {
          const raw = await api.getTuiAgentModels(agentKind)
          if (sessionIdentityRef.current !== targetSessionId) return
          const normalized = normalizeCatalog(raw, agentKind)
          setCatalog(normalized)
        }
      } catch (e: any) {
        if (sessionIdentityRef.current !== targetSessionId) return
        const msg = e?.message || String(e)
        if (refresh) {
          setCatalogError(
            t('tuiAgents.modelPicker.refreshFailed', { error: msg }),
          )
        } else {
          setCatalogError(
            t('tuiAgents.modelPicker.loadCatalogFailed', { error: msg }),
          )
        }
      } finally {
        if (sessionIdentityRef.current === targetSessionId) {
          setLoadingCatalog(false)
          setIsRefreshing(false)
        }
      }
    },
    [agentKind, api, sessionId, t],
  )

  useEffect(() => {
    if (isOpen && session) {
      setCatalog(null)
      setCatalogError(null)
      setSwitchError(null)
      setRefreshSuccess(null)
      setIsCustomMode(false)
      setCustomModelText('')
      const active = resolveActiveModel(session)
      setSelectedModel(active)
      fetchCatalog(false)
    }
  }, [isOpen, sessionId, agentKind, fetchCatalog])

  // When catalog arrives, initialize selected model if empty
  useEffect(() => {
    if (catalog && !selectedModel) {
      if (currentActiveModel) {
        setSelectedModel(currentActiveModel)
      } else if (catalog.defaultModel) {
        setSelectedModel(catalog.defaultModel)
      } else if (catalog.models.length > 0) {
        setSelectedModel(catalog.models[0].id)
      }
    }
  }, [catalog, selectedModel, currentActiveModel])

  const hasActiveInCatalog = useMemo(() => {
    if (!currentActiveModel || !catalog?.models) return true
    return catalog.models.some(
      m => m.id.toLowerCase() === currentActiveModel.toLowerCase(),
    )
  }, [currentActiveModel, catalog?.models])

  if (!isOpen || !session) return null

  const targetModel = isCustomMode ? customModelText.trim() : selectedModel.trim()
  const isTargetAlreadyActive = Boolean(
    currentActiveModel && targetModel.toLowerCase() === currentActiveModel.toLowerCase(),
  )
  const canSwitch =
    Boolean(targetModel) &&
    !isTargetAlreadyActive &&
    !isSwitching &&
    !loadingCatalog

  const handleSwitch = async () => {
    if (!canSwitch) return
    const targetSessionId = session.id
    setIsSwitching(true)
    setSwitchError(null)
    setRefreshSuccess(null)

    const effectiveProjectSlug = projectSlug || session.createdByProjectId || undefined
    const switchFn = (sid: string, model: string) => {
      if (effectiveProjectSlug && typeof api.switchProjectTuiSessionModel === 'function') {
        return api.switchProjectTuiSessionModel(effectiveProjectSlug, sid, model)
      }
      return api.switchTuiSessionModel(sid, model)
    }

    const outcome = await performModelSwitch(
      switchFn,
      session,
      targetModel,
    )

    if (sessionIdentityRef.current !== targetSessionId) return

    if (outcome.success) {
      onModelSwitched?.(session.id, outcome.activeModel)
      onClose()
    } else {
      // Keep shown active model UNCHANGED; surface actionable error
      setSwitchError(outcome.error || 'Failed to switch model')
    }
    setIsSwitching(false)
  }

  const handleRefresh = () => {
    fetchCatalog(true)
  }

  return (
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
      role="dialog"
      aria-modal="true"
      aria-labelledby="tui-model-picker-title"
    >
      <div
        style={{
          width: '90%',
          maxWidth: '540px',
          background: 'var(--bg-panel, #ffffff)',
          color: 'var(--text-default, #1e293b)',
          borderRadius: '8px',
          padding: '22px',
          boxShadow: '0 12px 36px rgba(0,0,0,0.25)',
          border: '1px solid var(--border-default, #e2e8f0)',
          display: 'flex',
          flexDirection: 'column',
          gap: '16px',
        }}
      >
        {/* Header */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Cpu size={20} style={{ color: '#0284c7' }} />
            <h3 id="tui-model-picker-title" style={{ margin: 0, fontSize: '16px', fontWeight: 600 }}>
              {t('tuiAgents.modelPicker.title', {
                name: session.sessionName || session.id.slice(0, 10),
              })}
            </h3>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={t('tuiAgents.page.btnClose')}
            style={{
              background: 'transparent',
              border: 'none',
              cursor: 'pointer',
              color: 'var(--text-muted, #64748b)',
              padding: '4px',
              display: 'flex',
              alignItems: 'center',
            }}
          >
            <X size={18} />
          </button>
        </div>

        {/* Current Active Model Banner */}
        <div
          style={{
            background: 'var(--bg-active-model, #f0f9ff)',
            border: '1px solid var(--border-active-model, #bae6fd)',
            borderRadius: '6px',
            padding: '10px 14px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            fontSize: '13px',
          }}
        >
          <div>
            <span style={{ color: 'var(--text-muted, #64748b)', marginRight: '6px' }}>
              {t('tuiAgents.modelPicker.activeModelLabel')}:
            </span>
            <strong style={{ color: '#0369a1', fontFamily: 'monospace', fontSize: '13px' }}>
              {currentActiveModel || t('tuiAgents.page.default')}
            </strong>
          </div>
          <span
            style={{
              background: '#0284c7',
              color: '#ffffff',
              fontSize: '11px',
              fontWeight: 600,
              padding: '2px 8px',
              borderRadius: '10px',
            }}
          >
            {t('tuiAgents.modelPicker.activeBadge')}
          </span>
        </div>

        {/* Feedback Messages */}
        {refreshSuccess && (
          <div
            style={{
              background: '#f0fdf4',
              border: '1px solid #bbf7d0',
              color: '#166534',
              borderRadius: '6px',
              padding: '8px 12px',
              fontSize: '12px',
              display: 'flex',
              alignItems: 'center',
              gap: '6px',
            }}
          >
            <CheckCircle2 size={15} />
            <span>{refreshSuccess}</span>
          </div>
        )}

        {catalogError && (
          <div
            style={{
              background: '#fffbeb',
              border: '1px solid #fde68a',
              color: '#92400e',
              borderRadius: '6px',
              padding: '8px 12px',
              fontSize: '12px',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: '8px',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
              <AlertCircle size={15} />
              <span>{catalogError}</span>
            </div>
            <button
              type="button"
              onClick={() => fetchCatalog(false)}
              style={{
                fontSize: '11px',
                padding: '2px 6px',
                borderRadius: '4px',
                border: '1px solid #d97706',
                background: '#fff',
                cursor: 'pointer',
              }}
            >
              {t('tuiAgents.page.btnRefreshQuota') /* or Retry */}
            </button>
          </div>
        )}

        {switchError && (
          <div
            style={{
              background: '#fef2f2',
              border: '1px solid #fecaca',
              color: '#b91c1c',
              borderRadius: '6px',
              padding: '10px 12px',
              fontSize: '12px',
              display: 'flex',
              alignItems: 'flex-start',
              gap: '8px',
            }}
          >
            <AlertCircle size={16} style={{ flexShrink: 0, marginTop: '2px' }} />
            <div style={{ flex: 1, lineHeight: '1.4' }}>{switchError}</div>
          </div>
        )}

        {/* Catalog Fallback Notice */}
        {catalog?.isFallback && (
          <div
            style={{
              fontSize: '11px',
              color: 'var(--text-muted, #64748b)',
              background: 'var(--bg-subtle, #f8fafc)',
              padding: '6px 10px',
              borderRadius: '4px',
              border: '1px dashed var(--border-default, #cbd5e1)',
            }}
          >
            {t('tuiAgents.modelPicker.fallbackNotice')}
          </div>
        )}

        {/* Model Picker Controls */}
        <div>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '6px' }}>
            <label style={{ fontSize: '13px', fontWeight: 500 }}>
              {t('tuiAgents.modelPicker.selectModelLabel')}
            </label>
            <button
              type="button"
              onClick={handleRefresh}
              disabled={isRefreshing || loadingCatalog}
              style={{
                background: 'transparent',
                border: '1px solid var(--border-default, #cbd5e1)',
                borderRadius: '4px',
                padding: '3px 8px',
                fontSize: '11px',
                cursor: isRefreshing || loadingCatalog ? 'not-allowed' : 'pointer',
                display: 'flex',
                alignItems: 'center',
                gap: '4px',
                color: 'var(--text-muted, #475569)',
              }}
            >
              <RefreshCw
                size={12}
                style={{
                  animation: isRefreshing ? 'spin 1s linear infinite' : undefined,
                }}
              />
              <span>
                {isRefreshing
                  ? t('tuiAgents.modelPicker.refreshing')
                  : t('tuiAgents.modelPicker.btnRefresh')}
              </span>
            </button>
          </div>

          {loadingCatalog ? (
            <div style={{ padding: '16px', textAlign: 'center', color: 'var(--text-muted, #64748b)', fontSize: '12px' }}>
              {t('tuiAgents.modelPicker.loadingCatalog', { agent: session.agentKind })}
            </div>
          ) : isCustomMode ? (
            <div>
              <input
                type="text"
                value={customModelText}
                onChange={e => setCustomModelText(e.target.value)}
                placeholder={t('tuiAgents.modelPicker.customModelPlaceholder')}
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  borderRadius: '4px',
                  border: '1px solid var(--border-default, #cbd5e1)',
                  boxSizing: 'border-box',
                  fontSize: '13px',
                }}
                autoFocus
              />
            </div>
          ) : (
            <select
              value={selectedModel}
              onChange={e => setSelectedModel(e.target.value)}
              style={{
                width: '100%',
                padding: '8px 10px',
                borderRadius: '4px',
                border: '1px solid var(--border-default, #cbd5e1)',
                boxSizing: 'border-box',
                fontSize: '13px',
                background: 'var(--bg-input, #fff)',
                color: 'inherit',
              }}
            >
              {currentActiveModel && !hasActiveInCatalog && (
                <option key={currentActiveModel} value={currentActiveModel}>
                  {currentActiveModel} ({t('tuiAgents.modelPicker.activeBadge')})
                </option>
              )}
              {catalog?.models && catalog.models.length > 0 ? (
                catalog.models.map(m => {
                  const isActive =
                    Boolean(currentActiveModel) &&
                    m.id.toLowerCase() === currentActiveModel.toLowerCase()
                  const isDef = Boolean(catalog.defaultModel && m.id === catalog.defaultModel)
                  const tag = isActive
                    ? ` (${t('tuiAgents.modelPicker.activeBadge')})`
                    : isDef
                      ? ` (${t('tuiAgents.modelPicker.defaultBadge')})`
                      : ''
                  return (
                    <option key={m.id} value={m.id}>
                      {m.displayName || m.id}
                      {tag}
                    </option>
                  )
                })
              ) : (
                !currentActiveModel && (
                  <option value="" disabled>
                    {t('tuiAgents.modelPicker.noModelsFound')}
                  </option>
                )
              )}
            </select>
          )}

          {/* Toggle for free text if catalog supports it */}
          {catalog?.supportsFreeText && (
            <div style={{ marginTop: '8px', fontSize: '12px' }}>
              <label style={{ display: 'inline-flex', alignItems: 'center', gap: '6px', cursor: 'pointer' }}>
                <input
                  type="checkbox"
                  checked={isCustomMode}
                  onChange={e => {
                    const next = e.target.checked
                    setIsCustomMode(next)
                    if (next && !customModelText) {
                      setCustomModelText(selectedModel || currentActiveModel || '')
                    }
                  }}
                />
                <span>{t('tuiAgents.modelPicker.useCustomModel')}</span>
              </label>
            </div>
          )}

          {/* Warning if selected model is already active */}
          {isTargetAlreadyActive && (
            <div style={{ marginTop: '6px', fontSize: '11px', color: '#d97706' }}>
              {t('tuiAgents.modelPicker.alreadyActiveHint')}
            </div>
          )}
        </div>

        {/* Actions */}
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '8px' }}>
          <button
            type="button"
            onClick={onClose}
            disabled={isSwitching}
            style={{
              padding: '6px 14px',
              borderRadius: '4px',
              border: '1px solid var(--border-default, #cbd5e1)',
              background: 'transparent',
              cursor: isSwitching ? 'not-allowed' : 'pointer',
              fontSize: '13px',
            }}
          >
            {t('tuiAgents.page.btnCancel')}
          </button>
          <button
            type="button"
            onClick={handleSwitch}
            disabled={!canSwitch}
            style={{
              padding: '6px 16px',
              borderRadius: '4px',
              border: 'none',
              background: canSwitch ? '#0284c7' : '#94a3b8',
              color: '#ffffff',
              cursor: canSwitch ? 'pointer' : 'not-allowed',
              fontSize: '13px',
              fontWeight: 500,
            }}
          >
            {isSwitching
              ? t('tuiAgents.modelPicker.switching')
              : t('tuiAgents.modelPicker.btnSwitch')}
          </button>
        </div>
      </div>
    </div>
  )
}
