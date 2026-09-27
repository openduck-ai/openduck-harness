const SLUG_PATTERN = /^[a-z0-9]+(-[a-z0-9]+)*$/
const MAX_SLUG_LENGTH = 80

export function isValidProjectSlug(slug: string): boolean {
  return slug.length > 0 && slug.length <= MAX_SLUG_LENGTH && SLUG_PATTERN.test(slug)
}

export function slugFromDirectoryName(name: string): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, MAX_SLUG_LENGTH)
    .replace(/-+$/g, '')
  return isValidProjectSlug(slug) ? slug : ''
}

export function titleFromDirectoryName(name: string): string {
  const words = name
    .trim()
    .split(/[-_\s]+/)
    .filter(Boolean)
  if (words.length === 0) return name.trim()
  return words
    .map(word => (/[A-Z]/.test(word) ? word : word.charAt(0).toUpperCase() + word.slice(1)))
    .join(' ')
}

export function uniqueSlug(base: string, taken: Iterable<string>): string {
  if (!isValidProjectSlug(base)) return ''
  const used = new Set(taken)
  if (!used.has(base)) return base
  for (let index = 2; index < 1000; index += 1) {
    const suffix = `-${index}`
    const stem = base.slice(0, MAX_SLUG_LENGTH - suffix.length).replace(/-+$/g, '')
    if (!stem) continue
    const candidate = `${stem}${suffix}`
    if (isValidProjectSlug(candidate) && !used.has(candidate)) return candidate
  }
  return ''
}

export function isProjectDirectoryName(name: string): boolean {
  if (!name || name !== name.trim() || name === '.' || name === '..' || name.length > 255) {
    return false
  }
  if (name.includes('/') || name.includes('\\') || name.includes('\0')) return false
  for (const character of name) {
    if (character.charCodeAt(0) < 32) return false
  }
  return true
}

export function directoryPathsMatch(left: string, right: string): boolean {
  const normalize = (value: string) => value.replace(/[\\/]+$/, '')
  return normalize(left) === normalize(right)
}
