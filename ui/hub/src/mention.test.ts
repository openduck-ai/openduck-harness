import assert from 'node:assert/strict'
import test from 'node:test'
import {
  fuzzyMatch,
  detectMentionTrigger,
  applyMentionInsertion,
  insertMentionText,
  filterAndRankMentionItems,
  shouldIgnoreMentionTrigger,
  debounce,
  mentionItemsFromAttachmentPaths,
  mergeMentionItems,
  scanProjectFiles,
  searchProjectFiles,
  type MentionDisplayItem,
  type DismissedMention,
} from './mention.ts'

test('fuzzyMatch returns 0 score and empty matches for empty pattern', () => {
  const res = fuzzyMatch('', 'src/index.ts')
  assert.equal(res.score, 0)
  assert.deepEqual(res.matches, [])
})

test('fuzzyMatch matches exact substring and calculates score with matches', () => {
  const res = fuzzyMatch('chat', 'ProjectChat.tsx')
  assert.ok(res.score > 0)
  assert.equal(res.matches.length, 4)
  assert.deepEqual(res.matches, [7, 8, 9, 10])
})

test('fuzzyMatch matches fuzzy characters separated in string', () => {
  const res = fuzzyMatch('pjchat', 'ProjectChat.tsx')
  assert.ok(res.score > 0)
  assert.equal(res.matches.length, 6)
})

test('fuzzyMatch returns -1 for non-matching pattern', () => {
  const res = fuzzyMatch('xyz', 'ProjectChat.tsx')
  assert.equal(res.score, -1)
  assert.deepEqual(res.matches, [])
})

test('fuzzyMatch gives higher score to word boundary and start of filename matches', () => {
  const scoreA = fuzzyMatch('main', 'crates/openduck/src/main.rs').score
  const scoreB = fuzzyMatch('main', 'crates/openduck/src/domain.rs').score
  assert.ok(scoreA > scoreB)
})

test('detectMentionTrigger detects @ at start of input', () => {
  const res = detectMentionTrigger('@dev', 4)
  assert.deepEqual(res, { query: 'dev', mentionStart: 0 })
})

test('detectMentionTrigger detects @ immediately when typed', () => {
  const res = detectMentionTrigger('@', 1)
  assert.deepEqual(res, { query: '', mentionStart: 0 })
})

test('detectMentionTrigger detects @ after space or punctuation in sentence', () => {
  const res1 = detectMentionTrigger('Please ask @developer to help', 21)
  assert.deepEqual(res1, { query: 'developer', mentionStart: 11 })

  const res2 = detectMentionTrigger('Check file (@src/main.rs', 24)
  assert.deepEqual(res2, { query: 'src/main.rs', mentionStart: 12 })
})

test('detectMentionTrigger ignores email addresses or @ inside words', () => {
  const res = detectMentionTrigger('user@example.com', 16)
  assert.equal(res, null)
})

test('detectMentionTrigger returns null if space exists between @ and cursor', () => {
  const res = detectMentionTrigger('Hello @dev and more', 19)
  assert.equal(res, null)
})

test('detectMentionTrigger returns null if cursor is before @', () => {
  const res = detectMentionTrigger('Hello @developer', 5)
  assert.equal(res, null)
})

test('applyMentionInsertion replaces mention at start of string', () => {
  const result = applyMentionInsertion('@dev', 0, 3, '@developer ')
  assert.equal(result.newText, '@developer ')
  assert.equal(result.newCursorPos, 11)
})

test('applyMentionInsertion replaces mention in middle of sentence preserving surrounding text', () => {
  const input = 'Please check with @code regarding the fix.'
  const result = applyMentionInsertion(input, 18, 4, '@developer ')
  assert.equal(result.newText, 'Please check with @developer regarding the fix.')
  assert.equal(result.newCursorPos, 29)
})

test('applyMentionInsertion replaces file mention with path', () => {
  const input = 'Read @main'
  const result = applyMentionInsertion(input, 5, 4, '@src/main.rs ')
  assert.equal(result.newText, 'Read @src/main.rs ')
  assert.equal(result.newCursorPos, 18)
})

