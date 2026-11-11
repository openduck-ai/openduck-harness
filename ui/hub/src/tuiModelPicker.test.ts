import assert from 'node:assert/strict'
import test from 'node:test'
import {
  resolveActiveModel,
  normalizeCatalog,
  performModelSwitch,
  performCatalogRefresh,
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
