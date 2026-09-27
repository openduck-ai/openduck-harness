import { useState, useEffect, lazy, Suspense } from 'react'
import type { ProjectDetail as ProjectDetailType, createApi } from '@aaif/goose-hub-core'
import {
  ArrowLeft,
  FlaskConical,
  MessageSquare,
  GitBranch,
  FolderTree,
  Terminal,
  BarChart3,
  Settings,
  Folder,
  Code2,
  ScrollText,
} from 'lucide-react'
import { TabSkeleton, ProjectDetailHeaderSkeleton } from './Skeletons'

export type ProjectSubTab =
  | 'harness'
  | 'chat'
  | 'git'
  | 'files'
  | 'rules'
  | 'terminal'
  | 'insights'
  | 'settings'

// Lazy-load subtab components for code splitting & faster initial project load
const ProjectHarnessTab = lazy(() =>
  import('./ProjectHarnessTab').then(m => ({ default: m.ProjectHarnessTab })),
)
const ProjectChat = lazy(() =>
  import('./ProjectChat').then(m => ({ default: m.ProjectChat })),
)
const ProjectGit = lazy(() =>
  import('./ProjectGit').then(m => ({ default: m.ProjectGit })),
)
const ProjectFiles = lazy(() =>
  import('./ProjectFiles').then(m => ({ default: m.ProjectFiles })),
)
const ProjectRules = lazy(() =>
  import('./ProjectRules').then(m => ({ default: m.ProjectRules })),
)
const ProjectTerminal = lazy(() =>
  import('./ProjectTerminal').then(m => ({ default: m.ProjectTerminal })),
)
const ProjectInsights = lazy(() =>
  import('./ProjectInsights').then(m => ({ default: m.ProjectInsights })),
)
const ProjectSettings = lazy(() =>
  import('./ProjectSettings').then(m => ({ default: m.ProjectSettings })),
)

export function preloadTab(tab: ProjectSubTab) {
  switch (tab) {
    case 'harness':
      void import('./ProjectHarnessTab')
      break
    case 'chat':
      void import('./ProjectChat')
      break
    case 'git':
      void import('./ProjectGit')
      break
    case 'files':
      void import('./ProjectFiles')
      break
    case 'rules':
      void import('./ProjectRules')
      break
    case 'terminal':
      void import('./ProjectTerminal')
      break
    case 'insights':
      void import('./ProjectInsights')
      break
    case 'settings':
      void import('./ProjectSettings')
      break
  }
}

interface ProjectDetailProps {
  detail: ProjectDetailType
  baseUrl: string
  secret: string
  api: ReturnType<typeof createApi>
  onBack: () => void
  onArchive: () => Promise<void>
  onDelete: () => Promise<void>
  onReload: () => void
  selectedSessionId?: string | null
  isLoadingDetail?: boolean
}

