import assert from 'node:assert/strict'
import test from 'node:test'
import { resources } from './i18n/catalogs.ts'
import { changeLocale, i18nInstance, initI18n } from './i18n/index.ts'
import {
  chooseInitialLocale,
  readStoredLocale,
  resolveInitialLocale,
  writeStoredLocale,
} from './i18n/locales.ts'

test('write->read round-trip with fake storage and precedence over opposing browser language', () => {
  const canonicalLocales = ['en', 'zh-CN'] as const
  for (const locale of canonicalLocales) {
    const map = new Map<string, string>()
    let setItemCalls = 0
    const fakeStorage = {
      setItem(key: string, value: string) {
        setItemCalls++
        map.set(key, value)
      },
      getItem(key: string) {
        return map.get(key) ?? null
      },
    }

    const written = writeStoredLocale(fakeStorage, locale)
    assert.equal(written, true)
    assert.equal(setItemCalls, 1)
    assert.equal(map.get('goose-hub-locale'), locale)

    const read = readStoredLocale(fakeStorage)
    assert.equal(read, locale)

    const opposingLanguages = locale === 'en' ? ['zh-CN'] : ['en']
    const chosen = chooseInitialLocale(read, opposingLanguages)
    assert.equal(chosen, locale)
  }
})

test('writeStoredLocale failure modes return false without escaping', () => {
  assert.equal(writeStoredLocale(null, 'en'), false)
  assert.equal(writeStoredLocale(undefined, 'en'), false)
  assert.equal(writeStoredLocale(null, 'zh-CN'), false)
  assert.equal(writeStoredLocale(undefined, 'zh-CN'), false)

  const throwingStorage = {
    setItem() {
      throw new Error('Storage write denied')
    },
  }
  assert.doesNotThrow(() => {
    assert.equal(writeStoredLocale(throwingStorage, 'en'), false)
  })
  assert.doesNotThrow(() => {
    assert.equal(writeStoredLocale(throwingStorage, 'zh-CN'), false)
  })
})

test('chooseInitialLocale resolves detected locale and stored precedence', () => {
  assert.equal(chooseInitialLocale(null, ['fr', 'zh-CN']), 'zh-CN')
  assert.equal(chooseInitialLocale(null, []), 'en')
  assert.equal(chooseInitialLocale(null, ['fr', 'de', 'ja']), 'en')
  assert.equal(chooseInitialLocale('en', ['zh-CN']), 'en')
  assert.equal(chooseInitialLocale('zh-CN', ['en']), 'zh-CN')
})

test('real i18next transition en -> zh-CN -> en', async (t) => {
  await initI18n('en')
  changeLocale('en')

  await t.test('switch en -> zh-CN', async () => {
    let eventCount = 0
    let translationInHandler = ''
    const onLanguageChanged = () => {
      eventCount++
      translationInHandler = i18nInstance.t('header.projects')
    }

    try {
      i18nInstance.on('languageChanged', onLanguageChanged)
      changeLocale('zh-CN')

      assert.equal(i18nInstance.language, 'zh-CN')
      assert.equal(i18nInstance.resolvedLanguage, 'zh-CN')
      assert.equal(i18nInstance.t('header.projects'), '\u9879\u76ee')
      assert.equal(eventCount, 1)
      assert.equal(translationInHandler, '\u9879\u76ee')

      await Promise.resolve()
      assert.equal(eventCount, 1)
    } finally {
      i18nInstance.off('languageChanged', onLanguageChanged)
    }
  })

  await t.test('switch zh-CN -> en', async () => {
    let eventCount = 0
    let translationInHandler = ''
    const onLanguageChanged = () => {
      eventCount++
      translationInHandler = i18nInstance.t('header.projects')
    }

    try {
      i18nInstance.on('languageChanged', onLanguageChanged)
      changeLocale('en')

      assert.equal(i18nInstance.language, 'en')
      assert.equal(i18nInstance.resolvedLanguage, 'en')
      assert.equal(i18nInstance.t('header.projects'), 'Projects')
      assert.equal(eventCount, 1)
      assert.equal(translationInHandler, 'Projects')

      await Promise.resolve()
      assert.equal(eventCount, 1)
    } finally {
      i18nInstance.off('languageChanged', onLanguageChanged)
      changeLocale('en')
    }
  })
})

test('storage adapter integration with changeLocale and resolveInitialLocale', async () => {
  await initI18n('en')
  const originalDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'window')
  const map = new Map<string, string>()

  const fakeStorage = {
    getItem(key: string) {
      return map.get(key) ?? null
    },
    setItem(key: string, value: string) {
      map.set(key, value)
    },
    removeItem(key: string) {
      map.delete(key)
    },
    clear() {
      map.clear()
    },
  }

  try {
    Object.defineProperty(globalThis, 'window', {
      value: { localStorage: fakeStorage },
      writable: true,
      configurable: true,
    })

    changeLocale('zh-CN')
    assert.equal(map.get('goose-hub-locale'), 'zh-CN')
    assert.equal(resolveInitialLocale(), 'zh-CN')

    changeLocale('en')
    assert.equal(map.get('goose-hub-locale'), 'en')
    assert.equal(resolveInitialLocale(), 'en')

    const throwingSetterStorage = {
      getItem(key: string) {
        return map.get(key) ?? null
      },
      setItem() {
        throw new Error('QuotaExceededError in setter')
      },
    }
    Object.defineProperty(globalThis, 'window', {
      value: { localStorage: throwingSetterStorage },
      writable: true,
      configurable: true,
    })

    changeLocale('zh-CN')
    assert.equal(i18nInstance.language, 'zh-CN')
    assert.equal(i18nInstance.t('header.projects'), '\u9879\u76ee')

    const windowWithThrowingGetter = {}
    Object.defineProperty(windowWithThrowingGetter, 'localStorage', {
      get() {
        throw new Error('SecurityError: localStorage access blocked')
      },
      configurable: true,
    })
    Object.defineProperty(globalThis, 'window', {
      value: windowWithThrowingGetter,
      writable: true,
      configurable: true,
    })

    changeLocale('en')
    assert.equal(i18nInstance.language, 'en')
    assert.equal(i18nInstance.t('header.projects'), 'Projects')
  } finally {
    if (originalDescriptor) {
      Object.defineProperty(globalThis, 'window', originalDescriptor)
    } else {
      delete (globalThis as Record<string, unknown>).window
    }
    changeLocale('en')
  }
})

test('catalog endonyms and language label assertions', () => {
  const enTranslation = resources.en.translation
  const zhTranslation = resources['zh-CN'].translation

  assert.equal(enTranslation.language.english, 'English')
  assert.equal(enTranslation.language.chinese, '\u7b80\u4f53\u4e2d\u6587')

  assert.equal(zhTranslation.language.english, 'English')
  assert.equal(zhTranslation.language.chinese, '\u7b80\u4f53\u4e2d\u6587')

  assert.equal(enTranslation.language.label, 'Language')
  assert.equal(zhTranslation.language.label, '\u8bed\u8a00')
  assert.notEqual(enTranslation.language.label, zhTranslation.language.label)
})
