// The canonical journal codec (D3.2b-4, issue #118; design r5 sections 4.5, 4.7 and 4.10, decision 12.9, gate-review
// finding F14). Pure: bytes in, values out, and values in, bytes out. Nothing here reads or writes a file, repairs,
// truncates or moves anything, or decides a transition: classifying bytes is all it does, and the reducer
// (`apply_recorded`, through `replay`) judges what the decoded events mean.
//
// This is the only lifecycle file that names serde: the wire types below are its own, model types derive nothing, and
// the canonical form is enforced by re-encoding every decoded payload from the model values and comparing bytes.
//
// A frame is one line, so a journal stays readable with text tools:
//
//     ORDJ1 <payload length: 1-5 decimal digits, no leading zero> <crc32c: 8 lowercase hex> <payload>\n
//
// The CRC-32C covers `<length> <payload>`. The payload is a canonical JSON array of 1 to MAX_BATCH events.
//
// Classification of a stream (r5 4.7): `Clean` (every frame valid, ending right after a newline); `IncompleteTail` (the
// bytes after the last valid frame hold no newline at all and are at most MAX_TAIL_BYTES long, and are not a complete
// frame with a valid checksum and a wrong last byte); `NewerVersion` (a valid frame holding a record of a version this
// code does not know, F14); and `Corrupt` for everything else. A complete frame whose checksum is wrong is always
// corruption, never a torn tail, even when it is the last frame. Nothing after a corrupt frame is interpreted:
// `scan_after_corruption` lists later frames for audit only, as values that hold no event.
use serde::{Deserialize, Serialize};

use super::model::{
    Actor, ApprovalId, ApprovalKind, ApprovalRequest, AttemptNo, AuthorityScope, Binding, BindingTarget,
    CancelScope, CatalogId, Channel, CloseReason, ConcernKind, ConcernRef, ConcernResolution, Continuation, DeadlineKind,
    Decision, DecisionProvenance, Digest, Event, EventBody, EventKind, ExitStatus, ExpiryBasis, InterruptObservation,
    JournalRepair, Limits, NotStartedCause, OperationClass, OsIdentity, OutcomeClaim, PermissionClass, Producer,
    ProgressKind, ProjectId, ProjectRef, ProviderIdentity, RepairCause, RequestFields, Resolution, ResolutionProvenance,
    RiskClass, RunContext, RunId, RunSpec, Seq, ShortToken, StartFailCode, StopReason, TaskId, TaskRef, VoidReason,
    WallMs, WithdrawalReason, WorkspaceMode, MAX_BATCH, MAX_EVENT_BYTES, MAX_FRAME_BYTES, MAX_JOURNAL_BYTES,
    MAX_TAIL_BYTES, MAX_WALL_MS, RECORD_VERSION,
};

// ---------------------------------------------------------------------------------------------------------------
// Limits of the byte format

const MAGIC: &[u8] = b"ORDJ1 ";
const MAX_LENGTH_DIGITS: usize = 5;
const CHECKSUM_DIGITS: usize = 8;
/// JSON arrays and objects may nest at most this deep in a v1 payload. The deepest v1 record nests six levels (the
/// payload array, the event, its body, a run specification, its context, its limits).
const MAX_NESTING: usize = 8;
/// The nesting bound of every ORDJ1 payload, whatever its version, checked by a byte scan before serde parses anything
/// (the version peek included). Future versions must keep it, with the frame format, so that a newer record is always
/// read as newer rather than as damage.
const MAX_FRAME_NESTING: usize = 64;
/// The largest stream this codec reads. A journal accepts nothing at or above MAX_JOURNAL_BYTES + SAFETY_JOURNAL_BYTES
/// (D3.2c-1), so beyond that only the frame that crossed it and one torn tail can follow; one MiB above
/// MAX_JOURNAL_BYTES covers both with room to spare. A longer stream is not decoded at all.
pub(crate) const MAX_STREAM_BYTES: u64 = MAX_JOURNAL_BYTES + 1024 * 1024;
/// `scan_after_corruption` lists at most this many later frames.
pub(crate) const MAX_UNVERIFIED_FRAMES: usize = 1024;

// ---------------------------------------------------------------------------------------------------------------
// CRC-32C (Castagnoli, reflected polynomial 0x82F63B78), bitwise: no table, no dependency.

/// The CRC-32C of `bytes`; `crc32c(b"123456789") == 0xe3069283`.
pub(crate) fn crc32c(bytes: &[u8]) -> u32 {
    !crc32c_update(!0, bytes)
}

fn crc32c_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0x82F6_3B78 } else { crc >> 1 };
        }
    }
    crc
}

/// The checksum of a frame: over its length digits, one space and its payload.
fn frame_crc(digits: &[u8], payload: &[u8]) -> u32 {
    !crc32c_update(crc32c_update(crc32c_update(!0, digits), b" "), payload)
}

// ---------------------------------------------------------------------------------------------------------------
// Results

/// Why bytes are not a valid journal at an offset. Every reason quarantines the run; none is ever repaired here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CorruptReason {
    /// Not the `ORDJ1 ` magic.
    Magic,
    /// A length field that is empty, longer than five digits, has a leading zero, or holds another character.
    LengthField,
    /// A length above MAX_FRAME_BYTES.
    FrameTooLarge,
    /// A checksum field that is not eight lowercase hexadecimal digits followed by a space.
    ChecksumField,
    /// A complete frame whose CRC-32C does not match.
    Checksum,
    /// A complete frame with a valid checksum whose last byte is not a newline.
    FrameEnd,
    /// The length points past the end of the stream, and the bytes after the last valid frame hold a newline.
    CutFrame,
    /// Bytes after the last valid frame with no newline, longer than MAX_TAIL_BYTES: more than one frame's worth.
    TailTooLong,
    /// A stream longer than MAX_STREAM_BYTES.
    StreamTooLarge,
    /// JSON nested deeper than the closed v1 types allow.
    Nesting,
    /// An integer above 2^53 - 1.
    Integer,
    /// A payload that is not an array of v1 records: invalid UTF-8 or JSON, a missing or unknown field, a wrong type,
    /// a value its model constructor refuses, or a recorded risk or scope that is not the one Ordelis computes.
    Schema,
    /// An empty array of records.
    EmptyBatch,
    /// More than MAX_BATCH records.
    BatchTooLarge,
    /// A record whose canonical encoding exceeds MAX_EVENT_BYTES.
    EventTooLarge,
    /// A payload that decodes but is not byte for byte the canonical encoding of what it decodes to (whitespace, field
    /// order, escapes, number forms, duplicate keys).
    NotCanonical,
}

/// How a stream ends after its last valid frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StreamEnd {
    Clean,
    /// No newline at all and at most MAX_TAIL_BYTES: what a crash leaves after one unfinished frame. Never
    /// interpreted. Only a future adapter may move it (unchanged) to a sidecar, and only when the valid prefix reaches
    /// the run's high-water mark (r5 5.5, 5.7); `crc32c` lets the repair record be matched to exactly these bytes.
    IncompleteTail { offset: u64, len: u32, crc32c: u32 },
    Corrupt { offset: u64, reason: CorruptReason },
    /// A valid frame holding a record of a version above RECORD_VERSION (F14): read-only, never rewritten.
    NewerVersion { offset: u64, v: u64 },
}

/// One valid frame: its byte range and its records, decoded and canonical. Built only by `decode_stream`.
pub(crate) struct DecodedFrame {
    offset: u64,
    end: u64,
    events: Vec<Event>,
}

impl DecodedFrame {
    pub(crate) const fn offset(&self) -> u64 {
        self.offset
    }

    /// The offset just after the frame's newline.
    pub(crate) const fn end(&self) -> u64 {
        self.end
    }

    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }
}

/// A decoded stream: its valid frames in order, and how it ends. Built only by `decode_stream`.
pub(crate) struct Decoded {
    frames: Vec<DecodedFrame>,
    end: StreamEnd,
}

