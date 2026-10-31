import type { SessionNotification, ToolCallStatus } from '@agentclientprotocol/sdk'
import type { ChatMessage, ToolCallEntry } from './types.ts'

export function newMessageId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}

function brandMessageId(value: unknown): string | undefined {
  if (!value || typeof value !== 'object') return undefined
  const record = value as Record<string, unknown>
  for (const key of ['goose', 'openduck']) {
    const brand = record[key]
    if (!brand || typeof brand !== 'object') continue
    const id = (brand as Record<string, unknown>).messageId
    if (typeof id === 'string' && id) return id
  }
  return undefined
}

/** ACP `session/load` replay and live turns share `messageId` across chunks of one message. */
export function chunkMessageId(update: {
  messageId?: string | null
  _meta?: { [key: string]: unknown } | null
}): string | undefined {
  if (typeof update.messageId === 'string' && update.messageId) return update.messageId
  return brandMessageId(update._meta)
}

export function applyAcpSessionUpdate(
  messages: ChatMessage[],
  update: SessionNotification['update'],
): ChatMessage[] {
  if (update.sessionUpdate === 'agent_message_chunk' || update.sessionUpdate === 'user_message_chunk') {
    const role = update.sessionUpdate === 'user_message_chunk' ? 'user' : 'assistant'
    if (update.content.type !== 'text') return messages
    const messageId = chunkMessageId(update)
    const next = [...messages]
    if (messageId) {
      const existingIndex = next.findIndex(message => message.id === messageId)
      if (existingIndex >= 0) {
        const existing = next[existingIndex]!
        next[existingIndex] = {
          ...existing,
          text: existing.text + update.content.text,
          streaming: role === 'assistant',
        }
        return next
      }
    } else {
      const last = next[next.length - 1]
      if (last && last.role === role && last.streaming) {
        next[next.length - 1] = {
          ...last,
          text: last.text + update.content.text,
          streaming: role === 'assistant',
        }
        return next
      }
    }
    next.push({
      id: messageId ?? newMessageId(role),
      role,
      text: update.content.text,
      streaming: role === 'assistant',
      toolCalls: role === 'assistant' ? [] : undefined,
    })
    return next
  }

  if (update.sessionUpdate === 'tool_call' || update.sessionUpdate === 'tool_call_update') {
    const toolCallId = update.toolCallId
    const explicitTitle =
      'title' in update && typeof update.title === 'string' && update.title ? update.title : undefined
    const status = ('status' in update ? update.status : undefined) as ToolCallStatus | undefined
    const ownerId = chunkMessageId(update)
    const next = [...messages]
    let target = ownerId ? next.findIndex(message => message.id === ownerId && message.role === 'assistant') : -1
    if (target < 0) {
      for (let index = next.length - 1; index >= 0; index -= 1) {
        if (next[index]?.role === 'assistant') {
          target = index
          break
        }
      }
    }
    if (target < 0) {
      next.push({
        id: ownerId ?? newMessageId('assistant'),
        role: 'assistant',
        text: '',
        toolCalls: [],
      })
      target = next.length - 1
    }
    const message = { ...next[target]! }
    const tools = [...(message.toolCalls ?? [])]
    const existing = tools.findIndex(tool => tool.toolCallId === toolCallId)
    const patch: ToolCallEntry = {
      toolCallId,
      title: explicitTitle ?? (existing >= 0 ? tools[existing]!.title : toolCallId),
      status: status ?? tools[existing]?.status ?? 'pending',
      kind: ('kind' in update ? update.kind : tools[existing]?.kind) ?? undefined,
    }
    if (existing >= 0) tools[existing] = { ...tools[existing]!, ...patch }
    else tools.push(patch)
    message.toolCalls = tools
    next[target] = message
    return next
  }

  return messages
}

export function finalizeStreaming(messages: ChatMessage[]): ChatMessage[] {
  return messages.map(message => (message.streaming ? { ...message, streaming: false } : message))
}
