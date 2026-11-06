// Full deterministic replay of a decoded journal (D3.2b-4, issue #118; design r5 sections 4.6 and 5.7). Pure: it takes
// what `decode_stream` produced and rebuilds the run's projection frame by frame through the reducer's own
// `apply_recorded`, the same transition function as the live path, so live and replayed history cannot diverge.
//
// Replay produces history, never authority. A `ReplayedHistory` is read-only: it holds no supervision, no grant and no
// clock, and nothing turns it into one. The only way on is `RecordedHistory::from_replay` (reconcile.rs), which derives
// the history's status from this replay instead of taking it from the caller, and then reconciliation, whose
// `begin_reconciled` mints nothing.
use super::codec::{CorruptReason, Decoded, StreamEnd};
use super::model::{JournalFault, RunId, Seq};
use super::reduce::{apply_recorded, RunProjection};

/// The end of one replayed frame: its last byte offset (exclusive) and its last sequence number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Boundary {
    end: u64,
    last: Seq,
}

/// The bytes after the last valid frame of a stream that ended in an incomplete tail: never interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TailBytes {
    pub(crate) offset: u64,
    pub(crate) len: u32,
    pub(crate) crc32c: u32,
}

/// A run's history rebuilt from its decoded journal. Built only by `replay`; read-only.
pub(crate) struct ReplayedHistory {
    projection: RunProjection,
    fault: Option<JournalFault>,
    damage: Option<CorruptReason>,
    boundaries: Vec<Boundary>,
    tail: Option<TailBytes>,
}

impl ReplayedHistory {
    /// The state after the last valid record: the whole journal when there is no fault.
    pub(crate) fn projection(&self) -> &RunProjection {
        &self.projection
    }

    /// The first fault, which stops replay and quarantines the run: a reducer fault, `Codec` for corrupt bytes, or
    /// `NewerVersion` for a record of a version this code does not know.
    pub(crate) const fn fault(&self) -> Option<JournalFault> {
        self.fault
    }

    /// What the codec found when the fault is `Codec`, for display.
    pub(crate) const fn damage(&self) -> Option<CorruptReason> {
        self.damage
    }

    /// The incomplete tail after the last frame, when the stream ended in one and nothing faulted before it.
    pub(crate) const fn tail(&self) -> Option<TailBytes> {
        self.tail
    }

    /// The offset where the replayed frames end.
    pub(crate) fn valid_end(&self) -> u64 {
        self.boundaries.last().map_or(0, |boundary| boundary.end)
    }

    /// The bytes the run is held to (D3.2c-2): the replayed frames and an incomplete tail, if any. A torn tail counts until
    /// it is removed, so it can never lower the capacity the run is checked against.
    pub(crate) fn journal_bytes(&self) -> u64 {
        self.valid_end() + self.tail.map_or(0, |tail| u64::from(tail.len))
    }

    /// Checks the run's high-water mark against the replayed prefix (r5 5.7 step 3). The mark `(last_seq, byte_length)`
    /// is written durably after every acknowledged critical frame, and `(0, 0)` before the journal exists. A prefix
    /// that ends before the mark, or no frame ending exactly at the mark with the mark's sequence number, means
    /// acknowledged frames may be missing.
    pub(crate) fn check_mark(&self, mark: MarkReading) -> Result<(), MarkProblem> {
        let (last_seq, byte_length) = match mark {
            MarkReading::Read { last_seq, byte_length } => (last_seq, byte_length),
            MarkReading::Missing => return Err(MarkProblem::Missing),
            MarkReading::Unreadable => return Err(MarkProblem::Unreadable),
        };
        if (last_seq == 0) != (byte_length == 0) {
            return Err(MarkProblem::Invalid);
        }
        if byte_length == 0 {
            return Ok(());
        }
        if self.valid_end() < byte_length {
            return Err(MarkProblem::NotReached);
        }
        match self.boundaries.iter().find(|boundary| boundary.end == byte_length) {
            Some(boundary) if boundary.last.number() == last_seq => Ok(()),
            _ => Err(MarkProblem::Mismatch),
        }
    }

    /// The projection, giving up the replay. Only `RecordedHistory::from_replay` needs it.
    pub(crate) fn into_projection(self) -> RunProjection {
        self.projection
    }
}

/// The run's high-water mark as the caller read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarkReading {
    Read { last_seq: u64, byte_length: u64 },
    Missing,
    Unreadable,
}

/// Why the mark does not vouch for the replayed prefix. Every one means acknowledged frames may be lost: the future
/// adapter writes a quarantine marker (r5 5.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarkProblem {
    Missing,
    Unreadable,
    /// One of `last_seq` and `byte_length` is zero and the other is not.
    Invalid,
    /// The valid prefix ends before the mark.
    NotReached,
    /// No replayed frame ends exactly at the mark with the mark's sequence number.
    Mismatch,
}

/// Replays `decoded` as the journal of `run`, frame by frame, from the empty projection. The first fault stops it:
/// a frame the reducer refuses (wrong run, sequence gap, decreasing time or version, a forged producer label, a
/// duplicate key, an invalid transition, consequences that are not exactly the generated ones, a limit), corrupt bytes
/// (`Codec`) or a newer version. Records after a fault are never applied. Deterministic and total.
pub(crate) fn replay(run: RunId, decoded: &Decoded) -> ReplayedHistory {
    let mut projection = RunProjection::empty(run);
    let mut boundaries = Vec::new();
    let mut fault = None;
    for frame in decoded.frames() {
        match apply_recorded(&projection, frame.events()) {
            Ok(next) => {
                projection = next;
                if let Some(last) = frame.events().last() {
                    boundaries.push(Boundary { end: frame.end(), last: last.seq });
                }
            }
            Err(found) => {
                fault = Some(found);
                break;
            }
        }
    }
    let (mut damage, mut tail) = (None, None);
    if fault.is_none() {
        match decoded.end() {
            StreamEnd::Clean => {}
            StreamEnd::IncompleteTail { offset, len, crc32c } => tail = Some(TailBytes { offset, len, crc32c }),
            StreamEnd::Corrupt { reason, .. } => {
                fault = Some(JournalFault::Codec);
                damage = Some(reason);
            }
            StreamEnd::NewerVersion { .. } => fault = Some(JournalFault::NewerVersion),
        }
    }
    ReplayedHistory { projection, fault, damage, boundaries, tail }
}

#[cfg(test)]
mod tests;
