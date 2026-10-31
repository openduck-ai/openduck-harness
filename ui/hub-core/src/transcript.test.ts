import test from 'node:test'
import assert from 'node:assert/strict'
import type { SessionNotification } from '@agentclientprotocol/sdk'
import { applyAcpSessionUpdate, finalizeStreaming } from './transcript.ts'
import type { ChatMessage } from './types.ts'

function userChunk(text: string, messageId: string): SessionNotification['update'] {
  return {
    sessionUpdate: 'user_message_chunk',
    content: { type: 'text', text },
    messageId,
  }
}

function agentChunk(text: string, messageId: string): SessionNotification['update'] {
  return {
    sessionUpdate: 'agent_message_chunk',
    content: { type: 'text', text },
    messageId,
  }
}

test('replayed session chunks become the conversation transcript', () => {
  let messages: ChatMessage[] = []
  for (const update of [
    userChunk('hello', 'm1'),
    agentChunk('hi ', 'm2'),
    agentChunk('there', 'm2'),
    userChunk('next', 'm3'),
    agentChunk('done', 'm4'),
  ]) {
    messages = applyAcpSessionUpdate(messages, update)
  }
  messages = finalizeStreaming(messages)

  assert.equal(messages.length, 4)
  assert.deepEqual(
    messages.map(message => [message.role, message.text, message.streaming]),
    [
      ['user', 'hello', false],
      ['assistant', 'hi there', false],
      ['user', 'next', false],
      ['assistant', 'done', false],
    ],
  )
  assert.equal(messages[0]?.id, 'm1')
  assert.equal(messages[1]?.id, 'm2')
})

test('replayed tool calls stay on the assistant turn that owns them', () => {
  let messages: ChatMessage[] = []
  messages = applyAcpSessionUpdate(messages, agentChunk('working', 'a1'))
  messages = applyAcpSessionUpdate(messages, {
    sessionUpdate: 'tool_call',
    toolCallId: 't1',
    title: 'read file',
    status: 'pending',
    _meta: { goose: { messageId: 'a1' } },
  })
  messages = applyAcpSessionUpdate(messages, {
    sessionUpdate: 'tool_call_update',
    toolCallId: 't1',
    status: 'completed',
    _meta: { openduck: { messageId: 'a1' } },
  })

  assert.equal(messages.length, 1)
  assert.equal(messages[0]?.text, 'working')
  assert.equal(messages[0]?.toolCalls?.length, 1)
  assert.equal(messages[0]?.toolCalls?.[0]?.title, 'read file')
  assert.equal(messages[0]?.toolCalls?.[0]?.status, 'completed')
})

test('live assistant chunks without messageId still stream together', () => {
  let messages = applyAcpSessionUpdate([], {
    sessionUpdate: 'agent_message_chunk',
    content: { type: 'text', text: 'Hel' },
  })
  messages = applyAcpSessionUpdate(messages, {
    sessionUpdate: 'agent_message_chunk',
    content: { type: 'text', text: 'lo' },
  })
  assert.equal(messages.length, 1)
  assert.equal(messages[0]?.text, 'Hello')
  assert.equal(messages[0]?.streaming, true)
})
