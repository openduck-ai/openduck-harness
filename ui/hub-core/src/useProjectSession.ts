import { useCallback, useEffect, useMemo, useSyncExternalStore } from 'react'
import {
  projectSessionRegistry,
  type ProjectSessionOptions,
  type ProjectSessionState,
} from './sessionManager.ts'
import type { PermissionAction } from './types.ts'

export type { ProjectSessionOptions, ProjectSessionState }

export function useProjectSession(options: ProjectSessionOptions) {
  const instance = useMemo(() => {
    return projectSessionRegistry.getOrCreate(options)
  }, [options.baseUrl, options.client, options.cwd, options.projectId, options.secretKey, options.sessionId])

  useEffect(() => {
    instance.updateOptions(options)
  }, [instance, options])

  const state = useSyncExternalStore(
    onStoreChange => projectSessionRegistry.subscribe(onStoreChange),
    () => instance.getState(),
    () => instance.getState(),
  )

  const sendPrompt = useCallback(
    async (text: string) => {
      await instance.sendPrompt(text)
    },
    [instance],
  )

  const resolvePermission = useCallback(
    (action: PermissionAction) => {
      instance.resolvePermission(action)
    },
    [instance],
  )

  const reset = useCallback(
    async (newSession = true) => {
      await instance.reset(newSession)
    },
    [instance],
  )

  const loadSession = useCallback(
    async (sessionId: string) => {
      await instance.loadSession(sessionId)
    },
    [instance],
  )

  return {
    ...state,
    sendPrompt,
    resolvePermission,
    reset,
    loadSession,
  }
}

const emptyAllStates: Record<string, ProjectSessionState> = {}

export function useAllProjectSessions(): Record<string, ProjectSessionState> {
  return useSyncExternalStore(
    onStoreChange => projectSessionRegistry.subscribe(onStoreChange),
    () => projectSessionRegistry.getAllStates(),
    () => emptyAllStates,
  )
}
