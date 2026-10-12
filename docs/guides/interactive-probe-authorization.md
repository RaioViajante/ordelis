# Interactive probe authorization

`runtime/probe_confirmation.py` adds a **library-only** terminal confirmation in front of the [Codex zero-model-call probe runner](codex-zero-model-probes.md). A person must read the exact operation on the controlling terminal and type a phrase bound to it before `run_confirmed_probe` will run the runner. It has no CLI command, persists nothing, runs no provider by itself, and does not authorize dispatch.

**No real Codex probe has run.** All tests use synthetic fake executables and pseudo-terminals. No provider was authenticated, no credential was read, no model task ran, and no personal vault was touched.

## API

```python
from runtime.codex_probe_runner import ProbeAuthorization
from runtime.probe_confirmation import confirm_probe, run_confirmed_probe

proposal = ProbeAuthorization(...)                       # same fields as the runner guide; a proposal only
confirmed = confirm_probe(proposal, repo_root=checkout)  # prompts on /dev/tty, raises ProbeError unless approved
result = run_confirmed_probe(confirmed, cancel=event)    # runs the three allowlisted commands, once
```

Both functions raise `ProbeError` with a closed reason code on any refusal. Neither accepts a boolean, callback, file, stream or environment value that could carry consent (a test pins the signatures). `repo_root` is bound at confirmation, so `run_confirmed_probe` takes no checkout argument.

## Flow

1. `confirm_probe` refuses non-POSIX platforms, a non-`ProbeAuthorization` value, and an authorization that is expired or not yet valid.
2. It runs the runner's own path and executable validation (resolved path, no symlinks, no overlap with the checkout, protected roots or scratch parent, SHA-256 matches the real file). Nothing is created or consumed.
3. It opens `/dev/tty` and refuses unless that is a terminal whose foreground process group is this process's group. Standard input and output are never read or written.
4. It discards pending type-ahead, writes the prompt, and reads one line.
5. It accepts only the exact phrase `authorize <first 12 hex digits of the specification digest>`. Anything else, including `yes`, `y`, a blank line, a different case, extra spaces, a partial line, a non-UTF-8 or overlong line, EOF, Ctrl-C or an I/O error, denies. `n`/`no` report `declined`.
6. After the answer it re-checks the expiry window and re-runs step 2. If the executable, a protected root or any displayed value changed while the prompt was open, it denies.
7. On approval it records a random nonce in a process-local registry (bounded at 64, expired entries pruned) and returns `ConfirmedProbe`.
8. `run_confirmed_probe` removes the nonce (single use), re-checks expiry, recomputes the specification digest from the object it was given and the re-resolved paths, and only then calls the runner's shared implementation. The runner then applies all its existing checks, including `probe_id` single use, and stamps `authorization.origin: "terminal_confirmed_process_local"`.

Every refusal by the confirmation layer happens before a scratch directory or process exists; the tests assert that no command was logged and the scratch parent is empty after each negative case.

## What the user sees

The prompt shows, before asking anything: resolved executable path and SHA-256, the three commands in order, the scratch parent (a new private subdirectory is created and removed), protected roots and the excluded checkout, per-command and total timeouts, output limit, probe id, remaining validity, `read_only` as a declared mode, **zero** model calls, the full specification digest, and a list of what is **not** guaranteed: filesystem access, network access, native-binary identity, swap-after-check, descendants outside the process group, resource limits, and malicious same-user processes.

Every dynamic value is printed as an ASCII-escaped JSON string, so control characters and escape sequences in a path cannot clear the screen or fake a second prompt. The prompt reads no file contents and no environment values; vault contents and secrets cannot appear in it because nothing reads them.

## Specification binding

The digest is a SHA-256 over the whole `ProbeAuthorization`, the allowlisted command vectors, the resolved protected roots and the resolved checkout. The typed phrase embeds its prefix, so an answer typed for one specification does not approve another. At run time the digest is recomputed from the authorization object passed in, so a `dataclasses.replace` copy, an in-place `object.__setattr__` change, a swapped authorization, or a retargeted symlink in a protected root is refused. A refused run still spends the confirmation.

