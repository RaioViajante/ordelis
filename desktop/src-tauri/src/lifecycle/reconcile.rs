// Conservative crash reconciliation and cross-run start conflicts (D3.2b-3, issue #116; design r5 sections 7 and 8,
// decisions 12.7 and 12.18, with the gate review's F3). Pure: histories, observations, run summaries, monotonic
// times and epochs are values the caller supplies. Nothing here looks at a process, a process table, a file or a
// clock.
//
// Reconciliation turns a recorded history and untrusted observations into one opening frame of repair and reconciler
// drafts, informational findings and inert recommendations. It never creates authority: it ends the attempt that may
// still be in flight, and the reducer withdraws every pending request and voids every unused grant of that attempt as
// generated consequences. A recorded decision stays history. Reconciliation never concludes "never started" after a
// detected repair, never ends an attempt as succeeded or failed, and never stops anything: a termination proposal is a
// plain value with no method and no consumer, and acting on it needs a separate, future policy decision (#108
// decision 13).
use super::model::{
    AttemptNo, AttemptState, ConcernClass, ConcernRef, Event, EventBody, EventDraft, InterruptObservation, JournalDraft,
    JournalFault, JournalRepair, JournalSize, LiveEpoch, MonoMs, NotStartedCause, NotStartedReason, OsIdentity,
    ReconcilerDraft, Rejection, RunId, RunState, MAX_OBSERVATION_AGE_MS,
};
use super::live::SupervisedRun;
use super::reduce::{apply, AttemptRecord, RunProjection};

// ---------------------------------------------------------------------------------------------------------------
// Recorded histories

/// Why a recorded history cannot be trusted past its projection. A quarantined run is read-only, is never reconciled,
/// and is a blocking condition for every start until a separately approved recovery policy exists (r5 12.18).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum QuarantineReason {
    /// Replay stopped at this fault (codec damage included); the projection is the state before it.
    Fault(JournalFault),
    /// A quarantine marker exists for the run.
    Marker,
}

/// How far the caller's replay of a journal can be trusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryStatus {
    /// The whole journal replayed without a fault, after the marker, high-water-mark and sidecar checks of r5 5.7
    /// steps 1 to 5. The core cannot check this: it is the caller's statement.
    Verified,
    Quarantined(QuarantineReason),
}

/// A run's recorded history as the caller (the future replay and storage adapter, b-4 and later) supplies it: a
/// projection rebuilt from recorded frames, how far it can be trusted, and the repair records the journal still lacks.
/// History only: it can be summarized and reconciled, and nothing turns it into supervision or a grant except
/// `begin_reconciled`, which mints none.
pub(crate) struct RecordedHistory {
    projection: RunProjection,
    status: HistoryStatus,
    unrecorded_repairs: Vec<JournalRepair>,
}

impl RecordedHistory {
    pub(crate) fn new(projection: RunProjection, status: HistoryStatus, unrecorded_repairs: Vec<JournalRepair>) -> RecordedHistory {
        RecordedHistory { projection, status, unrecorded_repairs }
    }

    pub(crate) fn projection(&self) -> &RunProjection {
        &self.projection
    }

    pub(crate) const fn status(&self) -> HistoryStatus {
        self.status
    }

    pub(crate) fn unrecorded_repairs(&self) -> &[JournalRepair] {
        &self.unrecorded_repairs
    }
}

/// The repair recorded for `sidecar`, if any.
fn recorded_repair(projection: &RunProjection, sidecar: u32) -> Option<JournalRepair> {
    projection.events().iter().find_map(|event| match &event.body {
        EventBody::JournalRepaired(repair) if repair.sidecar() == sidecar => Some(*repair),
        _ => None,
    })
}

/// Nothing is in flight and nothing is owed: no attempt that is not terminal, no pending request, no unused grant,
/// and every repair the caller reported is recorded with the same content.
pub(crate) fn is_quiescent(projection: &RunProjection, repairs: &[JournalRepair]) -> bool {
    projection.current_attempt().is_none()
        && projection.approvals().iter().all(|record| !record.state().is_open())
        && repairs.iter().all(|repair| recorded_repair(projection, repair.sidecar()) == Some(*repair))
}

/// Whether a `JournalRepaired` follows the attempt's last lifecycle event: the removed bytes may have held a start.
fn repaired_since(projection: &RunProjection, record: &AttemptRecord) -> bool {
    projection.events().iter().any(|event| event.seq > record.last_change() && matches!(event.body, EventBody::JournalRepaired(_)))
}

