// The evidence vocabulary, mirrored from runtime/runtime_validation.py (see docs/guides/runtime-validation-foundations.md
// and the differences listed in docs/implementation/desktop-agents.md). It judges supplied records and nothing else.
//
// Records are unauthenticated caller data: an evaluation cannot tell a record built by trusted code from a forged one,
// so it has no field that means authorized, dispatchable or verified-by-Ordelis. A `Satisfied` status is consistency
// of the supplied records with a requirement, nothing more. No record is ever produced from a clock, a file, the
// environment or a process here; timestamps are values the caller states.
use super::catalog::{self, DocStatus, Feature, ProviderId};

/// At most this many records are judged at once, as in Python.
pub(crate) const MAX_EVIDENCE_RECORDS: usize = 64;

/// Largest timestamp accepted, in milliseconds: the largest integer a JSON number holds exactly.
pub(crate) const MAX_TIMESTAMP_MS: u64 = (1 << 53) - 1;

/// The eight capabilities of runtime_validation.py, in its order and with its names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Capability {
    ExecutableIdentity,
    ModelAcceptance,
    ReasoningEffort,
    ReadOnlyFilesystem,
    NetworkRestriction,
    OutputFormat,
    ProcessTermination,
    SessionPersistence,
}

impl Capability {
    pub(crate) const ALL: [Capability; 8] = [
        Capability::ExecutableIdentity,
        Capability::ModelAcceptance,
        Capability::ReasoningEffort,
        Capability::ReadOnlyFilesystem,
        Capability::NetworkRestriction,
        Capability::OutputFormat,
        Capability::ProcessTermination,
        Capability::SessionPersistence,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Capability::ExecutableIdentity => "executable_identity",
            Capability::ModelAcceptance => "model_acceptance",
            Capability::ReasoningEffort => "reasoning_effort",
            Capability::ReadOnlyFilesystem => "read_only_filesystem",
            Capability::NetworkRestriction => "network_restriction",
            Capability::OutputFormat => "output_format",
            Capability::ProcessTermination => "process_termination",
            Capability::SessionPersistence => "session_persistence",
        }
    }

    /// Only these capabilities carry a requested and an observed value (Python's `_VALUED`).
    pub(crate) const fn valued(self) -> bool {
        matches!(self, Capability::ModelAcceptance | Capability::ReasoningEffort)
    }

    /// Isolation properties cannot be inferred from a provider's own behavior or output (Python's `_ISOLATION`).
    pub(crate) const fn isolation(self) -> bool {
        matches!(self, Capability::ReadOnlyFilesystem | Capability::NetworkRestriction | Capability::SessionPersistence)
    }

    /// The weakest level that can satisfy a requirement on this capability (Python's `_MINIMUM_LEVEL`).
    pub(crate) const fn minimum_level(self) -> Level {
        if self.isolation() {
            Level::IndependentlyVerified
        } else {
            Level::RuntimeObserved
        }
    }
}

/// Where a record says it comes from, weakest first. Each source fixes exactly one level.
/// `VendorDocumentation` is the one addition to Python's four sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Source {
    /// A vendor's documentation, about the product in general rather than any installed file.
    VendorDocumentation,
    AdapterDeclaration,
    CliHelp,
    RuntimeProbe,
    ExternalCheck,
}

impl Source {
    pub(crate) const ALL: [Source; 5] =
        [Source::VendorDocumentation, Source::AdapterDeclaration, Source::CliHelp, Source::RuntimeProbe, Source::ExternalCheck];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Source::VendorDocumentation => "vendor_documentation",
            Source::AdapterDeclaration => "adapter_declaration",
            Source::CliHelp => "cli_help",
            Source::RuntimeProbe => "runtime_probe",
            Source::ExternalCheck => "external_check",
        }
    }

    /// The source's ceiling: a record can never claim more than this level.
    pub(crate) const fn level(self) -> Level {
        match self {
            Source::VendorDocumentation => Level::VendorDocumented,
            Source::AdapterDeclaration => Level::AdapterDeclared,
            Source::CliHelp => Level::CliAdvertised,
            Source::RuntimeProbe => Level::RuntimeObserved,
            Source::ExternalCheck => Level::IndependentlyVerified,
        }
    }
}

/// Evidence levels, weakest first. The last four are Python's `LEVELS`, with their names and order unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Level {
    VendorDocumented,
    AdapterDeclared,
    CliAdvertised,
    RuntimeObserved,
    IndependentlyVerified,
}

