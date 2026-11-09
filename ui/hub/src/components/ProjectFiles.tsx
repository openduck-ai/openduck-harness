import { useState, useEffect, useRef, lazy, Suspense } from 'react'
import type { FormEvent } from 'react'
import { createPortal } from 'react-dom'
import type { createApi, ProjectFileEntry } from '@aaif/goose-hub-core'
import { useTranslation } from 'react-i18next'
import { Eye, Maximize2, Minimize2, Pencil } from 'lucide-react'
import { formatDate, formatNumber } from '../i18n/format.ts'
import { isMarkdownPath } from '../markdown'
import { isImagePath, resolveProjectAssetPath } from '../pathUtils'
import { formatSelectionExtraPrompt } from '../dynamicPrompt.ts'
import {
  baselineWhenLeaving,
  directoriesToWatch,
  FILE_POLL_INTERVAL_MS,
  listingChanged,
  openFileRefreshAction,
  parentDirectory,
  withCacheBuster,
} from '../fileRefresh.ts'
import {
  DynamicPromptRunNotice,
  DynamicPromptSelectionMenu,
  useDynamicPromptTasks,
  useSelectionTaskRun,
} from './DynamicPromptSelectionMenu.tsx'
import { CopyLinkButton } from './CopyLinkButton.tsx'

const MarkdownPreview = lazy(() =>
  import('./MarkdownPreview').then(module => ({ default: module.MarkdownPreview })),
)

