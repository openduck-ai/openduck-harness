import assert from 'node:assert/strict'
import test from 'node:test'
import {
  directoryPathsMatch,
  isProjectDirectoryName,
  isValidProjectSlug,
  slugFromDirectoryName,
  titleFromDirectoryName,
  uniqueSlug,
} from './projectIdentity.ts'

test('builds a title and slug from a directory name', () => {
  assert.equal(titleFromDirectoryName('my-next.js-web_app'), 'My Next.js Web App')
  assert.equal(slugFromDirectoryName('my-next.js-web_app'), 'my-next-js-web-app')
  assert.equal(titleFromDirectoryName('MQTT-broker'), 'MQTT Broker')
  assert.equal(slugFromDirectoryName('MQTT-broker'), 'mqtt-broker')
  assert.equal(titleFromDirectoryName('中文项目'), '中文项目')
  assert.equal(slugFromDirectoryName('中文项目'), '')
  assert.equal(isValidProjectSlug('my-next-js-web-app'), true)
  assert.equal(isValidProjectSlug('My-App'), false)
  assert.equal(isValidProjectSlug('a--b'), false)
  assert.equal(isValidProjectSlug(''), false)
})

test('keeps generated slugs unique and within the identifier rules', () => {
  assert.equal(uniqueSlug('my-app', []), 'my-app')
  assert.equal(uniqueSlug('my-app', ['my-app']), 'my-app-2')
  assert.equal(uniqueSlug('my-app', ['my-app', 'my-app-2']), 'my-app-3')
  assert.equal(uniqueSlug('', ['my-app']), '')
  const long = 'a'.repeat(80)
  const next = uniqueSlug(long, [long])
  assert.equal(isValidProjectSlug(next), true)
  assert.notEqual(next, long)
  assert.ok(next.endsWith('-2'))
  assert.ok(next.length <= 80)
})

test('accepts a single folder name and matches directory paths', () => {
  assert.equal(isProjectDirectoryName('demo'), true)
  assert.equal(isProjectDirectoryName('my folder'), true)
  assert.equal(isProjectDirectoryName(''), false)
  assert.equal(isProjectDirectoryName(' foo'), false)
  assert.equal(isProjectDirectoryName('.'), false)
  assert.equal(isProjectDirectoryName('..'), false)
  assert.equal(isProjectDirectoryName('a/b'), false)
  assert.equal(isProjectDirectoryName('a\\b'), false)
  assert.equal(directoryPathsMatch('/work/demo/', '/work/demo'), true)
  assert.equal(directoryPathsMatch('/work/demo', '/work/demo2'), false)
})