const sampleItems: MentionDisplayItem[] = [
  {
    name: 'spec.pdf',
    extra: 'Attached file',
    itemType: 'Attachment',
    relativePath: '.goose/task-attachments/task-1/spec.pdf',
    insertText: '@.goose/task-attachments/task-1/spec.pdf ',
  },
  {
    name: 'developer',
    extra: 'Software engineer agent',
    itemType: 'Agent',
    relativePath: 'developer',
    insertText: '@developer ',
  },
  {
    name: 'ProjectChat.tsx',
    extra: 'src/components/ProjectChat.tsx',
    itemType: 'File',
    relativePath: 'src/components/ProjectChat.tsx',
    insertText: '@src/components/ProjectChat.tsx ',
  },
  {
    name: 'main.rs',
    extra: 'crates/openduck/src/main.rs',
    itemType: 'File',
    relativePath: 'crates/openduck/src/main.rs',
    insertText: '@crates/openduck/src/main.rs ',
  },
  {
    name: 'code-style',
    extra: 'Code style guidelines',
    itemType: 'Rule',
    relativePath: '.rules/code-style.md',
    insertText: '@code-style ',
  },
]

test('filterAndRankMentionItems returns all items sorted by type when query is empty', () => {
  const results = filterAndRankMentionItems(sampleItems, '')
  assert.equal(results.length, 5)
  assert.equal(results[0].itemType, 'Attachment')
  assert.equal(results[1].itemType, 'Agent')
  assert.equal(results[2].itemType, 'Rule')
})

test('filterAndRankMentionItems ranks attached files above repo files for the same query', () => {
  const results = filterAndRankMentionItems(sampleItems, 'spec')
  assert.ok(results.length >= 1)
  assert.equal(results[0].itemType, 'Attachment')
  assert.equal(results[0].name, 'spec.pdf')
})

test('mentionItemsFromAttachmentPaths labels workspace uploads as attached files', () => {
  const items = mentionItemsFromAttachmentPaths([
    '.goose/task-attachments/task-7/numbers.xlsx',
    '  ',
  ])
  assert.equal(items.length, 1)
  assert.equal(items[0].itemType, 'Attachment')
  assert.equal(items[0].name, 'numbers.xlsx')
  assert.equal(items[0].insertText, '@.goose/task-attachments/task-7/numbers.xlsx ')
})

test('mergeMentionItems keeps attachments first and drops duplicate paths', () => {
  const attached = mentionItemsFromAttachmentPaths([
    '.goose/task-attachments/task-1/spec.pdf',
  ])
  const duplicateFile: MentionDisplayItem = {
    name: 'spec.pdf',
    extra: '.goose/task-attachments/task-1/spec.pdf',
    itemType: 'File',
    relativePath: '.goose/task-attachments/task-1/spec.pdf',
    insertText: '@.goose/task-attachments/task-1/spec.pdf ',
  }
  const merged = mergeMentionItems([attached, [duplicateFile, sampleItems[1]]])
  assert.equal(merged.length, 2)
  assert.equal(merged[0].itemType, 'Attachment')
  assert.equal(merged[1].itemType, 'Agent')
})

test('insertMentionText replaces an active @ query with the attached file path', () => {
  const result = insertMentionText('Please read @sp', 15, '@.goose/task-attachments/task-1/spec.pdf ', {
    query: 'sp',
    mentionStart: 12,
  })
  assert.equal(result.newText, 'Please read @.goose/task-attachments/task-1/spec.pdf ')
})

test('insertMentionText inserts at the cursor when no mention is active', () => {
  const result = insertMentionText('Please read', 11, '@.goose/task-attachments/task-1/spec.pdf ')
  assert.equal(result.newText, 'Please read @.goose/task-attachments/task-1/spec.pdf ')
  assert.equal(result.newCursorPos, 53)
})

test('filterAndRankMentionItems filters and ranks items matching query', () => {
  const results = filterAndRankMentionItems(sampleItems, 'chat')
  assert.equal(results.length, 1)
  assert.equal(results[0].name, 'ProjectChat.tsx')
  assert.ok(results[0].matchScore > 0)
  assert.ok(results[0].matches.length > 0)
})

test('filterAndRankMentionItems prioritizes Agent mentions over file matches with equal prefix', () => {
  const items: MentionDisplayItem[] = [
    {
      name: 'code',
      extra: 'crates/code.rs',
      itemType: 'File',
      relativePath: 'crates/code.rs',
      insertText: '@crates/code.rs ',
    },
    {
      name: 'code',
      extra: 'Code agent',
      itemType: 'Agent',
      relativePath: 'code',
      insertText: '@code ',
    },
  ]

  const results = filterAndRankMentionItems(items, 'code')
  assert.equal(results.length, 2)
  assert.equal(results[0].itemType, 'Agent')
})

test('shouldIgnoreMentionTrigger returns true when trigger matches dismissed mention', () => {
  const trigger = { query: 'dev', mentionStart: 5 }
  const dismissed: DismissedMention = { query: 'dev', mentionStart: 5 }
  assert.equal(shouldIgnoreMentionTrigger(trigger, dismissed), true)
})

