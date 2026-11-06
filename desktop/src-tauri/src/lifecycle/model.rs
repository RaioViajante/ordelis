// The minimum stable lifecycle model of D3.2b design r5, section 11.1: identities, the run specification, the event
// envelope and the 22 event kinds with their bodies, run, attempt and approval states, outcome classes, concerns, risk
// classification, limits, and the closed rejection and fault codes. Constructors validate every bound the design
// fixes; nothing else here decides a transition. The reducer, the live layer, reconciliation and the codec are later,
// separately authorized increments (b-2 to b-4).
//
// Values are caller data. A decision, a grant state or an event says what was recorded, never that anything may be
// done: there is no type, field or variant here that could carry live authority, and none of these types can be
// turned into one.
use crate::agents::catalog::ProviderId;
use crate::agents::evidence::{Sha256, Token};

// ---------------------------------------------------------------------------------------------------------------
// Limits (r5 section 4.8 and 2.2). Values may still be tuned before the first persistence adapter freezes v1.

/// The record schema version this model writes.
pub(crate) const RECORD_VERSION: u16 = 1;
/// Attempts that may have executed (outcome class other than `never_started`) per run.
pub(crate) const MAX_STARTED_ATTEMPTS: u8 = 8;
/// Every `AttemptCreated` of a run, including attempts that never started.
pub(crate) const MAX_ATTEMPT_RECORDS: u8 = 32;
/// Approval requests per attempt, and open (pending or granted) approvals per attempt.
pub(crate) const MAX_APPROVALS_PER_ATTEMPT: u8 = 64;
pub(crate) const MAX_OPEN_APPROVALS: u8 = 4;
/// Events per run, of which the last `RESERVED_EVENTS` accept only reserved kinds.
pub(crate) const MAX_EVENTS: u32 = 4096;
pub(crate) const RESERVED_EVENTS: u32 = 64;
/// Events per frame, counting generated consequences.
pub(crate) const MAX_BATCH: usize = 8;
/// Canonical encoding of one event at its maximal field sizes, of a frame payload, and of a repairable tail.
pub(crate) const MAX_EVENT_BYTES: u32 = 2048;
pub(crate) const MAX_FRAME_BYTES: u32 = 17_408;
pub(crate) const MAX_TAIL_BYTES: u32 = MAX_FRAME_BYTES + 22;
/// Journal bytes per run, of which the last `RESERVED_JOURNAL_BYTES` accept only reserved kinds.
pub(crate) const MAX_JOURNAL_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const RESERVED_JOURNAL_BYTES: u64 = 512 * 1024;
/// The memory-only operation description, in bytes.
pub(crate) const MAX_DESCRIPTION_BYTES: usize = 2048;
pub(crate) const PROGRESS_COALESCE_MS: u64 = 10_000;
pub(crate) const GRANT_USE_MS: u64 = 60_000;
/// No approval is decided or used more than this long after its request.
pub(crate) const MAX_APPROVAL_LIFETIME_MS: u64 = 900_000;
pub(crate) const MAX_OBSERVATION_AGE_MS: u64 = 5_000;
/// The largest wall-clock value accepted, in milliseconds: the largest integer a JSON number holds exactly.
pub(crate) const MAX_WALL_MS: u64 = (1 << 53) - 1;
/// Ids are a prefix and 1 to 10 digits without a leading zero, as for tasks and projects.
const MAX_ID_NUMBER: u64 = 9_999_999_999;

/// Why a value was refused by its constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelError {
    /// Not the id's prefix followed by 1 to 10 digits without a leading zero, or a number outside 1 to 9 999 999 999.
    InvalidId,
    /// An attempt, sequence, revision, process id, sidecar or byte count outside its range.
    InvalidNumber,
    /// A wall-clock value above `MAX_WALL_MS`.
    InvalidTime,
    /// A duration outside the bounds of r5 section 2.2.
    InvalidLimits(LimitField),
    /// Requested permission classes not strictly increasing (sorted and unique).
    InvalidPermissions,
    /// `max_started_attempts` outside 1 to `MAX_STARTED_ATTEMPTS`.
    InvalidMaxStartedAttempts,
    /// An approval lifetime outside 1 000 to `MAX_APPROVAL_LIFETIME_MS`.
    InvalidTtl,
    /// A `start` request whose operation class is not `start`, or the reverse.
    KindClassMismatch,
    /// An operation description that is empty, too long, or holds a control character other than `\n` and `\t`.
    InvalidDescription,
    /// A concern reference whose attempt does not match its kind.
    InvalidConcernRef,
    /// A resolution that the concern's kind does not allow.
    ResolutionNotAllowed,
    /// An exit status with neither or both of an exit code and an ending signal number.
    InvalidExitStatus,
    /// An approval request offered by the wrong producer: start consent only from the supervisor, an operation
    /// request only from the provider relay.
    WrongProducer,
}

/// Which duration of `Limits` was out of bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LimitField {
    Start,
    Run,
    Stall,
    ApprovalTtl,
    StopGrace,
}

// ---------------------------------------------------------------------------------------------------------------
// Identities

/// `prefix` followed by 1 to 10 digits without a leading zero.
fn id_number(prefix: u8, text: &str) -> Option<u64> {
    let (first, digits) = text.as_bytes().split_first()?;
    if *first != prefix || !(1..=10).contains(&digits.len()) || digits[0] == b'0' || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(digits.iter().fold(0, |n, d| n * 10 + u64::from(d - b'0')))
}

fn checked_id(number: u64) -> Result<u64, ModelError> {
    if (1..=MAX_ID_NUMBER).contains(&number) {
        Ok(number)
    } else {
        Err(ModelError::InvalidId)
    }
}

/// A run, `r<n>`: allocated monotonically and never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RunId(u64);

impl RunId {
    pub(crate) fn new(number: u64) -> Result<RunId, ModelError> {
        checked_id(number).map(RunId)
    }

    pub(crate) fn parse(text: &str) -> Result<RunId, ModelError> {
        id_number(b'r', text).map(RunId).ok_or(ModelError::InvalidId)
    }

    pub(crate) const fn number(self) -> u64 {
        self.0
    }
}

/// An approval request within its run, `a<k>`: monotonic per run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ApprovalId(u64);

impl ApprovalId {
    pub(crate) fn new(number: u64) -> Result<ApprovalId, ModelError> {
        checked_id(number).map(ApprovalId)
    }

    pub(crate) fn parse(text: &str) -> Result<ApprovalId, ModelError> {
        id_number(b'a', text).map(ApprovalId).ok_or(ModelError::InvalidId)
    }

    pub(crate) const fn number(self) -> u64 {
        self.0
    }
}

/// A saved task's id, `t<n>`, as `tasks.json` v1 writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TaskId(u64);

impl TaskId {
    pub(crate) fn parse(text: &str) -> Result<TaskId, ModelError> {
        id_number(b't', text).map(TaskId).ok_or(ModelError::InvalidId)
    }

    pub(crate) const fn number(self) -> u64 {
        self.0
    }
}

/// A registered project's id, `p<n>`, as the project registry writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ProjectId(u64);

