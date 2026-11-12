import type { TuiAgentModelCatalog, TuiCatalogModelEntry } from './api.ts'

export interface SessionWithModel {
  id: string
  agentKind: string
  model?: string
  activeModel?: string
  currentModel?: string
}

/**
 * Resolves the currently active model from session properties with priority:
 * activeModel -> currentModel -> model -> fallback
 */
export function resolveActiveModel(
  session?: { activeModel?: string; currentModel?: string; model?: string } | null,
  fallback = '',
): string {
  if (!session) return fallback
  const candidates = [session.activeModel, session.currentModel, session.model]
  for (const c of candidates) {
    if (typeof c === 'string' && c.trim()) {
      return c.trim()
    }
  }
  return fallback
}

/**
 * Normalizes raw catalog data from API responses (supporting snake_case, camelCase,
 * wrapped envelopes, or partial structures).
 */
export function normalizeCatalog(raw: unknown, fallbackAgentKind = ''): TuiAgentModelCatalog {
  let source: any = raw
  if (source && typeof source === 'object' && 'catalog' in source && source.catalog) {
    source = source.catalog
  }

  const agent = typeof source?.agent === 'string' ? source.agent : fallbackAgentKind
  const defaultModel =
    typeof source?.defaultModel === 'string'
      ? source.defaultModel
      : typeof source?.default_model === 'string'
        ? source.default_model
        : null

  const discoveryCommand =
    typeof source?.discoveryCommand === 'string'
      ? source.discoveryCommand
      : typeof source?.discovery_command === 'string'
        ? source.discovery_command
        : null

  const cliDiscoveryCommand =
    typeof source?.cliDiscoveryCommand === 'string'
      ? source.cliDiscoveryCommand
      : typeof source?.cli_discovery_command === 'string'
        ? source.cli_discovery_command
        : getCliDiscoveryCommand(agent)

  const supportsFreeText = Boolean(source?.supportsFreeText ?? source?.supports_free_text ?? false)
  const isFallback = Boolean(source?.isFallback ?? source?.is_fallback ?? false)
  const cachedAtUnixSecs =
    typeof source?.cachedAtUnixSecs === 'number'
      ? source.cachedAtUnixSecs
      : typeof source?.cached_at_unix_secs === 'number'
        ? source.cached_at_unix_secs
        : undefined

  const rawModels = Array.isArray(source?.models) ? source.models : []
  const models: TuiCatalogModelEntry[] = rawModels
    .map((m: any): TuiCatalogModelEntry | null => {
      if (!m || typeof m !== 'object') return null
      const id = typeof m.id === 'string' ? m.id.trim() : ''
      if (!id) return null
      const displayName =
        typeof m.displayName === 'string'
          ? m.displayName
          : typeof m.display_name === 'string'
            ? m.display_name
            : id
      const description = typeof m.description === 'string' ? m.description : null
      const isDefault = Boolean(m.isDefault ?? m.is_default ?? (defaultModel ? id === defaultModel : false))
      const modelSource = m.source ?? (isFallback ? 'static_fallback' : 'dynamic_loader')
      return {
        id,
        displayName,
        display_name: displayName,
        description,
        isDefault,
        is_default: isDefault,
        source: modelSource,
      }
    })
    .filter((entry: TuiCatalogModelEntry | null): entry is TuiCatalogModelEntry => entry !== null)

  return {
    agent,
    models,
    defaultModel,
    default_model: defaultModel,
    discoveryCommand,
    discovery_command: discoveryCommand,
    cliDiscoveryCommand,
    cli_discovery_command: cliDiscoveryCommand,
    supportsFreeText,
    supports_free_text: supportsFreeText,
    isFallback,
    is_fallback: isFallback,
    cachedAtUnixSecs,
    cached_at_unix_secs: cachedAtUnixSecs,
  }
}

export interface SwitchModelOutcome {
  success: boolean
  activeModel: string
  error?: string
}

/**
 * Attempts to switch model on an active session.
 * CRITICAL INVARIANT: On errors, returns the unchanged activeModel alongside
 * an actionable error message.
 */
