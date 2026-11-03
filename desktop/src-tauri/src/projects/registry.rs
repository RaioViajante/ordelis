// The project registry and its file protocol (see "Storage" in docs/implementation/desktop-local-projects.md).
// Every file it creates, opens, renames or removes is one of the fixed names below inside `dir`, the app data
// directory resolved by Tauri; nothing here takes a path from the webview. The read, lock and atomic-replace
// steps are in crate::store, which receives only these names. On chosen and registered folders it only calls
// the metadata functions in folder.rs.
use super::folder::{self, Candidate, FileId};
use super::{AddOutcome, ErrorCode, Project, ProjectList, RemoveOutcome, ResetOutcome, Status};
use crate::store::{self, fault, sync_dir, trusted_dir, Files, Step, Store};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const REGISTRY: &str = "projects.json";
const LOCK: &str = "projects.lock";
const TEMP: &str = "projects.json.tmp";
const BACKUP_PREFIX: &str = "projects.json.corrupt-";
const SCHEMA_VERSION: u64 = 1;
const MAX_BYTES: u64 = 256 * 1024;
const MAX_RECORDS: usize = 256;
const MAX_PATH_BYTES: usize = 1024;
const MAX_NEXT_ID: u64 = 10_000_000_000;
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
const BACKUP_SUFFIXES: u32 = 15;

static WRITER: Mutex<()> = Mutex::new(());
static FILES: Files = Files { file: REGISTRY, lock: LOCK, temp: TEMP, writer: &WRITER };

/// The registry's store: its lock guards are `store::Locked<Registry>`.
pub(super) struct Registry;

impl Store for Registry {
    fn files() -> &'static Files {
        &FILES
    }
}

use ErrorCode::{RegistryCorrupt, RegistryFull, RegistryNewerVersion, RegistryUnavailable as Unavailable};

/// Every storage failure of the registry is registry_unavailable.
impl From<store::Unavailable> for ErrorCode {
    fn from(_: store::Unavailable) -> Self {
        Unavailable
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    schema_version: u64,
    next_id: u64,
    projects: Vec<Record>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    path: String,
    dev: u64,
    ino: u64,
    registered_at_ms: u64,
}

impl Stored {
    fn empty() -> Self {
        Stored { schema_version: SCHEMA_VERSION, next_id: 1, projects: Vec::new() }
    }
}

impl Record {
    fn file_id(&self) -> FileId {
        FileId { dev: self.dev, ino: self.ino }
    }

