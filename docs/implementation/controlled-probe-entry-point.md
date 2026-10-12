# Controlled zero-model-call probe entry point

Status: accepted for issue #15; its process-local replay decision is superseded by the
[private ledger decision](private-authorization-ledger.md) for issue #17.

## Decision

`runtime.probe_confirmation` owns the only supported probe entry point, `run_controlled_probe`. It accepts an exact `ProbeAuthorization`, validates it, obtains terminal confirmation, consumes that confirmation in the same process, and invokes the private runner. The entry point remains library-only. No `mco` command exposes it in this increment.

The authorization binds the canonical executable path and SHA-256 produced by the existing executable-identity inspection, plus the scratch parent, explicit private state root, protected roots, checkout, limits, execution mode, zero model calls, lifetime, probe id, and the three fixed command vectors. It accepts no caller-provided command arguments. Path validation reuses identity-based overlap checks, rejects symlinked or non-canonical executable, scratch and state paths, and rejects overlap among the checkout, state root, scratch parent and protected roots.

Terminal confirmation displays every bound value and requires the exact digest-derived phrase. Dynamic display values and the complete prompt have hard size limits; an exact value that cannot be displayed within those limits is rejected instead of truncated. The controlling terminal is only the channel for the challenge and response. Its existence is not evidence that a human is present.

After confirmation, a process-local single-use nonce carries the approved digest to the private runner. The runner accepts only the module-owned capability object by identity, atomically commits the probe id and digest to the private SQLite ledger, then creates scratch and executes argument arrays without a shell. It revalidates the executable path, file state, and hash before every allowlisted command. `ProbeAuthorization` and the confirmation value must have the exact expected types; subclasses are rejected because overridden attribute access can be mutable or stateful.

The supported surface is `ProbeAuthorization`, `ProbeError`, and `run_controlled_probe`. Confirmation splitting, nonce handling, command execution, and the raw runner remain internal implementation details. Python underscores, module placement, exact-type checks, and a private capability object prevent accidental use; they do not stop hostile code running as the same user from importing internals, reading module state, monkey-patching checks, writing to the terminal, or executing the target directly.

## Process and replay behavior

Pending approvals exist only in process memory. Concurrent prompts are serialized and each confirmation nonce is consumed once. Consumed probe ids are permanent rows in a private SQLite ledger: an immediate transaction and primary-key insert allow only one cooperating process to commit an identity. The commit occurs after confirmation and before scratch creation. A command failure, cancellation, runner exception or crash cannot undo it.

The ledger stores only the probe id, exact-specification digest and two timestamps. It never prunes rows; fixed row and byte limits fail closed instead of making an old id reusable. Missing initialized state, corruption, unsafe ownership or permissions, unexpected files and unknown schemas deny. Backup, recovery, reset, local-filesystem assumptions and the unavoidable reset/epoch tradeoff are specified in the [ledger ADR](private-authorization-ledger.md).

## Residual threat model

The runner executes the selected file as the current user. The scratch directory and minimal environment are hygiene controls, not a filesystem or network sandbox. Pathname execution still has a check-to-exec race, launcher hashes do not bind an interpreter or native child, and descendants that escape the process group are not contained. Evidence remains unauthenticated, `evidence_authenticated` remains false, and `dispatch_authorized` remains false.

A public CLI is deferred. It cannot be added in this increment without making a user-selected executable, including an installed Codex launcher, reachable from the installed command surface. All executed tests use synthetic fake executables. Real Codex probing requires a separate user-approved milestone that reviews the private ledger location plus the residual same-user and operating-system isolation risks.
