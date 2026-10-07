# Agent Log UX Plan

| Field | Value |
|---|---|
| **Title** | Task Agent Log as a coding-agent transcript |
| **Status** | Proposed |
| **Surface** | Web Hub task trajectory, `viewMode === 'log'` |
| **Reference UX** | Claude Code, Grok, OpenCode |
| **Primary files** | `ui/hub/src/components/TaskTrajectoryViewer.tsx`, `ui/hub/src/taskTrajectory.ts`, `ui/hub/src/styles.css` |
| **New file** | `ui/hub/src/components/AgentLogTranscript.tsx` |

## 1. Summary

The task Agent Log should read like a normal coding-agent session: a quiet, full-height transcript of the user prompt, the assistant’s prose, and one-line tool rows. A thought that is still in progress stays open in a shimmer style, then folds to one line when that thought ends. Harness telemetry (full prompts, Laya judgments, provider calls, step index, duration) stays in the product and leaves the default reading path.

Split Inspector, Stream Timeline, and Provider Stats stay as they are. Project Chat is a separate surface and is out of this change.

## 2. Current view

`AgentLogTurnCard` in `ui/hub/src/components/TaskTrajectoryViewer.tsx` renders every harness step as a message card.

- Each turn has an avatar, an author label (`OpenDuck Agent` / `Task Prompt`), a step pill, a clock, a duration, a Laya pill, and a provider pill.
- Prompt dumps, thinking, and Laya decisions are blocks above the reply.
- Tools sit under a `Tools (N)` header. Each row shows the raw tool name, the `describeToolCall` summary, and an ok / error / running badge.
- Auto-scroll appears twice: in the page toolbar and again in the log header, next to Prompts / Thinking / Judgments toggles and an intro sentence.
- The step scrubber and Prev / Next stepper stay visible in log mode.
- The transcript is a card capped at `min(72vh, 980px)`.
- A Final Answer banner above the log repeats the last message.
- `buildAgentLog` emits only the first user message. Later user text is buried inside per-step prompt telemetry.
- A live run shows “Agent is working…”. The hub refreshes when a step is recorded, so the UI cannot stream tokens. It can name the latest action.

`describeToolCall` in `ui/hub/src/taskTrajectory.ts` already produces the one-liners this view should lead with: `Read path`, `Run cargo clippy`, `Search "…" in path`, `Ask grok: …`.

Continue already exists. `onContinueTask` and `api.runProjectTask` accept `extraPrompt`, `continueFromRunId`, and `extraTurns`. The log exposes that as a warning banner of turn-count buttons, and only for incomplete runs.

## 3. Target

Log mode is one column that fills the space under a short toolbar.

```
Agent Log | Split | Stream | Providers          [search] [filter]

You
Fix the clippy warnings in this crate.

  Thought · 1 line                         folded once that thought ended
  I will run clippy first.                 plain markdown
  ✓  Run cargo clippy                      one line; click expands
  Lint is clean.                           last answer, same prose
  [file chips extracted from that answer]

While the latest step is still only a thought:

  Thinking…
  Clippy first, then the warnings.         open; shimmer on the text

Once that step has a reply or a tool, the same block is the folded row above.

                         ↓ Latest          only after the user scrolls up

● Run cargo test                           while live
[ follow-up message………………… ]  Continue     when the run can continue
```

Reading order inside an assistant step:

