import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import type { FormEvent } from 'react'
import {
  toolTitleFromPermission,
  useProjectSession,
  createApi,
  type ProjectDetail,
  type SessionSummary,
} from '@aaif/goose-hub-core'
import {
  Send,
  Plus,
  History,
  Copy,
  Check,
  Bot,
  User,
  Wrench,
  CheckCircle2,
  AlertCircle,
  Loader2,
  ShieldAlert,
  Sparkles,
  ChevronDown,
  ChevronRight,
  Terminal,
  Folder,
} from 'lucide-react'
import {
  MentionPopover,
  type MentionPopoverHandle,
} from './MentionPopover.tsx'
import {
  detectMentionTrigger,
  applyMentionInsertion,
  fetchAgentMentions,
  fetchRuleMentions,
  scanProjectFiles,
  searchProjectFiles,
  mergeMentionItems,
  shouldIgnoreMentionTrigger,
  debounce,
  type MentionDisplayItem,
  type DismissedMention,
} from '../mention.ts'

interface ProjectChatProps {
  detail: ProjectDetail
  baseUrl: string
  secret: string
  api?: ReturnType<typeof createApi>
  selectedSessionId?: string | null
}

export function ProjectChat({
  detail,
  baseUrl,
  secret,
  api,
  selectedSessionId,
}: ProjectChatProps) {
  const [prompt, setPrompt] = useState('')
  const [copiedSessionId, setCopiedSessionId] = useState(false)
  const [expandedTools, setExpandedTools] = useState<Record<string, boolean>>({})
  const transcriptEndRef = useRef<HTMLDivElement | null>(null)
  const textareaRef = useRef<HTMLTextAreaElement | null>(null)
  const inputContainerRef = useRef<HTMLDivElement | null>(null)

  const [mentionItems, setMentionItems] = useState<MentionDisplayItem[]>([])
  const [loadingMentions, setLoadingMentions] = useState(false)
  const [mentionPopover, setMentionPopover] = useState<{
    isOpen: boolean
    query: string
    mentionStart: number
    selectedIndex: number
  }>({
    isOpen: false,
    query: '',
    mentionStart: -1,
    selectedIndex: 0,
  })
  const mentionPopoverRef = useRef<MentionPopoverHandle | null>(null)
  const dismissedMentionRef = useRef<DismissedMention | null>(null)
  const lastDismissedTimeRef = useRef<number>(0)

  const session = useProjectSession({
    baseUrl,
    secretKey: secret,
    projectId: detail.project.slug,
    cwd: detail.project.path,
    client: 'goose-hub',
  })

  // If a specific session ID was requested from outside (e.g. Sessions page), load it
  useEffect(() => {
    if (selectedSessionId && selectedSessionId !== session.sessionId) {
      void session.loadSession(selectedSessionId)
    }
  }, [selectedSessionId])

  // Scroll transcript to bottom on new messages
  useEffect(() => {
    transcriptEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [session.messages, session.statusLine, session.isPrompting])

  const loadMentionSources = useCallback(async () => {
    setLoadingMentions(true)
    try {
      const apiInstance = api ?? createApi(baseUrl, secret)
      const [agentItems, ruleItems, fileItems] = await Promise.all([
        fetchAgentMentions(baseUrl, secret, detail.project.path, session.sessionId),
        fetchRuleMentions(baseUrl, secret, detail.project.path),
        scanProjectFiles(apiInstance, detail.project.slug),
      ])
      setMentionItems([...agentItems, ...ruleItems, ...fileItems])
    } catch (err) {
      console.debug('Failed to load mention items:', err)
    } finally {
      setLoadingMentions(false)
    }
  }, [api, baseUrl, secret, detail.project.slug, detail.project.path, session.sessionId])

  useEffect(() => {
    void loadMentionSources()
  }, [loadMentionSources])

  const [serverSearchItems, setServerSearchItems] = useState<MentionDisplayItem[]>([])
  const [isSearchingServer, setIsSearchingServer] = useState(false)

  useEffect(() => {
    const trimmed = mentionPopover.query.trim()
    if (!mentionPopover.isOpen || !trimmed) {
      setServerSearchItems([])
      return
    }

    let cancelled = false
    setIsSearchingServer(true)

    const timer = setTimeout(async () => {
      try {
        const apiInstance = api ?? createApi(baseUrl, secret)
        const results = await searchProjectFiles(apiInstance, detail.project.slug, trimmed)
        if (!cancelled) {
          setServerSearchItems(results)
        }
      } catch (err) {
        console.debug('Failed to search project files on server:', err)
      } finally {
        if (!cancelled) {
          setIsSearchingServer(false)
        }
      }
    }, 120)

    return () => {
      cancelled = true
      clearTimeout(timer)
      setIsSearchingServer(false)
    }
  }, [mentionPopover.isOpen, mentionPopover.query, api, baseUrl, secret, detail.project.slug])

  const combinedMentionItems = useMemo(() => {
    if (serverSearchItems.length === 0) return mentionItems
    return mergeMentionItems([serverSearchItems, mentionItems])
  }, [mentionItems, serverSearchItems])

  const copySessionId = () => {
    if (!session.sessionId) return
    void navigator.clipboard.writeText(session.sessionId)
    setCopiedSessionId(true)
    setTimeout(() => setCopiedSessionId(false), 2000)
  }

  const toggleToolExpand = (toolCallId: string) => {
    setExpandedTools(prev => ({
      ...prev,
      [toolCallId]: !prev[toolCallId],
    }))
  }

  const send = async (event: FormEvent) => {
    event.preventDefault()
    const text = prompt.trim()
    if (!text || session.isPrompting) return
    dismissedMentionRef.current = null
    setPrompt('')
    setMentionPopover(prev => ({ ...prev, isOpen: false }))
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto'
    }
    await session.sendPrompt(text)
  }

  const handleSuggestedPrompt = (suggestedText: string) => {
    setPrompt(suggestedText)
    textareaRef.current?.focus()
  }

  const updateMentionState = useCallback(
    (text: string, cursorPos: number) => {
      const trigger = detectMentionTrigger(text, cursorPos)
      if (!trigger) {
        dismissedMentionRef.current = null
        setMentionPopover(prev => (prev.isOpen ? { ...prev, isOpen: false } : prev))
        return
      }

      if (shouldIgnoreMentionTrigger(trigger, dismissedMentionRef.current)) {
        return
      }

      if (Date.now() - lastDismissedTimeRef.current < 200) {
        return
      }

      dismissedMentionRef.current = null

      if (mentionItems.length === 0 && !loadingMentions) {
        void loadMentionSources()
      }

      setMentionPopover({
        isOpen: true,
        query: trigger.query,
        mentionStart: trigger.mentionStart,
        selectedIndex: 0,
      })
    },
    [mentionItems.length, loadingMentions, loadMentionSources],
  )

  const debouncedCursorMentionUpdate = useMemo(
    () => debounce((text: string, pos: number) => updateMentionState(text, pos), 50),
    [updateMentionState],
  )

  const handleCloseMention = useCallback(() => {
    setMentionPopover(prev => {
      if (prev.isOpen) {
        dismissedMentionRef.current = {
          mentionStart: prev.mentionStart,
          query: prev.query,
        }
        lastDismissedTimeRef.current = Date.now()
      }
      return { ...prev, isOpen: false }
    })
  }, [])

  const handleTextareaInput = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = e.target.value
    const cursorPos = e.target.selectionStart ?? val.length
    setPrompt(val)
    e.target.style.height = 'auto'
    e.target.style.height = `${Math.min(e.target.scrollHeight, 180)}px`
    updateMentionState(val, cursorPos)
  }

  const handleMentionSelect = (item: MentionDisplayItem) => {
    dismissedMentionRef.current = null
    const { newText, newCursorPos } = applyMentionInsertion(
      prompt,
      mentionPopover.mentionStart,
      mentionPopover.query.length,
      item.insertText,
    )
    setPrompt(newText)
    setMentionPopover(prev => ({ ...prev, isOpen: false }))

    if (textareaRef.current) {
      textareaRef.current.value = newText
      textareaRef.current.focus()
      setTimeout(() => {
        if (textareaRef.current) {
          textareaRef.current.setSelectionRange(newCursorPos, newCursorPos)
          textareaRef.current.style.height = 'auto'
          textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 180)}px`
        }
      }, 0)
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (mentionPopover.isOpen && mentionPopoverRef.current) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        const displayItems = mentionPopoverRef.current.getDisplayItems()
        const maxIndex = Math.max(0, displayItems.length - 1)
        setMentionPopover(prev => ({
          ...prev,
          selectedIndex: Math.min(prev.selectedIndex + 1, maxIndex),
        }))
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setMentionPopover(prev => ({
          ...prev,
          selectedIndex: Math.max(prev.selectedIndex - 1, 0),
        }))
        return
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        const displayItems = mentionPopoverRef.current.getDisplayItems()
        if (displayItems.length > 0) {
          e.preventDefault()
          mentionPopoverRef.current.selectItem(mentionPopover.selectedIndex)
          return
        }
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        dismissedMentionRef.current = {
          mentionStart: mentionPopover.mentionStart,
          query: mentionPopover.query,
        }
        lastDismissedTimeRef.current = Date.now()
        setMentionPopover(prev => ({ ...prev, isOpen: false }))
        return
      }
    }

    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      void send(e)
    }
  }

  const [recents, setRecents] = useState<SessionSummary[]>(detail.recents?.sessions ?? [])

  useEffect(() => {
    if (detail.recents?.sessions && detail.recents.sessions.length > 0) {
      setRecents(detail.recents.sessions)
      return
    }
    const apiInstance = api ?? createApi(baseUrl, secret)
    apiInstance
      .listProjectSessions(detail.project.slug)
      .then(res => {
        if (res?.sessions) setRecents(res.sessions)
      })
      .catch(() => {})
  }, [api, baseUrl, secret, detail.project.slug, detail.recents])

  const suggestedPrompts = [
    'Explain the structure and main modules of this project',
    'Check if there are any lint or typecheck errors',
    'Review recent changes and suggest next steps',
    'Run the project tests and report results',
  ]

  return (
    <div className="panel chat-workspace-panel">
      {/* Top Session Toolbar */}
      <div className="chat-toolbar">
        <div className="chat-toolbar-left">
          <div className="chat-cwd-badge" title={detail.project.path}>
            <Folder size={12} />
            <span>{detail.project.slug}</span>
          </div>

          {session.sessionId && (
            <button
              type="button"
              className="session-id-pill"
              onClick={copySessionId}
              title={`Click to copy full Session ID: ${session.sessionId}`}
            >
              <span className="session-id-label">SID:</span>
              <code>{session.sessionId.slice(0, 8)}</code>
              {copiedSessionId ? (
                <Check size={12} className="text-success" />
              ) : (
                <Copy size={12} className="copy-icon" />
              )}
            </button>
          )}
        </div>

        <div className="chat-toolbar-actions">
          {recents.length > 0 && (
            <div className="history-picker-wrapper">
              <History size={13} className="history-icon" />
              <select
                className="history-select"
                value={session.sessionId ?? ''}
                onChange={e => {
                  if (e.target.value) {
                    void session.loadSession(e.target.value)
                  }
                }}
                title="Switch to a recent session"
              >
                <option value="" disabled>
                  History ({recents.length})…
                </option>
                {recents.map(r => (
                  <option key={r.id} value={r.id}>
                    {r.name ? `${r.name.slice(0, 24)} (${r.id.slice(0, 6)})` : r.id.slice(0, 8)}
                  </option>
                ))}
              </select>
            </div>
          )}

          <button
            type="button"
            className="secondary mini-btn new-chat-btn"
            title="Start new conversation"
            onClick={() => void session.reset(true)}
          >
            <Plus size={13} />
            <span>New Chat</span>
          </button>
        </div>
      </div>

      {/* Messages Transcript */}
      <div className="transcript-scroll-area">
        {session.messages.length === 0 && (
          <div className="empty-chat-welcome">
            <div className="welcome-icon-box">
              <Sparkles size={32} />
            </div>
            <h3>OpenDuck Agent Workspace</h3>
            <p className="muted">
              I have full access to tools and files in <code>{detail.project.path}</code>. How can I help you today?
            </p>

            <div className="suggested-prompts-grid">
              {suggestedPrompts.map((pText, i) => (
                <button
                  key={i}
                  type="button"
                  className="suggested-prompt-chip"
                  onClick={() => handleSuggestedPrompt(pText)}
                >
                  <Sparkles size={13} className="chip-sparkle" />
                  <span>{pText}</span>
                </button>
              ))}
            </div>
          </div>
        )}

        {session.messages.map(message => {
          const isUser = message.role === 'user'

          return (
            <article
              key={message.id}
              className={`message-bubble-row ${isUser ? 'user-row' : 'assistant-row'}`}
            >
              <div className="message-avatar">
                {isUser ? <User size={16} /> : <Bot size={16} />}
              </div>

              <div className="message-content-box">
                <div className="message-meta-header">
                  <span className="author-name">{isUser ? 'You' : 'OpenDuck Agent'}</span>
                  {message.streaming && (
                    <span className="streaming-badge">
                      <span className="stream-dot" /> Streaming
                    </span>
                  )}
                </div>

                {message.text && (
                  <div className="message-body-text">
                    {message.text}
                  </div>
                )}

                {/* Tool calls accordion */}
                {message.toolCalls && message.toolCalls.length > 0 && (
                  <div className="tool-calls-container">
                    <div className="tool-calls-title">
                      <Wrench size={13} />
                      <span>Executed Tools ({message.toolCalls.length})</span>
                    </div>

                    <div className="tool-calls-list">
                      {message.toolCalls.map(tool => {
                        const isExpanded = expandedTools[tool.toolCallId] ?? false
                        const isCompleted = tool.status === 'completed'
                        const isRunning = tool.status === 'in_progress' || tool.status === 'pending'
                        const isFailed = tool.status === 'failed'

                        return (
                          <div
                            key={tool.toolCallId}
                            className={`tool-call-card ${isExpanded ? 'expanded' : ''} ${tool.status}`}
                          >
                            <div
                              className="tool-call-header"
                              onClick={() => toggleToolExpand(tool.toolCallId)}
                              role="button"
                              tabIndex={0}
                            >
                              <div className="tool-header-left">
                                {isCompleted && <CheckCircle2 size={14} className="tool-status-icon success" />}
                                {isRunning && <Loader2 size={14} className="tool-status-icon running spinning" />}
                                {isFailed && <AlertCircle size={14} className="tool-status-icon failed" />}
                                {!isCompleted && !isRunning && !isFailed && <Terminal size={14} className="tool-status-icon" />}

                                <span className="tool-name">{tool.title || tool.toolCallId}</span>
                              </div>

                              <div className="tool-header-right">
                                <span className={`tool-status-badge ${tool.status}`}>
                                   {tool.status}
                                </span>
                                {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                              </div>
                            </div>

                            {isExpanded && (
                              <div className="tool-call-body">
                                <div className="tool-detail-item">
                                  <span className="tool-label">Tool ID:</span>
                                  <code>{tool.toolCallId}</code>
                                </div>
                                {tool.kind && (
                                  <div className="tool-detail-item">
                                    <span className="tool-label">Kind:</span>
                                    <span>{tool.kind}</span>
                                  </div>
                                )}
                              </div>
                            )}
                          </div>
                        )
                      })}
                    </div>
                  </div>
                )}
              </div>
            </article>
          )
        })}

        {/* Live Status Line */}
        {session.statusLine && (
          <div className="status-live-banner">
            <Loader2 size={14} className="spinning text-accent" />
            <span className="status-live-text">{session.statusLine}</span>
          </div>
        )}

        {/* Error notification */}
        {session.error && (
          <div className="chat-error-banner">
            <AlertCircle size={16} />
            <span>{session.error}</span>
          </div>
        )}

        {/* Permission Request Prompt */}
        {session.pendingPermission && (
          <div className="permission-card">
            <div className="permission-header">
              <ShieldAlert size={18} className="permission-alert-icon" />
              <div>
                <h4>Permission Required</h4>
                <p>
                  The agent requests execution permission for{' '}
                  <strong>{toolTitleFromPermission(session.pendingPermission.request)}</strong> on your host machine.
                </p>
              </div>
            </div>

            <div className="permission-actions-row">
              <button
                type="button"
                className="btn-primary mini-btn"
                onClick={() => session.resolvePermission('allow_once')}
              >
                Allow Once
              </button>
              <button
                type="button"
                className="secondary mini-btn"
                onClick={() => session.resolvePermission('always_allow')}
              >
                Always Allow for Session
              </button>
              <button
                type="button"
                className="danger mini-btn"
                onClick={() => session.resolvePermission('deny_once')}
              >
                Deny
              </button>
            </div>
          </div>
        )}

        <div ref={transcriptEndRef} />
      </div>

      {/* Chat Input Bar */}
      <form className="chat-input-wrapper" onSubmit={send}>
        <div ref={inputContainerRef} className="chat-input-container">
          <MentionPopover
            ref={mentionPopoverRef}
            isOpen={mentionPopover.isOpen}
            onClose={handleCloseMention}
            onSelect={handleMentionSelect}
            position={{ x: 0, y: 0 }}
            query={mentionPopover.query}
            selectedIndex={mentionPopover.selectedIndex}
            onSelectedIndexChange={index =>
              setMentionPopover(prev => ({ ...prev, selectedIndex: index }))
            }
            items={combinedMentionItems}
            loading={loadingMentions || isSearchingServer}
          />

          <textarea
            ref={textareaRef}
            rows={1}
            value={prompt}
            onChange={handleTextareaInput}
            onKeyUp={e => {
              if (
                e.key === 'Escape' ||
                e.key === 'ArrowUp' ||
                e.key === 'ArrowDown' ||
                e.key === 'Enter' ||
                e.key === 'Tab' ||
                e.key === 'Shift' ||
                e.key === 'Control' ||
                e.key === 'Alt' ||
                e.key === 'Meta'
              ) {
                return
              }
              const cursorPos = e.currentTarget.selectionStart ?? prompt.length
              debouncedCursorMentionUpdate(prompt, cursorPos)
            }}
            onClick={e => {
              const cursorPos = e.currentTarget.selectionStart ?? prompt.length
              debouncedCursorMentionUpdate(prompt, cursorPos)
            }}
            placeholder="Ask your agent or instruct a task (type @ to mention files, rules, agents; Enter to send)…"
            onKeyDown={handleKeyDown}
            className="chat-textarea"
          />

          <div className="chat-input-controls">
            <span className="keyboard-hint">Type @ to mention · Shift + Enter for new line</span>
            <button
              type="submit"
              className="send-button"
              disabled={!prompt.trim() || session.isPrompting}
              title="Send prompt (Enter)"
            >
              {session.isPrompting ? (
                <Loader2 size={16} className="spinning" />
              ) : (
                <Send size={16} />
              )}
            </button>
          </div>
        </div>
      </form>
    </div>
  )
}