impl Decoded {
    pub(crate) fn frames(&self) -> &[DecodedFrame] {
        &self.frames
    }

    pub(crate) const fn end(&self) -> StreamEnd {
        self.end
    }

    /// The offset where the valid frames end.
    pub(crate) fn valid_end(&self) -> u64 {
        self.frames.last().map_or(0, DecodedFrame::end)
    }
}

/// Why `encode_frame` refuses events. The reducer never produces such a frame; a refusal is a caller bug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EncodeRefused {
    Empty,
    TooManyEvents,
    /// Events of different runs, or sequence numbers that are not contiguous.
    NotContiguous,
    /// Only the current version is written.
    NotCurrentVersion,
    EventTooLarge,
    FrameTooLarge,
    Unencodable,
}

// ---------------------------------------------------------------------------------------------------------------
// Encoding

/// One frame holding `events`, canonical, with its newline. The events must be one run's, contiguous, and of the
/// current version; this is a pure encoding, and judging the frame is the reducer's.
pub(crate) fn encode_frame(events: &[Event]) -> Result<Vec<u8>, EncodeRefused> {
    if events.is_empty() {
        return Err(EncodeRefused::Empty);
    }
    if events.len() > MAX_BATCH {
        return Err(EncodeRefused::TooManyEvents);
    }
    if events.windows(2).any(|pair| pair[1].run != pair[0].run || pair[1].seq.number() != pair[0].seq.number() + 1) {
        return Err(EncodeRefused::NotContiguous);
    }
    if events.iter().any(|event| event.v != RECORD_VERSION) {
        return Err(EncodeRefused::NotCurrentVersion);
    }
    let mut payload = b"[".to_vec();
    for (index, event) in events.iter().enumerate() {
        let bytes = event_bytes(event)?;
        if bytes.len() > MAX_EVENT_BYTES as usize {
            return Err(EncodeRefused::EventTooLarge);
        }
        if index > 0 {
            payload.push(b',');
        }
        payload.extend_from_slice(&bytes);
    }
    payload.push(b']');
    if payload.len() > MAX_FRAME_BYTES as usize {
        return Err(EncodeRefused::FrameTooLarge);
    }
    let digits = payload.len().to_string().into_bytes();
    let mut frame = Vec::with_capacity(payload.len() + 22);
    frame.extend_from_slice(MAGIC);
    frame.extend_from_slice(&digits);
    frame.push(b' ');
    frame.extend_from_slice(&hex8(frame_crc(&digits, &payload)));
    frame.push(b' ');
    frame.extend_from_slice(&payload);
    frame.push(b'\n');
    Ok(frame)
}

fn hex8(value: u32) -> [u8; CHECKSUM_DIGITS] {
    let mut out = [0; CHECKSUM_DIGITS];
    for (index, slot) in out.iter_mut().enumerate() {
        let nibble = (value >> (28 - 4 * index)) & 0xf;
        *slot = b"0123456789abcdef"[nibble as usize];
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------
// Decoding

/// What the bytes at an offset hold.
enum Read<'a> {
    /// A whole frame with a valid header: its length digits, payload, stated checksum, total length (its last byte
    /// included) and whether that last byte is a newline.
    Whole { digits: &'a [u8], payload: &'a [u8], stated: u32, len: usize, newline: bool },
    /// The stream ends before the frame does, with everything before the end consistent with a frame.
    Short,
    Bad(CorruptReason),
}

fn read_frame(bytes: &[u8]) -> Read<'_> {
    let seen = bytes.len().min(MAGIC.len());
    if bytes[..seen] != MAGIC[..seen] {
        return Read::Bad(CorruptReason::Magic);
    }
    let start = MAGIC.len();
    let mut at = start;
    let mut length: usize = 0;
    loop {
        match bytes.get(at) {
            None => return Read::Short,
            Some(b' ') if at > start => break,
            Some(digit) if digit.is_ascii_digit() && at - start < MAX_LENGTH_DIGITS && !(at == start && *digit == b'0') => {
                length = length * 10 + usize::from(digit - b'0');
                at += 1;
            }
            Some(_) => return Read::Bad(CorruptReason::LengthField),
        }
    }
    if length > MAX_FRAME_BYTES as usize {
        return Read::Bad(CorruptReason::FrameTooLarge);
    }
    let digits = &bytes[start..at];
    at += 1;
    let mut stated: u32 = 0;
    for index in 0..CHECKSUM_DIGITS {
        let value = match bytes.get(at + index) {
            None => return Read::Short,
            Some(byte @ b'0'..=b'9') => byte - b'0',
            Some(byte @ b'a'..=b'f') => byte - b'a' + 10,
            Some(_) => return Read::Bad(CorruptReason::ChecksumField),
        };
        stated = (stated << 4) | u32::from(value);
    }
    at += CHECKSUM_DIGITS;
    match bytes.get(at) {
        None => return Read::Short,
        Some(b' ') => at += 1,
        Some(_) => return Read::Bad(CorruptReason::ChecksumField),
    }
    let (Some(payload), Some(last)) = (bytes.get(at..at + length), bytes.get(at + length)) else { return Read::Short };
    Read::Whole { digits, payload, stated, len: at + length + 1, newline: *last == b'\n' }
}

/// What remains after the last valid frame, when it is not a valid frame: the IncompleteTail and Corrupt rules of
/// r5 4.7. `parsed` is how the frame at `offset` failed to read.
fn classify_rest(offset: u64, rest: &[u8], failure: CorruptReason, short: bool) -> StreamEnd {
    let newline = rest.contains(&b'\n');
    if failure == CorruptReason::FrameEnd {
        return StreamEnd::Corrupt { offset, reason: CorruptReason::FrameEnd };
    }
    if !newline && rest.len() <= MAX_TAIL_BYTES as usize {
        return StreamEnd::IncompleteTail { offset, len: rest.len() as u32, crc32c: crc32c(rest) };
    }
    let reason = if !newline {
        CorruptReason::TailTooLong
    } else if short {
        CorruptReason::CutFrame
    } else {
        failure
    };
    StreamEnd::Corrupt { offset, reason }
}

/// A frame at the start of `bytes` checked down to its checksum and last byte: its payload and length, or why not.
/// `Err((reason, short))`: `short` when the stream ends inside the frame.
fn checked_frame(bytes: &[u8]) -> Result<(&[u8], usize), (CorruptReason, bool)> {
    match read_frame(bytes) {
        Read::Whole { digits, payload, stated, len, newline } => {
            if frame_crc(digits, payload) != stated {
                Err((CorruptReason::Checksum, false))
            } else if !newline {
                Err((CorruptReason::FrameEnd, false))
            } else {
                Ok((payload, len))
            }
        }
        Read::Short => Err((CorruptReason::CutFrame, true)),
        Read::Bad(reason) => Err((reason, false)),
    }
}

/// Decodes and classifies a whole journal. Never panics, allocates nothing beyond lengths it has validated, and
/// interprets nothing after the first invalid frame.
pub(crate) fn decode_stream(bytes: &[u8]) -> Decoded {
    let mut frames = Vec::new();
    let end = if bytes.len() as u64 > MAX_STREAM_BYTES {
        StreamEnd::Corrupt { offset: 0, reason: CorruptReason::StreamTooLarge }
    } else {
        decode_frames(bytes, &mut frames)
    };
    Decoded { frames, end }
}

/// Decodes valid frames into `frames` until the end of `bytes` or the first invalid one, and says how the stream ends.
fn decode_frames(bytes: &[u8], frames: &mut Vec<DecodedFrame>) -> StreamEnd {
    let mut at = 0;
    loop {
        if at == bytes.len() {
            break StreamEnd::Clean;
        }
        let rest = &bytes[at..];
        let offset = at as u64;
        match checked_frame(rest) {
            Ok((payload, len)) => match decode_payload(payload) {
                Ok(events) => {
                    frames.push(DecodedFrame { offset, end: (at + len) as u64, events });
                    at += len;
                }
                Err(PayloadFault::Corrupt(reason)) => break StreamEnd::Corrupt { offset, reason },
                Err(PayloadFault::Newer(v)) => break StreamEnd::NewerVersion { offset, v },
            },
            Err((reason, short)) => break classify_rest(offset, rest, reason, short),
        }
    }
}