    fn project(&self, home: Option<&str>) -> Project {
        Project {
            id: self.id.clone(),
            name: folder::name(&self.path),
            path_display: folder::display(&self.path, home),
            status: folder::status(&self.path, self.file_id()),
            registered_at_ms: self.registered_at_ms,
        }
    }
}

// Commands

pub(super) fn list(dir: &Path) -> Result<ProjectList, ErrorCode> {
    let stored = load(dir)?;
    let home = folder::display_home();
    Ok(ProjectList { schema_version: 1, projects: stored.projects.iter().map(|r| r.project(home.as_deref())).collect() })
}

/// Each record's id and registration time, for the task store's project links (see
/// docs/implementation/desktop-tasks.md). The read protocol only: no lock, no write, and no look at any folder.
pub(super) fn registrations(dir: &Path) -> Result<Vec<(String, u64)>, ErrorCode> {
    Ok(load(dir)?.projects.into_iter().map(|r| (r.id, r.registered_at_ms)).collect())
}

/// `picked` is the path the native panel returned. Nothing is created unless it passes the registration check.
pub(super) fn add(dir: &Path, picked: &Path) -> Result<AddOutcome, ErrorCode> {
    let candidate = folder::inspect(picked, valid_path)?;
    let locked = lock(dir, true)?.ok_or(Unavailable)?;
    let mut stored = load(dir)?;
    let home = folder::display_home();
    if let Some(existing) = duplicate(&stored, &candidate) {
        return Ok(AddOutcome::AlreadyRegistered { project: existing.project(home.as_deref()) });
    }
    if stored.next_id >= MAX_NEXT_ID {
        return Err(RegistryFull);
    }
    let record = Record {
        id: format!("p{}", stored.next_id),
        path: candidate.path.clone(),
        dev: candidate.id.dev,
        ino: candidate.id.ino,
        registered_at_ms: now_ms()?,
    };
    stored.next_id += 1;
    stored.projects.push(record.clone());
    // Test builds only: lets a test change the folder between the check above and this recheck.
    #[cfg(test)]
    tests::before_recheck();
    folder::recheck(&candidate)?;
    commit(&locked, &stored)?;
    Ok(AddOutcome::Added { project: record.project(home.as_deref()) })
}

/// Removes the record only; the folder it names is never touched.
pub(super) fn remove(dir: &Path, id: &str) -> Result<RemoveOutcome, ErrorCode> {
    if id_number(id).is_none() {
        return Err(ErrorCode::InvalidInput);
    }
    let locked = lock(dir, true)?.ok_or(Unavailable)?;
    let mut stored = load(dir)?;
    let Some(index) = stored.projects.iter().position(|r| r.id == id) else {
        return Ok(RemoveOutcome::NotFound);
    };
    stored.projects.remove(index);
    commit(&locked, &stored)?;
    Ok(RemoveOutcome::Removed)
}

/// Explicit recovery from a corrupt registry: the corrupt file is kept under a new name and the list starts
/// empty. Anything other than a corrupt file under the lock is left untouched, whoever calls this.
pub(super) fn reset(dir: &Path) -> Result<ResetOutcome, ErrorCode> {
    let Some(_locked) = lock(dir, false)? else {
        return Ok(ResetOutcome::NotNeeded);
    };
    // Decided from the file as it is now, never from an earlier list or an in-memory flag.
    let found = match read(dir)? {
        Loaded::Empty | Loaded::Valid(_) => return Ok(ResetOutcome::NotNeeded),
        Loaded::Newer => return Err(RegistryNewerVersion),
        Loaded::Corrupt(found) => found,
    };
    let registry = dir.join(REGISTRY);
    let backup = preserve(&registry, dir, now_ms()?)?;
    // Both names must be the very file that was found corrupt before the registry name is removed.
    let same = |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && FileId::of(&m) == found);
    if fault(Step::BackupCheck).is_err() || !same(&registry) || !same(&backup) {
        return Err(Unavailable);
    }
    sync_dir(dir, Step::BackupSync)?;
    fault(Step::ResetUnlink).and_then(|()| std::fs::remove_file(&registry)).map_err(|_| Unavailable)?;
    // Committed: the registry is empty (absent) and the backup holds the corrupt bytes.
    sync_dir(dir, Step::ResetSync)?;
    Ok(ResetOutcome::Reset)
}

/// A new name for the corrupt file in the same directory. link(2) never overwrites an existing name.
fn preserve(registry: &Path, dir: &Path, stamp: u64) -> Result<PathBuf, ErrorCode> {
    for suffix in 0..=BACKUP_SUFFIXES {
        let name = match suffix {
            0 => format!("{BACKUP_PREFIX}{stamp}"),
            n => format!("{BACKUP_PREFIX}{stamp}-{n}"),
        };
        let backup = dir.join(name);
        match fault(Step::BackupLink).and_then(|()| std::fs::hard_link(registry, &backup)) {
            Ok(()) => return Ok(backup),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            // Including a filesystem without hard links: there is no copy fallback.
            Err(_) => return Err(Unavailable),
        }
    }
    Err(Unavailable)
}

/// Rule 1: the same canonical path. Rule 2: the same stored (dev, ino), confirmed by lstat of that record's own
/// path right now, so a stale record whose inode was reused by another folder never matches.
fn duplicate<'a>(stored: &'a Stored, candidate: &Candidate) -> Option<&'a Record> {
    stored.projects.iter().find(|r| r.path == candidate.path).or_else(|| {
        stored
            .projects
            .iter()
            .find(|r| r.file_id() == candidate.id && folder::status(&r.path, r.file_id()) == Status::Available)
    })
}

