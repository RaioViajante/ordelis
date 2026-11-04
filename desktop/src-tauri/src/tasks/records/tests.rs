// Task store tests. Everything happens in private directories under std::env::temp_dir(); the real app data directory,
// real projects and the real registries are never touched. Registry files here are synthetic fixtures written by hand.
use super::*;
use crate::store::{arm, euid, Step, LOCK_BUDGET};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const REGISTRY: &str = "projects.json";

thread_local! {
    static AFTER_PROJECT_CHECK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = std::cell::RefCell::new(None);
}

/// Runs `change` on this thread once, in create, after the chosen registration was found in the registry and before
/// the tasks lock is taken: another instance changing the registry at that exact moment.
fn arm_after_project_check(change: impl FnOnce() + 'static) {
    AFTER_PROJECT_CHECK.with(|hook| *hook.borrow_mut() = Some(Box::new(change)));
}

pub(super) fn after_project_check() {
    if let Some(change) = AFTER_PROJECT_CHECK.with(|hook| hook.borrow_mut().take()) {
        change();
    }
}

// Fixtures

fn registry(projects: &[(&str, u64)]) -> Vec<u8> {
    let next = projects.iter().map(|(id, _)| id[1..].parse::<u64>().unwrap()).max().unwrap_or(0) + 1;
    let records: Vec<Value> = projects
        .iter()
        .map(|(id, at)| json!({ "id": id, "path": format!("/ordelis-test-nonexistent/{id}"), "dev": 1, "ino": 2, "registered_at_ms": at }))
        .collect();
    serde_json::to_vec(&json!({ "schema_version": 1, "next_id": next, "projects": records })).unwrap()
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ordelis-tasks-{}-{tag}", std::process::id()));
        if dir.exists() {
            let _ = Command::new("/bin/chmod").args(["-R", "u+rwx"]).arg(&dir).status();
            let _ = std::fs::remove_dir_all(&dir);
        }
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        Scratch(dir.canonicalize().unwrap())
    }

    /// The app data directory `D`, shared by the registry and the task store; created by `register`.
    fn data(&self) -> PathBuf {
        self.0.join("data")
    }

    fn file(&self, name: &str) -> PathBuf {
        self.data().join(name)
    }

    fn write(&self, name: &str, contents: &[u8]) {
        let _ = std::fs::DirBuilder::new().mode(0o700).create(self.data());
        std::fs::write(self.file(name), contents).unwrap();
    }

    /// A synthetic registry naming the given (id, registration time) pairs, with folders that do not exist.
    fn register(&self, projects: &[(&str, u64)]) {
        self.write(REGISTRY, &registry(projects));
    }

    fn tasks(&self) -> Vec<u8> {
        std::fs::read(self.file(TASKS)).unwrap()
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = match std::fs::read_dir(self.data()) {
            Ok(entries) => entries.map(|e| e.unwrap().file_name().into_string().unwrap()).collect(),
            Err(_) => return Vec::new(),
        };
        names.sort();
        names
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = Command::new("/bin/chmod").args(["-R", "u+rwx"]).arg(&self.0).status();
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn created(outcome: Result<CreateOutcome, TaskErrorCode>) -> Task {
    match outcome {
        Ok(CreateOutcome::Created { task }) => task,
        other => panic!("expected created, got {other:?}"),
    }
}

fn updated(outcome: Result<UpdateOutcome, TaskErrorCode>) -> Task {
    match outcome {
        Ok(UpdateOutcome::Updated { task }) => task,
        other => panic!("expected updated, got {other:?}"),
    }
}

fn found(dir: &Path, id: &str) -> Task {
    match get(dir, id) {
        Ok(GetOutcome::Found { task }) => task,
        other => panic!("expected found, got {other:?}"),
    }
}

fn links(dir: &Path) -> Vec<(String, Link)> {
    list(dir).unwrap().tasks.into_iter().map(|t| (t.id, t.project.link)).collect()
}

fn modified(path: &Path) -> std::time::SystemTime {
    std::fs::symlink_metadata(path).unwrap().modified().unwrap()
}

fn mkfifo(path: &Path) {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: c is a valid NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
}

/// Takes an flock on `path` through a descriptor of its own, as another writer would. In this process a second
/// descriptor is refused like another process's would be.
fn hold_flock(path: &Path) -> std::fs::File {
    let file = std::fs::OpenOptions::new().read(true).write(true).create(true).mode(0o600).open(path).unwrap();
    // SAFETY: flock on a descriptor owned by `file`.
    assert_eq!(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }, 0);
    file
}

/// A valid version-1 file with one task, as a JSON value to edit.
fn one_task() -> Value {
    json!({
        "schema_version": 1,
        "next_id": 2,
        "tasks": [{
            "id": "t1",
            "project": { "id": "p1", "registered_at_ms": 100 },
            "title": "Add rate limiting",
            "description": "Limit requests.\n\tKeep the health check exempt.",
            "status": "draft",
            "created_at_ms": 1000,
            "updated_at_ms": 2000,
            "revision": 3
        }]
    })
}

/// A task of the largest size whose JSON escaping is the longest possible: every character is a quote.
fn largest(id: u64) -> Value {
    json!({
        "id": format!("t{id}"),
        "project": { "id": "p9999999999", "registered_at_ms": MAX_SAFE_INTEGER },
        "title": "\"".repeat(MAX_TITLE_BYTES),
        "description": "\"".repeat(MAX_TEXT_BYTES - 2 - MAX_TITLE_BYTES),
        "status": "archived",
        "created_at_ms": MAX_SAFE_INTEGER,
        "updated_at_ms": MAX_SAFE_INTEGER,
        "revision": MAX_SAFE_INTEGER
    })
}

// 1. Creating, reading and persistence

