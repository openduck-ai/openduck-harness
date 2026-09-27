import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import {
  isPotentialFilePath,
  normalizeMentionedPath,
  getFileIcon,
} from '../pathUtils.ts'

interface MarkdownPreviewProps {
  content: string
  resolveImageUrl?: (src: string) => string
  onOpenFile?: (path: string) => void
}

export function MarkdownPreview({ content, resolveImageUrl, onOpenFile }: MarkdownPreviewProps) {
  const trimmed = content.trim()
  if (!trimmed) {
    return (
      <div className="markdown-preview markdown-preview-empty">
        <p className="muted">Nothing to preview.</p>
      </div>
    )
  }

  return (
    <div className="markdown-preview">
      <div className="markdown-preview-inner">
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          components={{
            a: ({ href, children }) => {
              const isExternal = href && /^(?:https?:|\/\/|mailto:)/i.test(href)
              if (!isExternal && href && onOpenFile) {
                const cleanPath = normalizeMentionedPath(href)
                return (
                  <a
                    href={href}
                    className="markdown-file-link"
                    onClick={(e) => {
                      e.preventDefault()
                      onOpenFile(cleanPath)
                    }}
                    title={`Preview ${cleanPath}`}
                  >
                    {children}
                  </a>
                )
              }
              return (
                <a href={href} target="_blank" rel="noopener noreferrer">
                  {children}
                </a>
              )
            },
            img: ({ src, alt }) => {
              const resolvedSrc = src && resolveImageUrl ? resolveImageUrl(src) : src
              const isClickable = Boolean(src && onOpenFile)
              return (
                <img
                  src={resolvedSrc}
                  alt={alt ?? ''}
                  loading="lazy"
                  className={isClickable ? 'markdown-clickable-image' : undefined}
                  onClick={isClickable ? () => onOpenFile!(src!) : undefined}
                  title={isClickable ? `Click to preview: ${src}` : undefined}
                />
              )
            },
            code: ({ className, children, ...props }) => {
              const isInline =
                !className?.startsWith('language-') && !String(children).includes('\n')

              if (isInline && onOpenFile) {
                const text = String(children).trim()

                // If entire code span is a file path
                if (isPotentialFilePath(text)) {
                  const cleanPath = normalizeMentionedPath(text)
                  const icon = getFileIcon(cleanPath)
                  return (
                    <code
                      {...props}
                      className={`clickable-file-code ${className || ''}`}
                      onClick={(e) => {
                        e.stopPropagation()
                        onOpenFile(cleanPath)
                      }}
                      title={`Click to preview file: ${cleanPath}`}
                      role="button"
                      tabIndex={0}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault()
                          onOpenFile(cleanPath)
                        }
                      }}
                    >
                      <span className="file-code-icon">{icon}</span>
                      <span>{children}</span>
                    </code>
                  )
                }

                // If code span contains multiple tokens (like command arguments)
                if (text.includes(' ') || text.includes('\t')) {
                  const parts = text.split(/(\s+)/)
                  const hasAnyFile = parts.some((part) => isPotentialFilePath(part))
                  if (hasAnyFile) {
                    return (
                      <code {...props} className={`code-with-files ${className || ''}`}>
                        {parts.map((part, idx) => {
                          if (isPotentialFilePath(part)) {
                            const clean = normalizeMentionedPath(part)
                            const icon = getFileIcon(clean)
                            return (
                              <span
                                key={idx}
                                className="clickable-file-token"
                                onClick={(e) => {
                                  e.stopPropagation()
                                  onOpenFile(clean)
                                }}
                                title={`Click to preview file: ${clean}`}
                                role="button"
                                tabIndex={0}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter' || e.key === ' ') {
                                    e.preventDefault()
                                    onOpenFile(clean)
                                  }
                                }}
                              >
                                <span className="file-token-icon">{icon}</span>
                                {part}
                              </span>
                            )
                          }
                          return part
                        })}
                      </code>
                    )
                  }
                }
              }

              return (
                <code className={className} {...props}>
                  {children}
                </code>
              )
            },
          }}
        >
          {content}
        </ReactMarkdown>
      </div>
    </div>
  )
}
