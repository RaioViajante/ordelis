// Live layer tests. Pure: monotonic and wall-clock times are synthetic values; nothing reads a clock, a file or a
// process. Includes the seeded property test of D3.2b decision 12.14.
use super::*;
use crate::lifecycle::model::*;
use crate::lifecycle::reduce::tests::*;
use crate::lifecycle::codec::{crc32c, encode_frame, PersistCheck, MAX_STREAM_BYTES};
use crate::lifecycle::reduce::{apply_recorded, Concern, RunProjection};

const EPOCH: LiveEpoch = LiveEpoch::new(1);

fn m(ms: u64) -> MonoMs {
    MonoMs::new(ms)
}

fn size_of(live: &LiveSupervision) -> JournalSize {
    JournalSize { events: live.projection().events().len() as u32, bytes: 0 }
}

fn live_with(epoch: LiveEpoch, run_created: EventDraft) -> LiveSupervision {
    match begin_new(epoch, run_id(), run_created, m(0)) {
        Ok(pending) => open(pending),
        Err(rejection) => panic!("{rejection:?}"),
    }
}

fn live() -> LiveSupervision {
    live_with(EPOCH, created(T0))
}

fn try_stage(live: &mut LiveSupervision, drafts: Vec<EventDraft>, grant: Option<LiveGrant>, mono: u64) -> Result<Staged, StageRejected> {
    let size = size_of(live);
    live.stage(&drafts, grant, m(mono), size)
}

fn staged(live: &mut LiveSupervision, drafts: Vec<EventDraft>, grant: Option<LiveGrant>, mono: u64) -> Staged {
    match try_stage(live, drafts, grant, mono) {
        Ok(staged) => staged,
        Err(refused) => panic!("refused: {:?}", refused.refusal),
    }
}

/// Stages and confirms one frame, returning the grants it minted.
fn commit(live: &mut LiveSupervision, drafts: Vec<EventDraft>, grant: Option<LiveGrant>, mono: u64) -> Vec<LiveGrant> {
    let staged = staged(live, drafts, grant, mono);
    match live.confirm(staged) {
        Ok(grants) => grants,
        Err(refusal) => panic!("{refusal:?}"),
    }
}

fn refusal(live: &mut LiveSupervision, drafts: Vec<EventDraft>, grant: Option<LiveGrant>, mono: u64) -> (LiveRefusal, Option<LiveGrant>) {
    match try_stage(live, drafts, grant, mono) {
        Ok(_) => panic!("staged"),
        Err(refused) => (refused.refusal, refused.grant),
    }
}

/// The supervision's journal as one frame per recorded event: `verify_write` compares events, not framing.
fn committed(live: &LiveSupervision) -> Vec<u8> {
    live.projection().events().iter().flat_map(|event| encode_frame(std::slice::from_ref(event)).unwrap()).collect()
}

/// Verifies `abandoned` against a file the closure builds from the committed bytes and the attempted frame.
fn verified_write(live: &mut LiveSupervision, abandoned: &Abandoned, file: impl Fn(&[u8], &[u8]) -> Vec<u8>) -> WriteOutcome {
    let committed = committed(live);
    let file = file(&committed, &abandoned.bytes);
    live.verify_write(abandoned, &committed, &file).unwrap()
}

/// The frame landed: the file is the committed bytes and exactly the frame.
fn landed(live: &mut LiveSupervision, abandoned: &Abandoned) -> WriteOutcome {
    let outcome = verified_write(live, abandoned, |committed, frame| [committed, frame].concat());
    assert_eq!(outcome.check(), PersistCheck::Landed);
    outcome
}

/// The frame is absent: the file is exactly the committed bytes.
fn absent(live: &mut LiveSupervision, abandoned: &Abandoned) -> WriteOutcome {
    let outcome = verified_write(live, abandoned, |committed, _| committed.to_vec());
    assert_eq!(outcome.check(), PersistCheck::Absent { torn: None });
    outcome
}

/// The frame was torn after `cut` bytes: the file is the committed bytes and that prefix.
fn torn(live: &mut LiveSupervision, abandoned: &Abandoned, cut: usize) -> WriteOutcome {
    let outcome = verified_write(live, abandoned, |committed, frame| [committed, &frame[..cut]].concat());
    assert!(matches!(outcome.check(), PersistCheck::Absent { torn: Some(_) }));
    outcome
}

fn single(mut grants: Vec<LiveGrant>) -> LiveGrant {
    assert_eq!(grants.len(), 1);
    grants.remove(0)
}

fn attempt_state(live: &LiveSupervision, number: u8) -> AttemptState {
    live.projection().attempt(n(number)).unwrap().state()
}

fn approval_state(live: &LiveSupervision, k: u64) -> ApprovalState {
    live.projection().approval(a(k)).unwrap().state()
}

/// A supervision whose attempt 1 has a granted start approval, and the grant (requested at mono 1 s, decided at 2 s).
fn approved_live() -> (LiveSupervision, LiveGrant) {
    let mut live = live();
    assert!(commit(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 1_000).is_empty());
    let grant = single(commit(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000));
    (live, grant)
}

/// A supervision whose attempt 1 runs (started at mono 4 s).
fn running_live() -> LiveSupervision {
    let (mut live, grant) = approved_live();
    assert!(commit(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), Some(grant), 3_000).is_empty());
    commit(&mut live, Vec::from([started(1, T0 + 4_000)]), None, 4_000);
    live
}

#[test]
fn a_supervision_begins_only_from_a_run_creation_and_opens_after_its_frame() {
    assert_eq!(begin_new(EPOCH, run_id(), cancel(CancelScope::Run, T0), m(0)).err(), Some(Rejection::InvalidTransition));
    assert_eq!(begin_new(EPOCH, run_id(), created(MAX_WALL_MS + 1), m(0)).err(), Some(Rejection::InvalidTime));
    let pending = match begin_new(EPOCH, run_id(), created(T0), m(0)) {
        Ok(pending) => pending,
        Err(rejection) => panic!("{rejection:?}"),
    };
    assert_eq!(pending.events().len(), 1);
    assert_eq!(pending.events()[0].body.kind(), EventKind::RunCreated);
    let live = open(pending);
    assert_eq!(live.projection().state(), Some(RunState::Open));
    let dropped = match begin_new(EPOCH, run_id(), created(T0), m(0)) {
        Ok(pending) => pending,
        Err(rejection) => panic!("{rejection:?}"),
    };
    drop_pending(dropped);
}

#[test]
fn a_grant_is_minted_once_for_a_fresh_decision_and_consumed_by_value() {
    let (mut live, grant) = approved_live();
    assert_eq!((grant.approval(), grant.attempt(), grant.kind()), (a(1), n(1), ApprovalKind::Start));
    assert_eq!(grant.use_by(), m(62_000));
    assert!(commit(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), Some(grant), 3_000).is_empty());
    assert_eq!(approval_state(&live, 1), ApprovalState::Consumed);
    assert_eq!(attempt_state(&live, 1), AttemptState::Starting);
    // The grant moved into the frame and is gone; a consumption is never retried, even as a duplicate.
    assert_eq!(refusal(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), None, 3_500).0, LiveRefusal::GrantRequired);
    // Re-delivering the decision mints nothing.
    let mut live2 = approved_live().0;
    assert!(commit(&mut live2, Vec::from([decide(1, Decision::Granted, T0 + 2_500)]), None, 2_500).is_empty());
}

#[test]
fn a_grant_works_only_for_its_own_consumption_and_comes_back_when_refused() {
    let mut live = running_live();
    commit(&mut live, Vec::from([operation(2, 1, T0 + 5_000)]), None, 5_000);
    commit(&mut live, Vec::from([operation(3, 1, T0 + 5_000)]), None, 5_000);
    let second = single(commit(&mut live, Vec::from([decide(2, Decision::Granted, T0 + 6_000)]), None, 6_000));
    let third = single(commit(&mut live, Vec::from([decide(3, Decision::Granted, T0 + 6_000)]), None, 6_000));
    // Another approval's grant.
    let (why, back) = refusal(&mut live, Vec::from([consumed(3, T0 + 7_000)]), Some(second), 7_000);
    assert_eq!(why, LiveRefusal::GrantMismatch);
    let second = back.unwrap();
    assert_eq!(second.approval(), a(2));
    // A grant with nothing to consume, and two consumptions for one grant.
    let (why, back) = refusal(&mut live, Vec::from([progress(1, T0 + 7_000)]), Some(second), 7_000);
    assert_eq!(why, LiveRefusal::GrantUnexpected);
    let second = back.unwrap();
    let (why, back) = refusal(&mut live, Vec::from([consumed(2, T0 + 7_000), consumed(3, T0 + 7_000)]), Some(second), 7_000);
    assert_eq!(why, LiveRefusal::GrantMismatch);
    let second = back.unwrap();
    // Without the grant, nothing is consumed.
    assert_eq!(refusal(&mut live, Vec::from([consumed(2, T0 + 7_000)]), None, 7_000).0, LiveRefusal::GrantRequired);
    commit(&mut live, Vec::from([consumed(2, T0 + 7_000)]), Some(second), 7_000);
    commit(&mut live, Vec::from([consumed(3, T0 + 7_000)]), Some(third), 7_000);
    assert_eq!((approval_state(&live, 2), approval_state(&live, 3)), (ApprovalState::Consumed, ApprovalState::Consumed));
}

#[test]
fn a_grant_from_another_supervision_is_refused() {
    let (mut mine, _) = approved_live();
    let mut theirs = live_with(LiveEpoch::new(2), created(T0));
    commit(&mut theirs, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let foreign = single(commit(&mut theirs, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000));
    let (why, back) = refusal(&mut mine, Vec::from([begin(1, 1, T0 + 3_000)]), Some(foreign), 3_000);
    assert_eq!(why, LiveRefusal::GrantMismatch);
    assert!(back.is_some());
}

#[test]
fn a_grant_expires_on_monotonic_time_and_is_then_voided() {
    let (mut live, grant) = approved_live();
    let (why, back) = refusal(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), Some(grant), 62_000);
    assert_eq!(why, LiveRefusal::WindowClosed);
    // The wall clock (stated three seconds after the decision) would still allow it: live time decides.
    let due = live.due(m(62_000), T0 + 5_000, size_of(&live));
    assert_eq!(due.clock, ClockCheck::Monotonic);
    assert_eq!(due.drafts.len(), 1);
    assert_eq!(*due.drafts[0].body(), EventBody::ApprovalVoided { approval: a(1), reason: VoidReason::UseByPassed });
    commit(&mut live, due.drafts, None, 62_000);
    assert_eq!(attempt_state(&live, 1), AttemptState::NotStarted(NotStartedReason::Voided));
    // The returned grant is now worthless.
    let (why, _) = refusal(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), back, 62_000);
    assert_eq!(why, LiveRefusal::GrantMismatch);
}

