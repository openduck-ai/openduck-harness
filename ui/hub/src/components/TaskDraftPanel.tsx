import { useTranslation } from 'react-i18next'
import { useEffect, useRef, type KeyboardEvent, type ReactNode, type Ref } from 'react'
import type { SubtaskExpectedOutput, SubtaskSpec, TaskDraftMessage } from '@aaif/goose-hub-core'
import { blankExpectedOutput, moveSubtask } from '../taskDraft.ts'

const MENTION_CURSOR_SKIP_KEYS = new Set([
  'Escape',
  'ArrowUp',
  'ArrowDown',
  'Enter',
  'Tab',
  'Shift',
  'Control',
  'Alt',
  'Meta',
])

export function TaskDraftChat({
  messages,
  input,
  drafting,
  onInput,
  onSend,
  textareaRef,
  mentionPopover,
  mentionOpen,
  onMentionKeyDown,
  onCursorActivity,
}: {
  messages: TaskDraftMessage[]
  input: string
  drafting: boolean
  onInput: (value: string, cursor: number) => void
  onSend: () => void
  textareaRef: Ref<HTMLTextAreaElement>
  mentionPopover: ReactNode
  mentionOpen: boolean
  onMentionKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => boolean
  onCursorActivity: (value: string, cursor: number) => void
}) {
  const { t } = useTranslation()
  const logRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    const log = logRef.current
    if (!log) return
    log.scrollTop = log.scrollHeight
  }, [messages, drafting])

  return (
    <div className={`task-draft-chat${mentionOpen ? ' mention-open' : ''}`}>
      <div className="task-draft-chat-head">
        <label className="form-label">{t('harness.draft.panel.draftTitle')}</label>
        <p className="task-draft-hint">
          {t('harness.draft.panel.draftHint')}
        </p>
      </div>
      <div className="task-draft-log" ref={logRef} role="log" aria-live="polite">
        {messages.length === 0 && !drafting && (
          <p className="task-draft-empty">{t('harness.draft.panel.empty')}</p>
        )}
        {messages.map((message, index) => (
          <div
            className={`task-draft-msg ${message.role === 'assistant' ? 'is-assistant' : 'is-user'}`}
            key={`${message.role}-${index}`}
          >
            <span className="task-draft-role">{message.role === 'assistant' ? t('harness.draft.panel.harness') : t('harness.draft.panel.you')}</span>
            <p>{message.content}</p>
          </div>
        ))}
        {drafting && <p className="task-draft-empty">{t('harness.draft.panel.drafting')}</p>}
      </div>
      <div className="task-draft-compose">
        <div className="task-draft-input">
          {mentionPopover}
          <textarea
            ref={textareaRef}
            className="form-textarea"
            rows={2}
            value={input}
            aria-label={t('harness.draft.panel.messageLabel')}
            placeholder={t('harness.draft.panel.messagePlaceholder')}
            onChange={event => {
              const cursor = event.target.selectionStart ?? event.target.value.length
              onInput(event.target.value, cursor)
            }}
            onKeyUp={event => {
              if (MENTION_CURSOR_SKIP_KEYS.has(event.key)) return
              const value = event.currentTarget.value
              onCursorActivity(value, event.currentTarget.selectionStart ?? value.length)
            }}
            onClick={event => {
              const value = event.currentTarget.value
              onCursorActivity(value, event.currentTarget.selectionStart ?? value.length)
            }}
            onKeyDown={event => {
              if (onMentionKeyDown(event)) return
              if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
                event.preventDefault()
                if (!drafting && input.trim()) onSend()
              }
            }}
          />
        </div>
        <button
          type="button"
          className="btn-primary"
          onClick={onSend}
          disabled={drafting || !input.trim()}
        >
          {drafting ? t('harness.draft.panel.drafting') : t('harness.draft.panel.send')}
        </button>
      </div>
    </div>
  )
}