export async function performModelSwitch(
  switchFn: (sessionId: string, model: string) => Promise<{ success: boolean; activeModel?: string; message?: string }>,
  session: SessionWithModel,
  targetModel: string,
): Promise<SwitchModelOutcome> {
  const currentActive = resolveActiveModel(session)
  const cleanTarget = targetModel.trim()

  if (!cleanTarget) {
    return {
      success: false,
      activeModel: currentActive,
      error: 'Please choose or enter a model identifier before switching.',
    }
  }

  if (currentActive && cleanTarget.toLowerCase() === currentActive.toLowerCase()) {
    return {
      success: false,
      activeModel: currentActive,
      error: `Model "${cleanTarget}" is already active in this session.`,
    }
  }

  try {
    const result = await switchFn(session.id, cleanTarget)
    if (!result || result.success === false) {
      const msg = result?.message || 'Operation rejected by server'
      return {
        success: false,
        activeModel: currentActive,
        error: `Failed to switch model to "${cleanTarget}": ${msg}. Active model remains "${currentActive || 'default'}".`,
      }
    }
    const newActive = (result.activeModel && result.activeModel.trim()) || cleanTarget
    return {
      success: true,
      activeModel: newActive,
    }
  } catch (e: any) {
    const errorDetails = e?.message || String(e)
    return {
      success: false,
      activeModel: currentActive,
      error: `Failed to switch model to "${cleanTarget}": ${errorDetails}. Active model remains "${currentActive || 'default'}". Please check model availability or agent status and try again.`,
    }
  }
}

export interface RefreshCatalogOutcome {
  success: boolean
  catalog: TuiAgentModelCatalog | null
  error?: string
}

/**
 * Refreshes model catalog for an agent kind.
 * Retains the prior catalog on refresh failure and returns actionable feedback.
 */
export async function performCatalogRefresh(
  refreshFn: (agentKind: string) => Promise<unknown>,
  agentKind: string,
  existingCatalog: TuiAgentModelCatalog | null,
): Promise<RefreshCatalogOutcome> {
  try {
    const raw = await refreshFn(agentKind)
    const normalized = normalizeCatalog(raw, agentKind)
    return {
      success: true,
      catalog: normalized,
    }
  } catch (e: any) {
    const errorDetails = e?.message || String(e)
    return {
      success: false,
      catalog: existingCatalog,
      error: `Failed to refresh catalog for ${agentKind}: ${errorDetails}. Retained existing model list.`,
    }
  }
}

export interface DiscoverModelsProgress {
  sessionId?: string
  status: string
  screen?: string
}

export interface DiscoverModelsOptions {
  discoveryCommand?: string | null
  onProgress?: (progress: DiscoverModelsProgress) => void
}

export interface DiscoverModelsOutcome {
  success: boolean
  catalog: TuiAgentModelCatalog | null
  error?: string
  screen?: string
  sessionId?: string
}

/**
 * Strips ANSI terminal escape sequences.
 */
export function stripAnsi(text: string): string {
  // eslint-disable-next-line no-control-regex
  return text.replace(/\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])/g, '')
}

/**
 * Checks if a string looks like a valid model identifier slug (e.g., 'gemini-2.5-flash', 'gpt-5', 'o3-mini', 'claude-3-7-sonnet').
 * Ensures it contains delimiters or digits so plain English words ('Session', 'Turn', 'Loading') are not treated as models.
 */
const KNOWN_MODEL_NAMES = new Set([
  'opus',
  'sonnet',
  'haiku',
  'fable',
  'gemini',
  'claude',
  'grok',
  'codex',
  'gpt',
  'llama',
  'mistral',
  'deepseek',
  'qwen',
  'phi',
])

