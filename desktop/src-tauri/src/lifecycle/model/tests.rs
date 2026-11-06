// Lifecycle model tests. Pure: synthetic values only; nothing reads a file, a clock or a process.
use super::*;

const T0: u64 = 1_793_836_800_000;

fn hash(c: char) -> Sha256 {
    Sha256::new(&c.to_string().repeat(64)).unwrap()
}

fn token(text: &str) -> Token {
    Token::new(text).unwrap()
}

fn attempt(n: u8) -> AttemptNo {
    AttemptNo::new(n).unwrap()
}

fn approval(k: u64) -> ApprovalId {
    ApprovalId::new(k).unwrap()
}

fn seq(n: u64) -> Seq {
    Seq::new(n).unwrap()
}

fn limits() -> Limits {
    Limits::new(60_000, 3_600_000, 120_000, 300_000, 5_000).unwrap()
}

fn target() -> BindingTarget {
    BindingTarget { provider: ProviderId::Codex, executable_sha256: hash('a') }
}

fn binding(class: OperationClass) -> Binding {
    Binding { spec_digest: hash('b'), target: target(), operation_class: class, operation_digest: hash('c') }
}

fn fields(kind: ApprovalKind, class: OperationClass, claim: Option<RiskClass>, ttl_ms: u64) -> RequestFields {
    RequestFields {
        approval: approval(1),
        run: RunId::new(1).unwrap(),
        attempt: attempt(1),
        kind,
        binding: binding(class),
        binding_digest: hash('d'),
        description_sha256: hash('e'),
        provider_risk_claim: claim,
        ttl_ms,
    }
}

fn request(kind: ApprovalKind, class: OperationClass) -> ApprovalRequest {
    ApprovalRequest::new(fields(kind, class, None, 60_000), WorkspaceMode::None).unwrap()
}

fn spec() -> RunSpec {
    let task = TaskRef::new(TaskId::parse("t1").unwrap(), WallMs::new(T0).unwrap(), 1).unwrap();
    let project = ProjectRef { project: ProjectId::parse("p1").unwrap(), registered_at_ms: WallMs::new(T0).unwrap() };
    let context = RunContext::new(project, WorkspaceMode::None, Vec::new(), limits(), token("policy-1"), None).unwrap();
    let provider = ProviderIdentity { provider: ProviderId::Codex, executable_sha256: hash('a'), advertised_version: Some(token("1.2.3")) };
    RunSpec::new(task, provider, context, MAX_STARTED_ATTEMPTS).unwrap()
}

fn orphan(n: u8) -> ConcernRef {
    ConcernRef::new(ConcernKind::PossibleOrphan, Some(attempt(n)), seq(9)).unwrap()
}

fn provenance() -> ResolutionProvenance {
    ResolutionProvenance { actor: Actor::User, channel: Channel::NativeConfirmation, presented_sha256: hash('f') }
}

/// One body of every kind, in `EventKind::ALL` order.
fn every_body() -> Vec<EventBody> {
    let decided = DecisionProvenance { actor: Actor::User, channel: Channel::NativeConfirmation, presented_binding_digest: hash('d') };
    Vec::from([
        EventBody::RunCreated { spec: spec(), spec_digest: hash('b') },
        EventBody::AttemptCreated { attempt: attempt(1), continuation: Continuation::Fresh },
        EventBody::ApprovalRequested(request(ApprovalKind::Start, OperationClass::Start)),
        EventBody::ApprovalDecided { approval: approval(1), decision: Decision::Granted, provenance: decided },
        EventBody::ApprovalExpired { approval: approval(1), basis: ExpiryBasis::Monotonic },
        EventBody::ApprovalWithdrawn { approval: approval(1), reason: WithdrawalReason::ClockFault },
        EventBody::ApprovalVoided { approval: approval(1), reason: VoidReason::WriteUncertain },
        EventBody::ApprovalConsumed { approval: approval(2), binding: binding(OperationClass::ShellRun) },
        EventBody::StartBegun { attempt: attempt(1), approval: approval(1), binding: binding(OperationClass::Start) },
        EventBody::AttemptStarted { attempt: attempt(1), os_identity: OsIdentity::new(4242, token("start-17"), hash('a')).unwrap() },
        EventBody::StartFailed { attempt: attempt(1), code: StartFailCode::EffectNotPerformed },
        EventBody::StopRequested { attempt: attempt(1), reason: StopReason::Deadline(DeadlineKind::Stall) },
        EventBody::CancelRequested { scope: CancelScope::Run },
        EventBody::AttemptExited { attempt: attempt(1), status: ExitStatus::new(Some(0), None).unwrap() },
        EventBody::AttemptInterrupted { attempt: attempt(1), observation: InterruptObservation::EvidenceLost },
        EventBody::AttemptNotStarted { attempt: attempt(1), reason: NotStartedCause::SupervisionLost },
        EventBody::ProviderSessionObserved { attempt: attempt(1), session: token("session-1") },
        EventBody::Progress { attempt: attempt(1), kind: ProgressKind::Heartbeat },
        EventBody::ProviderOutcomeClaimed { attempt: attempt(1), claim: OutcomeClaim::Failure },
        EventBody::RunClosed { reason: CloseReason::ByUser },
        EventBody::AttentionResolved(ConcernResolution::new(orphan(1), Resolution::StoppedOutsideOrdelis, provenance()).unwrap()),
        EventBody::JournalRepaired(JournalRepair::new(0, 17, 0xdead_beef, RepairCause::IncompleteTail).unwrap()),
    ])
}

