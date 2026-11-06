// Live authority, in memory only (D3.2b-2, issue #114; design r5 sections 3.5, 5.4 and 6.5, decision 12.17, with the
// gate review's F1, F2, F8 and F13). Pure: monotonic times, wall-clock readings and epochs are values the caller
// supplies; nothing here reads a clock.
//
// `LiveSupervision` owns the live projection of one run during one process lifetime. `LiveGrant` is the only value
// that can consume an approval. Neither can be cloned, copied, defaulted, formatted, serialized or built outside this
// file, and no event, projection or recorded frame can be turned into either. There are two doors to a
// `LiveSupervision`: `begin_new` for a new run, and `begin_reconciled` (D3.2b-3, issue #116) for a recorded run once
// reconciliation has left it quiescent. Neither mints a grant. The only place a grant is made is `confirm`, for a
// decision this supervision itself staged inside its monotonic window.
//
// A grant is consent made usable for recording one consumption. It is never Execution Safety Gate satisfaction, and
// nothing here, or anywhere in the lifecycle module, turns it or a staged frame into a process start.
//
// Uncertain writes are bound to evidence (D3.2c-2, issue #122). `verify_write` is the only producer of a
// `WriteOutcome`: it encodes the abandoned frame itself, checks that the committed bytes decode to exactly this
// supervision's recorded events, and compares them with the re-read file through the codec's `verify_persisted`. `adopt`
// takes only a `Landed` outcome for exactly that frame and `discard` only an `Absent` one; `Diverged`, `Unknown`, a torn
// tail the frame does not start with, contradictory re-reads and a second consecutive uncertain append poison the
// supervision. A torn tail leaves a repair owed, which only `stage_repair` can
// record, with the length and checksum the verification measured. A byte comparison is not durability: whether the
// bytes are on stable storage is the adapter's syncs, never this file's conclusion.
use super::codec::{decode_stream, encode_frame, verify_persisted, Divergence, PersistCheck, StreamEnd};
use super::model::{
    frame_durability, ApprovalId, ApprovalKind, ApprovalRequest, ApprovalState, AttemptNo, AttemptState, ConcernClass, ConcernRef,
    Decision, Durability, Event, EventBody, EventDraft, EventKind, ExpiryBasis, JournalDraft, JournalRepair, JournalSize, LiveEpoch,
    MonoMs, OperationDescription, Producer, Rejection, RepairCause, RunId, MAX_CONSECUTIVE_UNCERTAIN,
    StopReason, SupervisorDraft, VoidReason, WallMs, DeadlineKind, GRANT_USE_MS, MAX_APPROVAL_LIFETIME_MS, MAX_BATCH, MAX_EVENTS,
    MAX_EVENT_BYTES, MAX_FRAME_BYTES, MAX_TAIL_BYTES, MAX_WALL_MS, RESERVED_EVENTS,
};
use super::reconcile::{is_quiescent, HistoryStatus, QuarantineReason, ReconciledFrame, RecordedHistory};
use super::reduce::{apply, RunProjection};

/// The single grant made for one granted approval, consumed by value by `stage`.
#[must_use]
pub(crate) struct LiveGrant {
    epoch: LiveEpoch,
    run: RunId,
    attempt: AttemptNo,
    approval: ApprovalId,
    kind: ApprovalKind,
    /// The request as recorded: its binding and binding digest are what the grant is valid for.
    request: ApprovalRequest,
    use_by: MonoMs,
}

impl LiveGrant {
    pub(crate) const fn approval(&self) -> ApprovalId {
        self.approval
    }

    pub(crate) const fn attempt(&self) -> AttemptNo {
        self.attempt
    }

    pub(crate) const fn kind(&self) -> ApprovalKind {
        self.kind
    }

    pub(crate) const fn use_by(&self) -> MonoMs {
        self.use_by
    }
}

/// The live supervision of one run. Memory only; dies with the process.
pub(crate) struct LiveSupervision {
    epoch: LiveEpoch,
    projection: RunProjection,
    /// Monotonic input must never decrease; it is never lowered.
    last_mono: MonoMs,
    /// Entered on a monotonic regression and never left: no decision, consumption or grant, only safety drafts.
    clock_fault: bool,
    /// Entered for good when the journal may be out of capacity (D3.2c-1): a stated size at the absolute ceiling, or a
    /// protected draft refused with `LimitExceeded`. Memory only, for this supervision's lifetime; never cleared, by a
    /// smaller stated size or anything else. A faulted supervision mints and accepts no grant and no decision, stages
    /// only safety drafts, and its claim on its run excuses nothing in `start_conflicts`: the run then blocks every start
    /// while it is held. It is not a durable quarantine and records nothing.
    capacity_fault: bool,
    /// Frames staged so far; the number, with the frame's events, identifies the outstanding one.
    frames: u64,
    in_flight: Option<(u64, Vec<Event>)>,
    /// An abandoned frame not yet adopted or discarded (F8): nothing can be staged until it is.
    unresolved: Option<(u64, Vec<Event>)>,
    approvals: Vec<ApprovalClock>,
    attempts: Vec<AttemptClock>,
    descriptions: Vec<(ApprovalId, OperationDescription)>,
    /// Approvals with a minted, unconsumed grant.
    outstanding: Vec<ApprovalId>,
    /// The canonical bytes of every frame this supervision knows is in the journal (D3.2c-2): the replayed base, a torn
    /// tail included, and each frame confirmed or adopted since. Never lowered.
    bytes: u64,
    /// Uncertain appends (`abandon`) since the last confirmed one. Reaching `MAX_CONSECUTIVE_UNCERTAIN` poisons.
    uncertain: u32,
    /// Entered for good when the stream can no longer be trusted (r5 5.4): an outcome `Diverged` or `Unknown`, or too
    /// many consecutive uncertain appends. A poisoned supervision stages, verifies, adopts and discards nothing, and its
    /// claim on its run excuses nothing (`JournalPoisoned`). Memory only; the durable quarantine marker is the adapter's.
    poisoned: bool,
    /// A torn tail a verification showed and no confirmed `JournalRepaired` has recorded yet: only `stage_repair`.
    owed: Option<OwedRepair>,
    /// What the first verification of the unresolved frame showed: a later one that shows anything else is
    /// contradictory evidence, and poisons.
    checked: Option<PersistCheck>,
}

