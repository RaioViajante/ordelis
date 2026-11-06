// Reducer tests. Pure: synthetic values only; nothing reads a file, a clock or a process.
use super::*;
use crate::agents::catalog::ProviderId;
use crate::agents::evidence::{Sha256, Token};
use crate::lifecycle::model::*;

pub(crate) const T0: u64 = 1_793_836_800_000;

pub(crate) fn hash(c: char) -> Sha256 {
    Sha256::new(&c.to_string().repeat(64)).unwrap()
}

pub(crate) fn digest(n: u64) -> Sha256 {
    Sha256::new(&format!("{n:064x}")).unwrap()
}

pub(crate) fn token(text: &str) -> Token {
    Token::new(text).unwrap()
}

pub(crate) fn run_id() -> RunId {
    RunId::new(7).unwrap()
}

pub(crate) fn n(number: u8) -> AttemptNo {
    AttemptNo::new(number).unwrap()
}

pub(crate) fn a(number: u64) -> ApprovalId {
    ApprovalId::new(number).unwrap()
}

pub(crate) fn spec_with(max_started: u8, ttl_ms: u64) -> RunSpec {
    let limits = Limits::new(60_000, 3_600_000, 120_000, ttl_ms, 5_000).unwrap();
    let task = TaskRef::new(TaskId::parse("t1").unwrap(), WallMs::new(T0).unwrap(), 1).unwrap();
    let project = ProjectRef { project: ProjectId::parse("p1").unwrap(), registered_at_ms: WallMs::new(T0).unwrap() };
    let context = RunContext::new(project, WorkspaceMode::None, Vec::new(), limits, token("policy-1"), None).unwrap();
    let provider = ProviderIdentity { provider: ProviderId::Codex, executable_sha256: hash('a'), advertised_version: Some(token("1.2.3")) };
    RunSpec::new(task, provider, context, max_started).unwrap()
}

pub(crate) fn binding(class: OperationClass) -> Binding {
    Binding {
        spec_digest: hash('b'),
        target: BindingTarget { provider: ProviderId::Codex, executable_sha256: hash('a') },
        operation_class: class,
        operation_digest: hash('c'),
    }
}

pub(crate) fn request_with(k: u64, attempt: u8, kind: ApprovalKind, class: OperationClass, ttl_ms: u64) -> ApprovalRequest {
    let fields = RequestFields {
        approval: a(k),
        run: run_id(),
        attempt: n(attempt),
        kind,
        binding: binding(class),
        binding_digest: digest(k),
        description_sha256: hash('e'),
        provider_risk_claim: None,
        ttl_ms,
    };
    ApprovalRequest::new(fields, WorkspaceMode::None).unwrap()
}

pub(crate) fn description() -> OperationDescription {
    OperationDescription::new("Run the project's test suite").unwrap()
}

// Drafts, one per kind and producer.
pub(crate) fn created(at: u64) -> EventDraft {
    EventDraft::user(UserDraft::RunCreated { spec: spec_with(8, 300_000), spec_digest: hash('b') }, at)
}

pub(crate) fn attempt_pair(attempt: u8, k: u64, at: u64) -> Vec<EventDraft> {
    let request = request_with(k, attempt, ApprovalKind::Start, OperationClass::Start, 60_000);
    Vec::from([
        EventDraft::user(UserDraft::AttemptCreated { attempt: n(attempt), continuation: Continuation::Fresh }, at),
        EventDraft::supervisor(SupervisorDraft::StartRequested { request, description: description() }, at).unwrap(),
    ])
}

pub(crate) fn decide(k: u64, decision: Decision, at: u64) -> EventDraft {
    let provenance = DecisionProvenance { actor: Actor::User, channel: Channel::NativeConfirmation, presented_binding_digest: digest(k) };
    EventDraft::user(UserDraft::ApprovalDecided { approval: a(k), decision, provenance }, at)
}

pub(crate) fn supervisor(draft: SupervisorDraft, at: u64) -> EventDraft {
    EventDraft::supervisor(draft, at).unwrap()
}

pub(crate) fn relay(draft: RelayDraft, at: u64) -> EventDraft {
    EventDraft::relay(draft, at).unwrap()
}

pub(crate) fn begin(attempt: u8, k: u64, at: u64) -> EventDraft {
    supervisor(SupervisorDraft::StartBegun { attempt: n(attempt), approval: a(k), binding: binding(OperationClass::Start) }, at)
}

pub(crate) fn identity() -> OsIdentity {
    OsIdentity::new(4242, token("start-17"), hash('a')).unwrap()
}

pub(crate) fn started(attempt: u8, at: u64) -> EventDraft {
    supervisor(SupervisorDraft::AttemptStarted { attempt: n(attempt), os_identity: identity() }, at)
}

pub(crate) fn exited(attempt: u8, code: i32, at: u64) -> EventDraft {
    supervisor(SupervisorDraft::AttemptExited { attempt: n(attempt), status: ExitStatus::new(Some(code), None).unwrap() }, at)
}

pub(crate) fn stop(attempt: u8, reason: StopReason, at: u64) -> EventDraft {
    supervisor(SupervisorDraft::StopRequested { attempt: n(attempt), reason }, at)
}

pub(crate) fn cancel(scope: CancelScope, at: u64) -> EventDraft {
    EventDraft::user(UserDraft::CancelRequested { scope }, at)
}

pub(crate) fn operation(k: u64, attempt: u8, at: u64) -> EventDraft {
    let request = request_with(k, attempt, ApprovalKind::Operation, OperationClass::ShellRun, 60_000);
    relay(RelayDraft::OperationRequested { request, description: description() }, at)
}

pub(crate) fn consumed(k: u64, at: u64) -> EventDraft {
    supervisor(SupervisorDraft::ApprovalConsumed { approval: a(k), binding: binding(OperationClass::ShellRun) }, at)
}

pub(crate) fn progress(attempt: u8, at: u64) -> EventDraft {
    relay(RelayDraft::Progress { attempt: n(attempt), kind: ProgressKind::Heartbeat }, at)
}

pub(crate) fn interrupted(attempt: u8, observation: InterruptObservation, at: u64) -> EventDraft {
    EventDraft::reconciler(ReconcilerDraft::AttemptInterrupted { attempt: n(attempt), observation }, at)
}

pub(crate) fn repaired(sidecar: u32, at: u64) -> EventDraft {
    EventDraft::journal(JournalDraft::Repaired(JournalRepair::new(sidecar, 40, 7, RepairCause::IncompleteTail).unwrap()), at)
}

pub(crate) fn resolve(concern: ConcernRef, resolution: Resolution, at: u64) -> EventDraft {
    let provenance = ResolutionProvenance { actor: Actor::User, channel: Channel::NativeConfirmation, presented_sha256: hash('f') };
    EventDraft::user(UserDraft::AttentionResolved(ConcernResolution::new(concern, resolution, provenance).unwrap()), at)
}

/// A run's journal kept as frames, with its live projection.
pub(crate) struct Journal {
    pub(crate) projection: RunProjection,
    pub(crate) frames: Vec<Vec<Event>>,
}

impl Journal {
    pub(crate) fn new() -> Journal {
        let mut journal = Journal { projection: RunProjection::empty(run_id()), frames: Vec::new() };
        journal.ok(Vec::from([created(T0)]));
        journal
    }

    pub(crate) fn size(&self) -> JournalSize {
        JournalSize { events: self.projection.events().len() as u32, bytes: 0 }
    }

    pub(crate) fn record(&mut self, batch: Vec<EventDraft>) -> Result<Applied, Rejection> {
        let applied = apply(&self.projection, &batch, self.size())?;
        if !applied.events.is_empty() {
            self.frames.push(applied.events.clone());
        }
        self.projection = applied.projection.clone();
        Ok(applied)
    }

