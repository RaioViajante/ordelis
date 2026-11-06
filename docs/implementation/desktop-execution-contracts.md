# Execution lifecycle contracts

**Status:** Foundation. Implemented for [D3.2b-1 (#112)](https://github.com/RaioViajante/ordelis/issues/112), part of the [D3.2 design record (#108)](https://github.com/RaioViajante/ordelis/issues/108). Pure Rust in `desktop/src-tauri/src/lifecycle/`, with no command, no caller and no interface.
**Scope:** the vocabulary of runs, attempts, approvals and journal events that later steps will share. Nothing here starts, stops, finds or observes a process, reads a clock or a file, or persists anything.
**Related:** [Provider catalog and evidence vocabulary](desktop-agents.md) · [Execution Safety Gate](execution-safety-gate.md) · [Desktop architecture](desktop-architecture.md)

## Where execution contracts stand

| Part | State |
| --- | --- |
| Design (#108 decisions 10 and 11; D3.2b design r5) | Approved in principle. An event-sourced journal per run, history never authority, monotonic live windows, conservative reconciliation, a framed append log as the storage direction. Kept outside the repository; hashes on #108. |
| **Domain types and validation (D3.2b-1)** | **Implemented here.** Compiled into every build, used by nothing outside its tests. |
| Reducer, live layer and replay over events (D3.2b-2) | Not authorized. |
| Reconciliation and cross-run start conflicts (D3.2b-3) | Not authorized. |
| Codec and replay from bytes (D3.2b-4) | Not authorized. |
| Journal persistence, approval interface, execution boundary | Not authorized; each needs its own decision. |

No run can be created, stored or started today. A value of these types is a description, never permission: the live authority of later steps exists only in memory and is never built from them.

## Model (`lifecycle/model.rs`)

The minimum stable model of design r5, section 11.1:

- **Identities:** `RunId` (`r<n>`), `ApprovalId` (`a<k>`), `TaskId` (`t<n>`) and `ProjectId` (`p<n>`) follow the task and registry text rules (a prefix and 1 to 10 digits, no leading zero). `AttemptNo` is 1 to 32 and `Seq` starts at 1. `TaskRef` and `ProjectRef` carry a creation or registration time, so a reused id is never silently rebound, and `TaskRef` carries the task revision (at least 1).
- **Specification:** `ProviderIdentity` reuses the catalog's `ProviderId` and the evidence `Sha256` and `Token`. A grant binds only its `BindingTarget` (provider and executable hash), never the advertised version. `RunContext` holds the project, `WorkspaceMode::None` (no repository access in v1), sorted unique requested permission classes, `Limits`, a policy version and an optional environment digest. `RunSpec` adds the task and at most 8 started attempts.
- **Time:** `WallMs` (at most 2^53 − 1) is provenance only. `MonoMs` is a caller-supplied monotonic reading, meaningful within one process. They share no constructor or conversion, and nothing here reads either clock. `LiveEpoch` is an opaque value that can only be compared.
- **Limits:** `Limits` enforces start 1 s to 10 min, run 1 s to 24 h, stall from 30 s (3 × the progress coalescing window) to the run limit, approval lifetime 1 s to 900 s, and stop grace up to 60 s. An out-of-range value refuses the whole set; nothing is clamped.
- **Approvals:**
  - `ApprovalRequest::new` refuses a `start` request whose operation class is not `start`, and the reverse.
  - It refuses a lifetime outside 1 s to 900 s.
  - It computes the risk with `classify`: `start` is high and everything else is critical while there is no repository access. A provider's claim can only raise it.
  - `OperationDescription` is bounded (2048 bytes, no control character other than newline and tab). It is untrusted and sensitive, and it is memory-only in v1; events carry only its digest.
- **Events:** the 22 kinds of v1 and their bodies, which hold no wall-clock time. Every event has one or more semantic keys:
  - `StartBegun` occupies two keys: it begins the attempt and consumes the start grant.
  - `Progress` occupies none; it is coalesced instead.
  - Every way an attempt ends shares one key, so two terminal events can never both be recorded.
- **Producers:** `drafted_by` is the closed allow-list. The provider relay can request an operation approval, report a session or progress, and claim failure; nothing else.
- **Reserve and moves:** the reserved kinds keep room to stop, end, reconcile and resolve a run, and `JournalRepaired` is outside every limit. Only attempt events move an attempt.
- **States:** run, attempt and approval states.
  - Attempt outcomes are separated into classes: observed success, observed failure, observed stop, **unknown** and **never started**. Only an observed failure may be presented as a failure.
  - An unknown outcome counts against the started-attempt budget, because the attempt may have run.
  - A start failure is "never started".
- **Concerns:** only `PossibleOrphan` is blocking. Each kind has its own resolutions, and `Acknowledged` never closes an orphan. A `ConcernRef` names an attempt exactly when its kind is about one.
- **Codes:** closed `Rejection`, `JournalFault` and `CoalesceReason` sets for the reducer and replay of later steps.

### Differences from design r5, from its gate review

- **F9:** `WithdrawalReason` includes `ClockFault`.
- **F14:** `JournalFault` includes `ClampedMisused` and `MixedProducerFrame`.
- **F4:** `StartFailCode::EffectNotPerformed` also covers a failed high-water-mark write.
- **F7:** `ApprovalRequest` enforces kind and class consistency and computes the risk. The comparison of a request's `spec_digest` and target with the run's own needs the projection and is left to the reducer.

## Static boundary

`config.test.ts` holds the compiled `src/lifecycle*` files to the agents purity rules: no process, filesystem, network, environment, clock, thread, global state, unsafe code, FFI, Tauri, serde or libc; no command; nothing public outside the crate; no identifier that could hold a path, a search location, an argument vector or a command; and no macro other than `matches!`.

Decision 12.8 amends the rules for this module:

- It may reference the rest of the crate only through two whole-line imports in `lifecycle/model.rs`, each exactly once:
  - `use crate::agents::catalog::ProviderId;`
  - `use crate::agents::evidence::{Sha256, Token};`
- It may not name `super`, and it may name the agents module nowhere else.
- No identifier may suggest authorized, dispatched, enforced or terminated, in any letter case.
- The agents module never names the lifecycle model.
- `lib.rs` declares the module once, in the same pinned form as `mod agents;`, and no other compiled production file names it.

Bypass fixtures pin each amendment: an alias, a renamed module, a glob, a grouped import naming more, a re-export, a spaced path, a second copy, an import in the wrong file, a full path, an authority-like name, and the usual process, file, clock, environment and macro cases. `native-sources.json` lists the new files, so the compiler's dep-info check covers them. The ten commands, their grants, the generated permissions, the dependency set and the release-isolation verifier are unchanged.

## Tests

`lifecycle/model/tests.rs`, with synthetic values only:

- id text rules and number ranges;
- the wall-clock bound;
- every limit bound;
- permission order;
- the started-attempt budget;
- binding without the advertised version;
- risk classification and claims;
- kind and class consistency and lifetimes;
- description bounds in bytes and control characters;
- process identities and exit statuses;
- the producer allow-list for every kind;
- semantic keys;
- the reserve;
- attempt events;
- outcome classes;
- final states;
- concern classes and resolutions;
- repair sizes;
- duplicate content that ignores time and position;
- the relations between the frame, batch, tail and reserve limits.

## Limits

- This is vocabulary only. No transition is decided here; the reducer is the next step and is not authorized.
- Values may still change without a schema version bump until the first persistence adapter freezes v1.
- Open items from the design's gate review (recorded on #108) must be settled before b-2, b-3, b-4 or the adapter.
