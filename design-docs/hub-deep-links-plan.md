# Hub Deep Links Plan

| Field | Value |
|---|---|
| Status | In progress |
| Date | 2026-11-09 |
| Surface | Web Hub task history, Agent Log, and file preview |
| Scope | Frontend navigation using existing project APIs |

## Goal

Users can copy a link to project task history, a historical or live Agent Log, or a file preview. Opening the link in another tab and refreshing restores the destination after connection and project loading. Browser Back and Forward follow the same navigation as the visible UI.

## Current behavior

- `ui/hub/src/urlNav.ts` parses and formats project, tab, split, and top-level page query parameters.
- `ui/hub/src/App.tsx` restores those parameters and handles `popstate`.
- `ui/hub/src/components/ProjectDetail.tsx` mounts project tabs lazily.
- `ProjectHarnessTab.tsx` stores its internal section and selected log in component state. Historical detail is fetched with `getProjectHistoryDetail(projectSlug, fileName, paging)`; live inspection uses a job ID.
- `TaskTrajectoryViewer.tsx` stores its preview file path locally. `FilePreviewModal.tsx` also maintains an internal file navigation stack.
- `ProjectFiles.tsx` manages its selected file and preview mode independently of the URL.

These local states prevent the existing project links from identifying the full destination.

## URL contract

Extend the existing query format without adding a routing dependency or changing hosting requirements. Examples are relative to the Hub's current pathname.

| Destination | Query |
|---|---|
| Task history | `?project=my-project&tab=harness&view=history` |
| Historical Agent Log | `?project=my-project&tab=harness&view=log&history=run.json` |
| Live Agent Log | `?project=my-project&tab=harness&view=log&job=job-id` |
| File preview | `?project=my-project&tab=files&file=docs/report.md` |
| File preview over a historical log | `?project=my-project&tab=harness&view=log&history=run.json&file=docs/report.md` |
| File preview over a live log | `?project=my-project&tab=harness&view=log&job=job-id&file=docs/report.md` |
| Agent Log page | `?project=my-project&tab=harness&view=log&history=run.json&logPage=3&logSize=50` |
| Agent Log page size | `?project=my-project&tab=harness&view=log&job=job-id&logSize=100` |
| Agent Log, every turn | `?project=my-project&tab=harness&view=log&history=run.json&logSize=0` |

`history` is the existing history filename, not the display name or task ID. This permits direct loading without searching a paginated history list. File values are project-relative paths. Use `URLSearchParams` for encoding and decoding, including spaces, Unicode, `#`, `&`, and `+`. A slash in a file path is stored as `%2F`. The readable forms in the table still parse.

Agent Log paging uses `logPage` and `logSize`. The top-level `page` parameter remains the hub section (`running`, `settings`, or `tui-agents`) and is not the transcript page. `logPage` is 1-based. It is omitted when the log opens on its default page. `logPage=1` is kept when a live log is pinned to the first page. `logSize` is `25`, `50`, `100`, or `0` (every turn). `50` is omitted on the default page. A later page, and an explicit first page, include `logSize` so the slice does not follow a saved preference. `logSize=0` omits `logPage`. An omitted `logSize` uses the saved page size, or 50 when nothing is saved. Invalid `logPage` and `logSize` values are dropped. Opening another run clears both. A file preview keeps them. Changing page does not restart the live subscription or the full-screen log load. Live follow does not write `logPage`. Choosing Latest removes a pinned page. Refresh and Back/Forward open the page in the URL.

### Validation and normalization

- Keep existing URLs working with their current default destinations.
- Require a project for nested destinations. Require `tab=harness` for history/log views and `tab=files` for a standalone file preview.
- Model history and live log targets as mutually exclusive variants in TypeScript.
- A malformed log link containing both `history` and `job`, or neither, falls back to task history and removes the invalid log parameters with `replaceState`.
- Ignore unsupported `view` values and remove incompatible deep-link parameters when changing tabs, pages, or projects.
- Preserve the current pathname, hash, and valid split-project parameter. In the first version, nested destinations belong to the primary project pane; independent split-pane destinations are out of scope.
- Do not trim or otherwise rewrite file and history identifiers in ways that alter valid names. Reject empty identifiers and invalid path forms using the existing project path rules.
- Invalid syntax can be normalized. A syntactically valid target that fails to load stays in the URL so the user can retry or copy the failing destination.

## Navigation behavior

### Ownership and restoration

Keep the active destination in `App.tsx`, using a typed extension of `UrlNavState`. Pass the destination and navigation callbacks through `ProjectDetail.tsx` to the relevant children. Child components render and load the destination; they do not independently write browser history.

Use the same restoration path for first load and `popstate`. Retain the requested destination while connection, project lookup, and lazy component loading finish. Default component effects must not overwrite it during initialization. Opening a link performs reads and existing live subscriptions only; it never starts or continues a task.

Explicit navigation pushes one history entry. Canonicalization replaces the current entry. Data refreshes and live updates do not write history. Restore navigation without writing another entry, preventing `popstate` loops.

### Task history and Agent Log

- Selecting task history changes the URL to the history destination.
- Selecting a historical run changes it to a filename-based log destination and loads detail directly, even if the run is absent from the current history page.
- Selecting a running job changes it to a job-based log destination and uses the existing inspection and subscription lifecycle.
- Navigating away stops the previous live subscription. Scope async results to the project and target so late responses cannot overwrite the new destination.
- When a live job completes, keep the final log visible using the existing inspection behavior. `HarnessJobInspect` and the live `finished` event do not include a history filename, and `GET /projects/{slug}/harness/jobs/{jobId}` returns not found once the job leaves the active set. The live URL is therefore kept. A stream that closes may still fill the open view from the newest history row for that task, which is display-only and does not change the URL. Do not guess a history filename. Refreshing a finished job link shows the unavailable state until the job can be inspected again.
- The in-app log Back action returns to the section from which the log was opened. A directly opened log defaults to task history. Browser Back and Forward retain their normal history semantics.

