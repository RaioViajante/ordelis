// The deterministic lifecycle reducer (D3.2b-2, issue #114; design r5 sections 3, 4.3, 4.4 and 4.8, with the gate
// review's F7, F10, F12 and F14; hardened by D3.2c-1, issue #120: a start ends its frame, and a bounded safety tier).
// Pure: it reads no clock, file, environment or randomness, and every time it uses is a value it is given.
//
// Two entry points share one transition function. `apply` takes drafts and records them (it assigns `seq`, `at_ms` and
// `clamped`); `apply_recorded` takes one frame of events exactly as recorded. A batch or frame is applied to a copy, so
// a refusal leaves the projection untouched and nothing to append.
//
// A projection is history: what the journal says happened. It never says that anything may be done, and nothing here
// produces live authority. The live layer (live.rs) owns the only path from a recorded decision to a usable grant.
use super::model::{
    classify, ApprovalId, ApprovalKind, ApprovalRequest, ApprovalState, AttemptNo, AttemptState, CancelScope,
    CoalesceReason, ConcernClass, ConcernKind, ConcernRef, Continuation, Decision, Event, EventBody, EventDraft,
    EventKind, ExhaustedLast, FailureKind, InterruptObservation, JournalFault, JournalSize, NotStartedReason,
    OsIdentity, Producer, Rejection, Resolution, RunId, RunSpec, RunState, SemanticKey, Seq, SpecDigest, StopCause,
    StopReason, StoppedKind, VoidReason, WallMs, WithdrawalReason, GRANT_USE_MS, MAX_APPROVALS_PER_ATTEMPT,
    MAX_APPROVAL_LIFETIME_MS, MAX_ATTEMPT_RECORDS, MAX_BATCH, MAX_EVENTS, MAX_JOURNAL_BYTES, MAX_OPEN_APPROVALS,
    PROGRESS_COALESCE_MS, RECORD_VERSION, RESERVED_EVENTS, RESERVED_JOURNAL_BYTES, SAFETY_EVENTS, SAFETY_JOURNAL_BYTES,
    MAX_REPAIRS,
};

// ---------------------------------------------------------------------------------------------------------------
// Projection

/// One run as its journal says. Built only by this reducer, from the empty projection of its run id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunProjection {
    run: RunId,
    created: Option<Created>,
    /// Every recorded event, in `seq` order.
    events: Vec<Event>,
    /// Each semantic key and the index in `events` of the event that holds it.
    keys: Vec<(SemanticKey, usize)>,
    attempts: Vec<AttemptRecord>,
    approvals: Vec<ApprovalRecord>,
    concerns: Vec<Concern>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Created {
    spec: RunSpec,
    spec_digest: SpecDigest,
    state: RunState,
}

/// One attempt as recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttemptRecord {
    attempt: AttemptNo,
    state: AttemptState,
    continuation: Continuation,
    start_approval: Option<ApprovalId>,
    identity: Option<OsIdentity>,
    stop_recorded: bool,
    failure_claimed: bool,
    last_progress_at: Option<WallMs>,
    /// The event that put the attempt into its current state, approval-driven moves included.
    last_change: Seq,
}

impl AttemptRecord {
    pub(crate) const fn attempt(&self) -> AttemptNo {
        self.attempt
    }

    pub(crate) const fn state(&self) -> AttemptState {
        self.state
    }

    pub(crate) fn identity(&self) -> Option<&OsIdentity> {
        self.identity.as_ref()
    }

    /// The event that put the attempt into its current state (reconciliation's repair guard, D3.2b-3).
    pub(crate) const fn last_change(&self) -> Seq {
        self.last_change
    }
}

/// One approval request as recorded. A `Granted` state is history: it never makes anything usable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApprovalRecord {
    request: ApprovalRequest,
    state: ApprovalState,
    requested_at: WallMs,
    decided_at: Option<WallMs>,
    ended_at: Option<Seq>,
}

impl ApprovalRecord {
    pub(crate) fn request(&self) -> &ApprovalRequest {
        &self.request
    }

    pub(crate) fn id(&self) -> ApprovalId {
        self.request.fields().approval
    }

    pub(crate) fn attempt(&self) -> AttemptNo {
        self.request.fields().attempt
    }

    pub(crate) fn kind(&self) -> ApprovalKind {
        self.request.fields().kind
    }

    pub(crate) const fn state(&self) -> ApprovalState {
        self.state
    }

    /// The first wall-clock time at which the recorded decision window is closed, `None` if it overflows (closed).
    pub(crate) fn decision_closes_at(&self) -> Option<u64> {
        self.requested_at.ms().checked_add(self.request.fields().ttl_ms)
    }

    /// The first wall-clock time at which the recorded use window of a granted approval is closed, `None` if it is
    /// not granted or overflows (closed).
    pub(crate) fn use_closes_at(&self) -> Option<u64> {
        let decided = self.decided_at?.ms().checked_add(GRANT_USE_MS)?;
        let cap = self.requested_at.ms().checked_add(MAX_APPROVAL_LIFETIME_MS)?;
        Some(decided.min(cap))
    }
}

/// A raised concern and, once the user has answered it, the recorded resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Concern {
    concern: ConcernRef,
    resolution: Option<Resolution>,
}

impl Concern {
    pub(crate) const fn concern(&self) -> ConcernRef {
        self.concern
    }

    pub(crate) const fn resolution(&self) -> Option<Resolution> {
        self.resolution
    }

    pub(crate) fn is_unresolved_blocking(&self) -> bool {
        self.resolution.is_none() && matches!(self.concern.kind().class(), ConcernClass::Blocking)
    }
}

impl RunProjection {
    /// The projection of a run with no recorded event: only `RunCreated` can follow.
    pub(crate) fn empty(run: RunId) -> RunProjection {
        RunProjection {
            run,
            created: None,
            events: Vec::new(),
            keys: Vec::new(),
            attempts: Vec::new(),
            approvals: Vec::new(),
            concerns: Vec::new(),
        }
    }

    pub(crate) const fn run(&self) -> RunId {
        self.run
    }

    /// `None` before `RunCreated`.
    pub(crate) fn state(&self) -> Option<RunState> {
        self.created.as_ref().map(|created| created.state)
    }

