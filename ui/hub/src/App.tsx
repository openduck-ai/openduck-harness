import { useTranslation } from 'react-i18next'
import { translate } from './i18n/index.ts'
import { useCallback, useEffect, useMemo, useRef, useState, lazy, Suspense } from 'react'
import {
  stopProjectChat,
  useAllProjectSessions,
  type HarnessActiveJob,
  type Project,
  type ProjectDetail as ProjectDetailType,
  type ProjectInput,
} from '@aaif/goose-hub-core'
import { createApi } from './api'
import { Header, type NavPage } from './components/Header'
import { ProjectList } from './components/ProjectList'
import { RunningTasksPage } from './components/RunningTasksPage'
import { RegisterProjectModal } from './components/RegisterProjectModal'
import { TabSkeleton, ProjectDetailHeaderSkeleton } from './components/Skeletons'
import {
  absoluteHubUrl,
  formatUrlNavQuery,
  parseUrlNav,
  syncUrlNav,
  type HubDestination,
  type UrlNavState,
} from './urlNav'
import type { ProjectSubTab } from './lazyTabs'
import {
  LayoutDashboard,
  ShieldAlert,
  X,
  Columns2,
  Folder,
  Square,
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

// Lazy-load TuiAgentsPage
const TuiAgentsPage = lazy(() =>
  import('./components/TuiAgentsPage').then(m => ({ default: m.TuiAgentsPage })),
)

export default function App() {
  const { t } = useTranslation()
  const [initialNav] = useState<UrlNavState>(() =>
    typeof window !== 'undefined'
      ? parseUrlNav(window.location.search)
      : { page: 'projects', project: null, tab: null, split: null, destination: null },
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
  const [destination, setDestination] = useState<HubDestination | null>(initialNav.destination)
  const navRef = useRef<UrlNavState>(initialNav)
  navRef.current = {
    page: currentPage,
    project: activeSlug,
    tab: activeTab,
    split: splitSlug,
    destination,
  }
  const [showRegisterModal, setShowRegisterModal] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [isConnected, setIsConnected] = useState(false)
  const [runningJobs, setRunningJobs] = useState<HarnessActiveJob[]>([])
  const [runningJobsLoaded, setRunningJobsLoaded] = useState(false)
  const [runningJobsError, setRunningJobsError] = useState('')
  const [runningJobsRefreshing, setRunningJobsRefreshing] = useState(false)
  const [stoppingJobId, setStoppingJobId] = useState<string | null>(null)
  const [now, setNow] = useState(() => Date.now())
  const runningRequestRef = useRef(0)

  const allSessions = useAllProjectSessions()

  const publishNav = useCallback((patch: Partial<UrlNavState>, replace = false) => {
    const current = navRef.current
    const nextProject = patch.project !== undefined ? patch.project : current.project
    const nextTab = patch.tab !== undefined ? patch.tab : current.tab
    const nextPage = patch.page !== undefined ? patch.page : current.page
    const nextSplit = patch.split !== undefined ? patch.split : current.split
    let nextDestination = patch.destination !== undefined ? patch.destination : current.destination
    if (patch.project !== undefined && patch.project !== current.project && patch.destination === undefined) {
      nextDestination = null
    }
    if (patch.tab !== undefined && patch.tab !== current.tab && patch.destination === undefined) {
      nextDestination = null
    }
    if (!nextProject) nextDestination = null

    const next: UrlNavState = {
      page: nextProject ? 'projects' : nextPage,
      project: nextProject,
      tab: nextProject ? nextTab : null,
      split: nextProject ? nextSplit : null,
      destination: nextDestination,
    }
    navRef.current = next
    setCurrentPage(next.page)
    setActiveSlug(next.project)
    setActiveTab(next.tab)
    setSplitSlug(next.split)
    setDestination(next.destination)
    syncUrlNav(next, replace)
  }, [])

  const load = useCallback(async () => {
    setBusy(true)
    setError('')
    try {
      const res = await api.listProjects()
      setProjects(res.projects)
      setIsConnected(true)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'app.shell.connectError')
      setIsConnected(false)
    } finally {
      setBusy(false)
    }
  }, [api])

  const open = useCallback(
    async (project: Project, tab?: ProjectSubTab | null, options?: { destination?: HubDestination | null }) => {
      setError('')
      const targetTab = tab ?? navRef.current.tab

      const alreadyOpen = openProjects.find(p => p.project.slug === project.slug)
      if (alreadyOpen) {
        publishNav({
          page: 'projects',
          project: project.slug,
          tab: targetTab,
          ...(options?.destination !== undefined ? { destination: options.destination } : {}),
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
      publishNav({
        page: 'projects',
        project: project.slug,
        tab: targetTab,
        ...(options?.destination !== undefined ? { destination: options.destination } : {}),
      })

      try {
        const detail = await api.getProject(project.slug, { lazy: true })
        setOpenProjects(current =>
          current.map(p => (p.project.slug === project.slug ? detail : p)),
        )
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : 'app.shell.detailsError')
      }
    },
    [api, openProjects, publishNav],
  )

  useEffect(() => {
    const canonical = formatUrlNavQuery(initialNav)
    if (canonical !== window.location.search) {
      syncUrlNav(initialNav, true)
    }
  }, [initialNav])

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
            const matched = res.projects.find((p: Project) => p.slug === initialNav.project)
            if (matched) {
              void open(matched, initialNav.tab, { destination: initialNav.destination })
            }
          }

          if (initialNav.split && initialNav.split !== initialNav.project) {
            const splitMatch = res.projects.find((p: Project) => p.slug === initialNav.split)
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
          setError(cause instanceof Error ? cause.message : 'app.shell.connectError')
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
      navRef.current = nav
      setCurrentPage(nav.page)
      setActiveSlug(nav.project)
      setActiveTab(nav.tab)
      setSplitSlug(nav.split)
      setDestination(nav.destination)
      if (formatUrlNavQuery(nav) !== window.location.search) {
        syncUrlNav(nav, true)
      }

      if (nav.project && projects.length > 0) {
        const found = projects.find(p => p.slug === nav.project)
        if (found && !openProjects.some(p => p.project.slug === found.slug)) {
          void open(found, nav.tab, { destination: nav.destination })
        }
      }
    }

    window.addEventListener('popstate', handlePopState)
    return () => window.removeEventListener('popstate', handlePopState)
  }, [projects, openProjects, open])

  const handleSelectPage = (page: NavPage) => {
    const stayOnProject = page === 'projects'
    publishNav({
      page,
      project: stayOnProject ? navRef.current.project : null,
      tab: stayOnProject ? navRef.current.tab : null,
      split: stayOnProject ? navRef.current.split : null,
      destination: stayOnProject ? navRef.current.destination : null,
    })
  }

  const refreshRunningJobs = useCallback(async () => {
    const requestId = ++runningRequestRef.current
    try {
      const jobs = await api.listActiveHarnessJobs()
      if (requestId !== runningRequestRef.current) return
      setRunningJobs(jobs)
      setRunningJobsError('')
    } catch (cause) {
      if (requestId !== runningRequestRef.current) return
      setRunningJobsError(
        cause instanceof Error ? cause.message : 'app.shell.runningError',
      )
    } finally {
      if (requestId === runningRequestRef.current) {
        setRunningJobsLoaded(true)
        setRunningJobsRefreshing(false)
      }
    }
  }, [api])

  useEffect(() => {
    if (!isConnected) return
    void refreshRunningJobs()
    const interval = window.setInterval(() => {
      if (document.hidden) return
      void refreshRunningJobs()
    }, 3000)
    const onVisible = () => {
      if (!document.hidden) void refreshRunningJobs()
    }
    document.addEventListener('visibilitychange', onVisible)
    return () => {
      window.clearInterval(interval)
      document.removeEventListener('visibilitychange', onVisible)
    }
  }, [isConnected, refreshRunningJobs])

  useEffect(() => {
    if (runningJobs.length === 0) return
    const interval = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(interval)
  }, [runningJobs.length])

  const handleStopRunningJob = async (job: HarnessActiveJob) => {
    setStoppingJobId(job.jobId)
    setRunningJobsError('')
    try {
      await api.stopHarnessJob(job.jobId)
      await refreshRunningJobs()
    } catch (cause) {
      setRunningJobsError(cause instanceof Error ? cause.message : 'app.shell.stopError')
    } finally {
      setStoppingJobId(null)
    }
  }

  const closeTab = (slug: string, event?: React.MouseEvent) => {
    event?.stopPropagation()
    setOpenProjects(current => current.filter(p => p.project.slug !== slug))
    let nextActive = activeSlug
    if (activeSlug === slug) {
      const remaining = openProjects.filter(p => p.project.slug !== slug)
      nextActive = remaining[remaining.length - 1]?.project.slug ?? null
    }
    const nextSplit = splitSlug === slug ? null : splitSlug
    publishNav({
      page: 'projects',
      project: nextActive,
      tab: nextActive ? activeTab : null,
      split: nextSplit,
      ...(nextActive !== activeSlug ? { destination: null } : {}),
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
  const shareUrl = absoluteHubUrl({
    page: currentPage,
    project: activeSlug,
    tab: activeTab,
    split: splitSlug,
    destination,
  })
  const projectMissing =
    Boolean(activeSlug) &&
    isConnected &&
    !busy &&
    !projects.some(project => project.slug === activeSlug)
  const splitProject = splitSlug
    ? openProjects.find(p => p.project.slug === splitSlug) ?? null
    : null

  const activePromptingCount = Object.values(allSessions).filter(
    s => s?.isPrompting,
  ).length

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
        runningTasksCount={runningJobs.length + activePromptingCount}
      />

      <main className="app-main-content">
        {error && <p className="error global-error">{error === 'app.shell.connectError' ? translate('app.shell.connectError') : error === 'app.shell.detailsError' ? translate('app.shell.detailsError') : error}</p>}

        {currentPage === 'projects' && (
          <div className="page-view projects-page-view">
            {openProjects.length > 0 && (
              <nav className="tab-bar">
                <button
                  type="button"
                  className={`tab ${activeSlug === null ? 'active' : ''}`}
                  onClick={() => {
                    publishNav({ page: 'projects', project: null, destination: null })
                  }}
                >
                  <LayoutDashboard size={14} />
                  <span>{t('app.shell.allProjects')}</span>
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
                        publishNav({
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
                        <button
                          type="button"
                          className="tab-stop-btn mini-badge"
                          title={t('app.shell.workingTitle')}
                          onClick={e => {
                            e.stopPropagation()
                            void stopProjectChat(slug)
                          }}
                        >
                          <Square size={10} />
                          <span>{t('app.shell.stop')}</span>
                        </button>
                      )}
                      {hasPermission && (
                        <span className="badge permission-action-badge mini-badge" title={t('app.shell.permissionRequired')}>
                          <ShieldAlert size={11} />
                        </span>
                      )}
                      <button
                        type="button"
                        className="tab-close"
                        title={t('app.shell.closeTab')}
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
                          publishNav({
                            page: 'projects',
                            project: activeSlug,
                            tab: activeTab,
                            split: nextSplit,
                            destination,
                          })
                        }}
                        title={t('app.shell.splitTitle')}
                      >
                        <option value="">{t('app.shell.noSplit')}</option>
                        {openProjects
                          .filter(p => p.project.slug !== activeSlug)
                          .map(p => (
                            <option key={p.project.slug} value={p.project.slug}>
                              {t('app.shell.splitWith', { title: p.project.title || p.project.slug })}
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
                      destination={destination}
                      shareUrl={shareUrl}
                      onTabChange={tab => {
                        publishNav({
                          page: 'projects',
                          project: activeProject.project.slug,
                          tab,
                          split: splitSlug,
                        })
                      }}
                      onHubNavigate={(update, replace) => {
                        publishNav(
                          {
                            page: 'projects',
                            project: activeProject.project.slug,
                            split: splitSlug,
                            ...update,
                          },
                          replace,
                        )
                      }}
                      onBack={() => {
                        publishNav({ page: 'projects', project: null, destination: null })
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
                          publishNav({
                            page: 'projects',
                            project: activeSlug,
                            tab: activeTab,
                            split: null,
                            destination,
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
            ) : projectMissing ? (
              <div className="harness-card" style={{ margin: '1.5rem' }}>
                <h3 className="section-title">{t('deepLink.projectMissing', { project: activeSlug })}</h3>
                <p className="section-desc">{t('deepLink.projectMissingHint')}</p>
                <div style={{ display: 'flex', gap: '0.5rem' }}>
                  <button type="button" className="btn-secondary" onClick={() => void load()}>
                    {t('common.retry')}
                  </button>
                  <button
                    type="button"
                    className="btn-secondary"
                    onClick={() => publishNav({ page: 'projects', project: null, destination: null })}
                  >
                    {t('deepLink.backToProjects')}
                  </button>
                </div>
              </div>
            ) : (
              <ProjectList
                projects={projects}
                busy={busy}
                onOpenProject={open}
                onRefresh={() => void load()}
                allSessionsState={allSessions}
                onStopChat={slug => void stopProjectChat(slug)}
                onOpenRegisterModal={() => setShowRegisterModal(true)}
              />
            )}
          </div>
        )}

        {currentPage === 'running' && (
          <div className="page-view running-tasks-page-view">
            <RunningTasksPage
              jobs={runningJobs}
              projects={projects}
              loaded={runningJobsLoaded}
              isConnected={isConnected}
              error={runningJobsError === 'app.shell.runningError' ? translate('app.shell.runningError') : runningJobsError === 'app.shell.stopError' ? translate('app.shell.stopError') : runningJobsError}
              now={now}
              refreshing={runningJobsRefreshing}
              stoppingJobId={stoppingJobId}
              allSessionsState={allSessions}
              onStopChat={slug => void stopProjectChat(slug)}
              onRefresh={() => {
                setRunningJobsRefreshing(true)
                void refreshRunningJobs()
              }}
              onOpenProject={project => void open(project, 'harness')}
              onStopJob={job => void handleStopRunningJob(job)}
            />
          </div>
        )}

        {currentPage === 'tui-agents' && (
          <div className="page-view tui-agents-page-view">
            <Suspense
              fallback={
                <div className="page-view tui-agents-page-view">
                  <TabSkeleton tab="terminal" />
                </div>
              }
            >
              <TuiAgentsPage api={api} />
            </Suspense>
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
