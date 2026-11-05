import { useCallback, useEffect, useState } from 'react'
import type { ProjectDirectory, ProjectRoot } from '@aaif/goose-hub-core'
export type RootError = { kind: 'raw'; message: string } | { kind: 'fallback'; key: string }

export interface ProjectRootsApi {
  listProjectRoots: () => Promise<{ roots: ProjectRoot[] }>
  addProjectRoot: (input: {
    path: string
    allowUntrustedPath?: boolean
  }) => Promise<{ root: ProjectRoot }>
  removeProjectRoot: (path: string) => Promise<void>
  listProjectRootDirectories: (root: string) => Promise<{
    root: string
    directories: ProjectDirectory[]
    truncated: boolean
  }>
  createProjectRootDirectory: (input: {
    root: string
    name: string
  }) => Promise<{ directory: ProjectDirectory; created: boolean }>
}

function describeRootError(cause: unknown, fallbackKey: string): RootError {
  if (cause instanceof Error && cause.message) {
    if (cause.message.includes('allowUntrustedPath') || cause.message.includes('outside $HOME')) {
      return { kind: 'fallback', key: 'projectRoots.errors.outsideHome' }
    }
    return { kind: 'raw', message: cause.message }
  }
  return { kind: 'fallback', key: fallbackKey }
}

function sortRoots(roots: ProjectRoot[]): ProjectRoot[] {
  return [...roots].sort((left, right) => left.path.localeCompare(right.path))
}

export function useProjectRoots(api: ProjectRootsApi) {
  const [roots, setRoots] = useState<ProjectRoot[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<RootError | null>(null)

  useEffect(() => {
    let ignore = false
    const load = async () => {
      setLoading(true)
      setError(null)
      try {
        const response = await api.listProjectRoots()
        if (!ignore) setRoots(sortRoots(response.roots))
      } catch (cause) {
        if (!ignore) {
          setRoots([])
          setError(describeRootError(cause, 'projectRoots.errors.loadFailed'))
        }
      } finally {
        if (!ignore) setLoading(false)
      }
    }
    void load()
    return () => {
      ignore = true
    }
  }, [api])

  const addRoot = useCallback(
    async (path: string, allowUntrustedPath: boolean) => {
      setError(null)
      try {
        const response = await api.addProjectRoot({ path, allowUntrustedPath })
        setRoots(current =>
          sortRoots([...current.filter(root => root.path !== response.root.path), response.root]),
        )
        return response.root
      } catch (cause) {
        const message = describeRootError(cause, 'projectRoots.errors.addFailed')
        setError(message)
        throw cause
      }
    },
    [api],
  )

  const removeRoot = useCallback(
    async (path: string) => {
      setError(null)
      try {
        await api.removeProjectRoot(path)
        setRoots(current => current.filter(root => root.path !== path))
      } catch (cause) {
        const message = describeRootError(cause, 'projectRoots.errors.removeFailed')
        setError(message)
        throw cause
      }
    },
    [api],
  )

  return { roots, loading, error, addRoot, removeRoot, clearError: () => setError(null) }
}
