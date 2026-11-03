// The reviewed file protocol behind the project registry (see "Storage" in docs/implementation/desktop-local-projects.md):
// the directory and file checks, the read protocol, the lock and the atomic replace. A store is one file and its
// sidecar names, all fixed by the module that owns it, inside a directory Rust resolved; nothing here takes a name
// or a path from the webview. It knows nothing of what the file holds: parsing, validation, limits and what each
// failure means belong to the owner. Every failure here is `Unavailable`.
use std::fs::{DirBuilder, File, Metadata, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub(crate) const LOCK_RETRY: Duration = Duration::from_millis(50);
pub(crate) const LOCK_BUDGET: Duration = Duration::from_secs(2);

/// Any storage failure. The owner maps it to its own error code.
#[derive(Debug)]
pub(crate) struct Unavailable;

/// The names of one store inside its directory, each a constant of the module that owns it.
pub(crate) struct Files {
    /// The stored file. Only `replace` changes it.
    pub(crate) file: &'static str,
    /// The stable sidecar lock file: created once, never written, renamed or removed.
    pub(crate) lock: &'static str,
    /// The temporary file of an in-progress write; never read, and never promoted unless the same write created and
    /// synced it.
    pub(crate) temp: &'static str,
    /// Serializes this store's writers within the process.
    pub(crate) writer: &'static Mutex<()>,
}

// Read protocol: never creates, writes, renames, truncates or changes permissions of anything.

/// `None` when the directory or the file does not exist. Otherwise at most `max + 1` bytes, so the owner can tell
/// an oversized file, with the metadata of the descriptor they were read from. I/O and permission failures are
/// `Unavailable`, never a judgement about the contents.
pub(crate) fn read(dir: &Path, files: &Files, max: u64) -> Result<Option<(Vec<u8>, Metadata)>, Unavailable> {
    match std::fs::symlink_metadata(dir) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Unavailable),
        Ok(meta) => trusted_dir(&meta)?,
    }
    let file = match open_read(&dir.join(files.file)) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Unavailable),
        Ok(file) => file,
    };
    let meta = file.metadata().map_err(|_| Unavailable)?;
    owned_regular_file(&meta)?;
    let mut bytes = Vec::new();
    (&file).take(max + 1).read_to_end(&mut bytes).map_err(|_| Unavailable)?;
    Ok(Some((bytes, meta)))
}

/// A symlink (ELOOP) or anything unexpected fails; O_NONBLOCK makes a FIFO return at once instead of waiting
/// for a writer. std adds O_CLOEXEC.
fn open_read(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path)
}

pub(crate) fn euid() -> u32 {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() }
}

/// The directory itself, not a symlink, owned by this user and writable by nobody else.
pub(crate) fn trusted_dir(meta: &Metadata) -> Result<(), Unavailable> {
    let trusted = meta.is_dir() && meta.uid() == euid() && meta.mode() & 0o022 == 0;
    trusted.then_some(()).ok_or(Unavailable)
}

fn owned_regular_file(meta: &Metadata) -> Result<(), Unavailable> {
    (meta.is_file() && meta.uid() == euid()).then_some(()).ok_or(Unavailable)
}

// Write protocol

/// Steps 1–4 of the write protocol, held until dropped. Dropping it is step 12: fields drop in declaration
/// order, so the descriptor closes (releasing the flock) before the mutex is released.
pub(crate) struct Locked {
    _file: File,
    _writer: MutexGuard<'static, ()>,
}

/// `None` only when `dir` is missing and `create` is false. Existing permissions are never changed.
pub(crate) fn lock(dir: &Path, files: &Files, create: bool) -> Result<Option<Locked>, Unavailable> {
    // The mutex serializes this process's threads without spending the flock budget: a second descriptor in
    // the same process would also be refused by flock.
    let writer = files.writer.lock().unwrap_or_else(|e| e.into_inner());
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
    let file = open_lock(&dir.join(files.lock)).map_err(|_| Unavailable)?;
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

/// Steps 8–11, called by the owner with the store's lock held and `bytes` already checked against its limits.
/// Only a temp file this call created exclusively and synced is ever renamed over the stored file.
pub(crate) fn replace(dir: &Path, files: &Files, bytes: &[u8]) -> Result<(), Unavailable> {
    let temp = dir.join(files.temp);
    // unlink removes a symlink itself, never its target, and fails on a directory.
    match std::fs::remove_file(&temp) {
        Err(e) if e.kind() != ErrorKind::NotFound => return Err(Unavailable),
        _ => {}
    }
    let file = create_temp(&temp).map_err(|_| Unavailable)?;
    let replaced = write_synced(file, bytes).and_then(|()| {
        fault(Step::Rename)?;
        std::fs::rename(&temp, dir.join(files.file))
    });
    if replaced.is_err() {
        // If this cleanup fails too, the stale temp file is never read or promoted; the next write removes it.
        let _ = fault(Step::TempCleanup).and_then(|()| std::fs::remove_file(&temp));
        return Err(Unavailable);
    }
    // The rename is the commit point: every process now sees the new file. A failure below is still an error, and
    // callers re-read after any mutation error instead of assuming nothing changed.
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
pub(crate) fn sync_dir(dir: &Path, step: Step) -> Result<(), Unavailable> {
    match fault(step).and_then(|()| open_dir(dir)?.sync_all()) {
        Err(e) if !matches!(e.raw_os_error(), Some(libc::EINVAL | libc::ENOTSUP)) => Err(Unavailable),
        _ => Ok(()),
    }
}

fn open_dir(dir: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW).open(dir)
}

// Failure injection. Production builds compile `fault` to nothing; the test build arms it per thread with `arm`,
// so it cannot be reached through IPC (commands run storage work on other threads). The backup and reset steps
// belong to the registry's reset, which injects them here too.

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Step {
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
pub(crate) fn fault(_: Step) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
thread_local! {
    static FAULTS: std::cell::RefCell<std::collections::HashMap<Step, i32>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// One-shot faults armed on the calling thread only.
#[cfg(test)]
pub(crate) fn arm(step: Step, errno: i32) {
    FAULTS.with(|faults| faults.borrow_mut().insert(step, errno));
}

#[cfg(test)]
pub(crate) fn fault(step: Step) -> std::io::Result<()> {
    match FAULTS.with(|faults| faults.borrow_mut().remove(&step)) {
        Some(errno) => Err(std::io::Error::from_raw_os_error(errno)),
        None => Ok(()),
    }
}
