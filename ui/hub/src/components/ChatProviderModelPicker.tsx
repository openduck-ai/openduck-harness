import { useTranslation } from 'react-i18next'
import { Cpu } from 'lucide-react'
import type { ChatProviderModelView } from '../chatProviderModel.ts'

interface ChatProviderModelPickerProps {
  view: ChatProviderModelView
  disabled: boolean
  onChange: (configId: 'provider' | 'model', value: string) => void
}

export function ChatProviderModelPicker({ view, disabled, onChange }: ChatProviderModelPickerProps) {
  const { t } = useTranslation()
  const chooseProvider = view.providers.length > 1
  const chooseModel = view.models.length > 1
  if (!chooseProvider && !chooseModel) return null

  const formatModelLabel = (value: string, fallback: string) => {
    if (view.provider === 'adaptive') {
      if (value === 'auto') return t('config.adaptive.modeAuto')
      if (value === 'regular') return t('config.adaptive.modeRegular')
      if (value === 'economy') return t('config.adaptive.modeEconomy')
    }
    return fallback
  }

  const providerLabel =
    view.providers.find(choice => choice.value === view.provider)?.label || view.provider
  const rawModelLabel = view.models.find(choice => choice.value === view.model)?.label || view.model
  const modelLabel = formatModelLabel(view.model, rawModelLabel)

  return (
    <div
      className="chat-model-picker"
      title={t('chat.page.activeModel', { provider: providerLabel, model: modelLabel })}
    >
      <Cpu size={12} className="chat-model-icon" />
      {chooseProvider ? (
        <select
          className="chat-model-select chat-model-provider"
          aria-label={t('chat.page.provider')}
          title={t('chat.page.changeProvider')}
          value={view.provider}
          disabled={disabled}
          onChange={event => {
            const value = event.target.value
            if (value && value !== view.provider) onChange('provider', value)
          }}
        >
          {!view.provider && <option value="">{t('chat.page.provider')}</option>}
          {view.providers.map(choice => (
            <option key={choice.value} value={choice.value}>
              {choice.label}
            </option>
          ))}
        </select>
      ) : (
        <span className="chat-model-provider">{providerLabel}</span>
      )}
      {(providerLabel || modelLabel) && <span className="chat-model-separator">/</span>}
      {chooseModel ? (
        <select
          className="chat-model-select chat-model-name"
          aria-label={t('chat.page.model')}
          title={t('chat.page.changeModel')}
          value={view.model}
          disabled={disabled || !view.provider}
          onChange={event => {
            const value = event.target.value
            if (value && value !== view.model) onChange('model', value)
          }}
        >
          {!view.model && <option value="">{t('chat.page.model')}</option>}
          {view.models.map(choice => (
            <option key={choice.value} value={choice.value}>
              {formatModelLabel(choice.value, choice.label)}
            </option>
          ))}
        </select>
      ) : (
        modelLabel && <span className="chat-model-name">{modelLabel}</span>
      )}
    </div>
  )
}