#[test]
fn a_created_task_is_a_draft_at_revision_1_and_is_read_back_from_disk() {
    let scratch = Scratch::new("create");
    scratch.register(&[("p1", 100)]);
    let registry = std::fs::read(scratch.file(REGISTRY)).unwrap();
    let task = created(create(&scratch.data(), "p1", 100, "Add rate limiting", "Per IP.\nKeep the health check exempt."));
    assert_eq!((task.summary.id.as_str(), task.summary.status, task.summary.revision), ("t1", Status::Draft, 1));
    assert_eq!(task.summary.created_at_ms, task.summary.updated_at_ms);
    assert_eq!(task.summary.project, ProjectLink { id: "p1".into(), registered_at_ms: 100, link: Link::Linked });
    assert_eq!(task.description, "Per IP.\nKeep the health check exempt.");

    // Nothing is cached: every call reads the file again, as after a restart.
    assert_eq!(found(&scratch.data(), "t1"), task);
    assert_eq!(list(&scratch.data()).unwrap().tasks, [task.summary.clone()]);
    assert_eq!(created(create(&scratch.data(), "p1", 100, "Second", "")).summary.id, "t2");
    assert_eq!(get(&scratch.data(), "t3").unwrap(), GetOutcome::NotFound);
    assert_eq!(std::fs::read(scratch.file(REGISTRY)).unwrap(), registry, "the registry is only read");
    assert_eq!(scratch.names(), [REGISTRY, TASKS, LOCK]);
}

#[test]
fn the_saved_file_holds_exactly_the_approved_fields_and_no_folder_information() {
    let scratch = Scratch::new("fields");
    scratch.register(&[("p1", 100)]);
    created(create(&scratch.data(), "p1", 100, "Title", "Body"));
    let saved: Value = serde_json::from_slice(&scratch.tasks()).unwrap();
    let keys = |v: &Value| {
        let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(keys(&saved), ["next_id", "schema_version", "tasks"]);
    let record = &saved["tasks"][0];
    assert_eq!(
        keys(record),
        ["created_at_ms", "description", "id", "project", "revision", "status", "title", "updated_at_ms"]
    );
    assert_eq!(record["project"], json!({ "id": "p1", "registered_at_ms": 100 }));
    let text = String::from_utf8(scratch.tasks()).unwrap();
    assert!(!text.contains("ordelis-test-nonexistent"), "no registry path is copied into a task");
    // The response carries the saved identity and the link computed now, and nothing about the folder.
    let response = serde_json::to_value(found(&scratch.data(), "t1")).unwrap();
    assert_eq!(response["project"], json!({ "id": "p1", "registered_at_ms": 100, "link": "linked" }));
}

#[test]
fn created_files_are_owner_only() {
    let scratch = Scratch::new("modes");
    scratch.register(&[("p1", 100)]);
    created(create(&scratch.data(), "p1", 100, "Title", ""));
    for name in [TASKS, LOCK] {
        let meta = std::fs::symlink_metadata(scratch.file(name)).unwrap();
        assert_eq!(meta.mode() & 0o777, 0o600, "{name}");
        assert_eq!(meta.uid(), euid(), "{name}");
    }
}

#[test]
fn no_project_folder_is_looked_at_or_changed() {
    let scratch = Scratch::new("folder");
    let folder = scratch.0.join("project");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("secret.txt"), b"not for ordelis").unwrap();
    // The folder is unreadable and unlistable: any attempt to open or list it would fail the commands below.
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o000)).unwrap();
    let meta = std::fs::symlink_metadata(&folder).unwrap();
    let record = json!({ "id": "p1", "path": folder.to_str().unwrap(), "dev": meta.dev(), "ino": meta.ino(), "registered_at_ms": 100 });
    scratch.write(REGISTRY, &serde_json::to_vec(&json!({ "schema_version": 1, "next_id": 2, "projects": [record] })).unwrap());
    let task = created(create(&scratch.data(), "p1", 100, "Title", ""));
    updated(update(&scratch.data(), "t1", 1, "Title", "", "archived"));
    assert_eq!(found(&scratch.data(), "t1").summary.project.link, Link::Linked);
    assert_eq!(task.summary.project.link, Link::Linked);
    let after = std::fs::symlink_metadata(&folder).unwrap();
    assert_eq!((after.mode(), after.mtime(), after.ino()), (meta.mode(), meta.mtime(), meta.ino()));
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(std::fs::read(folder.join("secret.txt")).unwrap(), b"not for ordelis");
    assert!(!String::from_utf8(scratch.tasks()).unwrap().contains(folder.to_str().unwrap()));
}

// 2. Validation