/// A torn tail a verification of this supervision measured.
#[derive(Clone, Copy, PartialEq, Eq)]
struct OwedRepair {
    frame: u64,
    len: u32,
    crc32c: u32,
}

struct ApprovalClock {
    approval: ApprovalId,
    requested: MonoMs,
    use_by: Option<MonoMs>,
}

struct AttemptClock {
    attempt: AttemptNo,
    begun: Option<MonoMs>,
    started: Option<MonoMs>,
    last_progress: Option<MonoMs>,
}

/// A run being created: its `RunCreated` frame is staged and not yet durable. It holds no grant and accepts no draft.
#[must_use]
pub(crate) struct PendingSupervision {
    live: LiveSupervision,
    events: Vec<Event>,
}

/// A frame staged for writing. Consumed by exactly one of `confirm` (durable) or `abandon` (uncertain or failed);
/// dropping it leaves the supervision unable to stage anything. Its `events` are empty when the whole batch was a
/// duplicate or was coalesced: nothing is written, and it is still confirmed.
#[must_use]
pub(crate) struct Staged {
    epoch: LiveEpoch,
    frame: u64,
    events: Vec<Event>,
    projection: RunProjection,
    mono: MonoMs,
    /// Attempts that reported progress in this batch, coalesced reports included.
    progress: Vec<AttemptNo>,
    descriptions: Vec<(ApprovalId, OperationDescription)>,
    /// The grant this frame consumes; dropped, never returned, once the frame is confirmed or abandoned.
    grant: Option<LiveGrant>,
    /// The canonical encoding of `events` (empty when they are): exactly what the adapter writes.
    bytes: Vec<u8>,
}

/// A frame whose write was uncertain. It mints nothing; `adopt` (it landed) or `discard` (it is absent) resolves it.
#[must_use]
pub(crate) struct Abandoned {
    epoch: LiveEpoch,
    frame: u64,
    events: Vec<Event>,
    projection: RunProjection,
    mono: MonoMs,
    progress: Vec<AttemptNo>,
    descriptions: Vec<(ApprovalId, OperationDescription)>,
    bytes: Vec<u8>,
}

/// What a re-read journal showed about one abandoned frame (D3.2c-2, S3). Made only by `verify_write`, for exactly one
/// supervision, run and frame, with the bytes it expected; consumed by `adopt` or `discard`. It is evidence about bytes,
/// never about durability, and never authority.
#[must_use]
pub(crate) struct WriteOutcome {
    epoch: LiveEpoch,
    run: RunId,
    frame: u64,
    expected: Vec<u8>,
    check: PersistCheck,
}

impl WriteOutcome {
    pub(crate) const fn check(&self) -> PersistCheck {
        self.check
    }
}

/// A torn tail shown by a verification of this supervision (D3.2c-2, N7): its length and CRC-32C as measured. Made only
/// by `discard`; the adapter moves exactly these bytes to a new sidecar, and `stage_repair` records them.
#[must_use]
pub(crate) struct TornTailEvidence {
    epoch: LiveEpoch,
    run: RunId,
    owed: OwedRepair,
}

impl TornTailEvidence {
    pub(crate) const fn len(&self) -> u32 {
        self.owed.len
    }

    pub(crate) const fn crc32c(&self) -> u32 {
        self.owed.crc32c
    }
}

/// Why the live layer refused a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveRefusal {
    /// The reducer refused the batch.
    Rejected(Rejection),
    /// A staged frame is still outstanding.
    InFlight,
    /// An abandoned frame has not been adopted or discarded (F8).
    Unresolved,
    /// The supervision is in `clock_fault`: only stop, cancel, end, expiry, void, start-identity and repair drafts.
    ClockFault,
    /// The supervision is capacity-faulted (D3.2c-1): the same drafts as in `clock_fault`, and no grant.
    CapacityFault,
    /// A decision or consumption outside its monotonic window, or a window that cannot be computed.
    WindowClosed,
    /// A consumption without a grant.
    GrantRequired,
    /// A grant with no consumption to use it.
    GrantUnexpected,
    /// A grant that does not match this supervision, the consumption, or an outstanding grant; or more than one
    /// consumption in a batch.
    GrantMismatch,
    /// A staged or abandoned frame that is not this supervision's outstanding one.
    NotThisSupervision,
    /// A reconciler draft: a supervised run is never reconciled (r5 5.4); reconciliation opens a run instead, through
    /// `begin_reconciled`.
    Reconciliation,
    /// The supervision is poisoned (D3.2c-2): nothing more is staged or resolved through it.
    Poisoned,
    /// The committed bytes do not decode, as a clean stream, to exactly this supervision's recorded events.
    NotThisJournal,
    /// The outcome is for another supervision, run or frame, or does not show what the call needs: `Landed` to adopt,
    /// `Absent` to discard.
    OutcomeMismatch,
    /// A torn tail is owed its repair record: only `stage_repair`.
    RepairOwed,
    /// A repair without matching evidence: a journal draft through `stage`, evidence of another tail, a sidecar not
    /// above every recorded one, or a file that still holds the tail.
    RepairUnshown,
    /// The reducer accepted a frame the codec cannot encode; nothing is staged.
    Unencodable,
}

/// A refused `stage`. The grant, if one was handed over, comes back: it stays single-use because only one exists.
pub(crate) struct StageRejected {
    pub(crate) refusal: LiveRefusal,
    pub(crate) grant: Option<LiveGrant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClockCheck {
    Monotonic,
    Regressed,
}

/// What `due` proposes. Proposals only: the caller stages them.
pub(crate) struct Due {
    pub(crate) drafts: Vec<EventDraft>,
    pub(crate) clock: ClockCheck,
}

/// The kinds accepted while in `clock_fault` or a capacity fault: the supervisor can still stop and end its own child
/// (r5 6.5), record the identity of a child that started meanwhile, and record a repair (F2, D3.2c-1).
const fn safe_in_clock_fault(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::StopRequested
            | EventKind::CancelRequested
            | EventKind::AttemptExited
            | EventKind::StartFailed
            | EventKind::ApprovalExpired
            | EventKind::ApprovalWithdrawn
            | EventKind::ApprovalVoided
            | EventKind::AttemptStarted
            | EventKind::JournalRepaired
    )
}