export function looksLikeModelId(id: string): boolean {
  if (!id || id.length > 100 || id.length < 2 || id.startsWith('-')) return false
  if (/^https?:\/\//i.test(id)) return false
  // Reject version numbers e.g. v0.167.0, 1.2.3
  if (/^v\d+(\.\d+)+/i.test(id) || /^\d+(\.\d+)+$/.test(id)) return false
  if (!/^[a-zA-Z0-9][-a-zA-Z0-9._/:]{0,80}[a-zA-Z0-9]$/.test(id)) return false
  if (KNOWN_MODEL_NAMES.has(id.toLowerCase())) return true
  return /[-._/:]/.test(id) || /[0-9]/.test(id)
}

/**
 * Parses structured model entries directly from the TUI response/screen output of an agent.
 */
export function parseModelsFromTuiOutput(
  rawOutput: string,
  _agentKind = '',
): TuiCatalogModelEntry[] {
  const clean = stripAnsi(rawOutput || '')
  if (!clean.trim()) return []

  // 1. Try parsing JSON models if present
  try {
    const jsonMatch = clean.match(/\{[\s\S]*"models"\s*:\s*\[[\s\S]*\][\s\S]*\}/)
    if (jsonMatch) {
      const parsed = JSON.parse(jsonMatch[0])
      if (Array.isArray(parsed.models) && parsed.models.length > 0) {
        return parsed.models
          .map((m: any) => {
            const id = (m.slug || m.id || '').trim()
            if (!id || !looksLikeModelId(id)) return null
            const displayName = m.displayName || m.display_name || m.name || id
            return {
              id,
              displayName,
              display_name: displayName,
              description: m.description || null,
              isDefault: Boolean(m.isDefault ?? m.is_default ?? m.priority === 1),
              is_default: Boolean(m.isDefault ?? m.is_default ?? m.priority === 1),
              source: 'dynamic_loader' as const,
            }
          })
          .filter(Boolean)
      }
    }
  } catch {
    // Ignore JSON parse errors and continue to text parsing
  }

  // 2. Parse text lines
  const lines = clean.split('\n')
  const results: TuiCatalogModelEntry[] = []
  const seenIds = new Set<string>()

  const noiseKeywords = [
    'available models',
    'select a model',
    'choose a model',
    'esc to interrupt',
    'type /help',
    'update available',
    'loading',
    'fetching',
    'session active',
    'turn complete',
    'tip:',
    'usage:',
    'options:',
    'commands:',
  ]

  for (const line of lines) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.length < 3) continue

    const lower = trimmed.toLowerCase()
    if (noiseKeywords.some(kw => lower.startsWith(kw) || (lower.includes(kw) && lower.length < 40))) {
      continue
    }
    // Skip command echoes and prompts
    if (lower === '/models' || lower === '/model' || lower.startsWith('>') || lower.startsWith('›')) {
      continue
    }
    // Skip CLI flag lines (--flag or -f)
    const strippedFlags = trimmed.replace(/^[*•·●+\-–—\s]+/, '')
    if (strippedFlags.startsWith('--') || /^-[a-zA-Z]/.test(strippedFlags)) {
      continue
    }

    // Check default marker
    const isDefault =
      trimmed.startsWith('*') ||
      lower.includes('(default)') ||
      lower.includes('[default]') ||
      lower.includes('(current)')

    // Strip markers like *, -, •, >, 1., etc.
    let content = trimmed.replace(/^[*•\-+>]\s*/, '').replace(/^\d+[\.)]\s*/, '').trim()
    content = content.replace(/\s*\((default|current)\)/i, '').replace(/\s*\[(default|current)\]/i, '').trim()

    let id = ''
    let displayName = ''
    let description: string | null = null

    if (content.includes('\t')) {
      const parts = content.split('\t').map(p => p.trim()).filter(Boolean)
      if (parts.length >= 1 && looksLikeModelId(parts[0])) {
        id = parts[0]
        displayName = parts[1] || parts[0]
        description = parts[2] || null
      }
    } else if (/\s{2,}/.test(content)) {
      const parts = content.split(/\s{2,}/).map(p => p.trim()).filter(Boolean)
      if (parts.length >= 2 && looksLikeModelId(parts[0])) {
        id = parts[0]
        const [title, ...descParts] = parts[1].split(' · ')
        displayName = title
        description = descParts.join(' · ') || parts.slice(2).join(' ') || null
      }
    } else if (/ - /.test(content)) {
      const parts = content.split(' - ').map(p => p.trim())
      if (looksLikeModelId(parts[0])) {
        id = parts[0]
        displayName = parts[0]
        description = parts.slice(1).join(' - ')
      } else if (looksLikeModelId(parts[parts.length - 1])) {
        id = parts[parts.length - 1]
        displayName = parts.slice(0, -1).join(' - ')
      }
    } else if (/\[([a-zA-Z0-9][-a-zA-Z0-9._/:]{0,80}[a-zA-Z0-9])\]/.test(content)) {
      const match = content.match(/\[([a-zA-Z0-9][-a-zA-Z0-9._/:]{0,80}[a-zA-Z0-9])\]/)
      if (match && looksLikeModelId(match[1])) {
        id = match[1]
        displayName = content.replace(match[0], '').trim() || id
      }
    } else if (/\(([a-zA-Z0-9][-a-zA-Z0-9._/:]{0,80}[a-zA-Z0-9])\)/.test(content)) {
      const match = content.match(/\(([a-zA-Z0-9][-a-zA-Z0-9._/:]{0,80}[a-zA-Z0-9])\)/)
      if (match && looksLikeModelId(match[1])) {
        id = match[1]
        displayName = content.replace(match[0], '').trim() || id
      }
    } else {
      const firstToken = content.split(/\s+/)[0]?.trim() || ''
      if (looksLikeModelId(firstToken)) {
        id = firstToken
        displayName = content
      }
    }

    if (id && looksLikeModelId(id) && !seenIds.has(id.toLowerCase())) {
      seenIds.add(id.toLowerCase())
      results.push({
        id,
        displayName: displayName || id,
        display_name: displayName || id,
        description,
        isDefault,
        is_default: isDefault,
        source: 'dynamic_loader',
      })
    }
  }

  return results
}

