import assert from 'node:assert/strict'
import test from 'node:test'
import { resources } from './i18n/catalogs.ts'
import {
  DEFAULT_LOCALE,
  LOCALE_STORAGE_KEY,
  SUPPORTED_LOCALES,
  detectLocale,
  normalizeLocale,
  readStoredLocale,
  resolveInitialLocale,
} from './i18n/locales.ts'

test('locale constants are defined as specified', () => {
  assert.deepStrictEqual(SUPPORTED_LOCALES, ['en', 'zh-CN'])
  assert.equal(DEFAULT_LOCALE, 'en')
  assert.equal(LOCALE_STORAGE_KEY, 'goose-hub-locale')
})

test('normalizeLocale normalizes aliases and falls back to en', () => {
  const chineseTags = ['zh', 'zh-CN', 'zh-cn', 'zh_Hans', 'zh-Hans-CN', 'zh-SG', 'zh-MY']
  for (const tag of chineseTags) {
    assert.equal(normalizeLocale(tag), 'zh-CN', `Expected ${tag} to normalize to zh-CN`)
  }

  const englishTags = ['en', 'en-US']
  for (const tag of englishTags) {
    assert.equal(normalizeLocale(tag), 'en', `Expected ${tag} to normalize to en`)
  }

  const fallbackTags = ['fr', 'de-DE', 'zh-TW', 'zh-Hant', '']
  for (const tag of fallbackTags) {
    assert.equal(normalizeLocale(tag), 'en', `Expected ${tag} to fall back to en`)
  }
})

test('detectLocale returns first supported language or falls back to en', () => {
  assert.equal(detectLocale(['fr', 'zh-CN']), 'zh-CN')
  assert.equal(detectLocale(['en-US', 'zh']), 'en')
  assert.equal(detectLocale(['fr', 'zh-TW', 'zh-Hant', 'de-DE', 'zh-SG']), 'zh-CN')
  assert.equal(detectLocale(['fr', 'zh-TW', 'zh-HK', 'zh-MO']), 'en')
  assert.equal(detectLocale([]), 'en')
})

test('readStoredLocale accepts only canonical stored locales and handles errors', () => {
  assert.equal(readStoredLocale({ getItem: (k) => (k === LOCALE_STORAGE_KEY ? 'en' : null) }), 'en')
  assert.equal(readStoredLocale({ getItem: (k) => (k === LOCALE_STORAGE_KEY ? 'zh-CN' : null) }), 'zh-CN')

  assert.equal(readStoredLocale({ getItem: () => null }), null)
  assert.equal(readStoredLocale({ getItem: () => '' }), null)
  assert.equal(readStoredLocale({ getItem: () => 'zh' }), null)
  assert.equal(readStoredLocale({ getItem: () => 'fr' }), null)
  assert.equal(readStoredLocale({ getItem: () => 'garbage' }), null)

  assert.equal(readStoredLocale(null), null)
  assert.equal(readStoredLocale(undefined), null)

  assert.equal(
    readStoredLocale({
      getItem: () => {
        throw new Error('localStorage disabled')
      },
    }),
    null,
  )
})

test('resolveInitialLocale does not throw without window and returns en', () => {
  assert.equal(resolveInitialLocale(), 'en')
})

test('catalog parity: keys, trimmed non-empty strings, and placeholder sets match', () => {
  function flattenCatalog(record: Record<string, unknown>, prefix = ''): Record<string, string> {
    const result: Record<string, string> = {}
    for (const [key, value] of Object.entries(record)) {
      const fullKey = prefix ? `${prefix}.${key}` : key
      if (value && typeof value === 'object' && !Array.isArray(value)) {
        Object.assign(result, flattenCatalog(value as Record<string, unknown>, fullKey))
      } else if (typeof value === 'string') {
        result[fullKey] = value
      } else {
        throw new Error(`Invalid leaf at ${fullKey}: ${String(value)}`)
      }
    }
    return result
  }

  function extractPlaceholders(str: string): string[] {
    const matches = str.match(/\{\{([^}]+)\}\}/g)
    if (!matches) return []
    return matches.map((m) => m.slice(2, -2).trim()).sort()
  }

  const flatEn = flattenCatalog(resources.en.translation)
  const flatZh = flattenCatalog(resources['zh-CN'].translation)

  const enKeys = Object.keys(flatEn).sort()
  const zhKeys = Object.keys(flatZh).sort()
  const enKeySet = new Set(enKeys)
  const zhKeySet = new Set(zhKeys)
  const symmetricDiff = [
    ...enKeys.filter((k) => !zhKeySet.has(k)),
    ...zhKeys.filter((k) => !enKeySet.has(k)),
  ]

  assert.deepStrictEqual(
    enKeys,
    zhKeys,
    `Catalog keys must match. Symmetric difference: ${JSON.stringify(symmetricDiff)}`,
  )

  for (const [key, val] of Object.entries(flatEn)) {
    assert.equal(typeof val, 'string', `en key "${key}" must be a string`)
    assert.ok(val.trim().length > 0, `en key "${key}" must not be empty`)
    assert.equal(val, val.trim(), `en key "${key}" must be trimmed`)
  }

  for (const [key, val] of Object.entries(flatZh)) {
    assert.equal(typeof val, 'string', `zh key "${key}" must be a string`)
    assert.ok(val.trim().length > 0, `zh key "${key}" must not be empty`)
    assert.equal(val, val.trim(), `zh key "${key}" must be trimmed`)
  }

  for (const key of enKeys) {
    const enPlaceholders = extractPlaceholders(flatEn[key])
    const zhPlaceholders = extractPlaceholders(flatZh[key])
    assert.deepStrictEqual(
      enPlaceholders,
      zhPlaceholders,
      `Placeholders mismatch for key "${key}": expected ${JSON.stringify(enPlaceholders)}, got ${JSON.stringify(zhPlaceholders)}`,
    )
  }
})

test('exact seeded English values match current copy', () => {
  const t = resources.en.translation
  assert.equal(t.app.documentTitle, 'OpenDuck')
  assert.equal(t.app.errorBoundary.heading, 'Something went wrong in Goose Hub')
  assert.equal(t.app.errorBoundary.message, 'Unknown error')
  assert.equal(t.app.errorBoundary.reload, 'Reload application')
  assert.equal(t.header.projects, 'Projects')
  assert.equal(t.header.running, 'Running')
  assert.equal(t.header.settings, 'Settings')
  assert.equal(
    t.header.themeToggleTitle,
    'Theme: {{preference}} (showing {{resolved}}). Click to switch to {{next}} mode.',
  )
  assert.equal(t.header.themeToggleLabel, 'Toggle theme (currently {{resolved}})')
  assert.equal(t.header.connectedTitle, 'Connected to Goose / OpenDuck server')
  assert.equal(t.header.disconnectedTitle, 'Disconnected from server')
  assert.equal(t.common.cancel, 'Cancel')
  assert.equal(t.common.save, 'Save')
  assert.equal(t.common.retry, 'Retry')
  assert.equal(t.common.close, 'Close')
  assert.equal(t.common.loading, 'Loading\u2026')
  assert.equal(t.language.label, 'Language')
  assert.equal(t.language.english, 'English')
  assert.equal(t.language.chinese, '\u7b80\u4f53\u4e2d\u6587')
})