#[test]
fn every_validation_rule_rejects_its_fixture() {
    type Edit = Box<dyn Fn(&mut Value)>;
    let task = |field: &'static str, value: Value| -> Edit { Box::new(move |v: &mut Value| v["tasks"][0][field] = value.clone()) };
    let project = |field: &'static str, value: Value| -> Edit { Box::new(move |v: &mut Value| v["tasks"][0]["project"][field] = value.clone()) };
    let cases: Vec<(&str, Edit)> = vec![
        ("next_id 0", Box::new(|v| v["next_id"] = json!(0))),
        ("next_id over the limit", Box::new(|v| v["next_id"] = json!(MAX_NEXT_ID + 1))),
        ("next_id not an integer", Box::new(|v| v["next_id"] = json!("2"))),
        ("tasks not a list", Box::new(|v| v["tasks"] = json!({}))),
        ("unknown top-level field", Box::new(|v| v["plan"] = json!(null))),
        ("unknown task field", task("agent", json!("x"))),
        ("unknown project field", project("path", json!("/x"))),
        ("missing field", Box::new(|v| drop(v["tasks"][0].as_object_mut().unwrap().remove("revision")))),
        ("id t0", task("id", json!("t0"))),
        ("id with a leading zero", task("id", json!("t01"))),
        ("id in capitals", task("id", json!("T1"))),
        ("id of a project", task("id", json!("p1"))),
        ("id with a space", task("id", json!("t1 "))),
        ("id at next_id", task("id", json!("t2"))),
        ("project id of a task", project("id", json!("t1"))),
        ("project id p0", project("id", json!("p0"))),
        ("registration time over 2^53-1", project("registered_at_ms", json!(MAX_SAFE_INTEGER + 1))),
        ("empty title", task("title", json!(""))),
        ("blank title", task("title", json!("   "))),
        ("title with a leading space", task("title", json!(" Title"))),
        ("title with a trailing newline", task("title", json!("Title\n"))),
        ("title with a newline", task("title", json!("Two\nlines"))),
        ("title with a bell", task("title", json!("Ti\u{7}tle"))),
        ("title over 256 bytes", task("title", json!("é".repeat(129)))),
        ("description with a carriage return", task("description", json!("a\r\nb"))),
        ("description with NUL", task("description", json!("a\u{0}b"))),
        ("description with a C1 control", task("description", json!("a\u{85}b"))),
        ("description with DEL", task("description", json!("a\u{7f}b"))),
        ("text over 4096 bytes", task("description", json!("x".repeat(MAX_TEXT_BYTES - 2 - "Add rate limiting".len() + 1)))),
        ("status running", task("status", json!("running"))),
        ("status completed", task("status", json!("completed"))),
        ("status in capitals", task("status", json!("Draft"))),
        ("status empty", task("status", json!(""))),
        ("status null", task("status", json!(null))),
        ("created after updated", task("created_at_ms", json!(2001))),
        ("updated over 2^53-1", task("updated_at_ms", json!(MAX_SAFE_INTEGER + 1))),
        ("revision 0", task("revision", json!(0))),
        ("revision over 2^53-1", task("revision", json!(MAX_SAFE_INTEGER + 1))),
        ("negative revision", task("revision", json!(-1))),
        ("fractional revision", task("revision", json!(1.5))),
        (
            "duplicate ids",
            Box::new(|v| {
                let copy = v["tasks"][0].clone();
                v["tasks"].as_array_mut().unwrap().push(copy);
            }),
        ),
        (
            "over 500 tasks",
            Box::new(|v| {
                v["next_id"] = json!(MAX_TASKS + 2);
                let template = v["tasks"][0].clone();
                v["tasks"] = (1..=MAX_TASKS + 1)
                    .map(|n| {
                        let mut t = template.clone();
                        t["id"] = json!(format!("t{n}"));
                        t
                    })
                    .collect();
            }),
        ),
    ];
    let scratch = Scratch::new("validation");
    scratch.write(TASKS, &serde_json::to_vec(&one_task()).unwrap());
    assert_eq!(list(&scratch.data()).unwrap().tasks.len(), 1, "the base fixture is valid");
    for (name, edit) in cases {
        let mut value = one_task();
        edit(&mut value);
        let bytes = serde_json::to_vec(&value).unwrap();
        scratch.write(TASKS, &bytes);
        assert_eq!(list(&scratch.data()).unwrap_err(), TasksCorrupt, "{name}");
        assert_eq!(scratch.tasks(), bytes, "{name}: kept unchanged");
    }
    // Duplicate keys, which a JSON value cannot express, at every level.
    for duplicate in [
        r#"{"schema_version":1,"next_id":2,"next_id":2,"tasks":[]}"#,
        r#"{"schema_version":1,"next_id":2,"tasks":[{"id":"t1","id":"t1","project":{"id":"p1","registered_at_ms":1},"title":"T","description":"","status":"draft","created_at_ms":1,"updated_at_ms":1,"revision":1}]}"#,
        r#"{"schema_version":1,"next_id":2,"tasks":[{"id":"t1","project":{"id":"p1","id":"p2","registered_at_ms":1},"title":"T","description":"","status":"draft","created_at_ms":1,"updated_at_ms":1,"revision":1}]}"#,
        r#"{"schema_version":1,"schema_version":1,"next_id":1,"tasks":[]}"#,
    ] {
        scratch.write(TASKS, duplicate.as_bytes());
        assert_eq!(list(&scratch.data()).unwrap_err(), TasksCorrupt, "{duplicate}");
    }
    // Invalid UTF-8 and a lone surrogate escape.
    for bad in [&b"{\"schema_version\":1,\"next_id\":1,\"tasks\":[],\"x\":\"\xff\"}"[..], br#"{"schema_version":1,"next_id":2,"tasks":[{"id":"t1","project":{"id":"p1","registered_at_ms":1},"title":"\ud800","description":"","status":"draft","created_at_ms":1,"updated_at_ms":1,"revision":1}]}"#] {
        scratch.write(TASKS, bad);
        assert_eq!(list(&scratch.data()).unwrap_err(), TasksCorrupt);
    }
}

#[test]
fn a_newer_schema_is_recognized_from_the_version_alone_and_anything_else_unversioned_is_corrupt() {
    let scratch = Scratch::new("versions");
    for newer in [r#"{"schema_version":2}"#, r#"{"schema_version":2,"tasks":"anything","plan":[]}"#] {
        scratch.write(TASKS, newer.as_bytes());
        assert_eq!(list(&scratch.data()).unwrap_err(), TasksNewerVersion, "{newer}");
    }
    for corrupt in ["", "null", "[]", "{}", r#"{"schema_version":0,"next_id":1,"tasks":[]}"#, r#"{"schema_version":"1"}"#, "{\"schema_version\":1"] {
        scratch.write(TASKS, corrupt.as_bytes());
        assert_eq!(list(&scratch.data()).unwrap_err(), TasksCorrupt, "{corrupt:?}");
    }
}

#[test]
fn a_file_over_8_mib_is_corrupt_and_one_of_exactly_8_mib_parses() {
    let scratch = Scratch::new("size");
    let mut bytes = br#"{"schema_version":1,"next_id":1,"tasks":[]}"#.to_vec();
    bytes.resize(MAX_BYTES as usize, b' ');
    scratch.write(TASKS, &bytes);
    assert_eq!(list(&scratch.data()).unwrap().tasks, []);
    bytes.push(b' ');
    scratch.write(TASKS, &bytes);
    assert_eq!(list(&scratch.data()).unwrap_err(), TasksCorrupt);
}

#[test]
fn text_limits_are_inclusive_and_checked_in_bytes_before_anything_is_touched() {
    let scratch = Scratch::new("text");
    let title = "é".repeat(MAX_TITLE_BYTES / 2);
    assert_eq!(title.len(), MAX_TITLE_BYTES);
    let room = MAX_TEXT_BYTES - 2 - MAX_TITLE_BYTES;
    let description = "\n\tx".repeat(room / 3) + &"y".repeat(room % 3);
    assert_eq!(title.len() + 2 + description.len(), MAX_TEXT_BYTES);

    // Rejected before the registry is read: no directory is created even though no registry exists.
    let rejected = [
        ("p1", "", ""),
        ("p1", " Title", ""),
        ("p1", "Title ", ""),
        ("p1", "Two\nlines", ""),
        ("p1", "Tab\there", ""),
        ("p1", &"é".repeat(MAX_TITLE_BYTES / 2 + 1), ""),
        ("p1", &title, &(description.clone() + "z")),
        ("p1", "Title", "carriage\rreturn"),
        ("p1", "Title", "nul\u{0}"),
        ("p1", "Title", "escape\u{1b}[31m"),
        ("p0", "Title", ""),
        ("P1", "Title", ""),
        ("../projects.json", "Title", ""),
        ("/etc", "Title", ""),
        ("t1", "Title", ""),
    ];
    for (project, title, description) in rejected {
        assert_eq!(create(&scratch.data(), project, 100, title, description).unwrap_err(), InvalidInput, "{project} {title:?} {description:?}");
    }
    assert!(!scratch.data().exists(), "invalid input touches nothing");

    scratch.register(&[("p1", 100)]);
    let task = created(create(&scratch.data(), "p1", 100, &title, &description));
    assert_eq!((task.summary.title, task.description), (title, description));
}