/**
 * Resolves the non-interactive CLI command used for dynamic model discovery.
 * - agy / antigravity: 'agy models'
 * - grok / grok-build: 'grok models'
 * - codex / openai-codex: 'codex debug models'
 * - claude / claude-code: 'claude'
 */
export function getCliDiscoveryCommand(
  agentKind: string,
  overrideCommand?: string | null,
): string {
  const trimmed = overrideCommand?.trim()
  if (trimmed && trimmed.toLowerCase() !== 'auto' && trimmed.toLowerCase() !== 'default') {
    return trimmed
  }
  const normalized = (agentKind || '').toLowerCase().trim()
  switch (normalized) {
    case 'claude':
    case 'claude-code':
      return 'claude'
    case 'agy':
    case 'antigravity':
      return 'agy models'
    case 'grok':
    case 'grok-build':
      return 'grok models'
    case 'codex':
    case 'openai-codex':
      return 'codex debug models'
    default:
      return `${agentKind} models`
  }
}

export interface DiscoverModelsCliOutcome {
  success: boolean
  catalog: TuiAgentModelCatalog | null
  error?: string
}

/**
 * Discovers the model catalog for an agent profile directly using the backend CLI dynamic loader.
 * Calls `api.refreshTuiAgentModels(agentKind)` or `api.getTuiAgentModels(agentKind, { refresh: true })`
 * without starting an ephemeral TUI session.
 */
export async function discoverAgentModelsViaCli(
  api: {
    refreshTuiAgentModels?: (agentKind: string) => Promise<unknown>
    getTuiAgentModels?: (agentKind: string, options?: { refresh?: boolean }) => Promise<unknown>
  },
  agentKind: string,
  existingCatalog: TuiAgentModelCatalog | null = null,
): Promise<DiscoverModelsCliOutcome> {
  try {
    let raw: unknown = null
    if (typeof api.refreshTuiAgentModels === 'function') {
      raw = await api.refreshTuiAgentModels(agentKind)
    } else if (typeof api.getTuiAgentModels === 'function') {
      raw = await api.getTuiAgentModels(agentKind, { refresh: true })
    } else {
      throw new Error('No refresh method available on API client')
    }
    const normalized = normalizeCatalog(raw, agentKind)
    return {
      success: normalized.models.length > 0,
      catalog: normalized,
      error: normalized.models.length === 0 ? 'No models returned by CLI' : undefined,
    }
  } catch (err: any) {
    const errorDetails = err?.message || String(err)
    return {
      success: false,
      catalog: existingCatalog,
      error: errorDetails,
    }
  }
}