/// A draft the safety tier admits (D3.2c-1, as the reducer's `protected`): its refusal for capacity faults the supervision.
fn protected_draft(draft: &EventDraft) -> bool {
    match draft.body() {
        EventBody::AttentionResolved(resolved) => resolved.concern().kind().class() == ConcernClass::Blocking,
        EventBody::JournalRepaired(_) => true,
        body => body.kind().safety(),
    }
}

/// `a + b` in monotonic time; an overflowing deadline is already passed (fail closed).
fn deadline(start: MonoMs, duration: u64) -> MonoMs {
    MonoMs::new(start.ms().checked_add(duration).unwrap_or(0))
}

/// The one place a supervision is built: from a projection, with no clock, description or grant of any earlier
/// supervision.
fn supervision(epoch: LiveEpoch, projection: RunProjection, mono: MonoMs) -> LiveSupervision {
    LiveSupervision {
        epoch,
        projection,
        last_mono: mono,
        clock_fault: false,
        capacity_fault: false,
        frames: 0,
        in_flight: None,
        unresolved: None,
        approvals: Vec::new(),
        attempts: Vec::new(),
        descriptions: Vec::new(),
        outstanding: Vec::new(),
        bytes: 0,
        uncertain: 0,
        poisoned: false,
        owed: None,
        checked: None,
    }
}

/// The canonical bytes of a frame; nothing for an empty one.
fn encoded(events: &[Event]) -> Result<Vec<u8>, LiveRefusal> {
    if events.is_empty() {
        return Ok(Vec::new());
    }
    encode_frame(events).map_err(|_| LiveRefusal::Unencodable)
}

fn encoded_len(events: &[Event]) -> Option<u64> {
    encoded(events).ok().map(|bytes| bytes.len() as u64)
}

/// The one place a pending supervision is built.
fn pending(live: LiveSupervision, events: Vec<Event>) -> PendingSupervision {
    PendingSupervision { live, events }
}

/// Starts supervising a new run from its `RunCreated` draft. The result becomes live only through `open`, after the
/// frame it holds is durable.
pub(crate) fn begin_new(epoch: LiveEpoch, run: RunId, run_created: EventDraft, mono: MonoMs) -> Result<PendingSupervision, Rejection> {
    if run_created.body().kind() != EventKind::RunCreated {
        return Err(Rejection::InvalidTransition);
    }
    let applied = apply(&RunProjection::empty(run), &[run_created], JournalSize { events: 0, bytes: 0 })?;
    let mut live = supervision(epoch, applied.projection, mono);
    live.bytes = encoded_len(&applied.events).ok_or(Rejection::InvalidDraft)?;
    Ok(pending(live, applied.events))
}

/// That the caller holds an opened supervision of `run`, for `start_conflicts`. It borrows the supervision, so it exists
/// only while that supervision does, and exposes nothing but the run id.
pub(crate) struct SupervisedRun<'a> {
    live: &'a LiveSupervision,
}

impl SupervisedRun<'_> {
    pub(crate) fn run(&self) -> RunId {
        self.live.projection.run()
    }

    /// Whether the supervision is capacity-faulted (D3.2c-1): its claim then excuses nothing, and the run blocks every
    /// start while it is held.
    pub(crate) fn capacity_faulted(&self) -> bool {
        self.live.capacity_fault
    }

    /// Whether the supervision is poisoned (D3.2c-2): its claim then excuses nothing, and the run blocks every start.
    pub(crate) fn poisoned(&self) -> bool {
        self.live.poisoned
    }

    /// Whether a torn tail is owed its repair record (D3.2c-2).
    pub(crate) fn repair_owed(&self) -> bool {
        self.live.owed.is_some()
    }

    /// Whether this supervision's history records `repair` with the same content.
    pub(crate) fn records(&self, repair: &JournalRepair) -> bool {
        self.live.projection.events().iter().any(|event| matches!(&event.body, EventBody::JournalRepaired(recorded) if recorded == repair))
    }

    /// Whether this supervision's recorded history starts with `events`: it recorded what was summarized.
    pub(crate) fn extends(&self, events: &[Event]) -> bool {
        self.live.projection.events().starts_with(events)
    }

    /// The unresolved blocking concerns this supervision holds, which may be newer than any summary of the run.
    pub(crate) fn blocking(&self) -> Vec<ConcernRef> {
        self.live.projection.concerns().iter().filter(|concern| concern.is_unresolved_blocking()).map(|concern| concern.concern()).collect()
    }
}

/// How a reconciled run opens: at once when reconciliation had nothing to record, or after its opening frame is
/// durable (`open`; `drop_pending` if the write was uncertain or failed).
pub(crate) enum Opening {
    Ready(LiveSupervision),
    Pending(PendingSupervision),
}

/// Why `begin_reconciled` refused to open a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BeginRefused {
    /// The history is quarantined: read-only, never opened.
    Quarantined(QuarantineReason),
    /// The history holds no `RunCreated`.
    NoRun,
    /// The frame was computed for another history (status, projection or missing repairs) or epoch, or `mono` is earlier
    /// than the frame's. A later `mono` is accepted however late: every interruption blocks anyway.
    NotThisHistory,
    /// A draft of the frame is neither a repair record nor a reconciler draft.
    NotReconciliation,
    /// The reducer refused the frame.
    Rejected(Rejection),
    /// After the frame something would still be in flight or owed: an attempt that is not terminal, a pending request,
    /// an unused grant, or an unrecorded repair.
    NotQuiescent,
}