const PRODUCERS: [Producer; 5] = [Producer::User, Producer::Supervisor, Producer::ProviderRelay, Producer::Reconciler, Producer::Journal];

#[test]
fn ids_follow_the_task_and_registry_text_rules() {
    assert_eq!(RunId::parse("r1").unwrap().number(), 1);
    assert_eq!(RunId::parse("r9999999999").unwrap().number(), 9_999_999_999);
    assert_eq!(ApprovalId::parse("a42").unwrap().number(), 42);
    assert_eq!(TaskId::parse("t7").unwrap().number(), 7);
    assert_eq!(ProjectId::parse("p3").unwrap().number(), 3);
    for bad in ["", "r", "r0", "r01", "r12345678901", "a1", "R1", "r-1", "r1 ", "r1x", "r١"] {
        assert_eq!(RunId::parse(bad), Err(ModelError::InvalidId), "{bad:?}");
    }
    // Each kind of id has its own prefix: a task id never parses as a run id.
    assert_eq!(TaskId::parse("p1"), Err(ModelError::InvalidId));
    assert_eq!(RunId::new(0), Err(ModelError::InvalidId));
    assert_eq!(RunId::new(10_000_000_000), Err(ModelError::InvalidId));
    assert!(ApprovalId::new(9_999_999_999).is_ok());
}

#[test]
fn attempt_and_sequence_numbers_are_bounded() {
    assert_eq!(AttemptNo::new(0), Err(ModelError::InvalidNumber));
    assert_eq!(attempt(MAX_ATTEMPT_RECORDS).number(), 32);
    assert_eq!(AttemptNo::new(MAX_ATTEMPT_RECORDS + 1), Err(ModelError::InvalidNumber));
    assert_eq!(Seq::new(0), Err(ModelError::InvalidNumber));
    assert!(Seq::new(MAX_WALL_MS).is_ok());
    assert_eq!(Seq::new(MAX_WALL_MS + 1), Err(ModelError::InvalidNumber));
}

#[test]
fn wall_clock_values_are_bounded_and_monotonic_values_are_not_convertible() {
    assert_eq!(WallMs::new(MAX_WALL_MS).unwrap().ms(), MAX_WALL_MS);
    assert_eq!(WallMs::new(MAX_WALL_MS + 1), Err(ModelError::InvalidTime));
    assert_eq!(WallMs::new(0).unwrap().ms(), 0);
    // Any u64 is a valid monotonic reading; the two kinds of time share no constructor or conversion.
    assert_eq!(MonoMs::new(u64::MAX).ms(), u64::MAX);
    assert_eq!(LiveEpoch::new(1), LiveEpoch::new(1));
    assert_ne!(LiveEpoch::new(1), LiveEpoch::new(2));
}

#[test]
fn task_revisions_start_at_one() {
    let at = WallMs::new(T0).unwrap();
    let task = TaskId::parse("t1").unwrap();
    assert_eq!(TaskRef::new(task, at, 0), Err(ModelError::InvalidNumber));
    assert_eq!(TaskRef::new(task, at, MAX_WALL_MS + 1), Err(ModelError::InvalidNumber));
    let task_ref = TaskRef::new(task, at, 3).unwrap();
    assert_eq!((task_ref.task(), task_ref.created_at_ms(), task_ref.revision()), (task, at, 3));
}

