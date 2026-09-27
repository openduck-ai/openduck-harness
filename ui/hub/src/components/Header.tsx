import {
  FolderKanban,
  Settings,
  Plus,
  Server,
  Bot,
  Sun,
  Moon,
} from 'lucide-react'
import { useTheme } from '../contexts/ThemeContext'

export type NavPage = 'projects' | 'settings'

interface HeaderProps {
  currentPage: NavPage
  onSelectPage: (page: NavPage) => void
  isConnected: boolean
  onNewProject: () => void
  openProjectsCount: number
}

export function Header({
  currentPage,
  onSelectPage,
  isConnected,
  onNewProject,
  openProjectsCount,
}: HeaderProps) {
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
              <span className="eyebrow">Lightweight Agent Harness</span>
            </div>
            <h1 className="brand-title">OpenDuck Hub</h1>
          </div>
        </div>

        <nav className="main-nav">
          <button
            type="button"
            className={`nav-link ${currentPage === 'projects' ? 'active' : ''}`}
            onClick={() => onSelectPage('projects')}
          >
            <FolderKanban size={16} />
            <span>Projects</span>
            {openProjectsCount > 0 && <span className="nav-counter">{openProjectsCount}</span>}
          </button>
          <button
            type="button"
            className={`nav-link ${currentPage === 'settings' ? 'active' : ''}`}
            onClick={() => onSelectPage('settings')}
          >
            <Settings size={16} />
            <span>Settings</span>
          </button>
        </nav>
      </div>

      <div className="header-meta-actions">
        <button
          type="button"
          className="icon-btn theme-toggle-btn"
          onClick={toggleTheme}
          title={`Theme: ${themePreference} (showing ${resolvedTheme}). Click to switch to ${
            resolvedTheme === 'dark' ? 'light' : 'dark'
          } mode.`}
          aria-label={`Toggle theme (currently ${resolvedTheme})`}
        >
          {resolvedTheme === 'dark' ? <Sun size={15} /> : <Moon size={15} />}
        </button>

        <div
          className={`server-status-pill ${isConnected ? 'online' : 'offline'}`}
          title={isConnected ? 'Connected to Goose / OpenDuck server' : 'Disconnected from server'}
          onClick={() => onSelectPage('settings')}
          role="button"
          tabIndex={0}
        >
          <span className="status-dot" />
          <Server size={14} className="status-icon" />
          <span className="status-text">{isConnected ? 'Server Connected' : 'Server Offline'}</span>
        </div>

        <button type="button" className="btn-primary new-project-btn" onClick={onNewProject}>
          <Plus size={16} />
          <span>New Project</span>
        </button>
      </div>
    </header>
  )
}

