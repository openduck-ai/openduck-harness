import { resources } from './catalogs.ts'
import { i18nInstance, translate } from './index.ts'
import { DEFAULT_LOCALE, normalizeLocale, type Locale } from './locales.ts'

export type DateInput = Date | number | string
export type DateOptions = Intl.DateTimeFormatOptions & { locale?: Locale }
export type NumberOptions = Intl.NumberFormatOptions & { locale?: Locale }

function resolveLocale(optionsLocale?: Locale): {
  tag: 'en-US' | 'zh-CN'
  locale: Locale
} {
  const locale = normalizeLocale(optionsLocale ?? i18nInstance.language ?? DEFAULT_LOCALE)
  const tag = locale === 'zh-CN' ? 'zh-CN' : 'en-US'
  return { tag, locale }
}

function translateForLocale(
  key: string,
  values: Record<string, string | number>,
  locale: Locale,
): string {
  if (i18nInstance.isInitialized) {
    return i18nInstance.getFixedT(locale)(key, values)
  }
  const message = key
    .split('.')
    .reduce<unknown>(
      (current, part) =>
        current && typeof current === 'object'
          ? (current as Record<string, unknown>)[part]
          : undefined,
      resources[locale].translation,
    )
  if (typeof message !== 'string') return translate(key, values)
  return message.replace(/\{\{([^}]+)\}\}/g, (match, token: string) =>
    token.trim() in values ? String(values[token.trim()]) : match,
  )
}

const DATE_TIME_FIELD_KEYS = [
  'year',
  'month',
  'day',
  'hour',
  'minute',
  'second',
  'timeStyle',
  'dateStyle',
] as const

function hasDateTimeField(options: Intl.DateTimeFormatOptions): boolean {
  return DATE_TIME_FIELD_KEYS.some(key => options[key] !== undefined)
}

function parseDate(value: DateInput): Date | null {
  if (value === null || value === undefined) return null
  let date: Date
  if (value instanceof Date) {
    date = value
  } else if (typeof value === 'number') {
    if (Number.isNaN(value) || !Number.isFinite(value)) return null
    date = new Date(value)
  } else if (typeof value === 'string') {
    if (!value.trim()) return null
    date = new Date(value)
  } else {
    return null
  }
  return Number.isNaN(date.getTime()) ? null : date
}

export function formatDate(value: DateInput, options?: DateOptions): string {
  const date = parseDate(value)
  if (!date) return ''

  const { tag } = resolveLocale(options?.locale)
  const { locale: _locale, ...intlOptions } = options ?? {}

  const opts: Intl.DateTimeFormatOptions = hasDateTimeField(intlOptions)
    ? intlOptions
    : { year: 'numeric', month: 'numeric', day: 'numeric', ...intlOptions }

  try {
    return new Intl.DateTimeFormat(tag, opts).format(date)
  } catch {
    return ''
  }
}

export function formatDateTime(value: DateInput, options?: DateOptions): string {
  const date = parseDate(value)
  if (!date) return ''

  const { tag } = resolveLocale(options?.locale)
  const { locale: _locale, ...intlOptions } = options ?? {}

  const opts: Intl.DateTimeFormatOptions = hasDateTimeField(intlOptions)
    ? intlOptions
    : {
        year: 'numeric',
        month: 'numeric',
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
        second: '2-digit',
        ...intlOptions,
      }

  try {
    return new Intl.DateTimeFormat(tag, opts).format(date)
  } catch {
    return ''
  }
}

export function formatTime(value: DateInput, options?: DateOptions): string {
  const date = parseDate(value)
  if (!date) return ''

  const { tag } = resolveLocale(options?.locale)
  const { locale: _locale, ...intlOptions } = options ?? {}

  const opts: Intl.DateTimeFormatOptions = hasDateTimeField(intlOptions)
    ? intlOptions
    : {
        hour: 'numeric',
        minute: '2-digit',
        second: '2-digit',
        ...intlOptions,
      }

  try {
    return new Intl.DateTimeFormat(tag, opts).format(date)
  } catch {
    return ''
  }
}

