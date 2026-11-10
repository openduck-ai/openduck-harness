import assert from 'node:assert/strict'
import test from 'node:test'
import { projectModelChoices, projectProviderChoices } from './projectProviderOptions.ts'

test('provider choices come from the server list and keep a saved name that is missing', () => {
  const choices = projectProviderChoices({
    availableProviders: ['openai', 'anthropic'],
    globalProviderNames: ['ollama', 'groq'],
    current: ['custom-gateway', 'openai', '  '],
    overrides: ['anthropic'],
  })

  assert.deepEqual(choices, ['anthropic', 'custom-gateway', 'openai'])
})

test('provider choices fall back to global names when the project catalog is absent', () => {
  const choices = projectProviderChoices({
    availableProviders: undefined,
    globalProviderNames: ['openai'],
    current: ['ark'],
  })

  assert.deepEqual(choices, ['ark', 'openai'])
})

test('an empty server provider list does not invent names', () => {
  assert.deepEqual(
    projectProviderChoices({
      availableProviders: [],
      globalProviderNames: ['openai'],
      current: [null, ''],
    }),
    [],
  )
})

test('model choices prefer the project catalog for the selected provider', () => {
  const choices = projectModelChoices({
    provider: 'openai',
    providerModels: { openai: ['gpt-4o', 'gpt-4.1'], anthropic: ['claude'] },
    globalModels: ['global-only'],
    current: 'typed-model',
  })

  assert.deepEqual(choices, ['gpt-4o', 'gpt-4.1', 'typed-model'])
})

test('model choices use global models when the project catalog has no entry', () => {
  assert.deepEqual(
    projectModelChoices({
      provider: 'ollama',
      providerModels: { openai: ['gpt-4o'] },
      globalModels: ['llama3'],
      current: 'llama3',
    }),
    ['llama3'],
  )
})

test('an empty project model list stays empty except for the current value', () => {
  assert.deepEqual(
    projectModelChoices({
      provider: 'openai',
      providerModels: { openai: [] },
      globalModels: ['gpt-4o'],
      current: 'kept',
    }),
    ['kept'],
  )
})
