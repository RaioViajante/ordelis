// The project registry and its file protocol (see "Storage" in docs/implementation/desktop-local-projects.md).
// Every file it creates, opens, renames or removes is one of the fixed names below inside `dir`, the app data
// directory resolved by Tauri; nothing here takes a path from the webview. On chosen and registered folders
// it only calls the metadata functions in folder.rs.
use super::folder::{self, Candidate, FileId};
use super::{AddOutcome, ErrorCode, Project, ProjectList, RemoveOutcome, ResetOutcome, Status};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
const LOCK_RETRY: Duration = Duration::from_millis(50);
const LOCK_BUDGET: Duration = Duration::from_secs(2);
const BACKUP_SUFFIXES: u32 = 15;

use ErrorCode::{RegistryCorrupt, RegistryFull, RegistryNewerVersion, RegistryUnavailable as Unavailable};

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

/// `picked` is the path the native panel returned. Nothing is created unless it passes the registration check.
pub(super) fn add(dir: &Path, picked: &Path) -> Result<AddOutcome, ErrorCode> {
    let candidate = folder::inspect(picked, valid_path)?;
    let _locked = lock(dir, true)?.ok_or(Unavailable)?;
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
    commit(dir, &stored)?;
    Ok(AddOutcome::Added { project: record.project(home.as_deref()) })
}

/// Removes the record only; the folder it names is never touched.
pub(super) fn remove(dir: &Path, id: &str) -> Result<RemoveOutcome, ErrorCode> {
    if id_number(id).is_none() {
        return Err(ErrorCode::InvalidInput);
    }
    let _locked = lock(dir, true)?.ok_or(Unavailable)?;
    let mut stored = load(dir)?;
    let Some(index) = stored.projects.iter().position(|r| r.id == id) else {
        return Ok(RemoveOutcome::NotFound);
    };
    stored.projects.remove(index);
    commit(dir, &stored)?;
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
    match std::fs::symlink_metadata(dir) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Loaded::Empty),
        Err(_) => return Err(Unavailable),
        Ok(meta) => trusted_dir(&meta)?,
    }
    let file = match open_registry(&dir.join(REGISTRY)) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Loaded::Empty),
        Err(_) => return Err(Unavailable),
        Ok(file) => file,
    };
    let meta = file.metadata().map_err(|_| Unavailable)?;
    owned_regular_file(&meta)?;
    let mut bytes = Vec::new();
    (&file).take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|_| Unavailable)?;
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

/// A symlink (ELOOP) or anything unexpected fails; O_NONBLOCK makes a FIFO return at once instead of waiting
/// for a writer. std adds O_CLOEXEC.
fn open_registry(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path)
}

fn euid() -> u32 {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() }
}

/// The directory itself, not a symlink, owned by this user and writable by nobody else.
fn trusted_dir(meta: &std::fs::Metadata) -> Result<(), ErrorCode> {
    let trusted = meta.is_dir() && meta.uid() == euid() && meta.mode() & 0o022 == 0;
    trusted.then_some(()).ok_or(Unavailable)
}

fn owned_regular_file(meta: &std::fs::Metadata) -> Result<(), ErrorCode> {
    (meta.is_file() && meta.uid() == euid()).then_some(()).ok_or(Unavailable)
}

// Write protocol

static WRITER: Mutex<()> = Mutex::new(());

/// Steps 1–4 of the write protocol, held until dropped. Dropping it is step 12: fields drop in declaration
/// order, so the descriptor closes (releasing the flock) before the mutex is released.
struct Locked {
    _file: File,
    _writer: MutexGuard<'static, ()>,
}

