import type { ProjectFileEntry } from '@aaif/goose-hub-core'

/**
 * Project roots often live on mounts that do not deliver filesystem watch
 * events (for example WSL drvfs). The open Files tab re-reads the directory
 * it is showing instead of subscribing to a watcher.
 */
export const FILE_POLL_INTERVAL_MS = 2000

export function parentDirectory(filePath: string): string {
  const normalized = filePath.replace(/\\/g, '/').replace(/\/+$/g, '')
  const index = normalized.lastIndexOf('/')
  if (index <= 0) return ''
  return normalized.slice(0, index)
}

export function fileStampKey(entry: Pick<ProjectFileEntry, 'size' | 'modifiedAt'>): string {
  return `${entry.size ?? ''}|${entry.modifiedAt ?? ''}`
}

export function fileListSignature(entries: ProjectFileEntry[]): string {
  return entries
    .map(entry => `${entry.kind}\0${entry.path}\0${entry.size ?? ''}\0${entry.modifiedAt ?? ''}`)
    .sort()
    .join('\n')
}

export function listingChanged(
  previous: ProjectFileEntry[],
  next: ProjectFileEntry[],
): boolean {
  return fileListSignature(previous) !== fileListSignature(next)
}

/** Directories whose listings reveal changes to the visible folder and the open file. */
export function directoriesToWatch(
  currentDirectory: string,
  selectedPath: string | null,
): string[] {
  if (!selectedPath) return [currentDirectory]
  const parent = parentDirectory(selectedPath)
  if (parent === currentDirectory) return [currentDirectory]
  return [currentDirectory, parent]
}

export type OpenFileRefresh =
  | { action: 'unchanged' }
  | { action: 'reload'; stamp: string }
  | { action: 'conflict'; stamp: string }
  | { action: 'missing' }

/**
 * Decide what to do with the open file after a directory listing arrives.
 * A file absent from the previous snapshot is a baseline, not a change, so
 * the first observation does not reload content the editor already has.
 */
export function openFileRefreshAction(input: {
  directoryPath: string
  selectedPath: string | null
  previousEntries: ProjectFileEntry[]
  nextEntries: ProjectFileEntry[]
  dirty: boolean
  dismissedStamp?: string | null
  /** The open file was already reported missing, so a later appearance is a real change. */
  wasMissing?: boolean
}): OpenFileRefresh {
  const {
    directoryPath,
    selectedPath,
    previousEntries,
    nextEntries,
    dirty,
    dismissedStamp,
    wasMissing,
  } = input
  if (!selectedPath || parentDirectory(selectedPath) !== directoryPath) {
    return { action: 'unchanged' }
  }

  const next = nextEntries.find(entry => entry.path === selectedPath && entry.kind === 'file')
  if (!next) return { action: 'missing' }

  const previous = previousEntries.find(
    entry => entry.path === selectedPath && entry.kind === 'file',
  )
  const stamp = fileStampKey(next)
  if (!previous) {
    if (!wasMissing) return { action: 'unchanged' }
    if (dismissedStamp && dismissedStamp === stamp) return { action: 'unchanged' }
    if (dirty) return { action: 'conflict', stamp }
    return { action: 'reload', stamp }
  }

  if (fileStampKey(previous) === stamp) return { action: 'unchanged' }
  if (dismissedStamp && dismissedStamp === stamp) return { action: 'unchanged' }
  if (dirty) return { action: 'conflict', stamp }
  return { action: 'reload', stamp }
}

/** Remember the open file's directory listing when the visible folder changes. */
export function baselineWhenLeaving(input: {
  selectedPath: string | null
  fromDirectory: string
  toDirectory: string
  fromEntries: ProjectFileEntry[]
}): { directory: string; entries: ProjectFileEntry[] } | null {
  if (!input.selectedPath || input.fromDirectory === input.toDirectory) return null
  if (parentDirectory(input.selectedPath) !== input.fromDirectory) return null
  return { directory: input.fromDirectory, entries: input.fromEntries }
}

export function withCacheBuster(url: string, revision: number): string {
  if (!revision) return url
  const join = url.includes('?') ? '&' : '?'
  return `${url}${join}v=${revision}`
}
