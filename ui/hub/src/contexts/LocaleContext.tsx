import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useMemo,
  type ReactNode,
} from 'react'
import { useTranslation } from 'react-i18next'
import { changeLocale, syncLocaleDocument } from '../i18n/index.ts'
import { normalizeLocale, type Locale } from '../i18n/locales.ts'

export interface LocaleContextValue {
  locale: Locale
  setLocale: (locale: Locale) => void
}

const LocaleContext = createContext<LocaleContextValue | null>(null)

export interface LocaleProviderProps {
  initialLocale: Locale
  children: ReactNode
}

export function LocaleProvider({ initialLocale, children }: LocaleProviderProps) {
  const { i18n } = useTranslation()
  const locale: Locale = i18n.language ? normalizeLocale(i18n.language) : initialLocale

  const setLocale = useCallback((next: Locale) => {
    changeLocale(next)
  }, [])

  useLayoutEffect(() => {
    syncLocaleDocument()
  }, [locale])

  const value = useMemo(
    () => ({
      locale,
      setLocale,
    }),
    [locale, setLocale],
  )

  return <LocaleContext.Provider value={value}>{children}</LocaleContext.Provider>
}

export function useLocale(): LocaleContextValue {
  const context = useContext(LocaleContext)
  if (!context) {
    throw new Error('useLocale must be used within a LocaleProvider')
  }
  return context
}