impl ProjectId {
    pub(crate) fn parse(text: &str) -> Result<ProjectId, ModelError> {
        id_number(b'p', text).map(ProjectId).ok_or(ModelError::InvalidId)
    }

    pub(crate) const fn number(self) -> u64 {
        self.0
    }
}

/// An attempt's number within its run, 1 to `MAX_ATTEMPT_RECORDS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct AttemptNo(u8);

impl AttemptNo {
    pub(crate) fn new(number: u8) -> Result<AttemptNo, ModelError> {
        if (1..=MAX_ATTEMPT_RECORDS).contains(&number) {
            Ok(AttemptNo(number))
        } else {
            Err(ModelError::InvalidNumber)
        }
    }

    pub(crate) const fn number(self) -> u8 {
        self.0
    }
}

/// An event's position in its run's journal: 1, then contiguous.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Seq(u64);

impl Seq {
    pub(crate) fn new(number: u64) -> Result<Seq, ModelError> {
        if (1..=MAX_WALL_MS).contains(&number) {
            Ok(Seq(number))
        } else {
            Err(ModelError::InvalidNumber)
        }
    }

    pub(crate) const fn number(self) -> u64 {
        self.0
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Time. Wall-clock values are provenance only; monotonic values exist only during one process lifetime. The two are
// different types with no conversion between them, and nothing here reads either clock.

/// A caller-stated wall-clock time in milliseconds, at most `MAX_WALL_MS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct WallMs(u64);

impl WallMs {
    pub(crate) fn new(ms: u64) -> Result<WallMs, ModelError> {
        if ms <= MAX_WALL_MS {
            Ok(WallMs(ms))
        } else {
            Err(ModelError::InvalidTime)
        }
    }

    pub(crate) const fn ms(self) -> u64 {
        self.0
    }
}

/// A caller-supplied monotonic time in milliseconds, meaningful only within one process lifetime. Never recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MonoMs(u64);

impl MonoMs {
    pub(crate) const fn new(ms: u64) -> MonoMs {
        MonoMs(ms)
    }

    pub(crate) const fn ms(self) -> u64 {
        self.0
    }
}

/// Identifies one live supervision; supplied by the caller and unique per supervision. Opaque: it can only be compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LiveEpoch(u128);

impl LiveEpoch {
    pub(crate) const fn new(value: u128) -> LiveEpoch {
        LiveEpoch(value)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The run specification. It is immutable: any change is a new run.

/// The digest of a run specification's canonical encoding, computed outside the core (the core compares, never hashes).
pub(crate) type SpecDigest = Sha256;

/// The task as it was when the run was created. Identity is by both halves, so a reused id is detected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TaskRef {
    task: TaskId,
    created_at_ms: WallMs,
    revision: u64,
}

impl TaskRef {
    pub(crate) fn new(task: TaskId, created_at_ms: WallMs, revision: u64) -> Result<TaskRef, ModelError> {
        // tasks.json v1 starts every task at revision 1; revisions stay JSON-safe integers.
        if revision == 0 || revision > MAX_WALL_MS {
            return Err(ModelError::InvalidNumber);
        }
        Ok(TaskRef { task, created_at_ms, revision })
    }

    pub(crate) const fn task(&self) -> TaskId {
        self.task
    }

    pub(crate) const fn created_at_ms(&self) -> WallMs {
        self.created_at_ms
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }
}

/// A registered project, by id and registration time, never rebound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProjectRef {
    pub(crate) project: ProjectId,
    pub(crate) registered_at_ms: WallMs,
}

/// Which provider, and which exact executable file, a run targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProviderIdentity {
    pub(crate) provider: ProviderId,
    pub(crate) executable_sha256: Sha256,
    /// What the provider reports about itself. Never compared for authority, and treated as sensitive.
    pub(crate) advertised_version: Option<Token>,
}

impl ProviderIdentity {
    /// The part of the identity that a grant is bound to; `advertised_version` is never part of it.
    pub(crate) fn binding_target(&self) -> BindingTarget {
        BindingTarget { provider: self.provider, executable_sha256: self.executable_sha256.clone() }
    }
}

/// The provider and exact executable file a grant is bound to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BindingTarget {
    pub(crate) provider: ProviderId,
    pub(crate) executable_sha256: Sha256,
}

/// Repository access. v1 has no repository access at all; other modes need D3.3 and a schema version bump.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceMode {
    None,
}

/// A permission class a run requests. Requested, never granted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum PermissionClass {
    ReadInWorkspace,
    WriteInWorkspace,
    ShellRun,
    FileDelete,
    OutsideWorkspace,
    NetworkAccess,
    PermissionChange,
}

/// Durations of a run, in milliseconds. They are limits, not clock readings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    start_ms: u64,
    run_ms: u64,
    stall_ms: u64,
    approval_ttl_ms: u64,
    stop_grace_ms: u64,
}

impl Limits {
    /// Bounds of r5 section 2.2. Nothing is clamped: an out-of-range value refuses the whole set.
    pub(crate) fn new(start_ms: u64, run_ms: u64, stall_ms: u64, approval_ttl_ms: u64, stop_grace_ms: u64) -> Result<Limits, ModelError> {
        let checks = [
            (LimitField::Start, (1_000..=600_000).contains(&start_ms)),
            (LimitField::Run, (1_000..=86_400_000).contains(&run_ms)),
            (LimitField::Stall, stall_ms >= 3 * PROGRESS_COALESCE_MS && stall_ms <= run_ms),
            (LimitField::ApprovalTtl, (1_000..=MAX_APPROVAL_LIFETIME_MS).contains(&approval_ttl_ms)),
            (LimitField::StopGrace, stop_grace_ms <= 60_000),
        ];
        match checks.iter().find(|(_, ok)| !ok) {
            Some((field, _)) => Err(ModelError::InvalidLimits(*field)),
            None => Ok(Limits { start_ms, run_ms, stall_ms, approval_ttl_ms, stop_grace_ms }),
        }
    }

    pub(crate) const fn start_ms(&self) -> u64 {
        self.start_ms
    }

    pub(crate) const fn run_ms(&self) -> u64 {
        self.run_ms
    }

    pub(crate) const fn stall_ms(&self) -> u64 {
        self.stall_ms
    }

    pub(crate) const fn approval_ttl_ms(&self) -> u64 {
        self.approval_ttl_ms
    }

    pub(crate) const fn stop_grace_ms(&self) -> u64 {
        self.stop_grace_ms
    }
}

/// The immutable conditions a run executes under (the design's ExecutionContext).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunContext {
    project: ProjectRef,
    workspace: WorkspaceMode,
    permissions: Vec<PermissionClass>,
    limits: Limits,
    policy_version: Token,
    environment_sha256: Option<Sha256>,
}

