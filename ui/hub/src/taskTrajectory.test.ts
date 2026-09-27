import assert from 'node:assert/strict'
import test from 'node:test'
import {
  buildAgentLog,
  calculateTrajectoryMetrics,
  extractFinalAnswer,
  extractLlmTelemetry,
  filterAgentLogTurns,
  filterTrajectorySteps,
  formatToolArguments,
  describeToolCall,
  isLiveTrajectoryVisible,
  parseAgentAction,
  parseToolCallRecord,
  parseToolResults,
  evaluateAutoScrollState,
  extractAddedPromptMessages,
  extractToolResultOutput,
  formatPromptMessageText,
} from './taskTrajectory.ts'
import type { HarnessRunResponse } from '@aaif/goose-hub-core'

test('parseAgentAction handles Rust serde FinalAnswer format', () => {
  const action1 = { FinalAnswer: 'Task completed successfully with tests passing.' }
  const parsed1 = parseAgentAction(action1)
  assert.equal(parsed1.kind, 'final_answer')
  assert.equal(parsed1.finalAnswer, 'Task completed successfully with tests passing.')

  const action2 = JSON.stringify({ FinalAnswer: 'JSON stringified answer' })
  const parsed2 = parseAgentAction(action2)
  assert.equal(parsed2.kind, 'final_answer')
  assert.equal(parsed2.finalAnswer, 'JSON stringified answer')
})

test('parseAgentAction handles camelCase, snake_case, and type-tagged final answers', () => {
  const camel = { finalAnswer: 'camelCase answer' }
  assert.equal(parseAgentAction(camel).kind, 'final_answer')
  assert.equal(parseAgentAction(camel).finalAnswer, 'camelCase answer')

  const snake = { final_answer: 'snake_case answer' }
  assert.equal(parseAgentAction(snake).kind, 'final_answer')
  assert.equal(parseAgentAction(snake).finalAnswer, 'snake_case answer')

  const tagged = { type: 'FinalAnswer', content: 'tagged content' }
  assert.equal(parseAgentAction(tagged).kind, 'final_answer')
  assert.equal(parseAgentAction(tagged).finalAnswer, 'tagged content')
})

test('parseAgentAction handles CallTools', () => {
  const action = {
    CallTools: [
      { name: 'read_file', arguments: { path: 'src/main.rs' } },
      { name: 'shell', arguments: { command: 'cargo test' } },
    ],
  }
  const parsed = parseAgentAction(action)
  assert.equal(parsed.kind, 'call_tools')
  assert.equal(parsed.toolCalls?.length, 2)
  assert.equal(parsed.toolCalls?.[0].name, 'read_file')
  assert.deepEqual(parsed.toolCalls?.[0].arguments, { path: 'src/main.rs' })
})

test('parseAgentAction handles YieldControl and RequestInput', () => {
  const yieldAction = { YieldControl: { reason: 'TurnBudgetExceeded' } }
  const parsedYield = parseAgentAction(yieldAction)
  assert.equal(parsedYield.kind, 'yield_control')
  assert.equal(parsedYield.yieldReason, 'TurnBudgetExceeded')

  const reqAction = { RequestInput: { prompt: 'Do you want to apply this change?' } }
  const parsedReq = parseAgentAction(reqAction)
  assert.equal(parsedReq.kind, 'request_input')
  assert.equal(parsedReq.prompt, 'Do you want to apply this change?')
})

test('extractFinalAnswer prioritizes top-level finalAnswer', () => {
  const run: HarnessRunResponse = {
    taskId: 'task-1',
    status: 'Success',
    stepCount: 2,
    toolCallsCount: 1,
    durationMs: 1200,
    finalAnswer: 'Top level answer',
    trajectory: {
      taskId: 'task-1',
      policyName: 'agent',
      startedAt: '2026-08-30T00:00:00Z',
      completedAt: '2026-08-30T00:00:02Z',
      steps: [
        {
          stepIndex: 1,
          timestamp: '2026-08-30T00:00:01Z',
          durationMs: 500,
          agentAction: { FinalAnswer: 'Step answer' },
          toolResults: undefined,
        },
      ],
    },
  }

  assert.equal(extractFinalAnswer(run), 'Top level answer')
})

