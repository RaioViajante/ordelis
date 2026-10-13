# Desktop Context experience

For [issue #28](https://github.com/RaioViajante/synapse/issues/28). Frontend-only and display-only: nothing here reads a vault, history, the filesystem, a provider or the Python runtime. See [architecture](desktop-architecture.md) and [visual verification](desktop-visual-verification.md).

## Sources

All entries live in `fixtures/mock.ts` (`MOCK_CONTEXT_ENTRIES`), are invented, carry an explicit `provenance` string, and are labelled "sample" wherever they appear.

| Source | Label | Selectable | Rule |
| --- | --- | --- | --- |
| Notes | `your notes` | yes | scoped to one project |
| Private history | `private history` | yes, only while "Offer past decisions as context" is on | turning it off drops history selections, and turning it on does not restore them |
| Pending suggestions | `pending suggestion` | no | drafts are not knowledge until accepted, and accepting is not built |
| Unavailable record | n/a | no | shown with its reason, no excerpt |
| Malformed record | n/a | no | dropped by `usableEntries` (missing text, title or provenance, unknown source or project, duplicate id) |

## State

`WorkspaceProvider` owns `contextIds`, `contextEntries`, `selectable`, `historyOffered`, `toggleContext` and `clearContext`. Home, Context and the draft Preview all read these, so there is one selection. The stored ids are always resolved against what is selectable *now* (sample data on, current project, history setting), so a stale id cannot surface. A project change clears the selection, hiding sample data invalidates it through the existing revision counter, and the draft is never touched. The search text and which excerpts are open are local to the Context page and are not shared or kept. Nothing is persisted.

## Budget semantics (`app/contextBudget.ts`)

Counts are taken over the **shown excerpt**, which is what a selection would contribute:

- **Characters**: Unicode code points.
- **Bytes**: UTF-8 length.
- **Estimated tokens**: `ceil(characters / 4)`. A rule of thumb, never a tokenizer or provider measurement, and always written "est. tok" or "estimated".
- The **6,000 est. tok budget** and the **420-character excerpt limit** are illustrative constants. Nothing enforces them and no runtime limit is read.
- A cut excerpt shows `— cut here · N chars not shown —`.

## Interactions

Search ANDs whitespace-separated terms over title, location and text, case-insensitively, within the selected project. Results are grouped by source. Each row has a checkbox (notes and history), an `aria-expanded` "Excerpt / Close" toggle that opens the excerpt inline with provenance and counts, and a visible reason when it cannot be selected. Selection changes use the existing single polite `role="status"` region; the result count is a separate `aria-live="polite"` line. Empty states: sample data hidden, no project, no sample context for the project, and no results.

Draft Preview lists the selected entries with estimated counts, states "estimated, not measured", and says that previewing does not dispatch a task and does not authorize access to a vault or a provider.

## Not done

No vault, history or suggestion backend; no prompt assembly; no accepting suggestions; no persistence; no Tauri command, plugin or permission; no change to the Execution Safety Gate. The existing source scan in `Workspace.test.tsx` still guards against Tauri, network, storage and process APIs.
