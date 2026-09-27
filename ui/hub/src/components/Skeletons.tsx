import type { CSSProperties } from 'react'
import type { ProjectSubTab } from '../lazyTabs'

interface SkeletonBoxProps {
  width?: string | number
  height?: string | number
  borderRadius?: string | number
  className?: string
  style?: CSSProperties
}

export function SkeletonBox({
  width,
  height,
  borderRadius,
  className = '',
  style,
}: SkeletonBoxProps) {
  const styles: CSSProperties = {
    width: width ?? '100%',
    height: height ?? '1rem',
    borderRadius: borderRadius ?? 'var(--radius-sm)',
    ...style,
  }

  return <div className={`skeleton-shimmer skeleton-box ${className}`} style={styles} />
}

export function SkeletonLine({
  width = '100%',
  height = '0.875rem',
  className = '',
}: {
  width?: string
  height?: string
  className?: string
}) {
  return <SkeletonBox width={width} height={height} className={`skeleton-line ${className}`} />
}

export function SkeletonCircle({
  size = 32,
  className = '',
}: {
  size?: number
  className?: string
}) {
  return (
    <SkeletonBox
      width={size}
      height={size}
      borderRadius="50%"
      className={`skeleton-circle ${className}`}
    />
  )
}

export function ProjectListSkeleton({ viewMode = 'grid' }: { viewMode?: 'grid' | 'table' }) {
  if (viewMode === 'table') {
    return (
      <div className="panel table-panel skeleton-table-panel">
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
            {Array.from({ length: 5 }).map((_, i) => (
              <tr key={i} className="skeleton-table-row">
                <td>
                  <SkeletonLine width="140px" height="1rem" />
                  <SkeletonLine width="80px" height="0.75rem" className="mt-1" />
                </td>
                <td>
                  <SkeletonBox width="80px" height="1.25rem" borderRadius="999px" />
                </td>
                <td>
                  <SkeletonLine width="200px" height="0.875rem" />
                </td>
                <td>
                  <SkeletonBox width="60px" height="1.25rem" borderRadius="999px" />
                </td>
                <td>
                  <SkeletonLine width="90px" height="0.75rem" />
                </td>
                <td className="text-right">
                  <SkeletonBox width="60px" height="1.75rem" borderRadius="var(--radius-sm)" />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    )
  }

  return (
    <div className="project-cards-grid skeleton-grid">
      {Array.from({ length: 6 }).map((_, i) => (
        <div key={i} className="card modern-project-card skeleton-card">
          <div className="card-top-row">
            <div className="project-title-area" style={{ width: '65%' }}>
              <SkeletonLine width="75%" height="1.2rem" />
              <SkeletonLine width="45%" height="0.75rem" className="mt-1" />
            </div>
            <SkeletonBox width="64px" height="22px" borderRadius="999px" />
          </div>

          <div className="mt-3">
            <SkeletonLine width="90%" height="0.85rem" />
            <SkeletonLine width="60%" height="0.85rem" className="mt-1" />
          </div>

          <div className="project-card-path-box mt-3" style={{ borderStyle: 'dashed' }}>
            <SkeletonLine width="80%" height="0.75rem" />
          </div>

          <div className="card-footer-row mt-4">
            <div className="tags-row" style={{ gap: '0.5rem' }}>
              <SkeletonBox width="65px" height="20px" borderRadius="999px" />
              <SkeletonBox width="50px" height="20px" borderRadius="999px" />
            </div>
            <SkeletonBox width="45px" height="16px" />
          </div>
        </div>
      ))}
    </div>
  )
}

export function ProjectDetailHeaderSkeleton() {
  return (
    <div className="detail-head skeleton-detail-head">
      <div className="detail-head-info" style={{ width: '70%' }}>
        <div className="eyebrow-row">
          <SkeletonLine width="120px" height="0.75rem" />
          <SkeletonBox width="70px" height="18px" borderRadius="999px" />
        </div>
        <SkeletonLine width="320px" height="1.75rem" className="mt-2" />
        <SkeletonLine width="480px" height="0.9rem" className="mt-2" />
        <div className="project-meta-chips mt-3">
          <SkeletonBox width="220px" height="24px" borderRadius="var(--radius-sm)" />
          <SkeletonBox width="90px" height="24px" borderRadius="var(--radius-sm)" />
        </div>
      </div>
      <div className="detail-badges">
        <SkeletonBox width="75px" height="26px" borderRadius="999px" />
      </div>
    </div>
  )
}

export function TabSkeleton({ tab }: { tab: ProjectSubTab }) {
  switch (tab) {
    case 'harness':
      return (
        <div className="tab-skeleton-container">
          <div className="harness-skeleton-stats-grid">
            {Array.from({ length: 4 }).map((_, i) => (
              <div key={i} className="card skeleton-stat-card">
                <SkeletonLine width="50%" height="0.75rem" />
                <SkeletonLine width="35%" height="1.75rem" className="mt-2" />
                <SkeletonLine width="70%" height="0.7rem" className="mt-2" />
              </div>
            ))}
          </div>

          <div className="card skeleton-content-box mt-4">
            <div className="skeleton-toolbar">
              <SkeletonBox width="140px" height="32px" borderRadius="var(--radius-sm)" />
              <div className="skeleton-toolbar-right">
                <SkeletonBox width="200px" height="32px" borderRadius="var(--radius-sm)" />
                <SkeletonBox width="100px" height="32px" borderRadius="var(--radius-sm)" />
              </div>
            </div>

            <div className="skeleton-list-rows mt-4">
              {Array.from({ length: 4 }).map((_, i) => (
                <div key={i} className="skeleton-list-row">
                  <div style={{ flex: 1 }}>
                    <SkeletonLine width="40%" height="1.1rem" />
                    <SkeletonLine width="70%" height="0.8rem" className="mt-2" />
                  </div>
                  <SkeletonBox width="90px" height="28px" borderRadius="var(--radius-sm)" />
                </div>
              ))}
            </div>
          </div>
        </div>
      )

    case 'chat':
      return (
        <div className="tab-skeleton-container chat-skeleton-layout">
          <div className="chat-skeleton-messages">
            <div className="chat-skeleton-bubble chat-skeleton-user">
              <SkeletonLine width="180px" height="0.9rem" />
            </div>
            <div className="chat-skeleton-bubble chat-skeleton-agent">
              <SkeletonLine width="90%" height="0.9rem" />
              <SkeletonLine width="75%" height="0.9rem" className="mt-2" />
              <SkeletonLine width="50%" height="0.9rem" className="mt-2" />
            </div>
            <div className="chat-skeleton-bubble chat-skeleton-user">
              <SkeletonLine width="240px" height="0.9rem" />
            </div>
            <div className="chat-skeleton-bubble chat-skeleton-agent">
              <SkeletonLine width="80%" height="0.9rem" />
              <SkeletonLine width="65%" height="0.9rem" className="mt-2" />
            </div>
          </div>

          <div className="chat-skeleton-input-box">
            <SkeletonBox height="50px" borderRadius="var(--radius-md)" />
          </div>
        </div>
      )

    case 'git':
      return (
        <div className="tab-skeleton-container">
          <div className="git-skeleton-top">
            <SkeletonBox width="160px" height="32px" borderRadius="var(--radius-sm)" />
            <SkeletonBox width="100px" height="32px" borderRadius="var(--radius-sm)" />
          </div>
          <div className="card skeleton-git-card mt-3">
            <SkeletonLine width="120px" height="1rem" />
            <div className="skeleton-timeline mt-3">
              {Array.from({ length: 4 }).map((_, i) => (
                <div key={i} className="skeleton-timeline-item">
                  <SkeletonCircle size={16} />
                  <div style={{ flex: 1 }}>
                    <SkeletonLine width="55%" height="0.9rem" />
                    <SkeletonLine width="30%" height="0.75rem" className="mt-1" />
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )

    case 'files':
      return (
        <div className="tab-skeleton-container files-skeleton-layout">
          <div className="card files-skeleton-tree">
            <SkeletonLine width="100px" height="0.85rem" />
            <div className="mt-3" style={{ display: 'flex', flexDirection: 'column', gap: '0.6rem' }}>
              {Array.from({ length: 6 }).map((_, i) => (
                <div key={i} style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                  <SkeletonBox width="16px" height="16px" borderRadius="3px" />
                  <SkeletonLine width={`${50 + (i % 3) * 20}%`} height="0.8rem" />
                </div>
              ))}
            </div>
          </div>
          <div className="card files-skeleton-viewer">
            <SkeletonLine width="180px" height="0.9rem" />
            <SkeletonBox height="240px" className="mt-3" borderRadius="var(--radius-sm)" />
          </div>
        </div>
      )

    case 'rules':
      return (
        <div className="tab-skeleton-container rules-skeleton-layout">
          <div className="card rules-skeleton-sidebar">
            <SkeletonBox width="100%" height="32px" borderRadius="var(--radius-sm)" />
            <div className="mt-3" style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem' }}>
              {Array.from({ length: 4 }).map((_, i) => (
                <SkeletonBox key={i} height="40px" borderRadius="var(--radius-sm)" />
              ))}
            </div>
          </div>
          <div className="card rules-skeleton-editor">
            <SkeletonLine width="200px" height="1.2rem" />
            <SkeletonBox height="260px" className="mt-3" borderRadius="var(--radius-sm)" />
          </div>
        </div>
      )

    case 'terminal':
      return (
        <div className="tab-skeleton-container terminal-skeleton-layout">
          <div className="terminal-skeleton-window">
            <div className="terminal-skeleton-bar">
              <SkeletonCircle size={10} />
              <SkeletonCircle size={10} />
              <SkeletonCircle size={10} />
              <SkeletonLine width="120px" height="0.75rem" className="ml-2" />
            </div>
            <div className="terminal-skeleton-body">
              <SkeletonLine width="40%" height="0.85rem" />
              <SkeletonLine width="60%" height="0.85rem" className="mt-2" />
              <SkeletonLine width="30%" height="0.85rem" className="mt-2" />
              <div className="terminal-skeleton-cursor mt-3" />
            </div>
          </div>
        </div>
      )

    case 'insights':
      return (
        <div className="tab-skeleton-container">
          <div className="insights-skeleton-grid">
            {Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="card skeleton-stat-card">
                <SkeletonLine width="60%" height="0.8rem" />
                <SkeletonLine width="40%" height="1.8rem" className="mt-2" />
              </div>
            ))}
          </div>
          <div className="card skeleton-content-box mt-4">
            <SkeletonLine width="160px" height="1.1rem" />
            <SkeletonBox height="220px" className="mt-3" borderRadius="var(--radius-sm)" />
          </div>
        </div>
      )

    case 'settings':
    default:
      return (
        <div className="tab-skeleton-container settings-skeleton-layout">
          <div className="card skeleton-content-box">
            <SkeletonLine width="140px" height="1.2rem" />
            <SkeletonLine width="280px" height="0.8rem" className="mt-1" />

            <div className="mt-4" style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
              <div>
                <SkeletonLine width="100px" height="0.8rem" />
                <SkeletonBox height="36px" className="mt-1" borderRadius="var(--radius-sm)" />
              </div>
              <div>
                <SkeletonLine width="120px" height="0.8rem" />
                <SkeletonBox height="70px" className="mt-1" borderRadius="var(--radius-sm)" />
              </div>
              <div style={{ display: 'flex', gap: '0.75rem' }}>
                <SkeletonBox width="100px" height="34px" borderRadius="var(--radius-sm)" />
                <SkeletonBox width="90px" height="34px" borderRadius="var(--radius-sm)" />
              </div>
            </div>
          </div>
        </div>
      )
  }
}
