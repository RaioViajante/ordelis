// What Synapse looks at on a chosen or registered folder: its canonical path and lstat metadata. Nothing here
// opens, lists or reads a folder. Every check describes one instant and is no defense against the folder
// being replaced right afterwards (see "Project identity" in the architecture document).
use super::{ErrorCode, Status};
use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Device and inode as observed by lstat: a staleness hint, never a permanent proof of identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FileId {
    pub dev: u64,
    pub ino: u64,
}

impl FileId {
    pub fn of(meta: &std::fs::Metadata) -> Self {
        FileId { dev: meta.dev(), ino: meta.ino() }
    }
}

/// A chosen folder that passed the registration check.
#[derive(Debug)]
pub(super) struct Candidate {
    pub path: String,
    pub id: FileId,
}

/// Steps 1–3 of the registration check: canonicalize, lstat, refusal policy. `storable` is the registry's own
/// path rule, so a folder whose canonical path the registry would reject on load is never written.
pub(super) fn inspect(picked: &Path, storable: impl Fn(&str) -> bool) -> Result<Candidate, ErrorCode> {
    let canonical = std::fs::canonicalize(picked).map_err(|e| lookup_error(&e))?;
    let path = canonical.into_os_string().into_string().map_err(|_| ErrorCode::PathUnsupported)?;
    let id = directory_id(&path)?;
    if refused(id, home().as_deref()) {
        return Err(ErrorCode::FolderRefused);
    }
    if !storable(&path) {
        return Err(ErrorCode::PathUnsupported);
    }
    Ok(Candidate { path, id })
}

/// Step 5, under the registry lock and immediately before writing: the folder must still be the directory
/// that was checked. This catches a visible change; it does not close the race.
pub(super) fn recheck(candidate: &Candidate) -> Result<(), ErrorCode> {
    match directory_id(&candidate.path) {
        Ok(id) if id == candidate.id => Ok(()),
        Ok(_) | Err(ErrorCode::NotADirectory) => Err(ErrorCode::FolderChanged),
        Err(code) => Err(code),
    }
}

fn directory_id(path: &str) -> Result<FileId, ErrorCode> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| lookup_error(&e))?;
    if meta.file_type().is_symlink() {
        // A canonical path names no symlink, so one appearing here was swapped in after canonicalize.
        return Err(ErrorCode::FolderChanged);
    }
    if !meta.is_dir() {
        return Err(ErrorCode::NotADirectory);
    }
    Ok(FileId::of(&meta))
}

fn lookup_error(error: &std::io::Error) -> ErrorCode {
    match error.raw_os_error() {
        Some(libc::ENOENT | libc::ENOTDIR) => ErrorCode::FolderChanged,
        _ => ErrorCode::FolderInaccessible,
    }
}

/// Refuses `/`, the data volume root, the home directory and every ancestor of it, compared by identity
/// because firmlinks give one directory two canonical spellings. Fails closed without a resolvable home.
pub(super) fn refused(candidate: FileId, home: Option<&Path>) -> bool {
    let Some(home) = home.and_then(|h| std::fs::canonicalize(h).ok()) else {
        return true;
    };
    let identity = |path: &Path| std::fs::symlink_metadata(path).map(|meta| FileId::of(&meta));
    // The data volume root exists only on split system/data layouts.
    if identity(Path::new("/System/Volumes/Data")).is_ok_and(|id| id == candidate) {
        return true;
    }
    // ancestors() yields the home directory itself first and "/" last.
    home.ancestors().any(|dir| identity(dir).map_or(true, |id| id == candidate))
}

/// Live status of a stored registration, from lstat of its path. Computed on every list, never written back.
pub(super) fn status(path: &str, stored: FileId) -> Status {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && FileId::of(&meta) == stored => Status::Available,
        Ok(_) => Status::Changed,
        Err(e) if matches!(e.raw_os_error(), Some(libc::ENOENT | libc::ENOTDIR)) => Status::Missing,
        // EACCES, EPERM, and anything else (a network volume timing out): never retried or prompted.
        Err(_) => Status::Inaccessible,
    }
}

/// The final path component, derived for display only.
pub(super) fn name(path: &str) -> String {
    Path::new(path).file_name().and_then(OsStr::to_str).unwrap_or(path).to_string()
}

/// Replaces the home prefix with `~`, only at a component boundary (`/Users/me2` stays as it is).
pub(super) fn display(path: &str, home: Option<&str>) -> String {
    match home.and_then(|home| path.strip_prefix(home)) {
        Some("") => "~".to_string(),
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// The canonical home directory as a string, for display.
pub(super) fn display_home() -> Option<String> {
    std::fs::canonicalize(home()?).ok()?.into_os_string().into_string().ok()
}

/// The effective user's home directory from the password database, never from `$HOME`.
pub(super) fn home() -> Option<PathBuf> {
    let mut size = 4096;
    loop {
        let mut buffer = vec![0 as libc::c_char; size];
        // SAFETY: passwd is plain data that getpwuid_r fills in; result is null or points to it.
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: every pointer is valid for the call and buffer.len() is its real length.
        let rc = unsafe { libc::getpwuid_r(libc::geteuid(), &mut entry, buffer.as_mut_ptr(), buffer.len(), &mut result) };
        if rc == libc::ERANGE && size < 1 << 16 {
            size *= 2;
            continue;
        }
        if rc != 0 || result.is_null() || entry.pw_dir.is_null() {
            return None;
        }
        // SAFETY: pw_dir is a NUL-terminated string inside `buffer`, which is still alive.
        let dir = unsafe { CStr::from_ptr(entry.pw_dir) };
        let dir = PathBuf::from(OsStr::from_bytes(dir.to_bytes()));
        return dir.is_absolute().then_some(dir);
    }
}
