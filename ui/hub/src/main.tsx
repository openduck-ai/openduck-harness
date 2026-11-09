import { Component, type ErrorInfo, type ReactNode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { LocaleProvider } from './contexts/LocaleContext.tsx'
import { ThemeProvider } from './contexts/ThemeContext'
import { initI18n, resolveInitialLocale, translate } from './i18n/index.ts'

interface ErrorBoundaryProps {
  children: ReactNode
}

interface ErrorBoundaryState {
  hasError: boolean
  error: Error | null
}

class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props)
    this.state = { hasError: false, error: null }
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('Goose Hub ErrorBoundary caught an error:', error, errorInfo)
  }

  render() {
    if (this.state.hasError) {
      return (
        <main style={{ padding: '2rem', maxWidth: '800px', margin: 'auto' }}>
          <h2 style={{ color: 'var(--accent-rose, #ff9a9a)' }}>{translate('app.errorBoundary.heading')}</h2>
          <p className="error">{this.state.error?.message || translate('app.errorBoundary.message')}</p>
          <button
            onClick={() => {
              this.setState({ hasError: false, error: null })
              window.location.reload()
            }}
          >
            {translate('app.errorBoundary.reload')}
          </button>
        </main>
      )
    }
    return this.props.children
  }
}

async function bootstrap(): Promise<void> {
  const locale = resolveInitialLocale()
  await initI18n(locale)
  if (typeof document !== 'undefined') {
    document.title = translate('app.documentTitle')
  }

  createRoot(document.getElementById('root')!).render(
    <ErrorBoundary>
      <ThemeProvider>
        <LocaleProvider initialLocale={locale}>
          <App />
        </LocaleProvider>
      </ThemeProvider>
    </ErrorBoundary>,
  )
}

void bootstrap()

