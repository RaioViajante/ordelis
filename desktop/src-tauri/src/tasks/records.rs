// The task store and its schema (see docs/implementation/desktop-tasks.md). Its only files are the fixed names below
// inside `dir`, the app data directory resolved by Tauri, read and written through crate::store under the tasks lock,
// which is separate from the project registry's. A task's project identity comes from a read of the registry
// (crate::projects::registrations), which takes no lock; nothing here looks at a project folder or takes a path from
// the webview.
use super::{CreateOutcome, GetOutcome, Link, ProjectLink, Status, Task, TaskErrorCode, TaskList, TaskSummary, UpdateOutcome};
use crate::store::{self, Files, Store};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const TASKS: &str = "tasks.json";
const LOCK: &str = "tasks.lock";
const TEMP: &str = "tasks.json.tmp";
const SCHEMA_VERSION: u64 = 1;
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TASKS: usize = 500;
const MAX_TITLE_BYTES: usize = 256;
/// Title, the two newlines that will separate it from the description, and the description: the task limit of
/// `mco context plan`, so a saved task can later be planned without being cut.
const MAX_TEXT_BYTES: usize = 4096;
const MAX_NEXT_ID: u64 = 10_000_000_000;
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

static WRITER: Mutex<()> = Mutex::new(());
static FILES: Files = Files { file: TASKS, lock: LOCK, temp: TEMP, writer: &WRITER };

/// The task store: its lock guards are `store::Locked<TaskStore>`, which cannot authorize a registry write.
pub(super) struct TaskStore;

impl Store for TaskStore {
    fn files() -> &'static Files {
        &FILES
    }
}

use TaskErrorCode::{InvalidInput, ProjectNotFound, ProjectUnavailable, TasksCorrupt, TasksFull, TasksNewerVersion, TasksUnavailable as Unavailable};

/// Every storage failure of the task store is tasks_unavailable.
impl From<store::Unavailable> for TaskErrorCode {
    fn from(_: store::Unavailable) -> Self {
        Unavailable
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    schema_version: u64,
    next_id: u64,
    tasks: Vec<Record>,
}

/// Schema version 1. There is deliberately no path, plan, run or authority field: a saved task carries none.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    project: ProjectRef,
    title: String,
    description: String,
    status: Status,
    created_at_ms: u64,
    updated_at_ms: u64,
    revision: u64,
}

/// The project's identity when the task was created: its registration id and registration time. Never changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectRef {
    id: String,
    registered_at_ms: u64,
}

/// Registered projects as (id, registration time); `None` when the registry cannot be read.
type Registered = Option<Vec<(String, u64)>>;

impl Stored {
    fn empty() -> Self {
        Stored { schema_version: SCHEMA_VERSION, next_id: 1, tasks: Vec::new() }
    }
}

impl Record {
    fn summary(&self, registered: &Registered) -> TaskSummary {
        TaskSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            status: self.status,
            project: ProjectLink { id: self.project.id.clone(), link: link(&self.project, registered) },
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.updated_at_ms,
            revision: self.revision,
        }
    }

    fn task(&self, registered: &Registered) -> Task {
        Task { summary: self.summary(registered), description: self.description.clone() }
    }
}

/// Never rebinds: a reused id with a different registration time is `replaced`, not the same project.
fn link(project: &ProjectRef, registered: &Registered) -> Link {
    let Some(registered) = registered else {
        return Link::Unknown;
    };
    match registered.iter().find(|(id, _)| *id == project.id) {
        None => Link::Removed,
        Some((_, at)) if *at == project.registered_at_ms => Link::Linked,
        Some(_) => Link::Replaced,
    }
}

// Commands

/// Never writes, and takes no lock.
pub(super) fn list(dir: &Path) -> Result<TaskList, TaskErrorCode> {
    let stored = load(dir)?;
    let registered = crate::projects::registrations(dir);
    Ok(TaskList { schema_version: 1, tasks: stored.tasks.iter().map(|r| r.summary(&registered)).collect() })
}