#[test]
fn the_largest_possible_list_fits_the_file_limit() {
    // 500 tasks of the largest size, each written with the longest possible escaping.
    let tasks: Vec<Value> = (1..=MAX_TASKS as u64).map(largest).collect();
    let stored: Stored = serde_json::from_value(json!({ "schema_version": 1, "next_id": MAX_TASKS + 1, "tasks": tasks })).unwrap();
    assert!(valid(&stored));
    let bytes = serde_json::to_vec_pretty(&stored).unwrap();
    assert!(bytes.len() as u64 <= MAX_BYTES, "{} bytes", bytes.len());

    // Through the command: the 500th largest task is written, and the 501st is refused.
    let scratch = Scratch::new("largest");
    scratch.register(&[("p1", 100)]);
    let tasks: Vec<Value> = (1..MAX_TASKS as u64).map(largest).collect();
    scratch.write(TASKS, &serde_json::to_vec(&json!({ "schema_version": 1, "next_id": MAX_TASKS, "tasks": tasks })).unwrap());
    let title = "\"".repeat(MAX_TITLE_BYTES);
    let description = "\"".repeat(MAX_TEXT_BYTES - 2 - MAX_TITLE_BYTES);
    assert_eq!(created(create(&scratch.data(), "p1", 100, &title, &description)).summary.id, format!("t{MAX_TASKS}"));
    assert!(scratch.tasks().len() as u64 <= MAX_BYTES);
    let before = scratch.tasks();
    assert_eq!(create(&scratch.data(), "p1", 100, "One more", "").unwrap_err(), TasksFull);
    assert_eq!(scratch.tasks(), before);
}

#[test]
fn the_id_counter_stops_at_its_limit() {
    let scratch = Scratch::new("next-id");
    scratch.register(&[("p1", 100)]);
    scratch.write(TASKS, &serde_json::to_vec(&json!({ "schema_version": 1, "next_id": MAX_NEXT_ID, "tasks": [] })).unwrap());
    let before = scratch.tasks();
    assert_eq!(create(&scratch.data(), "p1", 100, "Title", "").unwrap_err(), TasksFull);
    assert_eq!(scratch.tasks(), before);
}

// 3. Project association

#[test]
fn project_links_are_linked_removed_replaced_or_unknown_and_never_rebound() {
    let scratch = Scratch::new("links");
    scratch.register(&[("p1", 100), ("p2", 200)]);
    created(create(&scratch.data(), "p1", 100, "First", ""));
    created(create(&scratch.data(), "p2", 200, "Second", ""));
    let at = |a: Link, b: Link| vec![("t1".to_string(), a), ("t2".to_string(), b)];
    assert_eq!(links(&scratch.data()), at(Link::Linked, Link::Linked));

    // p1 removed from the registry.
    scratch.register(&[("p2", 200)]);
    assert_eq!(links(&scratch.data()), at(Link::Removed, Link::Linked));
    // The registry was reset and p1 was given to a different folder: the old task is not rebound to it.
    scratch.register(&[("p1", 300)]);
    assert_eq!(links(&scratch.data()), at(Link::Replaced, Link::Removed));
    assert_eq!(found(&scratch.data(), "t1").summary.project, ProjectLink { id: "p1".into(), registered_at_ms: 100, link: Link::Replaced });
    // An update keeps the original identity, so the task stays replaced.
    let task = updated(update(&scratch.data(), "t1", 1, "First", "", "archived"));
    assert_eq!(task.summary.project.link, Link::Replaced);
    assert!(String::from_utf8(scratch.tasks()).unwrap().contains("\"registered_at_ms\": 100"));
    // A new task for the new p1 is linked; the old one is not.
    created(create(&scratch.data(), "p1", 300, "Third", ""));
    assert_eq!(links(&scratch.data())[2], ("t3".to_string(), Link::Linked));
    assert_eq!(links(&scratch.data())[0], ("t1".to_string(), Link::Replaced));

    // An unreadable registry: every link is unknown, and listing still works.
    for unreadable in [&b"{\"schema_version\":1,\"next_id\":\"x\"}"[..], br#"{"schema_version":2}"#] {
        scratch.write(REGISTRY, unreadable);
        assert!(links(&scratch.data()).iter().all(|(_, link)| *link == Link::Unknown));
        assert_eq!(std::fs::read(scratch.file(REGISTRY)).unwrap(), unreadable, "the registry is never repaired from here");
    }
    // No registry at all: no project is registered.
    std::fs::remove_file(scratch.file(REGISTRY)).unwrap();
    assert!(links(&scratch.data()).iter().all(|(_, link)| *link == Link::Removed));
}

#[test]
fn a_task_needs_a_registered_project_in_a_readable_registry() {
    let scratch = Scratch::new("needs-project");
    assert_eq!(create(&scratch.data(), "p1", 100, "Title", "").unwrap_err(), ProjectNotFound);
    assert!(!scratch.data().exists(), "no directory, lock or file is created");
    scratch.register(&[("p2", 200)]);
    assert_eq!(create(&scratch.data(), "p1", 100, "Title", "").unwrap_err(), ProjectNotFound);
    for unreadable in [&b"not json"[..], br#"{"schema_version":2}"#] {
        scratch.write(REGISTRY, unreadable);
        assert_eq!(create(&scratch.data(), "p2", 200, "Title", "").unwrap_err(), ProjectUnavailable);
    }
    assert_eq!(scratch.names(), [REGISTRY], "no task file or lock was created");
}