// ---------------------------------------------------------------------------------------------------------------
// Observations (untrusted input)

/// What a future adapter saw about a recorded child. Untrusted: it can only choose between kinds of "unknown"; it never
/// clears a concern, grants anything, or makes an outcome a success or a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Seen {
    /// Reserved for a future containment mechanism; treated as unobservable in v1.
    GoneConfirmed,
    PidAbsent,
    PidReused,
    /// The same pid, start token and executable hash. Still not proof of ownership.
    IdentityMatches,
    Unobservable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttemptObservation {
    pub(crate) attempt: AttemptNo,
    /// The identity the observer looked for; it must equal the journal's.
    pub(crate) recorded: OsIdentity,
    pub(crate) seen: Seen,
    pub(crate) epoch: LiveEpoch,
    pub(crate) taken_mono: MonoMs,
}

/// The reconciliation's own reading: the epoch of the supervision it opens, its monotonic time (for observation age),
/// the wall-clock reading its drafts state, and the journal size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReconcileInput {
    pub(crate) epoch: LiveEpoch,
    pub(crate) mono: MonoMs,
    pub(crate) stated_at_ms: u64,
    pub(crate) size: JournalSize,
}

/// A reconciliation note for display. Every finding is informational: none blocks, clears or permits anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FindingKind {
    /// The history is quarantined: nothing was reconciled.
    Quarantined,
    /// No observation of an attempt with a recorded identity: treated as unobservable.
    ObservationMissing,
    /// An observation of another epoch, taken after the reconciliation's time, or older than `MAX_OBSERVATION_AGE_MS`:
    /// treated as unobservable.
    ObservationStale,
    /// An observation whose identity differs from the recorded one, or of an attempt with no recorded identity: ignored.
    ObservationMismatched,
    /// An observation of an attempt that has ended or does not exist (a late report, for example): ignored.
    ObservationUnexpected,
    /// Fresh, matching observations of one attempt that disagree: treated as unobservable.
    ObservationsContradictory,
    /// `gone_confirmed` needs containment that v1 does not have: treated as unobservable.
    ConfirmationUnsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Finding {
    pub(crate) attempt: Option<AttemptNo>,
    pub(crate) kind: FindingKind,
}

// ---------------------------------------------------------------------------------------------------------------
// Recommendations (inert)

/// What a termination proposal rests on: the recorded pid, start token and executable hash matched a fresh observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProposalBasis {
    PidStartTokenAndHash,
}

/// Acting on a proposal needs a separate, future policy that re-verifies the process independently (#108 decision 13).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PolicyRequirement {
    SeparatePolicy,
}

/// Advice for the user, shown as information. Plain data: no method, no capability, no consumer. Nothing in Ordelis
/// acts on a recommendation; in particular a termination proposal carries no process identity and stops nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Recommendation {
    ReviewAttempt(AttemptNo),
    TerminationProposal { attempt: AttemptNo, basis: ProposalBasis, policy: PolicyRequirement },
    RetryPossible(AttemptNo),
}

// ---------------------------------------------------------------------------------------------------------------
// Reconciliation

/// The opening frame `reconcile` computed for one recorded history (its projection, status and missing repairs), one
/// epoch and one monotonic time. Built only here; `begin_reconciled` accepts it only for exactly that history and
/// epoch. It cannot be cloned and is consumed by value, so each frame opens at most once. That one run has at most one
/// supervision is the adapter's lock (r5 5.3): `reconcile` called twice gives two frames.
pub(crate) struct ReconciledFrame {
    epoch: LiveEpoch,
    mono: MonoMs,
    status: HistoryStatus,
    basis: RunProjection,
    repairs: Vec<JournalRepair>,
    drafts: Vec<EventDraft>,
}

impl ReconciledFrame {
    /// Repair records first, in sidecar order as reported, then at most one reconciler draft.
    pub(crate) fn drafts(&self) -> &[EventDraft] {
        &self.drafts
    }

    pub(crate) const fn epoch(&self) -> LiveEpoch {
        self.epoch
    }

    pub(crate) const fn mono(&self) -> MonoMs {
        self.mono
    }

