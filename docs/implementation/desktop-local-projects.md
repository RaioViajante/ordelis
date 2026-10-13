# Local project registration (architecture)

[Issue #45](https://github.com/RaioViajante/synapse/issues/45) (design), [issue #47](https://github.com/RaioViajante/synapse/issues/47) (implementation PR 1). **The Rust backend (milestone PR 1) is implemented; the Projects UI (PR 2) is not**, so nothing in the app calls the commands yet. See [Implementation status](#implementation-status) for what is built, verified and still open; the rest of this document is the approved design. It proposes the first real local-project integration: the user explicitly picks a folder, Synapse remembers it, Projects lists it, and the user can forget it again. Registration is a bookmark. It grants no access to the folder's contents and authorizes nothing.

It builds on the [IPC authorization](desktop-architecture.md#ipc-authorization) and follows the pattern of the [runtime bridge](desktop-runtime-bridge.md): a closed list of argument-light commands, fixed error messages, strict parsing and Rust-owned validation. Unlike the runtime bridge, it is meant to ship in release builds, so it must not depend on development-only trust.

This revision resolves the architecture review of the first draft (`8de65b6`); [Review record](#review-record) maps each finding to the section that answers it.

## Scope

| In | Out |
| --- | --- |
| A native folder picker opened by Rust | Typing or pasting a path; drag and drop; auto-discovery or scanning |
| Checking the chosen folder's metadata (exists, is a directory, identity) | Listing, reading, hashing or watching anything inside it |
| A small private registry file in the app data directory | Code, file names, Git data, tokens, credentials, vault data |
| Listing registrations with a revalidated status | Git, terminals, Python runtime, providers, agents, Obsidian |
| Removing a registration (the folder is untouched) | Deleting, moving or renaming anything outside the registry files |
| Explicit recovery from a corrupt registry file | Automatic repair, migration machinery |

The Python runtime is not involved: nothing it offers is needed to pick a folder or store a few fields, and the runtime bridge is development-only.

## Repository facts this design depends on

- Tauri `2.12.3`, `@tauri-apps/api` `2.12.3`, no plugins. `config.test.ts` fails if `Cargo.toml` mentions `tauri-plugin`, if `[dependencies]` holds anything other than `tauri`, `serde`, `serde_json` and `libc`, or if any module other than `src/app/runtimeStatus.ts` imports `@tauri-apps`. **Gap:** that dependency check reads only the `[dependencies]` table (`config.test.ts:40`); a `[target.'cfg(...)'.dependencies]` table is invisible to it today. Implementation PR 1 closes this (see [Dependencies](#dependencies)).
- Local-origin app commands are ACL-checked only because `build.rs` declares an app manifest; plugin commands are always checked ([details](desktop-architecture.md#ipc-authorization)). The existing Rust tests in `src-tauri/src/lib.rs` show the pattern every new command must follow.
- Tauri binds command arguments by key: each parameter is looked up in the JSON body by name (`tauri-2.12.3/src/ipc/command.rs`), so extra top-level keys are ignored, as `get_runtime_status` already relies on.
- Projects and Home are keyed by sample project **name** (`#/projects/<name>`, `Workspace.project`), and `src/fixtures/mock.ts` is the only data source.
- macOS is the only platform built and launched; desktop CI runs on `macos-latest` only. The bundle identifier is `dev.raioviajante.synapse` for debug and release builds alike.
- `rfd` 0.17.2 (latest stable, checked on crates.io) needs, on macOS, `objc2` 0.6, `objc2-app-kit`, `objc2-foundation` and `objc2-core-foundation` 0.3, `block2` 0.6, `dispatch2` 0.3 and `raw-window-handle` 0.6. All are already in `Cargo.lock` at compatible versions through Tauri, so on macOS `rfd` is the only new compiled crate.
- The official `tauri-plugin-dialog` 2.8.2 depends on `rfd ^0.16` and `tauri-plugin-fs ^2.7.0`. Its Rust API is usable without granting a frontend permission. Its frontend `open` command, when granted, adds the chosen folder to the asset-protocol and fs plugin scopes.

## Threat model

Attacker model: content inside the webview that should not be trusted (a future rendering bug, a dependency, a page loaded by mistake), a mistaken user, and ordinary filesystem churn (crashes, a second app instance, remounts, deleted folders). A malicious process running as the same user can edit any file Synapse can and is **out of scope**, as in the rest of the desktop app. The storage protocol still refuses symlinks, FIFOs and foreign owners so that unusual filesystem states fail closed instead of redirecting a write.

Three different mechanisms are involved and none substitutes for another:

- **The OS picker establishes consent.** The path came from a panel the user operated, not from webview text. It says nothing about whether the path is sensible, stable, or still the same folder a second later. The click on "Add a folder…" is **not** part of this: any script running in the `main` webview can call `add_project` without a click; what it cannot do is choose the folder.
- **Validation establishes properties at one instant.** Canonicalization and identity checks describe the folder when they ran. They are not a time-of-check/time-of-use defense; the path can be replaced immediately afterwards.
- **Capability enforcement establishes who may call a command.** The manifest plus capability restrict which window and origin can invoke each command; they say nothing about the path the command ends up handling. A UI confirmation dialog is not an authentication mechanism either: it protects the user from a misclick, not the command from a caller.

| # | Threat | Mitigation | Residual |
| --- | --- | --- | --- |
| 1 | Accidental discovery of private directories | Nothing is scanned or suggested; Rust never sets the panel's starting directory. Only the chosen folder's metadata is read, never its listing. Root, the home directory and its ancestors are [refused](#refusal-policy) | A user can still register a sensitive folder on purpose; registration exposes only its path to the UI. The panel may reopen where the user last browsed (OS state, see [Directory selection](#directory-selection)) |
| 2 | Untrusted path strings from the frontend | No command takes a path. `add_project` and `reset_projects` take no arguments; `remove_project` takes an ID that is looked up, never resolved | None for this slice; any future path-taking command reopens this |
| 3 | Symlinked selection | The chosen path is canonicalized and the **canonical** path is stored and shown, so the record names the real target | The UI shows a different path than the one clicked when a link was chosen; covered in the success message |
| 4 | Directory replaced, deleted or moved after registration | The record keeps the `(dev, ino)` observed at registration as a **staleness hint**. Listing reports `changed` or `missing`; nothing rebinds a record | Detection only at list time, and only as good as `(dev, ino)` stability ([Platform assumptions](#platform-and-durability-assumptions)) |
| 5 | TOCTOU between check and use | None claimed. The identity is re-checked immediately before commit to catch an observable change, which narrows but does not close the window | The stored path may point elsewhere at any later moment. Acceptable only because registration grants nothing; see [Future content access](#future-content-access) |
| 6 | Duplicates and aliases | Path equality, or an inode match **revalidated against the matching record's live path**, returns the existing record ([Duplicates](#duplicates-and-stale-records)) | Two registrations of one folder through an alias that canonicalization does not unify and whose inode cannot be revalidated (e.g. one copy stale) |
| 7 | Parent-directory traversal | No relative path ever enters Rust. Stored paths must be absolute and normalized, checked again on every load | — |
| 8 | Sensitive or misleading path exposure | Error messages are fixed per code and never contain a path, errno or OS text. Paths reach the UI only in records, home abbreviated to `~`, rendered as text. Control characters are refused; names are displayed with bidi isolation | The path itself is shown; that is the point of the feature |
| 9 | Permissions change after registration | Revalidation maps `EACCES`/`EPERM` to `inaccessible` without retrying or prompting | TCC behavior needs real-machine testing |
| 10 | Storage corruption and concurrent updates | [Write protocol](#write-protocol): in-process mutex, bounded `flock` on a stable sidecar lock file, exclusive temp file, `F_FULLFSYNC`, `rename`, directory sync | `flock` is advisory; a non-cooperating writer is out of scope. Durability is bounded by the device ([Durability](#durability)) |
| 11 | Malicious or malformed persisted records | [Read protocol](#read-protocol) and [validation](#validation): any violation makes the whole file `registry_corrupt`; it is never partially trusted, never rewritten, and never reset without an explicit `reset_projects` | A same-user edit that stays valid can add a record; it gains nothing beyond being listed |
| 12 | Registration mistaken for authorization | The record has no capability fields. The UI says the folder was not read and nothing can run there. The Execution Safety Gate is not imported or called | Future features must not read "registered" as "allowed"; see [Execution boundary](#execution-boundary) |
| 13 | Invocation from another window or origin | Each command in the manifest and granted only to window `main`, local origin, no `remote`. `add_project` also refuses unless the invoking window's label is `main` and parents the panel to it | In `tauri dev`, anything serving `localhost:1420` counts as local ([existing limitation](desktop-architecture.md#ipc-authorization)); it could list, remove, reset a corrupt registry, or open a panel the user must still operate |
| 14 | A webview script triggering the panel or a reset | The panel requires the user's choice; one panel at a time. `reset_projects` only ever acts on a file that is corrupt under the lock and preserves it, so it cannot destroy a valid registry | A script can open the panel repeatedly or surround it with misleading page text |

## Recommended architecture

**Option B: Rust owns selection and registration.** The frontend calls one argument-free command; Rust opens the panel, validates, deduplicates and persists, and returns a record or a cancellation.

```text
 Projects page ── invoke("add_project") ──▶ Rust command (async; window must be "main")
                                              │ 1. one panel at a time (busy guard)
                                              │ 2. native folder panel, sheet on the calling window
                                              │ 3. canonicalize → lstat → refusal policy
                                              │ 4. mutex → flock → read registry → dedupe
                                              │ 5. re-check identity → append → atomic write → unlock
 Projects page ◀── { outcome, project } ──────┘   (no path ever travels webview → Rust)
```

The lock is never held while the panel is open.

### Directory selection

- **Library.** `rfd` directly from Rust, not `tauri-plugin-dialog`. Both open `NSOpenPanel`. `rfd` adds no IPC surface; the plugin, even ungranted, registers commands, pulls in `tauri-plugin-fs`, and its frontend path is the one that widens scopes.
- **Call.** `rfd::AsyncFileDialog::new().set_title(<fixed>).set_can_create_directories(false).set_parent(&window).pick_folder().await`. No starting directory, no file filters, no multiple selection. The title is a Rust constant; macOS may not display a title on a sheet, so the page text next to the button carries the explanation.
- **Behavior verified in the `rfd` 0.17.2 source:** the folder panel enables "New Folder" unless told otherwise (`panel_ffi.rs`: `can_create_directories.unwrap_or(true)`), which would let a cancelled selection still leave a directory on disk; the async API with a parent uses `beginSheetModalForWindow` (`modal_future.rs`) while the synchronous API uses `runModal` (app-modal, not a sheet); off the main thread, `rfd` dispatches panel work to the main queue (`run_on_main` via `dispatch2`). The chosen path is read from `NSURL.path` as a `String`.
- **Threading.** `add_project` must be an `async` command: Tauri runs non-async commands on the main thread, where waiting on a panel would stall the UI. The window handle is obtained through Tauri's runtime (`WebviewWindow` implements `HasWindowHandle`). Sheet presentation and main-thread behavior are verified on a real build in PR 1, not inferred.
- **Window association.** The command receives the invoking `WebviewWindow`, refuses with `not_allowed` unless its label is `main`, and passes it as the parent.
- **Cancellation** returns `{ "outcome": "cancelled" }`, not an error. It does not create the app data directory, the lock file or the registry, and (with directory creation disabled) the panel cannot create a folder.
- **OS-remembered state.** `NSOpenPanel` may remember the last visited directory and view settings in the app's preferences. That is AppKit state outside Synapse's control; it contains no registration and is not read by Synapse.
- **One at a time.** A second `add_project` while a panel is open returns `selection_in_progress`, mirroring `runtime_busy`. The guard is an RAII flag released on every exit, including when the command's future is dropped.
- **Abstraction.** The panel sits behind a small picker trait so Rust tests inject `cancelled`, `picked(path)` and slow outcomes; production uses `rfd`. There is no generic path-selection command and the trait is not reachable over IPC.
- **Permissions.** No Tauri capability beyond the four command grants. No entitlement or `Info.plist` change: the app is not sandboxed and `NSOpenPanel` needs none.

### Refusal policy

After canonicalization the folder is refused with `folder_refused` if its `(dev, ino)` (from `lstat`) equals that of any of:

- `/`;
- `/System/Volumes/Data` if it exists (the data volume root, reachable as an alias of `/` for user data);
- the home directory, taken from `getpwuid_r(geteuid())` rather than `$HOME`, then canonicalized;
- every ancestor of the canonical home directory (on a default layout, `/Users`).

Identity, not string equality, is compared because firmlinks give one directory two canonical spellings: on the development Mac, `/Users` and `/System/Volumes/Data/Users` canonicalize to different strings but share `(dev, ino)`, and so do the home directory and `/System/Volumes/Data/Users/<name>`. `/` and `/System/Volumes/Data` were observed to share a `dev` with different inodes, so both are listed. If the home directory cannot be resolved, the check fails closed with `folder_refused`.

Not refused: other volume roots (`/Volumes/<name>`), `~/Library`, and any folder inside a registered one. **Nested registrations are allowed**: registration grants nothing, so overlap has no effect in this slice. This policy only prevents registrations that are obviously broader than a project; it is not a content-access policy, and a future access feature defines its own.

Limitations: network home directories and unusual layouts are not specially handled; the ancestor walk follows the canonical home path, so a home reachable through another alias is still covered by identity, but an alias outside that walk is not.

### Project identity

A registration has two unrelated identities:

- **Registration ID** (`p1`, `p2`, ...): Synapse's name for the record. Allocated from `next_id`, never reused while the file exists, so a stale UI reference cannot reach a different record. It says nothing about the filesystem.
- **Filesystem identity**: the canonical path plus the `(dev, ino)` observed at registration. The `(dev, ino)` pair is a **staleness hint**, not a permanent proof: `dev` can change when a volume is remounted, and inode numbers can be reused after deletion on some filesystems.

**Registration check**, on the blocking pool after the panel returns:

1. `canonicalize` the panel's path. A non-UTF-8 result is `path_unsupported` (defensive; `rfd` returns a `String` on macOS).
2. `lstat` the canonical path: `ENOENT`/`ENOTDIR` → `folder_changed` (it vanished after selection), `EACCES`/`EPERM` → `folder_inaccessible`, not a directory → `not_a_directory`.
3. Apply the [refusal policy](#refusal-policy).
4. Under the registry lock, [deduplicate](#duplicates-and-stale-records).
5. Immediately before writing, `lstat` the canonical path again; if `(dev, ino)` or the directory type changed, abort with `folder_changed` and write nothing.

Why not `open(O_DIRECTORY | O_NOFOLLOW)` plus `fstat`, as the first draft proposed: the stored path still comes from the earlier lookup, so describing "the object opened" does not bind path and identity; `O_NOFOLLOW` covers only the last component; and opening needs read permission, which would misreport an execute-only folder and may provoke privacy prompts during revalidation. `lstat` is std-only and reads only metadata. Neither approach removes TOCTOU (threat 5); step 5 only catches a change that happens to be visible.

**Revalidation** happens on every `list_projects`, per record, on the blocking pool: `lstat` the stored path.

| Result | Status |
| --- | --- |
| Directory, not a symlink, same `(dev, ino)` | `available` |
| `ENOENT` or `ENOTDIR` | `missing` |
| `EACCES` or `EPERM` | `inaccessible` |
| A symlink, not a directory, or different `(dev, ino)` | `changed` |
| Any other error (e.g. a network volume timing out) | `inaccessible` |

Status is computed, never written back. Listing never rewrites the registry.

**Rename or move** makes the path `missing`. Synapse does not search for the folder by inode, which would be scanning; the user removes and re-adds it.

### Duplicates and stale records

`add_project`, holding the lock, compares the candidate (canonical path `C`, identity `I`) with each record:

1. **Path match.** A record whose stored path equals `C` byte for byte is the existing registration: return `already_registered` with that record and its live status, even if the status is `missing` or `changed`.
2. **Revalidated inode match.** A record whose stored `(dev, ino)` equals `I` is a duplicate only if `lstat` of **that record's stored path** returns `I` right now. This catches aliases (firmlink spellings, other mount paths) without trusting a stale record whose inode was reused by a different folder. Only records with a matching stored pair are stat'ed.
3. Otherwise append a new record.

Rules that follow:

- **No rebinding.** A stored `(dev, ino)` or path is never updated in place. A folder deleted and recreated at the same path is `changed`; adding it again returns the old record as `already_registered` with status `changed`, and the user must Remove and Add to register the new folder. Recreating a directory never silently inherits a registration.
- Two records may legitimately hold the same stored `(dev, ino)` (a stale record and a later folder that reuses its inode), so the validator does not reject that.
- On the development Mac, `canonicalize` returned the on-disk case for a differently-cased input on APFS; rule 1 relies on that only as an optimization, and rule 2 covers what it misses. Canonical paths are not assumed to be globally unique on every filesystem.

### Future content access

Not part of this slice, but binding on any feature that later reads or acts inside a registered folder:

- It needs its own authorization model, reviewed policy, IPC contract and tests. Registration is never an input to "allowed".
- At the moment of use it opens the root once with `O_DIRECTORY | O_NOFOLLOW`, compares the descriptor's `(dev, ino)` with the record and refuses on mismatch, then works only relative to that descriptor (`openat` with `O_NOFOLLOW` on each component, or `O_NOFOLLOW_ANY` on macOS 11+). It never re-joins path strings and never relies on the stored path or inode alone.
- Policy is never inherited from an enclosing registration's path prefix (nested registrations are independent).
- Sandboxing the app would make stored paths unusable across launches; that would need security-scoped bookmarks, which neither `rfd` nor the dialog plugin provides. Deferred.

### Storage

All registry files live in `D = <app_data_dir>` (`~/Library/Application Support/dev.raioviajante.synapse/` on macOS), resolved by Rust's path resolver, never by the frontend. It is not the Python runtime's external state root and not browser storage. Registry functions take `D` as a parameter; only the commands resolve it, so tests never touch the real directory.

| File | Role |
| --- | --- |
| `projects.json` | The registry |
| `projects.lock` | Stable sidecar lock file. Created once, never written, renamed or removed |
| `projects.json.tmp` | Temporary file of an in-progress write; never read, and never promoted unless the same write created and synced it |
| `projects.json.corrupt-<ms>[-<n>]` | A corrupt registry preserved by `reset_projects`; never read, renamed or removed by Synapse |

```json
{
  "schema_version": 1,
  "next_id": 3,
  "projects": [
    { "id": "p2", "path": "/Users/me/code/atlas-api", "dev": 16777232, "ino": 4128803, "registered_at_ms": 1791936000000 }
  ]
}
```

#### Validation

Parsing is two-step with `serde` and `serde_json` only:

1. A probe struct with just `schema_version: u64` (unknown keys ignored, duplicate key rejected by serde). Not JSON, missing, non-integer or `0` → `corrupt`. Greater than `1` → `newer`.
2. For version `1`, the full structs with `#[serde(deny_unknown_fields)]` at every level; serde's derive also rejects duplicate fields (both behaviors checked against `serde_json` 1 on the development Mac). Then semantic checks; any failure → `corrupt`:
   - `next_id` is an integer in `1..=10_000_000_000`.
   - At most 256 records.
   - Each `id` matches `^p[1-9][0-9]{0,9}$`, its number is less than `next_id`, and IDs are unique.
   - Each `path` is at most 1024 bytes, contains no control character, starts with `/`, is not `/`, and equals its own normalized form (only a root followed by normal components: no `.`, `..`, empty component or trailing `/`). Paths are unique.
   - `dev` and `ino` are unsigned 64-bit integers (`serde_json` handles the full `u64` range). Duplicate `(dev, ino)` pairs are allowed (see above).
   - `registered_at_ms` is an integer no greater than `2^53 - 1`, so it survives JavaScript.

When `next_id` reaches `10_000_000_000`, adding returns `registry_full`. There is no migration machinery: a future version 2 adds an explicit version-1 reader when it exists.

#### Read protocol

Used by every command; it never creates, writes, renames, truncates or changes permissions of anything.

1. `lstat D`. `ENOENT` → **empty**. Not a directory, a symlink, owned by another user, or writable by group or others → **unavailable**.
2. `open(D/projects.json, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)`. `ENOENT` → **empty**. `ELOOP` (a symlink), `EACCES`, `EPERM` or any other error → **unavailable**. `O_NONBLOCK` makes opening a FIFO return immediately (checked on the development Mac) instead of waiting for a writer.
3. `fstat` the descriptor: not a regular file or not owned by `geteuid()` → **unavailable**.
4. Read through `take(256 KiB + 1)`. More than 256 KiB → **corrupt**. A read error → **unavailable**.
5. [Validate](#validation) → **valid**, **corrupt** or **newer**.

Outcomes map to errors as: **unavailable** → `registry_unavailable`, **corrupt** → `registry_corrupt`, **newer** → `registry_newer_version`. I/O and permission failures are never classified as corruption.

#### Write protocol

`add_project` and `remove_project` (and `reset_projects`, below) mutate only through this sequence:

1. Take the process-wide `Mutex`. `flock` alone would also exclude a second descriptor in the same process (checked on the development Mac: a second open of the same file gets `EWOULDBLOCK`); the mutex serializes this process's threads without spending the lock budget below.
2. `lstat D`. If missing, create it with `DirBuilder::mode(0o700)` (not recursive; the parent must exist). Then apply the same directory checks as the read protocol → `registry_unavailable` on failure. Existing permissions are never changed.
3. `open(D/projects.lock, O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0o600)`; `fstat`: regular file owned by `geteuid()`, else `registry_unavailable`.
4. `flock(LOCK_EX | LOCK_NB)`, retried every 50 ms for at most 2 s, then `registry_unavailable`. The lock is released by closing the descriptor, including when the process dies, so there is no stale-lock state. The lock is never held while the panel is open.
5. Read the registry with the [read protocol](#read-protocol). **corrupt**, **newer** or **unavailable** → the matching error, nothing written. **empty** → `{ schema_version: 1, next_id: 1, projects: [] }`.
6. Apply the change. If nothing changes (`already_registered`, `not_found`), release and return without writing.
7. Serialize; over 256 records or 256 KiB → `registry_full`.
8. `unlink(D/projects.json.tmp)`, ignoring `ENOENT` (unlink removes a symlink itself, never its target; a directory at that name fails). Any other unlink error → `registry_unavailable`, nothing written. Then `open(D/projects.json.tmp, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)`; `O_EXCL` fails on any existing name, including a symlink (checked: `EEXIST`). Failure → `registry_unavailable`.
9. `write_all`, then `File::sync_all` (on Apple targets Rust's std issues `fcntl(F_FULLFSYNC)`), close. Failure → unlink the temp file, `registry_unavailable`, old registry intact.
10. `rename(projects.json.tmp, projects.json)`, renaming only the temp file this call created and synced. Failure → unlink the temp file, `registry_unavailable`, old registry intact. **Success is the commit point**: every process now sees the new registry.
11. Open `D` and `sync_all` it. `EINVAL` or `ENOTSUP` are ignored. Any other failure returns `registry_unavailable` even though the change is committed; the frontend always re-lists after a mutation error rather than assuming nothing changed.
12. Close the lock descriptor, release the mutex.

Every file name in steps 3–11 is inside `D` as validated in step 2 (a same-user swap of `D` itself is out of scope, as in the [threat model](#threat-model)). Step 12 is the only place the lock is released: every early return and every cleanup from step 4 on happens while it is held. If a cleanup unlink in step 9 or 10 itself fails, the result is still `registry_unavailable` with the old registry intact; the leftover temp file is never read or promoted, and the next write's step 8 removes it.

A crash before step 10 leaves the old registry and possibly a stale temp file (removed by the next write, ignored by reads). A crash after step 10 leaves the new registry visible.

#### Durability

Atomic rename guarantees that readers see the old or the new file, never a mix. It does not by itself guarantee that the new file survives power loss. Steps 9 and 11 ask the OS to flush the file and the directory; `F_FULLFSYNC` asks the drive to flush its cache, which plain `fsync` on macOS does not. Whether the device honors it is outside Synapse's control, and on some filesystems (network, FAT/exFAT) the directory sync may be unsupported or weaker. The design promises crash consistency (old or new), and best-effort durability.

#### Corrupt and newer registries

- **Corrupt:** preserved byte for byte. `list_projects`, `add_project` and `remove_project` return `registry_corrupt` and write nothing. The UI says the project list could not be read, that the file was kept unchanged, and offers "Reset project list…".
- **Newer schema** (a downgrade, or a newer development build sharing the directory): preserved and never reset. Every command returns `registry_newer_version`, including `reset_projects`.

#### Recovery: `reset_projects`

Argument-free; offered by the UI only after `registry_corrupt`, behind a confirmation that says the damaged file is kept and the list will start empty.

1. Steps 1–4 of the [write protocol](#write-protocol), except that a missing `D` returns `{ "outcome": "not_needed" }` instead of creating it.
2. Read the registry under the lock. The decision is taken from the file as it is now, never from an in-memory flag or from an earlier `list_projects`:
   - **valid** or **empty** → `{ "outcome": "not_needed" }`, nothing touched (another instance may have reset it, or the file was repaired).
   - **newer** → `registry_newer_version`; **unavailable** → `registry_unavailable`. Nothing touched.
   - **corrupt** → continue, remembering the `(dev, ino)` of the descriptor the read protocol validated.
3. Preserve: `hard_link(D/projects.json, D/projects.json.corrupt-<ms>)`, both names inside the `D` validated in step 1. `link` never overwrites (checked: `EEXIST`); on `EEXIST` retry with suffixes `-1` … `-15`, then `registry_unavailable`. Any other link error (including a filesystem without hard links; there is no copy fallback) → `registry_unavailable`, nothing changed.
4. Verify: `lstat` both names; each must be a regular file with the `(dev, ino)` remembered in step 2, so the backup is the very file that was found corrupt. Then sync `D` (`EINVAL` or `ENOTSUP` ignored). A mismatch or another sync error → `registry_unavailable`, and `projects.json` is **not** removed.
5. `unlink(D/projects.json)`. Failure → `registry_unavailable`; both names remain and the registry is still corrupt. **Success is the commit point.**
6. Sync `D` (`EINVAL` or `ENOTSUP` ignored). Another error → `registry_unavailable` although the reset is committed; as after any mutation error, the frontend re-lists. Otherwise return `{ "outcome": "reset" }`.

The empty registry is the absence of `projects.json`: nothing is written until the next add, which creates it through the [write protocol](#write-protocol). The backup is never read, renamed or removed by Synapse, so no later step can discard it. Any failure before step 5 leaves `projects.json` in place, still corrupt, so commands keep returning `registry_corrupt` and a retry makes another preserved copy; a crash anywhere behaves the same way or leaves the committed reset. After a reset, IDs restart at `p1`; the frontend discards all cached selections and re-lists. Because step 2 refuses anything but a corrupt file, and steps 3–5 only add a name and remove one name of that same file, `reset_projects` never modifies a valid or newer-schema registry, whoever calls it.

A UI confirmation is not authentication: a script running in `main` with the grant can call `reset_projects` without it. The capability boundary governs who can call; step 2 bounds what any caller can do. The step 4 identity check confirms the link under the lock against cooperating writers; like every check here, it is not a defense against a same-user process racing the filesystem.

### IPC boundary

Four commands, each in the `build.rs` manifest, `generate_handler!`, `capabilities/default.json` (window `main`, no `remote`) and `config.test.ts`, with tracked `allow-`/`deny-` permission files. No filesystem, shell, dialog, opener or path plugin permission.

| Command | Input | Success output |
| --- | --- | --- |
| `list_projects` | none | `{ schema_version: 1, projects: [Project] }` |
| `add_project` | none | `{ outcome: "added", project }`, `{ outcome: "already_registered", project }` or `{ outcome: "cancelled" }` |
| `remove_project` | `id: string` matching `^p[1-9][0-9]{0,9}$` | `{ outcome: "removed" }` or `{ outcome: "not_found" }` |
| `reset_projects` | none | `{ outcome: "reset" }` or `{ outcome: "not_needed" }` |

`Project` is `{ id, name, path_display, status, registered_at_ms }`. `name` is the final path component, derived at response time and not stored. `path_display` replaces the home prefix with `~` only at a component boundary (`/Users/me2` is not `~2`). `status` is `available`, `missing`, `inaccessible` or `changed`. Device and inode numbers stay in Rust.

**Input handling.** Commands without parameters ignore any payload. For `remove_project`, extra top-level keys are ignored by Tauri's key-based binding; no custom parsing is added to reject them. A missing or non-string `id` is rejected by Tauri's binding before the command runs (its message names only the command and key); a string not matching the pattern returns `invalid_input`. Either way the registry is not touched.

**Errors** are `{ code, message }` with a fixed message per code and never a path, errno or OS text:

| Code | Commands |
| --- | --- |
| `not_allowed` (calling window is not `main`) | `add_project` |
| `selection_in_progress` | `add_project` |
| `not_a_directory`, `folder_refused`, `folder_changed`, `folder_inaccessible`, `path_unsupported` | `add_project` |
| `registry_full` | `add_project` |
| `invalid_input` | `remove_project` |
| `registry_corrupt` | `list_projects`, `add_project`, `remove_project` |
| `registry_newer_version`, `registry_unavailable` | all |
| `unsupported_platform` | all, on any target other than macOS |

The commands are registered on every target so the manifest, handler and capability agree everywhere; the registry and picker code is `cfg(target_os = "macos")`, and other targets return `unsupported_platform`.

The frontend adapter is one new module, `src/app/projects.ts`, mirroring `runtimeStatus.ts`: fixed command names, `isTauri()` check, outputs validated before use, `invoke` never exported. It never sends a path.

### Dependencies

- `rfd` under `[target.'cfg(target_os = "macos")'.dependencies]` with `default-features = false`. The default features (`xdg-portal`, `wayland`) are Linux-only; macOS needs none.
- `config.test.ts` must enumerate **every** Cargo table whose header ends in `dependencies]` (`[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`, and any `[target.'...'.dependencies]`) and pin each table's exact names, plus `rfd`'s exact specification. The current check would not see a target table at all.
- No frontend dependency.

### UI behavior

- **Add a folder…** is enabled only inside the desktop app. In the browser preview it stays disabled with the reason "Adding folders needs the desktop app" and no IPC call is made; no mock registry is invented.
- **States.** Loading the list shows a quiet placeholder. While the panel is open the button reads "Choose a folder…" and is disabled. Cancelled: no error, focus returns to the button. Added: the new row is selected and the live region says "Added <name>. Synapse has not read its contents." Already registered: that row is selected with a polite message (and its status, if stale). Errors: the fixed message, inline. After any mutation error the list is reloaded.
- **Corrupt registry.** The list area shows the fixed message, that the file was kept, and "Reset project list…" with a confirmation. Add and Remove are disabled until a list succeeds. A newer-version registry shows its message and no reset.
- **Registered and sample projects** are separate groups. "Your folders" never carries the Sample data label; sample projects keep it and still hide with the sample switch. With samples hidden and no registrations, the empty state offers Add a folder.
- **Detail for a registered project** shows name, path, status and date, plus "Not read. Git, terminals and agents are not connected to this folder." It has no Draft task here, Git, worktree or vault section in this slice. Home and Preview stay sample-only.
- **Selection and routes.** Registered projects are selected by ID in Projects-local state; no deep link yet, so sample `#/projects/<name>` routes are unchanged.
- **Status** uses the existing status pill and text, never colour alone. `changed` reads "Different folder at this path"; `missing` and `inaccessible` likewise in words.
- **Remove from Synapse** asks for confirmation that says the folder and its files stay where they are, then removes the record. Focus moves to the list.
- **Escaping.** Names and paths are React text, never HTML, inside `dir="auto"` with `unicode-bidi: isolate`.
- **Offline** behavior is identical; there is no network. Registrations persist across restarts because they live in the file, not in `Workspace`.

## Execution boundary

Registration does not authorize, enable or imply: reading files, listing or recursively walking the folder, Git commands, terminals, the Python runtime, providers, agent sessions, or Obsidian vault access. No read or execution permission may be inferred from a record. The registry module calls only `canonicalize`, `lstat` and the identity lookups of the refusal policy on chosen and stored folders, and file operations on its own files in `D`; a source test forbids `read_dir`, `File::open`/`OpenOptions` outside the registry-file functions, `std::process`, `Command::` and network APIs in it. The Execution Safety Gate is unchanged and not called. Each future capability on a registered folder follows [Future content access](#future-content-access).

## Alternatives

| | A. Frontend dialog plugin + Rust validation | **B. Rust-owned selection (recommended)** | C. Manual path entry |
| --- | --- | --- | --- |
| Trust boundary | The webview receives a path and sends it back; Rust cannot tell a picked path from a forged one. Granting `dialog:allow-open` also widens the asset and fs scopes for the chosen folder | No path crosses from webview to Rust | Path is pure untrusted text |
| IPC surface | Plugin commands plus a path-taking app command | Four app commands, one argument total | A path-taking command |
| Consent | Strong only if Rust independently knows the user picked it | Strong: Rust opened the panel | Weak: typed text, easy to mislead |
| Complexity | Plugin, permissions, a pending-selection token to bind pick to register | Main-thread dispatch for the panel | Lowest code, highest validation burden |
| Portability | Good | Good (`rfd` covers macOS, Windows, Linux portals) | Best |
| Testability | Mockable in the frontend; consent link hard to test | Picker behind a small trait so tests inject outcomes | Easy |
| UX | Native | Native | Error-prone; needs path completion to be usable |

A could be made safe by having Rust issue a one-time token bound to the dialog result, but that is option B with more moving parts. C may later serve as an accessibility or headless fallback, but only through the same Rust validation and still with explicit confirmation.

## Platform and durability assumptions

Stated so that no guarantee is read into them:

- **`(dev, ino)` is Unix metadata.** On macOS `dev` is assigned when a volume mounts and can differ after a remount (removable and network volumes especially), so an unchanged folder can show `changed`. That false positive is accepted as fail-safe. Inode reuse after deletion is possible, more plausibly on non-APFS volumes; rule 2 of [duplicates](#duplicates-and-stale-records) is why a stale inode never matches.
- **Case and normalization.** APFS is case-insensitive by default and normalization-insensitive; case-sensitive APFS exists. Path comparison is byte equality on canonical paths; identity covers the rest where it can.
- **Aliases.** Firmlinks (`/System/Volumes/Data/...`), symlinks such as `/tmp` → `/private/tmp`, and other mount paths give one directory several spellings; `canonicalize` resolves symlinks but not firmlinks.
- **TCC.** Whether `lstat` of a folder under Desktop, Documents, Downloads, iCloud Drive, removable or network volumes prompts or fails with `EPERM` on later launches for this non-sandboxed app must be measured. Revalidation never prompts deliberately; a refusal is `inaccessible`.
- **Unavailable volumes.** An unmounted volume makes its folders `missing`. An unreachable network volume can make `lstat` slow, delaying `list_projects`; not mitigated in this slice.
- **`flock`.** Advisory, and on macOS conflicts between separate open file descriptions even within one process. It may be unsupported or local-only on network home directories; any failure is `registry_unavailable`.
- **Durability.** See [Durability](#durability): crash consistency yes, power-loss durability best-effort.
- **Shared registry.** Debug and release builds share the identifier and therefore `projects.json`. A newer development build can block an older release build with `registry_newer_version`; that is intended.
- **Sandboxing and security-scoped bookmarks** are deferred. Windows (no inode; volume serial and file index, `LockFileEx`) and Linux (portal picker, bind mounts) are unsupported and return `unsupported_platform`.

## Acceptance criteria for the implementation

Each criterion names the layer that proves it: **[unit]** Rust registry tests, **[ipc]** Rust mock-runtime tests against the real generated context, **[fe]** Vitest and `config.test.ts`, **[native]** a real macOS build, recorded like [#42](desktop-visual-verification.md#native-end-to-end-runtime-status-issue-42).

Registry files

1. **[unit]** A `projects.json.tmp` that is a symlink to a sentinel file: after an add, the sentinel is byte-identical and the registry holds the new record. A directory at `projects.json.tmp` makes the add return `registry_unavailable` with the registry unchanged. A valid-looking `projects.json.tmp` with no `projects.json` lists as empty and is never promoted.
2. **[unit]** `projects.json` as a FIFO: `list_projects` returns `registry_unavailable` within 1 s. As a symlink to a valid registry: `registry_unavailable`. A directory or a file owned by another user (skipped when not root-capable): `registry_unavailable`.
3. **[unit]** A 256 KiB + 1 byte file is `registry_corrupt`; a valid file of exactly 256 KiB parses; serializing past either limit returns `registry_full`.
4. **[unit]** Every [validation](#validation) rule has a failing fixture (unknown field at each level, duplicate field, bad or duplicate ID, ID not below `next_id`, `next_id` out of range, relative, non-normalized, overlong or control-character path, duplicate path, `registered_at_ms` above `2^53 - 1`, `schema_version` missing or `0`). A file with two records sharing `(dev, ino)` is valid.
5. **[unit]** A missing `D` or `projects.json` lists as empty and creates nothing; `D` that is a symlink or group-writable gives `registry_unavailable`; a created `D` is mode `0700` and files are `0600`.
6. **[unit]** A second process (the test binary re-invoking itself) holding `projects.lock` makes a mutation return `registry_unavailable` within 2.5 s; after release it succeeds. Many threads and two processes adding distinct folders end with every record and unique IDs. `projects.lock` exists after every operation and is never replaced.
7. **[unit]** With failures injected before `rename`, the previous `projects.json` is byte-identical and the temp file is gone; if the cleanup unlink also fails, the result is still `registry_unavailable` with `projects.json` byte-identical, and the next write succeeds. With a failure injected in the directory sync after `rename`, the command returns `registry_unavailable` and the next list shows the committed record.
8. **[unit]** Listing never changes `projects.json` (bytes and modification time) in any state, including corrupt and newer.

Corruption and recovery

9. **[unit, ipc]** A corrupt file: list, add and remove return `registry_corrupt` and leave it byte-identical.
10. **[unit, ipc]** `reset_projects` on a corrupt file preserves its exact bytes as `projects.json.corrupt-<ms>`, never overwrites an existing `.corrupt-*` file (pre-created collision → suffixed name), and leaves an empty registry. On a valid, empty or repaired file it returns `not_needed` and touches nothing. On a newer-version file it returns `registry_newer_version` and touches nothing. With failures injected at the link (other than `EEXIST`), the identity check, the directory sync before removal, or the unlink, it returns `registry_unavailable`, `projects.json` is still the corrupt file, any backup holds its exact bytes, and a retry completes the reset without overwriting that backup. With a failure injected in the final directory sync, it returns `registry_unavailable` and the next list is empty with the backup present. No command ever reads a `.corrupt-*` file.
11. **[unit]** A newer `schema_version` is never written, renamed or reset by any command.

Identity and selection

12. **[unit]** Deleting a registered folder and creating one at the same path lists `changed`; adding it returns `already_registered` for the old ID with status `changed`; the record's `(dev, ino)` is unchanged.
13. **[unit]** A record whose stored `(dev, ino)` equals the candidate's but whose path no longer resolves to it (inode reuse simulated through the test's metadata seam) does not block the add.
14. **[unit, native]** Adding the same folder through a symlink, a different case, or the `/System/Volumes/Data/...` spelling returns `already_registered` with the existing ID.
15. **[unit, native]** `/`, `/System/Volumes/Data`, the home directory, `/Users` and `/System/Volumes/Data/Users/<name>` return `folder_refused`; a folder inside home and a folder inside a registered folder are added.
16. **[unit]** Revalidation maps missing, swapped-in symlink, replaced folder and `chmod 000` parent (skipped as root) to `missing`, `changed`, `changed`, `inaccessible`; a `chmod 000` folder itself stays `available`.
17. **[ipc]** With the picker injected: cancelled writes nothing and creates no file or directory; a second concurrent `add_project` returns `selection_in_progress`; the busy flag is released after a dropped call.
18. **[native]** The panel is a sheet on the main window, has no New Folder button, cancel shows no error and creates nothing, a chosen folder is listed as `available` with a `~` path and survives quit and relaunch.

Authorization and boundaries

19. **[ipc]** Each of the four commands is callable from `main` at `tauri://localhost`; rejected for another window, for a remote origin, and with its grant removed while the manifest remains. Manifest, handler, capability and permission files name exactly the same commands. `add_project` and `reset_projects` behave identically with a hostile payload (e.g. `{ "path": "/etc" }`); `remove_project` returns `invalid_input` for malformed IDs and ignores extra keys.
20. **[fe]** `config.test.ts` pins every Cargo dependency table, `rfd` only in the macOS target table with default features off, no `tauri-plugin`, exactly two `@tauri-apps` importers and exactly the five command names in the frontend.
21. **[unit, fe]** Source scans: the registry module contains no `read_dir`, `std::process`, `Command::` or network API, and opens files only in its registry-file functions. The Execution Safety Gate is not imported.
22. **[native]** During add, list, remove and reset no child process is created (kqueue `NOTE_FORK` watch as in #42). Nothing under the chosen folder is read: verified with `fs_usage` where a root-capable machine is available, otherwise recorded as covered only by the source scan.
23. **[native]** Remove from Synapse leaves the folder byte-for-byte unchanged (tree hash before and after).
24. **[fe]** The browser preview shows the disabled button with its reason and makes no IPC call. The corrupt state offers reset only after confirmation; the newer state offers none. Existing tests pass.

Needs real macOS: 14 and 15 for firmlink behavior on a real volume, 18, 22, 23, plus the TCC, removable-volume and network-volume observations in [Platform assumptions](#platform-and-durability-assumptions), on debug **and release** builds.

## Testing strategy

| Layer | Covers |
| --- | --- |
| Rust unit (registry module, temp directories under `std::env::temp_dir()`, never the real `D`) | Criteria 1–16, 21. Failure injection and metadata (for inode reuse) go through a test-only seam in the registry module, not through IPC |
| Rust concurrency | Criterion 6: threads, and two processes through the test binary re-invoking itself |
| Rust IPC (mock runtime, like the existing ACL tests) | Criteria 9, 10, 17, 19. Picker injected through the trait |
| Frontend (Vitest, mocked adapter) | Criteria 20, 24, plus loading, empty, list, added, already registered, cancelled, each error code, corrupt and reset flow, remove with confirmation and focus, sample versus registered grouping, bidi and control-character names rendered as text |
| Native end to end (manual, macOS) | Criteria 14, 15, 18, 22, 23 and the platform observations |

## Milestones

Two implementation PRs, each reviewable and secure on its own.

**PR 1: Rust registry and authorized commands.**

- Registry module: validation, read and write protocols, refusal policy, identity, revalidation, `reset_projects` recovery.
- Picker trait and the `rfd` implementation.
- The four commands, with manifest, handler, capability, tracked permission files and the ACL tests of criterion 19.
- `config.test.ts` Rust-side changes: every dependency table, the macOS-only `rfd`, four commands in `build.rs` and the handler, and the registry source scans (criteria 20 Rust part, 21).
- Rust unit, concurrency and IPC tests (criteria 1–17, 19).
- A native **debug-build smoke test**, invoking the commands from the Web Inspector console (`window.__TAURI_INTERNALS__.invoke(...)`): sheet, no New Folder, cancel, add, list, remove, reset of a hand-corrupted file, relaunch persistence, no child process (criteria 18 and 22 for these calls).
- Ships no UI; the commands are present but uncalled in release builds, which the authorization tests make safe on their own.

**PR 2: Projects UI.**

- `src/app/projects.ts` adapter and the `config.test.ts` frontend pins.
- Grouped list, add, remove and reset flows, corrupt and newer states, browser-preview behavior.
- Mocked frontend tests (criteria 20 frontend part, 24).
- Native verification on debug **and** release builds through the UI (criteria 14, 15, 18, 22, 23 and the platform observations).
- This document updated from "proposed" to "implemented".

## Decisions

Resolved by the review of the first draft:

| Question | Decision |
| --- | --- |
| `rfd` or `tauri-plugin-dialog` | `rfd`, macOS target table, default features off |
| What to refuse | `/`, `/System/Volumes/Data`, home and its ancestors, by identity ([Refusal policy](#refusal-policy)) |
| Nested registrations | Allowed |
| Home's draft composer | Not in this slice; Home and Preview stay sample-only |
| Corrupt registry | Preserved and blocking until an explicit, confirmed `reset_projects` |
| Platforms | macOS only; others return `unsupported_platform` |
| Registration and permissions | Fully separate; registration authorizes nothing |

Still open, none blocking implementation:

- Whether Windows and Linux support is a goal before any content-access feature.
- Whether development builds should use a separate identifier so they stop sharing the release registry.
- TCC and network-volume behavior, to be measured natively in PR 1 and PR 2.

## Implementation status

| Part | State |
| --- | --- |
| Registry: validation, read and write protocols, `reset_projects`, identity, refusal policy, revalidation (`src-tauri/src/projects/`) | Implemented in #47 |
| `rfd` folder panel, the four commands, manifest, capability and generated permission files | Implemented in #47 |
| `config.test.ts` Rust side: every Cargo dependency table, `rfd`'s specification, six commands, registry source scans | Implemented in #47 |
| `src/app/projects.ts`, the Projects UI, the confirmed reset, browser-preview state, criteria 20 (frontend part) and 24 | Pending (PR 2). No frontend module names a project command; `config.test.ts` fails if one does, so `reset_projects` cannot gain a caller before its confirmation exists |
| Other targets | Answer `unsupported_platform`. Only macOS is built: neither this machine nor CI has another target installed, so that code path is not compiled anywhere |

### Where the implementation refines the design

None of these changes the security model.

- **Unstorable canonical paths.** A canonical path containing a control character is refused with `path_unsupported` before anything is written, because the registry would otherwise reject its own file on the next load. The 1024-byte rule is enforced the same way but cannot be reached on macOS, where `PATH_MAX` is 1024 bytes including the terminator.
- **State and injection.** The app data directory is resolved once, in Tauri's `setup`, by the path resolver and kept in managed state (`Projects`); the panel is a second managed state (`Picker<R>`), because `generate_handler!` cannot name a runtime type argument. The Rust tests manage a scratch directory and a fake picker instead, so no test touches the real directory. Failure injection is a `#[cfg(test)]` thread-local seam; in production `fault()` is `Ok(())`, which `config.test.ts` pins.
- **Lock file.** Opened without `O_NONBLOCK`: on macOS an `O_RDWR` open of a FIFO returns at once (checked), and the `fstat` check then refuses it (tested). The directory sync opens `D` with `O_DIRECTORY | O_NOFOLLOW`.
- **Panel guard scope.** The one-panel guard is held until the registration that follows the panel completes, not only while the panel is open.
- **Listing order** is the file's record order, which is registration order.

### Verification record (PR 1)

**Automated.** `cargo test` (default and `--release`): 94 tests, 55 of them new: 46 registry unit tests (`projects/registry/tests.rs`) and 9 IPC tests in `lib.rs`. `cargo check --all-targets` with and without `dev-runtime`, and `pnpm test`, `lint` and `typecheck`, all without warnings. The tests cover criteria 1–17 and 19, the Rust part of 20, and 21. Notes:

- Criterion 6 uses two real processes: the test binary re-invokes itself, once to hold `projects.lock` (the mutation fails after the 2 s budget) and once to add folders concurrently with eight threads.
- A registry file owned by another user needs root to set up and is not tested. A data directory owned by another user is tested with `/usr`.
- The source scans were mutation-checked: a `read_dir`, a `Command::`, a stray `File::open`, a `remove_file` of another path, `rfd` with default features, an extra target or dotted dependency table, and a frontend mention of `reset_projects` each make `config.test.ts` fail.

**Release build** (`pnpm tauri build --bundles app`). `verify-release-isolation.sh` passes. Compared with `main`'s release binary, the executable imports exactly six more symbols (`flock`, `geteuid`, `linkat`, `nanosleep`, `rename`, `unlink`) and links the same 14 system libraries; `rfd` adds no framework. The commands are present in the release binary and authorized like every other command. The development Python bridge is unchanged and still unavailable in release.

**Native smoke test.** Run on 2026-10-13, macOS 27.0.1 (arm64), debug bundle from `509bd5e`. The commands were invoked from the Web Inspector console (`window.__TAURI_INTERNALS__.invoke`), and the window and panel were driven through the accessibility API. A kqueue `NOTE_FORK` watcher was attached to the app for both sessions ([procedure](desktop-visual-verification.md#reproducing-it)). The folder registered for testing was a throwaway directory in a temporary location; its tree hash (names, modes, sizes, modification times and contents) was the same before registration, while registered and after removal.

| Check | Observed |
| --- | --- |
| Panel presentation | An `AXSheet` attached to the main window, showing the fixed message as its title line. Its controls: back, forward, "Where:", search, Cancel, Open. **No New Folder button** |
| Cancel | `{"outcome":"cancelled"}`. The data directory listing and the throwaway tree hash were unchanged |
| Add | `added`, `status` `available`. `path_display` was the full canonical path for the throwaway folder (outside home) and `~/Documents/Projects` for the folder registered in the incident below |
| Second `add_project` while the panel is open | `selection_in_progress` ("A folder selection is already open."); one panel only |
| Quit and relaunch | `list_projects` in the new process returned the same record and ID |
| Remove | `removed`; the next ID was not reused; the folder was unchanged |
| Hand-corrupted `projects.json` | `list_projects` and `remove_project` returned `registry_corrupt` with the fixed message. `reset_projects` (with a hostile `{path:"/etc"}` payload) returned `reset` and kept the bytes as `projects.json.corrupt-<ms>` (same SHA-256). The next list was empty, and a second reset returned `not_needed` |
| Created files | The data directory was mode `0700`; `projects.json`, `projects.lock` and the backup were `0600` |
| Child processes | **0 forks** in both sessions (about 13 and 6 minutes) |
| Core command from the webview | ⌥⌘I made the page call `webview.internal_toggle_devtools`, which the ACL rejected ("not allowed"). That is the existing core-permission boundary, observed natively |

**Incident during the smoke test.** On first use, AppKit opened the panel in `~/Documents/Projects`: an OS-chosen location, since Synapse sets no starting directory. Twice the open panel was confirmed with its default Open button while the automation was only reading accessibility attributes, and that folder was registered (`p1`, then `p2`). Only its metadata was looked at (`canonicalize`, `lstat`). Both records were removed at once through `remove_project`. The cause was not determined: a later 20-second hands-off run kept the panel open, so outside input is the likely explanation, but it is not proven. For the rest of the run, the panel's remembered location (the app's own `NSNavLastRootDirectory` default) was pointed at the throwaway folder, so any confirmation could only register throwaway paths. One more such confirmation registered its `src` subfolder, which was the record used in the table above. **Lesson for PR 2:** the panel can open in a real folder the user did not pick for Synapse, and its default button is one keypress away, so the UI's own copy (already planned next to the button) matters.

**Cleanup.** The data directory did not exist before the run. Its remaining files (`projects.lock` and the test's corrupt backup) and the directory itself were removed afterwards, as were the panel keys AppKit wrote into the app's defaults.

**Not verified natively** (still open for PR 2 or later):

- Release-build native behavior.
- Aliases (criterion 14) and refusals (criterion 15) through the real panel. Unit tests cover them on this machine's APFS volume, including the firmlink spelling.
- `fs_usage` evidence that nothing under the folder is read (criterion 22): no root was available. This is covered by the source scans and by a unit test that registers a mode-000 folder containing a FIFO.
- TCC, removable and network volumes.
- The `not_allowed` path: the capability already confines the command to `main`, so it is covered by an IPC test only.

## Review record

Findings of the architecture review of `8de65b6` and where they are resolved:

| Finding | Resolution |
| --- | --- |
| S1 temp file opened with `O_TRUNC` follows a symlink | [Write protocol](#write-protocol) step 8; criterion 1 |
| S2 read path: FIFO, symlink, unbounded read | [Read protocol](#read-protocol); criteria 2, 3 |
| S3 directory and lock-file hygiene, bounded lock | Write protocol steps 1–4, [Storage](#storage) file table; criteria 5, 6 |
| S4 cancel could leave a new folder (`rfd` default) | [Directory selection](#directory-selection); criteria 17, 18 |
| S5 the click is not a trust boundary | [Threat model](#threat-model) and threat 14 |
| I1 automatic quarantine races and resets silently | [Corrupt and newer registries](#corrupt-and-newer-registries), [Recovery](#recovery-reset_projects); criteria 9–11 |
| I2 inode match against stale records; `dev` instability | [Duplicates and stale records](#duplicates-and-stale-records), [Platform assumptions](#platform-and-durability-assumptions); criteria 12, 13 |
| I3 `open` + `fstat` overstated | [Project identity](#project-identity) (lstat with pre-commit re-check, no TOCTOU claim); criterion 16 |
| I4 incomplete semantic validation | [Validation](#validation); criterion 4 |
| I5 durability and post-commit errors | Write protocol steps 9–11, [Durability](#durability); criterion 7 |
| I6 future access must not trust the record | [Future content access](#future-content-access) |
| P1 source scans and `config.test.ts` deferred to the UI PR | [Milestones](#milestones) PR 1 |
| P2 dependency allowlist misses target tables | [Dependencies](#dependencies); criterion 20 |
| P3 native behavior unverified in PR 1 | PR 1 debug-build smoke test |

One review claim was corrected rather than adopted: it said `canonicalize` does not fold case on macOS; on the development Mac it returned the on-disk case on APFS. The design does not rely on either behavior.
