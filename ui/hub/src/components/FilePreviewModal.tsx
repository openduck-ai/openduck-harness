import { useState, useEffect, useMemo, useRef } from 'react'
import type { createApi } from '@aaif/goose-hub-core'
import type { FileContentResponse } from '@aaif/goose-hub-core'
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
import {
  isImagePath,
  isMarkdownPath,
  getFileIcon,
  getFileCategory,
  resolveProjectAssetPath,
  normalizeMentionedPath,
} from '../pathUtils.ts'
import { MarkdownPreview } from './MarkdownPreview.tsx'
import { formatSelectionExtraPrompt } from '../dynamicPrompt.ts'
import {
  DynamicPromptRunNotice,
  DynamicPromptSelectionMenu,
  useDynamicPromptTasks,
  useSelectionTaskRun,
} from './DynamicPromptSelectionMenu.tsx'

interface FilePreviewModalProps {
  api: ReturnType<typeof createApi>
  projectSlug: string
  filePath: string
  onClose: () => void
  onNavigateToFile?: (path: string) => void
}

function formatBytes(bytes?: number): string {
  if (bytes === undefined || bytes === null) return '—'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

export function FilePreviewModal({
  api,
  projectSlug,
  filePath,
  onClose,
  onNavigateToFile,
}: FilePreviewModalProps) {
  const [history, setHistory] = useState<string[]>([filePath])
  const currentPath = history[history.length - 1] || filePath

  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
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
            setError(err instanceof Error ? err.message : 'Failed to read file')
            setLoading(false)
          }
        }
      })

    return () => {
      cancelled = true
    }
  }, [api, projectSlug, currentPath, isImage])

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
    const clean = normalizeMentionedPath(nextPath)
    if (clean && clean !== currentPath) {
      setHistory((prev) => [...prev, clean])
    }
  }

  const handleBack = () => {
    if (history.length > 1) {
      setHistory((prev) => prev.slice(0, -1))
    }
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
            {history.length > 1 && (
              <button
                type="button"
                className="file-preview-btn file-preview-back-btn"
                onClick={handleBack}
                title="Back to previous file"
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
                  <span className="file-lines-badge">{lines.length} lines</span>
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
                  title="Formatted Markdown"
                >
                  <Eye size={13} />
                  <span>Preview</span>
                </button>
                <button
                  type="button"
                  className={`file-preview-tab-btn ${viewMode === 'raw' ? 'active' : ''}`}
                  onClick={() => setViewMode('raw')}
                  title="Raw Markdown"
                >
                  <Code size={13} />
                  <span>Raw</span>
                </button>
              </div>
            )}

            {/* Copy Path */}
            <button
              type="button"
              className={`file-preview-btn ${copiedPath ? 'is-copied' : ''}`}
              onClick={handleCopyPath}
              title="Copy file path"
            >
              {copiedPath ? <Check size={14} /> : <Copy size={14} />}
              <span>{copiedPath ? 'Copied' : 'Copy Path'}</span>
            </button>

            {/* Copy Content (for text files) */}
            {fileData && !fileData.isBinary && !isImage && (
              <button
                type="button"
                className={`file-preview-btn ${copiedContent ? 'is-copied' : ''}`}
                onClick={handleCopyContent}
                title="Copy file contents"
              >
                {copiedContent ? <Check size={14} /> : <Copy size={14} />}
                <span>{copiedContent ? 'Copied' : 'Copy Content'}</span>
              </button>
            )}

            {/* Download */}
            <a
              href={imageBytesUrl}
              download={fileName}
              className="file-preview-btn file-preview-download-btn"
              title="Download file"
            >
              <Download size={14} />
              <span>Download</span>
            </a>

            {/* Open in Files Tab */}
            {onNavigateToFile && (
              <button
                type="button"
                className="file-preview-btn file-preview-external-btn"
                onClick={() => {
                  onClose()
                  onNavigateToFile(currentPath)
                }}
                title="Open in Project Files tab"
              >
                <ExternalLink size={14} />
                <span>Open in Files</span>
              </button>
            )}

            {/* Close */}
            <button
              type="button"
              className="file-preview-close-btn"
              onClick={onClose}
              title="Close preview (Esc)"
              aria-label="Close"
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
              <p>Loading file content…</p>
            </div>
          ) : error ? (
            <div className="file-preview-error">
              <AlertCircle size={32} />
              <h4>Unable to load file</h4>
              <p className="file-preview-error-msg">{error}</p>
              <p className="muted file-preview-error-hint">
                The file <code>{currentPath}</code> may not exist or has been removed from the repository.
              </p>
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
              <p>Binary file ({formatBytes(fileData.size)}). Viewing inline is not supported.</p>
              <a href={imageBytesUrl} download={fileName} className="btn-action file-preview-btn-primary">
                <Download size={15} /> Download Binary
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