    /// Whether this frame was computed for exactly this history: the same status, projection and missing repairs.
    pub(crate) fn computed_for(&self, history: &RecordedHistory) -> bool {
        self.status == history.status && self.basis == history.projection && self.repairs == history.unrecorded_repairs
    }
}

/// What reconciliation found. `blocking` and `informational` are the run's unresolved concerns after the frame, kept
/// apart: each blocking concern stops a new attempt in this run and every start elsewhere until the user resolves it;
/// informational ones never block. The recommendations are private and nothing outside tests reads them: showing them
/// belongs to a later, separately reviewed interface.
pub(crate) struct Reconciliation {
    pub(crate) frame: ReconciledFrame,
    pub(crate) findings: Vec<Finding>,
    recommendations: Vec<Recommendation>,
    pub(crate) blocking: Vec<ConcernRef>,
    pub(crate) informational: Vec<ConcernRef>,
}

impl Reconciliation {
    #[cfg(test)]
    pub(crate) fn recommendations(&self) -> &[Recommendation] {
        &self.recommendations
    }
}

/// Why no reconciliation could be computed. The run then stays unreconciled, which blocks every start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReconcileRefused {
    /// No `RunCreated`: a run with no valid frame is the adapter's to classify (gate-review finding F6).
    NoRun,
    /// More than two repair records are missing: opening failed repeatedly, and the adapter quarantines (r5 5.7).
    TooManyRepairs,
    /// A reported repair names a sidecar already recorded with other content, or a sidecar reported twice.
    RepairConflict,
    /// The reducer refused the frame.
    Rejected(Rejection),
}

/// Reconciles one recorded history (r5 section 7.2). A quarantined history yields no draft at all. Otherwise the frame
/// records the missing repairs, then ends the attempt that is not terminal, if any:
///
/// - `proposed` or `approved`, journal intact since its last lifecycle event: `not_started{supervision_lost}`;
/// - the same after a recorded or reported repair: `interrupted{evidence_lost}`, never "not started";
/// - `starting`, or `stopping` without identity: `interrupted{no_identity}`;
/// - with a recorded identity: by its observations, `not_running` (pid absent or reused), `possibly_running` (identity
///   matches), or `unknown` (missing, stale, mismatched, contradictory or `gone_confirmed`).
///
/// Every interruption raises a blocking `possible_orphan`. Deterministic, and idempotent: on a history that already
/// holds its frame it yields no draft.
pub(crate) fn reconcile(history: &RecordedHistory, observations: &[AttemptObservation], input: ReconcileInput) -> Result<Reconciliation, ReconcileRefused> {
    let projection = &history.projection;
    let mut findings = Vec::new();
    let mut recommendations = Vec::new();
    let mut drafts = Vec::new();
    let quarantined = matches!(history.status, HistoryStatus::Quarantined(_));
    if quarantined {
        findings.push(Finding { attempt: None, kind: FindingKind::Quarantined });
    } else if projection.spec().is_none() {
        return Err(ReconcileRefused::NoRun);
    } else {
        for (index, repair) in history.unrecorded_repairs.iter().enumerate() {
            if history.unrecorded_repairs[..index].iter().any(|earlier| earlier.sidecar() == repair.sidecar()) {
                return Err(ReconcileRefused::RepairConflict);
            }
            match recorded_repair(projection, repair.sidecar()) {
                Some(recorded) if recorded == *repair => {}
                Some(_) => return Err(ReconcileRefused::RepairConflict),
                None => drafts.push(EventDraft::journal(JournalDraft::Repaired(*repair), input.stated_at_ms)),
            }
        }
        if drafts.len() > 2 {
            return Err(ReconcileRefused::TooManyRepairs);
        }
    }
    let repairing = !drafts.is_empty();
    let mut observed = None;
    if let Some(record) = projection.current_attempt() {
        let attempt = record.attempt();
        if quarantined {
            recommendations.push(Recommendation::ReviewAttempt(attempt));
        } else {
            let draft = match (record.state(), record.identity()) {
                (AttemptState::Proposed | AttemptState::Approved, _) if repairing || repaired_since(projection, record) => {
                    ReconcilerDraft::AttemptInterrupted { attempt, observation: InterruptObservation::EvidenceLost }
                }
                (AttemptState::Proposed | AttemptState::Approved, _) => {
                    ReconcilerDraft::AttemptNotStarted { attempt, reason: NotStartedCause::SupervisionLost }
                }
                (_, None) => ReconcilerDraft::AttemptInterrupted { attempt, observation: InterruptObservation::NoIdentity },
                (_, Some(identity)) => {
                    observed = Some(attempt);
                    let observation = judge_observations(attempt, identity, observations, input, &mut findings);
                    ReconcilerDraft::AttemptInterrupted { attempt, observation }
                }
            };
            if let ReconcilerDraft::AttemptInterrupted { observation, .. } = draft {
                recommendations.push(Recommendation::ReviewAttempt(attempt));
                if observation == InterruptObservation::PossiblyRunning {
                    recommendations.push(Recommendation::TerminationProposal {
                        attempt,
                        basis: ProposalBasis::PidStartTokenAndHash,
                        policy: PolicyRequirement::SeparatePolicy,
                    });
                }
            }
            drafts.push(EventDraft::reconciler(draft, input.stated_at_ms));
        }
    }
    // Observations of anything else are ignored: of an attempt with no recorded identity, or of one that has ended or
    // does not exist.
    for observation in observations.iter().filter(|observation| Some(observation.attempt) != observed) {
        let kind = match projection.attempt(observation.attempt).map(AttemptRecord::state) {
            Some(state) if !state.is_terminal() => FindingKind::ObservationMismatched,
            _ => FindingKind::ObservationUnexpected,
        };
        findings.push(Finding { attempt: Some(observation.attempt), kind });
    }
    findings.sort();
    findings.dedup();
    let after = if drafts.is_empty() {
        projection.clone()
    } else {
        apply(projection, &drafts, input.size).map_err(ReconcileRefused::Rejected)?.projection
    };
    if let Some(record) = projection.current_attempt() {
        let retry = after.attempt(record.attempt()).map(AttemptRecord::state)
            == Some(AttemptState::NotStarted(NotStartedReason::SupervisionLost));
        if retry && after.state() == Some(RunState::Open) && !after.has_unresolved_blocking_concern() {
            recommendations.push(Recommendation::RetryPossible(record.attempt()));
        }
    }
    let unresolved = |class: ConcernClass| {
        after.concerns().iter().filter(|concern| concern.resolution().is_none() && concern.concern().kind().class() == class).map(|concern| concern.concern()).collect()
    };
    let (blocking, informational) = (unresolved(ConcernClass::Blocking), unresolved(ConcernClass::Informational));
    let frame = ReconciledFrame {
        epoch: input.epoch,
        mono: input.mono,
        status: history.status,
        basis: projection.clone(),
        repairs: history.unrecorded_repairs.clone(),
        drafts,
    };
    Ok(Reconciliation { frame, findings, recommendations, blocking, informational })
}

