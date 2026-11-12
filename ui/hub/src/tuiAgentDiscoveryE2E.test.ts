import test from 'node:test'
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  getModelDiscoveryCommand,
  parseModelsFromTuiOutput,
  discoverAgentModelsViaSession,
} from './tuiModelPicker.ts'

function commandExists(cmd: string): boolean {
  try {
    const res = spawnSync('which', [cmd], { stdio: 'pipe' })
    return res.status === 0
  } catch {
    return false
  }
}

test('E2E: live Claude Code TUI session model discovery via /model in tmux', async t => {
  if (!commandExists('tmux')) {
    t.skip('tmux is not installed on this system')
    return
  }
  if (!commandExists('claude')) {
    t.skip('claude CLI is not installed on this system')
    return
  }

  const agent = 'claude'
  const sessionName = `e2e_live_claude_${Date.now()}`

  try {
    // 1. Automatically determine discovery command
    const cmd = getModelDiscoveryCommand(agent)
    assert.equal(cmd, '/model')

    // 2. Start real tmux session with claude
    const startRes = spawnSync('tmux', [
      'new-session',
      '-d',
      '-s',
      sessionName,
      '-x',
      '120',
      '-y',
      '40',
      'claude',
    ])
    assert.equal(startRes.status, 0, 'tmux new-session should succeed')

    // 3. Wait for Claude prompt to become ready
    let ready = false
    for (let i = 0; i < 20; i++) {
      spawnSync('sleep', ['0.5'])
      const screen = spawnSync('tmux', ['capture-pane', '-t', sessionName, '-p']).stdout.toString()
      if (screen.includes('❯') || screen.includes('manual mode') || screen.includes('Try "')) {
        ready = true
        break
      }
    }
    assert.ok(ready, 'Claude Code TUI should become ready to receive commands')

    // 4. Send discovery command
    spawnSync('tmux', ['send-keys', '-t', sessionName, cmd, 'Enter'])
    spawnSync('sleep', ['2'])

    // 5. Capture terminal screen
    const rawScreen = spawnSync('tmux', ['capture-pane', '-t', sessionName, '-p']).stdout.toString()
    assert.ok(
      rawScreen.includes('Select model') || rawScreen.includes('Claude models'),
      'Terminal should render Claude model selection menu',
    )

    // 6. Parse models from TUI screen
    const models = parseModelsFromTuiOutput(rawScreen, agent)
    assert.ok(models.length >= 4, `Should parse at least 4 models, got ${models.length}`)

    const modelIds = models.map(m => m.id.toLowerCase())
    assert.ok(modelIds.includes('opus'), 'Should parse Opus model')
    assert.ok(modelIds.includes('sonnet'), 'Should parse Sonnet model')
    assert.ok(modelIds.includes('haiku'), 'Should parse Haiku model')

    // Verify displayName and description
    const opus = models.find(m => m.id.toLowerCase() === 'opus')
    assert.ok(opus?.displayName?.includes('Opus'), 'Opus should have display name')
    assert.ok(opus?.description?.includes('Mtok'), 'Opus should have description from pricing')
  } finally {
    spawnSync('tmux', ['kill-session', '-t', sessionName])
  }
})

test('E2E: live Agy model catalog extraction', async t => {
  if (!commandExists('agy')) {
    t.skip('agy CLI is not installed on this system')
    return
  }

  const agent = 'agy'
  const cmd = getModelDiscoveryCommand(agent)
  assert.equal(cmd, '/models')

  // Execute agy models with timeout and one retry in case of upstream network latency
  let res = spawnSync('agy', ['models'], { stdio: 'pipe', timeout: 15000 })
  if (res.status !== 0 || res.error) {
    res = spawnSync('agy', ['models'], { stdio: 'pipe', timeout: 15000 })
  }

  const stdout = res.stdout ? res.stdout.toString() : ''
  const models = parseModelsFromTuiOutput(stdout, agent)

  if (models.length === 0 && res.status !== 0) {
    const errText = res.stderr?.toString() || res.error?.message || `exit code ${res.status}`
    t.skip(`agy models upstream network request timed out or failed: ${errText.trim()}`)
    return
  }

  assert.ok(models.length >= 5, `agy models should extract multiple models, got ${models.length}`)

  const ids = models.map(m => m.id)
  assert.ok(ids.some(id => id.includes('gemini')), 'Should include Gemini models')
  assert.ok(ids.some(id => id.includes('argon') || id.includes('flash')), 'Should include Argon/Flash models')
})

test('E2E: discoverAgentModelsViaSession end-to-end with live tmux runner', async t => {
  if (!commandExists('tmux') || !commandExists('claude')) {
    t.skip('tmux or claude not installed')
    return
  }

  // Construct API adapter driving actual tmux session
  let activeSession: string | null = null
  const liveTmuxApi = {
    startManagedTuiSession: async (data: { agent: string; sessionName?: string }) => {
      const name = data.sessionName || `e2e_api_${Date.now()}`
      activeSession = name
      spawnSync('tmux', ['new-session', '-d', '-s', name, '-x', '120', '-y', '40', data.agent])
      for (let i = 0; i < 20; i++) {
        spawnSync('sleep', ['0.5'])
        const screen = spawnSync('tmux', ['capture-pane', '-t', name, '-p']).stdout.toString()
        if (screen.includes('❯') || screen.includes('manual mode')) {
          break
        }
      }
      return { session: { id: name, sessionName: name } }
    },
    promptTuiSession: async (id: string, payload: { prompt: string }) => {
      spawnSync('tmux', ['send-keys', '-t', id, payload.prompt, 'Enter'])
      spawnSync('sleep', ['2'])
      const screen = spawnSync('tmux', ['capture-pane', '-t', id, '-p']).stdout.toString()
      return { response: screen, screenSnapshot: screen }
    },
    stopManagedTuiSession: async (id: string) => {
      spawnSync('tmux', ['kill-session', '-t', id])
      activeSession = null
      return { success: true }
    },
    getManagedTuiSession: async (id: string) => {
      const screen = spawnSync('tmux', ['capture-pane', '-t', id, '-p']).stdout.toString()
      return { session: { id }, screen }
    },
  }

  try {
    const outcome = await discoverAgentModelsViaSession(liveTmuxApi, 'claude', null)
    assert.equal(outcome.success, true, 'Live discovery should succeed')
    assert.ok(outcome.catalog, 'Should return catalog')
    assert.ok(outcome.catalog!.models.length >= 4, 'Catalog should contain parsed models')
    assert.ok(outcome.catalog!.models.some(m => m.id.toLowerCase() === 'opus'))
  } finally {
    if (activeSession) {
      spawnSync('tmux', ['kill-session', '-t', activeSession])
    }
  }
})

