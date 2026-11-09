import { useState, useEffect, useMemo, useRef } from 'react'
import type { createApi } from '@aaif/goose-hub-core'
import type { FileContentResponse } from '@aaif/goose-hub-core'
import { useTranslation, Trans } from 'react-i18next'
import {
  X,
  Copy,
  Check,
  Download,
  ExternalLink,
  Eye,
  Code,
  ArrowLeft,
  AlertCircle,
} from 'lucide-react'
import { formatNumber } from '../i18n/format.ts'
import {
  isImagePath,
  isMarkdownPath,
  getFileIcon,
  getFileCategory,
  resolveProjectAssetPath,
  normalizeMentionedPath,
} from '../pathUtils.ts'
import { MarkdownPreview } from './MarkdownPreview.tsx'
import { CopyLinkButton } from './CopyLinkButton.tsx'
import { isValidProjectFilePath } from '../urlNav.ts'
import { formatSelectionExtraPrompt } from '../dynamicPrompt.ts'
import {
  DynamicPromptRunNotice,
  DynamicPromptSelectionMenu,
  useDynamicPromptTasks,
  useSelectionTaskRun,
} from './DynamicPromptSelectionMenu.tsx'

type ErrorText = { kind: 'raw'; message: string } | { kind: 'fallback'; key: string }

interface FilePreviewModalProps {
  api: ReturnType<typeof createApi>
  projectSlug: string
  filePath: string
  shareUrl?: string
  onClose: () => void
  onOpenLinkedFile?: (path: string) => void
  onOpenInFiles?: (path: string) => void
}

