export interface SettingsNavItem {
  id: string
  labelKey: string
}

export interface SettingsNavGroup {
  labelKey: string
  items: SettingsNavItem[]
}

/** In-page targets for the Hub settings menu. */
export const settingsNavGroups: SettingsNavGroup[] = [
  {
    labelKey: 'settings.nav.server',
    items: [
      { id: 'settings-connection', labelKey: 'settings.nav.connection' },
      { id: 'settings-roots', labelKey: 'settings.nav.roots' },
      { id: 'settings-appearance', labelKey: 'settings.nav.appearance' },
      { id: 'settings-diagnostics', labelKey: 'settings.nav.diagnostics' },
      { id: 'settings-about', labelKey: 'settings.nav.about' },
    ],
  },
  {
    labelKey: 'settings.nav.monitoring',
    items: [
      { id: 'settings-network', labelKey: 'settings.nav.network' },
      { id: 'settings-memory', labelKey: 'settings.nav.memory' },
    ],
  },
  {
    labelKey: 'settings.nav.configuration',
    items: [
      { id: 'settings-providers', labelKey: 'settings.nav.providers' },
      { id: 'settings-proxies', labelKey: 'settings.nav.proxies' },
      { id: 'settings-tool-mode', labelKey: 'settings.nav.toolMode' },
      { id: 'settings-harness', labelKey: 'settings.nav.harness' },
      { id: 'settings-tui-proxies', labelKey: 'settings.nav.tuiProxies' },
      { id: 'settings-judge', labelKey: 'settings.nav.judge' },
      { id: 'settings-other', labelKey: 'settings.nav.other' },
    ],
  },
]

/** Shown before the OpenDuck config sections have mounted. */
export const OPENDUCK_CONFIG_FALLBACK_ID = 'settings-openduck'

const configSectionIds = new Set(
  settingsNavGroups
    .find((group) => group.labelKey === 'settings.nav.configuration')
    ?.items.map((item) => item.id) ?? [],
)

/**
 * Scroll to the requested section. Configuration items land on the config
 * panel while that panel is still the connect or loading placeholder.
 */
export function resolveSettingsSectionTarget(
  id: string,
  available: ReadonlySet<string>,
): string {
  if (available.has(id)) return id
  if (configSectionIds.has(id)) return OPENDUCK_CONFIG_FALLBACK_ID
  return id
}
