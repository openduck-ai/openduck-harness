import { useTranslation } from 'react-i18next'
import { formatDateTime } from '../i18n/format.ts'
import { useState, useMemo, useEffect, useRef } from 'react'
import type {
  createApi,
  ProjectTaskDefinition,
  ProjectTaskSummary,
} from '@aaif/goose-hub-core'
import {
  ListTodo,
  Play,
  Pencil,
  Trash2,
  Plus,
  Search,
  Check,
  Loader2,
  AlertCircle,
  Clock,
  MessageSquare,
  ShieldCheck,
  ChevronDown,
  ChevronUp,
  RefreshCw,
  Sparkles,
} from 'lucide-react'

export interface ProjectChatTasksDrawerProps {
  isOpen: boolean
  onClose: () => void
  projectSlug: string
  api: ReturnType<typeof createApi>
  tasks: ProjectTaskSummary[]
  loadingTasks: boolean
  onRefreshTasks: () => Promise<void>
  onSendToChat?: (text: string) => void
  onTaskStarted?: (taskId: string, taskName: string, runPromise: Promise<unknown>) => void
}

interface TaskFormData {
  id: string
  name: string
  category: string
  tagsString: string
  prompt: string
  dynamicPrompt: boolean
  cron: string
  verifierCommand: string
  maxTurns: string
}

const emptyFormData: TaskFormData = {
  id: '',
  name: '',
  category: '',
  tagsString: '',
  prompt: '',
  dynamicPrompt: false,
  cron: '',
  verifierCommand: '',
  maxTurns: '',
}