1. Thinking. The current thought stays open with the shimmer in [Current thinking](#current-thinking). Every finished thought is one folded row.
2. Assistant markdown, flush on the canvas. No bubble and no author row.
3. Tool rows, collapsed. A failed tool shows the first line of the error on the row and starts expanded.
4. One **Details** disclosure when the step has prompt messages, judgments, or provider calls. Closed by default. A triggered Laya verdict is an amber dot on that row, with the point name. Passing judgments stay inside the disclosure.

User turns are a compact block: a small “You” label and a markdown body, with no avatar. Each distinct user message in the trajectory is its own turn, including a follow-up sent on continue. Tool-result messages stay paired with the tool row. System nudges stay inside Details.

In log mode, the answer is the last prose block. Mentioned-file chips sit under that block. The page-header metrics stay.

The Final Answer banner, the system-prompt inspector, the step scrubber, the Prev / Next stepper, and the “Expand complete LLM prompts” checkbox render for Split and Stream.

Auto-scroll follows the viewport. Near the bottom, new steps scroll into view. Scrolling up pauses it and shows one **Latest** button.

While `liveInspect` is on, a sticky line at the bottom names the current action from the latest step: `Thinking…`, or `describeToolCall` of the newest tool (`Run cargo test`, `Read src/lib.rs`). When the run ends, that line shows the status word already used in the header (`Success`, `Cancelled`, and the rest).

### Current thinking

One thinking block has two states. Only the latest assistant step of a live run can be current, and only while that step has thinking text and does not yet have assistant prose or a tool call.

**Show it.** The block is open. The label is `Thinking…`. The body is the thinking text, muted and smaller than the answer, in the transcript font. A slow light band sweeps across the glyphs, the same treatment Claude Code, Grok, and OpenCode use for an in-progress thought. The dashed border stays off. The sweep restyles the thinking string already stored on the step. A refresh that replaces the string keeps the sweep on the new text.

**Fold it.** When that step gains prose or a tool call, when a newer assistant step arrives, or when `liveInspect` turns off, collapse the block to `Thought · N lines` without a click. The sweep stops. Opening it later shows the same words in the quiet static style. A finished run, and a live step that already includes a reply or a tool, paint every thought folded.

**Leave the reader’s choice alone.** Collapsing the current thought stays collapsed on the next refresh. Opening an older thought stays open, and that text does not sweep.

**Motion.** Under `prefers-reduced-motion: reduce`, the current thought stays open and muted, with no sweep.

## 4. Tool rows

Add a pure helper next to `describeToolCall`:

```ts
presentToolCall(tool) -> {
  summary,    // describeToolCall
  kind,       // shell | read | write | search | list | advisor | other
  language,
  argsText,
  outputText, // omitted when empty
  diffText?,  // set when arguments or output contain a unified diff
  path?,      // read, write, and list, when arguments contain one
}
```

`kind` comes from the existing `shortToolName` helper. Shell `argsText` is the command alone. Other tools use the formatted arguments. `diffText` is set when the text contains `diff --git` or a `@@` hunk, and those lines render with `gitDiffLineClass` from `ui/hub/src/gitDiff.ts`. A full-file `write_file` body stays a truncated `<pre>`.

The row is one button: status icon, summary, chevron. The raw tool name appears inside the expanded body. Expansion shows arguments, then output or diff, each with the existing copy button. A path in the summary opens `FilePreviewModal`. Output taller than about 12 lines is clamped with a local “Show more”.

A pending tool (no output yet, and not an error) uses a spinner and no status word.

## 5. Slice 1 — transcript

1. **Presentation helper and tests** in `ui/hub/src/taskTrajectory.ts` and `ui/hub/src/taskTrajectory.test.ts`.
   - Cover `presentToolCall` for shell, `read_file`, `write_file`, `grep_search`, `consult_advisor`, an unknown MCP name, a failed result, and a unified-diff output.
   - `buildAgentLog` emits a user turn whenever an added prompt message has role `user` and its text differs from the previous user turn. The first prompt stays turn 0, so existing tests keep passing. Add a fixture where step 3’s added messages include a new user follow-up, and assert a second user turn. Tool and system added-messages stay on the assistant step’s `promptMessages` only.

2. **Extract the log UI** into `ui/hub/src/components/AgentLogTranscript.tsx`.
   - Props cover the already-filtered turns, expansion state, the live flag, the copy handler, and the file-preview callback.
   - `TaskTrajectoryViewer` keeps data loading, `buildAgentLog`, filters, auto-scroll state, and the view switcher. It renders `AgentLogTranscript` only when `viewMode === 'log'`.
   - New class names live under `.agent-transcript`: `.agent-user`, `.agent-prose`, `.agent-tool-row`, `.agent-thinking`, `.agent-details`. Leave `.message-bubble-row` and `.tool-call-card` in place so Project Chat is unchanged.
   - The log-mode toolbar is the view switcher, search, and the existing filter values (All / Tools / Answer / Stops / Telemetry) in one menu. The scrubber, stepper, jump-to-answer control, LLM-expand checkbox, intro sentence, and Prompts / Thinking / Judgments toggles are omitted in this mode.
   - The transcript uses `flex: 1; min-height: 0; overflow: auto` so it fills the viewport. Drop the `72vh` cap for this view.
   - `j` / `k` / `f` keep working in Split and Stream. In log mode those keys do nothing, so they do not move a hidden step index. Escape still clears search.

3. **Styles** in `ui/hub/src/styles.css`, scoped under `.agent-transcript`.
   - User block: subtle tint, no avatar column.
   - Prose: no card border. The final answer uses the existing lime tint as a left border.
   - Tool row: one monospace line and a hover outline. The expanded body uses the current canvas background.
   - Thinking: the current thought uses `.agent-thinking.is-current`, label `Thinking…`, body open, with the glyph sweep from [Current thinking](#current-thinking). Scope the sweep to `.agent-transcript .agent-thinking.is-current .agent-thinking-body`. A finished thought is one muted row, `Thought · N lines`. A dashed border appears only while a finished thought is open. Leave `.skeleton-shimmer` for loading placeholders.
   - Details: one muted row. A dashed border appears only while open.
   - Sticky live line at the bottom of the panel.
   - Use the light-theme tokens the trajectory view already uses. At a narrow width the filter menu and search wrap, and tool summaries truncate.

4. **Hide duplicate chrome in log mode only.**
   - Final Answer banner and the system-prompt inspector card.
   - Mentioned-file chips move under the final-answer prose, reusing `extractMentionedFiles` and `FilePreviewModal`.
   - `DynamicPromptSelectionMenu` stays attached to that final prose block.

## 6. Slice 2 — follow-up composer

After the transcript matches the reading UX, pin a composer to the bottom of the log.

- Visible when `onContinueTask` is provided, the run is continuable (`isContinuableStatus`), and `liveInspect` is off.
- One textarea. Enter sends. Shift+Enter inserts a newline. Placeholder: “Send a follow-up”.
- Send continues with the typed text as `extraPrompt` and a default of +25 turns, the same default the task list uses for Continue +25. A small turns control next to the button offers 10 / 25 / 50. “Specified turns” still opens the existing modal.
- Extend `onContinueTask` to `(taskId, extraTurns, extraPrompt?)`. `ProjectHarnessTab` already forwards `extraPrompt` into `handleRunTask`; pass the new argument through.
- Once the composer covers continue, remove the turn-count buttons from the incomplete-run banner. Keep a one-line explanation and the compacted summary as a dismissible note above the composer when `run.continuation.compactedSummary` is present.
- A running task shows the live status line in that slot. The composer returns when the run stops in a continuable status.

## 7. Out of scope

- Token streaming. The hub refreshes when a step is recorded. The current-thinking sweep restyles text already on the step.
- Restyling Split Inspector, Stream Timeline, Provider Stats, or Project Chat.
- Server or trajectory schema changes.
- Editing task files, turn-budget rules, or how continue seeds the next run.

## 8. Verification

- `cd ui/hub && pnpm test` covers `presentToolCall` and the extra user turn. Existing `buildAgentLog` assertions stay green.
- In the browser, on a finished run and a live run when one is available:
  - Agent Log is the default. A multi-step run reads top to bottom as user prompt, prose, one-line tools, and folded thinking.
  - On a live run whose latest step is thinking only, that thought is open, labeled `Thinking…`, and the text sweeps. When the step gains a reply or a tool, or the run ends, it folds to `Thought · N lines` and the sweep stops. Expanding a folded thought shows static text. Collapsing the current thought stays collapsed on the next refresh. A finished run shows every thought folded on first paint.
  - Expand a shell tool, a read or write tool, and a failed tool. Copy output. Open a file-path chip.
  - Open Details on a step that has Laya or provider calls. The row is closed on first paint.
  - Split and Stream still show the scrubber, stepper, final-answer banner, and prompt inspector.
  - Scrolling up during live inspect shows Latest. Sitting at the bottom follows new steps.
  - A follow-up from the composer shows up as a user turn on the next run.
  - At a narrow viewport the toolbar wraps, tool summaries truncate, and the composer stays usable.
- `cd ui/hub && pnpm run typecheck`. Known errors in `TaskTrajectoryViewer.tsx` on `TaskProviderStats` and `ProviderCallRecord.callId` predate this work. This change clears nothing it did not introduce, and it adds no new type errors in the files it edits.

## 9. Risks

- `TaskTrajectoryViewer.tsx` is already past 2,500 lines. Extracting the log is part of slice 1 so the new layout does not land as more branches in that file.
- Hiding the final-answer banner in log mode has to keep file preview and the dynamic-prompt selection menu reachable. Both move onto the final prose block.
- Filters still hide turns. The user prompt stays visible for the tools, answer, and stops filters, which is what `filterAgentLogTurns` does today.

## 10. Implementation order

1. `presentToolCall` and the follow-up user turn in `buildAgentLog`, with tests.
2. `AgentLogTranscript` and the log-mode toolbar, with duplicate chrome hidden in log mode.
3. Styles, then a browser pass on the transcript.
4. Composer and the `onContinueTask` signature, then a browser pass on continue.
5. Current-thinking sweep and auto-fold in `AgentLogTranscript` and `.agent-thinking`. No trajectory schema change.