/// Never writes, and takes no lock.
pub(super) fn get(dir: &Path, id: &str) -> Result<GetOutcome, TaskErrorCode> {
    if id_number('t', id).is_none() {
        return Err(InvalidInput);
    }
    let stored = load(dir)?;
    let Some(record) = stored.tasks.iter().find(|r| r.id == id) else {
        return Ok(GetOutcome::NotFound);
    };
    Ok(GetOutcome::Found { task: record.task(&crate::projects::registrations(dir)) })
}

/// Everything is validated before any file is touched. The registry is read before the tasks lock is taken, and the
/// registry's own lock is never taken, so the two locks are never held together.
pub(super) fn create(dir: &Path, project_id: &str, title: &str, description: &str) -> Result<CreateOutcome, TaskErrorCode> {
    if id_number('p', project_id).is_none() || !valid_text(title, description) {
        return Err(InvalidInput);
    }
    let registered = crate::projects::registrations(dir).ok_or(ProjectUnavailable)?;
    let registered_at_ms = registered.iter().find(|(id, _)| id == project_id).map(|(_, at)| *at).ok_or(ProjectNotFound)?;
    let locked = lock(dir, true)?.ok_or(Unavailable)?;
    let mut stored = load(dir)?;
    // The task count is checked in commit, like the registry's record count.
    if stored.next_id >= MAX_NEXT_ID {
        return Err(TasksFull);
    }
    let now = now_ms()?;
    let record = Record {
        id: format!("t{}", stored.next_id),
        project: ProjectRef { id: project_id.to_string(), registered_at_ms },
        title: title.to_string(),
        description: description.to_string(),
        status: Status::Draft,
        created_at_ms: now,
        updated_at_ms: now,
        revision: 1,
    };
    stored.next_id += 1;
    stored.tasks.push(record.clone());
    commit(&locked, &stored)?;
    Ok(CreateOutcome::Created { task: record.task(&Some(registered)) })
}

/// Replaces the title, description and status of the task at `revision`. Any other revision is a conflict that
/// writes nothing and returns the current task. The project identity and creation time never change.
pub(super) fn update(
    dir: &Path,
    id: &str,
    revision: u64,
    title: &str,
    description: &str,
    status: &str,
) -> Result<UpdateOutcome, TaskErrorCode> {
    let status = match status {
        "draft" => Status::Draft,
        "archived" => Status::Archived,
        _ => return Err(InvalidInput),
    };
    if id_number('t', id).is_none() || !(1..=MAX_SAFE_INTEGER).contains(&revision) || !valid_text(title, description) {
        return Err(InvalidInput);
    }
    let registered = crate::projects::registrations(dir);
    let Some(locked) = lock(dir, false)? else {
        return Ok(UpdateOutcome::NotFound);
    };
    let mut stored = load(dir)?;
    let Some(record) = stored.tasks.iter_mut().find(|r| r.id == id) else {
        return Ok(UpdateOutcome::NotFound);
    };
    if record.revision != revision {
        return Ok(UpdateOutcome::Conflict { task: record.task(&registered) });
    }
    if record.revision >= MAX_SAFE_INTEGER {
        return Err(TasksFull);
    }
    record.title = title.to_string();
    record.description = description.to_string();
    record.status = status;
    // Never earlier than the last change, even if the clock went back, so created <= updated always holds.
    record.updated_at_ms = now_ms()?.max(record.updated_at_ms);
    record.revision += 1;
    let task = record.task(&registered);
    commit(&locked, &stored)?;
    Ok(UpdateOutcome::Updated { task })
}

fn now_ms() -> Result<u64, TaskErrorCode> {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| Unavailable)?.as_millis();
    u64::try_from(ms).ok().filter(|ms| *ms <= MAX_SAFE_INTEGER).ok_or(Unavailable)
}

// Validation

