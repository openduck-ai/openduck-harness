import assert from 'node:assert/strict'
import test from 'node:test'
import {
  PROJECT_SUBTABS,
  getTabLabel,
  getTabModulePath,
  isProjectSubTab,
  type ProjectSubTab,
} from './lazyTabs.ts'

test('PROJECT_SUBTABS contains all 8 expected subtabs', () => {
  const expected: ProjectSubTab[] = [
    'harness',
    'chat',
    'git',
    'files',
    'rules',
    'terminal',
    'insights',
    'settings',
  ]
  assert.deepEqual([...PROJECT_SUBTABS], expected)
})

test('isProjectSubTab validates valid and invalid tabs', () => {
  assert.equal(isProjectSubTab('harness'), true)
  assert.equal(isProjectSubTab('chat'), true)
  assert.equal(isProjectSubTab('git'), true)
  assert.equal(isProjectSubTab('unknown'), false)
  assert.equal(isProjectSubTab(null), false)
  assert.equal(isProjectSubTab(undefined), false)
})

test('getTabModulePath maps tabs to correct dynamic component modules', () => {
  assert.equal(getTabModulePath('harness'), './ProjectHarnessTab')
  assert.equal(getTabModulePath('chat'), './ProjectChat')
  assert.equal(getTabModulePath('git'), './ProjectGit')
  assert.equal(getTabModulePath('files'), './ProjectFiles')
  assert.equal(getTabModulePath('rules'), './ProjectRules')
  assert.equal(getTabModulePath('terminal'), './ProjectTerminal')
  assert.equal(getTabModulePath('insights'), './ProjectInsights')
  assert.equal(getTabModulePath('settings'), './ProjectSettings')
})

test('getTabLabel returns human readable labels', () => {
  assert.equal(getTabLabel('harness'), 'Harness')
  assert.equal(getTabLabel('chat'), 'Chat')
  assert.equal(getTabLabel('git'), 'Git')
  assert.equal(getTabLabel('terminal'), 'Terminal')
})
