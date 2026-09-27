import assert from 'node:assert/strict'
import test from 'node:test'
import type { ProjectFileEntry } from '@aaif/goose-hub-core'
import {
  baselineWhenLeaving,
  directoriesToWatch,
  fileListSignature,
  fileStampKey,
  listingChanged,
  openFileRefreshAction,
  parentDirectory,
  withCacheBuster,
} from './fileRefresh.ts'

function entry(partial: Partial<ProjectFileEntry> & Pick<ProjectFileEntry, 'name' | 'path'>): ProjectFileEntry {
  return {
    kind: 'file',
    ...partial,
  }
}

const readme = entry({
  name: 'README.md',
  path: 'README.md',
  size: 12,
  modifiedAt: '2026-09-26T00:00:00Z',
})

const mainRs = entry({
  name: 'main.rs',
  path: 'src/main.rs',
  size: 40,
  modifiedAt: '2026-09-26T00:00:00Z',
})

test('parentDirectory returns the relative parent', () => {
  assert.equal(parentDirectory('README.md'), '')
  assert.equal(parentDirectory('src/main.rs'), 'src')
  assert.equal(parentDirectory('src/a/b.ts'), 'src/a')
  assert.equal(parentDirectory('src/a/'), 'src')
})

test('fileListSignature ignores entry order and detects listing edits', () => {
  const folder = entry({ name: 'src', path: 'src', kind: 'dir', modifiedAt: '2026-09-26T00:00:00Z' })
  const flipped = [folder, readme]
  assert.equal(fileListSignature([readme, folder]), fileListSignature(flipped))
  assert.equal(listingChanged([readme], [readme]), false)
  assert.equal(
    listingChanged([readme], [{ ...readme, size: 99, modifiedAt: '2026-09-26T01:00:00Z' }]),
    true,
  )
  assert.equal(listingChanged([readme], [readme, mainRs]), true)
  assert.equal(listingChanged([readme, mainRs], [mainRs]), true)
})

test('directoriesToWatch includes the open file parent when it is not the visible directory', () => {
  assert.deepEqual(directoriesToWatch('src', 'src/main.rs'), ['src'])
  assert.deepEqual(directoriesToWatch('', 'src/main.rs'), ['', 'src'])
  assert.deepEqual(directoriesToWatch('docs', null), ['docs'])
})

test('openFileRefreshAction reloads a clean file when its stamp changes', () => {
  const updated = { ...readme, size: 20, modifiedAt: '2026-09-26T02:00:00Z' }
  assert.deepEqual(
    openFileRefreshAction({
      directoryPath: '',
      selectedPath: 'README.md',
      previousEntries: [readme],
      nextEntries: [updated],
      dirty: false,
    }),
    { action: 'reload', stamp: fileStampKey(updated) },
  )
})

test('openFileRefreshAction keeps dirty editor content and reports a conflict', () => {
  const updated = { ...readme, modifiedAt: '2026-09-26T02:00:00Z' }
  const stamp = fileStampKey(updated)
  assert.deepEqual(
    openFileRefreshAction({
      directoryPath: '',
      selectedPath: 'README.md',
      previousEntries: [readme],
      nextEntries: [updated],
      dirty: true,
    }),
    { action: 'conflict', stamp },
  )
  assert.deepEqual(
    openFileRefreshAction({
      directoryPath: '',
      selectedPath: 'README.md',
      previousEntries: [readme],
      nextEntries: [updated],
      dirty: true,
      dismissedStamp: stamp,
    }),
    { action: 'unchanged' },
  )
})

test('openFileRefreshAction ignores another directory and the first observation of a file', () => {
  assert.deepEqual(
    openFileRefreshAction({
      directoryPath: 'docs',
      selectedPath: 'src/main.rs',
      previousEntries: [],
      nextEntries: [],
      dirty: false,
    }),
    { action: 'unchanged' },
  )
  assert.deepEqual(
    openFileRefreshAction({
      directoryPath: 'src',
      selectedPath: 'src/main.rs',
      previousEntries: [],
      nextEntries: [mainRs],
      dirty: false,
    }),
    { action: 'unchanged' },
  )
})

test('openFileRefreshAction reloads a file that reappears after it was removed', () => {
  assert.deepEqual(
    openFileRefreshAction({
      directoryPath: 'src',
      selectedPath: 'src/main.rs',
      previousEntries: [],
      nextEntries: [mainRs],
      dirty: false,
      wasMissing: true,
    }),
    { action: 'reload', stamp: fileStampKey(mainRs) },
  )
})

test('baselineWhenLeaving keeps the open file directory when the visible folder changes', () => {
  assert.deepEqual(
    baselineWhenLeaving({
      selectedPath: 'src/main.rs',
      fromDirectory: 'src',
      toDirectory: 'src/components',
      fromEntries: [mainRs],
    }),
    { directory: 'src', entries: [mainRs] },
  )
  assert.equal(
    baselineWhenLeaving({
      selectedPath: 'src/main.rs',
      fromDirectory: 'src',
      toDirectory: 'src',
      fromEntries: [mainRs],
    }),
    null,
  )
  assert.equal(
    baselineWhenLeaving({
      selectedPath: 'README.md',
      fromDirectory: 'src',
      toDirectory: '',
      fromEntries: [mainRs],
    }),
    null,
  )
})

test('openFileRefreshAction reports a file removed from its directory', () => {
  assert.deepEqual(
    openFileRefreshAction({
      directoryPath: 'src',
      selectedPath: 'src/main.rs',
      previousEntries: [mainRs],
      nextEntries: [],
      dirty: true,
    }),
    { action: 'missing' },
  )
})

test('withCacheBuster leaves the original url until a revision is set', () => {
  assert.equal(withCacheBuster('/hub/file?path=a.png', 0), '/hub/file?path=a.png')
  assert.equal(withCacheBuster('/hub/file?path=a.png', 2), '/hub/file?path=a.png&v=2')
  assert.equal(withCacheBuster('/hub/file', 1), '/hub/file?v=1')
})
