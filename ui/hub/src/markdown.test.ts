import assert from 'node:assert/strict'
import test from 'node:test'
import { isMarkdownPath } from './markdown.ts'

test('isMarkdownPath detects common markdown extensions', () => {
  assert.equal(isMarkdownPath('README.md'), true)
  assert.equal(isMarkdownPath('docs/guide.markdown'), true)
  assert.equal(isMarkdownPath('notes.MDX'), true)
  assert.equal(isMarkdownPath('changelog.mdown'), true)
  assert.equal(isMarkdownPath('nested/path/intro.Md'), true)
})

test('isMarkdownPath rejects non-markdown files', () => {
  assert.equal(isMarkdownPath('src/main.rs'), false)
  assert.equal(isMarkdownPath('file.md.bak'), false)
  assert.equal(isMarkdownPath('markdown.txt'), false)
  assert.equal(isMarkdownPath(''), false)
})
