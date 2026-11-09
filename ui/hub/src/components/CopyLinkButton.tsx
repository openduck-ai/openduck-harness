import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Check, Link2 } from 'lucide-react'

export function CopyLinkButton({ url, className }: { url: string; className?: string }) {
  const { t } = useTranslation()
  const [status, setStatus] = useState<'idle' | 'copied' | 'failed'>('idle')

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(url)
      setStatus('copied')
      window.setTimeout(() => setStatus('idle'), 2000)
    } catch {
      setStatus('failed')
    }
  }

  const label =
    status === 'copied'
      ? t('deepLink.linkCopied')
      : status === 'failed'
        ? t('deepLink.copyLinkFailed')
        : t('deepLink.copyLink')

  return (
    <button
      type="button"
      className={className ?? 'btn-secondary mini-btn'}
      style={{ display: 'inline-flex', alignItems: 'center', gap: '0.35rem' }}
      onClick={() => void copy()}
      title={label}
    >
      {status === 'copied' ? <Check size={14} /> : <Link2 size={14} />}
      <span>{label}</span>
    </button>
  )
}