fn now_ms() -> Result<u64, ErrorCode> {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| Unavailable)?.as_millis();
    u64::try_from(ms).ok().filter(|ms| *ms <= MAX_SAFE_INTEGER).ok_or(Unavailable)
}

// One-time import from the former bundle identifier's data directory (see "Registry migration semantics (I2)"
// in docs/migration/ordelis.md).

/// What the import did. Every outcome leaves both directories consistent; startup continues after any of them.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Import {
    /// The legacy registry was committed into `dir`, records, ids and `next_id` exactly as validated.
    Imported,
    /// The legacy directory holds no valid version-1 registry (absent, corrupt, newer, untrusted or unreadable).
    /// Nothing was created or changed.
    NoLegacy,
    /// `dir` already holds a registry or a corrupt-registry backup, which always takes precedence.
    Superseded,
    /// The lock or the write failed. Before the rename nothing was committed and the next launch tries again;
    /// after it, the import is complete and the next launch finds the registry.
    Failed,
}

/// Copy-only. `legacy` is derived in Rust from a fixed name, never from the webview; nothing in it is created,
/// written, locked, renamed or removed. Never merges: the import is whole into a directory with no registry, or
/// nothing.
pub(super) fn import(dir: &Path, legacy: &Path) -> Import {
    // 1. The read protocol creates nothing. Anything but a valid version-1 registry ends the import here, before
    // `dir` is created or touched.
    let Ok(Loaded::Valid(stored)) = read(legacy) else {
        return Import::NoLegacy;
    };
    // 2. A fast path without the lock; not authoritative.
    if occupied(dir) {
        return Import::Superseded;
    }
    // 3. Creates `dir` if absent, exactly as every write does. This creates projects.lock, which is therefore not a
    // precedence signal.
    let Ok(Some(locked)) = lock(dir, true) else {
        return Import::Failed;
    };
    // 4. The decisive check, under the lock: an add or another instance's import that committed first wins.
    if occupied(dir) {
        return Import::Superseded;
    }
    // 5. The atomic write path, which also removes a stale projects.json.tmp.
    match commit(&locked, &stored) {
        Ok(()) => Import::Imported,
        Err(_) => Import::Failed,
    }
}

/// The only precedence signals: projects.json in any form (a file, symlink, FIFO or directory), or any
/// corrupt-registry backup. A lookup that fails other than with "not found" counts as present.
fn occupied(dir: &Path) -> bool {
    let registry = !matches!(std::fs::symlink_metadata(dir.join(REGISTRY)), Err(e) if e.kind() == ErrorKind::NotFound);
    registry || backup_present(dir)
}

/// Criterion 21's one reviewed exception: the only directory listing in the registry module. It lists `dir`, the
/// app data directory, and nothing else; it reads entry names only, and never opens, stats, parses, changes or
/// removes an entry. Every doubt answers true: an untrusted directory, an entry that cannot be read, or a
/// directory that is no longer the one that was checked once the names have been read.
fn backup_present(dir: &Path) -> bool {
    let before = match std::fs::symlink_metadata(dir) {
        Err(e) if e.kind() == ErrorKind::NotFound => return false,
        Err(_) => return true,
        Ok(meta) => meta,
    };
    if trusted_dir(&before).is_err() {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return true;
    };
    for entry in entries {
        match entry {
            Ok(entry) if !entry.file_name().as_bytes().starts_with(BACKUP_PREFIX.as_bytes()) => {}
            _ => return true,
        }
    }
    // Test builds only: lets a test replace the directory after its names were read.
    #[cfg(test)]
    tests::after_backup_scan();
    // The names must have come from the directory that was checked: still trusted, and the same (dev, ino).
    let same = std::fs::symlink_metadata(dir)
        .is_ok_and(|after| trusted_dir(&after).is_ok() && FileId::of(&after) == FileId::of(&before));
    !same
}

// Validation

enum Parsed {
    Valid(Stored),
    Corrupt,
    Newer,
}