test('shouldIgnoreMentionTrigger returns false when query or start position differs', () => {
  const trigger = { query: 'developer', mentionStart: 5 }
  const dismissed: DismissedMention = { query: 'dev', mentionStart: 5 }
  assert.equal(shouldIgnoreMentionTrigger(trigger, dismissed), false)

  const trigger2 = { query: 'dev', mentionStart: 10 }
  assert.equal(shouldIgnoreMentionTrigger(trigger2, dismissed), false)
})

test('shouldIgnoreMentionTrigger returns false when trigger or dismissed is null', () => {
  const trigger = { query: 'dev', mentionStart: 5 }
  const dismissed: DismissedMention = { query: 'dev', mentionStart: 5 }
  assert.equal(shouldIgnoreMentionTrigger(null, dismissed), false)
  assert.equal(shouldIgnoreMentionTrigger(trigger, null), false)
  assert.equal(shouldIgnoreMentionTrigger(null, null), false)
})

test('debounce delays execution and cancels properly', async () => {
  let callCount = 0
  let lastArg = ''

  const debounced = debounce((arg: string) => {
    callCount++
    lastArg = arg
  }, 20)

  debounced('first')
  debounced('second')
  assert.equal(callCount, 0)

  await new Promise(resolve => setTimeout(resolve, 35))
  assert.equal(callCount, 1)
  assert.equal(lastArg, 'second')

  // Test cancellation
  debounced('third')
  debounced.cancel()
  await new Promise(resolve => setTimeout(resolve, 35))
  assert.equal(callCount, 1)
  assert.equal(lastArg, 'second')
})

test('scanProjectFiles uses BFS so peer directories and root files are discovered', async () => {
  const mockApi = {
    listFiles: async (_slug: string, path = '') => {
      if (path === '') {
        return {
          currentPath: '',
          parentPath: null,
          entries: [
            { name: 'abs_admin_vue', path: 'abs_admin_vue', kind: 'dir' as const },
            { name: 'broker', path: 'broker', kind: 'dir' as const },
            { name: 'go.mod', path: 'go.mod', kind: 'file' as const },
            { name: 'README.md', path: 'README.md', kind: 'file' as const },
          ],
        }
      }
      if (path === 'abs_admin_vue') {
        // Many files in the first subdirectory
        return {
          currentPath: 'abs_admin_vue',
          parentPath: '',
          entries: Array.from({ length: 40 }, (_, i) => ({
            name: `component_${i}.vue`,
            path: `abs_admin_vue/component_${i}.vue`,
            kind: 'file' as const,
          })),
        }
      }
      if (path === 'broker') {
        return {
          currentPath: 'broker',
          parentPath: '',
          entries: [
            { name: 'server.go', path: 'broker/server.go', kind: 'file' as const },
            { name: 'session.go', path: 'broker/session.go', kind: 'file' as const },
          ],
        }
      }
      return { currentPath: path, parentPath: '', entries: [] }
    },
  }

  const results = await scanProjectFiles(mockApi, 'test-project', {
    maxDepth: 2,
    maxResults: 20, // Strict budget lower than total entries (4 + 40 + 2 = 46)
    maxEntriesPerDir: 10,
  })

  const names = results.map(r => r.name)
  // Top-level files and directories must be found first
  assert.ok(names.includes('abs_admin_vue'))
  assert.ok(names.includes('broker'))
  assert.ok(names.includes('go.mod'))
  assert.ok(names.includes('README.md'))

  // Next level items from broker must also be reached
  assert.ok(names.includes('server.go'))
})

test('searchProjectFiles maps backend search results to mention items', async () => {
  const mockApi = {
    searchFiles: async (_slug: string, query: string, _limit?: number) => {
      assert.equal(query, 'claude-kyc')
      return {
        query,
        entries: [
          {
            name: '2026-04-16-claude-kyc-script.md',
            path: '2026-04-16-claude-kyc-script.md',
            kind: 'file' as const,
          },
          {
            name: '2026-04-17-claude-kyc-v2-script.md',
            path: '2026-04-17-claude-kyc-v2-script.md',
            kind: 'file' as const,
          },
        ],
      }
    },
  }

  const results = await searchProjectFiles(mockApi, 'test-project', 'claude-kyc')
  assert.equal(results.length, 2)
  assert.equal(results[0].name, '2026-04-16-claude-kyc-script.md')
  assert.equal(results[0].itemType, 'File')
  assert.equal(results[0].insertText, '@2026-04-16-claude-kyc-script.md ')
  assert.equal(results[1].name, '2026-04-17-claude-kyc-v2-script.md')
})

