# Desktop Sessions experience

For [issue #30](https://github.com/RaioViajante/synapse/issues/30). Frontend-only and display-only: nothing here starts, resumes, inspects or controls a process, agent CLI, provider, worktree or credential. See [architecture](desktop-architecture.md) and [visual verification](desktop-visual-verification.md). Issue #23 (the production session-daemon readiness race) is untouched.

## Data model (`fixtures/sessions.ts`)

Five invented sessions cover `running`, `paused`, `stopped`, `finished` and `blocked`. Each record carries a `provenance` string saying it is synthetic and was not recorded from anything. Ids are made-up labels, not process or provider ids.

| Field | Meaning |
| --- | --- |
| `id`, `agent`, `taskId`, `project` | identity; `taskId` and `project` must exist in `mock.ts` or the record is dropped by `usableSessions` |
| `state`, `tag`, `reason` | lifecycle state, the short qualifier shown in lists, and why |
| `started` | invented text, always suffixed "(sample)" |
| `worktree`, `worktreeNote` | inert text; `null` when none was created. Never a path |
| `events[]` | `{ t, kind, text? }`; `t` is seconds from the example's own start. Order is derived from `t` by `orderedEvents`, so array position does not matter. A missing `text` renders "no detail recorded" |
| `result` | `available / partial / pending / none`, a summary, and what the result does not tell you |
| `handoff` | whether a context summary could be prepared, and why |
| `usage[]` | `{ label, value?, basis }`, `basis` is `measured / estimated / reported / unknown`; unknown figures carry no value |
| `resume` | `advertised / unknown`. Advertised is never shown as verified |

Even "measured" figures are invented; the page says so.

## Interaction contract

- **Routes.** `#/sessions` is the list, `#/sessions/<id>` the detail. An unknown or malformed id shows "Session not found" (the id is rendered as text, never as markup). `#/sessions/` falls back to the list. Sample data off: the list shows the empty state and any detail route shows not-found, so no synthetic session is reachable.
- **List.** Search (task, agent or id; every word must match), state filter and project filter combine with AND. A polite `aria-live` line reports "N of M sample sessions". Rows are a list of buttons (`aria-pressed`) plus a separate "Details" link, so no interactive element is nested. Filtered-empty offers "Clear filters". A selection hidden by the filters is not shown as selected and its comparison is replaced by "No session selected".
- **Selection** lives in `WorkspaceProvider` (`session`, `selectSession`), defaults to `b71c` like the reference, resolves against sample mode and the revision counter like the project does, and is set when a detail route opens, so list and detail never disagree. Project selection is deliberately not coupled: a project filter defaulting to the selected project would hide most sessions.
- **Detail.** Task and project, agent and state, the recorded timeline ("synthetic example · not live, not streaming"), result with limits, session facts, usage with basis, related links, and the continuity comparison at full width.
- **Continuity** (`components/Continuity.tsx`): Continue this session / New run with this context / Repeat with new settings, compared on CLI conversation, worktree, files, context, agent/model and authorization, as a real table with row and column headers. All three actions are `aria-disabled` with a reason and do nothing. Cells adapt to the session (a blocked session has no worktree, files or context to carry over).
- **Cross-page links** only where ids correspond: Projects worktree ids link to their session, the task preview lists its sessions, and a session links back to its task and project.

## Responsive

Rows keep five columns until 900 px, then drop Project and Worktree. The filter bar wraps. The two-column detail collapses at 1100 px. Timeline text wraps anywhere. The 900 px comparison table scrolls inside its own container. No transition or animation was added, so reduced motion needs no extra rule.

## Not done

No resume, retry, fork, repeat or restart; no Tauri command, plugin or permission; no persistence; no change to the Execution Safety Gate, the session manager or the Python runtime. The source scan in `Workspace.test.tsx` still guards against Tauri, network, storage and process APIs, and `Sessions.test.tsx` repeats it for the new files.
