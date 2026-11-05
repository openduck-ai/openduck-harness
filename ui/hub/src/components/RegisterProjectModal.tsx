import { useEffect, useMemo, useRef, useState } from 'react'
import type { FormEvent } from 'react'
import type { Project, ProjectDirectory, ProjectInput, ProjectKind } from '@aaif/goose-hub-core'
import { useTranslation } from 'react-i18next'
import { Folder, FolderPlus, Plus, X } from 'lucide-react'
import {
  directoryPathsMatch,
  isProjectDirectoryName,
  isValidProjectSlug,
  slugFromDirectoryName,
  titleFromDirectoryName,
  uniqueSlug,
} from '../projectIdentity'
import { ProjectRootsEditor } from './ProjectRootsEditor'
import { useProjectRoots, type ProjectRootsApi } from './useProjectRoots'

type ErrorText = { kind: 'raw'; message: string } | { kind: 'fallback'; key: string }

const emptyForm: ProjectInput = {
  slug: '',
  title: '',
  description: '',
  path: '',
  kind: 'software',
  status: 'active',
  tags: [],
  notes: '',
  emailRecipients: [],
}

interface RegisterProjectModalProps {
  api: ProjectRootsApi
  projects: Project[]
  onClose: () => void
  onCreateProject: (input: ProjectInput) => Promise<void>
}