test('extractFinalAnswer falls back to latest step FinalAnswer if top-level is absent', () => {
  const run: HarnessRunResponse = {
    taskId: 'task-2',
    status: 'Success',
    stepCount: 2,
    toolCallsCount: 1,
    durationMs: 1200,
    finalAnswer: null,
    trajectory: {
      taskId: 'task-2',
      policyName: 'agent',
      startedAt: '2026-08-30T00:00:00Z',
      completedAt: '2026-08-30T00:00:02Z',
      steps: [
        {
          stepIndex: 1,
          timestamp: '2026-08-30T00:00:01Z',
          durationMs: 500,
          agentAction: { CallTools: [{ name: 'read_file' }] },
          toolResults: undefined,
        },
        {
          stepIndex: 2,
          timestamp: '2026-08-30T00:00:02Z',
          durationMs: 500,
          agentAction: { FinalAnswer: 'Fallback answer from last step' },
          toolResults: undefined,
        },
      ],
    },
  }

  assert.equal(extractFinalAnswer(run), 'Fallback answer from last step')
})

test('formatToolArguments handles objects and strings', () => {
  assert.equal(formatToolArguments({ a: 1 }), '{\n  "a": 1\n}')
  assert.equal(formatToolArguments('{"key":"value"}'), '{\n  "key": "value"\n}')
  assert.equal(formatToolArguments('plain text'), 'plain text')
  assert.equal(formatToolArguments(null), '')
})

test('describeToolCall summarizes harness tools from arguments', () => {
  assert.equal(describeToolCall('shell', { command: 'cargo clippy' }), 'Run cargo clippy')
  assert.equal(describeToolCall('read_file', { path: 'src/main.rs' }), 'Read src/main.rs')
  assert.equal(describeToolCall('write_file', { path: 'src/lib.rs' }), 'Write src/lib.rs')
  assert.equal(describeToolCall('list_dir', {}), 'List the workspace root')
  assert.equal(describeToolCall('list_dir', { path: 'crates/' }), 'List crates/')
  assert.equal(describeToolCall('grep_search', { query: 'TODO' }), 'Search "TODO"')
  assert.equal(
    describeToolCall('grep_search', { query: 'AgentLog', path: 'ui/hub' }),
    'Search "AgentLog" in ui/hub',
  )
  assert.equal(
    describeToolCall('consult_advisor', { command: 'grok', prompt: 'Review the trajectory parser' }),
    'Ask grok: Review the trajectory parser',
  )
  assert.equal(describeToolCall('shell', {}), 'Execute a shell command in the sandbox')
  assert.equal(
    describeToolCall('developer__shell', '{"command":"ls -la"}'),
    'Run ls -la',
  )
  assert.equal(describeToolCall('mystery_tool', { url: 'https://example.com' }), 'https://example.com')
  assert.equal(describeToolCall('mystery_tool', {}), '')

  const longCommand = 'a'.repeat(120)
  const described = describeToolCall('shell', { command: longCommand })
  assert.equal(described.startsWith('Run '), true)
  assert.equal(described.endsWith('…'), true)
  assert.ok(described.length <= 100)
})

