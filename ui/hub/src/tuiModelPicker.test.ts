import assert from 'node:assert/strict'
import test from 'node:test'
import {
  resolveActiveModel,
  normalizeCatalog,
  performModelSwitch,
  performCatalogRefresh,
  discoverAgentModelsViaSession,
  getModelDiscoveryCommand,
  getCliDiscoveryCommand,
  discoverAgentModelsViaCli,
  parseModelsFromTuiOutput,
  stripAnsi,
} from './tuiModelPicker.ts'

test('resolveActiveModel prioritizes activeModel over currentModel over model', () => {
  assert.equal(
    resolveActiveModel({ activeModel: 'm-active', currentModel: 'm-current', model: 'm-initial' }),
    'm-active',
  )
  assert.equal(
    resolveActiveModel({ currentModel: 'm-current', model: 'm-initial' }),
    'm-current',
  )
  assert.equal(
    resolveActiveModel({ model: 'm-initial' }),
    'm-initial',
  )
  assert.equal(
    resolveActiveModel({ activeModel: '   ', currentModel: 'm-current', model: 'm-initial' }),
    'm-current',
  )
  assert.equal(
    resolveActiveModel({ activeModel: '', currentModel: '', model: '' }, 'default-fallback'),
    'default-fallback',
  )
  assert.equal(
    resolveActiveModel(null, 'default-fallback'),
    'default-fallback',
  )
})

test('normalizeCatalog handles both direct and wrapped envelopes with snake_case and camelCase', () => {
  const directSnake = {
    agent: 'grok',
    default_model: 'grok-4',
    supports_free_text: true,
    is_fallback: true,
    cached_at_unix_secs: 1700000000,
    models: [
      { id: 'grok-4', display_name: 'Grok 4', description: 'Flagship' },
      { id: 'grok-code', display_name: 'Grok Code' },
    ],
  }

  const normalized1 = normalizeCatalog(directSnake, 'grok')
  assert.equal(normalized1.agent, 'grok')
  assert.equal(normalized1.defaultModel, 'grok-4')
  assert.equal(normalized1.supportsFreeText, true)
  assert.equal(normalized1.isFallback, true)
  assert.equal(normalized1.models.length, 2)
  assert.equal(normalized1.models[0].displayName, 'Grok 4')
  assert.equal(normalized1.models[0].isDefault, true)
  assert.equal(normalized1.models[0].source, 'static_fallback')
  assert.equal(normalized1.models[1].displayName, 'Grok Code')
  assert.equal(normalized1.models[1].isDefault, false)

  const wrappedCamel = {
    catalog: {
      agent: 'codex',
      defaultModel: 'gpt-5-codex',
      supportsFreeText: false,
      isFallback: false,
      models: [
        { id: 'gpt-5-codex', displayName: 'GPT-5 Codex', isDefault: true, source: 'dynamic_loader' },
      ],
    },
  }

  const normalized2 = normalizeCatalog(wrappedCamel, 'codex')
  assert.equal(normalized2.agent, 'codex')
  assert.equal(normalized2.defaultModel, 'gpt-5-codex')
  assert.equal(normalized2.supportsFreeText, false)
  assert.equal(normalized2.isFallback, false)
  assert.equal(normalized2.models[0].source, 'dynamic_loader')
})

test('normalizeCatalog gracefully handles empty, null or invalid payload without crashing or hardcoded lists', () => {
  const empty = normalizeCatalog(null, 'agy')
  assert.equal(empty.agent, 'agy')
  assert.deepEqual(empty.models, [])
  assert.equal(empty.supportsFreeText, false)
  assert.equal(empty.isFallback, false)
})

test('performModelSwitch succeeds and updates active model', async () => {
  const session = { id: 's1', agentKind: 'agy', model: 'gemini-2.5-pro' }
  const mockSwitch = async (id: string, model: string) => {
    assert.equal(id, 's1')
    assert.equal(model, 'gemini-2.5-flash')
    return { success: true, activeModel: 'gemini-2.5-flash' }
  }

  const outcome = await performModelSwitch(mockSwitch, session, 'gemini-2.5-flash')
  assert.equal(outcome.success, true)
  assert.equal(outcome.activeModel, 'gemini-2.5-flash')
  assert.equal(outcome.error, undefined)
})

