import { GooseClient, type GooseClientCallbacks } from '@openduck/sdk'
import {
  PROTOCOL_VERSION,
  type RequestPermissionRequest,
  type RequestPermissionResponse,
} from '@agentclientprotocol/sdk'
import { permissionKey, permissionResponseForAction } from './permissions.ts'
import { absoluteServerBase } from './url.ts'
import { applyAcpSessionUpdate, finalizeStreaming, newMessageId } from './transcript.ts'
import type { ChatMessage, PendingPermission, PermissionAction } from './types.ts'

export interface ProjectSessionOptions {
  baseUrl: string
  secretKey: string
  projectId: string
  cwd: string
  client: 'goose-hub' | 'goose-mobile'
  sessionId?: string | null
}

export interface ProjectSessionState {
  projectId: string
  cwd: string
  sessionId: string | null
  messages: ChatMessage[]
  isPrompting: boolean
  statusLine: string | null
  pendingPermission: PendingPermission | null
  error: string | null
}

type Listener = () => void

function storageKey(projectId: string): string {
  return `goose-hub:session:${projectId}`
}

function getStoredSessionId(projectId: string): string | null {
  if (typeof localStorage === 'undefined') return null
  return localStorage.getItem(storageKey(projectId))
}

function setStoredSessionId(projectId: string, sessionId: string | null): void {
  if (typeof localStorage === 'undefined') return
  if (sessionId) {
    localStorage.setItem(storageKey(projectId), sessionId)
  } else {
    localStorage.removeItem(storageKey(projectId))
  }
}

export class ProjectSessionInstance {
  options: ProjectSessionOptions
  client: GooseClient | null = null
  sessionId: string | null = null
  messages: ChatMessage[] = []
  isPrompting = false
  statusLine: string | null = null
  pendingPermission: PendingPermission | null = null
  error: string | null = null
  permissionResolvers = new Map<string, (response: RequestPermissionResponse) => void>()
  private connectingPromise: Promise<GooseClient> | null = null
  private cachedState: ProjectSessionState | null = null
  private readonly onChange: () => void

  constructor(options: ProjectSessionOptions, onChange: () => void) {
    this.options = options
    this.onChange = onChange
    this.sessionId = options.sessionId ?? getStoredSessionId(options.projectId)
  }

  updateOptions(options: ProjectSessionOptions) {
    this.options = options
  }

  getState(): ProjectSessionState {
    if (!this.cachedState) {
      this.cachedState = {
        projectId: this.options.projectId,
        cwd: this.options.cwd,
        sessionId: this.sessionId,
        messages: this.messages,
        isPrompting: this.isPrompting,
        statusLine: this.statusLine,
        pendingPermission: this.pendingPermission,
        error: this.error,
      }
    }
    return this.cachedState
  }

  private notify() {
    this.cachedState = null
    this.onChange()
  }

  clearPermissions() {
    for (const resolve of this.permissionResolvers.values()) {
      resolve({ outcome: { outcome: 'cancelled' } })
    }
    this.permissionResolvers.clear()
    this.pendingPermission = null
    this.notify()
  }

  async ensureClient(): Promise<GooseClient> {
    if (this.client && this.sessionId) {
      return this.client
    }

    if (this.connectingPromise) {
      return this.connectingPromise
    }

    this.connectingPromise = (async () => {
      try {
        const callbacks: GooseClientCallbacks = {
          sessionUpdate: async notification => {
            if (this.sessionId && String(notification.sessionId) !== this.sessionId) return
            const update = notification.update as any
            if (update?.sessionUpdate === 'status_message' && update?.status?.type === 'progress') {
              this.statusLine = update.status.message
              this.notify()
            }
            this.messages = applyAcpSessionUpdate(this.messages, notification.update)
            this.notify()
          },
          requestPermission: async (request: RequestPermissionRequest) => {
            if (this.sessionId && String(request.sessionId) !== this.sessionId) {
              return { outcome: { outcome: 'cancelled' } }
            }
            const key = permissionKey(request.sessionId, request.toolCall.toolCallId)
            return new Promise<RequestPermissionResponse>(resolve => {
              this.permissionResolvers.set(key, resolve)
              this.pendingPermission = { key, request }
              this.notify()
            })
          },
        }

        const client = new GooseClient(() => callbacks, {
          url: absoluteServerBase(this.options.baseUrl),
          secretKey: this.options.secretKey,
        })

        await client.initialize({
          protocolVersion: PROTOCOL_VERSION,
          clientInfo: { name: this.options.client, version: '0.1.0' },
          clientCapabilities: {},
        })

        const targetSessionId = this.sessionId ?? this.options.sessionId ?? getStoredSessionId(this.options.projectId)
        let sessionCreated = false

        if (targetSessionId) {
          try {
            await client.loadSession({
              sessionId: targetSessionId,
              cwd: this.options.cwd,
              mcpServers: [],
            })
            this.sessionId = targetSessionId
            setStoredSessionId(this.options.projectId, targetSessionId)
            sessionCreated = true
          } catch {
            // If loading past session fails, create a new one
          }
        }

        if (!sessionCreated) {
          const session = await client.newSession({
            cwd: this.options.cwd,
            mcpServers: [],
            _meta: { projectId: this.options.projectId, client: this.options.client },
          })
          this.sessionId = session.sessionId
          setStoredSessionId(this.options.projectId, session.sessionId)
        }

        this.client = client
        this.notify()
        return client
      } finally {
        this.connectingPromise = null
      }
    })()

    return this.connectingPromise
  }

