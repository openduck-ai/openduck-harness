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
import { parseUrlNav, syncUrlNav } from './urlNav'
import type { ProjectSubTab } from './lazyTabs'
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
  const [initialNav] = useState(() =>
    typeof window !== 'undefined'
      ? parseUrlNav(window.location.search)
      : { page: 'projects' as NavPage, project: null, tab: null, split: null },
  )
  const [currentPage, setCurrentPage] = useState<NavPage>(initialNav.page)
  const [baseUrl, setBaseUrl] = useState(() => localStorage.getItem('goose-hub-url') ?? '')
  const [secret, setSecret] = useState(() => localStorage.getItem('goose-hub-secret') ?? '')
  const api = useMemo(
    () => createApi(baseUrl || window.location.origin, secret),
    [baseUrl, secret],
  )
  const [projects, setProjects] = useState<Project[]>([])
  const [openProjects, setOpenProjects] = useState<ProjectDetailType[]>([])
  const [activeSlug, setActiveSlug] = useState<string | null>(initialNav.project)
  const [activeTab, setActiveTab] = useState<ProjectSubTab | null>(initialNav.tab)
  const [splitSlug, setSplitSlug] = useState<string | null>(initialNav.split)
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

  const open = useCallback(
    async (project: Project, tab?: ProjectSubTab | null) => {
      setError('')
      const targetTab = tab ?? activeTab

      const alreadyOpen = openProjects.find(p => p.project.slug === project.slug)
      if (alreadyOpen) {
        setActiveSlug(project.slug)
        setCurrentPage('projects')
        if (tab) setActiveTab(tab)
        syncUrlNav({
          page: 'projects',
          project: project.slug,
          tab: targetTab,
          split: splitSlug,
        })
        return
      }

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
      if (tab) setActiveTab(tab)
      syncUrlNav({
        page: 'projects',
        project: project.slug,
        tab: targetTab,
        split: splitSlug,
      })

      try {
        const detail = await api.getProject(project.slug, { lazy: true })
        setOpenProjects(current =>
          current.map(p => (p.project.slug === project.slug ? detail : p)),
        )
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : 'Unable to load project details')
      }
    },
    [activeTab, api, openProjects, splitSlug],
  )

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

          if (initialNav.project) {
            const matched = res.projects.find(p => p.slug === initialNav.project)
            if (matched) {
              void open(matched, initialNav.tab)
            }
          }

          if (initialNav.split && initialNav.split !== initialNav.project) {
            const splitMatch = res.projects.find(p => p.slug === initialNav.split)
            if (splitMatch) {
              try {
                const detail = await api.getProject(splitMatch.slug, { lazy: true })
                if (!ignore) {
                  setOpenProjects(current =>
                    current.some(p => p.project.slug === splitMatch.slug)
                      ? current
                      : [...current, detail],
                  )
                }
              } catch {
                // Ignore split load failure
              }
            }
          }
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

  useEffect(() => {
    const handlePopState = () => {
      const nav = parseUrlNav(window.location.search)
      setCurrentPage(nav.page)
      setActiveSlug(nav.project)
      setActiveTab(nav.tab)
      setSplitSlug(nav.split)

      if (nav.project && projects.length > 0) {
        const found = projects.find(p => p.slug === nav.project)
        if (found && !openProjects.some(p => p.project.slug === found.slug)) {
          void open(found, nav.tab)
        }
      }
    }

    window.addEventListener('popstate', handlePopState)
    return () => window.removeEventListener('popstate', handlePopState)
  }, [projects, openProjects, open])

  const handleSelectPage = (page: NavPage) => {
    setCurrentPage(page)
    syncUrlNav({
      page,
      project: page === 'settings' ? null : activeSlug,
      tab: page === 'settings' ? null : activeTab,
      split: page === 'settings' ? null : splitSlug,
    })
  }

  const closeTab = (slug: string, event?: React.MouseEvent) => {
    event?.stopPropagation()
    setOpenProjects(current => current.filter(p => p.project.slug !== slug))
    let nextActive = activeSlug
    if (activeSlug === slug) {
      const remaining = openProjects.filter(p => p.project.slug !== slug)
      nextActive = remaining[remaining.length - 1]?.project.slug ?? null
      setActiveSlug(nextActive)
    }
    let nextSplit = splitSlug
    if (splitSlug === slug) {
      nextSplit = null
      setSplitSlug(null)
    }
    syncUrlNav({
      page: 'projects',
      project: nextActive,
      tab: nextActive ? activeTab : null,
      split: nextSplit,
    })
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
        onSelectPage={handleSelectPage}
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
                  onClick={() => {
                    setActiveSlug(null)
                    syncUrlNav({ page: 'projects', project: null })
                  }}
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
                      onClick={() => {
                        setActiveSlug(slug)
                        syncUrlNav({
                          page: 'projects',
                          project: slug,
                          tab: activeTab,
                          split: splitSlug,
                        })
                      }}
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
                        onChange={e => {
                          const nextSplit = e.target.value || null
                          setSplitSlug(nextSplit)
                          syncUrlNav({
                            page: 'projects',
                            project: activeSlug,
                            tab: activeTab,
                            split: nextSplit,
                          })
                        }}
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
                      initialTab={activeTab ?? undefined}
                      onTabChange={tab => {
                        setActiveTab(tab)
                        syncUrlNav({
                          page: 'projects',
                          project: activeProject.project.slug,
                          tab,
                          split: splitSlug,
                        })
                      }}
                      onBack={() => {
                        setActiveSlug(null)
                        syncUrlNav({ page: 'projects', project: null })
                      }}
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
                        onBack={() => {
                          setSplitSlug(null)
                          syncUrlNav({
                            page: 'projects',
                            project: activeSlug,
                            tab: activeTab,
                            split: null,
                          })
                        }}
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