/// Two steps: the version alone, then the full schema with unknown fields denied at every level. serde's derive also
/// rejects duplicate fields and unknown statuses. Any violation makes the whole file corrupt; nothing is partially
/// trusted.
fn parse(bytes: &[u8]) -> Result<Stored, TaskErrorCode> {
    #[derive(Deserialize)]
    struct Probe {
        schema_version: u64,
    }
    match serde_json::from_slice::<Probe>(bytes) {
        Ok(probe) if probe.schema_version == SCHEMA_VERSION => {}
        Ok(probe) if probe.schema_version > SCHEMA_VERSION => return Err(TasksNewerVersion),
        _ => return Err(TasksCorrupt),
    }
    match serde_json::from_slice::<Stored>(bytes) {
        Ok(stored) if valid(&stored) => Ok(stored),
        _ => Err(TasksCorrupt),
    }
}

fn valid(stored: &Stored) -> bool {
    let mut ids = HashSet::new();
    (1..=MAX_NEXT_ID).contains(&stored.next_id)
        && stored.tasks.len() <= MAX_TASKS
        && stored.tasks.iter().all(|r| {
            id_number('t', &r.id).is_some_and(|n| n < stored.next_id)
                && ids.insert(r.id.as_str())
                && id_number('p', &r.project.id).is_some()
                && r.project.registered_at_ms <= MAX_SAFE_INTEGER
                && valid_text(&r.title, &r.description)
                && r.created_at_ms <= r.updated_at_ms
                && r.updated_at_ms <= MAX_SAFE_INTEGER
                && (1..=MAX_SAFE_INTEGER).contains(&r.revision)
        })
}

/// `prefix` followed by 1 to 10 digits without a leading zero: `t<n>` for tasks, `p<n>` for registry ids.
fn id_number(prefix: char, id: &str) -> Option<u64> {
    let digits = id.strip_prefix(prefix)?;
    let plain = (1..=10).contains(&digits.len()) && !digits.starts_with('0') && digits.bytes().all(|b| b.is_ascii_digit());
    plain.then(|| digits.parse().ok()).flatten()
}

/// The title has 1 to 256 bytes, no control character and no leading or trailing whitespace. The description may hold
/// `\n` and `\t` but no other control character. Together with the two separating newlines they fit in 4096 bytes.
/// Both are valid UTF-8 by construction: serde_json refuses invalid bytes and lone surrogates.
fn valid_text(title: &str, description: &str) -> bool {
    !title.is_empty()
        && title.len() <= MAX_TITLE_BYTES
        && title.trim().len() == title.len()
        && !title.chars().any(char::is_control)
        && !description.chars().any(|c| c.is_control() && c != '\n' && c != '\t')
        && title.len() + 2 + description.len() <= MAX_TEXT_BYTES
}

// Storage, through crate::store with the task store's own names.

/// I/O and permission failures are tasks_unavailable, never corruption. A missing directory or file is an empty list.
fn load(dir: &Path) -> Result<Stored, TaskErrorCode> {
    let Some((bytes, _)) = store::read::<TaskStore>(dir, MAX_BYTES)? else {
        return Ok(Stored::empty());
    };
    if bytes.len() as u64 > MAX_BYTES {
        return Err(TasksCorrupt);
    }
    parse(&bytes)
}

/// Steps 1–4 of the write protocol, held until dropped (step 12). See store::lock.
fn lock(dir: &Path, create: bool) -> Result<Option<store::Locked<TaskStore>>, TaskErrorCode> {
    Ok(store::lock::<TaskStore>(dir, create)?)
}

/// Steps 7–11, only under the tasks lock, in the directory it locked. Nothing larger than the limits is written.
fn commit(locked: &store::Locked<TaskStore>, stored: &Stored) -> Result<(), TaskErrorCode> {
    let bytes = serde_json::to_vec_pretty(stored).map_err(|_| Unavailable)?;
    if stored.tasks.len() > MAX_TASKS || bytes.len() as u64 > MAX_BYTES {
        return Err(TasksFull);
    }
    Ok(store::replace(locked, &bytes)?)
}

#[cfg(test)]
mod tests;