test('performModelSwitch on error keeps shown active model UNCHANGED and returns actionable error', async () => {
  const session = { id: 's1', agentKind: 'agy', activeModel: 'gemini-2.5-pro' }
  const mockSwitch = async () => {
    throw new Error('Connection to CLI backend timed out')
  }

  const outcome = await performModelSwitch(mockSwitch, session, 'gemini-invalid-model')
  assert.equal(outcome.success, false)
  // Active model MUST remain unchanged!
  assert.equal(outcome.activeModel, 'gemini-2.5-pro')
  assert.ok(outcome.error?.includes('Failed to switch model to "gemini-invalid-model"'))
  assert.ok(outcome.error?.includes('Active model remains "gemini-2.5-pro"'))
  assert.ok(outcome.error?.includes('Connection to CLI backend timed out'))
})

test('performModelSwitch rejects blank model and keeps active model unchanged', async () => {
  const session = { id: 's1', agentKind: 'claude', activeModel: 'claude-3-7-sonnet' }
  const mockSwitch = async () => {
    throw new Error('should not be called')
  }

  const outcome = await performModelSwitch(mockSwitch, session, '   ')
  assert.equal(outcome.success, false)
  assert.equal(outcome.activeModel, 'claude-3-7-sonnet')
  assert.ok(outcome.error?.includes('Please choose or enter a model'))
})

test('performModelSwitch rejects switching to already active model', async () => {
  const session = { id: 's1', agentKind: 'claude', activeModel: 'claude-3-7-sonnet' }
  const mockSwitch = async () => {
    throw new Error('should not be called')
  }

  const outcome = await performModelSwitch(mockSwitch, session, 'claude-3-7-sonnet')
  assert.equal(outcome.success, false)
  assert.equal(outcome.activeModel, 'claude-3-7-sonnet')
  assert.ok(outcome.error?.includes('already active'))
})

test('performModelSwitch handles server rejection without throwing and preserves active model', async () => {
  const session = { id: 's2', agentKind: 'codex', activeModel: 'o3-mini' }
  const mockSwitch = async () => ({ success: false, message: 'Provider quota exhausted' })

  const outcome = await performModelSwitch(mockSwitch, session, 'o3-high')
  assert.equal(outcome.success, false)
  assert.equal(outcome.activeModel, 'o3-mini')
  assert.ok(outcome.error?.includes('Provider quota exhausted'))
  assert.ok(outcome.error?.includes('Active model remains "o3-mini"'))
})

test('performModelSwitch trims whitespace and checks case-insensitively', async () => {
  const session = { id: 's3', agentKind: 'codex', activeModel: 'GPT-5' }
  const mockSwitch = async () => {
    throw new Error('should not be called')
  }

  const outcome = await performModelSwitch(mockSwitch, session, '  gpt-5  ')
  assert.equal(outcome.success, false)
  assert.equal(outcome.activeModel, 'GPT-5')
  assert.ok(outcome.error?.includes('already active'))
})

test('performCatalogRefresh retains existing catalog on error with actionable feedback', async () => {
  const existingCatalog = normalizeCatalog({
    agent: 'codex',
    models: [{ id: 'gpt-5', displayName: 'GPT-5' }],
  }, 'codex')

  const mockRefreshFail = async () => {
    throw new Error('Network unreachable')
  }

  const outcome = await performCatalogRefresh(mockRefreshFail, 'codex', existingCatalog)
  assert.equal(outcome.success, false)
  // Retains existing catalog!
  assert.deepEqual(outcome.catalog, existingCatalog)
  assert.ok(outcome.error?.includes('Network unreachable'))
  assert.ok(outcome.error?.includes('Retained existing model list'))
})

test('performCatalogRefresh updates catalog on success', async () => {
  const existingCatalog = normalizeCatalog({
    agent: 'codex',
    models: [{ id: 'gpt-5', displayName: 'GPT-5' }],
  }, 'codex')

  const mockRefreshSuccess = async () => {
    return {
      agent: 'codex',
      models: [
        { id: 'gpt-5', displayName: 'GPT-5' },
        { id: 'gpt-5-mini', displayName: 'GPT-5 Mini' },
      ],
    }
  }

  const outcome = await performCatalogRefresh(mockRefreshSuccess, 'codex', existingCatalog)
  assert.equal(outcome.success, true)
  assert.equal(outcome.catalog?.models.length, 2)
  assert.equal(outcome.error, undefined)
})