#[test]
fn a_use_window_never_extends_past_nine_hundred_seconds_after_the_request() {
    let mut live = live_with(EPOCH, EventDraft::user(UserDraft::RunCreated { spec: spec_with(8, 900_000), spec_digest: hash('b') }, T0));
    let request = request_with(1, 1, ApprovalKind::Start, OperationClass::Start, 900_000);
    let pair = Vec::from([
        EventDraft::user(UserDraft::AttemptCreated { attempt: n(1), continuation: Continuation::Fresh }, T0),
        EventDraft::supervisor(SupervisorDraft::StartRequested { request, description: description() }, T0).unwrap(),
    ]);
    commit(&mut live, pair, None, 1_000);
    let grant = single(commit(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 899_000)]), None, 900_000));
    assert_eq!(grant.use_by(), m(901_000));
}

#[test]
fn a_decision_needs_an_open_monotonic_window() {
    let mut live = live();
    commit(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let mut last = live_with(EPOCH, created(T0));
    commit(&mut last, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    assert_eq!(refusal(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 61_000).0, LiveRefusal::WindowClosed);
    let _grant = single(commit(&mut last, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 60_999));
}

#[test]
fn a_grant_is_minted_only_if_the_frame_leaves_its_approval_granted() {
    // F13: a decision and a cancel in one frame end with the approval voided, and no grant.
    let mut live = live();
    commit(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let grants = commit(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_000), cancel(CancelScope::Attempt(n(1)), T0 + 2_000)]), None, 2_000);
    assert!(grants.is_empty());
    assert_eq!(approval_state(&live, 1), ApprovalState::Voided);
}

#[test]
fn an_adopted_decision_is_history_and_mints_nothing() {
    let mut live = live();
    commit(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let staged = staged(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000);
    let abandoned = live.abandon(staged).ok().unwrap();
    assert_eq!(abandoned.events().len(), 1);
    // Nothing can be staged while the uncertain frame is unresolved (F8).
    assert_eq!(refusal(&mut live, Vec::from([cancel(CancelScope::Run, T0 + 2_100)]), None, 2_100).0, LiveRefusal::Unresolved);
    let outcome = landed(&mut live, &abandoned);
    assert!(live.adopt(abandoned, outcome).is_ok());
    assert_eq!(approval_state(&live, 1), ApprovalState::Granted);
    // The approval is recorded as granted, but no grant exists: the start cannot be staged.
    assert_eq!(refusal(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), None, 3_000).0, LiveRefusal::GrantRequired);
    // The supervisor records that the write was uncertain (r5 5.4), and the attempt never starts.
    let void = supervisor(SupervisorDraft::ApprovalVoided { approval: a(1), reason: VoidReason::WriteUncertain }, T0 + 3_000);
    commit(&mut live, Vec::from([void]), None, 3_000);
    assert_eq!(attempt_state(&live, 1), AttemptState::NotStarted(NotStartedReason::Voided));
}

#[test]
fn an_adopted_grant_is_voided_by_its_use_window() {
    let mut live = live();
    commit(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let staged = staged(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000);
    let abandoned = live.abandon(staged).ok().unwrap();
    let outcome = landed(&mut live, &abandoned);
    live.adopt(abandoned, outcome).ok().unwrap();
    assert!(live.due(m(61_999), T0 + 2_000, size_of(&live)).drafts.is_empty());
    let due = live.due(m(62_000), T0 + 2_000, size_of(&live));
    assert_eq!(*due.drafts[0].body(), EventBody::ApprovalVoided { approval: a(1), reason: VoidReason::UseByPassed });
}

#[test]
fn an_abandoned_consumption_drops_its_grant() {
    let (mut live, grant) = approved_live();
    let staged = staged(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), Some(grant), 3_000);
    assert_eq!(refusal(&mut live, Vec::from([cancel(CancelScope::Run, T0 + 3_000)]), None, 3_000).0, LiveRefusal::InFlight);
    let abandoned = live.abandon(staged).ok().unwrap();
    // The start did not land: the frame is discarded, and the start can never be retried with that grant.
    let outcome = absent(&mut live, &abandoned);
    assert!(live.discard(abandoned, outcome).ok().unwrap().is_none());
    assert_eq!(attempt_state(&live, 1), AttemptState::Approved);
    assert_eq!(refusal(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), None, 3_000).0, LiveRefusal::GrantRequired);
}