test('extractLlmTelemetry parses prompt messages, thinking, tool calls, and token metrics', () => {
  const step = {
    stepIndex: 1,
    timestamp: '2026-08-30T00:00:01Z',
    durationMs: 400,
    agentAction: { CallTools: [{ name: 'read_file', arguments: { path: 'file.txt' } }] },
    tokenUsage: {
      inputTokens: 150,
      outputTokens: 50,
      totalTokens: 200,
    },
    llmRequest: {
      systemPrompt: 'You are an AI assistant.',
      messages: [
        { role: 'user', content: 'Please inspect the repository.' },
        {
          role: 'assistant',
          content: [
            { type: 'thinking', thinking: 'Let me think about how to inspect.' },
            { type: 'toolRequest', toolCall: { name: 'read_file', arguments: { path: 'file.txt' } } },
          ],
        },
      ],
    },
    llmResponse: {
      text: 'Inspecting repository now.',
      reply: {
        content: [
          { type: 'thinking', thinking: 'Found the file.' },
          { type: 'toolRequest', toolCall: { name: 'read_file', arguments: { path: 'file.txt' } } },
        ],
      },
    },
  }

  const telemetry = extractLlmTelemetry(step)
  assert.ok(telemetry !== null)
  if (!telemetry) return
  assert.equal(telemetry.systemPrompt, 'You are an AI assistant.')
  assert.equal(telemetry.messages.length, 2)
  assert.equal(telemetry.messages[0].role, 'user')
  assert.equal(telemetry.messages[0].text, 'Please inspect the repository.')
  assert.equal(telemetry.messages[1].role, 'assistant')
  assert.equal(telemetry.messages[1].thinking, 'Let me think about how to inspect.')
  assert.equal(telemetry.messages[1].toolCalls?.length, 1)
  assert.equal(telemetry.messages[1].toolCalls?.[0].name, 'read_file')

  assert.equal(telemetry.completionText, 'Inspecting repository now.')
  assert.equal(telemetry.thinking, 'Found the file.')
  assert.equal(telemetry.modelToolCalls?.length, 1)
  assert.equal(telemetry.modelToolCalls?.[0].name, 'read_file')
  assert.equal(telemetry.inputTokens, 150)
  assert.equal(telemetry.outputTokens, 50)
  assert.equal(telemetry.totalTokens, 200)
  assert.equal(telemetry.tokensPerSec, 125) // (50 / 400) * 1000 = 125 tok/s
})

test('filterTrajectorySteps filters by tools, final answer, and text search', () => {
  const steps = [
    {
      stepIndex: 1,
      timestamp: '2026-08-30T00:00:01Z',
      durationMs: 300,
      agentAction: { CallTools: [{ name: 'read_file', arguments: { path: 'package.json' } }] },
      llmRequest: { messages: [{ role: 'user', content: 'check dependencies' }] },
    },
    {
      stepIndex: 2,
      timestamp: '2026-08-30T00:00:02Z',
      durationMs: 400,
      agentAction: { CallTools: [{ name: 'execute_command', arguments: { cmd: 'cargo test' } }] },
      llmRequest: { messages: [{ role: 'user', content: 'run cargo test' }] },
    },
    {
      stepIndex: 3,
      timestamp: '2026-08-30T00:00:03Z',
      durationMs: 200,
      agentAction: { FinalAnswer: 'All cargo tests passed!' },
      llmResponse: { text: 'All cargo tests passed!' },
    },
  ]

  // Filter: tools only
  const toolSteps = filterTrajectorySteps(steps, 'tools', '')
  assert.equal(toolSteps.length, 2)
  assert.equal(toolSteps[0].step.stepIndex, 1)
  assert.equal(toolSteps[1].step.stepIndex, 2)

  // Filter: final answer only
  const finalSteps = filterTrajectorySteps(steps, 'final_answer', '')
  assert.equal(finalSteps.length, 1)
  assert.equal(finalSteps[0].step.stepIndex, 3)

  // Search by tool name 'cargo'
  const searchCargo = filterTrajectorySteps(steps, 'all', 'cargo')
  assert.equal(searchCargo.length, 2) // Step 2 (tool arguments) and Step 3 (final answer)

  // Search by step index '3'
  const searchStep3 = filterTrajectorySteps(steps, 'all', '3')
  assert.equal(searchStep3.length, 1)
  assert.equal(searchStep3[0].step.stepIndex, 3)
})

