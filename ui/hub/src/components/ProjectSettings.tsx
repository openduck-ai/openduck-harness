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
  const [archiving, setArchiving] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [error, setError] = useState('')
  const [emailInput, setEmailInput] = useState('')
  const [savingEmail, setSavingEmail] = useState(false)
  const [emailSavedSuccess, setEmailSavedSuccess] = useState(false)

  useEffect(() => {
    setEmailInput((detail.project.emailRecipients || []).join(', '))
  }, [detail.project.emailRecipients])

  const handleSaveEmailRecipients = async () => {
    setSavingEmail(true)
    setError('')
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
      setError(cause instanceof Error ? cause.message : 'Failed to update email recipients')
    } finally {
      setSavingEmail(false)
    }
  }

  const handleArchiveToggle = async () => {
    setArchiving(true)
    setError('')
    try {
      await onArchive()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Failed to change archive status')
    } finally {
      setArchiving(false)
    }
  }

  const handleDelete = async () => {
    if (
      !confirm(
        `Are you sure you want to completely unregister project "${detail.project.title || detail.project.slug}"? This does NOT delete files on disk.`,
      )
    ) {
      return
    }
    setDeleting(true)
    setError('')
    try {
      await onDelete()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Failed to delete project')
    } finally {
      setDeleting(false)
    }
  }

  return (
    <div className="project-settings-container">
      {/* 1. Project Email Notifications Configuration */}
      <div className="panel form-panel">
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.25rem' }}>
          <Mail size={18} className="text-primary" />
          <h3 style={{ margin: 0 }}>Project Email Notifications</h3>
        </div>
        <p className="muted">
          Configure email recipients for this project. When tasks or scheduler jobs finish, execution reports will be automatically dispatched to these addresses.
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
            <span>Email recipients for project "{detail.project.slug}" saved successfully!</span>
          </div>
        )}

        <div className="settings-field-group">
          <div className="settings-item">
            <label>Email Recipients (Comma-separated)</label>
            <input
              type="text"
              value={emailInput}
              onChange={e => setEmailInput(e.target.value)}
              placeholder="dev@beiwanai.com, ops@beiwanai.com, zhanghu@beiwanai.com"
              className="form-input"
            />
            <small className="muted" style={{ display: 'block', marginTop: '0.25rem' }}>
              Example: <code>team@beiwanai.com, lead@beiwanai.com</code>
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
            {savingEmail ? 'Saving…' : 'Save Email Recipients'}
          </button>
        </div>
      </div>

      {/* 2. General Project Configuration */}
      <div className="panel form-panel" style={{ marginTop: '1.5rem' }}>
        <h3>Project Metadata</h3>
        <p className="muted">Metadata and configuration for this project registered in OpenDuck.</p>

        {error && (
          <div style={{ color: '#ef4444', display: 'flex', alignItems: 'center', gap: '0.3rem', marginBottom: '1rem' }}>
            <AlertCircle size={16} />
            <span>{error}</span>
          </div>
        )}

        <div className="settings-field-group">
          <div className="settings-item">
            <label>Project Slug (ID)</label>
            <input value={detail.project.slug} readOnly className="readonly-input" />
          </div>

          <div className="settings-item">
            <label>Title</label>
            <input value={detail.project.title || detail.project.slug} readOnly className="readonly-input" />
          </div>

          <div className="settings-item">
            <label>Root Directory Path</label>
            <input value={detail.project.path} readOnly className="readonly-input" />
          </div>

          <div className="row">
            <div className="settings-item">
              <label>Project Kind</label>
              <input value={detail.project.kind} readOnly className="readonly-input" />
            </div>
            <div className="settings-item">
              <label>Language</label>
              <input value={detail.project.language || 'Not specified'} readOnly className="readonly-input" />
            </div>
            <div className="settings-item">
              <label>Current Status</label>
              <span className={`status ${detail.project.status}`}>{detail.project.status}</span>
            </div>
          </div>

          {detail.project.description && (
            <div className="settings-item">
              <label>Description</label>
              <textarea value={detail.project.description} readOnly className="readonly-input" />
            </div>
          )}
        </div>
      </div>

      {/* 3. Danger Zone */}
      <div className="panel danger-zone-panel" style={{ marginTop: '1.5rem' }}>
        <h3 className="danger-title">Danger Zone</h3>
        <p className="muted">Actions that affect the visibility and registration of this project.</p>

        <div className="danger-actions-list">
          <div className="danger-action-row">
            <div>
              <strong>
                {detail.project.status === 'archived' ? 'Unarchive Project' : 'Archive Project'}
              </strong>
              <p className="muted">
                {detail.project.status === 'archived'
                  ? 'Restore this project to active status.'
                  : 'Hide this project from the default active list.'}
              </p>
            </div>
            <button
              type="button"
              className="secondary"
              disabled={archiving}
              onClick={() => void handleArchiveToggle()}
            >
              {archiving
                ? 'Updating…'
                : detail.project.status === 'archived'
                ? 'Restore Project'
                : 'Archive Project'}
            </button>
          </div>

          <div className="danger-action-row">
            <div>
              <strong>Delete / Unregister Project</strong>
              <p className="muted">Remove project registration from OpenDuck. Files on disk will remain untouched.</p>
            </div>
            <button
              type="button"
              className="danger"
              disabled={deleting}
              onClick={() => void handleDelete()}
            >
              {deleting ? 'Deleting…' : 'Unregister Project'}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