export function RegisterProjectModal({
  api,
  projects,
  onClose,
  onCreateProject,
}: RegisterProjectModalProps) {
  const { t } = useTranslation()
  const { roots, loading, error, addRoot, removeRoot, clearError } = useProjectRoots(api)
  const [showRoots, setShowRoots] = useState(false)
  const [selectedRoot, setSelectedRoot] = useState('')
  const [directories, setDirectories] = useState<ProjectDirectory[]>([])
  const [directoriesLoading, setDirectoriesLoading] = useState(false)
  const [truncated, setTruncated] = useState(false)
  const [filter, setFilter] = useState('')
  const [createName, setCreateName] = useState('')
  const [creatingDir, setCreatingDir] = useState(false)
  const [dirError, setDirError] = useState<ErrorText | null>(null)
  const [selectedDir, setSelectedDir] = useState<ProjectDirectory | null>(null)
  const [form, setForm] = useState<ProjectInput>(emptyForm)
  const [slugEdited, setSlugEdited] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState<ErrorText | null>(null)
  const directoryRequest = useRef(0)

  const projectSlugs = useMemo(() => projects.map(project => project.slug), [projects])
  const usableRoots = roots.filter(root => root.available)
  const rootsEditorOpen = showRoots || roots.length === 0

  useEffect(() => {
    setSelectedRoot(current => {
      if (current && roots.some(root => root.path === current && root.available)) return current
      return roots.find(root => root.available)?.path ?? ''
    })
  }, [roots])

  useEffect(() => {
    let ignore = false
    const requestId = ++directoryRequest.current
    setSelectedDir(null)
    setSlugEdited(false)
    setFilter('')
    setDirError(null)
    setForm(current => ({ ...current, path: '', title: '', slug: '' }))
    if (!selectedRoot) {
      setDirectories([])
      setTruncated(false)
      setDirectoriesLoading(false)
      return
    }
    setDirectoriesLoading(true)
    void api
      .listProjectRootDirectories(selectedRoot)
      .then(response => {
        if (ignore || directoryRequest.current !== requestId) return
        setDirectories(response.directories)
        setTruncated(response.truncated)
      })
      .catch(cause => {
        if (ignore || directoryRequest.current !== requestId) return
        setDirectories([])
        setTruncated(false)
        setDirError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'projectRegistration.modal.errorListFolders' })
      })
      .finally(() => {
        if (!ignore && directoryRequest.current === requestId) setDirectoriesLoading(false)
      })
    return () => {
      ignore = true
    }
  }, [api, selectedRoot])

  const filteredDirectories = useMemo(() => {
    const query = filter.trim().toLowerCase()
    if (!query) return directories
    return directories.filter(directory => directory.name.toLowerCase().includes(query))
  }, [directories, filter])

  const registeredProject = selectedDir
    ? projects.find(project => directoryPathsMatch(project.path, selectedDir.path))
    : undefined

  const applyDirectory = (directory: ProjectDirectory) => {
    setSelectedDir(directory)
    setSlugEdited(false)
    setDirError(null)
    setForm(current => ({
      ...current,
      path: directory.path,
      title: titleFromDirectoryName(directory.name),
      slug: uniqueSlug(slugFromDirectoryName(directory.name), projectSlugs),
    }))
  }

  const createDirectory = async () => {
    const name = createName.trim()
    if (!selectedRoot) return
    if (!isProjectDirectoryName(name)) {
      setDirError({ kind: 'fallback', key: 'projectRegistration.modal.errorSingleFolder' })
      return
    }
    const requestId = ++directoryRequest.current
    setCreatingDir(true)
    setDirError(null)
    try {
      const created = await api.createProjectRootDirectory({ root: selectedRoot, name })
      const listing = await api.listProjectRootDirectories(selectedRoot)
      if (directoryRequest.current !== requestId) return
      setDirectories(listing.directories)
      setTruncated(listing.truncated)
      setDirectoriesLoading(false)
      const directory =
        listing.directories.find(entry => entry.path === created.directory.path) ??
        created.directory
      applyDirectory(directory)
      setCreateName('')
      setFilter('')
    } catch (cause) {
      if (directoryRequest.current !== requestId) return
      setDirectoriesLoading(false)
      setDirError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'projectRegistration.modal.errorCreateFolder' })
    } finally {
      setCreatingDir(false)
    }
  }

  const title = (form.title ?? '').trim()
  const slug = form.slug.trim()
  const canSubmit =
    !submitting &&
    !!selectedDir &&
    !registeredProject &&
    title.length > 0 &&
    isValidProjectSlug(slug)

  const handleCreate = async (event: FormEvent) => {
    event.preventDefault()
    if (!selectedDir || registeredProject) return
    if (!title || !isValidProjectSlug(slug)) {
      setFormError({ kind: 'fallback', key: 'projectRegistration.modal.errorFormIncomplete' })
      return
    }
    setSubmitting(true)
    setFormError(null)
    try {
      await onCreateProject({
        ...form,
        title,
        slug,
        path: selectedDir.path,
      })
      onClose()
    } catch (cause) {
      setFormError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'projectRegistration.modal.errorCreateProject' })
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal-content glass-modal register-project-modal"
        onClick={event => event.stopPropagation()}
      >
        <form className="panel form-panel" onSubmit={event => void handleCreate(event)}>
          <div className="modal-header-row">
            <div className="modal-header-title">
              <div className="modal-icon-badge">
                <Plus size={20} />
              </div>
              <div>
                <h2>{t('projectRegistration.modal.title')}</h2>
                <p className="muted text-sm">
                  {t('projectRegistration.modal.subtitle')}
                </p>
              </div>
            </div>
            <button type="button" className="close-btn" onClick={onClose} title={t('projectRegistration.modal.close')}>
              <X size={18} />
            </button>
          </div>

          {formError && <p className="error">{formError.kind === 'raw' ? formError.message : t(formError.key)}</p>}

          <section className="register-section">
            <div className="register-section-head">
              <span className="form-label">{t('projectRegistration.modal.rootLabel')}</span>
              {roots.length > 0 && (
                <button
                  type="button"
                  className="ghost-btn"
                  onClick={() => setShowRoots(current => !current)}
                >
                  {showRoots ? t('projectRegistration.modal.hideRoots') : t('projectRegistration.modal.configureRoots')}
                </button>
              )}
            </div>
            {usableRoots.length > 0 && (
              <select
                className="form-select"
                value={selectedRoot}
                onChange={event => setSelectedRoot(event.target.value)}
              >
                {roots.map(root => (
                  <option key={root.path} value={root.path} disabled={!root.available}>
                    {root.path}
                    {root.available ? '' : ` ${t('projectRegistration.modal.rootMissing')}`}
                  </option>
                ))}
              </select>
            )}
            {!loading && roots.length > 0 && usableRoots.length === 0 && (
              <p className="form-hint">
                {t('projectRegistration.modal.rootsInaccessible')}
              </p>
            )}
            {rootsEditorOpen && (
              <ProjectRootsEditor
                roots={roots}
                loading={loading}
                error={error}
                framed
                clearError={clearError}
                onAdd={addRoot}
                onRemove={removeRoot}
                onAdded={root => {
                  if (root.available) setSelectedRoot(root.path)
                  if (roots.length === 0) setShowRoots(false)
                }}
              />
            )}
          </section>

          <section className="register-section">
            <span className="form-label">{t('projectRegistration.modal.folderLabel')}</span>
            <p className="form-hint">
              {t('projectRegistration.modal.folderHint')}
            </p>
            {selectedRoot ? (
              <>
                <input
                  value={filter}
                  onChange={event => setFilter(event.target.value)}
                  onKeyDown={event => {
                    if (event.key === 'Enter') event.preventDefault()
                  }}
                  placeholder={t('projectRegistration.modal.filterFoldersPlaceholder')}
                  className="form-input"
                />
                <div className="register-dir-list">
                  {directoriesLoading && <p className="form-hint">{t('projectRegistration.modal.loadingFolders')}</p>}
                  {!directoriesLoading && filteredDirectories.length === 0 && (
                    <p className="form-hint">
                      {filter.trim() ? t('projectRegistration.modal.noFoldersMatch') : t('projectRegistration.modal.noFoldersYet')}
                    </p>
                  )}
                  {filteredDirectories.map(directory => {
                    const registered = projects.some(project =>
                      directoryPathsMatch(project.path, directory.path),
                    )
                    const selected = selectedDir?.path === directory.path
                    return (
                      <button
                        key={directory.path}
                        type="button"
                        className={`register-dir-option${selected ? ' selected' : ''}`}
                        aria-pressed={selected}
                        onClick={() => applyDirectory(directory)}
                      >
                        <Folder size={15} />
                        <span className="register-dir-name">{directory.name}</span>
                        {registered && <span className="register-dir-badge">{t('projectRegistration.modal.registeredBadge')}</span>}
                      </button>
                    )
                  })}
                </div>
                {truncated && <p className="form-hint">{t('projectRegistration.modal.showingFirst500')}</p>}
                <div className="register-create-row">
                  <input
                    value={createName}
                    onChange={event => setCreateName(event.target.value)}
                    onKeyDown={event => {
                      if (event.key === 'Enter') {
                        event.preventDefault()
                        void createDirectory()
                      }
                    }}
                    placeholder={t('projectRegistration.modal.newFolderPlaceholder')}
                    className="form-input"
                    spellCheck={false}
                    autoComplete="off"
                  />
                  <button
                    type="button"
                    className="secondary"
                    disabled={creatingDir || !createName.trim()}
                    onClick={() => void createDirectory()}
                  >
                    <FolderPlus size={15} />
                    <span>{creatingDir ? t('projectRegistration.modal.creating') : t('projectRegistration.modal.create')}</span>
                  </button>
                </div>
                {dirError && <p className="error">{dirError.kind === 'raw' ? dirError.message : t(dirError.key)}</p>}
                {selectedDir && <code className="register-selected-path">{selectedDir.path}</code>}
                {registeredProject && (
                  <p className="form-hint">
                    {t('projectRegistration.modal.alreadyRegistered', { name: registeredProject.title || registeredProject.slug })}
                  </p>
                )}
              </>
            ) : (
              !rootsEditorOpen && (
                <p className="form-hint">
                  {roots.length > 0
                    ? t('projectRegistration.modal.loadingFolders')
                    : t('projectRegistration.modal.addRootFirst')}
                </p>
              )
            )}
          </section>

          <div className="register-identity-grid">
            <label className="form-group">
              <span className="form-label">{t('projectRegistration.modal.titleLabel')}</span>
              <input
                value={form.title}
                onChange={event => {
                  const title = event.target.value
                  setForm(current => ({
                    ...current,
                    title,
                    slug: slugEdited
                      ? current.slug
                      : uniqueSlug(slugFromDirectoryName(title), projectSlugs),
                  }))
                }}
                onKeyDown={event => {
                  if (event.key === 'Enter') event.preventDefault()
                }}
                placeholder={t('projectRegistration.modal.titlePlaceholder')}
                className="form-input"
              />
            </label>
            <label className="form-group">
              <span className="form-label">{t('projectRegistration.modal.slugLabel')}</span>
              <input
                required
                pattern="[a-z0-9]+(-[a-z0-9]+)*"
                value={form.slug}
                onChange={event => {
                  setSlugEdited(true)
                  setForm(current => ({ ...current, slug: event.target.value }))
                }}
                onKeyDown={event => {
                  if (event.key === 'Enter') event.preventDefault()
                }}
                placeholder={t('projectRegistration.modal.slugPlaceholder')}
                className="form-input code-font"
                spellCheck={false}
                autoComplete="off"
              />
            </label>
          </div>
          <p className="form-hint">{t('projectRegistration.modal.identityHint')}</p>
          {selectedDir && !isValidProjectSlug(form.slug.trim()) && (
            <p className="form-hint">
              {t('projectRegistration.modal.invalidSlugHint')}
            </p>
          )}

          <div className="register-identity-grid">
            <label className="form-group">
              <span className="form-label">{t('projectRegistration.modal.kindLabel')}</span>
              <select
                value={form.kind}
                onChange={event =>
                  setForm(current => ({ ...current, kind: event.target.value as ProjectKind }))
                }
                className="form-select"
              >
                <option value="software">{t('projectRegistration.modal.kindSoftware')}</option>
                <option value="docs">{t('projectRegistration.modal.kindDocs')}</option>
                <option value="automation">{t('projectRegistration.modal.kindAutomation')}</option>
                <option value="other">{t('projectRegistration.modal.kindOther')}</option>
              </select>
            </label>
            <label className="form-group">
              <span className="form-label">{t('projectRegistration.modal.languageLabel')}</span>
              <input
                value={form.language ?? ''}
                onChange={event => setForm(current => ({ ...current, language: event.target.value }))}
                onKeyDown={event => {
                  if (event.key === 'Enter') event.preventDefault()
                }}
                placeholder={t('projectRegistration.modal.languagePlaceholder')}
                className="form-input"
              />
            </label>
          </div>

          <label className="form-group">
            <span className="form-label">{t('projectRegistration.modal.descriptionLabel')}</span>
            <textarea
              rows={3}
              value={form.description}
              onChange={event => setForm(current => ({ ...current, description: event.target.value }))}
              placeholder={t('projectRegistration.modal.descriptionPlaceholder')}
              className="form-textarea"
            />
          </label>

          <label className="form-group">
            <span className="form-label">{t('projectRegistration.modal.emailLabel')}</span>
            <input
              value={(form.emailRecipients || []).join(', ')}
              onChange={event =>
                setForm(current => ({
                  ...current,
                  emailRecipients: event.target.value
                    .split(',')
                    .map(recipient => recipient.trim())
                    .filter(Boolean),
                }))
              }
              onKeyDown={event => {
                if (event.key === 'Enter') event.preventDefault()
              }}
              placeholder={t('projectRegistration.modal.emailPlaceholder')}
              className="form-input"
            />
            <small className="form-hint">{t('projectRegistration.modal.emailHint')}</small>
          </label>

          <div className="modal-actions-row">
            <button type="button" className="secondary" onClick={onClose}>
              {t('projectRegistration.modal.cancel')}
            </button>
            <button type="submit" className="btn-primary" disabled={!canSubmit}>
              {submitting ? t('projectRegistration.modal.registering') : t('projectRegistration.modal.registerProject')}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
