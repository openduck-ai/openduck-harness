import type { ChangeEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { useLocale } from '../contexts/LocaleContext.tsx'
import { SUPPORTED_LOCALES, type Locale } from '../i18n/locales.ts'

export function LocaleSwitcher() {
  const { t } = useTranslation()
  const { locale, setLocale } = useLocale()

  const handleChange = (e: ChangeEvent<HTMLSelectElement>) => {
    const val = e.target.value
    if ((SUPPORTED_LOCALES as readonly string[]).includes(val)) {
      setLocale(val as Locale)
    }
  }

  return (
    <select
      className="locale-switcher"
      value={locale}
      aria-label={t('language.label')}
      onChange={handleChange}
    >
      <option value="en">{t('language.english')}</option>
      <option value="zh-CN">{t('language.chinese')}</option>
    </select>
  )
}
