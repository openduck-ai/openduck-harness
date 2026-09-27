import { useState } from 'react'
import type { FormEvent } from 'react'
import type { ProjectRoot } from '@aaif/goose-hub-core'
import { Trash2 } from 'lucide-react'

interface ProjectRootsEditorProps {
  roots: ProjectRoot[]
  loading: boolean
  error: string
  framed?: boolean
  onAdd: (path: string, allowUntrustedPath: boolean) => Promise<ProjectRoot>
  onRemove: (path: string) => Promise<void>
  onAdded?: (root: ProjectRoot) => void
  clearError: () => void
}

export function ProjectRootsEditor({
  roots,
  loading,
  error,
  framed = false,
  onAdd,
  onRemove,
  onAdded,
  clearError,
}: ProjectRootsEditorProps) {
  const [draft, setDraft] = useState('')
  const [allowUntrusted, setAllowUntrusted] = useState(false)
  const [localError, setLocalError] = useState('')
  const [adding, setAdding] = useState(false)
  const [removingPath, setRemovingPath] = useState<string | null>(null)
  const message = localError || error

  const addRoot = async (event?: FormEvent) => {
    event?.preventDefault()
    const path = draft.trim()
    if (!path) {
      setLocalError('Enter an absolute directory path.')
      return
    }
    setLocalError('')
    clearError()
    setAdding(true)
    try {
      const root = await onAdd(path, allowUntrusted)
      setDraft('')
      setAllowUntrusted(false)
      onAdded?.(root)
    } catch {
      // The hook stores the message shown above the field.
    } finally {
      setAdding(false)
    }
  }

  const removeRoot = async (path: string) => {
    setLocalError('')
    clearError()
    setRemovingPath(path)
    try {
      await onRemove(path)
    } catch {
      // The hook stores the message.
    } finally {
      setRemovingPath(null)
    }
  }

  return (
    <div className={`register-roots-editor${framed ? ' framed' : ''}`}>
      {loading && roots.length === 0 && <p className="form-hint">Loading project roots…</p>}
      {!loading && roots.length === 0 && (
        <p className="form-hint">No roots configured yet. Add a parent folder for your projects.</p>
      )}
      {roots.length > 0 && (
        <ul className="register-root-list">
          {roots.map(root => (
            <li
              key={root.path}
              className={`register-root-item${root.available ? '' : ' unavailable'}`}
            >
              <span className="register-root-path" title={root.path}>
                {root.path}
              </span>
              {!root.available && <span className="register-dir-badge">Missing</span>}
              <button
                type="button"
                className="secondary mini-btn"
                disabled={removingPath !== null || adding}
                onClick={() => void removeRoot(root.path)}
              >
                <Trash2 size={13} />
                <span>{removingPath === root.path ? 'Removing…' : 'Remove'}</span>
              </button>
            </li>
          ))}
        </ul>
      )}

      {message && <p className="error">{message}</p>}

      <label className="form-group">
        <span className="form-label">Add root directory</span>
        <input
          value={draft}
          onChange={event => {
            setDraft(event.target.value)
            setLocalError('')
            clearError()
          }}
          onKeyDown={event => {
            if (event.key === 'Enter') {
              event.preventDefault()
              void addRoot()
            }
          }}
          placeholder="/home/user/projects"
          className="form-input code-font"
          spellCheck={false}
          autoComplete="off"
        />
      </label>
      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={allowUntrusted}
          onChange={event => setAllowUntrusted(event.target.checked)}
        />
        <span>Allow this root outside the home directory</span>
      </label>
      <div>
        <button
          type="button"
          className="secondary"
          disabled={adding || removingPath !== null || !draft.trim()}
          onClick={() => void addRoot()}
        >
          {adding ? 'Adding…' : 'Add root'}
        </button>
      </div>
    </div>
  )
}
