import assert from 'node:assert/strict'
import test from 'node:test'
import {
  OPENDUCK_CONFIG_FALLBACK_ID,
  resolveSettingsSectionTarget,
  settingsNavGroups,
} from './settingsNav.ts'

test('settings nav section ids are unique', () => {
  const ids = settingsNavGroups.flatMap((group) => group.items.map((item) => item.id))
  assert.equal(new Set(ids).size, ids.length)
  assert.equal(ids.includes(OPENDUCK_CONFIG_FALLBACK_ID), false)
})

test('resolveSettingsSectionTarget keeps a mounted section', () => {
  const available = new Set(['settings-connection', 'settings-providers'])
  assert.equal(
    resolveSettingsSectionTarget('settings-providers', available),
    'settings-providers',
  )
  assert.equal(
    resolveSettingsSectionTarget('settings-connection', available),
    'settings-connection',
  )
})

test('resolveSettingsSectionTarget falls back while configuration sections are absent', () => {
  const available = new Set(['settings-connection', OPENDUCK_CONFIG_FALLBACK_ID])
  assert.equal(
    resolveSettingsSectionTarget('settings-judge', available),
    OPENDUCK_CONFIG_FALLBACK_ID,
  )
  assert.equal(
    resolveSettingsSectionTarget('settings-roots', available),
    'settings-roots',
  )
})
