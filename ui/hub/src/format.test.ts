import assert from 'node:assert/strict'
import test from 'node:test'
import { changeLocale, i18nInstance, initI18n } from './i18n/index.ts'
import {
  formatDate,
  formatDateTime,
  formatDurationMs,
  formatExportDuration,
  formatNumber,
  formatPercent,
  formatRelativeTime,
  formatTime,
} from './i18n/format.ts'

test('formatDurationMs produces exact English strings for all 9 standard cases', () => {
  assert.equal(formatDurationMs(500), '<1s')
  assert.equal(formatDurationMs(999), '<1s')
  assert.equal(formatDurationMs(1000), '1s')
  assert.equal(formatDurationMs(2140), '2s')
  assert.equal(formatDurationMs(45000), '45s')
  assert.equal(formatDurationMs(120000), '2m')
  assert.equal(formatDurationMs(134000), '2m 14s')
  assert.equal(formatDurationMs(3600000), '1h')
  assert.equal(formatDurationMs(3780000), '1h 3m')
})

test('formatDurationMs produces natural Chinese strings with locale option', () => {
  assert.equal(formatDurationMs(500, { locale: 'zh-CN' }), '<1秒')
  assert.equal(formatDurationMs(999, { locale: 'zh-CN' }), '<1秒')
  assert.equal(formatDurationMs(1000, { locale: 'zh-CN' }), '1秒')
  assert.equal(formatDurationMs(2140, { locale: 'zh-CN' }), '2秒')
  assert.equal(formatDurationMs(45000, { locale: 'zh-CN' }), '45秒')
  assert.equal(formatDurationMs(120000, { locale: 'zh-CN' }), '2分钟')
  assert.equal(formatDurationMs(134000, { locale: 'zh-CN' }), '2分14秒')
  assert.equal(formatDurationMs(3600000, { locale: 'zh-CN' }), '1小时')
  assert.equal(formatDurationMs(3780000, { locale: 'zh-CN' }), '1小时3分钟')
  assert.equal(formatDurationMs(3794000, { locale: 'zh-CN' }), '1小时3分14秒')
})

test('formatRelativeTime reproduces exact English strings matching projectTasks.test.ts', () => {
  const now = Date.parse('2024-10-07T12:00:00.000Z')
  assert.equal(formatRelativeTime('', now), '')
  assert.equal(formatRelativeTime('invalid-date', now), '')
  assert.equal(formatRelativeTime(null, now), '')
  assert.equal(formatRelativeTime(undefined, now), '')
  assert.equal(formatRelativeTime('2024-10-07T11:59:30.000Z', now), 'just now') // 30s ago
  assert.equal(formatRelativeTime('2024-10-07T11:57:00.000Z', now), '3m ago') // 3m ago
  assert.equal(formatRelativeTime('2024-10-07T07:00:00.000Z', now), '5h ago') // 5h ago
  assert.equal(formatRelativeTime('2024-10-05T12:00:00.000Z', now), '2d ago') // 2d ago
  assert.equal(formatRelativeTime('2024-09-01T12:00:00.000Z', now), 'Sep 1') // UTC fallback >= 7d
})

test('formatRelativeTime clamps future dates by default', () => {
  const now = Date.parse('2024-10-07T12:00:00.000Z')
  // Future dates clamp to 'just now' / '刚刚' by default
  assert.equal(formatRelativeTime('2024-10-07T12:05:00.000Z', now), 'just now')
  assert.equal(formatRelativeTime('2024-10-07T12:05:00.000Z', now, { locale: 'zh-CN' }), '刚刚')
})

test('formatRelativeTime handles future: relative for new callers', () => {
  const now = Date.parse('2024-10-07T12:00:00.000Z')
  // <45s future
  assert.equal(
    formatRelativeTime('2024-10-07T12:00:30.000Z', now, { future: 'relative' }),
    'in <1m',
  )
  assert.equal(
    formatRelativeTime('2024-10-07T12:00:30.000Z', now, { locale: 'zh-CN', future: 'relative' }),
    '<1分钟后',
  )

  // Minutes future
  assert.equal(
    formatRelativeTime('2024-10-07T12:05:00.000Z', now, { future: 'relative' }),
    'in 5m',
  )
  assert.equal(
    formatRelativeTime('2024-10-07T12:05:00.000Z', now, { locale: 'zh-CN', future: 'relative' }),
    '5分钟后',
  )

  // Hours future
  assert.equal(
    formatRelativeTime('2024-10-07T14:00:00.000Z', now, { future: 'relative' }),
    'in 2h',
  )
  assert.equal(
    formatRelativeTime('2024-10-07T14:00:00.000Z', now, { locale: 'zh-CN', future: 'relative' }),
    '2小时后',
  )

  // Days future
  assert.equal(
    formatRelativeTime('2024-10-10T12:00:00.000Z', now, { future: 'relative' }),
    'in 3d',
  )
  assert.equal(
    formatRelativeTime('2024-10-10T12:00:00.000Z', now, { locale: 'zh-CN', future: 'relative' }),
    '3天后',
  )

  // >= 7d future falls back to UTC short date
  assert.equal(
    formatRelativeTime('2024-10-15T12:00:00.000Z', now, { future: 'relative' }),
    'Oct 15',
  )
  assert.equal(
    formatRelativeTime('2024-10-15T12:00:00.000Z', now, { locale: 'zh-CN', future: 'relative' }),
    '10月15日',
  )
})