impl RunContext {
    pub(crate) fn new(
        project: ProjectRef,
        workspace: WorkspaceMode,
        permissions: Vec<PermissionClass>,
        limits: Limits,
        policy_version: Token,
        environment_sha256: Option<Sha256>,
    ) -> Result<RunContext, ModelError> {
        if permissions.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(ModelError::InvalidPermissions);
        }
        Ok(RunContext { project, workspace, permissions, limits, policy_version, environment_sha256 })
    }

    pub(crate) const fn project(&self) -> ProjectRef {
        self.project
    }

    pub(crate) const fn workspace(&self) -> WorkspaceMode {
        self.workspace
    }

    pub(crate) fn permissions(&self) -> &[PermissionClass] {
        &self.permissions
    }

    pub(crate) const fn limits(&self) -> Limits {
        self.limits
    }

    pub(crate) fn policy_version(&self) -> &Token {
        &self.policy_version
    }

    pub(crate) fn environment_sha256(&self) -> Option<&Sha256> {
        self.environment_sha256.as_ref()
    }
}

/// One fixed specification for one provider identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunSpec {
    task: TaskRef,
    provider: ProviderIdentity,
    context: RunContext,
    max_started_attempts: u8,
}

impl RunSpec {
    pub(crate) fn new(task: TaskRef, provider: ProviderIdentity, context: RunContext, max_started_attempts: u8) -> Result<RunSpec, ModelError> {
        if (1..=MAX_STARTED_ATTEMPTS).contains(&max_started_attempts) {
            Ok(RunSpec { task, provider, context, max_started_attempts })
        } else {
            Err(ModelError::InvalidMaxStartedAttempts)
        }
    }

    pub(crate) const fn task(&self) -> TaskRef {
        self.task
    }

    pub(crate) fn provider(&self) -> &ProviderIdentity {
        &self.provider
    }

    pub(crate) fn context(&self) -> &RunContext {
        &self.context
    }

    pub(crate) const fn max_started_attempts(&self) -> u8 {
        self.max_started_attempts
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Approvals

/// What an approval is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ApprovalKind {
    Start,
    Operation,
}

/// The class of the exact operation an approval covers. `Other` is anything unrecognized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperationClass {
    Start,
    ReadInWorkspace,
    WriteInWorkspace,
    ShellRun,
    FileDelete,
    OutsideWorkspace,
    NetworkAccess,
    PermissionChange,
    Other,
}

/// Risk, weakest first. There is no routine or low class: any approval is consent to something consequential.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RiskClass {
    Elevated,
    High,
    Critical,
}

/// The risk of an operation class, computed by Ordelis and never taken from a provider (r5 section 6.4). Without
/// repository access every workspace class is in fact outside the workspace, and unrecognized classes fail closed.
pub(crate) fn classify(class: OperationClass, workspace: WorkspaceMode) -> RiskClass {
    match (class, workspace) {
        (OperationClass::Start, _) => RiskClass::High,
        (OperationClass::ReadInWorkspace | OperationClass::WriteInWorkspace, WorkspaceMode::None) => RiskClass::Critical,
        (
            OperationClass::ShellRun
            | OperationClass::FileDelete
            | OperationClass::OutsideWorkspace
            | OperationClass::NetworkAccess
            | OperationClass::PermissionChange
            | OperationClass::Other,
            _,
        ) => RiskClass::Critical,
    }
}

/// What a grant is valid for, and nothing else. Every field is compared when a grant is consumed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) spec_digest: SpecDigest,
    pub(crate) target: BindingTarget,
    pub(crate) operation_class: OperationClass,
    pub(crate) operation_digest: Sha256,
}

/// The only approval scope of v1: one use. There is no "always", session-wide or pattern scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthorityScope {
    Once,
}

/// The fields of an approval request as the caller supplies them; `ApprovalRequest::new` validates them and
/// computes the risk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RequestFields {
    pub(crate) approval: ApprovalId,
    pub(crate) run: RunId,
    pub(crate) attempt: AttemptNo,
    pub(crate) kind: ApprovalKind,
    pub(crate) binding: Binding,
    pub(crate) binding_digest: Sha256,
    pub(crate) description_sha256: Sha256,
    pub(crate) provider_risk_claim: Option<RiskClass>,
    pub(crate) ttl_ms: u64,
}

/// A request for one human decision about one exact operation inside one attempt. Its request time is its event's
/// `at_ms`; the body holds no wall-clock time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApprovalRequest {
    fields: RequestFields,
    scope: AuthorityScope,
    risk: RiskClass,
}

impl ApprovalRequest {
    /// Refuses a `start` request whose operation class is not `start` (and the reverse) and a lifetime outside
    /// 1 000 ms to 900 s. The risk is the classification, raised (never lowered) by the provider's claim. Checks that
    /// need the run (`spec_digest`, target, `ttl_ms` against the run's limits) belong to the reducer.
    pub(crate) fn new(fields: RequestFields, workspace: WorkspaceMode) -> Result<ApprovalRequest, ModelError> {
        let start_class = fields.binding.operation_class == OperationClass::Start;
        if (fields.kind == ApprovalKind::Start) != start_class {
            return Err(ModelError::KindClassMismatch);
        }
        if fields.ttl_ms < 1_000 || fields.ttl_ms > MAX_APPROVAL_LIFETIME_MS {
            return Err(ModelError::InvalidTtl);
        }
        let classified = classify(fields.binding.operation_class, workspace);
        let risk = fields.provider_risk_claim.map_or(classified, |claim| claim.max(classified));
        Ok(ApprovalRequest { fields, scope: AuthorityScope::Once, risk })
    }

    pub(crate) fn fields(&self) -> &RequestFields {
        &self.fields
    }

    pub(crate) const fn scope(&self) -> AuthorityScope {
        self.scope
    }

    pub(crate) const fn risk(&self) -> RiskClass {
        self.risk
    }
}

/// The text an agent proposes for an operation: untrusted, sensitive, and memory-only in v1 (events carry only its
/// digest). It may hold anything, including secrets and paths; it is bounded, not sanitized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OperationDescription(String);

impl OperationDescription {
    pub(crate) fn new(text: &str) -> Result<OperationDescription, ModelError> {
        let allowed = |c: char| !c.is_control() || c == '\n' || c == '\t';
        if !text.is_empty() && text.len() <= MAX_DESCRIPTION_BYTES && text.chars().all(allowed) {
            Ok(OperationDescription(text.to_owned()))
        } else {
            Err(ModelError::InvalidDescription)
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Who decides or resolves: only a person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Actor {
    User,
}

/// How a decision or resolution was presented. The mechanism is #108 decision 5 (deferred).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Channel {
    NativeConfirmation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Granted,
    Denied,
}

/// What was shown when the decision was made. The core can compare the digest; it cannot know what was displayed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecisionProvenance {
    pub(crate) actor: Actor,
    pub(crate) channel: Channel,
    pub(crate) presented_binding_digest: Sha256,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExpiryBasis {
    Monotonic,
    Wall,
    ClockFault,
}

/// Why a pending request was withdrawn. `ClockFault` is r5 gate-review finding F9.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WithdrawalReason {
    AttemptEnded,
    Cancelled,
    SupervisionLost,
    ClockFault,
}

/// Why an unused grant was voided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoidReason {
    AttemptEnded,
    Cancelled,
    UseByPassed,
    SupervisionLost,
    WriteUncertain,
    ClockFault,
}

// ---------------------------------------------------------------------------------------------------------------
// Attempts

/// How an attempt continues its run: from scratch, or resuming a provider session observed earlier in the same run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Continuation {
    Fresh,
    Resume { from_attempt: AttemptNo, session: Token },
}