test('performModelSwitch adopts authoritative canonical activeModel from server response', async () => {
  const session = { id: 's4', agentKind: 'agy', activeModel: 'gemini-2.5-flash' }
  const mockSwitch = async () => ({
    success: true,
    activeModel: 'gemini-2.5-pro',
    message: 'Switched model to gemini-2.5-pro',
  })

  const outcome = await performModelSwitch(mockSwitch, session, 'Gemini 2.5 Pro')
  assert.equal(outcome.success, true)
  assert.equal(outcome.activeModel, 'gemini-2.5-pro')
  assert.equal(outcome.error, undefined)
})

test('performModelSwitch preserves prior confirmed active model when server responds with rejection message', async () => {
  const session = { id: 's5', agentKind: 'grok', activeModel: 'grok-4' }
  const mockSwitch = async () => ({
    success: false,
    message: 'Process busy executing turn',
    activeModel: 'grok-4',
  })

  const outcome = await performModelSwitch(mockSwitch, session, 'grok-3')
  assert.equal(outcome.success, false)
  assert.equal(outcome.activeModel, 'grok-4')
  assert.ok(outcome.error?.includes('Process busy executing turn'))
  assert.ok(outcome.error?.includes('Active model remains "grok-4"'))
})

test('project and global routing dispatch to appropriate switch method based on ownership', async () => {
  let routedEndpoint = ''
  let payloadSessionId = ''
  let payloadModel = ''

  const fakeApi = {
    switchProjectTuiSessionModel: async (slug: string, sid: string, model: string) => {
      routedEndpoint = `project:${slug}`
      payloadSessionId = sid
      payloadModel = model
      return { success: true, activeModel: model }
    },
    switchTuiSessionModel: async (sid: string, model: string) => {
      routedEndpoint = 'global'
      payloadSessionId = sid
      payloadModel = model
      return { success: true, activeModel: model }
    },
  }

  // 1. Session with createdByProjectId routes to project switch
  const projectSession = {
    id: 'sess-proj-1',
    agentKind: 'claude',
    activeModel: 'claude-3-5-sonnet',
    createdByProjectId: 'alpha-proj',
  }
  const effectiveProjectSlug = projectSession.createdByProjectId
  const switchFnProj = (sid: string, model: string) => {
    if (effectiveProjectSlug && typeof fakeApi.switchProjectTuiSessionModel === 'function') {
      return fakeApi.switchProjectTuiSessionModel(effectiveProjectSlug, sid, model)
    }
    return fakeApi.switchTuiSessionModel(sid, model)
  }
  const res1 = await performModelSwitch(switchFnProj, projectSession, 'claude-3-7-sonnet')
  assert.equal(res1.success, true)
  assert.equal(res1.activeModel, 'claude-3-7-sonnet')
  assert.equal(routedEndpoint, 'project:alpha-proj')
  assert.equal(payloadSessionId, 'sess-proj-1')
  assert.equal(payloadModel, 'claude-3-7-sonnet')

  // 2. Global session without project routes to global switch
  const globalSession = {
    id: 'sess-global-1',
    agentKind: 'grok',
    activeModel: 'grok-3',
  }
  const switchFnGlobal = (sid: string, model: string) => {
    return fakeApi.switchTuiSessionModel(sid, model)
  }
  const res2 = await performModelSwitch(switchFnGlobal, globalSession, 'grok-4')
  assert.equal(res2.success, true)
  assert.equal(res2.activeModel, 'grok-4')
  assert.equal(routedEndpoint, 'global')
  assert.equal(payloadSessionId, 'sess-global-1')
  assert.equal(payloadModel, 'grok-4')
})

