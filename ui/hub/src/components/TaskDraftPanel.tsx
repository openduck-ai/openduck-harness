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
  const logRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    const log = logRef.current
    if (!log) return
    log.scrollTop = log.scrollHeight
  }, [messages, drafting])

  return (
    <div className={`task-draft-chat${mentionOpen ? ' mention-open' : ''}`}>
      <div className="task-draft-chat-head">
        <label className="form-label">Draft with the harness</label>
        <p className="task-draft-hint">
          Describe the task. Type @ to mention files, agents, or attachments. A long run is split
          into subtasks you can edit. Each subtask runs only after the previous one succeeds and
          its expected outputs pass.
        </p>
      </div>
      <div className="task-draft-log" ref={logRef} role="log" aria-live="polite">
        {messages.length === 0 && !drafting && (
          <p className="task-draft-empty">No messages yet. Say what the agent should do.</p>
        )}
        {messages.map((message, index) => (
          <div
            className={`task-draft-msg ${message.role === 'assistant' ? 'is-assistant' : 'is-user'}`}
            key={`${message.role}-${index}`}
          >
            <span className="task-draft-role">{message.role === 'assistant' ? 'Harness' : 'You'}</span>
            <p>{message.content}</p>
          </div>
        ))}
        {drafting && <p className="task-draft-empty">Drafting…</p>}
      </div>
      <div className="task-draft-compose">
        <div className="task-draft-input">
          {mentionPopover}
          <textarea
            ref={textareaRef}
            className="form-textarea"
            rows={2}
            value={input}
            aria-label="Message the harness about this task"
            placeholder="Describe the task, or ask to split, merge, or change a subtask. Type @ to mention"
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
          {drafting ? 'Drafting…' : 'Send'}
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
        <label className="form-label">Subtasks</label>
        <button type="button" className="btn-secondary btn-sm" onClick={onAdd}>
          Add subtask
        </button>
      </div>
      {subtasks.length === 0 && (
        <p className="task-draft-hint">
          This task runs as one session. Add at least two subtasks to run a long task in order.
        </p>
      )}
      {subtasks.length === 1 && (
        <p className="task-draft-hint">
          Add another subtask to run them one by one. A single subtask still runs as one session.
        </p>
      )}
      {subtasks.length >= 2 && (
        <p className="task-draft-hint">
          These run in order in the same workspace. The next one starts only when this one
          succeeds and its expected outputs pass.
        </p>
      )}
      {subtasks.map((subtask, index) => (
        <div className="subtask-card" key={subtask.id || index}>
          <div className="subtask-card-head">
            <span className="subtask-index">Subtask {index + 1}</span>
            <div className="subtask-card-actions">
              <button
                type="button"
                className="btn-secondary btn-sm"
                onClick={() => onChange(moveSubtask(subtasks, index, -1))}
                disabled={index === 0}
              >
                Up
              </button>
              <button
                type="button"
                className="btn-secondary btn-sm"
                onClick={() => onChange(moveSubtask(subtasks, index, 1))}
                disabled={index === subtasks.length - 1}
              >
                Down
              </button>
              <button
                type="button"
                className="btn-secondary btn-sm"
                onClick={() => onChange(subtasks.filter((_, itemIndex) => itemIndex !== index))}
              >
                Remove
              </button>
            </div>
          </div>
          <label className="form-label">Title</label>
          <input
            className="form-input"
            value={subtask.title}
            onChange={event => update(index, { title: event.target.value })}
          />
          <label className="form-label">Instructions</label>
          <textarea
            className="form-textarea"
            rows={3}
            value={subtask.description}
            onChange={event => update(index, { description: event.target.value })}
          />
          <div className="form-row-2">
            <div className="form-group">
              <label className="form-label">Max turns</label>
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
              <label className="form-label">Target files</label>
              <input
                className="form-input"
                value={(subtask.targetFiles ?? []).join(', ')}
                placeholder="src/a.rs, src/b.rs"
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
              <label className="form-label">Expected outputs</label>
              <button
                type="button"
                className="btn-secondary btn-sm"
                onClick={() =>
                  update(index, {
                    expectedOutputs: [...(subtask.expectedOutputs ?? []), blankExpectedOutput()],
                  })
                }
              >
                Add output
              </button>
            </div>
            {(subtask.expectedOutputs ?? []).map((output, outputIndex) => (
              <div className="expected-output-row" key={`${subtask.id}-out-${outputIndex}`}>
                <input
                  className="form-input"
                  value={output.description}
                  placeholder="What must be true before the next subtask"
                  aria-label={`Subtask ${index + 1} output ${outputIndex + 1}`}
                  onChange={event => updateOutput(index, outputIndex, { description: event.target.value })}
                />
                <select
                  className="form-input"
                  value={output.kind || 'note'}
                  aria-label={`Subtask ${index + 1} output ${outputIndex + 1} check`}
                  onChange={event =>
                    updateOutput(index, outputIndex, {
                      kind: event.target.value,
                    })
                  }
                >
                  <option value="note">Final answer</option>
                  <option value="file">File</option>
                  <option value="command">Command</option>
                </select>
                {output.kind === 'file' && (
                  <>
                    <input
                      className="form-input"
                      value={output.filePath || ''}
                      placeholder="path/to/file"
                      aria-label={`Subtask ${index + 1} file path`}
                      onChange={event => updateOutput(index, outputIndex, { filePath: event.target.value })}
                    />
                    <input
                      className="form-input"
                      value={output.expectedContent || ''}
                      placeholder="Text the file must contain"
                      aria-label={`Subtask ${index + 1} expected file text`}
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
                      placeholder="cargo test"
                      aria-label={`Subtask ${index + 1} command`}
                      onChange={event => updateOutput(index, outputIndex, { command: event.target.value })}
                    />
                    <input
                      className="form-input"
                      value={output.expectedStdout || ''}
                      placeholder="Stdout must contain (optional)"
                      aria-label={`Subtask ${index + 1} expected stdout`}
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
                  Remove
                </button>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}