#[test]
fn limits_enforce_every_bound_and_never_clamp() {
    let ok = limits();
    assert_eq!((ok.start_ms(), ok.run_ms(), ok.stall_ms(), ok.approval_ttl_ms(), ok.stop_grace_ms()), (60_000, 3_600_000, 120_000, 300_000, 5_000));
    // The inclusive edges are accepted (the stall limit never exceeds the run limit).
    assert!(Limits::new(1_000, 1_000, 30_000, 1_000, 0).is_err(), "stall above run");
    assert!(Limits::new(1_000, 30_000, 30_000, 1_000, 0).is_ok());
    assert!(Limits::new(600_000, 86_400_000, 86_400_000, 900_000, 60_000).is_ok());
    let cases = [
        ((999, 3_600_000, 120_000, 300_000, 5_000), LimitField::Start),
        ((600_001, 3_600_000, 120_000, 300_000, 5_000), LimitField::Start),
        ((60_000, 999, 120_000, 300_000, 5_000), LimitField::Run),
        ((60_000, 86_400_001, 120_000, 300_000, 5_000), LimitField::Run),
        // The stall limit stays compatible with progress coalescing, and never exceeds the run limit.
        ((60_000, 3_600_000, 3 * PROGRESS_COALESCE_MS - 1, 300_000, 5_000), LimitField::Stall),
        ((60_000, 100_000, 100_001, 300_000, 5_000), LimitField::Stall),
        ((60_000, 3_600_000, 120_000, 999, 5_000), LimitField::ApprovalTtl),
        ((60_000, 3_600_000, 120_000, MAX_APPROVAL_LIFETIME_MS + 1, 5_000), LimitField::ApprovalTtl),
        ((60_000, 3_600_000, 120_000, 300_000, 60_001), LimitField::StopGrace),
    ];
    for ((start, run, stall, ttl, grace), field) in cases {
        assert_eq!(Limits::new(start, run, stall, ttl, grace), Err(ModelError::InvalidLimits(field)), "{field:?}");
    }
}

#[test]
fn requested_permissions_are_sorted_and_unique() {
    let project = ProjectRef { project: ProjectId::parse("p1").unwrap(), registered_at_ms: WallMs::new(T0).unwrap() };
    let context = |permissions: Vec<PermissionClass>| RunContext::new(project, WorkspaceMode::None, permissions, limits(), token("policy-1"), None);
    assert!(context(Vec::from([PermissionClass::ReadInWorkspace, PermissionClass::ShellRun, PermissionClass::NetworkAccess])).is_ok());
    assert_eq!(context(Vec::from([PermissionClass::ShellRun, PermissionClass::ReadInWorkspace])), Err(ModelError::InvalidPermissions));
    assert_eq!(context(Vec::from([PermissionClass::ShellRun, PermissionClass::ShellRun])), Err(ModelError::InvalidPermissions));
}

#[test]
fn a_run_may_start_at_most_eight_attempts() {
    let base = spec();
    let again = |n| RunSpec::new(base.task(), base.provider().clone(), base.context().clone(), n);
    assert_eq!(again(0), Err(ModelError::InvalidMaxStartedAttempts));
    assert_eq!(again(1).unwrap().max_started_attempts(), 1);
    assert_eq!(again(MAX_STARTED_ATTEMPTS + 1), Err(ModelError::InvalidMaxStartedAttempts));
}

#[test]
fn a_grant_binds_the_executable_but_never_the_advertised_version() {
    let identity = spec().provider().clone();
    assert_eq!(identity.binding_target(), target());
    let other_version = ProviderIdentity { advertised_version: Some(token("9.9.9")), ..identity.clone() };
    assert_eq!(other_version.binding_target(), identity.binding_target());
    let other_file = ProviderIdentity { executable_sha256: hash('9'), ..identity.clone() };
    assert_ne!(other_file.binding_target(), identity.binding_target());
}

#[test]
fn risk_is_computed_by_ordelis_and_a_provider_can_only_raise_it() {
    // Without repository access every workspace operation is outside the workspace; unknown classes fail closed.
    assert_eq!(classify(OperationClass::Start, WorkspaceMode::None), RiskClass::High);
    for class in [
        OperationClass::ReadInWorkspace,
        OperationClass::WriteInWorkspace,
        OperationClass::ShellRun,
        OperationClass::FileDelete,
        OperationClass::OutsideWorkspace,
        OperationClass::NetworkAccess,
        OperationClass::PermissionChange,
        OperationClass::Other,
    ] {
        assert_eq!(classify(class, WorkspaceMode::None), RiskClass::Critical, "{class:?}");
    }
    let start = |claim| ApprovalRequest::new(fields(ApprovalKind::Start, OperationClass::Start, claim, 60_000), WorkspaceMode::None).unwrap().risk();
    assert_eq!(start(None), RiskClass::High);
    assert_eq!(start(Some(RiskClass::Elevated)), RiskClass::High, "a lower claim never lowers the risk");
    assert_eq!(start(Some(RiskClass::Critical)), RiskClass::Critical);
}

