/**
 * Resolves an asset path referenced in a markdown file to a project-relative path.
 *
 * Examples:
 * - resolveProjectAssetPath('docs/guide.md', './images/flow.png') => 'docs/images/flow.png'
 * - resolveProjectAssetPath('docs/guide.md', '../assets/logo.svg') => 'assets/logo.svg'
 * - resolveProjectAssetPath('README.md', 'diagram.png') => 'diagram.png'
 * - resolveProjectAssetPath('docs/sub/guide.md', '/root-img.png') => 'root-img.png'
 * - resolveProjectAssetPath('README.md', 'https://example.com/pic.png') => 'https://example.com/pic.png'
 */
export function resolveProjectAssetPath(currentFilePath: string, assetPath: string): string {
  const trimmed = assetPath.trim()
  if (!trimmed) return ''

  // Preserve absolute URLs, protocol-relative, data URLs, blobs, and anchor fragments
  if (/^(?:[a-z]+:|\/\/|#|data:|blob:)/i.test(trimmed)) {
    return trimmed
  }

  // Determine the directory containing currentFilePath
  const baseDir = currentFilePath.includes('/')
    ? currentFilePath.slice(0, currentFilePath.lastIndexOf('/'))
    : ''

  // If path starts with leading slash, it's relative to the project root
  const isRootRelative = trimmed.startsWith('/')
  const cleanAssetPath = isRootRelative ? trimmed.replace(/^\/+/, '') : trimmed

  const rawPath = isRootRelative
    ? cleanAssetPath
    : baseDir
      ? `${baseDir}/${cleanAssetPath}`
      : cleanAssetPath

  const parts = rawPath.split('/')
  const resolved: string[] = []

  for (const part of parts) {
    if (!part || part === '.') continue
    if (part === '..') {
      resolved.pop()
    } else {
      resolved.push(part)
    }
  }

  return resolved.join('/')
}

const IMAGE_EXTENSION_REGEX = /\.(png|jpe?g|gif|webp|svg|ico|bmp|avif)$/i

export function isImagePath(path: string): boolean {
  return IMAGE_EXTENSION_REGEX.test(path.trim())
}

const MARKDOWN_EXTENSION_REGEX = /\.(md|markdown|mdown|mkd|mdx)$/i

export function isMarkdownPath(path: string): boolean {
  return MARKDOWN_EXTENSION_REGEX.test(path.trim())
}

export const KNOWN_FILE_EXTENSIONS = new Set([
  'ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs',
  'rs', 'py', 'sh', 'bash', 'zsh',
  'json', 'yaml', 'yml', 'toml', 'xml', 'csv', 'sql',
  'md', 'markdown', 'mdx', 'txt', 'log',
  'html', 'htm', 'css', 'scss', 'sass', 'less',
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico', 'bmp', 'avif',
  'pdf', 'doc', 'docx', 'xls', 'xlsx',
  'go', 'java', 'c', 'cpp', 'cc', 'h', 'hpp', 'cs', 'rb', 'php', 'vue', 'svelte',
  'env', 'lock', 'dockerfile', 'makefile', 'proto', 'graphql',
])

/**
 * Strips surrounding quotes, brackets, punctuation, and leading `./` or `file://`.
 */
export function normalizeMentionedPath(raw: string): string {
  let cleaned = raw.trim()

  // Remove file:// protocol if present
  if (cleaned.startsWith('file://')) {
    cleaned = cleaned.slice(7)
  }

  // Standardize backslashes to forward slashes
  cleaned = cleaned.replace(/\\/g, '/')

  // Repeatedly strip surrounding quotes, backticks, brackets, and trailing punctuation
  let prev = ''
  while (cleaned !== prev) {
    prev = cleaned
    cleaned = cleaned.replace(/^[`'"<(\[]+|[`'">)\]]+$/g, '')
    cleaned = cleaned.replace(/[:;,.]+$/g, '')
  }

  // Remove leading './'
  cleaned = cleaned.replace(/^\.\//, '')

  // Remove redundant consecutive slashes
  cleaned = cleaned.replace(/\/+/g, '/')

  return cleaned.trim()
}

/**
 * Checks whether a given string appears to refer to a file path.
 */
export function isPotentialFilePath(text: string): boolean {
  if (!text) return false

  const path = normalizeMentionedPath(text)
  if (!path) return false

  // Disallow whitespaces or newlines
  if (/\s/.test(path)) return false

  // Disallow common URLs or fragments
  if (/^(?:https?:\/\/|\/\/|mailto:|git@|#)/i.test(path)) return false

  // Disallow CLI flags (--test, -p)
  if (/^--?[a-zA-Z0-9]/.test(path)) return false

  // Disallow common programming/CSS tokens that contain colons, quotes, braces, brackets
  if (/[:;"'{}<>[\]|?*^$]/.test(path)) return false

  // Disallow pure version numbers or numbers (e.g. 1.0, 2.0.1, 42)
  if (/^v?\d+(\.\d+)+$/i.test(path) || /^\d+$/.test(path)) return false

  // Disallow CSS selectors (.class-name without slash)
  if (path.startsWith('.') && !path.includes('/') && !['.env', '.gitignore', '.dockerignore'].includes(path)) {
    return false
  }

  // Check extension
  const extMatch = path.match(/\.([a-zA-Z0-9_-]{1,10})$/)
  const ext = extMatch ? extMatch[1].toLowerCase() : null

  if (ext) {
    // If extension is purely numeric, it's likely a version number or port, not a file
    if (/^\d+$/.test(ext)) return false

    // If it has a known extension, it's definitely a file path
    if (KNOWN_FILE_EXTENSIONS.has(ext)) {
      return true
    }

    // If it contains directory slashes and has an extension, it's very likely a file path
    if (path.includes('/')) {
      return true
    }
  }

  // Special case: well-known dotfiles
  if (path.startsWith('.') && ['.env', '.gitignore', '.dockerignore'].includes(path)) {
    return true
  }

  // Special case: common files without extension
  const baseName = path.split('/').pop()?.toLowerCase() || ''
  if (['dockerfile', 'makefile', 'gemfile', 'licence', 'license'].includes(baseName)) {
    return true
  }

  return false
}

/**
 * Extracts unique mentioned file paths from markdown text (e.g. Final Agent Answer).
 */
export function extractMentionedFiles(text: string): string[] {
  if (!text || !text.trim()) return []

  const results: string[] = []
  const seen = new Set<string>()

  const addPath = (candidate: string) => {
    const normalized = normalizeMentionedPath(candidate)
    if (normalized && isPotentialFilePath(normalized) && !seen.has(normalized)) {
      seen.add(normalized)
      results.push(normalized)
    }
  }

  // 1. Extract from markdown links and images: [text](path) or ![alt](path)
  const mdLinkRegex = /!?\[(?:[^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g
  let linkMatch: RegExpExecArray | null
  while ((linkMatch = mdLinkRegex.exec(text)) !== null) {
    const href = linkMatch[1]
    if (href && !/^(?:https?:\/\/|\/\/|mailto:|#)/i.test(href)) {
      addPath(href)
    }
  }

  // 2. Extract from inline code: `some/path.ext` or `command some/path.ext`
  const codeSpanRegex = /`([^`\n]+)`/g
  let codeMatch: RegExpExecArray | null
  while ((codeMatch = codeSpanRegex.exec(text)) !== null) {
    const codeContent = codeMatch[1].trim()
    if (isPotentialFilePath(codeContent)) {
      addPath(codeContent)
    } else if (codeContent.includes(' ') || codeContent.includes('\t')) {
      // Split command string into arguments/tokens
      const tokens = codeContent.split(/\s+/)
      for (const token of tokens) {
        if (isPotentialFilePath(token)) {
          addPath(token)
        }
      }
    }
  }

  // 3. Extract free-text paths matching directory/file patterns
  const freePathRegex = /(?:^|[\s(])([a-zA-Z0-9_.-]+\/[a-zA-Z0-9_./-]+\.[a-zA-Z0-9_-]+)/g
  let freeMatch: RegExpExecArray | null
  while ((freeMatch = freePathRegex.exec(text)) !== null) {
    addPath(freeMatch[1])
  }

  return results
}

export type FileCategory = 'image' | 'markdown' | 'code' | 'config' | 'text' | 'binary'

export function getFileCategory(path: string): FileCategory {
  if (isImagePath(path)) return 'image'
  if (isMarkdownPath(path)) return 'markdown'

  const extMatch = path.match(/\.([a-zA-Z0-9_-]+)$/)
  const ext = extMatch ? extMatch[1].toLowerCase() : ''

  if (['ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs', 'rs', 'py', 'go', 'java', 'c', 'cpp', 'cc', 'h', 'hpp', 'cs', 'rb', 'php', 'vue', 'svelte', 'sh', 'bash', 'zsh', 'html', 'css', 'scss'].includes(ext)) {
    return 'code'
  }
  if (['json', 'yaml', 'yml', 'toml', 'xml', 'env', 'proto', 'graphql'].includes(ext)) {
    return 'config'
  }
  if (['txt', 'log', 'csv', 'sql'].includes(ext)) {
    return 'text'
  }
  return 'binary'
}

export function getFileIcon(path: string): string {
  const cat = getFileCategory(path)
  switch (cat) {
    case 'image':
      return '🖼️'
    case 'markdown':
      return '📝'
    case 'code': {
      if (path.endsWith('.sh') || path.endsWith('.bash')) return '⚙️'
      return '📄'
    }
    case 'config':
      return '⚙️'
    case 'text':
      return '📄'
    case 'binary':
      return '📦'
    default:
      return '📄'
  }
}