test('discoverAgentModelsViaSession launches ephemeral session, sends /models, refreshes catalog, and cleans up', async () => {
  const calls: string[] = []
  let stoppedSessionId = ''
  let promptCommand = ''

  const fakeApi = {
    startManagedTuiSession: async (data: any) => {
      calls.push(`start:${data.agent}:${data.sessionName}`)
      return { session: { id: 'ephemeral-sess-1', sessionName: data.sessionName } }
    },
    promptTuiSession: async (name: string, payload: any) => {
      calls.push(`prompt:${name}:${payload.prompt}`)
      promptCommand = payload.prompt
      return { output: 'models' }
    },
    refreshTuiAgentModels: async (agentKind: string) => {
      calls.push(`refresh:${agentKind}`)
      return {
        agent: agentKind,
        models: [
          { id: 'grok-4', displayName: 'Grok 4', isDefault: true },
          { id: 'grok-beta', displayName: 'Grok Beta', isDefault: false },
        ],
        defaultModel: 'grok-4',
      }
    },
    stopManagedTuiSession: async (id: string, reason?: string) => {
      calls.push(`stop:${id}:${reason}`)
      stoppedSessionId = id
      return { success: true }
    },
  }

  const outcome = await discoverAgentModelsViaSession(fakeApi, 'grok', null)
  assert.equal(outcome.success, true)
  assert.equal(outcome.catalog?.models.length, 2)
  assert.equal(outcome.catalog?.models[0].id, 'grok-4')
  assert.equal(promptCommand, '/models')
  assert.equal(stoppedSessionId, 'ephemeral-sess-1')
  assert.equal(calls.some(c => c.startsWith('start:grok:discover_grok_')), true)
  assert.equal(calls.includes('refresh:grok'), true)
  assert.equal(calls.includes('stop:ephemeral-sess-1:discovery_complete'), true)
})

test('discoverAgentModelsViaSession handles launch failure gracefully by falling back to refresh', async () => {
  let stopped = false
  const fakeApi = {
    startManagedTuiSession: async () => {
      throw new Error('tmux server unavailable')
    },
    refreshTuiAgentModels: async (agentKind: string) => ({
      agent: agentKind,
      models: [{ id: 'gemini-2.5-flash', displayName: 'Gemini 2.5 Flash', isDefault: true }],
    }),
    stopManagedTuiSession: async () => {
      stopped = true
      return { success: true }
    },
  }

  const outcome = await discoverAgentModelsViaSession(fakeApi, 'agy', null)
  assert.equal(outcome.success, true)
  assert.equal(outcome.catalog?.models[0].id, 'gemini-2.5-flash')
  assert.equal(stopped, false)
})

test('discoverAgentModelsViaSession guarantees cleanup of ephemeral session even if refresh throws', async () => {
  let cleanedUpId = ''
  const fakeApi = {
    startManagedTuiSession: async (data: any) => ({
      session: { id: 'temp-sess-fail-1', sessionName: data.sessionName },
    }),
    promptTuiSession: async () => ({ output: '' }),
    refreshTuiAgentModels: async () => {
      throw new Error('Backend timeout fetching models')
    },
    stopManagedTuiSession: async (id: string) => {
      cleanedUpId = id
      return { success: true }
    },
  }

  const fallbackCatalog = {
    agent: 'codex',
    models: [{ id: 'o3-mini', displayName: 'o3-mini', isDefault: true, source: 'static_fallback' as const }],
    defaultModel: 'o3-mini',
    supportsFreeText: true,
    isFallback: true,
    cachedAtUnixSecs: 0,
  }

  const outcome = await discoverAgentModelsViaSession(fakeApi, 'codex', fallbackCatalog)
  assert.equal(outcome.success, false)
  assert.match(outcome.error ?? '', /Backend timeout/)
  assert.equal(outcome.catalog?.models[0].id, 'o3-mini')
  assert.equal(cleanedUpId, 'temp-sess-fail-1')
})

