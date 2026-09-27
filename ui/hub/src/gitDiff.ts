export function gitDiffLineClass(line: string): string {
  if (line.startsWith('+++ ') || line.startsWith('--- ')) return 'git-diff-file'
  if (line.startsWith('+')) return 'git-diff-add'
  if (line.startsWith('-')) return 'git-diff-del'
  if (line.startsWith('@@')) return 'git-diff-hunk'
  if (
    line.startsWith('diff ') ||
    line.startsWith('index ') ||
    line.startsWith('new file') ||
    line.startsWith('deleted file') ||
    line.startsWith('old mode') ||
    line.startsWith('new mode') ||
    line.startsWith('similarity index') ||
    line.startsWith('rename from') ||
    line.startsWith('rename to') ||
    line.startsWith('\\')
  ) {
    return 'git-diff-meta'
  }
  return 'git-diff-context'
}

export interface ParsedDiffFile {
  path: string
  oldPath?: string
  status: 'modified' | 'added' | 'deleted' | 'renamed'
  additions: number
  deletions: number
  lines: string[]
  diff: string
}

function cleanDiffPath(raw: string): string {
  let p = raw.trim()
  if (p.startsWith('"') && p.endsWith('"')) {
    p = p.slice(1, -1)
  }
  if (p.startsWith('a/') || p.startsWith('b/')) {
    p = p.slice(2)
  }
  return p
}

export function parseGitDiff(rawDiff: string): ParsedDiffFile[] {
  if (!rawDiff || !rawDiff.trim()) return []

  const lines = rawDiff.split('\n')
  const fileChunks: { header: string; lines: string[] }[] = []
  let currentChunk: { header: string; lines: string[] } | null = null

  for (const line of lines) {
    if (line.startsWith('diff --git ')) {
      if (currentChunk) {
        fileChunks.push(currentChunk)
      }
      currentChunk = { header: line, lines: [line] }
    } else if (currentChunk) {
      currentChunk.lines.push(line)
    }
  }
  if (currentChunk) {
    fileChunks.push(currentChunk)
  }

  // Fallback: If no "diff --git" found but diff has "--- " and "+++ "
  if (fileChunks.length === 0) {
    const plusLine = lines.find(l => l.startsWith('+++ '))
    const minusLine = lines.find(l => l.startsWith('--- '))
    if (plusLine || minusLine) {
      fileChunks.push({ header: plusLine || minusLine || '', lines })
    } else {
      return []
    }
  }

  return fileChunks.map(chunk => {
    let path = ''
    let oldPath: string | undefined
    let status: ParsedDiffFile['status'] = 'modified'
    let additions = 0
    let deletions = 0

    const diffGitMatch = chunk.header.match(/^diff --git\s+(?:"a\/(.*?)"|a\/(.*?))\s+(?:"b\/(.*?)"|b\/(.*))$/)
    if (diffGitMatch) {
      const pathA = diffGitMatch[1] || diffGitMatch[2] || ''
      const pathB = diffGitMatch[3] || diffGitMatch[4] || ''
      path = pathB || pathA
      if (pathA && pathB && pathA !== pathB) {
        oldPath = pathA
        status = 'renamed'
      }
    }

    let insideHunk = false
    for (const line of chunk.lines) {
      if (line.startsWith('new file mode')) {
        status = 'added'
      } else if (line.startsWith('deleted file mode')) {
        status = 'deleted'
      } else if (line.startsWith('rename from ')) {
        oldPath = cleanDiffPath(line.slice('rename from '.length))
        status = 'renamed'
      } else if (line.startsWith('rename to ')) {
        path = cleanDiffPath(line.slice('rename to '.length))
        status = 'renamed'
      } else if (line.startsWith('--- /dev/null')) {
        status = 'added'
      } else if (line.startsWith('+++ /dev/null')) {
        status = 'deleted'
      } else if (!path && line.startsWith('+++ ')) {
        const raw = line.slice(4).trim()
        if (raw !== '/dev/null') {
          path = cleanDiffPath(raw)
        }
      } else if (!path && line.startsWith('--- ')) {
        const raw = line.slice(4).trim()
        if (raw !== '/dev/null') {
          path = cleanDiffPath(raw)
        }
      }

      if (line.startsWith('@@')) {
        insideHunk = true
      } else if (insideHunk) {
        if (line.startsWith('+') && !line.startsWith('+++')) {
          additions++
        } else if (line.startsWith('-') && !line.startsWith('---')) {
          deletions++
        }
      }
    }

    if (!path && oldPath) {
      path = oldPath
    }

    return {
      path: path || 'unknown',
      oldPath,
      status,
      additions,
      deletions,
      lines: chunk.lines,
      diff: chunk.lines.join('\n'),
    }
  })
}
