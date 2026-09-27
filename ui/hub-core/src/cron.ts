export function defaultJobId(slug: string, shortName: string): string {
  const name = shortName
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '') || 'job'
  return `${slug}-${name}`
}

export function describeCron(cron: string): string {
  const parts = cron.trim().split(/\s+/)
  if (parts.length === 5 && parts[1] && parts[0]) return `daily-ish at ${parts[1]}:${parts[0].padStart(2, '0')}`
  if (parts.length === 6) return cron
  return cron
}
