import type { HarnessJudgmentRecord, HarnessRunResponse, HarnessTrajectoryStep } from '@aaif/goose-hub-core'

export type { HarnessJudgmentRecord }

export interface ParsedToolCall {
  id?: string
  name: string
  arguments?: Record<string, unknown> | string
}

export interface ParsedAgentAction {
  kind: 'final_answer' | 'call_tools' | 'yield_control' | 'request_input' | 'raw'
  finalAnswer?: string
  toolCalls?: ParsedToolCall[]
  yieldReason?: string
  prompt?: string
  raw: unknown
}

/**
 * Parses any agentAction representation (Rust enum serialized to JSON, camelCase, snake_case, etc.)
 * into a structured normalized format for trajectory UI rendering.
 */
export function parseAgentAction(action: unknown): ParsedAgentAction {
  if (action === null || action === undefined) {
    return { kind: 'raw', raw: action }
  }

  let obj = action
  if (typeof action === 'string') {
    const trimmed = action.trim()
    if ((trimmed.startsWith('{') && trimmed.endsWith('}')) || (trimmed.startsWith('[') && trimmed.endsWith(']'))) {
      try {
        obj = JSON.parse(trimmed)
      } catch {
        return { kind: 'raw', raw: action }
      }
    } else {
      return { kind: 'raw', raw: action }
    }
  }

  if (typeof obj !== 'object' || obj === null) {
    return { kind: 'raw', raw: action }
  }

  const record = obj as Record<string, unknown>

  // 1. FinalAnswer / final_answer / finalAnswer
  const finalAns = record.FinalAnswer ?? record.finalAnswer ?? record.final_answer
  if (finalAns !== undefined && finalAns !== null) {
    const text = typeof finalAns === 'string' ? finalAns : JSON.stringify(finalAns, null, 2)
    return { kind: 'final_answer', finalAnswer: text, raw: action }
  }
  if (record.type === 'FinalAnswer' || record.type === 'final_answer') {
    const text = record.answer ?? record.content ?? record.text ?? record.prompt
    if (text !== undefined && text !== null) {
      return {
        kind: 'final_answer',
        finalAnswer: typeof text === 'string' ? text : JSON.stringify(text, null, 2),
        raw: action,
      }
    }
  }

  // 2. CallTools / call_tools / callTools
  const tools = record.CallTools ?? record.callTools ?? record.call_tools ?? record.tools
  if (Array.isArray(tools)) {
    const toolCalls: ParsedToolCall[] = tools.map(t => parseToolCallRecord(t))
    return { kind: 'call_tools', toolCalls, raw: action }
  }

  // 3. YieldControl / yield_control / yieldControl
  const yieldCtrl = record.YieldControl ?? record.yieldControl ?? record.yield_control
  if (yieldCtrl !== undefined && yieldCtrl !== null) {
    const reason =
      typeof yieldCtrl === 'object' && yieldCtrl !== null
        ? ((yieldCtrl as Record<string, unknown>).reason as string) ?? JSON.stringify(yieldCtrl)
        : String(yieldCtrl)
    return { kind: 'yield_control', yieldReason: reason, raw: action }
  }

  // 4. RequestInput / request_input / requestInput
  const reqInput = record.RequestInput ?? record.requestInput ?? record.request_input
  if (reqInput !== undefined && reqInput !== null) {
    const prompt =
      typeof reqInput === 'object' && reqInput !== null
        ? ((reqInput as Record<string, unknown>).prompt as string) ?? JSON.stringify(reqInput)
        : String(reqInput)
    return { kind: 'request_input', prompt, raw: action }
  }

  return { kind: 'raw', raw: action }
}

/**
 * Extracts the final agent answer from a HarnessRunResponse,
 * falling back to the last FinalAnswer step in the trajectory if top-level field is absent.
 */