export function ProjectChatTasksDrawer({
  isOpen,
  onClose,
  projectSlug,
  api,
  tasks,
  loadingTasks,
  onRefreshTasks,
  onSendToChat,
  onTaskStarted,
}: ProjectChatTasksDrawerProps) {
  const { t } = useTranslation()
  const [searchQuery, setSearchQuery] = useState('')
  const [selectedTag, setSelectedTag] = useState('')
  const [expandedTaskId, setExpandedTaskId] = useState<string | null>(null)
  const [editingTask, setEditingTask] = useState<ProjectTaskSummary | null>(null)
  const [isCreatingNew, setIsCreatingNew] = useState(false)
  const [formData, setFormData] = useState<TaskFormData>(emptyFormData)
  const [savingTask, setSavingTask] = useState(false)
  const [formError, setFormError] = useState<{ key: string; values?: Record<string, string> } | { raw: string } | null>(null)
  const [successMessage, setSuccessMessage] = useState<{ key: string; values: Record<string, string> } | null>(null)

  // Running task states
  const [runningTaskId, setRunningTaskId] = useState<string | null>(null)
  const [dynamicPromptModalTask, setDynamicPromptModalTask] = useState<ProjectTaskSummary | null>(null)
  const [dynamicExtraPrompt, setDynamicExtraPrompt] = useState('')

  const drawerRef = useRef<HTMLDivElement | null>(null)

  // Clear messages after a delay
  useEffect(() => {
    if (successMessage) {
      const timer = setTimeout(() => setSuccessMessage(null), 4000)
      return () => clearTimeout(timer)
    }
  }, [successMessage])

  // Extract unique tags
  const allTags = useMemo(() => {
    const set = new Set<string>()
    for (const t of tasks) {
      for (const tag of t.tags || []) {
        if (tag.trim()) set.add(tag.trim())
      }
    }
    return Array.from(set).sort()
  }, [tasks])

  // Filter tasks
  const filteredTasks = useMemo(() => {
    return tasks.filter(t => {
      if (selectedTag && !t.tags?.includes(selectedTag)) {
        return false
      }
      if (searchQuery.trim()) {
        const q = searchQuery.trim().toLowerCase()
        const matchesId = t.id.toLowerCase().includes(q)
        const matchesName = t.name?.toLowerCase().includes(q)
        const matchesCat = t.category?.toLowerCase().includes(q)
        const matchesPrompt = t.promptPreview?.toLowerCase().includes(q)
        const matchesTags = t.tags?.some(tag => tag.toLowerCase().includes(q))
        if (!matchesId && !matchesName && !matchesCat && !matchesPrompt && !matchesTags) {
          return false
        }
      }
      return true
    })
  }, [tasks, selectedTag, searchQuery])

  // Open creation form
  const handleOpenCreate = () => {
    setEditingTask(null)
    setFormData(emptyFormData)
    setFormError(null)
    setIsCreatingNew(true)
  }

  // Open edit form
  const handleOpenEdit = async (task: ProjectTaskSummary) => {
    setFormError(null)
    setEditingTask(task)
    setIsCreatingNew(false)

    try {
      const fullDef = await api.getProjectTask(projectSlug, task.id)
      const verifierCmd =
        fullDef.verifier && 'command' in fullDef.verifier
          ? fullDef.verifier.command
          : ''

      setFormData({
        id: fullDef.id,
        name: fullDef.name || '',
        category: fullDef.category || '',
        tagsString: (fullDef.tags || []).join(', '),
        prompt: fullDef.prompt || '',
        dynamicPrompt: Boolean(fullDef.dynamicPrompt),
        cron: fullDef.cron || '',
        verifierCommand: verifierCmd,
        maxTurns: fullDef.maxTurns ? String(fullDef.maxTurns) : '',
      })
    } catch {
      setFormData({
        id: task.id,
        name: task.name || '',
        category: task.category || '',
        tagsString: (task.tags || []).join(', '),
        prompt: '',
        dynamicPrompt: Boolean(task.dynamicPrompt),
        cron: task.cron || '',
        verifierCommand: '',
        maxTurns: '',
      })
    }
  }

  const handleCancelForm = () => {
    setIsCreatingNew(false)
    setEditingTask(null)
    setFormData(emptyFormData)
    setFormError(null)
  }

  // Save task (Create or Edit)
  const handleSaveForm = async (e: React.FormEvent) => {
    e.preventDefault()
    setFormError(null)

    const id = formData.id.trim()
    const prompt = formData.prompt.trim()

    if (!id) {
      setFormError({ key: 'chat.tasks.idRequired' })
      return
    }
    if (!prompt) {
      setFormError({ key: 'chat.tasks.promptRequired' })
      return
    }

    const tags = formData.tagsString
      .split(',')
      .map(s => s.trim())
      .filter(Boolean)

    const verifier = formData.verifierCommand.trim()
      ? {
          type: 'command',
          command: formData.verifierCommand.trim(),
          expectedExitCode: 0,
        }
      : undefined

    const maxTurns = formData.maxTurns.trim()
      ? parseInt(formData.maxTurns.trim(), 10)
      : undefined

    const payload: ProjectTaskDefinition = {
      id,
      name: formData.name.trim() || undefined,
      category: formData.category.trim() || undefined,
      tags,
      prompt,
      dynamicPrompt: formData.dynamicPrompt,
      cron: formData.cron.trim() || undefined,
      verifier: verifier as any,
      maxTurns: isNaN(maxTurns as number) ? undefined : maxTurns,
    }

    setSavingTask(true)
    try {
      if (editingTask) {
        await api.updateProjectTask(projectSlug, editingTask.id, payload)
        setSuccessMessage({ key: 'chat.tasks.updated', values: { id } })
      } else {
        await api.createProjectTask(projectSlug, payload)
        setSuccessMessage({ key: 'chat.tasks.createdNotice', values: { id } })
      }
      setIsCreatingNew(false)
      setEditingTask(null)
      setFormData(emptyFormData)
      await onRefreshTasks()
    } catch (err) {
      setFormError(err instanceof Error ? { raw: err.message } : { key: 'chat.tasks.saveFailed' })
    } finally {
      setSavingTask(false)
    }
  }

  // Delete task
  const handleDeleteTask = async (task: ProjectTaskSummary) => {
    const displayName = task.name?.trim() || task.id
    if (!window.confirm(t('chat.tasks.confirmDelete', { name: displayName }))) {
      return
    }

    try {
      await api.deleteProjectTask(projectSlug, task.id)
      setSuccessMessage({ key: 'chat.tasks.deleted', values: { name: displayName } })
      await onRefreshTasks()
    } catch (err) {
      setFormError(err instanceof Error ? { raw: err.message } : { key: 'chat.tasks.deleteFailed' })
    }
  }

  // Run / Start task
  const handleInitiateRun = (task: ProjectTaskSummary) => {
    if (task.dynamicPrompt) {
      setDynamicPromptModalTask(task)
      setDynamicExtraPrompt('')
    } else {
      executeRun(task.id, task.name || task.id)
    }
  }

  const executeRun = (taskId: string, taskName: string, extraPrompt?: string) => {
    setRunningTaskId(taskId)
    setDynamicPromptModalTask(null)

    const runPromise = api.runProjectTask(projectSlug, taskId, {
      record: false,
      extraPrompt: extraPrompt?.trim() || undefined,
    })

    if (onTaskStarted) {
      onTaskStarted(taskId, taskName, runPromise)
    }

    runPromise
      .then(res => {
        setSuccessMessage({ key: 'chat.tasks.finished', values: { name: taskName, status: res.status } })
        void onRefreshTasks()
      })
      .catch(err => {
        setFormError(err instanceof Error ? { raw: err.message } : { key: 'chat.tasks.runFailed', values: { name: taskName } })
      })
      .finally(() => {
        setRunningTaskId(null)
      })
  }

  // Send task reference or prompt into chat
  const handleDiscussInChat = (task: ProjectTaskSummary) => {
    if (!onSendToChat) return
    const text = t('chat.tasks.discuss', { id: task.id, name: task.name || task.id })
    onSendToChat(text)
    onClose()
  }

  if (!isOpen) return null

  return (
    <div className="modal-backdrop task-editor-backdrop" onClick={onClose}>
      <div
        ref={drawerRef}
        className="harness-card task-editor-drawer project-chat-tasks-drawer"
        role="dialog"
        aria-modal="true"
        aria-label={t('chat.tasks.title')}
        onClick={e => e.stopPropagation()}
      >
        {/* Drawer Header */}
        <div className="task-editor-drawer-header">
          <div className="chat-tasks-title-box">
            <ListTodo size={18} className="text-accent" />
            <h3 className="section-title" style={{ margin: 0 }}>
              {t('chat.tasks.title')}
            </h3>
            <span className="badge-count">{tasks.length}</span>
          </div>

          <div className="chat-tasks-header-actions">
            {!isCreatingNew && !editingTask && (
              <>
                <button
                  type="button"
                  className="btn-primary mini-btn"
                  onClick={handleOpenCreate}
                  title={t('chat.tasks.createTooltip')}
                >
                  <Plus size={13} />
                  <span>{t('chat.tasks.newTask')}</span>
                </button>
                <button
                  type="button"
                  className="secondary mini-btn icon-only"
                  onClick={() => void onRefreshTasks()}
                  disabled={loadingTasks}
                  title={t('chat.tasks.refresh')}
                >
                  <RefreshCw size={13} className={loadingTasks ? 'spinning' : ''} />
                </button>
              </>
            )}
            <button
              type="button"
              className="modal-close-btn"
              onClick={onClose}
              aria-label={t('chat.tasks.close')}
            >
              ✕
            </button>
          </div>
        </div>

        {/* Success or Error alert */}
        {successMessage && (
          <div className="alert-banner alert-success mb-3">
            <Check size={14} />
            <span>{t(successMessage.key, successMessage.values)}</span>
          </div>
        )}
        {formError && (
          <div className="alert-banner alert-danger mb-3">
            <AlertCircle size={14} />
            <span>{'raw' in formError ? formError.raw : t(formError.key, formError.values)}</span>
          </div>
        )}

        {/* VIEW 1: Task Form (Create or Edit) */}
        {(isCreatingNew || editingTask) ? (
          <form className="task-edit-form" onSubmit={handleSaveForm}>
            <div className="form-subhead mb-3">
              <h4>{editingTask ? t('chat.tasks.editTitle', { name: editingTask.name || editingTask.id }) : t('chat.tasks.createTitle')}</h4>
              <p className="muted small">
                {t('chat.tasks.formHint')}
              </p>
            </div>

            <div className="form-grid">
              <div className="form-row-2">
                <div className="form-group">
                  <label className="form-label">{t('chat.tasks.taskId')}</label>
                  <input
                    type="text"
                    className="form-input"
                    value={formData.id}
                    onChange={e => setFormData({ ...formData, id: e.target.value })}
                    placeholder={t('chat.tasks.idPlaceholder')}
                    disabled={Boolean(editingTask)}
                    required
                  />
                </div>
                <div className="form-group">
                  <label className="form-label">{t('chat.tasks.displayName')}</label>
                  <input
                    type="text"
                    className="form-input"
                    value={formData.name}
                    onChange={e => setFormData({ ...formData, name: e.target.value })}
                    placeholder={t('chat.tasks.namePlaceholder')}
                  />
                </div>
              </div>

              <div className="form-row-2">
                <div className="form-group">
                  <label className="form-label">{t('chat.tasks.category')}</label>
                  <input
                    type="text"
                    className="form-input"
                    value={formData.category}
                    onChange={e => setFormData({ ...formData, category: e.target.value })}
                    placeholder={t('chat.tasks.categoryPlaceholder')}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label">{t('chat.tasks.tags')}</label>
                  <input
                    type="text"
                    className="form-input"
                    value={formData.tagsString}
                    onChange={e => setFormData({ ...formData, tagsString: e.target.value })}
                    placeholder={t('chat.tasks.tagsPlaceholder')}
                  />
                </div>
              </div>

              <div className="form-group">
                <label className="form-label">{t('chat.tasks.instructions')}</label>
                <textarea
                  className="form-textarea"
                  rows={5}
                  value={formData.prompt}
                  onChange={e => setFormData({ ...formData, prompt: e.target.value })}
                  placeholder={t('chat.tasks.instructionsPlaceholder')}
                  required
                />
              </div>

              <div className="form-check-group">
                <label className="checkbox-label">
                  <input
                    type="checkbox"
                    checked={formData.dynamicPrompt}
                    onChange={e => setFormData({ ...formData, dynamicPrompt: e.target.checked })}
                  />
                  <span>
                    <strong>{t('chat.tasks.allowDynamic')}</strong>
                    <span className="muted small d-block">
                      {t('chat.tasks.dynamicHint')}
                    </span>
                  </span>
                </label>
              </div>

              <div className="form-row-2">
                <div className="form-group">
                  <label className="form-label">{t('chat.tasks.cron')}</label>
                  <input
                    type="text"
                    className="form-input"
                    value={formData.cron}
                    onChange={e => setFormData({ ...formData, cron: e.target.value })}
                    placeholder={t('chat.tasks.cronPlaceholder')}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label">{t('chat.tasks.maxTurns')}</label>
                  <input
                    type="number"
                    className="form-input"
                    value={formData.maxTurns}
                    onChange={e => setFormData({ ...formData, maxTurns: e.target.value })}
                    placeholder={t('chat.tasks.turnsPlaceholder')}
                    min={1}
                  />
                </div>
              </div>

              <div className="form-group">
                <label className="form-label">{t('chat.tasks.verifierCommand')}</label>
                <input
                  type="text"
                  className="form-input"
                  value={formData.verifierCommand}
                  onChange={e => setFormData({ ...formData, verifierCommand: e.target.value })}
                  placeholder={t('chat.tasks.verifierPlaceholder')}
                />
              </div>

              <div className="form-actions mt-3">
                <button
                  type="submit"
                  className="btn-primary"
                  disabled={savingTask}
                >
                  {savingTask ? (
                    <>
                      <Loader2 size={14} className="spinning mr-1" />
                      {t('chat.tasks.saving')}
                    </>
                  ) : (
                    <>
                      <Check size={14} className="mr-1" />
                      {editingTask ? t('chat.tasks.updateTask') : t('chat.tasks.createTask')}
                    </>
                  )}
                </button>
                <button
                  type="button"
                  className="secondary"
                  onClick={handleCancelForm}
                  disabled={savingTask}
                >
                  {t('chat.tasks.cancel')}
                </button>
              </div>
            </div>
          </form>
        ) : (
          /* VIEW 2: Task List (view, start, edit entry points) */
          <div className="chat-tasks-list-view">
            {/* Search and Filters */}
            <div className="chat-tasks-search-bar">
              <div className="search-input-box">
                <Search size={14} className="search-icon" />
                <input
                  type="text"
                  className="form-input search-input"
                  value={searchQuery}
                  onChange={e => setSearchQuery(e.target.value)}
                  placeholder={t('chat.tasks.searchPlaceholder')}
                />
                {searchQuery && (
                  <button
                    type="button"
                    className="clear-search-btn"
                    onClick={() => setSearchQuery('')}
                  >
                    ✕
                  </button>
                )}
              </div>
            </div>

            {/* Tag chips filter */}
            {allTags.length > 0 && (
              <div className="chat-tasks-tags-filter">
                <button
                  type="button"
                  className={`tag-chip ${selectedTag === '' ? 'active' : ''}`}
                  onClick={() => setSelectedTag('')}
                >
                  {t('chat.tasks.all', { count: tasks.length })}
                </button>
                {allTags.map(tag => (
                  <button
                    key={tag}
                    type="button"
                    className={`tag-chip ${selectedTag === tag ? 'active' : ''}`}
                    onClick={() => setSelectedTag(selectedTag === tag ? '' : tag)}
                  >
                    #{tag}
                  </button>
                ))}
              </div>
            )}

            {/* Tasks list */}
            {loadingTasks && tasks.length === 0 ? (
              <div className="tasks-loading-state">
                <Loader2 size={24} className="spinning text-accent" />
                <p>{t('chat.tasks.loading')}</p>
              </div>
            ) : filteredTasks.length === 0 ? (
              <div className="empty-tasks-box">
                <ListTodo size={32} className="muted" />
                <h4>{t('chat.tasks.noTasks')}</h4>
                <p className="muted small">
                  {tasks.length === 0
                    ? t('chat.tasks.emptyHint')
                    : t('chat.tasks.noMatches')}
                </p>
                <button
                  type="button"
                  className="btn-primary mini-btn mt-2"
                  onClick={handleOpenCreate}
                >
                  <Plus size={13} />
                  <span>{t('chat.tasks.createFirst')}</span>
                </button>
              </div>
            ) : (
              <div className="chat-tasks-cards-container">
                {filteredTasks.map(task => {
                  const isExpanded = expandedTaskId === task.id
                  const isRunning = runningTaskId === task.id || Boolean(task.currentlyRunning)

                  return (
                    <div
                      key={task.id}
                      className={`chat-task-card ${isRunning ? 'running' : ''}`}
                    >
                      <div className="task-card-main-header">
                        <div className="task-card-info">
                          <div className="task-title-line">
                            <h4 className="task-card-name">
                              {task.name || task.id}
                            </h4>
                            <span className="task-id-badge">{task.id}</span>
                            {task.category && (
                              <span className="task-cat-badge">{task.category}</span>
                            )}
                          </div>

                          {/* Chips row */}
                          <div className="task-chips-row">
                            {task.cron && (
                              <span className="meta-badge cron-badge" title={t('chat.tasks.scheduleTooltip', { cron: task.cron })}>
                                <Clock size={11} />
                                <span>{task.cron}</span>
                              </span>
                            )}
                            {task.dynamicPrompt && (
                              <span className="meta-badge dynamic-badge" title={t('chat.tasks.dynamicTooltip')}>
                                <Sparkles size={11} />
                                <span>{t('chat.tasks.dynamic')}</span>
                              </span>
                            )}
                            {task.hasVerifier && (
                              <span className="meta-badge verifier-badge" title={t('chat.tasks.verifierTooltip')}>
                                <ShieldCheck size={11} />
                                <span>{t('chat.tasks.verifier')}</span>
                              </span>
                            )}
                            {task.tags?.map(t => (
                              <span key={t} className="task-tag-pill">
                                #{t}
                              </span>
                            ))}
                          </div>
                        </div>

                        {/* Quick action buttons */}
                        <div className="task-card-quick-actions">
                          <button
                            type="button"
                            className="btn-primary mini-btn run-task-action-btn"
                            onClick={() => handleInitiateRun(task)}
                            disabled={isRunning}
                            title={t('chat.tasks.runTooltip')}
                          >
                            {isRunning ? (
                              <>
                                <Loader2 size={12} className="spinning" />
                                <span>{t('chat.tasks.running')}</span>
                              </>
                            ) : (
                              <>
                                <Play size={12} />
                                <span>{t('chat.tasks.run')}</span>
                              </>
                            )}
                          </button>

                          <button
                            type="button"
                            className="secondary mini-btn icon-only"
                            onClick={() => void handleOpenEdit(task)}
                            title={t('chat.tasks.editTooltip')}
                          >
                            <Pencil size={12} />
                          </button>

                          {onSendToChat && (
                            <button
                              type="button"
                              className="secondary mini-btn icon-only"
                              onClick={() => handleDiscussInChat(task)}
                              title={t('chat.tasks.discussTooltip')}
                            >
                              <MessageSquare size={12} />
                            </button>
                          )}

                          <button
                            type="button"
                            className="danger mini-btn icon-only"
                            onClick={() => void handleDeleteTask(task)}
                            title={t('chat.tasks.deleteTooltip')}
                          >
                            <Trash2 size={12} />
                          </button>
                        </div>
                      </div>

                      {/* Prompt preview and expand toggle */}
                      {task.promptPreview && (
                        <div className="task-preview-section">
                          <p className="task-prompt-preview">
                            {task.promptPreview}
                          </p>
                          <button
                            type="button"
                            className="toggle-expand-btn"
                            onClick={() => setExpandedTaskId(isExpanded ? null : task.id)}
                          >
                            {isExpanded ? (
                              <>
                                <span>{t('chat.tasks.hideDetails')}</span>
                                <ChevronUp size={12} />
                              </>
                            ) : (
                              <>
                                <span>{t('chat.tasks.viewDetails')}</span>
                                <ChevronDown size={12} />
                              </>
                            )}
                          </button>
                        </div>
                      )}

                      {/* Expanded details */}
                      {isExpanded && (
                        <div className="task-card-expanded-body">
                          <div className="expanded-detail-row">
                            <span className="detail-label">{t('chat.tasks.filePath')}</span>
                            <code>{task.filePath}</code>
                          </div>
                          {task.nextRunAt && (
                            <div className="expanded-detail-row">
                              <span className="detail-label">{t('chat.tasks.nextRun')}</span>
                              <span>{formatDateTime(task.nextRunAt)}</span>
                            </div>
                          )}
                          <div className="expanded-detail-row">
                            <span className="detail-label">{t('chat.tasks.created')}</span>
                            <span>{formatDateTime(task.createdAt)}</span>
                          </div>
                        </div>
                      )}
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        )}

        {/* Dynamic Extra Prompt Modal */}
        {dynamicPromptModalTask && (
          <div className="modal-backdrop sub-modal" onClick={() => setDynamicPromptModalTask(null)}>
            <div
              className="harness-card dynamic-prompt-modal-box"
              onClick={e => e.stopPropagation()}
            >
              <div className="dynamic-modal-header">
                <Sparkles size={16} className="text-accent" />
                <h4>{t('chat.tasks.runTitle', { name: dynamicPromptModalTask.name || dynamicPromptModalTask.id })}</h4>
              </div>
              <p className="muted small">
                {t('chat.tasks.extraHint')}
              </p>

              <div className="form-group mt-2">
                <label className="form-label">{t('chat.tasks.extraInstructions')}</label>
                <textarea
                  className="form-textarea"
                  rows={4}
                  value={dynamicExtraPrompt}
                  onChange={e => setDynamicExtraPrompt(e.target.value)}
                  placeholder={t('chat.tasks.extraPlaceholder')}
                  autoFocus
                />
              </div>

              <div className="modal-actions mt-3">
                <button
                  type="button"
                  className="btn-primary"
                  onClick={() =>
                    executeRun(
                      dynamicPromptModalTask.id,
                      dynamicPromptModalTask.name || dynamicPromptModalTask.id,
                      dynamicExtraPrompt,
                    )
                  }
                >
                  <Play size={13} className="mr-1" />
                  <span>{t('chat.tasks.startRun')}</span>
                </button>
                <button
                  type="button"
                  className="secondary"
                  onClick={() => setDynamicPromptModalTask(null)}
                >
                  {t('chat.tasks.cancel')}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