/// Opens a recorded run for writing after reconciliation (r5 6.5 and 5.7 step 6). The history must be verified and hold
/// its `RunCreated`; the frame must be the one `reconcile` computed for exactly this history and epoch; and the run must
/// be quiescent after it. The frame is applied here, through the reducer, and never through `stage`, which keeps
/// refusing reconciler drafts.
///
/// The new supervision starts with no approval clock, no description and no outstanding grant: nothing recorded
/// before it, a granted decision included, can be consumed through it. Any later attempt needs a new request, a new
/// decision staged and confirmed by this supervision, and only then a grant. This core cannot observe durability:
/// the caller must call `open` only after the adapter reports the opening frame durable, and no such adapter exists yet.
pub(crate) fn begin_reconciled(epoch: LiveEpoch, history: RecordedHistory, frame: ReconciledFrame, mono: MonoMs, size: JournalSize) -> Result<Opening, BeginRefused> {
    if let HistoryStatus::Quarantined(reason) = history.status() {
        return Err(BeginRefused::Quarantined(reason));
    }
    if history.projection().spec().is_none() {
        return Err(BeginRefused::NoRun);
    }
    if frame.epoch() != epoch || mono < frame.mono() || !frame.computed_for(&history) {
        return Err(BeginRefused::NotThisHistory);
    }
    if frame.drafts().iter().any(|draft| !matches!(draft.producer(), Producer::Journal | Producer::Reconciler)) {
        return Err(BeginRefused::NotReconciliation);
    }
    // The history's own events and bytes are the floor of the size (D3.2c-2): a stated size can only raise it.
    let size = history.floor(size).raised(frame.size());
    let (projection, events) = if frame.drafts().is_empty() {
        (history.projection().clone(), Vec::new())
    } else {
        let applied = apply(history.projection(), frame.drafts(), size).map_err(BeginRefused::Rejected)?;
        (applied.projection, applied.events)
    };
    if !is_quiescent(&projection, history.unrecorded_repairs()) {
        return Err(BeginRefused::NotQuiescent);
    }
    let mut live = supervision(epoch, projection, mono);
    // The replayed bytes, a torn tail included (removed by the opening frame's repair, but counted for this lifetime),
    // and the opening frame's.
    let opening = encoded_len(&events).ok_or(BeginRefused::Rejected(Rejection::InvalidDraft))?;
    live.bytes = history.journal_bytes().saturating_add(opening);
    // A run opened at the ceiling opens capacity-faulted (D3.2c-1).
    live.capacity_fault = live.stated(size).exhausted();
    Ok(if events.is_empty() { Opening::Ready(live) } else { Opening::Pending(pending(live, events)) })
}

impl PendingSupervision {
    /// The `RunCreated` frame to write.
    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }

    /// How durable the opening frame must be before `open` (D3.2c-2): the strictest class among its events.
    pub(crate) fn durability(&self) -> Durability {
        frame_durability(&self.events)
    }
}

/// Call only after the pending frame is durable.
pub(crate) fn open(pending: PendingSupervision) -> LiveSupervision {
    pending.live
}

/// The pending frame's write was uncertain or failed: nothing becomes live. The caller releases the run and starts over.
pub(crate) fn drop_pending(pending: PendingSupervision) {
    let PendingSupervision { live: _, events: _ } = pending;
}

impl LiveSupervision {
    pub(crate) fn projection(&self) -> &RunProjection {
        &self.projection
    }