// Issue #98. Before the fix, create took only the id and copied whatever registration held it when Rust read the
// registry: after a reset that reused p1, a task the user wrote for the first p1 was saved for, and shown as linked
// to, the new folder.
#[test]
fn a_task_is_created_only_for_the_registration_the_caller_chose() {
    let scratch = Scratch::new("chosen");
    scratch.register(&[("p1", 100)]); // what the interface listed, and the user chose
    scratch.register(&[("p1", 300)]); // another instance resets the registry; p1 now names another folder
    assert_eq!(create(&scratch.data(), "p1", 100, "Meant for the first p1", "").unwrap_err(), ProjectNotFound);
    assert_eq!(scratch.names(), [REGISTRY], "no task file or lock was created");
    // The same id with a time no registration has, and a time no registry can hold.
    assert_eq!(create(&scratch.data(), "p1", 101, "Title", "").unwrap_err(), ProjectNotFound);
    assert_eq!(create(&scratch.data(), "p1", MAX_SAFE_INTEGER + 1, "Title", "").unwrap_err(), InvalidInput);
    // Chosen with its own registration time, the new p1 works, and the task saves exactly that identity.
    let task = created(create(&scratch.data(), "p1", 300, "For the new p1", ""));
    assert_eq!(task.summary.project, ProjectLink { id: "p1".into(), registered_at_ms: 300, link: Link::Linked });
    let saved: Value = serde_json::from_slice(&scratch.tasks()).unwrap();
    assert_eq!(saved["tasks"][0]["project"], json!({ "id": "p1", "registered_at_ms": 300 }));
}

// The registry is read without its lock, so it can change between the check and the commit. The task then saves the
// identity that was checked: it can become detached, but never linked to the registration that took the id.
#[test]
fn a_registry_change_after_the_check_can_only_detach_the_new_task() {
    let scratch = Scratch::new("after-check");
    scratch.register(&[("p1", 100)]);
    for (change, link) in [(vec![("p1", 300)], Link::Replaced), (vec![], Link::Removed)] {
        let (file, replacement) = (scratch.file(REGISTRY), registry(&change));
        arm_after_project_check(move || std::fs::write(file, replacement).unwrap());
        let task = created(create(&scratch.data(), "p1", 100, "Checked against p1@100", ""));
        assert_eq!(task.summary.project, ProjectLink { id: "p1".into(), registered_at_ms: 100, link }, "{link:?}");
        assert_eq!(found(&scratch.data(), &task.summary.id).summary.project.registered_at_ms, 100);
        scratch.register(&[("p1", 100)]);
    }
}

#[test]
fn a_task_can_be_read_updated_archived_and_restored_after_its_project_is_removed() {
    let scratch = Scratch::new("orphan");
    scratch.register(&[("p1", 100)]);
    created(create(&scratch.data(), "p1", 100, "Title", "Body"));
    scratch.register(&[]);
    assert_eq!(found(&scratch.data(), "t1").summary.project.link, Link::Removed);
    let archived = updated(update(&scratch.data(), "t1", 1, "Title", "Body", "archived"));
    let restored = updated(update(&scratch.data(), "t1", 2, "Title", "Body", "draft"));
    assert_eq!((archived.summary.status, restored.summary.status), (Status::Archived, Status::Draft));
    assert_eq!(restored.summary.project, ProjectLink { id: "p1".into(), registered_at_ms: 100, link: Link::Removed });
}

// 4. Updates and revisions

#[test]
fn an_update_replaces_text_and_status_and_increments_the_revision_only() {
    let scratch = Scratch::new("update");
    scratch.register(&[("p1", 100)]);
    let first = created(create(&scratch.data(), "p1", 100, "Title", "Body"));
    let archived = updated(update(&scratch.data(), "t1", 1, "New title", "New body", "archived"));
    assert_eq!(archived.summary.revision, 2);
    assert_eq!((archived.summary.title.as_str(), archived.description.as_str(), archived.summary.status), ("New title", "New body", Status::Archived));
    assert_eq!((archived.summary.created_at_ms, archived.summary.project.clone()), (first.summary.created_at_ms, first.summary.project.clone()));
    assert!(archived.summary.updated_at_ms >= first.summary.updated_at_ms);
    assert_eq!(found(&scratch.data(), "t1"), archived, "persisted");
    let restored = updated(update(&scratch.data(), "t1", 2, "New title", "New body", "draft"));
    assert_eq!((restored.summary.revision, restored.summary.status), (3, Status::Draft));
    // An update with nothing changed is still a new revision.
    assert_eq!(updated(update(&scratch.data(), "t1", 3, "New title", "New body", "draft")).summary.revision, 4);
}

#[test]
fn a_stale_revision_is_a_conflict_that_writes_nothing_and_returns_the_current_task() {
    let scratch = Scratch::new("conflict");
    scratch.register(&[("p1", 100)]);
    created(create(&scratch.data(), "p1", 100, "Title", "Body"));
    // Two independent readers see revision 1.
    let (a, b) = (found(&scratch.data(), "t1"), found(&scratch.data(), "t1"));
    assert_eq!((a.summary.revision, b.summary.revision), (1, 1));
    let winner = updated(update(&scratch.data(), "t1", a.summary.revision, "From A", "", "draft"));
    let after_a = scratch.tasks();
    for stale in [b.summary.revision, 3, 99] {
        match update(&scratch.data(), "t1", stale, "From B", "", "archived") {
            Ok(UpdateOutcome::Conflict { task }) => assert_eq!(task, winner, "the current task, not B's"),
            other => panic!("revision {stale}: expected conflict, got {other:?}"),
        }
        assert_eq!(scratch.tasks(), after_a, "a conflict writes nothing");
    }
}

#[test]
fn an_update_validates_everything_before_touching_the_file() {
    let scratch = Scratch::new("update-input");
    assert_eq!(update(&scratch.data(), "t1", 1, "Title", "", "draft"), Ok(UpdateOutcome::NotFound));
    assert!(!scratch.data().exists(), "nothing is created for an absent store");
    scratch.register(&[("p1", 100)]);
    created(create(&scratch.data(), "p1", 100, "Title", ""));
    let before = (scratch.tasks(), modified(&scratch.file(TASKS)));
    for (id, revision, title, status) in [
        ("t0", 1, "Title", "draft"),
        ("../tasks.json", 1, "Title", "draft"),
        ("p1", 1, "Title", "draft"),
        ("t1", 0, "Title", "draft"),
        ("t1", MAX_SAFE_INTEGER + 1, "Title", "draft"),
        ("t1", 1, "", "draft"),
        ("t1", 1, "Title", "running"),
        ("t1", 1, "Title", "completed"),
        ("t1", 1, "Title", "Draft"),
        ("t1", 1, "Title", ""),
    ] {
        assert_eq!(update(&scratch.data(), id, revision, title, "", status).unwrap_err(), InvalidInput, "{id} {revision} {status}");
    }
    assert_eq!(update(&scratch.data(), "t9", 1, "Title", "", "draft"), Ok(UpdateOutcome::NotFound));
    assert_eq!((scratch.tasks(), modified(&scratch.file(TASKS))), before);
    assert_eq!(get(&scratch.data(), "../tasks.json").unwrap_err(), InvalidInput);
}