impl Level {
    pub(crate) const ALL: [Level; 5] = [
        Level::VendorDocumented,
        Level::AdapterDeclared,
        Level::CliAdvertised,
        Level::RuntimeObserved,
        Level::IndependentlyVerified,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Level::VendorDocumented => "vendor_documented",
            Level::AdapterDeclared => "adapter_declared",
            Level::CliAdvertised => "cli_advertised",
            Level::RuntimeObserved => "runtime_observed",
            Level::IndependentlyVerified => "independently_verified",
        }
    }

    /// The coarse class shown to people. There is deliberately no "enforced" class: an independently verified
    /// observation is not the Execution Safety Gate's independently enforced control, which needs a mechanism Ordelis
    /// applied and which nothing here can express.
    pub(crate) const fn class(self) -> Class {
        match self {
            Level::VendorDocumented | Level::AdapterDeclared | Level::CliAdvertised => Class::Advertised,
            Level::RuntimeObserved => Class::Observed,
            Level::IndependentlyVerified => Class::Verified,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    /// Someone says it: a vendor, an adapter or a help text.
    Advertised,
    /// Seen in a run, with the provider's own output as the witness.
    Observed,
    /// Checked by Ordelis from outside the provider process.
    Verified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Pass,
    Fail,
    Unsupported,
}

impl Outcome {
    pub(crate) const ALL: [Outcome; 3] = [Outcome::Pass, Outcome::Fail, Outcome::Unsupported];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Outcome::Pass => "pass",
            Outcome::Fail => "fail",
            Outcome::Unsupported => "unsupported",
        }
    }
}

/// The closed failure vocabulary for capabilities without values (Python's `OBSERVATIONS`). Never allowed on a pass.
pub(crate) const OBSERVATIONS: [&str; 7] =
    ["timeout", "cancelled", "descendant_survived", "write_succeeded", "network_reachable", "session_data_written", "malformed_output"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EvidenceError {
    /// Not 1 to 128 characters of `[A-Za-z0-9._:/@+=-]`.
    InvalidToken,
    /// Not 64 lowercase hexadecimal characters.
    InvalidSha256,
    /// Values on a capability without values, or an observation outside the closed vocabulary or on a pass.
    InvalidValues,
    /// Vendor documentation must be bound to a provider, and every other source to one probe run.
    SourceBindingMismatch,
    /// Documentation describes; it cannot witness a failure or observe a value.
    InvalidDocumentationRecord,
    /// Above MAX_TIMESTAMP_MS.
    InvalidTimestamp,
    /// More than MAX_EVIDENCE_RECORDS records.
    TooManyRecords,
    /// A capability required twice, or a requested value on a capability without values.
    InvalidRequirement,
}

/// A short identifier or value (Python's `_TOKEN`), so that no record can carry free text, a prompt or a secret-shaped
/// string longer than 128 characters.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Token(String);

impl Token {
    pub(crate) fn new(value: &str) -> Result<Token, EvidenceError> {
        let allowed = |b: u8| b.is_ascii_alphanumeric() || b"._:/@+=-".contains(&b);
        if (1..=128).contains(&value.len()) && value.bytes().all(allowed) {
            Ok(Token(value.to_owned()))
        } else {
            Err(EvidenceError::InvalidToken)
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// The SHA-256 of the resolved executable file a probe ran against, as 64 lowercase hexadecimal characters. It
/// identifies that file only: a launcher script's hash says nothing about the interpreter or package it starts.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Sha256(String);

impl Sha256 {
    pub(crate) fn new(value: &str) -> Result<Sha256, EvidenceError> {
        if value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            Ok(Sha256(value.to_owned()))
        } else {
            Err(EvidenceError::InvalidSha256)
        }
    }
}

/// What a record is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Binding {
    /// A provider's documentation, tied to no executable and no run. Never stale, and never enough to satisfy.
    Documentation(ProviderId),
    /// One probe run against one executable file.
    Probe { executable_sha256: Sha256, probe_id: Token },
}

/// One fact about one capability. Fields are private: a record exists only if it passed validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Record {
    capability: Capability,
    source: Source,
    outcome: Outcome,
    binding: Binding,
    requested: Option<Token>,
    observed: Option<Token>,
    /// When the producing code recorded the fact, as the caller states it. Provenance only: no rule here reads it,
    /// so a later record never overrides an earlier failure.
    recorded_at_ms: u64,
}

impl Record {
    pub(crate) fn new(
        capability: Capability,
        source: Source,
        outcome: Outcome,
        binding: Binding,
        requested: Option<&str>,
        observed: Option<&str>,
        recorded_at_ms: u64,
    ) -> Result<Record, EvidenceError> {
        let requested = requested.map(Token::new).transpose()?;
        let observed = observed.map(Token::new).transpose()?;
        if !capability.valued()
            && (requested.is_some()
                || observed.as_ref().is_some_and(|o| !OBSERVATIONS.contains(&o.as_str()) || outcome == Outcome::Pass))
        {
            return Err(EvidenceError::InvalidValues);
        }
        let documentation = source == Source::VendorDocumentation;
        if documentation != matches!(binding, Binding::Documentation(_)) {
            return Err(EvidenceError::SourceBindingMismatch);
        }
        if documentation && (outcome == Outcome::Fail || requested.is_some() || observed.is_some()) {
            return Err(EvidenceError::InvalidDocumentationRecord);
        }
        if recorded_at_ms > MAX_TIMESTAMP_MS {
            return Err(EvidenceError::InvalidTimestamp);
        }
        Ok(Record { capability, source, outcome, binding, requested, observed, recorded_at_ms })
    }

    pub(crate) fn capability(&self) -> Capability {
        self.capability
    }

    pub(crate) fn source(&self) -> Source {
        self.source
    }

    pub(crate) fn outcome(&self) -> Outcome {
        self.outcome
    }

    pub(crate) fn binding(&self) -> &Binding {
        &self.binding
    }

    pub(crate) fn recorded_at_ms(&self) -> u64 {
        self.recorded_at_ms
    }
}

/// A capability a caller requires, with the value it wants for a valued capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Requirement {
    capability: Capability,
    wanted: Option<Token>,
}