test('calculateTrajectoryMetrics aggregates tokens, duration, and tool usage frequencies', () => {
  const run: HarnessRunResponse = {
    taskId: 'task-test',
    status: 'Success',
    stepCount: 2,
    toolCallsCount: 3,
    durationMs: 1500,
    finalAnswer: 'Finished successfully',
    trajectory: {
      taskId: 'task-test',
      policyName: 'default-policy',
      startedAt: '2026-08-30T00:00:00Z',
      completedAt: '2026-08-30T00:00:02Z',
      steps: [
        {
          stepIndex: 1,
          timestamp: '2026-08-30T00:00:01Z',
          durationMs: 700,
          agentAction: {
            CallTools: [
              { name: 'read_file', arguments: { path: 'src/main.rs' } },
              { name: 'read_file', arguments: { path: 'src/lib.rs' } },
            ],
          },
          tokenUsage: { inputTokens: 500, outputTokens: 100, totalTokens: 600 },
        },
        {
          stepIndex: 2,
          timestamp: '2026-08-30T00:00:02Z',
          durationMs: 800,
          agentAction: {
            CallTools: [{ name: 'edit_file', arguments: { path: 'src/main.rs' } }],
          },
          tokenUsage: { inputTokens: 400, outputTokens: 200, totalTokens: 600 },
        },
      ],
    },
  }

  const metrics = calculateTrajectoryMetrics(run)
  assert.equal(metrics.totalSteps, 2)
  assert.equal(metrics.inputTokens, 900)
  assert.equal(metrics.outputTokens, 300)
  assert.equal(metrics.totalTokens, 1200)
  assert.equal(metrics.durationMs, 1500)
  assert.equal(metrics.toolUsageFrequencies['read_file'], 2)
  assert.equal(metrics.toolUsageFrequencies['edit_file'], 1)
  assert.equal(metrics.hasFinalAnswer, true)
})

test('isLiveTrajectoryVisible is true only for the inspected job on the trajectory view', () => {
  assert.equal(isLiveTrajectoryVisible('job-1', 'job-1', true, true), true)
  assert.equal(isLiveTrajectoryVisible('job-1', 'job-1', true, false), false)
  assert.equal(isLiveTrajectoryVisible('job-1', 'job-2', true, true), false)
  assert.equal(isLiveTrajectoryVisible('job-1', 'job-1', false, true), false)
  assert.equal(isLiveTrajectoryVisible('job-1', null, true, true), false)
})

test('parseToolCallRecord unwraps Goose toolCall success envelopes', () => {
  const nested = parseToolCallRecord({
    type: 'toolRequest',
    id: 'call-1',
    toolCall: {
      status: 'success',
      value: { name: 'read_file', arguments: { path: 'src/lib.rs' } },
    },
  })
  assert.equal(nested.id, 'call-1')
  assert.equal(nested.name, 'read_file')
  assert.deepEqual(nested.arguments, { path: 'src/lib.rs' })
})

test('parseToolResults accepts snake_case and camelCase error flags', () => {
  const results = parseToolResults([
    { id: 'a', name: 'shell', output: 'ok', is_error: false },
    { id: 'b', name: 'shell', output: 'boom', isError: true },
  ])
  assert.equal(results.length, 2)
  assert.equal(results[0].isError, false)
  assert.equal(results[1].isError, true)
  assert.equal(results[1].output, 'boom')
})

test('extractLlmTelemetry reads tool names from toolCall.value', () => {
  const telemetry = extractLlmTelemetry({
    stepIndex: 1,
    timestamp: '2026-09-14T00:00:01Z',
    durationMs: 200,
    agentAction: { CallTools: [{ id: 'c1', name: 'shell', arguments: { command: 'ls' } }] },
    llmResponse: {
      reply: {
        content: [
          { type: 'thinking', thinking: 'Listing files first.' },
          {
            type: 'toolRequest',
            id: 'c1',
            toolCall: { status: 'success', value: { name: 'shell', arguments: { command: 'ls' } } },
          },
        ],
      },
    },
  })
  assert.equal(telemetry?.thinking, 'Listing files first.')
  assert.equal(telemetry?.modelToolCalls?.[0].name, 'shell')
  assert.equal(telemetry?.modelToolCalls?.[0].id, 'c1')
})