/// What the observations of a started attempt support. Only a fresh observation, of this epoch, of exactly the recorded
/// identity counts; fresh observations must all agree. Anything else is `unknown`, with a finding.
fn judge_observations(
    attempt: AttemptNo,
    identity: &OsIdentity,
    observations: &[AttemptObservation],
    input: ReconcileInput,
    findings: &mut Vec<Finding>,
) -> InterruptObservation {
    let mut note = |kind| findings.push(Finding { attempt: Some(attempt), kind });
    let mut fresh: Vec<Seen> = Vec::new();
    let mut any = false;
    for observation in observations.iter().filter(|observation| observation.attempt == attempt) {
        any = true;
        if observation.recorded != *identity {
            note(FindingKind::ObservationMismatched);
            continue;
        }
        let age = input.mono.ms().checked_sub(observation.taken_mono.ms());
        if observation.epoch != input.epoch || age.map_or(true, |age| age > MAX_OBSERVATION_AGE_MS) {
            note(FindingKind::ObservationStale);
            continue;
        }
        fresh.push(observation.seen);
    }
    if !any {
        note(FindingKind::ObservationMissing);
    }
    let Some(first) = fresh.first().copied() else { return InterruptObservation::Unknown };
    if fresh.iter().any(|seen| *seen != first) {
        note(FindingKind::ObservationsContradictory);
        return InterruptObservation::Unknown;
    }
    match first {
        Seen::PidAbsent | Seen::PidReused => InterruptObservation::NotRunning,
        Seen::IdentityMatches => InterruptObservation::PossiblyRunning,
        Seen::Unobservable => InterruptObservation::Unknown,
        Seen::GoneConfirmed => {
            note(FindingKind::ConfirmationUnsupported);
            InterruptObservation::Unknown
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Cross-run start conflicts (r5 7.4 and decision 12.18, with gate-review finding F3)

/// Why a run could not be verified. Every one of these blocks every start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Unverified {
    /// The counter allocated the id, but the journal is missing while its mark says something was written.
    Missing,
    Unreadable,
    /// Another instance holds the run's lock.
    HeldElsewhere,
    /// The valid prefix does not match the high-water mark of a run nobody has opened yet.
    MarkMismatch,
    /// A journal with no `RunCreated` that is not recorded as never created (F6).
    NoRunCreated,
}

/// How a verified summary was read. Only a full replay under the run's lock can show that a run has no blocking
/// condition; a read without the lock, or a derived index, may only add conflicts, never remove them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadBasis {
    Locked,
    LockFree,
    DerivedIndex,
}

/// A run summarized from a verified history. Built only by `summarize`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedRun {
    run: RunId,
    read: ReadBasis,
    orphans: Vec<AttemptNo>,
    /// An unresolved blocking concern about the run rather than one attempt (none exists in v1).
    run_blocked: bool,
    in_flight: Option<AttemptNo>,
    repairs_pending: bool,
    /// The recorded events summarized, so that a supervision claiming the run can be checked against them.
    events: Vec<Event>,
}

