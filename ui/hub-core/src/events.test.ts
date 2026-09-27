import assert from 'node:assert/strict'
import test from 'node:test'
import {
  jobLiveStreamUrl,
  projectEventsUrl,
  subscribeJobLiveStream,
  subscribeProjectEvents,
} from './events.ts'
import type { HarnessJobLiveEvent, HarnessProjectEvent } from './types.ts'

test('events urls generates projectEventsUrl with and without secret token', () => {
  assert.equal(
    projectEventsUrl('http://localhost:3000', 'my-project'),
    'http://localhost:3000/api/v1/projects/my-project/harness/events',
  )
  assert.equal(
    projectEventsUrl('http://localhost:3000/', 'my project', 'secret123'),
    'http://localhost:3000/api/v1/projects/my%20project/harness/events?token=secret123',
  )
  assert.equal(
    projectEventsUrl('1.13.3.104', 'my-project'),
    'http://1.13.3.104/api/v1/projects/my-project/harness/events',
  )
})

test('events urls generates jobLiveStreamUrl with and without secret token', () => {
  assert.equal(
    jobLiveStreamUrl('http://localhost:3000', 'my-project', 'job-1'),
    'http://localhost:3000/api/v1/projects/my-project/harness/jobs/job-1/stream',
  )
  assert.equal(
    jobLiveStreamUrl('http://localhost:3000/', 'my-project', 'job-1', 'secret123'),
    'http://localhost:3000/api/v1/projects/my-project/harness/jobs/job-1/stream?token=secret123',
  )
})

test('subscribeProjectEvents parses valid SSE messages and passes to onEvent callback', () => {
  const originalEventSource = globalThis.EventSource
  const mockListeners: Record<string, (e: any) => void> = {}
  let closed = false

  class MockEventSource {
    set onmessage(fn: any) {
      mockListeners.message = fn
    }
    set onerror(fn: any) {
      mockListeners.error = fn
    }
    close() {
      closed = true
    }
  }

  // @ts-expect-error test mock
  globalThis.EventSource = MockEventSource

  let receivedEvent: HarnessProjectEvent | null = null
  const onEvent = (e: HarnessProjectEvent) => {
    receivedEvent = e
  }

  try {
    const unsubscribe = subscribeProjectEvents(
      'http://localhost:3000',
      'token123',
      'demo-project',
      onEvent,
    )

    const eventData: HarnessProjectEvent = {
      type: 'task_status_changed',
      projectSlug: 'demo-project',
      taskId: 'task-1',
      jobId: 'job-100',
      status: 'running',
    }

    mockListeners.message?.({
      data: JSON.stringify(eventData),
    })

    assert.deepEqual(receivedEvent, eventData)

    unsubscribe()
    assert.equal(closed, true)
  } finally {
    globalThis.EventSource = originalEventSource
  }
})

test('subscribeJobLiveStream handles step_update and auto-completes on finished event', () => {
  const originalEventSource = globalThis.EventSource
  const mockListeners: Record<string, (e: any) => void> = {}
  let closed = false

  class MockEventSource {
    set onmessage(fn: any) {
      mockListeners.message = fn
    }
    set onerror(fn: any) {
      mockListeners.error = fn
    }
    close() {
      closed = true
    }
  }

  // @ts-expect-error test mock
  globalThis.EventSource = MockEventSource

  let receivedEvent: HarnessJobLiveEvent | null = null
  let completed = false
  const onEvent = (e: HarnessJobLiveEvent) => {
    receivedEvent = e
  }
  const onComplete = () => {
    completed = true
  }

  try {
    const unsubscribe = subscribeJobLiveStream(
      'http://localhost:3000',
      'token123',
      'demo-project',
      'job-100',
      onEvent,
      onComplete,
    )

    const finishEvent: HarnessJobLiveEvent = {
      type: 'finished',
      jobId: 'job-100',
      status: 'success',
      durationMs: 2500,
      finalAnswer: 'Finished successfully',
    }

    mockListeners.message?.({
      data: JSON.stringify(finishEvent),
    })

    assert.deepEqual(receivedEvent, finishEvent)
    assert.equal(closed, true)
    assert.equal(completed, true)

    unsubscribe()
  } finally {
    globalThis.EventSource = originalEventSource
  }
})
