import en from '../locales/en.json' with { type: 'json' }
import zh from '../locales/zh.json' with { type: 'json' }
import type { Locale } from './locales.ts'

export const resources = {
  en: { translation: en },
  'zh-CN': { translation: zh },
} satisfies Record<Locale, { translation: typeof en }>