export function extractFinalAnswer(run: HarnessRunResponse | null | undefined): string | null {
  if (!run) return null

  if (typeof run.finalAnswer === 'string' && run.finalAnswer.trim().length > 0) {
    return run.finalAnswer
  }

  const steps = run.trajectory?.steps
  if (Array.isArray(steps)) {
    for (let i = steps.length - 1; i >= 0; i--) {
      const step = steps[i] as HarnessTrajectoryStep
      const parsed = parseAgentAction(step.agentAction)
      if (parsed.kind === 'final_answer' && parsed.finalAnswer && parsed.finalAnswer.trim().length > 0) {
        return parsed.finalAnswer
      }
    }
  }

  return null
}

/**
 * Formats tool arguments for preview display in step cards.
 */
export function formatToolArguments(args: unknown): string {
  if (args === null || args === undefined) return ''
  if (typeof args === 'string') {
    try {
      const parsed = JSON.parse(args)
      return JSON.stringify(parsed, null, 2)
    } catch {
      return args
    }
  }
  return JSON.stringify(args, null, 2)
}

const HARNESS_TOOL_PURPOSE: Record<string, string> = {
  shell: 'Execute a shell command in the sandbox',
  read_file: 'Read a file in the workspace',
  write_file: 'Write a file in the workspace',
  list_dir: 'List a workspace directory',
  grep_search: 'Search workspace files',
  consult_advisor: 'Consult an external advisor',
}

function shortToolName(name: string): string {
  const idx = name.lastIndexOf('__')
  return idx >= 0 ? name.slice(idx + 2) : name
}

function toolArgsRecord(args?: Record<string, unknown> | string): Record<string, unknown> {
  if (args === undefined || args === null) return {}
  if (typeof args === 'string') {
    try {
      const parsed = JSON.parse(args)
      return asRecord(parsed) ?? { value: args }
    } catch {
      return { value: args }
    }
  }
  return args
}

function stringArg(record: Record<string, unknown>, ...keys: string[]): string | undefined {
  for (const key of keys) {
    const value = record[key]
    if (typeof value === 'string' && value.trim()) return value.trim()
  }
  return undefined
}

function truncateToolDesc(value: string, max = 96): string {
  return value.length > max ? `${value.slice(0, max - 1)}…` : value
}

/** One-line Agent Log summary of what a tool call did. */
export function describeToolCall(name: string, args?: Record<string, unknown> | string): string {
  const tool = shortToolName(name)
  const record = toolArgsRecord(args)

  switch (tool) {
    case 'shell': {
      const command = stringArg(record, 'command')
      return command ? `Run ${truncateToolDesc(command)}` : HARNESS_TOOL_PURPOSE.shell
    }
    case 'read_file': {
      const path = stringArg(record, 'path')
      return path ? `Read ${path}` : HARNESS_TOOL_PURPOSE.read_file
    }
    case 'write_file': {
      const path = stringArg(record, 'path')
      return path ? `Write ${path}` : HARNESS_TOOL_PURPOSE.write_file
    }
    case 'list_dir': {
      const path = stringArg(record, 'path')
      return path ? `List ${path}` : 'List the workspace root'
    }
    case 'grep_search': {
      const query = stringArg(record, 'query')
      if (!query) return HARNESS_TOOL_PURPOSE.grep_search
      const path = stringArg(record, 'path')
      return path ? `Search "${truncateToolDesc(query, 64)}" in ${path}` : `Search "${truncateToolDesc(query, 72)}"`
    }
    case 'consult_advisor': {
      const advisor = stringArg(record, 'command') ?? 'advisor'
      const prompt = stringArg(record, 'prompt')
      return prompt ? `Ask ${advisor}: ${truncateToolDesc(prompt, 72)}` : `Consult ${advisor}`
    }
    default: {
      const purpose = HARNESS_TOOL_PURPOSE[tool]
      if (purpose) return purpose
      const preview =
        stringArg(record, 'path', 'command', 'query', 'prompt', 'url', 'name', 'value') ??
        Object.entries(record)
          .filter(([, value]) => typeof value === 'string' && value.trim())
          .map(([key, value]) => `${key}: ${String(value).trim()}`)[0]
      return preview ? truncateToolDesc(preview) : ''
    }
  }
}

export type TrajectoryFilterType = 'all' | 'tools' | 'final_answer' | 'yield' | 'telemetry'
export type TrajectoryViewMode = 'log' | 'split' | 'stream'