#[test]
fn the_update_time_never_goes_back_even_if_the_clock_does() {
    let scratch = Scratch::new("clock");
    let future = now_ms().unwrap() + 10_000_000_000;
    let mut value = one_task();
    value["tasks"][0]["created_at_ms"] = json!(future);
    value["tasks"][0]["updated_at_ms"] = json!(future);
    scratch.write(TASKS, &serde_json::to_vec(&value).unwrap());
    let task = updated(update(&scratch.data(), "t1", 3, "Add rate limiting", "", "archived"));
    assert_eq!((task.summary.created_at_ms, task.summary.updated_at_ms), (future, future));
    assert_eq!(list(&scratch.data()).unwrap().tasks.len(), 1, "the file is still valid");
}

// 5. Corrupt and newer files

#[test]
fn a_corrupt_or_newer_task_file_is_never_written_renamed_or_repaired() {
    let scratch = Scratch::new("preserve");
    scratch.register(&[("p1", 100)]);
    let registry = std::fs::read(scratch.file(REGISTRY)).unwrap();
    for (bytes, code) in [(&b"{\"schema_version\":1,\"tasks\":\"oops\"}"[..], TasksCorrupt), (br#"{"schema_version":7,"x":1}"#, TasksNewerVersion)] {
        scratch.write(TASKS, bytes);
        let before = modified(&scratch.file(TASKS));
        assert_eq!(list(&scratch.data()).unwrap_err(), code);
        assert_eq!(get(&scratch.data(), "t1").unwrap_err(), code);
        assert_eq!(create(&scratch.data(), "p1", 100, "Title", "").unwrap_err(), code);
        assert_eq!(update(&scratch.data(), "t1", 1, "Title", "", "draft").unwrap_err(), code);
        assert_eq!((scratch.tasks(), modified(&scratch.file(TASKS))), (bytes.to_vec(), before));
        assert_eq!(scratch.names(), [REGISTRY, TASKS, LOCK], "no backup and no temp file");
        // The project registry is unaffected.
        assert_eq!(crate::projects::registrations(&scratch.data()), Some(vec![("p1".to_string(), 100)]));
    }
    assert_eq!(std::fs::read(scratch.file(REGISTRY)).unwrap(), registry);
}

// 6. File type, owner and directory

#[test]
fn a_symlinked_fifo_or_directory_task_file_is_unavailable_without_following_or_blocking() {
    let scratch = Scratch::new("types");
    scratch.register(&[("p1", 100)]);
    let elsewhere = scratch.0.join("elsewhere.json");
    std::fs::write(&elsewhere, serde_json::to_vec(&one_task()).unwrap()).unwrap();
    let target = std::fs::read(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, scratch.file(TASKS)).unwrap();
    assert_eq!(list(&scratch.data()).unwrap_err(), Unavailable);
    assert_eq!(create(&scratch.data(), "p1", 100, "Title", "").unwrap_err(), Unavailable);
    assert_eq!(std::fs::read(&elsewhere).unwrap(), target, "a symlink is never followed");
    std::fs::remove_file(scratch.file(TASKS)).unwrap();

    mkfifo(&scratch.file(TASKS));
    let started = Instant::now();
    assert_eq!(list(&scratch.data()).unwrap_err(), Unavailable);
    assert!(started.elapsed() < Duration::from_secs(1), "a FIFO never blocks");
    std::fs::remove_file(scratch.file(TASKS)).unwrap();

    std::fs::create_dir(scratch.file(TASKS)).unwrap();
    assert_eq!(list(&scratch.data()).unwrap_err(), Unavailable);
    assert_eq!(create(&scratch.data(), "p1", 100, "Title", "").unwrap_err(), Unavailable);
    assert!(scratch.file(TASKS).is_dir());
}

#[test]
fn an_untrusted_or_symlinked_data_directory_is_unavailable() {
    let scratch = Scratch::new("untrusted");
    scratch.register(&[("p1", 100)]);
    created(create(&scratch.data(), "p1", 100, "Title", ""));
    let before = scratch.tasks();
    std::fs::set_permissions(scratch.data(), std::fs::Permissions::from_mode(0o770)).unwrap();
    assert_eq!(list(&scratch.data()).unwrap_err(), Unavailable);
    assert_eq!(get(&scratch.data(), "t1").unwrap_err(), Unavailable);
    assert_eq!(update(&scratch.data(), "t1", 1, "Title", "", "archived").unwrap_err(), Unavailable);
    // The registry is in the same directory and is refused as well, so no project can be confirmed.
    assert_eq!(create(&scratch.data(), "p1", 100, "Title", "").unwrap_err(), ProjectUnavailable);
    std::fs::set_permissions(scratch.data(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(scratch.tasks(), before);

    let link = scratch.0.join("linked-data");
    std::os::unix::fs::symlink(scratch.data(), &link).unwrap();
    assert_eq!(list(&link).unwrap_err(), Unavailable);
    assert_eq!(update(&link, "t1", 1, "Title", "", "archived").unwrap_err(), Unavailable);
    assert_eq!(scratch.tasks(), before);
}

#[test]
fn a_task_file_or_lock_owned_by_another_user_is_unavailable() {
    let foreign = Path::new("/private/etc/hosts");
    let scratch = Scratch::new("foreign-owner");
    scratch.register(&[("p1", 100)]);
    let unavailable = if euid() == 0 {
        Some("running as root, so no file is foreign-owned".to_string())
    } else {
        std::fs::hard_link(foreign, scratch.file(TASKS)).err().map(|e| format!("cannot link {foreign:?}: {e}"))
    };
    if let Some(reason) = unavailable {
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), "SKIPPED a_task_file_or_lock_owned_by_another_user: {reason}");
        assert!(std::env::var_os("CI").is_none(), "the foreign-owner fixture must run in CI: {reason}");
        return;
    }
    let before = (std::fs::read(foreign).unwrap(), modified(foreign));
    assert_ne!(std::fs::symlink_metadata(scratch.file(TASKS)).unwrap().uid(), euid());
    assert_eq!(list(&scratch.data()).unwrap_err(), Unavailable);
    assert_eq!(create(&scratch.data(), "p1", 100, "Title", "").unwrap_err(), Unavailable);
    std::fs::remove_file(scratch.file(TASKS)).unwrap();
    std::fs::remove_file(scratch.file(LOCK)).unwrap();
    std::fs::hard_link(foreign, scratch.file(LOCK)).unwrap();
    assert_eq!(create(&scratch.data(), "p1", 100, "Title", "").unwrap_err(), Unavailable);
    std::fs::remove_file(scratch.file(LOCK)).unwrap();
    assert!(!scratch.file(TASKS).exists());
    assert_eq!((std::fs::read(foreign).unwrap(), modified(foreign)), before);
}

#[test]
fn the_temp_name_never_redirects_a_write() {
    let scratch = Scratch::new("temp");
    scratch.register(&[("p1", 100)]);
    created(create(&scratch.data(), "p1", 100, "Title", ""));
    let sentinel = scratch.0.join("sentinel");
    std::fs::write(&sentinel, b"sentinel bytes").unwrap();
    std::os::unix::fs::symlink(&sentinel, scratch.file(TEMP)).unwrap();
    created(create(&scratch.data(), "p1", 100, "Second", ""));
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"sentinel bytes");
    assert!(!scratch.file(TEMP).exists());

    std::fs::create_dir(scratch.file(TEMP)).unwrap();
    let before = scratch.tasks();
    assert_eq!(create(&scratch.data(), "p1", 100, "Third", "").unwrap_err(), Unavailable);
    assert_eq!(scratch.tasks(), before);
}

