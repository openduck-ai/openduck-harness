import { useState } from 'react'
import type { Project } from '@aaif/goose-hub-core'
import {
  Search,
  X,
  LayoutGrid,
  List,
  RefreshCw,
  Zap,
  ShieldAlert,
  Clock,
  ArrowRight,
  Plus,
  Copy,
  Check,
  Folder,
  Code2,
  FileText,
  Workflow,
  Sparkles,
  Mail,
} from 'lucide-react'
import { ProjectListSkeleton } from './Skeletons'

interface ProjectListProps {
  projects: Project[]
  busy: boolean
  onOpenProject: (project: Project) => void
  onRefresh: () => void
  allSessionsState: Record<string, { isPrompting?: boolean; pendingPermission?: unknown }>
  onOpenRegisterModal: () => void
}

export function ProjectList({
  projects,
  busy,
  onOpenProject,
  onRefresh,
  allSessionsState,
  onOpenRegisterModal,
}: ProjectListProps) {
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<'all' | 'active' | 'archived'>('all')
  const [kindFilter, setKindFilter] = useState<string>('all')
  const [viewMode, setViewMode] = useState<'grid' | 'table'>('grid')
  const [copiedSlug, setCopiedSlug] = useState<string | null>(null)

  const copyPath = (path: string, slug: string, e: React.MouseEvent) => {
    e.stopPropagation()
    void navigator.clipboard.writeText(path)
    setCopiedSlug(slug)
    setTimeout(() => setCopiedSlug(null), 2000)
  }

  const filteredProjects = projects.filter(p => {
    if (statusFilter !== 'all' && p.status !== statusFilter) return false
    if (kindFilter !== 'all' && p.kind !== kindFilter) return false
    if (!search.trim()) return true
    const q = search.toLowerCase()
    return (
      p.title.toLowerCase().includes(q) ||
      p.slug.toLowerCase().includes(q) ||
      p.path.toLowerCase().includes(q) ||
      (p.description && p.description.toLowerCase().includes(q)) ||
      (p.language && p.language.toLowerCase().includes(q))
    )
  })

  const activeProjectsCount = projects.filter(p => p.status === 'active').length

  const getKindIcon = (kind: string) => {
    switch (kind) {
      case 'software':
        return <Code2 size={13} className="kind-icon" />
      case 'docs':
        return <FileText size={13} className="kind-icon" />
      case 'automation':
        return <Workflow size={13} className="kind-icon" />
      default:
        return <Folder size={13} className="kind-icon" />
    }
  }

  return (
    <div className="project-list-container">
      {/* Toolbar Controls */}
      <div className="section-toolbar">
        <div className="toolbar-left">
          <div className="search-box">
            <Search size={16} className="search-icon" />
            <input
              type="text"
              className="search-input"
              placeholder="Search projects by name, path, language…"
              value={search}
              onChange={e => setSearch(e.target.value)}
            />
            {search && (
              <button
                type="button"
                className="clear-search-btn"
                onClick={() => setSearch('')}
                title="Clear search"
              >
                <X size={14} />
              </button>
            )}
          </div>

          <div className="filter-group">
            <select
              value={statusFilter}
              onChange={e => setStatusFilter(e.target.value as 'all' | 'active' | 'archived')}
              className="filter-select"
            >
              <option value="all">All Status ({projects.length})</option>
              <option value="active">Active Only ({activeProjectsCount})</option>
              <option value="archived">Archived Only ({projects.length - activeProjectsCount})</option>
            </select>

            <select
              value={kindFilter}
              onChange={e => setKindFilter(e.target.value)}
              className="filter-select"
            >
              <option value="all">All Kinds</option>
              <option value="software">Software</option>
              <option value="docs">Documentation</option>
              <option value="automation">Automation</option>
              <option value="other">Other</option>
            </select>
          </div>
        </div>

        <div className="toolbar-right">
          <div className="view-mode-toggle">
            <button
              type="button"
              className={`toggle-btn ${viewMode === 'grid' ? 'active' : ''}`}
              onClick={() => setViewMode('grid')}
              title="Grid View"
            >
              <LayoutGrid size={15} />
              <span>Grid</span>
            </button>
            <button
              type="button"
              className={`toggle-btn ${viewMode === 'table' ? 'active' : ''}`}
              onClick={() => setViewMode('table')}
              title="Table View"
            >
              <List size={15} />
              <span>Table</span>
            </button>
          </div>

          <button
            type="button"
            className="secondary refresh-btn"
            onClick={onRefresh}
            title="Refresh project list"
            disabled={busy}
          >
            <RefreshCw size={15} className={busy ? 'spinning' : ''} />
            <span>Refresh</span>
          </button>
        </div>
      </div>

      {busy && projects.length === 0 && (
        <ProjectListSkeleton viewMode={viewMode} />
      )}

      {filteredProjects.length === 0 && !busy && (
        <div className="empty-state-box">
          <div className="empty-state-icon-box">
            <Sparkles size={32} />
          </div>
          <h3>{search || statusFilter !== 'all' || kindFilter !== 'all' ? 'No matching projects found' : 'No projects registered yet'}</h3>
          <p className="muted">
            {search || statusFilter !== 'all' || kindFilter !== 'all'
              ? 'Try adjusting your search terms or filters to find what you are looking for.'
              : 'Register a local project directory to connect your OpenDuck agent workspace.'}
          </p>
          {search || statusFilter !== 'all' || kindFilter !== 'all' ? (
            <button
              type="button"
              className="secondary mini-btn mt-3"
              onClick={() => {
                setSearch('')
                setStatusFilter('all')
                setKindFilter('all')
              }}
            >
              Reset filters
            </button>
          ) : (
            <button
              type="button"
              className="btn-primary mt-3"
              onClick={onOpenRegisterModal}
            >
              <Plus size={16} />
              <span>Register First Project</span>
            </button>
          )}
        </div>
      )}

      {viewMode === 'grid' ? (
        <div className="project-cards-grid">
          {filteredProjects.map(project => {
            const sState = allSessionsState[project.slug]
            const isWorking = sState?.isPrompting
            const hasPermission = !!sState?.pendingPermission
            const isCopied = copiedSlug === project.slug

            return (
              <div
                className={`card modern-project-card ${isWorking ? 'card-working' : ''}`}
                key={project.slug}
                onClick={() => onOpenProject(project)}
                onMouseEnter={() => void import('./ProjectDetail')}
                role="button"
                tabIndex={0}
              >
                <div className="card-top-row">
                  <div className="project-title-area">
                    <strong className="project-card-title">{project.title || project.slug}</strong>
                    <span className="project-slug-badge">{project.slug}</span>
                  </div>

                  <div className="card-badges">
                    {isWorking && (
                      <span className="badge working-pulse-badge" title="Agent is working in background">
                        <Zap size={12} className="pulse-icon" /> Working
                      </span>
                    )}
                    {hasPermission && (
                      <span className="badge permission-action-badge" title="Permission required">
                        <ShieldAlert size={12} /> Action
                      </span>
                    )}
                    <span className={`status-pill ${project.status}`}>{project.status}</span>
                  </div>
                </div>

                <p className="project-card-desc">{project.description || 'No description provided.'}</p>

                <div
                  className="project-card-path-box"
                  onClick={e => copyPath(project.path, project.slug, e)}
                  title="Click to copy path"
                >
                  <code className="path-text">{project.path}</code>
                  <span className="copy-icon-btn">
                    {isCopied ? <Check size={13} className="text-success" /> : <Copy size={13} />}
                  </span>
                </div>

                <div className="card-footer-row">
                  <div className="tags-row">
                    <span className="tag-kind">
                      {getKindIcon(project.kind)}
                      {project.kind}
                    </span>
                    {project.language && <span className="tag-lang">{project.language}</span>}
                    {project.emailRecipients && project.emailRecipients.length > 0 && (
                      <span className="tag-lang" title={project.emailRecipients.join(', ')}>
                        <Mail size={12} />
                        {project.emailRecipients.length} alert{project.emailRecipients.length > 1 ? 's' : ''}
                      </span>
                    )}
                    {project.lastActivityAt && (
                      <span className="tag-time" title="Last activity">
                        <Clock size={12} />
                        {new Date(project.lastActivityAt).toLocaleDateString()}
                      </span>
                    )}
                  </div>

                  <div className="open-action-link">
                    <span>Open</span>
                    <ArrowRight size={14} className="arrow-icon" />
                  </div>
                </div>
              </div>
            )
          })}
        </div>
      ) : (
        <div className="panel table-panel">
          <table className="management-table">
            <thead>
              <tr>
                <th>Project Name</th>
                <th>Type & Stack</th>
                <th>Directory Path</th>
                <th>Status</th>
                <th>Last Active</th>
                <th className="text-right">Action</th>
              </tr>
            </thead>
            <tbody>
              {filteredProjects.map(project => {
                const sState = allSessionsState[project.slug]
                const isWorking = sState?.isPrompting
                const hasPermission = !!sState?.pendingPermission

                return (
                  <tr
                    key={project.slug}
                    className="clickable-row"
                    onClick={() => onOpenProject(project)}
                    onMouseEnter={() => void import('./ProjectDetail')}
                  >
                    <td>
                      <div className="project-table-title">
                        <strong>{project.title || project.slug}</strong>
                        <small className="muted">{project.slug}</small>
                      </div>
                    </td>
                    <td>
                      <div className="table-tags-cell">
                        <span className="tag-kind">
                          {getKindIcon(project.kind)}
                          {project.kind}
                        </span>
                        {project.language && <span className="tag-lang">{project.language}</span>}
                      </div>
                    </td>
                    <td className="path-cell">
                      <code>{project.path}</code>
                    </td>
                    <td>
                      <div className="table-badges">
                        <span className={`status-pill ${project.status}`}>{project.status}</span>
                        {isWorking && <span className="badge working-pulse-badge">⚡ Working</span>}
                        {hasPermission && <span className="badge permission-action-badge">⚠️ Action</span>}
                      </div>
                    </td>
                    <td className="muted text-sm">
                      {project.lastActivityAt
                        ? new Date(project.lastActivityAt).toLocaleString()
                        : '—'}
                    </td>
                    <td className="text-right">
                      <button
                        type="button"
                        className="secondary mini-btn"
                        onClick={e => {
                          e.stopPropagation()
                          onOpenProject(project)
                        }}
                      >
                        <span>Open</span>
                        <ArrowRight size={13} />
                      </button>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

