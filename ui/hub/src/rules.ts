import type { SourceEntry } from '@openduck/sdk'

export type RuleDraft = {
  path: string | null
  name: string
  description: string
  content: string
  globsText: string
  alwaysApply: boolean
  tagsText: string
  order: number
  global: boolean
  writable: boolean
  properties: Record<string, unknown>
}

const KNOWN_PROPERTY_KEYS = new Set([
  'globs',
  'tags',
  'alwaysApply',
  'always_apply',
  'order',
])

export function splitCommaList(value: string): string[] {
  return value
    .split(',')
    .map(item => item.trim())
    .filter(Boolean)
}

export function joinCommaList(values: string[]): string {
  return values.join(', ')
}

function asStringArray(value: unknown): string[] {
  if (Array.isArray(value)) {
    return value.map(item => String(item).trim()).filter(Boolean)
  }
  if (typeof value === 'string') {
    return splitCommaList(value)
  }
  return []
}

export function parseRuleProperties(properties?: Record<string, unknown> | null): {
  globs: string[]
  tags: string[]
  alwaysApply: boolean | undefined
  order: number
  extra: Record<string, unknown>
} {
  const props = properties ?? {}
  const extra: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(props)) {
    if (!KNOWN_PROPERTY_KEYS.has(key)) {
      extra[key] = value
    }
  }

  const alwaysApplyRaw = props.alwaysApply ?? props.always_apply
  const alwaysApply = typeof alwaysApplyRaw === 'boolean' ? alwaysApplyRaw : undefined
  const orderRaw = props.order
  const order = typeof orderRaw === 'number' && Number.isFinite(orderRaw) ? orderRaw : 0

  return {
    globs: asStringArray(props.globs),
    tags: asStringArray(props.tags),
    alwaysApply,
    order,
    extra,
  }
}

export function emptyRuleDraft(global: boolean): RuleDraft {
  return {
    path: null,
    name: '',
    description: '',
    content: '',
    globsText: '',
    alwaysApply: true,
    tagsText: '',
    order: 0,
    global,
    writable: true,
    properties: {},
  }
}

export function draftFromSource(source: SourceEntry): RuleDraft {
  const parsed = parseRuleProperties(source.properties)
  return {
    path: source.path,
    name: source.name,
    description: source.description,
    content: source.content,
    globsText: joinCommaList(parsed.globs),
    alwaysApply: parsed.alwaysApply ?? parsed.globs.length === 0,
    tagsText: joinCommaList(parsed.tags),
    order: parsed.order,
    global: source.global,
    writable: source.writable !== false,
    properties: parsed.extra,
  }
}

export function buildRuleProperties(draft: RuleDraft): Record<string, unknown> {
  return {
    ...draft.properties,
    globs: splitCommaList(draft.globsText),
    tags: splitCommaList(draft.tagsText),
    alwaysApply: draft.alwaysApply,
    order: draft.order,
  }
}

export function draftsEqual(a: RuleDraft, b: RuleDraft): boolean {
  return (
    a.path === b.path &&
    a.name === b.name &&
    a.description === b.description &&
    a.content === b.content &&
    a.alwaysApply === b.alwaysApply &&
    a.order === b.order &&
    a.global === b.global &&
    a.globsText === b.globsText &&
    a.tagsText === b.tagsText
  )
}

export function validateRuleName(name: string): string | null {
  const trimmed = name.trim()
  if (!trimmed) return 'Rule name is required.'
  if (trimmed.length > 80) return 'Rule name must be at most 80 characters.'
  if (/[\\/]/.test(trimmed)) return 'Rule name must not contain path separators.'
  return null
}