#[test]
fn a_start_request_is_exactly_a_request_for_the_start_operation() {
    let new = |kind, class| ApprovalRequest::new(fields(kind, class, None, 60_000), WorkspaceMode::None);
    assert_eq!(new(ApprovalKind::Start, OperationClass::ShellRun), Err(ModelError::KindClassMismatch));
    assert_eq!(new(ApprovalKind::Operation, OperationClass::Start), Err(ModelError::KindClassMismatch));
    let operation = new(ApprovalKind::Operation, OperationClass::ShellRun).unwrap();
    assert_eq!((operation.scope(), operation.risk()), (AuthorityScope::Once, RiskClass::Critical));
    assert_eq!(operation.fields().binding, binding(OperationClass::ShellRun));
}

#[test]
fn approval_lifetimes_are_bounded_by_the_absolute_cap() {
    let ttl = |ms| ApprovalRequest::new(fields(ApprovalKind::Start, OperationClass::Start, None, ms), WorkspaceMode::None);
    assert!(ttl(1_000).is_ok());
    assert!(ttl(MAX_APPROVAL_LIFETIME_MS).is_ok());
    assert_eq!(ttl(999), Err(ModelError::InvalidTtl));
    assert_eq!(ttl(MAX_APPROVAL_LIFETIME_MS + 1), Err(ModelError::InvalidTtl));
}

#[test]
fn operation_descriptions_are_bounded_text_without_control_characters() {
    assert_eq!(OperationDescription::new("rm -rf build\n\twith a tab").unwrap().as_str(), "rm -rf build\n\twith a tab");
    assert!(OperationDescription::new(&"x".repeat(MAX_DESCRIPTION_BYTES)).is_ok());
    for bad in [String::new(), "x".repeat(MAX_DESCRIPTION_BYTES + 1), "bell\u{7}".to_owned(), "escape\u{1b}[2J".to_owned(), "nul\0".to_owned(), "cr\r".to_owned(), "c1\u{85}".to_owned()] {
        assert_eq!(OperationDescription::new(&bad), Err(ModelError::InvalidDescription), "{bad:?}");
    }
    // The bound is in bytes, not characters.
    assert!(OperationDescription::new(&"é".repeat(MAX_DESCRIPTION_BYTES / 2)).is_ok());
    assert!(OperationDescription::new(&"é".repeat(MAX_DESCRIPTION_BYTES / 2 + 1)).is_err());
}

#[test]
fn process_identities_and_exit_statuses_are_well_formed() {
    assert_eq!(OsIdentity::new(0, token("s"), hash('a')), Err(ModelError::InvalidNumber));
    let identity = OsIdentity::new(1, token("s"), hash('a')).unwrap();
    assert_eq!((identity.pid(), identity.start_token(), identity.executable_sha256()), (1, &token("s"), &hash('a')));
    assert!(ExitStatus::new(Some(0), None).unwrap().is_zero_exit());
    assert!(!ExitStatus::new(Some(1), None).unwrap().is_zero_exit());
    assert!(!ExitStatus::new(None, Some(9)).unwrap().is_zero_exit());
    for (code, ended_by) in [(None, None), (Some(0), Some(9)), (None, Some(0)), (None, Some(128))] {
        assert_eq!(ExitStatus::new(code, ended_by), Err(ModelError::InvalidExitStatus), "{code:?} {ended_by:?}");
    }
}

