import {
  Activity,
  FolderKanban,
  Settings,
  Plus,
  Server,
  Bot,
  Sun,
  Moon,
  Terminal,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useTheme } from '../contexts/ThemeContext'
import type { NavPage } from '../urlNav'
import { LocaleSwitcher } from './LocaleSwitcher.tsx'

export type { NavPage }

interface HeaderProps {
  currentPage: NavPage
  onSelectPage: (page: NavPage) => void
  isConnected: boolean
  onNewProject: () => void
  openProjectsCount: number
  runningTasksCount: number
}

export function Header({
  currentPage,
  onSelectPage,
  isConnected,
  onNewProject,
  openProjectsCount,
  runningTasksCount,
}: HeaderProps) {
  const { t } = useTranslation()
  const { resolvedTheme, themePreference, toggleTheme } = useTheme()

  return (
    <header className="app-header">
      <div className="brand-section">
        <div className="brand-logo-area" onClick={() => onSelectPage('projects')} role="button" tabIndex={0}>
          <div className="brand-badge-wrapper">
            <Bot className="brand-icon" size={24} />
          </div>
          <div>
            <div className="eyebrow-row">
              <span className="eyebrow">{t('app.shell.tagline')}</span>
            </div>
            <h1 className="brand-title">{t('app.shell.brand')}</h1>
          </div>
        </div>

        <nav className="main-nav">
          <button
            type="button"
            className={`nav-link ${currentPage === 'projects' ? 'active' : ''}`}
            onClick={() => onSelectPage('projects')}
          >
            <FolderKanban size={16} />
            <span>{t('header.projects')}</span>
            {openProjectsCount > 0 && <span className="nav-counter">{openProjectsCount}</span>}
          </button>
          <button
            type="button"
            className={`nav-link ${currentPage === 'running' ? 'active' : ''}`}
            onClick={() => onSelectPage('running')}
          >
            <Activity size={16} />
            <span>{t('header.running')}</span>
            {runningTasksCount > 0 && (
              <span className="nav-counter">{runningTasksCount}</span>
            )}
          </button>
          <button
            type="button"
            className={`nav-link ${currentPage === 'tui-agents' ? 'active' : ''}`}
            onClick={() => onSelectPage('tui-agents')}
          >
            <Terminal size={16} />
            <span>{t('app.shell.tuiAgents')}</span>
          </button>
          <button
            type="button"
            className={`nav-link ${currentPage === 'settings' ? 'active' : ''}`}
            onClick={() => onSelectPage('settings')}
          >
            <Settings size={16} />
            <span>{t('header.settings')}</span>
          </button>
        </nav>
      </div>

      <div className="header-meta-actions">
        <button
          type="button"
          className="icon-btn theme-toggle-btn"
          onClick={toggleTheme}
          title={t('header.themeToggleTitle', {
            preference: themePreference,
            resolved: resolvedTheme,
            next: resolvedTheme === 'dark' ? 'light' : 'dark',
          })}
          aria-label={t('header.themeToggleLabel', { resolved: resolvedTheme })}
        >
          {resolvedTheme === 'dark' ? <Sun size={15} /> : <Moon size={15} />}
        </button>

        <LocaleSwitcher />

        <div
          className={`server-status-pill ${isConnected ? 'online' : 'offline'}`}
          title={isConnected ? t('header.connectedTitle') : t('header.disconnectedTitle')}
          onClick={() => onSelectPage('settings')}
          role="button"
          tabIndex={0}
        >
          <span className="status-dot" />
          <Server size={14} className="status-icon" />
          <span className="status-text">{isConnected ? t('app.shell.serverConnected') : t('app.shell.serverOffline')}</span>
        </div>

        <button type="button" className="btn-primary new-project-btn" onClick={onNewProject}>
          <Plus size={16} />
          <span>{t('app.shell.newProject')}</span>
        </button>
      </div>
    </header>
  )
}

