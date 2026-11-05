import { useTranslation } from 'react-i18next'
import { useState } from 'react'
import { formatDate, formatDateTime } from '../i18n/format.ts'
import type { Project } from '@aaif/goose-hub-core'
import {
  Search,
  X,
  LayoutGrid,
  List,
  RefreshCw,
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
  Square,
} from 'lucide-react'
import { ProjectListSkeleton } from './Skeletons'

interface ProjectListProps {
  projects: Project[]
  busy: boolean
  onOpenProject: (project: Project) => void
  onRefresh: () => void
  allSessionsState: Record<string, { isPrompting?: boolean; pendingPermission?: unknown }>
  onOpenRegisterModal: () => void
  onStopChat?: (slug: string) => void
}

export function ProjectList({
  projects,
  busy,
  onOpenProject,
  onRefresh,
  allSessionsState,
  onOpenRegisterModal,
  onStopChat,
}: ProjectListProps) {
  const { t } = useTranslation()
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
              placeholder={t('projectList.page.searchPlaceholder')}
              value={search}
              onChange={e => setSearch(e.target.value)}
            />
            {search && (
              <button
                type="button"
                className="clear-search-btn"
                onClick={() => setSearch('')}
                title={t('projectList.page.clearSearch')}
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
              <option value="all">{t('projectList.page.allStatus', { count: projects.length })}</option>
              <option value="active">{t('projectList.page.activeOnly', { count: activeProjectsCount })}</option>
              <option value="archived">{t('projectList.page.archivedOnly', { count: projects.length - activeProjectsCount })}</option>
            </select>

            <select
              value={kindFilter}
              onChange={e => setKindFilter(e.target.value)}
              className="filter-select"
            >
              <option value="all">{t('projectList.page.allKinds')}</option>
              <option value="software">{t('projectList.page.software')}</option>
              <option value="docs">{t('projectList.page.documentation')}</option>
              <option value="automation">{t('projectList.page.automation')}</option>
              <option value="other">{t('projectList.page.other')}</option>
            </select>
          </div>
        </div>

        <div className="toolbar-right">
          <div className="view-mode-toggle">
            <button
              type="button"
              className={`toggle-btn ${viewMode === 'grid' ? 'active' : ''}`}
              onClick={() => setViewMode('grid')}
              title={t('projectList.page.gridView')}
            >
              <LayoutGrid size={15} />
              <span>{t('projectList.page.grid')}</span>
            </button>
            <button
              type="button"
              className={`toggle-btn ${viewMode === 'table' ? 'active' : ''}`}
              onClick={() => setViewMode('table')}
              title={t('projectList.page.tableView')}
            >
              <List size={15} />
              <span>{t('projectList.page.table')}</span>
            </button>
          </div>

          <button
            type="button"
            className="secondary refresh-btn"
            onClick={onRefresh}
            title={t('projectList.page.refreshTitle')}
            disabled={busy}
          >
            <RefreshCw size={15} className={busy ? 'spinning' : ''} />
            <span>{t('projectList.page.refresh')}</span>
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
          <h3>{search || statusFilter !== 'all' || kindFilter !== 'all' ? t('projectList.page.noMatches') : t('projectList.page.noProjects')}</h3>
          <p className="muted">
            {search || statusFilter !== 'all' || kindFilter !== 'all'
              ? t('projectList.page.adjustFilters')
              : t('projectList.page.registerHint')}
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
              {t('projectList.page.resetFilters')}
            </button>
          ) : (
            <button
              type="button"
              className="btn-primary mt-3"
              onClick={onOpenRegisterModal}
            >
              <Plus size={16} />
              <span>{t('projectList.page.registerFirst')}</span>
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
                      <button
                        type="button"
                        className="badge working-pulse-badge stop-project-chat-badge"
                        title={t('projectList.page.stopTitle')}
                        onClick={e => {
                          e.stopPropagation()
                          void onStopChat?.(project.slug)
                        }}
                      >
                        <Square size={10} className="stop-icon" />
                        <span>{t('projectList.page.stop')}</span>
                      </button>
                    )}
                    {hasPermission && (
                      <span className="badge permission-action-badge" title={t('projectList.page.permission')}>
                        <ShieldAlert size={12} />{' '}{t('projectList.page.action')}
                      </span>
                    )}
                    <span className={`status-pill ${project.status}`}>{project.status}</span>
                  </div>
                </div>

                <p className="project-card-desc">{project.description || t('projectList.page.noDescription')}</p>

                <div
                  className="project-card-path-box"
                  onClick={e => copyPath(project.path, project.slug, e)}
                  title={t('projectList.page.copyPath')}
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
                        {t(project.emailRecipients.length === 1 ? 'projectList.page.alerts_one' : 'projectList.page.alerts_other', { count: project.emailRecipients.length })}
                      </span>
                    )}
                    {project.lastActivityAt && (
                      <span className="tag-time" title={t('projectList.page.lastActivity')}>
                        <Clock size={12} />
                        {formatDate(project.lastActivityAt)}
                      </span>
                    )}
                  </div>

                  <div className="open-action-link">
                    <span>{t('projectList.page.open')}</span>
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
                <th>{t('projectList.page.projectName')}</th>
                <th>{t('projectList.page.typeStack')}</th>
                <th>{t('projectList.page.directoryPath')}</th>
                <th>{t('projectList.page.status')}</th>
                <th>{t('projectList.page.lastActive')}</th>
                <th className="text-right">{t('projectList.page.action')}</th>
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
                        {isWorking && (
                          <button
                            type="button"
                            className="badge working-pulse-badge stop-project-chat-badge"
                            title={t('projectList.page.stopTitle')}
                            onClick={e => {
                              e.stopPropagation()
                              void onStopChat?.(project.slug)
                            }}
                          >
                            <Square size={10} className="stop-icon" />{' '}{t('projectList.page.stop')}
                          </button>
                        )}
                        {hasPermission && <span className="badge permission-action-badge">{t('projectList.page.warningAction')}</span>}
                      </div>
                    </td>
                    <td className="muted text-sm">
                      {project.lastActivityAt
                        ? formatDateTime(project.lastActivityAt)
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
                        <span>{t('projectList.page.open')}</span>
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