test('buildAgentLog reconstructs a chat-style task transcript', () => {
  const run: HarnessRunResponse = {
    taskId: 'fix-lint',
    status: 'Success',
    stepCount: 2,
    toolCallsCount: 1,
    durationMs: 900,
    finalAnswer: 'Lint is clean.',
    trajectory: {
      taskId: 'fix-lint',
      policyName: 'agent',
      startedAt: '2026-09-14T00:00:00Z',
      completedAt: '2026-09-14T00:00:02Z',
      steps: [
        {
          stepIndex: 1,
          timestamp: '2026-09-14T00:00:01Z',
          durationMs: 400,
          agentAction: {
            CallTools: [{ id: 't1', name: 'shell', arguments: { command: 'cargo clippy' } }],
          },
          toolResults: [{ id: 't1', name: 'shell', output: 'Finished', is_error: false }],
          llmRequest: {
            messages: [{ role: 'user', content: 'Fix clippy warnings in this crate.' }],
          },
          llmResponse: {
            text: 'I will run clippy first.',
            reply: {
              content: [{ type: 'thinking', thinking: 'Need the current lint output.' }],
            },
          },
        },
        {
          stepIndex: 2,
          timestamp: '2026-09-14T00:00:02Z',
          durationMs: 500,
          agentAction: { FinalAnswer: 'Lint is clean.' },
          llmResponse: { text: 'Lint is clean.' },
        },
      ],
    },
  }

  const log = buildAgentLog(run)
  assert.equal(log.length, 3)
  assert.equal(log[0].kind, 'user')
  assert.equal(log[0].text, 'Fix clippy warnings in this crate.')
  assert.equal(log[1].kind, 'assistant')
  assert.equal(log[1].thinking, 'Need the current lint output.')
  assert.equal(log[1].text, 'I will run clippy first.')
  assert.equal(log[1].toolCalls?.[0].name, 'shell')
  assert.equal(log[1].toolCalls?.[0].output, 'Finished')
  assert.equal(log[1].toolCalls?.[0].isError, false)
  assert.equal(log[2].kind, 'final_answer')
  assert.equal(log[2].text, 'Lint is clean.')

  const toolsOnly = filterAgentLogTurns(log, 'tools', '')
  assert.equal(toolsOnly.length, 1)
  assert.equal(toolsOnly[0].stepIndex, 1)

  const searched = filterAgentLogTurns(log, 'all', 'clippy')
  assert.ok(searched.length >= 2)
})