test('discoverAgentModelsViaSession reports progress and captures live terminal screen interactions', async () => {
  const progressReports: Array<{ status: string; screen?: string }> = []
  let screenCalls = 0

  const fakeApi = {
    startManagedTuiSession: async (data: any) => ({
      session: { id: 'sess-progress-1', sessionName: data.sessionName },
      screen: '> OpenAI Codex TUI (v1.0)\nType /help for options.',
    }),
    getManagedTuiSession: async (id: string) => {
      screenCalls++
      return {
        session: { id },
        screen: `> OpenAI Codex TUI (v1.0)\n/models\nListing available models... (${screenCalls})`,
      }
    },
    promptTuiSession: async () => ({ output: 'ok' }),
    refreshTuiAgentModels: async (agent: string) => ({
      agent,
      models: [
        { id: 'gpt-6.1-sol', displayName: 'GPT-6.1-Sol', isDefault: true },
        { id: 'gpt-6-astra', displayName: 'GPT-6-Astra', isDefault: false },
      ],
      defaultModel: 'gpt-6.1-sol',
    }),
    stopManagedTuiSession: async () => ({ success: true }),
  }

  const outcome = await discoverAgentModelsViaSession(
    fakeApi,
    'codex',
    null,
    p => progressReports.push({ status: p.status, screen: p.screen }),
  )

  assert.equal(outcome.success, true)
  assert.equal(outcome.catalog?.models.length, 2)
  assert.ok(progressReports.length >= 3)
  assert.ok(outcome.screen?.includes('OpenAI Codex TUI'))
  assert.ok(outcome.screen?.includes('gpt-6.1-sol'))
  assert.ok(progressReports.some(p => p.status.includes('Interacting with codex') || p.status.includes('TUI')))
})

test('getModelDiscoveryCommand returns correct command per agent kind and respects overrides', () => {
  assert.equal(getModelDiscoveryCommand('agy'), '/models')
  assert.equal(getModelDiscoveryCommand('antigravity'), '/models')
  assert.equal(getModelDiscoveryCommand('claude'), '/model')
  assert.equal(getModelDiscoveryCommand('claude-code'), '/model')
  assert.equal(getModelDiscoveryCommand('grok'), '/models')
  assert.equal(getModelDiscoveryCommand('codex'), '/models')
  assert.equal(getModelDiscoveryCommand('custom'), '/models')

  // Explicit override takes precedence
  assert.equal(getModelDiscoveryCommand('claude', '/models'), '/models')
  assert.equal(getModelDiscoveryCommand('agy', '/custom-cmd'), '/custom-cmd')
  assert.equal(getModelDiscoveryCommand('codex', 'none'), 'none')
  assert.equal(getModelDiscoveryCommand('codex', ''), '/models')
  assert.equal(getModelDiscoveryCommand('claude', '  '), '/model')
  assert.equal(getModelDiscoveryCommand('codex', 'auto'), '/models')
  assert.equal(getModelDiscoveryCommand('claude', 'default'), '/model')
})

test('discoverAgentModelsViaSession sends agent-specific commands (e.g. /model for claude, /models for agy)', async () => {
  const sentPrompts: string[] = []

  const fakeApi = {
    startManagedTuiSession: async (data: any) => ({
      session: { id: `sess-${data.agent}`, sessionName: data.sessionName },
    }),
    promptTuiSession: async (_name: string, payload: any) => {
      sentPrompts.push(payload.prompt)
      return { output: 'ok' }
    },
    refreshTuiAgentModels: async (agent: string) => ({
      agent,
      models: [{ id: 'model-1', displayName: 'Model 1', isDefault: true }],
    }),
    stopManagedTuiSession: async () => ({ success: true }),
  }

  // Claude agent sends /model by default
  await discoverAgentModelsViaSession(fakeApi, 'claude', null)
  assert.equal(sentPrompts[0], '/model')

  // Agy agent sends /models by default
  await discoverAgentModelsViaSession(fakeApi, 'agy', null)
  assert.equal(sentPrompts[1], '/models')

  // Custom discoveryCommand option overrides default
  await discoverAgentModelsViaSession(fakeApi, 'claude', null, {
    discoveryCommand: '/choose-model',
  })
  assert.equal(sentPrompts[2], '/choose-model')

  // 'none' skips sending prompt completely
  const promptCountBefore = sentPrompts.length
  await discoverAgentModelsViaSession(fakeApi, 'custom', null, {
    discoveryCommand: 'none',
  })
  assert.equal(sentPrompts.length, promptCountBefore)
})

