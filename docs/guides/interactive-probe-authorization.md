# Interactive probe authorization

`runtime/probe_confirmation.py` owns the **library-only** controlled entry point in front of the [Codex zero-model-call probe runner](codex-zero-model-probes.md). The exact operation is shown on the controlling terminal and a digest-bound phrase must be typed before `run_controlled_probe` invokes the private runner. It has no CLI command and does not authorize dispatch. After confirmation it persists only authorization-consumption metadata in the [private ledger](../implementation/private-authorization-ledger.md). See also the [entry-point decision](../implementation/controlled-probe-entry-point.md).

**No real Codex probe has run.** All tests use synthetic fake executables and pseudo-terminals. No provider was authenticated, no credential was read, no model task ran, and no personal vault was touched.

## API

```python
from runtime.codex_probe_runner import ProbeAuthorization
from runtime.probe_confirmation import run_controlled_probe

proposal = ProbeAuthorization(...)  # same exact fields as the runner guide
result = run_controlled_probe(proposal, repo_root=checkout, cancel=event)
```

The function raises `ProbeError` on any refusal. Path and executable validation reuses the runner's checks, which can raise `ProbeError` or `PlanError`. The function accepts no boolean, callback, file, stream or environment value that could carry consent. Splitting confirmation from execution, confirmation objects, and the raw runner are internal APIs.

## Flow

1. `run_controlled_probe` refuses non-POSIX platforms, any value whose exact type is not `ProbeAuthorization`, and an authorization that is expired or not yet valid. Subclasses are rejected before their attributes are read.
2. It runs the runner's own path, private-state and executable validation (resolved paths, no symlinks, safe state ownership/mode, no overlap among the checkout, state root, protected roots or scratch parent, SHA-256 matches the real file). An existing ledger is checked for replay, specification conflict, corruption and capacity, so nobody is asked to approve a run that cannot start. A first-use ledger is not initialized before confirmation.
3. It takes a process-wide prompt lock (one prompt at a time, so concurrent callers cannot interleave on one terminal), opens `/dev/tty`, and refuses unless that is a terminal in line mode (`ICANON`) whose foreground process group is this process's group. Standard input and output are never read or written. Terminal attributes are never changed, so there is nothing to restore.
4. It discards pending type-ahead, writes the prompt, and reads one line.
5. It accepts only the exact phrase `authorize <first 12 hex digits of the specification digest>`. Anything else, including `yes`, `y`, a blank line, a different case, extra spaces, a partial line, a non-UTF-8 or overlong line, EOF, Ctrl-C or an I/O error, denies. `n`/`no` report `declined`.
6. After the answer it re-checks the expiry window and re-runs step 2. If the executable, a protected root or any displayed value changed while the prompt was open, it denies.
7. On approval the internal confirmation step records a random nonce in a process-local registry (bounded at 64, expired entries pruned).
8. The internal execution step removes the nonce, re-checks expiry, recomputes the specification digest, and calls the private runner with a module-owned capability.
9. The runner revalidates paths and expiry, atomically commits the `probe_id` and digest to the private SQLite ledger, then—and only then—creates scratch. It fully re-hashes the executable before every command and stamps `authorization.origin: "terminal_confirmed_private_ledger"` and `single_use_scope: "cooperating_processes"`.

Every refusal by the confirmation layer happens before a scratch directory or process exists; the tests assert that no command was logged and the scratch parent is empty after each negative case.

## What the user sees

The prompt shows, before asking anything: resolved executable path and SHA-256, the three commands in order, the scratch parent (a new private subdirectory is created and removed), the private state root, protected roots and the excluded checkout, per-command and total timeouts, output limit, probe id, exact issue and expiry timestamps, remaining validity, `read_only` as a declared mode, **zero** model calls, the full specification digest, and a list of what is **not** guaranteed: filesystem access, network access, native-binary identity, swap-after-check, descendants outside the process group, resource limits, and malicious same-user processes. It also says that approval durably consumes the id before scratch creation and lists what is not stored.

Every dynamic value is printed as an ASCII-escaped JSON string, so control characters and escape sequences in a path cannot clear the screen or fake a second prompt. Each displayed value is limited to 512 rendered characters and the complete prompt to 8,192 UTF-8 bytes. An oversized exact value is rejected; it is never truncated. The prompt reads no file contents and no environment values; vault contents and secrets cannot appear in it because nothing reads them.

## Specification binding

The digest is a SHA-256 over the whole `ProbeAuthorization` (including the private state root), the allowlisted command vectors, the resolved protected roots and the resolved checkout. The typed phrase embeds its prefix, so an answer typed for one specification does not approve another. At run time the digest is recomputed from the authorization object passed in, so a `dataclasses.replace` copy, an in-place `object.__setattr__` change, a swapped authorization, or a retargeted symlink in a protected root is refused. A refused run still spends the confirmation. A committed row with the same id and another digest is a specification conflict, never a new authorization.

## Reason codes

