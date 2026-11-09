import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import { resources } from './catalogs.ts'
import {
  DEFAULT_LOCALE,
  SUPPORTED_LOCALES,
  normalizeLocale,
  resolveInitialLocale,
  writeStoredLocale,
  type Locale,
} from './locales.ts'

export const i18nInstance = i18n.use(initReactI18next)

export { resolveInitialLocale }

let initPromise: Promise<void> | null = null

export function initI18n(locale: Locale): Promise<void> {
  if (initPromise) {
    return initPromise
  }
  initPromise = (async () => {
    await i18nInstance.init({
      resources,
      lng: locale,
      fallbackLng: 'en',
      supportedLngs: [...SUPPORTED_LOCALES],
      load: 'currentOnly',
      ns: ['translation'],
      defaultNS: 'translation',
      interpolation: { escapeValue: false },
      initAsync: false,
      react: { useSuspense: false },
    })
    syncLocaleDocument()
  })()
  return initPromise
}

export function syncLocaleDocument(): void {
  if (typeof document === 'undefined') return
  try {
    const canonical = normalizeLocale(i18nInstance.language || DEFAULT_LOCALE)
    document.documentElement.lang = canonical
    document.title = translate('app.documentTitle')
  } catch {
    // Ignore document sync errors
  }
}

export function changeLocale(locale: Locale): void {
  void i18nInstance.changeLanguage(locale)
  syncLocaleDocument()
  try {
    if (typeof window !== 'undefined') {
      writeStoredLocale(window.localStorage, locale)
    }
  } catch {
    // Ignore storage errors (setItem failure must not block the in-memory change)
  }
}

function resolveFallback(key: string, values?: Record<string, string | number>): string {
  try {
    const parts = key.split('.')
    let current: unknown = resources.en.translation
    for (const part of parts) {
      if (current && typeof current === 'object' && part in current) {
        current = (current as Record<string, unknown>)[part]
      } else {
        return key
      }
    }
    if (typeof current === 'string') {
      if (!values) return current
      return current.replace(/\{\{([^}]+)\}\}/g, (_, token: string) => {
        const trimmed = token.trim()
        return trimmed in values ? String(values[trimmed]) : `{{${token}}}`
      })
    }
    return key
  } catch {
    return key
  }
}

export function translate(key: string, values?: Record<string, string | number>): string {
  if (i18nInstance.isInitialized) {
    try {
      const result = i18nInstance.t(key, values)
      if (typeof result === 'string' && result !== key) {
        return result
      }
    } catch {
      // Fall through to fallback
    }
  }
  return resolveFallback(key, values)
}
