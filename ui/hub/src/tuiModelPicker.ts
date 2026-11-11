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
