// Replay tests (D3.2b-4). Pure: journals are synthetic bytes encoded here; nothing reads a file, a clock or a process.
// Covers replay against the live projection, every recorded-history fault, the derived status of a recorded history,
// crash recovery through b-3's reconciliation, and that nothing replayed becomes usable authority.
use super::*;
use crate::lifecycle::codec::tests::{frames_of, golden_not_started, golden_reconciled, golden_supervised, random_journal, reframe, payload_of, stream, Rng};
use crate::lifecycle::codec::{crc32c, decode_stream, encode_frame, scan_after_corruption, CorruptReason, StreamEnd};
use crate::lifecycle::live::{begin_new, begin_reconciled, open, BeginRefused, LiveSupervision, Opening};
use crate::lifecycle::model::*;
use crate::lifecycle::reconcile::{
    reconcile, start_conflicts, summarize, HistoryRefused, HistoryStatus, MarkerState, OpeningEvidence, QuarantineReason, ReadBasis,
    ReconcileInput, RecordedHistory, RunInventory, StartConflict,
};
use crate::lifecycle::reduce::tests::*;

const EPOCH: LiveEpoch = LiveEpoch::new(21);
const MONO: u64 = 70_000;
const AT: u64 = T0 + 900_000;

fn replayed(bytes: &[u8]) -> ReplayedHistory {
    replay(run_id(), &decode_stream(bytes))
}

/// The ends of each frame of `frames`, as byte offsets, with the last sequence number.
fn ends(frames: &[Vec<Event>]) -> Vec<(u64, u64)> {
    let mut at = 0;
    frames
        .iter()
        .map(|frame| {
            at += encode_frame(frame).unwrap().len() as u64;
            (at, frame.last().unwrap().seq.number())
        })
        .collect()
}

/// The high-water mark after the first `count` frames were acknowledged.
fn mark_after(frames: &[Vec<Event>], count: usize) -> MarkReading {
    match count {
        0 => MarkReading::Read { last_seq: 0, byte_length: 0 },
        _ => {
            let (byte_length, last_seq) = ends(frames)[count - 1];
            MarkReading::Read { last_seq, byte_length }
        }
    }
}

fn evidence(mark: MarkReading, repairs: Vec<JournalRepair>) -> OpeningEvidence {
    OpeningEvidence { marker: MarkerState::Absent, mark, repairs }
}

/// The recorded history of a whole, intact journal whose every frame was acknowledged.
fn intact(journal: &Journal) -> RecordedHistory {
    let bytes = stream(&journal.frames);
    match RecordedHistory::from_replay(replayed(&bytes), evidence(mark_after(&journal.frames, journal.frames.len()), Vec::new())) {
        Ok(history) => history,
        Err(refused) => panic!("{refused:?}"),
    }
}

fn input(projection: &RunProjection) -> ReconcileInput {
    ReconcileInput { epoch: EPOCH, mono: MonoMs::new(MONO), stated_at_ms: AT, size: JournalSize { events: projection.events().len() as u32, bytes: 0 } }
}

fn tail_repair(sidecar: u32, tail: &[u8]) -> JournalRepair {
    JournalRepair::new(sidecar, tail.len() as u32, crc32c(tail), RepairCause::IncompleteTail).unwrap()
}