/// The recorded identity of a started child, used only as evidence for review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OsIdentity {
    pid: u32,
    start_token: Token,
    executable_sha256: Sha256,
}

impl OsIdentity {
    pub(crate) fn new(pid: u32, start_token: Token, executable_sha256: Sha256) -> Result<OsIdentity, ModelError> {
        if pid == 0 {
            return Err(ModelError::InvalidNumber);
        }
        Ok(OsIdentity { pid, start_token, executable_sha256 })
    }

    pub(crate) const fn pid(&self) -> u32 {
        self.pid
    }

    pub(crate) fn start_token(&self) -> &Token {
        &self.start_token
    }

    pub(crate) fn executable_sha256(&self) -> &Sha256 {
        &self.executable_sha256
    }
}

/// Why a start did not happen, reported only when the supervisor knows no child ever existed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartFailCode {
    /// Refused before exec, for example by a gate or policy denial.
    SpawnRefused,
    /// The executable's hash, taken again just before exec, differed.
    HashMismatch,
    /// The spawn call failed and created no child.
    StartError,
    /// The start record landed but the exec was skipped: the append was uncertain, or the high-water mark could not
    /// be written (r5 gate-review finding F4).
    EffectNotPerformed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeadlineKind {
    Start,
    Run,
    Stall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopReason {
    Deadline(DeadlineKind),
    JournalFull,
    ClockFault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CancelScope {
    Attempt(AttemptNo),
    Run,
}

/// How a child ended: exactly one of an exit code and the number of the signal that ended it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExitStatus {
    code: Option<i32>,
    ended_by: Option<u8>,
}

impl ExitStatus {
    pub(crate) fn new(code: Option<i32>, ended_by: Option<u8>) -> Result<ExitStatus, ModelError> {
        match (code, ended_by) {
            (Some(_), None) | (None, Some(1..=127)) => Ok(ExitStatus { code, ended_by }),
            _ => Err(ModelError::InvalidExitStatus),
        }
    }

    pub(crate) const fn code(&self) -> Option<i32> {
        self.code
    }

    pub(crate) const fn ended_by(&self) -> Option<u8> {
        self.ended_by
    }

    /// The only status that can end an attempt as `succeeded`, absent a provider's failure claim.
    pub(crate) const fn is_zero_exit(&self) -> bool {
        matches!(self.code, Some(0))
    }
}

/// What reconciliation could say about an attempt it ended. None of these is a success or a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InterruptObservation {
    NoIdentity,
    NotRunning,
    Unknown,
    PossiblyRunning,
    EvidenceLost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotStartedCause {
    SupervisionLost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProgressKind {
    Heartbeat,
    ToolActivity,
}

/// A provider can only worsen an outcome: the one claim it can make is failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutcomeClaim {
    Failure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CloseReason {
    ByUser,
}

// ---------------------------------------------------------------------------------------------------------------
// Concerns

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ConcernKind {
    PossibleOrphan,
    WorkspaceUnknown,
    JournalRepaired,
    ClockIrregular,
}

/// Only an unresolved blocking concern prevents a new attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConcernClass {
    Blocking,
    Informational,
}

/// A user statement about a concern. Ordelis verifies none of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Resolution {
    ConfirmedGone,
    StoppedOutsideOrdelis,
    AcceptedRisk,
    Reviewed,
    Acknowledged,
}

impl ConcernKind {
    pub(crate) const fn class(self) -> ConcernClass {
        match self {
            ConcernKind::PossibleOrphan => ConcernClass::Blocking,
            ConcernKind::WorkspaceUnknown | ConcernKind::JournalRepaired | ConcernKind::ClockIrregular => ConcernClass::Informational,
        }
    }

    /// Orphan and workspace concerns are about one attempt; repair and clock concerns are about the run.
    pub(crate) const fn about_attempt(self) -> bool {
        matches!(self, ConcernKind::PossibleOrphan | ConcernKind::WorkspaceUnknown)
    }

    pub(crate) const fn allows(self, resolution: Resolution) -> bool {
        match self {
            ConcernKind::PossibleOrphan => matches!(
                resolution,
                Resolution::ConfirmedGone | Resolution::StoppedOutsideOrdelis | Resolution::AcceptedRisk
            ),
            ConcernKind::WorkspaceUnknown => matches!(resolution, Resolution::Reviewed | Resolution::AcceptedRisk),
            ConcernKind::JournalRepaired | ConcernKind::ClockIrregular => matches!(resolution, Resolution::Acknowledged),
        }
    }
}

/// One raised concern, named exactly: its kind, its attempt when it has one, and the event that raised it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ConcernRef {
    kind: ConcernKind,
    attempt: Option<AttemptNo>,
    raised_at: Seq,
}

impl ConcernRef {
    pub(crate) fn new(kind: ConcernKind, attempt: Option<AttemptNo>, raised_at: Seq) -> Result<ConcernRef, ModelError> {
        if kind.about_attempt() != attempt.is_some() {
            return Err(ModelError::InvalidConcernRef);
        }
        Ok(ConcernRef { kind, attempt, raised_at })
    }

    pub(crate) const fn kind(&self) -> ConcernKind {
        self.kind
    }

    pub(crate) const fn attempt(&self) -> Option<AttemptNo> {
        self.attempt
    }

    pub(crate) const fn raised_at(&self) -> Seq {
        self.raised_at
    }
}

/// What was shown when a concern was resolved: a digest over persisted records only, computed outside the core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolutionProvenance {
    pub(crate) actor: Actor,
    pub(crate) channel: Channel,
    pub(crate) presented_sha256: Sha256,
}

/// A resolution of one concern, with a resolution its kind allows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConcernResolution {
    concern: ConcernRef,
    resolution: Resolution,
    provenance: ResolutionProvenance,
}

impl ConcernResolution {
    pub(crate) fn new(concern: ConcernRef, resolution: Resolution, provenance: ResolutionProvenance) -> Result<ConcernResolution, ModelError> {
        if !concern.kind.allows(resolution) {
            return Err(ModelError::ResolutionNotAllowed);
        }
        Ok(ConcernResolution { concern, resolution, provenance })
    }

    pub(crate) const fn concern(&self) -> ConcernRef {
        self.concern
    }

    pub(crate) const fn resolution(&self) -> Resolution {
        self.resolution
    }

    pub(crate) fn provenance(&self) -> &ResolutionProvenance {
        &self.provenance
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Journal repair (the body is provisional until the storage direction, decision 12.10, is final)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RepairCause {
    IncompleteTail,
}

/// That bytes after the last valid frame were moved, unchanged, to sidecar `sidecar`: the `JournalRepaired` body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JournalRepair {
    sidecar: u32,
    removed_bytes: u32,
    removed_crc32c: u32,
    cause: RepairCause,
}

impl JournalRepair {
    pub(crate) fn new(sidecar: u32, removed_bytes: u32, removed_crc32c: u32, cause: RepairCause) -> Result<JournalRepair, ModelError> {
        if (1..=MAX_TAIL_BYTES).contains(&removed_bytes) {
            Ok(JournalRepair { sidecar, removed_bytes, removed_crc32c, cause })
        } else {
            Err(ModelError::InvalidNumber)
        }
    }