/// The closed summary of one allocated run id (F3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RunSummary {
    /// The id was allocated and nothing was ever written (mark `(0, 0)` and the never-created marker).
    NeverCreated,
    Quarantined(QuarantineReason),
    Unverified(Unverified),
    Verified(VerifiedRun),
}

/// Summarizes one recorded history read as `read`.
pub(crate) fn summarize(history: &RecordedHistory, read: ReadBasis) -> RunSummary {
    if let HistoryStatus::Quarantined(reason) = history.status {
        return RunSummary::Quarantined(reason);
    }
    let projection = &history.projection;
    if projection.spec().is_none() {
        return RunSummary::Unverified(Unverified::NoRunCreated);
    }
    let blocking: Vec<ConcernRef> = projection.concerns().iter().filter(|concern| concern.is_unresolved_blocking()).map(|concern| concern.concern()).collect();
    RunSummary::Verified(VerifiedRun {
        run: projection.run(),
        read,
        orphans: blocking.iter().filter_map(ConcernRef::attempt).collect(),
        run_blocked: blocking.iter().any(|concern| concern.attempt().is_none()),
        in_flight: projection.current_attempt().map(AttemptRecord::attempt),
        // A repair not recorded with the same content, or a sidecar reported twice (which `reconcile` refuses), is owed.
        repairs_pending: history.unrecorded_repairs.iter().enumerate().any(|(index, repair)| {
            recorded_repair(projection, repair.sidecar()) != Some(*repair)
                || history.unrecorded_repairs[..index].iter().any(|earlier| earlier.sidecar() == repair.sidecar())
        }),
        events: projection.events().to_vec(),
    })
}

/// Every run the caller knows of, enumerated from the run-id counter: ids `r1` to `r<allocated>`, each with exactly one
/// summary. `Unknown` when the counter or the marks could not be read. Never a directory listing or a cached index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RunInventory {
    Unknown,
    Enumerated { allocated: u64, summaries: Vec<(RunId, RunSummary)> },
}

/// One reason that no start may happen now, anywhere. Global in v1 (r5 12.18): a blocking condition in any run stops
/// every start in every run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StartConflict {
    InventoryUnknown,
    /// Allocated ids with no summary.
    Unaccounted { from: RunId, through: RunId },
    /// A summary that contradicts the inventory: a second one for an id, an id the counter never allocated, a summary
    /// of another run, or a supervised run that is missing from the inventory or said never to have been created.
    Contradictory(RunId),
    Quarantined(RunId, QuarantineReason),
    Unverified(RunId, Unverified),
    /// Read without the lock or from an index: it can block, never clear.
    UnverifiedRead(RunId),
    PossibleOrphan(RunId, AttemptNo),
    /// An unresolved blocking concern about the run itself.
    BlockingConcern(RunId),
    /// An attempt that is not terminal and not supervised by the caller: never reconciled, a failed reconciliation
    /// append, or held by another instance.
    Unreconciled(RunId, AttemptNo),
    /// Repair records the journal still lacks.
    RepairPending(RunId),
}