#[test]
fn a_frame_is_confirmed_only_by_its_own_supervision() {
    let mut mine = live();
    let mut theirs = live_with(LiveEpoch::new(2), created(T0));
    let foreign = staged(&mut theirs, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    assert_eq!(mine.confirm(foreign).err(), Some(LiveRefusal::NotThisSupervision));
    let foreign = staged(&mut live_with(LiveEpoch::new(3), created(T0)), attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    assert_eq!(mine.abandon(foreign).err(), Some(LiveRefusal::NotThisSupervision));
    assert_eq!(mine.projection().attempts().len(), 0);
}

#[test]
fn a_supervised_run_is_never_reconciled() {
    // r5 5.4: reconciliation is for a run nobody supervises. A reconciler void here would leave the attempt approved
    // with nothing to end it, and an interruption would end an attempt whose child this supervision still owns.
    let (mut live, grant) = approved_live();
    let void = EventDraft::reconciler(ReconcilerDraft::ApprovalVoided { approval: a(1), reason: VoidReason::SupervisionLost }, T0 + 3_000);
    let (why, back) = refusal(&mut live, Vec::from([void]), Some(grant), 3_000);
    assert_eq!(why, LiveRefusal::Reconciliation);
    assert!(back.is_some());
    let mut live = running_live();
    assert_eq!(refusal(&mut live, Vec::from([interrupted(1, InterruptObservation::PossiblyRunning, T0 + 5_000)]), None, 5_000).0, LiveRefusal::Reconciliation);
    // Also under clock_fault, where withdrawals and voids are otherwise accepted.
    let _ = live.due(m(1), T0 + 5_000, size_of(&live));
    let withdraw = EventDraft::reconciler(ReconcilerDraft::ApprovalWithdrawn { approval: a(1), reason: WithdrawalReason::SupervisionLost }, T0 + 5_000);
    assert_eq!(refusal(&mut live, Vec::from([withdraw]), None, 5_000).0, LiveRefusal::Reconciliation);
    assert_eq!(attempt_state(&live, 1), AttemptState::Running);
}

#[test]
fn a_frame_of_another_supervision_with_the_same_epoch_is_refused() {
    // Epochs are unique only by the caller's promise; a frame must also extend this supervision's own projection.
    let mut first = live();
    let mut second = live();
    let theirs = staged(&mut first, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let mine = staged(&mut second, Vec::from([cancel(CancelScope::Run, T0 + 1_000)]), None, 1_000);
    assert_eq!(second.confirm(theirs).err(), Some(LiveRefusal::NotThisSupervision));
    assert!(second.confirm(mine).ok().unwrap().is_empty());
    assert_eq!(second.projection().state(), Some(RunState::Cancelled));
    assert!(second.projection().attempts().is_empty());
}

#[test]
fn a_monotonic_regression_leaves_only_safety_drafts() {
    // Starting attempt, held operation-free: the clock goes back.
    let (mut live, grant) = approved_live();
    commit(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), Some(grant), 3_000);
    let due = live.due(m(2_000), T0 + 4_000, size_of(&live));
    assert_eq!(due.clock, ClockCheck::Regressed);
    assert!(live.in_clock_fault());
    assert_eq!(due.drafts.len(), 1);
    assert_eq!(*due.drafts[0].body(), EventBody::StopRequested { attempt: n(1), reason: StopReason::ClockFault });
    commit(&mut live, due.drafts, None, 2_000);
    // F2: the child's identity can still be recorded and its exit observed.
    commit(&mut live, Vec::from([started(1, T0 + 5_000)]), None, 5_000);
    // The exit's write is torn: a repair can still be recorded (D3.2c-2: only with the verified tail), then the exit.
    let staged = staged(&mut live, Vec::from([exited(1, 0, T0 + 6_000)]), None, 6_000);
    let abandoned = live.abandon(staged).ok().unwrap();
    let outcome = torn(&mut live, &abandoned, 10);
    let evidence = live.discard(abandoned, outcome).ok().unwrap().unwrap();
    let journal = committed(&live);
    let repair = live.stage_repair(&evidence, 1, &journal, &journal, T0 + 6_500, m(6_500), size_of(&live)).ok().unwrap();
    live.confirm(repair).ok().unwrap();
    commit(&mut live, Vec::from([exited(1, 0, T0 + 7_000)]), None, 7_000);
    assert_eq!(attempt_state(&live, 1), AttemptState::Stopped(StoppedKind::ClockFault));
    // A new attempt cannot.
    assert_eq!(refusal(&mut live, attempt_pair(2, 2, T0 + 7_000), None, 7_000).0, LiveRefusal::ClockFault);
}

#[test]
fn after_a_regression_no_decision_or_grant_is_accepted() {
    let mut live = running_live();
    commit(&mut live, Vec::from([operation(2, 1, T0 + 5_000)]), None, 5_000);
    commit(&mut live, Vec::from([operation(3, 1, T0 + 5_000)]), None, 5_000);
    let grant = single(commit(&mut live, Vec::from([decide(2, Decision::Granted, T0 + 6_000)]), None, 6_000));
    // The regression is seen by stage itself.
    assert_eq!(refusal(&mut live, Vec::from([decide(3, Decision::Granted, T0 + 6_500)]), None, 1_000).0, LiveRefusal::ClockFault);
    let (why, back) = refusal(&mut live, Vec::from([consumed(2, T0 + 7_000)]), Some(grant), 7_000);
    assert_eq!(why, LiveRefusal::ClockFault);
    assert!(back.is_some());
    // The stop that due proposes withdraws and voids what is left, with the clock-fault reason.
    let due = live.due(m(7_000), T0 + 7_000, size_of(&live));
    let staged = staged(&mut live, due.drafts, None, 7_000);
    let bodies: Vec<EventBody> = staged.events().iter().map(|event| event.body.clone()).collect();
    assert_eq!(
        bodies,
        [
            EventBody::StopRequested { attempt: n(1), reason: StopReason::ClockFault },
            EventBody::ApprovalVoided { approval: a(2), reason: VoidReason::ClockFault },
            EventBody::ApprovalWithdrawn { approval: a(3), reason: WithdrawalReason::ClockFault },
        ]
    );
    assert!(live.confirm(staged).ok().unwrap().is_empty());
}

#[test]
fn a_regression_before_the_start_expires_and_voids_what_is_open() {
    let mut live = live();
    commit(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 5_000);
    let due = live.due(m(4_000), T0 + 5_000, size_of(&live));
    assert_eq!(due.clock, ClockCheck::Regressed);
    assert_eq!(*due.drafts[0].body(), EventBody::ApprovalExpired { approval: a(1), basis: ExpiryBasis::ClockFault });
    commit(&mut live, due.drafts, None, 5_000);
    assert_eq!(attempt_state(&live, 1), AttemptState::NotStarted(NotStartedReason::Expired));
}

#[test]
fn deadlines_follow_monotonic_time_with_a_fixed_precedence() {
    // Start deadline: begun at 3 s, 60 s allowed.
    let (mut live, grant) = approved_live();
    commit(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), Some(grant), 3_000);
    assert!(live.due(m(62_999), T0, size_of(&live)).drafts.is_empty());
    let due = live.due(m(63_000), T0, size_of(&live));
    assert_eq!(*due.drafts[0].body(), EventBody::StopRequested { attempt: n(1), reason: StopReason::Deadline(DeadlineKind::Start) });
    // Stall: started at 4 s, 120 s without progress; a pending approval stops the stall clock and expires on its own.
    let mut live = running_live();
    commit(&mut live, Vec::from([operation(2, 1, T0 + 100_000)]), None, 100_000);
    assert!(live.due(m(124_000), T0 + 124_000, size_of(&live)).drafts.is_empty());
    let due = live.due(m(160_000), T0 + 124_000, size_of(&live));
    assert_eq!(*due.drafts[0].body(), EventBody::ApprovalExpired { approval: a(2), basis: ExpiryBasis::Monotonic });
    commit(&mut live, due.drafts, None, 160_000);
    let due = live.due(m(160_000), T0 + 160_000, size_of(&live));
    assert_eq!(*due.drafts[0].body(), EventBody::StopRequested { attempt: n(1), reason: StopReason::Deadline(DeadlineKind::Stall) });
    // A full journal comes before a deadline, and a clock fault before a full journal (F1).
    let mut live = running_live();
    let reserve = JournalSize { events: MAX_EVENTS - RESERVED_EVENTS, bytes: 0 };
    let due = live.due(m(124_000), T0 + 124_000, reserve);
    assert_eq!(due.drafts.len(), 1);
    assert_eq!(*due.drafts[0].body(), EventBody::StopRequested { attempt: n(1), reason: StopReason::JournalFull });
    let due = live.due(m(1), T0 + 124_000, reserve);
    assert_eq!(due.drafts.len(), 1);
    assert_eq!(*due.drafts[0].body(), EventBody::StopRequested { attempt: n(1), reason: StopReason::ClockFault });
}

#[test]
fn the_wall_clock_can_only_close_an_approval_window_early() {
    let mut live = running_live();
    commit(&mut live, Vec::from([operation(2, 1, T0 + 100_000)]), None, 100_000);
    let due = live.due(m(101_000), T0 + 160_000, size_of(&live));
    assert_eq!(*due.drafts[0].body(), EventBody::ApprovalExpired { approval: a(2), basis: ExpiryBasis::Wall });
    // A wall clock far behind keeps nothing open past the monotonic deadline.
    let due = live.due(m(160_000), T0, size_of(&live));
    assert_eq!(*due.drafts[0].body(), EventBody::ApprovalExpired { approval: a(2), basis: ExpiryBasis::Monotonic });
}

#[test]
fn coalesced_progress_still_counts_for_the_stall_clock() {
    let mut live = running_live();
    commit(&mut live, Vec::from([progress(1, T0 + 100_000)]), None, 100_000);
    let staged = staged(&mut live, Vec::from([progress(1, T0 + 105_000)]), None, 105_000);
    assert!(staged.events().is_empty());
    live.confirm(staged).ok().unwrap();
    assert!(live.due(m(224_999), T0 + 224_999, size_of(&live)).drafts.is_empty());
    assert_eq!(live.due(m(225_000), T0 + 224_999, size_of(&live)).drafts.len(), 1);
}

#[test]
fn an_overflowing_window_is_already_closed() {
    // A decision so late on the monotonic clock that its use window overflows mints no grant.
    let mut live = live();
    commit(&mut live, attempt_pair(1, 1, T0 + 1_000), None, u64::MAX - 70_000);
    assert!(commit(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, u64::MAX - 69_000).is_empty());
    // And a deadline that overflows is due at once.
    let due = live.due(m(u64::MAX - 69_000), T0 + 2_000, size_of(&live));
    assert_eq!(*due.drafts[0].body(), EventBody::ApprovalVoided { approval: a(1), reason: VoidReason::UseByPassed });
}

#[test]
fn an_operation_description_stays_in_memory_while_its_request_is_pending() {
    let mut live = running_live();
    let staged = staged(&mut live, Vec::from([operation(2, 1, T0 + 5_000)]), None, 5_000);
    // The recorded event carries the request and its digest, never the text.
    assert!(matches!(&staged.events()[0].body, EventBody::ApprovalRequested(request) if request.fields().description_sha256 == hash('e')));
    live.confirm(staged).ok().unwrap();
    assert_eq!(live.description(a(2)), Some(&description()));
    commit(&mut live, Vec::from([decide(2, Decision::Denied, T0 + 6_000)]), None, 6_000);
    assert_eq!(live.description(a(2)), None);
}

// ---------------------------------------------------------------------------------------------------------------
// Seeded property test (decision 12.14). A hand-rolled generator (SplitMix64) drives random drafts from every
// producer, forged labels, clock regressions, absurd wall readings, uncertain writes and wrong grants through
// stage/confirm/abandon/adopt/discard, and checks invariants I1–I14 of r5 section 9 after every step, and that the
// recorded frames alone rebuild the live projection. A seed that ever fails is added to REGRESSION_SEEDS.

/// Seeds that failed during development, kept as regression fixtures and run with more steps.
const REGRESSION_SEEDS: [u64; 0] = [];
const SEEDS: u64 = 400;
const STEPS: usize = 160;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

struct World {
    rng: Rng,
    live: LiveSupervision,
    frames: Vec<Vec<Event>>,
    grants: Vec<LiveGrant>,
    mono: u64,
    wall: u64,
    minted: usize,
    granted_decisions: usize,
    /// Whether the last append was uncertain: the driver never makes two in a row, which poisons (tested apart).
    uncertain: bool,
    /// The last sidecar a repair used.
    sidecar: u32,
}

impl World {
    fn new(seed: u64) -> World {
        let mut live = live();
        let frames = Vec::from([live.projection().events().to_vec()]);
        let _ = &mut live;
        World {
            rng: Rng(seed),
            live,
            frames,
            grants: Vec::new(),
            mono: 1_000,
            wall: T0 + 1_000,
            minted: 0,
            granted_decisions: 0,
            uncertain: false,
            sidecar: 0,
        }
    }

    fn tick(&mut self) -> (u64, u64) {
        if self.rng.below(1_000) < 3 {
            // A regression: the monotonic reading goes back, and the supervision is in clock_fault for good.
            self.mono = self.mono.saturating_sub(self.rng.below(5_000) + 1);
        } else if self.rng.chance(2) {
            // A long pause: windows and deadlines pass.
            self.mono += 60_000 + self.rng.below(200_000);
        } else {
            self.mono += self.rng.below(6_000);
        }
        let wall = match self.rng.below(100) {
            0..=2 => u64::MAX - self.rng.below(10),
            3..=6 => self.wall.saturating_sub(self.rng.below(100_000)),
            _ => {
                self.wall += self.rng.below(6_000);
                self.wall
            }
        };
        (self.mono, wall)
    }

    fn attempt_no(&mut self) -> u8 {
        let current = self.live.projection().current_attempt().map(|record| record.attempt().number());
        let count = self.live.projection().attempts().len() as u8;
        match current {
            Some(number) if !self.rng.chance(10) => number,
            _ => (self.rng.below(u64::from(count) + 2) as u8).clamp(1, MAX_ATTEMPT_RECORDS),
        }
    }

    fn approval_no(&mut self) -> u64 {
        let count = self.live.projection().approvals().len() as u64;
        let open = self.live.projection().approvals().iter().rev().find(|r| r.state().is_open()).map(|r| r.id().number());
        if let (Some(open), true) = (open, self.rng.chance(60)) {
            return open;
        }
        if count > 0 && !self.rng.chance(10) {
            count - self.rng.below(count.min(3))
        } else {
            self.rng.below(count + 2).max(1)
        }
    }

    /// One random batch, and whether it is a consumption that wants a grant.
    fn batch(&mut self, wall: u64) -> Vec<EventDraft> {
        let attempt = self.attempt_no();
        let k = self.approval_no();
        let next_k = self.live.projection().approvals().len() as u64 + 1;
        let next_n = self.live.projection().attempts().len() as u8 + 1;
        let one = |draft| Vec::from([draft]);
        let reasons = [StopReason::Deadline(DeadlineKind::Run), StopReason::JournalFull, StopReason::ClockFault, StopReason::Deadline(DeadlineKind::Stall)];
        let observations = [InterruptObservation::NoIdentity, InterruptObservation::NotRunning, InterruptObservation::Unknown, InterruptObservation::PossiblyRunning, InterruptObservation::EvidenceLost];
        // Weighted: actions that end the whole run are rare, so most runs reach grants, starts and ends.
        const WEIGHTS: [u64; 22] = [6, 0, 10, 0, 10, 8, 3, 2, 1, 8, 10, 4, 2, 1, 3, 1, 1, 1, 1, 1, 2, 4];
        let mut pick = self.rng.below(WEIGHTS.iter().sum());
        let action = WEIGHTS.iter().position(|w| {
            if pick < *w {
                true
            } else {
                pick -= w;
                false
            }
        });
        match action.unwrap_or(21) as u64 {
            0 | 1 => attempt_pair(next_n.min(MAX_ATTEMPT_RECORDS), next_k, wall),
            2 | 3 => one(decide(k, if self.rng.chance(70) { Decision::Granted } else { Decision::Denied }, wall)),
            4 => one(begin(attempt, k, wall)),
            5 => one(started(attempt, wall)),
            6 => one(exited(attempt, if self.rng.chance(60) { 0 } else { 1 }, wall)),
            7 => one(stop(attempt, reasons[self.rng.below(4) as usize], wall)),
            8 => one(cancel(if self.rng.chance(85) { CancelScope::Attempt(n(attempt)) } else { CancelScope::Run }, wall)),
            9 => one(operation(next_k, attempt, wall)),
            10 => one(consumed(k, wall)),
            11 => one(progress(attempt, wall)),
            12 => one(relay(RelayDraft::SessionObserved { attempt: n(attempt), session: token(&format!("s-{}", self.rng.below(3))) }, wall)),
            13 => one(relay(RelayDraft::OutcomeClaimed { attempt: n(attempt), claim: OutcomeClaim::Failure }, wall)),
            14 => {
                let open = self.live.projection().concerns().iter().filter(|c| c.resolution().is_none()).map(Concern::concern).collect::<Vec<_>>();
                match open.first() {
                    Some(concern) => {
                        let resolution = match concern.kind() {
                            ConcernKind::PossibleOrphan => Resolution::ConfirmedGone,
                            ConcernKind::WorkspaceUnknown => Resolution::AcceptedRisk,
                            _ => Resolution::Acknowledged,
                        };
                        one(resolve(*concern, resolution, wall))
                    }
                    None if self.rng.chance(10) => one(EventDraft::user(UserDraft::RunClosed { reason: CloseReason::ByUser }, wall)),
                    None => one(progress(attempt, wall)),
                }
            }
            15 => one(interrupted(attempt, observations[self.rng.below(5) as usize], wall)),
            16 => one(EventDraft::reconciler(ReconcilerDraft::AttemptNotStarted { attempt: n(attempt), reason: NotStartedCause::SupervisionLost }, wall)),
            17 => one(repaired(self.rng.below(3) as u32, wall)),
            18 => one(supervisor(SupervisorDraft::StartFailed { attempt: n(attempt), code: StartFailCode::StartError }, wall)),
            19 => one(supervisor(SupervisorDraft::ApprovalVoided { approval: a(k), reason: VoidReason::WriteUncertain }, wall)),
            20 => {
                // A forged label: any body with a producer it does not allow.
                let body = match self.rng.below(4) {
                    0 => decide(k, Decision::Granted, wall).body().clone(),
                    1 => exited(attempt, 0, wall).body().clone(),
                    2 => begin(attempt, k, wall).body().clone(),
                    _ => resolve(ConcernRef::new(ConcernKind::JournalRepaired, None, Seq::new(1).unwrap()).unwrap(), Resolution::Acknowledged, wall).body().clone(),
                };
                let producers = [Producer::User, Producer::Supervisor, Producer::ProviderRelay, Producer::Reconciler, Producer::Journal];
                let forged = producers.into_iter().filter(|p| !body.drafted_by(*p)).nth(self.rng.below(2) as usize).unwrap_or(Producer::ProviderRelay);
                one(EventDraft::forged(forged, wall, body))
            }
            _ => self.live.due(MonoMs::new(self.mono), wall, size_of(&self.live)).drafts,
        }
    }

    fn step(&mut self) {
        let (mono, wall) = self.tick();
        let before = self.live.projection().clone();
        let mut drafts = self.batch(wall);
        if self.rng.chance(5) && drafts.len() > 1 {
            drafts.reverse();
        }
        let wants_grant = drafts.iter().any(|d| matches!(d.body().kind(), EventKind::StartBegun | EventKind::ApprovalConsumed));
        let named = drafts.iter().find_map(|d| match d.body() {
            EventBody::StartBegun { approval, .. } | EventBody::ApprovalConsumed { approval, .. } => Some(*approval),
            _ => None,
        });
        let matching = self.grants.iter().position(|g| Some(g.approval()) == named);
        let grant = if let (Some(index), true) = (matching, self.rng.chance(85)) {
            Some(self.grants.remove(index))
        } else if wants_grant && !self.grants.is_empty() && self.rng.chance(50) {
            let index = self.rng.below(self.grants.len() as u64) as usize;
            Some(self.grants.remove(index))
        } else if !wants_grant && !self.grants.is_empty() && self.rng.chance(3) {
            Some(self.grants.remove(0))
        } else {
            None
        };
        let size = if self.rng.chance(4) { JournalSize { events: MAX_EVENTS - RESERVED_EVENTS, bytes: 0 } } else { size_of(&self.live) };
        let creates = drafts.first().is_some_and(|d| d.body().kind() == EventKind::AttemptCreated);
        let relay_only = drafts.iter().all(|d| d.producer() == Producer::ProviderRelay);
        let blocked = before.has_unresolved_blocking_concern();
        let reconciling = drafts.iter().any(|d| d.producer() == Producer::Reconciler);
        match self.live.stage(&drafts, grant, MonoMs::new(mono), size) {
            Ok(_) if reconciling => panic!("a supervised run accepted a reconciler draft"),
            Err(refused) => {
                if let Some(grant) = refused.grant {
                    self.grants.push(grant);
                }
                // I12: a blocking concern is the only reason given for refusing a new attempt because of concerns.
                if refused.refusal == LiveRefusal::Rejected(Rejection::BlockingConcern) {
                    assert!(blocked, "BlockingConcern without a blocking concern");
                }
                assert_eq!(self.live.projection(), &before, "a refused stage changed the projection");
            }
            Ok(staged) => {
                if creates && !staged.events().is_empty() {
                    assert!(!blocked, "I12: an attempt was created while a blocking concern was open");
                }
                let decisions = staged
                    .events()
                    .iter()
                    .filter(|e| matches!(e.body, EventBody::ApprovalDecided { decision: Decision::Granted, .. }))
                    .count();
                let events = staged.events().to_vec();
                let roll = if self.uncertain { 0 } else { self.rng.below(100) };
                self.uncertain = roll > 84;
                match roll {
                    0..=84 => {
                        let grants = self.live.confirm(staged).ok().unwrap();
                        // Grants come only from decisions this very frame recorded.
                        assert!(grants.len() <= decisions, "more grants than fresh granted decisions");
                        self.granted_decisions += decisions;
                        self.minted += grants.len();
                        for grant in &grants {
                            assert_eq!(self.live.projection().approval(grant.approval()).unwrap().state(), ApprovalState::Granted);
                        }
                        self.grants.extend(grants);
                        if !events.is_empty() {
                            self.frames.push(events);
                        }
                    }
                    85..=94 => {
                        let abandoned = self.live.abandon(staged).ok().unwrap();
                        assert_eq!(self.live.projection(), &before);
                        if events.is_empty() {
                            // Nothing was to be written: the file is the committed bytes.
                            let outcome = absent(&mut self.live, &abandoned);
                            assert!(self.live.discard(abandoned, outcome).ok().unwrap().is_none());
                        } else {
                            let outcome = landed(&mut self.live, &abandoned);
                            self.live.adopt(abandoned, outcome).ok().unwrap();
                            self.frames.push(events);
                        }
                    }
                    _ => {
                        let abandoned = self.live.abandon(staged).ok().unwrap();
                        let len = abandoned.bytes.len();
                        let repairs = self.live.projection().events().iter().filter(|e| e.body.kind() == EventKind::JournalRepaired).count();
                        if len > 1 && repairs < 30 && self.rng.chance(50) {
                            // Torn: the repair is owed, nothing else stages, and only the measured tail is recorded.
                            let cut = 1 + self.rng.below(len as u64 - 1) as usize;
                            let outcome = torn(&mut self.live, &abandoned, cut);
                            let evidence = self.live.discard(abandoned, outcome).ok().unwrap().unwrap();
                            assert_eq!(evidence.len() as usize, cut);
                            assert_eq!(self.live.projection(), &before);
                            assert_eq!(self.live.stage(&[], None, MonoMs::new(self.mono), size_of(&self.live)).err().map(|r| r.refusal), Some(LiveRefusal::RepairOwed));
                            let journal = committed(&self.live);
                            self.sidecar += 1;
                            let repair = self
                                .live
                                .stage_repair(&evidence, self.sidecar, &journal, &journal, wall, MonoMs::new(self.mono), size_of(&self.live))
                                .ok()
                                .unwrap();
                            let recorded = repair.events().to_vec();
                            assert!(matches!(&recorded[0].body, EventBody::JournalRepaired(r) if r.removed_bytes() as usize == cut));
                            self.live.confirm(repair).ok().unwrap();
                            self.frames.push(recorded);
                            self.uncertain = false;
                        } else {
                            let outcome = absent(&mut self.live, &abandoned);
                            assert!(self.live.discard(abandoned, outcome).ok().unwrap().is_none());
                        }
                        assert_eq!(self.live.projection().events()[..before.events().len()], before.events()[..]);
                    }
                }
            }
        }
        // A live repair recorded in this step is not the relay's.
        let repaired = self.live.projection().events()[before.events().len()..].iter().any(|e| e.body.kind() == EventKind::JournalRepaired);
        if relay_only && !repaired {
            check_relay_changed_nothing(&before, self.live.projection());
        }
        check_invariants(&before, self.live.projection());
        // I13 and replay fidelity: the recorded frames alone rebuild the live projection.
        let replayed = self.frames.iter().fold(RunProjection::empty(run_id()), |projection, frame| match apply_recorded(&projection, frame) {
            Ok(next) => next,
            Err(fault) => panic!("recorded frame faulted: {fault:?}"),
        });
        assert_eq!(&replayed, self.live.projection(), "replay differs from the live projection");
        // F1: what due proposes never repeats a key and stages as one batch.
        if !self.live.in_clock_fault() || self.rng.chance(50) {
            let probe = self.live.due(MonoMs::new(self.mono), wall, size_of(&self.live));
            let mut keys = Vec::new();
            for draft in &probe.drafts {
                for key in draft.body().semantic_keys() {
                    assert!(!keys.contains(&key), "due repeated a key");
                    keys.push(key);
                }
            }
            if !probe.drafts.is_empty() {
                let applied = crate::lifecycle::reduce::apply(self.live.projection(), &probe.drafts, size_of(&self.live));
                assert!(applied.is_ok(), "due proposed a batch the reducer refuses: {:?}", applied.err());
            }
        }
    }
}

/// I6: a provider-relayed batch changes no lifecycle state, decision or concern; it may only add pending requests.
/// The one concern a relay-labelled event can raise is `clock_irregular` (r5 3.6), and only when Ordelis's own stated
/// time was clamped: it reflects the caller's clock, never the provider's content.
fn check_relay_changed_nothing(before: &RunProjection, after: &RunProjection) {
    assert_eq!(before.state(), after.state());
    let states = |p: &RunProjection| p.attempts().iter().map(|r| r.state()).collect::<Vec<_>>();
    assert_eq!(states(before), states(after));
    assert_eq!(before.concerns(), &after.concerns()[..before.concerns().len()]);
    for added in &after.concerns()[before.concerns().len()..] {
        assert_eq!(added.concern().kind(), ConcernKind::ClockIrregular);
        assert!(after.events()[added.concern().raised_at().number() as usize - 1].clamped);
    }
    for (old, new) in before.approvals().iter().zip(after.approvals()) {
        assert_eq!(old.state(), new.state());
    }
    assert!(after.approvals()[before.approvals().len()..].iter().all(|r| r.state() == ApprovalState::Pending && r.kind() == ApprovalKind::Operation));
}

fn check_invariants(before: &RunProjection, after: &RunProjection) {
    let spec = after.spec().unwrap();
    // I1: at most one attempt is not terminal.
    assert!(after.attempts().iter().filter(|r| !r.state().is_terminal()).count() <= 1, "I1");
    // I2: terminal states never change.
    for (old, new) in before.attempts().iter().zip(after.attempts()) {
        if old.state().is_terminal() {
            assert_eq!(old.state(), new.state(), "I2 attempt");
        }
    }
    for (old, new) in before.approvals().iter().zip(after.approvals()) {
        if old.state().is_terminal() {
            assert_eq!(old.state(), new.state(), "I2 approval");
        }
    }
    if before.state().is_some_and(RunState::is_terminal) {
        assert_eq!(before.state(), after.state(), "I2 run");
    }
    let events = after.events();
    for (index, event) in events.iter().enumerate() {
        // I8: contiguous sequence, non-decreasing time, clamped only on an event that kept the previous time.
        assert_eq!(event.seq.number(), index as u64 + 1, "I8 seq");
        if index > 0 {
            assert!(event.at_ms >= events[index - 1].at_ms, "I8 time");
            if event.clamped {
                assert_eq!(event.at_ms, events[index - 1].at_ms, "I8 clamped");
            }
        }
        match &event.body {
            // I3: a start identity follows a start that consumed the attempt's granted start approval, and is the
            // run's executable.
            EventBody::AttemptStarted { attempt, os_identity } => {
                let begun = events[..index].iter().any(|e| matches!(&e.body, EventBody::StartBegun { attempt: x, .. } if x == attempt));
                assert!(begun, "I3 begun");
                assert_eq!(*os_identity.executable_sha256(), spec.provider().binding_target().executable_sha256, "I3 hash");
            }
            EventBody::StartBegun { attempt, approval, binding } => {
                let record = after.approval(*approval).unwrap();
                assert_eq!(record.attempt(), *attempt, "I5 own attempt");
                assert_eq!(record.kind(), ApprovalKind::Start);
                assert_eq!(*binding, record.request().fields().binding, "I3 binding");
            }
            // I4: every grant is a user decision with the presented digest, inside its recorded window.
            EventBody::ApprovalDecided { approval, decision: Decision::Granted, provenance } => {
                let record = after.approval(*approval).unwrap();
                assert_eq!(event.producer, Producer::User, "I4 producer");
                assert_eq!(provenance.presented_binding_digest, record.request().fields().binding_digest, "I4 digest");
                assert!(event.at_ms.ms() < record.decision_closes_at().unwrap(), "I4 window");
            }
            _ => {}
        }
        // I6: no provider-relayed event decides, consumes, ends, changes a lifecycle or resolves.
        if event.producer == Producer::ProviderRelay {
            assert!(matches!(event.body.kind(), EventKind::ApprovalRequested | EventKind::ProviderSessionObserved | EventKind::Progress | EventKind::ProviderOutcomeClaimed), "I6");
        }
    }
    // I5: each approval is consumed at most once.
    for record in after.approvals() {
        let uses = events
            .iter()
            .filter(|e| matches!(&e.body, EventBody::StartBegun { approval, .. } | EventBody::ApprovalConsumed { approval, .. } if *approval == record.id()))
            .count();
        assert!(uses <= 1, "I5");
        if uses == 1 {
            assert_eq!(record.state(), ApprovalState::Consumed, "I5 state");
        }
    }
    // I7: the run succeeded exactly when one attempt succeeded, and it is the last.
    let succeeded: Vec<_> = after.attempts().iter().filter(|r| r.state() == AttemptState::Succeeded).collect();
    assert_eq!(after.state() == Some(RunState::Succeeded), succeeded.len() == 1 && after.attempts().last().unwrap().state() == AttemptState::Succeeded, "I7");
    assert!(succeeded.len() <= 1, "I7 at most one");
    // I10: only an observed failure is presented as a failure.
    if let Some(RunState::Exhausted(last)) = after.state() {
        let last_started = after.attempts().iter().rev().filter_map(|r| r.state().outcome()).find(|c| c.may_have_run());
        assert_eq!(ExhaustedLast::of(last_started.unwrap_or(OutcomeClass::NeverStarted)), Some(last), "I10");
    }
    // I11: both attempt budgets hold.
    assert!(after.started_attempts() <= usize::from(spec.max_started_attempts()), "I11 started");
    assert!(after.attempts().len() <= usize::from(MAX_ATTEMPT_RECORDS), "I11 records");
    // I14: a start failure never started and raised no workspace concern.
    for record in after.attempts() {
        if let AttemptState::NotStarted(NotStartedReason::StartFailed(_)) = record.state() {
            assert!(!after.concerns().iter().any(|c| c.concern().attempt() == Some(record.attempt())), "I14");
        }
        if record.state().outcome() == Some(OutcomeClass::Unknown) {
            assert!(after.concerns().iter().any(|c| c.concern().kind() == ConcernKind::PossibleOrphan && c.concern().attempt() == Some(record.attempt())), "orphan");
        }
    }
}

fn run_seed(seed: u64, steps: usize) -> (RunProjection, Vec<Vec<Event>>, usize) {
    let mut world = World::new(seed);
    for _ in 0..steps {
        world.step();
    }
    assert!(world.minted <= world.granted_decisions);
    (world.live.projection().clone(), world.frames, world.minted)
}

#[test]
fn seeded_property_runs_keep_every_invariant_and_replay_identically() {
    for seed in REGRESSION_SEEDS {
        run_seed(seed, STEPS * 4);
    }
    let mut minted = 0;
    let mut events = 0;
    let mut terminal = 0;
    let mut kinds: Vec<(EventKind, usize)> = Vec::new();
    for seed in 0..SEEDS {
        let (projection, frames, grants) = run_seed(seed, STEPS);
        // Determinism: the same seed gives the same journal and projection.
        let (again, frames_again, _) = run_seed(seed, STEPS);
        assert_eq!(projection, again, "seed {seed}");
        assert_eq!(frames, frames_again, "seed {seed}");
        minted += grants;
        events += projection.events().len();
        terminal += projection.attempts().iter().filter(|r| r.state().is_terminal()).count();
        for kind in EventKind::ALL {
            let c = projection.events().iter().filter(|e| e.body.kind() == kind).count();
            match kinds.iter_mut().find(|(k, _)| *k == kind) {
                Some(slot) => slot.1 += c,
                None => kinds.push((kind, c)),
            }
        }
    }
    // The generator reaches every kind a supervised run can record, and the interesting states, not only refusals.
    // Interruption and supervision-lost ends come only from reconciliation, which a supervised run refuses.
    for (kind, count) in &kinds {
        let reconciler_only = matches!(kind, EventKind::AttemptInterrupted | EventKind::AttemptNotStarted);
        assert_eq!(*count > 0, !reconciler_only, "{kind:?} recorded {count} times");
    }
    assert!(minted > SEEDS as usize / 2, "few grants were minted: {minted}");
    assert!(events > (SEEDS as usize) * 10, "few events were recorded: {events}");
    assert!(terminal > SEEDS as usize, "few attempts ended: {terminal}");
}

// ---------------------------------------------------------------------------------------------------------------
// Seeded crash-reconciliation property test (D3.2b-3, issue #116). The generator above runs a supervision to a random
// crash point. Then the process "ends": only the recorded frames remain. They are reconciled with random repair
// records and random observations (fresh, stale, mismatched, contradictory, of other attempts), opened through
// begin_reconciled, and driven further by the same generator, still holding the dead supervision's grants. After the
// crash: reconciliation is deterministic and idempotent, never says "never started" after a repair, ends every attempt
// in flight, preserves every earlier concern, proposes termination only on a fresh matching identity, and the reopened
// supervision has no grant, clock or description from before; no old grant is ever accepted again.

use crate::lifecycle::reconcile::{
    is_quiescent, reconcile, start_conflicts, summarize, AttemptObservation, HistoryStatus, ReadBasis, ReconcileInput,
    Recommendation, RecordedHistory, RunInventory, RunSummary, Seen, StartConflict,
};
use crate::lifecycle::reconcile::tests::never;

/// Seeds that failed during development, kept as regression fixtures.
const CRASH_REGRESSION_SEEDS: [u64; 0] = [];
const CRASH_SEEDS: u64 = 300;

#[derive(Default)]
struct CrashStats {
    outcomes: Vec<String>,
    ready: usize,
    pending: usize,
    proposals: usize,
}

fn replay_frames(frames: &[Vec<Event>]) -> RunProjection {
    frames.iter().fold(RunProjection::empty(run_id()), |projection, frame| match apply_recorded(&projection, frame) {
        Ok(next) => next,
        Err(fault) => panic!("recorded frame faulted: {fault:?}"),
    })
}

fn inventory_with(summary: RunSummary) -> RunInventory {
    let mut summaries: Vec<(RunId, RunSummary)> = (1..=6).map(|number| (RunId::new(number).unwrap(), never(number))).collect();
    summaries.push((run_id(), summary));
    RunInventory::Enumerated { allocated: 7, summaries, observed: Vec::new() }
}

fn crash_and_reconcile(seed: u64, stats: &mut CrashStats) {
    let mut world = World::new(seed ^ 0x5eed_0b3a);
    let crash_at = 1 + world.rng.below(STEPS as u64) as usize;
    for _ in 0..crash_at {
        world.step();
        // Crash more often while a child's identity is recorded, a short-lived state in the generator.
        let identified = world.live.projection().current_attempt().is_some_and(|record| record.identity().is_some());
        if identified && world.rng.chance(30) {
            break;
        }
    }
    let recorded = replay_frames(&world.frames);
    assert_eq!(&recorded, world.live.projection());
    let old_approvals = recorded.approvals().len() as u64;
    // Repair records still missing from the journal: usually none, sometimes new sidecars, sometimes one already recorded.
    let fresh_repair = |sidecar| JournalRepair::new(sidecar, 1 + sidecar % 40, 7, RepairCause::IncompleteTail).unwrap();
    let already = recorded.events().iter().find_map(|e| match &e.body {
        EventBody::JournalRepaired(repair) => Some(*repair),
        _ => None,
    });
    let repairs = match world.rng.below(10) {
        0..=5 => Vec::new(),
        6 | 7 => Vec::from([fresh_repair(10)]),
        8 => Vec::from([fresh_repair(10), fresh_repair(11)]),
        _ => Vec::from([already.unwrap_or(fresh_repair(12))]),
    };
    // A new process: a new monotonic clock, and an epoch that is new (or, against the caller's promise, reused).
    let mono = 6_000 + world.rng.below(10_000);
    let epoch = if world.rng.chance(50) { EPOCH } else { LiveEpoch::new(2) };
    let mut observations = Vec::new();
    let in_flight = world.live.projection().current_attempt().map(|record| record.attempt());
    for index in 0..world.rng.below(4) {
        // The first observation is usually about the attempt in flight; the others about any attempt.
        let attempt = match in_flight {
            Some(attempt) if index == 0 && world.rng.chance(80) => attempt,
            _ => n(world.attempt_no()),
        };
        let recorded_identity = if world.rng.chance(80) { identity() } else { OsIdentity::new(4243, token("start-17"), hash('a')).unwrap() };
        let seen = [Seen::IdentityMatches, Seen::GoneConfirmed, Seen::PidAbsent, Seen::PidReused, Seen::IdentityMatches, Seen::Unobservable][world.rng.below(6) as usize];
        let observed_epoch = if world.rng.chance(85) { epoch } else { LiveEpoch::new(77) };
        let taken = if world.rng.chance(10) { mono + 1 } else { mono.saturating_sub(world.rng.below(7_000)) };
        observations.push(AttemptObservation { attempt, recorded: recorded_identity, seen, epoch: observed_epoch, taken_mono: MonoMs::new(taken) });
    }
    let size = JournalSize { events: recorded.events().len() as u32, bytes: 0 };
    let stated = if world.rng.chance(5) { u64::MAX } else { world.wall + world.rng.below(5_000) };
    let input = ReconcileInput { epoch, mono: MonoMs::new(mono), stated_at_ms: stated, size };
    let history = || RecordedHistory::new(recorded.clone(), HistoryStatus::Verified, repairs.clone());
    let reconciliation = match reconcile(&history(), &observations, input) {
        Ok(reconciliation) => reconciliation,
        Err(refused) => panic!("seed {seed}: reconciliation refused: {refused:?}"),
    };
    // Determinism, whatever the order of the observations.
    let mut reversed = observations.clone();
    reversed.reverse();
    for again in [reconcile(&history(), &observations, input), reconcile(&history(), &reversed, input)] {
        let again = again.ok().unwrap();
        assert_eq!(again.frame.drafts(), reconciliation.frame.drafts(), "seed {seed}");
        assert_eq!(again.findings, reconciliation.findings);
        assert_eq!(again.recommendations(), reconciliation.recommendations());
        assert_eq!(again.blocking, reconciliation.blocking);
        assert_eq!(again.informational, reconciliation.informational);
    }
    // The frame: repair records first, then at most one reconciler end; no stop, no cancel, nothing else.
    let drafts = reconciliation.frame.drafts();
    let repair_count = drafts.iter().take_while(|d| d.producer() == Producer::Journal).count();
    assert!(drafts[repair_count..].iter().all(|d| d.producer() == Producer::Reconciler));
    assert!(drafts.len() - repair_count <= 1);
    assert!(drafts.iter().all(|d| matches!(d.body().kind(), EventKind::JournalRepaired | EventKind::AttemptInterrupted | EventKind::AttemptNotStarted)));
    let current = recorded.current_attempt().cloned();
    assert_eq!(current.is_some(), drafts.len() > repair_count, "seed {seed}: the attempt in flight is ended, and only it");
    let ending = drafts.last().filter(|d| d.producer() == Producer::Reconciler).map(|d| d.body().clone());
    if let (Some(record), Some(body)) = (&current, &ending) {
        let repaired = repair_count > 0 || recorded.events().iter().any(|e| e.seq > record.last_change() && e.body.kind() == EventKind::JournalRepaired);
        match body {
            EventBody::AttemptNotStarted { .. } => {
                assert!(!repaired, "seed {seed}: never started after a repair");
                assert!(matches!(record.state(), AttemptState::Proposed | AttemptState::Approved));
            }
            EventBody::AttemptInterrupted { observation, .. } => {
                stats.outcomes.push(format!("{observation:?}"));
                let fresh_match = observations.iter().any(|o| {
                    o.attempt == record.attempt()
                        && Some(&o.recorded) == record.identity()
                        && o.seen == Seen::IdentityMatches
                        && o.epoch == epoch
                        && o.taken_mono.ms() <= mono
                        && mono - o.taken_mono.ms() <= MAX_OBSERVATION_AGE_MS
                });
                if *observation == InterruptObservation::PossiblyRunning {
                    assert!(fresh_match, "seed {seed}: possibly running without a fresh matching observation");
                }
            }
            _ => panic!("seed {seed}: unexpected reconciliation draft"),
        }
        if matches!(body, EventBody::AttemptNotStarted { .. }) {
            stats.outcomes.push("NotStarted".into());
        }
    }
    let proposals = reconciliation.recommendations().iter().filter(|r| matches!(r, Recommendation::TerminationProposal { .. })).count();
    let possibly = matches!(ending, Some(EventBody::AttemptInterrupted { observation: InterruptObservation::PossiblyRunning, .. }));
    assert_eq!(proposals, usize::from(possibly), "seed {seed}: a proposal exactly for a fresh matching identity");
    stats.proposals += proposals;
    // Before opening, the run blocks every start: its attempt in flight was never reconciled, or a repair is pending.
    let conflicts = start_conflicts(&inventory_with(summarize(&history(), ReadBasis::Locked)), &[]);
    if let Some(record) = &current {
        assert!(conflicts.contains(&StartConflict::Unreconciled(run_id(), record.attempt())), "seed {seed}");
    }
    // Opening.
    let wrong_epoch = reconcile(&history(), &observations, ReconcileInput { epoch: LiveEpoch::new(55), ..input }).ok().unwrap();
    assert!(begin_reconciled(epoch, history(), wrong_epoch.frame, MonoMs::new(mono), size).is_err());
    let mut live = match begin_reconciled(epoch, history(), reconciliation.frame, MonoMs::new(mono), size) {
        Ok(Opening::Ready(live)) => {
            assert_eq!(repair_count + usize::from(current.is_some()), 0);
            stats.ready += 1;
            live
        }
        Ok(Opening::Pending(pending)) => {
            stats.pending += 1;
            world.frames.push(pending.events().to_vec());
            open(pending)
        }
        Err(refused) => panic!("seed {seed}: begin_reconciled refused: {refused:?}"),
    };
    let after = live.projection().clone();
    assert_eq!(replay_frames(&world.frames), after, "seed {seed}: the opening frame replays");
    assert!(is_quiescent(&after, &repairs));
    check_invariants(&recorded, &after);
    // Every concern recorded before stays, with its resolution; the attempt it ended is a blocking orphan unless it
    // never started.
    assert_eq!(recorded.concerns(), &after.concerns()[..recorded.concerns().len()]);
    if let Some(record) = &current {
        if after.attempt(record.attempt()).unwrap().state().outcome() == Some(OutcomeClass::Unknown) {
            assert!(reconciliation.blocking.iter().any(|c| c.kind() == ConcernKind::PossibleOrphan && c.attempt() == Some(record.attempt())));
        }
    }
    let unresolved_blocking: Vec<ConcernRef> = after.concerns().iter().filter(|c| c.is_unresolved_blocking()).map(Concern::concern).collect();
    assert_eq!(reconciliation.blocking, unresolved_blocking);
    // No authority crosses: no grant, no clock, no description, no outstanding frame.
    assert!(live.outstanding.is_empty() && live.approvals.is_empty() && live.attempts.is_empty() && live.descriptions.is_empty());
    assert!(live.in_flight.is_none() && live.unresolved.is_none() && !live.clock_fault);
    // Idempotence: the opened history reconciles to nothing.
    let settled = RecordedHistory::new(after.clone(), HistoryStatus::Verified, repairs.clone());
    let again = reconcile(&settled, &observations, input).ok().unwrap();
    assert!(again.frame.drafts().is_empty(), "seed {seed}: reconciliation is not idempotent");
    assert_eq!(again.blocking, reconciliation.blocking);
    assert_eq!(again.informational, reconciliation.informational);
    // Once supervised, the run blocks starts only through its unresolved orphans.
    let orphans: Vec<StartConflict> = unresolved_blocking.iter().map(|c| StartConflict::PossibleOrphan(run_id(), c.attempt().unwrap())).collect();
    assert_eq!(start_conflicts(&inventory_with(summarize(&settled, ReadBasis::Locked)), &[live.held()]), orphans);
    // Keep going with the reopened supervision, still holding the dead supervision's grants.
    std::mem::swap(&mut world.live, &mut live);
    world.mono = mono;
    world.uncertain = false;
    world.sidecar = after
        .events()
        .iter()
        .filter_map(|e| match &e.body {
            EventBody::JournalRepaired(repair) => Some(repair.sidecar()),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    for _ in 0..world.rng.below(STEPS as u64 / 2) {
        world.step();
        assert!(world.live.outstanding.iter().all(|approval| approval.number() > old_approvals), "seed {seed}: an old approval became usable");
    }
}

#[test]
fn seeded_crash_reconciliation_never_carries_authority_across_a_restart() {
    let mut stats = CrashStats::default();
    for seed in CRASH_REGRESSION_SEEDS {
        crash_and_reconcile(seed, &mut stats);
    }
    for seed in 0..CRASH_SEEDS {
        crash_and_reconcile(seed, &mut stats);
    }
    // The generator reaches every reconciliation outcome, both openings and the termination proposal.
    for outcome in ["NotStarted", "EvidenceLost", "NoIdentity", "NotRunning", "Unknown", "PossiblyRunning"] {
        let count = stats.outcomes.iter().filter(|o| *o == outcome).count();
        assert!(count > 3, "{outcome} reached {count} times");
    }
    assert!(stats.ready > 0 && stats.pending > 0, "ready {} pending {}", stats.ready, stats.pending);
    assert!(stats.proposals > 0);
}

#[test]
fn a_start_batched_with_anything_is_refused_and_the_grant_comes_back() {
    // D3.2c-1 (#120), integration review probe p4.
    let (mut live, grant) = approved_live();
    let before = live.projection().clone();
    let (refusal, grant) = refusal(&mut live, Vec::from([begin(1, 1, T0 + 3_000), started(1, T0 + 3_000)]), Some(grant), 3_000);
    assert_eq!(refusal, LiveRefusal::Rejected(Rejection::StartNotLast));
    assert_eq!(live.projection(), &before);
    // The same grant still records the start alone; the identity follows in its own frame.
    commit(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), grant, 3_000);
    commit(&mut live, Vec::from([started(1, T0 + 4_000)]), None, 4_000);
    assert_eq!(attempt_state(&live, 1), AttemptState::Running);
}

// ---------------------------------------------------------------------------------------------------------------
// Evidence-bound uncertain writes, repairs and sizes (D3.2c-2, issue #122)

/// `running_live`, with the canonical bytes of its frames counted independently of the supervision.
fn running_live_counted() -> (LiveSupervision, u64) {
    let mut live = live();
    let mut bytes = encode_frame(live.projection().events()).unwrap().len() as u64;
    let mut take = |live: &mut LiveSupervision, drafts: Vec<EventDraft>, grant: Option<LiveGrant>, mono: u64| {
        let staged = staged(live, drafts, grant, mono);
        bytes += encode_frame(staged.events()).unwrap().len() as u64;
        live.confirm(staged).ok().unwrap()
    };
    take(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let grant = single(take(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000));
    take(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), Some(grant), 3_000);
    take(&mut live, Vec::from([started(1, T0 + 4_000)]), None, 4_000);
    (live, bytes)
}

/// `staged`, under a name a local binding cannot shadow.
fn stage_one(live: &mut LiveSupervision, drafts: Vec<EventDraft>, grant: Option<LiveGrant>, mono: u64) -> Staged {
    staged(live, drafts, grant, mono)
}

/// A supervision with attempt 1 proposed, and its decision staged and abandoned: the abandoned frame and its bytes.
fn uncertain_decision() -> (LiveSupervision, Abandoned) {
    let mut live = live();
    commit(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let staged = stage_one(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000);
    let abandoned = live.abandon(staged).ok().unwrap();
    (live, abandoned)
}

#[test]
fn an_outcome_resolves_only_its_own_frame_and_only_as_it_shows() {
    // Absent cannot adopt: the projection does not move.
    let (mut live, abandoned) = uncertain_decision();
    let before = live.projection().clone();
    let outcome = absent(&mut live, &abandoned);
    let abandoned = match live.adopt(abandoned, outcome) {
        Err(LiveRefusal::OutcomeMismatch) => None,
        other => Some(other),
    };
    assert!(abandoned.is_none(), "adopted on Absent");
    assert_eq!(live.projection(), &before);
    // Landed cannot discard.
    let (mut live, abandoned) = uncertain_decision();
    let outcome = landed(&mut live, &abandoned);
    assert_eq!(live.discard(abandoned, outcome).err(), Some(LiveRefusal::OutcomeMismatch));
    assert_eq!(attempt_state(&live, 1), AttemptState::Proposed);
    // An outcome another supervision made for its own identical frame (same run, other epoch) is refused.
    let (mut live, abandoned) = uncertain_decision();
    let mut other = live_with(LiveEpoch::new(2), created(T0));
    commit(&mut other, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let theirs = stage_one(&mut other, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000);
    let theirs = other.abandon(theirs).ok().unwrap();
    assert_eq!(theirs.bytes, abandoned.bytes);
    let foreign = landed(&mut other, &theirs);
    assert_eq!(live.adopt(abandoned, foreign).err(), Some(LiveRefusal::OutcomeMismatch));
    // So is an outcome of this supervision's earlier frame, presented for a later one.
    let (mut live, abandoned) = uncertain_decision();
    let stale = absent(&mut live, &abandoned);
    let outcome = absent(&mut live, &abandoned);
    live.discard(abandoned, outcome).ok().unwrap();
    commit(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_050)]), None, 2_050);
    let staged = stage_one(&mut live, Vec::from([cancel(CancelScope::Run, T0 + 2_100)]), None, 2_100);
    let later = live.abandon(staged).ok().unwrap();
    assert_eq!(live.discard(later, stale).err(), Some(LiveRefusal::OutcomeMismatch));
}

#[test]
fn the_committed_bytes_must_be_exactly_the_supervisions_frames() {
    let (mut live, abandoned) = uncertain_decision();
    let committed = committed(&live);
    // Missing the last frame, an extra frame, and bytes that are not a clean stream.
    let short = encode_frame(&live.projection().events()[..1]).unwrap();
    let mut longer = committed.clone();
    longer.extend_from_slice(&abandoned.bytes);
    let mut torn_committed = committed.clone();
    torn_committed.extend_from_slice(&abandoned.bytes[..5]);
    for wrong in [short, longer, torn_committed, Vec::new()] {
        assert_eq!(live.verify_write(&abandoned, &wrong, &committed).err(), Some(LiveRefusal::NotThisJournal));
    }
    assert!(!live.poisoned());
    // A frame staged by another supervision is not this one's to verify.
    let mut other = live_with(LiveEpoch::new(2), created(T0));
    commit(&mut other, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let theirs = stage_one(&mut other, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000);
    let theirs = other.abandon(theirs).ok().unwrap();
    assert_eq!(live.verify_write(&theirs, &committed, &committed).err(), Some(LiveRefusal::NotThisSupervision));
}

#[test]
fn a_diverged_or_unknown_outcome_poisons_for_good() {
    let altered = |committed: &[u8], frame: &[u8]| {
        let mut file = [committed, frame].concat();
        file[3] ^= 1;
        file
    };
    let foreign = |committed: &[u8], frame: &[u8]| [committed, frame, frame].concat();
    let shortened = |committed: &[u8], _: &[u8]| committed[..committed.len() - 1].to_vec();
    let corrupt = |committed: &[u8], frame: &[u8]| {
        let mut tail = frame.to_vec();
        tail[8] ^= 0x20;
        [committed, &tail].concat()
    };
    let too_large = |committed: &[u8], _: &[u8]| [committed, &vec![b'x'; (MAX_STREAM_BYTES as usize) + 1]].concat();
    let files: [&dyn Fn(&[u8], &[u8]) -> Vec<u8>; 5] = [&altered, &foreign, &shortened, &corrupt, &too_large];
    for file in files {
        let (mut live, abandoned) = uncertain_decision();
        let outcome = verified_write(&mut live, &abandoned, file);
        assert!(matches!(outcome.check(), PersistCheck::Diverged(_) | PersistCheck::Unknown(_)), "{:?}", outcome.check());
        assert!(live.poisoned());
        // Nothing resolves it, and nothing more stages.
        assert_eq!(live.discard(abandoned, outcome).err(), Some(LiveRefusal::Poisoned));
        assert_eq!(refusal(&mut live, Vec::from([cancel(CancelScope::Run, T0 + 3_000)]), None, 3_000).0, LiveRefusal::Poisoned);
        // Its claim excuses nothing and blocks every start.
        let conflicts = start_conflicts(&inventory_with(summarize(&RecordedHistory::new(live.projection().clone(), HistoryStatus::Verified, Vec::new()), ReadBasis::Locked)), &[live.held()]);
        assert!(conflicts.contains(&StartConflict::JournalPoisoned(run_id())), "{conflicts:?}");
    }
}

#[test]
fn a_torn_tail_the_frame_does_not_start_with_poisons() {
    // Review of #122 (P1): bytes after the committed ones that are not a prefix of the frame written were never written
    // by this supervision. As after an empty frame, they poison; they are never repaired as this frame's tail.
    let (mut live, abandoned) = uncertain_decision();
    let outcome = verified_write(&mut live, &abandoned, |committed, _| [committed, &b"ORDL1 999"[..]].concat());
    assert!(matches!(outcome.check(), PersistCheck::Absent { torn: Some(tail) } if !tail.pending_prefix));
    assert!(live.poisoned());
    assert_eq!(live.discard(abandoned, outcome).err(), Some(LiveRefusal::Poisoned));
    // A zero-filled tail is not this frame's either.
    let (mut live, abandoned) = uncertain_decision();
    let _ = verified_write(&mut live, &abandoned, |committed, _| [committed, &[0u8; 64][..]].concat());
    assert!(live.poisoned());
    // After an empty frame, the same bytes are foreign too: one verdict for both.
    let mut fresh = self::live();
    commit(&mut fresh, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let duplicate = stage_one(&mut fresh, attempt_pair(1, 1, T0 + 1_000), None, 1_100);
    let empty = fresh.abandon(duplicate).ok().unwrap();
    let outcome = verified_write(&mut fresh, &empty, |committed, _| [committed, &b"ORDL1 999"[..]].concat());
    assert!(matches!(outcome.check(), PersistCheck::Diverged(_)));
    assert!(fresh.poisoned());
}

#[test]
fn an_opening_frame_states_its_durability() {
    let pending = begin_new(EPOCH, run_id(), created(T0), m(0)).ok().unwrap();
    assert_eq!(pending.durability(), Durability::EffectGate);
    drop_pending(pending);
}

#[test]
fn contradictory_verifications_of_one_frame_poison() {
    // The same re-read twice is consistent, and the frame resolves.
    let (mut live, abandoned) = uncertain_decision();
    let first = absent(&mut live, &abandoned);
    let again = absent(&mut live, &abandoned);
    drop(first);
    assert!(!live.poisoned());
    live.discard(abandoned, again).ok().unwrap();
    // Absent, then Landed for the same frame: a caller shopping for an outcome poisons the supervision.
    let (mut live, abandoned) = uncertain_decision();
    let absent_outcome = absent(&mut live, &abandoned);
    let landed_outcome = landed(&mut live, &abandoned);
    assert!(live.poisoned());
    assert_eq!(live.adopt(abandoned, landed_outcome).err(), Some(LiveRefusal::Poisoned));
    drop(absent_outcome);
    // Landed, then a torn tail: poisoned too.
    let (mut live, abandoned) = uncertain_decision();
    let _ = landed(&mut live, &abandoned);
    let _ = torn(&mut live, &abandoned, 5);
    assert!(live.poisoned());
}

#[test]
fn two_uncertain_appends_in_a_row_poison_and_a_durable_one_resets_the_count() {
    // Abandon, discard, and abandon again: poisoned at the second, whatever its outcome would be.
    let (mut live, abandoned) = uncertain_decision();
    let outcome = absent(&mut live, &abandoned);
    assert!(live.discard(abandoned, outcome).ok().unwrap().is_none());
    assert!(!live.poisoned());
    let staged = stage_one(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_100)]), None, 2_100);
    let abandoned = live.abandon(staged).ok().unwrap();
    assert!(live.poisoned());
    let committed = committed(&live);
    assert_eq!(live.verify_write(&abandoned, &committed, &committed).err(), Some(LiveRefusal::Poisoned));
    // Abandon, adopt (it landed, but the append was still uncertain), and abandon again: poisoned too.
    let (mut live, abandoned) = uncertain_decision();
    let outcome = landed(&mut live, &abandoned);
    live.adopt(abandoned, outcome).ok().unwrap();
    let staged = stage_one(&mut live, Vec::from([cancel(CancelScope::Run, T0 + 3_000)]), None, 3_000);
    let _ = live.abandon(staged).ok().unwrap();
    assert!(live.poisoned());
    // Abandon, discard, confirm, abandon: the confirmed append ends the run of uncertain ones.
    let (mut live, abandoned) = uncertain_decision();
    let outcome = absent(&mut live, &abandoned);
    live.discard(abandoned, outcome).ok().unwrap();
    commit(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_100)]), None, 2_100);
    let staged = stage_one(&mut live, Vec::from([cancel(CancelScope::Run, T0 + 3_000)]), None, 3_000);
    let abandoned = live.abandon(staged).ok().unwrap();
    assert!(!live.poisoned());
    let outcome = landed(&mut live, &abandoned);
    live.adopt(abandoned, outcome).ok().unwrap();
    assert_eq!(MAX_CONSECUTIVE_UNCERTAIN, 2);
}