test('formatDate, formatDateTime, formatTime produce different output for en vs zh-CN and never throw', () => {
  const instant = '2024-09-01T12:34:56.000Z'

  // formatDate
  const enDate = formatDate(instant, { timeZone: 'UTC', locale: 'en' })
  const zhDate = formatDate(instant, { timeZone: 'UTC', locale: 'zh-CN' })
  assert.equal(enDate, '9/1/2024')
  assert.equal(zhDate, '2024/9/1')
  assert.notEqual(enDate, zhDate)

  // formatDateTime
  const enDateTime = formatDateTime(instant, { timeZone: 'UTC', locale: 'en' })
  const zhDateTime = formatDateTime(instant, { timeZone: 'UTC', locale: 'zh-CN' })
  assert.equal(enDateTime, '9/1/2024, 12:34:56 PM')
  assert.equal(zhDateTime, '2024/9/1 12:34:56')
  assert.notEqual(enDateTime, zhDateTime)

  // formatTime
  const enTime = formatTime(instant, { timeZone: 'UTC', locale: 'en' })
  const zhTime = formatTime(instant, { timeZone: 'UTC', locale: 'zh-CN' })
  assert.equal(enTime, '12:34:56 PM')
  assert.equal(zhTime, '12:34:56')
  assert.notEqual(enTime, zhTime)

  // Caller supplied fields replace defaults
  const customDate = formatDate(instant, { month: 'short', day: 'numeric', timeZone: 'UTC', locale: 'en' })
  assert.equal(customDate, 'Sep 1')

  // Invalid inputs never throw and return empty string
  const invalidInputs = ['', 'not-a-date', '   ', NaN, new Date('invalid')]
  for (const input of invalidInputs) {
    assert.equal(formatDate(input), '')
    assert.equal(formatDateTime(input), '')
    assert.equal(formatTime(input), '')
  }
})

test('formatNumber and formatPercent show locale differences', () => {
  // Notation compact shows distinct English vs Chinese notation
  const enCompact = formatNumber(10000, { notation: 'compact', locale: 'en' })
  const zhCompact = formatNumber(10000, { notation: 'compact', locale: 'zh-CN' })
  assert.equal(enCompact, '10K')
  assert.equal(zhCompact, '1万')
  assert.notEqual(enCompact, zhCompact)

  // Currency formatting differs
  const enCurrency = formatNumber(10, { style: 'currency', currency: 'USD', locale: 'en' })
  const zhCurrency = formatNumber(10, { style: 'currency', currency: 'USD', locale: 'zh-CN' })
  assert.equal(enCurrency, '$10.00')
  assert.equal(zhCurrency, 'US$10.00')
  assert.notEqual(enCurrency, zhCurrency)

  // Standard formatNumber
  assert.equal(formatNumber(1234567.89, { locale: 'en' }), '1,234,567.89')

  // formatPercent
  assert.equal(formatPercent(0.125, { locale: 'en' }), '12.5%')
  assert.equal(formatPercent(0.5, { locale: 'en' }), '50%')
  assert.equal(formatPercent(1, { locale: 'en' }), '100%')
  assert.equal(formatPercent(0.1254, { maximumFractionDigits: 2, locale: 'en' }), '12.54%')
})

test('formatExportDuration tests exact boundaries', () => {
  assert.equal(formatExportDuration(0), '0s')
  assert.equal(formatExportDuration(-10), '0s')
  assert.equal(formatExportDuration(999), '999ms')
  assert.equal(formatExportDuration(1000), '1.00s')
  assert.equal(formatExportDuration(59999), '60.00s')
  assert.equal(formatExportDuration(60000), '1m 0s')
  assert.equal(formatExportDuration(61500), '1m 2s')
  assert.equal(formatExportDuration(119999), '1m 60s') // preserves rounding edge
})

test('locale option overrides the global i18next language', async () => {
  await initI18n('en')
  const instant = '2024-09-01T12:34:56.000Z'
  const initialLang = i18nInstance.language

  try {
    // Switch global language to zh-CN
    changeLocale('zh-CN')

    // Calls without option follow the global language
    assert.equal(formatDate(instant, { timeZone: 'UTC' }), '2024/9/1')
    assert.equal(formatDurationMs(120000), '2分钟')

    // Explicit locale: 'en' overrides global language
    assert.equal(formatDate(instant, { timeZone: 'UTC', locale: 'en' }), '9/1/2024')
    assert.equal(formatDurationMs(120000, { locale: 'en' }), '2m')

    // Switch global language to en
    changeLocale('en')
    assert.equal(formatDate(instant, { timeZone: 'UTC' }), '9/1/2024')

    // Explicit locale: 'zh-CN' overrides global language
    assert.equal(formatDate(instant, { timeZone: 'UTC', locale: 'zh-CN' }), '2024/9/1')
    assert.equal(formatDurationMs(120000, { locale: 'zh-CN' }), '2分钟')
  } finally {
    changeLocale((initialLang as 'en' | 'zh-CN') || 'en')
  }
})
