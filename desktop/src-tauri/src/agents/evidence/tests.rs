// Evidence vocabulary tests. Pure: synthetic records only. The only file read is runtime/runtime_validation.py's source
// text, to pin parity with the Python vocabulary; no Python runs.
use super::*;

const T0: u64 = 1_793_836_800_000;

fn hash(c: char) -> Sha256 {
    Sha256::new(&c.to_string().repeat(64)).unwrap()
}

fn run(id: &str) -> Token {
    Token::new(id).unwrap()
}

fn probe(h: char, probe_id: &str) -> Binding {
    Binding::Probe { executable_sha256: hash(h), probe_id: run(probe_id) }
}

/// The value a requirement on a valued capability names, and that a passing record for it reports.
fn value(capability: Capability) -> Option<&'static str> {
    capability.valued().then_some("v1")
}

/// A record bound to executable `a` and probe run `p1`, the configuration every evaluation below asks about.
fn bound(capability: Capability, source: Source, outcome: Outcome) -> Record {
    let observed = if outcome == Outcome::Pass { value(capability) } else { None };
    Record::new(capability, source, outcome, probe('a', "p1"), value(capability), observed, T0).unwrap()
}

fn docs(provider: ProviderId, capability: Capability, outcome: Outcome) -> Record {
    Record::new(capability, Source::VendorDocumentation, outcome, Binding::Documentation(provider), None, None, T0).unwrap()
}

fn require(capabilities: &[Capability]) -> Vec<Requirement> {
    capabilities.iter().map(|&c| Requirement::new(c, value(c)).unwrap()).collect()
}

fn judge(required: &[Requirement], records: &[Record]) -> Evaluation {
    evaluate(ProviderId::Codex, &hash('a'), &run("p1"), required, records).unwrap()
}

fn status_of(evaluation: &Evaluation, capability: Capability) -> Status {
    evaluation.capabilities.iter().find(|r| r.capability == capability).unwrap().status
}

fn one(capability: Capability, records: &[Record]) -> Status {
    status_of(&judge(&require(&[capability]), records), capability)
}

// Vocabulary and validation

#[test]
fn tokens_and_hashes_follow_the_python_patterns() {
    for ok in ["a", "gpt-5.5", "provider/model", "a:b@c+d=e_f.g", &"x".repeat(128)] {
        assert!(Token::new(ok).is_ok(), "{ok:?}");
    }
    for bad in ["", &"x".repeat(129), "two words", "line\nbreak", "é", "quote\"", "semi;colon", "back\\slash", "tab\t"] {
        assert_eq!(Token::new(bad), Err(EvidenceError::InvalidToken), "{bad:?}");
    }
    assert!(Sha256::new(&"0123456789abcdef".repeat(4)).is_ok());
    for bad in ["", &"a".repeat(63), &"a".repeat(65), &"A".repeat(64), &"g".repeat(64), &format!("{} ", "a".repeat(63))] {
        assert_eq!(Sha256::new(bad), Err(EvidenceError::InvalidSha256), "{bad:?}");
    }
}

