import { Trans, useTranslation } from 'react-i18next'
import { useState, useEffect } from 'react'
import { Mail, Check, AlertCircle, Bot, Plus, Trash2 } from 'lucide-react'
import type {
  ProjectDetail,
  createApi,
  ProjectProviderEntry,
  ProjectProvidersConfigResponse,
  ProjectProvidersConfigPutBody,
  ProvidersConfigResponse,
} from '@aaif/goose-hub-core'
import { projectModelChoices, projectProviderChoices } from '../projectProviderOptions.ts'

interface ProjectSettingsProps {
  detail: ProjectDetail
  api: ReturnType<typeof createApi>
  onArchive: () => Promise<void>
  onDelete: () => Promise<void>
  onReload: () => void
}

export function ProjectSettings({
  detail,
  api,
  onArchive,
  onDelete,
  onReload,
}: ProjectSettingsProps) {
  const { t } = useTranslation()
  const [archiving, setArchiving] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [error, setError] = useState<{ message: string } | { key: string } | null>(null)
  const [emailInput, setEmailInput] = useState('')
  const [savingEmail, setSavingEmail] = useState(false)
  const [emailSavedSuccess, setEmailSavedSuccess] = useState(false)

  // Project Providers Configuration state
  const [providersLoading, setProvidersLoading] = useState(false)
  const [savingProviders, setSavingProviders] = useState(false)
  const [providersSavedSuccess, setProvidersSavedSuccess] = useState(false)
  const [providersError, setProvidersError] = useState<string | null>(null)

  const [projectProvidersConfig, setProjectProvidersConfig] =
    useState<ProjectProvidersConfigResponse | null>(null)
  const [globalProvidersConfig, setGlobalProvidersConfig] =
    useState<ProvidersConfigResponse | null>(null)

  const [majorProvider, setMajorProvider] = useState<string>('')
  const [majorModel, setMajorModel] = useState<string>('')
  const [secondaryProvider, setSecondaryProvider] = useState<string>('')
  const [secondaryModel, setSecondaryModel] = useState<string>('')
  const [providerOverrides, setProviderOverrides] = useState<
    Record<string, ProjectProviderEntry>
  >({})
  const [newOverrideProvider, setNewOverrideProvider] = useState<string>('')

  useEffect(() => {
    setEmailInput((detail.project.emailRecipients || []).join(', '))
  }, [detail.project.emailRecipients])

  useEffect(() => {
    let cancelled = false
    async function loadProviders() {
      setProvidersLoading(true)
      try {
        const [projRes, globRes] = await Promise.all([
          api.getProjectProvidersConfig?.(detail.project.slug).catch(() => null),
          api.getProvidersConfig?.().catch(() => null),
        ])
        if (cancelled) return
        if (projRes) {
          setProjectProvidersConfig(projRes)
          setMajorProvider(projRes.majorProvider || '')
          setMajorModel(projRes.majorModel || '')
          setSecondaryProvider(projRes.secondaryProvider || '')
          setSecondaryModel(projRes.secondaryModel || '')
          setProviderOverrides(projRes.providers || {})
        }
        if (globRes) {
          setGlobalProvidersConfig(globRes)
        }
      } catch (err) {
        console.error('Failed to load project providers config:', err)
      } finally {
        if (!cancelled) setProvidersLoading(false)
      }
    }
    void loadProviders()
    return () => {
      cancelled = true
    }
  }, [api, detail.project.slug])

  const handleSaveProviders = async () => {
    setSavingProviders(true)
    setProvidersError(null)
    setProvidersSavedSuccess(false)
    try {
      const payload: ProjectProvidersConfigPutBody = {
        majorProvider: majorProvider.trim() || undefined,
        majorModel: majorModel.trim() || undefined,
        secondaryProvider: secondaryProvider.trim() || undefined,
        secondaryModel: secondaryModel.trim() || undefined,
        providers: providerOverrides,
      }
      const updated = await api.updateProjectProvidersConfig(
        detail.project.slug,
        payload,
      )
      setProjectProvidersConfig(updated)
      setMajorProvider(updated.majorProvider || '')
      setMajorModel(updated.majorModel || '')
      setSecondaryProvider(updated.secondaryProvider || '')
      setSecondaryModel(updated.secondaryModel || '')
      setProviderOverrides(updated.providers || {})
      setProvidersSavedSuccess(true)
      onReload()
      setTimeout(() => setProvidersSavedSuccess(false), 3500)
    } catch (cause) {
      setProvidersError(
        cause instanceof Error
          ? cause.message
          : t('projectSettings.page.providersFailed'),
      )
    } finally {
      setSavingProviders(false)
    }
  }

  const knownProviderNames = (globalProvidersConfig?.providers || []).map(
    p => p.name,
  )
  const allProviderNames = projectProviderChoices({
    availableProviders: projectProvidersConfig?.availableProviders,
    globalProviderNames: globalProvidersConfig ? knownProviderNames : undefined,
    current: [
      majorProvider,
      secondaryProvider,
      projectProvidersConfig?.majorProvider,
      projectProvidersConfig?.secondaryProvider,
    ],
    overrides: Object.keys(providerOverrides),
  })

  const effectiveMajorProvider =
    majorProvider || projectProvidersConfig?.globalMajorProvider || ''
  const effectiveSecondaryProvider =
    secondaryProvider || projectProvidersConfig?.globalSecondaryProvider || ''

  const modelsFor = (providerName: string) =>
    (globalProvidersConfig?.providers.find(p => p.name === providerName)?.models || []).map(
      model => model.id,
    )

  const majorModelOptions = projectModelChoices({
    provider: effectiveMajorProvider,
    providerModels: projectProvidersConfig?.providerModels,
    globalModels: modelsFor(effectiveMajorProvider),
    current: majorModel,
  })

  const secondaryModelOptions = projectModelChoices({
    provider: effectiveSecondaryProvider,
    providerModels: projectProvidersConfig?.providerModels,
    globalModels: modelsFor(effectiveSecondaryProvider),
    current: secondaryModel,
  })

  const handleSaveEmailRecipients = async () => {
    setSavingEmail(true)
    setError(null)
    setEmailSavedSuccess(false)
    try {
      const list = emailInput
        .split(',')
        .map(r => r.trim())
        .filter(Boolean)
      await api.patchProject(detail.project.slug, { emailRecipients: list })
      setEmailSavedSuccess(true)
      onReload()
      setTimeout(() => setEmailSavedSuccess(false), 3500)
    } catch (cause) {
      setError(cause instanceof Error ? { message: cause.message } : { key: 'projectSettings.page.emailFailed' })
    } finally {
      setSavingEmail(false)
    }
  }

  const handleArchiveToggle = async () => {
    setArchiving(true)
    setError(null)
    try {
      await onArchive()
    } catch (cause) {
      setError(cause instanceof Error ? { message: cause.message } : { key: 'projectSettings.page.archiveFailed' })
    } finally {
      setArchiving(false)
    }
  }

  const handleDelete = async () => {
    if (
      !confirm(
        t('projectSettings.page.confirmUnregister', { name: detail.project.title || detail.project.slug }),
      )
    ) {
      return
    }
    setDeleting(true)
    setError(null)
    try {
      await onDelete()
    } catch (cause) {
      setError(cause instanceof Error ? { message: cause.message } : { key: 'projectSettings.page.deleteFailed' })
    } finally {
      setDeleting(false)
    }
  }

  const errorMessage = error ? ('message' in error ? error.message : t(error.key)) : ''

  return (
    <div className="project-settings-container">
      {/* 1. Project Email Notifications Configuration */}
      <div className="panel form-panel">
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.25rem' }}>
          <Mail size={18} className="text-primary" />
          <h3 style={{ margin: 0 }}>{t('projectSettings.page.emailTitle')}</h3>
        </div>
        <p className="muted">
          {t('projectSettings.page.emailHint')}
        </p>

        {emailSavedSuccess && (
          <div
            style={{
              padding: '0.6rem 0.8rem',
              backgroundColor: 'rgba(16, 185, 129, 0.1)',
              border: '1px solid rgba(16, 185, 129, 0.3)',
              borderRadius: '6px',
              color: '#10b981',
              fontSize: '0.875rem',
              marginBottom: '1rem',
              display: 'flex',
              alignItems: 'center',
              gap: '0.4rem',
            }}
          >
            <Check size={16} />
            <span>{t('projectSettings.page.emailSaved', { slug: detail.project.slug })}</span>
          </div>
        )}

        <div className="settings-field-group">
          <div className="settings-item">
            <label>{t('projectSettings.page.emailRecipients')}</label>
            <input
              type="text"
              value={emailInput}
              onChange={e => setEmailInput(e.target.value)}
              placeholder={t('projectSettings.page.emailPlaceholder')}
              className="form-input"
            />
            <small className="muted" style={{ display: 'block', marginTop: '0.25rem' }}>
              <Trans t={t} i18nKey="projectSettings.page.emailExample" components={{ code: <code /> }} />
            </small>
          </div>
        </div>

        <div style={{ marginTop: '1rem', display: 'flex', justifyContent: 'flex-end' }}>
          <button
            type="button"
            className="btn-primary"
            disabled={savingEmail}
            onClick={() => void handleSaveEmailRecipients()}
          >
            {savingEmail ? t('projectSettings.page.saving') : t('projectSettings.page.saveEmail')}
          </button>
        </div>
      </div>

      {/* 2. Project Providers & Models Configuration */}
      <div className="panel form-panel" style={{ marginTop: '1.5rem' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.25rem' }}>
          <Bot size={18} className="text-primary" />
          <h3 style={{ margin: 0 }}>{t('projectSettings.page.providersTitle')}</h3>
        </div>
        <p className="muted">{t('projectSettings.page.providersHint')}</p>

        {providersSavedSuccess && (
          <div
            style={{
              padding: '0.6rem 0.8rem',
              backgroundColor: 'rgba(16, 185, 129, 0.1)',
              border: '1px solid rgba(16, 185, 129, 0.3)',
              borderRadius: '6px',
              color: '#10b981',
              fontSize: '0.875rem',
              marginBottom: '1rem',
              display: 'flex',
              alignItems: 'center',
              gap: '0.4rem',
            }}
          >
            <Check size={16} />
            <span>{t('projectSettings.page.providersSaved', { slug: detail.project.slug })}</span>
          </div>
        )}

        {providersError && (
          <div style={{ color: '#ef4444', display: 'flex', alignItems: 'center', gap: '0.3rem', marginBottom: '1rem' }}>
            <AlertCircle size={16} />
            <span>{providersError}</span>
          </div>
        )}

        <div className="settings-field-group">
          <div className="row" style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))', gap: '1rem' }}>
            <div className="settings-item">
              <label>{t('projectSettings.page.majorProvider')}</label>
              <select
                className="form-input"
                value={majorProvider}
                onChange={e => setMajorProvider(e.target.value)}
              >
                <option value="">
                  {projectProvidersConfig?.globalMajorProvider
                    ? t('projectSettings.page.inheritGlobal', { name: projectProvidersConfig.globalMajorProvider })
                    : t('projectSettings.page.inheritGlobalDefault')}
                </option>
                {allProviderNames.map(name => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </div>

            <div className="settings-item">
              <label>{t('projectSettings.page.majorModel')}</label>
              <input
                type="text"
                list="major-model-options"
                className="form-input"
                value={majorModel}
                onChange={e => setMajorModel(e.target.value)}
                placeholder={
                  effectiveMajorProvider
                    ? t('projectSettings.page.inheritProviderDefault')
                    : (projectProvidersConfig?.globalMajorModel || 'Default')
                }
              />
              <datalist id="major-model-options">
                {majorModelOptions.map(m => (
                  <option key={m} value={m} />
                ))}
              </datalist>
            </div>
          </div>

          <div className="row" style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))', gap: '1rem', marginTop: '1rem' }}>
            <div className="settings-item">
              <label>{t('projectSettings.page.secondaryProvider')}</label>
              <select
                className="form-input"
                value={secondaryProvider}
                onChange={e => setSecondaryProvider(e.target.value)}
              >
                <option value="">
                  {projectProvidersConfig?.globalSecondaryProvider
                    ? t('projectSettings.page.inheritGlobal', { name: projectProvidersConfig.globalSecondaryProvider })
                    : t('projectSettings.page.none')}
                </option>
                {allProviderNames.map(name => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </div>

            <div className="settings-item">
              <label>{t('projectSettings.page.secondaryModel')}</label>
              <input
                type="text"
                list="secondary-model-options"
                className="form-input"
                value={secondaryModel}
                onChange={e => setSecondaryModel(e.target.value)}
                placeholder={
                  secondaryProvider
                    ? t('projectSettings.page.inheritProviderDefault')
                    : (projectProvidersConfig?.globalSecondaryModel || t('projectSettings.page.none'))
                }
              />
              <datalist id="secondary-model-options">
                {secondaryModelOptions.map(m => (
                  <option key={m} value={m} />
                ))}
              </datalist>
            </div>
          </div>

          {/* Per-Provider Overrides */}
          <div style={{ marginTop: '1.5rem', borderTop: '1px solid var(--border-default, #333)', paddingTop: '1rem' }}>
            <h4 style={{ margin: '0 0 0.25rem 0', fontSize: '0.95rem' }}>{t('projectSettings.page.providerListTitle')}</h4>
            <p className="muted" style={{ margin: '0 0 1rem 0', fontSize: '0.85rem' }}>
              {t('projectSettings.page.providerListHint')}
            </p>

            {Object.keys(providerOverrides).length === 0 ? (
              <p className="muted" style={{ fontStyle: 'italic', fontSize: '0.875rem' }}>
                {t('projectSettings.page.noOverrides')}
              </p>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '0.75rem' }}>
                {Object.entries(providerOverrides).map(([pName, entry]) => (
                  <div
                    key={pName}
                    style={{
                      padding: '0.75rem',
                      borderRadius: '6px',
                      background: 'var(--bg-secondary, rgba(255,255,255,0.03))',
                      border: '1px solid var(--border-default, rgba(255,255,255,0.1))',
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '0.5rem' }}>
                      <strong style={{ fontSize: '0.9rem' }}>{pName}</strong>
                      <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}>
                        <label style={{ display: 'flex', alignItems: 'center', gap: '0.3rem', fontSize: '0.82rem', cursor: 'pointer' }}>
                          <input
                            type="checkbox"
                            checked={entry.enabled ?? true}
                            onChange={e => {
                              setProviderOverrides({
                                ...providerOverrides,
                                [pName]: { ...entry, enabled: e.target.checked },
                              })
                            }}
                          />
                          {t('projectSettings.page.enabled')}
                        </label>
                        <button
                          type="button"
                          className="btn-icon"
                          style={{ background: 'transparent', border: 'none', color: '#ef4444', cursor: 'pointer', padding: '2px' }}
                          title="Remove override"
                          onClick={() => {
                            const next = { ...providerOverrides }
                            delete next[pName]
                            setProviderOverrides(next)
                          }}
                        >
                          <Trash2 size={15} />
                        </button>
                      </div>
                    </div>

                    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: '0.5rem' }}>
                      <div>
                        <small className="muted" style={{ display: 'block', marginBottom: '0.2rem' }}>
                          {t('projectSettings.page.majorModel')}
                        </small>
                        <input
                          type="text"
                          list={`project-override-models-${encodeURIComponent(pName)}`}
                          className="form-input"
                          style={{ width: '100%', fontSize: '0.85rem' }}
                          value={entry.model || ''}
                          placeholder={t('projectSettings.page.inheritProviderDefault')}
                          onChange={e => {
                            setProviderOverrides({
                              ...providerOverrides,
                              [pName]: { ...entry, model: e.target.value || undefined },
                            })
                          }}
                        />
                        <datalist id={`project-override-models-${encodeURIComponent(pName)}`}>
                          {projectModelChoices({
                            provider: pName,
                            providerModels: projectProvidersConfig?.providerModels,
                            globalModels: modelsFor(pName),
                            current: entry.model,
                          }).map(model => (
                            <option key={model} value={model} />
                          ))}
                        </datalist>
                      </div>
                      <div>
                        <small className="muted" style={{ display: 'block', marginBottom: '0.2rem' }}>
                          {t('projectSettings.page.fastModel')}
                        </small>
                        <input
                          type="text"
                          className="form-input"
                          style={{ width: '100%', fontSize: '0.85rem' }}
                          value={entry.fastModel || ''}
                          placeholder="Default fast model"
                          onChange={e => {
                            setProviderOverrides({
                              ...providerOverrides,
                              [pName]: { ...entry, fastModel: e.target.value || undefined },
                            })
                          }}
                        />
                      </div>
                      <div>
                        <small className="muted" style={{ display: 'block', marginBottom: '0.2rem' }}>
                          {t('projectSettings.page.secondaryModel')}
                        </small>
                        <input
                          type="text"
                          className="form-input"
                          style={{ width: '100%', fontSize: '0.85rem' }}
                          value={entry.secondaryModel || ''}
                          placeholder="Fallback model"
                          onChange={e => {
                            setProviderOverrides({
                              ...providerOverrides,
                              [pName]: { ...entry, secondaryModel: e.target.value || undefined },
                            })
                          }}
                        />
                      </div>
                      <div>
                        <small className="muted" style={{ display: 'block', marginBottom: '0.2rem' }}>
                          {t('projectSettings.page.proxy')}
                        </small>
                        <input
                          type="text"
                          className="form-input"
                          style={{ width: '100%', fontSize: '0.85rem' }}
                          value={entry.proxy || ''}
                          placeholder="http://127.0.0.1:7890"
                          onChange={e => {
                            setProviderOverrides({
                              ...providerOverrides,
                              [pName]: { ...entry, proxy: e.target.value || undefined },
                            })
                          }}
                        />
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}

            {/* Add Provider Selector */}
            <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginTop: '0.75rem' }}>
              <select
                className="form-input"
                style={{ maxWidth: '220px' }}
                value={newOverrideProvider}
                onChange={e => setNewOverrideProvider(e.target.value)}
              >
                <option value="">{t('projectSettings.page.selectProvider')}</option>
                {allProviderNames
                  .filter(name => !providerOverrides[name])
                  .map(name => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
              </select>
              <button
                type="button"
                className="btn-secondary"
                disabled={!newOverrideProvider}
                onClick={() => {
                  if (newOverrideProvider) {
                    setProviderOverrides({
                      ...providerOverrides,
                      [newOverrideProvider]: { enabled: true },
                    })
                    setNewOverrideProvider('')
                  }
                }}
                style={{ display: 'flex', alignItems: 'center', gap: '0.3rem', padding: '0.5rem 0.8rem' }}
              >
                <Plus size={15} />
                <span>{t('projectSettings.page.addProvider')}</span>
              </button>
            </div>
          </div>
        </div>

        <div style={{ marginTop: '1.25rem', display: 'flex', justifyContent: 'flex-end' }}>
          <button
            type="button"
            className="btn-primary"
            disabled={savingProviders || providersLoading}
            onClick={() => void handleSaveProviders()}
          >
            {savingProviders ? t('projectSettings.page.saving') : t('projectSettings.page.saveProviders')}
          </button>
        </div>
      </div>

      {/* 3. General Project Configuration */}
      <div className="panel form-panel" style={{ marginTop: '1.5rem' }}>
        <h3>{t('projectSettings.page.metadata')}</h3>
        <p className="muted">{t('projectSettings.page.metadataHint')}</p>

        {errorMessage && (
          <div style={{ color: '#ef4444', display: 'flex', alignItems: 'center', gap: '0.3rem', marginBottom: '1rem' }}>
            <AlertCircle size={16} />
            <span>{errorMessage}</span>
          </div>
        )}

        <div className="settings-field-group">
          <div className="settings-item">
            <label>{t('projectSettings.page.slug')}</label>
            <input value={detail.project.slug} readOnly className="readonly-input" />
          </div>

          <div className="settings-item">
            <label>{t('projectSettings.page.title')}</label>
            <input value={detail.project.title || detail.project.slug} readOnly className="readonly-input" />
          </div>

          <div className="settings-item">
            <label>{t('projectSettings.page.rootPath')}</label>
            <input value={detail.project.path} readOnly className="readonly-input" />
          </div>

          <div className="row">
            <div className="settings-item">
              <label>{t('projectSettings.page.kind')}</label>
              <input value={detail.project.kind} readOnly className="readonly-input" />
            </div>
            <div className="settings-item">
              <label>{t('projectSettings.page.language')}</label>
              <input value={detail.project.language || t('projectSettings.page.notSpecified')} readOnly className="readonly-input" />
            </div>
            <div className="settings-item">
              <label>{t('projectSettings.page.currentStatus')}</label>
              <span className={`status ${detail.project.status}`}>{detail.project.status}</span>
            </div>
          </div>

          {detail.project.description && (
            <div className="settings-item">
              <label>{t('projectSettings.page.description')}</label>
              <textarea value={detail.project.description} readOnly className="readonly-input" />
            </div>
          )}
        </div>
      </div>

      {/* 3. Danger Zone */}
      <div className="panel danger-zone-panel" style={{ marginTop: '1.5rem' }}>
        <h3 className="danger-title">{t('projectSettings.page.danger')}</h3>
        <p className="muted">{t('projectSettings.page.dangerHint')}</p>

        <div className="danger-actions-list">
          <div className="danger-action-row">
            <div>
              <strong>
                {detail.project.status === 'archived' ? t('projectSettings.page.unarchive') : t('projectSettings.page.archive')}
              </strong>
              <p className="muted">
                {detail.project.status === 'archived'
                  ? t('projectSettings.page.restoreHint')
                  : t('projectSettings.page.archiveHint')}
              </p>
            </div>
            <button
              type="button"
              className="secondary"
              disabled={archiving}
              onClick={() => void handleArchiveToggle()}
            >
              {archiving
                ? t('projectSettings.page.updating')
                : detail.project.status === 'archived'
                ? t('projectSettings.page.restore')
                : t('projectSettings.page.archive')}
            </button>
          </div>

          <div className="danger-action-row">
            <div>
              <strong>{t('projectSettings.page.unregisterTitle')}</strong>
              <p className="muted">{t('projectSettings.page.unregisterHint')}</p>
            </div>
            <button
              type="button"
              className="danger"
              disabled={deleting}
              onClick={() => void handleDelete()}
            >
              {deleting ? t('projectSettings.page.deleting') : t('projectSettings.page.unregister')}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