#[test]
fn adopting_mints_nothing_and_takes_exactly_the_landed_frame() {
    let (mut live, abandoned) = uncertain_decision();
    let expected = [live.projection().events(), abandoned.events()].concat();
    let outcome = landed(&mut live, &abandoned);
    live.adopt(abandoned, outcome).ok().unwrap();
    assert_eq!(live.projection().events(), &expected[..]);
    assert!(live.outstanding.is_empty());
    assert_eq!(refusal(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), None, 3_000).0, LiveRefusal::GrantRequired);
}

#[test]
fn a_torn_tail_is_owed_until_the_repair_of_exactly_its_bytes_is_confirmed() {
    let (mut live, abandoned) = uncertain_decision();
    let frame = abandoned.bytes.clone();
    let base = live.journal_size().bytes;
    let cut = 17;
    let outcome = torn(&mut live, &abandoned, cut);
    let evidence = live.discard(abandoned, outcome).ok().unwrap().unwrap();
    assert_eq!((evidence.len() as usize, evidence.crc32c()), (cut, crc32c(&frame[..cut])));
    let journal = committed(&live);
    // While owed: the tail counts toward the size, nothing else stages, and the run blocks every start.
    assert_eq!(live.journal_size().bytes, base + cut as u64);
    assert_eq!(refusal(&mut live, Vec::from([cancel(CancelScope::Run, T0 + 3_000)]), None, 3_000).0, LiveRefusal::RepairOwed);
    let history = || RecordedHistory::new(live.projection().clone(), HistoryStatus::Verified, Vec::new());
    assert!(start_conflicts(&inventory_with(summarize(&history(), ReadBasis::Locked)), &[live.held()]).contains(&StartConflict::RepairPending(run_id())));
    // The file must no longer hold the tail, and the committed bytes must be this supervision's.
    let with_tail = [&journal[..], &frame[..cut]].concat();
    assert_eq!(live.stage_repair(&evidence, 1, &journal, &with_tail, T0 + 3_000, m(3_000), size_of(&live)).err(), Some(LiveRefusal::RepairUnshown));
    assert_eq!(live.stage_repair(&evidence, 1, &with_tail, &with_tail, T0 + 3_000, m(3_000), size_of(&live)).err(), Some(LiveRefusal::NotThisJournal));
    // Evidence of another supervision's tail is not this one's.
    let (mut other, theirs) = uncertain_decision();
    let outcome = torn(&mut other, &theirs, cut);
    let foreign = other.discard(theirs, outcome).ok().unwrap().unwrap();
    let mut impostor = live_with(LiveEpoch::new(2), created(T0));
    commit(&mut impostor, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    let impostor_journal = committed(&impostor);
    assert_eq!(impostor.stage_repair(&foreign, 1, &impostor_journal, &impostor_journal, T0 + 3_000, m(3_000), size_of(&impostor)).err(), Some(LiveRefusal::RepairUnshown));
    // The record is built from the measured tail, never from the caller's figures.
    let staged = live.stage_repair(&evidence, 1, &journal, &journal, T0 + 3_000, m(3_000), size_of(&live)).ok().unwrap();
    assert_eq!(staged.events().len(), 1);
    assert!(matches!(&staged.events()[0].body, EventBody::JournalRepaired(r) if r.sidecar() == 1 && r.removed_bytes() as usize == cut && r.removed_crc32c() == crc32c(&frame[..cut])));
    assert_eq!(staged.durability(), Durability::Critical);
    let repair_bytes = encode_frame(staged.events()).unwrap().len() as u64;
    live.confirm(staged).ok().unwrap();
    // Paid: the tail no longer counts, and the same evidence cannot record a second repair.
    assert_eq!(live.journal_size().bytes, base + repair_bytes);
    let journal = committed(&live);
    assert_eq!(live.stage_repair(&evidence, 2, &journal, &journal, T0 + 3_100, m(3_100), size_of(&live)).err(), Some(LiveRefusal::RepairUnshown));
    commit(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 3_200)]), None, 3_200);
}