#[test]
fn every_kind_has_exactly_the_producers_of_the_allow_list() {
    let bodies = every_body();
    assert_eq!(bodies.iter().map(EventBody::kind).collect::<Vec<_>>(), EventKind::ALL);
    let allowed = |body: &EventBody| PRODUCERS.into_iter().filter(|p| body.drafted_by(*p)).collect::<Vec<_>>();
    let expected: [&[Producer]; 22] = [
        &[Producer::User],
        &[Producer::User],
        &[Producer::Supervisor],
        &[Producer::User],
        &[Producer::Supervisor],
        &[Producer::Supervisor, Producer::Reconciler],
        &[Producer::Supervisor, Producer::Reconciler],
        &[Producer::Supervisor],
        &[Producer::Supervisor],
        &[Producer::Supervisor],
        &[Producer::Supervisor],
        &[Producer::Supervisor],
        &[Producer::User],
        &[Producer::Supervisor],
        &[Producer::Reconciler],
        &[Producer::Reconciler],
        &[Producer::ProviderRelay],
        &[Producer::ProviderRelay],
        &[Producer::ProviderRelay],
        &[Producer::User],
        &[Producer::User],
        &[Producer::Journal],
    ];
    for (body, expected) in bodies.iter().zip(expected) {
        assert_eq!(allowed(body), expected, "{:?}", body.kind());
    }
    // An operation request comes only from the provider relay; a start request never does.
    let operation = EventBody::ApprovalRequested(request(ApprovalKind::Operation, OperationClass::ShellRun));
    assert_eq!(allowed(&operation), [Producer::ProviderRelay]);
}

#[test]
fn the_provider_relay_can_decide_consume_end_or_resolve_nothing() {
    for body in every_body() {
        if body.drafted_by(Producer::ProviderRelay) {
            assert!(
                matches!(body.kind(), EventKind::ApprovalRequested | EventKind::ProviderSessionObserved | EventKind::Progress | EventKind::ProviderOutcomeClaimed),
                "{:?}",
                body.kind()
            );
        }
    }
    assert!(!EventKind::ProviderOutcomeClaimed.is_attempt_event());
}

#[test]
fn semantic_keys_identify_one_fact_each() {
    let keys: Vec<Vec<SemanticKey>> = every_body().iter().map(EventBody::semantic_keys).collect();
    let (n, k) = (attempt(1), approval(1));
    let expected: [Vec<SemanticKey>; 22] = [
        Vec::from([SemanticKey::Run]),
        Vec::from([SemanticKey::Attempt(n)]),
        Vec::from([SemanticKey::Approval(k)]),
        // A decision, an expiry and a withdrawal are mutually exclusive outcomes of one request.
        Vec::from([SemanticKey::ApprovalDecision(k)]),
        Vec::from([SemanticKey::ApprovalDecision(k)]),
        Vec::from([SemanticKey::ApprovalDecision(k)]),
        // A void and a consumption are mutually exclusive ends of one grant.
        Vec::from([SemanticKey::ApprovalEnd(k)]),
        Vec::from([SemanticKey::ApprovalEnd(approval(2))]),
        // StartBegun begins the attempt and consumes the start grant.
        Vec::from([SemanticKey::AttemptBegin(n), SemanticKey::ApprovalEnd(k)]),
        Vec::from([SemanticKey::AttemptStart(n)]),
        // Every way an attempt ends occupies one key, so two terminal events can never both be recorded.
        Vec::from([SemanticKey::AttemptEnd(n)]),
        // A stop is not an end: a stop and its exit are both recorded.
        Vec::from([SemanticKey::AttemptStop(n)]),
        Vec::from([SemanticKey::CancelRun]),
        Vec::from([SemanticKey::AttemptEnd(n)]),
        Vec::from([SemanticKey::AttemptEnd(n)]),
        Vec::from([SemanticKey::AttemptEnd(n)]),
        Vec::from([SemanticKey::AttemptSession(n, token("session-1"))]),
        // Progress is coalesced instead of keyed.
        Vec::new(),
        Vec::from([SemanticKey::AttemptClaim(n)]),
        Vec::from([SemanticKey::RunClose]),
        Vec::from([SemanticKey::Attention(orphan(1))]),
        Vec::from([SemanticKey::Repair(0)]),
    ];
    for (i, expected) in expected.iter().enumerate() {
        assert_eq!(&keys[i], expected, "{:?}", EventKind::ALL[i]);
    }
    // Different sessions of one attempt, and cancels of different scopes, are different facts.
    let session = |s| EventBody::ProviderSessionObserved { attempt: n, session: token(s) }.semantic_keys();
    assert_ne!(session("s1"), session("s2"));
    assert_eq!(EventBody::CancelRequested { scope: CancelScope::Attempt(n) }.semantic_keys(), [SemanticKey::CancelAttempt(n)]);
}