    pub(crate) fn ok(&mut self, batch: Vec<EventDraft>) -> Applied {
        match self.record(batch) {
            Ok(applied) => applied,
            Err(rejection) => panic!("refused: {rejection:?}"),
        }
    }

    pub(crate) fn one(&mut self, draft: EventDraft) -> Applied {
        self.ok(Vec::from([draft]))
    }

    pub(crate) fn refused(&mut self, batch: Vec<EventDraft>) -> Rejection {
        let before = self.projection.clone();
        let rejection = self.record(batch).err().expect("accepted");
        assert_eq!(self.projection, before, "a refused batch changed the projection");
        rejection
    }

    /// The projection rebuilt from the recorded frames alone.
    pub(crate) fn replayed(&self) -> RunProjection {
        self.frames.iter().fold(RunProjection::empty(run_id()), |projection, frame| apply_recorded(&projection, frame).unwrap())
    }

    pub(crate) fn attempt(&self, number: u8) -> AttemptState {
        self.projection.attempt(n(number)).unwrap().state()
    }

    pub(crate) fn approval(&self, k: u64) -> ApprovalState {
        self.projection.approval(a(k)).unwrap().state()
    }

    pub(crate) fn run(&self) -> RunState {
        self.projection.state().unwrap()
    }
}

// Fixtures for every attempt state, attempt 1 with start approval a1.
pub(crate) fn proposed() -> Journal {
    let mut journal = Journal::new();
    journal.ok(attempt_pair(1, 1, T0 + 1_000));
    journal
}

pub(crate) fn approved() -> Journal {
    let mut journal = proposed();
    journal.one(decide(1, Decision::Granted, T0 + 2_000));
    journal
}

pub(crate) fn starting() -> Journal {
    let mut journal = approved();
    journal.one(begin(1, 1, T0 + 3_000));
    journal
}

pub(crate) fn running() -> Journal {
    let mut journal = starting();
    journal.one(started(1, T0 + 4_000));
    journal
}

fn with(mut journal: Journal, draft: EventDraft) -> Journal {
    journal.one(draft);
    journal
}

const AT: u64 = T0 + 50_000;

fn state_fixtures() -> Vec<(&'static str, Journal)> {
    Vec::from([
        ("proposed", proposed()),
        ("approved", approved()),
        ("starting", starting()),
        ("running", running()),
        ("stopping, no identity", with(starting(), cancel(CancelScope::Attempt(n(1)), T0 + 5_000))),
        ("stopping, identity", with(running(), stop(1, StopReason::Deadline(DeadlineKind::Run), T0 + 5_000))),
        ("succeeded", with(running(), exited(1, 0, T0 + 5_000))),
        ("failed", with(running(), exited(1, 3, T0 + 5_000))),
        ("cancelled", with(with(running(), cancel(CancelScope::Attempt(n(1)), T0 + 5_000)), exited(1, 0, T0 + 6_000))),
        ("timed out", with(with(running(), stop(1, StopReason::Deadline(DeadlineKind::Run), T0 + 5_000)), exited(1, 0, T0 + 6_000))),
        ("interrupted", with(running(), interrupted(1, InterruptObservation::Unknown, T0 + 5_000))),
        ("not started", with(proposed(), decide(1, Decision::Denied, T0 + 2_000))),
    ])
}

