# Development-only Python runtime bridge

[Issue #36](https://github.com/RaioViajante/synapse/issues/36). The second step of the native runtime integration plan, after the [IPC authorization](desktop-architecture.md#ipc-authorization) of PR #33. The desktop Rust backend can ask one fixed question of the Python runtime, "what are you?", through one authorized command, `get_runtime_status`. **It is a development tool.** A default or release build cannot start Python, and no UI uses the command yet.

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

At each request the path is checked again, because the file system may have changed after the build: it must be absolute; it is **canonicalized once** (symlinks such as Homebrew's or a virtualenv's `python` are resolved and the resolved file is what runs, so a venv symlink runs its base interpreter, which `-I -S` makes irrelevant); the target must be a regular executable file owned by the current user or root and not group- or world-writable; and its directory must be owned by the user or root and not group- or world-writable. Any failure is `runtime_not_installed`.

This rejects some common installs: the python.org macOS framework package is owned by root but group-writable by `admin`, so `/Library/Frameworks/Python.framework/Versions/3.x/bin/python3.x` and its `bin` directory fail the check. Use a Homebrew or other user-owned interpreter, or run `sudo chmod g-w` on the binary and its `bin` directory. The Desktop workflow does the latter on its ephemeral runner.

## Release isolation

- `dev-runtime` is **off by default** and is the only feature.
- The process code (`runtime_bridge/process.rs`) is compiled only with `dev-runtime` on macOS (and in tests). Without it `get_runtime_status` returns `runtime_not_installed` and there is no spawn code, interpreter path or checkout path in the binary.
- `dev-runtime` with a release profile fails the build: `build.rs` checks Cargo's `PROFILE`, and `lib.rs` has a `compile_error!` for builds without debug assertions. `debug_assertions` alone is not the gate, because a profile can turn it on.
- A debug `.app` built with `dev-runtime` contains the interpreter and checkout paths. **It must never be distributed.** `desktop/scripts/verify-release-isolation.sh` detects this: it searches the bundle for the checkout path, any configured interpreter path and the bridge's marker strings, rejects Python files, and rejects non-system libraries. It was run against the real `pnpm tauri build --bundles app` output (passes) and against a `dev-runtime` debug binary (fails, as intended).
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

`-I` (isolated), `-S` (no `site`) and `-B` (no bytecode) narrow what Python loads and writes. They are not OS sandboxing.

No reader thread exists, so none can be abandoned. The 5 s figure is the deadline for the child's own output and exit; after it Rust sends `SIGKILL` and reaps, which is bounded by the OS delivering the signal, not by a timer. A child stuck in uninterruptible kernel sleep could delay the reap beyond 5 s.

## Import closure

The query imports `runtime/__init__` (which imports `artifacts` and `contracts`) and `runtime.execution_safety_gate` for `POLICY_VERSION`. The gate is deliberately unchanged, and its own imports bring in `runtime_validation`, `synapse_plan`, `config`, `vault_context`, `policy`, `execution_modes`, `answer_transport` and `types`. That closure is **not reduced**: the only way to avoid it without editing the gate would be to scrape the constant out of the source text, which is worse than importing the real value. Importing these modules reads nothing from a vault, project or configuration. The exact 12-module closure is pinned in `tests/test_runtime_status_bridge.py`; adding a module fails the test and needs review.

## Evidence

| Area | Tests |
| --- | --- |
| Python (`tests/test_runtime_status_bridge.py`, 9) | exact keys and values; no path/env/provenance in output; unsupported version before runtime imports; bootstrap syntax; pinned import closure; no CLI/adapter/session/MCP modules; audit-hook tripwire for socket, subprocess, exec/fork, ctypes, mutation and write opens with canary files and variables; only checkout `runtime/`, the bridge directory and the standard library are read; no bytecode written |
| Rust bridge (`runtime_bridge/process/tests.rs`) | valid report; Rust-set provenance; child-supplied provenance, unknown, duplicate, missing and mistyped fields, bad strings, trailing data rejected; unsupported schema; exit 78; other failures never echo output; missing interpreter; oversize stops and reaps the child; exact-limit boundary; timeout reaps the child **and** its same-group descendant; early-closed stdout still times out; 4 MB stderr flood; cleared hostile environment, `cwd` `/`, null stdin; non-CLOEXEC descriptor not inherited; second concurrent call is busy then recovers; fixed error messages; serialized shape; interpreter validation (relative, missing, directory, non-executable, writable file or directory, symlink resolution) |
| Rust with `--features dev-runtime` | fixed argv; real end-to-end call against the configured interpreter, checking the version against `pyproject.toml` |
| Tauri ACL (`lib.rs`) | both commands callable from `tauri://localhost` in `main`; unlisted and unknown commands, other windows, remote origins and core/plugin commands denied; grant removal denies both; payload ignored; exact generated permission files; manifest, handler and capability name the same two commands |
| Release | `cargo test --release` (default features) returns the unavailable error; `verify-release-isolation.sh` on the real release bundle; `dev-runtime` + release profile fails to build |

CI (`.github/workflows/desktop.yml`) installs Python 3.10, runs these, and triggers on `runtime/**` as well as `desktop/**`. `gate.yml` still runs the full Python suite, Node tests, version and skill checks and the npm packaging smoke on every pull request; that smoke does not validate the desktop crate.

## Remaining limitations

- Not a sandbox: the child runs with the user's privileges. A same-user attacker who can edit the interpreter, `.dev-python` before a build, the checkout or `runtime/` can run code. Validation has a time-of-check/time-of-use window.
- Under `tauri dev` the webview's origin is `http://localhost:1420`, which Tauri treats as local, so whatever answers on that port can call `get_runtime_status`. The effect is bounded by the contract above (fixed argv, one request at a time, 5 s), and no remote origin is configured.
- Process-group cleanup is not containment: a descendant that leaves the group with `setsid` survives, and descendant cleanup was verified only for same-group children. The shipped script spawns none; the tripwire test guards that.
- Audit-hook tests are regression tripwires inside Python, not proof of confinement. Native extension code can bypass them.
- Only Python 3.14 was run locally; CI uses 3.10, the supported minimum. Unsupported-version handling on a real old interpreter was tested through the function with a stand-in version tuple, not a real 3.9 binary.
- macOS only. Release builds were not signed or notarized.
- Closing inherited descriptors relies on marking, which assumes no descriptor 3–`OPEN_MAX` is needed after exec.
