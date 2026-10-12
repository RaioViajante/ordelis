# Codex zero-model-call probe runner

`runtime/codex_probe_runner.py` is the **private, library-only** execution half of the [controlled probe entry point](interactive-probe-authorization.md). It durably consumes a confirmed authorization, executes three non-model Codex CLI commands, and returns bounded, structured, **unauthenticated** evidence. It has no CLI command and does not authorize dispatch. Its only persistence is the [private authorization ledger](../implementation/private-authorization-ledger.md); `mco context preview`, `mco context plan` and `mco context inspect-codex` do not call it.

**This increment ran no real Codex probe.** All tests use synthetic fake executables. No provider was authenticated, no credential file was read, no model task ran, and no personal vault was touched.

## Supported API

```python
from runtime.codex_probe_runner import ProbeAuthorization
from runtime.probe_confirmation import run_controlled_probe

authorization = ProbeAuthorization(
    probe_id="run-1",
    executable_path="/resolved/path/to/codex",     # must already be the resolved file, no symlinks
    executable_sha256="<64 hex>",                   # e.g. binary.sha256 from inspect-codex
    scratch_parent="/external/dedicated/parent",    # canonical path, no symlinked component
    state_root="/external/private-state",            # existing, canonical, user-owned mode 0700
    protected_roots=("/path/to/vault",),            # 1 to 8 directories that must never overlap scratch
    execution_mode="read_only",
    max_model_calls=0,
    command_timeout_seconds=5,                      # 1 to 10
    total_timeout_seconds=20,                       # 1 to 30, not below the command timeout
    max_output_bytes=65536,                         # 1 to 65536 per command
    issued_at=now, expires_at=now + 300,            # lifetime at most 900 seconds
)
result = run_controlled_probe(
    authorization,
    repo_root=Path("/path/to/checkout"),
    cancel=threading.Event(),
)
```

`run_controlled_probe` accepts **no command arguments**. It validates the exact authorization, obtains terminal confirmation, and executes in the same process. It raises `ProbeError` (a `PlanError`) on any refusal and otherwise returns a dictionary. Caller requirements:

- Build the proposal from explicit values, then let `run_controlled_probe` display and confirm them.
- Use a **dedicated** scratch parent that nothing else writes to while the probe runs. Any other change to its entries is reported as a filesystem failure.
- Use an explicit private state root outside the checkout, scratch area, vault and real projects. There is no repo-local or platform-default fallback.
- Do not redirect the result into the repository. Nothing here writes it anywhere.
- Pass a `threading.Event` as `cancel` if the run must be abortable.
- Pass the result's `evidence` straight to `evaluate_validation` in the same process; do not store or reload it.

## Supported commands

The allowlist is `ALLOWED_COMMANDS`, enforced in code: `_run_command` raises `ProbeError("command_not_allowed")` for any vector outside `("--version",)`, `("--help",)`, `("exec", "--help")` before a process is created. The supported API cannot supply arguments, and a test scans the module for shell and model-task flags. The three commands run in that order and the run stops at the first failure, anomaly, cancellation or executable change. Remaining commands report `not_run`.

## Authorization lifecycle

1. **Default deny.** `None`, a dictionary or any non-`ProbeAuthorization` value is refused (`authorization_missing`). Nothing is read from files, environment variables, model responses or provider output.
2. **Construction** validates the shape: read-only mode, zero model calls, bounded timeouts and output budget, at least one protected root, absolute paths, and a lifetime above 0 and at most 900 seconds. The object is frozen.
3. **Exact type and confirmation.** Authorization or confirmation subclasses are rejected. The exact specification must receive the digest-bound terminal confirmation described in the [entry-point guide](interactive-probe-authorization.md).
4. **Run time** refuses a not-yet-valid (`issued_at` in the future) or expired authorization before and after confirmation and immediately before consumption.
5. **Preflight.** Executable and path failures occur before consumption because the terminal step never approved a valid runnable specification. The private state root must be existing, canonical, user-owned mode `0700`, and disjoint by filesystem identity from checkout, scratch and protected roots.
6. **Consumption.** After confirmation, `BEGIN IMMEDIATE` and a primary-key insert atomically commit `probe_id` and the exact-specification digest before scratch creation. The same digest reports `authorization_reused`; another digest for that id reports `authorization_specification_mismatch`. Execution failure, cancellation or crash never removes the row. Expiry and stored timestamps never prune or revive it.
7. **Scope of single use: cooperating processes.** SQLite is the sole consumed-id authority. The bounded process-local registry now stores only pending confirmation nonces. Fixed ledger row and byte limits fail closed; no automatic pruning occurs.