#[test]
fn the_reserve_keeps_room_to_stop_end_reconcile_and_resolve() {
    let reserved: Vec<EventKind> = EventKind::ALL.into_iter().filter(|k| k.reserved()).collect();
    assert_eq!(
        reserved,
        [
            EventKind::ApprovalExpired,
            EventKind::ApprovalWithdrawn,
            EventKind::ApprovalVoided,
            EventKind::AttemptStarted,
            EventKind::StartFailed,
            EventKind::StopRequested,
            EventKind::CancelRequested,
            EventKind::AttemptExited,
            EventKind::AttemptInterrupted,
            EventKind::AttemptNotStarted,
            EventKind::ProviderOutcomeClaimed,
            EventKind::RunClosed,
            EventKind::AttentionResolved,
        ]
    );
    // Nothing that creates, requests, decides, consumes or begins anything is reserved.
    for kind in [EventKind::RunCreated, EventKind::AttemptCreated, EventKind::ApprovalRequested, EventKind::ApprovalDecided, EventKind::ApprovalConsumed, EventKind::StartBegun] {
        assert!(!kind.reserved(), "{kind:?}");
    }
    // The safety tier (D3.2c-1) admits a subset of the reserved kinds: the ones that stop, start-record, end or reconcile
    // the attempt in flight, end its approvals, or report a provider failure. Not a resolution, a close or a repair by
    // kind (the reducer admits a blocking resolution and a reconciliation's repairs by content).
    let safety: Vec<EventKind> = EventKind::ALL.into_iter().filter(|k| k.safety()).collect();
    assert_eq!(
        safety,
        [
            EventKind::ApprovalExpired,
            EventKind::ApprovalWithdrawn,
            EventKind::ApprovalVoided,
            EventKind::AttemptStarted,
            EventKind::StartFailed,
            EventKind::StopRequested,
            EventKind::CancelRequested,
            EventKind::AttemptExited,
            EventKind::AttemptInterrupted,
            EventKind::AttemptNotStarted,
            EventKind::ProviderOutcomeClaimed,
        ]
    );
    assert!(safety.iter().all(|kind| kind.reserved()));
    let size = |events, bytes| JournalSize { events, bytes };
    assert!(!size(MAX_EVENTS - RESERVED_EVENTS - 1, 0).in_reserve());
    assert!(size(MAX_EVENTS - RESERVED_EVENTS, 0).in_reserve());
    assert!(!size(0, MAX_JOURNAL_BYTES - RESERVED_JOURNAL_BYTES - 1).in_reserve());
    assert!(size(0, MAX_JOURNAL_BYTES - RESERVED_JOURNAL_BYTES).in_reserve());
}

#[test]
fn attempt_events_are_the_only_events_that_move_an_attempt() {
    let movers: Vec<EventKind> = EventKind::ALL.into_iter().filter(|k| k.is_attempt_event()).collect();
    assert_eq!(
        movers,
        [
            EventKind::AttemptCreated,
            EventKind::StartBegun,
            EventKind::AttemptStarted,
            EventKind::StartFailed,
            EventKind::StopRequested,
            EventKind::CancelRequested,
            EventKind::AttemptExited,
            EventKind::AttemptInterrupted,
            EventKind::AttemptNotStarted,
        ]
    );
}

#[test]
fn outcomes_separate_failure_from_unknown_and_never_started() {
    let terminal = [
        (AttemptState::Succeeded, OutcomeClass::ObservedSuccess),
        (AttemptState::Failed(FailureKind::Exit), OutcomeClass::ObservedFailure),
        (AttemptState::Failed(FailureKind::ProviderReported), OutcomeClass::ObservedFailure),
        (AttemptState::Cancelled, OutcomeClass::ObservedStop),
        (AttemptState::TimedOut(DeadlineKind::Stall), OutcomeClass::ObservedStop),
        (AttemptState::Stopped(StoppedKind::ClockFault), OutcomeClass::ObservedStop),
        (AttemptState::Interrupted(InterruptObservation::NotRunning), OutcomeClass::Unknown),
        (AttemptState::Interrupted(InterruptObservation::EvidenceLost), OutcomeClass::Unknown),
        (AttemptState::NotStarted(NotStartedReason::Denied), OutcomeClass::NeverStarted),
        (AttemptState::NotStarted(NotStartedReason::SupervisionLost), OutcomeClass::NeverStarted),
        (AttemptState::NotStarted(NotStartedReason::StartFailed(StartFailCode::EffectNotPerformed)), OutcomeClass::NeverStarted),
    ];
    for (state, class) in terminal {
        assert_eq!(state.outcome(), Some(class), "{state:?}");
        assert!(state.is_terminal());
    }
    for state in [AttemptState::Proposed, AttemptState::Approved, AttemptState::Starting, AttemptState::Running, AttemptState::Stopping(StopCause::Cancel)] {
        assert_eq!(state.outcome(), None, "{state:?}");
    }
    // An interrupted attempt may have run (it counts against the budget), but it is not a failure and never a success.
    assert!(OutcomeClass::Unknown.may_have_run());
    assert!(!OutcomeClass::NeverStarted.may_have_run());
    assert!(OutcomeClass::Unknown.leaves_workspace_unknown());
    assert!(!OutcomeClass::NeverStarted.leaves_workspace_unknown());
    assert!(!OutcomeClass::ObservedSuccess.leaves_workspace_unknown());
    assert_eq!(ExhaustedLast::of(OutcomeClass::ObservedSuccess), None);
    assert_eq!(ExhaustedLast::of(OutcomeClass::Unknown), Some(ExhaustedLast::Unknown));
}