enum PayloadFault {
    Corrupt(CorruptReason),
    Newer(u64),
}

/// The version of each record, read before anything else (F14): a v2 record must read as `NewerVersion`, not as a v1
/// record with unknown fields. Future versions keep `v` as an integer field of every record object.
#[derive(Deserialize)]
struct Peek {
    v: u64,
}

fn decode_payload(payload: &[u8]) -> Result<Vec<Event>, PayloadFault> {
    let corrupt = PayloadFault::Corrupt;
    if !nesting_within(payload, MAX_FRAME_NESTING) {
        return Err(corrupt(CorruptReason::Nesting));
    }
    // The version of every record first (F14); only then the limits of v1.
    let peeks: Vec<Peek> = serde_json::from_slice(payload).map_err(|_| corrupt(CorruptReason::Schema))?;
    if peeks.is_empty() {
        return Err(corrupt(CorruptReason::EmptyBatch));
    }
    if peeks.iter().any(|peek| peek.v > MAX_WALL_MS) {
        return Err(corrupt(CorruptReason::Integer));
    }
    if peeks.iter().any(|peek| peek.v == 0) {
        return Err(corrupt(CorruptReason::Schema));
    }
    if let Some(peek) = peeks.iter().find(|peek| peek.v > u64::from(RECORD_VERSION)) {
        return Err(PayloadFault::Newer(peek.v));
    }
    if peeks.len() > MAX_BATCH {
        return Err(corrupt(CorruptReason::BatchTooLarge));
    }
    if !nesting_within(payload, MAX_NESTING) {
        return Err(corrupt(CorruptReason::Nesting));
    }
    let records: Vec<WireRecord> = serde_json::from_slice(payload).map_err(|_| corrupt(CorruptReason::Schema))?;
    let mut events = Vec::with_capacity(records.len());
    for record in records {
        events.push(event_from_wire(record).map_err(corrupt)?);
    }
    let mut canonical = Vec::with_capacity(payload.len());
    canonical.push(b'[');
    for (index, event) in events.iter().enumerate() {
        let bytes = event_bytes(event).map_err(|_| corrupt(CorruptReason::Schema))?;
        if bytes.len() > MAX_EVENT_BYTES as usize {
            return Err(corrupt(CorruptReason::EventTooLarge));
        }
        if index > 0 {
            canonical.push(b',');
        }
        canonical.extend_from_slice(&bytes);
    }
    canonical.push(b']');
    if canonical != payload {
        return Err(corrupt(CorruptReason::NotCanonical));
    }
    Ok(events)
}

/// Whether JSON arrays and objects in `payload` nest at most `limit` deep (brackets inside strings do not count).
fn nesting_within(payload: &[u8], limit: usize) -> bool {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in payload {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => {
                depth += 1;
                if depth > limit {
                    return false;
                }
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    true
}

// ---------------------------------------------------------------------------------------------------------------
// Audit scan after corruption

/// A later frame found by `scan_after_corruption`. Unverified by definition: it follows corruption, so nothing proves
/// it belongs to this journal's history. It holds no event, so it can never be applied, replayed or reconciled; it
/// only says where a frame-shaped line is, whether its checksum matches, and which kinds it would name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnverifiedFrame {
    offset: u64,
    len: u64,
    checksum_matches: bool,
    kinds: Vec<EventKind>,
}

impl UnverifiedFrame {
    pub(crate) const fn offset(&self) -> u64 {
        self.offset
    }

    pub(crate) const fn len(&self) -> u64 {
        self.len
    }

    pub(crate) const fn checksum_matches(&self) -> bool {
        self.checksum_matches
    }

    /// The kinds its records name, when its checksum matches and it decodes as v1; empty otherwise.
    pub(crate) fn kinds(&self) -> &[EventKind] {
        &self.kinds
    }
}

/// Lists, for audit only, the frame-shaped lines that start after a newline past `offset` (the offset of the first
/// invalid frame): at most MAX_UNVERIFIED_FRAMES of them, with `true` when more were left out. The scan never feeds a
/// projection and never makes the stream less corrupt.
pub(crate) fn scan_after_corruption(bytes: &[u8], offset: u64) -> (Vec<UnverifiedFrame>, bool) {
    let mut found = Vec::new();
    let limit = bytes.len().min(MAX_STREAM_BYTES as usize);
    let Ok(mut at) = usize::try_from(offset) else { return (found, false) };
    at = at.saturating_add(1);
    while at < limit {
        if bytes[at - 1] != b'\n' || !bytes[at..limit].starts_with(MAGIC) {
            at += 1;
            continue;
        }
        if found.len() == MAX_UNVERIFIED_FRAMES {
            return (found, true);
        }
        match read_frame(&bytes[at..limit]) {
            Read::Whole { digits, payload, stated, len, newline: true } => {
                let checksum_matches = frame_crc(digits, payload) == stated;
                let kinds = match checksum_matches.then(|| decode_payload(payload)) {
                    Some(Ok(events)) => events.iter().map(|event| event.body.kind()).collect(),
                    _ => Vec::new(),
                };
                found.push(UnverifiedFrame { offset: at as u64, len: len as u64, checksum_matches, kinds });
                at += len;
            }
            _ => at += 1,
        }
    }
    (found, false)
}

// ---------------------------------------------------------------------------------------------------------------
// Uncertain writes (r5 5.4)

/// What a re-read journal says about one frame whose write was uncertain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PersistCheck {
    /// The file is exactly the committed bytes, possibly followed by a torn tail (no newline, at most one frame's
    /// worth): the pending frame is not part of the interpreted history.
    Absent { torn: Option<TornTail> },
    /// The file is exactly the committed bytes followed by exactly the pending frame.
    Landed,
    /// Anything else: the stream is poisoned and the future adapter must quarantine it (r5 5.4).
    Diverged(Divergence),
    /// The inputs cannot support any conclusion: the outcome stays uncertain, which the adapter treats as `Diverged`.
    Unknown(Insufficient),
}