#[test]
fn a_repair_needs_a_new_sidecar_and_never_stages_as_a_plain_draft() {
    let mut live = running_live();
    // A journal draft through `stage` is refused, whatever its figures (N7).
    assert_eq!(refusal(&mut live, Vec::from([repaired(1, T0 + 5_000)]), None, 5_000).0, LiveRefusal::RepairUnshown);
    // Two torn writes, each repaired; the second must use a sidecar above the first.
    for wall in [T0 + 6_000, T0 + 7_000] {
        let sidecar = 4;
        let next = live.projection().approvals().len() as u64 + 1;
        let staged = stage_one(&mut live, Vec::from([operation(next, 1, wall)]), None, wall - T0);
        let abandoned = live.abandon(staged).ok().unwrap();
        let outcome = torn(&mut live, &abandoned, 3);
        let evidence = live.discard(abandoned, outcome).ok().unwrap().unwrap();
        let journal = committed(&live);
        let result = live.stage_repair(&evidence, sidecar, &journal, &journal, wall, m(wall - T0), size_of(&live));
        if live.projection().events().iter().any(|e| matches!(&e.body, EventBody::JournalRepaired(r) if r.sidecar() == 4)) {
            assert_eq!(result.err(), Some(LiveRefusal::RepairUnshown));
            let staged = live.stage_repair(&evidence, 5, &journal, &journal, wall, m(wall - T0), size_of(&live)).ok().unwrap();
            live.confirm(staged).ok().unwrap();
        } else {
            live.confirm(result.ok().unwrap()).ok().unwrap();
        }
    }
    let sidecars: Vec<u32> = live.projection().events().iter().filter_map(|e| match &e.body {
        EventBody::JournalRepaired(r) => Some(r.sidecar()),
        _ => None,
    }).collect();
    assert_eq!(sidecars, [4, 5]);
}