impl Requirement {
    pub(crate) fn new(capability: Capability, wanted: Option<&str>) -> Result<Requirement, EvidenceError> {
        if wanted.is_some() && !capability.valued() {
            return Err(EvidenceError::InvalidRequirement);
        }
        Ok(Requirement { capability, wanted: wanted.map(Token::new).transpose()? })
    }
}

/// A capability's status, most severe first. Evaluation returns the first that applies, so a conflict always
/// resolves to the more severe status (Python's `_capability_status`, same names and order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Status {
    Failed,
    Mismatch,
    Unsupported,
    Satisfied,
    InsufficientEvidence,
    Unknown,
}

impl Status {
    pub(crate) const ALL: [Status; 6] =
        [Status::Failed, Status::Mismatch, Status::Unsupported, Status::Satisfied, Status::InsufficientEvidence, Status::Unknown];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Status::Failed => "failed",
            Status::Mismatch => "mismatch",
            Status::Unsupported => "unsupported",
            Status::Satisfied => "satisfied",
            Status::InsufficientEvidence => "insufficient_evidence",
            Status::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CapabilityResult {
    pub(crate) capability: Capability,
    pub(crate) required: bool,
    pub(crate) minimum_level: Level,
    /// The strongest level among applicable passing records, which may be below the minimum.
    pub(crate) highest_level: Option<Level>,
    pub(crate) status: Status,
}

/// The judgement of supplied records for one provider, one executable and one probe run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Evaluation {
    /// Every capability, in Capability::ALL order.
    pub(crate) capabilities: Vec<CapabilityResult>,
    /// Probe records for another executable file: ignored, because a changed file is a different configuration.
    pub(crate) stale_evidence: usize,
    /// Probe records for this file from another probe run: ignored.
    pub(crate) unbound_evidence: usize,
    /// Documentation records about another provider: ignored.
    pub(crate) other_provider_evidence: usize,
    /// The required capabilities that are not satisfied, with their status, in Capability::ALL order.
    pub(crate) blocking: Vec<(Capability, Status)>,
}

