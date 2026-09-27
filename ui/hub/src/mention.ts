import type { createApi, ProjectFileEntry, FileListResponse } from '@aaif/goose-hub-core'
import { getSourcesClient, listRuleSources } from './acp.ts'

export type MentionItemType = 'Attachment' | 'Agent' | 'File' | 'Directory' | 'Rule'

export interface MentionDisplayItem {
  name: string
  description?: string
  extra: string
  itemType: MentionItemType
  relativePath: string
  insertText: string
}

export interface FuzzyMatchResult {
  score: number
  matches: number[]
}

export interface MentionMatchItem extends MentionDisplayItem {
  matchScore: number
  matches: number[]
  matchedText: string
}

export interface MentionTriggerInfo {
  query: string
  mentionStart: number
}

export interface DismissedMention {
  mentionStart: number
  query: string
}

/**
 * Check if the detected mention trigger matches a previously dismissed mention state.
 */
export function shouldIgnoreMentionTrigger(
  trigger: MentionTriggerInfo | null,
  dismissed: DismissedMention | null,
): boolean {
  if (!trigger || !dismissed) return false
  return trigger.mentionStart === dismissed.mentionStart && trigger.query === dismissed.query
}

/**
 * Debounce helper for delaying callback execution.
 */
export function debounce<Args extends unknown[]>(
  fn: (...args: Args) => void,
  waitMs: number,
): ((...args: Args) => void) & { cancel: () => void } {
  let timeoutId: ReturnType<typeof setTimeout> | null = null

  const debounced = (...args: Args) => {
    if (timeoutId !== null) {
      clearTimeout(timeoutId)
    }
    timeoutId = setTimeout(() => {
      timeoutId = null
      fn(...args)
    }, waitMs)
  }

  debounced.cancel = () => {
    if (timeoutId !== null) {
      clearTimeout(timeoutId)
      timeoutId = null
    }
  }

  return debounced
}

const typeOrder: Record<MentionItemType, number> = {
  Attachment: 0,
  Agent: 1,
  Rule: 2,
  Directory: 3,
  File: 4,
}

/**
 * Enhanced fuzzy matching algorithm with word boundary bonuses and match index tracking.
 */
export function fuzzyMatch(pattern: string, text: string): FuzzyMatchResult {
  if (!pattern) return { score: 0, matches: [] }

  const patternLower = pattern.toLowerCase()
  const textLower = text.toLowerCase()

  // If there's an exact substring match, prefer its contiguous index range
  const subIndex = textLower.indexOf(patternLower)
  if (subIndex !== -1) {
    const matches = Array.from({ length: patternLower.length }, (_, k) => subIndex + k)
    let score = 50 + patternLower.length * 10 - text.length * 0.05

    // Exact prefix bonus
    if (subIndex === 0) {
      score += 30
    }

    // Word boundary bonus
    if (
      subIndex === 0 ||
      textLower[subIndex - 1] === '/' ||
      textLower[subIndex - 1] === '\\' ||
      textLower[subIndex - 1] === '_' ||
      textLower[subIndex - 1] === '-' ||
      textLower[subIndex - 1] === '.' ||
      textLower[subIndex - 1] === ' '
    ) {
      score += 25
    }

    // Filename match bonus
    const lastSlash = Math.max(textLower.lastIndexOf('/'), textLower.lastIndexOf('\\'))
    if (lastSlash !== -1 && subIndex === lastSlash + 1) {
      score += 30
    }

    return { score, matches }
  }

  const matches: number[] = []
  let patternIndex = 0
  let score = 0
  let consecutiveMatches = 0

  for (let i = 0; i < textLower.length && patternIndex < patternLower.length; i++) {
    if (textLower[i] === patternLower[patternIndex]) {
      matches.push(i)
      patternIndex++
      consecutiveMatches++

      // Consecutive match bonus
      score += consecutiveMatches * 3

      // Word boundary bonuses
      if (
        i === 0 ||
        textLower[i - 1] === '/' ||
        textLower[i - 1] === '\\' ||
        textLower[i - 1] === '_' ||
        textLower[i - 1] === '-' ||
        textLower[i - 1] === '.' ||
        textLower[i - 1] === ' '
      ) {
        score += 10
      }

      // Bonus for matching beginning of filename (after last /)
      const lastSlash = Math.max(textLower.lastIndexOf('/', i), textLower.lastIndexOf('\\', i))
      if (lastSlash !== -1 && i === lastSlash + 1) {
        score += 15
      }
    } else {
      consecutiveMatches = 0
    }
  }

  // If entire pattern matched
  if (patternIndex === patternLower.length) {
    score -= text.length * 0.05

    const fileName = text.split(/[/\\]/).pop()?.toLowerCase() || ''
    if (fileName.includes(patternLower)) {
      score += 25
    }

    return { score, matches }
  }

  return { score: -1, matches: [] }
}

