# Local project registration (architecture)

[Issue #45](https://github.com/RaioViajante/synapse/issues/45). **Design only; nothing here is implemented.** It proposes the first real local-project integration: the user explicitly picks a folder, Synapse remembers it, Projects lists it, and the user can forget it again. Registration is a bookmark. It grants no access to the folder's contents and authorizes nothing.

It builds on the [IPC authorization](desktop-architecture.md#ipc-authorization) and follows the pattern of the [runtime bridge](desktop-runtime-bridge.md): a closed list of argument-light commands, fixed error messages, strict parsing and Rust-owned validation. Unlike the runtime bridge, it is meant to ship in release builds, so it must not depend on development-only trust.

## Scope

| In | Out |
| --- | --- |
| A native folder picker opened by Rust after an explicit click | Typing or pasting a path; drag and drop; auto-discovery or scanning |
| Validating the chosen folder (exists, is a directory, identity) | Listing, reading, hashing or watching anything inside it |
| A small private registry file in the app data directory | Code, file names, Git data, tokens, credentials, vault data |
| Listing registrations with a revalidated status | Git, terminals, Python runtime, providers, agents, Obsidian |
| Removing a registration (the folder is untouched) | Deleting, moving or renaming anything on disk |

The Python runtime is not involved: nothing it offers is needed to pick a folder or store three fields, and the runtime bridge is development-only.

## Repository facts this design depends on

- Tauri `2.12.3`, `@tauri-apps/api` `2.12.3`, no plugins. `config.test.ts` fails if `Cargo.toml` mentions `tauri-plugin`, if a runtime dependency other than `tauri`, `serde`, `serde_json` and `libc` appears, or if any module other than `src/app/runtimeStatus.ts` imports `@tauri-apps`.
- Local-origin app commands are ACL-checked only because `build.rs` declares an app manifest; plugin commands are always checked ([details](desktop-architecture.md#ipc-authorization)).
- Projects and Home are keyed by sample project **name** (`#/projects/<name>`, `Workspace.project`), and `src/fixtures/mock.ts` is the only data source.
- macOS is the only platform built and launched. The bundle identifier is `dev.raioviajante.synapse`.
- The official `tauri-plugin-dialog` (latest stable `2.8.2`, checked on crates.io for this document) wraps `rfd` (`0.16`; `rfd` itself is at `0.17.2`). Its Rust API (`app.dialog().file().set_parent(..).blocking_pick_folder()`) is usable without granting any frontend permission. Its frontend `open` command, when granted, **adds the chosen folder to the asset-protocol scope and to the fs plugin scope** (`commands.rs`, `allow_directory`), and the crate depends on `tauri-plugin-fs`.

## Threat model

Attacker model: content inside the webview that should not be trusted (a future rendering bug, a dependency, a page loaded by mistake), a mistaken user, and ordinary filesystem churn. A malicious process running as the same user can edit any file Synapse can and is **out of scope**, as in the rest of the desktop app; the design limits what such edits can make Synapse do, not whether they happen.

Three different mechanisms are involved and none substitutes for another:

- **The OS picker establishes consent.** The path came from a dialog the user operated, not from webview text. It says nothing about whether the path is sensible, stable, or still the same folder a second later.
- **Validation establishes properties at one instant.** Canonicalization and an identity check describe the folder when they ran. They are not a time-of-check/time-of-use defense; the path can be replaced immediately afterwards.
- **Capability enforcement establishes who may call a command.** The manifest plus capability restrict which window and origin can invoke `add_project`; they say nothing about the path the command ends up handling.

| # | Threat | Mitigation | Residual |
| --- | --- | --- | --- |
| 1 | Accidental discovery of private directories | Nothing is scanned or suggested; the picker opens at the OS default. Only the chosen folder is stat'ed, never listed. `/` and the home directory itself are refused | A user can still register a sensitive folder on purpose; registration exposes only its path to the UI |
| 2 | Untrusted path strings from the frontend | No command takes a path. `add_project` takes no arguments; `remove_project` takes an ID that is looked up, never resolved | None for this slice; any future path-taking command reopens this |
| 3 | Symlinked selection | The chosen path is canonicalized (symlinks and `..` resolved) and the **canonical** path is stored and shown, so the record names the real target | The UI shows a different path than the one clicked when a link was chosen; covered in the success message |
| 4 | Directory replaced after registration | The record keeps the device and inode of the folder that was actually opened. Listing re-opens the path and reports `changed` on mismatch; it never updates the record silently | Detection only at list time; nothing acts on the folder anyway |
| 5 | TOCTOU between check and use | Canonicalize, then `open(O_DIRECTORY \| O_NOFOLLOW)` and `fstat` the descriptor, so identity describes the object opened, not a later lookup | The path may already point elsewhere when stored. Acceptable only because registration grants nothing; future content access must re-open and re-verify at use, and still cannot rule out a same-user race |
| 6 | Duplicates and aliases | Duplicate if the canonical path **or** the `(dev, ino)` pair matches an existing record (catches case variants on case-insensitive APFS, firmlinks such as `/System/Volumes/Data/...`, bind mounts). Returns the existing record | Two distinct folders sharing an inode after a volume remount could false-match; reported, not fatal |
| 7 | Parent-directory traversal | No relative path ever enters Rust. Stored paths must be absolute with no `.` or `..` components, checked again on load | — |
| 8 | Sensitive or misleading path exposure | Error messages are fixed per code and never contain a path or OS text. Paths reach the UI only in records, home abbreviated to `~`, rendered as text. Control characters are refused at registration; names are displayed with bidi isolation so a right-to-left override cannot disguise them | The path itself is shown; that is the point of the feature |
| 9 | Permissions change after registration | Revalidation maps `EACCES`/`EPERM` to `inaccessible` without retrying or prompting | macOS privacy prompts (TCC) for Desktop, Documents, Downloads and removable volumes need platform testing (see [Platform behavior](#platform-behavior)) |
| 10 | Storage corruption and concurrent updates | Read-modify-write under an in-process mutex **and** an advisory `flock` on a lock file (a second app instance), then write a temp file, `fsync`, `rename`, `fsync` the directory | `flock` is advisory; a non-cooperating writer is out of scope |
| 11 | Malicious or malformed persisted records | Strict parse: unknown keys, duplicate keys, wrong types, bad IDs, relative or overlong paths, control characters or more than 256 records reject the **whole** file; it is never partially trusted | A same-user edit that stays valid can add a record; it gains nothing beyond being listed |
| 12 | Registration mistaken for authorization | The record has no capability fields. The UI says the folder was not read and nothing can run there. The Execution Safety Gate is not imported or called | Future features must not read "registered" as "allowed"; stated in [Execution boundary](#execution-boundary) |
| 13 | Invocation from another window or origin | Each command in the manifest and granted only to window `main`, local origin, no `remote`. `add_project` also refuses if the invoking window's label is not `main` and parents the picker to it | In `tauri dev`, anything serving `localhost:1420` counts as local ([existing limitation](desktop-architecture.md#ipc-authorization)); it could list, remove, or open a picker the user must still operate |

## Recommended architecture

**Option B: Rust owns selection and registration.** The frontend calls one argument-free command; Rust opens the picker, validates, deduplicates and persists, and returns a record or a cancellation.

```text
 Projects page ── invoke("add_project") ──▶ Rust command (window must be "main")
                                              │ 1. one picker at a time (busy guard)
                                              │ 2. native folder picker, parented to the window
                                              │ 3. canonicalize → open O_DIRECTORY|O_NOFOLLOW → fstat
                                              │ 4. lock → read registry → dedupe → append → atomic write
 Projects page ◀── { outcome, project } ──────┘   (no path ever travels webview → Rust)
```

### Directory selection

- **Library.** Use `rfd` directly from Rust (recommended) rather than registering `tauri-plugin-dialog`. Both open the same native panel (`NSOpenPanel` on macOS). `rfd` adds no IPC surface at all; the plugin, even ungranted, registers `open`, `save` and `message` commands and pulls in `tauri-plugin-fs`, and its frontend path is the one that widens scopes. The plugin's advantages (official, handles main-thread dispatch) are small here; this is an [open decision](#open-decisions).
- **Threading.** The command is `async` and does not block the UI thread. `NSOpenPanel` must run on the main thread; use `rfd`'s async dialog (which dispatches there) or `run_on_main_thread`, and verify on a real build.
- **Window association.** The command receives the invoking `WebviewWindow`, refuses unless its label is `main`, and passes it as the dialog parent, so the panel is a sheet on the window that asked.
- **Cancellation** returns `{ "outcome": "cancelled" }`, not an error, and writes nothing.
- **One at a time.** A second `add_project` while a picker is open returns `selection_in_progress`, mirroring `runtime_busy`.
- **Permissions.** No Tauri capability beyond the three command grants. No entitlement or `Info.plist` change: the app is not sandboxed, and `NSOpenPanel` needs none.

### Project identity

| Field | Meaning |
| --- | --- |
| `id` | `p` followed by a decimal counter (`p1`, `p2`, ...) from `next_id` in the file. Never reused after removal, so a stale UI reference cannot hit a different folder. No UUID dependency needed |
| `path` | Canonical absolute UTF-8 path. A non-UTF-8 path is refused (`path_unsupported`) rather than lossily converted |
| `dev`, `ino` | From `fstat` on the opened directory (Unix). Used for duplicate detection and change detection only |
| `registered_at_ms` | Wall clock at registration |

The display name is derived from the final path component at render time, not stored, so it cannot drift from the path.

**Rename or move** makes the path `missing`. Synapse does not search for the folder by inode, which would be scanning; the user removes and re-adds it.

**Revalidation** happens on every `list_projects`, per record, on the blocking pool: open the stored path as above, `fstat`, compare. Status is one of `available`, `missing` (`ENOENT`/`ENOTDIR`), `inaccessible` (`EACCES`/`EPERM`), `changed` (different `dev`/`ino`, or no longer a directory, or now a symlink). Status is never written back.

### Storage

`<app_data_dir>/projects.json` (`~/Library/Application Support/dev.raioviajante.synapse/` on macOS), resolved by Rust's path resolver, never by the frontend. Directory created `0700`, file written `0600`. It is not the Python runtime's external state root and not browser storage.

```json
{
  "schema_version": 1,
  "next_id": 3,
  "projects": [
    { "id": "p2", "path": "/Users/me/code/atlas-api", "dev": 16777232, "ino": 4128803, "registered_at_ms": 1791936000000 }
  ]
}
```

- **Limits.** 256 records, 256 KiB file, 1024-byte paths. Exceeding them refuses the write (`registry_full`) rather than pruning.
- **Writes.** All mutations: take the in-process mutex, then `flock(LOCK_EX)` on `projects.lock` (via the existing `libc` dependency), re-read, modify, write `projects.json.tmp` with `O_TRUNC`, `fsync`, `rename` over `projects.json`, `fsync` the directory, release. A crash leaves the old file or the new one; a stale `.tmp` is overwritten by the next write and ignored by reads.
- **Missing file** means an empty registry. Nothing is created until the first registration.
- **Corrupt file.** Never overwritten. It is renamed to `projects.json.corrupt-<ms>`, kept, and the registry starts empty; the first `list_projects` reports `recovered: true` so the UI can say so once.
- **Newer `schema_version`** (a downgrade) is neither quarantined nor written: listing and changes fail with `registry_newer_version`.
- **Removal** deletes the record by ID and rewrites the file. Removing an unknown ID returns `not_found`. Nothing on disk outside `projects.json` is touched.

### IPC boundary

Three commands, each in the `build.rs` manifest, `generate_handler!`, `capabilities/default.json` (window `main`, no `remote`) and `config.test.ts`, with tracked `allow-`/`deny-` permission files. No filesystem, dialog or path plugin permission.

| Command | Input | Output |
| --- | --- | --- |
| `list_projects` | none | `{ schema_version: 1, recovered: bool, projects: [Project] }` |
| `add_project` | none | `{ outcome: "added", project }`, `{ outcome: "already_registered", project }` or `{ outcome: "cancelled" }` |
| `remove_project` | `{ id: string }`, must match `^p[1-9][0-9]{0,9}$`, no other key | `{ outcome: "removed" }` or `{ outcome: "not_found" }` |

`Project` on the wire is `{ id, name, path_display, status, registered_at_ms }`. `path_display` replaces the home prefix with `~`; device and inode numbers stay in Rust.

Errors are `{ code, message }` with a fixed message per code: `selection_in_progress`, `not_a_directory`, `folder_refused` (root or home), `path_unsupported`, `folder_unreadable`, `registry_full`, `registry_unavailable` (I/O or lock failure), `registry_newer_version`, `invalid_input`, `unsupported_platform`.

The frontend adapter is one new module, `src/app/projects.ts`, mirroring `runtimeStatus.ts`: fixed command names, `isTauri()` check, outputs validated before use, `invoke` never exported. `config.test.ts` then allows exactly two `@tauri-apps` importers and exactly these four command names.

### UI behavior

- **Add a folder…** is enabled only inside the desktop app. In the browser preview it stays disabled with the reason "Adding folders needs the desktop app"; no mock registry is invented.
- **States.** Loading the list shows a quiet placeholder. While the picker is open the button reads "Choose a folder…" and is disabled. Cancelled: no error, focus returns to the button. Added: the new row is selected and the live region says "Added <name>. Synapse has not read its contents." Already registered: that row is selected with a polite message. Errors: the fixed message, inline.
- **Registered and sample projects** are separate groups. "Your folders" never carries the Sample data label; sample projects keep it and still hide with the sample switch. With samples hidden and no registrations, the empty state offers Add a folder.
- **Detail for a registered project** shows name, path, status and date, plus "Not read. Git, terminals and agents are not connected to this folder." It has no Draft task here, Git, worktree or vault section in this slice. Home and Preview stay sample-only (open decision).
- **Selection and routes.** Registered projects are selected by ID in Projects-local state; no deep link yet, so sample `#/projects/<name>` routes are unchanged.
- **Status** uses the existing status pill and text, never colour alone. `changed` reads "Different folder at this path"; `missing`, `inaccessible` likewise in words.
- **Remove from Synapse** asks for confirmation that says the folder and its files stay where they are, then removes the record. Focus moves to the list.
- **Escaping.** Names and paths are React text, never HTML, inside `dir="auto"` with `unicode-bidi: isolate`.
- **Offline** behavior is identical; there is no network. Registrations persist across restarts because they live in the file, not in `Workspace`.

## Execution boundary

Registration does not authorize, enable or imply: reading files, listing or recursively walking the folder, Git commands, terminals, the Python runtime, providers, agent sessions, or Obsidian vault access. The registry module uses `open` and `fstat` on the chosen folder itself and on its own files, and nothing else; a source test forbids `read_dir`, `std::process`, `Command::` and network APIs in it. The Execution Safety Gate is unchanged and not called. Each future capability on a registered folder needs its own reviewed policy, command and tests, and must revalidate identity at the moment of use.

## Alternatives

| | A. Frontend dialog plugin + Rust validation | **B. Rust-owned selection (recommended)** | C. Manual path entry |
| --- | --- | --- | --- |
| Trust boundary | The webview receives a path and sends it back; Rust cannot tell a picked path from a forged one. Granting `dialog:allow-open` also widens the asset and fs scopes for the chosen folder | No path crosses from webview to Rust | Path is pure untrusted text |
| IPC surface | Plugin commands plus a path-taking app command | Three app commands, one argument total | A path-taking command |
| Consent | Strong only if Rust independently knows the user picked it | Strong: Rust opened the panel | Weak: typed text, easy to mislead |
| Complexity | Plugin, permissions, a pending-selection token to bind pick to register | Main-thread dispatch for the panel | Lowest code, highest validation burden |
| Portability | Good | Good (`rfd` covers macOS, Windows, Linux portals) | Best |
| Testability | Mockable in the frontend; consent link hard to test | Picker behind a small trait so tests inject outcomes | Easy |
| UX | Native | Native | Error-prone; needs path completion to be usable |

A could be made safe by having Rust issue a one-time token bound to the dialog result, but that is option B with more moving parts. C may later serve as an accessibility or headless fallback, but only through the same Rust validation and still with explicit confirmation.

## Acceptance criteria for the implementation

1. A real macOS build: Add a folder opens a native sheet on the main window; choosing a folder lists it with name, `~`-abbreviated path and `available`; it survives quit and relaunch.
2. Cancelling writes nothing and shows no error.
3. Adding the same folder through a symlink, a different case, or a firmlink path yields `already_registered` for the existing ID.
4. Renaming, deleting or replacing the folder shows `missing` or `changed` on the next list; `chmod 000` on its parent shows `inaccessible`. No record is rewritten by listing.
5. Remove from Synapse deletes the record only; the folder and its contents are byte-for-byte unchanged.
6. A corrupt `projects.json` is preserved under a new name and the UI says the list was reset; a newer schema version is reported and never overwritten.
7. The three commands are callable from `main` at the local origin and rejected for other windows, remote origins and when their grant is removed; no command accepts a path.
8. No frontend dependency added; one Rust dependency (`rfd`), justified in the PR. No plugin permission, scope or entitlement added.
9. Nothing under the chosen folder is listed or read, and no process is spawned. Source tests cover both. Natively, spawns are checked with the kqueue `NOTE_FORK` watcher from the [runtime verification](desktop-visual-verification.md#native-end-to-end-runtime-status-issue-42); file access needs a separate trace (`fs_usage` requires root), chosen in milestone 4.
10. Browser preview shows the disabled button with its reason and makes no IPC call. Existing tests pass.

## Testing strategy

| Layer | Covers |
| --- | --- |
| Rust unit (registry module, temp directories under `std::env::temp_dir()`) | Parse: valid file, every malformed shape, unknown and duplicate keys, bad IDs, relative or `..` paths, control characters, oversize file and record count, newer schema. Identity: canonical-path duplicate, `(dev, ino)` duplicate via symlink, refusal of `/` and home, non-directory, non-UTF-8 name. Revalidation: missing, replaced (remove and recreate same path), symlink swapped in, `chmod 000` (skipped as root). Persistence: atomic write leaves old or new file under injected failure before rename, stale `.tmp`, corrupt file quarantined not deleted, `next_id` never reused, removal of unknown ID |
| Rust concurrency | Many threads adding distinct folders end with all records and unique IDs; two processes (test binary re-invoking itself) contending on `flock` lose no update |
| Rust IPC (mock runtime, like the existing ACL tests) | Each command allowed for `main` at `tauri://localhost`; rejected for another window, a remote origin, with grant removed; `add_project` ignores a supplied payload; `remove_project` rejects extra keys and malformed IDs; manifest, handler, capability and permission files agree. Picker injected through a trait: cancelled, picked, busy |
| Frontend (Vitest, mocked adapter) | Loading, empty, list, added, already registered, cancelled, each error code, `recovered` notice, remove with confirmation and focus, sample versus registered grouping, bidi and control-character names rendered as text, browser preview disables the button and never calls `invoke`. `config.test.ts` pins importers and command names; a source scan forbids `read_dir`/process APIs in the registry module |
| Native end to end (manual, macOS, recorded like #42) | Real panel as a sheet, cancel, add, relaunch persistence, symlink and firmlink duplicates, rename/delete/replace, TCC-protected folder, removable volume, remove leaves files intact (tree hash before and after), no child process, no reads under the folder. Run on a **release** build too, since this feature is not development-only |

### Platform behavior

Needs real-machine testing, not mocks: `NSOpenPanel` sheet behavior and main-thread requirement; whether revalidating a folder under Desktop, Documents, Downloads or iCloud Drive triggers a TCC prompt or returns `EPERM` on later launches for a non-sandboxed app; case-insensitive APFS duplicates; firmlinks; `/tmp` → `/private/tmp`; network and removable volumes (unmounted → `missing`, inode stability across remounts). Windows (no inode; would need the volume serial and file index, and `LockFileEx`) and Linux (portal picker, bind mounts) are unsupported in the first slice and return `unsupported_platform`, matching the runtime bridge.

## Milestones

1. **Registry core.** Rust module with parsing, identity, revalidation and atomic persistence; unit and concurrency tests. No commands.
2. **Commands and authorization.** Three commands, manifest, capability, permission files, ACL tests, `rfd` behind a picker trait.
3. **Projects UI.** Adapter module, grouped list, add/remove flows, mocked tests, updated `config.test.ts`.
4. **Native verification.** Debug and release builds on macOS, the checklist above, documentation updated from "proposed" to "implemented".

Milestones 1–2 and 3–4 can be two PRs.

## Open decisions

- `rfd` directly (recommended) or the official `tauri-plugin-dialog` Rust API with no frontend grant.
- Whether to refuse more than `/` and the home directory (for example `~/Library` or other volumes' roots), or allow nested registrations (a folder inside a registered one).
- Whether registered projects can be chosen in Home's draft composer in this slice (proposed: not yet).
- Whether a corrupt registry should be quarantined automatically (proposed) or block until the user acts.
- Whether Windows and Linux support is a goal before any content-access feature.
