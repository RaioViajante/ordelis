// Registry, storage protocol and folder identity tests. Everything happens in private directories under
// std::env::temp_dir(); the real app data directory and real projects are never touched. Numbers in test
// names' comments refer to the acceptance criteria in docs/implementation/desktop-local-projects.md.
use super::*;
use crate::projects::ProjectError;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Command, Stdio};

// Failure injection: one-shot faults armed on the calling thread only.

thread_local! {
    static FAULTS: RefCell<HashMap<Step, i32>> = RefCell::new(HashMap::new());
}

fn arm(step: Step, errno: i32) {
    FAULTS.with(|faults| faults.borrow_mut().insert(step, errno));
}

pub(super) fn fault(step: Step) -> std::io::Result<()> {
    match FAULTS.with(|faults| faults.borrow_mut().remove(&step)) {
        Some(errno) => Err(std::io::Error::from_raw_os_error(errno)),
        None => Ok(()),
    }
}

// Fixtures

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("synapse-projects-{}-{tag}", std::process::id()));
        if dir.exists() {
            let _ = Command::new("/bin/chmod").args(["-R", "u+rwx"]).arg(&dir).status();
            let _ = std::fs::remove_dir_all(&dir);
        }
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        Scratch(dir.canonicalize().unwrap())
    }

    /// The registry directory `D`; not created until a write creates it.
    fn data(&self) -> PathBuf {
        self.0.join("data")
    }

    fn file(&self, name: &str) -> PathBuf {
        self.data().join(name)
    }

    /// A throwaway project folder, created if needed, as its canonical path.
    fn folder(&self, name: &str) -> PathBuf {
        let path = self.0.join("folders").join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_registry(&self, contents: &[u8]) {
        let _ = std::fs::DirBuilder::new().mode(0o700).create(self.data());
        std::fs::write(self.file(REGISTRY), contents).unwrap();
    }

    fn registry(&self) -> Vec<u8> {
        std::fs::read(self.file(REGISTRY)).unwrap()
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = match std::fs::read_dir(self.data()) {
            Ok(entries) => entries.map(|e| e.unwrap().file_name().into_string().unwrap()).collect(),
            Err(_) => return Vec::new(),
        };
        names.sort();
        names
    }

    fn backups(&self) -> Vec<String> {
        self.names().into_iter().filter(|n| n.starts_with(BACKUP_PREFIX)).collect()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = Command::new("/bin/chmod").args(["-R", "u+rwx"]).arg(&self.0).status();
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn added(outcome: Result<AddOutcome, ErrorCode>) -> Project {
    match outcome {
        Ok(AddOutcome::Added { project }) => project,
        other => panic!("expected added, got {other:?}"),
    }
}

fn existing(outcome: Result<AddOutcome, ErrorCode>) -> Project {
    match outcome {
        Ok(AddOutcome::AlreadyRegistered { project }) => project,
        other => panic!("expected already_registered, got {other:?}"),
    }
}

fn listed(dir: &Path) -> Vec<Project> {
    list(dir).unwrap().projects
}

fn ids(dir: &Path) -> Vec<String> {
    listed(dir).into_iter().map(|p| p.id).collect()
}

fn stored(scratch: &Scratch) -> Stored {
    serde_json::from_slice(&scratch.registry()).unwrap()
}

fn record(id: &str, path: &str, dev: u64, ino: u64) -> serde_json::Value {
    serde_json::json!({ "id": id, "path": path, "dev": dev, "ino": ino, "registered_at_ms": 1_791_936_000_000u64 })
}

fn registry_json(next_id: u64, records: Vec<serde_json::Value>) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "schema_version": 1, "next_id": next_id, "projects": records })).unwrap()
}

fn identity(path: &Path) -> FileId {
    FileId::of(&std::fs::symlink_metadata(path).unwrap())
}

fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path).unwrap().mode() & 0o7777
}

fn modified(path: &Path) -> std::time::SystemTime {
    std::fs::symlink_metadata(path).unwrap().modified().unwrap()
}

fn mkfifo(path: &Path) {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: c is a valid NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
}

fn is_root() -> bool {
    euid() == 0
}

/// A recursive snapshot of names, types, modes, sizes, modification times and file contents.
fn snapshot(path: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            let contents = if meta.is_file() { std::fs::read(&path).unwrap() } else { Vec::new() };
            out.push(format!("{path:?} {:o} {} {:?} {contents:?}", meta.mode(), meta.len(), meta.modified().unwrap()));
            if meta.is_dir() {
                stack.push(path);
            }
        }
    }
    out.sort();
    out
}

// Listing and persistence

#[test]
fn missing_directory_or_registry_lists_empty_and_creates_nothing() {
    let scratch = Scratch::new("missing");
    assert_eq!(listed(&scratch.data()), []);
    assert!(!scratch.data().exists());
    std::fs::DirBuilder::new().mode(0o700).create(scratch.data()).unwrap();
    assert_eq!(listed(&scratch.data()), []);
    assert_eq!(scratch.names(), Vec::<String>::new());
}

#[test]
fn added_folders_persist_in_registration_order_with_stable_ids() {
    let scratch = Scratch::new("persist");
    let alpha = added(add(&scratch.data(), &scratch.folder("alpha")));
    let beta = added(add(&scratch.data(), &scratch.folder("beta")));
    assert_eq!((alpha.id.as_str(), alpha.name.as_str(), alpha.status), ("p1", "alpha", Status::Available));
    assert_eq!(beta.id, "p2");
    assert_eq!(beta.path_display, scratch.folder("beta").to_str().unwrap());
    // Every call reads the file: there is no in-memory registry to survive or lose across a restart.
    assert_eq!(listed(&scratch.data()), [alpha.clone(), beta.clone()]);
    assert_eq!(listed(&scratch.data()), listed(&scratch.data()));
    let file = stored(&scratch);
    assert_eq!(file.next_id, 3);
    assert_eq!(file.projects[0].path, scratch.folder("alpha").to_str().unwrap());
    assert_eq!(file.projects[0].file_id(), identity(&scratch.folder("alpha")));
    assert_eq!(file.projects[0].registered_at_ms, alpha.registered_at_ms);
}

#[test]
fn the_response_carries_no_device_or_inode_numbers() {
    let scratch = Scratch::new("shape");
    let project = added(add(&scratch.data(), &scratch.folder("shape")));
    let value = serde_json::to_value(&project).unwrap();
    let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["id", "name", "path_display", "registered_at_ms", "status"]);
    let list = serde_json::to_value(list(&scratch.data()).unwrap()).unwrap();
    assert_eq!(list["schema_version"], 1);
    assert_eq!(list.as_object().unwrap().len(), 2);
    let added = serde_json::to_value(AddOutcome::Added { project: project.clone() }).unwrap();
    assert_eq!(added["outcome"], "added");
    assert_eq!(serde_json::to_value(AddOutcome::Cancelled).unwrap(), serde_json::json!({ "outcome": "cancelled" }));
    assert_eq!(serde_json::to_value(RemoveOutcome::NotFound).unwrap(), serde_json::json!({ "outcome": "not_found" }));
    assert_eq!(serde_json::to_value(ResetOutcome::NotNeeded).unwrap(), serde_json::json!({ "outcome": "not_needed" }));
}

