// Live authority, in memory only (D3.2b-2, issue #114; design r5 sections 3.5, 5.4 and 6.5, decision 12.17, with the
// gate review's F1, F2, F8 and F13). Pure: monotonic times, wall-clock readings and epochs are values the caller
// supplies; nothing here reads a clock.
//
// `LiveSupervision` owns the live projection of one run during one process lifetime. `LiveGrant` is the only value
// that can consume an approval. Neither can be cloned, copied, defaulted, formatted, serialized or built outside this
// file, and no event, projection or recorded frame can be turned into either: the only door to a `LiveSupervision`
// in this increment is `begin_new` (D3.2b-3 adds `begin_reconciled`), and the only place a grant is made is
// `confirm`, for a decision this supervision itself staged inside its monotonic window.
//
// A grant is consent made usable for recording one consumption. It is never Execution Safety Gate satisfaction, and
// nothing here, or anywhere in the lifecycle module, turns it or a staged frame into a process start.
use super::model::{
    ApprovalId, ApprovalKind, ApprovalRequest, ApprovalState, AttemptNo, AttemptState, Decision, Event, EventBody,
    EventDraft, EventKind, ExpiryBasis, JournalSize, LiveEpoch, MonoMs, OperationDescription, Rejection, RunId,
    StopReason, SupervisorDraft, VoidReason, WallMs, DeadlineKind, GRANT_USE_MS, MAX_APPROVAL_LIFETIME_MS, MAX_EVENTS,
    MAX_WALL_MS, RESERVED_EVENTS,
};
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
    /// Frames staged so far; the number identifies the outstanding one.
    frames: u64,
    in_flight: Option<u64>,
    /// An abandoned frame not yet adopted or discarded (F8): nothing can be staged until it is.
    unresolved: Option<u64>,
    approvals: Vec<ApprovalClock>,
    attempts: Vec<AttemptClock>,
    descriptions: Vec<(ApprovalId, OperationDescription)>,
    /// Approvals with a minted, unconsumed grant.
    outstanding: Vec<ApprovalId>,
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
pub(crate) struct PendingSupervision {
    live: LiveSupervision,
    events: Vec<Event>,
}

/// A frame staged for writing. Consumed by exactly one of `confirm` (durable) or `abandon` (uncertain or failed).
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
}

