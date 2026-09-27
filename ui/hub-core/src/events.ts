import type { HarnessJobLiveEvent, HarnessProjectEvent } from './types.ts'
import { absoluteServerBase } from './url.ts'

export function projectEventsUrl(baseUrl: string, slug: string, secretKey = ''): string {
  const base = absoluteServerBase(baseUrl)
  const cleanSlug = encodeURIComponent(slug)
  const tokenParam = secretKey ? `?token=${encodeURIComponent(secretKey)}` : ''
  return `${base}/api/v1/projects/${cleanSlug}/harness/events${tokenParam}`
}

export function jobLiveStreamUrl(
  baseUrl: string,
  slug: string,
  jobId: string,
  secretKey = '',
): string {
  const base = absoluteServerBase(baseUrl)
  const cleanSlug = encodeURIComponent(slug)
  const cleanJobId = encodeURIComponent(jobId)
  const tokenParam = secretKey ? `?token=${encodeURIComponent(secretKey)}` : ''
  return `${base}/api/v1/projects/${cleanSlug}/harness/jobs/${cleanJobId}/stream${tokenParam}`
}

export function subscribeProjectEvents(
  baseUrl: string,
  secretKey: string,
  slug: string,
  onEvent: (event: HarnessProjectEvent) => void,
  onError?: (err: globalThis.Event) => void,
): () => void {
  if (typeof EventSource === 'undefined' || !slug) {
    return () => {}
  }
  const url = projectEventsUrl(baseUrl, slug, secretKey)
  const es = new EventSource(url)

  es.onmessage = (e: MessageEvent) => {
    try {
      const data = JSON.parse(e.data) as HarnessProjectEvent
      if (data && typeof data.type === 'string') {
        onEvent(data)
      }
    } catch {
      // Ignore unparseable or keep-alive frames
    }
  }

  if (onError) {
    es.onerror = (e: globalThis.Event) => {
      onError(e)
    }
  }

  return () => {
    es.close()
  }
}

export function subscribeJobLiveStream(
  baseUrl: string,
  secretKey: string,
  slug: string,
  jobId: string,
  onEvent: (event: HarnessJobLiveEvent) => void,
  onComplete?: () => void,
  onError?: (err: globalThis.Event) => void,
): () => void {
  if (typeof EventSource === 'undefined' || !jobId) {
    return () => {}
  }
  const url = jobLiveStreamUrl(baseUrl, slug, jobId, secretKey)
  const es = new EventSource(url)

  es.onmessage = (e: MessageEvent) => {
    try {
      const data = JSON.parse(e.data) as HarnessJobLiveEvent
      if (data && typeof data.type === 'string') {
        onEvent(data)
        if (data.type === 'finished') {
          es.close()
          onComplete?.()
        }
      }
    } catch {
      // Ignore unparseable or keep-alive frames
    }
  }

  es.onerror = (e: globalThis.Event) => {
    es.close()
    onError?.(e)
  }

  return () => {
    es.close()
  }
}
