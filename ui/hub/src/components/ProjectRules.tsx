import { useTranslation, Trans } from 'react-i18next'
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

type RuleError =
  | { kind: 'raw'; message: string }
  | { kind: 'fallback'; key: string }
  | { kind: 'validation'; name: string }

function errorMessage(cause: unknown, key: string): RuleError {
  return cause instanceof Error && cause.message.trim()
    ? { kind: 'raw', message: cause.message }
    : { kind: 'fallback', key }
}

export function ProjectRules({
  baseUrl,
  secret,
  projectDir,
  projectSlug,
}: ProjectRulesProps) {
  const { t } = useTranslation()
  const [rules, setRules] = useState<RuleDraft[]>([])
  const [draft, setDraft] = useState<RuleDraft | null>(null)
  const [baseline, setBaseline] = useState<RuleDraft | null>(null)
  const [filter, setFilter] = useState('')
  const [scopeFilter, setScopeFilter] = useState<ScopeFilter>('all')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<RuleError | null>(null)
  const [saved, setSaved] = useState(false)
  const [viewMode, setViewMode] = useState<'edit' | 'preview'>('edit')

  const selected = draft
  const isNew = Boolean(selected && !selected.path)
  const isDirty = Boolean(selected && baseline && !draftsEqual(selected, baseline))
  const readOnly = Boolean(selected && !selected.writable)

  const loadRules = useCallback(
    async (selectPath?: string | null, quiet = false) => {
      if (!quiet) setLoading(true)
      setError(null)
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
        setError(errorMessage(cause, 'rules.page.loadFailed'))
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
    if (isDirty && !confirm(t('rules.page.discardConfirm'))) return
    const next = emptyRuleDraft(global)
    setDraft(next)
    setBaseline(next)
    setViewMode('edit')
    setSaved(false)
    setError(null)
  }

  const selectRule = (rule: RuleDraft) => {
    if (isDirty && !confirm(t('rules.page.discardConfirm'))) return
    setDraft(rule)
    setBaseline(rule)
    setViewMode('edit')
    setSaved(false)
    setError(null)
  }

  const handleSave = async () => {
    if (!selected) return
    const nameError = validateRuleName(selected.name)
    if (nameError) {
      setError({ kind: 'validation', name: selected.name })
      return
    }
    setSaving(true)
    setError(null)
    setSaved(false)
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
      setSaved(true)
      await loadRules(saved.path, true)
      setTimeout(() => setSaved(false), 2500)
    } catch (cause) {
      setError(errorMessage(cause, 'rules.page.saveFailed'))
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
    if (!confirm(t('rules.page.deleteConfirm', { name: selected.name }))) {
      return
    }
    setSaving(true)
    setError(null)
    try {
      await deleteRuleSource(baseUrl, secret, selected.path)
      await loadRules(null, true)
    } catch (cause) {
      setError(errorMessage(cause, 'rules.page.deleteFailed'))
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
            {t('rules.page.title')}
          </h3>
          <p className="muted rules-subtitle">
            <Trans t={t} i18nKey="rules.page.subtitle" values={{ project: projectSlug }} components={{ project: <code />, path: <code /> }} />
          </p>
        </div>
        <div className="files-actions">
          <input
            className="filter-input"
            placeholder={t('rules.page.search')}
            value={filter}
            onChange={event => setFilter(event.target.value)}
          />
          <button
            type="button"
            className="secondary mini-btn"
            onClick={() => startNewRule(false)}
          >
            <Plus size={14} />
            {t('rules.page.projectRule')}
          </button>
          <button
            type="button"
            className="secondary mini-btn"
            onClick={() => startNewRule(true)}
          >
            <Plus size={14} />
            {t('rules.page.globalRule')}
          </button>
          <button
            type="button"
            className="secondary mini-btn"
            onClick={() => {
              if (isDirty && !confirm(t('rules.page.reloadConfirm'))) return
              void loadRules(selected?.path ?? undefined, true)
            }}
            title={t('rules.page.reload')}
          >
            <RefreshCw size={14} />
          </button>
        </div>
      </div>

      <div className="rules-scope-filter" role="tablist" aria-label={t('rules.page.scopeLabel')}>
        {(['all', 'project', 'global'] as const).map(scope => (
          <button
            key={scope}
            type="button"
            role="tab"
            aria-selected={scopeFilter === scope}
            className={scopeFilter === scope ? 'active' : ''}
            onClick={() => setScopeFilter(scope)}
          >
            {scope === 'all' ? t('rules.page.all') : scope === 'project' ? t('rules.page.project') : t('rules.page.global')}
          </button>
        ))}
      </div>

      {error && (
        <p className="error rules-error">
          <AlertCircle size={14} />
          {error.kind === 'raw' ? error.message : error.kind === 'validation' ? validateRuleName(error.name) : t(error.key)}
        </p>
      )}

      <div className="files-body">
        <div className="file-list-pane">
          {loading ? (
            <p className="muted rules-empty">{t('rules.page.loading')}</p>
          ) : filteredRules.length === 0 && !isNew ? (
            <div className="rules-empty">
              <p>{scopeFilter === 'all' ? t('rules.page.emptyAll') : scopeFilter === 'project' ? t('rules.page.emptyProject') : t('rules.page.emptyGlobal')}</p>
              <p className="muted">
                {t('rules.page.createHint')}
              </p>
            </div>
          ) : (
            <table className="files-table">
              <thead>
                <tr>
                  <th>{t('rules.page.name')}</th>
                  <th>{t('rules.page.scope')}</th>
                </tr>
              </thead>
              <tbody>
                {isNew && selected && (
                  <tr className="file-row selected-row">
                    <td>
                      <strong>{selected.name.trim() || t('rules.page.newRule')}</strong>
                      <div className="muted rules-row-desc">{t('rules.page.unsaved')}</div>
                    </td>
                    <td>
                      <span className={`file-kind-badge ${selected.global ? 'global' : ''}`}>
                        {selected.global ? t('rules.page.global') : t('rules.page.project')}
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
                        {rule.global ? t('rules.page.global') : t('rules.page.project')}
                      </span>
                      {!rule.writable && (
                        <Lock size={12} className="rules-lock" aria-label={t('rules.page.readOnly')} />
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
                <strong>{isNew ? t('rules.page.newRule') : selected.name}</strong>
                {selected.path && <small className="muted">{selected.path}</small>}
                <span className={`file-kind-badge ${selected.global ? 'global' : ''}`}>
                  {selected.global ? t('rules.page.global') : t('rules.page.project')}
                </span>
                {readOnly && <span className="file-kind-badge">{t('rules.page.readOnly')}</span>}
                {isDirty && <span className="dirty-badge">{t('rules.page.modified')}</span>}
                {saved && <span className="save-status">{t('rules.page.saved')}</span>}
              </div>
              <div className="editor-actions">
                <div className="view-mode-toggle" role="group" aria-label={t('rules.page.contentView')}>
                  <button
                    type="button"
                    className={viewMode === 'preview' ? 'active' : ''}
                    aria-pressed={viewMode === 'preview'}
                    onClick={() => setViewMode('preview')}
                  >
                    <Eye size={13} />
                    {t('rules.page.preview')}
                  </button>
                  <button
                    type="button"
                    className={viewMode === 'edit' ? 'active' : ''}
                    aria-pressed={viewMode === 'edit'}
                    onClick={() => setViewMode('edit')}
                  >
                    <Pencil size={13} />
                    {t('rules.page.edit')}
                  </button>
                </div>
                <button
                  type="button"
                  className="secondary mini-btn"
                  disabled={!isDirty || saving || readOnly}
                  onClick={() => baseline && setDraft(baseline)}
                >
                  {t('rules.page.discard')}
                </button>
                <button
                  type="button"
                  className="mini-btn"
                  disabled={!isDirty || saving || readOnly}
                  onClick={() => void handleSave()}
                >
                  {saving ? t('rules.page.saving') : t('rules.page.save')}
                </button>
                <button
                  type="button"
                  className="danger mini-btn"
                  disabled={saving || (Boolean(selected.path) && readOnly)}
                  onClick={() => void handleDelete()}
                >
                  <Trash2 size={13} />
                  {selected.path ? t('rules.page.delete') : t('rules.page.cancel')}
                </button>
              </div>
            </div>

            <div className="rules-form">
              <label className="form-group">
                <span className="form-label">{t('rules.page.name')}</span>
                <input
                  className="form-input"
                  value={selected.name}
                  disabled={readOnly || saving}
                  onChange={event => setDraft({ ...selected, name: event.target.value })}
                  placeholder={t('rules.page.namePlaceholder')}
                />
              </label>
              <label className="form-group">
                <span className="form-label">{t('rules.page.order')}</span>
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
                <span className="form-label">{t('rules.page.description')}</span>
                <input
                  className="form-input"
                  value={selected.description}
                  disabled={readOnly || saving}
                  onChange={event => setDraft({ ...selected, description: event.target.value })}
                  placeholder={t('rules.page.descriptionPlaceholder')}
                />
              </label>
              <label className="form-group">
                <span className="form-label">{t('rules.page.globs')}</span>
                <input
                  className="form-input code-font"
                  value={selected.globsText}
                  disabled={readOnly || saving}
                  onChange={event => setDraft({ ...selected, globsText: event.target.value })}
                  placeholder={t('rules.page.globsPlaceholder')}
                />
              </label>
              <label className="form-group">
                <span className="form-label">{t('rules.page.tags')}</span>
                <input
                  className="form-input"
                  value={selected.tagsText}
                  disabled={readOnly || saving}
                  onChange={event => setDraft({ ...selected, tagsText: event.target.value })}
                  placeholder={t('rules.page.tagsPlaceholder')}
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
                {t('rules.page.alwaysApply')}
              </label>
            </div>

            {viewMode === 'preview' ? (
              <Suspense
                fallback={
                  <div className="markdown-preview markdown-preview-empty">
                    <p className="muted">{t('rules.page.loadingPreview')}</p>
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
                placeholder={t('rules.page.contentPlaceholder')}
                spellCheck={false}
              />
            )}
          </div>
        )}
      </div>
    </div>
  )
}
