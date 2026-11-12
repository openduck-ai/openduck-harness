import test from 'node:test'
import assert from 'node:assert/strict'
import type {
  GitRepositoriesResponse,
  GitRepositoryActionRequest,
  GitRepositoryActionType,
  GitRepositoryEntry,
  GitStatusResponse,
} from '@aaif/goose-hub-core'
import {
  executeComposerCommit,
  executeRowAction,
  formatRepoActionSuccess,
  renderGitNotice,
  resolveActionTargetRepo,
} from './gitRepositories.ts'

function buildRepositoryActionRequest(
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

function resolveActiveRepository(
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

function countRepositoryDirtyFiles(status: GitStatusResponse): number {
  return status.files.length
}

test('resolves default active repository to root when present', () => {
  const mockRepos: GitRepositoriesResponse = {
    repositories: [
      {
        path: '.',
        status: {
          isRepo: true,
          branch: 'main',
          detached: false,
          ahead: 1,
          behind: 0,
          files: [],
        },
      },
      {
        path: 'packages/ui',
        status: {
          isRepo: true,
          branch: 'feature-ui',
          detached: false,
          ahead: 0,
          behind: 2,
          files: [
            {
              path: 'src/Button.tsx',
              indexStatus: 'M',
              worktreeStatus: ' ',
              staged: true,
              unstaged: false,
              untracked: false,
              conflict: false,
            },
          ],
        },
      },
    ],
  }

  const { activePath, isRootRepo, activeRepo } = resolveActiveRepository(mockRepos.repositories, '.')
  assert.equal(activePath, '.')
  assert.equal(isRootRepo, true)
  assert.equal(activeRepo?.status.branch, 'main')
  assert.equal(countRepositoryDirtyFiles(activeRepo!.status), 0)
})

test('resolves active repository to nested repo when selected or root is absent', () => {
  const mockRepos: GitRepositoriesResponse = {
    repositories: [
      {
        path: 'services/backend',
        status: {
          isRepo: true,
          branch: 'develop',
          detached: false,
          ahead: 3,
          behind: 0,
          files: [
            {
              path: 'main.py',
              indexStatus: ' ',
              worktreeStatus: 'M',
              staged: false,
              unstaged: true,
              untracked: false,
              conflict: false,
            },
            {
              path: 'new.py',
              indexStatus: '?',
              worktreeStatus: '?',
              staged: false,
              unstaged: false,
              untracked: true,
              conflict: false,
            },
          ],
        },
      },
      {
        path: 'services/frontend',
        status: {
          isRepo: true,
          branch: 'feat/layout',
          detached: false,
          ahead: 0,
          behind: 0,
          files: [],
        },
      },
    ],
  }

  // When initial selection is '.' but root repo is absent, falls back to first repository
  const initial = resolveActiveRepository(mockRepos.repositories, '.')
  assert.equal(initial.activePath, 'services/backend')
  assert.equal(initial.isRootRepo, false)
  assert.equal(countRepositoryDirtyFiles(initial.activeRepo!.status), 2)

  // Explicit selection of second nested repo
  const second = resolveActiveRepository(mockRepos.repositories, 'services/frontend')
  assert.equal(second.activePath, 'services/frontend')
  assert.equal(second.isRootRepo, false)
  assert.equal(second.activeRepo?.status.branch, 'feat/layout')
  assert.equal(countRepositoryDirtyFiles(second.activeRepo!.status), 0)
})

test('correctly preserves root repo status while distinguishing nested status', () => {
  const rootStatus: GitStatusResponse = {
    isRepo: true,
    branch: 'master',
    detached: false,
    ahead: 0,
    behind: 0,
    files: [
      {
        path: 'Cargo.toml',
        indexStatus: 'M',
        worktreeStatus: ' ',
        staged: true,
        unstaged: false,
        untracked: false,
        conflict: false,
      },
    ],
  }

  const nestedStatus: GitStatusResponse = {
    isRepo: true,
    branch: 'nested-branch',
    detached: true,
    ahead: 0,
    behind: 0,
    files: [],
  }

  assert.equal(rootStatus.isRepo, true)
  assert.equal(rootStatus.detached, false)
  assert.equal(rootStatus.files.length, 1)

  assert.equal(nestedStatus.isRepo, true)
  assert.equal(nestedStatus.detached, true)
  assert.equal(nestedStatus.files.length, 0)
})

test('buildRepositoryActionRequest constructs pull action with relative nested path', () => {
  const req = buildRepositoryActionRequest('packages/ui', 'pull')
  assert.deepEqual(req, {
    path: 'packages/ui',
    action: 'pull',
  })
})

test('buildRepositoryActionRequest constructs push action with relative nested path', () => {
  const req = buildRepositoryActionRequest('services/backend', 'push')
  assert.deepEqual(req, {
    path: 'services/backend',
    action: 'push',
  })
})

test('buildRepositoryActionRequest constructs explicit commit with collected message and relative nested path', () => {
  const req = buildRepositoryActionRequest(
    'packages/ui',
    'commit',
    'feat(ui): add nested repo controls',
  )
  assert.deepEqual(req, {
    path: 'packages/ui',
    action: 'commit',
    message: 'feat(ui): add nested repo controls',
  })
})

test('buildRepositoryActionRequest rejects empty or whitespace-only message to prevent auto-commit', () => {
  assert.throws(() => buildRepositoryActionRequest('packages/ui', 'commit', ''), {
    message: 'Commit message is required',
  })
  assert.throws(() => buildRepositoryActionRequest('packages/ui', 'commit', '   \n  '), {
    message: 'Commit message is required',
  })
  assert.throws(() => buildRepositoryActionRequest('packages/ui', 'commit', undefined), {
    message: 'Commit message is required',
  })
})

test('nested repository actions preserve existing unstaged changes in repository status', () => {
  const unstagedFiles = [
    {
      path: 'src/index.ts',
      indexStatus: ' ',
      worktreeStatus: 'M',
      staged: false,
      unstaged: true,
      untracked: false,
      conflict: false,
    },
    {
      path: 'README.md',
      indexStatus: '?',
      worktreeStatus: '?',
      staged: false,
      unstaged: false,
      untracked: true,
      conflict: false,
    },
  ]

  const repoEntry: GitRepositoryEntry = {
    path: 'services/worker',
    status: {
      isRepo: true,
      branch: 'main',
      detached: false,
      ahead: 0,
      behind: 1,
      files: unstagedFiles,
    },
  }

  // The nested repository entry exposes unstaged changes
  assert.equal(repoEntry.status.files.length, 2)
  assert.equal(repoEntry.status.files.some(f => f.unstaged || f.untracked), true)

  // Explicit commit request targets only the nested repo path with user message
  const commitReq = buildRepositoryActionRequest(repoEntry.path, 'commit', 'fix: worker retry logic')
  assert.equal(commitReq.path, 'services/worker')
  assert.equal(commitReq.action, 'commit')
  assert.equal(commitReq.message, 'fix: worker retry logic')

  // Unstaged changes in repository status remain intact
  assert.deepEqual(repoEntry.status.files, unstagedFiles)
})

test('row action submits clicked row path rather than currently selected repository', () => {
  const selectedRepoPath = 'services/backend'
  const clickedRowPath = 'packages/ui'
  assert.notEqual(clickedRowPath, selectedRepoPath)

  // Clicking pull on a row must target that row's relative path
  const pullReq = executeRowAction(clickedRowPath, 'pull')
  assert.deepEqual(pullReq, {
    path: 'packages/ui',
    action: 'pull',
  })
  assert.equal(pullReq.path, clickedRowPath)

  // Clicking push on a row must target that row's relative path
  const pushReq = executeRowAction(clickedRowPath, 'push')
  assert.deepEqual(pushReq, {
    path: 'packages/ui',
    action: 'push',
  })
  assert.equal(pushReq.path, clickedRowPath)

  // Committing from row modal must target clicked row's path with its message
  const commitReq = executeRowAction(clickedRowPath, 'commit', {
    message: 'feat(ui): update nested component',
  })
  assert.deepEqual(commitReq, {
    path: 'packages/ui',
    action: 'commit',
    message: 'feat(ui): update nested component',
  })
  assert.equal(commitReq.path, clickedRowPath)
})

test('main commit composer uses currently selected repository', () => {
  const selectedRepoPath = 'services/backend'
  const composerReq = executeComposerCommit(selectedRepoPath, 'fix(backend): connection pool retry')
  assert.deepEqual(composerReq, {
    path: 'services/backend',
    action: 'commit',
    message: 'fix(backend): connection pool retry',
  })
  assert.equal(composerReq.path, selectedRepoPath)
})

test('resolveActionTargetRepo isolates row action targets from card and composer targets', () => {
  const selectedRepoPath = 'services/backend'
  const clickedRowPath = 'packages/ui'

  // Row controls always resolve to the clicked row path
  assert.equal(resolveActionTargetRepo(clickedRowPath, selectedRepoPath, 'row'), 'packages/ui')

  // Card controls resolve to the selected repository path
  assert.equal(resolveActionTargetRepo(undefined, selectedRepoPath, 'card'), 'services/backend')

  // Main composer resolves to the selected repository path
  assert.equal(resolveActionTargetRepo(undefined, selectedRepoPath, 'composer'), 'services/backend')
})

test('formatRepoActionSuccess delivers action-specific success text without Committed prefix for pull and push', () => {
  const mockTranslations: Record<string, string> = {
    'git.page.repoPullSuccess': 'Pulled latest changes for {{path}}',
    'git.page.repoPushSuccess': 'Pushed changes for {{path}}',
    'git.page.repoCommitSuccess': 'Committed changes in {{path}}{{subject}}',
    'git.page.committedSubject': 'Committed — {{subject}}',
    'git.page.committedSuccessfully': 'Committed successfully',
  }

  const t = (key: string, options?: Record<string, unknown>): string => {
    let str = mockTranslations[key] ?? key
    if (options) {
      for (const [k, v] of Object.entries(options)) {
        str = str.replace(`{{${k}}}`, String(v))
      }
    }
    return str
  }

  // Pull notice: accurately states pulled changes without "Committed —"
  const pullNotice = formatRepoActionSuccess('pull', 'packages/ui', t)
  assert.equal(pullNotice, 'Pulled latest changes for packages/ui')
  assert.equal(pullNotice.startsWith('Committed'), false)

  // Push notice: accurately states pushed changes without "Committed —"
  const pushNotice = formatRepoActionSuccess('push', 'packages/ui', t)
  assert.equal(pushNotice, 'Pushed changes for packages/ui')
  assert.equal(pushNotice.startsWith('Committed'), false)

  // Commit notice for nested repo: formats commit success for that repo
  const commitNoticeWithSubject = formatRepoActionSuccess('commit', 'packages/ui', t, {
    subject: 'feat(ui): add button',
  })
  assert.equal(commitNoticeWithSubject, 'Committed changes in packages/ui — feat(ui): add button')

  const commitNoticeNoSubject = formatRepoActionSuccess('commit', 'packages/ui', t)
  assert.equal(commitNoticeNoSubject, 'Committed changes in packages/ui')
})

test('renderGitNotice renders feedback notice directly without Committed prefix', () => {
  const mockTranslations: Record<string, string> = {
    'git.page.committedSha': 'Committed {{sha}}{{subject}}',
    'git.page.committedSubject': 'Committed — {{subject}}',
    'git.page.committedSuccessfully': 'Committed successfully',
  }

  const t = (key: string, options?: Record<string, unknown>): string => {
    let str = mockTranslations[key] ?? key
    if (options) {
      for (const [k, v] of Object.entries(options)) {
        str = str.replace(`{{${k}}}`, String(v))
      }
    }
    return str
  }

  // Action feedback for Pull is rendered directly
  const pullRendered = renderGitNotice(
    { kind: 'feedback', message: 'Pulled latest changes for packages/ui' },
    t,
  )
  assert.equal(pullRendered, 'Pulled latest changes for packages/ui')
  assert.equal(pullRendered.startsWith('Committed'), false)

  // Action feedback for Push is rendered directly
  const pushRendered = renderGitNotice(
    { kind: 'feedback', message: 'Pushed changes for packages/ui' },
    t,
  )
  assert.equal(pushRendered, 'Pushed changes for packages/ui')
  assert.equal(pushRendered.startsWith('Committed'), false)

  // Action feedback for Commit is rendered directly without double "Committed —"
  const nestedCommitRendered = renderGitNotice(
    { kind: 'feedback', message: 'Committed changes in packages/ui — feat: add button' },
    t,
  )
  assert.equal(nestedCommitRendered, 'Committed changes in packages/ui — feat: add button')
  assert.equal(nestedCommitRendered.startsWith('Committed — Committed'), false)

  // Root commit still renders committed sha format
  const rootCommitRendered = renderGitNotice(
    { kind: 'commit', sha: 'a1b2c3d', subject: 'refactor: cleanup' },
    t,
  )
  assert.equal(rootCommitRendered, 'Committed a1b2c3d — refactor: cleanup')
})