#[test]
fn records_follow_the_closed_schema() {
    let make = |capability, source, outcome, binding, requested, observed, at| Record::new(capability, source, outcome, binding, requested, observed, at);
    use Capability::*;
    use Outcome::*;
    // Values only on valued capabilities; observations only from the closed vocabulary and never on a pass.
    assert_eq!(make(OutputFormat, Source::RuntimeProbe, Pass, probe('a', "p1"), Some("x"), None, T0), Err(EvidenceError::InvalidValues));
    assert_eq!(make(OutputFormat, Source::RuntimeProbe, Fail, probe('a', "p1"), None, Some("weird"), T0), Err(EvidenceError::InvalidValues));
    assert_eq!(make(OutputFormat, Source::RuntimeProbe, Pass, probe('a', "p1"), None, Some("timeout"), T0), Err(EvidenceError::InvalidValues));
    for observation in OBSERVATIONS {
        assert!(make(ProcessTermination, Source::RuntimeProbe, Fail, probe('a', "p1"), None, Some(observation), T0).is_ok());
    }
    assert!(make(ModelAcceptance, Source::RuntimeProbe, Pass, probe('a', "p1"), Some("m1"), Some("m1"), T0).is_ok());
    assert_eq!(make(ModelAcceptance, Source::RuntimeProbe, Pass, probe('a', "p1"), Some("two words"), None, T0), Err(EvidenceError::InvalidToken));
    // Documentation is bound to a provider; every other source to one probe run.
    assert_eq!(make(OutputFormat, Source::VendorDocumentation, Pass, probe('a', "p1"), None, None, T0), Err(EvidenceError::SourceBindingMismatch));
    for source in [Source::AdapterDeclaration, Source::CliHelp, Source::RuntimeProbe, Source::ExternalCheck] {
        assert_eq!(make(OutputFormat, source, Pass, Binding::Documentation(ProviderId::Codex), None, None, T0), Err(EvidenceError::SourceBindingMismatch));
    }
    // Documentation only describes: a pass with no values, never a failure, an unsupported outcome or an observation.
    let documentation = || Binding::Documentation(ProviderId::Claude);
    assert!(make(OutputFormat, Source::VendorDocumentation, Pass, documentation(), None, None, T0).is_ok());
    for outcome in [Fail, Unsupported] {
        assert_eq!(make(OutputFormat, Source::VendorDocumentation, outcome, documentation(), None, None, T0), Err(EvidenceError::InvalidDocumentationRecord));
    }
    assert_eq!(make(ModelAcceptance, Source::VendorDocumentation, Pass, documentation(), Some("m1"), None, T0), Err(EvidenceError::InvalidDocumentationRecord));
    assert_eq!(make(ModelAcceptance, Source::VendorDocumentation, Pass, documentation(), None, Some("m1"), T0), Err(EvidenceError::InvalidDocumentationRecord));
    // Timestamps are explicit and bounded; provenance is kept as given.
    assert_eq!(make(OutputFormat, Source::RuntimeProbe, Pass, probe('a', "p1"), None, None, MAX_TIMESTAMP_MS + 1), Err(EvidenceError::InvalidTimestamp));
    let record = make(OutputFormat, Source::CliHelp, Pass, probe('a', "p1"), None, None, MAX_TIMESTAMP_MS).unwrap();
    assert_eq!((record.capability(), record.source(), record.outcome(), record.recorded_at_ms()), (OutputFormat, Source::CliHelp, Pass, MAX_TIMESTAMP_MS));
    assert_eq!(record.binding(), &probe('a', "p1"));
}

#[test]
fn requirements_and_record_counts_are_bounded() {
    assert_eq!(Requirement::new(Capability::OutputFormat, Some("json")), Err(EvidenceError::InvalidRequirement));
    // As in Python, a valued capability is only ever required for a named value (model_acceptance for the model).
    for capability in [Capability::ModelAcceptance, Capability::ReasoningEffort] {
        assert_eq!(Requirement::new(capability, None), Err(EvidenceError::InvalidRequirement));
        assert!(Requirement::new(capability, Some("v1")).is_ok());
    }
    let twice = require(&[Capability::OutputFormat, Capability::OutputFormat]);
    assert_eq!(evaluate(ProviderId::Codex, &hash('a'), &run("p1"), &twice, &[]), Err(EvidenceError::InvalidRequirement));
    let records = vec![bound(Capability::OutputFormat, Source::CliHelp, Outcome::Pass); MAX_EVIDENCE_RECORDS];
    assert!(evaluate(ProviderId::Codex, &hash('a'), &run("p1"), &[], &records).is_ok());
    let records = vec![bound(Capability::OutputFormat, Source::CliHelp, Outcome::Pass); MAX_EVIDENCE_RECORDS + 1];
    assert_eq!(evaluate(ProviderId::Codex, &hash('a'), &run("p1"), &[], &records), Err(EvidenceError::TooManyRecords));
}

// Ceilings: each source has one level, and only the levels at or above a capability's minimum can satisfy it.

