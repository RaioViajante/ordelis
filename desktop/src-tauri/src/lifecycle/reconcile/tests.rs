// Reconciliation, begin_reconciled and start-conflict tests. Pure: every history, observation and time is synthetic;
// nothing reads a file, a clock or a process table, and nothing is started or stopped.
use super::*;
use crate::agents::catalog::ProviderId;
use crate::lifecycle::live::{begin_new, begin_reconciled, open, BeginRefused, LiveGrant, LiveRefusal, LiveSupervision, Opening};
use crate::lifecycle::model::*;
use crate::lifecycle::reduce::tests::*;
use crate::lifecycle::reduce::{apply_recorded, Applied};

const EPOCH: LiveEpoch = LiveEpoch::new(9);
const MONO: u64 = 50_000;
const AT: u64 = T0 + 100_000;

fn size(projection: &RunProjection) -> JournalSize {
    JournalSize { events: projection.events().len() as u32, bytes: 0 }
}

fn input(projection: &RunProjection) -> ReconcileInput {
    ReconcileInput { epoch: EPOCH, mono: MonoMs::new(MONO), stated_at_ms: AT, size: size(projection) }
}

fn verified(journal: &Journal) -> RecordedHistory {
    let replayed = journal.replayed();
    assert_eq!(replayed, journal.projection);
    RecordedHistory::new(replayed, HistoryStatus::Verified, Vec::new())
}

fn with(mut journal: Journal, draft: EventDraft) -> Journal {
    journal.one(draft);
    journal
}

fn observe(seen: Seen) -> AttemptObservation {
    AttemptObservation { attempt: n(1), recorded: identity(), seen, epoch: EPOCH, taken_mono: MonoMs::new(MONO - 1_000) }
}

fn repair(sidecar: u32) -> JournalRepair {
    JournalRepair::new(sidecar, 40, 7, RepairCause::IncompleteTail).unwrap()
}

fn reconciled(history: &RecordedHistory, observations: &[AttemptObservation]) -> Reconciliation {
    match reconcile(history, observations, input(history.projection())) {
        Ok(reconciliation) => reconciliation,
        Err(refused) => panic!("{refused:?}"),
    }
}

/// The projection after the reconciliation's frame.
fn after(history: &RecordedHistory, reconciliation: &Reconciliation) -> RunProjection {
    if reconciliation.frame.drafts().is_empty() {
        return history.projection().clone();
    }
    match apply(history.projection(), reconciliation.frame.drafts(), size(history.projection())) {
        Ok(applied) => applied.projection,
        Err(rejection) => panic!("{rejection:?}"),
    }
}

fn kinds(reconciliation: &Reconciliation) -> Vec<(Producer, EventBody)> {
    reconciliation.frame.drafts().iter().map(|draft| (draft.producer(), draft.body().clone())).collect()
}

fn interruption(observation: InterruptObservation) -> Vec<(Producer, EventBody)> {
    Vec::from([(Producer::Reconciler, EventBody::AttemptInterrupted { attempt: n(1), observation })])
}

fn orphan(projection: &RunProjection, attempt: u8) -> ConcernRef {
    projection.concerns().iter().map(|concern| concern.concern()).find(|c| c.kind() == ConcernKind::PossibleOrphan && c.attempt() == Some(n(attempt))).unwrap()
}

fn finding(kind: FindingKind) -> Finding {
    Finding { attempt: Some(n(1)), kind }
}