export function SubtaskEditor({
  subtasks,
  maxTurnsLimit,
  onChange,
  onAdd,
}: {
  subtasks: SubtaskSpec[]
  maxTurnsLimit: number
  onChange: (next: SubtaskSpec[]) => void
  onAdd: () => void
}) {
  const { t } = useTranslation()
  const update = (index: number, patch: Partial<SubtaskSpec>) => {
    onChange(subtasks.map((subtask, itemIndex) => (itemIndex === index ? { ...subtask, ...patch } : subtask)))
  }

  const updateOutput = (subtaskIndex: number, outputIndex: number, patch: Partial<SubtaskExpectedOutput>) => {
    const subtask = subtasks[subtaskIndex]
    const expectedOutputs = (subtask.expectedOutputs ?? []).map((output, itemIndex) =>
      itemIndex === outputIndex ? { ...output, ...patch } : output,
    )
    update(subtaskIndex, { expectedOutputs })
  }

  return (
    <div className="subtask-editor">
      <div className="subtask-editor-head">
        <label className="form-label">{t('harness.draft.panel.subtasks')}</label>
        <button type="button" className="btn-secondary btn-sm" onClick={onAdd}>
          {t('harness.draft.panel.addSubtask')}
        </button>
      </div>
      {subtasks.length === 0 && (
        <p className="task-draft-hint">
          {t('harness.draft.panel.noSubtasks')}
        </p>
      )}
      {subtasks.length === 1 && (
        <p className="task-draft-hint">
          {t('harness.draft.panel.oneSubtask')}
        </p>
      )}
      {subtasks.length >= 2 && (
        <p className="task-draft-hint">
          {t('harness.draft.panel.orderedHint')}
        </p>
      )}
      {subtasks.map((subtask, index) => (
        <div className="subtask-card" key={subtask.id || index}>
          <div className="subtask-card-head">
            <span className="subtask-index">{t('harness.draft.panel.subtask', { index: index + 1 })}</span>
            <div className="subtask-card-actions">
              <button
                type="button"
                className="btn-secondary btn-sm"
                onClick={() => onChange(moveSubtask(subtasks, index, -1))}
                disabled={index === 0}
              >
                {t('harness.draft.panel.up')}
              </button>
              <button
                type="button"
                className="btn-secondary btn-sm"
                onClick={() => onChange(moveSubtask(subtasks, index, 1))}
                disabled={index === subtasks.length - 1}
              >
                {t('harness.draft.panel.down')}
              </button>
              <button
                type="button"
                className="btn-secondary btn-sm"
                onClick={() => onChange(subtasks.filter((_, itemIndex) => itemIndex !== index))}
              >
                {t('harness.draft.panel.remove')}
              </button>
            </div>
          </div>
          <label className="form-label">{t('harness.draft.panel.title')}</label>
          <input
            className="form-input"
            value={subtask.title}
            onChange={event => update(index, { title: event.target.value })}
          />
          <label className="form-label">{t('harness.draft.panel.instructions')}</label>
          <textarea
            className="form-textarea"
            rows={3}
            value={subtask.description}
            onChange={event => update(index, { description: event.target.value })}
          />
          <div className="form-row-2">
            <div className="form-group">
              <label className="form-label">{t('harness.draft.panel.maxTurns')}</label>
              <input
                type="number"
                min={1}
                max={maxTurnsLimit}
                className="form-input"
                value={subtask.maxTurns || 8}
                onChange={event =>
                  update(index, {
                    maxTurns: Math.max(1, Math.min(maxTurnsLimit, Number(event.target.value) || 1)),
                  })
                }
              />
            </div>
            <div className="form-group">
              <label className="form-label">{t('harness.draft.panel.targetFiles')}</label>
              <input
                className="form-input"
                value={(subtask.targetFiles ?? []).join(', ')}
                placeholder={t('harness.draft.panel.targetFilesPlaceholder')}
                onChange={event =>
                  update(index, {
                    targetFiles: event.target.value.split(',').map(file => file.trim()),
                  })
                }
              />
            </div>
          </div>
          <div className="expected-output-list">
            <div className="subtask-editor-head">
              <label className="form-label">{t('harness.draft.panel.outputs')}</label>
              <button
                type="button"
                className="btn-secondary btn-sm"
                onClick={() =>
                  update(index, {
                    expectedOutputs: [...(subtask.expectedOutputs ?? []), blankExpectedOutput()],
                  })
                }
              >
                {t('harness.draft.panel.addOutput')}
              </button>
            </div>
            {(subtask.expectedOutputs ?? []).map((output, outputIndex) => (
              <div className="expected-output-row" key={`${subtask.id}-out-${outputIndex}`}>
                <input
                  className="form-input"
                  value={output.description}
                  placeholder={t('harness.draft.panel.outputPlaceholder')}
                  aria-label={t('harness.draft.panel.outputLabel', { index: index + 1, output: outputIndex + 1 })}
                  onChange={event => updateOutput(index, outputIndex, { description: event.target.value })}
                />
                <select
                  className="form-input"
                  value={output.kind || 'note'}
                  aria-label={t('harness.draft.panel.outputCheckLabel', { index: index + 1, output: outputIndex + 1 })}
                  onChange={event =>
                    updateOutput(index, outputIndex, {
                      kind: event.target.value,
                    })
                  }
                >
                  <option value="note">{t('harness.draft.panel.finalAnswer')}</option>
                  <option value="file">{t('harness.draft.panel.file')}</option>
                  <option value="command">{t('harness.draft.panel.command')}</option>
                </select>
                {output.kind === 'file' && (
                  <>
                    <input
                      className="form-input"
                      value={output.filePath || ''}
                      placeholder={t('harness.draft.panel.filePathPlaceholder')}
                      aria-label={t('harness.draft.panel.filePathLabel', { index: index + 1 })}
                      onChange={event => updateOutput(index, outputIndex, { filePath: event.target.value })}
                    />
                    <input
                      className="form-input"
                      value={output.expectedContent || ''}
                      placeholder={t('harness.draft.panel.fileContentPlaceholder')}
                      aria-label={t('harness.draft.panel.fileTextLabel', { index: index + 1 })}
                      onChange={event =>
                        updateOutput(index, outputIndex, { expectedContent: event.target.value })
                      }
                    />
                  </>
                )}
                {output.kind === 'command' && (
                  <>
                    <input
                      className="form-input"
                      value={output.command || ''}
                      placeholder={t('harness.draft.panel.commandPlaceholder')}
                      aria-label={t('harness.draft.panel.commandLabel', { index: index + 1 })}
                      onChange={event => updateOutput(index, outputIndex, { command: event.target.value })}
                    />
                    <input
                      className="form-input"
                      value={output.expectedStdout || ''}
                      placeholder={t('harness.draft.panel.stdoutPlaceholder')}
                      aria-label={t('harness.draft.panel.stdoutLabel', { index: index + 1 })}
                      onChange={event =>
                        updateOutput(index, outputIndex, { expectedStdout: event.target.value })
                      }
                    />
                  </>
                )}
                <button
                  type="button"
                  className="btn-secondary btn-sm"
                  onClick={() =>
                    update(index, {
                      expectedOutputs: (subtask.expectedOutputs ?? []).filter(
                        (_, itemIndex) => itemIndex !== outputIndex,
                      ),
                    })
                  }
                >
                  {t('harness.draft.panel.remove')}
                </button>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}
