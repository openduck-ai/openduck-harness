import { isProjectSubTab, type ProjectSubTab } from './lazyTabs.ts'

export type NavPage = 'projects' | 'settings'

export interface UrlNavState {
  page: NavPage
  project: string | null
  tab: ProjectSubTab | null
  split: string | null
}

export function parseUrlNav(search: string): UrlNavState {
  const cleanSearch = search.startsWith('?') ? search.slice(1) : search
  const params = new URLSearchParams(cleanSearch)

  const pageParam = params.get('page')
  const page: NavPage = pageParam === 'settings' ? 'settings' : 'projects'

  const rawProject = params.get('project')?.trim() || null
  const project = rawProject || null

  const rawTab = params.get('tab')
  const tab: ProjectSubTab | null = isProjectSubTab(rawTab) ? rawTab : null

  const rawSplit = params.get('split')?.trim() || null
  const split = rawSplit || null

  return {
    page: project ? 'projects' : page,
    project,
    tab,
    split,
  }
}

export function formatUrlNavQuery(state: Partial<UrlNavState>): string {
  const params = new URLSearchParams()

  if (state.project) {
    params.set('project', state.project)
    if (state.tab) {
      params.set('tab', state.tab)
    }
    if (state.split) {
      params.set('split', state.split)
    }
  } else if (state.page === 'settings') {
    params.set('page', 'settings')
  }

  const query = params.toString()
  return query ? `?${query}` : ''
}

export function syncUrlNav(state: Partial<UrlNavState>, replace = false): void {
  if (typeof window === 'undefined' || !window.history) {
    return
  }

  const query = formatUrlNavQuery(state)
  const newUrl = `${window.location.pathname}${query}${window.location.hash}`
  const currentUrl = `${window.location.pathname}${window.location.search}${window.location.hash}`

  if (newUrl === currentUrl) {
    return
  }

  if (replace) {
    window.history.replaceState(null, '', newUrl)
  } else {
    window.history.pushState(null, '', newUrl)
  }
}