#[test]
fn the_size_is_derived_from_the_frames_and_a_caller_can_only_raise_it() {
    let mut live = live();
    let mut frames = Vec::from([live.projection().events().to_vec()]);
    let encoded = |frames: &[Vec<Event>]| frames.iter().map(|f| encode_frame(f).unwrap().len() as u64).sum::<u64>();
    assert_eq!(live.journal_size(), JournalSize { events: 1, bytes: encoded(&frames) });
    let staged = stage_one(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    assert_eq!(staged.bytes(), &encode_frame(staged.events()).unwrap()[..]);
    frames.push(staged.events().to_vec());
    live.confirm(staged).ok().unwrap();
    assert_eq!(live.journal_size(), JournalSize { events: 3, bytes: encoded(&frames) });
    // An empty frame (a duplicate) has no bytes and adds none.
    let duplicate = stage_one(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 1_100);
    assert!(duplicate.events().is_empty() && duplicate.bytes().is_empty());
    live.confirm(duplicate).ok().unwrap();
    assert_eq!(live.journal_size().bytes, encoded(&frames));
    // An understated size cannot hide what the supervision holds: at the ceiling it faults (c-1's second defense).
    live.bytes = MAX_JOURNAL_BYTES + SAFETY_JOURNAL_BYTES;
    assert_eq!(refusal(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000).0, LiveRefusal::CapacityFault);
    assert!(live.in_capacity_fault());
    // In the reserve by bytes alone, `due` proposes the journal-full stop with a size of zero stated.
    let mut running = running_live();
    running.bytes = MAX_JOURNAL_BYTES - RESERVED_JOURNAL_BYTES;
    let due = running.due(m(5_000), T0 + 5_000, JournalSize { events: 0, bytes: 0 });
    assert_eq!(*due.drafts[0].body(), EventBody::StopRequested { attempt: n(1), reason: StopReason::JournalFull });
}

#[test]
fn a_frame_is_as_durable_as_its_strictest_event_generated_consequences_included() {
    let mut live = live();
    let staged = stage_one(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 1_000);
    assert_eq!(staged.durability(), Durability::Critical);
    live.confirm(staged).ok().unwrap();
    let staged = stage_one(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000);
    assert_eq!(staged.durability(), Durability::EffectGate);
    let grant = single(live.confirm(staged).ok().unwrap());
    // A cancel ends the attempt and voids its grant as generated consequences: the frame is critical.
    let staged = stage_one(&mut live, Vec::from([cancel(CancelScope::Attempt(n(1)), T0 + 2_500)]), None, 2_500);
    assert!(staged.events().len() > 1);
    assert_eq!(staged.durability(), Durability::Critical);
    let _ = live.abandon(staged).ok().unwrap();
    drop(grant);
    // The start is an effect gate on its own frame.
    let (mut live, grant) = approved_live();
    let staged = stage_one(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), Some(grant), 3_000);
    assert_eq!(staged.durability(), Durability::EffectGate);
    live.confirm(staged).ok().unwrap();
    commit(&mut live, Vec::from([started(1, T0 + 4_000)]), None, 4_000);
    // Progress alone is advisory; mixed with nothing stricter it stays so.
    let staged = stage_one(&mut live, Vec::from([progress(1, T0 + 5_000)]), None, 5_000);
    assert_eq!(staged.durability(), Durability::Advisory);
}