### The user-origin trust boundary

`run_controlled_probe` requires the exact phrase from `/dev/tty`, bound to every displayed value. A controlling terminal is a channel, not proof that a human is present. Same-user code can drive the terminal, import private runner objects, patch the module, or execute the target itself. Therefore:

- Results state `authorization.origin: "terminal_confirmed_private_ledger"` and `single_use_scope: "cooperating_processes"`. This is a lifecycle claim, not authenticated human provenance.
- Evidence stays `evidence_authenticated: false` and `dispatch_authorized: false`.
- Python naming and module visibility prevent accidental calls only. They cannot enforce a boundary against hostile same-user code.

## Filesystem isolation

What the runner does:

- Refuses a scratch parent that is not an existing, absolute, canonical directory (no symlink in any component) or that overlaps the checkout or any protected root. Overlap is decided by **file identity** through the shared check described in [Path overlap checks](private-execution-plan.md#path-overlap-checks), so case aliases on case-insensitive filesystems are caught.
- Refuses an unsafe private state root and any overlap among state, checkout, scratch and protected roots. The ledger subdirectory is mode `0700`; its database is mode `0600`; unexpected files, symlinks, unsafe ownership or permissions, corruption, missing initialized state and unsupported schemas deny.
- Refuses an executable located inside the checkout, a protected root, the scratch parent or the state root.
- Creates a fresh directory with `tempfile.mkdtemp` (mode 0700, verified), re-runs `check_probe_authorization` on it (must be an empty non-symlink directory outside the checkout and protected roots) and creates `home`, `codex-home`, `tmp` and `work` inside it. The working directory, `HOME`, `CODEX_HOME`, `TMPDIR`, `TMP` and `TEMP` point there.
- Never executes in the Synapse checkout and never lists or reads protected roots (they are only compared by `stat`).
- Removes only the directory it created, only if its device and inode still match, and never follows a symlink. If the scratch directory was replaced, nothing is deleted (`cleanup: skipped_identity_mismatch`). If removal fails, `cleanup: failed` and `scratch_leftover` name the path for manual removal. Entries already in the scratch parent, including a stray file a command wrote there, are never deleted.

What it does **not** do: a temporary directory is **not an OS security sandbox**. The probed process runs as the user and can read or write anything that user can. Path validation is point-in-time evidence; a path can change after the check. Network access is not restricted.

## Process isolation

- Argument arrays only, no shell. `stdin` is `/dev/null`. The child starts in a new session and process group.
- The environment is an allowlist: `PATH` plus the five isolated variables. API keys, tokens, proxies and every other inherited variable are dropped, and the environment is re-checked (`environment_not_isolated`) before any spawn. `PATH` is inherited because launcher scripts such as `#!/usr/bin/env node` need it, but only absolute, existing entries outside the checkout, the protected roots and the scratch parent are kept (empty or relative entries would resolve launchers from the working directory); an empty result becomes `/usr/bin:/bin`. The remaining entries, and any interpreter a launcher selects from them, are not verified or hashed, so a writable `PATH` directory can still change what a launcher runs.
- Per-command and total wall-clock limits and a per-command output limit are enforced. On timeout, output overflow, cancellation or error the whole process group receives `SIGKILL`.
- After a command exits, a live process group or a pipe still held open is reported as `descendant_survived`, the group is killed, and `process.group_cleanup` records whether it disappeared (`verified` or `failed`). It is also `failed` if a pipe is still held open after the kill, which means a descendant outside the group survived.
- **Descendant containment is process-group only.** A descendant that starts its own session or group is invisible to this check and is not killed. A test documents this. No cgroup, job object or sandbox is used. No `RLIMIT` limits are applied; the CPU, memory and disk budgets are not enforced.

## Executable identity

The authorization names the resolved path and SHA-256. The runner hashes the file with the existing `codex_capabilities` routine, compares the hash, and records the file's device, inode, size, mtime and ctime. It re-stats and re-hashes the file before every spawn and after the run. Replacement or in-place modification is reported as `identity_check: changed_during_probe` and a failed `executable_identity` record; identical bytes in a new file also count as changed. No later command starts after a detected change.

This is **not race-free**: the file is executed by pathname, so it can be swapped between the check and the exec, and a change that is undone before the next check is not seen. The hash identifies the launcher file only, not any native binary it starts (`native_binary_identity: not_established`).

## Evidence

Only this module builds the records, and they pass `parse_evidence`. Provider output is treated as untrusted data: only its byte count and SHA-256 appear, never its text, filenames it created or any claim it makes. A test shows output that claims a passing isolation check changes nothing.

| Record | When | Source |
| --- | --- | --- |
| `executable_identity` pass/fail | file unchanged or changed across the run | `runtime_probe` |
| `process_termination` fail (`timeout`, `cancelled`, `descendant_survived`) | the runner had to kill, or found survivors | `runtime_probe` |
| `output_format` fail (`malformed_output`) | output over the limit or not UTF-8 | `runtime_probe` |
| `session_persistence` fail (`session_data_written`) | any entry appeared in the scratch directory | `external_check` |
| `read_only_filesystem` fail (`write_succeeded`) | the scratch parent's entries changed | `external_check` |

The result's `limits.declared_execution_mode` echoes the authorization and `execution_mode_enforced` is `false`: nothing enforces read-only behavior. Because they are not tied to a task, the runner **never emits a pass** for anything but `executable_identity`. Absence of a failure from one help command is not enforcement. Records remain unauthenticated: another component could build the same dictionaries, so the trusted-construction rule in [Runtime-validation foundations](runtime-validation-foundations.md#evidence-trust-boundary) still applies.

Evaluating a clean run with `evaluate_validation` yields `executable_identity: satisfied` and `unknown` for model acceptance, read-only filesystem, output format, process termination and session persistence, so `validation.status` stays `blocked`. Tests also show that provider-labeled passes for isolation stay `insufficient_evidence`.

## What remains unknown

Help commands cannot establish any of: authentication state, model acceptance, reasoning-effort behavior, read-only filesystem enforcement, network restriction (always `unknown`), real session persistence, or cancellation of a real task. The result's `verification` block says so, and `inspect-codex` observations of help flags remain "advertised, not enforced".

## Platform behavior

POSIX only. On other platforms `run_controlled_probe` raises `unsupported_platform` before consuming the authorization. Tests rely on process groups and run on macOS and Linux; the Windows CI job does not run the Python suite. Behavior on other POSIX systems is untested.

## Why dispatch stays blocked

A completed probe shows, at most, that three help commands behaved on one executable at one moment after terminal confirmation and durable authorization consumption. Dispatch would additionally need authenticated evidence from a trusted runner, independent isolation checks, a fresh identity check, a separately approved plan, and explicit user approval. `context plan` is unchanged and still reports `validation.status: "blocked"`.

## Prerequisites for a controlled real probe

1. A separately reviewed public CLI milestone; the controlled entry point remains library-only in this increment.
2. An explicit user-approved run against the real binary, with the user's own installation and a reviewed private local state root, still with zero model calls.
3. Independent isolation witnesses before any claim about filesystem, network or session behavior: an OS-level mechanism applied by Synapse, canary writes and read attempts, and network observation outside the provider process.
4. Resource limits (`RLIMIT`, cgroups or equivalent) if resource budgets are to be enforced rather than only time and output.
5. Only after all of that, a separate increment for a bounded model call with `max_model_calls > 0`.