export interface AgentLogToolCall {
  id?: string
  name: string
  arguments?: Record<string, unknown> | string
  output?: string
  isError?: boolean
}

export type AgentLogTurnKind =
  | 'user'
  | 'assistant'
  | 'final_answer'
  | 'yield'
  | 'request_input'
  | 'error'

export interface AgentLogTurn {
  id: string
  role: 'user' | 'assistant'
  kind: AgentLogTurnKind
  stepIndex?: number
  timestamp?: string
  durationMs?: number
  thinking?: string
  text?: string
  toolCalls?: AgentLogToolCall[]
  promptMessages?: ParsedPromptMessage[]
  systemPrompt?: string
  lastPromptText?: string
  judgments?: HarnessJudgmentRecord[]
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : null
}

function stringField(record: Record<string, unknown> | null, ...keys: string[]): string | undefined {
  if (!record) return undefined
  for (const key of keys) {
    const value = record[key]
    if (typeof value === 'string' && value.length > 0) return value
  }
  return undefined
}

/**
 * Unwraps Goose Message toolCall serialization:
 * `{ name, arguments }`, `{ status: "success", value: { name, arguments } }`, or `{ Ok: { name } }`.
 */
export function parseToolCallRecord(value: unknown): ParsedToolCall {
  const record = asRecord(value)
  if (!record) {
    return { name: String(value) }
  }

  const nested = asRecord(record.toolCall ?? record.tool_call) ?? record
  const unwrapped = asRecord(nested.value) ?? asRecord(nested.Ok) ?? nested

  const name =
    stringField(unwrapped, 'name') ??
    stringField(nested, 'name') ??
    stringField(record, 'name') ??
    'unknown_tool'
  const id = stringField(record, 'id') ?? stringField(nested, 'id') ?? stringField(unwrapped, 'id')
  const args = unwrapped.arguments ?? unwrapped.args ?? nested.arguments ?? record.arguments ?? record.args

  return {
    id,
    name,
    arguments: args as Record<string, unknown> | string | undefined,
  }
}

export function parseToolResults(raw: unknown): AgentLogToolCall[] {
  if (!Array.isArray(raw)) return []

  return raw.map((item, index) => {
    const record = asRecord(item)
    if (!record) {
      return { name: `result_${index + 1}`, output: String(item) }
    }
    const output =
      typeof record.output === 'string'
        ? record.output
        : record.output !== undefined
          ? JSON.stringify(record.output, null, 2)
          : JSON.stringify(record, null, 2)
    return {
      id: stringField(record, 'id'),
      name: stringField(record, 'name') ?? `tool_${index + 1}`,
      output,
      isError: Boolean(record.is_error ?? record.isError),
    }
  })
}

export function parseMessageContentBlocks(content: unknown): {
  text?: string
  thinking?: string
  toolCalls: ParsedToolCall[]
  toolResults: unknown[]
} {
  const toolCalls: ParsedToolCall[] = []
  const toolResults: unknown[] = []
  const textParts: string[] = []
  const thinkingParts: string[] = []

  const consumeBlock = (part: unknown) => {
    if (typeof part === 'string') {
      textParts.push(part)
      return
    }
    const p = asRecord(part)
    if (!p) return

    const type = typeof p.type === 'string' ? p.type : ''
    if (type === 'text' && typeof p.text === 'string') {
      textParts.push(p.text)
      return
    }
    if ((type === 'thinking' || type === 'reasoning') && (typeof p.thinking === 'string' || typeof p.text === 'string')) {
      thinkingParts.push(typeof p.thinking === 'string' ? p.thinking : String(p.text))
      return
    }
    if (type === 'toolRequest' || type === 'tool_request' || p.toolCall || p.tool_call) {
      toolCalls.push(parseToolCallRecord(p))
      return
    }
    if (type === 'toolResponse' || type === 'tool_response' || p.toolResult || p.tool_result) {
      toolResults.push(p)
      return
    }
    if (typeof p.thinking === 'string') {
      thinkingParts.push(p.thinking)
    }
    if (typeof p.text === 'string') {
      textParts.push(p.text)
    }
  }

  if (typeof content === 'string') {
    textParts.push(content)
  } else if (Array.isArray(content)) {
    content.forEach(consumeBlock)
  } else if (content !== undefined && content !== null) {
    consumeBlock(content)
  }

  return {
    text: textParts.length > 0 ? textParts.join('\n') : undefined,
    thinking: thinkingParts.length > 0 ? thinkingParts.join('\n') : undefined,
    toolCalls,
    toolResults,
  }
}