/// D3.2c-2 property: random sequences of uncertain appends and outcomes never let a frame be adopted or discarded
/// against what the file shows, never mint through adoption, and poison at the second uncertain append in a row or on
/// any divergence. The live size always equals the canonical bytes of the frames it took, plus an owed tail.
#[test]
fn seeded_uncertain_writes_follow_the_file_and_poison_as_specified() {
    let mut poisoned = 0;
    let mut adopted = 0;
    let mut repaired = 0;
    for seed in 0..200 {
        let mut rng = Rng(seed ^ 0xc2_0bad);
        let (mut live, mut expected) = running_live_counted();
        let mut mono = 5_000;
        let mut run = 0u32;
        let mut sidecar = 0;
        for step in 0..40u64 {
            mono += 10;
            let next = live.projection().approvals().len() as u64 + 1;
            let pending = live.projection().approvals().iter().find(|r| r.state() == ApprovalState::Pending).map(|r| r.id().number());
            let draft = match (pending, step % 3) {
                (_, 0) => progress(1, T0 + mono),
                (Some(k), _) => decide(k, Decision::Denied, T0 + mono),
                (None, _) => operation(next, 1, T0 + mono),
            };
            let staged = match try_stage(&mut live, Vec::from([draft]), None, mono) {
                Ok(staged) => staged,
                Err(refused) => panic!("seed {seed} step {step}: {:?}", refused.refusal),
            };
            let bytes = staged.bytes().len() as u64;
            if rng.chance(50) {
                live.confirm(staged).ok().unwrap();
                expected += bytes;
                run = 0;
                continue;
            }
            let before = live.projection().clone();
            let abandoned = live.abandon(staged).ok().unwrap();
            run += 1;
            assert_eq!(live.poisoned(), run >= 2);
            if live.poisoned() {
                poisoned += 1;
                break;
            }
            let frame = abandoned.bytes.clone();
            if frame.is_empty() {
                // A coalesced report: nothing was to be written, and nothing landed.
                let outcome = absent(&mut live, &abandoned);
                assert!(live.discard(abandoned, outcome).ok().unwrap().is_none());
                continue;
            }
            match rng.below(4) {
                0 => {
                    let outcome = landed(&mut live, &abandoned);
                    live.adopt(abandoned, outcome).ok().unwrap();
                    assert!(live.outstanding.is_empty());
                    expected += bytes;
                    adopted += 1;
                }
                1 => {
                    let outcome = absent(&mut live, &abandoned);
                    assert!(live.discard(abandoned, outcome).ok().unwrap().is_none());
                    assert_eq!(live.projection(), &before);
                }
                2 => {
                    let cut = 1 + rng.below(frame.len() as u64 - 1) as usize;
                    let outcome = torn(&mut live, &abandoned, cut);
                    let evidence = live.discard(abandoned, outcome).ok().unwrap().unwrap();
                    assert_eq!(live.journal_size().bytes, expected + cut as u64);
                    let journal = committed(&live);
                    sidecar += 1;
                    let staged = live.stage_repair(&evidence, sidecar, &journal, &journal, T0 + mono, m(mono), size_of(&live)).ok().unwrap();
                    expected += staged.bytes().len() as u64;
                    live.confirm(staged).ok().unwrap();
                    run = 0;
                    repaired += 1;
                }
                _ => {
                    let outcome = verified_write(&mut live, &abandoned, |committed, frame| [committed, &frame[..frame.len() - 1], b"X"].concat());
                    assert!(matches!(outcome.check(), PersistCheck::Diverged(_)));
                    assert!(live.poisoned());
                    assert_eq!(live.adopt(abandoned, outcome).err(), Some(LiveRefusal::Poisoned));
                    poisoned += 1;
                    break;
                }
            }
            // The derived size is exactly the canonical bytes of the frames taken (no tail owed here).
            assert_eq!(live.journal_size().bytes, expected, "seed {seed}");
        }
    }
    assert!(poisoned > 50 && adopted > 50 && repaired > 50, "poisoned {poisoned} adopted {adopted} repaired {repaired}");
}
