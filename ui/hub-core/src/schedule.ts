export type ScheduleKind = 'none' | 'hourly' | 'daily' | 'weekdays' | 'weekly'

export type FriendlySchedule = {
  kind: ScheduleKind
  hour: number
  minute: number
  weekday: number
}

export const WEEKDAYS = [
  'Sunday',
  'Monday',
  'Tuesday',
  'Wednesday',
  'Thursday',
  'Friday',
  'Saturday',
] as const

export const defaultSchedule: FriendlySchedule = {
  kind: 'none',
  hour: 9,
  minute: 0,
  weekday: 0,
}

const DOW_ALIASES: Record<string, number> = {
  '0': 0,
  '7': 0,
  sun: 0,
  sunday: 0,
  '1': 1,
  mon: 1,
  monday: 1,
  '2': 2,
  tue: 2,
  tuesday: 2,
  '3': 3,
  wed: 3,
  wednesday: 3,
  '4': 4,
  thu: 4,
  thursday: 4,
  '5': 5,
  fri: 5,
  friday: 5,
  '6': 6,
  sat: 6,
  saturday: 6,
}

function isSingleNumber(value: string): boolean {
  return /^\d+$/.test(value)
}

function cronFields(cron: string): {
  minute: string
  hour: string
  dayOfMonth: string
  month: string
  dayOfWeek: string
} | null {
  const parts = cron.trim().split(/\s+/)
  if (parts.length === 5) {
    return {
      minute: parts[0],
      hour: parts[1],
      dayOfMonth: parts[2],
      month: parts[3],
      dayOfWeek: parts[4],
    }
  }
  if (parts.length === 6) {
    return {
      minute: parts[1],
      hour: parts[2],
      dayOfMonth: parts[3],
      month: parts[4],
      dayOfWeek: parts[5],
    }
  }
  return null
}

function parseWeekday(value: string): number | null {
  const parsed = DOW_ALIASES[value.trim().toLowerCase()]
  return parsed === undefined ? null : parsed
}

function clampHour(value: number): number {
  if (!Number.isFinite(value)) return 9
  return Math.min(23, Math.max(0, Math.trunc(value)))
}

function clampMinute(value: number): number {
  if (!Number.isFinite(value)) return 0
  return Math.min(59, Math.max(0, Math.trunc(value)))
}

export function parseTimeInput(value: string): { hour: number; minute: number } {
  const [hourPart, minutePart] = value.split(':')
  return {
    hour: clampHour(Number(hourPart)),
    minute: clampMinute(Number(minutePart)),
  }
}

export function timeInputValue(hour: number, minute: number): string {
  return `${String(clampHour(hour)).padStart(2, '0')}:${String(clampMinute(minute)).padStart(2, '0')}`
}

export function formatClock(hour: number, minute: number): string {
  return new Date(1970, 0, 1, clampHour(hour), clampMinute(minute)).toLocaleTimeString([], {
    hour: 'numeric',
    minute: '2-digit',
  })
}

export function parseFriendlySchedule(cron?: string | null): FriendlySchedule {
  if (!cron?.trim()) {
    return { ...defaultSchedule }
  }
  const fields = cronFields(cron)
  if (!fields) {
    return { ...defaultSchedule }
  }

  const minute = isSingleNumber(fields.minute) ? clampMinute(Number(fields.minute)) : 0
  const hour = isSingleNumber(fields.hour) ? clampHour(Number(fields.hour)) : 9
  const starDate = fields.dayOfMonth === '*' && fields.month === '*'

  if (starDate && fields.hour === '*' && fields.dayOfWeek === '*') {
    return { kind: 'hourly', hour: 0, minute: 0, weekday: 0 }
  }

  if (starDate && isSingleNumber(fields.minute) && isSingleNumber(fields.hour)) {
    const weekday = parseWeekday(fields.dayOfWeek)
    if (fields.dayOfWeek === '*' || fields.dayOfWeek === '?') {
      return { kind: 'daily', hour, minute, weekday: 0 }
    }
    if (fields.dayOfWeek === '1-5' || fields.dayOfWeek.toLowerCase() === 'mon-fri') {
      return { kind: 'weekdays', hour, minute, weekday: 1 }
    }
    if (weekday !== null) {
      return { kind: 'weekly', hour, minute, weekday }
    }
    return { kind: 'daily', hour, minute, weekday: 0 }
  }

  return { ...defaultSchedule }
}

export function scheduleToCron(schedule: FriendlySchedule): string | undefined {
  if (schedule.kind === 'none') {
    return undefined
  }
  const minute = clampMinute(schedule.minute)
  const hour = clampHour(schedule.hour)
  const weekday = Math.min(6, Math.max(0, Math.trunc(schedule.weekday)))
  switch (schedule.kind) {
    case 'hourly':
      return '0 * * * *'
    case 'daily':
      return `${minute} ${hour} * * *`
    case 'weekdays':
      return `${minute} ${hour} * * 1-5`
    case 'weekly':
      return `${minute} ${hour} * * ${weekday}`
  }
}

export function describeSchedule(schedule: FriendlySchedule): string {
  const time = formatClock(schedule.hour, schedule.minute)
  switch (schedule.kind) {
    case 'none':
      return 'Only when you click Run'
    case 'hourly':
      return 'Every hour'
    case 'daily':
      return `Every day at ${time}`
    case 'weekdays':
      return `Weekdays at ${time}`
    case 'weekly':
      return `Every ${WEEKDAYS[schedule.weekday] ?? WEEKDAYS[0]} at ${time}`
  }
}

export function describeTaskSchedule(cron?: string | null, paused = false): string {
  if (!cron?.trim()) {
    return ''
  }
  const description = describeSchedule(parseFriendlySchedule(cron))
  return paused ? `${description} — auto-run is off` : description
}
