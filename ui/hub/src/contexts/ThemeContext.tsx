import { createContext, useContext, useEffect, useState, useMemo, type ReactNode } from 'react'
import {
  THEME_STORAGE_KEY,
  applyThemeToDocument,
  getSystemTheme,
  type ResolvedTheme,
  type ThemePreference,
} from '../theme'

export interface ThemeContextValue {
  themePreference: ThemePreference
  setThemePreference: (preference: ThemePreference) => void
  resolvedTheme: ResolvedTheme
  toggleTheme: () => void
}

const ThemeContext = createContext<ThemeContextValue | null>(null)

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [themePreference, setThemePreferenceState] = useState<ThemePreference>(() => {
    if (typeof window === 'undefined') return 'system'
    try {
      const stored = localStorage.getItem(THEME_STORAGE_KEY)
      if (stored === 'light' || stored === 'dark' || stored === 'system') {
        return stored
      }
    } catch {
      // ignore storage errors
    }
    return 'system'
  })

  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(getSystemTheme)

  useEffect(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    const handleChange = () => {
      setSystemTheme(mediaQuery.matches ? 'dark' : 'light')
    }
    mediaQuery.addEventListener('change', handleChange)
    return () => mediaQuery.removeEventListener('change', handleChange)
  }, [])

  const resolvedTheme: ResolvedTheme = themePreference === 'system' ? systemTheme : themePreference

  useEffect(() => {
    applyThemeToDocument(resolvedTheme)
  }, [resolvedTheme])

  const setThemePreference = (pref: ThemePreference) => {
    setThemePreferenceState(pref)
    try {
      localStorage.setItem(THEME_STORAGE_KEY, pref)
    } catch {
      // ignore storage errors
    }
  }

  const toggleTheme = () => {
    const nextTheme: ThemePreference = resolvedTheme === 'dark' ? 'light' : 'dark'
    setThemePreference(nextTheme)
  }

  const value = useMemo(
    () => ({
      themePreference,
      setThemePreference,
      resolvedTheme,
      toggleTheme,
    }),
    [themePreference, resolvedTheme],
  )

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext)
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider')
  }
  return context
}