/// A frame whose write was uncertain. It mints nothing; `adopt` (it landed) or `discard` (it is absent) resolves it.
pub(crate) struct Abandoned {
    epoch: LiveEpoch,
    frame: u64,
    events: Vec<Event>,
    projection: RunProjection,
    mono: MonoMs,
    progress: Vec<AttemptNo>,
    descriptions: Vec<(ApprovalId, OperationDescription)>,
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

/// The kinds accepted while in `clock_fault`: the supervisor can still stop and end its own child (r5 6.5), record the
/// identity of a child that started meanwhile, and record a repair (F2).
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

/// `a + b` in monotonic time; an overflowing deadline is already passed (fail closed).
fn deadline(start: MonoMs, duration: u64) -> MonoMs {
    MonoMs::new(start.ms().checked_add(duration).unwrap_or(0))
}

/// Starts supervising a new run from its `RunCreated` draft. The result becomes live only through `open`, after the
/// frame it holds is durable.
pub(crate) fn begin_new(epoch: LiveEpoch, run: RunId, run_created: EventDraft, mono: MonoMs) -> Result<PendingSupervision, Rejection> {
    if run_created.body().kind() != EventKind::RunCreated {
        return Err(Rejection::InvalidTransition);
    }
    let applied = apply(&RunProjection::empty(run), &[run_created], JournalSize { events: 0, bytes: 0 })?;
    let live = LiveSupervision {
        epoch,
        projection: applied.projection,
        last_mono: mono,
        clock_fault: false,
        frames: 0,
        in_flight: None,
        unresolved: None,
        approvals: Vec::new(),
        attempts: Vec::new(),
        descriptions: Vec::new(),
        outstanding: Vec::new(),
    };
    Ok(PendingSupervision { live, events: applied.events })
}

impl PendingSupervision {
    /// The `RunCreated` frame to write.
    pub(crate) fn events(&self) -> &[Event] {
        &self.events
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

    pub(crate) const fn in_clock_fault(&self) -> bool {
        self.clock_fault
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
        let (mono, _) = self.observe(mono);
        let refuse = |refusal, grant| Err(StageRejected { refusal, grant });
        if self.in_flight.is_some() {
            return refuse(LiveRefusal::InFlight, grant);
        }
        if self.unresolved.is_some() {
            return refuse(LiveRefusal::Unresolved, grant);
        }
        if self.clock_fault && (grant.is_some() || drafts.iter().any(|draft| !safe_in_clock_fault(draft.body().kind()))) {
            return refuse(LiveRefusal::ClockFault, grant);
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
        let applied = match apply(&self.projection, drafts, size) {
            Ok(applied) => applied,
            Err(rejection) => return refuse(LiveRefusal::Rejected(rejection), grant),
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
        self.frames += 1;
        self.in_flight = Some(self.frames);
        Ok(Staged { epoch: self.epoch, frame: self.frames, events: applied.events, projection: applied.projection, mono, progress, descriptions, grant })
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
        if staged.epoch != self.epoch || self.in_flight != Some(staged.frame) {
            return Err(LiveRefusal::NotThisSupervision);
        }
        self.in_flight = None;
        let Staged { events, projection, mono, progress, descriptions, grant, .. } = staged;
        if let Some(grant) = grant {
            self.outstanding.retain(|approval| *approval != grant.approval);
        }
        Ok(self.settle(&events, projection, mono, &progress, descriptions, true))
    }

    /// The staged frame's write was uncertain or failed. Nothing is minted, a consumed grant is dropped (consumptions
    /// are never retried), and the supervision stays unresolved until `adopt` or `discard` (F8).
    pub(crate) fn abandon(&mut self, staged: Staged) -> Result<Abandoned, LiveRefusal> {
        if staged.epoch != self.epoch || self.in_flight != Some(staged.frame) {
            return Err(LiveRefusal::NotThisSupervision);
        }
        self.in_flight = None;
        self.unresolved = Some(staged.frame);
        let Staged { epoch, frame, events, projection, mono, progress, descriptions, grant } = staged;
        if let Some(grant) = grant {
            self.outstanding.retain(|approval| *approval != grant.approval);
        }
        Ok(Abandoned { epoch, frame, events, projection, mono, progress, descriptions })
    }

    /// The abandoned frame is in the file exactly (the adapter's byte-exact check said it landed): the file is the
    /// truth, so the live projection takes it. Nothing is minted; the caller records what r5 5.4 requires (for example
    /// `ApprovalVoided{write_uncertain}` for a landed grant).
    pub(crate) fn adopt(&mut self, abandoned: Abandoned) -> Result<(), LiveRefusal> {
        if abandoned.epoch != self.epoch || self.unresolved != Some(abandoned.frame) {
            return Err(LiveRefusal::NotThisSupervision);
        }
        self.unresolved = None;
        let Abandoned { events, projection, mono, progress, descriptions, .. } = abandoned;
        self.settle(&events, projection, mono, &progress, descriptions, false);
        Ok(())
    }

    /// The abandoned frame is absent from the file: nothing changes, and its non-consumption drafts may be staged again.
    pub(crate) fn discard(&mut self, abandoned: Abandoned) -> Result<(), LiveRefusal> {
        if abandoned.epoch != self.epoch || self.unresolved != Some(abandoned.frame) {
            return Err(LiveRefusal::NotThisSupervision);
        }
        self.unresolved = None;
        Ok(())
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
                    if let (true, false, true, Some(use_by)) = (mint, self.clock_fault, still_granted, use_by) {
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
        let in_reserve = size.in_reserve() || self.projection.events().len() >= (MAX_EVENTS - RESERVED_EVENTS) as usize;
        let stop = match (started_or_starting, limits) {
            (false, _) | (_, None) => None,
            (true, _) if self.clock_fault => Some(StopReason::ClockFault),
            (true, _) if in_reserve => Some(StopReason::JournalFull),
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
}

impl Abandoned {
    /// The frame whose write was uncertain, for the adapter's byte-exact comparison.
    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }
}

#[cfg(test)]
mod tests;