### File preview

- Opening a standalone file link selects the file in `ProjectFiles.tsx` and opens its preview rather than entering edit mode by default.
- Opening a file from a log adds `file` while retaining the log target. Closing the preview removes only `file` and keeps the underlying log.
- Navigating to another file inside a preview updates the destination. Browser Back returns to the previous file. Replace the modal's independent navigation stack with controlled navigation, or derive that stack from the shared navigation state.
- A direct file link must close safely to its underlying Files tab or log without requiring a preceding browser-history entry.
- Preserve the existing unsaved-edit protection when navigation leaves a file being edited.
- Use existing path normalization and server project access checks. Links do not contain credentials or bypass authentication.

### Sharing and failures

Add a Copy link action to task history, Agent Log, and file preview. Generate an absolute URL from the current origin, deployment pathname, and canonical destination. Copy only navigation state, never connection secrets. Add English and Chinese labels and copy feedback.

Show loading, error, retry, and a route back to the containing view for missing or inaccessible projects, history records, jobs, and files. A file failure should leave the underlying log usable. Keep the requested destination through authentication and connection recovery.

## Implementation sequence

1. **Navigation types and helpers.** Extend `urlNav.ts` with typed destinations, parsing, formatting, validation, and canonicalization. Add focused cases to `urlNav.test.ts`.
2. **App and project integration.** Thread destination state and callbacks through `App.tsx` and `ProjectDetail.tsx`. Consolidate URL writes so project/tab changes preserve or clear nested state deliberately. Handle initial loading, lazy mounts, split panes, and `popstate`.
3. **History and logs.** Adapt `ProjectHarnessTab.tsx` to restore destinations and share loading logic with click navigation. Add request identity checks and live-subscription cleanup. Verify completed-job resolution against existing API behavior before deciding whether an API change is needed.
4. **File previews.** Wire controlled preview navigation through `TaskTrajectoryViewer.tsx`, `FilePreviewModal.tsx`, and `ProjectFiles.tsx`. Preserve underlying log context and unsaved-edit behavior.
5. **Share actions and localization.** Add Copy link actions, clipboard feedback, and localized loading/error text where existing messages are insufficient.
6. **Verification and documentation.** Complete the checks below, document the final URL contract, and update `openduck-self-test.yaml` with suitable feature validation under the repository contribution rules.

## Verification plan

Automated coverage should verify behavior rather than duplicate component implementation:

- Parse/format round trips for every supported destination and existing project/tab/split links.
- Reserved characters and Unicode in project identifiers, history filenames, and file paths.
- Conflicting or missing log identifiers, unsupported views, and incompatible tab/project combinations.
- Navigation transitions that retain a log beneath a file preview and clear nested state when switching projects or tabs.
- Stale detail responses cannot replace the latest project or log; navigation stops the previous live subscription.

Manual browser acceptance checks:

| Scenario | Expected result |
|---|---|
| Paste or refresh each supported link | Correct destination opens after loading |
| Open history record outside the loaded page | Detail loads without finding a table row |
| Open live log, then let the job finish | Final state remains visible; durable URL replaces it only if resolvable |
| Open file from log, follow another file link, use Back/Forward | URL and preview stay synchronized; underlying run remains selected |
| Close preview from a directly opened link | Containing Files tab or log remains usable |
| Switch projects/tabs, then use Back/Forward | No stale destination leaks into another project |
| Navigate rapidly between runs or projects | Late requests do not overwrite the active destination |
| Open missing/inaccessible target | Useful error and retry; no unrelated fallback content |
| Navigate while editing an unsaved file | Existing unsaved-edit protection is respected |
| Use a deployment subpath or split view | Path and split context are preserved; destination targets primary pane |
| Copy a link and open it in another tab | Same destination opens without secrets in the link |

Planned frontend commands, run when build/test verification is requested:

```bash
cd ui/hub
pnpm test
pnpm run typecheck
pnpm run i18n:check
```

For feature implementation, follow the repository self-test update, rebuild, and recipe validation requirements when authorized to build/test. If Rust changes become necessary, run `cargo fmt` and the applicable requested checks. This document-only change does not require those commands.

## Non-goals

- Task-history list filters, sorting, pagination, and scroll positions in shareable URLs. Agent Log turn page and page size are in the URL.
- Links to individual log messages, steps, tool calls, or file line ranges.
- Persisting log display preferences or file edits in the URL.
- Independent nested destinations for both split panes.
- Changes to agent execution, CLI, desktop, mobile, or authentication semantics.
- A new router dependency or backend endpoint unless existing APIs cannot resolve the agreed destinations.

## Completion criteria and upstream workflow

The feature is complete when all supported destinations can be copied, restored on a fresh load, and navigated with Back/Forward; file previews retain their containing context; and failures and stale requests behave as specified above.

This is a proposed local design document. Before implementation intended for an upstream PR, link the agreed design and verification plan to an issue on the Goose Issues board and confirm Status **Ready**. Record material design changes, including any required backend lookup for completed jobs, in that issue before expanding scope. The external PR must link the Ready issue and report verification actually performed.
