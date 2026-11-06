# Execution lifecycle contracts

**Status:** Foundation. Implemented for [D3.2b-1 (#112)](https://github.com/RaioViajante/ordelis/issues/112), [D3.2b-2 (#114)](https://github.com/RaioViajante/ordelis/issues/114) and [D3.2b-3 (#116)](https://github.com/RaioViajante/ordelis/issues/116), part of the [D3.2 design record (#108)](https://github.com/RaioViajante/ordelis/issues/108). Pure Rust in `desktop/src-tauri/src/lifecycle/`, with no command, no caller and no interface.
**Scope:** the vocabulary of runs, attempts, approvals and journal events, the deterministic reducer of their state machines, the in-memory live authority of one supervised run, and conservative crash reconciliation with cross-run start conflicts. Nothing here starts, stops, finds or observes a process, reads a clock or a file, or persists anything.
**Related:** [Provider catalog and evidence vocabulary](desktop-agents.md) · [Execution Safety Gate](execution-safety-gate.md) · [Desktop architecture](desktop-architecture.md)

## Where execution contracts stand

| Part | State |
| --- | --- |
| Design (#108 decisions 10 and 11; D3.2b design r5) | Approved in principle. An event-sourced journal per run, history never authority, monotonic live windows, conservative reconciliation, a framed append log as the storage direction. Kept outside the repository; hashes on #108. |
| **Domain types and validation (D3.2b-1)** | **Implemented here.** |
| **Reducer and in-memory live authority (D3.2b-2)** | **Implemented here.** Compiled into every build, used by nothing outside its tests. |
| **Reconciliation, `begin_reconciled` and cross-run start conflicts (D3.2b-3)** | **Implemented here.** Used by nothing outside its tests; it reads caller-supplied histories and synthetic observations only. |
| Codec, framing, `replay` and `ReplayedHistory` (D3.2b-4) | Not authorized. |
| Journal persistence, approval interface, execution boundary | Not authorized; each needs its own decision. |

No run can be created, stored or started today. A value of these types is a description, never permission. Live authority exists only in memory (`live.rs`), is never built from a recorded event, and never starts anything: a grant records consent for one consumption and is never Execution Safety Gate satisfaction.

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
- **Producers:** `drafted_by` is the closed allow-list. The provider relay can request an operation approval, report a session or progress, and claim failure; nothing else. Since D3.2b-2 a draft is built only from a typed body per producer (below).
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

### Bounds stricter than design r5

r5 does not state these; the model adds them so that no value outside a sensible range can be built:

- an approval lifetime of at least 1 s (r5 gives only the upper bound);
- a non-empty operation description;
- an exit status with exactly one of an exit code and an ending signal number (1 to 127);
- sequence numbers and task revisions of at most 2^53 − 1, so they stay exact JSON numbers;
- a task revision of at least 1, as `tasks.json` v1 writes it;
- a process id other than 0;
- a repair that removed between 1 and `MAX_TAIL_BYTES` bytes.

### Names

- The `JournalRepaired` body is `JournalRepair`. The name `RepairRecord` stays free for b-3's reconciliation input.
- `SpecDigest` is an alias of `Sha256`.

## Producer construction (D3.2b-2)

`EventDraft` has private fields. A draft is built only from one typed body per producer, and its producer label is derived from that type; no caller can name a producer:

| Constructor | Body type | Can express |
| --- | --- | --- |
| `EventDraft::user` | `UserDraft` | run creation, attempt creation, approval decision, cancel, run close, concern resolution |
| `EventDraft::supervisor` | `SupervisorDraft` | start consent request (kind `start` only), start begun, child started, start failed, stop, exit, operation consumption, approval expiry, withdrawal, void |
| `EventDraft::relay` | `RelayDraft` | operation approval request (kind `operation` only), session observed, progress, failure claim |
| `EventDraft::reconciler` | `ReconcilerDraft` | interrupted, not started, approval withdrawal and void (its rules are D3.2b-3) |
| `EventDraft::journal` | `JournalDraft` | repair record |

- `RelayDraft` has no way to express a decision, a consumption, a lifecycle change, a resolution or a process exit, and the relay cannot ask for start consent (`WrongProducer`).
- The reducer still checks the allow-list for every draft and every recorded event, first (defense in depth). A recorded producer is a label: a frame whose label its body does not allow is a `ProducerNotAllowed` fault.
- An approval request's operation description travels beside the draft, in memory only; no event holds it.
- The one way around the typed constructors is `EventDraft::forged`, compiled only in test builds, so tests can prove the reducer refuses a forged label.

## Reducer (`lifecycle/reduce.rs`)

`apply(projection, drafts, size)` records a batch as one frame; `apply_recorded(projection, frame)` applies one frame of in-memory events exactly as recorded. Both use one transition function, work on a copy, and leave the projection untouched when they refuse. A `RunProjection` is history: what the journal says happened, never what may be done.

- **Check order**, per draft: producer, version and time, structure, a drafted consequence, semantic identity, limits and reserve, terminal run/attempt/approval, then transition legality including the batch rules. Every input has exactly one result: appended, `Duplicate`, `Coalesced` or one `Rejection`.
- **Time:** `at_ms = max(previous, stated)`; a lower or invalid stated time keeps the previous one, sets `clamped` and raises one `clock_irregular` concern until it is acknowledged. Only `RunCreated` refuses an invalid time. Windows recorded in wall time can only close early.
- **Identity:** a draft whose every semantic key maps to the same earlier event with the same producer and body is a `Duplicate` (nothing appended). Anything else on a held key is a `Conflict`. A batch mixing duplicates and new events is a `PartialDuplicate`. The same fact twice in one batch conflicts with itself.
- **Batch shapes:** one producer throughout; or exactly `[AttemptCreated (user), ApprovalRequested{start} (supervisor)]` for the same attempt (F10); or repair records, then (after at most two) reconciler drafts. Anything else is `BatchShape`.
- **Approval-driven moves:** a start approval's own outcome moves its attempt before anything runs, only from the user or the supervisor and only when the batch holds no attempt event for that attempt; when it does, the approval event is refused unless that batch also ends the attempt (F10). A reconciler approval event never moves an attempt.
- **Requests match their run (F7):** the specification digest, the binding target, the risk Ordelis recomputes, `ttl_ms ≤ min(approval_ttl_ms, 900 s)`, contiguous approval ids, the current attempt (`proposed` for start, `running` for operation), and at most 4 open and 64 total approvals per attempt.
- **Decisions and consumptions:** a decision needs the presented binding digest and must be recorded before `request time + ttl`; a consumption restates the exact binding and must be recorded before `min(decision + 60 s, request + 900 s)`.
- **Generated consequences:** right after an attempt event that ends or stops an attempt, one `ApprovalWithdrawn` per pending and one `ApprovalVoided` per granted approval of that attempt, in approval order, with the cause's producer, time and version, never clamped, and a reason fixed by the cause (`cancelled`, `clock_fault`, `supervision_lost` or `attempt_ended`). Replay requires the records after a cause to be exactly them.
- **Run outcomes:** derived immediately after the attempt event that causes them. A run-scope cancel moves the run to `closing` first, so `cancelled` wins over `exhausted` (F12). `exhausted` carries the outcome class of the last attempt that may have run.
- **Outcomes and concerns:** rule W (a provider can only turn a later zero exit into a reported failure); an exit while stopping needs the child's recorded identity and is decided by the first stop reason (F2); a start failure is `never_started` and raises no concern; every interruption raises a blocking `possible_orphan`; after a recorded repair, a not-yet-started attempt can end only as `interrupted{evidence_lost}`, never as "not started".
- **Progress:** coalesced (accepted, not appended) within 10 s of the attempt's last recorded progress, live and in replay; a stored progress the rule would have coalesced is a `CoalesceViolated` fault.
- **Limits and reserve:** inside the last 64 events or 512 KiB only reserved kinds are accepted; other provider-relayed drafts are `Coalesced`, other drafts `ReserveOnly`. At 4096 events or 4 MiB only a repair is accepted. A frame holds at most 8 events, generated ones included. Replay checks the reserve by event count only (F14).
- **Recorded envelope:** a known version that never decreases (`NewerVersion`, `VersionDecreased`), this run (`WrongRun`), the next `seq` (`SeqNotFirst`, `SeqGap`), a time that never decreases (`TimeDecreased`), and `clamped` only with the previous time (`ClampedMisused`). Expiry and void timing is not checked against `at_ms` in replay (F14).

## Live authority (`lifecycle/live.rs`)

| Type | Role | Rules |
| --- | --- | --- |
| `PendingSupervision` | A run being created: `begin_new` staged its `RunCreated` frame. | Holds no grant, accepts no draft. `open` makes it live after the frame is durable; `drop_pending` discards it. |
| `LiveSupervision` | The live projection of one run during one process lifetime, with its monotonic clocks, memory-only descriptions and outstanding grants. | Built only by `begin_new` and `begin_reconciled` (D3.2b-3), through one private constructor. One outstanding `Staged` at a time. Refuses every reconciler draft: a supervised run is never reconciled (r5 5.4). |
| `Staged` / `Abandoned` | A frame to write, and a frame whose write was uncertain. | `#[must_use]`. `confirm` (durable) or `abandon` (uncertain). After `abandon` the supervision is unresolved until `adopt` (the frame landed) or `discard` (it is absent) (F8). Each call accepts only the frame this supervision holds outstanding (same epoch, number, events and run, extending its current projection). A wholly duplicate or coalesced batch stages an empty frame: nothing is written, and it is still confirmed. |
| `LiveGrant` | The only value that can consume an approval. | Minted only by `confirm`, for a decision that frame newly recorded, inside its monotonic window, for a request this supervision recorded, whose approval is still granted after the frame (F13). Bound to the epoch, run, attempt, approval, kind and exact recorded request. Consumed by value by `stage`; returned inside the refusal when `stage` refuses. Valid until `min(decision + 60 s, request + 900 s)` on the monotonic clock. |

- None of these types derives or implements `Clone`, `Copy`, `Default`, `Debug` or serde, none has a field visible outside `live.rs`, and none is named by any other file. No conversion leads from an event, a projection or a recorded frame to any of them; `adopt` never mints.
- **Monotonic time:** every entry point that takes `mono` (`begin_new`, `stage`, `due`) checks that it never decreases. A regression puts the supervision in `clock_fault` for good: no decision, no consumption, no further grant; only stop, cancel, exit, start failure, expiry, withdrawal, void, the identity of a child that started meanwhile, and repairs (F2). An overflowing window or deadline is already closed.
- **`due`:** proposes, from monotonic stamps and never from `at_ms`, at most one draft per semantic key (F1): for a starting or running attempt only its stop, by `clock_fault` > `journal_full` > `deadline(start | run | stall)` (stall only without a pending approval); otherwise expiry by `clock_fault` > `monotonic` > `wall` and void by `clock_fault` or a passed use window. Every progress report counts for the stall clock, coalesced ones included.
- **No execution:** nothing turns a grant or a staged frame into a process start. `stage` with a grant records a consumption; starting anything is a future execution boundary that must also satisfy the Execution Safety Gate.

### Differences from design r5 in D3.2b-2

- `replay` over a whole journal and `ReplayedHistory` move to D3.2b-4 with the codec (#108 records "b-4: the codec and replay"). `apply_recorded` over one in-memory frame is here, so live and recorded projections are compared now.
- `begin_new` takes the run id, because a `RunCreated` body carries none.
- `confirm`, `abandon`, `adopt` and `discard` return a `LiveRefusal` for a frame that is not this supervision's outstanding one, compared by its events and projection as well as its epoch, so two supervisions given the same epoch by mistake cannot exchange frames.
- `stage` refuses reconciler drafts (`LiveRefusal::Reconciliation`); r5 states this only for the adapter (5.4).
- Three rejection codes make refusals precise: `BindingMismatch`, `WindowClosed` and `BlockingConcern`.
- `AttemptInterrupted` must fit what was recorded: `no_identity` exactly when no child identity was recorded, `evidence_lost` only after a repair.
- A `clock_irregular` concern can be raised by a provider-relayed event whose stated time was clamped. The stated time is Ordelis's own reading, never the provider's, so invariant I6 is read as "no relay event changes a lifecycle state, a decision or a concern from its content".

## Reconciliation (`lifecycle/reconcile.rs`, D3.2b-3)

Reconciliation is for a run that nobody supervises, after a restart. It reads a **caller-supplied** history and untrusted observations, and returns one opening frame, findings and recommendations. It never creates authority, never concludes "never started" after a detected repair, never ends an attempt as succeeded or failed, and never stops anything.

### Inputs

- **`RecordedHistory`**: a projection rebuilt from recorded frames, a `HistoryStatus` (`Verified`, or `Quarantined(Fault(JournalFault) | Marker)`), and the repair records the journal still lacks. It stands in for b-4's `ReplayedHistory`: until the codec and replay exist, the caller states the status, and the core cannot check it.
- **`AttemptObservation`**: the attempt, the identity the observer looked for, what it saw (`gone_confirmed`, `pid_absent`, `pid_reused`, `identity_matches`, `unobservable`), its epoch and its monotonic time. Untrusted: an observation can only choose between kinds of unknown.
- **`ReconcileInput`**: the epoch of the supervision to open, its monotonic time (for observation age), the wall-clock reading its drafts state, and the journal size.

### Rules (r5 7.2)

| Attempt not terminal at the crash | Draft (reconciler) | Concerns | Recommendation |
| --- | --- | --- | --- |
| `proposed` or `approved`, journal intact since its last lifecycle event | `AttemptNotStarted{supervision_lost}` | none | `RetryPossible`, if the run stays open with no blocking concern |
| `proposed` or `approved`, a repair recorded after its last change or reported now | `AttemptInterrupted{evidence_lost}` | blocking `possible_orphan`, `workspace_unknown` | `ReviewAttempt` |
| `starting`, or `stopping` without identity | `AttemptInterrupted{no_identity}` | as above | `ReviewAttempt`, never a proposal (no identity) |
| started, fresh matching `pid_absent` or `pid_reused` | `AttemptInterrupted{not_running}` | as above | `ReviewAttempt` |
| started, fresh matching `identity_matches` | `AttemptInterrupted{possibly_running}` | as above | `ReviewAttempt` and `TerminationProposal` |
| started, anything else | `AttemptInterrupted{unknown}` | as above | `ReviewAttempt` |

- **Observations count only when fresh and matching:** the same epoch, taken no later than the reconciliation's time and at most `MAX_OBSERVATION_AGE_MS` (5 s) before it, for exactly the recorded identity. Missing, stale, mismatched or contradictory observations, and `gone_confirmed` (no containment in v1), give `unknown`. A missing, stale or mismatched observation never establishes that the process did not run.
- **Approvals:** the reconciler drafts only the attempt event. Pending requests are withdrawn and unused grants voided by the reducer's generated consequences (`supervision_lost`). A recorded decision stays history.
- **The frame:** the missing repair records first (journal producer, at most two; more is `TooManyRepairs`, so the adapter quarantines), then at most one reconciler draft. No stop, cancel or signal is ever drafted.
- **Findings** (`Quarantined`, `ObservationMissing`, `ObservationStale`, `ObservationMismatched`, `ObservationUnexpected`, `ObservationsContradictory`, `ConfirmationUnsupported`) are informational display notes, sorted and without repeats. None blocks, clears or permits anything.
- **Concerns stay apart:** the result lists the run's unresolved **blocking** concerns and its unresolved **informational** concerns separately. A blocking `possible_orphan` stays until a user `AttentionResolved` with a resolution its kind allows (`confirmed_gone`, `stopped_outside_ordelis`, `accepted_risk`), naming it exactly and carrying its provenance; a later observation, a later reconciliation or any other producer never clears it.
- **Quarantine:** a quarantined history yields no draft, a `Quarantined` finding, and `ReviewAttempt` for an attempt in flight.
- **Deterministic and idempotent:** the same inputs give the same frame, whatever the order of the observations. Re-applying the frame is a `Duplicate`; reconciling a history that holds its frame drafts nothing. A crash before the frame is written leaves the history as it was, and reconciling again gives the same frame.
- **Refusals** (`ReconcileRefused`): no `RunCreated` (the adapter classifies a run with no valid frame, F6), more than two missing repairs, a reported repair whose sidecar is recorded with other content or reported twice, or a frame the reducer refuses. The run then stays unreconciled, which blocks every start.

### Termination proposals

`Recommendation::TerminationProposal { attempt, basis: PidStartTokenAndHash, policy: SeparatePolicy }` is plain data: no process id, no method, no trait beyond `Clone, Copy, Debug, PartialEq, Eq`, no consumer. The list of recommendations is a private field of `Reconciliation` with a test-only accessor, so no production code reads a recommendation back (showing them belongs to a later, separately reviewed interface); acting on one needs a separate, future policy that re-verifies the process independently (#108 decision 13). Nothing is ever terminated automatically.

## Opening a reconciled run (`begin_reconciled`)

`begin_reconciled(epoch, history, frame, mono, size)` is the second door to a `LiveSupervision`:

- It refuses a quarantined history (`Quarantined`), a history without `RunCreated` (`NoRun`), a frame computed for another history (another status, projection or set of missing repairs: a frame of a quarantined history cannot open the same projection relabelled as verified), another epoch, or a monotonic time earlier than the frame's (`NotThisHistory`; a later time is accepted however late, since every interruption blocks anyway), a frame with any draft other than a repair or a reconciler draft (`NotReconciliation`), a frame the reducer refuses (`Rejected`), and any result that is not **quiescent** (`NotQuiescent`): an attempt not terminal, a pending request, an unused grant, or a reported repair left unrecorded.
- The frame (`ReconciledFrame`) is built only by `reconcile`, has private fields, cannot be cloned and is consumed by value, so each frame opens at most once, for the history and epoch it was computed for. Reconciling the same history twice gives two frames: that one run has at most one supervision is the adapter's lock (r5 5.3), which the core cannot see.
- It applies the frame through the reducer itself. Regular `stage` still refuses every reconciler draft.
- It returns `Opening::Ready` when there is nothing to write, or `Opening::Pending` with the frame to write; `open` makes it live and `drop_pending` discards it.
- **It never mints a grant.** The new supervision starts with no approval clock, description or outstanding grant. A historical decision cannot be staged again (this supervision has no window for that request), a historical grant cannot be consumed, and any later attempt needs a new request and a new decision staged and confirmed by this supervision.
- **No durability is faked.** There is no persistence adapter: `open` after a durable write and `drop_pending` after an uncertain one are in-memory contracts for a future trusted caller, which must also hold the run's lock, check markers, sidecars and the high-water mark, and replay the whole journal before calling it (r5 5.7).

## Cross-run start conflicts (`start_conflicts`, F3)

`start_conflicts(inventory, supervised)` returns every reason no start may happen now, anywhere, sorted and without repeats. An empty result says only that the supplied data shows no conflict: it is **not** a start authorization, and the future execution boundary must still refuse to record a start while any conflict exists and apply its own policy and the Execution Safety Gate.

- **Closed summaries:** `RunSummary` is `NeverCreated`, `Quarantined(reason)`, `Unverified(Missing | Unreadable | HeldElsewhere | MarkMismatch | NoRunCreated)` or `Verified`. A verified summary is built only by `summarize(history, read)` from a verified history, and records how it was read (`Locked`, `LockFree`, `DerivedIndex`).
- **Enumeration from the counter:** `RunInventory::Enumerated { allocated, summaries }` must account for every id `r1` to `r<allocated>` exactly once. `RunInventory::Unknown` (the counter or marks unreadable), an allocation beyond the id range, a gap (`Unaccounted`), a second summary, an id beyond the counter, a summary of another run, or a supervised run that is missing or said never to have been created (`Contradictory`) all block. A directory listing that lost a journal cannot clear it.
- **Supervised runs** are named by `SupervisedRun` values, which only `LiveSupervision::held` makes and which borrow that opened supervision, so a pending or dropped supervision cannot be claimed. A claim excuses the run's attempt in flight only when that supervision's own recorded history extends the summarized one; a supervision opened under a reused id, or two claims for one run, are `Contradictory` and excuse nothing. A claimed supervision's own unresolved blocking concerns are conflicts too, since it may be ahead of every summary (an orphan its opening frame just raised). That a supervision is the run's only one remains the adapter's lock and counter.
- **Blocking conditions** (global in v1): an unresolved possible orphan (or any other unresolved blocking concern, `BlockingConcern`, for a run-level kind a later version might add), a quarantine for any reason, an unverified run, a read without the lock or from a derived index (it can add conflicts, never clear them), a repair record the journal still lacks, and an attempt not terminal in a run the caller does not supervise (never reconciled, a failed reconciliation append, or another instance).
- **No cross-journal order:** nothing compares times or sequence numbers across runs; the result does not depend on the order of the summaries.
- **Projects do not scope it:** a new run id, or a project removed and registered again, does not escape a conflict in another run.

**Global-blocking decision (r5 12.18), evaluated and kept.** Narrower scopes (same task, same project, same workspace) need an identity the core cannot see: a `ProjectRef` changes when a folder is removed and registered again, and workspaces are D3.3. The cost is accepted: one unresolved orphan, unverified or unreconciled run, or quarantine stops every start until the user resolves it or a recovery policy applies. A separately approved recovery and archive policy for quarantined runs remains a prerequisite of the execution boundary. Any change to this policy needs its own architectural approval.

### Differences from design r5 in D3.2b-3

- `reconcile` takes a `RecordedHistory` (projection, status and missing repairs) instead of b-4's `ReplayedHistory` and a separate repairs list, so `begin_reconciled` can check that no repair was dropped. `ReconcileInput` also carries the journal size.
- `begin_reconciled` takes the `ReconciledFrame` (bound to its history, epoch and monotonic time) rather than the whole `Reconciliation`, so the live layer never names a recommendation; it also takes the journal size. The opening frame is applied by `begin_reconciled` through the reducer, never by `stage`.
- `start_conflicts` takes a counter-enumerated `RunInventory` instead of a list of other runs, and `SupervisedRun` values borrowed from opened supervisions instead of plain run ids; it adds `NoRunCreated`, `Unaccounted`, `Contradictory`, `UnverifiedRead`, `BlockingConcern` and `RepairPending` to the F3 classification.
- `close_run` is not a recommendation: no rule of r5 7.2 produces it. r5's `requires_policy: true` is the one-value enum `PolicyRequirement::SeparatePolicy`, and the observation field `seen` has the type `Seen`.
- A late `AttemptExited` after an interruption is refused as a `Conflict` (identity is checked before the terminal state, r5 4.4), where r5 7.3 says `AttemptTerminal`; a late relayed report is `AttemptTerminal`. Both are refused and change nothing.
- `JournalFault` is ordered, so that conflicts sort deterministically; `AttemptRecord` exposes its last lifecycle event for the repair guard.

## Static boundary

`config.test.ts` holds the compiled `src/lifecycle*` files to the agents purity rules: no process, filesystem, network, environment, clock, thread, global state, unsafe code, FFI, Tauri, serde or libc; no command; nothing public outside the crate; no identifier that could hold a path, a search location, an argument vector or a command; and no macro other than `matches!`.

Decision 12.8 amends the rules for this module:

- It may reference the rest of the crate only through two whole-line imports in `lifecycle/model.rs`, adjacent and in this order, as its first lines of code (only line comments before them, so no `pub(crate)` or attribute can prefix them):
  - `use crate::agents::catalog::ProviderId;`
  - `use crate::agents::evidence::{Sha256, Token};`
- Any other copy or position is held to the full rules and fails.
- The module root names no `super`; a submodule names `super` only to reach a sibling (`super::model`, `super::reduce`, `super::live`, `super::reconcile`; decision 12.8(b), D3.2b-2 and D3.2b-3). No file imports with a glob. It may name the agents module nowhere else.
- No identifier may contain authoriz/authoris, dispatch, enforc, terminat or execute_now, in any letter case and at any position.
- The agents module never names the lifecycle model.
- `lib.rs` declares the module once, in the same pinned form as `mod agents;`, and no other compiled production file names it.

Bypass fixtures pin each amendment:

- an alias, a renamed module, a glob, a grouped import naming more;
- a re-export, including one split over two lines;
- a spaced path, a second copy, an import after other code, the imports swapped, an import in the wrong file, a full path;
- authority-like names as a prefix, inside a name and as a type;
- the usual process, file, clock, environment and macro cases.

D3.2b-2 adds pins for live authority and draft construction, each with a bypass fixture:

- `LiveGrant`, `LiveSupervision`, `PendingSupervision`, `Staged` and `Abandoned` are declared once in `live.rs` with no attribute but `#[must_use]`, have no visible field, have no trait implemented for them anywhere, and are named by no other file.
- A `LiveGrant` and a `LiveSupervision` are each built in exactly one place. Only `stage` (and its private check) takes a grant; only `confirm` (through its private `settle` and `mint`) returns grants, whatever the signature looks like.
- `EventDraft` has no visible field and is built in one place; no other file builds one; `forged` is test-only.
- No lifecycle file builds a value through `Self { … }`, aliases or renames a guarded type, or implements a trait for one through a path; no `use` tree holds a glob.
- Fixtures: a cloneable grant, a formattable supervision, a trait implemented by hand, through an alias or through a path, a visible field, a second construction site of any live type, a trait for a parenthesized type, construction through `Self` (a grant twin, a staged frame, a labelled draft), another function taking or returning a grant (plain, generic, nested generics, after a tuple parameter), a renamed grant, an aliased draft, a reducer naming a grant, a visible draft field, a draft built elsewhere, a forging helper in production; `super` from the root, to the crate root, to another crate module, spaced, aliased, as a glob or as a glob inside braces; and a process, a clock, a sleep, a launch word or serde in the new files.

D3.2b-3 adds one reviewed exception and the pins that keep reconciliation inert:

- **The termination-proposal exception.** The `terminat` rule is unchanged. Exactly the identifier `TerminationProposal` is set aside, only in `lifecycle/reconcile.rs`, and only when it occurs exactly twice: in the pinned declaration of `Recommendation` (its derives, its three variants and the proposal's three data fields, an attempt number and two one-value enums whose declarations are pinned too), and in the one place it is built and pushed. Any other spelling, letter case, prefix, suffix, raw identifier, file, field, derive or count leaves the rule in force. The test also checks that the file would fail without the exception.
- **Inert recommendations.** `Recommendation`, `ProposalBasis` and `PolicyRequirement` have no method, no trait implementation, no alias or rename, and no function takes one; every mention of `Recommendation` in `reconcile.rs` is its declaration, the one private field that holds them, a value pushed onto it, or the test-only accessor, and every mention of that field is pinned the same way; no other production file names these types or `recommendations` at all.
- **Printable ASCII.** Lifecycle code (comments and strings aside) is printable ASCII only, so a look-alike letter cannot carry a forbidden word past the rules.
- **Frames and histories.** `ReconciledFrame` and `RecordedHistory` are declared once in `reconcile.rs` with no attribute (no derive) and no visible field, are built once, have no trait implemented for them anywhere, and are never aliased, renamed or built by another file. `Opening` is declared once in `live.rs` with no attribute, has no trait, and is named by no other file. `SupervisedRun` is declared once in `live.rs` with no attribute and no visible field, is built only by `held`, and has no trait. `LiveSupervision` and `PendingSupervision` stay built in one place each, now shared by both doors.
- Fixtures: the proposal in snake, upper and lower camel case, with a prefix, a suffix, a digit or an underscore, as a raw identifier, mentioned a third time (a `matches!`), imported under an alias, in the live layer, the reducer, the module root or the model, assembled by a macro, carrying an action or a process id, built a second time; a termination method or state, a kill, a signal, a process, a clock, a process-table read, a command, serde or a `super` escape beside it; a method, a trait or another derive on `Recommendation`, another variant, a function taking one, a match on one, a recommendation read back, the list counted in the live layer, in reconciliation or through the accessor, a recommendation pushed into another list, a production accessor, a visible list, named by the live layer or the reducer, aliased or renamed; a look-alike letter in a forbidden word; a supervised run claimed without a supervision, built twice, cloneable or with a visible field; a cloneable frame or history, a visible frame field, a frame or history built twice or by the live layer, a trait for or an alias of a frame; a cloneable opening, a trait for one, one named by reconciliation, a supervision built a second time.

`native-sources.json` lists the new files, so the compiler's dep-info check covers them. The ten commands, their grants, the generated permissions, the dependency set and the release-isolation verifier are unchanged.

## Tests

All synthetic: no file, clock, process, network, provider or VM.

`lifecycle/reduce/tests.rs` (36 tests):

- the attempt transition table: every attempt state (12 fixtures) × every event kind, with the exact result (moved, unchanged, duplicate or refused), and each result replayed from its frames to the same projection;
- every kind under every producer it does not allow, refused first;
- typed constructors: the relay cannot ask for start consent; forged labels on recorded events are faults;
- run specification immutability, request-to-run matching (F7), batch shapes (F10), atomicity, duplicates, conflicts and partial duplicates;
- generated consequences, their order, producer and reasons, drafted consequences, and recorded consequences that differ;
- approval-driven moves and their suppression, decision provenance and windows, single use and the 900 s cap;
- operation consumption, executable identity, first stop reason, rule W, start failures, orphans and resolutions, repairs and `evidence_lost`;
- run outcomes (`succeeded`, `exhausted`, `cancelled` over `exhausted`, `closing`, `closed`), unknown outcomes, resume continuations;
- clamping and absurd wall readings, `Progress` coalescing live and recorded, the reserve and hard limits, approval limits, and every recorded-envelope fault.

`lifecycle/reconcile/tests.rs` (25 tests): crash before the start, with an intact journal and after a recorded or reported repair; crash after a possible start without identity; every observation kind; stale (another epoch, from the future, one millisecond too old, the exact age limit), mismatched (pid, start token, executable), contradictory and duplicate observations, and observations of ended or unknown attempts; pending and unused approvals ended only by generated consequences; a closing run; quarantine by a stored double ending and by a marker; determinism under reordered observations and idempotence (re-applied frame, reconciled result, crash before the write); late and conflicting events; refusals; `begin_reconciled` (`Ready`, `Pending`, every refusal including forged frames and a quarantined frame relabelled as verified, a sidecar reported twice, two frames of one history opening twice as the adapter-lock boundary); no historical grant or decision usable, fresh consent for a new attempt; a blocking orphan kept until an exact, allowed, user-provenanced resolution, while informational concerns never block; frames with no stop and inert recommendations; and `start_conflicts` (global orphan block across run ids and a re-registered project, every quarantine and unverified kind, lock-free and index reads, unreconciled runs, owner and impostor supervisions, a supervision ahead of its summary, pending and twice-reported repairs, unknown and incomplete inventories, contradictions, order independence, informational concerns).

`lifecycle/live/tests.rs` (24 tests):

- the only doors (`begin_new`, `open`, `drop_pending`); reconciler drafts refused, also under `clock_fault`; frames of another supervision with the same epoch refused; minting once per fresh decision; consumption by value; grants for another approval, another supervision, nothing, or two consumptions; the grant returned on refusal;
- monotonic expiry and voiding, the 900 s cap, decision windows, no grant for a frame that voids its approval (F13), adopted decisions that mint nothing, uncertain writes and the unresolved state (F8), dropped consumptions;
- clock regressions at `due` and `stage`, safety drafts and the child identity under `clock_fault` (F2), deadline and expiry precedence (F1), the wall clock only closing windows, coalesced progress and the stall clock, overflow, memory-only descriptions;
- **seeded property test** (decision 12.14): a hand-rolled SplitMix64 generator, 400 seeds × 160 steps, drives drafts from every producer, forged labels, clock regressions, absurd and backward wall readings, reserve sizes, wrong grants and uncertain writes through `stage`/`confirm`/`abandon`/`adopt`/`discard`. After every step it checks invariants I1–I14 of r5 section 9, that grants come only from fresh granted decisions, that reconciler drafts are always refused, that `due` never repeats a key and always stages as one batch, and that the recorded frames alone rebuild the live projection. Each seed runs twice and must give the same journal. The run asserts that every kind a supervised run can record is reached (the reconciler-only ends are covered by the reducer tests). Seeds that ever fail are kept in `REGRESSION_SEEDS` (none has failed in the final implementation).
- **seeded crash-reconciliation property test** (D3.2b-3): 300 seeds. The same generator runs a supervision to a random crash point (more often while a child identity is recorded); only the recorded frames survive. They are reconciled with random repair records and random observations (fresh, stale, from the future, mismatched, of other attempts, contradictory), at a new monotonic time, under a new or (against the caller's promise) reused epoch. It checks determinism under reordered observations; a frame of repairs then at most one reconciler end, with no stop; the attempt in flight ended and only it; never "not started" after a repair; a proposal exactly for a fresh matching identity; the unreconciled run blocking starts; `begin_reconciled` refusing another epoch and opening this one with no grant, clock, description or frame from before; the opening frame replaying; quiescence; invariants I1–I14 across the restart; every earlier concern kept with its resolution; the orphan blocking; idempotence; and, once supervised, conflicts only from unresolved orphans. The reopened supervision is then driven further while the generator still holds the dead supervision's grants: no approval from before the crash ever becomes usable again. The run asserts that all six reconciliation outcomes, both openings and proposals are reached. Seeds that ever fail are kept in `CRASH_REGRESSION_SEEDS` (none has failed).

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

- Nothing calls the reducer, the live layer or reconciliation outside tests. No journal exists on disk, no approval interface exists, nothing observes a process, and nothing can start or stop one.
- Values may still change without a schema version bump until the first persistence adapter freezes v1.
- Rust privacy holds against safe code in the crate, not against code that chooses to use the user or supervisor constructors: each future caller (approval interface, spawn boundary, provider relay) must be limited to its own constructor by review and static pins when it is added.
- The core cannot observe durability, honest digests, a unique `LiveEpoch` or a true `JournalSize`; callers must call `confirm` only after a durable write and `adopt` only after a byte-exact check (r5 section 8).
- Reconciliation trusts its caller for the history's status and missing repairs, the observations' provenance, a unique epoch, and that no live supervision holds the run it reconciles (r5 5.4); `start_conflicts` trusts the caller's inventory and `NeverCreated` claims (a supervised run must be backed by an opened supervision whose history extends the summary). A false caller can hide a blocking condition, never create a grant.
- The model module allows dead code in test builds, because parts of the vocabulary serve only later increments.
- **D3.2b-4:** the canonical codec, framing and CRC-32C, `decode_stream` with version peeking before strict decoding (F14), `verify_persisted`, `scan_after_corruption`, `replay` and the read-only `ReplayedHistory` (which must produce the `RecordedHistory` reconciliation reads, with its status derived rather than stated), and golden v1 fixtures.
- **Adapter:** F4, F5, F6 and F15 from the gate review (frame boundary at the mark, atomic sidecars, zero-frame runs, a sleep-inclusive clock or an explicit `clock_fault` entry point); the run lock for the lifetime of a supervision; durable opening frames before `open`; the run-id counter, marks and never-created markers that `RunInventory` and `RunSummary` are built from, under the lock for anything that clears a conflict; the quarantine recovery and archive policy (12.18) before any execution boundary.
- **Execution boundary** (not authorized): refusing to record a start while `start_conflicts` reports anything, with no bypass, plus its own release policy and the Execution Safety Gate (#108 decisions 3, 5, 11, B, C).