function pairToolCallsWithResults(
  calls: ParsedToolCall[],
  results: AgentLogToolCall[],
): AgentLogToolCall[] {
  const unused = [...results]
  const paired: AgentLogToolCall[] = calls.map(call => {
    let matchIndex = -1
    if (call.id) {
      matchIndex = unused.findIndex(result => result.id === call.id)
    }
    if (matchIndex < 0) {
      matchIndex = unused.findIndex(result => result.name === call.name)
    }
    const match = matchIndex >= 0 ? unused.splice(matchIndex, 1)[0] : undefined
    return {
      id: call.id ?? match?.id,
      name: call.name,
      arguments: call.arguments,
      output: match?.output,
      isError: match?.isError,
    }
  })
  return [...paired, ...unused]
}

/**
 * Extracts only the newly added prompt messages for a step,
 * omitting previous conversation history from prior steps.
 */
export function extractAddedPromptMessages(
  messages: ParsedPromptMessage[] | undefined | null,
): ParsedPromptMessage[] {
  if (!messages || messages.length === 0) return []

  // Find the last assistant message in the prompt.
  // Everything after the last assistant message represents the prompt
  // (tool results, system guidance, user prompts) added for the current step.
  let lastAssistantIdx = -1
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i].role === 'assistant') {
      lastAssistantIdx = i
      break
    }
  }

  if (lastAssistantIdx >= 0) {
    return messages.slice(lastAssistantIdx + 1)
  }

  // If there are no prior assistant messages (e.g. step 1),
  // all messages are the initial prompts for this step.
  return [...messages]
}

export function extractToolResultOutput(raw: unknown): string {
  if (!raw) return ''
  if (typeof raw === 'string') return raw
  const record = asRecord(raw)
  if (!record) return String(raw)

  if (typeof record.output === 'string') {
    return record.output
  }

  const toolResult = asRecord(record.toolResult) ?? asRecord(record.tool_result) ?? record
  const unwrapped = asRecord(toolResult.Ok) ?? asRecord(toolResult.value) ?? toolResult

  if (typeof unwrapped.output === 'string') {
    return unwrapped.output
  }
  if (typeof unwrapped.text === 'string') {
    return unwrapped.text
  }

  const content = unwrapped.content
  if (typeof content === 'string') {
    return content
  }
  if (Array.isArray(content)) {
    const textPieces = content
      .map(item => {
        if (typeof item === 'string') return item
        const itemRec = asRecord(item)
        if (itemRec && typeof itemRec.text === 'string') return itemRec.text
        return undefined
      })
      .filter((t): t is string => typeof t === 'string' && t.trim().length > 0)

    if (textPieces.length > 0) {
      return textPieces.join('\n')
    }
  }

  if (toolResult.Err || toolResult.error) {
    const err = toolResult.Err ?? toolResult.error
    return `Error: ${typeof err === 'string' ? err : JSON.stringify(err, null, 2)}`
  }

  return JSON.stringify(raw, null, 2)
}

export function formatPromptMessageText(msg: ParsedPromptMessage): string {
  if (msg.text && msg.text.trim()) {
    return msg.text
  }
  if (msg.toolResults && msg.toolResults.length > 0) {
    const pieces = msg.toolResults.map(extractToolResultOutput).filter(Boolean)
    if (pieces.length > 0) {
      return pieces.join('\n\n')
    }
    return JSON.stringify(msg.toolResults, null, 2)
  }
  if (typeof msg.content === 'string') {
    return msg.content
  }
  return JSON.stringify(msg.content, null, 2)
}

/**
 * Rebuilds a chronological chat-style agent log from a project-task trajectory,
 * similar to a normal agent session transcript.
 */
