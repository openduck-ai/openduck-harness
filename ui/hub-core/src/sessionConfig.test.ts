import test from 'node:test'
import assert from 'node:assert/strict'
import { configOptionsFromSessionResponse, configOptionsFromSessionUpdate } from './sessionConfig.ts'

const providerOption = {
  id: 'provider',
  name: 'Provider',
  type: 'select' as const,
  currentValue: 'gemini',
  options: [{ value: 'gemini', name: 'Gemini' }],
}

test('config option updates replace the session provider and model menu', () => {
  assert.equal(configOptionsFromSessionUpdate(null), null)
  assert.equal(configOptionsFromSessionUpdate({ sessionUpdate: 'agent_message_chunk' }), null)
  assert.deepEqual(
    configOptionsFromSessionUpdate({
      sessionUpdate: 'config_option_update',
      configOptions: [providerOption],
    }),
    [providerOption],
  )
  assert.deepEqual(
    configOptionsFromSessionUpdate({
      sessionUpdate: 'config_option_update',
      configOptions: [],
    }),
    [],
  )
})

test('new and loaded sessions keep their config options', () => {
  assert.equal(configOptionsFromSessionResponse(undefined), null)
  assert.equal(configOptionsFromSessionResponse({}), null)
  assert.deepEqual(configOptionsFromSessionResponse({ configOptions: [providerOption] }), [providerOption])
})