  async sendPrompt(text: string) {
    const trimmed = text.trim()
    if (!trimmed) return
    this.error = null
    this.isPrompting = true
    this.statusLine = 'Sending…'
    this.messages = [
      ...finalizeStreaming(this.messages),
      { id: newMessageId('user'), role: 'user', text: trimmed },
    ]
    this.notify()

    try {
      const client = await this.ensureClient()
      const sessionId = this.sessionId
      if (!sessionId) throw new Error('Session was not created')
      await client.prompt({
        sessionId,
        prompt: [{ type: 'text', text: trimmed }],
      })
      this.messages = finalizeStreaming(this.messages)
      this.statusLine = null
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : 'Prompt failed'
      this.statusLine = null
    } finally {
      this.isPrompting = false
      this.notify()
    }
  }

  resolvePermission(action: PermissionAction) {
    const pending = this.pendingPermission
    if (!pending) return
    const resolve = this.permissionResolvers.get(pending.key)
    this.permissionResolvers.delete(pending.key)
    this.pendingPermission = null
    this.notify()
    resolve?.(permissionResponseForAction(pending.request, action))
  }

  async reset(newSession = true) {
    this.clearPermissions()
    this.client = null
    this.sessionId = null
    setStoredSessionId(this.options.projectId, null)
    this.messages = []
    this.isPrompting = false
    this.statusLine = null
    this.error = null
    this.notify()
    if (newSession) {
      await this.ensureClient().catch(() => {})
    }
  }

  async loadSession(sessionId: string) {
    this.clearPermissions()
    this.client = null
    this.sessionId = sessionId
    setStoredSessionId(this.options.projectId, sessionId)
    this.messages = []
    this.isPrompting = false
    this.statusLine = null
    this.error = null
    this.notify()
    await this.ensureClient().catch(cause => {
      this.error = cause instanceof Error ? cause.message : 'Unable to load session'
      this.notify()
    })
  }
}

class ProjectSessionRegistry {
  private sessions = new Map<string, ProjectSessionInstance>()
  private listeners = new Set<Listener>()
  private cachedSnapshot: Record<string, ProjectSessionState> = {}

  private notify() {
    this.updateSnapshot()
    for (const listener of this.listeners) {
      listener()
    }
  }

  private updateSnapshot() {
    const states: Record<string, ProjectSessionState> = {}
    for (const [id, instance] of this.sessions.entries()) {
      states[id] = instance.getState()
    }
    this.cachedSnapshot = states
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  getOrCreate(options: ProjectSessionOptions): ProjectSessionInstance {
    let session = this.sessions.get(options.projectId)
    if (!session) {
      session = new ProjectSessionInstance(options, () => this.notify())
      this.sessions.set(options.projectId, session)
      this.notify()
    } else {
      session.updateOptions(options)
    }
    return session
  }

  getSession(projectId: string): ProjectSessionInstance | undefined {
    return this.sessions.get(projectId)
  }

  getAllStates(): Record<string, ProjectSessionState> {
    return this.cachedSnapshot
  }
}

export const projectSessionRegistry = new ProjectSessionRegistry()