function formatBytes(bytes?: number): string {
  if (bytes === undefined || bytes === null) return '—'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${formatNumber(bytes / 1024, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} KB`
  return `${formatNumber(bytes / (1024 * 1024), { minimumFractionDigits: 1, maximumFractionDigits: 1 })} MB`
}

export function FilePreviewModal({
  api,
  projectSlug,
  filePath,
  shareUrl,
  onClose,
  onOpenLinkedFile,
  onOpenInFiles,
}: FilePreviewModalProps) {
  const { t } = useTranslation()
  const currentPath = filePath
  const stackRef = useRef<string[]>([filePath])
  const [canGoBack, setCanGoBack] = useState(false)
  const [reloadToken, setReloadToken] = useState(0)

  useEffect(() => {
    const stack = stackRef.current
    if (stack[stack.length - 1] === filePath) {
      setCanGoBack(stack.length > 1)
      return
    }
    if (stack.length > 1 && stack[stack.length - 2] === filePath) stack.pop()
    else stack.push(filePath)
    setCanGoBack(stack.length > 1)
  }, [filePath])

  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<ErrorText | null>(null)
  const [fileData, setFileData] = useState<FileContentResponse | null>(null)
  const [viewMode, setViewMode] = useState<'preview' | 'raw'>('preview')
  const [copiedPath, setCopiedPath] = useState(false)
  const [copiedContent, setCopiedContent] = useState(false)
  const reviewRef = useRef<HTMLDivElement>(null)
  const dynamicTasks = useDynamicPromptTasks(api, projectSlug)
  const {
    notice: selectionRunNotice,
    running: selectionRunBusy,
    runWithSelection,
  } = useSelectionTaskRun(api, projectSlug)

  const isImage = isImagePath(currentPath)
  const isMarkdown = isMarkdownPath(currentPath)
  const category = getFileCategory(currentPath)
  const icon = getFileIcon(currentPath)
  const fileName = currentPath.split('/').pop() || currentPath

  // Fetch file content whenever currentPath changes
  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    setFileData(null)

    // For images, we can initialize with default view and fetch metadata in background
    api
      .readFile(projectSlug, currentPath)
      .then((data) => {
        if (!cancelled) {
          setFileData(data)
          setLoading(false)
        }
      })
      .catch((err) => {
        if (!cancelled) {
          // If image fails to read as text/json, we can still attempt to show image bytes
          if (isImage) {
            setFileData({
              path: currentPath,
              content: '',
              size: 0,
              isBinary: true,
            })
            setLoading(false)
          } else {
            setError(err instanceof Error ? { kind: 'raw', message: err.message } : { kind: 'fallback', key: 'files.preview.errorRead' })
            setLoading(false)
          }
        }
      })

    return () => {
      cancelled = true
    }
  }, [api, projectSlug, currentPath, isImage, reloadToken])

  // Keyboard shortcut to close on Escape
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (
          e.defaultPrevented ||
          document.querySelector('[data-dynamic-prompt-menu], [data-dynamic-prompt-confirm]')
        ) {
          return
        }
        onClose()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [onClose])

  const handleCopyPath = async () => {
    try {
      await navigator.clipboard.writeText(currentPath)
      setCopiedPath(true)
      setTimeout(() => setCopiedPath(false), 2000)
    } catch {
      // Ignore clipboard write error
    }
  }

  const handleCopyContent = async () => {
    if (!fileData?.content) return
    try {
      await navigator.clipboard.writeText(fileData.content)
      setCopiedContent(true)
      setTimeout(() => setCopiedContent(false), 2000)
    } catch {
      // Ignore clipboard write error
    }
  }

  const handleNavigateInternal = (nextPath: string) => {
    let clean = normalizeMentionedPath(nextPath)
    if (!isValidProjectFilePath(clean)) {
      clean = resolveProjectAssetPath(currentPath, nextPath)
    }
    if (!isValidProjectFilePath(clean) || clean === currentPath) return
    onOpenLinkedFile?.(clean)
  }

  const handleBack = () => {
    if (canGoBack) window.history.back()
  }

  const lines = useMemo(() => {
    if (!fileData?.content) return []
    return fileData.content.split('\n')
  }, [fileData?.content])

  const imageBytesUrl = useMemo(() => {
    return api.getFileBytesUrl(projectSlug, currentPath)
  }, [api, projectSlug, currentPath])

  return (
    <div className="modal-backdrop file-preview-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div className="glass-modal file-preview-modal" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <div className="file-preview-header">
          <div className="file-preview-header-left">
            {canGoBack && (
              <button
                type="button"
                className="file-preview-btn file-preview-back-btn"
                onClick={handleBack}
                title={t('files.preview.backTitle')}
              >
                <ArrowLeft size={16} />
              </button>
            )}
            <div className="file-preview-icon-badge" title={category}>
              {icon}
            </div>
            <div className="file-preview-title-info">
              <div className="file-preview-filename-row">
                <h3 className="file-preview-filename">{fileName}</h3>
                <span className="file-kind-badge">{category.toUpperCase()}</span>
                {fileData?.size !== undefined && fileData.size > 0 && (
                  <span className="file-size-badge">{formatBytes(fileData.size)}</span>
                )}
                {lines.length > 0 && !isImage && (
                  <span className="file-lines-badge">{t('files.preview.lines', { count: lines.length })}</span>
                )}
              </div>
              <p className="file-preview-path" title={currentPath}>
                {currentPath}
              </p>
            </div>
          </div>

          <div className="file-preview-header-actions">
            {/* Markdown Preview/Raw tabs */}
            {isMarkdown && !fileData?.isBinary && (
              <div className="file-preview-tabs">
                <button
                  type="button"
                  className={`file-preview-tab-btn ${viewMode === 'preview' ? 'active' : ''}`}
                  onClick={() => setViewMode('preview')}
                  title={t('files.preview.formattedMarkdown')}
                >
                  <Eye size={13} />
                  <span>{t('files.preview.preview')}</span>
                </button>
                <button
                  type="button"
                  className={`file-preview-tab-btn ${viewMode === 'raw' ? 'active' : ''}`}
                  onClick={() => setViewMode('raw')}
                  title={t('files.preview.rawMarkdown')}
                >
                  <Code size={13} />
                  <span>{t('files.preview.raw')}</span>
                </button>
              </div>
            )}

            {shareUrl && <CopyLinkButton url={shareUrl} className="file-preview-btn" />}

            {/* Copy Path */}
            <button
              type="button"
              className={`file-preview-btn ${copiedPath ? 'is-copied' : ''}`}
              onClick={handleCopyPath}
              title={t('files.preview.copyPathTitle')}
            >
              {copiedPath ? <Check size={14} /> : <Copy size={14} />}
              <span>{copiedPath ? t('files.preview.copied') : t('files.preview.copyPath')}</span>
            </button>

            {/* Copy Content (for text files) */}
            {fileData && !fileData.isBinary && !isImage && (
              <button
                type="button"
                className={`file-preview-btn ${copiedContent ? 'is-copied' : ''}`}
                onClick={handleCopyContent}
                title={t('files.preview.copyContentTitle')}
              >
                {copiedContent ? <Check size={14} /> : <Copy size={14} />}
                <span>{copiedContent ? t('files.preview.copied') : t('files.preview.copyContent')}</span>
              </button>
            )}

            {/* Download */}
            <a
              href={imageBytesUrl}
              download={fileName}
              className="file-preview-btn file-preview-download-btn"
              title={t('files.preview.downloadFileTitle')}
            >
              <Download size={14} />
              <span>{t('files.preview.download')}</span>
            </a>

            {/* Open in Files Tab */}
            {onOpenInFiles && (
              <button
                type="button"
                className="file-preview-btn file-preview-external-btn"
                onClick={() => onOpenInFiles(currentPath)}
                title={t('files.preview.openInFilesTitle')}
              >
                <ExternalLink size={14} />
                <span>{t('files.preview.openInFiles')}</span>
              </button>
            )}

            {/* Close */}
            <button
              type="button"
              className="file-preview-close-btn"
              onClick={onClose}
              title={t('files.preview.closeTitle')}
              aria-label={t('files.preview.closeAria')}
            >
              <X size={18} />
            </button>
          </div>
        </div>

        <DynamicPromptRunNotice notice={selectionRunNotice} />

        {/* Modal Body */}
        <div className="file-preview-body" ref={reviewRef}>
          {loading ? (
            <div className="file-preview-loading">
              <div className="file-preview-spinner" />
              <p>{t('files.preview.loading')}</p>
            </div>
          ) : error ? (
            <div className="file-preview-error">
              <AlertCircle size={32} />
              <h4>{t('files.preview.errorHeading')}</h4>
              <p className="file-preview-error-msg">{error.kind === 'raw' ? error.message : t(error.key)}</p>
              <p className="muted file-preview-error-hint">
                <Trans
                  t={t}
                  i18nKey="files.preview.errorHint"
                  values={{ path: currentPath }}
                  components={{ code: <code /> }}
                />
              </p>
              <button type="button" className="btn-secondary" onClick={() => setReloadToken(token => token + 1)}>
                {t('files.preview.retry')}
              </button>
            </div>
          ) : isImage ? (
            <div className="file-preview-image-container">
              <img
                src={imageBytesUrl}
                alt={currentPath}
                className="file-preview-image-img"
              />
            </div>
          ) : isMarkdown && viewMode === 'preview' && fileData ? (
            <div className="file-preview-markdown-container">
              <MarkdownPreview
                content={fileData.content}
                resolveImageUrl={(src) =>
                  api.getFileBytesUrl(projectSlug, resolveProjectAssetPath(currentPath, src))
                }
                onOpenFile={handleNavigateInternal}
              />
            </div>
          ) : fileData?.isBinary ? (
            <div className="file-preview-binary-notice">
              <p>{t('files.preview.binaryNotice', { size: formatBytes(fileData.size) })}</p>
              <a href={imageBytesUrl} download={fileName} className="btn-action file-preview-btn-primary">
                <Download size={15} />{' '}{t('files.preview.downloadBinary')}
              </a>
            </div>
          ) : fileData ? (
            <div className="file-preview-code-container">
              <div className="file-preview-line-numbers" aria-hidden="true">
                {lines.map((_, i) => (
                  <span key={i} className="file-line-num">
                    {i + 1}
                  </span>
                ))}
              </div>
              <pre className="file-preview-code-pre">
                <code>{fileData.content}</code>
              </pre>
            </div>
          ) : null}
        </div>
        <DynamicPromptSelectionMenu
          containerRef={reviewRef}
          tasks={dynamicTasks}
          enabled={Boolean(fileData && !fileData.isBinary && !isImage && !loading && !error)}
          busy={selectionRunBusy}
          api={api}
          projectSlug={projectSlug}
          onSelect={(task, text) => {
            void runWithSelection(task, formatSelectionExtraPrompt(currentPath, text))
          }}
        />
      </div>
    </div>
  )
}
