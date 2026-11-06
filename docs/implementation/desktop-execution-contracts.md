# Execution lifecycle contracts

**Status:** Foundation. Implemented for [D3.2b-1 (#112)](https://github.com/RaioViajante/ordelis/issues/112), [D3.2b-2 (#114)](https://github.com/RaioViajante/ordelis/issues/114), [D3.2b-3 (#116)](https://github.com/RaioViajante/ordelis/issues/116) and [D3.2b-4 (#118)](https://github.com/RaioViajante/ordelis/issues/118), hardened by [D3.2c-1 (#120)](https://github.com/RaioViajante/ordelis/issues/120), part of the [D3.2 design record (#108)](https://github.com/RaioViajante/ordelis/issues/108). Pure Rust in `desktop/src-tauri/src/lifecycle/`, with no command, no caller and no interface.
**Scope:** the vocabulary of runs, attempts, approvals and journal events, the deterministic reducer of their state machines, the in-memory live authority of one supervised run, conservative crash reconciliation with cross-run start conflicts, and the canonical journal codec with full replay. Nothing here starts, stops, finds or observes a process, reads a clock or a file, or persists anything: the codec turns bytes it is given into values and values into bytes.
**Related:** [Provider catalog and evidence vocabulary](desktop-agents.md) · [Execution Safety Gate](execution-safety-gate.md) · [Desktop architecture](desktop-architecture.md)

## Where execution contracts stand

| Part | State |
| --- | --- |
| Design (#108 decisions 10 and 11; D3.2b design r5) | Approved in principle. An event-sourced journal per run, history never authority, monotonic live windows, conservative reconciliation, a framed append log as the storage direction. Kept outside the repository; hashes on #108. |
| **Domain types and validation (D3.2b-1)** | **Implemented here.** |
| **Reducer and in-memory live authority (D3.2b-2)** | **Implemented here.** Compiled into every build, used by nothing outside its tests. |
| **Reconciliation, `begin_reconciled` and cross-run start conflicts (D3.2b-3)** | **Implemented here.** Used by nothing outside its tests; it reads caller-supplied histories and synthetic observations only. |
| **Codec, framing, `replay`, `ReplayedHistory` and `verify_persisted` (D3.2b-4)** | **Implemented here.** Used by nothing outside its tests; it reads caller-supplied bytes only. |
| **Reducer safety hardening (D3.2c-1)** | **Implemented here.** A start ends its frame; a bounded safety tier past the event limit. |
| D3.2c-2, D3.2c-3 | Approved as the next pure increments; not started. |
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
- **Reserve and moves:** the reserved kinds keep room to stop, end, reconcile and resolve a run; past the event limit, the safety kinds (D3.2c-1) do. Only attempt events move an attempt.
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
- **Time:** `at_ms = max(previous, stated)`; a lower or invalid stated time keeps the previous one, sets `clamped` and raises one `clock_irregular` concern until it is acknowledged (an acknowledgement of that concern raises no new one, D3.2c-1). Only `RunCreated` refuses an invalid time. Windows recorded in wall time can only close early.
- **Identity:** a draft whose every semantic key maps to the same earlier event with the same producer and body is a `Duplicate` (nothing appended). Anything else on a held key is a `Conflict`. A batch mixing duplicates and new events is a `PartialDuplicate`. The same fact twice in one batch conflicts with itself.
- **Batch shapes:** one producer throughout; or exactly `[AttemptCreated (user), ApprovalRequested{start} (supervisor)]` for the same attempt (F10); or repair records, then (after at most two) reconciler drafts. Anything else is `BatchShape`.
- **Approval-driven moves:** a start approval's own outcome moves its attempt before anything runs, only from the user or the supervisor and only when the batch holds no attempt event for that attempt; when it does, the approval event is refused unless that batch also ends the attempt (F10). A reconciler approval event never moves an attempt.
- **Requests match their run (F7):** the specification digest, the binding target, the risk Ordelis recomputes, `ttl_ms ≤ min(approval_ttl_ms, 900 s)`, contiguous approval ids, the current attempt (`proposed` for start, `running` for operation), and at most 4 open and 64 total approvals per attempt.
- **Decisions and consumptions:** a decision needs the presented binding digest and must be recorded before `request time + ttl`; a consumption restates the exact binding and must be recorded before `min(decision + 60 s, request + 900 s)`.
- **Generated consequences:** right after an attempt event that ends or stops an attempt, one `ApprovalWithdrawn` per pending and one `ApprovalVoided` per granted approval of that attempt, in approval order, with the cause's producer, time and version, never clamped, and a reason fixed by the cause (`cancelled`, `clock_fault`, `supervision_lost` or `attempt_ended`). Replay requires the records after a cause to be exactly them.
- **Run outcomes:** derived immediately after the attempt event that causes them. A run-scope cancel moves the run to `closing` first, so `cancelled` wins over `exhausted` (F12). `exhausted` carries the outcome class of the last attempt that may have run.
- **Outcomes and concerns:** rule W (a provider can only turn a later zero exit into a reported failure); an exit while stopping needs the child's recorded identity and is decided by the first stop reason (F2); a start failure is `never_started` and raises no concern; every interruption raises a blocking `possible_orphan`; after a recorded repair, a not-yet-started attempt can end only as `interrupted{evidence_lost}`, never as "not started".
- **Progress:** coalesced (accepted, not appended) within 10 s of the attempt's last recorded progress, live and in replay; a stored progress the rule would have coalesced is a `CoalesceViolated` fault.
- **Limits and reserve:** inside the last 64 events or 512 KiB only reserved kinds and repairs are accepted; other provider-relayed drafts are `Coalesced`, other drafts `ReserveOnly`. Past 4096 events or 4 MiB only the bounded safety tier is open, and past its ceiling nothing at all (D3.2c-1, below). A frame holds at most 8 events, generated ones included. Replay checks every tier by event count only (F14).
- **Recorded envelope:** a known version that never decreases (`NewerVersion`, `VersionDecreased`), this run (`WrongRun`), the next `seq` (`SeqNotFirst`, `SeqGap`), a time that never decreases (`TimeDecreased`), and `clamped` only with the previous time (`ClampedMisused`). Expiry and void timing is not checked against `at_ms` in replay (F14).

## Safety hardening (D3.2c-1)

Two reducer rules found by the D3.2b integrated review (findings S1 and S2), applied identically by `apply` (live, so also `stage`, `reconcile` and `begin_reconciled`) and by `apply_recorded` (replay).

### A start ends its frame (S1)

A `StartBegun` must be the **last event of its frame**. Anything after it in the same batch (the child's `AttemptStarted`, an exit, a stop, a cancel, a progress report, from any producer) is refused with `Rejection::StartNotLast`; a recorded frame that holds one is the fault `JournalFault::StartNotLast` and quarantines the run. A `StartBegun` generates no consequence, so on replay it is also the frame's last record. A refused batch changes nothing, and a grant handed to `stage` comes back.

- **What it proves:** only that no recorded frame can show a start and its effects as one write, as r5 5.2.1 (effect after record) requires: the child's identity, exit or anything else always comes in a later frame.
- **What it does not prove:** durability. It authorizes no execution. A future execution boundary must still require, independently, a durable `StartBegun` and high-water mark, Execution Safety Gate satisfaction, valid live consent and global start-conflict clearance before any process starts.

### Bounded safety capacity (S2)

Before D3.2c-1:
- reserved kinds could fill the 64-event reserve. In particular, a clamped acknowledgement of `clock_irregular` raised the next concern each time;
- at 4096 events every stop, end and reconciliation was refused, the run could never be reconciled, and every start stayed blocked;
- a repair was exempt from every limit.

Each dimension now has four tiers, and a journal is in the higher of its two:

| Tier | Events | Bytes (live only) | Accepted |
| --- | --- | --- | --- |
| Ordinary | below 4 032 | below 3.5 MiB | any event |
| Reserve | 4 032 to 4 095 | 3.5 MiB to below 4 MiB | reserved kinds and repairs (at most `MAX_REPAIRS` = 40 per run, in every tier); other relayed drafts `Coalesced`, others `ReserveOnly` |
| Safety | 4 096 to 4 159 (`SAFETY_EVENTS` = 64) | 4 MiB to below 4.25 MiB (`SAFETY_JOURNAL_BYTES` = 256 KiB) | only **protected** events; anything else `LimitExceeded` |
| Exhausted | 4 160 and above | 4.25 MiB and above | nothing, a repair included: `LimitExceeded` |

What the safety tier admits:
- **Protected events:**
  - the safety kinds `StopRequested`, `CancelRequested`, `AttemptStarted`, `StartFailed`, `AttemptExited`, `AttemptInterrupted`, `AttemptNotStarted`, `ApprovalExpired`, `ApprovalWithdrawn`, `ApprovalVoided` and `ProviderOutcomeClaimed` (`EventKind::safety`, a subset of the reserved kinds);
  - an `AttentionResolved` of a **blocking** concern (a possible orphan);
  - a `JournalRepaired`, on its own or in a reconciliation frame. Repairs are capped at `MAX_REPAIRS` (40) per run,
    counted over the whole journal: the 41st is refused in every tier, and `reconcile` reports a tail that would need it as `TooManyRepairs`, so the adapter quarantines the run instead. A repair
    must be admitted on its own, because a protected frame written after the attempt ended (an orphan resolution, a run
    cancel) can itself tear, and only a repair makes the run reconcilable again.
- **Generated consequences** count. A protected event must fit, with its consequences, under the absolute ceiling (4 160 events); any other event under 4 096. Otherwise the whole batch is refused.
- **Not admitted:** informational resolutions (`clock_irregular`, `workspace_unknown`, `journal_repaired`), `RunClosed`, progress, sessions, attempts, requests, decisions, consumptions and starts. A run whose last attempt ends past 4 096 events therefore stays `Open`, and cannot be closed. That does not block any start: `start_conflicts` ignores an open run with nothing in flight and no blocking concern.

**Why 64 is enough.** Past 4 096 events no attempt or request can be created, so only the one attempt in flight and its open approvals can still produce protected events, each kind at most once per semantic key. That bounds what a run can still record to **12 events** plus repairs:
- one stop and two cancels (attempt and run);
- the child's identity, a failure claim and the end;
- at most four approval endings;
- at most two orphan resolutions (one is reachable; counting two is conservative).

Repairs add at most `MAX_REPAIRS` over the run's whole life, so 12 + 40 = 52 ≤ 64. In bytes, 64 events of 2 048 bytes in one-event frames fit 256 KiB.

A seeded property test (400 seeds, 23 draft shapes from every producer, from states with an attempt in flight and from arbitrary reachable states) checks three things: the 12-event bound, the repair cap, and that nothing unprotected (an attempt, a request, a decision, a consumption, a start, a close, a progress report) is ever recorded. The highest non-repair count it observes is 9.

**Repeated findings and resolutions.** The acknowledgement of a `clock_irregular` concern never raises the next one, even when its own time was clamped: the clamp stays recorded on the event, and the loop of acknowledgements is gone. Every other clamped event, the resolution of any other concern included, still raises one concern while none is open.

**Failure behaviour (fails closed).** At the ceiling nothing is recorded: a stop, an end, a reconciliation frame and a repair are all refused with `LimitExceeded`. A refused event is never reported as recorded, never turns into a terminal state, and is never taken for an observed exit or cancellation. No evidence is deleted to make room. How the run then blocks starts depends on whether it is supervised:

- **Unsupervised (a recorded history, after a restart):** blocked by conservative reconciliation. The attempt in flight stays in flight (reconciliation itself is refused), so its summary reports `Unreconciled`. An unresolved orphan stays `PossibleOrphan`. A tail past the repair cap is `TooManyRepairs` and `RepairPending`. A quarantine is `Quarantined`.
- **Supervised (a live supervision holds the run):** blocked by an in-memory supervision-health invariant, the **capacity fault**.
  - **How it is set:** a `LiveSupervision` enters it, for its whole lifetime, in any of three cases:
    - a stated size reaches either ceiling (`stage`, `due` or `begin_reconciled`; the size is never taken below what the supervision holds);
    - a frame it stages would bring the journal to a ceiling (bytes counted at the largest such a frame can take), whether that frame is then confirmed or abandoned: nothing could follow it, not even the end of an attempt it stops;
    - a batch holding a protected draft (a stop, an exit, a cancel, an end, an approval ending, a claim, an orphan resolution or a repair) is refused with `LimitExceeded`. This is conservative: it includes an unprotected draft of the same batch refused for capacity, and the run's repair cap.

    A batch over `MAX_BATCH` is a caller error, not a capacity fault, and does not set it.
  - **No way out:** nothing clears it. No later, smaller stated size, no method, no field, no conversion.
  - **What the faulted supervision still does:** it stages only the drafts allowed under `clock_fault` (stop, cancel, end, start identity, expiry, withdrawal, void, repair), so evidence that still fits is recorded; it mints no grant (not even for a decision already in flight) and accepts none, and accepts no decision; `due` keeps proposing a `journal_full` stop. An outstanding grant becomes unusable at once; `due` proposes its void only once its use window passes, as before.
  - **Its claim excuses nothing:** `start_conflicts` reports `CapacityExhausted(run)` for every faulted claim, whatever the run's summary, and `Unreconciled` when that summary shows an attempt in flight, globally, whatever other run ids or project registrations exist. Its own blocking concerns still count as before.
  - **After it is dropped:** the run's recorded summary blocks on its own (above).
  - **Trade-off:** a supervision faulted by a transient overstatement keeps blocking every start until it is dropped, even if a later stop fits and is recorded.
- **What the fault is not:** a durable quarantine. It lives in memory and dies with the process. It records nothing, because there may be no room to record it; it stops nothing, and terminates nothing (r5 5.8 and decision 13 still apply: an owned child is still stopped, and is not treated as stopped).
- **Still the future adapter's:**
  - a byte-derived, honest `JournalSize`: with honest sizes the ceiling cannot be reached while an attempt is in flight (4 096 + 10 + 40 events at most);
  - durable evidence before every effect;
  - writing a quarantine marker for a run whose stop, end or repair could not be recorded, and dropping its supervision rather than reporting it settled;
  - stating the current size on every `stage` and `due`;
  - a locked, counter-enumerated `RunInventory`.
- **Still open for D3.2c-2:** `NeverCreated` is still caller-stated (S4), and a dishonest inventory (an understated counter) can still hide any run. Each conceals a run only by a false statement.
- **Live and replay:** live counts the caller's stated size (events and bytes); replay counts events only, so it accepts at least what the live path accepted with the same count. Concerns never depend on bytes, so live and replayed projections stay equal.

### Differences from design r5 in D3.2c-1

- r5 4.4 and 4.8 exempted `JournalRepaired` from every limit. A repair is now accepted in the ordinary, reserve and safety tiers, capped at `MAX_REPAIRS` (40) per run, and never at the ceiling. A repair that cannot be recorded leaves the run to be quarantined (r5 5.5 already requires it).
- r5 4.8 sized the reserve for under 25 events without counting resolutions or re-raised clock concerns. The safety tier and the non-raising resolution close that.
- r5 3 (clock anomalies): a clamped acknowledgement of `clock_irregular` raises no new one.
- New codes: `Rejection::StartNotLast` and `JournalFault::StartNotLast`. New constants: `SAFETY_EVENTS`, `SAFETY_JOURNAL_BYTES` and `MAX_REPAIRS`.
- None of this changes the wire format or any golden v1 fixture (no golden frame batches a start, comes near the reserve or holds a clamped resolution). v1 is still unfrozen.

## Live authority (`lifecycle/live.rs`)

| Type | Role | Rules |
| --- | --- | --- |
| `PendingSupervision` | A run being created: `begin_new` staged its `RunCreated` frame. | Holds no grant, accepts no draft. `open` makes it live after the frame is durable; `drop_pending` discards it. |
| `LiveSupervision` | The live projection of one run during one process lifetime, with its monotonic clocks, memory-only descriptions and outstanding grants. | Built only by `begin_new` and `begin_reconciled` (D3.2b-3), through one private constructor. One outstanding `Staged` at a time. Refuses every reconciler draft: a supervised run is never reconciled (r5 5.4). |
| `Staged` / `Abandoned` | A frame to write, and a frame whose write was uncertain. | `#[must_use]`. `confirm` (durable) or `abandon` (uncertain). After `abandon` the supervision is unresolved until `adopt` (the frame landed) or `discard` (it is absent) (F8). Each call accepts only the frame this supervision holds outstanding (same epoch, number, events and run, extending its current projection). A wholly duplicate or coalesced batch stages an empty frame: nothing is written, and it is still confirmed. |
| `LiveGrant` | The only value that can consume an approval. | Minted only by `confirm`, for a decision that frame newly recorded, inside its monotonic window, for a request this supervision recorded, whose approval is still granted after the frame (F13). Bound to the epoch, run, attempt, approval, kind and exact recorded request. Consumed by value by `stage`; returned inside the refusal when `stage` refuses. Valid until `min(decision + 60 s, request + 900 s)` on the monotonic clock. |

- None of these types derives or implements `Clone`, `Copy`, `Default`, `Debug` or serde, none has a field visible outside `live.rs`, and none is named by any other file. No conversion leads from an event, a projection or a recorded frame to any of them; `adopt` never mints.
- **Monotonic time:** every entry point that takes `mono` (`begin_new`, `stage`, `due`) checks that it never decreases. A regression puts the supervision in `clock_fault` for good: no decision, no consumption, no further grant; only stop, cancel, exit, start failure, expiry, withdrawal, void, the identity of a child that started meanwhile, and repairs (F2). An overflowing window or deadline is already closed.
- **Capacity fault (D3.2c-1):** a stated size at either ceiling, or a protected draft refused for capacity, puts the supervision in a capacity fault for good: only safety drafts, no grant minted or accepted, no decision, and its `SupervisedRun` claim excuses nothing (it reports `CapacityExhausted`). See "Safety hardening (D3.2c-1)".
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

- **`RecordedHistory`**: a projection rebuilt from recorded frames, a `HistoryStatus` (`Verified`, or `Quarantined(Fault(JournalFault) | Marker)`), and the repair records the journal still lacks. Since D3.2b-4 it is built only from a replay, by `RecordedHistory::from_replay`, which derives the status (below); `RecordedHistory::new`, which states one, exists in test builds only.
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
- **Refusals** (`ReconcileRefused`): no `RunCreated` (the adapter classifies a run with no valid frame, F6), more than two missing repairs or more than the run's `MAX_REPAIRS` (D3.2c-1), a reported repair whose sidecar is recorded with other content or reported twice, or a frame the reducer refuses. The run then stays unreconciled, which blocks every start.

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
- **Blocking conditions** (global in v1): a capacity-faulted supervision (`CapacityExhausted`, D3.2c-1, whose claim then excuses nothing), an unresolved possible orphan (or any other unresolved blocking concern, `BlockingConcern`, for a run-level kind a later version might add), a quarantine for any reason, an unverified run, a read without the lock or from a derived index (it can add conflicts, never clear them), a repair record the journal still lacks, and an attempt not terminal in a run the caller does not supervise (never reconciled, a failed reconciliation append, or another instance).
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

## Journal codec (`lifecycle/codec.rs`, D3.2b-4)

The codec turns events into bytes and bytes into a classified result. It reads and writes no file, repairs, truncates or moves nothing, and judges no transition: what decoded events mean is the reducer's, through `replay`. It is the only lifecycle file that names serde, through its own private wire types; no model type derives anything.

### Frame

```text
ORDJ1 <payload length> <crc32c> <payload>\n
```

- **Header:** the literal `ORDJ1 `; the length as 1 to 5 decimal digits with no leading zero, at most `MAX_FRAME_BYTES` (17 408); one space; exactly 8 lowercase hexadecimal digits; one space. Anything else is corruption: another magic, a sign, a sixth digit, upper-case hex, a tab, two spaces.
- **CRC-32C** (Castagnoli), pure and bitwise, no table, over `<length digits> <payload>`, so a damaged length is detected like a damaged payload. Pinned by `"123456789"` → `e3069283` and the RFC 3720 vectors.
- **Payload:** a canonical JSON array of 1 to `MAX_BATCH` (8) records, one line: a canonical payload never holds a raw newline.

### Canonical JSON v1

- Every record is `{"v","run","seq","at_ms","clamped","producer","kind","body"}` in that order, with a body per kind whose fields have a fixed order.
- Closed values are snake_case strings (`"provider_relay"`, `"deadline_run"`, `"incomplete_tail"`). Values with data are flat: a stop reason is `deadline_start | deadline_run | deadline_stall | journal_full | clock_fault`; a cancel scope is `{"scope":"attempt","attempt":n}` or `{"scope":"run","attempt":null}`; a continuation is `{"continuation":"fresh"|"resume","from_attempt","session"}`.
- Every optional field is always present, `null` when absent. Numbers are integers; only the exit code may be negative.
- An approval request records its `scope` and `risk`; decoding recomputes both (`once`, and the classification raised by the provider's claim) and refuses a record whose stored values differ, so a stored risk can never be lowered.
- **Decoding**, in order: nesting at most 64 levels (the bound of every ORDJ1 payload), checked by a byte scan before serde parses anything; the version of every record (below); then the v1 limits, at most 8 records and 8 nesting levels; strict decoding into the wire types with unknown fields denied; every value through its model constructor; every integer at most 2^53 − 1; and the payload re-encoded from the model values must equal the input **byte for byte**. The re-encoding rejects whitespace, field order, escapes (`r`, `\/`), number forms (`-0`, `1.0`, `1e0`), omitted nulls, records written as arrays, and duplicate keys (an envelope duplicate fails strict decoding; a body is read through a JSON value that keeps one of two equal keys, and the re-encoding refuses it).
- **Version first (F14).** Each record's `v` is read before strict decoding, through a type that ignores the rest. A valid frame holding a record above `RECORD_VERSION` ends the stream as `NewerVersion`, not as v1 corruption, whatever else the record holds within the bounds every ORDJ1 version keeps (the frame header and size, and 64 nesting levels): the v1 limits on records per frame and nesting apply only after the version is known to be 1. `v = 0`, a missing or non-integer `v`, and a `v` above 2^53 − 1 are corruption. Future versions must keep `v` an integer field of every record object. Only v1 exists, so there is no older version to upcast.
- **Bounds:** `MAX_EVENT_BYTES` (2 048) per record, with a test that encodes the largest instance of every kind; `MAX_BATCH` records per frame; `MAX_FRAME_BYTES` per payload; and `MAX_STREAM_BYTES` (`MAX_JOURNAL_BYTES` + 1 MiB) per stream: a journal accepts no ordinary event at or above `MAX_JOURNAL_BYTES`, so beyond it only the frame that crossed it, repair records and one torn tail can follow. Nothing is allocated beyond a validated length, and decoding never panics.
- **serde_json features:** the crate compiled for the app has `std`, `alloc`, `default` and `raw_value`. No crate enables `arbitrary_precision` (a test fails if one does); `unbounded_depth` is enabled only in the host copy that proc-macros use, and the nesting scan makes the bound independent of it.

`encode_frame(events)` writes one frame of 1 to 8 contiguous events of one run at the current version; anything else is refused (a caller bug).

### Classification (`decode_stream`)

`decode_stream(bytes)` returns the valid frames, each with its byte range and events, and how the stream ends:

| End | When | What may follow |
| --- | --- | --- |
| `Clean` | Every frame valid; the stream ends right after a newline. | Replay. |
| `IncompleteTail { offset, len, crc32c }` | The bytes after the last valid frame hold **no newline at all** and are at most `MAX_TAIL_BYTES`, and are not a complete frame with a valid checksum and a wrong last byte: a cut header or payload, a complete frame whose newline is missing because the stream ends there, a short zero-filled extension. | Never interpreted. Only a future adapter may move it, unchanged, to a sidecar, only when the valid prefix reaches the high-water mark, and the repair record must match it exactly (below). |
| `Corrupt { offset, reason }` | Everything else: a complete frame whose checksum is wrong (even the last one), a valid checksum with another byte where the newline belongs (`FrameEnd`, a CRLF included), a length past the end with a newline later (`CutFrame`), a newline-free tail longer than one frame (`TailTooLong`), a bad header, a payload that fails nesting, schema, integer, batch, size or canonical checks, a stream above `MAX_STREAM_BYTES`. | Quarantine. Nothing after the offset is interpreted, nothing is truncated, skipped or repaired. |
| `NewerVersion { offset, v }` | A valid frame holding a record of a version this code does not know. | Quarantine, read-only, bytes untouched. |

A corrupted frame in the middle stops decoding there: later frames are never skipped into the history. That the codec itself never turns bytes after a corruption into events rests on review of `codec.rs` (decoding stops at the first invalid frame, and the scan below holds no event); the static pins keep every other file from reading unverified frames. `scan_after_corruption(bytes, offset)` lists, for audit only, the frame-shaped lines after the corruption (at most 1 024, with a flag when more exist): each `UnverifiedFrame` carries its offset, its length, whether its checksum matches and the kinds it would name, and **no event**, so it can never be applied, replayed or reconciled.

## Replay (`lifecycle/replay.rs`, D3.2b-4)

`replay(run, &decoded)` rebuilds the run's projection from the empty one, frame by frame, through the reducer's `apply_recorded`, the same transition function as the live path. The first fault stops it, and nothing after it is applied:

- every reducer fault of a recorded frame (`WrongRun`, `SeqNotFirst`, `SeqGap`, `TimeDecreased`, `ClampedMisused`, `VersionDecreased`, `ProducerNotAllowed` for a forged label, `ConsequenceMismatch` for generated consequences that are not exactly the generated ones, `DuplicateKey` for a repeated or conflicting terminal event, `InvalidTransition`, `CoalesceViolated`, `MixedProducerFrame`, `LimitExceeded`);
- `Codec` for a `Corrupt` end, with the codec's reason kept for display;
- `NewerVersion` for a `NewerVersion` end.

`ReplayedHistory` is read-only: its projection (the state after the last valid record), its fault, the codec's reason, the incomplete tail if the stream ended in one, and the frame boundaries. It holds no supervision, no grant and no clock, has no mutable method, and nothing but `RecordedHistory::from_replay` consumes it. Replay is deterministic: the same bytes give the same history, and the bytes of a live journal replay to its live projection after every frame (tested as a property, and through `stage`/`confirm` of a live supervision).

`check_mark(mark)` checks the run's high-water mark against the replayed prefix (r5 5.7 step 3): `(0, 0)` vouches for nothing having been acknowledged; otherwise a frame must end exactly at `byte_length` with `last_seq`, inside the prefix. A missing, unreadable or invalid mark, a prefix that ends before it (acknowledged frames lost at a frame boundary, which a clean decode alone cannot see), or a mark that is not a frame boundary of that sequence number is a `MarkProblem`.

## Recorded histories from replay

`RecordedHistory::from_replay(replayed, evidence)` is the only production way to build the history reconciliation reads. `OpeningEvidence` is what only the future adapter can see: whether a quarantine marker exists, the high-water mark as read, and every sidecar without a `JournalRepaired` record (as repair records in sidecar order, the one about to be made for a tail included). In order:

1. a reported marker → `Quarantined(Marker)`;
2. a replay fault → `Quarantined(Fault(fault))`, codec damage and newer versions included;
3. a mark problem → refused (`HistoryRefused::Mark`): the adapter writes a marker;
4. no `RunCreated` → refused (`NoRunCreated`): the adapter's never-created path (F6);
5. an incomplete tail → refused (`TailNotRepaired`) unless a reported repair of cause `incomplete_tail` removes exactly its length and CRC-32C **into a new sidecar**: one that no `JournalRepaired` record holds, numbered after every recorded one. A recorded repair of equal size and checksum (two zero-filled blocks have the same CRC-32C) removed earlier bytes and would draft no new record, so it never vouches for a new tail;
6. otherwise `Verified`.

Step 5 is what keeps a torn tail from becoming "not started": the matched repair record goes into the opening frame first, so reconciliation ends a `proposed` or `approved` attempt as `interrupted{evidence_lost}`. With a clean stream the same attempt is `not_started{supervision_lost}`, which is correct only because frames beyond the high-water mark were never acknowledged (write-ahead, r5 5.2): a property test cuts journals at random offsets under random marks (25 per fixture, 64 fixtures) and checks that no verified history reaches below its mark, that a repair the journal already records never vouches for a tail, and that "not started" appears only on a clean stream holding no start of that attempt.

The core checks everything the bytes can show. It cannot see a marker or a sidecar, so a dishonest caller can hide those (as with every adapter fact, r5 section 8), but cannot make a faulted, torn or mark-short journal `Verified`, and nothing it supplies creates authority: reconciliation and `begin_reconciled` still apply, and mint nothing.

## Uncertain writes (`verify_persisted`)

`verify_persisted(file, committed, pending)` is the pure, byte-exact check of r5 5.4: `committed` is what the writer knows it acknowledged, `pending` the frame whose write was uncertain, `file` the journal re-read under its lock.

| Result | Meaning | The future adapter then |
| --- | --- | --- |
| `Absent { torn }` | The file is exactly `committed`, possibly followed by a torn tail (no newline, at most one frame), reported with its length and whether it is a prefix of `pending`. | Repairs the tail (5.5), then may retry non-consumption drafts unchanged; `discard`s the abandoned frame. |
| `Landed` | The file is exactly `committed` followed by exactly `pending`. | `adopt`s it: the live projection gains the frame, and **no grant is minted** (a landed decision is voided `write_uncertain`; a landed start is followed by `effect_not_performed`). |
| `Diverged(reason)` | Shortened, altered, a foreign complete frame, extra bytes after the pending frame, or corrupt bytes after `committed`. | Poisons the stream: durable quarantine marker, no further appends; live state is kept so its own child can still be stopped (5.8). |
| `Unknown(reason)` | The inputs cannot support a conclusion: `committed` is not a clean stream, `pending` is not exactly one valid frame, `pending` does not continue `committed` (another run, or not the next sequence numbers), or the file is beyond the stream bound. | Treats it as `Diverged`. |

What it establishes: whether these bytes hold that frame, exactly. What it cannot establish, and the adapter must: that `file` is the journal (lock, `O_NOFOLLOW`, owned regular file), that what it read is durable (`F_FULLFSYNC` and the directory sync), that `committed` is truly what was acknowledged (the high-water mark), and that no other writer exists (the lock for the whole supervision lifetime). It never replays, never reconciles a supervised run, never substitutes either for the prefix check, and its result is never authority: `Landed` only lets the live layer adopt a frame, which mints nothing (tested through `abandon`, `verify_persisted` and `adopt`).

### Differences from design r5 in D3.2b-4

- `replay(run, &decoded)` takes the run id and the whole decoded stream rather than `&[Vec<Event>]`, so the stream's end (tail, corruption, newer version) is part of the history. `ReplayedHistory` carries the fault, the codec's reason, the tail and the frame boundaries; `valid_through_seq` is the last event of its projection.
- `RecordedHistory::from_replay` derives the status and is the only production constructor; `RecordedHistory::new` is test-only. The repair of an incomplete tail is bound to the tail's exact length and CRC-32C; r5 does not state that binding.
- The high-water-mark check is pure (`check_mark`); a mark problem refuses the conversion (the summary is `Unverified(MarkMismatch)` until the adapter writes a marker).
- `StreamEnd::NewerVersion` is its own end (F14); `scan_after_corruption` returns at most 1 024 `UnverifiedFrame`s without events and a flag; `verify_persisted` adds `Unknown` and reports torn tails.
- New bounds: `MAX_STREAM_BYTES`, a frame nesting depth of 64 checked before serde (8 for v1 after the version peek), and 1 024 unverified frames. r5 5.7 step 5 lets a sidecar holding the tail's bytes be reused; here only a sidecar without a record may vouch for a tail.
- The v1 wire form above (names, flat encodings, always-present nulls, recomputed risk and scope) is fixed here; it may still change with review until the first persistence adapter freezes v1.
- To build values from text the codec needs the provider catalog's and evidence vocabulary's types: `Sha256` gains an `as_str` accessor (agents, no other change), and `model.rs` names them for the rest of the lifecycle through three aliases (`CatalogId`, `Digest`, `ShortToken`), keeping its two reviewed imports the only reference to the agents module.

## Static boundary

`config.test.ts` holds the compiled `src/lifecycle*` files to the agents purity rules: no process, filesystem, network, environment, clock, thread, global state, unsafe code, FFI, Tauri, serde or libc; no command; nothing public outside the crate; no identifier that could hold a path, a search location, an argument vector or a command; and no macro other than `matches!`.

Decision 12.8 amends the rules for this module:

- It may reference the rest of the crate only through two whole-line imports in `lifecycle/model.rs`, adjacent and in this order, as its first lines of code (only line comments before them, so no `pub(crate)` or attribute can prefix them):
  - `use crate::agents::catalog::ProviderId;`
  - `use crate::agents::evidence::{Sha256, Token};`
- Any other copy or position is held to the full rules and fails.
- The module root names no `super`; a submodule names `super` only to reach a reviewed sibling (decision 12.8(b), D3.2b-2 to D3.2b-4): the model none, the reducer the model, the live layer the model, the reducer and reconciliation, reconciliation the model, the reducer, the live layer and replay, the codec only the model, and replay the model, the reducer and the codec. A lifecycle file with no reviewed list fails. No file imports with a glob. It may name the agents module nowhere else.
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

D3.2b-4 adds one reviewed serde exception and the pins that keep bytes from becoming history or authority:

- **Serde in the codec only (decisions 12.8(c), 12.9).** The serde rule is unchanged. In `lifecycle/codec.rs` only, exactly these forms are set aside before the rules run: the import `use serde::{Deserialize, Serialize};` as its first line of code (only line comments before it), the pure in-memory functions `serde_json::from_slice`, `serde_json::to_vec`, `serde_json::from_value` and the type `serde_json::Value`, and the attribute `#[serde(deny_unknown_fields)]`. A reader, a writer, a serde path, a renamed `serde_json`, another attribute (a rename, a custom deserializer), a second import, an import after other code, an import naming more, or any of these in another file stays a violation, and the test checks that the codec would fail without the exception.
- **Serialization on wire types only.** No other lifecycle file names `Serialize`, `Deserialize`, `Serializer`, `Deserializer`, serde or serde_json; nothing implements a serde trait or a visitor by hand; and every derive that names one is on a private codec struct named for the wire (`W…`, `WireOut`, `WireRecord`) or the version peek (`Peek`). No model, live, replay or reconciliation type can be serialized.
- **Decoded and unverified bytes.** `Decoded` and `DecodedFrame` are declared once in `codec.rs` with no attribute and no visible field, and built once there; `UnverifiedFrame` has exactly the derives `Clone, Debug, PartialEq, Eq`, no visible field and no field holding an event, a body, a decoded frame or a projection. No trait is implemented for these types, none is aliased or renamed, no other file builds one, and no other file names `UnverifiedFrame` or `scan_after_corruption`.
- **Replayed history.** `ReplayedHistory` is declared once in `replay.rs` with no attribute and no visible field, built once there, has no trait, alias or rename, and `replay.rs` takes no `&mut self`. Only `replay.rs` and `reconcile.rs` name it.
- **No change after construction.** No lifecycle file takes a `&mut` of a replayed or recorded history or binds one `mut` with its type named; `replay.rs` and `reconcile.rs` have no `mut self`, and assign no history field, call no mutating method on one (`take`, `clear`, `push`, `retain` and the like, whatever the spacing or parentheses) and borrow none mutably, by `&mut` or `ref mut`. These pins cover the ordinary ways to change a value; anything more contrived rests on review and on the Rust tests of faults, tails and repair binding; only `replay` returns a replayed history; and no lifecycle file uses a conditional attribute (`cfg_attr`), which could derive or rename what the rules never see. A reviewed `serde_json` item may not be renamed.
- **Recorded history.** `RecordedHistory::new` is `#[cfg(test)]`; the private `assemble` holds the one literal and is called exactly four times (by `new` and by `from_replay`); no other function in `reconcile.rs` returns a recorded history; and `HistoryStatus::Verified` is named once in production code, in `from_replay`, and nowhere outside `reconcile.rs`.
- Fixtures cover each of these, and a process, a file, a clock, an environment read, a path, a launch word and a macro in the new files.

`native-sources.json` lists the new files, so the compiler's dep-info check covers them. The ten commands, their grants, the generated permissions, the dependency set and the release-isolation verifier are unchanged.

## Tests

All synthetic: no file, clock, process, network, provider or VM.

`lifecycle/codec/tests.rs` (26 tests), with **golden v1 fixtures** pinned as test constants (one frame per line, synthetic values only):

- three golden histories built through the reducer: a supervised run (start consent, a session, progress, an operation consumed, one expired and one voided by a run-deadline stop, a failure claim, an exit, an attempt cancelled before its start, a failed start, a resolution, the close), a run reconciled after a recorded repair (`evidence_lost` and a `supervision_lost` void), and a run reconciled as never started (a `supervision_lost` withdrawal). Together they hold all 22 kinds, both request kinds, single- and multi-event frames and every producer's generated consequences; they encode to exactly the pinned bytes and decode and replay to pinned projections;
- CRC-32C check value and RFC 3720 vectors; the frame layout and envelope field order; encode refusals; the largest instance of every kind against `MAX_EVENT_BYTES` and of a full batch against `MAX_FRAME_BYTES`; every closed value by name;
- canonical and schema refusals (whitespace, order, escapes, floats, exponents, leading zeros, signs, negative zero, duplicate keys in the envelope and the body with equal or different values, unknown and missing fields, omitted nulls, unknown names, a lowered risk, another scope, integers above 2^53 − 1 and above u64, records as arrays, non-ASCII and look-alike letters, bidi controls, lone surrogates, escaped controls, invalid UTF-8), empty and oversized batches, deep nesting (65 and 8 000 levels for any version, 9 for a v1 record), and the version peek (v2 with unknown fields or any shape, nested deeper or with more records than v1 allows, a newer record inside a frame, frames before kept, nothing after interpreted);
- framing: malformed headers, truncation at every byte offset of a journal, torn versus corrupt tails (missing newline, zero runs at and over `MAX_TAIL_BYTES`, a wrong last byte, a bad checksum on the last frame, a torn frame followed by more, a length past the end, garbage), a corrupted middle frame, the audit scan and its bound, the stream bound, `serde_json` without `arbitrary_precision`;
- `verify_persisted`: every `Absent`, `Landed`, `Diverged` and `Unknown` case, including the first frame of a journal;
- **seeded property tests** (no dependency; failing seeds go to `CODEC_REGRESSION_SEEDS`, none so far): 200 random journals from every producer (backward and absurd times included) round-trip byte for byte and reach every kind; 300 journals × 20 random mutations (bit flips, truncation, insertion, deletion, garbage, overwrites, duplication, zero tails) never panic, never lose a frame before the first changed byte, and keep every decoded frame canonical; a single bit flip anywhere in a journal is always detected at its frame; 2 000 random byte strings, half framed with a valid checksum, never panic; `verify_persisted` on every frame of 80 random journals.

`lifecycle/replay/tests.rs` (20 tests): replay equals the live projection for every fixture and golden history, after every frame of 150 random journals, and for frames staged and confirmed by a live supervision; every recorded-history fault at its frame with the projection before it (another run, missing first frame, missing, repeated and swapped frames, decreasing time, a misused clamp, forged producer labels, consequences with another reason or producer, dropped or added, a second exit, a conflicting terminal state, a second start, a start before it began, a coalesced progress, a mixed frame); a corrupted middle frame, a newer version and a bad last checksum quarantining with the bytes unchanged; the derived status (marker, fault, every mark problem including acknowledged frames lost at a frame boundary, never-created); a torn tail refused without its exact repair, and refused with a repair the journal already records or a sidecar below a recorded one, and reconciled as `evidence_lost` with a new one; a property over truncated journals and random marks that never yields "not started" after a torn tail or a recorded start; contradictory repair evidence; crash recovery through `reconcile`, `begin_reconciled` and the opening frame replayed, with the orphan and the quarantine blocking every start; replayed decisions and grants never usable; a landed uncertain decision adopted with no grant; and (D3.2c-1) a recorded start followed by anything quarantining, a run at 4 096 real events stopping, reconciling after a torn tail and resolving its orphan, replay refusing unprotected records past the event limit, and a protected write torn after the attempt ended (an orphan resolution, a run cancel) repaired and reconciled past the limit, and a tail past the repair cap refused as `TooManyRepairs`.

`lifecycle/reduce/tests.rs` (43 tests):

- the attempt transition table: every attempt state (12 fixtures) × every event kind, with the exact result (moved, unchanged, duplicate or refused), and each result replayed from its frames to the same projection;
- every kind under every producer it does not allow, refused first;
- typed constructors: the relay cannot ask for start consent; forged labels on recorded events are faults;
- run specification immutability, request-to-run matching (F7), batch shapes (F10), atomicity, duplicates, conflicts and partial duplicates;
- generated consequences, their order, producer and reasons, drafted consequences, and recorded consequences that differ;
- approval-driven moves and their suppression, decision provenance and windows, single use and the 900 s cap;
- operation consumption, executable identity, first stop reason, rule W, start failures, orphans and resolutions, repairs and `evidence_lost`;
- run outcomes (`succeeded`, `exhausted`, `cancelled` over `exhausted`, `closing`, `closed`), unknown outcomes, resume continuations;
- clamping and absurd wall readings, `Progress` coalescing live and recorded, the reserve and hard limits, approval limits, and every recorded-envelope fault.
- (D3.2c-1) a start followed by anything refused, live and recorded, and the valid order kept; the reproduction of integration-review probe p6 (one clock acknowledgement, then stop and exit in the reserve); the safety tier at 4 096 real events (claims, stops, cancels, exits, a reconciliation frame and an orphan resolution accepted; progress, requests, starts, attempts and informational resolutions refused, lone repairs admitted; replay equal); the byte tiers and both ceilings refusing everything, a stop whose consequences would cross the ceiling refused whole; the per-run repair cap; a clamped orphan resolution still flagging the clock; and the seeded bound of the safety tier (12 non-repair events plus repairs, nothing unprotected recorded).

`lifecycle/reconcile/tests.rs` (32 tests; the last seven are D3.2c-1: a supervision at the ceiling blocking every start while held and its summary after it is dropped, refused protected stops, exits and run cancels faulting it, no grant minted or used after the fault, `due` at the ceiling, a run opened at the ceiling, a frame that reaches the ceiling faulting at once, and an oversized batch faulting nothing): crash before the start, with an intact journal and after a recorded or reported repair; crash after a possible start without identity; every observation kind; stale (another epoch, from the future, one millisecond too old, the exact age limit), mismatched (pid, start token, executable), contradictory and duplicate observations, and observations of ended or unknown attempts; pending and unused approvals ended only by generated consequences; a closing run; quarantine by a stored double ending and by a marker; determinism under reordered observations and idempotence (re-applied frame, reconciled result, crash before the write); late and conflicting events; refusals; `begin_reconciled` (`Ready`, `Pending`, every refusal including forged frames and a quarantined frame relabelled as verified, a sidecar reported twice, two frames of one history opening twice as the adapter-lock boundary); no historical grant or decision usable, fresh consent for a new attempt; a blocking orphan kept until an exact, allowed, user-provenanced resolution, while informational concerns never block; frames with no stop and inert recommendations; and `start_conflicts` (global orphan block across run ids and a re-registered project, every quarantine and unverified kind, lock-free and index reads, unreconciled runs, owner and impostor supervisions, a supervision ahead of its summary, pending and twice-reported repairs, unknown and incomplete inventories, contradictions, order independence, informational concerns).

`lifecycle/live/tests.rs` (25 tests):

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
- Reconciliation trusts its caller for the adapter facts behind a history's status (a marker, the high-water mark, the sidecars without a record), though since D3.2b-4 the status itself is derived from the replayed bytes, the observations' provenance, a unique epoch, and that no live supervision holds the run it reconciles (r5 5.4); `start_conflicts` trusts the caller's inventory and `NeverCreated` claims (a supervised run must be backed by an opened supervision whose history extends the summary). A false caller can hide a blocking condition, never create a grant.
- The model module allows dead code in test builds, because parts of the vocabulary serve only later increments.
- **Codec and replay (D3.2b-4):** they read only bytes the caller supplies. Nothing reads a journal file, writes one, repairs a tail, writes a sidecar or a marker, or keeps a high-water mark: those are the adapter's. The golden v1 fixtures and wire form may still change, with review, until the first adapter freezes v1. Replay is linear in the journal; there is no snapshot (r5 4.9).
- **D3.2c-2 and D3.2c-3 (approved, not started):** bind `adopt`/`discard` to a `verify_persisted` outcome with a poison counter (S3); build `NeverCreated` from evidence (S4); derive `JournalSize` from bytes; refresh summaries after opening; classify critical frames; and, from the review of #120, admit a live repair only for a tail the adapter has shown (today `stage` accepts any journal-producer repair, so a faulty caller could spend the per-run repair cap). Then end-to-end byte-level crash properties, boundary fixtures and the remaining simplifications.
- **Adapter:** F4, F5, F6 and F15 from the gate review (frame boundary at the mark, atomic sidecars, zero-frame runs, a sleep-inclusive clock or an explicit `clock_fault` entry point); the run lock for the lifetime of a supervision; durable opening frames before `open`; the run-id counter, marks and never-created markers that `RunInventory` and `RunSummary` are built from, under the lock for anything that clears a conflict; the quarantine recovery and archive policy (12.18) before any execution boundary. For the codec (D3.2b-4): read the whole journal under the lock with a bound (`MAX_STREAM_BYTES`), call `decode_stream`, `replay` and `RecordedHistory::from_replay` with honest `OpeningEvidence`, write a marker on any refusal or quarantine, repair only a tail the conversion accepted, into a sidecar no record holds (a sidecar is reused only while it has no record), and after an uncertain append keep the committed bytes, re-read the file and `adopt` only on `Landed`, `discard` only on `Absent`, and poison the stream on anything else.
- **Execution boundary** (not authorized): refusing to record a start while `start_conflicts` reports anything, with no bypass, plus its own release policy and the Execution Safety Gate (#108 decisions 3, 5, 11, B, C).