export function buildAgentLog(run: HarnessRunResponse | null | undefined): AgentLogTurn[] {
  if (!run) return []

  const steps = run.trajectory?.steps ?? []
  const turns: AgentLogTurn[] = []

  let userPrompt: string | undefined
  let userTimestamp: string | undefined
  for (const step of steps) {
    const telemetry = extractLlmTelemetry(step)
    const userMessage = telemetry?.messages.find(message => message.role === 'user' && message.text?.trim())
    if (userMessage?.text) {
      userPrompt = userMessage.text
      userTimestamp = step.timestamp
      break
    }
  }
  if (!userPrompt) {
    const r = run as unknown as Record<string, unknown>
    const taskSpec = asRecord(r.taskSpec)
    userPrompt =
      stringField(r, 'problemStatement', 'problem_statement', 'prompt', 'taskPrompt') ??
      stringField(taskSpec, 'problemStatement', 'problem_statement')
  }
  if (userPrompt) {
    turns.push({
      id: 'user-prompt',
      role: 'user',
      kind: 'user',
      text: userPrompt,
      timestamp: userTimestamp ?? run.trajectory?.startedAt ?? steps[0]?.timestamp,
    })
  }

  for (const step of steps) {
    const action = parseAgentAction(step.agentAction)
    const telemetry = extractLlmTelemetry(step)
    const results = parseToolResults(step.toolResults)
    const toolCalls = pairToolCallsWithResults(action.toolCalls ?? telemetry?.modelToolCalls ?? [], results)

    let kind: AgentLogTurnKind = 'assistant'
    let text = telemetry?.completionText?.trim() || undefined

    if (action.kind === 'final_answer') {
      kind = 'final_answer'
      text = action.finalAnswer?.trim() || text
    } else if (action.kind === 'yield_control') {
      kind = 'yield'
      text = action.yieldReason || text
    } else if (action.kind === 'request_input') {
      kind = 'request_input'
      text = action.prompt || text
    } else if (action.kind === 'raw' && !text) {
      kind = 'error'
      text = typeof step.agentAction === 'string' ? step.agentAction : undefined
    }

    const llmError = asRecord(step.llmResponse)?.policyError
    if (typeof llmError === 'string' && llmError.trim()) {
      kind = 'error'
      text = llmError
    }

    const addedPrompts = extractAddedPromptMessages(telemetry?.messages)

    let lastPromptText: string | undefined
    if (addedPrompts.length > 0) {
      const incoming = [...addedPrompts].reverse().find(m => m.role !== 'assistant') ?? addedPrompts[addedPrompts.length - 1]
      if (incoming) {
        lastPromptText = formatPromptMessageText(incoming)
      }
    }

    turns.push({
      id: `step-${step.stepIndex}`,
      role: 'assistant',
      kind,
      stepIndex: step.stepIndex,
      timestamp: step.timestamp,
      durationMs: step.durationMs,
      thinking: telemetry?.thinking,
      text,
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      promptMessages: addedPrompts.length > 0 ? addedPrompts : undefined,
      systemPrompt: telemetry?.systemPrompt,
      lastPromptText,
      judgments: step.judgments && step.judgments.length > 0 ? step.judgments : undefined,
    })
  }

  return turns
}

export function filterAgentLogTurns(
  turns: AgentLogTurn[],
  filterType: TrajectoryFilterType,
  searchQuery: string,
): AgentLogTurn[] {
  const query = searchQuery.trim().toLowerCase()

  return turns.filter(turn => {
    if (filterType === 'tools' && !(turn.toolCalls && turn.toolCalls.length > 0)) {
      return false
    }
    if (filterType === 'final_answer' && turn.kind !== 'final_answer' && turn.kind !== 'user') {
      return false
    }
    if (filterType === 'yield' && turn.kind !== 'yield' && turn.kind !== 'request_input' && turn.kind !== 'user') {
      return false
    }
    if (filterType === 'telemetry' && turn.kind === 'user') {
      return Boolean(query)
    }
    if (filterType === 'telemetry' && !turn.thinking && !turn.text && !turn.toolCalls) {
      return false
    }

    if (!query) return true

    const haystacks: string[] = [
      turn.text ?? '',
      turn.thinking ?? '',
      turn.lastPromptText ?? '',
      turn.kind,
      turn.stepIndex !== undefined ? String(turn.stepIndex) : '',
      ...(turn.toolCalls ?? []).flatMap(call => [
        call.name,
        describeToolCall(call.name, call.arguments),
        call.output ?? '',
        typeof call.arguments === 'string' ? call.arguments : JSON.stringify(call.arguments ?? ''),
      ]),
    ]
    return haystacks.some(part => part.toLowerCase().includes(query))
  })
}