export function ProjectDetail({
  detail,
  baseUrl,
  secret,
  api,
  onBack,
  onArchive,
  onDelete,
  onReload,
  selectedSessionId,
  isLoadingDetail = false,
}: ProjectDetailProps) {
  const [activeTab, setActiveTab] = useState<ProjectSubTab>(
    selectedSessionId ? 'chat' : 'harness',
  )

  useEffect(() => {
    if (selectedSessionId) {
      setActiveTab('chat')
    }
  }, [selectedSessionId])

  return (
    <section className="project-detail-section">
      <div className="detail-top-nav">
        <button className="back-btn" onClick={onBack}>
          <ArrowLeft size={16} />
          <span>All Projects</span>
        </button>
      </div>

      {isLoadingDetail && !detail?.project ? (
        <ProjectDetailHeaderSkeleton />
      ) : (
        <div className="detail-head">
          <div className="detail-head-info">
            <div className="eyebrow-row">
              <span className="eyebrow">PROJECT WORKSPACE</span>
              <span className="project-slug-badge">{detail.project.slug}</span>
            </div>
            <h2 className="project-detail-title">
              {detail.project.title || detail.project.slug}
            </h2>
            <p className="muted project-detail-desc">
              {detail.project.description || 'No description provided'}
            </p>
            <div className="project-meta-chips">
              <span className="meta-chip path-chip">
                <Folder size={12} />
                <code>{detail.project.path}</code>
              </span>
              {detail.project.language && (
                <span className="meta-chip lang-chip">
                  <Code2 size={12} />
                  <span>{detail.project.language}</span>
                </span>
              )}
            </div>
          </div>
          <div className="detail-badges">
            <span className={`status-pill ${detail.project.status}`}>
              {detail.project.status}
            </span>
          </div>
        </div>
      )}

      <div className="project-subtabs">
        <button
          type="button"
          className={`subtab ${activeTab === 'harness' ? 'active' : ''}`}
          onClick={() => setActiveTab('harness')}
          onMouseEnter={() => preloadTab('harness')}
          onFocus={() => preloadTab('harness')}
        >
          <FlaskConical size={15} />
          <span>Harness</span>
        </button>
        <button
          type="button"
          className={`subtab ${activeTab === 'chat' ? 'active' : ''}`}
          onClick={() => setActiveTab('chat')}
          onMouseEnter={() => preloadTab('chat')}
          onFocus={() => preloadTab('chat')}
        >
          <MessageSquare size={15} />
          <span>Chat</span>
        </button>
        <button
          type="button"
          className={`subtab ${activeTab === 'git' ? 'active' : ''}`}
          onClick={() => setActiveTab('git')}
          onMouseEnter={() => preloadTab('git')}
          onFocus={() => preloadTab('git')}
        >
          <GitBranch size={15} />
          <span>Git</span>
        </button>
        <button
          type="button"
          className={`subtab ${activeTab === 'files' ? 'active' : ''}`}
          onClick={() => setActiveTab('files')}
          onMouseEnter={() => preloadTab('files')}
          onFocus={() => preloadTab('files')}
        >
          <FolderTree size={15} />
          <span>Files</span>
        </button>
        <button
          type="button"
          className={`subtab ${activeTab === 'rules' ? 'active' : ''}`}
          onClick={() => setActiveTab('rules')}
          onMouseEnter={() => preloadTab('rules')}
          onFocus={() => preloadTab('rules')}
        >
          <ScrollText size={15} />
          <span>Rules</span>
        </button>
        <button
          type="button"
          className={`subtab ${activeTab === 'terminal' ? 'active' : ''}`}
          onClick={() => setActiveTab('terminal')}
          onMouseEnter={() => preloadTab('terminal')}
          onFocus={() => preloadTab('terminal')}
        >
          <Terminal size={15} />
          <span>Terminal</span>
        </button>
        <button
          type="button"
          className={`subtab ${activeTab === 'insights' ? 'active' : ''}`}
          onClick={() => setActiveTab('insights')}
          onMouseEnter={() => preloadTab('insights')}
          onFocus={() => preloadTab('insights')}
        >
          <BarChart3 size={15} />
          <span>Insights</span>
        </button>
        <button
          type="button"
          className={`subtab ${activeTab === 'settings' ? 'active' : ''}`}
          onClick={() => setActiveTab('settings')}
          onMouseEnter={() => preloadTab('settings')}
          onFocus={() => preloadTab('settings')}
        >
          <Settings size={15} />
          <span>Settings</span>
        </button>
      </div>

      <div className="subtab-view-container">
        <Suspense fallback={<TabSkeleton tab={activeTab} />}>
          {activeTab === 'harness' && (
            <div className="tab-content">
              <ProjectHarnessTab
                api={api}
                selectedProjectSlug={detail.project.slug}
                onNavigateToFiles={() => setActiveTab('files')}
              />
            </div>
          )}

          {activeTab === 'chat' && (
            <div className="tab-content">
              <ProjectChat
                detail={detail}
                baseUrl={baseUrl}
                secret={secret}
                api={api}
                selectedSessionId={selectedSessionId}
              />
            </div>
          )}

          {activeTab === 'git' && (
            <div className="tab-content">
              <ProjectGit api={api} slug={detail.project.slug} />
            </div>
          )}

          {activeTab === 'files' && (
            <div className="tab-content">
              <ProjectFiles
                api={api}
                slug={detail.project.slug}
                rootPath={detail.project.path}
              />
            </div>
          )}

          {activeTab === 'rules' && (
            <div className="tab-content">
              <ProjectRules
                baseUrl={baseUrl}
                secret={secret}
                projectDir={detail.project.path}
                projectSlug={detail.project.slug}
              />
            </div>
          )}

          {activeTab === 'terminal' && (
            <div className="tab-content">
              <ProjectTerminal
                api={api}
                slug={detail.project.slug}
                rootPath={detail.project.path}
                language={detail.project.language}
              />
            </div>
          )}

          {activeTab === 'insights' && (
            <div className="tab-content">
              <ProjectInsights detail={detail} api={api} />
            </div>
          )}

          {activeTab === 'settings' && (
            <div className="tab-content">
              <ProjectSettings
                detail={detail}
                api={api}
                onArchive={onArchive}
                onDelete={onDelete}
                onReload={onReload}
              />
            </div>
          )}
        </Suspense>
      </div>
    </section>
  )
}
