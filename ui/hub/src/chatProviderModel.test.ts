import test from 'node:test'
import assert from 'node:assert/strict'
import { chatProviderModelView } from './chatProviderModel.ts'

const gemini = {
  name: 'gemini',
  enabled: true,
  configured: true,
  model: 'gemini-3.8-flash',
  models: [
    { id: 'gemini-3.8-flash', name: 'Gemini 3.8 Flash' },
    { id: 'gemini-pro', name: 'Gemini Pro' },
  ],
}

test('a live session menu is the provider and model list', () => {
  const view = chatProviderModelView({
    configOptions: [
      {
        id: 'provider',
        type: 'select',
        currentValue: 'gemini',
        options: [
          { value: 'goose', name: 'Goose (Default)' },
          { value: 'gemini', name: 'Gemini' },
        ],
      },
      {
        id: 'mode',
        type: 'select',
        currentValue: 'auto',
        options: [{ value: 'auto', name: 'Auto' }],
      },
      {
        id: 'model',
        type: 'select',
        currentValue: 'gemini-pro',
        options: [
          { group: 'recommended', name: 'Recommended', options: [{ value: 'gemini-pro', name: 'Gemini Pro' }] },
          { value: 'gemini-3.8-flash', name: 'Flash' },
        ],
      },
    ],
    fallbackProvider: 'openai',
    fallbackModel: 'gpt-4o',
    globalProviders: [gemini],
  })

  assert.equal(view.provider, 'gemini')
  assert.equal(view.model, 'gemini-pro')
  assert.deepEqual(view.providers.map(choice => choice.label), ['Goose (Default)', 'Gemini'])
  assert.deepEqual(view.models.map(choice => choice.value), ['gemini-pro', 'gemini-3.8-flash'])
})

test('before a session exists the picker uses the project catalog and the current model', () => {
  const view = chatProviderModelView({
    availableProviders: ['gemini'],
    providerModels: { gemini: ['gemini-3.8-flash'] },
    globalProviders: [gemini, { name: 'openai', enabled: false, configured: true, model: 'gpt-4o' }],
    fallbackProvider: 'gemini',
    fallbackModel: 'custom-model',
  })

  assert.equal(view.provider, 'gemini')
  assert.equal(view.model, 'custom-model')
  assert.deepEqual(view.providers.map(choice => choice.value), ['gemini'])
  assert.deepEqual(view.models.map(choice => choice.value), ['gemini-3.8-flash', 'custom-model'])
  assert.equal(view.models[0]?.label, 'Gemini 3.8 Flash')
})

test('a provider override keeps the session provider list and loads the new provider models', () => {
  const view = chatProviderModelView({
    configOptions: [
      {
        id: 'provider',
        type: 'select',
        currentValue: 'gemini',
        options: [
          { value: 'gemini', name: 'Gemini' },
          { value: 'openai', name: 'OpenAI' },
        ],
      },
      {
        id: 'model',
        type: 'select',
        currentValue: 'gemini-pro',
        options: [{ value: 'gemini-pro', name: 'Gemini Pro' }],
      },
    ],
    globalProviders: [
      gemini,
      {
        name: 'openai',
        enabled: true,
        configured: true,
        model: 'gpt-4o',
        models: [{ id: 'gpt-4o', name: 'GPT-4o' }],
      },
    ],
    providerOverride: 'openai',
  })

  assert.equal(view.provider, 'openai')
  assert.equal(view.model, 'gpt-4o')
  assert.deepEqual(view.providers.map(choice => choice.value), ['gemini', 'openai'])
  assert.deepEqual(view.models.map(choice => choice.value), ['gpt-4o'])
})

test('an empty server provider list does not invent provider names', () => {
  const view = chatProviderModelView({
    availableProviders: [],
    fallbackProvider: 'saved-provider',
    fallbackModel: 'saved-model',
  })

  assert.deepEqual(view.providers.map(choice => choice.value), ['saved-provider'])
  assert.deepEqual(view.models.map(choice => choice.value), ['saved-model'])
})