    pub(crate) fn spec(&self) -> Option<&RunSpec> {
        self.created.as_ref().map(|created| &created.spec)
    }

    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }

    pub(crate) fn last_at(&self) -> Option<WallMs> {
        self.events.last().map(|event| event.at_ms)
    }

    pub(crate) fn attempts(&self) -> &[AttemptRecord] {
        &self.attempts
    }

    pub(crate) fn approvals(&self) -> &[ApprovalRecord] {
        &self.approvals
    }

    pub(crate) fn concerns(&self) -> &[Concern] {
        &self.concerns
    }

    pub(crate) fn attempt(&self, attempt: AttemptNo) -> Option<&AttemptRecord> {
        self.attempts.get(usize::from(attempt.number()) - 1)
    }

    pub(crate) fn approval(&self, approval: ApprovalId) -> Option<&ApprovalRecord> {
        let index = usize::try_from(approval.number()).ok()?.checked_sub(1)?;
        self.approvals.get(index)
    }

    /// The attempt that is not terminal, if any; there is at most one.
    pub(crate) fn current_attempt(&self) -> Option<&AttemptRecord> {
        self.attempts.iter().find(|record| !record.state.is_terminal())
    }

    /// Attempts whose outcome says they may have executed.
    pub(crate) fn started_attempts(&self) -> usize {
        self.attempts.iter().filter(|record| record.state.outcome().is_some_and(|class| class.may_have_run())).count()
    }

    pub(crate) fn has_unresolved_blocking_concern(&self) -> bool {
        self.concerns.iter().any(Concern::is_unresolved_blocking)
    }

    fn approval_index(&self, approval: ApprovalId) -> Option<usize> {
        let index = usize::try_from(approval.number()).ok()?.checked_sub(1)?;
        (index < self.approvals.len()).then_some(index)
    }

    fn attempt_index(&self, attempt: AttemptNo) -> Option<usize> {
        let index = usize::from(attempt.number()) - 1;
        (index < self.attempts.len()).then_some(index)
    }

    fn next_seq(&self) -> Result<Seq, Rejection> {
        Seq::new(self.events.len() as u64 + 1).map_err(|_| Rejection::LimitExceeded)
    }

    fn find_key(&self, key: &SemanticKey) -> Option<usize> {
        self.keys.iter().find(|(held, _)| held == key).map(|(_, index)| *index)
    }

    fn push(&mut self, event: Event) {
        let index = self.events.len();
        for key in event.body.semantic_keys() {
            self.keys.push((key, index));
        }
        self.events.push(event);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Results

/// What `apply` accepted. `events` is the frame to append (generated consequences included); it is empty when the
/// whole batch was a duplicate of recorded events or every draft was coalesced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Applied {
    pub(crate) projection: RunProjection,
    pub(crate) events: Vec<Event>,
    pub(crate) coalesced: Vec<Coalesced>,
    pub(crate) duplicate: bool,
}

/// A draft accepted without being appended, so the caller can record a finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Coalesced {
    pub(crate) index: usize,
    pub(crate) reason: CoalesceReason,
}

enum Verdict {
    /// Recorded, with the consequences generated for it (already pushed after it).
    Append(Vec<Event>),
    Duplicate,
    Coalesced(CoalesceReason),
}

/// What the transition function needs to know about the batch or frame it is judging.
struct Batch<'a> {
    /// Producer and body of every entry of the batch or frame, generated consequences included for a frame.
    entries: Vec<(Producer, &'a EventBody)>,
    /// Events recorded before this batch or frame.
    base: usize,
    /// Events and bytes of the journal before this batch, as the caller states them; `None` when replaying.
    size: Option<JournalSize>,
}

// ---------------------------------------------------------------------------------------------------------------
// Entry points

/// Records a batch of drafts as one frame, or refuses all of it. The batch rules, the check order and every
/// transition are those of r5 section 3; `at_ms` is `max(previous, stated)` and `clamped` is set when the stated time
/// was not used (lower than the previous time, or not a valid wall-clock value).
pub(crate) fn apply(projection: &RunProjection, batch: &[EventDraft], size: JournalSize) -> Result<Applied, Rejection> {
    if batch.is_empty() {
        return Err(Rejection::BatchShape);
    }
    if batch.len() > MAX_BATCH {
        return Err(Rejection::LimitExceeded);
    }
    let base = projection.events.len();
    let context = Batch { entries: batch.iter().map(|draft| (draft.producer(), draft.body())).collect(), base, size: Some(size) };
    let mut next = projection.clone();
    let mut shape = Shape::default();
    let mut coalesced = Vec::new();
    let mut duplicates = 0;
    for (index, draft) in batch.iter().enumerate() {
        // (1) producer, then (2) version and time; the rest is judged on the stamped event.
        if !draft.body().drafted_by(draft.producer()) {
            return Err(Rejection::ProducerNotAllowed);
        }
        // S1 (D3.2c-1): a start's record stands alone at the end of its frame, so it can be durable before any effect.
        if draft.body().kind() == EventKind::StartBegun && index + 1 != batch.len() {
            return Err(Rejection::StartNotLast);
        }
        if draft.v() != RECORD_VERSION {
            return Err(Rejection::InvalidDraft);
        }
        let (at_ms, clamped) = stamp(next.last_at(), draft.stated_at_ms())?;
        let event = Event {
            v: draft.v(),
            run: next.run,
            seq: next.next_seq()?,
            at_ms,
            clamped,
            producer: draft.producer(),
            body: draft.body().clone(),
        };
        match judge(&mut next, event, &context, &mut shape)? {
            Verdict::Duplicate => {
                if next.events.len() > base {
                    return Err(Rejection::PartialDuplicate);
                }
                duplicates += 1;
            }
            Verdict::Coalesced(reason) => coalesced.push(Coalesced { index, reason }),
            Verdict::Append(_) => {
                if duplicates > 0 {
                    return Err(Rejection::PartialDuplicate);
                }
                if next.events.len() - base > MAX_BATCH {
                    return Err(Rejection::LimitExceeded);
                }
            }
        }
    }
    if duplicates == batch.len() {
        return Ok(Applied { projection: projection.clone(), events: Vec::new(), coalesced: Vec::new(), duplicate: true });
    }
    if duplicates > 0 {
        return Err(Rejection::PartialDuplicate);
    }
    if !shape.complete() {
        return Err(Rejection::BatchShape);
    }
    let events = next.events[base..].to_vec();
    Ok(Applied { projection: next, events, coalesced, duplicate: false })
}

/// Applies one frame exactly as it was recorded, or reports the first fault. Each record is judged under the rules of
/// its own version (only v1 exists). The recorded `at_ms` and `clamped` are checked, not assigned; the generated
/// consequences of every cause must follow it exactly; expiry and void timing is not checked against `at_ms` (an
/// absurd wall reading is clamped into the window), and the reserve is checked by event count only (r5 gate-review
/// finding F14).
pub(crate) fn apply_recorded(projection: &RunProjection, frame: &[Event]) -> Result<RunProjection, JournalFault> {
    if frame.is_empty() {
        return Err(JournalFault::Codec);
    }
    if frame.len() > MAX_BATCH {
        return Err(JournalFault::LimitExceeded);
    }
    let base = projection.events.len();
    let context = Batch { entries: frame.iter().map(|event| (event.producer, &event.body)).collect(), base, size: None };
    let mut next = projection.clone();
    let mut shape = Shape::default();
    let mut index = 0;
    while index < frame.len() {
        let record = &frame[index];
        envelope(&next, record)?;
        // S1 (D3.2c-1): a `StartBegun` generates no consequence, so it must be the frame's last record.
        if record.body.kind() == EventKind::StartBegun && index + 1 != frame.len() {
            return Err(JournalFault::StartNotLast);
        }
        match judge(&mut next, record.clone(), &context, &mut shape).map_err(fault_of)? {
            Verdict::Append(generated) => {
                let recorded = frame.get(index + 1..index + 1 + generated.len()).ok_or(JournalFault::ConsequenceMismatch)?;
                if recorded != generated.as_slice() {
                    return Err(JournalFault::ConsequenceMismatch);
                }
                index += 1 + generated.len();
            }
            Verdict::Duplicate => return Err(JournalFault::DuplicateKey),
            Verdict::Coalesced(CoalesceReason::ProgressWindow) => return Err(JournalFault::CoalesceViolated),
            Verdict::Coalesced(CoalesceReason::Reserve) => return Err(JournalFault::LimitExceeded),
        }
    }
    if !shape.complete() {
        return Err(JournalFault::MixedProducerFrame);
    }
    Ok(next)
}