    pub(crate) const fn sidecar(&self) -> u32 {
        self.sidecar
    }

    pub(crate) const fn removed_bytes(&self) -> u32 {
        self.removed_bytes
    }

    pub(crate) const fn removed_crc32c(&self) -> u32 {
        self.removed_crc32c
    }

    pub(crate) const fn cause(&self) -> RepairCause {
        self.cause
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Events

/// Who recorded an event. On disk a label only; in process, fixed by which typed constructor built the draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Producer {
    User,
    Supervisor,
    ProviderRelay,
    Reconciler,
    Journal,
}

/// The 22 event kinds of v1 (r5 section 4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EventKind {
    RunCreated,
    AttemptCreated,
    ApprovalRequested,
    ApprovalDecided,
    ApprovalExpired,
    ApprovalWithdrawn,
    ApprovalVoided,
    ApprovalConsumed,
    StartBegun,
    AttemptStarted,
    StartFailed,
    StopRequested,
    CancelRequested,
    AttemptExited,
    AttemptInterrupted,
    AttemptNotStarted,
    ProviderSessionObserved,
    Progress,
    ProviderOutcomeClaimed,
    RunClosed,
    AttentionResolved,
    JournalRepaired,
}

impl EventKind {
    pub(crate) const ALL: [EventKind; 22] = [
        EventKind::RunCreated,
        EventKind::AttemptCreated,
        EventKind::ApprovalRequested,
        EventKind::ApprovalDecided,
        EventKind::ApprovalExpired,
        EventKind::ApprovalWithdrawn,
        EventKind::ApprovalVoided,
        EventKind::ApprovalConsumed,
        EventKind::StartBegun,
        EventKind::AttemptStarted,
        EventKind::StartFailed,
        EventKind::StopRequested,
        EventKind::CancelRequested,
        EventKind::AttemptExited,
        EventKind::AttemptInterrupted,
        EventKind::AttemptNotStarted,
        EventKind::ProviderSessionObserved,
        EventKind::Progress,
        EventKind::ProviderOutcomeClaimed,
        EventKind::RunClosed,
        EventKind::AttentionResolved,
        EventKind::JournalRepaired,
    ];

    /// Events that move an attempt (batch rule 1). Approval events never do, except the approval-driven rows before
    /// anything runs, which the reducer decides.
    pub(crate) const fn is_attempt_event(self) -> bool {
        matches!(
            self,
            EventKind::AttemptCreated
                | EventKind::StartBegun
                | EventKind::AttemptStarted
                | EventKind::StartFailed
                | EventKind::StopRequested
                | EventKind::CancelRequested
                | EventKind::AttemptExited
                | EventKind::AttemptInterrupted
                | EventKind::AttemptNotStarted
        )
    }

    /// Kinds accepted inside the reserve, so that a run can always be stopped, ended, reconciled and resolved.
    pub(crate) const fn reserved(self) -> bool {
        matches!(
            self,
            EventKind::AttemptStarted
                | EventKind::StartFailed
                | EventKind::StopRequested
                | EventKind::CancelRequested
                | EventKind::AttemptExited
                | EventKind::AttemptInterrupted
                | EventKind::AttemptNotStarted
                | EventKind::ApprovalExpired
                | EventKind::ApprovalWithdrawn
                | EventKind::ApprovalVoided
                | EventKind::ProviderOutcomeClaimed
                | EventKind::RunClosed
                | EventKind::AttentionResolved
        )
    }

    /// A repair can always be recorded: it is outside every limit and accepted in every run state.
    pub(crate) const fn unlimited(self) -> bool {
        matches!(self, EventKind::JournalRepaired)
    }
}

/// A fact as recorded in a run's journal (r5 section 4.2). Bodies hold no wall-clock time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EventBody {
    RunCreated { spec: RunSpec, spec_digest: SpecDigest },
    AttemptCreated { attempt: AttemptNo, continuation: Continuation },
    ApprovalRequested(ApprovalRequest),
    ApprovalDecided { approval: ApprovalId, decision: Decision, provenance: DecisionProvenance },
    ApprovalExpired { approval: ApprovalId, basis: ExpiryBasis },
    ApprovalWithdrawn { approval: ApprovalId, reason: WithdrawalReason },
    ApprovalVoided { approval: ApprovalId, reason: VoidReason },
    ApprovalConsumed { approval: ApprovalId, binding: Binding },
    StartBegun { attempt: AttemptNo, approval: ApprovalId, binding: Binding },
    AttemptStarted { attempt: AttemptNo, os_identity: OsIdentity },
    StartFailed { attempt: AttemptNo, code: StartFailCode },
    StopRequested { attempt: AttemptNo, reason: StopReason },
    CancelRequested { scope: CancelScope },
    AttemptExited { attempt: AttemptNo, status: ExitStatus },
    AttemptInterrupted { attempt: AttemptNo, observation: InterruptObservation },
    AttemptNotStarted { attempt: AttemptNo, reason: NotStartedCause },
    ProviderSessionObserved { attempt: AttemptNo, session: Token },
    Progress { attempt: AttemptNo, kind: ProgressKind },
    ProviderOutcomeClaimed { attempt: AttemptNo, claim: OutcomeClaim },
    RunClosed { reason: CloseReason },
    AttentionResolved(ConcernResolution),
    JournalRepaired(JournalRepair),
}

/// What an event is about, used to detect duplicates: at most one event per key in a journal.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SemanticKey {
    Run,
    Attempt(AttemptNo),
    AttemptBegin(AttemptNo),
    AttemptStart(AttemptNo),
    AttemptEnd(AttemptNo),
    AttemptStop(AttemptNo),
    AttemptSession(AttemptNo, Token),
    AttemptClaim(AttemptNo),
    Approval(ApprovalId),
    ApprovalDecision(ApprovalId),
    ApprovalEnd(ApprovalId),
    CancelAttempt(AttemptNo),
    CancelRun,
    RunClose,
    Attention(ConcernRef),
    Repair(u32),
}

impl EventBody {
    pub(crate) const fn kind(&self) -> EventKind {
        match self {
            EventBody::RunCreated { .. } => EventKind::RunCreated,
            EventBody::AttemptCreated { .. } => EventKind::AttemptCreated,
            EventBody::ApprovalRequested(_) => EventKind::ApprovalRequested,
            EventBody::ApprovalDecided { .. } => EventKind::ApprovalDecided,
            EventBody::ApprovalExpired { .. } => EventKind::ApprovalExpired,
            EventBody::ApprovalWithdrawn { .. } => EventKind::ApprovalWithdrawn,
            EventBody::ApprovalVoided { .. } => EventKind::ApprovalVoided,
            EventBody::ApprovalConsumed { .. } => EventKind::ApprovalConsumed,
            EventBody::StartBegun { .. } => EventKind::StartBegun,
            EventBody::AttemptStarted { .. } => EventKind::AttemptStarted,
            EventBody::StartFailed { .. } => EventKind::StartFailed,
            EventBody::StopRequested { .. } => EventKind::StopRequested,
            EventBody::CancelRequested { .. } => EventKind::CancelRequested,
            EventBody::AttemptExited { .. } => EventKind::AttemptExited,
            EventBody::AttemptInterrupted { .. } => EventKind::AttemptInterrupted,
            EventBody::AttemptNotStarted { .. } => EventKind::AttemptNotStarted,
            EventBody::ProviderSessionObserved { .. } => EventKind::ProviderSessionObserved,
            EventBody::Progress { .. } => EventKind::Progress,
            EventBody::ProviderOutcomeClaimed { .. } => EventKind::ProviderOutcomeClaimed,
            EventBody::RunClosed { .. } => EventKind::RunClosed,
            EventBody::AttentionResolved(_) => EventKind::AttentionResolved,
            EventBody::JournalRepaired(_) => EventKind::JournalRepaired,
        }
    }