test('extractAddedPromptMessages isolates added prompts of current step from history', () => {
  // Step 1: No previous assistant messages -> returns all initial messages
  const step1Messages = [
    { index: 1, role: 'user', content: 'Fix the bug in main.rs', text: 'Fix the bug in main.rs' },
  ]
  const step1Added = extractAddedPromptMessages(step1Messages)
  assert.equal(step1Added.length, 1)
  assert.equal(step1Added[0].text, 'Fix the bug in main.rs')

  // Step 2: Cumulative messages (User, Assistant, Tool result) -> returns only Tool result
  const step2Messages = [
    { index: 1, role: 'user', content: 'Fix the bug in main.rs', text: 'Fix the bug in main.rs' },
    { index: 2, role: 'assistant', content: 'I will list directory', text: 'I will list directory' },
    { index: 3, role: 'tool', content: 'Cargo.toml\nsrc/main.rs', text: 'Cargo.toml\nsrc/main.rs' },
  ]
  const step2Added = extractAddedPromptMessages(step2Messages)
  assert.equal(step2Added.length, 1)
  assert.equal(step2Added[0].role, 'tool')
  assert.equal(step2Added[0].text, 'Cargo.toml\nsrc/main.rs')

  // Step 3: Multiple tool results and system guidance after assistant
  const step3Messages = [
    { index: 1, role: 'user', content: 'Fix the bug in main.rs', text: 'Fix the bug in main.rs' },
    { index: 2, role: 'assistant', content: 'I will list directory', text: 'I will list directory' },
    { index: 3, role: 'tool', content: 'Cargo.toml\nsrc/main.rs', text: 'Cargo.toml\nsrc/main.rs' },
    { index: 4, role: 'assistant', content: 'I will read main.rs', text: 'I will read main.rs' },
    { index: 5, role: 'tool', content: 'fn main() {}', text: 'fn main() {}' },
    { index: 6, role: 'system', content: 'Anti-stagnation nudge: proceed to test', text: 'Anti-stagnation nudge: proceed to test' },
  ]
  const step3Added = extractAddedPromptMessages(step3Messages)
  assert.equal(step3Added.length, 2)
  assert.equal(step3Added[0].role, 'tool')
  assert.equal(step3Added[0].text, 'fn main() {}')
  assert.equal(step3Added[1].role, 'system')
  assert.equal(step3Added[1].text, 'Anti-stagnation nudge: proceed to test')

  // Empty / null edge cases
  assert.deepEqual(extractAddedPromptMessages([]), [])
  assert.deepEqual(extractAddedPromptMessages(null), [])
  assert.deepEqual(extractAddedPromptMessages(undefined), [])
})

test('formatPromptMessageText extracts clean text from tool results', () => {
  // Plain text
  assert.equal(
    formatPromptMessageText({ index: 1, role: 'user', content: 'hello', text: 'hello' }),
    'hello',
  )

  // MCP toolResponse with text content block
  const toolResponseBlock = {
    type: 'toolResponse',
    id: 'call_1',
    toolResult: {
      Ok: {
        content: [{ type: 'text', text: 'All 24 tests passed!' }],
      },
    },
  }
  assert.equal(
    formatPromptMessageText({
      index: 1,
      role: 'user',
      content: [toolResponseBlock],
      toolResults: [toolResponseBlock],
    }),
    'All 24 tests passed!',
  )

  // Direct output field
  assert.equal(
    extractToolResultOutput({ name: 'shell', output: 'build succeeded' }),
    'build succeeded',
  )
})