/**
 * Filter and rank mention items based on the search query.
 */
export function filterAndRankMentionItems(
  items: MentionDisplayItem[],
  query: string,
): MentionMatchItem[] {
  const trimmedQuery = query.trim()

  if (!trimmedQuery) {
    return items
      .map(item => ({
        ...item,
        matchScore: 100 - (typeOrder[item.itemType] ?? 99) * 10,
        matches: [],
        matchedText: item.name,
      }))
      .sort((a, b) => {
        const typeDiff = (typeOrder[a.itemType] ?? 99) - (typeOrder[b.itemType] ?? 99)
        if (typeDiff !== 0) return typeDiff
        return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
      })
  }

  return items
    .map(item => {
      const matchCandidates = [
        { match: fuzzyMatch(trimmedQuery, item.name), text: item.name },
        { match: fuzzyMatch(trimmedQuery, item.relativePath), text: item.relativePath },
        { match: fuzzyMatch(trimmedQuery, item.extra), text: item.extra },
      ]

      const bestCandidate = matchCandidates.reduce((best, curr) =>
        curr.match.score > best.match.score ? curr : best,
      )

      let finalScore = bestCandidate.match.score
      if (finalScore > 0) {
        if (item.itemType === 'Attachment') {
          finalScore += 120
        } else if (item.itemType === 'Agent') {
          finalScore += 100
        } else if (item.itemType === 'Rule') {
          finalScore += 50
        }

        // Favor shallower path depths for files/directories
        if (item.relativePath) {
          const depth = item.relativePath.split(/[/\\]/).length
          finalScore += Math.max(0, 30 - depth * 5)
        }
      }

      return {
        ...item,
        matchScore: finalScore,
        matches: bestCandidate.match.matches,
        matchedText: bestCandidate.text,
      }
    })
    .filter(item => item.matchScore > 0)
    .sort((a, b) => {
      const scoreDiff = b.matchScore - a.matchScore
      if (Math.abs(scoreDiff) >= 0.5) return scoreDiff
      const typeDiff = (typeOrder[a.itemType] ?? 99) - (typeOrder[b.itemType] ?? 99)
      if (typeDiff !== 0) return typeDiff
      return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
    })
}

/**
 * Detect if cursor is currently placed right after an '@' mention query.
 */
export function detectMentionTrigger(
  text: string,
  cursorPosition: number,
): MentionTriggerInfo | null {
  if (cursorPosition <= 0 || cursorPosition > text.length) return null

  const beforeCursor = text.slice(0, cursorPosition)
  const lastAtIndex = beforeCursor.lastIndexOf('@')

  if (lastAtIndex === -1) return null

  // Ensure @ is at start of string or preceded by whitespace or common punctuation
  if (lastAtIndex > 0) {
    const charBeforeAt = beforeCursor[lastAtIndex - 1]
    const validPrecedingChars = [' ', '\t', '\n', '(', '[', '{', '"', "'", '`', ',', ';', ':']
    if (!validPrecedingChars.includes(charBeforeAt)) {
      return null
    }
  }

  // Check if there is whitespace or newline between @ and the cursor
  const afterAt = beforeCursor.slice(lastAtIndex + 1)
  if (/[\s\r\n]/.test(afterAt)) {
    return null
  }

  return {
    query: afterAt,
    mentionStart: lastAtIndex,
  }
}

/**
 * Splice the selected mention into the text and return the new text and cursor position.
 */