test('parseModelsFromTuiOutput extracts models from bulleted, tabular, and JSON TUI responses', () => {
  const agyOutput = `
> /models
Fetching models...
gemini-2.5-flash\tGemini 2.5 Flash\tFast balanced model
* gemini-2.5-pro (default)\tGemini 2.5 Pro\tDeep reasoning model
claude-3-7-sonnet\tClaude 3.7 Sonnet
`
  const agyModels = parseModelsFromTuiOutput(agyOutput, 'agy')
  assert.equal(agyModels.length, 3)
  assert.equal(agyModels[0].id, 'gemini-2.5-flash')
  assert.equal(agyModels[0].displayName, 'Gemini 2.5 Flash')
  assert.equal(agyModels[0].description, 'Fast balanced model')
  assert.equal(agyModels[1].id, 'gemini-2.5-pro')
  assert.equal(agyModels[1].isDefault, true)
  assert.equal(agyModels[2].id, 'claude-3-7-sonnet')

  const codexOutput = `
OpenAI Codex (v0.167.0)
Available models:
  * gpt-5 (default) - Flagship reasoning model
  - gpt-4o - Fast multimodal model
  - o3-mini - High intelligence reasoning model
`
  const codexModels = parseModelsFromTuiOutput(codexOutput, 'codex')
  assert.equal(codexModels.length, 3)
  assert.equal(codexModels[0].id, 'gpt-5')
  assert.equal(codexModels[0].isDefault, true)
  assert.equal(codexModels[1].id, 'gpt-4o')
  assert.equal(codexModels[2].id, 'o3-mini')

  const jsonOutput = `
{"models": [
  {"slug": "claude-3-7-sonnet", "display_name": "Claude 3.7 Sonnet", "priority": 1},
  {"slug": "claude-3-5-haiku", "display_name": "Claude 3.5 Haiku", "priority": 2}
]}
`
  const jsonModels = parseModelsFromTuiOutput(jsonOutput, 'claude')
  assert.equal(jsonModels.length, 2)
  assert.equal(jsonModels[0].id, 'claude-3-7-sonnet')
  assert.equal(jsonModels[0].isDefault, true)
  assert.equal(jsonModels[1].id, 'claude-3-5-haiku')

  // Real Claude Code live interactive terminal screen from /model
  const realClaudeScreen = `
   Select model
   Switch between Claude models. Your pick becomes the default for new sessions. For other/previous model names, specify with --model.

   ❯ 1. Default (recommended) ✔  Use the default model (currently Opus 5.5) · $4/$20 per Mtok
     2. Opus                     Opus 5.5 · Best for everyday, complex tasks · $4/$20 per Mtok
     3. Fable                    Fable 5.1 · Most capable for your hardest and longest-running tasks · $10/$50 per Mtok
     4. Sonnet                   Sonnet 5.5 · Efficient for routine tasks · $2/$10 per Mtok
     5. Haiku                    Haiku 4.5 · Fastest for quick answers · $1/$5 per Mtok

   Enter to set as default · s to use this session only · Esc to cancel
`
  const realClaudeModels = parseModelsFromTuiOutput(realClaudeScreen, 'claude')
  assert.equal(realClaudeModels.length, 4)
  assert.equal(realClaudeModels[0].id, 'Opus')
  assert.equal(realClaudeModels[0].displayName, 'Opus 5.5')
  assert.equal(realClaudeModels[1].id, 'Fable')
  assert.equal(realClaudeModels[2].id, 'Sonnet')
  assert.equal(realClaudeModels[3].id, 'Haiku')
})

test('discoverAgentModelsViaSession fetches model catalog directly from executed TUI agent response', async () => {
  let promptSentWithWait: boolean | undefined
  let promptTimeoutSecs: number | undefined

  const fakeApi = {
    startManagedTuiSession: async (data: any) => ({
      session: { id: 'sess-live-exec-1', sessionName: data.sessionName },
    }),
    promptManagedTuiSession: async (_id: string, payload: any) => {
      promptSentWithWait = payload.wait
      promptTimeoutSecs = payload.timeoutSecs
      return {
        response: `
Available models:
  * gemini-2.5-pro (default) - Google frontier model
  - gemini-2.5-flash - Google fast model
  - claude-3-7-sonnet - Anthropic hybrid reasoning
`,
        screenSnapshot: 'Session active',
      }
    },
    getManagedTuiSession: async (id: string) => ({
      session: { id },
      screen: 'Turn complete',
    }),
    refreshTuiAgentModels: async () => {
      throw new Error('Backend refresh should not be reached when TUI response contains models')
    },
    stopManagedTuiSession: async () => ({ success: true }),
  }

  const outcome = await discoverAgentModelsViaSession(fakeApi, 'agy', null)
  assert.equal(outcome.success, true)
  assert.equal(promptSentWithWait, true)
  assert.equal(promptTimeoutSecs, 15)
  assert.equal(outcome.catalog?.models.length, 3)
  assert.equal(outcome.catalog?.models[0].id, 'gemini-2.5-pro')
  assert.equal(outcome.catalog?.models[0].isDefault, true)
  assert.equal(outcome.catalog?.defaultModel, 'gemini-2.5-pro')
  assert.equal(outcome.catalog?.models[0].source, 'dynamic_loader')
  assert.ok(outcome.screen?.includes('Discovered 3 model(s) from TUI agent response'))
})

