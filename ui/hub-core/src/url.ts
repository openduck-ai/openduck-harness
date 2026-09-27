// A host without a scheme is a relative URL. fetch() resolves it against the
// /hub/ page and requests /hub/<host>/api/... instead of /api/...
export function absoluteServerBase(baseUrl: string): string {
  const base = baseUrl.trim().replace(/\/$/, '')
  if (!base || /^[a-z][a-z0-9+.-]*:/i.test(base) || base.startsWith('/')) {
    return base
  }
  const protocol =
    typeof location !== 'undefined' && location.protocol ? location.protocol : 'http:'
  const scheme = protocol.endsWith(':') ? protocol : `${protocol}:`
  return `${scheme}//${base}`
}
