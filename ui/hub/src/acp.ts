import { absoluteServerBase } from '@aaif/goose-hub-core/url'
import {
  GooseClient,
  type GooseClientCallbacks,
  type SourceEntry,
  type SourceScope,
} from '@openduck/sdk'
import { PROTOCOL_VERSION, type SessionNotification } from '@agentclientprotocol/sdk'

export type ChatCallbacks = {
  onUpdate: (notification: SessionNotification) => void
  onStatus: (message: string) => void
}

const idleCallbacks: GooseClientCallbacks = {
  requestPermission: async () => ({ outcome: { outcome: 'cancelled' } }),
  sessionUpdate: async () => {},
}

type CachedSourcesClient = {
  key: string
  client: GooseClient
}

let cachedSourcesClient: CachedSourcesClient | null = null
let pendingSourcesClient: { key: string; promise: Promise<GooseClient> } | null = null

function connectionKey(baseUrl: string, secretKey: string): string {
  return `${baseUrl.replace(/\/+$/, '')}\0${secretKey}`
}

function hubClientCallbacks(callbacks?: ChatCallbacks): GooseClientCallbacks {
  if (!callbacks) return idleCallbacks
  return {
    requestPermission: async () => ({ outcome: { outcome: 'cancelled' } }),
    sessionUpdate: async notification => {
      const update = notification.update as any
      if (update?.sessionUpdate === 'status_message' && update?.status?.type === 'progress') {
        callbacks.onStatus(update.status.message)
      }
      callbacks.onUpdate(notification)
    },
  }
}

async function initializeGooseClient(
  baseUrl: string,
  secretKey: string,
  callbacks?: ChatCallbacks,
): Promise<GooseClient> {
  const client = new GooseClient(() => hubClientCallbacks(callbacks), {
    url: absoluteServerBase(baseUrl),
    secretKey,
  })
  await client.initialize({
    protocolVersion: PROTOCOL_VERSION,
    clientInfo: { name: 'goose-hub', version: '0.1.0' },
    clientCapabilities: {},
  })
  return client
}

/** Creates an ACP session scoped to a registered project. The server remains the source of truth for tools and permissions. */
export async function createProjectSession(
  baseUrl: string,
  secretKey: string,
  projectId: string,
  cwd: string,
  callbacks: ChatCallbacks,
) {
  const client = await initializeGooseClient(baseUrl, secretKey, callbacks)
  const session = await client.newSession({
    cwd,
    mcpServers: [],
    _meta: { projectId, client: 'goose-hub' },
  })
  return { client, sessionId: session.sessionId }
}

export async function getSourcesClient(baseUrl: string, secretKey: string): Promise<GooseClient> {
  const key = connectionKey(baseUrl, secretKey)
  if (cachedSourcesClient?.key === key) {
    return cachedSourcesClient.client
  }
  if (pendingSourcesClient?.key === key) {
    return pendingSourcesClient.promise
  }

  const promise: Promise<GooseClient> = (async () => {
    try {
      const client = await initializeGooseClient(baseUrl, secretKey)
      cachedSourcesClient = { key, client }
      return client
    } catch (cause) {
      cachedSourcesClient = null
      throw cause
    } finally {
      if (pendingSourcesClient?.key === key) {
        pendingSourcesClient = null
      }
    }
  })()
  pendingSourcesClient = { key, promise }
  return promise
}

export async function listRuleSources(
  baseUrl: string,
  secretKey: string,
  projectDir: string,
): Promise<SourceEntry[]> {
  const client = await getSourcesClient(baseUrl, secretKey)
  const response = await client.goose.sourcesList_unstable({
    type: 'rule',
    projectDir,
  })
  return [...response.sources].sort(
    (a, b) =>
      Number(a.global) - Number(b.global) ||
      a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }) ||
      a.path.localeCompare(b.path),
  )
}

export async function createRuleSource(
  baseUrl: string,
  secretKey: string,
  input: {
    name: string
    description: string
    content: string
    global: boolean
    projectDir: string
    properties: Record<string, unknown>
  },
): Promise<SourceEntry> {
  const client = await getSourcesClient(baseUrl, secretKey)
  const target: SourceScope = input.global
    ? { scope: 'global' }
    : { scope: 'projectDir', projectDir: input.projectDir }
  const response = await client.goose.sourcesCreate_unstable({
    type: 'rule',
    name: input.name,
    description: input.description,
    content: input.content,
    target,
    properties: input.properties,
  })
  return response.source
}

export async function updateRuleSource(
  baseUrl: string,
  secretKey: string,
  input: {
    path: string
    name: string
    description: string
    content: string
    properties: Record<string, unknown>
  },
): Promise<SourceEntry> {
  const client = await getSourcesClient(baseUrl, secretKey)
  const response = await client.goose.sourcesUpdate_unstable({
    type: 'rule',
    path: input.path,
    name: input.name,
    description: input.description,
    content: input.content,
    properties: input.properties,
  })
  return response.source
}

export async function deleteRuleSource(
  baseUrl: string,
  secretKey: string,
  path: string,
): Promise<void> {
  const client = await getSourcesClient(baseUrl, secretKey)
  await client.goose.sourcesDelete_unstable({
    type: 'rule',
    path,
  })
}

