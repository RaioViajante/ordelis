// Codec tests (D3.2b-4). Pure: every journal is synthetic bytes built here; nothing reads a file, a clock or a process.
// Includes the golden v1 fixtures and a seeded property test (decision 12.14: hand-rolled SplitMix64, no dependency).
use super::*;
use crate::lifecycle::model::*;
use crate::lifecycle::reduce::tests::*;
use crate::lifecycle::reduce::{apply_recorded, RunProjection};

// ---------------------------------------------------------------------------------------------------------------
// Shared helpers (also used by replay/tests.rs)

/// SplitMix64: a seeded, reproducible generator.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub(crate) fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            0
        } else {
            self.next() % bound
        }
    }

    pub(crate) fn percent(&mut self, chance: u64) -> bool {
        self.below(100) < chance
    }
}

/// The bytes of a journal kept as frames.
pub(crate) fn stream(frames: &[Vec<Event>]) -> Vec<u8> {
    frames.iter().flat_map(|frame| encode_frame(frame).unwrap()).collect()
}

/// The frames a decoded stream holds, as event lists.
pub(crate) fn frames_of(decoded: &Decoded) -> Vec<Vec<Event>> {
    decoded.frames().iter().map(|frame| frame.events().to_vec()).collect()
}

/// Re-frames `payload` with a correct header and checksum, so that only the payload is under test.
pub(crate) fn reframe(payload: &[u8]) -> Vec<u8> {
    let digits = payload.len().to_string().into_bytes();
    let mut frame = MAGIC.to_vec();
    frame.extend_from_slice(&digits);
    frame.push(b' ');
    frame.extend_from_slice(&hex8(frame_crc(&digits, payload)));
    frame.push(b' ');
    frame.extend_from_slice(payload);
    frame.push(b'\n');
    frame
}

/// The payload of a single encoded frame.
pub(crate) fn payload_of(frame: &[u8]) -> Vec<u8> {
    let start = frame.iter().enumerate().filter(|(_, b)| **b == b' ').nth(2).unwrap().0 + 1;
    frame[start..frame.len() - 1].to_vec()
}

/// A journal driven by random drafts from every producer, with random (sometimes backward or absurd) times. Refused
/// drafts are skipped. Returns the journal and the projection after each recorded frame.
pub(crate) fn random_journal(seed: u64, steps: usize) -> (Journal, Vec<RunProjection>) {
    let mut rng = Rng::new(seed);
    let mut journal = Journal::new();
    let mut after = Vec::from([journal.projection.clone()]);
    let mut at = T0;
    for _ in 0..steps {
        at = match rng.below(100) {
            0..=4 => at.saturating_sub(rng.below(20_000)),
            5 | 6 => u64::MAX - rng.below(10),
            _ => at + rng.below(15_000),
        }
        .min(MAX_WALL_MS - 1_000_000);
        let stated = if rng.percent(3) { u64::MAX } else { at };
        let batch = random_batch(&mut rng, &journal.projection, stated);
        let before = journal.frames.len();
        if journal.record(batch).is_ok() && journal.frames.len() > before {
            after.push(journal.projection.clone());
        }
    }
    (journal, after)
}