    /// Whether `producer` may draft this event (the closed allow-list of r5 section 3.1). A start request comes from
    /// the supervisor and an operation request from the provider relay. Approval consequences that the reducer
    /// generates are not drafts and are not judged here. No provider-relayed event can decide, consume, end or
    /// resolve anything.
    pub(crate) const fn drafted_by(&self, producer: Producer) -> bool {
        match self {
            EventBody::ApprovalRequested(request) => match request.fields.kind {
                ApprovalKind::Start => matches!(producer, Producer::Supervisor),
                ApprovalKind::Operation => matches!(producer, Producer::ProviderRelay),
            },
            _ => match self.kind() {
                EventKind::RunCreated
                | EventKind::AttemptCreated
                | EventKind::ApprovalDecided
                | EventKind::CancelRequested
                | EventKind::RunClosed
                | EventKind::AttentionResolved => matches!(producer, Producer::User),
                EventKind::StartBegun
                | EventKind::AttemptStarted
                | EventKind::StartFailed
                | EventKind::StopRequested
                | EventKind::AttemptExited
                | EventKind::ApprovalConsumed
                | EventKind::ApprovalExpired => matches!(producer, Producer::Supervisor),
                EventKind::ApprovalWithdrawn | EventKind::ApprovalVoided => {
                    matches!(producer, Producer::Supervisor | Producer::Reconciler)
                }
                EventKind::AttemptInterrupted | EventKind::AttemptNotStarted => matches!(producer, Producer::Reconciler),
                EventKind::ProviderSessionObserved | EventKind::Progress | EventKind::ProviderOutcomeClaimed => {
                    matches!(producer, Producer::ProviderRelay)
                }
                EventKind::JournalRepaired => matches!(producer, Producer::Journal),
                // Handled above.
                EventKind::ApprovalRequested => false,
            },
        }
    }

    /// The semantic keys this event occupies. `StartBegun` occupies two (it begins the attempt and consumes the start
    /// grant); `Progress` occupies none and is coalesced instead.
    pub(crate) fn semantic_keys(&self) -> Vec<SemanticKey> {
        let one = |key| Vec::from([key]);
        match self {
            EventBody::RunCreated { .. } => one(SemanticKey::Run),
            EventBody::AttemptCreated { attempt, .. } => one(SemanticKey::Attempt(*attempt)),
            EventBody::ApprovalRequested(request) => one(SemanticKey::Approval(request.fields.approval)),
            EventBody::ApprovalDecided { approval, .. }
            | EventBody::ApprovalExpired { approval, .. }
            | EventBody::ApprovalWithdrawn { approval, .. } => one(SemanticKey::ApprovalDecision(*approval)),
            EventBody::ApprovalVoided { approval, .. } | EventBody::ApprovalConsumed { approval, .. } => {
                one(SemanticKey::ApprovalEnd(*approval))
            }
            EventBody::StartBegun { attempt, approval, .. } => {
                Vec::from([SemanticKey::AttemptBegin(*attempt), SemanticKey::ApprovalEnd(*approval)])
            }
            EventBody::AttemptStarted { attempt, .. } => one(SemanticKey::AttemptStart(*attempt)),
            EventBody::StartFailed { attempt, .. }
            | EventBody::AttemptExited { attempt, .. }
            | EventBody::AttemptInterrupted { attempt, .. }
            | EventBody::AttemptNotStarted { attempt, .. } => one(SemanticKey::AttemptEnd(*attempt)),
            EventBody::StopRequested { attempt, .. } => one(SemanticKey::AttemptStop(*attempt)),
            EventBody::CancelRequested { scope: CancelScope::Attempt(attempt) } => one(SemanticKey::CancelAttempt(*attempt)),
            EventBody::CancelRequested { scope: CancelScope::Run } => one(SemanticKey::CancelRun),
            EventBody::ProviderSessionObserved { attempt, session } => {
                one(SemanticKey::AttemptSession(*attempt, session.clone()))
            }
            EventBody::Progress { .. } => Vec::new(),
            EventBody::ProviderOutcomeClaimed { attempt, .. } => one(SemanticKey::AttemptClaim(*attempt)),
            EventBody::RunClosed { .. } => one(SemanticKey::RunClose),
            EventBody::AttentionResolved(resolved) => one(SemanticKey::Attention(resolved.concern)),
            EventBody::JournalRepaired(record) => one(SemanticKey::Repair(record.sidecar)),
        }
    }
}

// Drafts by producer (r5 section 3.1, closed by D3.2b-2). A draft's producer is fixed by which of these types its body
// came from; no caller can name a producer, and each type can express only the kinds its producer may record. The
// provider relay in particular can request an operation approval, report a session or progress, and claim failure:
// it has no way to express a decision, a consumption, a lifecycle change, a resolution or a process exit.

/// What the user records, through the future native confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UserDraft {
    RunCreated { spec: RunSpec, spec_digest: SpecDigest },
    AttemptCreated { attempt: AttemptNo, continuation: Continuation },
    ApprovalDecided { approval: ApprovalId, decision: Decision, provenance: DecisionProvenance },
    CancelRequested { scope: CancelScope },
    RunClosed { reason: CloseReason },
    AttentionResolved(ConcernResolution),
}

/// What the future single spawn boundary records about its own child and its own approval requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SupervisorDraft {
    /// The start consent request of a new attempt; its kind must be `start`.
    StartRequested { request: ApprovalRequest, description: OperationDescription },
    StartBegun { attempt: AttemptNo, approval: ApprovalId, binding: Binding },
    AttemptStarted { attempt: AttemptNo, os_identity: OsIdentity },
    StartFailed { attempt: AttemptNo, code: StartFailCode },
    StopRequested { attempt: AttemptNo, reason: StopReason },
    AttemptExited { attempt: AttemptNo, status: ExitStatus },
    ApprovalConsumed { approval: ApprovalId, binding: Binding },
    ApprovalExpired { approval: ApprovalId, basis: ExpiryBasis },
    ApprovalWithdrawn { approval: ApprovalId, reason: WithdrawalReason },
    ApprovalVoided { approval: ApprovalId, reason: VoidReason },
}

/// What the supervisor forwards from normalized provider output. Untrusted: it can add a pending request, add
/// metadata, show activity or worsen an outcome, and nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RelayDraft {
    /// An operation approval request; its kind must be `operation`.
    OperationRequested { request: ApprovalRequest, description: OperationDescription },
    SessionObserved { attempt: AttemptNo, session: Token },
    Progress { attempt: AttemptNo, kind: ProgressKind },
    OutcomeClaimed { attempt: AttemptNo, claim: OutcomeClaim },
}