// 7. Interrupted writes

#[test]
fn a_failure_before_the_rename_commits_nothing_and_the_next_write_succeeds() {
    let scratch = Scratch::new("fail-before");
    scratch.register(&[("p1", 100)]);
    created(create(&scratch.data(), "p1", 100, "First", ""));
    for step in [Step::TempWrite, Step::TempSync, Step::Rename] {
        let before = scratch.tasks();
        arm(step, libc::EIO);
        assert_eq!(create(&scratch.data(), "p1", 100, "Lost", "").unwrap_err(), Unavailable, "{step:?}");
        assert_eq!(scratch.tasks(), before, "{step:?}");
        assert!(!scratch.file(TEMP).exists(), "{step:?}");
        arm(step, libc::EIO);
        assert_eq!(update(&scratch.data(), "t1", 1, "Lost", "", "archived").unwrap_err(), Unavailable, "{step:?}");
        assert_eq!(scratch.tasks(), before, "{step:?}");
    }
    // The id that was never committed is used by the next successful write.
    assert_eq!(created(create(&scratch.data(), "p1", 100, "Second", "")).summary.id, "t2");
    assert_eq!(updated(update(&scratch.data(), "t1", 1, "First", "", "archived")).summary.revision, 2);
}

#[test]
fn a_failed_cleanup_leaves_a_stale_temp_file_that_is_never_read_and_the_next_write_removes() {
    let scratch = Scratch::new("fail-cleanup");
    scratch.register(&[("p1", 100)]);
    created(create(&scratch.data(), "p1", 100, "First", ""));
    arm(Step::TempSync, libc::EIO);
    arm(Step::TempCleanup, libc::EIO);
    assert_eq!(create(&scratch.data(), "p1", 100, "Lost", "").unwrap_err(), Unavailable);
    assert!(scratch.file(TEMP).exists());
    assert_eq!(list(&scratch.data()).unwrap().tasks.len(), 1, "the stale temp file is not read");
    created(create(&scratch.data(), "p1", 100, "Second", ""));
    assert!(!scratch.file(TEMP).exists());
}

#[test]
fn a_failed_directory_sync_after_the_rename_is_reported_but_the_change_is_committed() {
    let scratch = Scratch::new("fail-after");
    scratch.register(&[("p1", 100)]);
    arm(Step::CommitSync, libc::EIO);
    assert_eq!(create(&scratch.data(), "p1", 100, "First", "").unwrap_err(), Unavailable);
    assert_eq!(found(&scratch.data(), "t1").summary.title, "First", "committed at the rename");
    for unsupported in [libc::EINVAL, libc::ENOTSUP] {
        arm(Step::CommitSync, unsupported);
        created(create(&scratch.data(), "p1", 100, "Next", ""));
    }
    assert_eq!(list(&scratch.data()).unwrap().tasks.len(), 3);
}

// 8. Locking and concurrency

const CHILD_TASK: &str = "ORDELIS_TASKS_TEST_CHILD";
const CHILD_TEST: &str = "tasks::records::tests::child_process";
// libtest may print these on the same line as its own "test ... " header.
const LOCKED: &str = "ordelis-child-locked";
const READY: &str = "ordelis-child-ready";

/// Not a test of its own: the body runs only in a copy of this test binary started by the tests below.
#[test]
fn child_process() {
    let Ok(task) = std::env::var(CHILD_TASK) else { return };
    let mut lines = task.lines();
    let (mode, dir) = (lines.next().unwrap(), PathBuf::from(lines.next().unwrap()));
    let count: usize = lines.next().map_or(0, |n| n.parse().unwrap());
    let mut stdin = String::new();
    match mode {
        "hold" => {
            let _locked = lock(&dir, true).unwrap().unwrap();
            println!("{LOCKED}");
            std::io::stdin().read_to_string(&mut stdin).unwrap();
        }
        "create" => {
            println!("{READY}");
            std::io::stdin().read_line(&mut stdin).unwrap();
            for n in 0..count {
                created(create(&dir, "p1", 100, &format!("child {n}"), ""));
            }
        }
        "increment" => {
            println!("{READY}");
            std::io::stdin().read_line(&mut stdin).unwrap();
            increment(&dir, count);
        }
        _ => unreachable!(),
    }
}

/// A cleared environment: a runtime bridge test sets hostile process-wide variables (DYLD_INSERT_LIBRARIES among them)
/// and never unsets them, and a child that inherited them would not even start.
fn spawn_child(task: &str) -> (Child, impl Iterator<Item = String>) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .env_clear()
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

/// Read-modify-write of t1, `times` times, retrying on conflict: the pattern a client must follow.
fn increment(dir: &Path, times: usize) {
    for _ in 0..times {
        loop {
            let current = found(dir, "t1");
            match update(dir, "t1", current.summary.revision, &current.summary.title, &current.description, "draft") {
                Ok(UpdateOutcome::Updated { .. }) => break,
                Ok(UpdateOutcome::Conflict { .. }) | Err(Unavailable) => continue,
                other => panic!("unexpected {other:?}"),
            }
        }
    }
}

