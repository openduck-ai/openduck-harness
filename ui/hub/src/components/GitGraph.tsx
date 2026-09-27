import { useEffect, useState, type MouseEvent } from 'react'
import { createPortal } from 'react-dom'
import type { GitCommitEntry } from '@aaif/goose-hub-core'
import { layoutGitGraph } from '../gitGraph'

const ROW_HEIGHT = 36
const COL_WIDTH = 14
const LANE_COLORS = [
  'var(--accent-lime-text)',
  'var(--accent-sky)',
  'var(--accent-purple)',
  'var(--accent-amber)',
  'var(--accent-emerald)',
  'var(--accent-rose)',
]

interface HoverTip {
  text: string
  x: number
  y: number
  below: boolean
}

function laneColor(index: number): string {
  return LANE_COLORS[index % LANE_COLORS.length]
}

function formatTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

interface GitGraphProps {
  commits: GitCommitEntry[]
  selectedSha: string | null
  onSelect: (sha: string) => void
}

function GitRefs({
  refs,
  onShowTip,
  onHideTip,
}: {
  refs: string[]
  onShowTip: (event: MouseEvent<HTMLElement>, text: string) => void
  onHideTip: () => void
}) {
  if (refs.length === 0) return null

  const named = refs.filter(ref => ref !== 'HEAD')
  const extra = Math.max(0, named.length - 1)
  const allRefs = refs.join('\n')

  return (
    <span
      className="git-refs"
      aria-label={allRefs}
      onMouseEnter={event => onShowTip(event, allRefs)}
      onMouseLeave={onHideTip}
    >
      {named.length === 0 && <span className="git-head-dot" />}
      {named[0] && <span className="git-ref">{named[0]}</span>}
      {extra > 0 && <span className="git-ref git-ref-more">+{extra}</span>}
    </span>
  )
}

export function GitGraph({ commits, selectedSha, onSelect }: GitGraphProps) {
  const rows = layoutGitGraph(commits)
  const maxLanes = Math.max(1, ...rows.map(row => row.laneCount))
  const width = maxLanes * COL_WIDTH + 8
  const height = Math.max(rows.length * ROW_HEIGHT, ROW_HEIGHT)
  const [tip, setTip] = useState<HoverTip | null>(null)

  useEffect(() => {
    if (!tip) return
    const hide = () => setTip(null)
    window.addEventListener('scroll', hide, true)
    window.addEventListener('resize', hide)
    return () => {
      window.removeEventListener('scroll', hide, true)
      window.removeEventListener('resize', hide)
    }
  }, [tip])

  const showTip = (event: MouseEvent<HTMLElement>, text: string) => {
    const rect = event.currentTarget.getBoundingClientRect()
    setTip({
      text,
      x: Math.min(Math.max(rect.left + rect.width / 2, 16), window.innerWidth - 16),
      y: rect.top < 48 ? rect.bottom : rect.top,
      below: rect.top < 48,
    })
  }

  if (commits.length === 0) {
    return <p className="muted git-empty">No commits in this repository yet.</p>
  }

  return (
    <div className="git-log">
      <svg
        className="git-graph-svg"
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        aria-hidden="true"
      >
        {rows.map((row, index) => {
          const y = index * ROW_HEIGHT + ROW_HEIGHT / 2
          const nextY = y + ROW_HEIGHT
          return (
            <g key={row.sha}>
              {row.connections.map((lane, laneIndex) => {
                const x1 = lane.from * COL_WIDTH + COL_WIDTH / 2 + 4
                const x2 = lane.to * COL_WIDTH + COL_WIDTH / 2 + 4
                const color = laneColor(Math.max(lane.from, lane.to))
                if (x1 === x2) {
                  return (
                    <line
                      key={`${row.sha}-${laneIndex}`}
                      x1={x1}
                      y1={y}
                      x2={x2}
                      y2={nextY}
                      stroke={color}
                      strokeWidth="1.6"
                    />
                  )
                }
                const mid = y + ROW_HEIGHT / 2
                return (
                  <path
                    key={`${row.sha}-${laneIndex}`}
                    d={`M ${x1} ${y} C ${x1} ${mid}, ${x2} ${mid}, ${x2} ${nextY}`}
                    fill="none"
                    stroke={color}
                    strokeWidth="1.6"
                  />
                )
              })}
              {commits[index]?.refs.includes('HEAD') && (
                <circle
                  cx={row.column * COL_WIDTH + COL_WIDTH / 2 + 4}
                  cy={y}
                  r={7.5}
                  fill="none"
                  stroke="var(--accent-lime-text)"
                  strokeWidth="1.4"
                />
              )}
              <circle
                cx={row.column * COL_WIDTH + COL_WIDTH / 2 + 4}
                cy={y}
                r={selectedSha === row.sha ? 5 : 4}
                fill={laneColor(row.column)}
                stroke={selectedSha === row.sha ? 'var(--text-heading)' : 'var(--bg-card)'}
                strokeWidth={selectedSha === row.sha ? 1.5 : 1}
              />
            </g>
          )
        })}
      </svg>
      <ol className="git-log-rows">
        {commits.map(commit => (
          <li key={commit.sha}>
            <button
              type="button"
              className={`git-log-row ${selectedSha === commit.sha ? 'selected' : ''}`}
              onClick={() => onSelect(commit.sha)}
            >
              <span className="git-log-subject">
                <span className="git-subject-text" title={commit.subject || '(no subject)'}>
                  {commit.subject || '(no subject)'}
                </span>
                <GitRefs refs={commit.refs} onShowTip={showTip} onHideTip={() => setTip(null)} />
              </span>
              <span className="git-log-meta">
                <code>{commit.shortSha}</code>
                <span>{commit.authorName}</span>
                <span>{formatTime(commit.authoredAt)}</span>
              </span>
            </button>
          </li>
        ))}
      </ol>
      {tip &&
        createPortal(
          <div
            className={`git-hover-tip${tip.below ? ' below' : ''}`}
            style={{ left: tip.x, top: tip.y }}
            role="tooltip"
          >
            {tip.text.split('\n').map(line => (
              <div key={line}>{line}</div>
            ))}
          </div>,
          document.body,
        )}
    </div>
  )
}
