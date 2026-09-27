import { useCallback, useEffect, useMemo, useState, lazy, Suspense } from 'react'
import {
  useAllProjectSessions,
  type Project,
  type ProjectDetail as ProjectDetailType,
  type ProjectInput,
} from '@aaif/goose-hub-core'
import { createApi } from './api'
import { Header, type NavPage } from './components/Header'
import { ProjectList } from './components/ProjectList'
import { RegisterProjectModal } from './components/RegisterProjectModal'
import { TabSkeleton, ProjectDetailHeaderSkeleton } from './components/Skeletons'
import {
  LayoutDashboard,
  Zap,
  ShieldAlert,
  X,
  Columns2,
  Folder,
} from 'lucide-react'
import './styles.css'

// Lazy-load ProjectDetail to keep initial bundle ultra lightweight
const ProjectDetail = lazy(() =>
  import('./components/ProjectDetail').then(m => ({ default: m.ProjectDetail })),
)

// Lazy-load ServerSettings to reduce initial bundle size
const ServerSettings = lazy(() =>
  import('./components/ServerSettings').then(m => ({ default: m.ServerSettings })),
)

export default function App() {
  const [currentPage, setCurrentPage] = useState<NavPage>('projects')
  const [baseUrl, setBaseUrl] = useState(() => localStorage.getItem('goose-hub-url') ?? '')
  const [secret, setSecret] = useState(() => localStorage.getItem('goose-hub-secret') ?? '')
  const api = useMemo(
    () => createApi(baseUrl || window.location.origin, secret),
    [baseUrl, secret],
  )
  const [projects, setProjects] = useState<Project[]>([])
  const [openProjects, setOpenProjects] = useState<ProjectDetailType[]>([])
  const [activeSlug, setActiveSlug] = useState<string | null>(null)
  const [splitSlug, setSplitSlug] = useState<string | null>(null)
  const [showRegisterModal, setShowRegisterModal] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [isConnected, setIsConnected] = useState(false)

  const allSessions = useAllProjectSessions()

  const load = useCallback(async () => {
    setBusy(true)
    setError('')
    try {
      const res = await api.listProjects()
      setProjects(res.projects)
      setIsConnected(true)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Unable to connect to Goose server')
      setIsConnected(false)
    } finally {
      setBusy(false)
    }
  }, [api])

  useEffect(() => {
    let ignore = false
    const loadProjects = async () => {
      setBusy(true)
      setError('')
      try {
        const res = await api.listProjects()
        if (!ignore) {
          setProjects(res.projects)
          setIsConnected(true)
        }
      } catch (cause) {
        if (!ignore) {
          setError(cause instanceof Error ? cause.message : 'Unable to connect to Goose server')
          setIsConnected(false)
        }
      } finally {
        if (!ignore) {
          setBusy(false)
        }
      }
    }

    void loadProjects()
    return () => {
      ignore = true
    }
  }, [api])

  const open = async (project: Project) => {
    setError('')

    // 1. If project is already open, immediately activate its tab
    const alreadyOpen = openProjects.find(p => p.project.slug === project.slug)
    if (alreadyOpen) {
      setActiveSlug(project.slug)
      setCurrentPage('projects')
      return
    }

    // 2. Optimistically open tab immediately with initial metadata (0ms perceived delay)
    const initialDetail: ProjectDetailType = {
      project,
      insight: {
        exists: false,
        entryCount: 0,
        entries: [],
        truncated: false,
      },
    }

    setOpenProjects(current => [...current, initialDetail])
    setActiveSlug(project.slug)
    setCurrentPage('projects')

    // 3. Fetch project details in background in lightweight lazy mode
    try {
      const detail = await api.getProject(project.slug, { lazy: true })
      setOpenProjects(current =>
        current.map(p => (p.project.slug === project.slug ? detail : p)),
      )
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Unable to load project details')
    }
  }

  const closeTab = (slug: string, event?: React.MouseEvent) => {
    event?.stopPropagation()
    setOpenProjects(current => current.filter(p => p.project.slug !== slug))
    if (activeSlug === slug) {
      const remaining = openProjects.filter(p => p.project.slug !== slug)
      setActiveSlug(remaining[remaining.length - 1]?.project.slug ?? null)
    }
    if (splitSlug === slug) {
      setSplitSlug(null)
    }
  }

  const handleCreateProject = async (input: ProjectInput) => {
    await api.createProject(input)
    await load()
  }

  const handleArchiveProject = async (slug: string) => {
    const current = projects.find(p => p.slug === slug)
    const newStatus = current?.status === 'archived' ? 'active' : 'archived'
    await api.archiveProject(slug)
    if (newStatus === 'archived') {
      closeTab(slug)
    }
    await load()
  }

  const handleDeleteProject = async (slug: string) => {
    await api.deleteProject(slug)
    closeTab(slug)
    await load()
  }

  const handleSaveConnection = (url: string, newSecret: string) => {
    setBaseUrl(url)
    setSecret(newSecret)
    localStorage.setItem('goose-hub-url', url)
    localStorage.setItem('goose-hub-secret', newSecret)
  }

  const activeProject = openProjects.find(p => p.project.slug === activeSlug) ?? null
  const splitProject = splitSlug
    ? openProjects.find(p => p.project.slug === splitSlug) ?? null
    : null

  const activeLiveSessionsCount = Object.values(allSessions).filter(
    s => s?.isPrompting || s?.sessionId,
  ).length

  return (
    <div className="app-root">
      <Header
        currentPage={currentPage}
        onSelectPage={page => setCurrentPage(page)}
        isConnected={isConnected}
        onNewProject={() => {
          setCurrentPage('projects')
          setShowRegisterModal(true)
        }}
        openProjectsCount={openProjects.length}
      />

      <main className="app-main-content">
        {error && <p className="error global-error">{error}</p>}

        {currentPage === 'projects' && (
          <div className="page-view projects-page-view">
            {openProjects.length > 0 && (
              <nav className="tab-bar">
                <button
                  type="button"
                  className={`tab ${activeSlug === null ? 'active' : ''}`}
                  onClick={() => setActiveSlug(null)}
                >
                  <LayoutDashboard size={14} />
                  <span>All Projects</span>
                </button>
                {openProjects.map(p => {
                  const slug = p.project.slug
                  const sState = allSessions[slug]
                  const isWorking = sState?.isPrompting
                  const hasPermission = !!sState?.pendingPermission
                  const isActive = activeSlug === slug
                  const isSplit = splitSlug === slug

                  return (
                    <div
                      key={slug}
                      className={`tab project-tab ${isActive ? 'active' : ''} ${
                        isSplit ? 'split' : ''
                      }`}
                      onClick={() => setActiveSlug(slug)}
                    >
                      <Folder size={14} className="tab-icon" />
                      <span className="tab-title">{p.project.title || slug}</span>
                      {isWorking && (
                        <span className="badge working-pulse-badge mini-badge" title="Agent is working…">
                          <Zap size={11} className="pulse-icon" />
                        </span>
                      )}
                      {hasPermission && (
                        <span className="badge permission-action-badge mini-badge" title="Permission required">
                          <ShieldAlert size={11} />
                        </span>
                      )}
                      <button
                        type="button"
                        className="tab-close"
                        title="Close tab"
                        onClick={e => closeTab(slug, e)}
                      >
                        <X size={12} />
                      </button>
                    </div>
                  )
                })}
                {openProjects.length > 1 && (
                  <div className="tab-actions">
                    <div className="split-select-wrapper">
                      <Columns2 size={13} className="split-icon" />
                      <select
                        className="split-select"
                        value={splitSlug ?? ''}
                        onChange={e => setSplitSlug(e.target.value || null)}
                        title="Split View side-by-side with another project"
                      >
                        <option value="">No Split</option>
                        {openProjects
                          .filter(p => p.project.slug !== activeSlug)
                          .map(p => (
                            <option key={p.project.slug} value={p.project.slug}>
                              Split with: {p.project.title || p.project.slug}
                            </option>
                          ))}
                      </select>
                    </div>
                  </div>
                )}
              </nav>
            )}

            {activeProject ? (
              <div className={`detail-container ${splitProject ? 'split-layout' : ''}`}>
                <div className="detail-pane">
                  <Suspense
                    fallback={
                      <div className="tab-content" style={{ padding: '1.5rem' }}>
                        <ProjectDetailHeaderSkeleton />
                        <div className="mt-4">
                          <TabSkeleton tab="harness" />
                        </div>
                      </div>
                    }
                  >
                    <ProjectDetail
                      detail={activeProject}
                      baseUrl={baseUrl || window.location.origin}
                      secret={secret}
                      api={api}
                      onBack={() => setActiveSlug(null)}
                      onArchive={() => handleArchiveProject(activeProject.project.slug)}
                      onDelete={() => handleDeleteProject(activeProject.project.slug)}
                      onReload={async () => {
                        const refreshed = await api.getProject(activeProject.project.slug)
                        setOpenProjects(current =>
                          current.map(p =>
                            p.project.slug === activeProject.project.slug ? refreshed : p,
                          ),
                        )
                      }}
                    />
                  </Suspense>
                </div>
                {splitProject && (
                  <div className="detail-pane split-pane">
                    <Suspense
                      fallback={
                        <div className="tab-content" style={{ padding: '1.5rem' }}>
                          <ProjectDetailHeaderSkeleton />
                          <div className="mt-4">
                            <TabSkeleton tab="harness" />
                          </div>
                        </div>
                      }
                    >
                      <ProjectDetail
                        detail={splitProject}
                        baseUrl={baseUrl || window.location.origin}
                        secret={secret}
                        api={api}
                        onBack={() => setSplitSlug(null)}
                        onArchive={() => handleArchiveProject(splitProject.project.slug)}
                        onDelete={() => handleDeleteProject(splitProject.project.slug)}
                        onReload={async () => {
                          const refreshed = await api.getProject(splitProject.project.slug)
                          setOpenProjects(current =>
                            current.map(p =>
                              p.project.slug === splitProject.project.slug ? refreshed : p,
                            ),
                          )
                        }}
                      />
                    </Suspense>
                  </div>
                )}
              </div>
            ) : (
              <ProjectList
                projects={projects}
                busy={busy}
                onOpenProject={open}
                onRefresh={() => void load()}
                allSessionsState={allSessions}
                onOpenRegisterModal={() => setShowRegisterModal(true)}
              />
            )}
          </div>
        )}

        {currentPage === 'settings' && (
          <div className="page-view settings-page-view">
            <Suspense
              fallback={
                <div className="page-view settings-page-view">
                  <TabSkeleton tab="settings" />
                </div>
              }
            >
              <ServerSettings
                baseUrl={baseUrl}
                secret={secret}
                onSaveConnection={handleSaveConnection}
                isConnected={isConnected}
                api={api}
                totalProjects={projects.length}
                activeSessionsCount={activeLiveSessionsCount}
              />
            </Suspense>
          </div>
        )}
      </main>
      {showRegisterModal && (
        <RegisterProjectModal
          api={api}
          projects={projects}
          onClose={() => setShowRegisterModal(false)}
          onCreateProject={handleCreateProject}
        />
      )}
    </div>
  )
}
