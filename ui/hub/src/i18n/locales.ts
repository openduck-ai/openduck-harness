export type Locale = 'en' | 'zh-CN'
export const SUPPORTED_LOCALES: readonly Locale[] = ['en', 'zh-CN']
export const DEFAULT_LOCALE: Locale = 'en'
export const LOCALE_STORAGE_KEY = 'goose-hub-locale'

function matchSupportedTag(tag: string): Locale | null {
  const normalized = tag.trim().replace(/_/g, '-').toLowerCase()
  if (!normalized) return null
  const parts = normalized.split('-')
  if (parts[0] === 'en') {
    return 'en'
  }
  if (parts[0] === 'zh') {
    if (parts.length === 1) return 'zh-CN'
    if (parts[1] === 'hans') return 'zh-CN'
    if (parts[1] === 'cn' || parts[1] === 'sg' || parts[1] === 'my') return 'zh-CN'
  }
  return null
}

export function normalizeLocale(tag: string): Locale {
  return matchSupportedTag(tag) ?? DEFAULT_LOCALE
}

export function readStoredLocale(
  storage: { getItem(key: string): string | null } | null | undefined,
): Locale | null {
  if (!storage) return null
  try {
    const value = storage.getItem(LOCALE_STORAGE_KEY)
    if (value === 'en' || value === 'zh-CN') {
      return value
    }
    return null
  } catch {
    return null
  }
}

export function writeStoredLocale(
  storage: { setItem(key: string, value: string): void } | null | undefined,
  locale: Locale,
): boolean {
  if (!storage) return false
  try {
    storage.setItem(LOCALE_STORAGE_KEY, locale)
    return true
  } catch {
    return false
  }
}

export function chooseInitialLocale(
  storedLocale: Locale | null,
  languages: readonly string[],
): Locale {
  if (storedLocale !== null) {
    return storedLocale
  }
  return detectLocale(languages)
}

export function detectLocale(languages: readonly string[]): Locale {
  for (const tag of languages) {
    const match = matchSupportedTag(tag)
    if (match) {
      return match
    }
  }
  return DEFAULT_LOCALE
}

export function resolveInitialLocale(): Locale {
  if (typeof window === 'undefined') {
    return DEFAULT_LOCALE
  }

  try {
    const stored = readStoredLocale(window.localStorage)
    if (stored) {
      return stored
    }
  } catch {
    // Ignore storage access errors
  }

  let languages: readonly string[] = []
  try {
    if (typeof navigator !== 'undefined') {
      const navLanguages = navigator.languages
      if (navLanguages && navLanguages.length > 0) {
        languages = navLanguages
      } else if (navigator.language) {
        languages = [navigator.language]
      }
    }
  } catch {
    // Ignore navigator access errors
  }

  return chooseInitialLocale(null, languages)
}