    /// That this supervision holds its run, for cross-run start conflicts.
    pub(crate) fn held(&self) -> SupervisedRun<'_> {
        SupervisedRun { live: self }
    }

    pub(crate) const fn in_clock_fault(&self) -> bool {
        self.clock_fault
    }

    pub(crate) const fn in_capacity_fault(&self) -> bool {
        self.capacity_fault
    }

    pub(crate) const fn poisoned(&self) -> bool {
        self.poisoned
    }

    /// The journal size this supervision derives (D3.2c-2): its events, and the canonical bytes of its known frames plus
    /// an owed torn tail.
    pub(crate) fn journal_size(&self) -> JournalSize {
        let events = u32::try_from(self.projection.events().len()).unwrap_or(u32::MAX);
        let tail = self.owed.map_or(0, |owed| u64::from(owed.len));
        JournalSize { events, bytes: self.bytes.saturating_add(tail) }
    }

    /// The size used for every check: the derived one, raised (never lowered) by the caller's stated size.
    fn stated(&self, size: JournalSize) -> JournalSize {
        size.raised(self.journal_size())
    }

    /// Whether `committed` is exactly this supervision's recorded history as a clean stream of frames.
    fn holds(&self, committed: &[u8]) -> bool {
        let decoded = decode_stream(committed);
        decoded.end() == StreamEnd::Clean && decoded.frames().iter().flat_map(|frame| frame.events()).eq(self.projection.events().iter())
    }

    /// The memory-only description of a pending approval.
    pub(crate) fn description(&self, approval: ApprovalId) -> Option<&OperationDescription> {
        self.descriptions.iter().find(|(id, _)| *id == approval).map(|(_, text)| text)
    }

    /// Checks a monotonic reading: a regression enters `clock_fault` for good. Returns the reading to use, which is
    /// never lower than any earlier one.
    fn observe(&mut self, mono: MonoMs) -> (MonoMs, ClockCheck) {
        if mono < self.last_mono {
            self.clock_fault = true;
            return (self.last_mono, ClockCheck::Regressed);
        }
        self.last_mono = mono;
        (mono, ClockCheck::Monotonic)
    }

    fn approval_clock(&self, approval: ApprovalId) -> Option<&ApprovalClock> {
        self.approvals.iter().find(|clock| clock.approval == approval)
    }

    fn attempt_clock(&mut self, attempt: AttemptNo) -> &mut AttemptClock {
        if let Some(index) = self.attempts.iter().position(|clock| clock.attempt == attempt) {
            return &mut self.attempts[index];
        }
        self.attempts.push(AttemptClock { attempt, begun: None, started: None, last_progress: None });
        let last = self.attempts.len() - 1;
        &mut self.attempts[last]
    }

    fn attempt_times(&self, attempt: AttemptNo) -> Option<&AttemptClock> {
        self.attempts.iter().find(|clock| clock.attempt == attempt)
    }

    /// Stages one batch as one frame: the reducer's rules plus the live checks (monotonic windows, possession of the
    /// matching grant). Mints nothing. At most one frame is outstanding.
    pub(crate) fn stage(&mut self, drafts: &[EventDraft], grant: Option<LiveGrant>, mono: MonoMs, size: JournalSize) -> Result<Staged, StageRejected> {
        self.stage_frame(drafts, grant, mono, size, false)
    }

    /// Records the repair of the torn tail `evidence` shows (D3.2c-2, N7). The record is built here from the measured
    /// length and checksum, never from the caller's figures. `committed` must be exactly this supervision's frames and
    /// `file`, re-read, exactly `committed`: the tail is gone from the journal. The sidecar must come after every
    /// recorded one. That the sidecar exists and holds those bytes is the adapter's (F5), and so is the truncation's
    /// durability. The repair is owed until this frame is confirmed; an uncertain write of it is a second consecutive
    /// uncertain append, which poisons.
    pub(crate) fn stage_repair(
        &mut self,
        evidence: &TornTailEvidence,
        sidecar: u32,
        committed: &[u8],
        file: &[u8],
        stated_at_ms: u64,
        mono: MonoMs,
        size: JournalSize,
    ) -> Result<Staged, LiveRefusal> {
        if self.poisoned {
            return Err(LiveRefusal::Poisoned);
        }
        if self.in_flight.is_some() {
            return Err(LiveRefusal::InFlight);
        }
        if self.unresolved.is_some() {
            return Err(LiveRefusal::Unresolved);
        }
        let shown = evidence.epoch == self.epoch && evidence.run == self.projection.run() && self.owed == Some(evidence.owed);
        let newest = self
            .projection
            .events()
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::JournalRepaired(recorded) => Some(recorded.sidecar()),
                _ => None,
            })
            .max();
        if !shown || newest.is_some_and(|newest| sidecar <= newest) {
            return Err(LiveRefusal::RepairUnshown);
        }
        if !self.holds(committed) {
            return Err(LiveRefusal::NotThisJournal);
        }
        if file != committed {
            return Err(LiveRefusal::RepairUnshown);
        }
        let repair = JournalRepair::new(sidecar, evidence.owed.len, evidence.owed.crc32c, RepairCause::IncompleteTail)
            .map_err(|_| LiveRefusal::RepairUnshown)?;
        let draft = EventDraft::journal(JournalDraft::Repaired(repair), stated_at_ms);
        self.stage_frame(&[draft], None, mono, size, true).map_err(|rejected| rejected.refusal)
    }

    /// `repair` is set only by `stage_repair`, for the one repair draft it built from evidence.
    fn stage_frame(&mut self, drafts: &[EventDraft], grant: Option<LiveGrant>, mono: MonoMs, size: JournalSize, repair: bool) -> Result<Staged, StageRejected> {
        let (mono, _) = self.observe(mono);
        if self.stated(size).exhausted() {
            self.capacity_fault = true;
        }
        let refuse = |refusal, grant| Err(StageRejected { refusal, grant });
        if self.poisoned {
            return refuse(LiveRefusal::Poisoned, grant);
        }
        if self.in_flight.is_some() {
            return refuse(LiveRefusal::InFlight, grant);
        }
        if self.unresolved.is_some() {
            return refuse(LiveRefusal::Unresolved, grant);
        }
        // A repair is recorded only with the evidence of the tail it removes (N7).
        if !repair && self.owed.is_some() {
            return refuse(LiveRefusal::RepairOwed, grant);
        }
        if !repair && drafts.iter().any(|draft| draft.producer() == Producer::Journal) {
            return refuse(LiveRefusal::RepairUnshown, grant);
        }
        if drafts.iter().any(|draft| draft.producer() == Producer::Reconciler) {
            return refuse(LiveRefusal::Reconciliation, grant);
        }
        if self.clock_fault && (grant.is_some() || drafts.iter().any(|draft| !safe_in_clock_fault(draft.body().kind()))) {
            return refuse(LiveRefusal::ClockFault, grant);
        }
        if self.capacity_fault && (grant.is_some() || drafts.iter().any(|draft| !safe_in_clock_fault(draft.body().kind()))) {
            return refuse(LiveRefusal::CapacityFault, grant);
        }
        let consumptions: Vec<&EventDraft> =
            drafts.iter().filter(|draft| matches!(draft.body().kind(), EventKind::StartBegun | EventKind::ApprovalConsumed)).collect();
        match (consumptions.as_slice(), &grant) {
            ([], None) => {}
            ([], Some(_)) => return refuse(LiveRefusal::GrantUnexpected, grant),
            ([_], None) => return refuse(LiveRefusal::GrantRequired, grant),
            ([consumption], Some(held)) => {
                if let Err(refusal) = self.check_grant(consumption, held, mono) {
                    return refuse(refusal, grant);
                }
            }
            _ => return refuse(LiveRefusal::GrantMismatch, grant),
        }
        for draft in drafts {
            if let EventBody::ApprovalDecided { approval, .. } = draft.body() {
                if !self.decision_window_open(*approval, mono) {
                    return refuse(LiveRefusal::WindowClosed, grant);
                }
            }
        }
        let applied = match apply(&self.projection, drafts, self.stated(size)) {
            Ok(applied) => applied,
            Err(rejection) => {
                // A protected draft refused for capacity (D3.2c-1): its stop, end or repair could not be recorded, so
                // nothing about this run may be taken as settled while it is supervised. Conservative: a protected
                // draft in an oversized batch faults too.
                if rejection == Rejection::LimitExceeded && drafts.len() <= MAX_BATCH && drafts.iter().any(protected_draft) {
                    self.capacity_fault = true;
                }
                return refuse(LiveRefusal::Rejected(rejection), grant);
            }
        };
        let progress = drafts
            .iter()
            .filter_map(|draft| match draft.body() {
                EventBody::Progress { attempt, .. } => Some(*attempt),
                _ => None,
            })
            .collect();
        let descriptions = drafts
            .iter()
            .filter_map(|draft| match (draft.body(), draft.description()) {
                (EventBody::ApprovalRequested(request), Some(text)) => Some((request.fields().approval, text.clone())),
                _ => None,
            })
            .filter(|(approval, _)| applied.events.iter().any(|event| matches!(&event.body, EventBody::ApprovalRequested(request) if request.fields().approval == *approval)))
            .collect();
        // A frame that brings the journal to a ceiling leaves no room for what may follow (the end of the attempt it
        // stops, for example): the supervision faults now, whether the frame is then confirmed or abandoned (review of
        // #120). Bytes are counted at the largest a frame of these events can take: each event at its largest with its
        // separator, the payload's brackets, and the frame's header and newline.
        let stated = self.stated(size);
        let count = u32::try_from(applied.events.len()).unwrap_or(u32::MAX);
        let largest = u64::from(count) * (u64::from(MAX_EVENT_BYTES) + 1) + 1 + u64::from(MAX_TAIL_BYTES - MAX_FRAME_BYTES);
        let after = JournalSize { events: stated.events.saturating_add(count), bytes: stated.bytes.saturating_add(largest) };
        if after.exhausted() {
            self.capacity_fault = true;
        }
        let bytes = match encoded(&applied.events) {
            Ok(bytes) => bytes,
            Err(refusal) => return refuse(refusal, grant),
        };
        self.frames += 1;
        self.in_flight = Some((self.frames, applied.events.clone()));
        Ok(Staged { epoch: self.epoch, frame: self.frames, events: applied.events, projection: applied.projection, mono, progress, descriptions, grant, bytes })
    }

    /// The grant must be this supervision's, for this run, attempt, approval, kind and exact binding, still
    /// outstanding, and inside its monotonic use window.
    fn check_grant(&self, consumption: &EventDraft, grant: &LiveGrant, mono: MonoMs) -> Result<(), LiveRefusal> {
        let (approval, binding, kind, attempt) = match consumption.body() {
            EventBody::StartBegun { attempt, approval, binding } => (*approval, binding, ApprovalKind::Start, Some(*attempt)),
            EventBody::ApprovalConsumed { approval, binding } => (*approval, binding, ApprovalKind::Operation, None),
            _ => return Err(LiveRefusal::GrantMismatch),
        };
        let recorded = self.projection.approval(approval).ok_or(LiveRefusal::GrantMismatch)?;
        let matches = grant.epoch == self.epoch
            && grant.run == self.projection.run()
            && grant.approval == approval
            && grant.kind == kind
            && attempt.map_or(true, |attempt| attempt == grant.attempt)
            && recorded.attempt() == grant.attempt
            && grant.request == *recorded.request()
            && *binding == grant.request.fields().binding
            && self.outstanding.contains(&approval);
        if !matches {
            return Err(LiveRefusal::GrantMismatch);
        }
        if mono >= grant.use_by {
            return Err(LiveRefusal::WindowClosed);
        }
        Ok(())
    }

    /// A decision is accepted only while `mono < requested + ttl`, for a request this supervision recorded.
    fn decision_window_open(&self, approval: ApprovalId, mono: MonoMs) -> bool {
        let (Some(clock), Some(recorded)) = (self.approval_clock(approval), self.projection.approval(approval)) else {
            return false;
        };
        mono < deadline(clock.requested, recorded.request().fields().ttl_ms)
    }

    /// The staged frame is durable: the live projection advances and a grant is minted for each decision this frame
    /// newly granted, if the approval is still granted after the frame (F13). Call only after the adapter reports the
    /// frame durable.
    pub(crate) fn confirm(&mut self, staged: Staged) -> Result<Vec<LiveGrant>, LiveRefusal> {
        if !self.is_next(staged.epoch, &staged.projection, &staged.events, self.in_flight.as_ref(), staged.frame) {
            return Err(LiveRefusal::NotThisSupervision);
        }
        self.in_flight = None;
        let Staged { events, projection, mono, progress, descriptions, grant, bytes, .. } = staged;
        if let Some(grant) = grant {
            self.outstanding.retain(|approval| *approval != grant.approval);
        }
        // A durable append ends a run of uncertain ones; a confirmed repair pays what was owed (only `stage_repair` can
        // stage one while a repair is owed).
        self.uncertain = 0;
        if events.iter().any(|event| matches!(event.body, EventBody::JournalRepaired(_))) {
            self.owed = None;
        }
        self.bytes = self.bytes.saturating_add(bytes.len() as u64);
        Ok(self.settle(&events, projection, mono, &progress, descriptions, true))
    }

    /// The staged frame's write was uncertain or failed. Nothing is minted, a consumed grant is dropped (consumptions
    /// are never retried), and the supervision stays unresolved until `adopt` or `discard` (F8). The second uncertain
    /// append in a row, with no confirmed one between, poisons the supervision (r5 5.4); the frame is still returned so
    /// that nothing is left outstanding, and nothing can resolve it.
    pub(crate) fn abandon(&mut self, staged: Staged) -> Result<Abandoned, LiveRefusal> {
        if !self.is_next(staged.epoch, &staged.projection, &staged.events, self.in_flight.as_ref(), staged.frame) {
            return Err(LiveRefusal::NotThisSupervision);
        }
        self.unresolved = self.in_flight.take();
        self.checked = None;
        self.uncertain = self.uncertain.saturating_add(1);
        if self.uncertain >= MAX_CONSECUTIVE_UNCERTAIN {
            self.poisoned = true;
        }
        let Staged { epoch, frame, events, projection, mono, progress, descriptions, grant, bytes } = staged;
        if let Some(grant) = grant {
            self.outstanding.retain(|approval| *approval != grant.approval);
        }
        Ok(Abandoned { epoch, frame, events, projection, mono, progress, descriptions, bytes })
    }

    /// Compares the re-read journal with the abandoned frame (S3). `committed` must decode, as a clean stream, to exactly
    /// this supervision's recorded events (framed otherwise, the comparison then diverges and poisons); `file` is the journal as the adapter re-read it under the run's lock, which this core cannot
    /// check. The expected bytes are this supervision's own encoding of the frame, never the caller's. A `Diverged` or
    /// `Unknown` result poisons the supervision at once, whatever the caller does with the outcome next, and so do a torn
    /// tail that is not a prefix of the frame and a second verification of the same frame that shows anything other
    /// than the first.
    pub(crate) fn verify_write(&mut self, abandoned: &Abandoned, committed: &[u8], file: &[u8]) -> Result<WriteOutcome, LiveRefusal> {
        if self.poisoned {
            return Err(LiveRefusal::Poisoned);
        }
        if !self.is_next(abandoned.epoch, &abandoned.projection, &abandoned.events, self.unresolved.as_ref(), abandoned.frame) {
            return Err(LiveRefusal::NotThisSupervision);
        }
        if !self.holds(committed) {
            return Err(LiveRefusal::NotThisJournal);
        }
        let expected = encoded(&abandoned.events)?;
        let check = if expected.is_empty() {
            // Nothing was to be written: the file must be exactly the committed bytes.
            match file.strip_prefix(committed) {
                Some([]) => PersistCheck::Absent { torn: None },
                Some(_) => PersistCheck::Diverged(Divergence::Foreign),
                None if committed.starts_with(file) => PersistCheck::Diverged(Divergence::Shortened),
                None => PersistCheck::Diverged(Divergence::Altered),
            }
        } else {
            verify_persisted(file, committed, &expected)
        };
        // The same frame re-read twice must show the same thing: a caller cannot shop for the outcome it wants.
        // A torn tail that is not a prefix of the frame written is bytes this supervision never wrote: foreign, as after an
        // empty frame (review of #122).
        let foreign_tail = matches!(check, PersistCheck::Absent { torn: Some(tail) } if !tail.pending_prefix);
        if matches!(check, PersistCheck::Diverged(_) | PersistCheck::Unknown(_)) || foreign_tail || self.checked.is_some_and(|seen| seen != check) {
            self.poisoned = true;
        }
        self.checked = Some(check);
        Ok(WriteOutcome { epoch: self.epoch, run: self.projection.run(), frame: abandoned.frame, expected, check })
    }

    /// Whether `outcome` was made by this supervision for exactly `abandoned`.
    fn outcome_for(&self, outcome: &WriteOutcome, abandoned: &Abandoned) -> bool {
        outcome.epoch == self.epoch && outcome.run == self.projection.run() && outcome.frame == abandoned.frame && outcome.expected == abandoned.bytes
    }

    /// The abandoned frame is in the file exactly (`outcome` is `Landed` for this frame): the file is the truth, so the
    /// live projection takes it. Nothing is minted; the caller records what r5 5.4 requires (for example
    /// `ApprovalVoided{write_uncertain}` for a landed grant). The append still counts as uncertain.
    pub(crate) fn adopt(&mut self, abandoned: Abandoned, outcome: WriteOutcome) -> Result<(), LiveRefusal> {
        if self.poisoned {
            return Err(LiveRefusal::Poisoned);
        }
        if !self.is_next(abandoned.epoch, &abandoned.projection, &abandoned.events, self.unresolved.as_ref(), abandoned.frame) {
            return Err(LiveRefusal::NotThisSupervision);
        }
        if !self.outcome_for(&outcome, &abandoned) || outcome.check != PersistCheck::Landed {
            return Err(LiveRefusal::OutcomeMismatch);
        }
        self.unresolved = None;
        let Abandoned { events, projection, mono, progress, descriptions, bytes, .. } = abandoned;
        self.bytes = self.bytes.saturating_add(bytes.len() as u64);
        self.settle(&events, projection, mono, &progress, descriptions, false);
        Ok(())
    }

    /// The abandoned frame is absent from the file (`outcome` is `Absent` for this frame): the live projection does not
    /// move, and its non-consumption drafts may be staged again. A torn tail is returned as evidence and owed its repair
    /// (`stage_repair`) before anything else is staged; it counts toward the journal's size until then.
    pub(crate) fn discard(&mut self, abandoned: Abandoned, outcome: WriteOutcome) -> Result<Option<TornTailEvidence>, LiveRefusal> {
        if self.poisoned {
            return Err(LiveRefusal::Poisoned);
        }
        if !self.is_next(abandoned.epoch, &abandoned.projection, &abandoned.events, self.unresolved.as_ref(), abandoned.frame) {
            return Err(LiveRefusal::NotThisSupervision);
        }
        if !self.outcome_for(&outcome, &abandoned) {
            return Err(LiveRefusal::OutcomeMismatch);
        }
        let PersistCheck::Absent { torn } = outcome.check else { return Err(LiveRefusal::OutcomeMismatch) };
        self.unresolved = None;
        Ok(torn.map(|tail| {
            let owed = OwedRepair { frame: abandoned.frame, len: tail.len, crc32c: tail.crc32c };
            self.owed = Some(owed);
            TornTailEvidence { epoch: self.epoch, run: self.projection.run(), owed }
        }))
    }

    /// Whether a staged or abandoned frame is the one this supervision holds outstanding (`expected`): same epoch, same
    /// frame number and events, same run, and a projection that is the current one plus exactly that frame. Epochs are
    /// unique only by the caller's promise; this keeps a frame of another supervision from replacing this one's history.
    fn is_next(&self, epoch: LiveEpoch, projection: &RunProjection, events: &[Event], expected: Option<&(u64, Vec<Event>)>, frame: u64) -> bool {
        let current = self.projection.events();
        epoch == self.epoch
            && expected.is_some_and(|(number, held)| *number == frame && held.as_slice() == events)
            && projection.run() == self.projection.run()
            && projection.events().len() == current.len() + events.len()
            && projection.events().starts_with(current)
    }

    /// Takes a frame into the live projection and records the monotonic clocks it starts. Mints only when `mint` is
    /// set (a confirmed frame) and never in `clock_fault`.
    fn settle(
        &mut self,
        events: &[Event],
        projection: RunProjection,
        mono: MonoMs,
        progress: &[AttemptNo],
        descriptions: Vec<(ApprovalId, OperationDescription)>,
        mint: bool,
    ) -> Vec<LiveGrant> {
        self.projection = projection;
        self.descriptions.extend(descriptions);
        let mut grants = Vec::new();
        for event in events {
            match &event.body {
                EventBody::ApprovalRequested(request) => {
                    self.approvals.push(ApprovalClock { approval: request.fields().approval, requested: mono, use_by: None });
                }
                EventBody::ApprovalDecided { approval, decision: Decision::Granted, .. } => {
                    let Some(index) = self.approvals.iter().position(|clock| clock.approval == *approval) else { continue };
                    let requested = self.approvals[index].requested;
                    let use_by = match (mono.ms().checked_add(GRANT_USE_MS), requested.ms().checked_add(MAX_APPROVAL_LIFETIME_MS)) {
                        (Some(used), Some(cap)) => Some(MonoMs::new(used.min(cap))),
                        _ => None,
                    };
                    self.approvals[index].use_by = Some(use_by.unwrap_or(MonoMs::new(0)));
                    let still_granted = self.projection.approval(*approval).map(|record| record.state()) == Some(ApprovalState::Granted);
                    if let (true, false, false, true, Some(use_by)) = (mint, self.clock_fault, self.capacity_fault, still_granted, use_by) {
                        if let Some(grant) = self.mint(*approval, use_by) {
                            self.outstanding.push(*approval);
                            grants.push(grant);
                        }
                    }
                }
                EventBody::StartBegun { attempt, .. } => self.attempt_clock(*attempt).begun = Some(mono),
                EventBody::AttemptStarted { attempt, .. } => {
                    let clock = self.attempt_clock(*attempt);
                    clock.started = Some(mono);
                    clock.last_progress = Some(mono);
                }
                _ => {}
            }
        }
        for attempt in progress {
            self.attempt_clock(*attempt).last_progress = Some(mono);
        }
        // Descriptions live only while their request is pending; grants only while their approval is granted.
        let projection = &self.projection;
        self.descriptions.retain(|(approval, _)| projection.approval(*approval).map(|record| record.state()) == Some(ApprovalState::Pending));
        self.outstanding.retain(|approval| projection.approval(*approval).map(|record| record.state()) == Some(ApprovalState::Granted));
        grants
    }

    /// The only place a `LiveGrant` is made.
    fn mint(&self, approval: ApprovalId, use_by: MonoMs) -> Option<LiveGrant> {
        let recorded = self.projection.approval(approval)?;
        Some(LiveGrant {
            epoch: self.epoch,
            run: self.projection.run(),
            attempt: recorded.attempt(),
            approval,
            kind: recorded.kind(),
            request: recorded.request().clone(),
            use_by,
        })
    }

    /// Proposes what monotonic time (and the wall clock, which can only close a window early) says is due, at most one
    /// draft per semantic key (F1). Stop reasons: `clock_fault` > `journal_full` > `deadline(start | run | stall)`;
    /// expiry: `clock_fault` > `monotonic` > `wall`. For an attempt it stops, it proposes only the stop: the stop's
    /// generated consequences end that attempt's approvals.
    pub(crate) fn due(&mut self, mono: MonoMs, stated_at_ms: u64, size: JournalSize) -> Due {
        let (mono, clock) = self.observe(mono);
        if self.stated(size).exhausted() {
            self.capacity_fault = true;
        }
        let mut drafts = Vec::new();
        let Some(current) = self.projection.current_attempt() else { return Due { drafts, clock } };
        let attempt = current.attempt();
        let state = current.state();
        // The wall time a draft staged now would be recorded with.
        let previous = self.projection.last_at().map_or(0, WallMs::ms);
        let wall = if stated_at_ms <= MAX_WALL_MS { stated_at_ms.max(previous) } else { previous };
        let started_or_starting = matches!(state, AttemptState::Starting | AttemptState::Running);
        let pending_approval = self
            .projection
            .approvals()
            .iter()
            .any(|record| record.attempt() == attempt && record.state() == ApprovalState::Pending);
        let times = self.attempt_times(attempt);
        let passed = |start: Option<MonoMs>, duration: u64| start.map_or(true, |start| mono >= deadline(start, duration));
        let limits = self.projection.spec().map(|spec| spec.context().limits());
        let in_reserve = self.stated(size).in_reserve() || self.projection.events().len() >= (MAX_EVENTS - RESERVED_EVENTS) as usize;
        let stop = match (started_or_starting, limits) {
            (false, _) | (_, None) => None,
            (true, _) if self.clock_fault => Some(StopReason::ClockFault),
            (true, _) if in_reserve || self.capacity_fault => Some(StopReason::JournalFull),
            (true, Some(limits)) => match state {
                AttemptState::Starting if passed(times.and_then(|t| t.begun), limits.start_ms()) => Some(StopReason::Deadline(DeadlineKind::Start)),
                AttemptState::Running if passed(times.and_then(|t| t.started), limits.run_ms()) => Some(StopReason::Deadline(DeadlineKind::Run)),
                AttemptState::Running if !pending_approval && passed(times.and_then(|t| t.last_progress), limits.stall_ms()) => {
                    Some(StopReason::Deadline(DeadlineKind::Stall))
                }
                _ => None,
            },
        };
        if let Some(reason) = stop {
            if let Ok(draft) = EventDraft::supervisor(SupervisorDraft::StopRequested { attempt, reason }, stated_at_ms) {
                drafts.push(draft);
            }
            return Due { drafts, clock };
        }
        if matches!(state, AttemptState::Stopping(_)) {
            return Due { drafts, clock };
        }
        for record in self.projection.approvals().iter().filter(|record| record.attempt() == attempt) {
            let approval = record.id();
            let clock_of = self.approval_clock(approval);
            let draft = match record.state() {
                ApprovalState::Pending => {
                    let basis = if self.clock_fault {
                        Some(ExpiryBasis::ClockFault)
                    } else if clock_of.map_or(true, |clock| mono >= deadline(clock.requested, record.request().fields().ttl_ms)) {
                        Some(ExpiryBasis::Monotonic)
                    } else if record.decision_closes_at().map_or(true, |closes| wall >= closes) {
                        Some(ExpiryBasis::Wall)
                    } else {
                        None
                    };
                    basis.map(|basis| SupervisorDraft::ApprovalExpired { approval, basis })
                }
                ApprovalState::Granted => {
                    let reason = if self.clock_fault {
                        Some(VoidReason::ClockFault)
                    } else if clock_of.and_then(|clock| clock.use_by).map_or(true, |use_by| mono >= use_by)
                        || record.use_closes_at().map_or(true, |closes| wall >= closes)
                    {
                        Some(VoidReason::UseByPassed)
                    } else {
                        None
                    };
                    reason.map(|reason| SupervisorDraft::ApprovalVoided { approval, reason })
                }
                _ => None,
            };
            if let Some(Ok(draft)) = draft.map(|draft| EventDraft::supervisor(draft, stated_at_ms)) {
                drafts.push(draft);
            }
        }
        Due { drafts, clock }
    }
}

impl Staged {
    /// The frame to write.
    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }

    /// The frame's canonical bytes, exactly what the adapter appends; empty when nothing is to be written.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// How durable the frame must be before it is confirmed (D3.2c-2): the strictest class among its events, generated
    /// consequences included. Classification only; the barriers themselves are the adapter's.
    pub(crate) fn durability(&self) -> Durability {
        frame_durability(&self.events)
    }
}

impl Abandoned {
    /// The frame whose write was uncertain.
    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }
}

#[cfg(test)]
mod tests;