/**
 * Resolves the interactive slash or prompt command used for model listing/discovery
 * for a specific agent CLI kind.
 *
 * Supported defaults:
 * - agy / antigravity: '/models'
 * - claude / claude-code: '/model'
 * - grok / grok-build: '/models'
 * - codex / openai-codex: '/models'
 * - custom / generic: '/models'
 *
 * Automatically determines default discovery command if override is empty, null, undefined,
 * or 'auto'/'default'. Pass 'none' to disable issuing any interactive prompt.
 */
export function getModelDiscoveryCommand(
  agentKind: string,
  overrideCommand?: string | null,
): string {
  const trimmed = overrideCommand?.trim()
  if (trimmed && trimmed.toLowerCase() !== 'auto' && trimmed.toLowerCase() !== 'default') {
    return trimmed
  }
  const normalized = (agentKind || '').toLowerCase().trim()
  switch (normalized) {
    case 'claude':
    case 'claude-code':
      return '/model'
    case 'agy':
    case 'antigravity':
      return '/models'
    case 'grok':
    case 'grok-build':
      return '/models'
    case 'codex':
    case 'openai-codex':
      return '/models'
    default:
      return '/models'
  }
}

/**
 * Discovers the model catalog for an agent profile by starting an ephemeral
 * interactive TUI session, issuing the agent-appropriate discovery command
 * (e.g. /models for agy/grok/codex, /model for claude, or custom/none),
 * executing it, capturing terminal interactions, and fetching the updated catalog
 * directly from the response of the agent TUI.
 * Ephemeral session is safely stopped on completion or failure.
 */