/// `None` only when `dir` is missing and `create` is false. Existing permissions are never changed.
fn lock(dir: &Path, create: bool) -> Result<Option<Locked>, ErrorCode> {
    // The mutex serializes this process's threads without spending the flock budget: a second descriptor in
    // the same process would also be refused by flock.
    let writer = WRITER.lock().unwrap_or_else(|e| e.into_inner());
    let meta = match std::fs::symlink_metadata(dir) {
        Err(e) if e.kind() == ErrorKind::NotFound && !create => return Ok(None),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            // Not recursive: the parent must already exist.
            match DirBuilder::new().mode(0o700).create(dir) {
                Err(e) if e.kind() != ErrorKind::AlreadyExists => return Err(Unavailable),
                _ => std::fs::symlink_metadata(dir),
            }
        }
        other => other,
    }
    .map_err(|_| Unavailable)?;
    trusted_dir(&meta)?;
    let file = open_lock(&dir.join(LOCK)).map_err(|_| Unavailable)?;
    owned_regular_file(&file.metadata().map_err(|_| Unavailable)?)?;
    let deadline = Instant::now() + LOCK_BUDGET;
    loop {
        // SAFETY: flock on a descriptor owned by `file`. Closing it releases the lock, also when the process dies.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(Some(Locked { _file: file, _writer: writer }));
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EWOULDBLOCK | libc::EINTR) if Instant::now() < deadline => std::thread::sleep(LOCK_RETRY),
            _ => return Err(Unavailable),
        }
    }
}

/// The stable sidecar lock file: created once, never written, renamed or removed.
fn open_lock(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(path)
}

/// Steps 7–11. Only a temp file this call created exclusively and synced is ever renamed over the registry.
fn commit(dir: &Path, stored: &Stored) -> Result<(), ErrorCode> {
    let bytes = serde_json::to_vec_pretty(stored).map_err(|_| Unavailable)?;
    if stored.projects.len() > MAX_RECORDS || bytes.len() as u64 > MAX_BYTES {
        return Err(RegistryFull);
    }
    let temp = dir.join(TEMP);
    // unlink removes a symlink itself, never its target, and fails on a directory.
    match std::fs::remove_file(&temp) {
        Err(e) if e.kind() != ErrorKind::NotFound => return Err(Unavailable),
        _ => {}
    }
    let file = create_temp(&temp).map_err(|_| Unavailable)?;
    let replaced = write_synced(file, &bytes).and_then(|()| {
        fault(Step::Rename)?;
        std::fs::rename(&temp, dir.join(REGISTRY))
    });
    if replaced.is_err() {
        // If this cleanup fails too, the stale temp file is never read or promoted; the next write removes it.
        let _ = fault(Step::TempCleanup).and_then(|()| std::fs::remove_file(&temp));
        return Err(Unavailable);
    }
    // The rename is the commit point: every process now sees the new registry. A failure below is still an
    // error, and the frontend re-lists after any mutation error instead of assuming nothing changed.
    sync_dir(dir, Step::CommitSync)
}

/// O_EXCL fails on any existing name, including a symlink.
fn create_temp(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(path)
}

/// On Apple targets File::sync_all issues fcntl(F_FULLFSYNC), which asks the drive to flush its cache too.
/// The file is closed when it goes out of scope.
fn write_synced(mut file: File, bytes: &[u8]) -> std::io::Result<()> {
    fault(Step::TempWrite)?;
    file.write_all(bytes)?;
    fault(Step::TempSync)?;
    file.sync_all()
}

/// Best effort where the filesystem does not support it (EINVAL, ENOTSUP); any other failure is an error.
fn sync_dir(dir: &Path, step: Step) -> Result<(), ErrorCode> {
    match fault(step).and_then(|()| open_dir(dir)?.sync_all()) {
        Err(e) if !matches!(e.raw_os_error(), Some(libc::EINVAL | libc::ENOTSUP)) => Err(Unavailable),
        _ => Ok(()),
    }
}

fn open_dir(dir: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW).open(dir)
}

// Failure injection. Production builds compile `fault` to nothing; the test build arms it per thread from
// tests.rs, so it cannot be reached through IPC (commands run registry work on other threads).

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Step {
    TempWrite,
    TempSync,
    Rename,
    TempCleanup,
    CommitSync,
    BackupLink,
    BackupCheck,
    BackupSync,
    ResetUnlink,
    ResetSync,
}

#[cfg(not(test))]
fn fault(_: Step) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn fault(step: Step) -> std::io::Result<()> {
    tests::fault(step)
}

#[cfg(test)]
mod tests;