test('buildAgentLog only shows the added prompts of the current step', () => {
  const run: HarnessRunResponse = {
    taskId: 'multi-step-task',
    status: 'Success',
    stepCount: 3,
    toolCallsCount: 2,
    durationMs: 900,
    trajectory: {
      taskId: 'multi-step-task',
      policyName: 'agent',
      startedAt: '2026-09-14T00:00:00Z',
      completedAt: '2026-09-14T00:00:03Z',
      steps: [
        {
          stepIndex: 1,
          timestamp: '2026-09-14T00:00:01Z',
          durationMs: 300,
          agentAction: {
            CallTools: [{ id: 't1', name: 'shell', arguments: { command: 'cargo check' } }],
          },
          toolResults: [{ id: 't1', name: 'shell', output: 'ok', is_error: false }],
          llmRequest: {
            messages: [{ role: 'user', content: 'Run cargo check and fix errors' }],
          },
          llmResponse: { text: 'Running cargo check' },
        },
        {
          stepIndex: 2,
          timestamp: '2026-09-14T00:00:02Z',
          durationMs: 400,
          agentAction: {
            CallTools: [{ id: 't2', name: 'shell', arguments: { command: 'cargo test' } }],
          },
          toolResults: [{ id: 't2', name: 'shell', output: 'passed', is_error: false }],
          llmRequest: {
            // Step 2 cumulative messages in LLM request
            messages: [
              { role: 'user', content: 'Run cargo check and fix errors' },
              { role: 'assistant', content: 'Running cargo check' },
              { role: 'user', content: [{ type: 'toolResponse', id: 't1', toolResult: { Ok: { content: [{ type: 'text', text: 'ok' }] } } }] },
            ],
          },
          llmResponse: { text: 'Running tests' },
        },
        {
          stepIndex: 3,
          timestamp: '2026-09-14T00:00:03Z',
          durationMs: 200,
          agentAction: { FinalAnswer: 'Everything is done!' },
          llmRequest: {
            // Step 3 cumulative messages in LLM request
            messages: [
              { role: 'user', content: 'Run cargo check and fix errors' },
              { role: 'assistant', content: 'Running cargo check' },
              { role: 'user', content: [{ type: 'toolResponse', id: 't1', toolResult: { Ok: { content: [{ type: 'text', text: 'ok' }] } } }] },
              { role: 'assistant', content: 'Running tests' },
              { role: 'user', content: [{ type: 'toolResponse', id: 't2', toolResult: { Ok: { content: [{ type: 'text', text: 'passed' }] } } }] },
            ],
          },
          llmResponse: { text: 'Everything is done!' },
        },
      ],
    },
  }

  const turns = buildAgentLog(run)
  // Turn 0: user prompt
  // Turn 1: step 1 (assistant) -> added prompts: 1 (the initial user prompt)
  // Turn 2: step 2 (assistant) -> added prompts: 1 (tool result t1), NOT 3
  // Turn 3: step 3 (final answer) -> added prompts: 1 (tool result t2), NOT 5
  assert.equal(turns.length, 4)
  assert.equal(turns[1].promptMessages?.length, 1)
  assert.equal(turns[1].promptMessages?.[0].role, 'user')

  assert.equal(turns[2].promptMessages?.length, 1)
  assert.equal(turns[2].promptMessages?.[0].toolResults?.length, 1)

  assert.equal(turns[3].promptMessages?.length, 1)
  assert.equal(turns[3].promptMessages?.[0].toolResults?.length, 1)
})

test('evaluateAutoScrollState pauses when user scrolls up and resumes at bottom', () => {
  // At bottom initially: autoScroll remains true
  const atBottom = evaluateAutoScrollState({
    distanceFromBottom: 0,
    currentAutoScroll: true,
    userExplicitlyDisabled: false,
  })
  assert.equal(atBottom.autoScroll, true)
  assert.equal(atBottom.pausedByUserScroll, false)

  // Scrolled up past threshold (> 60px): autoScroll pauses
  const scrolledUp = evaluateAutoScrollState({
    distanceFromBottom: 150,
    currentAutoScroll: true,
    userExplicitlyDisabled: false,
  })
  assert.equal(scrolledUp.autoScroll, false)
  assert.equal(scrolledUp.pausedByUserScroll, true)

  // In intermediate zone (e.g. 40px): maintains current paused state
  const intermediate = evaluateAutoScrollState({
    distanceFromBottom: 40,
    currentAutoScroll: false,
    userExplicitlyDisabled: false,
  })
  assert.equal(intermediate.autoScroll, false)

  // Scrolled back near bottom (<= 20px): automatically resumes
  const backAtBottom = evaluateAutoScrollState({
    distanceFromBottom: 10,
    currentAutoScroll: false,
    userExplicitlyDisabled: false,
  })
  assert.equal(backAtBottom.autoScroll, true)
  assert.equal(backAtBottom.pausedByUserScroll, false)

  // User explicitly disabled autoScroll: remains disabled even at bottom
  const explicitlyDisabled = evaluateAutoScrollState({
    distanceFromBottom: 0,
    currentAutoScroll: false,
    userExplicitlyDisabled: true,
  })
  assert.equal(explicitlyDisabled.autoScroll, false)
  assert.equal(explicitlyDisabled.pausedByUserScroll, false)
})