#[test]
fn run_and_approval_states_know_when_they_are_final() {
    assert!(!RunState::Open.is_terminal());
    assert!(!RunState::Closing.is_terminal());
    for state in [RunState::Succeeded, RunState::Exhausted(ExhaustedLast::NeverStarted), RunState::Cancelled, RunState::ClosedByUser] {
        assert!(state.is_terminal(), "{state:?}");
    }
    let open: Vec<ApprovalState> = [
        ApprovalState::Pending,
        ApprovalState::Granted,
        ApprovalState::Denied,
        ApprovalState::Expired,
        ApprovalState::Withdrawn,
        ApprovalState::Consumed,
        ApprovalState::Voided,
    ]
    .into_iter()
    .filter(|s| s.is_open())
    .collect();
    assert_eq!(open, [ApprovalState::Pending, ApprovalState::Granted]);
}

#[test]
fn only_an_orphan_blocks_and_each_concern_has_its_own_resolutions() {
    assert_eq!(ConcernKind::PossibleOrphan.class(), ConcernClass::Blocking);
    for kind in [ConcernKind::WorkspaceUnknown, ConcernKind::JournalRepaired, ConcernKind::ClockIrregular] {
        assert_eq!(kind.class(), ConcernClass::Informational, "{kind:?}");
    }
    let all = [Resolution::ConfirmedGone, Resolution::StoppedOutsideOrdelis, Resolution::AcceptedRisk, Resolution::Reviewed, Resolution::Acknowledged];
    let allowed = |kind: ConcernKind| all.into_iter().filter(|r| kind.allows(*r)).collect::<Vec<_>>();
    assert_eq!(allowed(ConcernKind::PossibleOrphan), [Resolution::ConfirmedGone, Resolution::StoppedOutsideOrdelis, Resolution::AcceptedRisk]);
    assert_eq!(allowed(ConcernKind::WorkspaceUnknown), [Resolution::AcceptedRisk, Resolution::Reviewed]);
    assert_eq!(allowed(ConcernKind::JournalRepaired), [Resolution::Acknowledged]);
    assert_eq!(allowed(ConcernKind::ClockIrregular), [Resolution::Acknowledged]);
    // "Acknowledged" never closes an orphan.
    assert_eq!(ConcernResolution::new(orphan(1), Resolution::Acknowledged, provenance()), Err(ModelError::ResolutionNotAllowed));
    let resolved = ConcernResolution::new(orphan(2), Resolution::AcceptedRisk, provenance()).unwrap();
    assert_eq!((resolved.concern(), resolved.resolution(), resolved.provenance()), (orphan(2), Resolution::AcceptedRisk, &provenance()));
}

#[test]
fn a_concern_names_an_attempt_exactly_when_its_kind_is_about_one() {
    assert_eq!(ConcernRef::new(ConcernKind::PossibleOrphan, None, seq(1)), Err(ModelError::InvalidConcernRef));
    assert_eq!(ConcernRef::new(ConcernKind::JournalRepaired, Some(attempt(1)), seq(1)), Err(ModelError::InvalidConcernRef));
    let clock = ConcernRef::new(ConcernKind::ClockIrregular, None, seq(5)).unwrap();
    assert_eq!((clock.kind(), clock.attempt(), clock.raised_at()), (ConcernKind::ClockIrregular, None, seq(5)));
    // The same kind raised by different events is a different concern.
    assert_ne!(ConcernRef::new(ConcernKind::PossibleOrphan, Some(attempt(1)), seq(9)), ConcernRef::new(ConcernKind::PossibleOrphan, Some(attempt(1)), seq(10)));
}