/// What startup reconciliation records (its rules are D3.2b-3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReconcilerDraft {
    AttemptInterrupted { attempt: AttemptNo, observation: InterruptObservation },
    AttemptNotStarted { attempt: AttemptNo, reason: NotStartedCause },
    ApprovalWithdrawn { approval: ApprovalId, reason: WithdrawalReason },
    ApprovalVoided { approval: ApprovalId, reason: VoidReason },
}

/// What the future storage adapter records after moving an incomplete tail to a sidecar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum JournalDraft {
    Repaired(JournalRepair),
}

/// What a caller asks the reducer to record. `stated_at_ms` is the raw wall-clock reading: it may be invalid, in
/// which case the reducer keeps the previous time and marks the event `clamped` (only `RunCreated` is refused).
///
/// The fields are private: a draft is built only from one of the producer types above, so its producer label always
/// matches what its body may express. An approval request's description travels beside the body, in memory only,
/// and never becomes part of an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EventDraft {
    producer: Producer,
    v: u16,
    stated_at_ms: u64,
    body: EventBody,
    description: Option<OperationDescription>,
}

impl EventDraft {
    fn new(producer: Producer, stated_at_ms: u64, body: EventBody, description: Option<OperationDescription>) -> EventDraft {
        EventDraft { producer, v: RECORD_VERSION, stated_at_ms, body, description }
    }

    pub(crate) fn user(draft: UserDraft, stated_at_ms: u64) -> EventDraft {
        let body = match draft {
            UserDraft::RunCreated { spec, spec_digest } => EventBody::RunCreated { spec, spec_digest },
            UserDraft::AttemptCreated { attempt, continuation } => EventBody::AttemptCreated { attempt, continuation },
            UserDraft::ApprovalDecided { approval, decision, provenance } => EventBody::ApprovalDecided { approval, decision, provenance },
            UserDraft::CancelRequested { scope } => EventBody::CancelRequested { scope },
            UserDraft::RunClosed { reason } => EventBody::RunClosed { reason },
            UserDraft::AttentionResolved(resolution) => EventBody::AttentionResolved(resolution),
        };
        EventDraft::new(Producer::User, stated_at_ms, body, None)
    }

    /// Refuses a start request whose kind is not `start` (`WrongProducer`).
    pub(crate) fn supervisor(draft: SupervisorDraft, stated_at_ms: u64) -> Result<EventDraft, ModelError> {
        let (body, description) = match draft {
            SupervisorDraft::StartRequested { request, description } => {
                if request.fields.kind != ApprovalKind::Start {
                    return Err(ModelError::WrongProducer);
                }
                (EventBody::ApprovalRequested(request), Some(description))
            }
            SupervisorDraft::StartBegun { attempt, approval, binding } => (EventBody::StartBegun { attempt, approval, binding }, None),
            SupervisorDraft::AttemptStarted { attempt, os_identity } => (EventBody::AttemptStarted { attempt, os_identity }, None),
            SupervisorDraft::StartFailed { attempt, code } => (EventBody::StartFailed { attempt, code }, None),
            SupervisorDraft::StopRequested { attempt, reason } => (EventBody::StopRequested { attempt, reason }, None),
            SupervisorDraft::AttemptExited { attempt, status } => (EventBody::AttemptExited { attempt, status }, None),
            SupervisorDraft::ApprovalConsumed { approval, binding } => (EventBody::ApprovalConsumed { approval, binding }, None),
            SupervisorDraft::ApprovalExpired { approval, basis } => (EventBody::ApprovalExpired { approval, basis }, None),
            SupervisorDraft::ApprovalWithdrawn { approval, reason } => (EventBody::ApprovalWithdrawn { approval, reason }, None),
            SupervisorDraft::ApprovalVoided { approval, reason } => (EventBody::ApprovalVoided { approval, reason }, None),
        };
        Ok(EventDraft::new(Producer::Supervisor, stated_at_ms, body, description))
    }

    /// Refuses an operation request whose kind is not `operation` (`WrongProducer`): the relay can never ask for
    /// start consent.
    pub(crate) fn relay(draft: RelayDraft, stated_at_ms: u64) -> Result<EventDraft, ModelError> {
        let (body, description) = match draft {
            RelayDraft::OperationRequested { request, description } => {
                if request.fields.kind != ApprovalKind::Operation {
                    return Err(ModelError::WrongProducer);
                }
                (EventBody::ApprovalRequested(request), Some(description))
            }
            RelayDraft::SessionObserved { attempt, session } => (EventBody::ProviderSessionObserved { attempt, session }, None),
            RelayDraft::Progress { attempt, kind } => (EventBody::Progress { attempt, kind }, None),
            RelayDraft::OutcomeClaimed { attempt, claim } => (EventBody::ProviderOutcomeClaimed { attempt, claim }, None),
        };
        Ok(EventDraft::new(Producer::ProviderRelay, stated_at_ms, body, description))
    }

    pub(crate) fn reconciler(draft: ReconcilerDraft, stated_at_ms: u64) -> EventDraft {
        let body = match draft {
            ReconcilerDraft::AttemptInterrupted { attempt, observation } => EventBody::AttemptInterrupted { attempt, observation },
            ReconcilerDraft::AttemptNotStarted { attempt, reason } => EventBody::AttemptNotStarted { attempt, reason },
            ReconcilerDraft::ApprovalWithdrawn { approval, reason } => EventBody::ApprovalWithdrawn { approval, reason },
            ReconcilerDraft::ApprovalVoided { approval, reason } => EventBody::ApprovalVoided { approval, reason },
        };
        EventDraft::new(Producer::Reconciler, stated_at_ms, body, None)
    }

    pub(crate) fn journal(draft: JournalDraft, stated_at_ms: u64) -> EventDraft {
        let JournalDraft::Repaired(repair) = draft;
        EventDraft::new(Producer::Journal, stated_at_ms, EventBody::JournalRepaired(repair), None)
    }

    /// Test builds only: a draft with any producer label, to prove that the reducer refuses a label its body does not
    /// allow even if a draft were built around the typed constructors.
    #[cfg(test)]
    pub(crate) fn forged(producer: Producer, stated_at_ms: u64, body: EventBody) -> EventDraft {
        EventDraft::new(producer, stated_at_ms, body, None)
    }

    pub(crate) const fn producer(&self) -> Producer {
        self.producer
    }

    pub(crate) const fn v(&self) -> u16 {
        self.v
    }

    pub(crate) const fn stated_at_ms(&self) -> u64 {
        self.stated_at_ms
    }

    pub(crate) fn body(&self) -> &EventBody {
        &self.body
    }

    pub(crate) fn description(&self) -> Option<&OperationDescription> {
        self.description.as_ref()
    }
}

/// One recorded event. `v`, `seq`, `at_ms` and `clamped` say when and where a fact was recorded; equality of content
/// for duplicate detection is over `producer` and `body` only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Event {
    pub(crate) v: u16,
    pub(crate) run: RunId,
    pub(crate) seq: Seq,
    pub(crate) at_ms: WallMs,
    pub(crate) clamped: bool,
    pub(crate) producer: Producer,
    pub(crate) body: EventBody,
}