/// The recorded envelope: a known version that never decreases, this run, the next `seq`, a time that never
/// decreases, and `clamped` only on an event that kept the previous time.
fn envelope(projection: &RunProjection, record: &Event) -> Result<(), JournalFault> {
    if record.v > RECORD_VERSION {
        return Err(JournalFault::NewerVersion);
    }
    if record.v == 0 {
        return Err(JournalFault::Codec);
    }
    if projection.events.last().is_some_and(|last| record.v < last.v) {
        return Err(JournalFault::VersionDecreased);
    }
    if record.run != projection.run {
        return Err(JournalFault::WrongRun);
    }
    let expected = projection.events.len() as u64 + 1;
    if record.seq.number() != expected {
        return Err(if expected == 1 { JournalFault::SeqNotFirst } else { JournalFault::SeqGap });
    }
    match projection.last_at() {
        Some(previous) if record.at_ms < previous => Err(JournalFault::TimeDecreased),
        Some(previous) if record.clamped && record.at_ms != previous => Err(JournalFault::ClampedMisused),
        None if record.clamped => Err(JournalFault::ClampedMisused),
        _ => Ok(()),
    }
}

/// The recorded time of a draft: the stated time when it is valid and not earlier than the previous time, otherwise
/// the previous time, marked `clamped`. With no previous time (the run's first event) an invalid reading refuses the
/// draft, because run creation is never a safety event.
fn stamp(previous: Option<WallMs>, stated_at_ms: u64) -> Result<(WallMs, bool), Rejection> {
    let stated = WallMs::new(stated_at_ms).ok();
    match (previous, stated) {
        (None, Some(stated)) => Ok((stated, false)),
        (None, None) => Err(Rejection::InvalidTime),
        (Some(previous), Some(stated)) if stated >= previous => Ok((stated, false)),
        (Some(previous), _) => Ok((previous, true)),
    }
}

