import { Trans, useTranslation } from 'react-i18next'
import { useState, useEffect } from 'react'
import { Mail, Check, AlertCircle } from 'lucide-react'
import type { ProjectDetail, createApi } from '@aaif/goose-hub-core'

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

  useEffect(() => {
    setEmailInput((detail.project.emailRecipients || []).join(', '))
  }, [detail.project.emailRecipients])

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

      {/* 2. General Project Configuration */}
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