`unsupported_platform`, `authorization_missing`, `authorization_not_yet_valid`, `authorization_expired`, `display_value_too_long`, `confirmation_prompt_too_long`, `no_controlling_terminal`, `terminal_not_foreground`, `terminal_not_line_mode`, `terminal_io_failed`, `eof`, `interrupted`, `declined`, `not_confirmed`, `specification_changed_during_confirmation`, `specification_changed_after_confirmation`, `confirmation_missing`, `confirmation_invalid_or_reused`, `confirmation_registry_full`, `authorization_reused`, `authorization_specification_mismatch`, and the closed `state_root_*` / `authorization_ledger_*` ledger refusals, plus the runner's other refusals (`executable_hash_mismatch` and so on).

## Threat model and trust boundaries

| Party or input | Treated as |
| --- | --- |
| Bytes read from the controlling terminal | The only accepted consent signal, and only when they match the digest-bound phrase. This does not identify a person. |
| A coding agent's text, files and generated code | Untrusted. Cannot supply consent. If it can run code as the same user, see the non-guarantees. |
| Provider output | Untrusted data. Never read here; the runner only hashes and counts it. |
| Repository content, vault notes | Untrusted. Never read or displayed by the confirmation. |
| Environment variables, config files, JSON files, redirected stdin | Never consent. The module does not read them (a test scans the source and a real child process with piped input and flag variables is denied). |
| `ProbeAuthorization`, internal confirmation objects | Caller data plus a process-local nonce. Subclasses and hand-built confirmation objects are refused. They are **not** independently authenticated. |
| Private SQLite ledger | The cooperating-process single-use authority. It is private local state, not proof of human identity and not resistant to hostile same-user modification or deletion. |

### Guarantees

- No approval without a matching phrase read from the controlling terminal and bound to the displayed specification.
- Default deny on every failure path listed above, before any process creation.
- The terminal answer and prompt are not persisted or exported. A committed `probe_id` stays single use across cooperating processes using the same ledger.
- Any change to a displayed value between approval and the start of the run is refused. This is a point-in-time check: the executable is still started by pathname and can be swapped after the last check (see the runner guide).

### Non-guarantees

- **Not resistant to a malicious process running as the same user.** Such a process can write to the terminal (for example with `TIOCSTI` where the OS still allows it), read its own prompt, replace this module, import private capability objects, forge registry entries, call internal Python functions, or execute the selected file directly. Python naming and module visibility do not enforce this boundary.
- Synapse cannot tell whether a human or a program typed the phrase. A controlling terminal is not proof of a person: agent harnesses and automation commonly run commands inside a pseudo-terminal that the controlling program reads and writes itself, and terminals can be shared or recorded (remote shells, multiplexers). Run the future CLI from a terminal you operate yourself.
- Oversized dynamic values or prompts are rejected before `/dev/tty` is opened.
- The approval records that this process wrote the prompt and read the matching phrase; it proves neither human presence nor identity.
- Evidence stays unauthenticated (`evidence_authenticated: false`); dispatch stays unauthorized.
- Filesystem, network, native-binary identity and race-freedom remain as limited as in the [runner guide](codex-zero-model-probes.md).
- Pending confirmations are lost on exit. Committed consumption survives exit and crashes, but deletion or rollback of the private ledger is a trust reset that can make old ids reusable.
- SQLite atomicity is supported only on suitable local filesystems; arbitrary network shares and hostile same-user filesystem races are outside the guarantee.

## Supported platforms

POSIX with a controlling terminal: macOS and Linux, exercised locally on macOS and in Linux CI. Windows, a missing `/dev/tty` (cron, CI without a pty, `setsid` without a terminal, detached services), and background jobs deny. Behavior on other POSIX systems is untested.

## Future public CLI integration

A later, separately reviewed increment may add a command now that cross-process replay and private state have a library-only design. It would:

1. build the `ProbeAuthorization` from values the user passes explicitly (never from model output or a config file) and the existing `inspect-codex` hash;
2. call `run_controlled_probe` and, on `ProbeError`, print only the reason code and exit non-zero;
3. pass `result["evidence"]` to `evaluate_validation` in memory;
4. refuse to run when stdin or stdout is not a terminal, to make automation failures obvious (the confirmation would deny anyway);
5. remain unable to start model tasks or authenticated provider dispatch.

No such command exists. Adding it now would make an installed Codex launcher reachable from the public command surface, which is outside this increment.

## Why real Codex execution is still disabled

The confirmation records that the exact phrase arrived through the controlling terminal; it does not prove a human was present. The ledger prevents replay among cooperating processes, but it does not make the probe sandboxed, authenticate its evidence, or resist hostile same-user code. The runner remains library-only, nothing calls the entry point outside tests, `context plan` still reports `blocked`, and a test asserts the public CLI does not reference the confirmation module. A real probe additionally needs the prerequisites in [the runner guide](codex-zero-model-probes.md#prerequisites-for-a-controlled-real-probe).
