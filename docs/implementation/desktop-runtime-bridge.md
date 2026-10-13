# Development-only Python runtime bridge

[Issue #36](https://github.com/RaioViajante/synapse/issues/36). The second step of the native runtime integration plan, after the [IPC authorization](desktop-architecture.md#ipc-authorization) of PR #33. The desktop Rust backend can ask one fixed question of the Python runtime, "what are you?", through one authorized command, `get_runtime_status`. **It is a development tool.** A default or release build cannot start Python. The Settings → Runtime tab ([issue #40](https://github.com/RaioViajante/synapse/issues/40)) is its only caller; see [Runtime status in Settings](#runtime-status-in-settings).

This is not a sandbox, not provider authorization and not isolation from a same-user attacker. Launching a process is a security boundary even when the question is read-only, so the bridge fails closed whenever its environment, interpreter or answer cannot be trusted for this narrow contract.

## What it does and does not do

| Does | Does not |
| --- | --- |
| Run `desktop/runtime-bridge/status.py` once per request with a fixed argv | Run any other script, module, CLI command or user-supplied code |
| Report the runtime version, the Python version and the Execution Safety Gate's static `POLICY_VERSION` | Report provider readiness, models, authentication, projects, sessions, vault data, paths, environment or sandbox claims |
| Let Rust set provenance, observation time and duration | Trust the child to describe how it ran |
| Say the gate has a policy version | Say anything about a live gate decision or authorization |

## Contract

`get_runtime_status` takes **no arguments**. A payload sent by the frontend has nothing to bind to (tested).

```json
{
  "schema_version": 1,
  "provenance": { "source": "python_runtime", "interpreter_kind": "dev_checkout", "observed_at_ms": 0, "duration_ms": 0 },
  "runtime": { "version": "0.12.1", "python": "3.14.7" },
  "gate": { "policy_version": 1 }
}
```

Python prints only `schema_version`, `runtime` and `gate`. `provenance` is written by Rust from its own clock; a child that includes it is rejected as malformed. `observed_at_ms` is wall-clock milliseconds when the answer was accepted and `duration_ms` is the monotonic time from just before spawn to acceptance. The `python` and `version` strings must be 1–32 characters of `[A-Za-z0-9._+-]`, which keeps a path or message from riding in an identity field.

Errors are `{ "code": ..., "message": ... }` with a fixed message per code and nothing from the child, the filesystem or the environment:

| Code | Meaning |
| --- | --- |
| `runtime_not_installed` | Default or release build; no `.dev-python`; interpreter missing, relative, not executable, or failed the ownership/permission check; or the OS refused to start it. All of these look the same on purpose. |
| `runtime_unsupported_version` | Interpreter below Python 3.10 (see below), or a `schema_version` other than 1 |
| `runtime_timeout` | No complete answer and exit within 5 seconds |
| `runtime_output_too_large` | More than 16 KiB on stdout |
| `runtime_output_malformed` | Not exactly the documented JSON: invalid, unknown or duplicate key, wrong type, bad identity string, trailing data |
| `runtime_failed` | Any other non-zero exit, a signal, or an internal failure |
| `runtime_busy` | Another request is in flight |
| `unsupported_platform` | Not macOS |

### Unsupported interpreter protocol

`status.py` is standard library only and avoids syntax newer than Python 3.7. Before importing anything from `runtime` it compares `sys.version_info` with `(3, 10)`. If the interpreter is older it exits with status **78** and prints nothing; Rust maps exactly that status to `runtime_unsupported_version`. A test confirms no `runtime` module is imported on that path.

## Trust boundaries

```text
React (main window) ──ACL: allow-get-runtime-status──▶ Rust command (async, spawn_blocking)
                                                         │ one request at a time, 5 s, 16 KiB
                                                         ▼
              absolute interpreter chosen at build time ── python -I -S -B status.py
                                                         ▼
                 runtime/__init__ → execution_safety_gate (+ its imports), nothing else
```

- **Frontend → Rust.** Authorized by the Tauri app manifest and capability. The command is registered, in the manifest, has a generated permission file and is granted to window `main` only. See [IPC authorization](desktop-architecture.md#ipc-authorization).
- **Rust → interpreter.** The interpreter path comes from `build.rs`, not from the frontend, `PATH` or any environment variable.
- **Interpreter → checkout.** The script and the `runtime` package are read from the same checkout the app was built from. Anyone who can write to that checkout, the interpreter or its directory can run code as the developer; that is the developer's own trust, the same as running `python` by hand.

## Development interpreter

1. Create `desktop/src-tauri/.dev-python` containing one absolute path to a Python ≥ 3.10, for example `/opt/homebrew/opt/python@3.14/bin/python3.14`. The file is Git-ignored. Do not commit it.
2. Build with the feature: `pnpm tauri dev --features dev-runtime`, or `cargo test --features dev-runtime` inside `desktop/src-tauri`.

`build.rs` reads the file only when `dev-runtime` is enabled, rejects control characters (they could inject `cargo:` directives) and relative paths, tells Cargo to watch the file (`rerun-if-changed`, which also fires when it is first created) and bakes the path and the checkout root into the debug binary. There is no run-time lookup, no `PATH` search, no fallback to `/usr/bin/python3` and no override variable.

At each request the path is checked again, because the file system may have changed after the build: it must be absolute; it is **canonicalized once** (symlinks such as Homebrew's or a virtualenv's `python` are resolved and the resolved file is what runs, so a venv symlink runs its base interpreter, which `-I -S` makes irrelevant); the target must be a regular file with an execute bit; and every component of the canonical path, from the file up to `/`, must be owned by the current user or root. The file and its directory must also be writable by no group or other user. Each further ancestor must not be world-writable (a sticky bit does not make one acceptable). Any failure is `runtime_not_installed`.

Group-writable ancestors above the interpreter's directory are accepted on purpose. Homebrew's own layout has them: `/opt/homebrew/Cellar` is `drwxrwxr-x` owned by the user, group `admin`. Rejecting them would reject the most common development interpreter on macOS. The consequence is that **every member of such a group is trusted as much as the owner**: anyone who can write that ancestor could rename the directories below it and substitute an interpreter. For `admin` that adds little, since its members can already use `sudo`; for any other group it is a real widening of trust, and choosing an interpreter under such a directory is the developer's decision.

### Choosing an interpreter

Use an interpreter that you own and that no one else can write:

| Option | Passes the check | Trust it adds |
| --- | --- | --- |
| Homebrew (`/opt/homebrew/opt/python@3.x/bin/python3.x`) | Yes (verified locally) | Your account and the `admin` group (Homebrew's `Cellar` is group-writable) |
| pyenv or uv-managed Python under your home directory | Expected yes (not verified here) | Your account only |
| A virtualenv | Only if its base interpreter passes: the venv's `python` symlink is resolved and its base interpreter runs | Whatever the base interpreter needs |
| python.org framework package (`/Library/Frameworks/Python.framework/...`) | No: owned by root but group-writable by `admin` | n/a |
| Apple's `/usr/bin/python3` | Passes ownership, but is a 3.9 shim on many systems and then reports `runtime_unsupported_version` | System |

Do not change the permissions of a shared installation such as the python.org framework to make it pass. That affects every account and every installer that expects to write there, and it hides the condition the check exists to report. The Desktop workflow does tighten that framework on its runner, because the runner is a throwaway VM; that adjustment is CI-only.

## Release isolation

- `dev-runtime` is **off by default** and is the only feature.
- The process code (`runtime_bridge/process.rs`) is compiled only with `dev-runtime` on macOS (and in tests). Without it `get_runtime_status` returns `runtime_not_installed` and there is no spawn code, interpreter path or checkout path in the binary.
- `dev-runtime` with a release profile fails the build: `build.rs` checks Cargo's `PROFILE`, and `lib.rs` has a `compile_error!` for builds without debug assertions. `debug_assertions` alone is not the gate, because a profile can turn it on.
- A debug `.app` built with `dev-runtime` contains the interpreter and checkout paths and the process code. **It must never be distributed.** Nothing in the repository stops someone running `pnpm tauri build --debug --features dev-runtime --bundles app,dmg` and handing out the result: the build succeeds by design, since that is how the bridge is developed. Preventing that on every distribution path needs a packaging and signing control (for example a release pipeline that only signs or notarizes artifacts the verifier accepted), which is a separate increment. Until then the guarantee is: the documented release build cannot contain the bridge, and the verifier below detects one that does.

### Artifact verifier

`desktop/scripts/verify-release-isolation.sh <Synapse.app>` exits 0 only after inspecting the executable that `Contents/Info.plist` names, which must be `synapse-desktop`, a regular non-symlink readable executable, and Mach-O (`lipo -archs`). It exits 2 if the bundle cannot be inspected: bad layout, missing or invalid executable, or any failure of `PlistBuddy`, `lipo`, `otool`, `nm`, `grep`, `find` or `shasum`. It exits 1 on any of these independent kinds of evidence:

1. **Paths and markers** anywhere in the bundle: the checkout path, the configured interpreter path, `.dev-python`, `runtime-bridge`, `status.py`, `SYNAPSE_DEV_PYTHON`, `SYNAPSE_DEV_CHECKOUT` and the source path `runtime_bridge/process.rs` (present through panic locations whenever that module is compiled).
2. **Imported `_killpg`.** Only the bridge's cleanup code uses it. The release binary imports none, and the `dev-runtime` binary does.
3. **Python files** of any kind.
4. **Non-system libraries** in `otool -L`.

On success it prints the executable's name, SHA-256 and the number of libraries and imported symbols inspected.

What these checks cannot establish: that the release binary can start no process at all. Tauri and std already import `posix_spawnp`, `setpgid` and `poll` in the release binary, so their absence is not a usable signal. The proof that the bridge's spawn path is absent is the `cfg` gating at compile time. The artifact checks are an independent tripwire for regressions of that gating, not a substitute for it. A deliberately obfuscated build would evade string and symbol scans.

`desktop/scripts/test-verify-release-isolation.sh` tests the verifier with fixture bundles: missing or symlinked executable, empty `Contents/MacOS`, missing `Contents` or `Info.plist`, an unexpected executable name, a non-Mach-O or unreadable executable, `nm` and `otool` failing (with and without partial output), `nm` reporting no symbols, an unreadable file inside the bundle, and each kind of evidence on its own. Given a real bundle built with `--features dev-runtime`, it also requires the verifier to reject that bundle through the `_killpg`, source-marker and checkout-path checks. CI runs both against the real artifacts.
- Nothing bundles Python or the checkout; `tauri.conf.json` resources are unchanged.
- Windows and Linux builds (with or without the feature) answer `unsupported_platform`. The bridge code is Unix-only and has only been exercised on macOS.

## Process lifecycle and limits

| Control | Value |
| --- | --- |
| argv | `[interpreter, -I, -S, -B, <checkout>/desktop/runtime-bridge/status.py]`, no shell |
| environment | cleared (`env_clear`); hostile `PATH`, `PYTHONPATH`, `PYTHONHOME`, `DYLD_*` are tested not to reach the child |
| working directory | `/` |
| stdin / stderr | null / discarded, so neither can stall the child |
| stdout | piped, read with `poll(2)` on the calling thread in 4 KiB chunks, never more than 16 KiB + 1 byte held |
| inherited descriptors | every descriptor ≥ 3 is marked close-on-exec between fork and exec |
| deadline | 5 s from just before spawn, covering reading stdout and waiting for exit |
| concurrency | one request; a second gets `runtime_busy` immediately |
| UI thread | never blocked: the command is `async` and the work runs on `spawn_blocking` |
| process group | the child leads its own group (`process_group(0)`) |
| failure cleanup | `SIGKILL` to the group, then `wait` to reap, before returning the error |
| success | the direct child has exited and been reaped; the group is **not** signalled, so a descendant that closed stdout and kept running would outlive the request (the shipped script starts none, and the Python tests forbid process creation) |

`-I` (isolated), `-S` (no `site`) and `-B` (no bytecode) narrow what Python loads and writes. They are not OS sandboxing.

No reader thread exists, so none can be abandoned. The 5 s figure is the deadline for the child's own output and exit; after it Rust sends `SIGKILL` and reaps, which is bounded by the OS delivering the signal, not by a timer. A child stuck in uninterruptible kernel sleep could delay the reap beyond 5 s.

## Import closure

The query imports `runtime/__init__` (which imports `artifacts` and `contracts`) and `runtime.execution_safety_gate` for `POLICY_VERSION`. The gate is deliberately unchanged, and its own imports bring in `runtime_validation`, `synapse_plan`, `config`, `vault_context`, `policy`, `execution_modes`, `answer_transport` and `types`. That closure is **not reduced**: the only way to avoid it without editing the gate would be to scrape the constant out of the source text, which is worse than importing the real value. Importing these modules reads nothing from a vault, project or configuration file. One import-time side effect exists: `runtime/config.py` computes its default `~/.mco` path with `os.path.expanduser("~")`, which reads the `HOME` variable (the value only; no file is opened, and the output never contains it). The Python tests pin environment access to exactly `HOME`. The exact 12-module closure is pinned in `tests/test_runtime_status_bridge.py`; adding a module fails the test and needs review.

## Evidence

| Area | Tests |
| --- | --- |
| Python (`tests/test_runtime_status_bridge.py`, 11) | exact keys and values; no path/env/provenance in output; unsupported version before runtime imports; bootstrap syntax; pinned import closure; no CLI/adapter/session/MCP modules; audit-hook tripwire for socket, subprocess, exec/fork, ctypes, mutation and write opens; opens and directory listings resolved against the child's own working directory, with canary files in the cwd and `HOME`; directory listings limited to the import system's search of the standard library, the checkout root and `runtime/`; environment reads limited to `HOME` (see below); only checkout `runtime/`, the bridge directory and the standard library are read; no bytecode written |
| Rust bridge (`runtime_bridge/process/tests.rs`) | valid report; Rust-set provenance; child-supplied provenance, unknown, duplicate, missing and mistyped fields, bad strings, trailing data rejected; unsupported schema; exit 78; other failures never echo output; missing interpreter; oversize stops and reaps the child; exact-limit boundary; timeout reaps the child **and** its same-group descendant; early-closed stdout still times out; 4 MB stderr flood; cleared hostile environment, `cwd` `/`, null stdin; non-CLOEXEC descriptor not inherited; second concurrent call is busy then recovers; fixed error messages; serialized shape; interpreter validation (relative, missing, directory, non-executable, writable file or directory, symlink resolution, world-writable grandparent, Homebrew-like group-writable ancestor, foreign owner at every depth, world-writable `/`); child stderr goes to `/dev/null` and a stderr canary never reaches the parent's fd 2; zero `policy_version` rejected |
| Rust with `--features dev-runtime` | fixed argv; real end-to-end call against the configured interpreter, checking the version against `pyproject.toml` |
| Tauri ACL (`lib.rs`) | both commands callable from `tauri://localhost` in `main`; unlisted and unknown commands, other windows, remote origins and core/plugin commands denied; grant removal denies both; payload ignored; exact generated permission files; manifest, handler and capability name the same two commands |
| Frontend (issue #40) | adapter validation and error mapping; Settings → Runtime states and lifecycle; only `runtimeStatus.ts` imports `@tauri-apps/api` and it calls only this command (see [Runtime status in Settings](#runtime-status-in-settings)) |
| Native (issue #42) | the real Tauri window, `dev-runtime` and release builds: answer, Refresh, tab return, no polling, missing interpreter, release unavailable, with requests counted from outside the app; see [desktop-visual-verification.md](desktop-visual-verification.md#native-end-to-end-runtime-status-issue-42) |
| Release | `cargo test --release` (default features) returns the unavailable error; `verify-release-isolation.sh` accepts the real release bundle; `test-verify-release-isolation.sh` with fixtures and the real `dev-runtime` bundle; `dev-runtime` + release profile fails to build |

CI (`.github/workflows/desktop.yml`) installs Python 3.10, runs these, and triggers on `runtime/**` as well as `desktop/**`. `gate.yml` still runs the full Python suite, Node tests, version and skill checks and the npm packaging smoke on every pull request; that smoke does not validate the desktop crate.

## Runtime status in Settings

[Issue #40](https://github.com/RaioViajante/synapse/issues/40). **Settings → Runtime** shows the answer. It is observability only: it does not check agents, providers, models or authentication, and a successful answer says nothing about readiness to run anything.

| Field | Meaning |
| --- | --- |
| Runtime version | `runtime.version`, the version the checkout's Python runtime reports |
| Python | `runtime.python`, the interpreter version that answered |
| Gate policy | `gate.policy_version`, the Execution Safety Gate's static `POLICY_VERSION`. Metadata only: it is not a gate decision and authorizes nothing |
| Source | Fixed copy for `python_runtime` / `dev_checkout`: the Python runtime of this development checkout. No path is shown |
| Observed | Local time from `observed_at_ms` and the request's `duration_ms` |

**When it queries.** Once when the Runtime tab opens, and again only when Refresh is pressed. There is no polling and no automatic retry, including after `runtime_busy`. A request already in flight is shared rather than repeated, so repeated clicks, React Strict Mode's second effect run and leaving and reopening the tab never start a second request; the bridge's own one-request limit stays the backstop. The tab unmounts when hidden, so it never queries in the background, and a late answer for an unmounted tab is dropped. Answers live in component state only; nothing is written to `localStorage` or any other storage.

**Browser preview.** `pnpm dev` in a browser has no Tauri IPC. The section checks `isTauri()` from `@tauri-apps/api/core` and, outside Tauri, makes no call and says it is a browser preview. That check only decides whether to try: whether the command is allowed and available is decided by the native answer, never by `isTauri()`, `__TAURI_INTERNALS__` or the build mode.

**Release builds.** A default or release build answers `runtime_not_installed`, shown as *Not available in this build*. Non-macOS builds answer `unsupported_platform`, shown as *Not supported on this platform*.

**Adapter.** `desktop/src/app/runtimeStatus.ts` is the only module that imports `@tauri-apps/api` (exactly `invoke` and `isTauri` from `core`). It calls `invoke("get_runtime_status")` with the name in one constant and no arguments, and exports no general invoke. Each answer is validated before display: exact keys at every level, `schema_version` 1, `source` `python_runtime`, `interpreter_kind` `dev_checkout`, `observed_at_ms` a positive safe integer that `Date` can represent, `duration_ms` a non-negative safe integer, identity strings matching the Rust rule (1–32 of `[A-Za-z0-9._+-]`), and `policy_version` an integer from 1 to 2³²−1. Anything else, including a rejection that isn't exactly `{ code, message }` with a known code (an ACL denial, for example), becomes *Status couldn't be read*. Each known code has fixed frontend copy; the native `message` and any other native text are never rendered.

Tests: `src/app/runtimeStatus.test.ts` (command name, empty payload, in-flight sharing, every error code, sensitive-text rejection, about 30 malformed shapes) and `src/app/Runtime.test.tsx` (each state, refresh, no polling, unmount, Strict Mode, keyboard, status announcement). Both use `mockIPC` from `@tauri-apps/api/mocks`; production code has no fallback answer.

## Remaining limitations

- Not a sandbox: the child runs with the user's privileges. A same-user attacker who can edit the interpreter, `.dev-python` before a build, the checkout or `runtime/` can run code. Validation has a time-of-check/time-of-use window.
- Under `tauri dev` the webview's origin is `http://localhost:1420`, which Tauri treats as local, so whatever answers on that port can call `get_runtime_status`. The effect is bounded by the contract above (fixed argv, one request at a time, 5 s), and no remote origin is configured.
- Process-group cleanup is not containment: a descendant that leaves the group with `setsid` survives, and descendant cleanup was verified only for same-group children. The shipped script spawns none; the tripwire test guards that.
- Audit-hook tests are regression tripwires inside Python, not proof of confinement. Native extension code can bypass them.
- Only Python 3.14 was run locally; CI uses 3.10, the supported minimum. Unsupported-version handling on a real old interpreter was tested through the function with a stand-in version tuple, not a real 3.9 binary.
- macOS only. Release builds were not signed or notarized.
- The interpreter checks accept group-writable ancestors above its directory, trusting those groups (see [Choosing an interpreter](#choosing-an-interpreter)).
- A `dev-runtime` debug bundle can still be built and handed out; see [Release isolation](#release-isolation).
- Closing inherited descriptors relies on marking, which assumes no descriptor 3–`OPEN_MAX` is needed after exec.
