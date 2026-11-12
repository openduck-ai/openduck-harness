import type {
  GitRepositoryActionRequest,
  GitRepositoryActionType,
  GitRepositoryEntry,
  GitStatusResponse,
} from '@aaif/goose-hub-core'

export type GitNotice =
  | { kind: 'commit'; sha: string; subject: string }
  | { kind: 'feedback'; message: string }

export function buildRepositoryActionRequest(
  path: string,
  action: GitRepositoryActionType,
  message?: string,
): GitRepositoryActionRequest {
  if (action === 'commit') {
    const trimmed = message?.trim()
    if (!trimmed) {
      throw new Error('Commit message is required')
    }
    return { path, action, message: trimmed }
  }
  return { path, action }
}

export function resolveActiveRepository(
  repositories: GitRepositoryEntry[],
  currentSelection: string,
): { activePath: string; isRootRepo: boolean; activeRepo: GitRepositoryEntry | null } {
  const exists = repositories.some(r => r.path === currentSelection)
  let activePath = currentSelection
  if (!exists) {
    if (repositories.some(r => r.path === '.')) {
      activePath = '.'
    } else if (repositories.length > 0) {
      activePath = repositories[0].path
    } else {
      activePath = '.'
    }
  }
  const activeRepo = repositories.find(r => r.path === activePath) ?? null
  const isRootRepo = activePath === '.'
  return { activePath, isRootRepo, activeRepo }
}

export function countRepositoryDirtyFiles(status: GitStatusResponse): number {
  return status.files.length
}

export function executeRowAction(
  rowPath: string,
  action: GitRepositoryActionType,
  options?: { message?: string },
): GitRepositoryActionRequest {
  return buildRepositoryActionRequest(rowPath, action, options?.message)
}

export function executeComposerCommit(
  selectedRepoPath: string,
  message: string,
): GitRepositoryActionRequest {
  return buildRepositoryActionRequest(selectedRepoPath, 'commit', message)
}

export function resolveActionTargetRepo(
  clickedRowPath: string | undefined,
  selectedRepoPath: string,
  context: 'row' | 'card' | 'composer',
): string {
  if (context === 'row' && clickedRowPath) {
    return clickedRowPath
  }
  return selectedRepoPath
}

export function formatRepoActionSuccess(
  action: GitRepositoryActionType,
  path: string,
  t: (key: string, options?: Record<string, unknown>) => string,
  details?: { subject?: string },
): string {
  if (action === 'pull') {
    return t('git.page.repoPullSuccess', { path })
  }
  if (action === 'push') {
    return t('git.page.repoPushSuccess', { path })
  }
  const subject = details?.subject?.trim()
  return t('git.page.repoCommitSuccess', {
    path,
    subject: subject ? ` — ${subject}` : '',
  })
}

export function renderGitNotice(
  notice: GitNotice,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  if (notice.kind === 'feedback') {
    return notice.message
  }
  if (notice.sha) {
    return t('git.page.committedSha', {
      sha: notice.sha,
      subject: notice.subject ? ` — ${notice.subject}` : '',
    })
  }
  if (notice.subject) {
    return t('git.page.committedSubject', { subject: notice.subject })
  }
  return t('git.page.committedSuccessfully')
}