export interface ParsedPromptMessage {
  role: string
  content: unknown
  text?: string
  thinking?: string
  toolCalls?: ParsedToolCall[]
  toolResults?: unknown[]
  index: number
}

export interface ParsedLlmTelemetry {
  systemPrompt?: string
  messages: ParsedPromptMessage[]
  completionText?: string
  reply?: unknown
  modelToolCalls?: ParsedToolCall[]
  thinking?: string
  inputTokens?: number
  outputTokens?: number
  totalTokens?: number
  tokensPerSec?: number
  durationMs: number
  rawRequest?: unknown
  rawResponse?: unknown
}

export interface TrajectoryMetrics {
  totalSteps: number
  toolCallsCount: number
  durationMs: number
  inputTokens: number
  outputTokens: number
  totalTokens: number
  toolUsageFrequencies: Record<string, number>
  hasFinalAnswer: boolean
  finalAnswer?: string | null
  status: string
}

/**
 * Extracts and normalizes LLM telemetry from a step.
 */
export function extractLlmTelemetry(step: HarnessTrajectoryStep): ParsedLlmTelemetry | null {
  const req = (step.llmRequest ?? null) as Record<string, unknown> | null
  const res = (step.llmResponse ?? null) as Record<string, unknown> | null
  const usage = step.tokenUsage ?? null

  if (!req && !res && !usage) {
    return null
  }

  // 1. System prompt
  let systemPrompt: string | undefined
  if (req && typeof req.systemPrompt === 'string') {
    systemPrompt = req.systemPrompt
  }

  // 2. Prompt messages
  const messages: ParsedPromptMessage[] = []
  if (req && Array.isArray(req.messages)) {
    req.messages.forEach((rawMsg, mIdx) => {
      if (!rawMsg || typeof rawMsg !== 'object') {
        messages.push({
          role: 'user',
          content: rawMsg,
          text: String(rawMsg),
          index: mIdx + 1,
        })
        return
      }
      const m = rawMsg as Record<string, unknown>
      const role = String(m.role || 'user').toLowerCase()
      const content = m.content ?? m.text ?? m
      const parsed = parseMessageContentBlocks(content)

      messages.push({
        role,
        content,
        text: parsed.text,
        thinking: parsed.thinking,
        toolCalls: parsed.toolCalls.length > 0 ? parsed.toolCalls : undefined,
        toolResults: parsed.toolResults.length > 0 ? parsed.toolResults : undefined,
        index: mIdx + 1,
      })
    })
  } else if (req && req.lastMessage) {
    const rawMsg = req.lastMessage
    const content =
      typeof rawMsg === 'object' && rawMsg !== null && 'content' in (rawMsg as Record<string, unknown>)
        ? (rawMsg as Record<string, unknown>).content
        : rawMsg
    const role =
      typeof rawMsg === 'object' && rawMsg !== null && 'role' in (rawMsg as Record<string, unknown>)
        ? String((rawMsg as Record<string, unknown>).role).toLowerCase()
        : 'user'
    const parsed = parseMessageContentBlocks(content)
    messages.push({
      role,
      content,
      text: parsed.text,
      thinking: parsed.thinking,
      toolCalls: parsed.toolCalls.length > 0 ? parsed.toolCalls : undefined,
      toolResults: parsed.toolResults.length > 0 ? parsed.toolResults : undefined,
      index: 1,
    })
  }

  // 3. Response details
  let completionText: string | undefined
  let replyObj: unknown
  let responseThinking: string | undefined
  const modelToolCalls: ParsedToolCall[] = []

  if (res) {
    if (typeof res.text === 'string' && res.text.trim().length > 0) {
      completionText = res.text
    }
    if (res.reply) {
      replyObj = res.reply
      const r = asRecord(res.reply)
      const parsedReply = parseMessageContentBlocks(r?.content ?? res.reply)
      responseThinking = parsedReply.thinking
      modelToolCalls.push(...parsedReply.toolCalls)
      if (!completionText && parsedReply.text) {
        completionText = parsedReply.text
      }
    }
    if (!responseThinking && typeof res.thinking === 'string') {
      responseThinking = res.thinking
    }
  }

  // 4. Token usage & speed
  const inputTokens = usage?.inputTokens ?? undefined
  const outputTokens = usage?.outputTokens ?? undefined
  const totalTokens =
    usage?.totalTokens ??
    (inputTokens !== undefined || outputTokens !== undefined
      ? (inputTokens || 0) + (outputTokens || 0)
      : undefined)

  let tokensPerSec: number | undefined
  if (outputTokens && step.durationMs > 0) {
    tokensPerSec = Number(((outputTokens / step.durationMs) * 1000).toFixed(1))
  }

  return {
    systemPrompt,
    messages,
    completionText,
    reply: replyObj,
    modelToolCalls: modelToolCalls.length > 0 ? modelToolCalls : undefined,
    thinking: responseThinking,
    inputTokens,
    outputTokens,
    totalTokens,
    tokensPerSec,
    durationMs: step.durationMs,
    rawRequest: req,
    rawResponse: res,
  }
}

