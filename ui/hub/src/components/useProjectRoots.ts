import { useCallback, useEffect, useState } from 'react'
import type { ProjectDirectory, ProjectRoot } from '@aaif/goose-hub-core'

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

function describeRootError(cause: unknown, fallback: string): string {
  const message = cause instanceof Error && cause.message ? cause.message : fallback
  if (message.includes('allowUntrustedPath') || message.includes('outside $HOME')) {
    return 'That folder is outside the home directory. Allow roots outside the home directory, then try again.'
  }
  return message
}

function sortRoots(roots: ProjectRoot[]): ProjectRoot[] {
  return [...roots].sort((left, right) => left.path.localeCompare(right.path))
}

export function useProjectRoots(api: ProjectRootsApi) {
  const [roots, setRoots] = useState<ProjectRoot[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')

  useEffect(() => {
    let ignore = false
    const load = async () => {
      setLoading(true)
      setError('')
      try {
        const response = await api.listProjectRoots()
        if (!ignore) setRoots(sortRoots(response.roots))
      } catch (cause) {
        if (!ignore) {
          setRoots([])
          setError(describeRootError(cause, 'Unable to load project roots'))
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
      setError('')
      try {
        const response = await api.addProjectRoot({ path, allowUntrustedPath })
        setRoots(current =>
          sortRoots([...current.filter(root => root.path !== response.root.path), response.root]),
        )
        return response.root
      } catch (cause) {
        const message = describeRootError(cause, 'Unable to add project root')
        setError(message)
        throw new Error(message)
      }
    },
    [api],
  )

  const removeRoot = useCallback(
    async (path: string) => {
      setError('')
      try {
        await api.removeProjectRoot(path)
        setRoots(current => current.filter(root => root.path !== path))
      } catch (cause) {
        const message = describeRootError(cause, 'Unable to remove project root')
        setError(message)
        throw new Error(message)
      }
    },
    [api],
  )

  return { roots, loading, error, addRoot, removeRoot, clearError: () => setError('') }
}