export function applyMentionInsertion(
  text: string,
  mentionStart: number,
  queryLength: number,
  insertText: string,
): { newText: string; newCursorPos: number } {
  const beforeMention = text.slice(0, mentionStart)
  let afterMention = text.slice(mentionStart + 1 + queryLength)

  // Avoid creating double spaces if insertText ends with space and afterMention starts with space
  if (insertText.endsWith(' ') && afterMention.startsWith(' ')) {
    afterMention = afterMention.slice(1)
  }

  const newText = `${beforeMention}${insertText}${afterMention}`
  const newCursorPos = mentionStart + insertText.length

  return {
    newText,
    newCursorPos,
  }
}

/**
 * Insert mention text at the cursor, replacing an active @ trigger when present.
 */
export function insertMentionText(
  text: string,
  cursorPos: number,
  insertText: string,
  activeTrigger: MentionTriggerInfo | null = null,
): { newText: string; newCursorPos: number } {
  if (activeTrigger) {
    return applyMentionInsertion(
      text,
      activeTrigger.mentionStart,
      activeTrigger.query.length,
      insertText,
    )
  }

  const before = text.slice(0, cursorPos)
  let after = text.slice(cursorPos)
  const prefix = before.length > 0 && !/\s$/.test(before) ? ' ' : ''
  if (insertText.endsWith(' ') && after.startsWith(' ')) {
    after = after.slice(1)
  }
  const inserted = `${prefix}${insertText}`
  return {
    newText: `${before}${inserted}${after}`,
    newCursorPos: cursorPos + inserted.length,
  }
}

/**
 * Build mention items for task attachments that live outside the regular file scan.
 */
export function mentionItemsFromAttachmentPaths(paths: string[]): MentionDisplayItem[] {
  return paths
    .map(path => path.trim())
    .filter(Boolean)
    .map(path => {
      const name = path.split(/[/\\]/).pop() ?? path
      return {
        name,
        extra: 'Attached file',
        itemType: 'Attachment' as const,
        relativePath: path,
        insertText: `@${path} `,
      }
    })
}

/**
 * Merge mention groups, keeping the first occurrence of each relative path.
 */
export function mergeMentionItems(groups: MentionDisplayItem[][]): MentionDisplayItem[] {
  const seen = new Set<string>()
  const merged: MentionDisplayItem[] = []
  for (const group of groups) {
    for (const item of group) {
      const key = item.relativePath || item.insertText
      if (seen.has(key)) continue
      seen.add(key)
      merged.push(item)
    }
  }
  return merged
}

const SKIP_DIRECTORIES = new Set([
  '.git',
  '.svn',
  '.hg',
  'node_modules',
  '__pycache__',
  'target',
  'dist',
  'build',
  '.cache',
  '.npm',
  '.yarn',
  '.next',
  '.turbo',
  '.nuxt',
  '.svelte-kit',
  '.venv',
  'venv',
  'env',
  '.idea',
  'coverage',
  'out',
  '.output',
  '.parcel-cache',
  '.gradle',
])

interface ScanBudget {
  operations: number
  results: number
}

interface ScanQueueItem {
  path: string
  depth: number
}

/**
 * Scan repository files and directories using Breadth-First Search (BFS).
 * BFS ensures top-level directories and root files are discovered first,
 * preventing any single sub-project from exhausting the scan budget.
 */
