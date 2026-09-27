import { useCallback, useEffect, useMemo, useState, lazy, Suspense } from 'react'
import {
  Eye,
  Pencil,
  Plus,
  RefreshCw,
  ScrollText,
  Trash2,
  AlertCircle,
  Lock,
} from 'lucide-react'
import {
  createRuleSource,
  deleteRuleSource,
  listRuleSources,
  updateRuleSource,
} from '../acp'
import {
  buildRuleProperties,
  draftFromSource,
  draftsEqual,
  emptyRuleDraft,
  validateRuleName,
  type RuleDraft,
} from '../rules'

const MarkdownPreview = lazy(() =>
  import('./MarkdownPreview').then(module => ({ default: module.MarkdownPreview })),
)

type ScopeFilter = 'all' | 'project' | 'global'

interface ProjectRulesProps {
  baseUrl: string
  secret: string
  projectDir: string
  projectSlug: string
}

function errorMessage(cause: unknown, fallback: string): string {
  return cause instanceof Error && cause.message.trim() ? cause.message : fallback
}

export function ProjectRules({
  baseUrl,
  secret,
  projectDir,
  projectSlug,
}: ProjectRulesProps) {
  const [rules, setRules] = useState<RuleDraft[]>([])
  const [draft, setDraft] = useState<RuleDraft | null>(null)
  const [baseline, setBaseline] = useState<RuleDraft | null>(null)
  const [filter, setFilter] = useState('')
  const [scopeFilter, setScopeFilter] = useState<ScopeFilter>('all')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const [status, setStatus] = useState('')
  const [viewMode, setViewMode] = useState<'edit' | 'preview'>('edit')

  const selected = draft
  const isNew = Boolean(selected && !selected.path)
  const isDirty = Boolean(selected && baseline && !draftsEqual(selected, baseline))
  const readOnly = Boolean(selected && !selected.writable)

  const loadRules = useCallback(
    async (selectPath?: string | null, quiet = false) => {
      if (!quiet) setLoading(true)
      setError('')
      try {
        const sources = await listRuleSources(baseUrl, secret, projectDir)
        const next = sources.map(draftFromSource)
        setRules(next)
        if (selectPath) {
          const match = next.find(rule => rule.path === selectPath)
          if (match) {
            setDraft(match)
            setBaseline(match)
            return
          }
        }
        if (selectPath === null) {
          setDraft(null)
          setBaseline(null)
          return
        }
        const first = next[0] ?? null
        setDraft(first)
        setBaseline(first)
      } catch (cause) {
        setError(errorMessage(cause, 'Unable to load rules'))
      } finally {
        setLoading(false)
      }
    },
    [baseUrl, secret, projectDir],
  )

  useEffect(() => {
    void loadRules()
  }, [loadRules])

  const filteredRules = useMemo(() => {
    const search = filter.trim().toLowerCase()
    return rules.filter(rule => {
      if (scopeFilter === 'project' && rule.global) return false
      if (scopeFilter === 'global' && !rule.global) return false
      if (!search) return true
      return (
        rule.name.toLowerCase().includes(search) ||
        rule.description.toLowerCase().includes(search) ||
        rule.path?.toLowerCase().includes(search)
      )
    })
  }, [filter, rules, scopeFilter])

  const startNewRule = (global: boolean) => {
    if (isDirty && !confirm('Discard unsaved changes to the current rule?')) return
    const next = emptyRuleDraft(global)
    setDraft(next)
    setBaseline(next)
    setViewMode('edit')
    setStatus('')
    setError('')
  }

  const selectRule = (rule: RuleDraft) => {
    if (isDirty && !confirm('Discard unsaved changes to the current rule?')) return
    setDraft(rule)
    setBaseline(rule)
    setViewMode('edit')
    setStatus('')
    setError('')
  }

  const handleSave = async () => {
    if (!selected) return
    const nameError = validateRuleName(selected.name)
    if (nameError) {
      setError(nameError)
      return
    }
    setSaving(true)
    setError('')
    setStatus('')
    const payload = {
      name: selected.name.trim(),
      description: selected.description.trim(),
      content: selected.content,
      properties: buildRuleProperties(selected),
    }
    try {
      const saved = selected.path
        ? await updateRuleSource(baseUrl, secret, {
            path: selected.path,
            ...payload,
          })
        : await createRuleSource(baseUrl, secret, {
            ...payload,
            global: selected.global,
            projectDir,
          })
      setStatus('Saved')
      await loadRules(saved.path, true)
      setTimeout(() => setStatus(''), 2500)
    } catch (cause) {
      setError(errorMessage(cause, 'Unable to save rule'))
    } finally {
      setSaving(false)
    }
  }

  const handleDelete = async () => {
    if (!selected?.path) {
      setDraft(null)
      setBaseline(null)
      return
    }
    if (!confirm(`Delete rule "${selected.name}"? This removes the markdown file on disk.`)) {
      return
    }
    setSaving(true)
    setError('')
    try {
      await deleteRuleSource(baseUrl, secret, selected.path)
      await loadRules(null, true)
    } catch (cause) {
      setError(errorMessage(cause, 'Unable to delete rule'))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="panel files-panel rules-panel">
      <div className="files-header">
        <div>
          <h3 className="rules-heading">
            <ScrollText size={16} />
            Rules
          </h3>
          <p className="muted rules-subtitle">
            Guidelines injected into agent sessions for <code>{projectSlug}</code>. Project
            rules live in <code>.agents/rules/</code>; global rules apply to every project.
          </p>
        </div>
        <div className="files-actions">
          <input
            className="filter-input"
            placeholder="Search rules…"
            value={filter}
            onChange={event => setFilter(event.target.value)}
          />
          <button
            type="button"
            className="secondary mini-btn"
            onClick={() => startNewRule(false)}
          >
            <Plus size={14} />
            Project rule
          </button>
          <button
            type="button"
            className="secondary mini-btn"
            onClick={() => startNewRule(true)}
          >
            <Plus size={14} />
            Global rule
          </button>
          <button
            type="button"
            className="secondary mini-btn"
            onClick={() => {
              if (isDirty && !confirm('Reload rules and discard unsaved changes?')) return
              void loadRules(selected?.path ?? undefined, true)
            }}
            title="Reload rules"
          >
            <RefreshCw size={14} />
          </button>
        </div>
      </div>

      <div className="rules-scope-filter" role="tablist" aria-label="Rule scope">
        {(['all', 'project', 'global'] as const).map(scope => (
          <button
            key={scope}
            type="button"
            role="tab"
            aria-selected={scopeFilter === scope}
            className={scopeFilter === scope ? 'active' : ''}
            onClick={() => setScopeFilter(scope)}
          >
            {scope === 'all' ? 'All' : scope === 'project' ? 'Project' : 'Global'}
          </button>
        ))}
      </div>

      {error && (
        <p className="error rules-error">
          <AlertCircle size={14} />
          {error}
        </p>
      )}

      <div className="files-body">
        <div className="file-list-pane">
          {loading ? (
            <p className="muted rules-empty">Loading rules…</p>
          ) : filteredRules.length === 0 && !isNew ? (
            <div className="rules-empty">
              <p>No {scopeFilter === 'all' ? '' : `${scopeFilter} `}rules yet.</p>
              <p className="muted">
                Create a project rule to constrain this workspace, or a global rule for every
                session.
              </p>
            </div>
          ) : (
            <table className="files-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Scope</th>
                </tr>
              </thead>
              <tbody>
                {isNew && selected && (
                  <tr className="file-row selected-row">
                    <td>
                      <strong>{selected.name.trim() || 'New rule'}</strong>
                      <div className="muted rules-row-desc">Unsaved</div>
                    </td>
                    <td>
                      <span className={`file-kind-badge ${selected.global ? 'global' : ''}`}>
                        {selected.global ? 'Global' : 'Project'}
                      </span>
                    </td>
                  </tr>
                )}
                {filteredRules.map(rule => (
                  <tr
                    key={rule.path}
                    className={`file-row ${selected?.path === rule.path ? 'selected-row' : ''}`}
                    onClick={() => selectRule(rule)}
                  >
                    <td>
                      <strong>{rule.name}</strong>
                      {rule.description && (
                        <div className="muted rules-row-desc">{rule.description}</div>
                      )}
                    </td>
                    <td>
                      <span className={`file-kind-badge ${rule.global ? 'global' : ''}`}>
                        {rule.global ? 'Global' : 'Project'}
                      </span>
                      {!rule.writable && (
                        <Lock size={12} className="rules-lock" aria-label="Read-only" />
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        {selected && (
          <div className="file-editor-pane rules-editor-pane">
            <div className="editor-header">
              <div className="editor-title">
                <strong>{isNew ? 'New rule' : selected.name}</strong>
                {selected.path && <small className="muted">{selected.path}</small>}
                <span className={`file-kind-badge ${selected.global ? 'global' : ''}`}>
                  {selected.global ? 'Global' : 'Project'}
                </span>
                {readOnly && <span className="file-kind-badge">Read-only</span>}
                {isDirty && <span className="dirty-badge">● Modified</span>}
                {status && <span className="save-status">{status}</span>}
              </div>
              <div className="editor-actions">
                <div className="view-mode-toggle" role="group" aria-label="Rule content view">
                  <button
                    type="button"
                    className={viewMode === 'preview' ? 'active' : ''}
                    aria-pressed={viewMode === 'preview'}
                    onClick={() => setViewMode('preview')}
                  >
                    <Eye size={13} />
                    Preview
                  </button>
                  <button
                    type="button"
                    className={viewMode === 'edit' ? 'active' : ''}
                    aria-pressed={viewMode === 'edit'}
                    onClick={() => setViewMode('edit')}
                  >
                    <Pencil size={13} />
                    Edit
                  </button>
                </div>
                <button
                  type="button"
                  className="secondary mini-btn"
                  disabled={!isDirty || saving || readOnly}
                  onClick={() => baseline && setDraft(baseline)}
                >
                  Discard
                </button>
                <button
                  type="button"
                  className="mini-btn"
                  disabled={!isDirty || saving || readOnly}
                  onClick={() => void handleSave()}
                >
                  {saving ? 'Saving…' : 'Save'}
                </button>
                <button
                  type="button"
                  className="danger mini-btn"
                  disabled={saving || (Boolean(selected.path) && readOnly)}
                  onClick={() => void handleDelete()}
                >
                  <Trash2 size={13} />
                  {selected.path ? 'Delete' : 'Cancel'}
                </button>
              </div>
            </div>

            <div className="rules-form">
              <label className="form-group">
                <span className="form-label">Name</span>
                <input
                  className="form-input"
                  value={selected.name}
                  disabled={readOnly || saving}
                  onChange={event => setDraft({ ...selected, name: event.target.value })}
                  placeholder="rust-style"
                />
              </label>
              <label className="form-group">
                <span className="form-label">Order</span>
                <input
                  className="form-input"
                  type="number"
                  value={selected.order}
                  disabled={readOnly || saving}
                  onChange={event =>
                    setDraft({ ...selected, order: Number(event.target.value) || 0 })
                  }
                />
              </label>
              <label className="form-group full-width">
                <span className="form-label">Description</span>
                <input
                  className="form-input"
                  value={selected.description}
                  disabled={readOnly || saving}
                  onChange={event => setDraft({ ...selected, description: event.target.value })}
                  placeholder="Standards for Rust development"
                />
              </label>
              <label className="form-group">
                <span className="form-label">Globs</span>
                <input
                  className="form-input code-font"
                  value={selected.globsText}
                  disabled={readOnly || saving}
                  onChange={event => setDraft({ ...selected, globsText: event.target.value })}
                  placeholder="**/*.rs, **/*.toml"
                />
              </label>
              <label className="form-group">
                <span className="form-label">Tags</span>
                <input
                  className="form-input"
                  value={selected.tagsText}
                  disabled={readOnly || saving}
                  onChange={event => setDraft({ ...selected, tagsText: event.target.value })}
                  placeholder="rust, style"
                />
              </label>
              <label className="checkbox-row rules-always-apply">
                <input
                  type="checkbox"
                  checked={selected.alwaysApply}
                  disabled={readOnly || saving}
                  onChange={event =>
                    setDraft({ ...selected, alwaysApply: event.target.checked })
                  }
                />
                Always apply this rule
              </label>
            </div>

            {viewMode === 'preview' ? (
              <Suspense
                fallback={
                  <div className="markdown-preview markdown-preview-empty">
                    <p className="muted">Loading preview…</p>
                  </div>
                }
              >
                <MarkdownPreview content={selected.content} />
              </Suspense>
            ) : (
              <textarea
                className="code-editor"
                value={selected.content}
                disabled={readOnly || saving}
                onChange={event => setDraft({ ...selected, content: event.target.value })}
                placeholder="- Prefer anyhow::Result&#10;- Don't add comments that restate the code"
                spellCheck={false}
              />
            )}
          </div>
        )}
      </div>
    </div>
  )
}
