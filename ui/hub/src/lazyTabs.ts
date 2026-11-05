import { translate } from './i18n/index.ts'

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
      return translate('tabs.harness')
    case 'chat':
      return translate('tabs.chat')
    case 'git':
      return translate('tabs.git')
    case 'files':
      return translate('tabs.files')
    case 'rules':
      return translate('tabs.rules')
    case 'terminal':
      return translate('tabs.terminal')
    case 'insights':
      return translate('tabs.insights')
    case 'settings':
      return translate('tabs.settings')
  }
}