export function formatNumber(value: number, options?: NumberOptions): string {
  const { tag } = resolveLocale(options?.locale)
  const { locale: _locale, ...intlOptions } = options ?? {}

  try {
    return new Intl.NumberFormat(tag, intlOptions).format(value)
  } catch {
    return String(value)
  }
}

export function formatPercent(ratio: number, options?: NumberOptions): string {
  const opts: NumberOptions = {
    maximumFractionDigits: 1,
    ...options,
  }
  return `${formatNumber(ratio * 100, opts)}%`
}

export function formatRelativeTime(
  iso: string | null | undefined,
  now: number,
  options?: { locale?: Locale; future?: 'clamp' | 'relative' },
): string {
  if (!iso) return ''
  const timestamp = Date.parse(iso)
  if (Number.isNaN(timestamp)) return ''

  const { locale } = resolveLocale(options?.locale)
  const futureMode = options?.future ?? 'clamp'

  const deltaMs = now - timestamp

  let isFuture = false
  let diffMs: number

  if (deltaMs < 0) {
    if (futureMode === 'relative') {
      isFuture = true
      diffMs = -deltaMs
    } else {
      diffMs = 0
    }
  } else {
    diffMs = deltaMs
  }

  const diffSec = Math.floor(diffMs / 1000)

  if (diffSec < 45) {
    if (isFuture) {
      return translateForLocale('format.relative.inLessThanMinute', {}, locale)
    }
    return translateForLocale('format.relative.justNow', {}, locale)
  }

  if (diffSec < 3600) {
    const mins = Math.max(1, Math.floor(diffSec / 60))
    if (isFuture) {
      return translateForLocale('format.relative.inMinutes', { count: mins }, locale)
    }
    return translateForLocale('format.relative.minutesAgo', { count: mins }, locale)
  }

  if (diffSec < 86400) {
    const hours = Math.floor(diffSec / 3600)
    if (isFuture) {
      return translateForLocale('format.relative.inHours', { count: hours }, locale)
    }
    return translateForLocale('format.relative.hoursAgo', { count: hours }, locale)
  }

  if (diffSec < 7 * 86400) {
    const days = Math.floor(diffSec / 86400)
    if (isFuture) {
      return translateForLocale('format.relative.inDays', { count: days }, locale)
    }
    return translateForLocale('format.relative.daysAgo', { count: days }, locale)
  }

  return formatDate(timestamp, {
    month: 'short',
    day: 'numeric',
    timeZone: 'UTC',
    locale,
  })
}

export function formatDurationMs(ms: number, options?: { locale?: Locale }): string {
  const { locale } = resolveLocale(options?.locale)

  if (ms < 1000) return translateForLocale('format.duration.lessThanSecond', {}, locale)
  const totalSeconds = Math.round(ms / 1000)
  if (totalSeconds < 1) return translateForLocale('format.duration.lessThanSecond', {}, locale)

  const hours = Math.floor(totalSeconds / 3600)
  const remainingSeconds = totalSeconds % 3600
  const minutes = Math.floor(remainingSeconds / 60)
  const seconds = remainingSeconds % 60

  const values = { hours, minutes, seconds }
  if (hours > 0) {
    if (minutes > 0) {
      return translateForLocale(
        seconds > 0 ? 'format.duration.hoursMinutesSeconds' : 'format.duration.hoursMinutes',
        values,
        locale,
      )
    }
    return translateForLocale(
      seconds > 0 ? 'format.duration.hoursSeconds' : 'format.duration.hours',
      values,
      locale,
    )
  }
  if (minutes > 0) {
    return translateForLocale(
      seconds > 0 ? 'format.duration.minutesSeconds' : 'format.duration.minutes',
      values,
      locale,
    )
  }
  return seconds > 0 ? translateForLocale('format.duration.seconds', values, locale) : ''
}

export function formatExportDuration(ms: number, options?: { locale?: Locale }): string {
  resolveLocale(options?.locale)
  if (ms <= 0) return '0s'
  if (ms < 1000) return `${ms}ms`
  const seconds = ms / 1000
  if (seconds < 60) {
    return `${seconds.toFixed(2)}s`
  }
  const mins = Math.floor(seconds / 60)
  const remainingSecs = Math.round(seconds % 60)
  return `${mins}m ${remainingSecs}s`
}
