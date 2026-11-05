import { useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Loader2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { createApi } from '@aaif/goose-hub-core'
import {
  clampMenuPosition,
  dynamicPromptTasks,
  type DynamicPromptTaskOption,
} from '../dynamicPrompt.ts'

export type { DynamicPromptTaskOption }

type NoticeText =
  | { kind: 'raw'; message: string }
  | { kind: 'fallback'; key: string; values?: Record<string, string> }

export type SelectionRunNotice = NoticeText & {
  tone: 'running' | 'done' | 'error'
}

export function useDynamicPromptTasks(
  api: ReturnType<typeof createApi> | null | undefined,
  projectSlug: string | null | undefined,
): DynamicPromptTaskOption[] {
  const [tasks, setTasks] = useState<DynamicPromptTaskOption[]>([])

  useEffect(() => {
    if (!api || !projectSlug) {
      setTasks([])
      return
    }
    let cancelled = false
    api
      .listProjectTasks(projectSlug)
      .then(list => {
        if (!cancelled) setTasks(dynamicPromptTasks(list))
      })
      .catch(() => {
        if (!cancelled) setTasks([])
      })
    return () => {
      cancelled = true
    }
  }, [api, projectSlug])

  return tasks
}

export function useSelectionTaskRun(
  api: ReturnType<typeof createApi> | null | undefined,
  projectSlug: string | null | undefined,
): {
  notice: SelectionRunNotice | null
  running: boolean
  runWithSelection: (task: DynamicPromptTaskOption, extraPrompt: string) => Promise<void>
} {
  const [notice, setNotice] = useState<SelectionRunNotice | null>(null)
  const [running, setRunning] = useState(false)
  const runningRef = useRef(false)

  const runWithSelection = useCallback(
    async (task: DynamicPromptTaskOption, extraPrompt: string) => {
      const trimmed = extraPrompt.trim()
      if (!api || !projectSlug || !trimmed || runningRef.current) return
      runningRef.current = true
      setRunning(true)
      setNotice({
        tone: 'running',
        kind: 'fallback',
        key: 'dynamicPrompt.menu.runningNotice',
        values: { name: task.name },
      })
      try {
        const res = await api.runProjectTask(projectSlug, task.id, { extraPrompt: trimmed })
        setNotice({
          tone: 'done',
          kind: 'fallback',
          key: 'dynamicPrompt.menu.doneNotice',
          values: { name: task.name, status: res.status },
        })
      } catch (err) {
        setNotice(
          err instanceof Error
            ? { tone: 'error', kind: 'raw', message: err.message }
            : {
                tone: 'error',
                kind: 'fallback',
                key: 'dynamicPrompt.menu.runFailed',
                values: { name: task.name },
              },
        )
      } finally {
        runningRef.current = false
        setRunning(false)
      }
    },
    [api, projectSlug],
  )

  return { notice, running, runWithSelection }
}

export function DynamicPromptRunNotice({ notice }: { notice: SelectionRunNotice | null }) {
  const { t } = useTranslation()
  if (!notice) return null
  return (
    <div className={`dynamic-prompt-run-notice is-${notice.tone}`} role="status">
      {notice.tone === 'running' && <Loader2 size={14} className="spinning" />}
      <span>{notice.kind === 'raw' ? notice.message : t(notice.key, notice.values)}</span>
    </div>
  )
}

interface MenuState {
  text: string
  x: number
  y: number
}

function readSelectedText(container: HTMLElement): string | null {
  const active = document.activeElement
  if (active instanceof HTMLTextAreaElement && container.contains(active)) {
    const start = active.selectionStart ?? 0
    const end = active.selectionEnd ?? 0
    if (end > start) {
      const text = active.value.slice(start, end).trim()
      if (text) return text
    }
  }

  const selection = window.getSelection()
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null
  const range = selection.getRangeAt(0)
  const node = range.commonAncestorContainer
  const element = node instanceof Element ? node : node.parentElement
  if (!element || !container.contains(element)) return null
  const text = selection.toString().trim()
  return text || null
}

function selectionAnchor(): { x: number; y: number } | null {
  const selection = window.getSelection()
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null
  const range = selection.getRangeAt(0)
  const rects = range.getClientRects()
  const rect = rects.length > 0 ? rects[rects.length - 1] : range.getBoundingClientRect()
  if (!rect || (!rect.width && !rect.height)) return null
  return { x: rect.left, y: rect.bottom }
}

interface PendingDynamicRun {
  task: DynamicPromptTaskOption
  basePrompt: string
  dynamicPrompt: string
  loading: boolean
  error: NoticeText | null
}

export function DynamicPromptSelectionMenu({
  containerRef,
  tasks,
  enabled = true,
  busy = false,
  api,
  projectSlug,
  onSelect,
}: {
  containerRef: { readonly current: HTMLElement | null }
  tasks: DynamicPromptTaskOption[]
  enabled?: boolean
  busy?: boolean
  api?: ReturnType<typeof createApi> | null
  projectSlug?: string | null
  onSelect: (task: DynamicPromptTaskOption, selectedText: string) => void
}) {
  const { t } = useTranslation()
  const [menu, setMenu] = useState<MenuState | null>(null)
  const [pending, setPending] = useState<PendingDynamicRun | null>(null)
  const menuRef = useRef<HTMLDivElement | null>(null)
  const onSelectRef = useRef(onSelect)
  onSelectRef.current = onSelect
  const apiRef = useRef(api)
  apiRef.current = api
  const projectSlugRef = useRef(projectSlug)
  projectSlugRef.current = projectSlug

  const openConfirm = (task: DynamicPromptTaskOption, selectedText: string) => {
    setMenu(null)
    setPending({
      task,
      basePrompt: '',
      dynamicPrompt: selectedText,
      loading: true,
      error: null,
    })
    const currentApi = apiRef.current
    const slug = projectSlugRef.current
    if (!currentApi || !slug) {
      setPending(prev =>
        prev && prev.task.id === task.id
          ? { ...prev, loading: false, error: { kind: 'fallback', key: 'dynamicPrompt.menu.projectUnavailable' } }
          : prev,
      )
      return
    }
    currentApi
      .getProjectTask(slug, task.id)
      .then(detail => {
        setPending(prev =>
          prev && prev.task.id === task.id
            ? { ...prev, basePrompt: detail.prompt, loading: false }
            : prev,
        )
      })
      .catch(err => {
        setPending(prev =>
          prev && prev.task.id === task.id
            ? {
                ...prev,
                loading: false,
                error:
                  err instanceof Error
                    ? { kind: 'raw', message: err.message }
                    : { kind: 'fallback', key: 'dynamicPrompt.menu.loadFailed' },
              }
            : prev,
        )
      })
  }

  const confirmPending = () => {
    if (!pending || pending.loading || pending.error) return
    const { task, dynamicPrompt } = pending
    setPending(null)
    onSelectRef.current(task, dynamicPrompt)
  }

  useEffect(() => {
    if (!enabled) {
      setMenu(null)
      setPending(null)
      return
    }

    let frame = 0
    const placeMenu = (text: string, x: number, y: number) => {
      const pos = clampMenuPosition(x, y, window.innerWidth, window.innerHeight)
      setMenu({ text, ...pos })
    }

    const openFromSelection = (fallbackX: number, fallbackY: number) => {
      const container = containerRef.current
      if (!container) return
      const text = readSelectedText(container)
      if (!text) {
        setMenu(null)
        return
      }
      const anchor = selectionAnchor()
      placeMenu(text, anchor?.x ?? fallbackX, anchor?.y ?? fallbackY)
    }

    const scheduleOpen = (x: number, y: number) => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => openFromSelection(x, y))
    }

    const onMouseUp = (event: MouseEvent) => {
      const target = event.target
      if (target instanceof Node && menuRef.current?.contains(target)) return
      const container = containerRef.current
      if (!container || !(target instanceof Node) || !container.contains(target)) return
      scheduleOpen(event.clientX, event.clientY)
    }

    const onContextMenu = (event: MouseEvent) => {
      const container = containerRef.current
      const target = event.target
      if (!container || !(target instanceof Node) || !container.contains(target)) return
      if (!readSelectedText(container)) return
      event.preventDefault()
      scheduleOpen(event.clientX, event.clientY)
    }

    const onKeyUp = (event: KeyboardEvent) => {
      if (
        event.key === 'Escape' ||
        event.key === 'Shift' ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey
      ) {
        return
      }
      const container = containerRef.current
      const target = event.target
      if (!container || !(target instanceof Node) || !container.contains(target)) return
      if (target instanceof HTMLElement) {
        const rect = target.getBoundingClientRect()
        scheduleOpen(rect.left + 12, rect.top + 12)
        return
      }
      scheduleOpen(24, 24)
    }

    const onMouseDown = (event: MouseEvent) => {
      const target = event.target
      if (!(target instanceof Node)) return
      if (menuRef.current?.contains(target)) return
      const container = containerRef.current
      if (container?.contains(target)) return
      setMenu(null)
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || !menuRef.current) return
      event.preventDefault()
      event.stopPropagation()
      setMenu(null)
    }

    document.addEventListener('mouseup', onMouseUp)
    document.addEventListener('mousedown', onMouseDown)
    document.addEventListener('keyup', onKeyUp)
    document.addEventListener('contextmenu', onContextMenu)
    window.addEventListener('keydown', onKeyDown, true)
    return () => {
      cancelAnimationFrame(frame)
      document.removeEventListener('mouseup', onMouseUp)
      document.removeEventListener('mousedown', onMouseDown)
      document.removeEventListener('keyup', onKeyUp)
      document.removeEventListener('contextmenu', onContextMenu)
      window.removeEventListener('keydown', onKeyDown, true)
    }
  }, [containerRef, enabled])

  useEffect(() => {
    if (!pending) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      setPending(null)
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [pending])

  const menuNode =
    enabled && menu
      ? createPortal(
    <div
      ref={menuRef}
      data-dynamic-prompt-menu
      className="dynamic-prompt-menu"
      role="menu"
      aria-label={t('dynamicPrompt.menu.ariaLabel')}
      style={{ left: menu.x, top: menu.y }}
      onMouseDown={event => event.preventDefault()}
      onContextMenu={event => event.preventDefault()}
    >
      <div className="dynamic-prompt-menu-title">{t('dynamicPrompt.menu.title')}</div>
      <div className="dynamic-prompt-menu-list">
        {tasks.length === 0 ? (
          <div className="dynamic-prompt-menu-empty">{t('dynamicPrompt.menu.empty')}</div>
        ) : null}
        {tasks.map(task => (
          <button
            key={task.id}
            type="button"
            role="menuitem"
            className="dynamic-prompt-menu-item"
            disabled={busy}
            onClick={() => openConfirm(task, menu.text)}
          >
            <span className="dynamic-prompt-menu-name">{task.name}</span>
            <span className="dynamic-prompt-menu-id">{task.id}</span>
          </button>
        ))}
      </div>
    </div>,
    document.body,
  )
      : null

  const dialog = pending
    ? createPortal(
        <div
          className="modal-backdrop dynamic-prompt-confirm-backdrop"
          data-dynamic-prompt-confirm
          onClick={() => setPending(null)}
        >
          <div
            className="glass-modal dynamic-prompt-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="dynamic-prompt-confirm-title"
            onClick={event => event.stopPropagation()}
            onKeyDown={event => {
              if (event.key !== 'Escape') return
              event.preventDefault()
              event.stopPropagation()
              setPending(null)
            }}
          >
            <div className="form-panel">
              <div className="modal-header-row">
                <div className="modal-header-title">
                  <div className="modal-icon-badge dynamic-prompt-icon-badge">✎</div>
                  <div>
                    <h3 id="dynamic-prompt-confirm-title" className="section-title" style={{ margin: 0 }}>
                      {t('dynamicPrompt.menu.confirmTitle')}
                    </h3>
                    <span className="form-hint">
                      {t('dynamicPrompt.menu.confirmHint', { name: pending.task.name })}
                    </span>
                  </div>
                </div>
                <button
                  type="button"
                  className="close-btn"
                  onClick={() => setPending(null)}
                  aria-label={t('dynamicPrompt.menu.cancel')}
                >
                  ✕
                </button>
              </div>

              <label className="form-label" htmlFor="dynamic-prompt-confirm-current">
                {t('dynamicPrompt.menu.currentPrompt')}
              </label>
              {pending.loading ? (
                <p className="muted">{t('dynamicPrompt.menu.loadingPrompt')}</p>
              ) : pending.error ? (
                <p className="dynamic-prompt-error">{pending.error.kind === 'raw' ? pending.error.message : t(pending.error.key, pending.error.values)}</p>
              ) : (
                <pre id="dynamic-prompt-confirm-current" className="dynamic-prompt-current">
                  {pending.basePrompt}
                </pre>
              )}

              <label className="form-label" htmlFor="dynamic-prompt-confirm-extra">
                {t('dynamicPrompt.menu.dynamicPrompt')}
              </label>
              <textarea
                id="dynamic-prompt-confirm-extra"
                className="form-textarea"
                rows={5}
                value={pending.dynamicPrompt}
                onChange={event =>
                  setPending(prev => (prev ? { ...prev, dynamicPrompt: event.target.value } : prev))
                }
              />

              <div className="editor-button-row" style={{ marginTop: '1rem' }}>
                <button
                  type="button"
                  className="btn-primary"
                  disabled={pending.loading || Boolean(pending.error)}
                  onClick={confirmPending}
                >
                  {t('dynamicPrompt.menu.confirm')}
                </button>
                <button type="button" className="btn-secondary" onClick={() => setPending(null)}>
                  {t('dynamicPrompt.menu.cancel')}
                </button>
              </div>
            </div>
          </div>
        </div>,
        document.body,
      )
    : null

  if (!menuNode && !dialog) return null
  return (
    <>
      {menuNode}
      {dialog}
    </>
  )
}
