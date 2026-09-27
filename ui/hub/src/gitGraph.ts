export type GitGraphLane = { from: number; to: number }

export interface GitGraphRow {
  sha: string
  column: number
  laneCount: number
  connections: GitGraphLane[]
}

export function layoutGitGraph(
  commits: Array<{ sha: string; parents: string[] }>,
): GitGraphRow[] {
  const rows: GitGraphRow[] = []
  let previous: Array<string | null> = []

  for (const commit of commits) {
    const lanes = [...previous]
    let column = lanes.indexOf(commit.sha)
    if (column === -1) {
      column = lanes.findIndex(lane => lane === null)
      if (column === -1) {
        column = lanes.length
        lanes.push(commit.sha)
      } else {
        lanes[column] = commit.sha
      }
    }

    const next: Array<string | null> = lanes.map(lane => (lane === commit.sha ? null : lane))
    const parents = commit.parents.filter(Boolean)
    if (parents.length > 0) {
      next[column] = parents[0]
      for (const parent of parents.slice(1)) {
        if (next.includes(parent)) continue
        const empty = next.findIndex(lane => lane === null)
        if (empty === -1) next.push(parent)
        else next[empty] = parent
      }
      const first = parents[0]
      const existing = next.findIndex((lane, index) => index !== column && lane === first)
      if (existing !== -1 && next[column] === first) {
        next[column] = null
      }
    }

    const connections: GitGraphLane[] = []
    for (let from = 0; from < lanes.length; from += 1) {
      const sha = lanes[from]
      if (!sha) continue
      if (sha === commit.sha) {
        for (const parent of parents) {
          const to = next.indexOf(parent)
          if (to !== -1) connections.push({ from, to })
        }
      } else {
        const to = next.indexOf(sha)
        if (to !== -1) connections.push({ from, to })
      }
    }

    rows.push({
      sha: commit.sha,
      column,
      laneCount: Math.max(lanes.length, next.length, 1),
      connections,
    })
    previous = next
  }

  return rows
}
