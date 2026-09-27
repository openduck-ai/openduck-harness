import { useEffect, useMemo, useRef, useState } from 'react'
import type { FormEvent } from 'react'
import type { Project, ProjectDirectory, ProjectInput, ProjectKind } from '@aaif/goose-hub-core'
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
  const { roots, loading, error, addRoot, removeRoot, clearError } = useProjectRoots(api)
  const [showRoots, setShowRoots] = useState(false)
  const [selectedRoot, setSelectedRoot] = useState('')
  const [directories, setDirectories] = useState<ProjectDirectory[]>([])
  const [directoriesLoading, setDirectoriesLoading] = useState(false)
  const [truncated, setTruncated] = useState(false)
  const [filter, setFilter] = useState('')
  const [createName, setCreateName] = useState('')
  const [creatingDir, setCreatingDir] = useState(false)
  const [dirError, setDirError] = useState('')
  const [selectedDir, setSelectedDir] = useState<ProjectDirectory | null>(null)
  const [form, setForm] = useState<ProjectInput>(emptyForm)
  const [slugEdited, setSlugEdited] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState('')
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
    setDirError('')
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
        setDirError(cause instanceof Error ? cause.message : 'Unable to list folders')
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
    setDirError('')
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
      setDirError('Enter a single folder name without slashes.')
      return
    }
    const requestId = ++directoryRequest.current
    setCreatingDir(true)
    setDirError('')
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
      setDirError(cause instanceof Error ? cause.message : 'Unable to create folder')
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
      setFormError('Choose a folder, then keep a title and a lowercase slug.')
      return
    }
    setSubmitting(true)
    setFormError('')
    try {
      await onCreateProject({
        ...form,
        title,
        slug,
        path: selectedDir.path,
      })
      onClose()
    } catch (cause) {
      setFormError(cause instanceof Error ? cause.message : 'Unable to create project')
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
                <h2>Register New Project</h2>
                <p className="muted text-sm">
                  Pick a folder inside a configured root. The title and slug come from the folder name.
                </p>
              </div>
            </div>
            <button type="button" className="close-btn" onClick={onClose} title="Close">
              <X size={18} />
            </button>
          </div>

          {formError && <p className="error">{formError}</p>}

          <section className="register-section">
            <div className="register-section-head">
              <span className="form-label">Project root</span>
              {roots.length > 0 && (
                <button
                  type="button"
                  className="ghost-btn"
                  onClick={() => setShowRoots(current => !current)}
                >
                  {showRoots ? 'Hide roots' : 'Configure roots'}
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
                    {root.available ? '' : ' (missing)'}
                  </option>
                ))}
              </select>
            )}
            {!loading && roots.length > 0 && usableRoots.length === 0 && (
              <p className="form-hint">
                Configured roots are not accessible. Remove them or add a folder that exists.
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
            <span className="form-label">Project folder</span>
            <p className="form-hint">
              Select an existing folder in this root, or create a new one. Each project is a direct child of the root.
            </p>
            {selectedRoot ? (
              <>
                <input
                  value={filter}
                  onChange={event => setFilter(event.target.value)}
                  onKeyDown={event => {
                    if (event.key === 'Enter') event.preventDefault()
                  }}
                  placeholder="Filter folders"
                  className="form-input"
                />
                <div className="register-dir-list">
                  {directoriesLoading && <p className="form-hint">Loading folders…</p>}
                  {!directoriesLoading && filteredDirectories.length === 0 && (
                    <p className="form-hint">
                      {filter.trim() ? 'No folders match.' : 'No folders in this root yet.'}
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
                        {registered && <span className="register-dir-badge">Registered</span>}
                      </button>
                    )
                  })}
                </div>
                {truncated && <p className="form-hint">Showing the first 500 folders.</p>}
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
                    placeholder="new-folder"
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
                    <span>{creatingDir ? 'Creating…' : 'Create'}</span>
                  </button>
                </div>
                {dirError && <p className="error">{dirError}</p>}
                {selectedDir && <code className="register-selected-path">{selectedDir.path}</code>}
                {registeredProject && (
                  <p className="form-hint">
                    Already registered as {registeredProject.title || registeredProject.slug}.
                  </p>
                )}
              </>
            ) : (
              !rootsEditorOpen && (
                <p className="form-hint">
                  {roots.length > 0
                    ? 'Loading folders…'
                    : 'Add a project root before choosing a folder.'}
                </p>
              )
            )}
          </section>

          <div className="register-identity-grid">
            <label className="form-group">
              <span className="form-label">Project title</span>
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
                placeholder="Generated from the folder name"
                className="form-input"
              />
            </label>
            <label className="form-group">
              <span className="form-label">Slug identifier</span>
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
                placeholder="generated-from-the-folder"
                className="form-input code-font"
                spellCheck={false}
                autoComplete="off"
              />
            </label>
          </div>
          <p className="form-hint">Generated from the folder name. You can edit either field.</p>
          {selectedDir && !isValidProjectSlug(form.slug.trim()) && (
            <p className="form-hint">
              This folder name has no letters or digits for an identifier. Enter a slug.
            </p>
          )}

          <div className="register-identity-grid">
            <label className="form-group">
              <span className="form-label">Project kind</span>
              <select
                value={form.kind}
                onChange={event =>
                  setForm(current => ({ ...current, kind: event.target.value as ProjectKind }))
                }
                className="form-select"
              >
                <option value="software">Software Engineering</option>
                <option value="docs">Documentation & Knowledge</option>
                <option value="automation">Workflow / Automation</option>
                <option value="other">Other</option>
              </select>
            </label>
            <label className="form-group">
              <span className="form-label">Language / framework</span>
              <input
                value={form.language ?? ''}
                onChange={event => setForm(current => ({ ...current, language: event.target.value }))}
                onKeyDown={event => {
                  if (event.key === 'Enter') event.preventDefault()
                }}
                placeholder="TypeScript, React, Rust"
                className="form-input"
              />
            </label>
          </div>

          <label className="form-group">
            <span className="form-label">Description</span>
            <textarea
              rows={3}
              value={form.description}
              onChange={event => setForm(current => ({ ...current, description: event.target.value }))}
              placeholder="What this project does, and anything the agent should know"
              className="form-textarea"
            />
          </label>

          <label className="form-group">
            <span className="form-label">Email notification recipients</span>
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
              placeholder="dev@example.com, ops@example.com"
              className="form-input"
            />
            <small className="form-hint">Addresses that receive task execution reports.</small>
          </label>

          <div className="modal-actions-row">
            <button type="button" className="secondary" onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className="btn-primary" disabled={!canSubmit}>
              {submitting ? 'Registering…' : 'Register Project'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