#[test]
fn concurrent_threads_and_a_second_process_all_create_tasks_with_unique_ids() {
    let scratch = Scratch::new("concurrent");
    scratch.register(&[("p1", 100)]);
    let (mut child, mut lines) = spawn_child(&format!("create\n{}\n8", scratch.data().display()));
    assert!(lines.any(|line| line.contains(READY)));
    use std::io::Write as _;
    child.stdin.as_mut().unwrap().write_all(b"go\n").unwrap();
    std::thread::scope(|threads| {
        for n in 0..8 {
            let data = scratch.data();
            threads.spawn(move || {
                for m in 0..2 {
                    created(create(&data, "p1", 100, &format!("thread {n}.{m}"), ""));
                }
            });
        }
    });
    assert!(child.wait().unwrap().success());
    let tasks = list(&scratch.data()).unwrap().tasks;
    let mut ids: Vec<String> = tasks.iter().map(|t| t.id.clone()).collect();
    ids.sort_by_key(|id| id[1..].parse::<u64>().unwrap());
    assert_eq!(ids, (1..=24).map(|n| format!("t{n}")).collect::<Vec<_>>());
    assert_eq!(tasks.iter().filter(|t| t.title.starts_with("child ")).count(), 8);
}

#[test]
fn concurrent_writers_that_retry_on_conflict_never_lose_an_update() {
    let scratch = Scratch::new("increments");
    scratch.register(&[("p1", 100)]);
    created(create(&scratch.data(), "p1", 100, "Counter", ""));
    let (mut child, mut lines) = spawn_child(&format!("increment\n{}\n10", scratch.data().display()));
    assert!(lines.any(|line| line.contains(READY)));
    use std::io::Write as _;
    child.stdin.as_mut().unwrap().write_all(b"go\n").unwrap();
    std::thread::scope(|threads| {
        for _ in 0..4 {
            let data = scratch.data();
            threads.spawn(move || increment(&data, 10));
        }
    });
    assert!(child.wait().unwrap().success());
    assert_eq!(found(&scratch.data(), "t1").summary.revision, 1 + 4 * 10 + 10);
}

#[test]
fn another_process_holding_the_tasks_lock_makes_writes_unavailable_within_the_budget() {
    let scratch = Scratch::new("lock-process");
    scratch.register(&[("p1", 100)]);
    created(create(&scratch.data(), "p1", 100, "Title", ""));
    let before = scratch.tasks();
    let (mut holder, mut lines) = spawn_child(&format!("hold\n{}", scratch.data().display()));
    assert!(lines.any(|line| line.contains(LOCKED)), "the child never took the lock");

    let started = Instant::now();
    assert_eq!(create(&scratch.data(), "p1", 100, "Blocked", "").unwrap_err(), Unavailable);
    let waited = started.elapsed();
    assert!(waited >= LOCK_BUDGET && waited < Duration::from_millis(2500), "{waited:?}");
    assert_eq!(update(&scratch.data(), "t1", 1, "Blocked", "", "draft").unwrap_err(), Unavailable);
    assert_eq!(scratch.tasks(), before);
    // Reads never take the lock.
    assert_eq!(found(&scratch.data(), "t1").summary.title, "Title");

    drop(holder.stdin.take());
    assert!(holder.wait().unwrap().success());
    assert_eq!(created(create(&scratch.data(), "p1", 100, "Free", "")).summary.id, "t2");
}

#[test]
fn the_task_and_project_locks_are_independent() {
    let scratch = Scratch::new("independent");
    scratch.register(&[("p1", 100)]);
    // The registry's lock held by another writer: creating and updating a task still succeed, because they only
    // read the registry and never take its lock (taking it would fail once the budget ran out). No timing bound
    // here: other tests share this process's task-store mutex.
    let projects = hold_flock(&scratch.file("projects.lock"));
    created(create(&scratch.data(), "p1", 100, "Title", ""));
    updated(update(&scratch.data(), "t1", 1, "Title", "", "archived"));
    drop(projects);

    // The tasks lock held: task writes wait out the budget, while the registry can still be read.
    let tasks = hold_flock(&scratch.file(LOCK));
    let started = Instant::now();
    assert_eq!(create(&scratch.data(), "p1", 100, "Blocked", "").unwrap_err(), Unavailable);
    assert!(started.elapsed() >= LOCK_BUDGET);
    assert_eq!(crate::projects::registrations(&scratch.data()), Some(vec![("p1".to_string(), 100)]));
    drop(tasks);
}

// 9. The I2 import and the project registry

#[test]
fn task_files_never_count_as_a_registry_for_the_one_time_import() {
    let scratch = Scratch::new("i2");
    // The data directory holds only task files, including a stale temp file.
    scratch.write(TASKS, &serde_json::to_vec(&one_task()).unwrap());
    scratch.write(LOCK, b"");
    scratch.write(TEMP, b"partial");
    let tasks_before: Vec<(String, Vec<u8>)> = [TASKS, LOCK, TEMP].iter().map(|n| (n.to_string(), std::fs::read(scratch.file(n)).unwrap())).collect();
    let legacy = scratch.0.join("dev.raioviajante.synapse");
    std::fs::DirBuilder::new().mode(0o700).create(&legacy).unwrap();
    let legacy_registry = serde_json::to_vec(&json!({
        "schema_version": 1, "next_id": 2,
        "projects": [{ "id": "p1", "path": "/ordelis-test-nonexistent/p1", "dev": 1, "ino": 2, "registered_at_ms": 100 }]
    }))
    .unwrap();
    std::fs::write(legacy.join(REGISTRY), &legacy_registry).unwrap();

    crate::projects::Projects::new(Some(scratch.data())).import_legacy();
    assert_eq!(crate::projects::registrations(&scratch.data()), Some(vec![("p1".to_string(), 100)]), "imported");
    for (name, bytes) in &tasks_before {
        assert_eq!(&std::fs::read(scratch.file(name)).unwrap(), bytes, "{name} untouched by the import");
    }
    assert_eq!(std::fs::read(legacy.join(REGISTRY)).unwrap(), legacy_registry, "copy-only");
    // The imported registration keeps its id and time, so the task's link survives the import.
    assert_eq!(links(&scratch.data()), [("t1".to_string(), Link::Linked)]);
}