test('getCliDiscoveryCommand returns correct non-interactive CLI command per agent', () => {
  assert.equal(getCliDiscoveryCommand('agy'), 'agy models')
  assert.equal(getCliDiscoveryCommand('antigravity'), 'agy models')
  assert.equal(getCliDiscoveryCommand('grok'), 'grok models')
  assert.equal(getCliDiscoveryCommand('grok-build'), 'grok models')
  assert.equal(getCliDiscoveryCommand('codex'), 'codex debug models')
  assert.equal(getCliDiscoveryCommand('openai-codex'), 'codex debug models')
  assert.equal(getCliDiscoveryCommand('claude'), 'claude')
  assert.equal(getCliDiscoveryCommand('custom'), 'custom models')

  // Explicit override
  assert.equal(getCliDiscoveryCommand('codex', 'codex models'), 'codex models')
  assert.equal(getCliDiscoveryCommand('agy', 'custom-cmd'), 'custom-cmd')
  assert.equal(getCliDiscoveryCommand('agy', ''), 'agy models')
  assert.equal(getCliDiscoveryCommand('agy', 'auto'), 'agy models')
})

test('discoverAgentModelsViaCli refreshes models via backend API directly without TUI session', async () => {
  let refreshedKind: string | null = null
  const fakeApi = {
    refreshTuiAgentModels: async (agentKind: string) => {
      refreshedKind = agentKind
      return {
        agent: agentKind,
        models: [
          { id: 'gpt-6.1-sol', displayName: 'GPT-6.1-Sol', isDefault: true },
          { id: 'gpt-6-astra', displayName: 'GPT-6-Astra' },
        ],
        defaultModel: 'gpt-6.1-sol',
      }
    },
  }

  const outcome = await discoverAgentModelsViaCli(fakeApi, 'codex', null)
  assert.equal(outcome.success, true)
  assert.equal(refreshedKind, 'codex')
  assert.equal(outcome.catalog?.models.length, 2)
  assert.equal(outcome.catalog?.models[0].id, 'gpt-6.1-sol')
  assert.equal(outcome.catalog?.defaultModel, 'gpt-6.1-sol')
})

test('discoverAgentModelsViaCli falls back to getTuiAgentModels with refresh option', async () => {
  let queriedKind: string | null = null
  let queriedRefresh: boolean | undefined = undefined
  const fakeApi = {
    getTuiAgentModels: async (agentKind: string, options?: { refresh?: boolean }) => {
      queriedKind = agentKind
      queriedRefresh = options?.refresh
      return {
        agent: agentKind,
        models: [
          { id: 'gemini-4-argon-medium', displayName: 'Gemini 4 Argon (Medium)', isDefault: true },
        ],
      }
    },
  }

  const outcome = await discoverAgentModelsViaCli(fakeApi, 'agy', null)
  assert.equal(outcome.success, true)
  assert.equal(queriedKind, 'agy')
  assert.equal(queriedRefresh, true)
  assert.equal(outcome.catalog?.models.length, 1)
  assert.equal(outcome.catalog?.models[0].id, 'gemini-4-argon-medium')
})

test('discoverAgentModelsViaCli handles errors gracefully without throwing', async () => {
  const fakeApi = {
    refreshTuiAgentModels: async () => {
      throw new Error('Network timeout')
    },
  }

  const fallbackCatalog = {
    agent: 'codex',
    models: [{ id: 'gpt-5', displayName: 'GPT-5' }],
    defaultModel: 'gpt-5',
    supportsFreeText: true,
    isFallback: true,
  }

  const outcome = await discoverAgentModelsViaCli(fakeApi, 'codex', fallbackCatalog)
  assert.equal(outcome.success, false)
  assert.equal(outcome.error, 'Network timeout')
  assert.equal(outcome.catalog, fallbackCatalog)
})
