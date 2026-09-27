import type { SessionNotification, ToolCallStatus } from '@agentclientprotocol/sdk'
import type { ChatMessage, ToolCallEntry } from './types.ts'

export function newMessageId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}

export function applyAcpSessionUpdate(
  messages: ChatMessage[],
  update: SessionNotification['update'],
): ChatMessage[] {
  if (update.sessionUpdate === 'agent_message_chunk' || update.sessionUpdate === 'user_message_chunk') {
    const role = update.sessionUpdate === 'user_message_chunk' ? 'user' : 'assistant'
    if (update.content.type !== 'text') return messages
    const next = [...messages]
    const last = next[next.length - 1]
    if (last && last.role === role && last.streaming) {
      next[next.length - 1] = { ...last, text: last.text + update.content.text, streaming: role === 'assistant' }
      return next
    }
    next.push({
      id: newMessageId(role),
      role,
      text: update.content.text,
      streaming: role === 'assistant',
      toolCalls: role === 'assistant' ? [] : undefined,
    })
    return next
  }

  if (update.sessionUpdate === 'tool_call' || update.sessionUpdate === 'tool_call_update') {
    const toolCallId = update.toolCallId
    const title = 'title' in update && typeof update.title === 'string' ? update.title : toolCallId
    const status = ('status' in update ? update.status : 'pending') as ToolCallStatus | undefined
    const next = [...messages]
    let target = -1
    for (let index = next.length - 1; index >= 0; index -= 1) {
      if (next[index]?.role === 'assistant') {
        target = index
        break
      }
    }
    if (target < 0) {
      next.push({ id: newMessageId('assistant'), role: 'assistant', text: '', toolCalls: [] })
      target = next.length - 1
    }
    const message = { ...next[target]! }
    const tools = [...(message.toolCalls ?? [])]
    const existing = tools.findIndex(tool => tool.toolCallId === toolCallId)
    const patch: ToolCallEntry = {
      toolCallId,
      title: existing >= 0 ? (title || tools[existing]!.title) : title,
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
