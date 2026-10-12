# Controlled zero-model-call probe entry point

Status: accepted for issue #15.

## Decision

`runtime.probe_confirmation` owns the only supported probe entry point, `run_controlled_probe`. It accepts an exact `ProbeAuthorization`, validates it, obtains terminal confirmation, consumes that confirmation in the same process, and invokes the private runner. The entry point remains library-only. No `mco` command exposes it in this increment.

The authorization binds the canonical executable path and SHA-256 produced by the existing executable-identity inspection, plus the scratch parent, protected roots, checkout, limits, execution mode, zero model calls, lifetime, probe id, and the three fixed command vectors. It accepts no caller-provided command arguments. Path validation reuses identity-based overlap checks, rejects symlinked or non-canonical executable and scratch paths, and rejects executable locations inside the checkout, protected roots, or scratch parent.

Terminal confirmation displays every bound value and requires the exact digest-derived phrase. Dynamic display values and the complete prompt have hard size limits; an exact value that cannot be displayed within those limits is rejected instead of truncated. The controlling terminal is only the channel for the challenge and response. Its existence is not evidence that a human is present.

After confirmation, a process-local single-use nonce carries the approved digest to the private runner. The runner accepts only the module-owned capability object by identity, consumes the probe id, revalidates the executable path, file state, and hash before every allowlisted command, and executes argument arrays without a shell. `ProbeAuthorization` and the confirmation value must have the exact expected types; subclasses are rejected because overridden attribute access can be mutable or stateful.

The supported surface is `ProbeAuthorization`, `ProbeError`, and `run_controlled_probe`. Confirmation splitting, nonce handling, command execution, and the raw runner remain internal implementation details. Python underscores, module placement, exact-type checks, and a private capability object prevent accidental use; they do not stop hostile code running as the same user from importing internals, reading module state, monkey-patching checks, writing to the terminal, or executing the target directly.

## Process and replay behavior

Approvals and consumed probe ids exist only in process memory. Concurrent prompts are serialized, each confirmation is consumed once, and concurrent attempts using the same confirmation or probe id produce at most one execution. A process exit discards pending confirmation state. A different process can reuse the same specification and probe id after obtaining or bypassing its own process-local state.

Cross-process replay protection is deferred. An honest single-use guarantee across processes needs a durable private location, atomic creation or locking, ownership and permission rules, stale-record cleanup, and recovery semantics. This increment has no approved private state root and requires no unintended filesystem persistence. Adding an ad hoc file would create a misleading guarantee and a new sensitive-state lifecycle. A later increment can add an explicit private state root and an atomic protocol before any public CLI is introduced.

## Residual threat model

The runner executes the selected file as the current user. The scratch directory and minimal environment are hygiene controls, not a filesystem or network sandbox. Pathname execution still has a check-to-exec race, launcher hashes do not bind an interpreter or native child, and descendants that escape the process group are not contained. Evidence remains unauthenticated, `evidence_authenticated` remains false, and `dispatch_authorized` remains false.

A public CLI is deferred. It cannot be added in this increment without making a user-selected executable, including an installed Codex launcher, reachable from the installed command surface. All executed tests use synthetic fake executables. Real Codex probing requires a separate user-approved milestone that first resolves cross-process state and reviews the residual same-user and operating-system isolation risks.