export interface FilteredTrajectoryStep {
  step: HarnessTrajectoryStep
  originalIndex: number
  parsedAction: ParsedAgentAction
  telemetry: ParsedLlmTelemetry | null
}

/**
 * Filters and searches trajectory steps.
 */
export function filterTrajectorySteps(
  steps: HarnessTrajectoryStep[] | undefined | null,
  filterType: TrajectoryFilterType,
  searchQuery: string,
): FilteredTrajectoryStep[] {
  if (!Array.isArray(steps) || steps.length === 0) return []

  const query = searchQuery.trim().toLowerCase()

  const results: FilteredTrajectoryStep[] = []

  steps.forEach((step, originalIndex) => {
    const parsedAction = parseAgentAction(step.agentAction)
    const telemetry = extractLlmTelemetry(step)

    // Check filter type
    if (filterType === 'tools' && parsedAction.kind !== 'call_tools') {
      return
    }
    if (filterType === 'final_answer' && parsedAction.kind !== 'final_answer') {
      return
    }
    if (filterType === 'yield' && parsedAction.kind !== 'yield_control' && parsedAction.kind !== 'request_input') {
      return
    }
    if (filterType === 'telemetry' && !telemetry) {
      return
    }

    // Check search query
    if (query) {
      let matches = false

      // Match step index
      if (String(step.stepIndex) === query || `step #${step.stepIndex}`.toLowerCase().includes(query)) {
        matches = true
      }

      // Match tool names and args
      if (!matches && parsedAction.toolCalls) {
        for (const call of parsedAction.toolCalls) {
          if (call.name.toLowerCase().includes(query)) {
            matches = true
            break
          }
          if (call.arguments) {
            const argStr = typeof call.arguments === 'string' ? call.arguments : JSON.stringify(call.arguments)
            if (argStr.toLowerCase().includes(query)) {
              matches = true
              break
            }
          }
        }
      }

      // Match final answer
      if (!matches && parsedAction.finalAnswer && parsedAction.finalAnswer.toLowerCase().includes(query)) {
        matches = true
      }

      // Match yield reason / prompt
      if (!matches && parsedAction.yieldReason && parsedAction.yieldReason.toLowerCase().includes(query)) {
        matches = true
      }
      if (!matches && parsedAction.prompt && parsedAction.prompt.toLowerCase().includes(query)) {
        matches = true
      }

      // Match LLM telemetry content
      if (!matches && telemetry) {
        if (telemetry.completionText && telemetry.completionText.toLowerCase().includes(query)) {
          matches = true
        }
        if (!matches && telemetry.thinking && telemetry.thinking.toLowerCase().includes(query)) {
          matches = true
        }
        if (!matches && telemetry.messages) {
          for (const msg of telemetry.messages) {
            if (msg.text && msg.text.toLowerCase().includes(query)) {
              matches = true
              break
            }
          }
        }
      }

      if (!matches) {
        return
      }
    }

    results.push({
      step,
      originalIndex,
      parsedAction,
      telemetry,
    })
  })

  return results
}