/// A torn tail after the committed bytes, and whether it is a prefix of the pending frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TornTail {
    pub(crate) len: u32,
    pub(crate) pending_prefix: bool,
    /// The CRC-32C of the tail bytes themselves, so that the repair record that removes them can be bound to exactly
    /// these bytes (D3.2c-2, N7).
    pub(crate) crc32c: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Divergence {
    /// The file is a strict prefix of the committed bytes: acknowledged bytes are missing.
    Shortened,
    /// The file does not start with the committed bytes.
    Altered,
    /// A complete frame other than the pending one follows the committed bytes.
    Foreign,
    /// The pending frame landed, followed by more bytes.
    Extra,
    /// The bytes after the committed ones are corrupt.
    Corrupt(CorruptReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Insufficient {
    /// The committed bytes are not themselves a clean stream.
    CommittedNotClean,
    /// The pending bytes are not exactly one valid frame of the current version.
    PendingNotOneFrame,
    /// The pending frame does not continue the committed history (another run, or not the next sequence numbers).
    PendingNotNext,
    /// The file is longer than MAX_STREAM_BYTES.
    FileTooLarge,
}

/// Byte-exact check of an uncertain append (r5 5.4): `committed` is what the writer knows it acknowledged, `pending`
/// the frame it tried to write, `file` the journal re-read under its lock. Pure: it replays nothing, reconciles
/// nothing, and says nothing about durability, which only the adapter's syncs establish. Its result is never authority:
/// `Landed` only allows the live layer to adopt the frame, which mints no grant.
pub(crate) fn verify_persisted(file: &[u8], committed: &[u8], pending: &[u8]) -> PersistCheck {
    if file.len() as u64 > MAX_STREAM_BYTES {
        return PersistCheck::Unknown(Insufficient::FileTooLarge);
    }
    let known = decode_stream(committed);
    if known.end != StreamEnd::Clean {
        return PersistCheck::Unknown(Insufficient::CommittedNotClean);
    }
    let next = decode_stream(pending);
    let [frame] = next.frames.as_slice() else { return PersistCheck::Unknown(Insufficient::PendingNotOneFrame) };
    if next.end != StreamEnd::Clean {
        return PersistCheck::Unknown(Insufficient::PendingNotOneFrame);
    }
    let previous = known.frames.last().and_then(|last| last.events.last());
    let mut expected = previous.map(|event| (Some(event.run), event.seq.number() + 1)).unwrap_or((None, 1));
    for event in &frame.events {
        if expected.0.is_some_and(|run| run != event.run) || event.seq.number() != expected.1 {
            return PersistCheck::Unknown(Insufficient::PendingNotNext);
        }
        expected = (Some(event.run), expected.1 + 1);
    }
    if !file.starts_with(committed) {
        return PersistCheck::Diverged(if committed.starts_with(file) { Divergence::Shortened } else { Divergence::Altered });
    }
    let rest = &file[committed.len()..];
    if rest.is_empty() {
        return PersistCheck::Absent { torn: None };
    }
    if rest.starts_with(pending) {
        return if rest.len() == pending.len() { PersistCheck::Landed } else { PersistCheck::Diverged(Divergence::Extra) };
    }
    match checked_frame(rest) {
        Ok(_) => PersistCheck::Diverged(Divergence::Foreign),
        Err((reason, short)) => match classify_rest(0, rest, reason, short) {
            StreamEnd::IncompleteTail { len, crc32c, .. } => {
                PersistCheck::Absent { torn: Some(TornTail { len, pending_prefix: pending.starts_with(rest), crc32c }) }
            }
            StreamEnd::Corrupt { reason, .. } => PersistCheck::Diverged(Divergence::Corrupt(reason)),
            StreamEnd::Clean | StreamEnd::NewerVersion { .. } => PersistCheck::Diverged(Divergence::Foreign),
        },
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Wire types: the v1 JSON form, private to the codec. Field order is the canonical order. Every closed value is a
// snake_case string, every number an unsigned integer except the exit code, and every optional field is always
// present (`null` when absent), so each record has exactly one encoding.

#[derive(Serialize)]
struct WireOut<'a, B> {
    v: u16,
    run: String,
    seq: u64,
    at_ms: u64,
    clamped: bool,
    producer: &'a str,
    kind: &'a str,
    body: B,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRecord {
    v: u64,
    run: String,
    seq: u64,
    at_ms: u64,
    clamped: bool,
    producer: String,
    kind: String,
    body: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WRunCreated {
    spec: WSpec,
    spec_digest: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WSpec {
    task: WTask,
    provider: WProvider,
    context: WContext,
    max_started_attempts: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WTask {
    task: String,
    created_at_ms: u64,
    revision: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WProvider {
    provider: String,
    executable_sha256: String,
    advertised_version: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WContext {
    project: WProject,
    workspace: String,
    permissions: Vec<String>,
    limits: WLimits,
    policy_version: String,
    environment_sha256: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WProject {
    project: String,
    registered_at_ms: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WLimits {
    start_ms: u64,
    run_ms: u64,
    stall_ms: u64,
    approval_ttl_ms: u64,
    stop_grace_ms: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WAttemptCreated {
    attempt: u64,
    continuation: String,
    from_attempt: Option<u64>,
    session: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WRequest {
    approval: String,
    run: String,
    attempt: u64,
    kind: String,
    binding: WBinding,
    binding_digest: String,
    description_sha256: String,
    scope: String,
    risk: String,
    provider_risk_claim: Option<String>,
    ttl_ms: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WBinding {
    spec_digest: String,
    target: WTarget,
    operation_class: String,
    operation_digest: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WTarget {
    provider: String,
    executable_sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WDecided {
    approval: String,
    decision: String,
    provenance: WDecisionProvenance,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WDecisionProvenance {
    actor: String,
    channel: String,
    presented_binding_digest: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WExpired {
    approval: String,
    basis: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WApprovalEnded {
    approval: String,
    reason: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WConsumed {
    approval: String,
    binding: WBinding,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WStartBegun {
    attempt: u64,
    approval: String,
    binding: WBinding,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WStarted {
    attempt: u64,
    os_identity: WIdentity,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WIdentity {
    pid: u64,
    start_token: String,
    executable_sha256: String,
}

/// The body of every attempt event that carries one closed value: `StartFailed` (`code`), `StopRequested` (`reason`),
/// `AttemptInterrupted` (`observation`), `AttemptNotStarted` (`reason`), `Progress` (`kind`) and
/// `ProviderOutcomeClaimed` (`claim`) each have their own struct, so that field names stay exact.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WStartFailed {
    attempt: u64,
    code: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WAttemptReason {
    attempt: u64,
    reason: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WCancel {
    scope: String,
    attempt: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WExited {
    attempt: u64,
    status: WStatus,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WStatus {
    code: Option<i64>,
    ended_by: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WInterrupted {
    attempt: u64,
    observation: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WSession {
    attempt: u64,
    session: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WProgress {
    attempt: u64,
    kind: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WClaim {
    attempt: u64,
    claim: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WClosed {
    reason: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WResolved {
    concern: WConcern,
    resolution: String,
    provenance: WResolutionProvenance,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WConcern {
    kind: String,
    attempt: Option<u64>,
    raised_at: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WResolutionProvenance {
    actor: String,
    channel: String,
    presented_sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WRepaired {
    sidecar: u64,
    removed_bytes: u64,
    removed_crc32c: u64,
    cause: String,
}

// ---------------------------------------------------------------------------------------------------------------
// Names of closed values. Each `*_name` is exhaustive; each table lists every value, and decoding looks a name up in it.

fn lookup<T: Copy>(all: &[T], name_of: fn(T) -> &'static str, name: &str) -> Result<T, CorruptReason> {
    all.iter().copied().find(|value| name_of(*value) == name).ok_or(CorruptReason::Schema)
}

pub(crate) const PRODUCERS: [Producer; 5] = [Producer::User, Producer::Supervisor, Producer::ProviderRelay, Producer::Reconciler, Producer::Journal];

pub(crate) const fn producer_name(producer: Producer) -> &'static str {
    match producer {
        Producer::User => "user",
        Producer::Supervisor => "supervisor",
        Producer::ProviderRelay => "provider_relay",
        Producer::Reconciler => "reconciler",
        Producer::Journal => "journal",
    }
}

pub(crate) const fn kind_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::RunCreated => "run_created",
        EventKind::AttemptCreated => "attempt_created",
        EventKind::ApprovalRequested => "approval_requested",
        EventKind::ApprovalDecided => "approval_decided",
        EventKind::ApprovalExpired => "approval_expired",
        EventKind::ApprovalWithdrawn => "approval_withdrawn",
        EventKind::ApprovalVoided => "approval_voided",
        EventKind::ApprovalConsumed => "approval_consumed",
        EventKind::StartBegun => "start_begun",
        EventKind::AttemptStarted => "attempt_started",
        EventKind::StartFailed => "start_failed",
        EventKind::StopRequested => "stop_requested",
        EventKind::CancelRequested => "cancel_requested",
        EventKind::AttemptExited => "attempt_exited",
        EventKind::AttemptInterrupted => "attempt_interrupted",
        EventKind::AttemptNotStarted => "attempt_not_started",
        EventKind::ProviderSessionObserved => "provider_session_observed",
        EventKind::Progress => "progress",
        EventKind::ProviderOutcomeClaimed => "provider_outcome_claimed",
        EventKind::RunClosed => "run_closed",
        EventKind::AttentionResolved => "attention_resolved",
        EventKind::JournalRepaired => "journal_repaired",
    }
}

pub(crate) const WORKSPACES: [WorkspaceMode; 1] = [WorkspaceMode::None];

pub(crate) const fn workspace_name(mode: WorkspaceMode) -> &'static str {
    match mode {
        WorkspaceMode::None => "none",
    }
}

pub(crate) const PERMISSIONS: [PermissionClass; 7] = [
    PermissionClass::ReadInWorkspace,
    PermissionClass::WriteInWorkspace,
    PermissionClass::ShellRun,
    PermissionClass::FileDelete,
    PermissionClass::OutsideWorkspace,
    PermissionClass::NetworkAccess,
    PermissionClass::PermissionChange,
];

pub(crate) const fn permission_name(class: PermissionClass) -> &'static str {
    match class {
        PermissionClass::ReadInWorkspace => "read_in_workspace",
        PermissionClass::WriteInWorkspace => "write_in_workspace",
        PermissionClass::ShellRun => "shell_run",
        PermissionClass::FileDelete => "file_delete",
        PermissionClass::OutsideWorkspace => "outside_workspace",
        PermissionClass::NetworkAccess => "network_access",
        PermissionClass::PermissionChange => "permission_change",
    }
}

pub(crate) const APPROVAL_KINDS: [ApprovalKind; 2] = [ApprovalKind::Start, ApprovalKind::Operation];

pub(crate) const fn approval_kind_name(kind: ApprovalKind) -> &'static str {
    match kind {
        ApprovalKind::Start => "start",
        ApprovalKind::Operation => "operation",
    }
}

pub(crate) const OPERATION_CLASSES: [OperationClass; 9] = [
    OperationClass::Start,
    OperationClass::ReadInWorkspace,
    OperationClass::WriteInWorkspace,
    OperationClass::ShellRun,
    OperationClass::FileDelete,
    OperationClass::OutsideWorkspace,
    OperationClass::NetworkAccess,
    OperationClass::PermissionChange,
    OperationClass::Other,
];

pub(crate) const fn operation_class_name(class: OperationClass) -> &'static str {
    match class {
        OperationClass::Start => "start",
        OperationClass::ReadInWorkspace => "read_in_workspace",
        OperationClass::WriteInWorkspace => "write_in_workspace",
        OperationClass::ShellRun => "shell_run",
        OperationClass::FileDelete => "file_delete",
        OperationClass::OutsideWorkspace => "outside_workspace",
        OperationClass::NetworkAccess => "network_access",
        OperationClass::PermissionChange => "permission_change",
        OperationClass::Other => "other",
    }
}

pub(crate) const RISKS: [RiskClass; 3] = [RiskClass::Elevated, RiskClass::High, RiskClass::Critical];

pub(crate) const fn risk_name(risk: RiskClass) -> &'static str {
    match risk {
        RiskClass::Elevated => "elevated",
        RiskClass::High => "high",
        RiskClass::Critical => "critical",
    }
}

pub(crate) const fn scope_name(scope: AuthorityScope) -> &'static str {
    match scope {
        AuthorityScope::Once => "once",
    }
}

pub(crate) const ACTORS: [Actor; 1] = [Actor::User];

pub(crate) const fn actor_name(actor: Actor) -> &'static str {
    match actor {
        Actor::User => "user",
    }
}

pub(crate) const CHANNELS: [Channel; 1] = [Channel::NativeConfirmation];

pub(crate) const fn channel_name(channel: Channel) -> &'static str {
    match channel {
        Channel::NativeConfirmation => "native_confirmation",
    }
}

pub(crate) const DECISIONS: [Decision; 2] = [Decision::Granted, Decision::Denied];

pub(crate) const fn decision_name(decision: Decision) -> &'static str {
    match decision {
        Decision::Granted => "granted",
        Decision::Denied => "denied",
    }
}

pub(crate) const EXPIRY_BASES: [ExpiryBasis; 3] = [ExpiryBasis::Monotonic, ExpiryBasis::Wall, ExpiryBasis::ClockFault];

pub(crate) const fn expiry_name(basis: ExpiryBasis) -> &'static str {
    match basis {
        ExpiryBasis::Monotonic => "monotonic",
        ExpiryBasis::Wall => "wall",
        ExpiryBasis::ClockFault => "clock_fault",
    }
}

pub(crate) const WITHDRAWALS: [WithdrawalReason; 4] =
    [WithdrawalReason::AttemptEnded, WithdrawalReason::Cancelled, WithdrawalReason::SupervisionLost, WithdrawalReason::ClockFault];

pub(crate) const fn withdrawal_name(reason: WithdrawalReason) -> &'static str {
    match reason {
        WithdrawalReason::AttemptEnded => "attempt_ended",
        WithdrawalReason::Cancelled => "cancelled",
        WithdrawalReason::SupervisionLost => "supervision_lost",
        WithdrawalReason::ClockFault => "clock_fault",
    }
}

pub(crate) const VOIDS: [VoidReason; 6] = [
    VoidReason::AttemptEnded,
    VoidReason::Cancelled,
    VoidReason::UseByPassed,
    VoidReason::SupervisionLost,
    VoidReason::WriteUncertain,
    VoidReason::ClockFault,
];

pub(crate) const fn void_name(reason: VoidReason) -> &'static str {
    match reason {
        VoidReason::AttemptEnded => "attempt_ended",
        VoidReason::Cancelled => "cancelled",
        VoidReason::UseByPassed => "use_by_passed",
        VoidReason::SupervisionLost => "supervision_lost",
        VoidReason::WriteUncertain => "write_uncertain",
        VoidReason::ClockFault => "clock_fault",
    }
}

pub(crate) const START_FAILURES: [StartFailCode; 4] =
    [StartFailCode::SpawnRefused, StartFailCode::HashMismatch, StartFailCode::StartError, StartFailCode::EffectNotPerformed];

pub(crate) const fn start_failure_name(code: StartFailCode) -> &'static str {
    match code {
        StartFailCode::SpawnRefused => "spawn_refused",
        StartFailCode::HashMismatch => "hash_mismatch",
        StartFailCode::StartError => "start_error",
        StartFailCode::EffectNotPerformed => "effect_not_performed",
    }
}

pub(crate) const STOP_REASONS: [StopReason; 5] = [
    StopReason::Deadline(DeadlineKind::Start),
    StopReason::Deadline(DeadlineKind::Run),
    StopReason::Deadline(DeadlineKind::Stall),
    StopReason::JournalFull,
    StopReason::ClockFault,
];

pub(crate) const fn stop_name(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Deadline(DeadlineKind::Start) => "deadline_start",
        StopReason::Deadline(DeadlineKind::Run) => "deadline_run",
        StopReason::Deadline(DeadlineKind::Stall) => "deadline_stall",
        StopReason::JournalFull => "journal_full",
        StopReason::ClockFault => "clock_fault",
    }
}

pub(crate) const OBSERVATIONS: [InterruptObservation; 5] = [
    InterruptObservation::NoIdentity,
    InterruptObservation::NotRunning,
    InterruptObservation::Unknown,
    InterruptObservation::PossiblyRunning,
    InterruptObservation::EvidenceLost,
];

pub(crate) const fn observation_name(observation: InterruptObservation) -> &'static str {
    match observation {
        InterruptObservation::NoIdentity => "no_identity",
        InterruptObservation::NotRunning => "not_running",
        InterruptObservation::Unknown => "unknown",
        InterruptObservation::PossiblyRunning => "possibly_running",
        InterruptObservation::EvidenceLost => "evidence_lost",
    }
}

pub(crate) const NOT_STARTED_CAUSES: [NotStartedCause; 1] = [NotStartedCause::SupervisionLost];

pub(crate) const fn not_started_name(cause: NotStartedCause) -> &'static str {
    match cause {
        NotStartedCause::SupervisionLost => "supervision_lost",
    }
}

pub(crate) const PROGRESS_KINDS: [ProgressKind; 2] = [ProgressKind::Heartbeat, ProgressKind::ToolActivity];

pub(crate) const fn progress_name(kind: ProgressKind) -> &'static str {
    match kind {
        ProgressKind::Heartbeat => "heartbeat",
        ProgressKind::ToolActivity => "tool_activity",
    }
}

pub(crate) const CLAIMS: [OutcomeClaim; 1] = [OutcomeClaim::Failure];

pub(crate) const fn claim_name(claim: OutcomeClaim) -> &'static str {
    match claim {
        OutcomeClaim::Failure => "failure",
    }
}

pub(crate) const CLOSE_REASONS: [CloseReason; 1] = [CloseReason::ByUser];

pub(crate) const fn close_name(reason: CloseReason) -> &'static str {
    match reason {
        CloseReason::ByUser => "by_user",
    }
}

pub(crate) const CONCERN_KINDS: [ConcernKind; 4] =
    [ConcernKind::PossibleOrphan, ConcernKind::WorkspaceUnknown, ConcernKind::JournalRepaired, ConcernKind::ClockIrregular];

pub(crate) const fn concern_name(kind: ConcernKind) -> &'static str {
    match kind {
        ConcernKind::PossibleOrphan => "possible_orphan",
        ConcernKind::WorkspaceUnknown => "workspace_unknown",
        ConcernKind::JournalRepaired => "journal_repaired",
        ConcernKind::ClockIrregular => "clock_irregular",
    }
}

pub(crate) const RESOLUTIONS: [Resolution; 5] =
    [Resolution::ConfirmedGone, Resolution::StoppedOutsideOrdelis, Resolution::AcceptedRisk, Resolution::Reviewed, Resolution::Acknowledged];

pub(crate) const fn resolution_name(resolution: Resolution) -> &'static str {
    match resolution {
        Resolution::ConfirmedGone => "confirmed_gone",
        Resolution::StoppedOutsideOrdelis => "stopped_outside_ordelis",
        Resolution::AcceptedRisk => "accepted_risk",
        Resolution::Reviewed => "reviewed",
        Resolution::Acknowledged => "acknowledged",
    }
}

pub(crate) const REPAIR_CAUSES: [RepairCause; 1] = [RepairCause::IncompleteTail];

pub(crate) const fn repair_cause_name(cause: RepairCause) -> &'static str {
    match cause {
        RepairCause::IncompleteTail => "incomplete_tail",
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Model to wire

fn id_text(prefix: char, number: u64) -> String {
    let mut text = String::from(prefix);
    text.push_str(&number.to_string());
    text
}

fn owned(text: &str) -> String {
    text.to_owned()
}

fn wire_binding(binding: &Binding) -> WBinding {
    WBinding {
        spec_digest: owned(binding.spec_digest.as_str()),
        target: WTarget { provider: owned(binding.target.provider.id()), executable_sha256: owned(binding.target.executable_sha256.as_str()) },
        operation_class: owned(operation_class_name(binding.operation_class)),
        operation_digest: owned(binding.operation_digest.as_str()),
    }
}

fn wire_spec(spec: &RunSpec) -> WSpec {
    let task = spec.task();
    let provider = spec.provider();
    let context = spec.context();
    let limits = context.limits();
    let project = context.project();
    WSpec {
        task: WTask { task: id_text('t', task.task().number()), created_at_ms: task.created_at_ms().ms(), revision: task.revision() },
        provider: WProvider {
            provider: owned(provider.provider.id()),
            executable_sha256: owned(provider.executable_sha256.as_str()),
            advertised_version: provider.advertised_version.as_ref().map(|version| owned(version.as_str())),
        },
        context: WContext {
            project: WProject { project: id_text('p', project.project.number()), registered_at_ms: project.registered_at_ms.ms() },
            workspace: owned(workspace_name(context.workspace())),
            permissions: context.permissions().iter().map(|class| owned(permission_name(*class))).collect(),
            limits: WLimits {
                start_ms: limits.start_ms(),
                run_ms: limits.run_ms(),
                stall_ms: limits.stall_ms(),
                approval_ttl_ms: limits.approval_ttl_ms(),
                stop_grace_ms: limits.stop_grace_ms(),
            },
            policy_version: owned(context.policy_version().as_str()),
            environment_sha256: context.environment_sha256().map(|digest| owned(digest.as_str())),
        },
        max_started_attempts: u64::from(spec.max_started_attempts()),
    }
}

fn wire_request(request: &ApprovalRequest) -> WRequest {
    let fields = request.fields();
    WRequest {
        approval: id_text('a', fields.approval.number()),
        run: id_text('r', fields.run.number()),
        attempt: u64::from(fields.attempt.number()),
        kind: owned(approval_kind_name(fields.kind)),
        binding: wire_binding(&fields.binding),
        binding_digest: owned(fields.binding_digest.as_str()),
        description_sha256: owned(fields.description_sha256.as_str()),
        scope: owned(scope_name(request.scope())),
        risk: owned(risk_name(request.risk())),
        provider_risk_claim: fields.provider_risk_claim.map(|claim| owned(risk_name(claim))),
        ttl_ms: fields.ttl_ms,
    }
}

/// The canonical JSON of one event.
fn event_bytes(event: &Event) -> Result<Vec<u8>, EncodeRefused> {
    let attempt = |number: &AttemptNo| u64::from(number.number());
    let approval = |id: &ApprovalId| id_text('a', id.number());
    match &event.body {
        EventBody::RunCreated { spec, spec_digest } => {
            wire(event, WRunCreated { spec: wire_spec(spec), spec_digest: owned(spec_digest.as_str()) })
        }
        EventBody::AttemptCreated { attempt: number, continuation } => {
            let (name, from_attempt, session) = match continuation {
                Continuation::Fresh => ("fresh", None, None),
                Continuation::Resume { from_attempt, session } => ("resume", Some(attempt(from_attempt)), Some(owned(session.as_str()))),
            };
            wire(event, WAttemptCreated { attempt: attempt(number), continuation: owned(name), from_attempt, session })
        }
        EventBody::ApprovalRequested(request) => wire(event, wire_request(request)),
        EventBody::ApprovalDecided { approval: id, decision, provenance } => wire(
            event,
            WDecided {
                approval: approval(id),
                decision: owned(decision_name(*decision)),
                provenance: WDecisionProvenance {
                    actor: owned(actor_name(provenance.actor)),
                    channel: owned(channel_name(provenance.channel)),
                    presented_binding_digest: owned(provenance.presented_binding_digest.as_str()),
                },
            },
        ),
        EventBody::ApprovalExpired { approval: id, basis } => wire(event, WExpired { approval: approval(id), basis: owned(expiry_name(*basis)) }),
        EventBody::ApprovalWithdrawn { approval: id, reason } => {
            wire(event, WApprovalEnded { approval: approval(id), reason: owned(withdrawal_name(*reason)) })
        }
        EventBody::ApprovalVoided { approval: id, reason } => wire(event, WApprovalEnded { approval: approval(id), reason: owned(void_name(*reason)) }),
        EventBody::ApprovalConsumed { approval: id, binding } => wire(event, WConsumed { approval: approval(id), binding: wire_binding(binding) }),
        EventBody::StartBegun { attempt: number, approval: id, binding } => {
            wire(event, WStartBegun { attempt: attempt(number), approval: approval(id), binding: wire_binding(binding) })
        }
        EventBody::AttemptStarted { attempt: number, os_identity } => wire(
            event,
            WStarted {
                attempt: attempt(number),
                os_identity: WIdentity {
                    pid: u64::from(os_identity.pid()),
                    start_token: owned(os_identity.start_token().as_str()),
                    executable_sha256: owned(os_identity.executable_sha256().as_str()),
                },
            },
        ),
        EventBody::StartFailed { attempt: number, code } => wire(event, WStartFailed { attempt: attempt(number), code: owned(start_failure_name(*code)) }),
        EventBody::StopRequested { attempt: number, reason } => wire(event, WAttemptReason { attempt: attempt(number), reason: owned(stop_name(*reason)) }),
        EventBody::CancelRequested { scope } => {
            let (name, target) = match scope {
                CancelScope::Attempt(number) => ("attempt", Some(attempt(number))),
                CancelScope::Run => ("run", None),
            };
            wire(event, WCancel { scope: owned(name), attempt: target })
        }
        EventBody::AttemptExited { attempt: number, status } => wire(
            event,
            WExited { attempt: attempt(number), status: WStatus { code: status.code().map(i64::from), ended_by: status.ended_by().map(u64::from) } },
        ),
        EventBody::AttemptInterrupted { attempt: number, observation } => {
            wire(event, WInterrupted { attempt: attempt(number), observation: owned(observation_name(*observation)) })
        }
        EventBody::AttemptNotStarted { attempt: number, reason } => {
            wire(event, WAttemptReason { attempt: attempt(number), reason: owned(not_started_name(*reason)) })
        }
        EventBody::ProviderSessionObserved { attempt: number, session } => {
            wire(event, WSession { attempt: attempt(number), session: owned(session.as_str()) })
        }
        EventBody::Progress { attempt: number, kind } => wire(event, WProgress { attempt: attempt(number), kind: owned(progress_name(*kind)) }),
        EventBody::ProviderOutcomeClaimed { attempt: number, claim } => wire(event, WClaim { attempt: attempt(number), claim: owned(claim_name(*claim)) }),
        EventBody::RunClosed { reason } => wire(event, WClosed { reason: owned(close_name(*reason)) }),
        EventBody::AttentionResolved(resolved) => {
            let concern = resolved.concern();
            let provenance = resolved.provenance();
            wire(
                event,
                WResolved {
                    concern: WConcern {
                        kind: owned(concern_name(concern.kind())),
                        attempt: concern.attempt().as_ref().map(attempt),
                        raised_at: concern.raised_at().number(),
                    },
                    resolution: owned(resolution_name(resolved.resolution())),
                    provenance: WResolutionProvenance {
                        actor: owned(actor_name(provenance.actor)),
                        channel: owned(channel_name(provenance.channel)),
                        presented_sha256: owned(provenance.presented_sha256.as_str()),
                    },
                },
            )
        }
        EventBody::JournalRepaired(repair) => wire(
            event,
            WRepaired {
                sidecar: u64::from(repair.sidecar()),
                removed_bytes: u64::from(repair.removed_bytes()),
                removed_crc32c: u64::from(repair.removed_crc32c()),
                cause: owned(repair_cause_name(repair.cause())),
            },
        ),
    }
}

fn wire<B: Serialize>(event: &Event, body: B) -> Result<Vec<u8>, EncodeRefused> {
    let record = WireOut {
        v: event.v,
        run: id_text('r', event.run.number()),
        seq: event.seq.number(),
        at_ms: event.at_ms.ms(),
        clamped: event.clamped,
        producer: producer_name(event.producer),
        kind: kind_name(event.body.kind()),
        body,
    };
    serde_json::to_vec(&record).map_err(|_| EncodeRefused::Unencodable)
}

// ---------------------------------------------------------------------------------------------------------------
// Wire to model. Every refusal is `Schema`, except an integer above 2^53 - 1 (`Integer`).

fn int(number: u64) -> Result<u64, CorruptReason> {
    if number <= MAX_WALL_MS {
        Ok(number)
    } else {
        Err(CorruptReason::Integer)
    }
}

fn valid<T, E>(result: Result<T, E>) -> Result<T, CorruptReason> {
    result.map_err(|_| CorruptReason::Schema)
}

fn narrow<T: TryFrom<u64>>(number: u64) -> Result<T, CorruptReason> {
    valid(T::try_from(int(number)?))
}

fn attempt_no(number: u64) -> Result<AttemptNo, CorruptReason> {
    valid(AttemptNo::new(narrow(number)?))
}

fn approval_id(text: &str) -> Result<ApprovalId, CorruptReason> {
    valid(ApprovalId::parse(text))
}

fn wall(number: u64) -> Result<WallMs, CorruptReason> {
    valid(WallMs::new(int(number)?))
}

fn digest(text: &str) -> Result<Digest, CorruptReason> {
    valid(Digest::new(text))
}

fn short_token(text: &str) -> Result<ShortToken, CorruptReason> {
    valid(ShortToken::new(text))
}

fn catalog_id(text: &str) -> Result<CatalogId, CorruptReason> {
    CatalogId::from_id(text).ok_or(CorruptReason::Schema)
}

fn binding_from(wire: WBinding) -> Result<Binding, CorruptReason> {
    Ok(Binding {
        spec_digest: digest(&wire.spec_digest)?,
        target: BindingTarget { provider: catalog_id(&wire.target.provider)?, executable_sha256: digest(&wire.target.executable_sha256)? },
        operation_class: lookup(&OPERATION_CLASSES, operation_class_name, &wire.operation_class)?,
        operation_digest: digest(&wire.operation_digest)?,
    })
}

fn spec_from(wire: WSpec) -> Result<RunSpec, CorruptReason> {
    let task = valid(TaskRef::new(valid(TaskId::parse(&wire.task.task))?, wall(wire.task.created_at_ms)?, int(wire.task.revision)?))?;
    let provider = ProviderIdentity {
        provider: catalog_id(&wire.provider.provider)?,
        executable_sha256: digest(&wire.provider.executable_sha256)?,
        advertised_version: wire.provider.advertised_version.as_deref().map(short_token).transpose()?,
    };
    let context = wire.context;
    let project = ProjectRef { project: valid(ProjectId::parse(&context.project.project))?, registered_at_ms: wall(context.project.registered_at_ms)? };
    let limits = valid(Limits::new(
        int(context.limits.start_ms)?,
        int(context.limits.run_ms)?,
        int(context.limits.stall_ms)?,
        int(context.limits.approval_ttl_ms)?,
        int(context.limits.stop_grace_ms)?,
    ))?;
    let mut permissions = Vec::with_capacity(context.permissions.len());
    for name in &context.permissions {
        permissions.push(lookup(&PERMISSIONS, permission_name, name)?);
    }
    let run_context = valid(RunContext::new(
        project,
        lookup(&WORKSPACES, workspace_name, &context.workspace)?,
        permissions,
        limits,
        short_token(&context.policy_version)?,
        context.environment_sha256.as_deref().map(digest).transpose()?,
    ))?;
    valid(RunSpec::new(task, provider, run_context, narrow(wire.max_started_attempts)?))
}

fn request_from(wire: WRequest) -> Result<ApprovalRequest, CorruptReason> {
    let fields = RequestFields {
        approval: approval_id(&wire.approval)?,
        run: valid(RunId::parse(&wire.run))?,
        attempt: attempt_no(wire.attempt)?,
        kind: lookup(&APPROVAL_KINDS, approval_kind_name, &wire.kind)?,
        binding: binding_from(wire.binding)?,
        binding_digest: digest(&wire.binding_digest)?,
        description_sha256: digest(&wire.description_sha256)?,
        provider_risk_claim: wire.provider_risk_claim.as_deref().map(|name| lookup(&RISKS, risk_name, name)).transpose()?,
        ttl_ms: int(wire.ttl_ms)?,
    };
    // v1 has no repository access, so the risk is computed for `WorkspaceMode::None`. The recorded risk and scope must
    // be exactly what Ordelis computes (the classification raised by the provider's claim), so a stored risk can never
    // be lowered or replaced.
    let request = valid(ApprovalRequest::new(fields, WorkspaceMode::None))?;
    if wire.risk != risk_name(request.risk()) || wire.scope != scope_name(request.scope()) {
        return Err(CorruptReason::Schema);
    }
    Ok(request)
}

fn body<T: for<'de> Deserialize<'de>>(value: serde_json::Value) -> Result<T, CorruptReason> {
    valid(serde_json::from_value(value))
}

fn event_from_wire(record: WireRecord) -> Result<Event, CorruptReason> {
    let v = valid(u16::try_from(int(record.v)?))?;
    let kind = lookup(&EventKind::ALL, kind_name, &record.kind)?;
    let event_body = match kind {
        EventKind::RunCreated => {
            let wire: WRunCreated = body(record.body)?;
            EventBody::RunCreated { spec: spec_from(wire.spec)?, spec_digest: digest(&wire.spec_digest)? }
        }
        EventKind::AttemptCreated => {
            let wire: WAttemptCreated = body(record.body)?;
            let continuation = match (wire.continuation.as_str(), wire.from_attempt, wire.session) {
                ("fresh", None, None) => Continuation::Fresh,
                ("resume", Some(from_attempt), Some(session)) => {
                    Continuation::Resume { from_attempt: attempt_no(from_attempt)?, session: short_token(&session)? }
                }
                _ => return Err(CorruptReason::Schema),
            };
            EventBody::AttemptCreated { attempt: attempt_no(wire.attempt)?, continuation }
        }
        EventKind::ApprovalRequested => EventBody::ApprovalRequested(request_from(body(record.body)?)?),
        EventKind::ApprovalDecided => {
            let wire: WDecided = body(record.body)?;
            EventBody::ApprovalDecided {
                approval: approval_id(&wire.approval)?,
                decision: lookup(&DECISIONS, decision_name, &wire.decision)?,
                provenance: DecisionProvenance {
                    actor: lookup(&ACTORS, actor_name, &wire.provenance.actor)?,
                    channel: lookup(&CHANNELS, channel_name, &wire.provenance.channel)?,
                    presented_binding_digest: digest(&wire.provenance.presented_binding_digest)?,
                },
            }
        }
        EventKind::ApprovalExpired => {
            let wire: WExpired = body(record.body)?;
            EventBody::ApprovalExpired { approval: approval_id(&wire.approval)?, basis: lookup(&EXPIRY_BASES, expiry_name, &wire.basis)? }
        }
        EventKind::ApprovalWithdrawn => {
            let wire: WApprovalEnded = body(record.body)?;
            EventBody::ApprovalWithdrawn { approval: approval_id(&wire.approval)?, reason: lookup(&WITHDRAWALS, withdrawal_name, &wire.reason)? }
        }
        EventKind::ApprovalVoided => {
            let wire: WApprovalEnded = body(record.body)?;
            EventBody::ApprovalVoided { approval: approval_id(&wire.approval)?, reason: lookup(&VOIDS, void_name, &wire.reason)? }
        }
        EventKind::ApprovalConsumed => {
            let wire: WConsumed = body(record.body)?;
            EventBody::ApprovalConsumed { approval: approval_id(&wire.approval)?, binding: binding_from(wire.binding)? }
        }
        EventKind::StartBegun => {
            let wire: WStartBegun = body(record.body)?;
            EventBody::StartBegun { attempt: attempt_no(wire.attempt)?, approval: approval_id(&wire.approval)?, binding: binding_from(wire.binding)? }
        }
        EventKind::AttemptStarted => {
            let wire: WStarted = body(record.body)?;
            let identity = &wire.os_identity;
            let os_identity = valid(OsIdentity::new(narrow(identity.pid)?, short_token(&identity.start_token)?, digest(&identity.executable_sha256)?))?;
            EventBody::AttemptStarted { attempt: attempt_no(wire.attempt)?, os_identity }
        }
        EventKind::StartFailed => {
            let wire: WStartFailed = body(record.body)?;
            EventBody::StartFailed { attempt: attempt_no(wire.attempt)?, code: lookup(&START_FAILURES, start_failure_name, &wire.code)? }
        }
        EventKind::StopRequested => {
            let wire: WAttemptReason = body(record.body)?;
            EventBody::StopRequested { attempt: attempt_no(wire.attempt)?, reason: lookup(&STOP_REASONS, stop_name, &wire.reason)? }
        }
        EventKind::CancelRequested => {
            let wire: WCancel = body(record.body)?;
            let scope = match (wire.scope.as_str(), wire.attempt) {
                ("attempt", Some(number)) => CancelScope::Attempt(attempt_no(number)?),
                ("run", None) => CancelScope::Run,
                _ => return Err(CorruptReason::Schema),
            };
            EventBody::CancelRequested { scope }
        }
        EventKind::AttemptExited => {
            let wire: WExited = body(record.body)?;
            let code = wire.status.code.map(i32::try_from).transpose().map_err(|_| CorruptReason::Schema)?;
            let ended_by = wire.status.ended_by.map(narrow::<u8>).transpose()?;
            EventBody::AttemptExited { attempt: attempt_no(wire.attempt)?, status: valid(ExitStatus::new(code, ended_by))? }
        }
        EventKind::AttemptInterrupted => {
            let wire: WInterrupted = body(record.body)?;
            EventBody::AttemptInterrupted { attempt: attempt_no(wire.attempt)?, observation: lookup(&OBSERVATIONS, observation_name, &wire.observation)? }
        }
        EventKind::AttemptNotStarted => {
            let wire: WAttemptReason = body(record.body)?;
            EventBody::AttemptNotStarted { attempt: attempt_no(wire.attempt)?, reason: lookup(&NOT_STARTED_CAUSES, not_started_name, &wire.reason)? }
        }
        EventKind::ProviderSessionObserved => {
            let wire: WSession = body(record.body)?;
            EventBody::ProviderSessionObserved { attempt: attempt_no(wire.attempt)?, session: short_token(&wire.session)? }
        }
        EventKind::Progress => {
            let wire: WProgress = body(record.body)?;
            EventBody::Progress { attempt: attempt_no(wire.attempt)?, kind: lookup(&PROGRESS_KINDS, progress_name, &wire.kind)? }
        }
        EventKind::ProviderOutcomeClaimed => {
            let wire: WClaim = body(record.body)?;
            EventBody::ProviderOutcomeClaimed { attempt: attempt_no(wire.attempt)?, claim: lookup(&CLAIMS, claim_name, &wire.claim)? }
        }
        EventKind::RunClosed => {
            let wire: WClosed = body(record.body)?;
            EventBody::RunClosed { reason: lookup(&CLOSE_REASONS, close_name, &wire.reason)? }
        }
        EventKind::AttentionResolved => {
            let wire: WResolved = body(record.body)?;
            let concern = valid(ConcernRef::new(
                lookup(&CONCERN_KINDS, concern_name, &wire.concern.kind)?,
                wire.concern.attempt.map(attempt_no).transpose()?,
                valid(Seq::new(int(wire.concern.raised_at)?))?,
            ))?;
            let provenance = ResolutionProvenance {
                actor: lookup(&ACTORS, actor_name, &wire.provenance.actor)?,
                channel: lookup(&CHANNELS, channel_name, &wire.provenance.channel)?,
                presented_sha256: digest(&wire.provenance.presented_sha256)?,
            };
            let resolution = lookup(&RESOLUTIONS, resolution_name, &wire.resolution)?;
            EventBody::AttentionResolved(valid(ConcernResolution::new(concern, resolution, provenance))?)
        }
        EventKind::JournalRepaired => {
            let wire: WRepaired = body(record.body)?;
            EventBody::JournalRepaired(valid(JournalRepair::new(
                narrow(wire.sidecar)?,
                narrow(wire.removed_bytes)?,
                narrow(wire.removed_crc32c)?,
                lookup(&REPAIR_CAUSES, repair_cause_name, &wire.cause)?,
            ))?)
        }
    };
    Ok(Event {
        v,
        run: valid(RunId::parse(&record.run))?,
        seq: valid(Seq::new(int(record.seq)?))?,
        at_ms: wall(record.at_ms)?,
        clamped: record.clamped,
        producer: lookup(&PRODUCERS, producer_name, &record.producer)?,
        body: event_body,
    })
}

#[cfg(test)]
pub(crate) mod tests;