#[test]
fn a_repair_removes_at_most_one_frame_worth_of_bytes() {
    assert_eq!(JournalRepair::new(0, 0, 0, RepairCause::IncompleteTail), Err(ModelError::InvalidNumber));
    assert_eq!(JournalRepair::new(0, MAX_TAIL_BYTES + 1, 0, RepairCause::IncompleteTail), Err(ModelError::InvalidNumber));
    let record = JournalRepair::new(3, MAX_TAIL_BYTES, 7, RepairCause::IncompleteTail).unwrap();
    assert_eq!((record.sidecar(), record.removed_bytes(), record.removed_crc32c(), record.cause()), (3, MAX_TAIL_BYTES, 7, RepairCause::IncompleteTail));
}

#[test]
fn duplicate_detection_ignores_when_and_where_a_fact_was_recorded() {
    let event = |seq_n, at, clamped, producer| Event {
        v: RECORD_VERSION,
        run: RunId::new(1).unwrap(),
        seq: seq(seq_n),
        at_ms: WallMs::new(at).unwrap(),
        clamped,
        producer,
        body: EventBody::StopRequested { attempt: attempt(1), reason: StopReason::JournalFull },
    };
    let first = event(4, T0, false, Producer::Supervisor);
    assert!(first.same_content(&event(9, T0 + 5, true, Producer::Supervisor)));
    assert!(!first.same_content(&event(4, T0, false, Producer::User)));
    let mut other = first.clone();
    other.body = EventBody::StopRequested { attempt: attempt(1), reason: StopReason::ClockFault };
    assert!(!first.same_content(&other));
}

#[test]
fn the_limits_fit_together() {
    // The largest frame (two repair records, one attempt event and its consequences) fits one batch, and a batch the
    // reducer accepts always encodes into one frame and one repairable tail.
    assert!(MAX_BATCH >= 3 + usize::from(MAX_OPEN_APPROVALS));
    assert!(u64::from(MAX_FRAME_BYTES) >= MAX_BATCH as u64 * u64::from(MAX_EVENT_BYTES) + MAX_BATCH as u64 + 1);
    // "ORDJ1 ", five length digits, a space, eight hex digits, a space and the newline.
    assert_eq!(MAX_TAIL_BYTES - MAX_FRAME_BYTES, 6 + 5 + 1 + 8 + 1 + 1);
    assert!(MAX_FRAME_BYTES < 100_000, "the length field has at most five digits");
    // The reserve holds a stop, an exit, a reconciliation and resolutions with their consequences (under 25 events).
    assert!(RESERVED_EVENTS >= 25);
    assert!(RESERVED_JOURNAL_BYTES >= 25 * u64::from(MAX_FRAME_BYTES));
    // The safety tier (D3.2c-1) holds everything a run can still record once the journal is past MAX_EVENTS: stop, two
    // cancels, start record, failure claim, end, MAX_OPEN_APPROVALS approval endings and two orphan resolutions (12
    // events), plus every repair the run may still record (at most MAX_REPAIRS), in events and in bytes, each event in
    // its own frame.
    let need = 1 + 2 + 1 + 1 + 1 + u32::from(MAX_OPEN_APPROVALS) + 2;
    assert!(need + MAX_REPAIRS <= SAFETY_EVENTS);
    assert!(SAFETY_JOURNAL_BYTES >= u64::from(SAFETY_EVENTS) * u64::from(MAX_TAIL_BYTES - MAX_FRAME_BYTES + MAX_EVENT_BYTES + 2));
    // Reconciliation's own repair bound (two owed per opening) fits under the per-run cap.
    assert!(MAX_REPAIRS >= 2);
    // Nothing is accepted past the absolute ceilings, which stay finite and below the codec's stream bound.
    assert!(u64::from(MAX_EVENTS + SAFETY_EVENTS) < MAX_WALL_MS);
    assert!(MAX_JOURNAL_BYTES + SAFETY_JOURNAL_BYTES + 2 * u64::from(MAX_TAIL_BYTES) <= crate::lifecycle::codec::MAX_STREAM_BYTES);
    assert!(MAX_OPEN_APPROVALS <= MAX_APPROVALS_PER_ATTEMPT);
    assert!(MAX_STARTED_ATTEMPTS <= MAX_ATTEMPT_RECORDS);
    assert!(GRANT_USE_MS < MAX_APPROVAL_LIFETIME_MS);
    assert!(MAX_OBSERVATION_AGE_MS < PROGRESS_COALESCE_MS);
    assert_eq!(MAX_WALL_MS, crate::agents::evidence::MAX_TIMESTAMP_MS);
    assert_eq!(RECORD_VERSION, 1);
}