export async function scanProjectFiles(
  api: Pick<ReturnType<typeof createApi>, 'listFiles'> | ReturnType<typeof createApi>,
  projectSlug: string,
  options: {
    maxDepth?: number
    maxOperations?: number
    maxResults?: number
    maxEntriesPerDir?: number
  } = {},
): Promise<MentionDisplayItem[]> {
  const maxDepth = options.maxDepth ?? 4
  const budget: ScanBudget = {
    operations: options.maxOperations ?? 100,
    results: options.maxResults ?? 2000,
  }

  const results: MentionDisplayItem[] = []
  const queue: ScanQueueItem[] = [{ path: '', depth: 0 }]

  while (queue.length > 0 && budget.operations > 0 && budget.results > 0) {
    const current = queue.shift()!
    if (current.depth > maxDepth) continue

    budget.operations--
    let response: FileListResponse
    try {
      response = await api.listFiles(projectSlug, current.path)
    } catch {
      continue
    }

    const entries: ProjectFileEntry[] = response.entries || []
    let addedInDir = 0
    let enqueuedDirsInDir = 0
    const maxPerDir =
      options.maxEntriesPerDir ?? (current.depth === 0 ? 1000 : current.depth === 1 ? 100 : 50)

    for (const entry of entries) {
      if (budget.results <= 0) break

      const entryName = entry.name
      if (entry.kind === 'dir' && SKIP_DIRECTORIES.has(entryName)) {
        continue
      }
      if (entryName.startsWith('.') && entryName !== '.github' && entryName !== '.vscode') {
        continue
      }

      const relPath = entry.path.replace(/^\/+/, '')

      if (entry.kind === 'dir') {
        if (addedInDir < maxPerDir) {
          budget.results--
          addedInDir++
          results.push({
            name: entry.name,
            extra: relPath,
            itemType: 'Directory',
            relativePath: relPath,
            insertText: `@${relPath}/ `,
          })
        }

        if (current.depth < maxDepth && enqueuedDirsInDir < maxPerDir) {
          enqueuedDirsInDir++
          queue.push({ path: entry.path, depth: current.depth + 1 })
        }
      } else {
        if (addedInDir < maxPerDir) {
          budget.results--
          addedInDir++
          results.push({
            name: entry.name,
            extra: relPath,
            itemType: 'File',
            relativePath: relPath,
            insertText: `@${relPath} `,
          })
        }
      }
    }
  }

  return results
}

/**
 * Search project files dynamically from backend file search API.
 */
export async function searchProjectFiles(
  api: Pick<ReturnType<typeof createApi>, 'searchFiles'> | ReturnType<typeof createApi>,
  projectSlug: string,
  query: string,
  limit = 50,
): Promise<MentionDisplayItem[]> {
  const trimmed = query.trim()
  if (!trimmed) return []

  try {
    const response = await api.searchFiles(projectSlug, trimmed, limit)
    const entries: ProjectFileEntry[] = response.entries || []
    return entries.map(entry => {
      const relPath = entry.path.replace(/^\/+/, '')
      const isDir = entry.kind === 'dir'
      return {
        name: entry.name,
        extra: relPath,
        itemType: (isDir ? 'Directory' : 'File') as MentionItemType,
        relativePath: relPath,
        insertText: isDir ? `@${relPath}/ ` : `@${relPath} `,
      }
    })
  } catch (error) {
    console.debug('Failed to search project files:', error)
    return []
  }
}

/**
 * Fetch available agents and subrecipes for mention from OpenDuck ACP.
 */
export async function fetchAgentMentions(
  baseUrl: string,
  secretKey: string,
  cwd: string,
  sessionId?: string | null,
): Promise<MentionDisplayItem[]> {
  try {
    const client = await getSourcesClient(baseUrl, secretKey)
    const response = await client.goose.agentMentionsList_unstable({
      cwd: cwd.trim() || undefined,
      sessionId: sessionId?.trim() || undefined,
    })

    return (response.agents || []).map(agent => {
      const mention = agent.mention?.trim() || `@${agent.name}`
      const insertText = mention.endsWith(' ') ? mention : `${mention} `

      return {
        name: agent.name,
        description: agent.description,
        extra: agent.description || `Agent (${agent.sourceType || 'agent'})`,
        itemType: 'Agent',
        relativePath: agent.sourcePath || agent.name,
        insertText,
      }
    })
  } catch (error) {
    console.debug('Failed to fetch agent mentions:', error)
    return []
  }
}

/**
 * Fetch active rules for mention from OpenDuck sources API.
 */
export async function fetchRuleMentions(
  baseUrl: string,
  secretKey: string,
  cwd: string,
): Promise<MentionDisplayItem[]> {
  try {
    const sources = await listRuleSources(baseUrl, secretKey, cwd)
    return (sources || []).map(source => ({
      name: source.name,
      description: source.description,
      extra: source.description || source.path || 'Rule source',
      itemType: 'Rule',
      relativePath: source.path || source.name,
      insertText: `@${source.name} `,
    }))
  } catch (error) {
    console.debug('Failed to fetch rule mentions:', error)
    return []
  }
}