/// One representative batch of every kind, about attempt 1 and its start approval a1, from its allowed producer.
fn representative(kind: EventKind, journal: &Journal) -> Vec<EventDraft> {
    let one = |draft| Vec::from([draft]);
    match kind {
        EventKind::RunCreated => one(created(AT)),
        EventKind::AttemptCreated => attempt_pair(2, journal.projection.approvals().len() as u64 + 1, AT),
        EventKind::ApprovalRequested => one(operation(journal.projection.approvals().len() as u64 + 1, 1, AT)),
        EventKind::ApprovalDecided => one(decide(1, Decision::Granted, T0 + 2_500)),
        EventKind::ApprovalExpired => one(supervisor(SupervisorDraft::ApprovalExpired { approval: a(1), basis: ExpiryBasis::Monotonic }, AT)),
        EventKind::ApprovalWithdrawn => one(supervisor(SupervisorDraft::ApprovalWithdrawn { approval: a(1), reason: WithdrawalReason::AttemptEnded }, AT)),
        EventKind::ApprovalVoided => one(supervisor(SupervisorDraft::ApprovalVoided { approval: a(1), reason: VoidReason::UseByPassed }, AT)),
        EventKind::ApprovalConsumed => one(consumed(1, AT)),
        EventKind::StartBegun => one(begin(1, 1, T0 + 3_000)),
        EventKind::AttemptStarted => one(started(1, AT)),
        EventKind::StartFailed => one(supervisor(SupervisorDraft::StartFailed { attempt: n(1), code: StartFailCode::StartError }, AT)),
        EventKind::StopRequested => one(stop(1, StopReason::Deadline(DeadlineKind::Run), AT)),
        EventKind::CancelRequested => one(cancel(CancelScope::Attempt(n(1)), AT)),
        EventKind::AttemptExited => one(exited(1, 0, AT)),
        EventKind::AttemptInterrupted => one(interrupted(1, InterruptObservation::Unknown, AT)),
        EventKind::AttemptNotStarted => {
            one(EventDraft::reconciler(ReconcilerDraft::AttemptNotStarted { attempt: n(1), reason: NotStartedCause::SupervisionLost }, AT))
        }
        EventKind::ProviderSessionObserved => one(relay(RelayDraft::SessionObserved { attempt: n(1), session: token("session-1") }, AT)),
        EventKind::Progress => one(progress(1, AT)),
        EventKind::ProviderOutcomeClaimed => one(relay(RelayDraft::OutcomeClaimed { attempt: n(1), claim: OutcomeClaim::Failure }, AT)),
        EventKind::RunClosed => one(EventDraft::user(UserDraft::RunClosed { reason: CloseReason::ByUser }, AT)),
        EventKind::AttentionResolved => {
            let open = journal.projection.concerns().iter().find(|concern| concern.resolution().is_none()).map(Concern::concern);
            let concern = open.unwrap_or_else(|| ConcernRef::new(ConcernKind::JournalRepaired, None, Seq::new(1).unwrap()).unwrap());
            let resolution = match concern.kind() {
                ConcernKind::PossibleOrphan => Resolution::AcceptedRisk,
                ConcernKind::WorkspaceUnknown => Resolution::Reviewed,
                _ => Resolution::Acknowledged,
            };
            one(resolve(concern, resolution, AT))
        }
        EventKind::JournalRepaired => one(repaired(1, AT)),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Expect {
    Moved(AttemptState),
    Same,
    Duplicate,
    Refused,
}

/// The kinds whose representative each fixture already recorded with the same content: re-delivering one is a
/// duplicate in every state, terminal or not (identity is checked before terminal states).
fn recorded(state: &str) -> &'static [EventKind] {
    use EventKind as K;
    match state {
        "approved" => &[K::RunCreated, K::ApprovalDecided],
        "starting" => &[K::RunCreated, K::ApprovalDecided, K::StartBegun],
        "running" | "failed" => &[K::RunCreated, K::ApprovalDecided, K::StartBegun, K::AttemptStarted],
        "stopping, no identity" => &[K::RunCreated, K::ApprovalDecided, K::StartBegun, K::CancelRequested],
        "stopping, identity" => &[K::RunCreated, K::ApprovalDecided, K::StartBegun, K::AttemptStarted, K::StopRequested],
        "succeeded" => &[K::RunCreated, K::ApprovalDecided, K::StartBegun, K::AttemptStarted, K::AttemptExited],
        "cancelled" => &[K::RunCreated, K::ApprovalDecided, K::StartBegun, K::AttemptStarted, K::CancelRequested, K::AttemptExited],
        "timed out" => &[K::RunCreated, K::ApprovalDecided, K::StartBegun, K::AttemptStarted, K::StopRequested, K::AttemptExited],
        "interrupted" => &[K::RunCreated, K::ApprovalDecided, K::StartBegun, K::AttemptStarted, K::AttemptInterrupted],
        _ => &[K::RunCreated],
    }
}

/// The transition table of r5 3.2 for attempt 1, one row per (state, kind) that is not refused.
fn expected(state: &str, kind: EventKind) -> Expect {
    use AttemptState as S;
    use EventKind as K;
    let not_started = |reason| Expect::Moved(S::NotStarted(reason));
    if recorded(state).contains(&kind) {
        return Expect::Duplicate;
    }
    match (state, kind) {
        ("proposed", K::ApprovalDecided) => Expect::Moved(S::Approved),
        ("proposed", K::ApprovalExpired) => not_started(NotStartedReason::Expired),
        ("proposed", K::ApprovalWithdrawn) => not_started(NotStartedReason::Withdrawn),
        ("proposed" | "approved", K::CancelRequested) => not_started(NotStartedReason::Cancelled),
        ("proposed" | "approved", K::AttemptNotStarted) => not_started(NotStartedReason::SupervisionLost),
        ("approved", K::ApprovalVoided) => not_started(NotStartedReason::Voided),
        ("approved", K::StartBegun) => Expect::Moved(S::Starting),
        ("starting", K::AttemptStarted) => Expect::Moved(S::Running),
        ("starting" | "stopping, no identity", K::StartFailed) => not_started(NotStartedReason::StartFailed(StartFailCode::StartError)),
        ("starting" | "running", K::StopRequested) => Expect::Moved(S::Stopping(StopCause::Deadline(DeadlineKind::Run))),
        ("starting" | "running", K::CancelRequested) => Expect::Moved(S::Stopping(StopCause::Cancel)),
        ("running", K::ApprovalRequested) => Expect::Same,
        ("running", K::AttemptExited) => Expect::Moved(S::Succeeded),
        ("running" | "stopping, identity", K::AttemptInterrupted) => Expect::Moved(S::Interrupted(InterruptObservation::Unknown)),
        ("running" | "stopping, identity" | "stopping, no identity", K::ProviderSessionObserved | K::Progress | K::ProviderOutcomeClaimed) => {
            Expect::Same
        }
        ("stopping, no identity", K::AttemptStarted) => Expect::Same,
        ("stopping, no identity", K::StopRequested) => Expect::Same,
        ("stopping, identity", K::CancelRequested) => Expect::Same,
        ("stopping, identity", K::AttemptExited) => Expect::Moved(S::TimedOut(DeadlineKind::Run)),
        // After an end that leaves the run open, a new attempt and a close are possible, and concerns can be
        // resolved. An interrupted attempt raises a blocking orphan, which refuses the new attempt.
        ("failed" | "cancelled" | "timed out" | "not started", K::AttemptCreated | K::RunClosed) => Expect::Same,
        ("interrupted", K::RunClosed) => Expect::Same,
        // Succeeded and never-started attempts raise no concern; the representative then names none and is refused.
        ("failed" | "cancelled" | "timed out" | "interrupted", K::AttentionResolved) => Expect::Same,
        (_, K::JournalRepaired) => Expect::Same,
        _ => Expect::Refused,
    }
}

#[test]
fn the_attempt_transition_table_covers_every_state_and_kind() {
    let mut mismatches = Vec::new();
    for (state, fixture) in state_fixtures() {
        for kind in EventKind::ALL {
            let mut journal = Journal { projection: fixture.projection.clone(), frames: fixture.frames.clone() };
            let before = journal.attempt(1);
            let batch = representative(kind, &journal);
            let result = journal.record(batch);
            let got = match result {
                Err(_) => Expect::Refused,
                Ok(applied) if applied.duplicate => Expect::Duplicate,
                Ok(_) if journal.attempt(1) == before => Expect::Same,
                Ok(_) => Expect::Moved(journal.attempt(1)),
            };
            if got != expected(state, kind) {
                mismatches.push(format!("{state} × {kind:?}: got {got:?}, table says {:?}", expected(state, kind)));
            }
            assert_eq!(journal.replayed(), journal.projection, "{state} × {kind:?}: replay differs");
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn every_kind_from_a_producer_it_does_not_allow_is_refused_first() {
    const PRODUCERS: [Producer; 5] = [Producer::User, Producer::Supervisor, Producer::ProviderRelay, Producer::Reconciler, Producer::Journal];
    for (state, fixture) in state_fixtures() {
        for kind in EventKind::ALL {
            let batch = representative(kind, &fixture);
            let body = batch.last().unwrap().body().clone();
            for producer in PRODUCERS {
                if body.drafted_by(producer) {
                    continue;
                }
                let forged = EventDraft::forged(producer, AT, body.clone());
                let result = apply(&fixture.projection, &[forged], fixture.size());
                assert_eq!(result.err(), Some(Rejection::ProducerNotAllowed), "{state} × {kind:?} labelled {producer:?}");
            }
        }
    }
}

#[test]
fn the_relay_cannot_ask_for_start_consent_and_the_supervisor_cannot_relay_an_operation_request() {
    let start = request_with(1, 1, ApprovalKind::Start, OperationClass::Start, 60_000);
    let shell = request_with(1, 1, ApprovalKind::Operation, OperationClass::ShellRun, 60_000);
    let relayed = EventDraft::relay(RelayDraft::OperationRequested { request: start, description: description() }, AT);
    assert_eq!(relayed.err(), Some(ModelError::WrongProducer));
    let supervised = EventDraft::supervisor(SupervisorDraft::StartRequested { request: shell, description: description() }, AT);
    assert_eq!(supervised.err(), Some(ModelError::WrongProducer));
    // Every relay draft is labelled provider_relay, and none can decide, consume, end, change a lifecycle or resolve.
    for draft in [progress(1, AT), operation(2, 1, AT), relay(RelayDraft::OutcomeClaimed { attempt: n(1), claim: OutcomeClaim::Failure }, AT)] {
        assert_eq!(draft.producer(), Producer::ProviderRelay);
        assert!(!draft.body().kind().is_attempt_event());
    }
}

#[test]
fn a_forged_label_on_a_recorded_event_is_a_fault() {
    let mut journal = running();
    journal.one(exited(1, 0, AT));
    // A frame claiming the provider relay ended the attempt: the label is checked, never trusted.
    let mut journal = running();
    let mut frame = apply(&journal.projection, &[exited(1, 0, AT)], journal.size()).unwrap().events;
    frame[0].producer = Producer::ProviderRelay;
    assert_eq!(apply_recorded(&journal.projection, &frame), Err(JournalFault::ProducerNotAllowed));
    // And a user decision relabelled as the supervisor's.
    journal = approved();
    let decided = journal.frames.last().unwrap().clone();
    let before = proposed().projection;
    let mut forged = decided.clone();
    forged[0].producer = Producer::Supervisor;
    assert_eq!(apply_recorded(&before, &forged), Err(JournalFault::ProducerNotAllowed));
    assert!(apply_recorded(&before, &decided).is_ok());
}

#[test]
fn a_complete_attempt_succeeds_the_run_and_replays_identically() {
    let mut journal = running();
    journal.one(progress(1, T0 + 5_000));
    journal.one(exited(1, 0, T0 + 6_000));
    assert_eq!(journal.attempt(1), AttemptState::Succeeded);
    assert_eq!(journal.run(), RunState::Succeeded);
    assert_eq!(journal.approval(1), ApprovalState::Consumed);
    assert!(journal.projection.concerns().is_empty());
    assert_eq!(journal.replayed(), journal.projection);
    // The same journal built twice is identical, event for event.
    let mut again = running();
    again.one(progress(1, T0 + 5_000));
    again.one(exited(1, 0, T0 + 6_000));
    assert_eq!(again.projection, journal.projection);
    assert_eq!(again.frames, journal.frames);
}

#[test]
fn the_run_specification_is_fixed_by_its_first_event() {
    let mut journal = Journal::new();
    // The same creation again is a duplicate; any other specification conflicts and nothing else may be first.
    assert!(journal.ok(Vec::from([created(T0 + 1)])).duplicate);
    let other = EventDraft::user(UserDraft::RunCreated { spec: spec_with(2, 300_000), spec_digest: hash('b') }, T0);
    assert_eq!(journal.refused(Vec::from([other])), Rejection::Conflict { existing: Seq::new(1).unwrap() });
    let empty = RunProjection::empty(run_id());
    assert_eq!(apply(&empty, &[cancel(CancelScope::Run, T0)], JournalSize { events: 0, bytes: 0 }).err(), Some(Rejection::InvalidTransition));
    assert_eq!(apply(&empty, &[created(MAX_WALL_MS + 1)], JournalSize { events: 0, bytes: 0 }).err(), Some(Rejection::InvalidTime));
}

#[test]
fn a_request_must_match_its_run() {
    // F7: the specification digest, the target, the lifetime the run allows, contiguous ids and the current attempt.
    let mut journal = running();
    let shell = |mutate: &dyn Fn(&mut RequestFields)| {
        let mut fields = request_with(2, 1, ApprovalKind::Operation, OperationClass::ShellRun, 60_000).fields().clone();
        mutate(&mut fields);
        let request = ApprovalRequest::new(fields, WorkspaceMode::None).unwrap();
        relay(RelayDraft::OperationRequested { request, description: description() }, AT)
    };
    let cases: [(&str, &dyn Fn(&mut RequestFields), Rejection); 6] = [
        ("another specification", &|f| f.binding.spec_digest = hash('9'), Rejection::BindingMismatch),
        ("another executable", &|f| f.binding.target.executable_sha256 = hash('8'), Rejection::BindingMismatch),
        ("another provider", &|f| f.binding.target.provider = ProviderId::Claude, Rejection::BindingMismatch),
        ("a lifetime above the run's", &|f| f.ttl_ms = 300_001, Rejection::InvalidDraft),
        ("a skipped id", &|f| f.approval = a(3), Rejection::InvalidTransition),
        ("another run", &|f| f.run = RunId::new(8).unwrap(), Rejection::InvalidDraft),
    ];
    for (name, mutate, rejection) in cases {
        assert_eq!(journal.refused(Vec::from([shell(mutate)])), rejection, "{name}");
    }
    journal.one(shell(&|_| {}));
    assert_eq!(journal.approval(2), ApprovalState::Pending);
    // A start request for an attempt other than the one it is created with is not a valid pair.
    let mut journal = Journal::new();
    let mut pair = attempt_pair(1, 1, AT);
    pair[1] = attempt_pair(2, 1, AT).remove(1);
    assert_eq!(journal.refused(pair), Rejection::BatchShape);
}

#[test]
fn the_attempt_created_batch_is_exactly_the_attempt_and_its_start_request() {
    // F10 and batch rule 4.
    let mut journal = Journal::new();
    let pair = attempt_pair(1, 1, AT);
    assert_eq!(journal.refused(Vec::from([pair[0].clone()])), Rejection::BatchShape);
    assert_eq!(journal.refused(Vec::from([pair[1].clone()])), Rejection::BatchShape);
    assert_eq!(journal.refused(Vec::from([pair[1].clone(), pair[0].clone()])), Rejection::BatchShape);
    let mut three = pair.clone();
    three.push(decide(1, Decision::Granted, AT));
    assert_eq!(journal.refused(three), Rejection::BatchShape);
    journal.ok(pair);
    assert_eq!(journal.attempt(1), AttemptState::Proposed);
}

#[test]
fn a_batch_of_one_producer_cannot_mix_in_another() {
    let mut journal = running();
    assert_eq!(journal.refused(Vec::from([progress(1, AT), exited(1, 0, AT)])), Rejection::BatchShape);
    assert_eq!(journal.refused(Vec::from([stop(1, StopReason::JournalFull, AT), cancel(CancelScope::Attempt(n(1)), AT)])), Rejection::BatchShape);
    // Batch rule 3: a drafted event after a run-ending event is judged against the ended run.
    assert_eq!(journal.refused(Vec::from([exited(1, 0, AT), stop(1, StopReason::JournalFull, AT)])), Rejection::RunTerminal);
    assert_eq!(journal.refused(Vec::new()), Rejection::BatchShape);
    // Repairs first, then reconciliation, is the other mixed form; more than two repairs before it is not.
    let mut journal = running();
    journal.ok(Vec::from([repaired(1, AT), interrupted(1, InterruptObservation::Unknown, AT)]));
    let mut journal = running();
    let three = Vec::from([repaired(1, AT), repaired(2, AT), repaired(3, AT), interrupted(1, InterruptObservation::Unknown, AT)]);
    assert_eq!(journal.refused(three), Rejection::BatchShape);
}

#[test]
fn a_refused_batch_changes_nothing() {
    // The first draft is valid, the second is not: nothing of the batch is kept, the generated consequences included.
    let mut journal = running();
    journal.one(operation(2, 1, AT));
    let before = journal.projection.clone();
    let failed = supervisor(SupervisorDraft::StartFailed { attempt: n(1), code: StartFailCode::StartError }, AT);
    let rejection = journal.refused(Vec::from([stop(1, StopReason::Deadline(DeadlineKind::Run), AT), failed]));
    assert_eq!(rejection, Rejection::InvalidTransition);
    // A re-delivered fact after a new one is a partial duplicate, refused whole.
    assert_eq!(journal.refused(Vec::from([stop(1, StopReason::Deadline(DeadlineKind::Run), AT), started(1, AT)])), Rejection::PartialDuplicate);
    assert_eq!(journal.projection, before);
    assert_eq!(journal.approval(2), ApprovalState::Pending);
}

#[test]
fn duplicates_are_recognized_by_content_and_never_half_applied() {
    let mut journal = running();
    // Re-delivering the start with another stated time is a duplicate: nothing appended, the first time stands.
    let again = journal.ok(Vec::from([started(1, AT)]));
    assert!(again.duplicate && again.events.is_empty());
    // The same key with other content conflicts.
    let other = supervisor(SupervisorDraft::AttemptStarted { attempt: n(1), os_identity: OsIdentity::new(99, token("start-18"), hash('a')).unwrap() }, AT);
    assert!(matches!(journal.refused(Vec::from([other])), Rejection::Conflict { .. }));
    // Re-delivering the event that ended an attempt is a duplicate, not a terminal-state refusal.
    journal.one(exited(1, 0, T0 + 6_000));
    assert!(journal.ok(Vec::from([exited(1, 0, T0 + 9_000)])).duplicate);
    assert!(matches!(journal.refused(Vec::from([exited(1, 1, T0 + 9_000)])), Rejection::Conflict { .. }));
    // A batch mixing a duplicate with a new event is refused whole.
    let mut journal = running();
    journal.one(operation(2, 1, AT));
    let mixed = Vec::from([relay(RelayDraft::SessionObserved { attempt: n(1), session: token("s-1") }, AT), operation(2, 1, AT)]);
    assert_eq!(journal.refused(mixed), Rejection::PartialDuplicate);
    // The same fact twice in one batch conflicts with itself.
    let twice = Vec::from([relay(RelayDraft::SessionObserved { attempt: n(1), session: token("s-2") }, AT), relay(RelayDraft::SessionObserved { attempt: n(1), session: token("s-2") }, AT)]);
    assert!(matches!(journal.refused(twice), Rejection::Conflict { .. }));
}

#[test]
fn a_start_whose_two_keys_belong_to_different_events_conflicts() {
    // a1 is consumed by attempt 1's start; a StartBegun naming attempt 1 again with a fresh approval maps its keys to
    // one event and leaves the other free.
    let mut journal = starting();
    let draft = supervisor(SupervisorDraft::StartBegun { attempt: n(1), approval: a(2), binding: binding(OperationClass::Start) }, AT);
    assert!(matches!(journal.refused(Vec::from([draft])), Rejection::Conflict { .. }));
}

#[test]
fn consequences_are_generated_in_approval_order_with_the_cause_producer_and_reason() {
    let mut journal = running();
    journal.one(operation(2, 1, T0 + 5_000));
    journal.one(operation(3, 1, T0 + 5_000));
    journal.one(decide(3, Decision::Granted, T0 + 6_000));
    let applied = journal.one(exited(1, 0, T0 + 7_000));
    let bodies: Vec<&EventBody> = applied.events.iter().map(|event| &event.body).collect();
    assert_eq!(
        bodies[1..],
        [
            &EventBody::ApprovalWithdrawn { approval: a(2), reason: WithdrawalReason::AttemptEnded },
            &EventBody::ApprovalVoided { approval: a(3), reason: VoidReason::AttemptEnded },
        ]
    );
    for event in &applied.events[1..] {
        assert_eq!((event.producer, event.at_ms, event.clamped, event.v), (Producer::Supervisor, applied.events[0].at_ms, false, RECORD_VERSION));
    }
    assert_eq!(journal.attempt(1), AttemptState::Succeeded);
    assert_eq!(journal.replayed(), journal.projection);

    // A user cancel labels its consequences as the user's; a clock-fault stop and reconciliation give their reasons.
    let cases = [
        (cancel(CancelScope::Attempt(n(1)), AT), Producer::User, WithdrawalReason::Cancelled),
        (stop(1, StopReason::ClockFault, AT), Producer::Supervisor, WithdrawalReason::ClockFault),
        (interrupted(1, InterruptObservation::Unknown, AT), Producer::Reconciler, WithdrawalReason::SupervisionLost),
        (stop(1, StopReason::JournalFull, AT), Producer::Supervisor, WithdrawalReason::AttemptEnded),
    ];
    for (cause, producer, reason) in cases {
        let mut journal = running();
        journal.one(operation(2, 1, T0 + 5_000));
        let applied = journal.one(cause);
        assert_eq!(applied.events.len(), 2);
        assert_eq!(applied.events[1].producer, producer);
        assert_eq!(applied.events[1].body, EventBody::ApprovalWithdrawn { approval: a(2), reason });
        assert_eq!(journal.replayed(), journal.projection);
    }
}

#[test]
fn a_drafted_consequence_is_refused() {
    let mut journal = running();
    journal.one(operation(2, 1, T0 + 5_000));
    let drafted = supervisor(SupervisorDraft::ApprovalWithdrawn { approval: a(2), reason: WithdrawalReason::AttemptEnded }, AT);
    assert_eq!(journal.refused(Vec::from([exited(1, 0, AT), drafted])), Rejection::ConsequenceDrafted);
}

#[test]
fn recorded_consequences_must_be_exactly_the_generated_ones() {
    let mut journal = running();
    journal.one(operation(2, 1, T0 + 5_000));
    let before = journal.projection.clone();
    let frame = apply(&before, &[exited(1, 0, AT)], journal.size()).unwrap().events;
    assert!(apply_recorded(&before, &frame).is_ok());
    // Missing, altered or extra consequences are faults.
    assert_eq!(apply_recorded(&before, &frame[..1]), Err(JournalFault::ConsequenceMismatch));
    let mut altered = frame.clone();
    altered[1].body = EventBody::ApprovalWithdrawn { approval: a(2), reason: WithdrawalReason::Cancelled };
    assert_eq!(apply_recorded(&before, &altered), Err(JournalFault::ConsequenceMismatch));
    let mut clamped = frame.clone();
    clamped[1].clamped = true;
    assert_eq!(apply_recorded(&before, &clamped), Err(JournalFault::ConsequenceMismatch));
}

#[test]
fn a_start_approval_moves_its_attempt_only_alone_and_never_from_reconciliation() {
    // F10: a decision whose effect on its attempt would be suppressed by an attempt event in the same batch is refused,
    // unless that event ends the attempt.
    let mut journal = proposed();
    assert_eq!(journal.refused(Vec::from([decide(1, Decision::Granted, AT), begin(1, 1, AT)])), Rejection::InvalidTransition);
    let mut journal = proposed();
    journal.ok(Vec::from([decide(1, Decision::Granted, AT), cancel(CancelScope::Attempt(n(1)), AT)]));
    assert_eq!(journal.attempt(1), AttemptState::NotStarted(NotStartedReason::Cancelled));
    assert_eq!(journal.approval(1), ApprovalState::Voided);
    assert_eq!(journal.replayed(), journal.projection);
    // A reconciler void on an approved start grant ends the grant and leaves the attempt as it was.
    let mut journal = approved();
    journal.one(EventDraft::reconciler(ReconcilerDraft::ApprovalVoided { approval: a(1), reason: VoidReason::SupervisionLost }, AT));
    assert_eq!(journal.approval(1), ApprovalState::Voided);
    assert_eq!(journal.attempt(1), AttemptState::Approved);
}

#[test]
fn a_decision_is_bound_to_what_was_presented_and_to_its_window() {
    let mut journal = proposed();
    let provenance = DecisionProvenance { actor: Actor::User, channel: Channel::NativeConfirmation, presented_binding_digest: digest(9) };
    let other = EventDraft::user(UserDraft::ApprovalDecided { approval: a(1), decision: Decision::Granted, provenance }, AT);
    assert_eq!(journal.refused(Vec::from([other])), Rejection::BindingMismatch);
    // The request was recorded at T0 + 1 s with a 60 s lifetime: the last instant is accepted, the end is not.
    let mut late = proposed();
    assert_eq!(late.refused(Vec::from([decide(1, Decision::Granted, T0 + 61_000)])), Rejection::WindowClosed);
    let mut last = proposed();
    last.one(decide(1, Decision::Granted, T0 + 60_999));
    assert_eq!(last.attempt(1), AttemptState::Approved);
}

#[test]
fn a_grant_is_used_once_within_its_recorded_use_window() {
    // Decided at T0 + 2 s: usable until T0 + 62 s, never after.
    let mut late = approved();
    assert_eq!(late.refused(Vec::from([begin(1, 1, T0 + 62_000)])), Rejection::WindowClosed);
    let mut journal = approved();
    journal.one(begin(1, 1, T0 + 61_999));
    // The start consumed it; a second consumption is a conflict on its end key.
    assert_eq!(journal.approval(1), ApprovalState::Consumed);
    assert!(matches!(journal.refused(Vec::from([consumed(1, AT)])), Rejection::Conflict { .. }));
    // Another binding never matches.
    let mut journal = approved();
    let mut other = binding(OperationClass::Start);
    other.operation_digest = hash('7');
    let draft = supervisor(SupervisorDraft::StartBegun { attempt: n(1), approval: a(1), binding: other }, T0 + 3_000);
    assert_eq!(journal.refused(Vec::from([draft])), Rejection::BindingMismatch);
    // The use window never extends past 900 s after the request, however late the decision.
    let mut journal = Journal { projection: RunProjection::empty(run_id()), frames: Vec::new() };
    journal.one(EventDraft::user(UserDraft::RunCreated { spec: spec_with(8, 900_000), spec_digest: hash('b') }, T0));
    let request = request_with(1, 1, ApprovalKind::Start, OperationClass::Start, 900_000);
    journal.ok(Vec::from([
        EventDraft::user(UserDraft::AttemptCreated { attempt: n(1), continuation: Continuation::Fresh }, T0),
        supervisor(SupervisorDraft::StartRequested { request, description: description() }, T0),
    ]));
    journal.one(decide(1, Decision::Granted, T0 + 899_000));
    assert_eq!(journal.projection.approval(a(1)).unwrap().use_closes_at(), Some(T0 + 900_000));
    assert_eq!(journal.clone_refused(begin(1, 1, T0 + 900_000)), Rejection::WindowClosed);
    journal.one(begin(1, 1, T0 + 899_999));
}

impl Journal {
    fn clone_refused(&self, draft: EventDraft) -> Rejection {
        apply(&self.projection, &[draft], self.size()).err().unwrap()
    }
}

#[test]
fn an_operation_approval_is_consumed_only_by_its_running_attempt_with_its_binding() {
    let mut journal = running();
    journal.one(operation(2, 1, T0 + 5_000));
    assert!(matches!(journal.refused(Vec::from([consumed(2, T0 + 6_000)])), Rejection::InvalidTransition));
    journal.one(decide(2, Decision::Granted, T0 + 6_000));
    let other = supervisor(SupervisorDraft::ApprovalConsumed { approval: a(2), binding: binding(OperationClass::FileDelete) }, T0 + 7_000);
    assert_eq!(journal.refused(Vec::from([other])), Rejection::BindingMismatch);
    journal.one(consumed(2, T0 + 7_000));
    assert_eq!(journal.approval(2), ApprovalState::Consumed);
    // A start approval is never consumed as an operation.
    let mut journal = approved();
    let start = supervisor(SupervisorDraft::ApprovalConsumed { approval: a(1), binding: binding(OperationClass::Start) }, T0 + 3_000);
    assert_eq!(journal.refused(Vec::from([start])), Rejection::InvalidTransition);
}

#[test]
fn an_attempt_started_must_be_the_run_executable() {
    let mut journal = starting();
    let other = supervisor(SupervisorDraft::AttemptStarted { attempt: n(1), os_identity: OsIdentity::new(1, token("x"), hash('5')).unwrap() }, AT);
    assert_eq!(journal.refused(Vec::from([other])), Rejection::BindingMismatch);
    // A child that started while its stop was pending keeps its identity; then it can exit (F2).
    let mut journal = starting();
    journal.one(stop(1, StopReason::ClockFault, T0 + 5_000));
    assert_eq!(journal.refused(Vec::from([exited(1, 0, AT)])), Rejection::InvalidTransition);
    journal.ok(Vec::from([started(1, AT), exited(1, 0, AT)]));
    assert_eq!(journal.attempt(1), AttemptState::Stopped(StoppedKind::ClockFault));
    assert_eq!(journal.projection.attempt(n(1)).unwrap().identity(), Some(&identity()));
}

#[test]
fn the_first_stop_reason_decides_the_outcome() {
    let mut journal = running();
    journal.one(stop(1, StopReason::Deadline(DeadlineKind::Stall), T0 + 5_000));
    journal.one(cancel(CancelScope::Attempt(n(1)), T0 + 6_000));
    assert!(matches!(journal.refused(Vec::from([stop(1, StopReason::JournalFull, AT)])), Rejection::Conflict { .. }));
    journal.one(exited(1, 0, AT));
    assert_eq!(journal.attempt(1), AttemptState::TimedOut(DeadlineKind::Stall));
    // Cancel first, then a deadline: cancelled, even with a zero exit.
    let mut journal = running();
    journal.one(cancel(CancelScope::Attempt(n(1)), T0 + 5_000));
    journal.one(stop(1, StopReason::Deadline(DeadlineKind::Run), T0 + 6_000));
    journal.one(exited(1, 0, AT));
    assert_eq!(journal.attempt(1), AttemptState::Cancelled);
}

#[test]
fn a_provider_can_only_worsen_an_outcome() {
    // Rule W: a failure claim turns a later zero exit into a reported failure; nothing turns a failure into success.
    let mut journal = running();
    journal.one(relay(RelayDraft::OutcomeClaimed { attempt: n(1), claim: OutcomeClaim::Failure }, T0 + 5_000));
    journal.one(exited(1, 0, AT));
    assert_eq!(journal.attempt(1), AttemptState::Failed(FailureKind::ProviderReported));
    let mut journal = running();
    journal.one(exited(1, 2, AT));
    assert_eq!(journal.attempt(1), AttemptState::Failed(FailureKind::Exit));
    let signalled = supervisor(SupervisorDraft::AttemptExited { attempt: n(1), status: ExitStatus::new(None, Some(9)).unwrap() }, AT);
    let mut journal = running();
    journal.one(signalled);
    assert_eq!(journal.attempt(1), AttemptState::Failed(FailureKind::Exit));
}

#[test]
fn a_start_failure_never_started_and_raises_no_workspace_concern() {
    for code in [StartFailCode::SpawnRefused, StartFailCode::HashMismatch, StartFailCode::StartError, StartFailCode::EffectNotPerformed] {
        let mut journal = starting();
        journal.one(supervisor(SupervisorDraft::StartFailed { attempt: n(1), code }, AT));
        assert_eq!(journal.attempt(1), AttemptState::NotStarted(NotStartedReason::StartFailed(code)));
        assert!(journal.projection.concerns().is_empty());
        assert_eq!(journal.projection.started_attempts(), 0);
    }
    // Once a child's identity is recorded, a start failure is false.
    let mut journal = running();
    let failed = supervisor(SupervisorDraft::StartFailed { attempt: n(1), code: StartFailCode::StartError }, AT);
    assert_eq!(journal.refused(Vec::from([failed])), Rejection::InvalidTransition);
}

#[test]
fn an_interruption_raises_a_blocking_orphan_that_only_a_resolution_lifts() {
    let mut journal = running();
    journal.one(interrupted(1, InterruptObservation::NotRunning, AT));
    let kinds: Vec<ConcernKind> = journal.projection.concerns().iter().map(|concern| concern.concern().kind()).collect();
    assert_eq!(kinds, [ConcernKind::PossibleOrphan, ConcernKind::WorkspaceUnknown]);
    assert_eq!(journal.refused(attempt_pair(2, 2, AT)), Rejection::BlockingConcern);
    // Resolving the informational concern does not lift the block; resolving the orphan does.
    let workspace = journal.projection.concerns()[1].concern();
    journal.one(resolve(workspace, Resolution::Reviewed, AT));
    assert_eq!(journal.refused(attempt_pair(2, 2, AT)), Rejection::BlockingConcern);
    let orphan = journal.projection.concerns()[0].concern();
    journal.one(resolve(orphan, Resolution::StoppedOutsideOrdelis, AT));
    assert!(matches!(journal.refused(Vec::from([resolve(orphan, Resolution::AcceptedRisk, AT)])), Rejection::Conflict { .. }));
    journal.ok(attempt_pair(2, 2, AT));
    assert_eq!(journal.attempt(2), AttemptState::Proposed);
    // A resolution must name a raised concern exactly.
    let unknown = ConcernRef::new(ConcernKind::PossibleOrphan, Some(n(1)), Seq::new(3).unwrap()).unwrap();
    assert_eq!(journal.refused(Vec::from([resolve(unknown, Resolution::AcceptedRisk, AT)])), Rejection::InvalidTransition);
    assert_eq!(journal.replayed(), journal.projection);
}

#[test]
fn an_informational_concern_never_blocks_a_new_attempt() {
    let mut journal = running();
    journal.one(exited(1, 4, AT));
    assert_eq!(journal.projection.concerns().len(), 1);
    assert!(!journal.projection.has_unresolved_blocking_concern());
    journal.ok(attempt_pair(2, 2, AT));
}

#[test]
fn detected_repair_never_yields_never_started() {
    // Journal intact since the attempt's last change: supervision lost, never started.
    let mut journal = approved();
    journal.one(EventDraft::reconciler(ReconcilerDraft::AttemptNotStarted { attempt: n(1), reason: NotStartedCause::SupervisionLost }, AT));
    assert_eq!(journal.attempt(1), AttemptState::NotStarted(NotStartedReason::SupervisionLost));
    assert_eq!(journal.approval(1), ApprovalState::Voided);
    // A repair after it: the removed bytes may have held a start, so only evidence_lost is accepted.
    let mut journal = approved();
    let not_started = EventDraft::reconciler(ReconcilerDraft::AttemptNotStarted { attempt: n(1), reason: NotStartedCause::SupervisionLost }, AT);
    assert_eq!(journal.refused(Vec::from([repaired(1, AT), not_started])), Rejection::InvalidTransition);
    assert_eq!(journal.refused(Vec::from([interrupted(1, InterruptObservation::EvidenceLost, AT)])), Rejection::InvalidTransition);
    journal.ok(Vec::from([repaired(1, AT), interrupted(1, InterruptObservation::EvidenceLost, AT)]));
    assert_eq!(journal.attempt(1), AttemptState::Interrupted(InterruptObservation::EvidenceLost));
    assert!(journal.projection.has_unresolved_blocking_concern());
    assert_eq!(journal.replayed(), journal.projection);
    // An observation must fit what was recorded: no identity exactly when none was recorded.
    let mut journal = starting();
    assert_eq!(journal.refused(Vec::from([interrupted(1, InterruptObservation::PossiblyRunning, AT)])), Rejection::InvalidTransition);
    journal.one(interrupted(1, InterruptObservation::NoIdentity, AT));
    let mut journal = running();
    assert_eq!(journal.refused(Vec::from([interrupted(1, InterruptObservation::NoIdentity, AT)])), Rejection::InvalidTransition);
}

#[test]
fn run_outcomes_follow_their_attempts() {
    // One started attempt allowed: a failure exhausts the run as an observed failure.
    let mut journal = Journal { projection: RunProjection::empty(run_id()), frames: Vec::new() };
    journal.one(EventDraft::user(UserDraft::RunCreated { spec: spec_with(1, 300_000), spec_digest: hash('b') }, T0));
    journal.ok(attempt_pair(1, 1, T0));
    journal.one(decide(1, Decision::Granted, T0));
    journal.one(begin(1, 1, T0));
    journal.one(started(1, T0));
    journal.one(exited(1, 1, T0));
    assert_eq!(journal.run(), RunState::Exhausted(ExhaustedLast::ObservedFailure));
    assert_eq!(journal.refused(attempt_pair(2, 2, T0)), Rejection::RunTerminal);
    // Concerns can still be resolved and repairs recorded after the run ended; nothing else.
    let workspace = journal.projection.concerns()[0].concern();
    journal.one(resolve(workspace, Resolution::Reviewed, T0));
    journal.one(repaired(1, T0));
    assert_eq!(journal.refused(Vec::from([EventDraft::user(UserDraft::RunClosed { reason: CloseReason::ByUser }, T0)])), Rejection::RunTerminal);

    // Attempts that never started never exhaust the started budget, but the record budget ends the run as such.
    let mut journal = Journal::new();
    for attempt in 1..=MAX_ATTEMPT_RECORDS {
        journal.ok(attempt_pair(attempt, u64::from(attempt), T0));
        journal.one(decide(u64::from(attempt), Decision::Denied, T0));
    }
    assert_eq!(journal.run(), RunState::Exhausted(ExhaustedLast::NeverStarted));
    assert_eq!(journal.projection.started_attempts(), 0);

    // A run-scope cancel of the last possible attempt is a cancel, not an exhaustion (F12).
    let mut journal = Journal::new();
    for attempt in 1..MAX_ATTEMPT_RECORDS {
        journal.ok(attempt_pair(attempt, u64::from(attempt), T0));
        journal.one(decide(u64::from(attempt), Decision::Denied, T0));
    }
    journal.ok(attempt_pair(MAX_ATTEMPT_RECORDS, u64::from(MAX_ATTEMPT_RECORDS), T0));
    journal.one(cancel(CancelScope::Run, T0));
    assert_eq!(journal.run(), RunState::Cancelled);

    // Closing waits for the attempt; nothing new is created meanwhile.
    let mut journal = running();
    journal.one(cancel(CancelScope::Run, AT));
    assert_eq!(journal.run(), RunState::Closing);
    assert_eq!(journal.refused(attempt_pair(2, 2, AT)), Rejection::InvalidTransition);
    assert_eq!(journal.refused(Vec::from([EventDraft::user(UserDraft::RunClosed { reason: CloseReason::ByUser }, AT)])), Rejection::InvalidTransition);
    journal.one(exited(1, 0, AT));
    assert_eq!((journal.run(), journal.attempt(1)), (RunState::Cancelled, AttemptState::Cancelled));
    // A run with no attempt cancels or closes at once.
    let mut journal = Journal::new();
    journal.one(cancel(CancelScope::Run, AT));
    assert_eq!(journal.run(), RunState::Cancelled);
    let mut journal = Journal::new();
    journal.one(EventDraft::user(UserDraft::RunClosed { reason: CloseReason::ByUser }, AT));
    assert_eq!(journal.run(), RunState::ClosedByUser);
}

#[test]
fn an_unknown_outcome_is_never_a_failure() {
    let mut journal = Journal { projection: RunProjection::empty(run_id()), frames: Vec::new() };
    journal.one(EventDraft::user(UserDraft::RunCreated { spec: spec_with(1, 300_000), spec_digest: hash('b') }, T0));
    journal.ok(attempt_pair(1, 1, T0));
    journal.one(decide(1, Decision::Granted, T0));
    journal.one(begin(1, 1, T0));
    journal.one(started(1, T0));
    journal.one(interrupted(1, InterruptObservation::Unknown, T0));
    assert_eq!(journal.run(), RunState::Exhausted(ExhaustedLast::Unknown));
    assert_eq!(journal.attempt(1).outcome(), Some(OutcomeClass::Unknown));
    assert_eq!(journal.projection.started_attempts(), 1);
}

#[test]
fn a_resume_names_a_session_observed_earlier_in_the_run() {
    let mut journal = running();
    journal.one(relay(RelayDraft::SessionObserved { attempt: n(1), session: token("s-1") }, AT));
    journal.one(exited(1, 5, AT));
    let resume = |session: &str| {
        let mut pair = attempt_pair(2, 2, AT);
        pair[0] = EventDraft::user(UserDraft::AttemptCreated { attempt: n(2), continuation: Continuation::Resume { from_attempt: n(1), session: token(session) } }, AT);
        pair
    };
    assert_eq!(journal.refused(resume("s-9")), Rejection::InvalidTransition);
    journal.ok(resume("s-1"));
}

#[test]
fn wall_clock_readings_are_provenance_and_only_clamped() {
    let mut journal = running();
    // Back in time: the previous time is kept, marked clamped, and one clock concern is raised.
    let applied = journal.one(progress(1, T0));
    assert_eq!((applied.events[0].at_ms.ms(), applied.events[0].clamped), (T0 + 4_000, true));
    journal.one(relay(RelayDraft::SessionObserved { attempt: n(1), session: token("s-1") }, MAX_WALL_MS + 1));
    let clock: Vec<_> = journal.projection.concerns().iter().filter(|c| c.concern().kind() == ConcernKind::ClockIrregular).collect();
    assert_eq!(clock.len(), 1);
    // An absurd reading never blocks a stop or an end.
    journal.one(stop(1, StopReason::ClockFault, u64::MAX));
    let applied = journal.one(exited(1, 0, u64::MAX));
    assert!(applied.events[0].clamped);
    assert_eq!(journal.attempt(1), AttemptState::Stopped(StoppedKind::ClockFault));
    assert_eq!(journal.replayed(), journal.projection);
    // Once acknowledged, a new clamp raises a new concern.
    let concern = journal.projection.concerns().iter().find(|c| c.concern().kind() == ConcernKind::ClockIrregular).unwrap().concern();
    journal.one(resolve(concern, Resolution::Acknowledged, T0));
    let clock = journal.projection.concerns().iter().filter(|c| c.concern().kind() == ConcernKind::ClockIrregular).count();
    assert_eq!(clock, 2);
}

#[test]
fn progress_is_coalesced_by_recorded_time_live_and_in_replay() {
    let mut journal = running();
    assert_eq!(journal.one(progress(1, T0 + 10_000)).events.len(), 1);
    let coalesced = journal.one(progress(1, T0 + 19_999));
    assert!(coalesced.events.is_empty());
    assert_eq!(coalesced.coalesced, [Coalesced { index: 0, reason: CoalesceReason::ProgressWindow }]);
    assert_eq!(journal.one(progress(1, T0 + 20_000)).events.len(), 1);
    assert_eq!(journal.replayed(), journal.projection);
    // A stored progress record the rule would have coalesced is a fault.
    let before = journal.projection.clone();
    let mut frame = apply(&before, &[progress(1, T0 + 31_000)], journal.size()).unwrap().events;
    frame[0].at_ms = WallMs::new(T0 + 25_000).unwrap();
    assert_eq!(apply_recorded(&before, &frame), Err(JournalFault::CoalesceViolated));
}

#[test]
fn the_reserve_keeps_room_to_stop_end_and_resolve() {
    let journal = running();
    let reserve = JournalSize { events: MAX_EVENTS - RESERVED_EVENTS, bytes: 0 };
    let tried = |draft: EventDraft| apply(&journal.projection, &[draft], reserve);
    assert_eq!(tried(operation(2, 1, AT)).unwrap().coalesced, [Coalesced { index: 0, reason: CoalesceReason::Reserve }]);
    assert!(tried(progress(1, AT)).unwrap().events.is_empty());
    assert_eq!(tried(cancel(CancelScope::Run, AT)).unwrap().events.len(), 1);
    assert_eq!(tried(stop(1, StopReason::JournalFull, AT)).unwrap().events.len(), 1);
    assert_eq!(tried(exited(1, 0, AT)).unwrap().events.len(), 1);
    let proposed = proposed();
    assert_eq!(apply(&proposed.projection, &[decide(1, Decision::Granted, AT)], reserve).err(), Some(Rejection::ReserveOnly));
    // The byte reserve behaves the same; the hard limits refuse everything but a repair.
    let bytes = JournalSize { events: 10, bytes: MAX_JOURNAL_BYTES - RESERVED_JOURNAL_BYTES };
    assert_eq!(apply(&proposed.projection, &[decide(1, Decision::Granted, AT)], bytes).err(), Some(Rejection::ReserveOnly));
    let full = JournalSize { events: MAX_EVENTS, bytes: 0 };
    assert_eq!(tried_with(&journal, exited(1, 0, AT), full), Some(Rejection::LimitExceeded));
    assert_eq!(tried_with(&journal, repaired(1, AT), full), None);
    // A frame never exceeds the batch limit.
    let many: Vec<EventDraft> = (0..=MAX_BATCH).map(|i| relay(RelayDraft::SessionObserved { attempt: n(1), session: token(&format!("s-{i}")) }, AT)).collect();
    assert_eq!(apply(&journal.projection, &many, journal.size()).err(), Some(Rejection::LimitExceeded));
}

fn tried_with(journal: &Journal, draft: EventDraft, size: JournalSize) -> Option<Rejection> {
    apply(&journal.projection, &[draft], size).err()
}

#[test]
fn approvals_per_attempt_are_bounded() {
    let mut journal = running();
    for k in 2..2 + u64::from(MAX_OPEN_APPROVALS) {
        journal.one(operation(k, 1, AT));
    }
    let next = 2 + u64::from(MAX_OPEN_APPROVALS);
    assert_eq!(journal.refused(Vec::from([operation(next, 1, AT)])), Rejection::LimitExceeded);
    // Closing one makes room again.
    journal.one(decide(2, Decision::Denied, AT));
    journal.one(operation(next, 1, AT));
}

#[test]
fn the_recorded_envelope_is_checked() {
    let journal = running();
    let before = journal.projection.clone();
    let frame = apply(&before, &[progress(1, T0 + 5_000)], journal.size()).unwrap().events;
    let altered = |change: &dyn Fn(&mut Event)| {
        let mut frame = frame.clone();
        change(&mut frame[0]);
        apply_recorded(&before, &frame)
    };
    assert_eq!(altered(&|e| e.v = RECORD_VERSION + 1), Err(JournalFault::NewerVersion));
    assert_eq!(altered(&|e| e.v = 0), Err(JournalFault::Codec));
    assert_eq!(altered(&|e| e.run = RunId::new(8).unwrap()), Err(JournalFault::WrongRun));
    assert_eq!(altered(&|e| e.seq = Seq::new(9).unwrap()), Err(JournalFault::SeqGap));
    assert_eq!(altered(&|e| e.at_ms = WallMs::new(T0).unwrap()), Err(JournalFault::TimeDecreased));
    assert_eq!(altered(&|e| e.clamped = true), Err(JournalFault::ClampedMisused));
    assert_eq!(apply_recorded(&before, &[]), Err(JournalFault::Codec));
    let first = apply(&RunProjection::empty(run_id()), &[created(T0)], JournalSize { events: 0, bytes: 0 }).unwrap().events;
    let mut second = first.clone();
    second[0].seq = Seq::new(2).unwrap();
    assert_eq!(apply_recorded(&RunProjection::empty(run_id()), &second), Err(JournalFault::SeqNotFirst));
    // The same fact recorded twice, and an illegal transition, are faults.
    let mut journal = running();
    let started_again = journal.frames[4].clone();
    let mut duplicate = started_again.clone();
    duplicate[0].seq = Seq::new(journal.projection.events().len() as u64 + 1).unwrap();
    assert_eq!(apply_recorded(&journal.projection, &duplicate), Err(JournalFault::DuplicateKey));
    journal.one(exited(1, 0, AT));
    let mut late = apply(&running().projection, &[progress(1, AT)], running().size()).unwrap().events;
    late[0].seq = Seq::new(journal.projection.events().len() as u64 + 1).unwrap();
    assert_eq!(apply_recorded(&journal.projection, &late), Err(JournalFault::InvalidTransition));
}

#[test]
fn a_recorded_frame_of_mixed_producers_is_a_fault() {
    let journal = running();
    let before = journal.projection.clone();
    let mut frame = apply(&before, &[relay(RelayDraft::SessionObserved { attempt: n(1), session: token("s-1") }, AT)], journal.size()).unwrap().events;
    let mut next = apply(&apply_recorded(&before, &frame).unwrap(), &[exited(1, 0, AT)], journal.size()).unwrap().events;
    next[0].seq = Seq::new(frame[0].seq.number() + 1).unwrap();
    frame.extend(next);
    assert_eq!(apply_recorded(&before, &frame), Err(JournalFault::MixedProducerFrame));
}

#[test]
fn a_recorded_decision_is_history_and_mints_nothing() {
    // Replaying a granted decision rebuilds the granted state, and that is all: the projection has no grant, no
    // outstanding authority and no way to produce one (the live layer's tests show `stage` still requires a grant).
    let journal = approved();
    let replayed = journal.replayed();
    assert_eq!(replayed.approval(a(1)).unwrap().state(), ApprovalState::Granted);
    assert_eq!(replayed, journal.projection);
}