/// Every start conflict in `inventory`, sorted and without repeats. `supervised` names the runs the caller's own live
/// supervisions hold; each entry borrows an opened `LiveSupervision`, so a pending or dropped supervision cannot be
/// claimed. A supervision excuses a run's attempt in flight only if its own recorded history extends the summarized
/// one; a supervision that does not (one opened under a reused id, for example) is `Contradictory` and excuses nothing.
/// A supervision's own unresolved blocking concerns are conflicts too, since it may be ahead of every summary. That a
/// supervision is the run's only one remains the adapter's lock. Nothing here orders events across journals. An empty
/// result says only that the supplied data shows no conflict: it permits nothing, and the future execution boundary
/// must still refuse to record a start while any conflict exists and apply its own policy and the Execution Safety Gate.
pub(crate) fn start_conflicts(inventory: &RunInventory, supervised: &[SupervisedRun<'_>]) -> Vec<StartConflict> {
    let claimed = |run: &RunId| supervised.iter().filter(|held| held.run() == *run).collect::<Vec<_>>();
    let RunInventory::Enumerated { allocated, summaries } = inventory else { return Vec::from([StartConflict::InventoryUnknown]) };
    let allocated = *allocated;
    if allocated > 0 && RunId::new(allocated).is_err() {
        return Vec::from([StartConflict::InventoryUnknown]);
    }
    let mut conflicts = Vec::new();
    let mut seen: Vec<RunId> = Vec::new();
    for (run, summary) in summaries {
        if run.number() > allocated || seen.contains(run) {
            conflicts.push(StartConflict::Contradictory(*run));
        }
        seen.push(*run);
        let holders = claimed(run);
        let held = !holders.is_empty();
        if holders.len() > 1 {
            conflicts.push(StartConflict::Contradictory(*run));
        }
        match summary {
            RunSummary::NeverCreated if held => conflicts.push(StartConflict::Contradictory(*run)),
            RunSummary::NeverCreated => {}
            RunSummary::Quarantined(reason) => conflicts.push(StartConflict::Quarantined(*run, *reason)),
            RunSummary::Unverified(why) => conflicts.push(StartConflict::Unverified(*run, *why)),
            RunSummary::Verified(verified) => {
                if verified.run != *run {
                    conflicts.push(StartConflict::Contradictory(*run));
                }
                if verified.read != ReadBasis::Locked {
                    conflicts.push(StartConflict::UnverifiedRead(*run));
                }
                for attempt in &verified.orphans {
                    conflicts.push(StartConflict::PossibleOrphan(*run, *attempt));
                }
                if verified.run_blocked {
                    conflicts.push(StartConflict::BlockingConcern(*run));
                }
                if verified.repairs_pending {
                    conflicts.push(StartConflict::RepairPending(*run));
                }
                let owned = holders.len() == 1 && holders[0].extends(&verified.events);
                if held && !owned {
                    conflicts.push(StartConflict::Contradictory(*run));
                }
                if let (Some(attempt), false) = (verified.in_flight, owned) {
                    conflicts.push(StartConflict::Unreconciled(*run, attempt));
                }
            }
        }
    }
    for held in supervised {
        if !seen.contains(&held.run()) {
            conflicts.push(StartConflict::Contradictory(held.run()));
        }
        // A supervision may be ahead of every summary (an orphan its opening frame just raised, for example): its own
        // unresolved blocking concerns count too. This only ever adds conflicts.
        for concern in held.blocking() {
            conflicts.push(match concern.attempt() {
                Some(attempt) => StartConflict::PossibleOrphan(held.run(), attempt),
                None => StartConflict::BlockingConcern(held.run()),
            });
        }
    }
    // Allocated ids with no summary, as ranges.
    seen.sort();
    seen.dedup();
    let mut expected = 1;
    for run in seen.iter().filter(|run| run.number() <= allocated) {
        if run.number() > expected {
            if let (Ok(from), Ok(through)) = (RunId::new(expected), RunId::new(run.number() - 1)) {
                conflicts.push(StartConflict::Unaccounted { from, through });
            }
        }
        expected = run.number() + 1;
    }
    if expected <= allocated {
        if let (Ok(from), Ok(through)) = (RunId::new(expected), RunId::new(allocated)) {
            conflicts.push(StartConflict::Unaccounted { from, through });
        }
    }
    conflicts.sort();
    conflicts.dedup();
    conflicts
}

#[cfg(test)]
mod tests;
