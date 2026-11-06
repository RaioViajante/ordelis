# Execution lifecycle contracts

**Status:** Foundation. Implemented for [D3.2b-1 (#112)](https://github.com/RaioViajante/ordelis/issues/112) and [D3.2b-2 (#114)](https://github.com/RaioViajante/ordelis/issues/114), part of the [D3.2 design record (#108)](https://github.com/RaioViajante/ordelis/issues/108). Pure Rust in `desktop/src-tauri/src/lifecycle/`, with no command, no caller and no interface.
**Scope:** the vocabulary of runs, attempts, approvals and journal events, the deterministic reducer of their state machines, and the in-memory live authority of one supervised run. Nothing here starts, stops, finds or observes a process, reads a clock or a file, or persists anything.
**Related:** [Provider catalog and evidence vocabulary](desktop-agents.md) · [Execution Safety Gate](execution-safety-gate.md) · [Desktop architecture](desktop-architecture.md)

## Where execution contracts stand

| Part | State |
| --- | --- |
| Design (#108 decisions 10 and 11; D3.2b design r5) | Approved in principle. An event-sourced journal per run, history never authority, monotonic live windows, conservative reconciliation, a framed append log as the storage direction. Kept outside the repository; hashes on #108. |
| **Domain types and validation (D3.2b-1)** | **Implemented here.** |
| **Reducer and in-memory live authority (D3.2b-2)** | **Implemented here.** Compiled into every build, used by nothing outside its tests. |
| Reconciliation, `begin_reconciled` and cross-run start conflicts (D3.2b-3) | Not authorized. |
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
| `LiveSupervision` | The live projection of one run during one process lifetime, with its monotonic clocks, memory-only descriptions and outstanding grants. | Built only by `begin_new` (D3.2b-3 adds `begin_reconciled`). One outstanding `Staged` at a time. |
| `Staged` / `Abandoned` | A frame to write, and a frame whose write was uncertain. | `confirm` (durable) or `abandon` (uncertain). After `abandon` the supervision is unresolved until `adopt` (the frame landed) or `discard` (it is absent) (F8). |
| `LiveGrant` | The only value that can consume an approval. | Minted only by `confirm`, for a decision that frame newly recorded, inside its monotonic window, for a request this supervision recorded, whose approval is still granted after the frame (F13). Bound to the epoch, run, attempt, approval, kind and exact recorded request. Consumed by value by `stage`; returned inside the refusal when `stage` refuses. Valid until `min(decision + 60 s, request + 900 s)` on the monotonic clock. |

- None of these types derives or implements `Clone`, `Copy`, `Default`, `Debug` or serde, none has a field visible outside `live.rs`, and none is named by any other file. No conversion leads from an event, a projection or a recorded frame to any of them; `adopt` never mints.
- **Monotonic time:** every entry point that takes `mono` (`begin_new`, `stage`, `due`) checks that it never decreases. A regression puts the supervision in `clock_fault` for good: no decision, no consumption, no further grant; only stop, cancel, exit, start failure, expiry, withdrawal, void, the identity of a child that started meanwhile, and repairs (F2). An overflowing window or deadline is already closed.
- **`due`:** proposes, from monotonic stamps and never from `at_ms`, at most one draft per semantic key (F1): for a starting or running attempt only its stop, by `clock_fault` > `journal_full` > `deadline(start | run | stall)` (stall only without a pending approval); otherwise expiry by `clock_fault` > `monotonic` > `wall` and void by `clock_fault` or a passed use window. Every progress report counts for the stall clock, coalesced ones included.
- **No execution:** nothing turns a grant or a staged frame into a process start. `stage` with a grant records a consumption; starting anything is a future execution boundary that must also satisfy the Execution Safety Gate.

### Differences from design r5 in D3.2b-2

- `replay` over a whole journal and `ReplayedHistory` move to D3.2b-4 with the codec (#108 records "b-4: the codec and replay"). `apply_recorded` over one in-memory frame is here, so live and recorded projections are compared now.
- `begin_new` takes the run id, because a `RunCreated` body carries none.
- `confirm`, `abandon`, `adopt` and `discard` return a `LiveRefusal` for a frame that is not this supervision's outstanding one.
- Three rejection codes make refusals precise: `BindingMismatch`, `WindowClosed` and `BlockingConcern`.
- `AttemptInterrupted` must fit what was recorded: `no_identity` exactly when no child identity was recorded, `evidence_lost` only after a repair.
- A `clock_irregular` concern can be raised by a provider-relayed event whose stated time was clamped. The stated time is Ordelis's own reading, never the provider's, so invariant I6 is read as "no relay event changes a lifecycle state, a decision or a concern from its content".

## Static boundary

`config.test.ts` holds the compiled `src/lifecycle*` files to the agents purity rules: no process, filesystem, network, environment, clock, thread, global state, unsafe code, FFI, Tauri, serde or libc; no command; nothing public outside the crate; no identifier that could hold a path, a search location, an argument vector or a command; and no macro other than `matches!`.

Decision 12.8 amends the rules for this module:

- It may reference the rest of the crate only through two whole-line imports in `lifecycle/model.rs`, adjacent and in this order, as its first lines of code (only line comments before them, so no `pub(crate)` or attribute can prefix them):
  - `use crate::agents::catalog::ProviderId;`
  - `use crate::agents::evidence::{Sha256, Token};`
- Any other copy or position is held to the full rules and fails.
- The module root names no `super`; a submodule names `super` only to reach a sibling (`super::model`, `super::reduce`, `super::live`; decision 12.8(b), D3.2b-2). No file imports with a glob. It may name the agents module nowhere else.
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
- A `LiveGrant` and a `LiveSupervision` are each built in exactly one place. Only `stage` (and its private check) takes a grant; only `confirm` (and its private mint) returns grants.
- `EventDraft` has no visible field and is built in one place; no other file builds one; `forged` is test-only.
- Fixtures: a cloneable grant, a formattable supervision, a trait implemented by hand, a visible field, a second construction site, another function taking or returning a grant, a reducer naming a grant, a visible draft field, a draft built elsewhere, a forging helper in production; `super` from the root, to the crate root, to another crate module, spaced, aliased or as a glob; and a process, a clock, a sleep, a launch word or serde in the new files.

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

`lifecycle/live/tests.rs` (21 tests):

- the only doors (`begin_new`, `open`, `drop_pending`); minting once per fresh decision; consumption by value; grants for another approval, another supervision, nothing, or two consumptions; the grant returned on refusal;
- monotonic expiry and voiding, the 900 s cap, decision windows, no grant for a frame that voids its approval (F13), adopted decisions that mint nothing, uncertain writes and the unresolved state (F8), dropped consumptions;
- clock regressions at `due` and `stage`, safety drafts and the child identity under `clock_fault` (F2), deadline and expiry precedence (F1), the wall clock only closing windows, coalesced progress and the stall clock, overflow, memory-only descriptions;
- **seeded property test** (decision 12.14): a hand-rolled SplitMix64 generator, 400 seeds × 160 steps, drives drafts from every producer, forged labels, clock regressions, absurd and backward wall readings, reserve sizes, wrong grants and uncertain writes through `stage`/`confirm`/`abandon`/`adopt`/`discard`. After every step it checks invariants I1–I14 of r5 section 9, that grants come only from fresh granted decisions, that `due` never repeats a key and always stages as one batch, and that the recorded frames alone rebuild the live projection. Each seed runs twice and must give the same journal. The run asserts that every event kind is reached. Seeds that ever fail are kept in `REGRESSION_SEEDS` (none has failed in the final implementation).

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

- Nothing calls the reducer or the live layer outside tests. No journal exists on disk, no approval interface exists, and nothing can start a process.
- Values may still change without a schema version bump until the first persistence adapter freezes v1.
- Rust privacy holds against safe code in the crate, not against code that chooses to use the user or supervisor constructors: each future caller (approval interface, spawn boundary, provider relay) must be limited to its own constructor by review and static pins when it is added.
- The core cannot observe durability, honest digests, a unique `LiveEpoch` or a true `JournalSize`; callers must call `confirm` only after a durable write and `adopt` only after a byte-exact check (r5 section 8).
- The `terminat` rule also matches r5's b-3 type `termination_proposal`. b-3 needs a recorded, reviewed amendment for that one name (for example `terminat(?!ion_proposal)`), never a silent loosening.
- The model module allows dead code in test builds, because parts of the vocabulary serve only later increments.
- **D3.2b-3:** `reconcile`, observation validation, `Recommendation`, `begin_reconciled` (refusing faulted or non-quiescent histories), `start_conflicts` with a closed `RunSummary` (F3), and the `termination_proposal` naming amendment.
- **D3.2b-4:** the canonical codec, framing and CRC-32C, `decode_stream` with version peeking before strict decoding (F14), `verify_persisted`, `scan_after_corruption`, `replay` and the read-only `ReplayedHistory`, and golden v1 fixtures.
- **Adapter:** F4, F5, F6 and F15 from the gate review (frame boundary at the mark, atomic sidecars, zero-frame runs, a sleep-inclusive clock or an explicit `clock_fault` entry point).