#[test]
fn created_directory_and_files_are_owner_only() {
    let scratch = Scratch::new("modes");
    added(add(&scratch.data(), &scratch.folder("a")));
    assert_eq!(mode(&scratch.data()), 0o700);
    assert_eq!(mode(&scratch.file(REGISTRY)), 0o600);
    assert_eq!(mode(&scratch.file(LOCK)), 0o600);
    assert_eq!(scratch.names(), [REGISTRY, LOCK]);
}

#[test]
fn the_parent_of_the_data_directory_is_never_created() {
    let scratch = Scratch::new("parent");
    let nested = scratch.0.join("absent").join("data");
    assert_eq!(add(&nested, &scratch.folder("a")).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert!(!scratch.0.join("absent").exists());
}

// 5. Directory checks

#[test]
fn an_untrusted_data_directory_is_unavailable_for_every_command() {
    let scratch = Scratch::new("untrusted-dir");
    let folder = scratch.folder("a");
    let real = scratch.0.join("real");
    std::fs::DirBuilder::new().mode(0o700).create(&real).unwrap();
    std::os::unix::fs::symlink(&real, scratch.data()).unwrap();
    let check = |dir: &Path| {
        assert_eq!(list(dir).unwrap_err(), ErrorCode::RegistryUnavailable);
        assert_eq!(add(dir, &folder).unwrap_err(), ErrorCode::RegistryUnavailable);
        assert_eq!(remove(dir, "p1").unwrap_err(), ErrorCode::RegistryUnavailable);
        assert_eq!(reset(dir).unwrap_err(), ErrorCode::RegistryUnavailable);
    };
    check(&scratch.data());
    assert_eq!(std::fs::read_dir(&real).unwrap().count(), 0, "nothing was written through the symlink");
    std::fs::remove_file(scratch.data()).unwrap();
    for shared in [0o770, 0o720, 0o702, 0o707] {
        std::fs::DirBuilder::new().mode(0o700).create(scratch.data()).ok();
        std::fs::set_permissions(scratch.data(), std::fs::Permissions::from_mode(shared)).unwrap();
        check(&scratch.data());
        assert_eq!(mode(&scratch.data()), shared, "existing permissions are never changed");
    }
    // Owned by root: never this test's user unless it runs as root.
    if !is_root() {
        check(Path::new("/usr"));
    }
}

// 1. Temp file

#[test]
fn a_symlink_at_the_temp_name_is_removed_and_its_target_left_untouched() {
    let scratch = Scratch::new("temp-link");
    let sentinel = scratch.0.join("sentinel");
    std::fs::write(&sentinel, b"sentinel bytes").unwrap();
    std::fs::DirBuilder::new().mode(0o700).create(scratch.data()).unwrap();
    std::os::unix::fs::symlink(&sentinel, scratch.file(TEMP)).unwrap();
    let project = added(add(&scratch.data(), &scratch.folder("a")));
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"sentinel bytes");
    assert_eq!(listed(&scratch.data()), [project]);
    assert!(!scratch.file(TEMP).exists() && std::fs::symlink_metadata(scratch.file(TEMP)).is_err());
}