fn reconciler_bodies(history: &RecordedHistory) -> Vec<EventBody> {
    reconcile(history, &[], input(history.projection())).unwrap().frame.drafts().iter().map(|draft| draft.body().clone()).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Replay against the live projection

#[test]
fn replay_rebuilds_the_live_projection_exactly() {
    for (name, journal) in [
        ("proposed", proposed()),
        ("approved", approved()),
        ("starting", starting()),
        ("running", running()),
        ("supervised", golden_supervised()),
        ("reconciled", golden_reconciled()),
        ("not started", golden_not_started()),
    ] {
        let bytes = stream(&journal.frames);
        let first = replayed(&bytes);
        assert_eq!(first.fault(), None, "{name}");
        assert_eq!(first.tail(), None, "{name}");
        assert_eq!(first.projection(), &journal.projection, "{name}");
        assert_eq!(first.valid_end(), bytes.len() as u64, "{name}");
        // Deterministic: the same bytes give the same history.
        assert_eq!(replayed(&bytes).projection(), first.projection(), "{name}");
    }
}

#[test]
fn property_replay_matches_the_live_projection_after_every_frame() {
    for seed in 0..150 {
        let (journal, after) = random_journal(seed, 120);
        // `after[k]` is the projection once the first k + 1 frames are recorded.
        assert_eq!(after.len(), journal.frames.len(), "seed {seed}");
        let bytes = stream(&journal.frames);
        let full = replayed(&bytes);
        assert_eq!(full.fault(), None, "seed {seed}");
        assert_eq!(full.projection(), &journal.projection, "seed {seed}");
        let mut rng = Rng::new(seed);
        let boundaries = ends(&journal.frames);
        for _ in 0..6 {
            let count = 1 + rng.below(journal.frames.len() as u64) as usize;
            let cut = boundaries[count - 1].0 as usize;
            assert_eq!(replayed(&bytes[..cut]).projection(), &after[count - 1], "seed {seed}, {count} frames");
        }
    }
}

#[test]
fn replay_matches_frames_staged_and_confirmed_by_a_live_supervision() {
    let mono = MonoMs::new;
    let pending = begin_new(EPOCH, run_id(), created(T0), mono(0)).unwrap();
    let mut frames = Vec::from([pending.events().to_vec()]);
    let mut live = open(pending);
    let commit = |live: &mut LiveSupervision, drafts: Vec<EventDraft>, grant, at: u64, frames: &mut Vec<Vec<Event>>| {
        let size = JournalSize { events: live.projection().events().len() as u32, bytes: 0 };
        let staged = live.stage(&drafts, grant, mono(at), size).map_err(|refused| refused.refusal).unwrap();
        frames.push(staged.events().to_vec());
        live.confirm(staged).unwrap()
    };
    commit(&mut live, attempt_pair(1, 1, T0 + 1_000), None, 1_000, &mut frames);
    let mut grants = commit(&mut live, Vec::from([decide(1, Decision::Granted, T0 + 2_000)]), None, 2_000, &mut frames);
    commit(&mut live, Vec::from([begin(1, 1, T0 + 3_000)]), grants.pop(), 3_000, &mut frames);
    commit(&mut live, Vec::from([started(1, T0 + 4_000)]), None, 4_000, &mut frames);
    commit(&mut live, Vec::from([operation(2, 1, T0 + 5_000)]), None, 5_000, &mut frames);
    commit(&mut live, Vec::from([cancel(CancelScope::Run, T0 + 6_000)]), None, 6_000, &mut frames);
    let bytes = stream(&frames);
    assert_eq!(replayed(&bytes).projection(), live.projection());
    assert_eq!(frames_of(&decode_stream(&bytes)), frames);
}

// ---------------------------------------------------------------------------------------------------------------
// Faults: the first one stops replay, and nothing after it is applied

/// Replays `frames` and checks that it faults with `fault` at frame `at`, keeping exactly the projection before it.
fn faults(name: &str, frames: &[Vec<Event>], at: usize, fault: JournalFault) {
    let history = replayed(&stream(frames));
    assert_eq!(history.fault(), Some(fault), "{name}");
    let before = replayed(&stream(&frames[..at]));
    assert_eq!(history.projection(), before.projection(), "{name}");
    assert_eq!(history.valid_end(), before.valid_end(), "{name}");
}

#[test]
fn every_recorded_history_fault_quarantines_at_its_frame() {
    let journal = golden_supervised();
    let frames = journal.frames.clone();
    let edit = |index: usize, change: &dyn Fn(&mut Vec<Event>)| {
        let mut copy = frames.clone();
        change(&mut copy[index]);
        copy
    };
    let stop_frame = frames.iter().position(|frame| matches!(frame[0].body, EventBody::StopRequested { .. })).unwrap();
    let exit_frame = frames.iter().position(|frame| matches!(frame[0].body, EventBody::AttemptExited { .. })).unwrap();

    let mut other_run = frames.clone();
    for event in &mut other_run[0] {
        event.run = RunId::new(8).unwrap();
    }
    faults("another run", &other_run, 0, JournalFault::WrongRun);
    faults("a missing first frame", &frames[1..], 0, JournalFault::SeqNotFirst);
    let gap: Vec<Vec<Event>> = frames.iter().enumerate().filter(|(i, _)| *i != 3).map(|(_, f)| f.clone()).collect();
    faults("a missing frame", &gap, 3, JournalFault::SeqGap);
    let mut repeated = frames.clone();
    repeated.insert(4, frames[3].clone());
    faults("a repeated frame", &repeated, 4, JournalFault::SeqGap);
    let mut swapped = frames.clone();
    swapped.swap(3, 4);
    faults("swapped frames", &swapped, 3, JournalFault::SeqGap);
    faults("a decreasing time", &edit(5, &|f| f[0].at_ms = WallMs::new(T0).unwrap()), 5, JournalFault::TimeDecreased);
    faults("a clamped flag on a new time", &edit(5, &|f| f[0].clamped = true), 5, JournalFault::ClampedMisused);
    // A forged producer label on a recorded event.
    faults("a relayed exit", &edit(exit_frame, &|f| f[0].producer = Producer::ProviderRelay), exit_frame, JournalFault::ProducerNotAllowed);
    faults("a user-decided start", &edit(3, &|f| f[0].producer = Producer::User), 3, JournalFault::ProducerNotAllowed);
    // Generated consequences must be exactly the generated ones: reason, producer, presence and order.
    faults("another consequence reason", &edit(stop_frame, &|f| f[1].body = EventBody::ApprovalVoided { approval: a(4), reason: VoidReason::Cancelled }), stop_frame, JournalFault::ConsequenceMismatch);
    faults("a dropped consequence", &edit(stop_frame, &|f| { f.pop(); }), stop_frame, JournalFault::ConsequenceMismatch);
    faults("another consequence producer", &edit(stop_frame, &|f| f[1].producer = Producer::Reconciler), stop_frame, JournalFault::ConsequenceMismatch);
    let extra = |f: &mut Vec<Event>| {
        let mut more = f[1].clone();
        more.seq = Seq::new(more.seq.number() + 1).unwrap();
        more.body = EventBody::ApprovalVoided { approval: a(2), reason: VoidReason::AttemptEnded };
        f.push(more);
    };
    let mut shifted = edit(stop_frame, &extra);
    for frame in &mut shifted[stop_frame + 1..] {
        for event in frame {
            event.seq = Seq::new(event.seq.number() + 1).unwrap();
        }
    }
    assert_ne!(replayed(&stream(&shifted)).fault(), None, "an extra consequence");
    // Duplicate semantic keys and conflicting terminal states.
    let after = |extra: EventBody, producer: Producer| {
        let mut copy = frames[..=exit_frame].to_vec();
        let last = copy.last().unwrap()[0].clone();
        copy.push(Vec::from([Event { seq: Seq::new(last.seq.number() + 1).unwrap(), producer, body: extra, ..last }]));
        copy
    };
    let exit_again = frames[exit_frame][0].body.clone();
    faults("a second exit", &after(exit_again, Producer::Supervisor), exit_frame + 1, JournalFault::DuplicateKey);
    let interrupted = EventBody::AttemptInterrupted { attempt: n(1), observation: InterruptObservation::Unknown };
    faults("a conflicting terminal state", &after(interrupted, Producer::Reconciler), exit_frame + 1, JournalFault::DuplicateKey);
    let started_again = frames[4][0].body.clone();
    faults("a second start", &after(started_again, Producer::Supervisor), exit_frame + 1, JournalFault::DuplicateKey);
    // An invalid transition: a start recorded before any start began.
    let early = proposed();
    let mut invalid = early.frames.clone();
    let last = invalid.last().unwrap().last().unwrap().clone();
    invalid.push(Vec::from([Event {
        seq: Seq::new(last.seq.number() + 1).unwrap(),
        producer: Producer::Supervisor,
        body: EventBody::AttemptStarted { attempt: n(1), os_identity: identity() },
        ..last
    }]));
    faults("a start before it began", &invalid, invalid.len() - 1, JournalFault::InvalidTransition);
    // A stored progress report the coalescing rule would have dropped.
    let mut progress = frames[..7].to_vec();
    let last = progress[6][0].clone();
    progress.push(Vec::from([Event { seq: Seq::new(last.seq.number() + 1).unwrap(), at_ms: WallMs::new(last.at_ms.ms() + 1).unwrap(), ..last }]));
    faults("a coalesced progress", &progress, 7, JournalFault::CoalesceViolated);
    // A frame mixing producers.
    let mut mixed = frames[..7].to_vec();
    let report = mixed[6][0].clone();
    let stop_event = Event {
        seq: Seq::new(report.seq.number() + 1).unwrap(),
        producer: Producer::Supervisor,
        body: EventBody::StopRequested { attempt: n(1), reason: StopReason::JournalFull },
        ..report.clone()
    };
    mixed[6].push(stop_event);
    faults("a mixed frame", &mixed, 6, JournalFault::MixedProducerFrame);
}

#[test]
fn damaged_bytes_and_newer_versions_quarantine_without_touching_the_bytes() {
    let journal = golden_supervised();
    let bytes = stream(&journal.frames);
    let boundaries = ends(&journal.frames);
    // A corrupted middle frame.
    let mut damaged = bytes.clone();
    damaged[boundaries[4].0 as usize + 40] ^= 0x04;
    let history = replayed(&damaged);
    assert_eq!(history.fault(), Some(JournalFault::Codec));
    assert_eq!(history.damage(), Some(CorruptReason::Checksum));
    assert_eq!(history.projection(), replayed(&bytes[..boundaries[4].0 as usize]).projection());
    // The audit scan sees the later frames; replay never applies them.
    let (later, _) = scan_after_corruption(&damaged, boundaries[4].0);
    assert!(!later.is_empty());
    assert_eq!(history.projection().events().len(), boundaries[4].1 as usize);
    // A newer version.
    let created = &journal.frames[0][0];
    let payload = String::from_utf8(payload_of(&encode_frame(std::slice::from_ref(created)).unwrap())).unwrap();
    let mut newer = stream(&journal.frames[..3]);
    newer.extend(reframe(payload.replacen("\"v\":1", "\"v\":2", 1).as_bytes()));
    newer.extend(stream(&journal.frames[3..]));
    let original = newer.clone();
    let history = replayed(&newer);
    assert_eq!(history.fault(), Some(JournalFault::NewerVersion));
    assert_eq!(history.damage(), None);
    assert_eq!(history.projection(), replayed(&bytes[..boundaries[2].0 as usize]).projection());
    assert_eq!(newer, original);
    // A complete final frame with a bad checksum is corruption, not a repairable tail.
    let mut last = bytes.clone();
    let at = last.len() - 4;
    last[at] ^= 0x01;
    let history = replayed(&last);
    assert_eq!(history.fault(), Some(JournalFault::Codec));
    assert_eq!(history.tail(), None);
}

// ---------------------------------------------------------------------------------------------------------------
// The derived status of a recorded history

#[test]
fn a_recorded_history_is_verified_only_when_the_bytes_show_it() {
    let journal = approved();
    let frames = &journal.frames;
    let bytes = stream(frames);
    let all = frames.len();
    let full = || evidence(mark_after(frames, all), Vec::new());
    let history = RecordedHistory::from_replay(replayed(&bytes), full()).unwrap();
    assert_eq!(history.status(), HistoryStatus::Verified);
    assert_eq!(history.projection(), &journal.projection);
    // A quarantine marker quarantines, whatever the bytes say.
    let marked = RecordedHistory::from_replay(replayed(&bytes), OpeningEvidence { marker: MarkerState::Present, ..full() }).unwrap();
    assert_eq!(marked.status(), HistoryStatus::Quarantined(QuarantineReason::Marker));
    // A fault quarantines.
    let mut damaged = bytes.clone();
    damaged[20] ^= 0x01;
    let faulted = RecordedHistory::from_replay(replayed(&damaged), full()).unwrap();
    assert_eq!(faulted.status(), HistoryStatus::Quarantined(QuarantineReason::Fault(JournalFault::Codec)));
    // The high-water mark.
    let refused = |bytes: &[u8], mark: MarkReading| RecordedHistory::from_replay(replayed(bytes), evidence(mark, Vec::new())).err();
    assert_eq!(refused(&bytes, MarkReading::Missing), Some(HistoryRefused::Mark(MarkProblem::Missing)));
    assert_eq!(refused(&bytes, MarkReading::Unreadable), Some(HistoryRefused::Mark(MarkProblem::Unreadable)));
    assert_eq!(refused(&bytes, MarkReading::Read { last_seq: 0, byte_length: 10 }), Some(HistoryRefused::Mark(MarkProblem::Invalid)));
    assert_eq!(refused(&bytes, MarkReading::Read { last_seq: 3, byte_length: 0 }), Some(HistoryRefused::Mark(MarkProblem::Invalid)));
    // Acknowledged frames lost at a frame boundary: the shorter journal still decodes clean, and the mark catches it.
    let shorter = stream(&frames[..all - 1]);
    assert_eq!(decode_stream(&shorter).end(), StreamEnd::Clean);
    assert_eq!(refused(&shorter, mark_after(frames, all)), Some(HistoryRefused::Mark(MarkProblem::NotReached)));
    let (end, seq) = ends(frames)[1];
    assert_eq!(refused(&bytes, MarkReading::Read { last_seq: seq + 1, byte_length: end }), Some(HistoryRefused::Mark(MarkProblem::Mismatch)));
    assert_eq!(refused(&bytes, MarkReading::Read { last_seq: seq, byte_length: end - 1 }), Some(HistoryRefused::Mark(MarkProblem::Mismatch)));
    // Frames beyond the mark were never acknowledged: an earlier mark still vouches for the prefix.
    assert!(refused(&bytes, mark_after(frames, 1)).is_none());
    assert!(refused(&bytes, mark_after(frames, 0)).is_none());
    // No RunCreated: the adapter's never-created path (F6).
    assert_eq!(refused(b"", mark_after(frames, 0)), Some(HistoryRefused::NoRunCreated));
}

#[test]
fn an_incomplete_tail_must_be_matched_by_its_repair_record() {
    let journal = approved();
    let frames = &journal.frames;
    let all = frames.len();
    let mut bytes = stream(frames);
    let lost = encode_frame(&begin_frame(&journal)).unwrap();
    bytes.extend_from_slice(&lost[..lost.len() - 7]);
    let tail = &lost[..lost.len() - 7];
    let history = replayed(&bytes);
    assert_eq!(history.tail(), Some(TailBytes { offset: ends(frames)[all - 1].0, len: tail.len() as u32, crc32c: crc32c(tail) }));
    let mark = || mark_after(frames, all);
    let open = |repairs: Vec<JournalRepair>| RecordedHistory::from_replay(replayed(&bytes), evidence(mark(), repairs));
    assert_eq!(open(Vec::new()).err(), Some(HistoryRefused::TailNotRepaired));
    let wrong_length = JournalRepair::new(1, tail.len() as u32 - 1, crc32c(tail), RepairCause::IncompleteTail).unwrap();
    assert_eq!(open(Vec::from([wrong_length])).err(), Some(HistoryRefused::TailNotRepaired));
    let wrong_crc = JournalRepair::new(1, tail.len() as u32, crc32c(tail) ^ 1, RepairCause::IncompleteTail).unwrap();
    assert_eq!(open(Vec::from([wrong_crc])).err(), Some(HistoryRefused::TailNotRepaired));
    // With the matching record the history is verified, and reconciliation can never say "not started": the torn frame
    // may have held the start.
    let history = open(Vec::from([tail_repair(1, tail)])).unwrap();
    assert_eq!(history.status(), HistoryStatus::Verified);
    let bodies = reconciler_bodies(&history);
    assert!(matches!(bodies[0], EventBody::JournalRepaired(_)));
    assert_eq!(bodies[1], EventBody::AttemptInterrupted { attempt: n(1), observation: InterruptObservation::EvidenceLost });
    // Without the tail, the same journal reconciles as never started: the contrast is the repair record.
    let clean = intact(&journal);
    assert_eq!(reconciler_bodies(&clean), [EventBody::AttemptNotStarted { attempt: n(1), reason: NotStartedCause::SupervisionLost }]);
}

/// The frame that would begin attempt 1 of `journal` (approved).
fn begin_frame(journal: &Journal) -> Vec<Event> {
    let mut copy = Journal { projection: journal.projection.clone(), frames: journal.frames.clone() };
    copy.one(begin(1, 1, T0 + 3_000)).events
}

#[test]
fn property_no_damaged_or_truncated_journal_reconciles_a_possible_start_as_not_started() {
    let mut fixtures: Vec<Journal> = Vec::from([proposed(), approved(), starting(), running()]);
    fixtures.extend((0..60).map(|seed| random_journal(seed, 40).0));
    let mut checked = 0;
    for (index, journal) in fixtures.iter().enumerate() {
        let bytes = stream(&journal.frames);
        let boundaries = ends(&journal.frames);
        let mut rng = Rng::new(index as u64);
        for _ in 0..25 {
            let cut = rng.below(bytes.len() as u64 + 1) as usize;
            let acknowledged = rng.below(journal.frames.len() as u64 + 1) as usize;
            let prefix = &bytes[..cut];
            let decoded = decode_stream(prefix);
            let repairs = match decoded.end() {
                StreamEnd::IncompleteTail { offset, .. } => Vec::from([tail_repair(1, &prefix[offset as usize..])]),
                _ => Vec::new(),
            };
            let result = RecordedHistory::from_replay(replay(run_id(), &decoded), evidence(mark_after(&journal.frames, acknowledged), repairs));
            let mark_end = if acknowledged == 0 { 0 } else { boundaries[acknowledged - 1].0 as usize };
            let history = match result {
                Ok(history) => history,
                Err(HistoryRefused::Mark(_)) => {
                    assert!(cut < mark_end, "fixture {index}: a prefix reaching the mark was refused");
                    continue;
                }
                Err(HistoryRefused::NoRunCreated) => {
                    assert!(decoded.frames().is_empty());
                    continue;
                }
                Err(refused) => panic!("fixture {index}: {refused:?}"),
            };
            // A verified history always reaches the mark: no acknowledged frame was lost.
            assert!(cut >= mark_end, "fixture {index}: verified below the mark");
            let Ok(reconciliation) = reconcile(&history, &[], input(history.projection())) else { continue };
            for draft in reconciliation.frame.drafts() {
                if let EventBody::AttemptNotStarted { attempt, .. } = draft.body() {
                    checked += 1;
                    // Only with a clean stream, and only when no start of that attempt is in what was read.
                    assert_eq!(decoded.end(), StreamEnd::Clean, "fixture {index}: not started after a torn tail");
                    let began = decoded.frames().iter().flat_map(|frame| frame.events()).any(|event| matches!(event.body, EventBody::StartBegun { attempt: a, .. } if a == *attempt));
                    assert!(!began, "fixture {index}: not started after a recorded start");
                }
            }
        }
    }
    assert!(checked > 0, "the property never reached a not-started outcome");
}

// ---------------------------------------------------------------------------------------------------------------
// Crash recovery through b-3

#[test]
fn a_replayed_history_reconciles_opens_and_its_opening_frame_replays() {
    let journal = starting();
    let history = intact(&journal);
    let reconciliation = reconcile(&history, &[], input(history.projection())).unwrap();
    // A possible start without identity: interrupted, never "not started", with a blocking orphan.
    assert_eq!(
        reconciliation.frame.drafts()[0].body(),
        &EventBody::AttemptInterrupted { attempt: n(1), observation: InterruptObservation::NoIdentity }
    );
    assert_eq!(reconciliation.blocking.len(), 1);
    // Unreconciled, the run blocks every start.
    let inventory = RunInventory::Enumerated { allocated: 7, summaries: Vec::new() };
    assert!(!start_conflicts(&inventory, &[]).is_empty());
    let summaries = |history: &RecordedHistory| {
        let mut all: Vec<_> = (1..7).map(|k| (RunId::new(k).unwrap(), crate::lifecycle::reconcile::RunSummary::NeverCreated)).collect();
        all.push((run_id(), summarize(history, ReadBasis::Locked)));
        RunInventory::Enumerated { allocated: 7, summaries: all }
    };
    assert_eq!(start_conflicts(&summaries(&history), &[]), [StartConflict::Unreconciled(run_id(), n(1))]);
    let size = JournalSize { events: history.projection().events().len() as u32, bytes: 0 };
    let frame = reconciliation.frame;
    let Ok(Opening::Pending(pending)) = begin_reconciled(EPOCH, history, frame, MonoMs::new(MONO), size) else { panic!("not pending") };
    // The opening frame encodes, appends and replays to the opened projection.
    let mut frames = journal.frames.clone();
    frames.push(pending.events().to_vec());
    let live = open(pending);
    let bytes = stream(&frames);
    assert_eq!(replayed(&bytes).projection(), live.projection());
    // Reconciling the result again drafts nothing (idempotent), and the orphan still blocks every start.
    let reopened = RecordedHistory::from_replay(replayed(&bytes), evidence(mark_after(&frames, frames.len()), Vec::new())).unwrap();
    assert!(reconcile(&reopened, &[], input(reopened.projection())).unwrap().frame.drafts().is_empty());
    assert_eq!(start_conflicts(&summaries(&reopened), &[]), [StartConflict::PossibleOrphan(run_id(), n(1))]);
}

#[test]
fn a_quarantined_replay_is_never_reconciled_or_opened_and_blocks_every_start() {
    let journal = running();
    let mut bytes = stream(&journal.frames);
    let at = bytes.len() - 30;
    bytes[at] ^= 0x10;
    let history = RecordedHistory::from_replay(replayed(&bytes), evidence(mark_after(&journal.frames, 1), Vec::new())).unwrap();
    assert_eq!(history.status(), HistoryStatus::Quarantined(QuarantineReason::Fault(JournalFault::Codec)));
    let reconciliation = reconcile(&history, &[], input(history.projection())).unwrap();
    assert!(reconciliation.frame.drafts().is_empty());
    let summary = summarize(&history, ReadBasis::Locked);
    let mut summaries: Vec<_> = (1..7).map(|k| (RunId::new(k).unwrap(), crate::lifecycle::reconcile::RunSummary::NeverCreated)).collect();
    summaries.push((run_id(), summary));
    let conflicts = start_conflicts(&RunInventory::Enumerated { allocated: 7, summaries }, &[]);
    assert_eq!(conflicts, [StartConflict::Quarantined(run_id(), QuarantineReason::Fault(JournalFault::Codec))]);
    let size = JournalSize { events: 0, bytes: 0 };
    let refused = begin_reconciled(EPOCH, history, reconciliation.frame, MonoMs::new(MONO), size).err();
    assert!(matches!(refused, Some(BeginRefused::Quarantined(_))));
}

// ---------------------------------------------------------------------------------------------------------------
// No restored authority

#[test]
fn replayed_decisions_and_grants_never_become_usable() {
    // A running attempt with a granted, unused operation approval, and a granted start approval of a later run state.
    let mut journal = running();
    journal.one(operation(2, 1, T0 + 5_000));
    journal.one(decide(2, Decision::Granted, T0 + 6_000));
    assert_eq!(journal.approval(2), ApprovalState::Granted);
    let history = intact(&journal);
    assert_eq!(history.projection().approval(a(2)).unwrap().state(), ApprovalState::Granted);
    let reconciliation = reconcile(&history, &[], input(history.projection())).unwrap();
    let size = JournalSize { events: history.projection().events().len() as u32, bytes: 0 };
    let Ok(Opening::Pending(pending)) = begin_reconciled(EPOCH, history, reconciliation.frame, MonoMs::new(MONO), size) else { panic!() };
    let mut live = open(pending);
    // The grant was voided by the opening frame; nothing is open, and nothing can be consumed without a live grant.
    assert_eq!(live.projection().approval(a(2)).unwrap().state(), ApprovalState::Voided);
    assert!(live.projection().approvals().iter().all(|record| !record.state().is_open()));
    let size = JournalSize { events: live.projection().events().len() as u32, bytes: 0 };
    assert!(live.stage(&[consumed(2, AT)], None, MonoMs::new(MONO + 1), size).is_err());
    // A start approval granted before a crash cannot start the attempt after it.
    let history = intact(&approved());
    let reconciliation = reconcile(&history, &[], input(history.projection())).unwrap();
    let size = JournalSize { events: history.projection().events().len() as u32, bytes: 0 };
    let Ok(Opening::Pending(pending)) = begin_reconciled(EPOCH, history, reconciliation.frame, MonoMs::new(MONO), size) else { panic!() };
    let mut live = open(pending);
    let size = JournalSize { events: live.projection().events().len() as u32, bytes: 0 };
    assert!(live.stage(&[begin(1, 1, AT)], None, MonoMs::new(MONO + 1), size).is_err());
    assert_eq!(live.projection().attempt(n(1)).unwrap().state(), AttemptState::NotStarted(NotStartedReason::SupervisionLost));
}

#[test]
fn replaying_the_same_bytes_twice_gives_the_same_derived_history() {
    for journal in [golden_supervised(), golden_reconciled(), golden_not_started()] {
        let first = intact(&journal);
        let second = intact(&journal);
        assert_eq!(first.status(), second.status());
        assert_eq!(first.projection(), second.projection());
        assert_eq!(first.unrecorded_repairs(), second.unrecorded_repairs());
    }
}

#[test]
fn contradictory_repair_evidence_never_reconciles() {
    // The golden reconciled history records sidecar 1; reporting sidecar 1 again with other content, or reporting one
    // sidecar twice, is refused by reconciliation, so the run stays unreconciled and blocks every start.
    let journal = golden_reconciled();
    let bytes = stream(&journal.frames);
    let other = JournalRepair::new(1, 41, 7, RepairCause::IncompleteTail).unwrap();
    let mark = mark_after(&journal.frames, journal.frames.len());
    let history = RecordedHistory::from_replay(replayed(&bytes), evidence(mark, Vec::from([other]))).unwrap();
    assert_eq!(reconcile(&history, &[], input(history.projection())).err(), Some(crate::lifecycle::reconcile::ReconcileRefused::RepairConflict));
    let twice = Vec::from([JournalRepair::new(2, 9, 1, RepairCause::IncompleteTail).unwrap(), JournalRepair::new(2, 9, 1, RepairCause::IncompleteTail).unwrap()]);
    let history = RecordedHistory::from_replay(replayed(&bytes), evidence(mark, twice)).unwrap();
    assert_eq!(reconcile(&history, &[], input(history.projection())).err(), Some(crate::lifecycle::reconcile::ReconcileRefused::RepairConflict));
    // A torn tail plus one repair left by an earlier crash: both are recorded first, in the order reported.
    let journal = approved();
    let mut bytes = stream(&journal.frames);
    bytes.extend_from_slice(b"ORDJ1 12");
    let earlier = JournalRepair::new(1, 30, 5, RepairCause::IncompleteTail).unwrap();
    let repairs = Vec::from([earlier, tail_repair(2, b"ORDJ1 12")]);
    let history = RecordedHistory::from_replay(replayed(&bytes), evidence(mark_after(&journal.frames, journal.frames.len()), repairs)).unwrap();
    let bodies = reconciler_bodies(&history);
    assert!(matches!((&bodies[0], &bodies[1]), (EventBody::JournalRepaired(first), EventBody::JournalRepaired(second)) if first.sidecar() == 1 && second.sidecar() == 2));
    assert_eq!(bodies[2], EventBody::AttemptInterrupted { attempt: n(1), observation: InterruptObservation::EvidenceLost });
}

#[test]
fn a_landed_uncertain_decision_is_adopted_without_a_grant() {
    use crate::lifecycle::codec::{verify_persisted, PersistCheck};
    let mono = MonoMs::new;
    let pending = begin_new(EPOCH, run_id(), created(T0), mono(0)).unwrap();
    let mut frames = Vec::from([pending.events().to_vec()]);
    let mut live = open(pending);
    let size = |live: &LiveSupervision| JournalSize { events: live.projection().events().len() as u32, bytes: 0 };
    let staged = live.stage(&attempt_pair(1, 1, T0 + 1_000), None, mono(1_000), size(&live)).map_err(|r| r.refusal).unwrap();
    frames.push(staged.events().to_vec());
    assert!(live.confirm(staged).unwrap().is_empty());
    // The decision's write is uncertain: the writer keeps its committed bytes and the frame it tried to write.
    let committed = stream(&frames);
    let staged = live.stage(&[decide(1, Decision::Granted, T0 + 2_000)], None, mono(2_000), size(&live)).map_err(|r| r.refusal).unwrap();
    let abandoned = live.abandon(staged).unwrap();
    let attempted = encode_frame(abandoned.events()).unwrap();
    // A re-read shows it landed: the frame is adopted, and still no grant exists for it.
    let mut file = committed.clone();
    file.extend_from_slice(&attempted);
    assert_eq!(verify_persisted(&file, &committed, &attempted), PersistCheck::Landed);
    live.adopt(abandoned).unwrap();
    assert_eq!(live.projection().approval(a(1)).unwrap().state(), ApprovalState::Granted);
    assert!(live.stage(&[begin(1, 1, T0 + 3_000)], None, mono(3_000), size(&live)).is_err());
    // The adopted projection is exactly what the file replays to.
    assert_eq!(replayed(&file).projection(), live.projection());
}