/// Two steps: the version alone, then the full schema with unknown fields denied at every level. serde's
/// derive also rejects duplicate fields. Any violation makes the whole file corrupt; nothing is partially trusted.
fn parse(bytes: &[u8]) -> Parsed {
    #[derive(Deserialize)]
    struct Probe {
        schema_version: u64,
    }
    match serde_json::from_slice::<Probe>(bytes) {
        Ok(probe) if probe.schema_version == SCHEMA_VERSION => {}
        Ok(probe) if probe.schema_version > SCHEMA_VERSION => return Parsed::Newer,
        _ => return Parsed::Corrupt,
    }
    match serde_json::from_slice::<Stored>(bytes) {
        Ok(stored) if valid(&stored) => Parsed::Valid(stored),
        _ => Parsed::Corrupt,
    }
}

fn valid(stored: &Stored) -> bool {
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    (1..=MAX_NEXT_ID).contains(&stored.next_id)
        && stored.projects.len() <= MAX_RECORDS
        && stored.projects.iter().all(|r| {
            id_number(&r.id).is_some_and(|n| n < stored.next_id)
                && ids.insert(r.id.as_str())
                && valid_path(&r.path)
                && paths.insert(r.path.as_str())
                && r.registered_at_ms <= MAX_SAFE_INTEGER
        })
    // Two records may share (dev, ino): a stale record and a later folder that reuses its inode.
}

/// `p` followed by 1 to 10 digits without a leading zero.
fn id_number(id: &str) -> Option<u64> {
    let digits = id.strip_prefix('p')?;
    let plain = (1..=10).contains(&digits.len()) && !digits.starts_with('0') && digits.bytes().all(|b| b.is_ascii_digit());
    plain.then(|| digits.parse().ok()).flatten()
}

/// Absolute, normalized (a root followed only by normal components), not `/`, at most 1024 bytes, and free of
/// control characters.
fn valid_path(path: &str) -> bool {
    path.len() <= MAX_PATH_BYTES
        && !path.chars().any(char::is_control)
        && path.len() > 1
        && path.starts_with('/')
        && path[1..].split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

// Read protocol: never creates, writes, renames, truncates or changes permissions of anything.

enum Loaded {
    Empty,
    Valid(Stored),
    Corrupt(FileId),
    Newer,
}

fn load(dir: &Path) -> Result<Stored, ErrorCode> {
    match read(dir)? {
        Loaded::Empty => Ok(Stored::empty()),
        Loaded::Valid(stored) => Ok(stored),
        Loaded::Corrupt(_) => Err(RegistryCorrupt),
        Loaded::Newer => Err(RegistryNewerVersion),
    }
}

/// I/O and permission failures are `registry_unavailable`, never corruption.
fn read(dir: &Path) -> Result<Loaded, ErrorCode> {
    let Some((bytes, meta)) = store::read::<Registry>(dir, MAX_BYTES)? else {
        return Ok(Loaded::Empty);
    };
    let found = FileId::of(&meta);
    if bytes.len() as u64 > MAX_BYTES {
        return Ok(Loaded::Corrupt(found));
    }
    Ok(match parse(&bytes) {
        Parsed::Valid(stored) => Loaded::Valid(stored),
        Parsed::Corrupt => Loaded::Corrupt(found),
        Parsed::Newer => Loaded::Newer,
    })
}

// Write protocol, through crate::store with the registry's own names.

/// Steps 1–4 of the write protocol, held until dropped (step 12). See store::lock.
fn lock(dir: &Path, create: bool) -> Result<Option<store::Locked<Registry>>, ErrorCode> {
    Ok(store::lock::<Registry>(dir, create)?)
}

/// Steps 7–11, only under the registry's own lock, in the directory it locked. Only a temp file this call created
/// exclusively and synced is ever renamed over the registry.
fn commit(locked: &store::Locked<Registry>, stored: &Stored) -> Result<(), ErrorCode> {
    let bytes = serde_json::to_vec_pretty(stored).map_err(|_| Unavailable)?;
    if stored.projects.len() > MAX_RECORDS || bytes.len() as u64 > MAX_BYTES {
        return Err(RegistryFull);
    }
    Ok(store::replace(locked, &bytes)?)
}

#[cfg(test)]
mod tests;