#[test]
fn a_directory_at_the_temp_name_blocks_the_write_and_keeps_the_registry() {
    let scratch = Scratch::new("temp-dir");
    added(add(&scratch.data(), &scratch.folder("a")));
    let before = scratch.registry();
    std::fs::create_dir(scratch.file(TEMP)).unwrap();
    assert_eq!(add(&scratch.data(), &scratch.folder("b")).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert_eq!(remove(&scratch.data(), "p1").unwrap_err(), ErrorCode::RegistryUnavailable);
    assert_eq!(scratch.registry(), before);
    assert!(scratch.file(TEMP).is_dir());
}

#[test]
fn a_leftover_temp_file_is_never_read_or_promoted() {
    let scratch = Scratch::new("temp-stale");
    let folder = scratch.folder("ghost");
    let id = identity(&folder);
    let ghost = registry_json(2, vec![record("p1", folder.to_str().unwrap(), id.dev, id.ino)]);
    std::fs::DirBuilder::new().mode(0o700).create(scratch.data()).unwrap();
    std::fs::write(scratch.file(TEMP), &ghost).unwrap();
    assert_eq!(listed(&scratch.data()), []);
    assert!(!scratch.file(REGISTRY).exists());
    let project = added(add(&scratch.data(), &scratch.folder("real")));
    assert_eq!(project.id, "p1");
    assert_eq!(listed(&scratch.data()), [project]);
    assert!(!scratch.file(TEMP).exists());
}

// 2. Registry file type and owner

#[test]
fn a_fifo_registry_is_unavailable_without_blocking() {
    let scratch = Scratch::new("fifo");
    std::fs::DirBuilder::new().mode(0o700).create(scratch.data()).unwrap();
    mkfifo(&scratch.file(REGISTRY));
    let started = Instant::now();
    assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert_eq!(add(&scratch.data(), &scratch.folder("a")).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert_eq!(reset(&scratch.data()).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
}

#[test]
fn a_symlinked_or_directory_registry_is_unavailable() {
    let scratch = Scratch::new("registry-type");
    let folder = scratch.folder("a");
    let id = identity(&folder);
    let elsewhere = scratch.0.join("valid.json");
    std::fs::write(&elsewhere, registry_json(2, vec![record("p1", folder.to_str().unwrap(), id.dev, id.ino)])).unwrap();
    std::fs::DirBuilder::new().mode(0o700).create(scratch.data()).unwrap();
    std::os::unix::fs::symlink(&elsewhere, scratch.file(REGISTRY)).unwrap();
    assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert_eq!(add(&scratch.data(), &scratch.folder("b")).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert_eq!(reset(&scratch.data()).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert!(std::fs::symlink_metadata(scratch.file(REGISTRY)).unwrap().file_type().is_symlink());
    std::fs::remove_file(scratch.file(REGISTRY)).unwrap();
    std::fs::create_dir(scratch.file(REGISTRY)).unwrap();
    assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert_eq!(remove(&scratch.data(), "p1").unwrap_err(), ErrorCode::RegistryUnavailable);
    // A registry owned by another user needs root to set up, so that case is covered by the owner check alone.
}

#[test]
fn an_obstructed_lock_file_is_unavailable() {
    let scratch = Scratch::new("lock-type");
    std::fs::DirBuilder::new().mode(0o700).create(scratch.data()).unwrap();
    let target = scratch.0.join("target");
    std::fs::write(&target, b"").unwrap();
    std::os::unix::fs::symlink(&target, scratch.file(LOCK)).unwrap();
    assert_eq!(add(&scratch.data(), &scratch.folder("a")).unwrap_err(), ErrorCode::RegistryUnavailable);
    std::fs::remove_file(scratch.file(LOCK)).unwrap();
    std::fs::create_dir(scratch.file(LOCK)).unwrap();
    assert_eq!(add(&scratch.data(), &scratch.folder("a")).unwrap_err(), ErrorCode::RegistryUnavailable);
    std::fs::remove_dir(scratch.file(LOCK)).unwrap();
    mkfifo(&scratch.file(LOCK));
    assert_eq!(reset(&scratch.data()).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert!(!scratch.file(REGISTRY).exists());
}

// 3. Size limits

#[test]
fn a_file_over_256_kib_is_corrupt_and_one_of_exactly_256_kib_parses() {
    let scratch = Scratch::new("size");
    let folder = scratch.folder("a");
    let id = identity(&folder);
    let mut bytes = registry_json(2, vec![record("p1", folder.to_str().unwrap(), id.dev, id.ino)]);
    bytes.resize(MAX_BYTES as usize, b' ');
    scratch.write_registry(&bytes);
    assert_eq!(ids(&scratch.data()), ["p1"]);
    bytes.push(b' ');
    scratch.write_registry(&bytes);
    assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryCorrupt);
}

#[test]
fn adding_past_the_record_or_byte_limit_is_registry_full() {
    let scratch = Scratch::new("full");
    let records = (1..=MAX_RECORDS as u64).map(|n| record(&format!("p{n}"), &format!("/fill/{n}"), 1, n)).collect();
    scratch.write_registry(&registry_json(MAX_RECORDS as u64 + 1, records));
    let before = scratch.registry();
    assert_eq!(add(&scratch.data(), &scratch.folder("a")).unwrap_err(), ErrorCode::RegistryFull);
    assert_eq!(scratch.registry(), before);

    // Just under the byte limit: no record for a real folder fits in what is left.
    let long = |n: usize, len: usize| record(&format!("p{}", n + 1), &format!("/fill/{n:03}/{}", "x".repeat(len)), 1, 1);
    let size = |records: &[serde_json::Value]| {
        let stored: Stored = serde_json::from_slice(&registry_json(300, records.to_vec())).unwrap();
        serde_json::to_vec_pretty(&stored).unwrap().len()
    };
    let limit = MAX_BYTES as usize;
    let mut records: Vec<serde_json::Value> = Vec::new();
    while size(&[records.clone(), vec![long(records.len(), 950)]].concat()) <= limit {
        records.push(long(records.len(), 950));
    }
    let (n, base) = (records.len(), size(&records));
    let overhead = size(&[records.clone(), vec![long(n, 0)]].concat()) - base;
    if base + overhead + 20 <= limit {
        records.push(long(n, limit - 20 - base - overhead));
    }
    assert!(size(&records) <= limit && limit - size(&records) < overhead + 20);
    assert!(records.len() < MAX_RECORDS, "the byte limit, not the record limit, is what this hits");
    scratch.write_registry(&registry_json(300, records.clone()));
    assert_eq!(ids(&scratch.data()).len(), records.len());
    let before = scratch.registry();
    assert_eq!(add(&scratch.data(), &scratch.folder("a")).unwrap_err(), ErrorCode::RegistryFull);
    assert_eq!(scratch.registry(), before);
}

#[test]
fn the_id_counter_stops_at_its_limit() {
    let scratch = Scratch::new("counter");
    scratch.write_registry(&registry_json(MAX_NEXT_ID - 1, vec![]));
    assert_eq!(added(add(&scratch.data(), &scratch.folder("last"))).id, "p9999999999");
    assert_eq!(stored(&scratch).next_id, MAX_NEXT_ID);
    assert_eq!(ids(&scratch.data()), ["p9999999999"], "the written file is still valid");
    let before = scratch.registry();
    assert_eq!(add(&scratch.data(), &scratch.folder("one-more")).unwrap_err(), ErrorCode::RegistryFull);
    assert_eq!(scratch.registry(), before);
}

// 4. Validation

#[test]
fn every_validation_rule_rejects_its_fixture() {
    let scratch = Scratch::new("validation");
    let ok = r#"{"id":"p1","path":"/a/b","dev":1,"ino":2,"registered_at_ms":3}"#;
    let doc = |next: &str, records: &str| format!(r#"{{"schema_version":1,"next_id":{next},"projects":[{records}]}}"#);
    let rec = |id: &str, path: &str, ms: &str| format!(r#"{{"id":{id},"path":{path},"dev":1,"ino":2,"registered_at_ms":{ms}}}"#);
    let path_rec = |path: &str| rec(r#""p1""#, &serde_json::to_string(path).unwrap(), "3");
    let id_rec = |id: &str| rec(&serde_json::to_string(id).unwrap(), r#""/a""#, "3");
    let mut corrupt = vec![
        // Not a registry at all.
        String::new(),
        "{".into(),
        "[]".into(),
        "null".into(),
        format!("{} trailing", doc("2", ok)),
        // schema_version missing, zero, or not an unsigned integer.
        r#"{"next_id":1,"projects":[]}"#.into(),
        r#"{"schema_version":0,"next_id":1,"projects":[]}"#.into(),
        r#"{"schema_version":"1","next_id":1,"projects":[]}"#.into(),
        r#"{"schema_version":-1,"next_id":1,"projects":[]}"#.into(),
        r#"{"schema_version":1.0,"next_id":1,"projects":[]}"#.into(),
        r#"{"schema_version":null,"next_id":1,"projects":[]}"#.into(),
        // Unknown and duplicate fields at each level; missing fields.
        r#"{"schema_version":1,"next_id":1,"projects":[],"extra":1}"#.into(),
        r#"{"schema_version":1,"schema_version":1,"next_id":1,"projects":[]}"#.into(),
        r#"{"schema_version":1,"next_id":1,"next_id":1,"projects":[]}"#.into(),
        r#"{"schema_version":1,"next_id":1}"#.into(),
        r#"{"schema_version":1,"next_id":1,"projects":{}}"#.into(),
        doc("2", r#"{"id":"p1","path":"/a","dev":1,"ino":2,"registered_at_ms":3,"name":"x"}"#),
        doc("2", r#"{"id":"p1","id":"p1","path":"/a","dev":1,"ino":2,"registered_at_ms":3}"#),
        doc("2", r#"{"id":"p1","path":"/a","dev":1,"registered_at_ms":3}"#),
        // next_id out of range or not an integer.
        doc("0", ""),
        doc("10000000001", ""),
        doc("-1", ""),
        doc("1.5", ""),
        doc("\"2\"", ""),
        // IDs: grammar, uniqueness, below next_id.
        doc("10", &id_rec("p0")),
        doc("10", &id_rec("p01")),
        doc("10", &id_rec("q1")),
        doc("10", &id_rec("P1")),
        doc("10", &id_rec("p")),
        doc("10", &id_rec("p1 ")),
        doc("10", &id_rec("p-1")),
        doc("10", &id_rec("p٣")),
        doc("99999999999", &id_rec("p12345678901")),
        doc("2", &id_rec("p2")),
        doc("2", &id_rec("p3")),
        doc("3", &format!("{},{}", rec(r#""p1""#, r#""/a""#, "3"), rec(r#""p1""#, r#""/b""#, "3"))),
        doc("2", &rec("1", r#""/a""#, "3")),
        // Paths: relative, not normalized, root, overlong, control characters, duplicates.
        doc("2", &path_rec("a/b")),
        doc("2", &path_rec("")),
        doc("2", &path_rec("/")),
        doc("2", &path_rec("/a/./b")),
        doc("2", &path_rec("/a/../b")),
        doc("2", &path_rec("/a//b")),
        doc("2", &path_rec("/a/")),
        doc("2", &path_rec("/.")),
        doc("2", &path_rec("/..")),
        doc("2", &path_rec(&format!("/{}", "a".repeat(MAX_PATH_BYTES)))),
        doc("2", &path_rec("/a\u{7}b")),
        doc("2", &path_rec("/a\nb")),
        doc("2", &path_rec("/a\u{7f}b")),
        doc("2", &path_rec("/a\u{85}b")),
        doc("3", &format!("{},{}", rec(r#""p1""#, r#""/a""#, "3"), rec(r#""p2""#, r#""/a""#, "3"))),
        doc("2", &rec(r#""p1""#, "1", "3")),
        // dev and ino outside u64; registered_at_ms beyond JavaScript's safe integers.
        doc("2", r#"{"id":"p1","path":"/a","dev":-1,"ino":2,"registered_at_ms":3}"#),
        doc("2", r#"{"id":"p1","path":"/a","dev":1,"ino":18446744073709551616,"registered_at_ms":3}"#),
        doc("2", &rec(r#""p1""#, r#""/a""#, "9007199254740992")),
        doc("2", &rec(r#""p1""#, r#""/a""#, "-1")),
    ];
    corrupt.push(format!(r#"{{"schema_version":1,"next_id":258,"projects":[{}]}}"#, (1..=257).map(|n| rec(&format!(r#""p{n}""#), &format!(r#""/a{n}""#), "3")).collect::<Vec<_>>().join(",")));
    for fixture in &corrupt {
        scratch.write_registry(fixture.as_bytes());
        assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryCorrupt, "{fixture}");
    }

    // Positive controls at the edges of each rule.
    let valid = [
        doc("1", ""),
        doc("10000000000", &id_rec("p9999999999")),
        doc("2", &path_rec(&format!("/{}", "a".repeat(MAX_PATH_BYTES - 1)))),
        doc("2", &path_rec("/a b/ünïcode/日本/\u{202e}rtl")),
        doc("2", &rec(r#""p1""#, r#""/a""#, "9007199254740991")),
        doc("2", r#"{"id":"p1","path":"/a","dev":18446744073709551615,"ino":18446744073709551615,"registered_at_ms":0}"#),
        // Two records sharing (dev, ino) are allowed: a stale record and a later folder reusing its inode.
        doc("3", &format!("{},{}", rec(r#""p1""#, r#""/a""#, "3"), rec(r#""p2""#, r#""/b""#, "3"))),
        format!(" \n{}\n ", doc("1", "")),
        r#"{"projects":[],"next_id":1,"schema_version":1}"#.into(),
    ];
    for fixture in &valid {
        scratch.write_registry(fixture.as_bytes());
        assert!(list(&scratch.data()).is_ok(), "{fixture}");
    }
}

#[test]
fn a_newer_schema_is_recognized_from_the_version_alone() {
    let scratch = Scratch::new("newer-parse");
    for newer in [
        r#"{"schema_version":2}"#,
        r#"{"schema_version":2,"next_id":"anything","projects":"else","added_later":true}"#,
        r#"{"schema_version":18446744073709551615}"#,
    ] {
        scratch.write_registry(newer.as_bytes());
        assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryNewerVersion, "{newer}");
    }
    for corrupt in [r#"{"schema_version":2"#, r#"{"schema_version":2,"schema_version":2}"#] {
        scratch.write_registry(corrupt.as_bytes());
        assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryCorrupt, "{corrupt}");
    }
}

// 6. Locking and concurrency

const CHILD_TASK: &str = "SYNAPSE_PROJECTS_TEST_CHILD";
const CHILD_TEST: &str = "projects::registry::tests::child_process";
// libtest may print these on the same line as its own "test ... " header.
const LOCKED: &str = "synapse-child-locked";
const READY: &str = "synapse-child-ready";

/// Not a test of its own: the body runs only in a copy of this test binary started by the tests below.
#[test]
fn child_process() {
    let Ok(task) = std::env::var(CHILD_TASK) else { return };
    let mut lines = task.lines();
    let (mode, dir) = (lines.next().unwrap(), PathBuf::from(lines.next().unwrap()));
    let mut stdin = String::new();
    match mode {
        "hold" => {
            let _locked = lock(&dir, true).unwrap().unwrap();
            println!("{LOCKED}");
            std::io::stdin().read_to_string(&mut stdin).unwrap();
        }
        "add" => {
            println!("{READY}");
            std::io::stdin().read_line(&mut stdin).unwrap();
            for folder in lines {
                added(add(&dir, Path::new(folder)));
            }
        }
        _ => unreachable!(),
    }
}

fn spawn_child(task: &str) -> (Child, impl Iterator<Item = String>) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD_TEST, "--nocapture", "--test-threads", "1"])
        .env(CHILD_TASK, task)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let lines = BufReader::new(child.stdout.take().unwrap()).lines().map(Result::unwrap);
    (child, lines)
}

#[test]
fn another_process_holding_the_lock_makes_a_mutation_unavailable_within_the_budget() {
    let scratch = Scratch::new("lock-process");
    added(add(&scratch.data(), &scratch.folder("a")));
    let lock_file = identity(&scratch.file(LOCK));
    let before = scratch.registry();
    let (mut holder, mut lines) = spawn_child(&format!("hold\n{}", scratch.data().display()));
    assert!(lines.any(|line| line.contains(LOCKED)), "the child never took the lock");

    let started = Instant::now();
    assert_eq!(add(&scratch.data(), &scratch.folder("b")).unwrap_err(), ErrorCode::RegistryUnavailable);
    let waited = started.elapsed();
    assert!(waited >= LOCK_BUDGET && waited < Duration::from_millis(2500), "{waited:?}");
    assert_eq!(remove(&scratch.data(), "p1").unwrap_err(), ErrorCode::RegistryUnavailable);
    assert_eq!(scratch.registry(), before);
    // Reads never take the lock.
    assert_eq!(ids(&scratch.data()), ["p1"]);

    drop(holder.stdin.take());
    assert!(holder.wait().unwrap().success());
    assert_eq!(added(add(&scratch.data(), &scratch.folder("b"))).id, "p2");
    assert_eq!(identity(&scratch.file(LOCK)), lock_file, "the lock file is never replaced");
}

#[test]
fn concurrent_threads_and_a_second_process_all_register_with_unique_ids() {
    let scratch = Scratch::new("concurrent");
    let theirs: Vec<PathBuf> = (0..8).map(|n| scratch.folder(&format!("child-{n}"))).collect();
    let task = std::iter::once("add".to_string())
        .chain(std::iter::once(scratch.data().display().to_string()))
        .chain(theirs.iter().map(|p| p.display().to_string()))
        .collect::<Vec<_>>()
        .join("\n");
    let (mut child, mut lines) = spawn_child(&task);
    assert!(lines.any(|line| line.contains(READY)));
    let ours: Vec<PathBuf> = (0..16).map(|n| scratch.folder(&format!("thread-{n}"))).collect();
    use std::io::Write as _;
    child.stdin.as_mut().unwrap().write_all(b"go\n").unwrap();
    std::thread::scope(|threads| {
        for chunk in ours.chunks(2) {
            let data = scratch.data();
            threads.spawn(move || {
                for folder in chunk {
                    added(add(&data, folder));
                }
            });
        }
    });
    assert!(child.wait().unwrap().success());

    let projects = listed(&scratch.data());
    assert_eq!(projects.len(), 24);
    let mut ids: Vec<u64> = projects.iter().map(|p| id_number(&p.id).unwrap()).collect();
    ids.sort();
    assert_eq!(ids, (1..=24).collect::<Vec<_>>());
    let mut names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
    names.sort();
    let mut expected: Vec<String> = theirs.iter().chain(&ours).map(|p| p.file_name().unwrap().to_str().unwrap().into()).collect();
    expected.sort();
    assert_eq!(names, expected);
    assert_eq!(scratch.names(), [REGISTRY, LOCK]);
}

// 7. Failures before and after the commit point

#[test]
fn a_failure_before_rename_keeps_the_previous_registry_and_removes_the_temp_file() {
    let scratch = Scratch::new("fail-before");
    added(add(&scratch.data(), &scratch.folder("a")));
    let before = scratch.registry();
    for step in [Step::TempWrite, Step::TempSync, Step::Rename] {
        arm(step, libc::EIO);
        assert_eq!(add(&scratch.data(), &scratch.folder("b")).unwrap_err(), ErrorCode::RegistryUnavailable, "{step:?}");
        assert_eq!(scratch.registry(), before, "{step:?}");
        assert!(!scratch.file(TEMP).exists(), "{step:?}");
        arm(step, libc::EIO);
        assert_eq!(remove(&scratch.data(), "p1").unwrap_err(), ErrorCode::RegistryUnavailable, "{step:?}");
        assert_eq!(scratch.registry(), before, "{step:?}");
    }
    assert_eq!(added(add(&scratch.data(), &scratch.folder("b"))).id, "p2");
}

#[test]
fn a_failed_cleanup_leaves_a_stale_temp_file_that_the_next_write_removes() {
    let scratch = Scratch::new("fail-cleanup");
    added(add(&scratch.data(), &scratch.folder("a")));
    let before = scratch.registry();
    arm(Step::TempSync, libc::EIO);
    arm(Step::TempCleanup, libc::EIO);
    assert_eq!(add(&scratch.data(), &scratch.folder("b")).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert_eq!(scratch.registry(), before);
    assert!(scratch.file(TEMP).is_file(), "the leftover temp file is there");
    assert_eq!(ids(&scratch.data()), ["p1"], "and it is not read");
    assert_eq!(added(add(&scratch.data(), &scratch.folder("b"))).id, "p2");
    assert!(!scratch.file(TEMP).exists());
}

#[test]
fn a_failed_directory_sync_after_rename_is_reported_but_the_change_is_committed() {
    let scratch = Scratch::new("fail-after");
    arm(Step::CommitSync, libc::EIO);
    assert_eq!(add(&scratch.data(), &scratch.folder("a")).unwrap_err(), ErrorCode::RegistryUnavailable);
    assert_eq!(ids(&scratch.data()), ["p1"]);
    arm(Step::CommitSync, libc::EIO);
    assert_eq!(remove(&scratch.data(), "p1").unwrap_err(), ErrorCode::RegistryUnavailable);
    assert_eq!(ids(&scratch.data()), Vec::<String>::new());
    // A filesystem that cannot sync directories is accepted.
    for unsupported in [libc::EINVAL, libc::ENOTSUP] {
        arm(Step::CommitSync, unsupported);
        assert!(add(&scratch.data(), &scratch.folder(&format!("ok-{unsupported}"))).is_ok());
    }
}

// 8, 9, 11. Listing never writes; corrupt and newer registries are never modified.

fn assert_untouched(scratch: &Scratch, bytes: &[u8], modified_at: std::time::SystemTime, names: &[String]) {
    assert_eq!(scratch.registry(), bytes);
    assert_eq!(modified(&scratch.file(REGISTRY)), modified_at);
    assert_eq!(scratch.names(), names);
}

#[test]
fn listing_never_changes_the_registry_in_any_state() {
    let scratch = Scratch::new("list-readonly");
    let folder = scratch.folder("a");
    let id = identity(&folder);
    for contents in [
        registry_json(2, vec![record("p1", folder.to_str().unwrap(), id.dev, id.ino)]),
        registry_json(2, vec![record("p1", "/nonexistent/synapse-test", 1, 2)]),
        b"{ corrupt".to_vec(),
        br#"{"schema_version":7}"#.to_vec(),
    ] {
        scratch.write_registry(&contents);
        let (at, names) = (modified(&scratch.file(REGISTRY)), scratch.names());
        std::thread::sleep(Duration::from_millis(5));
        for _ in 0..3 {
            let _ = list(&scratch.data());
        }
        assert_untouched(&scratch, &contents, at, &names);
        assert_eq!(names, [REGISTRY], "no lock or temp file is created by a read");
    }
}

#[test]
fn a_corrupt_registry_blocks_every_mutation_and_stays_byte_identical() {
    let scratch = Scratch::new("corrupt");
    let corrupt = br#"{"schema_version":1,"next_id":1,"projects":[],"unexpected":true}"#;
    scratch.write_registry(corrupt);
    let (at, _) = (modified(&scratch.file(REGISTRY)), ());
    assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryCorrupt);
    assert_eq!(add(&scratch.data(), &scratch.folder("a")).unwrap_err(), ErrorCode::RegistryCorrupt);
    assert_eq!(remove(&scratch.data(), "p1").unwrap_err(), ErrorCode::RegistryCorrupt);
    assert_untouched(&scratch, corrupt, at, &[REGISTRY.to_string(), LOCK.to_string()]);
}

#[test]
fn a_newer_registry_is_never_written_renamed_or_reset() {
    let scratch = Scratch::new("newer");
    let newer = br#"{"schema_version":2,"next_id":1,"projects":[],"tags":["future"]}"#;
    scratch.write_registry(newer);
    let at = modified(&scratch.file(REGISTRY));
    assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryNewerVersion);
    assert_eq!(add(&scratch.data(), &scratch.folder("a")).unwrap_err(), ErrorCode::RegistryNewerVersion);
    assert_eq!(remove(&scratch.data(), "p1").unwrap_err(), ErrorCode::RegistryNewerVersion);
    assert_eq!(reset(&scratch.data()).unwrap_err(), ErrorCode::RegistryNewerVersion);
    assert_untouched(&scratch, newer, at, &[REGISTRY.to_string(), LOCK.to_string()]);
}

// 10. Reset

const CORRUPT: &[u8] = b"{\"schema_version\":1,\"next_id\":2,\"projects\":[{\"id\":\"p1\"}]}\n\x00\xff";

#[test]
fn reset_preserves_the_corrupt_bytes_and_starts_an_empty_list() {
    let scratch = Scratch::new("reset");
    scratch.write_registry(CORRUPT);
    let corrupt_id = identity(&scratch.file(REGISTRY));
    let before = now_ms().unwrap();
    assert_eq!(reset(&scratch.data()), Ok(ResetOutcome::Reset));
    let backups = scratch.backups();
    assert_eq!(backups.len(), 1);
    let stamp: u64 = backups[0].strip_prefix(BACKUP_PREFIX).unwrap().parse().unwrap();
    assert!(stamp >= before && stamp <= now_ms().unwrap());
    let backup = scratch.file(&backups[0]);
    assert_eq!(std::fs::read(&backup).unwrap(), CORRUPT);
    assert_eq!(identity(&backup), corrupt_id, "the backup is the very file that was found corrupt");
    assert!(!scratch.file(REGISTRY).exists());
    assert_eq!(listed(&scratch.data()), []);
    // IDs restart and the backup is left alone by later writes.
    assert_eq!(added(add(&scratch.data(), &scratch.folder("a"))).id, "p1");
    assert_eq!(std::fs::read(&backup).unwrap(), CORRUPT);
    assert_eq!(reset(&scratch.data()), Ok(ResetOutcome::NotNeeded));
    assert_eq!(scratch.backups(), backups);
}

#[test]
fn reset_never_overwrites_an_existing_backup_name() {
    let scratch = Scratch::new("reset-collision");
    scratch.write_registry(CORRUPT);
    let registry = scratch.file(REGISTRY);
    for name in ["projects.json.corrupt-42", "projects.json.corrupt-42-1"] {
        std::fs::write(scratch.file(name), name).unwrap();
    }
    assert_eq!(preserve(&registry, &scratch.data(), 42), Ok(scratch.file("projects.json.corrupt-42-2")));
    for name in ["projects.json.corrupt-42", "projects.json.corrupt-42-1"] {
        assert_eq!(std::fs::read(scratch.file(name)).unwrap(), name.as_bytes());
    }
    for suffix in 3..=BACKUP_SUFFIXES {
        std::fs::write(scratch.file(&format!("projects.json.corrupt-42-{suffix}")), b"taken").unwrap();
    }
    assert_eq!(preserve(&registry, &scratch.data(), 42), Err(ErrorCode::RegistryUnavailable));
    assert_eq!(scratch.backups().len(), 1 + BACKUP_SUFFIXES as usize);

    // End to end: every name the clock could produce during the call is already taken without a suffix.
    let scratch = Scratch::new("reset-collision-e2e");
    scratch.write_registry(CORRUPT);
    let start = now_ms().unwrap();
    let taken: Vec<String> = (start..start + 3000).map(|ms| format!("{BACKUP_PREFIX}{ms}")).collect();
    for name in &taken {
        std::fs::write(scratch.file(name), b"existing").unwrap();
    }
    assert_eq!(reset(&scratch.data()), Ok(ResetOutcome::Reset));
    assert!(taken.iter().all(|name| std::fs::read(scratch.file(name)).unwrap() == b"existing"));
    let new: Vec<String> = scratch.backups().into_iter().filter(|n| !taken.contains(n)).collect();
    assert_eq!(new.len(), 1);
    assert!(new[0].ends_with("-1"), "{new:?}");
    assert_eq!(std::fs::read(scratch.file(&new[0])).unwrap(), CORRUPT);
}

#[test]
fn reset_touches_nothing_unless_the_registry_is_corrupt_now() {
    let scratch = Scratch::new("reset-not-needed");
    assert_eq!(reset(&scratch.data()), Ok(ResetOutcome::NotNeeded));
    assert!(!scratch.data().exists(), "a missing directory is not created");
    std::fs::DirBuilder::new().mode(0o700).create(scratch.data()).unwrap();
    assert_eq!(reset(&scratch.data()), Ok(ResetOutcome::NotNeeded));
    assert!(!scratch.file(REGISTRY).exists());

    added(add(&scratch.data(), &scratch.folder("a")));
    let (valid, at) = (scratch.registry(), modified(&scratch.file(REGISTRY)));
    assert_eq!(reset(&scratch.data()), Ok(ResetOutcome::NotNeeded));
    assert_untouched(&scratch, &valid, at, &[REGISTRY.to_string(), LOCK.to_string()]);

    // Seen corrupt by an earlier list, repaired before the reset: the decision is taken from the file now.
    scratch.write_registry(CORRUPT);
    assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryCorrupt);
    scratch.write_registry(&valid);
    let at = modified(&scratch.file(REGISTRY));
    assert_eq!(reset(&scratch.data()), Ok(ResetOutcome::NotNeeded));
    assert_untouched(&scratch, &valid, at, &[REGISTRY.to_string(), LOCK.to_string()]);
}

#[test]
fn a_reset_failure_before_the_unlink_keeps_the_corrupt_registry_and_a_retry_completes() {
    for (step, leaves_backup) in [
        (Step::BackupLink, false),
        (Step::BackupCheck, true),
        (Step::BackupSync, true),
        (Step::ResetUnlink, true),
    ] {
        let scratch = Scratch::new(&format!("reset-fail-{step:?}"));
        scratch.write_registry(CORRUPT);
        arm(step, libc::EIO);
        assert_eq!(reset(&scratch.data()), Err(ErrorCode::RegistryUnavailable), "{step:?}");
        assert_eq!(scratch.registry(), CORRUPT, "{step:?}");
        assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryCorrupt, "{step:?}");
        let first = scratch.backups();
        assert_eq!(first.len(), leaves_backup as usize, "{step:?}");
        for name in &first {
            assert_eq!(std::fs::read(scratch.file(name)).unwrap(), CORRUPT, "{step:?}");
        }

        assert_eq!(reset(&scratch.data()), Ok(ResetOutcome::Reset), "{step:?}");
        assert!(!scratch.file(REGISTRY).exists());
        let all = scratch.backups();
        assert_eq!(all.len(), first.len() + 1, "{step:?}: the retry made its own copy");
        for name in &all {
            assert_eq!(std::fs::read(scratch.file(name)).unwrap(), CORRUPT, "{step:?}");
        }
    }
}

#[test]
fn a_reset_failure_after_the_unlink_is_reported_but_the_reset_is_committed() {
    let scratch = Scratch::new("reset-fail-after");
    scratch.write_registry(CORRUPT);
    arm(Step::ResetSync, libc::EIO);
    assert_eq!(reset(&scratch.data()), Err(ErrorCode::RegistryUnavailable));
    assert_eq!(listed(&scratch.data()), []);
    let backups = scratch.backups();
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(scratch.file(&backups[0])).unwrap(), CORRUPT);
    // Directory sync unsupported by the filesystem is not a failure.
    scratch.write_registry(CORRUPT);
    arm(Step::BackupSync, libc::ENOTSUP);
    arm(Step::ResetSync, libc::EINVAL);
    assert_eq!(reset(&scratch.data()), Ok(ResetOutcome::Reset));
}

#[test]
fn an_oversized_registry_can_be_reset() {
    let scratch = Scratch::new("reset-oversized");
    let big = vec![b' '; MAX_BYTES as usize + 10];
    scratch.write_registry(&big);
    assert_eq!(list(&scratch.data()).unwrap_err(), ErrorCode::RegistryCorrupt);
    assert_eq!(reset(&scratch.data()), Ok(ResetOutcome::Reset));
    assert_eq!(std::fs::read(scratch.file(&scratch.backups()[0])).unwrap(), big);
}

// Remove

#[test]
fn remove_validates_the_id_before_touching_anything() {
    let scratch = Scratch::new("remove-invalid");
    for bad in ["", "p", "p0", "p01", "P1", "1", "p1/../p2", "../projects.json", "/etc", "p12345678901", " p1", "p1\n", "p٣"] {
        assert_eq!(remove(&scratch.data(), bad), Err(ErrorCode::InvalidInput), "{bad:?}");
    }
    assert!(!scratch.data().exists());
}

#[test]
fn remove_forgets_the_record_and_leaves_the_folder_exactly_as_it_was() {
    let scratch = Scratch::new("remove");
    let folder = scratch.folder("keep");
    std::fs::create_dir(folder.join("src")).unwrap();
    std::fs::write(folder.join("src").join("main.rs"), b"fn main() {}").unwrap();
    std::fs::write(folder.join("README.md"), b"keep me").unwrap();
    std::os::unix::fs::symlink("/etc/hosts", folder.join("link")).unwrap();
    let before = snapshot(&folder);
    let folder_meta = (mode(&folder), modified(&folder));

    let project = added(add(&scratch.data(), &folder));
    added(add(&scratch.data(), &scratch.folder("other")));
    assert_eq!(remove(&scratch.data(), &project.id), Ok(RemoveOutcome::Removed));
    assert_eq!(ids(&scratch.data()), ["p2"]);
    assert_eq!(snapshot(&folder), before);
    assert_eq!((mode(&folder), modified(&folder)), folder_meta);

    let at = modified(&scratch.file(REGISTRY));
    assert_eq!(remove(&scratch.data(), "p1"), Ok(RemoveOutcome::NotFound));
    assert_eq!(remove(&scratch.data(), "p9999999999"), Ok(RemoveOutcome::NotFound));
    assert_eq!(modified(&scratch.file(REGISTRY)), at, "nothing is written when nothing changes");
    // IDs are never reused while the file exists, even after removing every record.
    assert_eq!(remove(&scratch.data(), "p2"), Ok(RemoveOutcome::Removed));
    assert_eq!(added(add(&scratch.data(), &folder)).id, "p3");
}

// 12–16. Identity, duplicates, refusal and status

#[test]
fn a_recreated_folder_is_changed_and_keeps_its_old_registration() {
    let scratch = Scratch::new("recreated");
    let folder = scratch.folder("project");
    let project = added(add(&scratch.data(), &folder));
    let original = stored(&scratch).projects[0].file_id();
    // Keep the old inode alive so the new directory cannot reuse it.
    let moved = scratch.0.join("moved-away");
    std::fs::rename(&folder, &moved).unwrap();
    std::fs::create_dir(&folder).unwrap();
    assert_eq!(listed(&scratch.data())[0].status, Status::Changed);
    let again = existing(add(&scratch.data(), &folder));
    assert_eq!((again.id.as_str(), again.status), (project.id.as_str(), Status::Changed));
    assert_eq!(stored(&scratch).projects.len(), 1);
    assert_eq!(stored(&scratch).projects[0].file_id(), original, "the record is never rebound");
    // Remove and add again to register the new folder.
    assert_eq!(remove(&scratch.data(), &project.id), Ok(RemoveOutcome::Removed));
    let fresh = added(add(&scratch.data(), &folder));
    assert_eq!((fresh.id.as_str(), fresh.status), ("p2", Status::Available));
}

#[test]
fn a_stale_record_with_a_reused_inode_does_not_block_a_new_folder() {
    let scratch = Scratch::new("inode-reuse");
    let candidate = scratch.folder("new-folder");
    let elsewhere = scratch.folder("other-folder");
    let reused = identity(&candidate);
    // Two stale records claim the candidate's (dev, ino): one whose path is gone, one whose path now names a
    // different folder. Neither is confirmed by lstat of its own path, so neither is a duplicate.
    scratch.write_registry(&registry_json(
        3,
        vec![
            record("p1", "/nonexistent/synapse-test/deleted", reused.dev, reused.ino),
            record("p2", elsewhere.to_str().unwrap(), reused.dev, reused.ino),
        ],
    ));
    let project = added(add(&scratch.data(), &candidate));
    assert_eq!(project.id, "p3");
    let statuses: Vec<Status> = listed(&scratch.data()).iter().map(|p| p.status).collect();
    assert_eq!(statuses, [Status::Missing, Status::Changed, Status::Available]);
}

#[test]
fn aliases_of_a_registered_folder_return_the_existing_registration() {
    let scratch = Scratch::new("aliases");
    let folder = scratch.folder("Atlas");
    let project = added(add(&scratch.data(), &folder));
    let link = scratch.0.join("link-to-atlas");
    std::os::unix::fs::symlink(&folder, &link).unwrap();
    let mut aliases = vec![link, folder.join("."), scratch.0.join("folders/../folders/Atlas")];
    // Only where the filesystem folds case (APFS default) or the data-volume firmlink exists.
    let upper = scratch.0.join("folders").join("ATLAS");
    if upper.exists() {
        aliases.push(upper);
    }
    let data = Path::new("/System/Volumes/Data").join(folder.strip_prefix("/").unwrap());
    if data.exists() {
        assert_ne!(data.canonicalize().unwrap(), folder, "a firmlink spelling canonicalizes to a different string");
        aliases.push(data);
    }
    for alias in &aliases {
        assert_eq!(existing(add(&scratch.data(), alias)), project, "{alias:?}");
    }
    assert_eq!(ids(&scratch.data()), ["p1"]);
    // Through a symlink, the canonical target is what is stored.
    let other = scratch.folder("target");
    let link = scratch.0.join("link-to-target");
    std::os::unix::fs::symlink(&other, &link).unwrap();
    let linked = added(add(&scratch.data(), &link));
    assert_eq!(linked.path_display, other.to_str().unwrap());
    assert_eq!(linked.name, "target");
}

#[test]
fn broad_folders_are_refused_by_identity_and_nothing_is_created() {
    let scratch = Scratch::new("refused");
    let home = folder::home().expect("this user has a home directory");
    let mut broad = vec![PathBuf::from("/"), home.clone(), PathBuf::from("/Users")];
    broad.extend(home.ancestors().skip(1).map(Path::to_path_buf));
    let data = Path::new("/System/Volumes/Data");
    if data.exists() {
        broad.push(data.to_path_buf());
        broad.push(data.join(home.canonicalize().unwrap().strip_prefix("/").unwrap()));
        broad.push(data.join("Users"));
    }
    for path in broad.iter().filter(|p| p.exists()) {
        assert_eq!(add(&scratch.data(), path).unwrap_err(), ErrorCode::FolderRefused, "{path:?}");
    }
    assert!(!scratch.data().exists());
}

#[test]
fn the_refusal_policy_follows_the_given_home_and_fails_closed_without_one() {
    let scratch = Scratch::new("refusal-policy");
    let home = scratch.folder("home/me");
    let inside = scratch.folder("home/me/code/project");
    let sibling = scratch.folder("home/me2");
    for refused_path in [&home, &scratch.0.join("folders/home"), &scratch.0, &PathBuf::from("/")] {
        assert!(folder::refused(identity(refused_path), Some(&home)), "{refused_path:?}");
    }
    for allowed in [&inside, &sibling, &home.join("code")] {
        assert!(!folder::refused(identity(allowed), Some(&home)), "{allowed:?}");
    }
    assert!(folder::refused(identity(&inside), None));
    assert!(folder::refused(identity(&inside), Some(Path::new("/nonexistent/synapse-test/home"))));
}

#[test]
fn nested_folders_and_folders_inside_home_like_trees_register_independently() {
    let scratch = Scratch::new("nested");
    let outer = added(add(&scratch.data(), &scratch.folder("outer")));
    let inner = added(add(&scratch.data(), &scratch.folder("outer/inner")));
    assert_eq!((outer.id.as_str(), inner.id.as_str()), ("p1", "p2"));
    assert_eq!(remove(&scratch.data(), "p1"), Ok(RemoveOutcome::Removed));
    assert_eq!(ids(&scratch.data()), ["p2"]);
}

#[test]
fn files_missing_paths_and_unsupported_paths_are_rejected_before_any_write() {
    let scratch = Scratch::new("rejected");
    let file = scratch.0.join("file.txt");
    std::fs::write(&file, b"x").unwrap();
    assert_eq!(add(&scratch.data(), &file).unwrap_err(), ErrorCode::NotADirectory);
    assert_eq!(add(&scratch.data(), &scratch.0.join("gone")).unwrap_err(), ErrorCode::FolderChanged);
    assert_eq!(add(&scratch.data(), &file.join("child")).unwrap_err(), ErrorCode::FolderChanged);
    // A canonical path the registry would reject on load is never written.
    // (An overlong canonical path cannot be produced on macOS: PATH_MAX is 1024 including the terminator.)
    let control = scratch.folder("bell\u{7}name");
    assert_eq!(add(&scratch.data(), &control).unwrap_err(), ErrorCode::PathUnsupported);
    if !is_root() {
        let locked = scratch.folder("locked-parent");
        let child = scratch.folder("locked-parent/child");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(add(&scratch.data(), &child).unwrap_err(), ErrorCode::FolderInaccessible);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert!(!scratch.data().exists());
}

#[test]
fn a_folder_that_changes_after_the_check_is_not_written() {
    let scratch = Scratch::new("recheck");
    let folder = scratch.folder("racing");
    let candidate = folder::inspect(&folder, valid_path).unwrap();
    assert_eq!(folder::recheck(&candidate), Ok(()));
    let moved = scratch.0.join("moved");
    std::fs::rename(&folder, &moved).unwrap();
    std::fs::create_dir(&folder).unwrap();
    assert_eq!(folder::recheck(&candidate), Err(ErrorCode::FolderChanged));
    std::fs::remove_dir(&folder).unwrap();
    std::os::unix::fs::symlink(&moved, &folder).unwrap();
    assert_eq!(folder::recheck(&candidate), Err(ErrorCode::FolderChanged));
    std::fs::remove_file(&folder).unwrap();
    assert_eq!(folder::recheck(&candidate), Err(ErrorCode::FolderChanged));
    std::fs::write(&folder, b"now a file").unwrap();
    assert_eq!(folder::recheck(&candidate), Err(ErrorCode::FolderChanged));
}

#[test]
fn revalidation_reports_missing_changed_and_inaccessible_folders() {
    let scratch = Scratch::new("status");
    let names = ["missing", "symlinked", "replaced", "file", "closed", "unreadable/child"];
    for name in names {
        added(add(&scratch.data(), &scratch.folder(name)));
    }
    let path = |name: &str| scratch.0.join("folders").join(name);
    std::fs::remove_dir(path("missing")).unwrap();
    let elsewhere = scratch.folder("elsewhere");
    std::fs::rename(path("symlinked"), scratch.0.join("kept-1")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, path("symlinked")).unwrap();
    std::fs::rename(path("replaced"), scratch.0.join("kept-2")).unwrap();
    std::fs::create_dir(path("replaced")).unwrap();
    std::fs::rename(path("file"), scratch.0.join("kept-3")).unwrap();
    std::fs::write(path("file"), b"").unwrap();
    // A folder without read or search permission is still available: its contents are never needed.
    std::fs::set_permissions(path("closed"), std::fs::Permissions::from_mode(0o000)).unwrap();
    std::fs::set_permissions(path("unreadable"), std::fs::Permissions::from_mode(0o000)).unwrap();

    let statuses: Vec<(String, Status)> = listed(&scratch.data()).into_iter().map(|p| (p.name, p.status)).collect();
    let parent_status = if is_root() { Status::Available } else { Status::Inaccessible };
    assert_eq!(
        statuses,
        [
            ("missing".into(), Status::Missing),
            ("symlinked".into(), Status::Changed),
            ("replaced".into(), Status::Changed),
            ("file".into(), Status::Changed),
            ("closed".into(), Status::Available),
            ("child".into(), parent_status),
        ]
    );
    // Listing never rewrites stored identities.
    let file = stored(&scratch);
    std::fs::set_permissions(path("unreadable"), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(stored(&scratch).projects.len(), file.projects.len());
}

#[test]
fn registering_needs_no_access_to_the_folder_contents() {
    let scratch = Scratch::new("no-content");
    let folder = scratch.folder("sealed");
    std::fs::write(folder.join("secret.txt"), b"not for synapse").unwrap();
    mkfifo(&folder.join("README.md"));
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o000)).unwrap();
    let project = added(add(&scratch.data(), &folder));
    assert_eq!(project.status, Status::Available);
    assert_eq!(listed(&scratch.data()), [project.clone()]);
    assert_eq!(remove(&scratch.data(), &project.id), Ok(RemoveOutcome::Removed));
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(std::fs::read(folder.join("secret.txt")).unwrap(), b"not for synapse");
    let registry = String::from_utf8(scratch.registry()).unwrap();
    assert!(!registry.contains("secret") && !registry.contains("README"));
}

// Display

#[test]
fn display_abbreviates_home_only_at_a_component_boundary() {
    let home = Some("/Users/me");
    assert_eq!(folder::display("/Users/me", home), "~");
    assert_eq!(folder::display("/Users/me/code/atlas", home), "~/code/atlas");
    assert_eq!(folder::display("/Users/me2/code", home), "/Users/me2/code");
    assert_eq!(folder::display("/Volumes/Work/me", home), "/Volumes/Work/me");
    assert_eq!(folder::display("/Users/me/code", None), "/Users/me/code");
    assert_eq!(folder::name("/Users/me/code/atlas-api"), "atlas-api");
    assert_eq!(folder::name("/Volumes/Work"), "Work");
}

#[test]
fn errors_carry_only_fixed_messages() {
    let all = [
        ErrorCode::NotAllowed,
        ErrorCode::SelectionInProgress,
        ErrorCode::NotADirectory,
        ErrorCode::FolderRefused,
        ErrorCode::FolderChanged,
        ErrorCode::FolderInaccessible,
        ErrorCode::PathUnsupported,
        ErrorCode::RegistryFull,
        ErrorCode::InvalidInput,
        ErrorCode::RegistryCorrupt,
        ErrorCode::RegistryNewerVersion,
        ErrorCode::RegistryUnavailable,
        ErrorCode::UnsupportedPlatform,
    ];
    let codes: Vec<String> = all
        .iter()
        .map(|code| {
            let value = serde_json::to_value(ProjectError::from(*code)).unwrap();
            assert_eq!(value.as_object().unwrap().len(), 2);
            let message = value["message"].as_str().unwrap();
            assert!(!message.contains('/') && !message.contains('~') && message.ends_with('.'), "{message}");
            value["code"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(
        codes,
        [
            "not_allowed",
            "selection_in_progress",
            "not_a_directory",
            "folder_refused",
            "folder_changed",
            "folder_inaccessible",
            "path_unsupported",
            "registry_full",
            "invalid_input",
            "registry_corrupt",
            "registry_newer_version",
            "registry_unavailable",
            "unsupported_platform"
        ]
    );
}