#[test]
fn each_source_has_one_ceiling_and_documentation_is_the_weakest() {
    let levels: Vec<Level> = Source::ALL.iter().map(|s| s.level()).collect();
    assert_eq!(levels, Level::ALL);
    assert!(Level::ALL.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(Source::VendorDocumentation.level(), Level::VendorDocumented);
    let classes: Vec<Class> = Level::ALL.iter().map(|l| l.class()).collect();
    assert_eq!(classes, [Class::Advertised, Class::Advertised, Class::Advertised, Class::Observed, Class::Verified]);
}

#[test]
fn a_pass_satisfies_only_at_or_above_the_capability_minimum() {
    for capability in Capability::ALL {
        let minimum = if capability.isolation() { Level::IndependentlyVerified } else { Level::RuntimeObserved };
        assert_eq!(capability.minimum_level(), minimum);
        for source in [Source::AdapterDeclaration, Source::CliHelp, Source::RuntimeProbe, Source::ExternalCheck] {
            let expected = if source.level() >= minimum { Status::Satisfied } else { Status::InsufficientEvidence };
            assert_eq!(one(capability, &[bound(capability, source, Outcome::Pass)]), expected, "{} {}", capability.name(), source.name());
        }
        assert_eq!(one(capability, &[docs(ProviderId::Codex, capability, Outcome::Pass)]), Status::InsufficientEvidence);
    }
    // An observation is not isolation: a runtime probe never satisfies an isolation capability.
    for capability in [Capability::ReadOnlyFilesystem, Capability::NetworkRestriction, Capability::SessionPersistence] {
        assert_eq!(one(capability, &[bound(capability, Source::RuntimeProbe, Outcome::Pass)]), Status::InsufficientEvidence);
    }
}

#[test]
fn documentation_is_never_promoted_however_much_of_it_there_is() {
    for capability in Capability::ALL {
        let records = vec![docs(ProviderId::Codex, capability, Outcome::Pass); MAX_EVIDENCE_RECORDS];
        let evaluation = judge(&require(&[capability]), &records);
        let result = evaluation.capabilities.iter().find(|r| r.capability == capability).unwrap();
        assert_eq!((result.status, result.highest_level), (Status::InsufficientEvidence, Some(Level::VendorDocumented)));
        assert_eq!(result.highest_level.map(Level::class), Some(Class::Advertised));
        assert_eq!(evaluation.blocking, [(capability, Status::InsufficientEvidence)]);
    }
}

#[test]
fn the_real_catalog_as_evidence_satisfies_nothing() {
    for provider in ProviderId::ALL {
        let records = documentation_records(provider, T0).unwrap();
        assert!(records.iter().all(|r| r.source() == Source::VendorDocumentation && r.binding() == &Binding::Documentation(provider)));
        let evaluation = evaluate(provider, &hash('a'), &run("p1"), &require(&Capability::ALL), &records).unwrap();
        assert_eq!(evaluation.blocking.len(), Capability::ALL.len(), "{}", provider.id());
        for result in &evaluation.capabilities {
            assert!(matches!(result.status, Status::InsufficientEvidence | Status::Unknown), "{} {:?}", provider.id(), result);
            assert!(result.highest_level.is_none_or(|l| l == Level::VendorDocumented));
        }
    }
}

#[test]
fn documentation_records_follow_the_catalog_statuses() {
    // Claude Code documents structured output, model selection, effort and running without persistence.
    let claude: Vec<(Capability, Outcome)> = documentation_records(ProviderId::Claude, T0).unwrap().iter().map(|r| (r.capability(), r.outcome())).collect();
    assert_eq!(claude, [
        (Capability::OutputFormat, Outcome::Pass),
        (Capability::ModelAcceptance, Outcome::Pass),
        (Capability::ReasoningEffort, Outcome::Pass),
        (Capability::SessionPersistence, Outcome::Pass),
    ]);
    // Not found, contradictory and unreviewed statements produce nothing: the capability stays unknown.
    let kimi: Vec<Capability> = documentation_records(ProviderId::Kimi, T0).unwrap().iter().map(|r| r.capability()).collect();
    assert_eq!(kimi, [Capability::OutputFormat]);
    let evaluation = evaluate(ProviderId::Kimi, &hash('a'), &run("p1"), &require(&[Capability::ReasoningEffort]), &documentation_records(ProviderId::Kimi, T0).unwrap()).unwrap();
    assert_eq!(status_of(&evaluation, Capability::ReasoningEffort), Status::Unknown);
    assert_eq!(documentation_records(ProviderId::Codex, MAX_TIMESTAMP_MS + 1), Err(EvidenceError::InvalidTimestamp));
}

// Conflicts and failures

#[test]
fn unknown_means_no_applicable_record() {
    for capability in Capability::ALL {
        assert_eq!(one(capability, &[]), Status::Unknown);
    }
}

#[test]
fn unsupported_outranks_any_pass() {
    for source in [Source::AdapterDeclaration, Source::CliHelp, Source::RuntimeProbe, Source::ExternalCheck] {
        let records = [bound(Capability::OutputFormat, Source::ExternalCheck, Outcome::Pass), bound(Capability::OutputFormat, source, Outcome::Unsupported)];
        assert_eq!(one(Capability::OutputFormat, &records), Status::Unsupported, "{}", source.name());
    }
}

#[test]
fn conflicts_resolve_to_the_most_severe_status() {
    use Capability::ModelAcceptance as M;
    let valued = |source, outcome, requested, observed| Record::new(M, source, outcome, probe('a', "p1"), requested, observed, T0).unwrap();
    let wanted = [Requirement::new(M, Some("m1")).unwrap()];
    let status = |records: &[Record]| status_of(&judge(&wanted, records), M);
    let satisfying = valued(Source::RuntimeProbe, Outcome::Pass, Some("m1"), Some("m1"));
    let mismatching = valued(Source::RuntimeProbe, Outcome::Pass, Some("m1"), Some("m2"));
    let unsupported = valued(Source::CliHelp, Outcome::Unsupported, None, None);
    let failing = valued(Source::AdapterDeclaration, Outcome::Fail, None, None);
    assert_eq!(status(&[satisfying.clone()]), Status::Satisfied);
    assert_eq!(status(&[satisfying.clone(), unsupported.clone()]), Status::Unsupported);
    assert_eq!(status(&[satisfying.clone(), unsupported.clone(), mismatching.clone()]), Status::Mismatch);
    assert_eq!(status(&[satisfying.clone(), unsupported, mismatching, failing]), Status::Failed);
    // Records about another requested value do not apply; records with no requested value always do.
    assert_eq!(status(&[valued(Source::RuntimeProbe, Outcome::Fail, Some("m2"), None)]), Status::Unknown);
    assert_eq!(status(&[valued(Source::RuntimeProbe, Outcome::Pass, None, None)]), Status::InsufficientEvidence);
    assert_eq!(status(&[valued(Source::RuntimeProbe, Outcome::Pass, Some("m1"), None)]), Status::InsufficientEvidence);
    // A pass with no requested value that observed another value is a mismatch, as in Python.
    assert_eq!(status(&[valued(Source::RuntimeProbe, Outcome::Pass, None, Some("m2"))]), Status::Mismatch);
    let order: Vec<&str> = Status::ALL.iter().map(|s| s.name()).collect();
    assert_eq!(order, ["failed", "mismatch", "unsupported", "satisfied", "insufficient_evidence", "unknown"]);
    assert!(Status::ALL.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn a_failure_persists_whatever_comes_later() {
    let fail = Record::new(Capability::ProcessTermination, Source::CliHelp, Outcome::Fail, probe('a', "p1"), None, Some("descendant_survived"), T0).unwrap();
    let later = |at| Record::new(Capability::ProcessTermination, Source::ExternalCheck, Outcome::Pass, probe('a', "p1"), None, None, at).unwrap();
    let records = [fail.clone(), later(T0 + 1), later(MAX_TIMESTAMP_MS)];
    assert_eq!(one(Capability::ProcessTermination, &records), Status::Failed);
    let reversed = [later(MAX_TIMESTAMP_MS), later(T0 + 1), fail];
    assert_eq!(one(Capability::ProcessTermination, &reversed), Status::Failed);
    // The weakest source's failure still blocks the strongest source's pass.
    let weakest = Record::new(Capability::ProcessTermination, Source::AdapterDeclaration, Outcome::Fail, probe('a', "p1"), None, None, T0).unwrap();
    assert_eq!(one(Capability::ProcessTermination, &[later(T0), weakest]), Status::Failed);
}

// Identity and binding

#[test]
fn evidence_for_another_executable_is_stale_and_ignored_both_ways() {
    let old = |outcome| Record::new(Capability::OutputFormat, Source::ExternalCheck, outcome, probe('b', "p1"), None, None, T0).unwrap();
    // A pass for the old file does not carry over to the new one.
    let evaluation = judge(&require(&[Capability::OutputFormat]), &[old(Outcome::Pass)]);
    assert_eq!((status_of(&evaluation, Capability::OutputFormat), evaluation.stale_evidence), (Status::Unknown, 1));
    // A failure recorded for the old file poisons only that file; the new file is a distinct configuration.
    let evaluation = judge(&require(&[Capability::OutputFormat]), &[old(Outcome::Fail), bound(Capability::OutputFormat, Source::RuntimeProbe, Outcome::Pass)]);
    assert_eq!((status_of(&evaluation, Capability::OutputFormat), evaluation.stale_evidence), (Status::Satisfied, 1));
}

#[test]
fn evidence_for_another_executable_and_run_counts_as_stale() {
    // The hash is checked first, as in Python: a record that differs in both is stale, not unbound.
    let other = Record::new(Capability::OutputFormat, Source::ExternalCheck, Outcome::Pass, probe('b', "p2"), None, None, T0).unwrap();
    let evaluation = judge(&require(&[Capability::OutputFormat]), &[other]);
    assert_eq!((evaluation.stale_evidence, evaluation.unbound_evidence), (1, 0));
}

#[test]
fn the_result_does_not_depend_on_record_order() {
    let mut records = vec![
        bound(Capability::OutputFormat, Source::RuntimeProbe, Outcome::Pass),
        bound(Capability::ModelAcceptance, Source::ExternalCheck, Outcome::Pass),
        bound(Capability::ModelAcceptance, Source::CliHelp, Outcome::Unsupported),
        bound(Capability::ReadOnlyFilesystem, Source::RuntimeProbe, Outcome::Pass),
        Record::new(Capability::ProcessTermination, Source::RuntimeProbe, Outcome::Fail, probe('a', "p1"), None, Some("timeout"), T0).unwrap(),
        Record::new(Capability::OutputFormat, Source::ExternalCheck, Outcome::Pass, probe('b', "p1"), None, None, T0).unwrap(),
        Record::new(Capability::OutputFormat, Source::ExternalCheck, Outcome::Fail, probe('a', "p2"), None, None, T0).unwrap(),
        docs(ProviderId::Codex, Capability::ReasoningEffort, Outcome::Pass),
        docs(ProviderId::Kimi, Capability::OutputFormat, Outcome::Pass),
    ];
    let required = require(&Capability::ALL);
    let forward = judge(&required, &records);
    records.reverse();
    assert_eq!(judge(&required, &records), forward);
    records.rotate_left(4);
    assert_eq!(judge(&required, &records), forward);
}

#[test]
fn evidence_from_another_probe_run_is_unbound_and_ignored() {
    let other_run = Record::new(Capability::OutputFormat, Source::ExternalCheck, Outcome::Pass, probe('a', "p2"), None, None, T0).unwrap();
    let evaluation = judge(&require(&[Capability::OutputFormat]), &[other_run]);
    assert_eq!((status_of(&evaluation, Capability::OutputFormat), evaluation.stale_evidence, evaluation.unbound_evidence), (Status::Unknown, 0, 1));
}

#[test]
fn documentation_applies_to_its_provider_only_and_never_goes_stale() {
    let records = [docs(ProviderId::Claude, Capability::OutputFormat, Outcome::Pass), docs(ProviderId::Codex, Capability::OutputFormat, Outcome::Pass)];
    // Asked about Codex: Claude's documentation is ignored and counted, and Codex's applies whatever the executable.
    for executable in ['a', 'b', 'c'] {
        let evaluation = evaluate(ProviderId::Codex, &hash(executable), &run("p9"), &require(&[Capability::OutputFormat]), &records).unwrap();
        assert_eq!(status_of(&evaluation, Capability::OutputFormat), Status::InsufficientEvidence);
        assert_eq!((evaluation.other_provider_evidence, evaluation.stale_evidence, evaluation.unbound_evidence), (1, 0, 0));
    }
    let evaluation = evaluate(ProviderId::Kimi, &hash('a'), &run("p1"), &require(&[Capability::OutputFormat]), &records).unwrap();
    assert_eq!((status_of(&evaluation, Capability::OutputFormat), evaluation.other_provider_evidence), (Status::Unknown, 2));
}

#[test]
fn the_evaluation_lists_every_capability_and_blocks_on_each_unmet_requirement() {
    let records = [bound(Capability::ExecutableIdentity, Source::RuntimeProbe, Outcome::Pass), bound(Capability::OutputFormat, Source::CliHelp, Outcome::Pass)];
    let evaluation = judge(&require(&[Capability::ExecutableIdentity, Capability::OutputFormat, Capability::NetworkRestriction]), &records);
    let listed: Vec<Capability> = evaluation.capabilities.iter().map(|r| r.capability).collect();
    assert_eq!(listed, Capability::ALL);
    assert_eq!(evaluation.blocking, [(Capability::NetworkRestriction, Status::Unknown), (Capability::OutputFormat, Status::InsufficientEvidence)]);
    let identity = &evaluation.capabilities[0];
    assert_eq!((identity.required, identity.minimum_level, identity.highest_level, identity.status), (true, Level::RuntimeObserved, Some(Level::RuntimeObserved), Status::Satisfied));
    assert!(!evaluation.capabilities[1].required);
}

// Parity with runtime/runtime_validation.py, read as text.

fn python() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/runtime_validation.py")).unwrap()
}

/// The quoted strings of a Python assignment `NAME = (...)` or `NAME = {...}`.
fn python_strings(source: &str, name: &str) -> Vec<String> {
    let start = source.find(&format!("\n{name} = ")).unwrap_or_else(|| panic!("{name} not found"));
    let rest = &source[start + name.len() + 4..];
    let close = match rest.trim_start().as_bytes()[0] {
        b'(' => ')',
        b'{' => '}',
        other => panic!("{name} starts with {}", other as char),
    };
    let body = &rest[..rest.find(close).unwrap()];
    body.split('"').skip(1).step_by(2).map(str::to_owned).collect()
}

#[test]
fn the_vocabulary_matches_runtime_validation_py() {
    let py = python();
    let names = |items: Vec<&str>| items.into_iter().map(str::to_owned).collect::<Vec<_>>();
    assert_eq!(python_strings(&py, "CAPABILITIES"), names(Capability::ALL.iter().map(|c| c.name()).collect()));
    // Python's sources and levels are the Rust ones after the added vendor-documentation entry, in the same order.
    assert_eq!(python_strings(&py, "SOURCES"), names(Source::ALL[1..].iter().map(|s| s.name()).collect()));
    assert_eq!(python_strings(&py, "LEVELS"), names(Level::ALL[1..].iter().map(|l| l.name()).collect()));
    assert_eq!(python_strings(&py, "OUTCOMES"), names(Outcome::ALL.iter().map(|o| o.name()).collect()));
    assert_eq!(python_strings(&py, "OBSERVATIONS"), names(OBSERVATIONS.to_vec()));
    let mut valued = python_strings(&py, "_VALUED");
    valued.sort();
    assert_eq!(valued, names(Capability::ALL.iter().filter(|c| c.valued()).map(|c| c.name()).collect()));
    let mut isolation = python_strings(&py, "_ISOLATION");
    isolation.sort();
    let mut rust_isolation = names(Capability::ALL.iter().filter(|c| c.isolation()).map(|c| c.name()).collect());
    rust_isolation.sort();
    assert_eq!(isolation, rust_isolation);
    // Minimum levels are LEVELS[3] for isolation and LEVELS[2] otherwise; LEVELS[2] and [3] are the Rust minimums.
    assert!(py.contains("\n_MINIMUM_LEVEL = {name: 3 if name in _ISOLATION else 2 for name in CAPABILITIES}\n"));
    assert_eq!(python_strings(&py, "LEVELS")[2], Level::RuntimeObserved.name());
    assert_eq!(python_strings(&py, "LEVELS")[3], Level::IndependentlyVerified.name());
    assert!(py.contains("\nMAX_EVIDENCE_RECORDS = 64\n") && MAX_EVIDENCE_RECORDS == 64);
    assert!(py.contains("\n_TOKEN = re.compile(r\"[A-Za-z0-9._:/@+=-]{1,128}\")\n"));
    assert!(py.contains("\n_SHA256 = re.compile(r\"[0-9a-f]{64}\")\n"));
}

#[test]
fn the_status_order_matches_python_capability_status() {
    let py = python();
    let start = py.find("\ndef _capability_status(").unwrap();
    let end = start + py[start..].find("\ndef evaluate_validation(").unwrap();
    let body = &py[start..end];
    let statuses: Vec<&str> = Status::ALL.iter().map(|s| s.name()).collect();
    let mut found: Vec<&str> = Vec::new();
    for quoted in body.split('"').skip(1).step_by(2) {
        if statuses.contains(&quoted) && !found.contains(&quoted) {
            found.push(quoted);
        }
    }
    assert_eq!(found, statuses);
    // The checks run in that order, each on the records relevant to the requested value.
    for line in [
        "relevant = [record for record in records\n                if wanted is None or record[\"requested\"] in (None, wanted)]",
        "if any(record[\"outcome\"] == \"fail\" for record in relevant):",
        "if wanted is not None and any(record[\"observed\"] not in (None, wanted) for record in passed):",
        "if any(record[\"outcome\"] == \"unsupported\" for record in relevant):",
        "and (wanted is None or (record[\"requested\"] == wanted and record[\"observed\"] == wanted))",
        "return (\"insufficient_evidence\" if passed else \"unknown\"), highest_level",
    ] {
        assert!(body.contains(line), "{line}");
    }
    // Records for another executable or probe run are counted and ignored there as here.
    assert!(py.contains("current = [record for record in records if record[\"executable_sha256\"] == current_hash]"));
    assert!(py.contains("bound = [record for record in current if record[\"probe_id\"] == run_id]"));
}