## Reason codes

`unsupported_platform`, `authorization_missing`, `authorization_not_yet_valid`, `authorization_expired`, `no_controlling_terminal`, `terminal_not_foreground`, `terminal_io_failed`, `eof`, `interrupted`, `declined`, `not_confirmed`, `specification_changed_during_confirmation`, `specification_changed_after_confirmation`, `confirmation_missing`, `confirmation_invalid_or_reused`, `confirmation_registry_full`, plus the runner's own reasons (`executable_hash_mismatch`, `authorization_reused`, and so on).

## Threat model and trust boundaries

| Party or input | Treated as |
| --- | --- |
| A person at a trusted local terminal | The only source of consent, and only through the typed phrase on `/dev/tty`. |
| A coding agent's text, files and generated code | Untrusted. Cannot supply consent. If it can run code as the same user, see the non-guarantees. |
| Provider output | Untrusted data. Never read here; the runner only hashes and counts it. |
| Repository content, vault notes | Untrusted. Never read or displayed by the confirmation. |
| Environment variables, config files, JSON files, redirected stdin | Never consent. The module does not read them (a test scans the source and a real child process with piped input and flag variables is denied). |
| `ProbeAuthorization`, `ConfirmedProbe` objects | Process-local data. A hand-built `ConfirmedProbe` has no registry entry and is refused. They are **not** independently authenticated. |

### Guarantees

- No approval without a typed phrase read from the controlling terminal that matches the displayed specification.
- Default deny on every failure path listed above, before any process creation.
- Approval is not persisted, exported to the environment, or reusable: one confirmation, one run, and `probe_id` stays single use per process.
- What was displayed is what runs: any later change to a displayed value is refused.

### Non-guarantees

- **Not resistant to a malicious process running as the same user.** Such a process can write to the terminal (for example with `TIOCSTI` where the OS still allows it), read its own prompt, replace this module, call `run_codex_probe` directly (which stays available and reports `caller_asserted_unverified`), or forge registry entries.
- The terminal can be shared or recorded (remote shells, terminal multiplexers); Synapse cannot tell whether a human or a program typed the phrase.
- The approval proves a person was prompted in this process, not who they were.
- Evidence stays unauthenticated (`evidence_authenticated: false`); dispatch stays unauthorized.
- Filesystem, network, native-binary identity and race-freedom remain as limited as in the [runner guide](codex-zero-model-probes.md).
- Single use is per process. A new process can reuse a `probe_id`, and confirmations are lost on exit by design.

## Supported platforms

POSIX with a controlling terminal: macOS and Linux, exercised locally on macOS and in Linux CI. Windows, a missing `/dev/tty` (cron, CI without a pty, `setsid` without a terminal, detached services), and background jobs deny. Behavior on other POSIX systems is untested.

## Integrating a future trusted CLI entry point

A later, separately reviewed increment may add a command that:

1. builds the `ProbeAuthorization` from values the user passes explicitly (never from model output or a config file) and the existing `inspect-codex` hash;
2. calls `confirm_probe` and, on `ProbeError`, prints only the reason code and exits non-zero;
3. calls `run_confirmed_probe` in the same process and passes `result["evidence"]` to `evaluate_validation` in memory;
4. refuses to run when stdin or stdout is not a terminal, to make automation failures obvious (the confirmation would deny anyway);
5. makes `run_codex_probe` private or removes its public export so the confirmed path is the only entry point.

None of this exists yet, and the command must not be added until a reviewer accepts the residual same-user risk above.

## Why real Codex execution is still disabled

The confirmation establishes that a person saw the operation. It does not make the probe sandboxed, authenticate its evidence, or decide cross-process replay. The runner remains library-only, nothing calls these functions outside tests, `context plan` still reports `blocked`, and a test asserts no other runtime module references the confirmation module. A real probe additionally needs the prerequisites in [the runner guide](codex-zero-model-probes.md#prerequisites-for-a-controlled-real-probe).
