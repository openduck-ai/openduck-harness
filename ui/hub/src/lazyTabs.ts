export type ProjectSubTab =
  | 'harness'
  | 'chat'
  | 'git'
  | 'files'
  | 'rules'
  | 'terminal'
  | 'insights'
  | 'settings'

export const PROJECT_SUBTABS: readonly ProjectSubTab[] = [
  'harness',
  'chat',
  'git',
  'files',
  'rules',
  'terminal',
  'insights',
  'settings',
] as const

export function isProjectSubTab(value: unknown): value is ProjectSubTab {
  return typeof value === 'string' && (PROJECT_SUBTABS as readonly string[]).includes(value)
}

export function getTabModulePath(tab: ProjectSubTab): string {
  switch (tab) {
    case 'harness':
      return './ProjectHarnessTab'
    case 'chat':
      return './ProjectChat'
    case 'git':
      return './ProjectGit'
    case 'files':
      return './ProjectFiles'
    case 'rules':
      return './ProjectRules'
    case 'terminal':
      return './ProjectTerminal'
    case 'insights':
      return './ProjectInsights'
    case 'settings':
      return './ProjectSettings'
  }
}

export function getTabLabel(tab: ProjectSubTab): string {
  switch (tab) {
    case 'harness':
      return 'Harness'
    case 'chat':
      return 'Chat'
    case 'git':
      return 'Git'
    case 'files':
      return 'Files'
    case 'rules':
      return 'Rules'
    case 'terminal':
      return 'Terminal'
    case 'insights':
      return 'Insights'
    case 'settings':
      return 'Settings'
  }
}