/// How a refusal reads when it is found in a recorded frame.
fn fault_of(rejection: Rejection) -> JournalFault {
    match rejection {
        Rejection::ProducerNotAllowed => JournalFault::ProducerNotAllowed,
        Rejection::ConsequenceDrafted => JournalFault::ConsequenceMismatch,
        Rejection::Conflict { .. } | Rejection::PartialDuplicate => JournalFault::DuplicateKey,
        Rejection::LimitExceeded | Rejection::ReserveOnly => JournalFault::LimitExceeded,
        Rejection::BatchShape => JournalFault::MixedProducerFrame,
        Rejection::StartNotLast => JournalFault::StartNotLast,
        Rejection::InvalidDraft
        | Rejection::InvalidTime
        | Rejection::RunTerminal
        | Rejection::AttemptTerminal
        | Rejection::ApprovalTerminal
        | Rejection::InvalidTransition
        | Rejection::RunQuarantined
        | Rejection::BindingMismatch
        | Rejection::WindowClosed
        | Rejection::BlockingConcern => JournalFault::InvalidTransition,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Batch shape (r5 section 3, batch rules 1 and 4; gate-review finding F10)

/// Tracks the drafted entries of a batch (generated consequences are not drafts). Exactly three shapes exist: one
/// producer throughout; `[AttemptCreated (user), ApprovalRequested{start} (supervisor)]` for the same attempt and
/// nothing else; and one or more `JournalRepaired` records, then (after at most two of them) only reconciler drafts.
#[derive(Default)]
struct Shape {
    form: Option<Form>,
    drafted: usize,
}

enum Form {
    Single(Producer),
    AttemptPair(AttemptNo),
    Repairs { repairs: usize, reconciling: bool },
}

impl Shape {
    fn admit(&mut self, producer: Producer, body: &EventBody) -> bool {
        let start_request = matches!(body, EventBody::ApprovalRequested(request) if request.fields().kind == ApprovalKind::Start);
        self.drafted += 1;
        match (&mut self.form, body) {
            (None, EventBody::AttemptCreated { attempt, .. }) => {
                self.form = Some(Form::AttemptPair(*attempt));
                true
            }
            (None, EventBody::JournalRepaired(_)) => {
                self.form = Some(Form::Repairs { repairs: 1, reconciling: false });
                true
            }
            (None, _) => {
                self.form = Some(Form::Single(producer));
                !start_request
            }
            (Some(Form::AttemptPair(attempt)), EventBody::ApprovalRequested(request)) => {
                self.drafted == 2 && start_request && producer == Producer::Supervisor && request.fields().attempt == *attempt
            }
            (Some(Form::AttemptPair(_)), _) => false,
            (Some(Form::Repairs { repairs, reconciling }), _) => match producer {
                Producer::Journal if !*reconciling => {
                    *repairs += 1;
                    true
                }
                Producer::Reconciler if *repairs <= 2 => {
                    *reconciling = true;
                    true
                }
                _ => false,
            },
            (Some(Form::Single(first)), _) => {
                producer == *first && !start_request && !matches!(body, EventBody::AttemptCreated { .. })
            }
        }
    }

    fn complete(&self) -> bool {
        !matches!(self.form, Some(Form::AttemptPair(_))) || self.drafted == 2
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The transition function

/// Judges one drafted or recorded event against `projection` in the fixed check order of r5 section 3 and, when it
/// is accepted, records it together with its derived run transition and its generated consequences. A refusal may
/// leave `projection` partly changed: every caller works on a copy and discards it.
fn judge(projection: &mut RunProjection, event: Event, batch: &Batch<'_>, shape: &mut Shape) -> Result<Verdict, Rejection> {
    let kind = event.body.kind();
    // (1) Producer allowed for the kind.
    if !event.body.drafted_by(event.producer) {
        return Err(Rejection::ProducerNotAllowed);
    }
    // (2) Structure: nothing but `RunCreated` before the run exists, and a request names its own run.
    if projection.created.is_none() && kind != EventKind::RunCreated {
        return Err(Rejection::InvalidTransition);
    }
    if let EventBody::ApprovalRequested(request) = &event.body {
        if request.fields().run != projection.run {
            return Err(Rejection::InvalidDraft);
        }
    }
    // (3) A drafted withdrawal or void of an approval this batch already ended.
    if let EventBody::ApprovalWithdrawn { approval, .. } | EventBody::ApprovalVoided { approval, .. } = &event.body {
        let ended_here = projection.approval(*approval).and_then(|record| record.ended_at).is_some_and(|seq| seq.number() as usize > batch.base);
        if ended_here {
            return Err(Rejection::ConsequenceDrafted);
        }
    }
    // (4) Semantic identity.
    let keys = event.body.semantic_keys();
    let held: Vec<Option<usize>> = keys.iter().map(|key| projection.find_key(key)).collect();
    if let Some(existing) = held.iter().flatten().next().copied() {
        let same = held.iter().all(|index| *index == Some(existing));
        if same && existing < batch.base && projection.events[existing].same_content(&event) {
            return Ok(Verdict::Duplicate);
        }
        return Err(Rejection::Conflict { existing: projection.events[existing].seq });
    }
    // (5) Capacity (r5 4.8, amended by D3.2c-1). Live, the caller's stated journal size counts (never less than what
    // this projection holds); replaying, the event count is all there is (F14), so replay accepts at least what was
    // accepted live.
    let journal_events = match batch.size {
        Some(size) => (size.events as usize).max(batch.base) + (projection.events.len() - batch.base),
        None => projection.events.len(),
    };
    let journal_bytes = batch.size.map_or(0, |size| size.bytes);
    let protected = protected(&event);
    // Repairs are admitted in every tier but the last, so they are capped per run: past `MAX_REPAIRS` the run must be
    // quarantined instead (r5 5.5), and repairs can never take more of the safety tier than that.
    let repairs = || projection.events.iter().filter(|recorded| recorded.body.kind() == EventKind::JournalRepaired).count();
    if kind == EventKind::JournalRepaired && repairs() >= MAX_REPAIRS as usize {
        return Err(Rejection::LimitExceeded);
    }
    match Capacity::of(journal_events, journal_bytes) {
        Capacity::Ordinary => {}
        Capacity::Reserve if kind.reserved() || kind == EventKind::JournalRepaired => {}
        Capacity::Reserve if event.producer == Producer::ProviderRelay => return Ok(Verdict::Coalesced(CoalesceReason::Reserve)),
        Capacity::Reserve => return Err(Rejection::ReserveOnly),
        Capacity::Safety if protected => {}
        Capacity::Safety | Capacity::Exhausted => return Err(Rejection::LimitExceeded),
    }
    // (6) Terminal run, attempt or approval.
    terminal_checks(projection, &event)?;
    // (7) Transition legality, including the batch rules.
    if !shape.admit(event.producer, &event.body) {
        return Err(Rejection::BatchShape);
    }
    let before = projection.events.len();
    let verdict = transition(projection, event, batch)?;
    // The event and its generated consequences must fit: under `MAX_EVENTS`, or under the absolute ceiling for an event
    // the safety tier admits. Nothing passes the ceiling.
    let ceiling = if protected { MAX_EVENTS + SAFETY_EVENTS } else { MAX_EVENTS };
    if journal_events + (projection.events.len() - before) > ceiling as usize {
        return Err(Rejection::LimitExceeded);
    }
    Ok(verdict)
}

/// Where a journal of `events` events and `bytes` bytes stands (r5 4.8, amended by D3.2c-1). Each dimension has the same
/// four tiers and the journal is in the higher of its two:
///
/// - `Ordinary`: below `MAX_EVENTS - RESERVED_EVENTS` and `MAX_JOURNAL_BYTES - RESERVED_JOURNAL_BYTES`: any event;
/// - `Reserve`: below `MAX_EVENTS` and `MAX_JOURNAL_BYTES`: reserved kinds and repairs (other provider-relayed drafts are
///   coalesced, other drafts refused);
/// - `Safety`: below `MAX_EVENTS + SAFETY_EVENTS` and `MAX_JOURNAL_BYTES + SAFETY_JOURNAL_BYTES`: only what `protected`
///   admits;
/// - `Exhausted`: nothing, a repair included. The run cannot record anything more; the adapter must quarantine rather
///   than lose a record, and the run keeps blocking every start.
///
/// Replay knows no bytes, so it places a journal by its event count only; it therefore accepts everything the live path
/// accepted with the same count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Capacity {
    Ordinary,
    Reserve,
    Safety,
    Exhausted,
}

impl Capacity {
    fn of(events: usize, bytes: u64) -> Capacity {
        let by_events = match events as u64 {
            n if n < u64::from(MAX_EVENTS - RESERVED_EVENTS) => Capacity::Ordinary,
            n if n < u64::from(MAX_EVENTS) => Capacity::Reserve,
            n if n < u64::from(MAX_EVENTS + SAFETY_EVENTS) => Capacity::Safety,
            _ => Capacity::Exhausted,
        };
        let by_bytes = match bytes {
            n if n < MAX_JOURNAL_BYTES - RESERVED_JOURNAL_BYTES => Capacity::Ordinary,
            n if n < MAX_JOURNAL_BYTES => Capacity::Reserve,
            n if n < MAX_JOURNAL_BYTES + SAFETY_JOURNAL_BYTES => Capacity::Safety,
            _ => Capacity::Exhausted,
        };
        by_events.max(by_bytes)
    }
}

/// Whether the safety tier admits `event` (D3.2c-1): a safety kind (`EventKind::safety`), the resolution of a blocking
/// concern, or a repair record. Every one of these is bounded once the journal is past `MAX_EVENTS`: no attempt or
/// request can be created there, so only the one attempt in flight and its open approvals can still produce the safety
/// kinds, each at most once per semantic key; a blocking concern is resolved once; and repairs are capped per run
/// (`MAX_REPAIRS`). A repair must be admitted on its own too: a protected frame written after the attempt ended (an
/// orphan resolution, a run cancel) can itself be torn, and its tail must be repairable.
fn protected(event: &Event) -> bool {
    match &event.body {
        EventBody::AttentionResolved(resolved) => resolved.concern().kind().class() == ConcernClass::Blocking,
        EventBody::JournalRepaired(_) => true,
        body => body.kind().safety(),
    }
}

/// Check (6): a terminal run accepts only resolutions, repairs and reconciliation; a terminal attempt accepts no
/// event about it; a terminal approval accepts no event about it.
fn terminal_checks(projection: &RunProjection, event: &Event) -> Result<(), Rejection> {
    let kind = event.body.kind();
    let run_terminal = projection.state().is_some_and(RunState::is_terminal);
    let after_end = matches!(kind, EventKind::AttentionResolved | EventKind::JournalRepaired) || event.producer == Producer::Reconciler;
    if run_terminal && !after_end {
        return Err(Rejection::RunTerminal);
    }
    if let Some(attempt) = attempt_of(&event.body) {
        if projection.attempt(attempt).is_some_and(|record| record.state.is_terminal()) {
            return Err(Rejection::AttemptTerminal);
        }
    }
    if let Some(approval) = approval_of(&event.body) {
        if projection.approval(approval).is_some_and(|record| record.state.is_terminal()) {
            return Err(Rejection::ApprovalTerminal);
        }
    }
    Ok(())
}

/// The attempt an event is about, for every kind that names one.
fn attempt_of(body: &EventBody) -> Option<AttemptNo> {
    match body {
        EventBody::StartBegun { attempt, .. }
        | EventBody::AttemptStarted { attempt, .. }
        | EventBody::StartFailed { attempt, .. }
        | EventBody::StopRequested { attempt, .. }
        | EventBody::AttemptExited { attempt, .. }
        | EventBody::AttemptInterrupted { attempt, .. }
        | EventBody::AttemptNotStarted { attempt, .. }
        | EventBody::ProviderSessionObserved { attempt, .. }
        | EventBody::Progress { attempt, .. }
        | EventBody::ProviderOutcomeClaimed { attempt, .. } => Some(*attempt),
        EventBody::CancelRequested { scope: CancelScope::Attempt(attempt) } => Some(*attempt),
        _ => None,
    }
}

/// The approval an approval event is about. `StartBegun` names its approval too.
fn approval_of(body: &EventBody) -> Option<ApprovalId> {
    match body {
        EventBody::ApprovalDecided { approval, .. }
        | EventBody::ApprovalExpired { approval, .. }
        | EventBody::ApprovalWithdrawn { approval, .. }
        | EventBody::ApprovalVoided { approval, .. }
        | EventBody::ApprovalConsumed { approval, .. }
        | EventBody::StartBegun { approval, .. } => Some(*approval),
        _ => None,
    }
}

/// Whether an entry of the batch is an attempt event about `attempt` (a run-scope cancel is about every attempt that
/// is not terminal), and whether it ends an attempt that has not started yet.
fn about_attempt(body: &EventBody, attempt: AttemptNo) -> (bool, bool) {
    match body {
        EventBody::CancelRequested { scope: CancelScope::Run } => (true, true),
        EventBody::CancelRequested { scope: CancelScope::Attempt(n) } => (*n == attempt, *n == attempt),
        EventBody::AttemptCreated { attempt: n, .. } => (*n == attempt, false),
        other if other.kind().is_attempt_event() => (attempt_of(other) == Some(attempt), false),
        _ => (false, false),
    }
}

fn transition(projection: &mut RunProjection, event: Event, batch: &Batch<'_>) -> Result<Verdict, Rejection> {
    let seq = event.seq;
    match &event.body {
        EventBody::RunCreated { spec, spec_digest } => {
            if seq.number() != 1 {
                return Err(Rejection::InvalidTransition);
            }
            projection.created = Some(Created { spec: spec.clone(), spec_digest: spec_digest.clone(), state: RunState::Open });
            append(projection, event)
        }
        EventBody::AttemptCreated { attempt, continuation } => {
            if projection.state() != Some(RunState::Open) {
                return Err(Rejection::InvalidTransition);
            }
            if projection.current_attempt().is_some() || usize::from(attempt.number()) != projection.attempts.len() + 1 {
                return Err(Rejection::InvalidTransition);
            }
            if projection.has_unresolved_blocking_concern() {
                return Err(Rejection::BlockingConcern);
            }
            let max_started = projection.spec().map_or(0, |spec| usize::from(spec.max_started_attempts()));
            if projection.started_attempts() >= max_started || projection.attempts.len() >= usize::from(MAX_ATTEMPT_RECORDS) {
                return Err(Rejection::LimitExceeded);
            }
            if let Continuation::Resume { from_attempt, session } = continuation {
                let observed = *from_attempt < *attempt
                    && projection.find_key(&SemanticKey::AttemptSession(*from_attempt, session.clone())).is_some();
                if !observed {
                    return Err(Rejection::InvalidTransition);
                }
            }
            projection.attempts.push(AttemptRecord {
                attempt: *attempt,
                state: AttemptState::Proposed,
                continuation: continuation.clone(),
                start_approval: None,
                identity: None,
                stop_recorded: false,
                failure_claimed: false,
                last_progress_at: None,
                last_change: seq,
            });
            append(projection, event)
        }
        EventBody::ApprovalRequested(request) => {
            let fields = request.fields();
            let created = projection.created.as_ref().ok_or(Rejection::InvalidTransition)?;
            // F7: the request must be for this run's specification and target, with the risk Ordelis computes and a
            // lifetime the run allows.
            let spec = &created.spec;
            if fields.binding.spec_digest != created.spec_digest || fields.binding.target != spec.provider().binding_target() {
                return Err(Rejection::BindingMismatch);
            }
            let classified = classify(fields.binding.operation_class, spec.context().workspace());
            let expected_risk = fields.provider_risk_claim.map_or(classified, |claim| claim.max(classified));
            if request.risk() != expected_risk {
                return Err(Rejection::InvalidDraft);
            }
            if fields.ttl_ms > spec.context().limits().approval_ttl_ms().min(MAX_APPROVAL_LIFETIME_MS) {
                return Err(Rejection::InvalidDraft);
            }
            if fields.approval.number() != projection.approvals.len() as u64 + 1 {
                return Err(Rejection::InvalidTransition);
            }
            let index = projection.attempt_index(fields.attempt).ok_or(Rejection::InvalidTransition)?;
            let record = &projection.attempts[index];
            match fields.kind {
                ApprovalKind::Start => {
                    if record.state != AttemptState::Proposed || record.start_approval.is_some() {
                        return Err(Rejection::InvalidTransition);
                    }
                }
                ApprovalKind::Operation => {
                    if record.state != AttemptState::Running {
                        return Err(Rejection::InvalidTransition);
                    }
                    let of_attempt = projection.approvals.iter().filter(|approval| approval.attempt() == fields.attempt);
                    let (total, open) = of_attempt.fold((0usize, 0usize), |(total, open), approval| (total + 1, open + usize::from(approval.state.is_open())));
                    if total >= usize::from(MAX_APPROVALS_PER_ATTEMPT) || open >= usize::from(MAX_OPEN_APPROVALS) {
                        return Err(Rejection::LimitExceeded);
                    }
                }
            }
            if fields.kind == ApprovalKind::Start {
                projection.attempts[index].start_approval = Some(fields.approval);
            }
            projection.approvals.push(ApprovalRecord {
                request: request.clone(),
                state: ApprovalState::Pending,
                requested_at: event.at_ms,
                decided_at: None,
                ended_at: None,
            });
            append(projection, event)
        }
        EventBody::ApprovalDecided { approval, decision, provenance } => {
            let index = projection.approval_index(*approval).ok_or(Rejection::InvalidTransition)?;
            let record = &projection.approvals[index];
            if record.state != ApprovalState::Pending {
                return Err(Rejection::InvalidTransition);
            }
            if provenance.presented_binding_digest != record.request.fields().binding_digest {
                return Err(Rejection::BindingMismatch);
            }
            if record.decision_closes_at().map_or(true, |closes| event.at_ms.ms() >= closes) {
                return Err(Rejection::WindowClosed);
            }
            let to = match decision {
                Decision::Granted => ApprovalState::Granted,
                Decision::Denied => ApprovalState::Denied,
            };
            let moved = match decision {
                Decision::Granted => AttemptState::Approved,
                Decision::Denied => AttemptState::NotStarted(NotStartedReason::Denied),
            };
            approval_event(projection, event, index, to, moved, batch)
        }
        EventBody::ApprovalExpired { approval, .. } => {
            let index = projection.approval_index(*approval).ok_or(Rejection::InvalidTransition)?;
            if projection.approvals[index].state != ApprovalState::Pending {
                return Err(Rejection::InvalidTransition);
            }
            let moved = AttemptState::NotStarted(NotStartedReason::Expired);
            approval_event(projection, event, index, ApprovalState::Expired, moved, batch)
        }
        EventBody::ApprovalWithdrawn { approval, .. } => {
            let index = projection.approval_index(*approval).ok_or(Rejection::InvalidTransition)?;
            if projection.approvals[index].state != ApprovalState::Pending {
                return Err(Rejection::InvalidTransition);
            }
            let moved = AttemptState::NotStarted(NotStartedReason::Withdrawn);
            approval_event(projection, event, index, ApprovalState::Withdrawn, moved, batch)
        }
        EventBody::ApprovalVoided { approval, .. } => {
            let index = projection.approval_index(*approval).ok_or(Rejection::InvalidTransition)?;
            if projection.approvals[index].state != ApprovalState::Granted {
                return Err(Rejection::InvalidTransition);
            }
            let moved = AttemptState::NotStarted(NotStartedReason::Voided);
            approval_event(projection, event, index, ApprovalState::Voided, moved, batch)
        }
        EventBody::ApprovalConsumed { approval, binding } => {
            let index = projection.approval_index(*approval).ok_or(Rejection::InvalidTransition)?;
            let record = &projection.approvals[index];
            if record.kind() != ApprovalKind::Operation || record.state != ApprovalState::Granted {
                return Err(Rejection::InvalidTransition);
            }
            if projection.attempt(record.attempt()).map(AttemptRecord::state) != Some(AttemptState::Running) {
                return Err(Rejection::InvalidTransition);
            }
            if *binding != record.request.fields().binding {
                return Err(Rejection::BindingMismatch);
            }
            if record.use_closes_at().map_or(true, |closes| event.at_ms.ms() >= closes) {
                return Err(Rejection::WindowClosed);
            }
            end_approval(projection, index, ApprovalState::Consumed, seq);
            append(projection, event)
        }
        EventBody::StartBegun { attempt, approval, binding } => {
            let attempt_index = projection.attempt_index(*attempt).ok_or(Rejection::InvalidTransition)?;
            let index = projection.approval_index(*approval).ok_or(Rejection::InvalidTransition)?;
            let record = &projection.attempts[attempt_index];
            if record.state != AttemptState::Approved || record.start_approval != Some(*approval) {
                return Err(Rejection::InvalidTransition);
            }
            let request = &projection.approvals[index];
            if request.state != ApprovalState::Granted {
                return Err(Rejection::InvalidTransition);
            }
            if *binding != request.request.fields().binding {
                return Err(Rejection::BindingMismatch);
            }
            if request.use_closes_at().map_or(true, |closes| event.at_ms.ms() >= closes) {
                return Err(Rejection::WindowClosed);
            }
            end_approval(projection, index, ApprovalState::Consumed, seq);
            move_attempt(projection, attempt_index, AttemptState::Starting, seq);
            append(projection, event)
        }
        EventBody::AttemptStarted { attempt, os_identity } => {
            let index = projection.attempt_index(*attempt).ok_or(Rejection::InvalidTransition)?;
            let target = projection.spec().map(|spec| spec.provider().binding_target()).ok_or(Rejection::InvalidTransition)?;
            if *os_identity.executable_sha256() != target.executable_sha256 {
                return Err(Rejection::BindingMismatch);
            }
            let record = &projection.attempts[index];
            match record.state {
                AttemptState::Starting => move_attempt(projection, index, AttemptState::Running, seq),
                // R7: a child that started while its stop was pending; its identity is kept as orphan evidence.
                AttemptState::Stopping(_) if record.identity.is_none() => {}
                _ => return Err(Rejection::InvalidTransition),
            }
            projection.attempts[index].identity = Some(os_identity.clone());
            append(projection, event)
        }
        EventBody::StartFailed { attempt, code } => {
            let index = projection.attempt_index(*attempt).ok_or(Rejection::InvalidTransition)?;
            let record = &projection.attempts[index];
            let pending_start = matches!(record.state, AttemptState::Starting | AttemptState::Stopping(_));
            if !pending_start || record.identity.is_some() {
                return Err(Rejection::InvalidTransition);
            }
            end_attempt(projection, index, AttemptState::NotStarted(NotStartedReason::StartFailed(*code)), event)
        }
        EventBody::StopRequested { attempt, reason } => {
            let index = projection.attempt_index(*attempt).ok_or(Rejection::InvalidTransition)?;
            let record = &projection.attempts[index];
            match record.state {
                AttemptState::Starting | AttemptState::Running => {
                    move_attempt(projection, index, AttemptState::Stopping(stop_cause(*reason)), seq);
                }
                AttemptState::Stopping(_) if !record.stop_recorded => {}
                _ => return Err(Rejection::InvalidTransition),
            }
            projection.attempts[index].stop_recorded = true;
            let attempt = *attempt;
            stopping(projection, event, attempt)
        }
        EventBody::CancelRequested { scope } => {
            let scope = *scope;
            cancel(projection, event, scope)
        }
        EventBody::AttemptExited { attempt, status } => {
            let index = projection.attempt_index(*attempt).ok_or(Rejection::InvalidTransition)?;
            let record = &projection.attempts[index];
            let to = match record.state {
                AttemptState::Running if status.is_zero_exit() && !record.failure_claimed => AttemptState::Succeeded,
                AttemptState::Running if status.is_zero_exit() => AttemptState::Failed(FailureKind::ProviderReported),
                AttemptState::Running => AttemptState::Failed(FailureKind::Exit),
                // F2: an exit during a stop needs the child's recorded identity.
                AttemptState::Stopping(cause) if record.identity.is_some() => stopped_outcome(cause),
                _ => return Err(Rejection::InvalidTransition),
            };
            end_attempt(projection, index, to, event)
        }
        EventBody::AttemptInterrupted { attempt, observation } => {
            let index = projection.attempt_index(*attempt).ok_or(Rejection::InvalidTransition)?;
            let record = &projection.attempts[index];
            let allowed = match (record.state, observation) {
                (AttemptState::Starting | AttemptState::Stopping(_), InterruptObservation::NoIdentity) => record.identity.is_none(),
                (AttemptState::Running | AttemptState::Stopping(_), InterruptObservation::NotRunning | InterruptObservation::Unknown | InterruptObservation::PossiblyRunning) => {
                    record.identity.is_some()
                }
                (AttemptState::Proposed | AttemptState::Approved, InterruptObservation::EvidenceLost) => repaired_since(projection, record.last_change),
                _ => false,
            };
            if !allowed {
                return Err(Rejection::InvalidTransition);
            }
            end_attempt(projection, index, AttemptState::Interrupted(*observation), event)
        }
        EventBody::AttemptNotStarted { attempt, .. } => {
            let index = projection.attempt_index(*attempt).ok_or(Rejection::InvalidTransition)?;
            let record = &projection.attempts[index];
            // Only while the journal is intact since the attempt's last change: by write-ahead, nothing ran.
            let before_start = matches!(record.state, AttemptState::Proposed | AttemptState::Approved);
            if !before_start || repaired_since(projection, record.last_change) {
                return Err(Rejection::InvalidTransition);
            }
            end_attempt(projection, index, AttemptState::NotStarted(NotStartedReason::SupervisionLost), event)
        }
        EventBody::ProviderSessionObserved { attempt, .. } => {
            relayed_attempt(projection, *attempt)?;
            append(projection, event)
        }
        EventBody::Progress { attempt, .. } => {
            let index = relayed_attempt(projection, *attempt)?;
            let last = projection.attempts[index].last_progress_at;
            if last.is_some_and(|last| event.at_ms.ms() < last.ms().saturating_add(PROGRESS_COALESCE_MS)) {
                return Ok(Verdict::Coalesced(CoalesceReason::ProgressWindow));
            }
            projection.attempts[index].last_progress_at = Some(event.at_ms);
            append(projection, event)
        }
        EventBody::ProviderOutcomeClaimed { attempt, .. } => {
            let index = relayed_attempt(projection, *attempt)?;
            projection.attempts[index].failure_claimed = true;
            append(projection, event)
        }
        EventBody::RunClosed { .. } => {
            if projection.state() != Some(RunState::Open) || projection.current_attempt().is_some() {
                return Err(Rejection::InvalidTransition);
            }
            set_run(projection, RunState::ClosedByUser);
            append(projection, event)
        }
        EventBody::AttentionResolved(resolved) => {
            let concern = projection.concerns.iter_mut().find(|concern| concern.concern == resolved.concern() && concern.resolution.is_none());
            let concern = concern.ok_or(Rejection::InvalidTransition)?;
            concern.resolution = Some(resolved.resolution());
            append(projection, event)
        }
        EventBody::JournalRepaired(_) => {
            raise(projection, ConcernKind::JournalRepaired, None, seq);
            append(projection, event)
        }
    }
}

/// Whether a `JournalRepaired` was recorded after `seq`: the removed bytes may have held a later lifecycle event.
fn repaired_since(projection: &RunProjection, seq: Seq) -> bool {
    projection.events.iter().any(|event| event.seq > seq && event.body.kind() == EventKind::JournalRepaired)
}

/// Provider-relayed events are about a started attempt that has not ended.
fn relayed_attempt(projection: &RunProjection, attempt: AttemptNo) -> Result<usize, Rejection> {
    let index = projection.attempt_index(attempt).ok_or(Rejection::InvalidTransition)?;
    match projection.attempts[index].state {
        AttemptState::Running | AttemptState::Stopping(_) => Ok(index),
        _ => Err(Rejection::InvalidTransition),
    }
}

/// An approval event on an approval that is not terminal. A start approval's own outcome moves its attempt before
/// anything runs (the approval-driven rows of r5 3.2), but only when the producer is the user or the supervisor and the
/// batch holds no attempt event for that attempt. When the batch does hold one, the move is suppressed, and that is
/// allowed only if the batch also ends the attempt (F10); otherwise the approval and its attempt would disagree.
fn approval_event(
    projection: &mut RunProjection,
    event: Event,
    index: usize,
    to: ApprovalState,
    moved: AttemptState,
    batch: &Batch<'_>,
) -> Result<Verdict, Rejection> {
    let seq = event.seq;
    let record = &projection.approvals[index];
    let attempt = record.attempt();
    let attempt_index = projection.attempt_index(attempt).ok_or(Rejection::InvalidTransition)?;
    let attempt_state = projection.attempts[attempt_index].state;
    let drives = record.kind() == ApprovalKind::Start
        && matches!((attempt_state, to), (AttemptState::Proposed, ApprovalState::Granted | ApprovalState::Denied | ApprovalState::Expired | ApprovalState::Withdrawn) | (AttemptState::Approved, ApprovalState::Voided));
    if record.kind() == ApprovalKind::Start && !drives && event.producer != Producer::Reconciler {
        // A start approval can only change while its attempt has not started.
        return Err(Rejection::InvalidTransition);
    }
    let mut suppressed = false;
    if drives && event.producer != Producer::Reconciler {
        let (about, ends) = batch.entries.iter().fold((false, false), |(about, ends), (_, body)| {
            let (a, e) = about_attempt(body, attempt);
            (about || a, ends || e)
        });
        if about && !ends {
            return Err(Rejection::InvalidTransition);
        }
        suppressed = about;
    }
    if to == ApprovalState::Granted || to == ApprovalState::Denied {
        projection.approvals[index].decided_at = Some(event.at_ms);
    }
    if to.is_terminal() {
        end_approval(projection, index, to, seq);
    } else {
        projection.approvals[index].state = to;
    }
    if drives && !suppressed && event.producer != Producer::Reconciler {
        if moved.is_terminal() {
            return end_attempt(projection, attempt_index, moved, event);
        }
        move_attempt(projection, attempt_index, moved, seq);
    }
    append(projection, event)
}

/// A user cancel. For an attempt that has not started it ends the attempt; for a started one it starts its stop; for
/// one already stopping it is recorded and changes nothing. A run-scope cancel moves the run to `closing` first, so
/// `cancelled` wins over `exhausted` when the cancelled attempt is the last one possible (F12).
fn cancel(projection: &mut RunProjection, event: Event, scope: CancelScope) -> Result<Verdict, Rejection> {
    let seq = event.seq;
    let index = match scope {
        CancelScope::Attempt(attempt) => Some(projection.attempt_index(attempt).ok_or(Rejection::InvalidTransition)?),
        CancelScope::Run => {
            if projection.state() != Some(RunState::Open) {
                return Err(Rejection::InvalidTransition);
            }
            let current = projection.current_attempt().map(AttemptRecord::attempt);
            set_run(projection, if current.is_some() { RunState::Closing } else { RunState::Cancelled });
            match current {
                Some(attempt) => projection.attempt_index(attempt),
                None => return append(projection, event),
            }
        }
    };
    let Some(index) = index else { return append(projection, event) };
    let attempt = projection.attempts[index].attempt;
    match projection.attempts[index].state {
        AttemptState::Proposed | AttemptState::Approved => end_attempt(projection, index, AttemptState::NotStarted(NotStartedReason::Cancelled), event),
        AttemptState::Starting | AttemptState::Running => {
            move_attempt(projection, index, AttemptState::Stopping(StopCause::Cancel), seq);
            stopping(projection, event, attempt)
        }
        AttemptState::Stopping(_) => stopping(projection, event, attempt),
        _ => Err(Rejection::AttemptTerminal),
    }
}

/// Records a stop or cancel of a started attempt, with the consequences for its open approvals.
fn stopping(projection: &mut RunProjection, event: Event, attempt: AttemptNo) -> Result<Verdict, Rejection> {
    let reasons = consequence_reasons(&event);
    let cause = event.clone();
    push_with_clock(projection, event);
    let generated = consequences(projection, attempt, reasons, &cause)?;
    Ok(Verdict::Append(generated))
}

/// Ends an attempt: records the event, raises its concerns, derives the run's state immediately after it (batch rule
/// 3), then generates the consequences for its open approvals (batch rule 2).
fn end_attempt(projection: &mut RunProjection, index: usize, to: AttemptState, event: Event) -> Result<Verdict, Rejection> {
    let seq = event.seq;
    let attempt = projection.attempts[index].attempt;
    move_attempt(projection, index, to, seq);
    if matches!(to, AttemptState::Interrupted(_)) {
        raise(projection, ConcernKind::PossibleOrphan, Some(attempt), seq);
    }
    if to.outcome().is_some_and(|class| class.leaves_workspace_unknown()) {
        raise(projection, ConcernKind::WorkspaceUnknown, Some(attempt), seq);
    }
    settle_run(projection, to);
    let reasons = consequence_reasons(&event);
    let cause = event.clone();
    push_with_clock(projection, event);
    let generated = consequences(projection, attempt, reasons, &cause)?;
    Ok(Verdict::Append(generated))
}

/// The run's state after one of its attempts reached the terminal state `ended`.
fn settle_run(projection: &mut RunProjection, ended: AttemptState) {
    let started = projection.started_attempts();
    let records = projection.attempts.len();
    let last = projection
        .attempts
        .iter()
        .rev()
        .filter_map(|record| record.state.outcome())
        .find(|class| class.may_have_run())
        .and_then(ExhaustedLast::of)
        .unwrap_or(ExhaustedLast::NeverStarted);
    let Some(created) = projection.created.as_mut() else { return };
    created.state = match created.state {
        RunState::Closing => RunState::Cancelled,
        RunState::Open if ended == AttemptState::Succeeded => RunState::Succeeded,
        RunState::Open if started >= usize::from(created.spec.max_started_attempts()) || records >= usize::from(MAX_ATTEMPT_RECORDS) => {
            RunState::Exhausted(last)
        }
        other => other,
    };
}

fn set_run(projection: &mut RunProjection, state: RunState) {
    if let Some(created) = projection.created.as_mut() {
        created.state = state;
    }
}

fn move_attempt(projection: &mut RunProjection, index: usize, to: AttemptState, seq: Seq) {
    let record = &mut projection.attempts[index];
    record.state = to;
    record.last_change = seq;
}

fn end_approval(projection: &mut RunProjection, index: usize, to: ApprovalState, seq: Seq) {
    let record = &mut projection.approvals[index];
    record.state = to;
    record.ended_at = Some(seq);
}

/// Raises a concern. A clock concern is raised only when none is open.
fn raise(projection: &mut RunProjection, kind: ConcernKind, attempt: Option<AttemptNo>, seq: Seq) {
    if kind == ConcernKind::ClockIrregular
        && projection.concerns.iter().any(|concern| concern.concern.kind() == ConcernKind::ClockIrregular && concern.resolution.is_none())
    {
        return;
    }
    if let Ok(concern) = ConcernRef::new(kind, attempt, seq) {
        projection.concerns.push(Concern { concern, resolution: None });
    }
}

/// Records an accepted event (raising `clock_irregular` when its time was clamped).
fn append(projection: &mut RunProjection, event: Event) -> Result<Verdict, Rejection> {
    push_with_clock(projection, event);
    Ok(Verdict::Append(Vec::new()))
}

/// A clamped time raises `clock_irregular`, except on the resolution of a `clock_irregular` concern (D3.2c-1):
/// acknowledging it with the clock still behind would otherwise raise the next one, an unbounded loop of events. The
/// clamp stays recorded on the event.
fn push_with_clock(projection: &mut RunProjection, event: Event) {
    let resolves_clock = matches!(&event.body, EventBody::AttentionResolved(resolved) if resolved.concern().kind() == ConcernKind::ClockIrregular);
    if event.clamped && !resolves_clock {
        raise(projection, ConcernKind::ClockIrregular, None, event.seq);
    }
    projection.push(event);
}

/// The reasons a cause gives its generated consequences: `cancelled` for a user cancel, `clock_fault` for a clock-fault
/// stop, `supervision_lost` for reconciliation, `attempt_ended` for any other stop or end.
fn consequence_reasons(cause: &Event) -> (WithdrawalReason, VoidReason) {
    match (&cause.body, cause.producer) {
        (EventBody::CancelRequested { .. }, _) => (WithdrawalReason::Cancelled, VoidReason::Cancelled),
        (EventBody::StopRequested { reason: StopReason::ClockFault, .. }, _) => (WithdrawalReason::ClockFault, VoidReason::ClockFault),
        (_, Producer::Reconciler) => (WithdrawalReason::SupervisionLost, VoidReason::SupervisionLost),
        _ => (WithdrawalReason::AttemptEnded, VoidReason::AttemptEnded),
    }
}

/// Batch rule 2: right after a cause, one `ApprovalWithdrawn` per pending approval and one `ApprovalVoided` per unused
/// grant of the attempt, in approval order, labelled with the cause's producer, time and version, never clamped.
fn consequences(projection: &mut RunProjection, attempt: AttemptNo, reasons: (WithdrawalReason, VoidReason), cause: &Event) -> Result<Vec<Event>, Rejection> {
    let mut generated = Vec::new();
    for index in 0..projection.approvals.len() {
        let record = &projection.approvals[index];
        if record.attempt() != attempt {
            continue;
        }
        let approval = record.id();
        let (body, to) = match record.state {
            ApprovalState::Pending => (EventBody::ApprovalWithdrawn { approval, reason: reasons.0 }, ApprovalState::Withdrawn),
            ApprovalState::Granted => (EventBody::ApprovalVoided { approval, reason: reasons.1 }, ApprovalState::Voided),
            _ => continue,
        };
        let seq = projection.next_seq()?;
        end_approval(projection, index, to, seq);
        let event = Event { v: cause.v, run: projection.run, seq, at_ms: cause.at_ms, clamped: false, producer: cause.producer, body };
        generated.push(event.clone());
        projection.push(event);
    }
    Ok(generated)
}

fn stop_cause(reason: StopReason) -> StopCause {
    match reason {
        StopReason::Deadline(kind) => StopCause::Deadline(kind),
        StopReason::JournalFull => StopCause::JournalFull,
        StopReason::ClockFault => StopCause::ClockFault,
    }
}

/// The outcome of an attempt that exits while stopping: decided by its first stop reason, whatever the exit status.
fn stopped_outcome(cause: StopCause) -> AttemptState {
    match cause {
        StopCause::Cancel => AttemptState::Cancelled,
        StopCause::Deadline(kind) => AttemptState::TimedOut(kind),
        StopCause::JournalFull => AttemptState::Stopped(StoppedKind::JournalFull),
        StopCause::ClockFault => AttemptState::Stopped(StoppedKind::ClockFault),
    }
}

#[cfg(test)]
pub(crate) mod tests;
