import type { SessionConfigOption } from '@agentclientprotocol/sdk'

export function configOptionsFromSessionUpdate(update: unknown): SessionConfigOption[] | null {
  if (!update || typeof update !== 'object') return null
  const record = update as Record<string, unknown>
  if (record.sessionUpdate !== 'config_option_update') return null
  return Array.isArray(record.configOptions) ? (record.configOptions as SessionConfigOption[]) : null
}

export function configOptionsFromSessionResponse(response: {
  configOptions?: SessionConfigOption[] | null
} | null | undefined): SessionConfigOption[] | null {
  if (!response?.configOptions) return null
  return response.configOptions
}