export async function discoverAgentModelsViaSession(
  api: {
    startManagedTuiSession?: (data: any) => Promise<any>
    promptTuiSession?: (arg1: any, arg2?: any, arg3?: any) => Promise<any>
    promptManagedTuiSession?: (id: string, payload: any) => Promise<any>
    keysManagedTuiSession?: (id: string, keys: string[]) => Promise<any>
    stopManagedTuiSession?: (id: string, reason?: string) => Promise<any>
    getManagedTuiSession?: (id: string) => Promise<{ session: any; screen?: string }>
    parseTuiAgentModels?: (agent: string, output: string) => Promise<any>
    refreshTuiAgentModels?: (agentKind: string) => Promise<unknown>
    getTuiAgentModels?: (agentKind: string, options?: { refresh?: boolean }) => Promise<unknown>
  },
  agentKind: string,
  existingCatalog: TuiAgentModelCatalog | null,
  optionsOrProgress?: DiscoverModelsOptions | ((progress: DiscoverModelsProgress) => void),
): Promise<DiscoverModelsOutcome> {
  const options: DiscoverModelsOptions =
    typeof optionsOrProgress === 'function'
      ? { onProgress: optionsOrProgress }
      : (optionsOrProgress || {})
  const onProgress = options.onProgress

  let tempSessionId: string | null = null
  let currentScreen = ''
  const tempSessionName = `discover_${agentKind}_${Date.now()}`

  const reportProgress = (status: string, screenText?: string) => {
    if (screenText !== undefined) {
      currentScreen = screenText
    }
    onProgress?.({
      sessionId: tempSessionId ?? undefined,
      status,
      screen: currentScreen,
    })
  }

  // 1. Determine the discovery command automatically for the agent (with override support)
  const rawCommand =
    options.discoveryCommand !== undefined
      ? options.discoveryCommand
      : existingCatalog?.discoveryCommand ?? existingCatalog?.discovery_command
  const command = getModelDiscoveryCommand(agentKind, rawCommand)
  const shouldSendPrompt = Boolean(command && command.toLowerCase() !== 'none')

  reportProgress(`Initializing TUI agent discovery session for '${agentKind}'...`)

  try {
    // 1. Launch ephemeral session with auto-approval
    if (typeof api.startManagedTuiSession === 'function') {
      try {
        reportProgress(`Launching interactive TUI session '${tempSessionName}'...`)
        const res = await api.startManagedTuiSession({
          agent: agentKind,
          sessionName: tempSessionName,
          autoApprove: true,
          cwd: '.',
        })
        if (res?.session?.id) {
          tempSessionId = res.session.id
          const initScreen = res.screen || res.initialTurn?.screenSnapshot || ''
          reportProgress(`Session active (ID: ${tempSessionId}). Interacting with ${agentKind}...`, initScreen)
        }
      } catch (err: any) {
        console.warn('Ephemeral discovery session launch error, falling back to direct refresh:', err)
        reportProgress(`Interactive session launch skipped (${err?.message || err}). Falling back to direct CLI discovery...`)
      }
    }

    // Read initial screen if available
    if (tempSessionId && typeof api.getManagedTuiSession === 'function') {
      try {
        const snap = await api.getManagedTuiSession(tempSessionId)
        if (snap?.screen) {
          reportProgress(`Captured live terminal screen from ${agentKind}...`, snap.screen)
        }
      } catch {
        // Ignore
      }
    }

    // 2. Send the discovery command to the agent and execute it
    let executedResponse = ''
    let executedSnapshot = ''
    if (tempSessionId && shouldSendPrompt) {
      reportProgress(`Executing discovery command '${command}' in TUI agent session...`, currentScreen)
      try {
        let turnRes: any = null
        if (typeof api.promptManagedTuiSession === 'function') {
          turnRes = await api.promptManagedTuiSession(tempSessionId, {
            prompt: command,
            wait: true,
            timeoutSecs: 15,
          })
        } else if (typeof api.promptTuiSession === 'function') {
          if (api.promptTuiSession.length >= 3) {
            turnRes = await (api.promptTuiSession as any)('', tempSessionId, {
              prompt: command,
              wait: true,
              timeoutSecs: 15,
            })
          } else {
            turnRes = await api.promptTuiSession(tempSessionId, {
              prompt: command,
              wait: true,
              timeoutSecs: 15,
            })
          }
        }
        if (turnRes) {
          executedResponse = turnRes.response || turnRes.initialTurn?.response || ''
          executedSnapshot = turnRes.screenSnapshot || turnRes.screen || ''
          const combined = [executedResponse, executedSnapshot].filter(Boolean).join('\n')
          if (combined) {
            reportProgress(`Received response from TUI agent...`, combined)
          }
        }
      } catch (promptErr: any) {
        console.warn('Ephemeral session prompt warning:', promptErr)
        reportProgress(`Prompt execution warning (${promptErr?.message || promptErr}), polling terminal output...`)
      }
    } else if (!shouldSendPrompt) {
      reportProgress(`Skipping interactive prompt for agent '${agentKind}'...`, currentScreen)
    }

    // Allow terminal updates to render and read latest screen
    for (let i = 0; i < 3; i++) {
      await new Promise(r => setTimeout(r, 200))
      if (tempSessionId && typeof api.getManagedTuiSession === 'function') {
        try {
          const snap = await api.getManagedTuiSession(tempSessionId)
          if (snap?.screen) {
            reportProgress(`TUI Agent interactive session active...`, snap.screen)
          }
        } catch {
          // Ignore
        }
      }
    }

    // 3. Fetch the model catalog from the response of agent TUI
    reportProgress(`Extracting model catalog from TUI agent response...`, currentScreen)
    const combinedOutput = [executedResponse, executedSnapshot, currentScreen].filter(Boolean).join('\n')
    let discoveredFromTui: TuiCatalogModelEntry[] = parseModelsFromTuiOutput(combinedOutput, agentKind)

    // Attempt backend parser endpoint if available and client-side found nothing
    if (discoveredFromTui.length === 0 && typeof api.parseTuiAgentModels === 'function' && combinedOutput.trim()) {
      try {
        const parsedRes: any = await api.parseTuiAgentModels(agentKind, combinedOutput)
        const normalizedParsed = normalizeCatalog(parsedRes, agentKind)
        if (normalizedParsed.models.length > 0) {
          discoveredFromTui = normalizedParsed.models
        }
      } catch {
        // Fall back to CLI refresh
      }
    }

    if (discoveredFromTui.length > 0) {
      const defaultModel = discoveredFromTui.find(m => m.isDefault)?.id || discoveredFromTui[0]?.id || null
      const catalog: TuiAgentModelCatalog = {
        agent: agentKind,
        models: discoveredFromTui,
        defaultModel,
        default_model: defaultModel,
        discoveryCommand: command,
        discovery_command: command,
        supportsFreeText: existingCatalog?.supportsFreeText ?? true,
        supports_free_text: existingCatalog?.supports_free_text ?? true,
        isFallback: false,
        is_fallback: false,
        cachedAtUnixSecs: Math.floor(Date.now() / 1000),
        cached_at_unix_secs: Math.floor(Date.now() / 1000),
      }
      const modelListStr = catalog.models.map(m => `  • ${m.id} (${m.displayName})`).join('\n')
      const summaryText = `\n[TUI Discovery Result]\nDiscovered ${catalog.models.length} model(s) from TUI agent response:\n${modelListStr}`
      const finalScreen = currentScreen ? `${currentScreen}\n${summaryText}` : summaryText
      reportProgress(`Discovered ${catalog.models.length} models from TUI agent response.`, finalScreen)
      return {
        success: true,
        catalog,
        screen: finalScreen,
        sessionId: tempSessionId ?? undefined,
      }
    }

    // Fallback: If no models could be parsed from the interactive TUI response, query backend catalog
    reportProgress(`Querying dynamic model discovery catalog for '${agentKind}'...`, currentScreen)
    let raw: unknown = null
    if (typeof api.refreshTuiAgentModels === 'function') {
      raw = await api.refreshTuiAgentModels(agentKind)
    } else if (typeof api.getTuiAgentModels === 'function') {
      raw = await api.getTuiAgentModels(agentKind, { refresh: true })
    }

    const normalized = normalizeCatalog(raw, agentKind)
    if (normalized.models.length > 0) {
      const modelListStr = normalized.models.map(m => `  • ${m.id} (${m.displayName})`).join('\n')
      const summaryText = `\n[TUI Discovery Result]\nDiscovered ${normalized.models.length} model(s) for ${agentKind}:\n${modelListStr}`
      const finalScreen = currentScreen ? `${currentScreen}\n${summaryText}` : summaryText
      reportProgress(`Discovered ${normalized.models.length} models for ${agentKind}.`, finalScreen)
      return {
        success: true,
        catalog: normalized,
        screen: finalScreen,
        sessionId: tempSessionId ?? undefined,
      }
    }

    return {
      success: Boolean(existingCatalog && existingCatalog.models.length > 0),
      catalog: existingCatalog,
      error: `No models discovered for agent CLI '${agentKind}'.`,
      screen: currentScreen,
      sessionId: tempSessionId ?? undefined,
    }
  } catch (e: any) {
    const errorDetails = e?.message || String(e)
    return {
      success: false,
      catalog: existingCatalog,
      error: `Failed to discover models for ${agentKind}: ${errorDetails}`,
      screen: currentScreen,
      sessionId: tempSessionId ?? undefined,
    }
  } finally {
    // 4. Terminate ephemeral session safely
    if (tempSessionId && typeof api.stopManagedTuiSession === 'function') {
      try {
        await api.stopManagedTuiSession(tempSessionId, 'discovery_complete')
      } catch {
        // Ignore cleanup failure
      }
    }
  }
}
