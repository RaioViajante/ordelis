# Saved tasks (native store and commands)

**Status:** implemented and tested, and exercised in the packaged app in a disposable, offline macOS VM: the native acceptance of `7656085` and its UI follow-up, verified at `74d06df` ([#103](https://github.com/RaioViajante/ordelis/issues/103)). The Rust store and its four commands ([issue #89](https://github.com/RaioViajante/ordelis/issues/89)) are used by the Tasks interface ([issue #96](https://github.com/RaioViajante/ordelis/issues/96)) and by Home's composer, the project pages and the sidebar ([issue #98](https://github.com/RaioViajante/ordelis/issues/98)), whose tests run against a mocked native layer. The approved design is [issue #86](https://github.com/RaioViajante/ordelis/issues/86) (D3.1), and the storage protocol it reuses is the [project registry's](desktop-local-projects.md#storage).
**Scope:** what a saved task is, how it is stored and validated, the native commands, how a task stays tied to its project, and the interface that uses them.
**Related:** [Local project registration](desktop-local-projects.md) · [Desktop architecture](desktop-architecture.md#ipc-authorization) · [Orchestration](../architecture/orchestration.md)

A saved task is a note about work the user wants done: a title and a description, tied to one registered project. Saving one starts nothing. A task has no plan, stage, agent, permission, path or run in it, and its stored status is only `draft` or `archived`. Planning and execution states belong to future records owned by the execution coordinator. Schema version 1 cannot express them, so neither a bug nor an edited file can make a saved task look like running work.

## Schema version 1

```json
{
  "schema_version": 1,
  "next_id": 3,
  "tasks": [
    {
      "id": "t2",
      "project": { "id": "p1", "registered_at_ms": 1793664437000 },
      "title": "Add rate limiting to the API",
      "description": "Limit unauthenticated requests per IP.\nKeep the health check exempt.",
      "status": "draft",
      "created_at_ms": 1793700000000,
      "updated_at_ms": 1793700000000,
      "revision": 1
    }
  ]
}
```

| Field | Rule |
| --- | --- |
| `next_id` | An integer in `1..=10_000_000_000`. IDs are allocated from it and never reused while the file exists |
| `id` | `t` followed by 1 to 10 digits without a leading zero, less than `next_id`, unique |
| `project` | `{ id, registered_at_ms }`: the registry ID (`p<n>`) and that registration's time, copied when the task is created and never changed. No path, name or folder metadata is copied |
| `title` | 1 to 256 UTF-8 bytes, no control character, no leading or trailing whitespace |
| `description` | Any UTF-8 text whose only control characters are `\n` and `\t`. Title + 2 + description ≤ 4096 bytes, the task limit of `mco context plan`, so a saved task can later be planned without being cut |
| `status` | `draft` or `archived`. Anything else, including `running` or `completed`, is rejected |
| `created_at_ms`, `updated_at_ms` | Integers no greater than 2^53 − 1, with `created ≤ updated` |
| `revision` | An integer in `1..=2^53 − 1`, starting at 1 and incremented by every successful update |

Limits: at most 500 tasks and an 8 MiB file. The largest possible list (500 tasks at maximum size, every character a quote so escaping doubles it) serializes to about 4.3 MB, and a test pins that it fits. A write that would exceed either limit is `tasks_full`, and nothing is written.

Parsing follows the registry's two steps. First the version alone: a version greater than 1 is `tasks_newer_version`, and anything that is not a version-1 file is corrupt. Then the full schema with unknown fields denied at every level; serde also rejects duplicate keys, unknown statuses, invalid UTF-8 and lone surrogate escapes. Any violation makes the whole file `tasks_corrupt`: nothing is partially trusted.

## Storage

The store lives in the app data directory `D`, the one the registry uses, with its own files:

| File | Role |
| --- | --- |
| `tasks.json` | The tasks |
| `tasks.lock` | The store's lock file, separate from `projects.lock`. Created once, never written, renamed or removed |
| `tasks.json.tmp` | Temporary file of an in-progress write; never read, never promoted unless the same write created and synced it |

Reads and writes go through `src-tauri/src/store.rs`, the [registry's reviewed protocol](desktop-local-projects.md#read-protocol). Reads are no-follow, never block on a FIFO, accept only an owned regular file in a trusted directory, and are bounded. Writes take the in-process mutex and a 2 s bounded `flock`, then write an exclusive temp file, `F_FULLFSYNC`, rename atomically and sync the directory. A crash leaves the old file or the new one. The task module itself performs no file operation: `config.test.ts` gives it no `std::fs` item at all.

**The lock guard.** Every store is a marker type that implements `store::Store` (`Registry` for the registry, `TaskStore` here; exactly these two are allowed). `store::lock::<S>` returns a `store::Locked<S>`, and `store::replace` takes that guard instead of a directory: it writes only in the directory the guard locked, and only that store's file. The guard's fields are private to `store.rs`, so it cannot be built elsewhere, and it is neither `Clone` nor `Send`. Compile-fail tests (`store/tests.rs`) compile the real `store.rs` and check that each of these is a compiler error, with its specific code:

- a write under another store's guard;
- a write given a directory instead of a guard;
- a forged guard;
- a copied guard;
- a guard sent to another thread;
- a guard retyped as another store's.

A correct use compiles in the same harness.

**Corrupt or newer files are kept.** All four commands return `tasks_corrupt` or `tasks_newer_version` and write nothing: no repair, no rename, no backup, no reset. The file stays byte-identical. There is no `reset_tasks` and no `delete_task` (decisions 4 and 6 of #86). To start over, the file has to be moved aside outside Ordelis.

**Independence from the registry.** The two stores have separate lock files and separate mutexes, and no operation holds both locks. The task store only reads the registry, through the read protocol, which takes no lock. A corrupt task file does not affect projects, and an unreadable registry only makes task links `unknown`. Task files are not import-precedence signals: the I2 import's `occupied` check looks only at `projects.json` and `projects.json.corrupt-*`, and a test imports a legacy registry into a directory that holds only task files.

## Project link

When a task is created, the caller names the registration the user chose by both halves of its identity: `project_id` and `expected_registered_at_ms` ([#98](https://github.com/RaioViajante/ordelis/issues/98)). Rust reads the registry and accepts the request only if that exact pair is registered, and the task saves that pair. If the pair is not registered, the answer is `project_not_found`, whether the ID is unknown or now names a different registration after `reset_projects`. Another registration is never substituted. An unreadable registry is `project_unavailable`. A registered project whose folder is missing or changed is accepted, because a task never touches its folder.

**Between the check and the write.** The registry is read through the read protocol before the tasks lock is taken, and its own lock is never taken, so the registry can change before the task is committed. The saved identity is always the pair that was checked. Such a change can therefore only make the new task `removed` or `replaced`, never linked to the registration that took the ID. The `created` response computes the link from the registry as it is after the commit, so the change is reported rather than hidden. Holding the registry lock across the task write would close the window, but it would hold both locks, which the stores' independence (above) rules out. A test changes the registry at exactly that point.

On every read the link is computed, never stored:

| Link | When |
| --- | --- |
| `linked` | A registration has the same ID and the same registration time |
| `removed` | No registration has that ID |
| `replaced` | The ID names a registration with a different time. Only possible after `reset_projects` restarted IDs at `p1` |
| `unknown` | The registry is corrupt, newer or unavailable |

A task is never rebound. Removing a project leaves its tasks unchanged and `removed`. Re-adding the folder gives it a new ID, and a reused ID shows `replaced`. Responses carry the saved identity and the computed link, `{ id, registered_at_ms, link }`, so the interface can match a task to a registration by both halves of its identity ([#98](https://github.com/RaioViajante/ordelis/issues/98); before that, responses carried only `{ id, link }`). The registration time is a number the project list already shows, never a path, and the stored schema did not change. Two registrations made in the same millisecond under a reused ID after a reset would be indistinguishable; that is the one residual case.

## Native commands

All four are declared in the app manifest, granted to window `main` only, run their file work on the blocking pool, and answer errors as `{ code, message }` with fixed messages that never contain task text or a path.

```text
list_tasks()                                          → { schema_version: 1, tasks: TaskSummary[] }
get_task({ id })                                      → { outcome: "found", task: Task } | { outcome: "not_found" }
create_task({ project_id, expected_registered_at_ms, title, description })
                                                      → { outcome: "created", task: Task }
update_task({ id, revision, title, description, status })
                                                      → { outcome: "updated", task } | { outcome: "conflict", task } | { outcome: "not_found" }

TaskSummary = { id, title, status, project: { id, registered_at_ms, link }, created_at_ms, updated_at_ms, revision }
Task        = TaskSummary & { description }
errors      = invalid_input | project_not_found | project_unavailable | tasks_full | tasks_corrupt
              | tasks_newer_version | tasks_unavailable | unsupported_platform
```

- **Keys** are snake_case, as documented (`create_task` declares `rename_all = "snake_case"`, so `projectId` and `expectedRegisteredAtMs` are not accepted).
- **Validation.** Rust validates every input before any file is touched; the frontend is never trusted to. A missing or mistyped argument is rejected by Tauri before the command runs, with Tauri's own message, which names the command and the key. Extra keys are ignored, and a path-like title is just text.
- **`list_tasks`** leaves out descriptions. It and `get_task` never write and take no lock.
- **`create_task`** always creates a `draft` at revision 1; the status and revision are not inputs.
  - `expected_registered_at_ms` is required. It is the `registered_at_ms` that `list_projects` returned for the chosen project.
  - Tauri rejects a negative, fractional or non-numeric value before the command runs. A value above 2^53 − 1 is `invalid_input`.
  - A pair that is not registered is `project_not_found`, with the fixed message "The project is not registered, or its registration changed." Nothing is created.
- **`update_task`** replaces the title, description and status (`draft` or `archived`), but only at the current `revision`.
  - Any other revision is `conflict`, which writes nothing and returns the current task.
  - A successful update increments the revision, even when nothing changed.
  - `updated_at_ms` never goes back, even if the clock does.
  - The project identity and creation time cannot be changed.
  - Archive and restore are ordinary updates.
- **Platforms.** Other platforms answer `unsupported_platform`, like the registry.

## Threat notes

- **What a caller can do.** Any script running in `main` with the grants can list, create, edit, archive and restore tasks, as it can already remove a project. It cannot delete a task, change a task's project, write a path, or reach any file outside the three names above. A task authorizes nothing.
- **Untrusted text.** Titles and descriptions are untrusted. They are stored verbatim and must be rendered as text, never as HTML or Markdown, with bidi isolation (D3.1c). Bidi control characters are format characters, not control characters, so they are accepted.
- **Privacy.** Task text may contain anything the user typed. It is stored mode `0600` in a `0700` directory, never in the repository, browser storage or logs, and is not encrypted at rest.
- **Development origin.** In `tauri dev`, whatever serves `localhost:1420` can call these commands ([existing limitation](desktop-architecture.md#ipc-authorization)).

## Tests

`tasks/records/tests.rs`, scratch directories with synthetic registries only:

- creation, the exact stored fields, and owner-only files;
- a project folder neither read nor changed;
- every validation rule, plus duplicate keys, invalid UTF-8, lone surrogates and the size limits;
- inclusive byte limits, and the largest possible list fitting in the file;
- ID and count limits;
- project links: linked, removed, replaced after a registry reset, and unknown;
- the registry requirement;
- creation only for the registration the caller chose: an ID reused after a reset is refused, the exact pair is saved, and a registry change after the check only detaches the new task;
- updates, conflicts between independent readers, input validation, and a clock that goes back;
- corrupt and newer files kept;
- symlinked, FIFO and directory files, an untrusted or symlinked directory, a foreign owner, and the temp name never redirecting a write;
- a fault at every write step;
- threads and a second process creating with unique IDs, and writers that retry on conflict losing no update;
- the lock budget, and the independence of the two locks;
- the I2 import with only task files present.

`store/tests.rs`: the guard compile-fail cases. `lib.rs`:

- the commands from `main`, refused to other windows, remote origins and an ungranted context;
- input validation over IPC, and an unresolved data directory;
- the registry keeping working while the tasks lock is held;
- a corrupt task file reported, kept, and not affecting projects.

`config.test.ts` pins the task module's files, its store calls and its registry accessor. It also pins that the module has no file operations, folder access, recovery or deletion, the exact schema fields and the two statuses, and exactly two `Store` implementations in the crate. `frontend-boundary.test.ts` refuses any frontend module or bundle that names a task command until a reviewed adapter exists.

Not verified: the commands and the interface in the packaged app. Native acceptance runs only under the [isolated procedure](../migration/ordelis.md#i2-native-acceptance-testing) (a dedicated VM or a separate standard macOS user in its own login session), when separately authorized.

## Interface

[Issue #96](https://github.com/RaioViajante/ordelis/issues/96) (D3.1c). Routes:

- `#/tasks` lists Drafts or Archived, as lifecycle rows.
- `#/tasks/new` is the composer's Prepare step.
- `#/tasks/<id>` is the task detail.

Tasks is `⌘2` in the sidebar, and New Task opens `#/tasks/new`.

- **The adapter, `src/app/tasks.ts`.** It is the only frontend module that names a task command; `config.test.ts` pins its four call sites and `frontend-boundary.test.ts` checks the bundle's modules.
  - **What it refuses before calling Rust:** an id, revision, status or text that Rust would refuse. The text limits are the ones above, counted in UTF-8 bytes.
  - **Answers outside the contract are refused,** never partly shown: an unexpected or missing field, an execution status, an unknown link, `created > updated`, a revision below 1 or a repeated id.
  - **Native messages are never shown.** Only known error codes are kept, and the UI words each one itself.
  - **A browser preview** makes no call at all.
- **The state, `TasksProvider` (`src/app/TaskState.tsx`).**
  - The list comes from Rust and is held in memory only; nothing is written to browser storage.
  - It is read again after every create and update, conflicts included.
  - Only one change runs at a time, and a repeated submission shares the request in flight.
  - A failed list is an error state, never an empty list and never the previous list.
  - In memory only, the composer's draft (shared by Home and New task, see below) and each task's unsaved edits (with the revision they started from) survive moving around the app, but not a restart.
- **Text.** Titles and descriptions are rendered as text nodes only, with `dir="auto"` and `unicode-bidi: isolate`, and descriptions keep their line breaks (`white-space: pre-wrap`). There is no HTML or Markdown rendering and no automatic links. Task text is never logged. Limits are checked and explained before saving, never cut: a title is trimmed of surrounding spaces and nothing else.
- **Conflicts.** An edit saves with the revision it started from. On `conflict` nothing is written and the latest saved version is shown, while the user's edits stay open beside it. The user can then save them over the newer version deliberately (with its revision), or discard them. Archive and Restore get the same treatment.
- **Project links.** The registered project's name is shown while the link is `linked`; otherwise "Project removed", "Project changed" or "Project unavailable", with an explanation on the detail page. A task is never rebound, and no folder is read to work this out.
- **Storage errors.** A corrupt or newer `tasks.json` shows an error that says the file was kept unchanged, with Try again; there is no reset, repair or recovery control. Projects keep working.
- **Execution.** Draft plan is disabled with its reason on New task and on the detail page, and no execution state (planned, running, finished, failed) is ever shown for a saved task.
- **Tests.** `src/app/Tasks.test.tsx` runs against a mocked native layer: an in-memory store that applies this contract. It covers:
  - the adapter's arguments, refusals and contract checks;
  - the list, empty, loading, error and browser-preview states;
  - creating, persisting across a restart, validating, single submission and failure keeping the text;
  - the detail page, editing, conflicts, discarding edits, archiving and restoring;
  - the project link states;
  - plain-text rendering;
  - keyboard navigation;
  - the absence of execution states.

  Eight mutations of these behaviors were each caught.

## Home, project pages and the sidebar

[Issue #98](https://github.com/RaioViajante/ordelis/issues/98) (D3.1d) connects the rest of the desktop to the same saved tasks. No command, permission or stored field was added. The native changes are `registered_at_ms` in the responses and the required `expected_registered_at_ms` on `create_task` (above).

- **One composer.** Home's composer and New task (`#/tasks/new`) edit one in-memory draft held by `TasksProvider`: a project registration (`{ id, registered_at_ms }`), a title and a description.
  - `useComposer` (`src/app/useComposer.ts`) gives both pages the same checks (Rust's, in UTF-8 bytes, never cutting text), the same reasons Save task is unavailable, and the same save through the provider's single write path.
  - A second click, or a click on the other page, never reaches Rust while a save is in flight.
  - The save sends the chosen registration's `registered_at_ms`. If Rust answers `project_not_found`, the project list is read again. A stale registration is then no longer offered, and the user chooses again; the text is kept.
  - A successful save opens the new task, unless the user has already moved to another page. In that case they stay where they are, and the live region says the task was saved.
  - A successful save clears the draft only if it is still the draft that was saved. Every edit makes a new draft object, so text changed after the save began is never cleared by that save's answer.
  - A failed save keeps every character.
  - While a save is in flight, every page freezes the draft: fields are read-only, and the picker, Discard and New task in this project are unavailable. The provider also refuses to discard during a save, so a draft being written is never reported as discarded.
- **Leaving and discarding.**
  - New task's secondary link is **Back to Tasks**, which leaves the draft as it is.
  - New task keeps **Draft plan** and **Save task** together on the right; when the row is too narrow they wrap as a pair, never Save task alone. The "What happens next" aside moves below the form at 1100 px, so the four actions fit on one row at 1024 px. Native screenshots confirmed one row with Discard draft… present from 1024 px down to the 760 px minimum, in the tab order Back to Tasks, Discard draft…, Draft plan, Save task ([#103](https://github.com/RaioViajante/ordelis/issues/103)).
  - **Discard draft…**, on both pages, appears once there is text and asks for confirmation (Cancel has focus, Escape cancels).
  - The pages say the draft is shared, stays while moving around Ordelis, and is not kept when Ordelis quits.
- **Home.** The composer (Title, Description, the project picker, Context and Draft plan disabled with their reasons, **Save task**), then the five most recently updated drafts with All tasks, then Recent projects with their draft counts. Loading, empty and error states say what is true; a task file that can't be read is an error with Try again, never an empty list.
- **Ownership.** A task belongs to a registration only when the ID and `registered_at_ms` both match and Rust reported it `linked` (`belongsTo` in `tasks.ts`). A task saved for a removed registration, or for an earlier one whose ID was reused after a reset, counts under no project; it stays in Tasks with its link label. The list is read again whenever the project list changes, because each link is computed against it. No folder is read to count anything.
- **Project pages.** A registered project shows a Tasks section with its drafts and archived tasks and their counts, and **New task in this project**, which puts that registration into the shared draft (keeping any text already written) and opens New task. The project is checked again before saving: a registration that was removed, or whose ID now names another folder, is no longer selected, and Save task asks for a project. Sample projects cannot hold saved tasks; their button is disabled with that reason. In a project's task list the date sits under the title, because that column is narrow at desktop widths and the one-line row left the title no room ([#103](https://github.com/RaioViajante/ordelis/issues/103)).
- **Sidebar.** Tasks and each registered project show a quiet number of saved drafts, with an accessible ", N drafts". It is not an attention badge, and there is no number while tasks are loading or can't be read, so a failure never reads as zero. `⌘1`–`⌘7` are unchanged and there is no `⌘N`.
- **Sample draft preview retired.** Home's sample composer, its sample context picker, Context's **Preview task** and `#/preview` were removed: a sample selection or a sample project never reaches a saved task. The labelled sample task previews (`#/task/<id>`) and Context's sample selection remain.
- **Tests.** `Tasks.test.tsx` adds creating from Home and finding the task under Tasks, Home, its project and the sidebar; Enter in the title; the shared draft and its single clearing; one request across both pages and no navigation after the user moved on; limits and failures keeping the text; Back to Tasks and confirmed discard; counts that exclude archived, removed and replaced tasks and follow archive and restore; project preselection and re-checking; a reused ID never taking over a draft; a registration that changed after it was chosen being refused, with the list read again; a draft changed during a save surviving that save's answer; discard refused during a save; listing again after a project change; and corrupt storage leaving every count blank. `Workspace.test.tsx` covers the composer in a browser preview, where nothing can be saved. Ten of eleven mutations were caught. The survivor removes the composer's own in-flight check, which the provider's shared request still enforces (one `create_task`), as a provider test pins.