/// Judges `records` against `required` for one probe run. Applicable records are the documentation records about
/// `provider` and the probe records bound to exactly this executable hash and probe id; everything else is counted
/// and ignored.
pub(crate) fn evaluate(
    provider: ProviderId,
    executable_sha256: &Sha256,
    probe_id: &Token,
    required: &[Requirement],
    records: &[Record],
) -> Result<Evaluation, EvidenceError> {
    if records.len() > MAX_EVIDENCE_RECORDS {
        return Err(EvidenceError::TooManyRecords);
    }
    for (i, requirement) in required.iter().enumerate() {
        if required[..i].iter().any(|earlier| earlier.capability == requirement.capability) {
            return Err(EvidenceError::InvalidRequirement);
        }
    }
    let (mut stale, mut unbound, mut other_provider) = (0, 0, 0);
    let mut applicable = Vec::new();
    for record in records {
        match &record.binding {
            Binding::Documentation(about) if *about == provider => applicable.push(record),
            Binding::Documentation(_) => other_provider += 1,
            Binding::Probe { executable_sha256: hash, .. } if hash != executable_sha256 => stale += 1,
            Binding::Probe { probe_id: run, .. } if run != probe_id => unbound += 1,
            Binding::Probe { .. } => applicable.push(record),
        }
    }
    let mut capabilities = Vec::new();
    let mut blocking = Vec::new();
    for capability in Capability::ALL {
        let requirement = required.iter().find(|r| r.capability == capability);
        let wanted = requirement.and_then(|r| r.wanted.as_ref());
        let of_capability: Vec<&Record> = applicable.iter().copied().filter(|r| r.capability == capability).collect();
        let (status, highest_level) = capability_status(capability, wanted, &of_capability);
        if requirement.is_some() && status != Status::Satisfied {
            blocking.push((capability, status));
        }
        capabilities.push(CapabilityResult {
            capability,
            required: requirement.is_some(),
            minimum_level: capability.minimum_level(),
            highest_level,
            status,
        });
    }
    Ok(Evaluation { capabilities, stale_evidence: stale, unbound_evidence: unbound, other_provider_evidence: other_provider, blocking })
}

/// Python's `_capability_status`, rule for rule.
fn capability_status(capability: Capability, wanted: Option<&Token>, records: &[&Record]) -> (Status, Option<Level>) {
    let relevant: Vec<&Record> = records
        .iter()
        .copied()
        .filter(|r| wanted.is_none() || r.requested.is_none() || r.requested.as_ref() == wanted)
        .collect();
    let passed: Vec<&Record> = relevant.iter().copied().filter(|r| r.outcome == Outcome::Pass).collect();
    let highest = passed.iter().map(|r| r.source.level()).max();
    if relevant.iter().any(|r| r.outcome == Outcome::Fail) {
        return (Status::Failed, highest);
    }
    if wanted.is_some() && passed.iter().any(|r| r.observed.is_some() && r.observed.as_ref() != wanted) {
        return (Status::Mismatch, highest);
    }
    if relevant.iter().any(|r| r.outcome == Outcome::Unsupported) {
        return (Status::Unsupported, highest);
    }
    let satisfies = |r: &&Record| {
        r.source.level() >= capability.minimum_level()
            && (wanted.is_none() || (r.requested.as_ref() == wanted && r.observed.as_ref() == wanted))
    };
    if passed.iter().any(satisfies) {
        return (Status::Satisfied, highest);
    }
    (if passed.is_empty() { Status::Unknown } else { Status::InsufficientEvidence }, highest)
}

/// The capability a documented feature speaks to, where one exists. Most features have no counterpart among the
/// eight capabilities and produce no record.
pub(crate) const fn documented_capability(feature: Feature) -> Option<Capability> {
    match feature {
        Feature::StructuredOutput => Some(Capability::OutputFormat),
        Feature::ModelSelection => Some(Capability::ModelAcceptance),
        Feature::ReasoningEffort => Some(Capability::ReasoningEffort),
        Feature::SessionPersistenceOff => Some(Capability::SessionPersistence),
        _ => None,
    }
}

/// The catalog's statements about `provider`, as vendor-documentation records stamped `recorded_at_ms`. Only a
/// documented feature becomes a record, a pass; not found, contradictory and not reviewed produce nothing, so the
/// capability stays unknown. Every record is at the weakest level and can never satisfy a requirement.
pub(crate) fn documentation_records(provider: ProviderId, recorded_at_ms: u64) -> Result<Vec<Record>, EvidenceError> {
    let mut records = Vec::new();
    for claim in catalog::provider(provider).claims {
        if let (DocStatus::Documented, Some(capability)) = (claim.status, documented_capability(claim.feature)) {
            records.push(Record::new(
                capability,
                Source::VendorDocumentation,
                Outcome::Pass,
                Binding::Documentation(provider),
                None,
                None,
                recorded_at_ms,
            )?);
        }
    }
    Ok(records)
}

#[cfg(test)]
mod tests;