impl Event {
    pub(crate) fn same_content(&self, other: &Event) -> bool {
        self.producer == other.producer && self.body == other.body
    }
}

/// The current size of a run's journal, supplied by the caller (encoded sizes are known only to the codec).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JournalSize {
    pub(crate) events: u32,
    pub(crate) bytes: u64,
}

impl JournalSize {
    /// Inside the reserve only reserved kinds are accepted.
    pub(crate) const fn in_reserve(self) -> bool {
        self.events >= MAX_EVENTS - RESERVED_EVENTS || self.bytes >= MAX_JOURNAL_BYTES - RESERVED_JOURNAL_BYTES
    }
}

// ---------------------------------------------------------------------------------------------------------------
// States and outcomes

/// What an ended attempt says about execution. Only `ObservedFailure` may be presented as a failure; `Unknown` and
/// `NeverStarted` never are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutcomeClass {
    ObservedSuccess,
    ObservedFailure,
    ObservedStop,
    Unknown,
    NeverStarted,
}

impl OutcomeClass {
    /// Attempts that may have executed count against the run's started-attempt budget.
    pub(crate) const fn may_have_run(self) -> bool {
        !matches!(self, OutcomeClass::NeverStarted)
    }

    /// After these outcomes the workspace may have been changed and is not known.
    pub(crate) const fn leaves_workspace_unknown(self) -> bool {
        matches!(self, OutcomeClass::ObservedFailure | OutcomeClass::ObservedStop | OutcomeClass::Unknown)
    }
}

/// Why an attempt is stopping; the first stop reason decides its outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopCause {
    Cancel,
    Deadline(DeadlineKind),
    JournalFull,
    ClockFault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FailureKind {
    Exit,
    ProviderReported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoppedKind {
    JournalFull,
    ClockFault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotStartedReason {
    Denied,
    Expired,
    Withdrawn,
    Voided,
    Cancelled,
    SupervisionLost,
    StartFailed(StartFailCode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttemptState {
    Proposed,
    Approved,
    Starting,
    Running,
    Stopping(StopCause),
    Succeeded,
    Failed(FailureKind),
    Cancelled,
    TimedOut(DeadlineKind),
    Stopped(StoppedKind),
    Interrupted(InterruptObservation),
    NotStarted(NotStartedReason),
}

impl AttemptState {
    /// The outcome class of a terminal state, `None` while the attempt is not terminal.
    pub(crate) const fn outcome(self) -> Option<OutcomeClass> {
        match self {
            AttemptState::Proposed | AttemptState::Approved | AttemptState::Starting | AttemptState::Running | AttemptState::Stopping(_) => None,
            AttemptState::Succeeded => Some(OutcomeClass::ObservedSuccess),
            AttemptState::Failed(_) => Some(OutcomeClass::ObservedFailure),
            AttemptState::Cancelled | AttemptState::TimedOut(_) | AttemptState::Stopped(_) => Some(OutcomeClass::ObservedStop),
            AttemptState::Interrupted(_) => Some(OutcomeClass::Unknown),
            AttemptState::NotStarted(_) => Some(OutcomeClass::NeverStarted),
        }
    }

    pub(crate) const fn is_terminal(self) -> bool {
        self.outcome().is_some()
    }
}

/// The outcome class of the last attempt that may have executed, or `NeverStarted` if none did. Never a success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExhaustedLast {
    ObservedFailure,
    ObservedStop,
    Unknown,
    NeverStarted,
}

impl ExhaustedLast {
    pub(crate) const fn of(class: OutcomeClass) -> Option<ExhaustedLast> {
        match class {
            OutcomeClass::ObservedSuccess => None,
            OutcomeClass::ObservedFailure => Some(ExhaustedLast::ObservedFailure),
            OutcomeClass::ObservedStop => Some(ExhaustedLast::ObservedStop),
            OutcomeClass::Unknown => Some(ExhaustedLast::Unknown),
            OutcomeClass::NeverStarted => Some(ExhaustedLast::NeverStarted),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunState {
    Open,
    /// The user cancelled the run while an attempt was not yet terminal.
    Closing,
    Succeeded,
    /// No further attempt is possible. Only `Exhausted(ObservedFailure)` may be presented as failed.
    Exhausted(ExhaustedLast),
    /// Records the user's intent; it does not assert that any process stopped.
    Cancelled,
    /// Closed by the user; claims nothing about any outcome.
    ClosedByUser,
}

impl RunState {
    pub(crate) const fn is_terminal(self) -> bool {
        !matches!(self, RunState::Open | RunState::Closing)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ApprovalState {
    Pending,
    Granted,
    Denied,
    Expired,
    Withdrawn,
    Consumed,
    Voided,
}

impl ApprovalState {
    pub(crate) const fn is_terminal(self) -> bool {
        !matches!(self, ApprovalState::Pending | ApprovalState::Granted)
    }

    /// Open approvals count against `MAX_OPEN_APPROVALS`.
    pub(crate) const fn is_open(self) -> bool {
        !self.is_terminal()
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Closed result codes

/// Why the reducer refuses a draft or batch. Nothing is appended for a refused batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rejection {
    ProducerNotAllowed,
    InvalidDraft,
    /// A draft withdraws or voids an approval that an earlier event of the same batch already ended.
    ConsequenceDrafted,
    Conflict { existing: Seq },
    PartialDuplicate,
    LimitExceeded,
    ReserveOnly,
    /// A `RunCreated` whose stated time is not a valid wall-clock value.
    InvalidTime,
    RunTerminal,
    AttemptTerminal,
    ApprovalTerminal,
    InvalidTransition,
    /// A batch whose shape the batch rules do not allow.
    BatchShape,
    RunQuarantined,
    /// A request that does not match its run, a consumer whose binding differs from its request, or a decision whose
    /// presented digest differs from the request's (r5 gate-review finding F7).
    BindingMismatch,
    /// A decision or consumption recorded at or after the end of its recorded window.
    WindowClosed,
    /// A new attempt while a blocking concern of the run is unresolved.
    BlockingConcern,
}

/// Why replay stopped. The run is then quarantined: read-only, never repaired, never reconciled, and a blocking
/// condition for every start. `Codec` details belong to the codec (b-4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JournalFault {
    Codec,
    NewerVersion,
    VersionDecreased,
    ConsequenceMismatch,
    WrongRun,
    SeqGap,
    SeqNotFirst,
    TimeDecreased,
    /// `clamped` recorded on an event whose time differs from the previous one (r5 gate-review finding F14).
    ClampedMisused,
    /// A frame mixing producers in a shape the batch rules do not allow (r5 gate-review finding F14).
    MixedProducerFrame,
    ProducerNotAllowed,
    DuplicateKey,
    CoalesceViolated,
    InvalidTransition,
    LimitExceeded,
}

/// Why a draft was accepted without being appended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoalesceReason {
    /// A `Progress` within `PROGRESS_COALESCE_MS` of the previous recorded one for the same attempt.
    ProgressWindow,
    /// A provider-relayed draft inside the reserve.
    Reserve,
}

#[cfg(test)]
mod tests;