function formatBytes(bytes?: number): string {
  if (bytes === undefined || bytes === null) return ''
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${formatNumber(bytes / 1024, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} KB`
  return `${formatNumber(bytes / (1024 * 1024), { minimumFractionDigits: 1, maximumFractionDigits: 1 })} MB`
}

type ErrorText = { kind: 'raw'; message: string } | { kind: 'fallback'; key: string }

interface ProjectFilesProps {
  api: ReturnType<typeof createApi>
  slug: string
  rootPath: string
  linkedFile?: string | null
  shareUrl?: string
  onLinkedFileChange?: (path: string | null, replace?: boolean) => void
}

export function ProjectFiles({
  api,
  slug,
  rootPath,
  linkedFile = null,
  shareUrl,
  onLinkedFileChange,
}: ProjectFilesProps) {
  const { t } = useTranslation()
  const [currentPath, setCurrentPath] = useState('')
  const [parentPath, setParentPath] = useState<string | null>(null)
  const [entries, setEntries] = useState<ProjectFileEntry[]>([])
  const [filter, setFilter] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<ErrorText | null>(null)

  // Editor states
  const [selectedFile, setSelectedFile] = useState<string | null>(null)
  const [fileContent, setFileContent] = useState('')
  const [originalContent, setOriginalContent] = useState('')
  const [isBinary, setIsBinary] = useState(false)
  const [fileSize, setFileSize] = useState(0)
  const [saving, setSaving] = useState(false)
  const [saveStatus, setSaveStatus] = useState('')
  const [viewMode, setViewMode] = useState<'edit' | 'preview'>('edit')
  const [readOnlyPreview, setReadOnlyPreview] = useState(false)
  const [linkedFileError, setLinkedFileError] = useState('')
  const [isFullscreen, setIsFullscreen] = useState(false)
  const [diskNotice, setDiskNotice] = useState<'modified' | 'missing' | null>(null)
  const [previewRevision, setPreviewRevision] = useState(0)

  // Create / Rename modals
  const [createKind, setCreateKind] = useState<'file' | 'dir' | null>(null)
  const [createName, setCreateName] = useState('')
  const [renamingEntry, setRenamingEntry] = useState<ProjectFileEntry | null>(null)
  const [renameTarget, setRenameTarget] = useState('')
  const reviewRef = useRef<HTMLDivElement>(null)
  const currentPathRef = useRef(currentPath)
  const entriesRef = useRef(entries)
  const selectedFileRef = useRef(selectedFile)
  const fileContentRef = useRef(fileContent)
  const originalContentRef = useRef(originalContent)
  const fileSizeRef = useRef(fileSize)
  const isBinaryRef = useRef(isBinary)
  const loadGeneration = useRef(0)
  const refreshingRef = useRef(false)
  const mutatingRef = useRef(false)
  const dismissedStampRef = useRef<string | null>(null)
  const conflictStampRef = useRef<string | null>(null)
  const wasMissingRef = useRef(false)
  const suspendOpenFileSyncRef = useRef(0)
  const offscreenEntriesRef = useRef<{ directory: string; entries: ProjectFileEntry[] } | null>(
    null,
  )
  const statusTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const refreshFromDiskRef = useRef<(isCancelled: () => boolean) => Promise<void>>(async () => {})
  const dynamicTasks = useDynamicPromptTasks(api, slug)
  const {
    notice: selectionRunNotice,
    running: selectionRunBusy,
    runWithSelection,
  } = useSelectionTaskRun(api, slug)

  currentPathRef.current = currentPath
  entriesRef.current = entries
  selectedFileRef.current = selectedFile
  fileContentRef.current = fileContent
  originalContentRef.current = originalContent
  fileSizeRef.current = fileSize
  isBinaryRef.current = isBinary

  const flashStatus = (message: string, duration = 2000) => {
    setSaveStatus(message)
    if (statusTimerRef.current) clearTimeout(statusTimerRef.current)
    statusTimerRef.current = setTimeout(() => setSaveStatus(''), duration)
  }

  const applyOpenFileAction = (
    directoryPath: string,
    previousEntries: ProjectFileEntry[],
    nextEntries: ProjectFileEntry[],
  ) => {
    const selected = selectedFileRef.current
    if (!selected || mutatingRef.current) return
    const action = openFileRefreshAction({
      directoryPath,
      selectedPath: selected,
      previousEntries,
      nextEntries,
      dirty: fileContentRef.current !== originalContentRef.current,
      dismissedStamp: dismissedStampRef.current,
      wasMissing: wasMissingRef.current,
    })
    if (action.action === 'missing') {
      wasMissingRef.current = true
      setDiskNotice('missing')
      return
    }
    if (action.action === 'unchanged') return
    wasMissingRef.current = false
    if (action.action === 'conflict') {
      conflictStampRef.current = action.stamp
      setDiskNotice('modified')
      return
    }
    setDiskNotice(null)
    void pullOpenFile(selected)
  }

  const pullOpenFile = async (path: string, force = false) => {
    try {
      const res = await api.readFile(slug, path)
      if (selectedFileRef.current !== path) return
      const dirty = fileContentRef.current !== originalContentRef.current
      if (!force && dirty) return
      const sameText = res.content === fileContentRef.current
      if (force || !sameText) {
        fileContentRef.current = res.content
        originalContentRef.current = res.content
        setFileContent(res.content)
        setOriginalContent(res.content)
        flashStatus(force ? t('files.page.reloadedFromDisk') : t('files.page.updatedFromDisk'))
      }
      const sizeChanged = res.size !== fileSizeRef.current
      fileSizeRef.current = res.size
      isBinaryRef.current = res.isBinary
      setIsBinary(res.isBinary)
      setFileSize(res.size)
      if ((res.isBinary || isImagePath(path)) && (force || !sameText || sizeChanged)) {
        setPreviewRevision(revision => revision + 1)
      }
    } catch (cause) {
      setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'files.page.errorRead' })
    }
  }

  const ingestSnapshots = (
    visible: { currentPath: string; parentPath?: string | null; entries: ProjectFileEntry[] },
    extras: { directory: string; entries: ProjectFileEntry[] }[],
    syncOpenFile: boolean,
  ) => {
    const previousVisible = entriesRef.current
    const previousOffscreen = offscreenEntriesRef.current
    const fromDirectory = currentPathRef.current
    const leaving = baselineWhenLeaving({
      selectedPath: selectedFileRef.current,
      fromDirectory,
      toDirectory: visible.currentPath,
      fromEntries: previousVisible,
    })
    if (leaving) offscreenEntriesRef.current = leaving

    if (
      listingChanged(previousVisible, visible.entries) ||
      visible.currentPath !== currentPathRef.current
    ) {
      entriesRef.current = visible.entries
      currentPathRef.current = visible.currentPath
      setCurrentPath(visible.currentPath)
      setParentPath(visible.parentPath ?? null)
      setEntries(visible.entries)
      setError(null)
    }

    if (!syncOpenFile) return
    const selected = selectedFileRef.current
    if (!selected) return
    const selectedDir = parentDirectory(selected)
    if (selectedDir === visible.currentPath) {
      const baseline =
        fromDirectory === visible.currentPath
          ? previousVisible
          : previousOffscreen?.directory === selectedDir
            ? previousOffscreen.entries
            : []
      applyOpenFileAction(selectedDir, baseline, visible.entries)
      return
    }
    const extra = extras.find(item => item.directory === selectedDir)
    if (!extra) return
    const previous =
      previousOffscreen?.directory === selectedDir ? previousOffscreen.entries : []
    offscreenEntriesRef.current = extra
    applyOpenFileAction(selectedDir, previous, extra.entries)
  }

  const loadDirectory = async (path: string) => {
    const generation = ++loadGeneration.current
    setLoading(true)
    setError(null)
    try {
      const res = await api.listFiles(slug, path)
      if (generation !== loadGeneration.current) return
      ingestSnapshots(res, [], true)
    } catch (cause) {
      if (generation !== loadGeneration.current) return
      setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'files.page.errorList' })
    } finally {
      if (generation === loadGeneration.current) setLoading(false)
    }
  }

  const refreshFromDisk = async (isCancelled: () => boolean) => {
    if (refreshingRef.current || isCancelled()) return
    if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return
    refreshingRef.current = true
    const generation = loadGeneration.current
    const visiblePath = currentPathRef.current
    const selected = selectedFileRef.current
    const syncToken = suspendOpenFileSyncRef.current
    try {
      const dirs = directoriesToWatch(visiblePath, selected)
      const snapshots = await Promise.all(dirs.map(dir => api.listFiles(slug, dir)))
      if (isCancelled() || generation !== loadGeneration.current) return
      if (currentPathRef.current !== visiblePath) return
      const visible = snapshots[0]
      if (!visible || visible.currentPath !== visiblePath) return
      const extras = dirs
        .slice(1)
        .map((directory, index) => ({
          directory,
          entries: snapshots[index + 1]?.entries ?? [],
        }))
        .filter((_, index) => snapshots[index + 1]?.currentPath === dirs[index + 1])
      ingestSnapshots(visible, extras, suspendOpenFileSyncRef.current === syncToken)
    } catch {
      // A background refresh keeps the last successful listing when the request fails.
    } finally {
      refreshingRef.current = false
    }
  }

  refreshFromDiskRef.current = refreshFromDisk

  useEffect(() => {
    dismissedStampRef.current = null
    conflictStampRef.current = null
    wasMissingRef.current = false
    offscreenEntriesRef.current = null
    setDiskNotice(null)
    setPreviewRevision(0)
    void loadDirectory('')
  }, [slug])

  useEffect(() => {
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined

    const schedule = () => {
      timer = setTimeout(() => {
        void run()
      }, FILE_POLL_INTERVAL_MS)
    }

    const run = async () => {
      await refreshFromDiskRef.current(() => cancelled)
      if (!cancelled) schedule()
    }

    const onForeground = () => {
      if (document.visibilityState === 'visible') {
        void refreshFromDiskRef.current(() => cancelled)
      }
    }

    document.addEventListener('visibilitychange', onForeground)
    window.addEventListener('focus', onForeground)
    schedule()

    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
      if (statusTimerRef.current) clearTimeout(statusTimerRef.current)
      document.removeEventListener('visibilitychange', onForeground)
      window.removeEventListener('focus', onForeground)
    }
  }, [slug])

  const confirmLeaveFile = () => {
    if (fileContentRef.current === originalContentRef.current) return true
    return window.confirm(t('files.page.unsavedNavigate'))
  }

  const openFile = async (
    entry: ProjectFileEntry,
    options?: { preview?: boolean; fromDestination?: boolean; force?: boolean },
  ) => {
    if (!options?.force && entry.path !== selectedFileRef.current && !confirmLeaveFile()) {
      if (options?.fromDestination) onLinkedFileChange?.(selectedFileRef.current, true)
      return
    }
    const syncToken = ++suspendOpenFileSyncRef.current
    selectedFileRef.current = entry.path
    dismissedStampRef.current = null
    conflictStampRef.current = null
    wasMissingRef.current = false
    setDiskNotice(null)
    setLoading(true)
    setError(null)
    setSaveStatus('')
    try {
      const res = await api.readFile(slug, entry.path)
      if (suspendOpenFileSyncRef.current !== syncToken) return
      selectedFileRef.current = res.path
      fileContentRef.current = res.content
      originalContentRef.current = res.content
      fileSizeRef.current = res.size
      isBinaryRef.current = res.isBinary
      setSelectedFile(res.path)
      setFileContent(res.content)
      setOriginalContent(res.content)
      setIsBinary(res.isBinary)
      setFileSize(res.size)
      setIsFullscreen(false)
      setLinkedFileError('')
      const preview = Boolean(options?.preview) || (!res.isBinary && isMarkdownPath(res.path))
      setReadOnlyPreview(Boolean(options?.preview) && !isMarkdownPath(res.path) && !res.isBinary && !isImagePath(res.path))
      setViewMode(preview && !res.isBinary ? 'preview' : 'edit')
      if (options?.fromDestination) {
        if (res.path !== entry.path) onLinkedFileChange?.(res.path, true)
      } else {
        onLinkedFileChange?.(res.path)
      }
    } catch (cause) {
      if (suspendOpenFileSyncRef.current !== syncToken) return
      const message = cause instanceof Error ? cause.message : t('files.page.errorRead')
      if (options?.fromDestination) setLinkedFileError(message)
      else setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'files.page.errorRead' })
    } finally {
      if (suspendOpenFileSyncRef.current === syncToken) setLoading(false)
    }
  }

  const saveFile = async () => {
    if (!selectedFile) return
    mutatingRef.current = true
    setSaving(true)
    setSaveStatus('')
    try {
      await api.writeFile(slug, selectedFile, fileContent)
      setOriginalContent(fileContent)
      originalContentRef.current = fileContent
      setDiskNotice(null)
      flashStatus(t('files.page.savedSuccessfully'), 3000)
    } catch (cause) {
      setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'files.page.errorSave' })
    } finally {
      mutatingRef.current = false
      setSaving(false)
    }
  }

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault()
    if (!createKind || !createName.trim()) return
    const targetRel = currentPath ? `${currentPath}/${createName.trim()}` : createName.trim()
    mutatingRef.current = true
    try {
      await api.createFileOrDir(slug, { path: targetRel, kind: createKind })
      setCreateKind(null)
      setCreateName('')
      await loadDirectory(currentPath)
    } catch (cause) {
      setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'files.page.errorCreate' })
    } finally {
      mutatingRef.current = false
    }
  }

  const handleRename = async (e: FormEvent) => {
    e.preventDefault()
    if (!renamingEntry || !renameTarget.trim()) return
    const parent = currentPath ? `${currentPath}/` : ''
    const newPath = `${parent}${renameTarget.trim()}`
    mutatingRef.current = true
    try {
      await api.renameFile(slug, { oldPath: renamingEntry.path, newPath })
      setRenamingEntry(null)
      setRenameTarget('')
      if (selectedFile === renamingEntry.path) {
        selectedFileRef.current = newPath
        setSelectedFile(newPath)
      }
      await loadDirectory(currentPath)
    } catch (cause) {
      setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'files.page.errorRename' })
    } finally {
      mutatingRef.current = false
    }
  }

  const handleDelete = async (entry: ProjectFileEntry) => {
    if (!confirm(t('files.page.confirmDelete', { name: entry.name }))) return
    mutatingRef.current = true
    try {
      await api.deleteFile(slug, entry.path)
      if (selectedFile === entry.path) {
        selectedFileRef.current = null
        setSelectedFile(null)
        setIsFullscreen(false)
        setDiskNotice(null)
      }
      await loadDirectory(currentPath)
    } catch (cause) {
      setError(cause instanceof Error ? { kind: 'raw', message: cause.message } : { kind: 'fallback', key: 'files.page.errorDelete' })
    } finally {
      mutatingRef.current = false
    }
  }

  useEffect(() => {
    if (!isFullscreen) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        if (
          event.defaultPrevented ||
          document.querySelector('[data-dynamic-prompt-menu], [data-dynamic-prompt-confirm]')
        ) {
          return
        }
        event.preventDefault()
        setIsFullscreen(false)
      }
    }
    const previousOverflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    window.addEventListener('keydown', onKeyDown)
    return () => {
      document.body.style.overflow = previousOverflow
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [isFullscreen])

  const closeFile = (notify = true) => {
    selectedFileRef.current = null
    dismissedStampRef.current = null
    conflictStampRef.current = null
    wasMissingRef.current = false
    setSelectedFile(null)
    setDiskNotice(null)
    setIsFullscreen(false)
    setReadOnlyPreview(false)
    if (notify) onLinkedFileChange?.(null)
  }

  const linkedSlugRef = useRef(slug)
  useEffect(() => {
    const slugChanged = linkedSlugRef.current !== slug
    linkedSlugRef.current = slug
    if (!linkedFile) {
      if (!selectedFileRef.current) return
      if (!slugChanged && !confirmLeaveFile()) {
        onLinkedFileChange?.(selectedFileRef.current, true)
        return
      }
      closeFile(false)
      return
    }
    if (!slugChanged && linkedFile === selectedFileRef.current) return
    const directory = parentDirectory(linkedFile)
    if (directory !== currentPathRef.current) void loadDirectory(directory)
    void openFile(
      { name: linkedFile.split('/').pop() || linkedFile, path: linkedFile, kind: 'file' },
      { preview: true, fromDestination: true, force: slugChanged },
    )
  }, [linkedFile, slug])

  const reloadFromDisk = async () => {
    const path = selectedFileRef.current
    if (!path) return
    dismissedStampRef.current = null
    conflictStampRef.current = null
    wasMissingRef.current = false
    setDiskNotice(null)
    await pullOpenFile(path, true)
  }

  const keepLocalEdits = () => {
    dismissedStampRef.current = conflictStampRef.current
    setDiskNotice(null)
  }

  const pathParts = currentPath.split('/').filter(Boolean)
  const isDirty = fileContent !== originalContent
  const isMarkdown = selectedFile ? isMarkdownPath(selectedFile) : false
  const isImage = selectedFile ? isImagePath(selectedFile) : false
  const showMarkdownPreview = Boolean(selectedFile && isMarkdown && !isBinary && viewMode === 'preview')

  const resolveImageUrl = (src: string) => {
    if (!selectedFile) return src
    const resolvedPath = resolveProjectAssetPath(selectedFile, src)
    if (/^(?:[a-z]+:|\/\/|#|data:|blob:)/i.test(resolvedPath)) {
      return resolvedPath
    }
    const url = api.getFileBytesUrl(slug, resolvedPath)
    if (selectedFile && resolvedPath === selectedFile) return withCacheBuster(url, previewRevision)
    return url
  }

  const filteredEntries = entries.filter(entry =>
    entry.name.toLowerCase().includes(filter.toLowerCase()),
  )

  const editorPane = selectedFile ? (
    <div
      className={`file-editor-pane ${isFullscreen ? 'is-fullscreen' : ''}`}
      role={isFullscreen ? 'dialog' : undefined}
      aria-modal={isFullscreen ? true : undefined}
      aria-label={isFullscreen ? (showMarkdownPreview ? t('files.page.fullscreenPreview') : t('files.page.fullscreenEditor')) : undefined}
    >
      <div className="editor-header">
        <div className="editor-title">
          <strong>{selectedFile.split('/').pop()}</strong>
          <small className="muted">{selectedFile}</small>
          {isMarkdown && !isBinary && <span className="file-kind-badge">{t('files.page.badgeMarkdown')}</span>}
          {isImage && <span className="file-kind-badge">{t('files.page.badgeImage')}</span>}
          {isDirty && <span className="dirty-badge">{t('files.page.badgeModified')}</span>}
          {saveStatus && <span className="save-status">{saveStatus}</span>}
        </div>
        <div className="editor-actions">
          {shareUrl && <CopyLinkButton url={shareUrl} />}
          {readOnlyPreview && !isBinary && !isImage && (
            <button type="button" className="secondary mini-btn" onClick={() => setReadOnlyPreview(false)}>
              <Pencil size={13} />
              {t('files.page.editTab')}
            </button>
          )}
          {isMarkdown && !isBinary && (
            <div className="view-mode-toggle" role="group" aria-label={t('files.page.viewModeAria')}>
              <button
                type="button"
                className={viewMode === 'preview' ? 'active' : ''}
                aria-pressed={viewMode === 'preview'}
                onClick={() => setViewMode('preview')}
              >
                <Eye size={13} />
                {t('files.page.previewTab')}
              </button>
              <button
                type="button"
                className={viewMode === 'edit' ? 'active' : ''}
                aria-pressed={viewMode === 'edit'}
                onClick={() => setViewMode('edit')}
              >
                <Pencil size={13} />
                {t('files.page.editTab')}
              </button>
            </div>
          )}
          <button
            type="button"
            className="secondary mini-btn"
            disabled={!isDirty || saving}
            onClick={() => setFileContent(originalContent)}
          >
            {t('files.page.discard')}
          </button>
          <button
            type="button"
            className="mini-btn"
            disabled={!isDirty || saving || isBinary}
            onClick={() => void saveFile()}
          >
            {saving ? t('files.page.saving') : t('files.page.save')}
          </button>
          {((isMarkdown && !isBinary) || isImage) && (
            <button
              type="button"
              className="icon-btn"
              title={isFullscreen ? t('files.page.exitFullscreen') : t('files.page.previewFullscreen')}
              aria-label={isFullscreen ? t('files.page.exitFullscreenAria') : t('files.page.previewFullscreen')}
              onClick={() => {
                if (isFullscreen) {
                  setIsFullscreen(false)
                } else {
                  if (isMarkdown) setViewMode('preview')
                  setIsFullscreen(true)
                }
              }}
            >
              {isFullscreen ? <Minimize2 size={15} /> : <Maximize2 size={15} />}
            </button>
          )}
          <button
            type="button"
            className="icon-btn"
            title={t('files.page.closeViewerTitle')}
            onClick={() => closeFile()}
          >
            ×
          </button>
        </div>
      </div>

      {diskNotice && (
        <div className="disk-notice" role="status">
          <span>
            {diskNotice === 'missing'
              ? t('files.page.fileRemovedNotice')
              : t('files.page.fileChangedNotice')}
          </span>
          <div className="disk-notice-actions">
            {diskNotice === 'modified' && (
              <>
                <button type="button" className="mini-btn" onClick={() => void reloadFromDisk()}>
                  {t('files.page.reload')}
                </button>
                <button type="button" className="secondary mini-btn" onClick={keepLocalEdits}>
                  {t('files.page.keepMine')}
                </button>
              </>
            )}
            {diskNotice === 'missing' && (
              <button type="button" className="secondary mini-btn" onClick={() => closeFile()}>
                {t('files.page.closeNotice')}
              </button>
            )}
          </div>
        </div>
      )}
      <DynamicPromptRunNotice notice={selectionRunNotice} />
      {isImage ? (
        <div className="image-preview-pane">
          <img
            src={withCacheBuster(api.getFileBytesUrl(slug, selectedFile), previewRevision)}
            alt={selectedFile}
            className="project-image-preview"
          />
        </div>
      ) : isBinary ? (
        <div className="binary-notice">
          <p>{t('files.page.binaryNotice', { size: formatBytes(fileSize) })}</p>
        </div>
      ) : (
        <div ref={reviewRef} className="file-review-surface">
          {showMarkdownPreview ? (
            <Suspense
              fallback={
                <div className="markdown-preview markdown-preview-empty">
                  <p className="muted">{t('files.page.loadingPreview')}</p>
                </div>
              }
            >
              <MarkdownPreview content={fileContent} resolveImageUrl={resolveImageUrl} />
            </Suspense>
          ) : readOnlyPreview ? (
            <pre className="code-editor">{fileContent}</pre>
          ) : (
            <textarea
              className="code-editor"
              value={fileContent}
              onChange={e => setFileContent(e.target.value)}
              spellCheck={false}
            />
          )}
          <DynamicPromptSelectionMenu
            containerRef={reviewRef}
            tasks={dynamicTasks}
            busy={selectionRunBusy}
            api={api}
            projectSlug={slug}
            onSelect={(task, text) => {
              if (!selectedFile) return
              void runWithSelection(task, formatSelectionExtraPrompt(selectedFile, text))
            }}
          />
        </div>
      )}
    </div>
  ) : null

  return (
    <div className="panel files-panel">
      {linkedFileError && (
        <div className="error-banner" role="alert">
          <p>{t('deepLink.fileMissingHint')}</p>
          <p>{linkedFileError}</p>
          <button
            type="button"
            className="btn-secondary mini-btn"
            onClick={() => {
              if (!linkedFile) return
              void openFile(
                { name: linkedFile.split('/').pop() || linkedFile, path: linkedFile, kind: 'file' },
                { preview: true, fromDestination: true },
              )
            }}
          >
            {t('deepLink.retryFile')}
          </button>
        </div>
      )}
      <div className="files-header">
        <div className="breadcrumbs">
          <button
            type="button"
            className={`breadcrumb-btn ${!currentPath ? 'active' : ''}`}
            onClick={() => void loadDirectory('')}
            title={rootPath}
          >
            🏠 {slug}
          </button>
          {pathParts.map((part, idx) => {
            const subPath = pathParts.slice(0, idx + 1).join('/')
            const isLast = idx === pathParts.length - 1
            return (
              <span key={subPath} className="breadcrumb-segment">
                <span className="breadcrumb-separator">/</span>
                <button
                  type="button"
                  className={`breadcrumb-btn ${isLast ? 'active' : ''}`}
                  onClick={() => void loadDirectory(subPath)}
                >
                  {part}
                </button>
              </span>
            )
          })}
        </div>

        <div className="files-actions">
          <input
            className="filter-input"
            placeholder={t('files.page.filterPlaceholder')}
            value={filter}
            onChange={e => setFilter(e.target.value)}
          />
          <button
            type="button"
            className="secondary mini-btn"
            onClick={() => {
              setCreateKind('file')
              setCreateName('')
            }}
          >
            {t('files.page.newFileBtn')}
          </button>
          <button
            type="button"
            className="secondary mini-btn"
            onClick={() => {
              setCreateKind('dir')
              setCreateName('')
            }}
          >
            {t('files.page.newFolderBtn')}
          </button>
          <button
            type="button"
            className="secondary mini-btn"
            onClick={() => void loadDirectory(currentPath)}
            title={t('files.page.refreshDirectoryTitle')}
          >
            🔄
          </button>
        </div>
      </div>

      {error && <p className="error">{error.kind === 'raw' ? error.message : t(error.key)}</p>}

      {createKind && (
        <form className="inline-modal" onSubmit={handleCreate}>
          <span>{createKind === 'dir' ? t('files.page.newFolder') : t('files.page.newFile')}</span>
          <input
            autoFocus
            required
            placeholder={createKind === 'dir' ? 'components' : 'index.ts'}
            value={createName}
            onChange={e => setCreateName(e.target.value)}
          />
          <button type="submit">{t('files.page.create')}</button>
          <button type="button" className="secondary" onClick={() => setCreateKind(null)}>
            {t('files.page.cancel')}
          </button>
        </form>
      )}

      {renamingEntry && (
        <form className="inline-modal" onSubmit={handleRename}>
          <span>{t('files.page.renamePrompt', { name: renamingEntry.name })}</span>
          <input
            autoFocus
            required
            value={renameTarget}
            onChange={e => setRenameTarget(e.target.value)}
          />
          <button type="submit">{t('files.page.rename')}</button>
          <button type="button" className="secondary" onClick={() => setRenamingEntry(null)}>
            {t('files.page.cancel')}
          </button>
        </form>
      )}

      <div className="files-body">
        <div className="file-list-pane">
          {loading && entries.length === 0 && <p className="muted p-2">{t('files.page.loadingContents')}</p>}

          <table className="files-table">
            <thead>
              <tr>
                <th>{t('files.page.colName')}</th>
                <th>{t('files.page.colSize')}</th>
                <th>{t('files.page.colModified')}</th>
                <th className="text-right">{t('files.page.colActions')}</th>
              </tr>
            </thead>
            <tbody>
              {parentPath !== null && (
                <tr
                  className="file-row directory-row"
                  onClick={() => void loadDirectory(parentPath)}
                >
                  <td colSpan={4}>
                    <span className="file-icon">📁</span> <strong>{t('files.page.parentDirectory')}</strong>
                  </td>
                </tr>
              )}
              {filteredEntries.map(entry => {
                const isSelected = selectedFile === entry.path
                return (
                  <tr
                    key={entry.path}
                    className={`file-row ${entry.kind === 'dir' ? 'directory-row' : ''} ${
                      isSelected ? 'selected-row' : ''
                    }`}
                  >
                    <td
                      onClick={() => {
                        if (entry.kind === 'dir') {
                          void loadDirectory(entry.path)
                        } else {
                          void openFile(entry)
                        }
                      }}
                    >
                      <span className="file-icon">{entry.kind === 'dir' ? '📁' : '📄'}</span>
                      <span className="file-name">{entry.name}</span>
                    </td>
                    <td className="muted">{entry.size !== undefined ? formatBytes(entry.size) : '—'}</td>
                    <td className="muted">
                      {entry.modifiedAt ? formatDate(entry.modifiedAt) : '—'}
                    </td>
                    <td className="text-right">
                      <button
                        type="button"
                        className="icon-btn"
                        title={t('files.page.renameTitle')}
                        onClick={e => {
                          e.stopPropagation()
                          setRenamingEntry(entry)
                          setRenameTarget(entry.name)
                        }}
                      >
                        ✏️
                      </button>
                      <button
                        type="button"
                        className="icon-btn danger-icon"
                        title={t('files.page.deleteTitle')}
                        onClick={e => {
                          e.stopPropagation()
                          void handleDelete(entry)
                        }}
                      >
                        🗑️
                      </button>
                    </td>
                  </tr>
                )
              })}
              {!loading && filteredEntries.length === 0 && (
                <tr>
                  <td colSpan={4} className="muted text-center p-3">
                    {filter ? t('files.page.noMatchingFiles') : t('files.page.directoryEmpty')}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>

        {selectedFile && !isFullscreen && editorPane}
      </div>
      {selectedFile && isFullscreen && createPortal(editorPane, document.body)}
    </div>
  )
}
