# Saved tasks (native store and commands)

**Status:** foundation. The Rust store and its four commands are implemented and tested ([issue #89](https://github.com/RaioViajante/ordelis/issues/89)); nothing in the interface calls them yet. The approved design is [issue #86](https://github.com/RaioViajante/ordelis/issues/86) (D3.1), and the storage protocol it reuses is the [project registry's](desktop-local-projects.md#storage).
**Scope:** what a saved task is, how it is stored and validated, the native commands, and how a task stays tied to its project.
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

When a task is created, Rust reads the registry, finds the requested ID and copies its `registered_at_ms`. An ID that is not registered is `project_not_found`, and an unreadable registry is `project_unavailable`. A registered project whose folder is missing or changed is accepted, because a task never touches its folder.

On every read the link is computed, never stored:

| Link | When |
| --- | --- |
| `linked` | A registration has the same ID and the same registration time |
| `removed` | No registration has that ID |
| `replaced` | The ID names a registration with a different time. Only possible after `reset_projects` restarted IDs at `p1` |
| `unknown` | The registry is corrupt, newer or unavailable |

A task is never rebound. Removing a project leaves its tasks unchanged and `removed`. Re-adding the folder gives it a new ID, and a reused ID shows `replaced`. The registry's registration time travels only between Rust modules; responses carry only `{ id, link }`. Two registrations made in the same millisecond under a reused ID after a reset would be indistinguishable; that is the one residual case.

## Native commands

All four are declared in the app manifest, granted to window `main` only, run their file work on the blocking pool, and answer errors as `{ code, message }` with fixed messages that never contain task text or a path.

```text
list_tasks()                                          → { schema_version: 1, tasks: TaskSummary[] }
get_task({ id })                                      → { outcome: "found", task: Task } | { outcome: "not_found" }
create_task({ project_id, title, description })       → { outcome: "created", task: Task }
update_task({ id, revision, title, description, status })
                                                      → { outcome: "updated", task } | { outcome: "conflict", task } | { outcome: "not_found" }

TaskSummary = { id, title, status, project: { id, link }, created_at_ms, updated_at_ms, revision }
Task        = TaskSummary & { description }
errors      = invalid_input | project_not_found | project_unavailable | tasks_full | tasks_corrupt
              | tasks_newer_version | tasks_unavailable | unsupported_platform
```

- **Keys** are snake_case, as documented (`create_task` declares `rename_all = "snake_case"`, so `projectId` is not accepted).
- **Validation.** Rust validates every input before any file is touched; the frontend is never trusted to. A missing or mistyped argument is rejected by Tauri before the command runs, with Tauri's own message, which names the command and the key. Extra keys are ignored, and a path-like title is just text.
- **`list_tasks`** leaves out descriptions. It and `get_task` never write and take no lock.
- **`create_task`** always creates a `draft` at revision 1; the status and revision are not inputs.
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

Not verified: the commands in a running app. They have no caller yet; native acceptance follows with the interface (D3.1c), under the [isolated procedure](../migration/ordelis.md#i2-native-acceptance-testing).