fn opened(history: RecordedHistory, frame: ReconciledFrame) -> LiveSupervision {
    let journal_size = size(history.projection());
    match begin_reconciled(EPOCH, history, frame, MonoMs::new(MONO), journal_size) {
        Ok(Opening::Ready(live)) => live,
        Ok(Opening::Pending(pending)) => open(pending),
        Err(refused) => panic!("{refused:?}"),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The rows of r5 7.2

#[test]
fn a_crash_before_the_start_with_an_intact_journal_never_started() {
    for (fixture, end) in [(proposed(), ApprovalState::Withdrawn), (approved(), ApprovalState::Voided)] {
        let history = verified(&fixture);
        let reconciliation = reconciled(&history, &[]);
        assert_eq!(
            kinds(&reconciliation),
            Vec::from([(Producer::Reconciler, EventBody::AttemptNotStarted { attempt: n(1), reason: NotStartedCause::SupervisionLost })])
        );
        let after = after(&history, &reconciliation);
        assert_eq!(after.attempt(n(1)).unwrap().state(), AttemptState::NotStarted(NotStartedReason::SupervisionLost));
        // The pending request is withdrawn and the unused grant voided, as generated consequences, never drafted.
        assert_eq!(after.approval(a(1)).unwrap().state(), end);
        let consequence = after.events().last().unwrap();
        assert_eq!(consequence.producer, Producer::Reconciler);
        assert!(matches!(
            consequence.body,
            EventBody::ApprovalWithdrawn { reason: WithdrawalReason::SupervisionLost, .. } | EventBody::ApprovalVoided { reason: VoidReason::SupervisionLost, .. }
        ));
        assert_eq!(reconciliation.recommendations, Vec::from([Recommendation::RetryPossible(n(1))]));
        assert!(reconciliation.blocking.is_empty());
        assert!(reconciliation.informational.is_empty());
        assert!(reconciliation.findings.is_empty());
    }
}

#[test]
fn a_detected_repair_is_never_read_as_never_started() {
    // A repair recorded after the attempt's last change, and a repair reported but not yet recorded.
    let recorded = with(approved(), repaired(1, T0 + 3_000));
    let reported = RecordedHistory::new(approved().projection, HistoryStatus::Verified, Vec::from([repair(2)]));
    for history in [verified(&recorded), reported] {
        let reconciliation = reconciled(&history, &[]);
        let drafts = kinds(&reconciliation);
        assert_eq!(drafts.last().unwrap(), &interruption(InterruptObservation::EvidenceLost)[0]);
        let after = after(&history, &reconciliation);
        assert_eq!(after.attempt(n(1)).unwrap().state(), AttemptState::Interrupted(InterruptObservation::EvidenceLost));
        assert_eq!(reconciliation.blocking, Vec::from([orphan(&after, 1)]));
        assert!(reconciliation.recommendations.contains(&Recommendation::ReviewAttempt(n(1))));
        assert!(!reconciliation.recommendations.iter().any(|r| matches!(r, Recommendation::RetryPossible(_))));
    }
    // A reported repair goes first, from the journal producer, and the reducer then sees it.
    let history = RecordedHistory::new(proposed().projection, HistoryStatus::Verified, Vec::from([repair(1), repair(2)]));
    let reconciliation = reconciled(&history, &[]);
    let producers: Vec<Producer> = reconciliation.frame.drafts().iter().map(EventDraft::producer).collect();
    assert_eq!(producers, Vec::from([Producer::Journal, Producer::Journal, Producer::Reconciler]));
    assert!(is_quiescent(&after(&history, &reconciliation), history.unrecorded_repairs()));
}

#[test]
fn a_crash_after_a_possible_start_without_identity_raises_an_orphan_and_proposes_nothing() {
    let stopping = with(starting(), cancel(CancelScope::Attempt(n(1)), T0 + 5_000));
    for fixture in [starting(), stopping] {
        let history = verified(&fixture);
        // An observation cannot apply to an attempt with no recorded identity, even one that "matches".
        let reconciliation = reconciled(&history, &[observe(Seen::IdentityMatches)]);
        assert_eq!(kinds(&reconciliation), interruption(InterruptObservation::NoIdentity));
        assert_eq!(reconciliation.recommendations, Vec::from([Recommendation::ReviewAttempt(n(1))]));
        assert_eq!(reconciliation.findings, Vec::from([finding(FindingKind::ObservationMismatched)]));
        let after = after(&history, &reconciliation);
        assert_eq!(reconciliation.blocking, Vec::from([orphan(&after, 1)]));
        assert_eq!(reconciliation.informational.iter().map(|c| c.kind()).collect::<Vec<_>>(), Vec::from([ConcernKind::WorkspaceUnknown]));
    }
}

#[test]
fn a_started_attempt_is_judged_only_by_fresh_matching_observations() {
    let history = verified(&running());
    let cases = [
        (Vec::new(), InterruptObservation::Unknown, Vec::from([FindingKind::ObservationMissing])),
        (Vec::from([observe(Seen::PidAbsent)]), InterruptObservation::NotRunning, Vec::new()),
        (Vec::from([observe(Seen::PidReused)]), InterruptObservation::NotRunning, Vec::new()),
        (Vec::from([observe(Seen::IdentityMatches)]), InterruptObservation::PossiblyRunning, Vec::new()),
        (Vec::from([observe(Seen::Unobservable)]), InterruptObservation::Unknown, Vec::new()),
        (Vec::from([observe(Seen::GoneConfirmed)]), InterruptObservation::Unknown, Vec::from([FindingKind::ConfirmationUnsupported])),
        (Vec::from([observe(Seen::PidAbsent), observe(Seen::PidAbsent)]), InterruptObservation::NotRunning, Vec::new()),
    ];
    for (observations, expected, notes) in cases {
        let reconciliation = reconciled(&history, &observations);
        assert_eq!(kinds(&reconciliation), interruption(expected), "{observations:?}");
        assert_eq!(reconciliation.findings, notes.into_iter().map(finding).collect::<Vec<_>>());
        // Every interruption is a blocking possible orphan, whatever was observed; nothing is stopped.
        let after = after(&history, &reconciliation);
        assert_eq!(reconciliation.blocking, Vec::from([orphan(&after, 1)]));
        let proposal = Recommendation::TerminationProposal { attempt: n(1), basis: ProposalBasis::PidStartTokenAndHash, policy: PolicyRequirement::SeparatePolicy };
        let expected_recommendations = if expected == InterruptObservation::PossiblyRunning {
            Vec::from([Recommendation::ReviewAttempt(n(1)), proposal])
        } else {
            Vec::from([Recommendation::ReviewAttempt(n(1))])
        };
        assert_eq!(reconciliation.recommendations, expected_recommendations);
    }
}

#[test]
fn stale_mismatched_and_contradictory_observations_never_establish_anything() {
    let history = verified(&running());
    let with_identity = |seen, recorded: OsIdentity| AttemptObservation { recorded, ..observe(seen) };
    let cases = [
        // Another epoch, taken after the reconciliation's time, one millisecond too old.
        (AttemptObservation { epoch: LiveEpoch::new(8), ..observe(Seen::PidAbsent) }, FindingKind::ObservationStale),
        (AttemptObservation { taken_mono: MonoMs::new(MONO + 1), ..observe(Seen::IdentityMatches) }, FindingKind::ObservationStale),
        (AttemptObservation { taken_mono: MonoMs::new(MONO - MAX_OBSERVATION_AGE_MS - 1), ..observe(Seen::PidAbsent) }, FindingKind::ObservationStale),
        // Another pid, start token or executable.
        (with_identity(Seen::PidAbsent, OsIdentity::new(4243, token("start-17"), hash('a')).unwrap()), FindingKind::ObservationMismatched),
        (with_identity(Seen::IdentityMatches, OsIdentity::new(4242, token("start-18"), hash('a')).unwrap()), FindingKind::ObservationMismatched),
        (with_identity(Seen::PidReused, OsIdentity::new(4242, token("start-17"), hash('d')).unwrap()), FindingKind::ObservationMismatched),
    ];
    for (observation, note) in cases {
        let reconciliation = reconciled(&history, &[observation.clone()]);
        assert_eq!(kinds(&reconciliation), interruption(InterruptObservation::Unknown), "{observation:?}");
        assert_eq!(reconciliation.findings, Vec::from([finding(note)]));
        assert!(!reconciliation.recommendations.iter().any(|r| matches!(r, Recommendation::TerminationProposal { .. })));
    }
    // Exactly at the maximum age an observation is still fresh.
    let edge = AttemptObservation { taken_mono: MonoMs::new(MONO - MAX_OBSERVATION_AGE_MS), ..observe(Seen::PidAbsent) };
    assert_eq!(kinds(&reconciled(&history, &[edge])), interruption(InterruptObservation::NotRunning));
    // Fresh observations that disagree, and a stale "gone" beside a fresh "matches": nothing is concluded from a stale one.
    let reconciliation = reconciled(&history, &[observe(Seen::PidAbsent), observe(Seen::IdentityMatches)]);
    assert_eq!(kinds(&reconciliation), interruption(InterruptObservation::Unknown));
    assert_eq!(reconciliation.findings, Vec::from([finding(FindingKind::ObservationsContradictory)]));
    let stale_gone = AttemptObservation { epoch: LiveEpoch::new(8), ..observe(Seen::PidAbsent) };
    let reconciliation = reconciled(&history, &[stale_gone, observe(Seen::IdentityMatches)]);
    assert_eq!(kinds(&reconciliation), interruption(InterruptObservation::PossiblyRunning));
    // Observations of an attempt that ended, or that does not exist, change nothing.
    let ended = verified(&with(running(), exited(1, 0, T0 + 5_000)));
    let reconciliation = reconciled(&ended, &[observe(Seen::IdentityMatches), AttemptObservation { attempt: n(5), ..observe(Seen::PidAbsent) }]);
    assert!(reconciliation.frame.drafts().is_empty());
    assert_eq!(reconciliation.findings, Vec::from([finding(FindingKind::ObservationUnexpected), Finding { attempt: Some(n(5)), kind: FindingKind::ObservationUnexpected }]));
}

#[test]
fn pending_and_unused_approvals_are_ended_by_generated_consequences_only() {
    // A running attempt with an operation request pending (a2) and another granted but unused (a3).
    let mut journal = running();
    journal.one(operation(2, 1, T0 + 5_000));
    journal.one(operation(3, 1, T0 + 6_000));
    journal.one(decide(3, Decision::Granted, T0 + 7_000));
    let history = verified(&journal);
    let reconciliation = reconciled(&history, &[observe(Seen::PidAbsent)]);
    // The reconciler drafts only the attempt event.
    assert_eq!(kinds(&reconciliation), interruption(InterruptObservation::NotRunning));
    let after = after(&history, &reconciliation);
    assert_eq!(after.approval(a(2)).unwrap().state(), ApprovalState::Withdrawn);
    assert_eq!(after.approval(a(3)).unwrap().state(), ApprovalState::Voided);
    let tail: Vec<&EventBody> = after.events()[journal.projection.events().len()..].iter().map(|e| &e.body).collect();
    assert_eq!(
        tail,
        Vec::from([
            &EventBody::AttemptInterrupted { attempt: n(1), observation: InterruptObservation::NotRunning },
            &EventBody::ApprovalWithdrawn { approval: a(2), reason: WithdrawalReason::SupervisionLost },
            &EventBody::ApprovalVoided { approval: a(3), reason: VoidReason::SupervisionLost },
        ])
    );
    assert!(is_quiescent(&after, &[]));
}

#[test]
fn a_closing_run_becomes_cancelled_when_its_attempt_is_interrupted() {
    let history = verified(&with(running(), cancel(CancelScope::Run, T0 + 5_000)));
    assert_eq!(history.projection().state(), Some(RunState::Closing));
    let reconciliation = reconciled(&history, &[observe(Seen::IdentityMatches)]);
    let after = after(&history, &reconciliation);
    assert_eq!(after.state(), Some(RunState::Cancelled));
    assert_eq!(after.attempt(n(1)).unwrap().state(), AttemptState::Interrupted(InterruptObservation::PossiblyRunning));
    assert_eq!(reconciliation.blocking, Vec::from([orphan(&after, 1)]));
}

#[test]
fn a_quarantined_history_is_never_reconciled_or_opened() {
    // Two terminal events in a stored journal: replay stops at the second (r5 7.3), and the run is quarantined.
    let journal = running();
    let mut frames = journal.frames.clone();
    let base = journal.projection.events().len() as u64;
    let exit = |seq: u64, code| Event {
        v: 1,
        run: run_id(),
        seq: Seq::new(seq).unwrap(),
        at_ms: WallMs::new(T0 + 6_000).unwrap(),
        clamped: false,
        producer: Producer::Supervisor,
        body: EventBody::AttemptExited { attempt: n(1), status: ExitStatus::new(Some(code), None).unwrap() },
    };
    frames.push(Vec::from([exit(base + 1, 0)]));
    frames.push(Vec::from([exit(base + 2, 1)]));
    let mut projection = RunProjection::empty(run_id());
    let mut fault = None;
    for frame in &frames {
        match apply_recorded(&projection, frame) {
            Ok(next) => projection = next,
            Err(found) => {
                fault = Some(found);
                break;
            }
        }
    }
    assert_eq!(fault, Some(JournalFault::DuplicateKey));
    let faulted = RecordedHistory::new(projection, HistoryStatus::Quarantined(QuarantineReason::Fault(JournalFault::DuplicateKey)), Vec::new());
    let reconciliation = reconciled(&faulted, &[observe(Seen::PidAbsent)]);
    assert!(reconciliation.frame.drafts().is_empty());
    assert!(reconciliation.findings.contains(&Finding { attempt: None, kind: FindingKind::Quarantined }));
    // A quarantine marker on a run with an attempt in flight: nothing is drafted, the attempt is shown for review.
    let marked = RecordedHistory::new(running().projection, HistoryStatus::Quarantined(QuarantineReason::Marker), Vec::from([repair(1)]));
    let reconciliation = reconciled(&marked, &[]);
    assert!(reconciliation.frame.drafts().is_empty());
    assert_eq!(reconciliation.recommendations, Vec::from([Recommendation::ReviewAttempt(n(1))]));
    let refused = begin_reconciled(EPOCH, marked, reconciliation.frame, MonoMs::new(MONO), JournalSize { events: 5, bytes: 0 });
    assert_eq!(refused.err(), Some(BeginRefused::Quarantined(QuarantineReason::Marker)));
    assert_eq!(summarize(&faulted, ReadBasis::Locked), RunSummary::Quarantined(QuarantineReason::Fault(JournalFault::DuplicateKey)));
}

#[test]
fn reconciliation_is_deterministic_and_idempotent() {
    let mut journal = running();
    journal.one(operation(2, 1, T0 + 5_000));
    let history = RecordedHistory::new(journal.replayed(), HistoryStatus::Verified, Vec::from([repair(4)]));
    let observations = [observe(Seen::PidAbsent), AttemptObservation { attempt: n(3), ..observe(Seen::Unobservable) }, AttemptObservation { epoch: LiveEpoch::new(2), ..observe(Seen::PidAbsent) }];
    let first = reconciled(&history, &observations);
    // The same input gives the same result, whatever the order of the observations.
    let mut reversed = observations.clone();
    reversed.reverse();
    for again in [reconciled(&history, &observations), reconciled(&history, &reversed)] {
        assert_eq!(kinds(&again), kinds(&first));
        assert_eq!(again.findings, first.findings);
        assert_eq!(again.recommendations, first.recommendations);
        assert_eq!(again.blocking, first.blocking);
        assert_eq!(again.informational, first.informational);
    }
    // Re-applying the frame is a duplicate; reconciling the result drafts nothing and keeps the orphan.
    let applied: Applied = apply(history.projection(), first.frame.drafts(), size(history.projection())).unwrap();
    let redelivered = apply(&applied.projection, first.frame.drafts(), size(&applied.projection)).unwrap();
    assert!(redelivered.duplicate && redelivered.events.is_empty());
    let settled = RecordedHistory::new(applied.projection.clone(), HistoryStatus::Verified, Vec::from([repair(4)]));
    let second = reconciled(&settled, &[observe(Seen::PidAbsent)]);
    assert!(second.frame.drafts().is_empty());
    assert_eq!(second.blocking, first.blocking);
    assert_eq!(second.informational, first.informational);
    // A crash before the frame was written leaves the history unchanged: reconciling again gives the same frame.
    assert_eq!(kinds(&reconciled(&history, &observations)), kinds(&first));
}

#[test]
fn late_and_conflicting_events_never_change_an_interrupted_attempt() {
    let history = verified(&running());
    let reconciliation = reconciled(&history, &[observe(Seen::PidAbsent)]);
    let mut journal = running();
    let end = Seq::new(journal.projection.events().len() as u64 + 1).unwrap();
    journal.ok(reconciliation.frame.drafts().to_vec());
    // A buffered exit delivered after the restart conflicts with the recorded end (identity is checked before the
    // terminal state, r5 4.4); another reconciler end conflicts too; a late relayed report meets the terminal attempt.
    assert_eq!(journal.refused(Vec::from([exited(1, 0, AT)])), Rejection::Conflict { existing: end });
    assert_eq!(journal.refused(Vec::from([interrupted(1, InterruptObservation::PossiblyRunning, AT)])), Rejection::Conflict { existing: end });
    assert_eq!(journal.refused(Vec::from([progress(1, AT)])), Rejection::AttemptTerminal);
    assert_eq!(journal.attempt(1), AttemptState::Interrupted(InterruptObservation::NotRunning));
}

#[test]
fn reconciliation_refuses_what_it_cannot_settle() {
    let empty = RecordedHistory::new(RunProjection::empty(run_id()), HistoryStatus::Verified, Vec::new());
    assert_eq!(reconcile(&empty, &[], input(empty.projection())).err(), Some(ReconcileRefused::NoRun));
    assert_eq!(summarize(&empty, ReadBasis::Locked), RunSummary::Unverified(Unverified::NoRunCreated));
    let three = RecordedHistory::new(proposed().projection, HistoryStatus::Verified, Vec::from([repair(1), repair(2), repair(3)]));
    assert_eq!(reconcile(&three, &[], input(three.projection())).err(), Some(ReconcileRefused::TooManyRepairs));
    // A repair already recorded with the same content is skipped; with other content it conflicts.
    let recorded = with(proposed(), repaired(1, T0 + 2_000)).projection;
    let same = RecordedHistory::new(recorded.clone(), HistoryStatus::Verified, Vec::from([repair(1)]));
    assert_eq!(kinds(&reconciled(&same, &[])), interruption(InterruptObservation::EvidenceLost));
    let other = JournalRepair::new(1, 41, 7, RepairCause::IncompleteTail).unwrap();
    let conflicting = RecordedHistory::new(recorded, HistoryStatus::Verified, Vec::from([other]));
    assert_eq!(reconcile(&conflicting, &[], input(conflicting.projection())).err(), Some(ReconcileRefused::RepairConflict));
}

// ---------------------------------------------------------------------------------------------------------------
// begin_reconciled

#[test]
fn begin_reconciled_opens_only_a_quiescent_run_from_its_own_frame() {
    // A frame to write: Pending, live only after open.
    let history = verified(&running());
    let reconciliation = reconciled(&history, &[observe(Seen::PidAbsent)]);
    let journal_size = size(history.projection());
    let pending = match begin_reconciled(EPOCH, history, reconciliation.frame, MonoMs::new(MONO), journal_size) {
        Ok(Opening::Pending(pending)) => pending,
        _ => panic!("expected a frame to write"),
    };
    assert_eq!(pending.events().len(), 1);
    assert_eq!(pending.events()[0].producer, Producer::Reconciler);
    let live = open(pending);
    assert_eq!(live.projection().attempt(n(1)).unwrap().state(), AttemptState::Interrupted(InterruptObservation::NotRunning));
    // Nothing to write: Ready at once.
    let ended = verified(&with(running(), exited(1, 0, T0 + 5_000)));
    let reconciliation = reconciled(&ended, &[]);
    let journal_size = size(ended.projection());
    assert!(matches!(begin_reconciled(EPOCH, ended, reconciliation.frame, MonoMs::new(MONO), journal_size), Ok(Opening::Ready(_))));
}

#[test]
fn begin_reconciled_refuses_another_history_epoch_or_time() {
    let refuse = |history: RecordedHistory, frame: ReconciledFrame, epoch: LiveEpoch, mono: u64| {
        let journal_size = size(history.projection());
        begin_reconciled(epoch, history, frame, MonoMs::new(mono), journal_size).err()
    };
    let frame = || reconciled(&verified(&running()), &[]).frame;
    assert_eq!(refuse(verified(&running()), frame(), LiveEpoch::new(10), MONO), Some(BeginRefused::NotThisHistory));
    assert_eq!(refuse(verified(&running()), frame(), EPOCH, MONO - 1), Some(BeginRefused::NotThisHistory));
    assert_eq!(refuse(verified(&starting()), frame(), EPOCH, MONO), Some(BeginRefused::NotThisHistory));
    let reported = RecordedHistory::new(running().projection, HistoryStatus::Verified, Vec::from([repair(1)]));
    assert_eq!(refuse(reported, frame(), EPOCH, MONO), Some(BeginRefused::NotThisHistory));
    let empty = RecordedHistory::new(RunProjection::empty(run_id()), HistoryStatus::Verified, Vec::new());
    assert_eq!(refuse(empty, frame(), EPOCH, MONO), Some(BeginRefused::NoRun));
    // Frames built around reconcile (possible only here, in its own tests) are still checked.
    let forged = |history: &RecordedHistory, drafts: Vec<EventDraft>| ReconciledFrame {
        epoch: EPOCH,
        mono: MonoMs::new(MONO),
        status: history.status(),
        basis: history.projection().clone(),
        repairs: history.unrecorded_repairs().to_vec(),
        drafts,
    };
    let running_history = verified(&running());
    let empty_frame = forged(&running_history, Vec::new());
    assert_eq!(refuse(running_history, empty_frame, EPOCH, MONO), Some(BeginRefused::NotQuiescent));
    let failed = || verified(&with(running(), exited(1, 3, T0 + 5_000)));
    let ended = failed();
    let concern = ended.projection().concerns()[0].concern();
    let user_frame = forged(&ended, Vec::from([resolve(concern, Resolution::Reviewed, AT)]));
    assert_eq!(refuse(ended, user_frame, EPOCH, MONO), Some(BeginRefused::NotReconciliation));
    // A quiescent run with a reported repair that the frame does not record.
    let reported_ended = RecordedHistory::new(failed().projection().clone(), HistoryStatus::Verified, Vec::from([repair(1)]));
    let unrecorded = forged(&reported_ended, Vec::new());
    assert_eq!(refuse(reported_ended, unrecorded, EPOCH, MONO), Some(BeginRefused::NotQuiescent));
    let repaired_history = verified(&with(approved(), repaired(1, T0 + 3_000)));
    let not_started = EventDraft::reconciler(ReconcilerDraft::AttemptNotStarted { attempt: n(1), reason: NotStartedCause::SupervisionLost }, AT);
    let wrong = forged(&repaired_history, Vec::from([not_started]));
    assert_eq!(refuse(repaired_history, wrong, EPOCH, MONO), Some(BeginRefused::Rejected(Rejection::InvalidTransition)));
    // A frame that leaves a reported repair unrecorded.
    let reported = RecordedHistory::new(proposed().projection, HistoryStatus::Verified, Vec::from([repair(1)]));
    let without = forged(&reported, Vec::from([EventDraft::reconciler(ReconcilerDraft::AttemptInterrupted { attempt: n(1), observation: InterruptObservation::EvidenceLost }, AT)]));
    assert_eq!(refuse(reported, without, EPOCH, MONO), Some(BeginRefused::Rejected(Rejection::InvalidTransition)));
}

#[test]
fn a_frame_is_bound_to_its_history_status_and_sidecars() {
    // A frame computed for a quarantined history does not open the same projection relabelled as verified.
    let quarantined = RecordedHistory::new(with(running(), exited(1, 3, T0 + 5_000)).projection, HistoryStatus::Quarantined(QuarantineReason::Fault(JournalFault::NewerVersion)), Vec::new());
    let frame = reconciled(&quarantined, &[]).frame;
    let relabelled = RecordedHistory::new(quarantined.projection().clone(), HistoryStatus::Verified, Vec::new());
    let journal_size = size(relabelled.projection());
    assert_eq!(begin_reconciled(EPOCH, relabelled, frame, MonoMs::new(MONO), journal_size).err(), Some(BeginRefused::NotThisHistory));
    // A sidecar reported twice is refused.
    let twice = RecordedHistory::new(proposed().projection, HistoryStatus::Verified, Vec::from([repair(1), repair(1)]));
    assert_eq!(reconcile(&twice, &[], input(twice.projection())).err(), Some(ReconcileRefused::RepairConflict));
}

#[test]
fn one_supervision_per_run_is_the_adapters_lock_not_the_frames() {
    // Each frame opens at most once (it is consumed), but reconciling the same history twice gives two frames, and the
    // core cannot tell that a second supervision of the run exists: r5 5.3 makes that the adapter's lock. Neither
    // supervision carries anything from before.
    let first = reconciled(&verified(&running()), &[observe(Seen::PidAbsent)]);
    let second = reconciled(&verified(&running()), &[observe(Seen::PidAbsent)]);
    let one = opened(verified(&running()), first.frame);
    let two = opened(verified(&running()), second.frame);
    assert_eq!(one.projection(), two.projection());
}

#[test]
fn a_reconciled_run_carries_no_historical_grant_and_needs_fresh_consent() {
    // Attempt 1 had a granted, unused start approval when the process ended.
    let history = verified(&approved());
    let reconciliation = reconciled(&history, &[]);
    let mut live = opened(history, reconciliation.frame);
    assert_eq!(live.projection().approval(a(1)).unwrap().state(), ApprovalState::Voided);
    let size = |live: &LiveSupervision| JournalSize { events: live.projection().events().len() as u32, bytes: 0 };
    // The historical start cannot be consumed: there is no grant, and the approval is void.
    let refused = live.stage(&[begin(1, 1, AT)], None, MonoMs::new(MONO + 1), size(&live)).err().unwrap();
    assert_eq!(refused.refusal, LiveRefusal::GrantRequired);
    // Re-staging the historical decision is refused: this supervision never recorded that request, so it has no
    // decision window for it.
    let redecided = live.stage(&[decide(1, Decision::Granted, AT)], None, MonoMs::new(MONO + 2), size(&live)).err().unwrap();
    assert_eq!(redecided.refusal, LiveRefusal::WindowClosed);
    // A new attempt needs a new request and a new decision, staged and confirmed by this supervision.
    let staged = live.stage(&attempt_pair(2, 2, AT), None, MonoMs::new(MONO + 3), size(&live)).ok().unwrap();
    assert!(live.confirm(staged).ok().unwrap().is_empty());
    let staged = live.stage(&[decide(2, Decision::Granted, AT + 1_000)], None, MonoMs::new(MONO + 4), size(&live)).ok().unwrap();
    let grants = live.confirm(staged).ok().unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!((grants[0].approval(), grants[0].attempt()), (a(2), n(2)));
}

#[test]
fn a_blocking_orphan_stays_until_the_user_resolves_it() {
    let history = verified(&running());
    let reconciliation = reconciled(&history, &[observe(Seen::IdentityMatches)]);
    let mut live = opened(history, reconciliation.frame);
    let size = |live: &LiveSupervision| JournalSize { events: live.projection().events().len() as u32, bytes: 0 };
    let orphan = orphan(live.projection(), 1);
    // A later observation never clears it: reconciling the opened run again drafts nothing and keeps it blocking.
    let again = RecordedHistory::new(live.projection().clone(), HistoryStatus::Verified, Vec::new());
    let second = reconciled(&again, &[observe(Seen::PidAbsent)]);
    assert!(second.frame.drafts().is_empty());
    assert_eq!(second.blocking, Vec::from([orphan]));
    // It blocks a new attempt; the informational workspace concern does not.
    let refused = live.stage(&attempt_pair(2, 2, AT), None, MonoMs::new(MONO + 1), size(&live)).err().unwrap();
    assert_eq!(refused.refusal, LiveRefusal::Rejected(Rejection::BlockingConcern));
    // Only a user statement resolves it, with a resolution its kind allows, naming it exactly.
    assert_eq!(ConcernResolution::new(orphan, Resolution::Acknowledged, ResolutionProvenance { actor: Actor::User, channel: Channel::NativeConfirmation, presented_sha256: hash('f') }).err(), Some(ModelError::ResolutionNotAllowed));
    let elsewhere = ConcernRef::new(ConcernKind::PossibleOrphan, Some(n(1)), Seq::new(2).unwrap()).unwrap();
    let wrong = live.stage(&[resolve(elsewhere, Resolution::ConfirmedGone, AT)], None, MonoMs::new(MONO + 2), size(&live)).err().unwrap();
    assert_eq!(wrong.refusal, LiveRefusal::Rejected(Rejection::InvalidTransition));
    let forged = EventDraft::forged(Producer::Reconciler, AT, resolve(orphan, Resolution::ConfirmedGone, AT).body().clone());
    assert_eq!(apply(live.projection(), &[forged.clone()], size(&live)).err(), Some(Rejection::ProducerNotAllowed));
    let reconciler = live.stage(&[forged], None, MonoMs::new(MONO + 3), size(&live)).err().unwrap();
    assert_eq!(reconciler.refusal, LiveRefusal::Reconciliation);
    let staged = live.stage(&[resolve(orphan, Resolution::StoppedOutsideOrdelis, AT)], None, MonoMs::new(MONO + 4), size(&live)).ok().unwrap();
    assert!(live.confirm(staged).ok().unwrap().is_empty());
    let resolved = live.projection().events().last().unwrap();
    assert_eq!(resolved.producer, Producer::User);
    let EventBody::AttentionResolved(resolution) = &resolved.body else { panic!("not a resolution") };
    assert_eq!(resolution.provenance(), &ResolutionProvenance { actor: Actor::User, channel: Channel::NativeConfirmation, presented_sha256: hash('f') });
    assert!(!live.projection().has_unresolved_blocking_concern());
    assert!(live.projection().concerns().iter().any(|c| c.concern().kind() == ConcernKind::WorkspaceUnknown && c.resolution().is_none()));
    let staged = live.stage(&attempt_pair(2, 2, AT), None, MonoMs::new(MONO + 5), size(&live)).ok().unwrap();
    assert!(live.confirm(staged).ok().unwrap().is_empty());
    assert_eq!(live.projection().attempt(n(2)).unwrap().state(), AttemptState::Proposed);
}

#[test]
fn reconciliation_drafts_no_stop_and_recommendations_are_inert_data() {
    // Whatever is observed, the frame holds only repair records and one reconciler end; no stop, cancel or signal.
    for seen in [Seen::GoneConfirmed, Seen::PidAbsent, Seen::PidReused, Seen::IdentityMatches, Seen::Unobservable] {
        let history = RecordedHistory::new(running().projection, HistoryStatus::Verified, Vec::from([repair(1)]));
        let reconciliation = reconciled(&history, &[observe(seen)]);
        for draft in reconciliation.frame.drafts() {
            assert!(matches!(draft.body().kind(), EventKind::JournalRepaired | EventKind::AttemptInterrupted | EventKind::AttemptNotStarted), "{seen:?}");
        }
    }
    // A proposal is a value: copying and comparing it is all that can be done with it.
    let proposal = Recommendation::TerminationProposal { attempt: n(1), basis: ProposalBasis::PidStartTokenAndHash, policy: PolicyRequirement::SeparatePolicy };
    let copy = proposal;
    assert_eq!(copy, proposal);
}

// ---------------------------------------------------------------------------------------------------------------
// Cross-run start conflicts

fn rid(number: u64) -> RunId {
    RunId::new(number).unwrap()
}

/// A created run with no attempt, under another id, for another project registration.
fn other_run(number: u64, registered_at: u64) -> RecordedHistory {
    let limits = Limits::new(60_000, 3_600_000, 120_000, 300_000, 5_000).unwrap();
    let task = TaskRef::new(TaskId::parse("t1").unwrap(), WallMs::new(T0).unwrap(), 1).unwrap();
    let project = ProjectRef { project: ProjectId::parse("p1").unwrap(), registered_at_ms: WallMs::new(registered_at).unwrap() };
    let context = RunContext::new(project, WorkspaceMode::None, Vec::new(), limits, token("policy-1"), None).unwrap();
    let provider = ProviderIdentity { provider: ProviderId::Codex, executable_sha256: hash('a'), advertised_version: None };
    let spec = RunSpec::new(task, provider, context, 8).unwrap();
    let draft = EventDraft::user(UserDraft::RunCreated { spec, spec_digest: hash('b') }, registered_at);
    let applied = apply(&RunProjection::empty(rid(number)), &[draft], JournalSize { events: 0, bytes: 0 }).unwrap();
    RecordedHistory::new(applied.projection, HistoryStatus::Verified, Vec::new())
}

/// Run r7 after reconciliation: attempt 1 interrupted, its orphan unresolved.
fn orphaned() -> RecordedHistory {
    let history = verified(&running());
    let reconciliation = reconciled(&history, &[observe(Seen::PidAbsent)]);
    RecordedHistory::new(after(&history, &reconciliation), HistoryStatus::Verified, Vec::new())
}

/// A supervision opened for a new, empty run under id `number`: it owns nothing that was recorded before.
fn supervising(number: u64) -> LiveSupervision {
    open(begin_new(EPOCH, rid(number), created(T0), MonoMs::new(0)).unwrap())
}

/// The supervision that opened this quiescent history: it owns exactly what was recorded.
fn holding(history: RecordedHistory) -> LiveSupervision {
    let frame = reconciled(&history, &[]).frame;
    opened(history, frame)
}

fn enumerated(allocated: u64, summaries: Vec<(u64, RunSummary)>) -> RunInventory {
    RunInventory::Enumerated { allocated, summaries: summaries.into_iter().map(|(number, summary)| (rid(number), summary)).collect() }
}

fn locked(history: &RecordedHistory) -> RunSummary {
    summarize(history, ReadBasis::Locked)
}

#[test]
fn an_orphan_anywhere_blocks_every_start_whatever_the_run_id_or_project() {
    // r7 holds an unresolved orphan. r1 (same project, registered again later) and r2 (the run about to start, held by
    // this caller) are clean. The conflict is global: a new run id or a re-registered project does not escape it.
    let mut summaries = Vec::from([(1, locked(&other_run(1, T0 + 9_000))), (2, locked(&other_run(2, T0)))]);
    summaries.extend((3..=6).map(|number| (number, RunSummary::NeverCreated)));
    summaries.push((7, locked(&orphaned())));
    let conflicts = start_conflicts(&enumerated(7, summaries.clone()), &[holding(other_run(2, T0)).held()]);
    assert_eq!(conflicts, Vec::from([StartConflict::PossibleOrphan(rid(7), n(1))]));
    // Once the user resolves it, nothing in the supplied data conflicts (which is still not permission to start).
    let mut journal = running();
    journal.ok(reconciled(&verified(&running()), &[observe(Seen::PidAbsent)]).frame.drafts().to_vec());
    let concern = orphan(&journal.projection, 1);
    journal.one(resolve(concern, Resolution::ConfirmedGone, AT));
    summaries[6] = (7, locked(&verified(&journal)));
    assert_eq!(start_conflicts(&enumerated(7, summaries), &[holding(other_run(2, T0)).held()]), Vec::new());
}

#[test]
fn a_supervision_ahead_of_its_summary_still_reports_its_orphan() {
    // r7 summarized from disk with attempt 1 in flight; then reconciled and opened. The supervision owns the summarized
    // history, so the attempt is excused, but the orphan its opening frame raised exists only in the supervision.
    let history = verified(&running());
    let summary = locked(&history);
    let frame = reconciled(&history, &[observe(Seen::PidAbsent)]).frame;
    let live = opened(history, frame);
    let inventory = enumerated(7, (1..=6).map(|number| (number, RunSummary::NeverCreated)).chain([(7, summary)]).collect());
    assert_eq!(start_conflicts(&inventory, &[live.held()]), Vec::from([StartConflict::PossibleOrphan(rid(7), n(1))]));
}

#[test]
fn quarantined_unverified_and_unreconciled_runs_block() {
    let clean = || (1, locked(&other_run(1, T0)));
    let cases = [
        (RunSummary::Quarantined(QuarantineReason::Fault(JournalFault::NewerVersion)), StartConflict::Quarantined(rid(2), QuarantineReason::Fault(JournalFault::NewerVersion))),
        (RunSummary::Quarantined(QuarantineReason::Marker), StartConflict::Quarantined(rid(2), QuarantineReason::Marker)),
        (RunSummary::Unverified(Unverified::Missing), StartConflict::Unverified(rid(2), Unverified::Missing)),
        (RunSummary::Unverified(Unverified::Unreadable), StartConflict::Unverified(rid(2), Unverified::Unreadable)),
        (RunSummary::Unverified(Unverified::HeldElsewhere), StartConflict::Unverified(rid(2), Unverified::HeldElsewhere)),
        (RunSummary::Unverified(Unverified::MarkMismatch), StartConflict::Unverified(rid(2), Unverified::MarkMismatch)),
        (RunSummary::Unverified(Unverified::NoRunCreated), StartConflict::Unverified(rid(2), Unverified::NoRunCreated)),
        (summarize(&other_run(2, T0), ReadBasis::LockFree), StartConflict::UnverifiedRead(rid(2))),
        (summarize(&other_run(2, T0), ReadBasis::DerivedIndex), StartConflict::UnverifiedRead(rid(2))),
    ];
    for (summary, conflict) in cases {
        assert_eq!(start_conflicts(&enumerated(2, Vec::from([clean(), (2, summary)])), &[holding(other_run(1, T0)).held()]), Vec::from([conflict]));
    }
    // A run with an attempt in flight that nobody here supervises was never reconciled: it blocks. Held by the
    // supervision that recorded it, it does not (that supervisor owns the attempt).
    let mut owner = supervising(7);
    let journal_size = JournalSize { events: owner.projection().events().len() as u32, bytes: 0 };
    let staged = owner.stage(&attempt_pair(1, 1, T0 + 1_000), None, MonoMs::new(1), journal_size).ok().unwrap();
    assert!(owner.confirm(staged).ok().unwrap().is_empty());
    let in_flight = RecordedHistory::new(owner.projection().clone(), HistoryStatus::Verified, Vec::new());
    let inventory = enumerated(7, (1..=6).map(|number| (number, RunSummary::NeverCreated)).chain([(7, locked(&in_flight))]).collect());
    assert_eq!(start_conflicts(&inventory, &[]), Vec::from([StartConflict::Unreconciled(rid(7), n(1))]));
    assert_eq!(start_conflicts(&inventory, &[owner.held()]), Vec::new());
    // A supervision opened under the same id that did not record it excuses nothing, nor do two claims for one run.
    let impostor = supervising(7);
    assert_eq!(start_conflicts(&inventory, &[impostor.held()]), Vec::from([StartConflict::Contradictory(rid(7)), StartConflict::Unreconciled(rid(7), n(1))]));
    assert_eq!(start_conflicts(&inventory, &[owner.held(), impostor.held()]), Vec::from([StartConflict::Contradictory(rid(7)), StartConflict::Unreconciled(rid(7), n(1))]));
    let running_history = verified(&running());
    let recorded = enumerated(7, (1..=6).map(|number| (number, RunSummary::NeverCreated)).chain([(7, locked(&running_history))]).collect());
    assert_eq!(start_conflicts(&recorded, &[supervising(7).held()]), Vec::from([StartConflict::Contradictory(rid(7)), StartConflict::Unreconciled(rid(7), n(1))]));
    // A repair the journal still lacks blocks until it is recorded; so does a recorded sidecar reported twice.
    let pending = RecordedHistory::new(other_run(1, T0).projection().clone(), HistoryStatus::Verified, Vec::from([repair(1)]));
    assert_eq!(start_conflicts(&enumerated(1, Vec::from([(1, locked(&pending))])), &[]), Vec::from([StartConflict::RepairPending(rid(1))]));
    let repaired_run = apply(other_run(1, T0).projection(), &[repaired(1, T0 + 1_000)], JournalSize { events: 1, bytes: 0 }).unwrap().projection;
    let recorded_once = RecordedHistory::new(repaired_run.clone(), HistoryStatus::Verified, Vec::from([repair(1)]));
    assert_eq!(start_conflicts(&enumerated(1, Vec::from([(1, locked(&recorded_once))])), &[]), Vec::new());
    let reported_twice = RecordedHistory::new(repaired_run, HistoryStatus::Verified, Vec::from([repair(1), repair(1)]));
    assert_eq!(start_conflicts(&enumerated(1, Vec::from([(1, locked(&reported_twice))])), &[]), Vec::from([StartConflict::RepairPending(rid(1))]));
    // A lock-free or index read can still add conflicts.
    let indexed = summarize(&orphaned(), ReadBasis::DerivedIndex);
    let inventory = enumerated(7, (1..=6).map(|number| (number, RunSummary::NeverCreated)).chain([(7, indexed)]).collect());
    assert_eq!(start_conflicts(&inventory, &[]), Vec::from([StartConflict::UnverifiedRead(rid(7)), StartConflict::PossibleOrphan(rid(7), n(1))]));
}

#[test]
fn an_unknown_or_incomplete_inventory_is_never_an_empty_safe_set() {
    assert_eq!(start_conflicts(&RunInventory::Unknown, &[]), Vec::from([StartConflict::InventoryUnknown]));
    assert_eq!(start_conflicts(&enumerated(0, Vec::new()), &[]), Vec::new());
    assert_eq!(start_conflicts(&RunInventory::Enumerated { allocated: 10_000_000_000, summaries: Vec::new() }, &[]), Vec::from([StartConflict::InventoryUnknown]));
    // The counter allocated r1 to r6; only r2 and r5 are accounted for.
    let inventory = enumerated(6, Vec::from([(2, RunSummary::NeverCreated), (5, RunSummary::NeverCreated)]));
    assert_eq!(
        start_conflicts(&inventory, &[]),
        Vec::from([
            StartConflict::Unaccounted { from: rid(1), through: rid(1) },
            StartConflict::Unaccounted { from: rid(3), through: rid(4) },
            StartConflict::Unaccounted { from: rid(6), through: rid(6) },
        ])
    );
    // Nothing at all for an allocated range: a directory listing that lost the orphan's journal cannot clear it.
    assert_eq!(start_conflicts(&enumerated(7, Vec::new()), &[]), Vec::from([StartConflict::Unaccounted { from: rid(1), through: rid(7) }]));
}

#[test]
fn contradictory_inventories_block() {
    let clean = locked(&other_run(1, T0));
    // A second summary for an id, an id the counter never allocated, a summary of another run under this id.
    let duplicate = enumerated(1, Vec::from([(1, clean.clone()), (1, RunSummary::NeverCreated)]));
    assert_eq!(start_conflicts(&duplicate, &[]), Vec::from([StartConflict::Contradictory(rid(1))]));
    let beyond = enumerated(1, Vec::from([(1, clean.clone()), (2, RunSummary::NeverCreated)]));
    assert_eq!(start_conflicts(&beyond, &[]), Vec::from([StartConflict::Contradictory(rid(2))]));
    let renamed = enumerated(2, Vec::from([(1, RunSummary::NeverCreated), (2, clean.clone())]));
    assert_eq!(start_conflicts(&renamed, &[]), Vec::from([StartConflict::Contradictory(rid(2))]));
    // A supervised run that is not in the inventory, or that is said never to have been created.
    assert_eq!(start_conflicts(&enumerated(1, Vec::from([(1, clean)])), &[supervising(3).held()]), Vec::from([StartConflict::Contradictory(rid(3))]));
    assert_eq!(start_conflicts(&enumerated(1, Vec::from([(1, RunSummary::NeverCreated)])), &[supervising(1).held()]), Vec::from([StartConflict::Contradictory(rid(1))]));
}

#[test]
fn start_conflicts_depend_on_no_order_across_journals() {
    let summaries = Vec::from([
        (1, locked(&other_run(1, T0 + 50_000))),
        (2, RunSummary::Quarantined(QuarantineReason::Marker)),
        (3, RunSummary::Unverified(Unverified::HeldElsewhere)),
        (4, locked(&other_run(4, T0))),
        (5, RunSummary::NeverCreated),
        (6, RunSummary::NeverCreated),
        (7, locked(&orphaned())),
    ]);
    let forward = start_conflicts(&enumerated(7, summaries.clone()), &[holding(other_run(4, T0)).held()]);
    let mut reversed = summaries.clone();
    reversed.reverse();
    assert_eq!(start_conflicts(&enumerated(7, reversed), &[holding(other_run(4, T0)).held()]), forward);
    let mut rotated = summaries;
    rotated.rotate_left(3);
    assert_eq!(start_conflicts(&enumerated(7, rotated), &[holding(other_run(4, T0)).held()]), forward);
    // The other runs' recorded times differ wildly (T0 + 50 s, T0), and nothing compares them.
    let mut sorted = forward.clone();
    sorted.sort();
    assert_eq!(forward, sorted);
    assert_eq!(
        forward,
        Vec::from([
            StartConflict::Quarantined(rid(2), QuarantineReason::Marker),
            StartConflict::Unverified(rid(3), Unverified::HeldElsewhere),
            StartConflict::PossibleOrphan(rid(7), n(1)),
        ])
    );
}

#[test]
fn informational_concerns_never_block_a_start() {
    // A failed run whose workspace concern is unresolved: informational, so it blocks nothing.
    let failed = verified(&with(running(), exited(1, 3, T0 + 5_000)));
    assert!(failed.projection().concerns().iter().any(|c| c.concern().kind() == ConcernKind::WorkspaceUnknown && c.resolution().is_none()));
    let inventory = enumerated(7, (1..=6).map(|number| (number, RunSummary::NeverCreated)).chain([(7, locked(&failed))]).collect());
    assert_eq!(start_conflicts(&inventory, &[]), Vec::new());
}

// ---------------------------------------------------------------------------------------------------------------
// D3.2c-1 (#120): a capacity-faulted supervision excuses nothing

const EXHAUSTED: JournalSize = JournalSize { events: MAX_EVENTS + SAFETY_EVENTS, bytes: 0 };

/// Stages and confirms one frame of `live`, keeping its events in `frames`, and returns the grants it minted.
fn committed(live: &mut LiveSupervision, frames: &mut Vec<Vec<Event>>, drafts: Vec<EventDraft>, grant: Option<LiveGrant>, mono: u64) -> Vec<LiveGrant> {
    let size = size(live.projection());
    let staged = match live.stage(&drafts, grant, MonoMs::new(mono), size) {
        Ok(staged) => staged,
        Err(refused) => panic!("{:?}", refused.refusal),
    };
    frames.push(staged.events().to_vec());
    match live.confirm(staged) {
        Ok(grants) => grants,
        Err(refusal) => panic!("{refusal:?}"),
    }
}

/// r7 supervised from its creation, attempt 1 running, with `requests` operation requests of which the first `granted`
/// are granted; returns the supervision, its recorded frames and the outstanding operation grants.
fn supervised_running(requests: u64, granted: u64) -> (LiveSupervision, Vec<Vec<Event>>, Vec<LiveGrant>) {
    let pending = begin_new(EPOCH, run_id(), created(T0), MonoMs::new(0)).unwrap();
    let mut frames = Vec::from([pending.events().to_vec()]);
    let mut live = open(pending);
    committed(&mut live, &mut frames, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let mut start = committed(&mut live, &mut frames, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000);
    committed(&mut live, &mut frames, Vec::from([begin(1, 1, T0 + 3_000)]), start.pop(), 3_000);
    committed(&mut live, &mut frames, Vec::from([started(1, T0 + 4_000)]), None, 4_000);
    let mut grants = Vec::new();
    for k in 2..2 + requests {
        let mono = 5_000 + 100 * k;
        committed(&mut live, &mut frames, Vec::from([operation(k, 1, T0 + mono)]), None, mono);
        if k < 2 + granted {
            grants.extend(committed(&mut live, &mut frames, Vec::from([decide(k, Decision::Granted, T0 + mono + 50)]), None, mono + 50));
        }
    }
    (live, frames, grants)
}

/// The summary of r7 as recorded in `frames`, with clean runs around it: r1 (same project, registered again later),
/// r2 to r6 never created, and r8 (a new run id).
fn inventory_with(frames: &[Vec<Event>]) -> RunInventory {
    let projection = frames.iter().fold(RunProjection::empty(run_id()), |p, frame| apply_recorded(&p, frame).unwrap());
    let r7 = locked(&RecordedHistory::new(projection, HistoryStatus::Verified, Vec::new()));
    let mut summaries = Vec::from([(1, locked(&other_run(1, T0 + 9_000)))]);
    summaries.extend((2..=6).map(|number| (number, RunSummary::NeverCreated)));
    summaries.push((7, r7));
    summaries.push((8, locked(&other_run(8, T0))));
    enumerated(8, summaries)
}

#[test]
fn a_supervision_at_the_ceiling_blocks_every_start_while_it_holds_its_run() {
    let (mut live, frames, _) = supervised_running(0, 0);
    // Held and healthy: the running attempt is excused, as for any supervised attempt.
    assert_eq!(start_conflicts(&inventory_with(&frames), &[live.held()]), Vec::new());
    // A stop stated at the ceiling is refused; the attempt stays running and nothing is recorded or settled.
    let refused = live.stage(&[stop(1, StopReason::JournalFull, AT)], None, MonoMs::new(7_000), EXHAUSTED).err().map(|r| r.refusal);
    assert_eq!(refused, Some(LiveRefusal::Rejected(Rejection::LimitExceeded)));
    assert!(live.in_capacity_fault());
    assert_eq!(live.projection().attempt(n(1)).unwrap().state(), AttemptState::Running);
    // The claim now excuses nothing: the run blocks every start, whatever the run id or the project registration.
    let conflicts = start_conflicts(&inventory_with(&frames), &[live.held()]);
    assert_eq!(conflicts, Vec::from([StartConflict::Unreconciled(run_id(), n(1)), StartConflict::CapacityExhausted(run_id())]));
    // A later, smaller stated size never clears the fault. A stop that fits is still recorded (evidence is kept), and
    // the run still blocks while held.
    let mut frames = frames;
    committed(&mut live, &mut frames, Vec::from([stop(1, StopReason::JournalFull, AT)]), None, 8_000);
    assert!(live.in_capacity_fault());
    assert_eq!(live.projection().attempt(n(1)).unwrap().state(), AttemptState::Stopping(StopCause::JournalFull));
    assert!(start_conflicts(&inventory_with(&frames), &[live.held()]).contains(&StartConflict::CapacityExhausted(run_id())));
    // Only safety drafts are staged while faulted.
    let progress = live.stage(&[progress(1, AT)], None, MonoMs::new(9_000), size(live.projection())).err().map(|r| r.refusal);
    assert_eq!(progress, Some(LiveRefusal::CapacityFault));
    // The fault changed no history: the recorded frames replay to the live projection.
    assert_eq!(frames.iter().fold(RunProjection::empty(run_id()), |p, f| apply_recorded(&p, f).unwrap()), *live.projection());
    // Dropping the supervision does not make the run look settled: its summary blocks on its own.
    drop(live);
    assert_eq!(start_conflicts(&inventory_with(&frames), &[]), Vec::from([StartConflict::Unreconciled(run_id(), n(1))]));
}

#[test]
fn a_refused_protected_stop_or_exit_faults_the_supervision() {
    // Four open operation requests, two granted: a stop or an exit generates four consequences. One event short of
    // room for them, at a stated size below the ceiling, the protected event is refused for capacity.
    let short = JournalSize { events: MAX_EVENTS + SAFETY_EVENTS - u32::from(MAX_OPEN_APPROVALS), bytes: 0 };
    assert!(!short.exhausted());
    for draft in [stop(1, StopReason::JournalFull, AT), exited(1, 0, AT), cancel(CancelScope::Run, AT)] {
        let (mut live, frames, _) = supervised_running(u64::from(MAX_OPEN_APPROVALS), 2);
        assert!(!live.in_capacity_fault());
        let refused = live.stage(&[draft], None, MonoMs::new(7_000), short).err().map(|r| r.refusal);
        assert_eq!(refused, Some(LiveRefusal::Rejected(Rejection::LimitExceeded)));
        assert!(live.in_capacity_fault());
        assert_eq!(live.projection().attempt(n(1)).unwrap().state(), AttemptState::Running);
        assert!(start_conflicts(&inventory_with(&frames), &[live.held()]).contains(&StartConflict::CapacityExhausted(run_id())));
    }
    // A refusal that is not about capacity faults nothing: a stop for an attempt that does not exist.
    let (mut live, _, _) = supervised_running(0, 0);
    assert!(live.stage(&[stop(2, StopReason::JournalFull, AT)], None, MonoMs::new(7_000), size(live.projection())).is_err());
    assert!(!live.in_capacity_fault());
}

#[test]
fn a_faulted_supervision_mints_and_accepts_no_grant() {
    let (mut live, mut frames, mut grants) = supervised_running(2, 1);
    let outstanding = grants.pop().unwrap();
    // A decision is staged, then (with that frame still in flight) the stated size reaches the ceiling.
    let decision = live.stage(&[decide(3, Decision::Granted, T0 + 6_500)], None, MonoMs::new(6_500), size(live.projection())).map_err(|r| r.refusal).unwrap();
    frames.push(decision.events().to_vec());
    assert!(live.stage(&[progress(1, AT)], None, MonoMs::new(6_600), EXHAUSTED).is_err());
    assert!(live.in_capacity_fault());
    // The decision's frame is durable, but no grant is minted for it.
    assert!(live.confirm(decision).unwrap().is_empty());
    assert_eq!(live.projection().approval(a(3)).unwrap().state(), ApprovalState::Granted);
    // A grant minted before the fault cannot be used; it comes back.
    let refused = live.stage(&[consumed(2, AT)], Some(outstanding), MonoMs::new(7_000), size(live.projection())).err().unwrap();
    assert_eq!(refused.refusal, LiveRefusal::CapacityFault);
    assert!(refused.grant.is_some());
    // No new decision can be staged either.
    let (mut other, _, _) = supervised_running(1, 0);
    assert!(other.stage(&[progress(1, AT)], None, MonoMs::new(7_000), EXHAUSTED).is_err());
    let refused = other.stage(&[decide(2, Decision::Granted, T0 + 6_000)], None, MonoMs::new(7_100), size(other.projection())).err().map(|r| r.refusal);
    assert_eq!(refused, Some(LiveRefusal::CapacityFault));
}

#[test]
fn due_at_the_ceiling_faults_and_proposes_a_stop() {
    let (mut live, frames, _) = supervised_running(0, 0);
    let due = live.due(MonoMs::new(7_000), AT, EXHAUSTED);
    assert!(live.in_capacity_fault());
    assert_eq!(due.drafts.len(), 1);
    assert_eq!(due.drafts[0].body(), &EventBody::StopRequested { attempt: n(1), reason: StopReason::JournalFull });
    // Even with honest sizes afterwards, it keeps proposing the stop and the run keeps blocking while held.
    let again = live.due(MonoMs::new(8_000), AT, size(live.projection()));
    assert_eq!(again.drafts[0].body(), &EventBody::StopRequested { attempt: n(1), reason: StopReason::JournalFull });
    assert!(start_conflicts(&inventory_with(&frames), &[live.held()]).contains(&StartConflict::CapacityExhausted(run_id())));
}

#[test]
fn a_run_opened_at_the_ceiling_opens_faulted() {
    // A quiescent history (nothing to reconcile) opened with a stated size at the ceiling.
    let history = other_run(7, T0);
    let frame = reconciled(&history, &[]).frame;
    let Ok(Opening::Ready(live)) = begin_reconciled(EPOCH, history, frame, MonoMs::new(MONO), EXHAUSTED) else { panic!("not ready") };
    assert!(live.in_capacity_fault());
    let mut summaries: Vec<(u64, RunSummary)> = (1..=6).map(|number| (number, RunSummary::NeverCreated)).collect();
    summaries.push((7, locked(&other_run(7, T0))));
    assert_eq!(start_conflicts(&enumerated(7, summaries), &[live.held()]), Vec::from([StartConflict::CapacityExhausted(run_id())]));
}

#[test]
fn a_frame_that_reaches_the_ceiling_faults_the_supervision_at_once() {
    // Review of #120: a stop accepted one event short of the ceiling leaves no room for the exit. The supervision faults
    // when the frame is staged, not at the next call, so its claim never excuses an attempt it can no longer end.
    let (mut live, mut frames, _) = supervised_running(0, 0);
    let edge = JournalSize { events: MAX_EVENTS + SAFETY_EVENTS - 1, bytes: 0 };
    let staged = live.stage(&[stop(1, StopReason::JournalFull, AT)], None, MonoMs::new(7_000), edge).map_err(|r| r.refusal).unwrap();
    assert!(live.in_capacity_fault());
    frames.push(staged.events().to_vec());
    live.confirm(staged).unwrap();
    let conflicts = start_conflicts(&inventory_with(&frames), &[live.held()]);
    assert_eq!(conflicts, Vec::from([StartConflict::Unreconciled(run_id(), n(1)), StartConflict::CapacityExhausted(run_id())]));
    // The same by bytes: a frame that may reach the byte ceiling.
    let (mut live, _, _) = supervised_running(0, 0);
    let near = JournalSize { events: 10, bytes: MAX_JOURNAL_BYTES + SAFETY_JOURNAL_BYTES - 100 };
    assert!(live.stage(&[stop(1, StopReason::JournalFull, AT)], None, MonoMs::new(7_000), near).is_ok());
    assert!(live.in_capacity_fault());
    // Far from either ceiling nothing faults.
    let (mut live, _, _) = supervised_running(0, 0);
    assert!(live.stage(&[stop(1, StopReason::JournalFull, AT)], None, MonoMs::new(7_000), size(live.projection())).is_ok());
    assert!(!live.in_capacity_fault());
}

#[test]
fn an_oversized_batch_is_a_caller_error_not_a_capacity_fault() {
    let (mut live, _, _) = supervised_running(0, 0);
    let stops: Vec<EventDraft> = (0..=MAX_BATCH).map(|_| stop(1, StopReason::JournalFull, AT)).collect();
    let refused = live.stage(&stops, None, MonoMs::new(7_000), size(live.projection())).err().map(|r| r.refusal);
    assert_eq!(refused, Some(LiveRefusal::Rejected(Rejection::LimitExceeded)));
    assert!(!live.in_capacity_fault());
}