fn random_batch(rng: &mut Rng, projection: &RunProjection, at: u64) -> Vec<EventDraft> {
    let attempts = projection.attempts().len() as u8;
    let current = projection.current_attempt().map_or(attempts.max(1), |record| record.attempt().number());
    let next_k = projection.approvals().len() as u64 + 1;
    let mut any_k = 1 + rng.below(next_k.saturating_sub(1).max(1));
    // Mostly the approval a well-behaved caller would name, sometimes any.
    let latest = |state: ApprovalState| projection.approvals().iter().rev().find(|record| record.state() == state).map(|record| record.id().number());
    let pick = |rng: &mut Rng, wanted: Option<u64>, any: u64| if rng.percent(80) { wanted.unwrap_or(any) } else { any };
    let one = |draft: EventDraft| Vec::from([draft]);
    // A granted operation request is usually consumed promptly, as a provider would.
    let granted_operation = projection
        .approvals()
        .iter()
        .find(|record| record.state() == ApprovalState::Granted && record.kind() == ApprovalKind::Operation)
        .map(|record| record.id().number());
    if let (Some(k), true) = (granted_operation, rng.percent(40)) {
        return one(consumed(k, at));
    }
    match rng.below(24) {
        0 | 1 if attempts < MAX_ATTEMPT_RECORDS => attempt_pair(attempts + 1, next_k, at),
        2..=4 => {
            any_k = pick(rng, latest(ApprovalState::Pending), any_k);
            one(decide(any_k, if rng.percent(80) { Decision::Granted } else { Decision::Denied }, at))
        }
        5 | 6 => one(begin(current, pick(rng, latest(ApprovalState::Granted), any_k), at)),
        7 | 8 => one(started(current, at)),
        9 => one(progress(current, at)),
        10 => one(operation(next_k, current, at)),
        11 => one(consumed(pick(rng, latest(ApprovalState::Granted), any_k), at)),
        12 => one(exited(current, [0, 1, -1][rng.below(3) as usize], at)),
        13 => one(stop(current, STOP_REASONS[rng.below(5) as usize], at)),
        14 => one(cancel(if rng.percent(50) { CancelScope::Attempt(n(current)) } else { CancelScope::Run }, at)),
        15 => one(relay(RelayDraft::OutcomeClaimed { attempt: n(current), claim: OutcomeClaim::Failure }, at)),
        16 => one(relay(RelayDraft::SessionObserved { attempt: n(current), session: token("session-1") }, at)),
        17 => one(supervisor(SupervisorDraft::ApprovalExpired { approval: a(any_k), basis: EXPIRY_BASES[rng.below(3) as usize] }, at)),
        18 => one(supervisor(SupervisorDraft::StartFailed { attempt: n(current), code: START_FAILURES[rng.below(4) as usize] }, at)),
        19 => one(repaired(rng.below(4) as u32, at)),
        20 => match projection.concerns().iter().find(|concern| concern.resolution().is_none()) {
            Some(concern) => {
                let kind = concern.concern().kind();
                let resolution = RESOLUTIONS.into_iter().find(|resolution| kind.allows(*resolution)).unwrap();
                one(resolve(concern.concern(), resolution, at))
            }
            None => one(progress(current, at)),
        },
        21 => one(interrupted(current, OBSERVATIONS[rng.below(5) as usize], at)),
        22 => one(EventDraft::reconciler(ReconcilerDraft::AttemptNotStarted { attempt: n(current), reason: NotStartedCause::SupervisionLost }, at)),
        _ => one(EventDraft::user(UserDraft::RunClosed { reason: CloseReason::ByUser }, at)),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Golden v1 histories. Built through the reducer; their bytes are pinned below.

/// A supervised run: start consent, a running attempt with a session, progress, an operation consumed, one expired and
/// one granted but unused (voided when the run deadline stops the attempt), a failure claim and an exit; an attempt
/// cancelled before its start (its pending request withdrawn); an attempt whose start failed; a resolution; the close.
pub(crate) fn golden_supervised() -> Journal {
    let mut journal = Journal::new();
    journal.ok(attempt_pair(1, 1, T0 + 1_000));
    journal.one(decide(1, Decision::Granted, T0 + 2_000));
    journal.one(begin(1, 1, T0 + 3_000));
    journal.one(started(1, T0 + 4_000));
    journal.one(relay(RelayDraft::SessionObserved { attempt: n(1), session: token("session-1") }, T0 + 5_000));
    journal.one(progress(1, T0 + 6_000));
    journal.one(operation(2, 1, T0 + 7_000));
    journal.one(decide(2, Decision::Granted, T0 + 8_000));
    journal.one(consumed(2, T0 + 9_000));
    journal.one(operation(3, 1, T0 + 10_000));
    journal.one(supervisor(SupervisorDraft::ApprovalExpired { approval: a(3), basis: ExpiryBasis::Monotonic }, T0 + 71_000));
    journal.one(operation(4, 1, T0 + 72_000));
    journal.one(decide(4, Decision::Granted, T0 + 73_000));
    journal.one(relay(RelayDraft::OutcomeClaimed { attempt: n(1), claim: OutcomeClaim::Failure }, T0 + 74_000));
    journal.one(stop(1, StopReason::Deadline(DeadlineKind::Run), T0 + 75_000));
    journal.one(exited(1, 0, T0 + 76_000));
    journal.ok(attempt_pair(2, 5, T0 + 77_000));
    journal.one(cancel(CancelScope::Attempt(n(2)), T0 + 78_000));
    journal.ok(attempt_pair(3, 6, T0 + 79_000));
    journal.one(decide(6, Decision::Granted, T0 + 80_000));
    journal.one(begin(3, 6, T0 + 81_000));
    journal.one(supervisor(SupervisorDraft::StartFailed { attempt: n(3), code: StartFailCode::HashMismatch }, T0 + 82_000));
    let concern = journal.projection.concerns()[0].concern();
    journal.one(resolve(concern, Resolution::Reviewed, T0 + 83_000));
    journal.one(EventDraft::user(UserDraft::RunClosed { reason: CloseReason::ByUser }, T0 + 84_000));
    journal
}

/// A run reconciled after a crash with a recorded repair: the approved attempt becomes `interrupted{evidence_lost}` and
/// its unused start grant is voided (`supervision_lost`), with the repair, orphan and workspace concerns.
pub(crate) fn golden_reconciled() -> Journal {
    let mut journal = Journal::new();
    journal.ok(attempt_pair(1, 1, T0 + 1_000));
    journal.one(decide(1, Decision::Granted, T0 + 2_000));
    journal.ok(Vec::from([repaired(1, T0 + 5_000), interrupted(1, InterruptObservation::EvidenceLost, T0 + 5_000)]));
    journal
}

/// A run reconciled with an intact journal: the proposed attempt never started, and its pending request is withdrawn.
pub(crate) fn golden_not_started() -> Journal {
    let mut journal = Journal::new();
    journal.ok(attempt_pair(1, 1, T0 + 1_000));
    journal.one(EventDraft::reconciler(ReconcilerDraft::AttemptNotStarted { attempt: n(1), reason: NotStartedCause::SupervisionLost }, T0 + 5_000));
    journal
}

pub(crate) fn golden_bytes(lines: &[&str]) -> Vec<u8> {
    lines.iter().flat_map(|line| line.bytes().chain([b'\n'])).collect()
}

// The pinned v1 bytes of the three golden histories, one frame per line (each line is followed by a newline in the
// journal). Synthetic: no credential, host data or real process. These never change once v1 freezes (r5 4.10); until
// then a change is a reviewed format change.
const GOLDEN_SUPERVISED: &[&str] = &[
    r#"ORDJ1 712 da654527 [{"v":1,"run":"r7","seq":1,"at_ms":1793836800000,"clamped":false,"producer":"user","kind":"run_created","body":{"spec":{"task":{"task":"t1","created_at_ms":1793836800000,"revision":1},"provider":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","advertised_version":"1.2.3"},"context":{"project":{"project":"p1","registered_at_ms":1793836800000},"workspace":"none","permissions":[],"limits":{"start_ms":60000,"run_ms":3600000,"stall_ms":120000,"approval_ttl_ms":300000,"stop_grace_ms":5000},"policy_version":"policy-1","environment_sha256":null},"max_started_attempts":8},"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}]"#,
    r#"ORDJ1 933 607595de [{"v":1,"run":"r7","seq":2,"at_ms":1793836801000,"clamped":false,"producer":"user","kind":"attempt_created","body":{"attempt":1,"continuation":"fresh","from_attempt":null,"session":null}},{"v":1,"run":"r7","seq":3,"at_ms":1793836801000,"clamped":false,"producer":"supervisor","kind":"approval_requested","body":{"approval":"a1","run":"r7","attempt":1,"kind":"start","binding":{"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","target":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"operation_class":"start","operation_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},"binding_digest":"0000000000000000000000000000000000000000000000000000000000000001","description_sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","scope":"once","risk":"high","provider_risk_claim":null,"ttl_ms":60000}}]"#,
    r#"ORDJ1 312 d2905210 [{"v":1,"run":"r7","seq":4,"at_ms":1793836802000,"clamped":false,"producer":"user","kind":"approval_decided","body":{"approval":"a1","decision":"granted","provenance":{"actor":"user","channel":"native_confirmation","presented_binding_digest":"0000000000000000000000000000000000000000000000000000000000000001"}}}]"#,
    r#"ORDJ1 470 3731f8d8 [{"v":1,"run":"r7","seq":5,"at_ms":1793836803000,"clamped":false,"producer":"supervisor","kind":"start_begun","body":{"attempt":1,"approval":"a1","binding":{"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","target":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"operation_class":"start","operation_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}}}]"#,
    r#"ORDJ1 275 3e988f19 [{"v":1,"run":"r7","seq":6,"at_ms":1793836804000,"clamped":false,"producer":"supervisor","kind":"attempt_started","body":{"attempt":1,"os_identity":{"pid":4242,"start_token":"start-17","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}]"#,
    r#"ORDJ1 172 fee1d22f [{"v":1,"run":"r7","seq":7,"at_ms":1793836805000,"clamped":false,"producer":"provider_relay","kind":"provider_session_observed","body":{"attempt":1,"session":"session-1"}}]"#,
    r#"ORDJ1 152 b36d61a1 [{"v":1,"run":"r7","seq":8,"at_ms":1793836806000,"clamped":false,"producer":"provider_relay","kind":"progress","body":{"attempt":1,"kind":"heartbeat"}}]"#,
    r#"ORDJ1 762 bcc628aa [{"v":1,"run":"r7","seq":9,"at_ms":1793836807000,"clamped":false,"producer":"provider_relay","kind":"approval_requested","body":{"approval":"a2","run":"r7","attempt":1,"kind":"operation","binding":{"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","target":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"operation_class":"shell_run","operation_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},"binding_digest":"0000000000000000000000000000000000000000000000000000000000000002","description_sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","scope":"once","risk":"critical","provider_risk_claim":null,"ttl_ms":60000}}]"#,
    r#"ORDJ1 313 bfb12e3f [{"v":1,"run":"r7","seq":10,"at_ms":1793836808000,"clamped":false,"producer":"user","kind":"approval_decided","body":{"approval":"a2","decision":"granted","provenance":{"actor":"user","channel":"native_confirmation","presented_binding_digest":"0000000000000000000000000000000000000000000000000000000000000002"}}}]"#,
    r#"ORDJ1 469 30e60548 [{"v":1,"run":"r7","seq":11,"at_ms":1793836809000,"clamped":false,"producer":"supervisor","kind":"approval_consumed","body":{"approval":"a2","binding":{"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","target":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"operation_class":"shell_run","operation_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}}}]"#,
    r#"ORDJ1 763 3376fdf9 [{"v":1,"run":"r7","seq":12,"at_ms":1793836810000,"clamped":false,"producer":"provider_relay","kind":"approval_requested","body":{"approval":"a3","run":"r7","attempt":1,"kind":"operation","binding":{"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","target":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"operation_class":"shell_run","operation_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},"binding_digest":"0000000000000000000000000000000000000000000000000000000000000003","description_sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","scope":"once","risk":"critical","provider_risk_claim":null,"ttl_ms":60000}}]"#,
    r#"ORDJ1 162 08cc6e70 [{"v":1,"run":"r7","seq":13,"at_ms":1793836871000,"clamped":false,"producer":"supervisor","kind":"approval_expired","body":{"approval":"a3","basis":"monotonic"}}]"#,
    r#"ORDJ1 763 0fd4def4 [{"v":1,"run":"r7","seq":14,"at_ms":1793836872000,"clamped":false,"producer":"provider_relay","kind":"approval_requested","body":{"approval":"a4","run":"r7","attempt":1,"kind":"operation","binding":{"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","target":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"operation_class":"shell_run","operation_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},"binding_digest":"0000000000000000000000000000000000000000000000000000000000000004","description_sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","scope":"once","risk":"critical","provider_risk_claim":null,"ttl_ms":60000}}]"#,
    r#"ORDJ1 313 a036561a [{"v":1,"run":"r7","seq":15,"at_ms":1793836873000,"clamped":false,"producer":"user","kind":"approval_decided","body":{"approval":"a4","decision":"granted","provenance":{"actor":"user","channel":"native_confirmation","presented_binding_digest":"0000000000000000000000000000000000000000000000000000000000000004"}}}]"#,
    r#"ORDJ1 168 1851a66c [{"v":1,"run":"r7","seq":16,"at_ms":1793836874000,"clamped":false,"producer":"provider_relay","kind":"provider_outcome_claimed","body":{"attempt":1,"claim":"failure"}}]"#,
    r#"ORDJ1 325 d5782351 [{"v":1,"run":"r7","seq":17,"at_ms":1793836875000,"clamped":false,"producer":"supervisor","kind":"stop_requested","body":{"attempt":1,"reason":"deadline_run"}},{"v":1,"run":"r7","seq":18,"at_ms":1793836875000,"clamped":false,"producer":"supervisor","kind":"approval_voided","body":{"approval":"a4","reason":"attempt_ended"}}]"#,
    r#"ORDJ1 172 faa0b09b [{"v":1,"run":"r7","seq":19,"at_ms":1793836876000,"clamped":false,"producer":"supervisor","kind":"attempt_exited","body":{"attempt":1,"status":{"code":0,"ended_by":null}}}]"#,
    r#"ORDJ1 935 7b8e99ad [{"v":1,"run":"r7","seq":20,"at_ms":1793836877000,"clamped":false,"producer":"user","kind":"attempt_created","body":{"attempt":2,"continuation":"fresh","from_attempt":null,"session":null}},{"v":1,"run":"r7","seq":21,"at_ms":1793836877000,"clamped":false,"producer":"supervisor","kind":"approval_requested","body":{"approval":"a5","run":"r7","attempt":2,"kind":"start","binding":{"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","target":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"operation_class":"start","operation_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},"binding_digest":"0000000000000000000000000000000000000000000000000000000000000005","description_sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","scope":"once","risk":"high","provider_risk_claim":null,"ttl_ms":60000}}]"#,
    r#"ORDJ1 308 c6d43a86 [{"v":1,"run":"r7","seq":22,"at_ms":1793836878000,"clamped":false,"producer":"user","kind":"cancel_requested","body":{"scope":"attempt","attempt":2}},{"v":1,"run":"r7","seq":23,"at_ms":1793836878000,"clamped":false,"producer":"user","kind":"approval_withdrawn","body":{"approval":"a5","reason":"cancelled"}}]"#,
    r#"ORDJ1 935 c0275181 [{"v":1,"run":"r7","seq":24,"at_ms":1793836879000,"clamped":false,"producer":"user","kind":"attempt_created","body":{"attempt":3,"continuation":"fresh","from_attempt":null,"session":null}},{"v":1,"run":"r7","seq":25,"at_ms":1793836879000,"clamped":false,"producer":"supervisor","kind":"approval_requested","body":{"approval":"a6","run":"r7","attempt":3,"kind":"start","binding":{"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","target":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"operation_class":"start","operation_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},"binding_digest":"0000000000000000000000000000000000000000000000000000000000000006","description_sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","scope":"once","risk":"high","provider_risk_claim":null,"ttl_ms":60000}}]"#,
    r#"ORDJ1 313 45c0555c [{"v":1,"run":"r7","seq":26,"at_ms":1793836880000,"clamped":false,"producer":"user","kind":"approval_decided","body":{"approval":"a6","decision":"granted","provenance":{"actor":"user","channel":"native_confirmation","presented_binding_digest":"0000000000000000000000000000000000000000000000000000000000000006"}}}]"#,
    r#"ORDJ1 471 ad1dc2a5 [{"v":1,"run":"r7","seq":27,"at_ms":1793836881000,"clamped":false,"producer":"supervisor","kind":"start_begun","body":{"attempt":3,"approval":"a6","binding":{"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","target":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"operation_class":"start","operation_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}}}]"#,
    r#"ORDJ1 157 a7e2b814 [{"v":1,"run":"r7","seq":28,"at_ms":1793836882000,"clamped":false,"producer":"supervisor","kind":"start_failed","body":{"attempt":3,"code":"hash_mismatch"}}]"#,
    r#"ORDJ1 360 e9ce958c [{"v":1,"run":"r7","seq":29,"at_ms":1793836883000,"clamped":false,"producer":"user","kind":"attention_resolved","body":{"concern":{"kind":"workspace_unknown","attempt":1,"raised_at":19},"resolution":"reviewed","provenance":{"actor":"user","channel":"native_confirmation","presented_sha256":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"}}}]"#,
    r#"ORDJ1 133 1358c7cb [{"v":1,"run":"r7","seq":30,"at_ms":1793836884000,"clamped":false,"producer":"user","kind":"run_closed","body":{"reason":"by_user"}}]"#,
];
const GOLDEN_RECONCILED: &[&str] = &[
    r#"ORDJ1 712 da654527 [{"v":1,"run":"r7","seq":1,"at_ms":1793836800000,"clamped":false,"producer":"user","kind":"run_created","body":{"spec":{"task":{"task":"t1","created_at_ms":1793836800000,"revision":1},"provider":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","advertised_version":"1.2.3"},"context":{"project":{"project":"p1","registered_at_ms":1793836800000},"workspace":"none","permissions":[],"limits":{"start_ms":60000,"run_ms":3600000,"stall_ms":120000,"approval_ttl_ms":300000,"stop_grace_ms":5000},"policy_version":"policy-1","environment_sha256":null},"max_started_attempts":8},"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}]"#,
    r#"ORDJ1 933 607595de [{"v":1,"run":"r7","seq":2,"at_ms":1793836801000,"clamped":false,"producer":"user","kind":"attempt_created","body":{"attempt":1,"continuation":"fresh","from_attempt":null,"session":null}},{"v":1,"run":"r7","seq":3,"at_ms":1793836801000,"clamped":false,"producer":"supervisor","kind":"approval_requested","body":{"approval":"a1","run":"r7","attempt":1,"kind":"start","binding":{"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","target":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"operation_class":"start","operation_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},"binding_digest":"0000000000000000000000000000000000000000000000000000000000000001","description_sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","scope":"once","risk":"high","provider_risk_claim":null,"ttl_ms":60000}}]"#,
    r#"ORDJ1 312 d2905210 [{"v":1,"run":"r7","seq":4,"at_ms":1793836802000,"clamped":false,"producer":"user","kind":"approval_decided","body":{"approval":"a1","decision":"granted","provenance":{"actor":"user","channel":"native_confirmation","presented_binding_digest":"0000000000000000000000000000000000000000000000000000000000000001"}}}]"#,
    r#"ORDJ1 534 4aa2d06d [{"v":1,"run":"r7","seq":5,"at_ms":1793836805000,"clamped":false,"producer":"journal","kind":"journal_repaired","body":{"sidecar":1,"removed_bytes":40,"removed_crc32c":7,"cause":"incomplete_tail"}},{"v":1,"run":"r7","seq":6,"at_ms":1793836805000,"clamped":false,"producer":"reconciler","kind":"attempt_interrupted","body":{"attempt":1,"observation":"evidence_lost"}},{"v":1,"run":"r7","seq":7,"at_ms":1793836805000,"clamped":false,"producer":"reconciler","kind":"approval_voided","body":{"approval":"a1","reason":"supervision_lost"}}]"#,
];
const GOLDEN_NOT_STARTED: &[&str] = &[
    r#"ORDJ1 712 da654527 [{"v":1,"run":"r7","seq":1,"at_ms":1793836800000,"clamped":false,"producer":"user","kind":"run_created","body":{"spec":{"task":{"task":"t1","created_at_ms":1793836800000,"revision":1},"provider":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","advertised_version":"1.2.3"},"context":{"project":{"project":"p1","registered_at_ms":1793836800000},"workspace":"none","permissions":[],"limits":{"start_ms":60000,"run_ms":3600000,"stall_ms":120000,"approval_ttl_ms":300000,"stop_grace_ms":5000},"policy_version":"policy-1","environment_sha256":null},"max_started_attempts":8},"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}]"#,
    r#"ORDJ1 933 607595de [{"v":1,"run":"r7","seq":2,"at_ms":1793836801000,"clamped":false,"producer":"user","kind":"attempt_created","body":{"attempt":1,"continuation":"fresh","from_attempt":null,"session":null}},{"v":1,"run":"r7","seq":3,"at_ms":1793836801000,"clamped":false,"producer":"supervisor","kind":"approval_requested","body":{"approval":"a1","run":"r7","attempt":1,"kind":"start","binding":{"spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","target":{"provider":"codex","executable_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"operation_class":"start","operation_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},"binding_digest":"0000000000000000000000000000000000000000000000000000000000000001","description_sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","scope":"once","risk":"high","provider_risk_claim":null,"ttl_ms":60000}}]"#,
    r#"ORDJ1 338 af66ef48 [{"v":1,"run":"r7","seq":4,"at_ms":1793836805000,"clamped":false,"producer":"reconciler","kind":"attempt_not_started","body":{"attempt":1,"reason":"supervision_lost"}},{"v":1,"run":"r7","seq":5,"at_ms":1793836805000,"clamped":false,"producer":"reconciler","kind":"approval_withdrawn","body":{"approval":"a1","reason":"supervision_lost"}}]"#,
];

// ---------------------------------------------------------------------------------------------------------------
// CRC-32C

#[test]
fn crc32c_matches_the_standard_check_value_and_known_vectors() {
    assert_eq!(crc32c(b"123456789"), 0xe306_9283);
    assert_eq!(crc32c(b""), 0);
    // RFC 3720 (iSCSI), appendix B.4.
    assert_eq!(crc32c(&[0u8; 32]), 0x8a91_36aa);
    assert_eq!(crc32c(&[0xffu8; 32]), 0x62a8_ab43);
    let ascending: Vec<u8> = (0..32).collect();
    assert_eq!(crc32c(&ascending), 0x46dd_794e);
    let descending: Vec<u8> = (0..32).rev().collect();
    assert_eq!(crc32c(&descending), 0x113f_db5c);
    // The frame checksum covers the length digits, one space and the payload.
    assert_eq!(frame_crc(b"12", b"[1]"), crc32c(b"12 [1]"));
}

// ---------------------------------------------------------------------------------------------------------------
// Golden fixtures

#[test]
fn golden_v1_histories_encode_to_their_pinned_bytes() {
    for (name, journal, golden) in [
        ("supervised", golden_supervised(), golden_bytes(GOLDEN_SUPERVISED)),
        ("reconciled", golden_reconciled(), golden_bytes(GOLDEN_RECONCILED)),
        ("not started", golden_not_started(), golden_bytes(GOLDEN_NOT_STARTED)),
    ] {
        assert_eq!(String::from_utf8(stream(&journal.frames)).unwrap(), String::from_utf8(golden.clone()).unwrap(), "{name}");
        let decoded = decode_stream(&golden);
        assert_eq!(decoded.end(), StreamEnd::Clean, "{name}");
        assert_eq!(frames_of(&decoded), journal.frames, "{name}");
        // The pinned bytes rebuild the projection the reducer built live.
        let replayed = journal.frames.iter().fold(RunProjection::empty(run_id()), |p, frame| apply_recorded(&p, frame).unwrap());
        assert_eq!(replayed, journal.projection, "{name}");
    }
}

#[test]
fn the_golden_histories_cover_every_kind_both_request_kinds_and_generated_consequences() {
    let mut kinds: Vec<EventKind> = Vec::new();
    for golden in [GOLDEN_SUPERVISED, GOLDEN_RECONCILED, GOLDEN_NOT_STARTED] {
        for frame in decode_stream(&golden_bytes(golden)).frames() {
            kinds.extend(frame.events().iter().map(|event| event.body.kind()));
        }
    }
    for kind in EventKind::ALL {
        assert!(kinds.contains(&kind), "{kind:?} missing from the golden histories");
    }
    // Generated consequences: the void after the deadline stop, the withdrawal after the cancel, and the reconciler's.
    let supervised = decode_stream(&golden_bytes(GOLDEN_SUPERVISED));
    let multi: Vec<&DecodedFrame> = supervised.frames().iter().filter(|frame| frame.events().len() > 1).collect();
    assert!(multi.iter().any(|frame| matches!(frame.events()[1].body, EventBody::ApprovalVoided { reason: VoidReason::AttemptEnded, .. })));
    assert!(multi.iter().any(|frame| matches!(frame.events()[1].body, EventBody::ApprovalWithdrawn { reason: WithdrawalReason::Cancelled, .. })));
    let reconciled = decode_stream(&golden_bytes(GOLDEN_RECONCILED));
    let last = reconciled.frames().last().unwrap().events();
    assert!(matches!(last[2].body, EventBody::ApprovalVoided { reason: VoidReason::SupervisionLost, .. }));
    assert_eq!(last[2].producer, Producer::Reconciler);
    let not_started = decode_stream(&golden_bytes(GOLDEN_NOT_STARTED));
    assert!(matches!(not_started.frames().last().unwrap().events()[1].body, EventBody::ApprovalWithdrawn { reason: WithdrawalReason::SupervisionLost, .. }));
}

#[test]
fn golden_histories_decode_to_pinned_projections() {
    let replay = |lines: &[&str]| {
        let decoded = decode_stream(&golden_bytes(lines));
        decoded.frames().iter().fold(RunProjection::empty(run_id()), |p, frame| apply_recorded(&p, frame.events()).unwrap())
    };
    let supervised = replay(GOLDEN_SUPERVISED);
    assert_eq!(supervised.state(), Some(RunState::ClosedByUser));
    let attempts: Vec<AttemptState> = supervised.attempts().iter().map(|record| record.state()).collect();
    assert_eq!(
        attempts,
        [
            AttemptState::TimedOut(DeadlineKind::Run),
            AttemptState::NotStarted(NotStartedReason::Cancelled),
            AttemptState::NotStarted(NotStartedReason::StartFailed(StartFailCode::HashMismatch)),
        ]
    );
    let approvals: Vec<ApprovalState> = supervised.approvals().iter().map(|record| record.state()).collect();
    assert_eq!(
        approvals,
        [
            ApprovalState::Consumed,
            ApprovalState::Consumed,
            ApprovalState::Expired,
            ApprovalState::Voided,
            ApprovalState::Withdrawn,
            ApprovalState::Consumed,
        ]
    );
    assert!(!supervised.has_unresolved_blocking_concern());
    assert_eq!(supervised.events().len(), 30);

    let reconciled = replay(GOLDEN_RECONCILED);
    assert_eq!(reconciled.attempt(n(1)).unwrap().state(), AttemptState::Interrupted(InterruptObservation::EvidenceLost));
    assert_eq!(reconciled.approval(a(1)).unwrap().state(), ApprovalState::Voided);
    assert!(reconciled.has_unresolved_blocking_concern());
    let kinds: Vec<ConcernKind> = reconciled.concerns().iter().map(|concern| concern.concern().kind()).collect();
    assert!(kinds.contains(&ConcernKind::PossibleOrphan) && kinds.contains(&ConcernKind::JournalRepaired));

    let not_started = replay(GOLDEN_NOT_STARTED);
    assert_eq!(not_started.attempt(n(1)).unwrap().state(), AttemptState::NotStarted(NotStartedReason::SupervisionLost));
    assert_eq!(not_started.approval(a(1)).unwrap().state(), ApprovalState::Withdrawn);
    assert!(!not_started.has_unresolved_blocking_concern());
}

#[test]
fn a_frame_is_one_line_with_a_fixed_header() {
    let frame = encode_frame(&Journal::new().frames[0]).unwrap();
    let text = String::from_utf8(frame.clone()).unwrap();
    assert!(text.starts_with("ORDJ1 "));
    assert!(text.ends_with('\n'));
    assert_eq!(text.matches('\n').count(), 1);
    let fields: Vec<&str> = text.splitn(4, ' ').collect();
    let payload = fields[3].trim_end_matches('\n');
    assert_eq!(fields[1], payload.len().to_string());
    assert_eq!(fields[2].len(), 8);
    assert!(fields[2].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    assert_eq!(u32::from_str_radix(fields[2], 16).unwrap(), crc32c(format!("{} {payload}", fields[1]).as_bytes()));
    // Canonical field order of the envelope.
    assert!(payload.starts_with(r#"[{"v":1,"run":"r7","seq":1,"at_ms":1793836800000,"clamped":false,"producer":"user","kind":"run_created","body":{"#));
}

// ---------------------------------------------------------------------------------------------------------------
// Encoding

#[test]
fn encoding_refuses_frames_the_reducer_never_produces() {
    let journal = golden_supervised();
    let events: Vec<Event> = journal.projection.events().to_vec();
    assert_eq!(encode_frame(&[]), Err(EncodeRefused::Empty));
    assert_eq!(encode_frame(&events[..9]), Err(EncodeRefused::TooManyEvents));
    assert_eq!(encode_frame(&[events[0].clone(), events[2].clone()]), Err(EncodeRefused::NotContiguous));
    let mut other_run = events[1].clone();
    other_run.run = RunId::new(8).unwrap();
    assert_eq!(encode_frame(&[events[0].clone(), other_run]), Err(EncodeRefused::NotContiguous));
    let mut old = events[0].clone();
    old.v = 2;
    assert_eq!(encode_frame(&[old]), Err(EncodeRefused::NotCurrentVersion));
}

/// The largest instance of every kind: every token at 128 characters, every optional field present, every number at
/// its largest value.
fn maximal_events() -> Vec<Event> {
    let long = |c: char| token(&c.to_string().repeat(128));
    let wall = WallMs::new(MAX_WALL_MS).unwrap();
    let seq = Seq::new(MAX_WALL_MS).unwrap();
    let attempt = n(MAX_ATTEMPT_RECORDS);
    let approval = ApprovalId::new(9_999_999_999).unwrap();
    let run = RunId::new(9_999_999_999).unwrap();
    let limits = Limits::new(600_000, 86_400_000, 86_400_000, 900_000, 60_000).unwrap();
    let task = TaskRef::new(TaskId::parse("t9999999999").unwrap(), wall, MAX_WALL_MS).unwrap();
    let project = ProjectRef { project: ProjectId::parse("p9999999999").unwrap(), registered_at_ms: wall };
    let permissions = PERMISSIONS.to_vec();
    let context = RunContext::new(project, WorkspaceMode::None, permissions, limits, long('p'), Some(hash('d'))).unwrap();
    let provider = ProviderIdentity { provider: CatalogId::OpenCode, executable_sha256: hash('a'), advertised_version: Some(long('v')) };
    let spec = RunSpec::new(task, provider, context, MAX_STARTED_ATTEMPTS).unwrap();
    let binding = Binding {
        spec_digest: hash('b'),
        target: BindingTarget { provider: CatalogId::OpenCode, executable_sha256: hash('a') },
        operation_class: OperationClass::PermissionChange,
        operation_digest: hash('c'),
    };
    let fields = RequestFields {
        approval,
        run,
        attempt,
        kind: ApprovalKind::Operation,
        binding: binding.clone(),
        binding_digest: hash('e'),
        description_sha256: hash('f'),
        provider_risk_claim: Some(RiskClass::Critical),
        ttl_ms: 900_000,
    };
    let request = ApprovalRequest::new(fields, WorkspaceMode::None).unwrap();
    let concern = ConcernRef::new(ConcernKind::WorkspaceUnknown, Some(attempt), seq).unwrap();
    let provenance = ResolutionProvenance { actor: Actor::User, channel: Channel::NativeConfirmation, presented_sha256: hash('f') };
    let decision = DecisionProvenance { actor: Actor::User, channel: Channel::NativeConfirmation, presented_binding_digest: hash('e') };
    let bodies = [
        EventBody::RunCreated { spec, spec_digest: hash('b') },
        EventBody::AttemptCreated { attempt, continuation: Continuation::Resume { from_attempt: attempt, session: long('s') } },
        EventBody::ApprovalRequested(request),
        EventBody::ApprovalDecided { approval, decision: Decision::Granted, provenance: decision },
        EventBody::ApprovalExpired { approval, basis: ExpiryBasis::ClockFault },
        EventBody::ApprovalWithdrawn { approval, reason: WithdrawalReason::SupervisionLost },
        EventBody::ApprovalVoided { approval, reason: VoidReason::SupervisionLost },
        EventBody::ApprovalConsumed { approval, binding: binding.clone() },
        EventBody::StartBegun { attempt, approval, binding },
        EventBody::AttemptStarted { attempt, os_identity: OsIdentity::new(u32::MAX, long('t'), hash('a')).unwrap() },
        EventBody::StartFailed { attempt, code: StartFailCode::EffectNotPerformed },
        EventBody::StopRequested { attempt, reason: StopReason::Deadline(DeadlineKind::Stall) },
        EventBody::CancelRequested { scope: CancelScope::Attempt(attempt) },
        EventBody::AttemptExited { attempt, status: ExitStatus::new(Some(i32::MIN), None).unwrap() },
        EventBody::AttemptInterrupted { attempt, observation: InterruptObservation::PossiblyRunning },
        EventBody::AttemptNotStarted { attempt, reason: NotStartedCause::SupervisionLost },
        EventBody::ProviderSessionObserved { attempt, session: long('s') },
        EventBody::Progress { attempt, kind: ProgressKind::ToolActivity },
        EventBody::ProviderOutcomeClaimed { attempt, claim: OutcomeClaim::Failure },
        EventBody::RunClosed { reason: CloseReason::ByUser },
        EventBody::AttentionResolved(ConcernResolution::new(concern, Resolution::AcceptedRisk, provenance).unwrap()),
        EventBody::JournalRepaired(JournalRepair::new(u32::MAX, MAX_TAIL_BYTES, u32::MAX, RepairCause::IncompleteTail).unwrap()),
    ];
    bodies
        .into_iter()
        .enumerate()
        .map(|(index, body)| Event {
            v: RECORD_VERSION,
            run,
            seq: Seq::new(MAX_WALL_MS - 30 + index as u64).unwrap(),
            at_ms: wall,
            clamped: true,
            producer: Producer::ProviderRelay,
            body,
        })
        .collect()
}

#[test]
fn every_kind_at_its_maximal_size_fits_the_event_and_frame_limits() {
    let events = maximal_events();
    assert_eq!(events.len(), EventKind::ALL.len());
    for event in &events {
        let bytes = event_bytes(event).unwrap();
        assert!(bytes.len() <= MAX_EVENT_BYTES as usize, "{:?}: {} bytes", event.body.kind(), bytes.len());
        let frame = encode_frame(std::slice::from_ref(event)).unwrap();
        let decoded = decode_stream(&frame);
        assert_eq!(decoded.end(), StreamEnd::Clean);
        assert_eq!(decoded.frames()[0].events(), std::slice::from_ref(event));
    }
    // MAX_BATCH events of the largest kind always fit one frame.
    let largest = events.iter().max_by_key(|event| event_bytes(event).unwrap().len()).unwrap();
    let batch: Vec<Event> = (0..MAX_BATCH as u64)
        .map(|index| Event { seq: Seq::new(index + 1).unwrap(), ..largest.clone() })
        .collect();
    let frame = encode_frame(&batch).unwrap();
    assert!(frame.len() <= MAX_FRAME_BYTES as usize + 22);
    assert_eq!(frames_of(&decode_stream(&frame)), [batch]);
}

#[test]
fn every_closed_value_round_trips_by_name() {
    fn names<T: Copy + PartialEq + core::fmt::Debug>(all: &[T], name_of: fn(T) -> &'static str) {
        for value in all {
            assert_eq!(lookup(all, name_of, name_of(*value)), Ok(*value));
        }
        let mut seen: Vec<&str> = all.iter().map(|value| name_of(*value)).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), all.len(), "duplicate names");
        assert!(seen.iter().all(|name| name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')));
    }
    names(&PRODUCERS, producer_name);
    names(&EventKind::ALL, kind_name);
    names(&WORKSPACES, workspace_name);
    names(&PERMISSIONS, permission_name);
    names(&APPROVAL_KINDS, approval_kind_name);
    names(&OPERATION_CLASSES, operation_class_name);
    names(&RISKS, risk_name);
    names(&ACTORS, actor_name);
    names(&CHANNELS, channel_name);
    names(&DECISIONS, decision_name);
    names(&EXPIRY_BASES, expiry_name);
    names(&WITHDRAWALS, withdrawal_name);
    names(&VOIDS, void_name);
    names(&START_FAILURES, start_failure_name);
    names(&STOP_REASONS, stop_name);
    names(&OBSERVATIONS, observation_name);
    names(&NOT_STARTED_CAUSES, not_started_name);
    names(&PROGRESS_KINDS, progress_name);
    names(&CLAIMS, claim_name);
    names(&CLOSE_REASONS, close_name);
    names(&CONCERN_KINDS, concern_name);
    names(&RESOLUTIONS, resolution_name);
    names(&REPAIR_CAUSES, repair_cause_name);
    assert_eq!(lookup(&PRODUCERS, producer_name, "User"), Err(CorruptReason::Schema));
    assert_eq!(lookup(&PRODUCERS, producer_name, "user "), Err(CorruptReason::Schema));
}

#[test]
fn serde_json_has_no_arbitrary_precision_numbers() {
    // With `arbitrary_precision`, a number keeps its text ("1e2"); without it, it is an f64 ("100.0"). The codec's
    // integer handling assumes the latter; this fails if any crate in the graph turns the feature on.
    let value: serde_json::Value = serde_json::from_str("1e2").unwrap();
    assert_eq!(value.to_string(), "100.0");
    let big: serde_json::Value = serde_json::from_str("123456789012345678901234567890").unwrap();
    assert!(big.is_f64());
}

// ---------------------------------------------------------------------------------------------------------------
// Canonical form and schema

/// A one-event frame whose payload is `edit` applied to the canonical payload of `event`.
fn edited(event: &Event, edit: impl Fn(&str) -> String) -> Vec<u8> {
    let payload = String::from_utf8(payload_of(&encode_frame(std::slice::from_ref(event)).unwrap())).unwrap();
    let changed = edit(&payload);
    assert_ne!(changed, payload, "the edit changed nothing");
    reframe(changed.as_bytes())
}

fn corrupt_reason(bytes: &[u8]) -> CorruptReason {
    match decode_stream(bytes).end() {
        StreamEnd::Corrupt { reason, .. } => reason,
        other => panic!("not corrupt: {other:?}"),
    }
}

#[test]
fn non_canonical_and_ambiguous_payloads_are_corrupt() {
    let journal = golden_supervised();
    let events = journal.projection.events();
    let created = &events[0];
    let request = events.iter().find(|event| matches!(event.body, EventBody::ApprovalRequested(_))).unwrap();
    let exited = events.iter().find(|event| matches!(event.body, EventBody::AttemptExited { .. })).unwrap();
    let cases: Vec<(&str, Vec<u8>, CorruptReason)> = Vec::from([
        ("a space after a colon", edited(created, |p| p.replacen("\"v\":1", "\"v\": 1", 1)), CorruptReason::NotCanonical),
        ("a trailing space", edited(created, |p| format!("{p} ")), CorruptReason::NotCanonical),
        ("a newline-free tab", edited(created, |p| p.replacen(',', ",\t", 1)), CorruptReason::NotCanonical),
        ("fields reordered", edited(created, |p| p.replacen("\"v\":1,\"run\":\"r7\"", "\"run\":\"r7\",\"v\":1", 1)), CorruptReason::NotCanonical),
        ("an escaped letter", edited(created, |p| p.replacen("\"r7\"", "\"\\u0072\\u0037\"", 1)), CorruptReason::NotCanonical),
        ("an escaped solidus", edited(created, |p| p.replacen("policy-1", "policy\\/1", 1)), CorruptReason::NotCanonical),
        ("a float", edited(created, |p| p.replacen("\"seq\":1", "\"seq\":1.0", 1)), CorruptReason::Schema),
        ("an exponent", edited(created, |p| p.replacen("\"seq\":1", "\"seq\":1e0", 1)), CorruptReason::Schema),
        ("a leading zero", edited(created, |p| p.replacen("\"seq\":1", "\"seq\":01", 1)), CorruptReason::Schema),
        ("a negative zero", edited(exited, |p| p.replacen("\"code\":0", "\"code\":-0", 1)), CorruptReason::Schema),
        ("a plus sign", edited(created, |p| p.replacen("\"seq\":1", "\"seq\":+1", 1)), CorruptReason::Schema),
        ("a duplicate key", edited(created, |p| p.replacen("\"v\":1,", "\"v\":1,\"v\":1,", 1)), CorruptReason::Schema),
        // A body is read through a JSON value, which keeps one of two equal keys: the re-encoding check refuses it.
        ("a duplicate body key", edited(created, |p| p.replacen("\"revision\":1", "\"revision\":1,\"revision\":1", 1)), CorruptReason::NotCanonical),
        ("a duplicate body key with another value", edited(created, |p| p.replacen("\"revision\":1", "\"revision\":2,\"revision\":1", 1)), CorruptReason::NotCanonical),
        ("a duplicate body key with another value last", edited(created, |p| p.replacen("\"revision\":1", "\"revision\":1,\"revision\":2", 1)), CorruptReason::NotCanonical),
        ("an unknown envelope field", edited(created, |p| p.replacen("\"v\":1,", "\"v\":1,\"x\":0,", 1)), CorruptReason::Schema),
        ("an unknown body field", edited(created, |p| p.replacen("\"revision\":1", "\"revision\":1,\"note\":\"x\"", 1)), CorruptReason::Schema),
        ("an omitted null", edited(created, |p| p.replacen(",\"environment_sha256\":null", "", 1)), CorruptReason::NotCanonical),
        ("a missing field", edited(created, |p| p.replacen("\"clamped\":false,", "", 1)), CorruptReason::Schema),
        ("a boolean as a number", edited(created, |p| p.replacen("\"clamped\":false", "\"clamped\":0", 1)), CorruptReason::Schema),
        ("an unknown kind", edited(created, |p| p.replacen("run_created", "run_opened", 1)), CorruptReason::Schema),
        ("an unknown producer", edited(created, |p| p.replacen("\"user\"", "\"admin\"", 1)), CorruptReason::Schema),
        ("an upper-case name", edited(created, |p| p.replacen("\"user\"", "\"User\"", 1)), CorruptReason::Schema),
        ("a lowered risk", edited(request, |p| p.replacen("\"risk\":\"high\"", "\"risk\":\"elevated\"", 1)), CorruptReason::Schema),
        ("another scope", edited(request, |p| p.replacen("\"scope\":\"once\"", "\"scope\":\"always\"", 1)), CorruptReason::Schema),
        ("a mismatched kind and class", edited(request, |p| p.replacen("\"operation_class\":\"start\"", "\"operation_class\":\"shell_run\"", 1)), CorruptReason::Schema),
        ("a seq above 2^53 - 1", edited(created, |p| p.replacen("\"seq\":1", "\"seq\":9007199254740992", 1)), CorruptReason::Integer),
        ("a time above 2^53 - 1", edited(created, |p| p.replacen("\"at_ms\":1793836800000", "\"at_ms\":18446744073709551615", 1)), CorruptReason::Integer),
        ("an integer above u64", edited(created, |p| p.replacen("\"seq\":1", "\"seq\":18446744073709551616", 1)), CorruptReason::Schema),
        ("a negative seq", edited(created, |p| p.replacen("\"seq\":1", "\"seq\":-1", 1)), CorruptReason::Schema),
        ("an exit code outside i32", edited(exited, |p| p.replacen("\"code\":0", "\"code\":2147483648", 1)), CorruptReason::Schema),
        ("an attempt number outside u8", edited(exited, |p| p.replacen("\"attempt\":1", "\"attempt\":257", 1)), CorruptReason::Schema),
        ("record as an array", edited(created, |p| format!("[[{}]]", &p[1..p.len() - 1])), CorruptReason::Schema),
        ("a version of zero", edited(created, |p| p.replacen("\"v\":1", "\"v\":0", 1)), CorruptReason::Schema),
        ("a version above 2^53 - 1", edited(created, |p| p.replacen("\"v\":1", "\"v\":9007199254740992", 1)), CorruptReason::Integer),
        ("not an array", edited(created, |p| p[1..p.len() - 1].to_owned()), CorruptReason::Schema),
    ]);
    for (name, bytes, reason) in cases {
        assert_eq!(decode_stream(&bytes).frames().len(), 0, "{name}");
        assert_eq!(corrupt_reason(&bytes), reason, "{name}");
    }
}

#[test]
fn unicode_and_invalid_text_are_corrupt() {
    let created = Journal::new().projection.events()[0].clone();
    let cases: Vec<(&str, Vec<u8>)> = Vec::from([
        ("a non-ASCII letter in a token", edited(&created, |p| p.replacen("policy-1", "pol\u{0456}cy-1", 1))),
        ("a right-to-left override", edited(&created, |p| p.replacen("policy-1", "policy-\u{202e}1", 1))),
        ("an escaped non-ASCII letter", edited(&created, |p| p.replacen("policy-1", "pol\\u0456cy-1", 1))),
        ("a lone surrogate", edited(&created, |p| p.replacen("policy-1", "policy\\ud800", 1))),
        ("an escaped control character", edited(&created, |p| p.replacen("policy-1", "policy\\u0000", 1))),
        ("an escaped newline", edited(&created, |p| p.replacen("policy-1", "policy\\n1", 1))),
    ]);
    for (name, bytes) in cases {
        assert!(matches!(decode_stream(&bytes).end(), StreamEnd::Corrupt { .. }), "{name}");
    }
    // Bytes that are not UTF-8.
    let mut payload = payload_of(&encode_frame(std::slice::from_ref(&created)).unwrap());
    let at = payload.windows(8).position(|w| w == b"policy-1").unwrap();
    payload[at] = 0xff;
    assert_eq!(corrupt_reason(&reframe(&payload)), CorruptReason::Schema);
    payload[at] = 0xc3;
    assert_eq!(corrupt_reason(&reframe(&payload)), CorruptReason::Schema);
}

#[test]
fn batches_are_bounded_and_never_empty() {
    assert_eq!(corrupt_reason(&reframe(b"[]")), CorruptReason::EmptyBatch);
    let events = golden_supervised().projection.events()[..8].to_vec();
    let mut nine = payload_of(&encode_frame(&events).unwrap());
    nine.pop();
    nine.push(b',');
    nine.extend_from_slice(&event_bytes(&golden_supervised().projection.events()[8]).unwrap());
    nine.push(b']');
    assert_eq!(corrupt_reason(&reframe(&nine)), CorruptReason::BatchTooLarge);
}

#[test]
fn deep_nesting_is_refused_before_serde_runs() {
    let deep = |levels: usize| {
        let mut payload = "[".repeat(levels).into_bytes();
        payload.extend(b"]".repeat(levels));
        payload
    };
    // Past the frame bound (64), and far past any recursion limit while still inside one frame: damage, before serde.
    assert_eq!(corrupt_reason(&reframe(&deep(65))), CorruptReason::Nesting);
    assert_eq!(corrupt_reason(&reframe(&deep(8_000))), CorruptReason::Nesting);
    // Inside the frame bound, arrays that are no records are a schema fault.
    assert_eq!(corrupt_reason(&reframe(&deep(9))), CorruptReason::Schema);
    // A v1 record nested deeper than v1 allows (8) is refused before strict decoding.
    let v1 = format!("[{{\"v\":1,\"x\":{}1{}}}]", "[".repeat(7), "]".repeat(7));
    assert_eq!(corrupt_reason(&reframe(v1.as_bytes())), CorruptReason::Nesting);
    let v1_ok = format!("[{{\"v\":1,\"x\":{}1{}}}]", "[".repeat(6), "]".repeat(6));
    assert_eq!(corrupt_reason(&reframe(v1_ok.as_bytes())), CorruptReason::Schema);
    // Brackets inside strings do not count.
    assert!(nesting_within(br#"[{"a":"[[[[[[[[[[[["}]"#, 2));
    assert!(nesting_within(br#"[{"a":"\"[[[[[[[[[["}]"#, 2));
    assert!(!nesting_within(b"[[[", 2));
}

#[test]
fn the_version_is_read_before_strict_decoding() {
    let created = Journal::new().projection.events()[0].clone();
    // A v2 record with a field v1 does not know is a newer version, not v1 corruption (F14).
    let v2 = edited(&created, |p| p.replacen("\"v\":1,", "\"v\":2,\"novel\":{\"x\":[1]},", 1));
    match decode_stream(&v2).end() {
        StreamEnd::NewerVersion { offset: 0, v: 2 } => {}
        other => panic!("{other:?}"),
    }
    // A v2 record nested deeper, or a v2 frame with more records, than v1 allows is still newer (review of #118).
    assert!(matches!(decode_stream(&reframe(br#"[{"v":2,"x":[[[[[[[[1]]]]]]]]}]"#)).end(), StreamEnd::NewerVersion { v: 2, .. }));
    let many = format!("[{}]", Vec::from(["{\"v\":2}"; MAX_BATCH + 1]).join(","));
    assert!(matches!(decode_stream(&reframe(many.as_bytes())).end(), StreamEnd::NewerVersion { v: 2, .. }));
    // Past the frame nesting bound every version is damage.
    let deep = format!("[{{\"v\":2,\"x\":{}1{}}}]", "[".repeat(64), "]".repeat(64));
    assert_eq!(corrupt_reason(&reframe(deep.as_bytes())), CorruptReason::Nesting);
    // Even a v2 record whose shape v1 cannot read at all.
    assert!(matches!(decode_stream(&reframe(br#"[{"v":2}]"#)).end(), StreamEnd::NewerVersion { v: 2, .. }));
    assert!(matches!(decode_stream(&reframe(br#"[{"v":65536,"k":[]}]"#)).end(), StreamEnd::NewerVersion { v: 65536, .. }));
    // A newer record anywhere in a frame makes the whole frame newer.
    let mut mixed = payload_of(&encode_frame(std::slice::from_ref(&created)).unwrap());
    mixed.pop();
    mixed.extend_from_slice(br#",{"v":2}]"#);
    assert!(matches!(decode_stream(&reframe(&mixed)).end(), StreamEnd::NewerVersion { .. }));
    // Valid frames before it stay decoded; nothing after it is interpreted.
    let mut bytes = encode_frame(std::slice::from_ref(&created)).unwrap();
    let offset = bytes.len() as u64;
    bytes.extend(v2);
    bytes.extend(encode_frame(&golden_supervised().frames[1]).unwrap());
    let decoded = decode_stream(&bytes);
    assert_eq!(decoded.frames().len(), 1);
    assert_eq!(decoded.end(), StreamEnd::NewerVersion { offset, v: 2 });
    // A record with no version, or a version that is not an integer, is v1 corruption.
    assert_eq!(corrupt_reason(&reframe(br#"[{"run":"r7"}]"#)), CorruptReason::Schema);
    assert_eq!(corrupt_reason(&reframe(br#"[{"v":"2"}]"#)), CorruptReason::Schema);
    assert_eq!(corrupt_reason(&reframe(br#"[{"v":1.5}]"#)), CorruptReason::Schema);
}

// ---------------------------------------------------------------------------------------------------------------
// Framing and classification

#[test]
fn malformed_headers_are_corrupt() {
    let frame = encode_frame(&Journal::new().frames[0]).unwrap();
    let text = String::from_utf8(frame.clone()).unwrap();
    let length = payload_of(&frame).len();
    let crc = &text[6 + length.to_string().len() + 1..][..8];
    let with = |from: &str, to: &str| text.replacen(from, to, 1).into_bytes();
    let cases: Vec<(&str, Vec<u8>, CorruptReason)> = Vec::from([
        ("another magic", with("ORDJ1 ", "ORDJ2 "), CorruptReason::Magic),
        ("a lower-case magic", with("ORDJ1 ", "ordj1 "), CorruptReason::Magic),
        ("a leading zero", with(&format!("ORDJ1 {length} "), &format!("ORDJ1 0{length} ")), CorruptReason::LengthField),
        ("six digits", with(&format!("ORDJ1 {length} "), &format!("ORDJ1 10{length:04} ")), CorruptReason::LengthField),
        ("a sign", with(&format!("ORDJ1 {length} "), &format!("ORDJ1 +{length} ")), CorruptReason::LengthField),
        ("no length", with(&format!("ORDJ1 {length} "), "ORDJ1  "), CorruptReason::LengthField),
        ("two spaces", with(&format!("{length} {crc}"), &format!("{length}  {crc}")), CorruptReason::ChecksumField),
        ("upper-case hex", with(crc, &crc.to_uppercase().replace(|c: char| c.is_ascii_digit(), "A")), CorruptReason::ChecksumField),
        ("seven hex digits", with(&format!("{crc} "), &format!("{} ", &crc[..7])), CorruptReason::ChecksumField),
        ("a tab separator", with(&format!("{crc} "), &format!("{crc}\t")), CorruptReason::ChecksumField),
        ("another checksum", with(crc, if crc == "00000000" { "00000001" } else { "00000000" }), CorruptReason::Checksum),
        ("a CRLF ending", text.replacen('\n', "\r\n", 1).into_bytes(), CorruptReason::FrameEnd),
        ("a length too large", with(&format!("ORDJ1 {length} "), &format!("ORDJ1 {} ", MAX_FRAME_BYTES + 1)), CorruptReason::FrameTooLarge),
    ]);
    for (name, bytes, reason) in cases {
        let decoded = decode_stream(&bytes);
        assert!(decoded.frames().is_empty(), "{name}");
        assert_eq!(decoded.end(), StreamEnd::Corrupt { offset: 0, reason }, "{name}");
    }
}

#[test]
fn truncation_at_every_offset_leaves_a_clean_prefix_and_an_incomplete_tail() {
    let journal = golden_supervised();
    let bytes = stream(&journal.frames);
    let ends: Vec<usize> = journal.frames.iter().scan(0, |at, frame| {
        *at += encode_frame(frame).unwrap().len();
        Some(*at)
    }).collect();
    for cut in 0..=bytes.len() {
        let decoded = decode_stream(&bytes[..cut]);
        let complete = ends.iter().filter(|end| **end <= cut).count();
        assert_eq!(frames_of(&decoded), journal.frames[..complete], "cut at {cut}");
        let boundary = if complete == 0 { 0 } else { ends[complete - 1] };
        if cut == boundary {
            assert_eq!(decoded.end(), StreamEnd::Clean, "cut at {cut}");
        } else {
            let tail = &bytes[boundary..cut];
            assert_eq!(
                decoded.end(),
                StreamEnd::IncompleteTail { offset: boundary as u64, len: tail.len() as u32, crc32c: crc32c(tail) },
                "cut at {cut}"
            );
        }
    }
}

#[test]
fn torn_tails_and_corrupt_tails_are_told_apart() {
    let journal = golden_supervised();
    let good = stream(&journal.frames[..3]);
    let last = encode_frame(&journal.frames[3]).unwrap();
    let offset = good.len() as u64;
    let with = |tail: &[u8]| {
        let mut bytes = good.clone();
        bytes.extend_from_slice(tail);
        decode_stream(&bytes)
    };
    // A complete last frame without its newline, the stream ending where the newline should be: a cut.
    assert!(matches!(with(&last[..last.len() - 1]).end(), StreamEnd::IncompleteTail { .. }));
    // A short zero-filled extension: a cut.
    assert!(matches!(with(&[0; 4096]).end(), StreamEnd::IncompleteTail { len: 4096, .. }));
    assert!(matches!(with(&[0; MAX_TAIL_BYTES as usize]).end(), StreamEnd::IncompleteTail { .. }));
    // A zero run longer than one frame could stand in for several acknowledged frames: corrupt.
    assert_eq!(with(&[0; MAX_TAIL_BYTES as usize + 1]).end(), StreamEnd::Corrupt { offset, reason: CorruptReason::TailTooLong });
    // A complete last frame with a valid checksum and another byte where the newline belongs: damage, not a cut.
    let mut wrong_end = last.clone();
    *wrong_end.last_mut().unwrap() = b' ';
    assert_eq!(with(&wrong_end).end(), StreamEnd::Corrupt { offset, reason: CorruptReason::FrameEnd });
    // A complete last frame whose checksum is wrong is corruption, even when it is the last frame.
    let mut flipped = last.clone();
    let at = flipped.len() - 10;
    flipped[at] ^= 0x01;
    assert_eq!(with(&flipped).end(), StreamEnd::Corrupt { offset, reason: CorruptReason::Checksum });
    // A torn frame followed by more bytes (a newline later on): corrupt, never a tail.
    let mut torn_then_more = last[..40].to_vec();
    torn_then_more.extend_from_slice(&encode_frame(&journal.frames[4]).unwrap());
    assert!(matches!(with(&torn_then_more).end(), StreamEnd::Corrupt { offset: o, .. } if o == offset));
    // A damaged length that points past the end, with a newline in the rest: corrupt.
    let text = String::from_utf8(last.clone()).unwrap();
    let length = payload_of(&last).len();
    let longer = text.replacen(&format!("ORDJ1 {length} "), &format!("ORDJ1 {} ", length + 5), 1);
    assert!(matches!(with(longer.as_bytes()).end(), StreamEnd::Corrupt { .. }));
    // Garbage after a valid frame with a newline in it: corrupt.
    assert_eq!(with(b"hello\n").end(), StreamEnd::Corrupt { offset, reason: CorruptReason::Magic });
    // A lone newline: corrupt.
    assert_eq!(with(b"\n").end(), StreamEnd::Corrupt { offset, reason: CorruptReason::Magic });
    // Every case kept the valid prefix and interpreted nothing after it.
    assert_eq!(frames_of(&with(&flipped)), journal.frames[..3]);
}

#[test]
fn a_corrupted_middle_frame_stops_decoding_and_is_never_skipped() {
    let journal = golden_supervised();
    let bytes = stream(&journal.frames);
    let middle = encode_frame(&journal.frames[0]).unwrap().len() + encode_frame(&journal.frames[1]).unwrap().len();
    let mut damaged = bytes.clone();
    damaged[middle + 30] ^= 0x20;
    let decoded = decode_stream(&damaged);
    assert_eq!(frames_of(&decoded), journal.frames[..2]);
    assert_eq!(decoded.end(), StreamEnd::Corrupt { offset: middle as u64, reason: CorruptReason::Checksum });
    // The audit scan lists the later frames as unverified only, with the kinds they would name.
    let (later, more) = scan_after_corruption(&damaged, middle as u64);
    assert!(!more);
    assert_eq!(later.len(), journal.frames.len() - 3);
    assert!(later.iter().all(UnverifiedFrame::checksum_matches));
    assert_eq!(later[0].kinds(), [EventKind::StartBegun]);
    assert_eq!(later[0].offset(), (middle + encode_frame(&journal.frames[2]).unwrap().len()) as u64);
    // A later frame whose own checksum fails is listed with no kinds.
    let mut twice = damaged.clone();
    let second = later[1].offset() as usize + later[1].len() as usize - 5;
    twice[second] ^= 0x01;
    let (relisted, _) = scan_after_corruption(&twice, middle as u64);
    assert!(!relisted[1].checksum_matches());
    assert!(relisted[1].kinds().is_empty());
}

#[test]
fn the_audit_scan_is_bounded() {
    let frame = encode_frame(&Journal::new().frames[0]).unwrap();
    let mut bytes = b"garbage\n".to_vec();
    for _ in 0..MAX_UNVERIFIED_FRAMES + 3 {
        bytes.extend_from_slice(&frame);
    }
    let (found, more) = scan_after_corruption(&bytes, 0);
    assert_eq!(found.len(), MAX_UNVERIFIED_FRAMES);
    assert!(more);
    assert_eq!(scan_after_corruption(&bytes, u64::MAX), (Vec::new(), false));
    assert_eq!(scan_after_corruption(b"", 0), (Vec::new(), false));
}

#[test]
fn a_stream_beyond_the_bound_is_not_decoded() {
    let bytes = vec![b' '; MAX_STREAM_BYTES as usize + 1];
    let decoded = decode_stream(&bytes);
    assert!(decoded.frames().is_empty());
    assert_eq!(decoded.end(), StreamEnd::Corrupt { offset: 0, reason: CorruptReason::StreamTooLarge });
}

// ---------------------------------------------------------------------------------------------------------------
// Uncertain writes

#[test]
fn verify_persisted_distinguishes_absent_landed_diverged_and_unknown() {
    let journal = golden_supervised();
    let committed = stream(&journal.frames[..4]);
    let pending = encode_frame(&journal.frames[4]).unwrap();
    let after = |tail: &[u8]| {
        let mut file = committed.clone();
        file.extend_from_slice(tail);
        file
    };
    assert_eq!(verify_persisted(&committed, &committed, &pending), PersistCheck::Absent { torn: None });
    assert_eq!(verify_persisted(&after(&pending), &committed, &pending), PersistCheck::Landed);
    // A torn write of the pending frame, and an unexplained short tail (a zero-filled extension).
    assert_eq!(
        verify_persisted(&after(&pending[..pending.len() - 1]), &committed, &pending),
        PersistCheck::Absent { torn: Some(TornTail { len: pending.len() as u32 - 1, pending_prefix: true, crc32c: crc32c(&pending[..pending.len() - 1]) }) }
    );
    assert_eq!(verify_persisted(&after(&[0; 64]), &committed, &pending), PersistCheck::Absent { torn: Some(TornTail { len: 64, pending_prefix: false, crc32c: crc32c(&[0; 64]) }) });
    // Anything else diverges.
    let mut extra = after(&pending);
    extra.push(b'x');
    assert_eq!(verify_persisted(&extra, &committed, &pending), PersistCheck::Diverged(Divergence::Extra));
    let foreign = encode_frame(&golden_reconciled().frames[2]).unwrap();
    assert_eq!(verify_persisted(&after(&foreign), &committed, &pending), PersistCheck::Diverged(Divergence::Foreign));
    assert_eq!(verify_persisted(&committed[..committed.len() - 1], &committed, &pending), PersistCheck::Diverged(Divergence::Shortened));
    let mut altered = after(&pending);
    altered[10] ^= 1;
    assert_eq!(verify_persisted(&altered, &committed, &pending), PersistCheck::Diverged(Divergence::Altered));
    let mut bad = pending.clone();
    let at = bad.len() - 3;
    bad[at] ^= 1;
    assert_eq!(verify_persisted(&after(&bad), &committed, &pending), PersistCheck::Diverged(Divergence::Corrupt(CorruptReason::Checksum)));
    assert_eq!(verify_persisted(&after(b"\n"), &committed, &pending), PersistCheck::Diverged(Divergence::Corrupt(CorruptReason::Magic)));
    assert_eq!(
        verify_persisted(&after(&[0; MAX_TAIL_BYTES as usize + 1]), &committed, &pending),
        PersistCheck::Diverged(Divergence::Corrupt(CorruptReason::TailTooLong))
    );
    // Inputs that cannot support a conclusion keep the outcome uncertain.
    assert_eq!(verify_persisted(&after(&pending), &committed[..committed.len() - 1], &pending), PersistCheck::Unknown(Insufficient::CommittedNotClean));
    assert_eq!(verify_persisted(&after(&pending), &committed, &pending[..pending.len() - 1]), PersistCheck::Unknown(Insufficient::PendingNotOneFrame));
    let two = stream(&journal.frames[4..6]);
    assert_eq!(verify_persisted(&after(&two), &committed, &two), PersistCheck::Unknown(Insufficient::PendingNotOneFrame));
    let skipped = encode_frame(&journal.frames[5]).unwrap();
    assert_eq!(verify_persisted(&after(&skipped), &committed, &skipped), PersistCheck::Unknown(Insufficient::PendingNotNext));
    let mut other_run = journal.frames[4].clone();
    for event in &mut other_run {
        event.run = RunId::new(8).unwrap();
    }
    let elsewhere = encode_frame(&other_run).unwrap();
    assert_eq!(verify_persisted(&after(&elsewhere), &committed, &elsewhere), PersistCheck::Unknown(Insufficient::PendingNotNext));
    let huge = vec![0; MAX_STREAM_BYTES as usize + 1];
    assert_eq!(verify_persisted(&huge, &committed, &pending), PersistCheck::Unknown(Insufficient::FileTooLarge));
    // The very first frame of a journal: nothing committed.
    let first = encode_frame(&journal.frames[0]).unwrap();
    assert_eq!(verify_persisted(&first, b"", &first), PersistCheck::Landed);
    assert_eq!(verify_persisted(b"", b"", &first), PersistCheck::Absent { torn: None });
    assert_eq!(verify_persisted(&pending, b"", &pending), PersistCheck::Unknown(Insufficient::PendingNotNext));
}

// ---------------------------------------------------------------------------------------------------------------
// Seeded property tests (decision 12.14)

#[test]
fn property_verify_persisted_on_every_frame_of_random_journals() {
    for seed in 0..80 {
        let (journal, _) = random_journal(seed, 60);
        let mut rng = Rng::new(seed);
        for index in 1..journal.frames.len() {
            let committed = stream(&journal.frames[..index]);
            let pending = encode_frame(&journal.frames[index]).unwrap();
            let with = |tail: &[u8]| [committed.as_slice(), tail].concat();
            assert_eq!(verify_persisted(&committed, &committed, &pending), PersistCheck::Absent { torn: None }, "seed {seed}");
            assert_eq!(verify_persisted(&with(&pending), &committed, &pending), PersistCheck::Landed, "seed {seed}");
            let cut = 1 + rng.below(pending.len() as u64 - 1) as usize;
            assert_eq!(
                verify_persisted(&with(&pending[..cut]), &committed, &pending),
                PersistCheck::Absent { torn: Some(TornTail { len: cut as u32, pending_prefix: true, crc32c: crc32c(&pending[..cut]) }) },
                "seed {seed}"
            );
            let mut flipped = pending.clone();
            let at = rng.below(flipped.len() as u64) as usize;
            flipped[at] ^= 1 << rng.below(8);
            assert!(!matches!(verify_persisted(&with(&flipped), &committed, &pending), PersistCheck::Landed), "seed {seed}: a damaged frame landed");
            if index + 1 < journal.frames.len() {
                let next = encode_frame(&journal.frames[index + 1]).unwrap();
                assert_eq!(verify_persisted(&with(&next), &committed, &pending), PersistCheck::Diverged(Divergence::Foreign), "seed {seed}");
            }
        }
    }
}

/// Seeds that ever failed are kept here and always run first.
const CODEC_REGRESSION_SEEDS: [u64; 0] = [];

fn seeds(count: u64) -> impl Iterator<Item = u64> {
    CODEC_REGRESSION_SEEDS.into_iter().chain(0..count)
}

#[test]
fn property_valid_journals_round_trip_byte_for_byte() {
    let mut kinds: Vec<EventKind> = Vec::new();
    for seed in seeds(200) {
        let (journal, _) = random_journal(seed, 120);
        let bytes = stream(&journal.frames);
        let decoded = decode_stream(&bytes);
        assert_eq!(decoded.end(), StreamEnd::Clean, "seed {seed}");
        assert_eq!(frames_of(&decoded), journal.frames, "seed {seed}");
        assert_eq!(stream(&frames_of(&decoded)), bytes, "seed {seed}: re-encoding changed the bytes");
        let mut at = 0;
        for frame in decoded.frames() {
            assert_eq!(frame.offset(), at, "seed {seed}");
            at = frame.end();
        }
        assert_eq!(decoded.valid_end(), bytes.len() as u64, "seed {seed}");
        kinds.extend(journal.projection.events().iter().map(|event| event.body.kind()));
    }
    for kind in EventKind::ALL {
        assert!(kinds.contains(&kind), "{kind:?} never generated");
    }
}

/// Applies one random mutation and returns the offset from which the bytes may differ from the original.
fn mutate(rng: &mut Rng, bytes: &mut Vec<u8>) -> usize {
    let len = bytes.len();
    let at = rng.below(len as u64 + 1) as usize;
    match rng.below(9) {
        0 if at < len => bytes[at] ^= 1 << rng.below(8),
        1 => bytes.truncate(at),
        2 => bytes.insert(at, [b'\n', b' ', b'0', b'{', b'"', 0xff][rng.below(6) as usize]),
        3 if at < len => {
            bytes.remove(at);
        }
        4 => bytes.extend(Vec::from([0u8; 1 + 64]).iter().take(1 + rng.below(64) as usize)),
        5 => {
            let garbage: Vec<u8> = (0..rng.below(300)).map(|_| rng.next() as u8).collect();
            bytes.splice(at..at, garbage);
        }
        6 if at < len => bytes[at] = rng.next() as u8,
        7 => {
            let copy: Vec<u8> = bytes[..at].to_vec();
            bytes.extend(copy);
            return len;
        }
        _ => {
            let end = (at + rng.below(200) as usize).min(len);
            bytes.drain(at..end);
        }
    }
    at.min(bytes.len())
}

#[test]
fn property_damaged_journals_never_panic_and_never_gain_a_frame() {
    for seed in seeds(300) {
        let (journal, _) = random_journal(seed, 60);
        let original = stream(&journal.frames);
        let mut rng = Rng::new(seed ^ 0x5eed);
        for round in 0..20 {
            let mut bytes = original.clone();
            let from = mutate(&mut rng, &mut bytes);
            let decoded = decode_stream(&bytes);
            // Every decoded frame is canonical, contiguous from the start, and inside the bytes.
            let mut at = 0u64;
            for frame in decoded.frames() {
                assert_eq!(frame.offset(), at, "seed {seed} round {round}");
                let encoded = encode_frame(frame.events()).unwrap();
                assert_eq!(&bytes[at as usize..frame.end() as usize], encoded.as_slice(), "seed {seed} round {round}");
                at = frame.end();
            }
            // Frames that end before the first changed byte decode exactly as before.
            let untouched = journal.frames.iter().scan(0usize, |end, frame| {
                *end += encode_frame(frame).unwrap().len();
                Some(*end)
            });
            let kept = untouched.take_while(|end| *end <= from).count();
            assert!(decoded.frames().len() >= kept, "seed {seed} round {round}: lost an untouched frame");
            assert_eq!(frames_of(&decoded)[..kept], journal.frames[..kept], "seed {seed} round {round}");
            // The tail rule: an incomplete tail has no newline and is at most one frame long.
            if let StreamEnd::IncompleteTail { offset, len, crc32c: crc } = decoded.end() {
                let tail = &bytes[offset as usize..];
                assert_eq!(tail.len(), len as usize);
                assert!(!tail.contains(&b'\n') && len <= MAX_TAIL_BYTES);
                assert_eq!(crc, crc32c(tail));
            }
            let _ = scan_after_corruption(&bytes, decoded.valid_end());
            let _ = verify_persisted(&bytes, &original, &bytes);
        }
    }
}

#[test]
fn property_a_single_bit_flip_is_always_detected() {
    let journal = golden_supervised();
    let bytes = stream(&journal.frames);
    let ends: Vec<usize> = journal.frames.iter().scan(0, |at, frame| {
        *at += encode_frame(frame).unwrap().len();
        Some(*at)
    }).collect();
    let mut rng = Rng::new(7);
    for at in 0..bytes.len() {
        let mut damaged = bytes.clone();
        damaged[at] ^= 1 << rng.below(8);
        let decoded = decode_stream(&damaged);
        let frame = ends.iter().position(|end| at < *end).unwrap();
        let start = if frame == 0 { 0 } else { ends[frame - 1] };
        assert_eq!(frames_of(&decoded), journal.frames[..frame], "flip at {at}");
        assert!(matches!(decoded.end(), StreamEnd::Corrupt { offset, .. } if offset == start as u64), "flip at {at}: {:?}", decoded.end());
    }
}

#[test]
fn property_random_bytes_never_panic() {
    let mut rng = Rng::new(0xbad);
    for _ in 0..2_000 {
        let len = rng.below(400) as usize;
        let mut bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        if rng.percent(50) {
            // A valid header around random bytes, so that the payload parser is reached.
            bytes = reframe(&bytes);
        }
        let decoded = decode_stream(&bytes);
        assert!(decoded.frames().iter().all(|frame| !frame.events().is_empty()));
        let _ = scan_after_corruption(&bytes, 0);
    }
}