/**
 * Calculates aggregate trajectory metrics across all steps.
 */
export function calculateTrajectoryMetrics(
  run: HarnessRunResponse | null | undefined,
): TrajectoryMetrics {
  const steps = run?.trajectory?.steps ?? []
  let inputTokens = 0
  let outputTokens = 0
  let totalTokens = 0
  const toolUsageFrequencies: Record<string, number> = {}

  for (const step of steps) {
    if (step.tokenUsage) {
      if (step.tokenUsage.inputTokens) inputTokens += step.tokenUsage.inputTokens
      if (step.tokenUsage.outputTokens) outputTokens += step.tokenUsage.outputTokens
      if (step.tokenUsage.totalTokens) {
        totalTokens += step.tokenUsage.totalTokens
      } else {
        totalTokens += (step.tokenUsage.inputTokens || 0) + (step.tokenUsage.outputTokens || 0)
      }
    }

    const action = parseAgentAction(step.agentAction)
    if (action.toolCalls) {
      for (const call of action.toolCalls) {
        toolUsageFrequencies[call.name] = (toolUsageFrequencies[call.name] || 0) + 1
      }
    }
  }

  const finalAnswer = extractFinalAnswer(run)

  return {
    totalSteps: run?.stepCount ?? steps.length,
    toolCallsCount: run?.toolCallsCount ?? Object.values(toolUsageFrequencies).reduce((a, b) => a + b, 0),
    durationMs: run?.durationMs ?? 0,
    inputTokens,
    outputTokens,
    totalTokens,
    toolUsageFrequencies,
    hasFinalAnswer: Boolean(finalAnswer),
    finalAnswer,
    status: String(run?.status || 'Unknown'),
  }
}

/**
 * Triggers a browser download of the trajectory run as formatted JSON.
 */
export function isLiveTrajectoryVisible(
  jobId: string,
  inspectingJobId: string | null,
  liveInspect: boolean,
  viewingTrajectory: boolean,
): boolean {
  return liveInspect && inspectingJobId === jobId && viewingTrajectory
}

export function downloadTrajectoryJson(run: HarnessRunResponse): void {
  const payload = JSON.stringify(run, null, 2)
  const blob = new Blob([payload], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  const dateStr = new Date().toISOString().replace(/[:.]/g, '-')
  a.href = url
  a.download = `trajectory-${run.taskId}-${dateStr}.json`
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  URL.revokeObjectURL(url)
}

export interface AutoScrollEvaluation {
  autoScroll: boolean
  pausedByUserScroll: boolean
}

/**
 * Determines whether auto-scroll should be active based on user scroll position
 * and explicit user preference.
 */
export function evaluateAutoScrollState(params: {
  distanceFromBottom: number
  currentAutoScroll: boolean
  userExplicitlyDisabled: boolean
  scrollUpThreshold?: number
  bottomThreshold?: number
}): AutoScrollEvaluation {
  const scrollUpThreshold = params.scrollUpThreshold ?? 60
  const bottomThreshold = params.bottomThreshold ?? 20

  if (params.userExplicitlyDisabled) {
    return { autoScroll: false, pausedByUserScroll: false }
  }

  if (params.distanceFromBottom > scrollUpThreshold) {
    return { autoScroll: false, pausedByUserScroll: true }
  }

  if (params.distanceFromBottom <= bottomThreshold) {
    return { autoScroll: true, pausedByUserScroll: false }
  }

  return {
    autoScroll: params.currentAutoScroll,
    pausedByUserScroll: !params.currentAutoScroll,
  }
}

