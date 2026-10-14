// Local project registration (see docs/implementation/desktop-local-projects.md). A registration is a
// bookmark: Rust opens the folder panel, checks the chosen folder's metadata and remembers its path. Nothing
// here lists, reads or runs anything inside a registered folder, and a registration authorizes nothing.
// The registry and the panel are macOS-only; other targets answer every command with unsupported_platform.
use serde::Serialize;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Runtime, WebviewWindow};

#[cfg(target_os = "macos")]
mod folder;
#[cfg(target_os = "macos")]
mod registry;

/// The only window that may open the folder panel; the capability grants the commands to it alone.
#[cfg(target_os = "macos")]
const MAIN_WINDOW: &str = "main";
#[cfg(target_os = "macos")]
const PANEL_TITLE: &str = "Choose a project folder. Synapse remembers its location and does not read its contents.";

// Most codes are only produced by the macOS registry, which other targets do not compile.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotAllowed,
    SelectionInProgress,
    NotADirectory,
    FolderRefused,
    FolderChanged,
    FolderInaccessible,
    PathUnsupported,
    RegistryFull,
    InvalidInput,
    RegistryCorrupt,
    RegistryNewerVersion,
    RegistryUnavailable,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    UnsupportedPlatform,
}

/// Every message is fixed: no path, errno or OS text reaches the UI.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct ProjectError {
    code: ErrorCode,
    message: &'static str,
}

impl From<ErrorCode> for ProjectError {
    fn from(code: ErrorCode) -> Self {
        let message = match code {
            ErrorCode::NotAllowed => "Folders can only be added from the main window.",
            ErrorCode::SelectionInProgress => "A folder selection is already open.",
            ErrorCode::NotADirectory => "The selected item is not a folder.",
            ErrorCode::FolderRefused => "This folder is too broad to register as a project.",
            ErrorCode::FolderChanged => "The folder changed while it was being added.",
            ErrorCode::FolderInaccessible => "The folder could not be accessed.",
            ErrorCode::PathUnsupported => "The folder's location is not supported.",
            ErrorCode::RegistryFull => "The project list is full.",
            ErrorCode::InvalidInput => "The project reference is not valid.",
            ErrorCode::RegistryCorrupt => "The project list could not be read. The file was kept unchanged.",
            ErrorCode::RegistryNewerVersion => "The project list was saved by a newer version of Synapse.",
            ErrorCode::RegistryUnavailable => "The project list is unavailable.",
            ErrorCode::UnsupportedPlatform => "Local projects are not supported on this platform.",
        };
        ProjectError { code, message }
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Available,
    Missing,
    Inaccessible,
    Changed,
}

/// What the UI sees of a registration. Device and inode numbers stay in Rust.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Project {
    id: String,
    name: String,
    path_display: String,
    status: Status,
    registered_at_ms: u64,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct ProjectList {
    schema_version: u8,
    projects: Vec<Project>,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AddOutcome {
    Added { project: Project },
    AlreadyRegistered { project: Project },
    Cancelled,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum RemoveOutcome {
    Removed,
    NotFound,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ResetOutcome {
    Reset,
    NotNeeded,
}

pub type PickedFolder = Pin<Box<dyn Future<Output = Option<PathBuf>> + Send>>;

/// The folder panel. Production opens NSOpenPanel through rfd; Rust tests inject outcomes. It is reachable only
/// from add_project, which takes no argument, so no path ever travels from the webview to Rust.
pub trait FolderPicker<R: Runtime>: Send + Sync + 'static {
    /// Opens one panel as a sheet on `parent`; `None` when the user cancels.
    fn pick(&self, parent: &WebviewWindow<R>) -> PickedFolder;
}

struct NativePicker;

impl<R: Runtime> FolderPicker<R> for NativePicker {
    fn pick(&self, parent: &WebviewWindow<R>) -> PickedFolder {
        // No starting directory, filter or multiple selection. The async API with a parent is a sheet
        // (beginSheetModalForWindow); "New Folder" is on by default for folder panels in rfd, so a cancelled
        // panel could otherwise leave a new directory behind.
        #[cfg(target_os = "macos")]
        {
            let panel = rfd::AsyncFileDialog::new()
                .set_title(PANEL_TITLE)
                .set_can_create_directories(false)
                .set_parent(parent)
                .pick_folder();
            Box::pin(async move { panel.await.map(|picked| picked.path().to_path_buf()) })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = parent;
            Box::pin(std::future::ready(None))
        }
    }
}

/// Managed state for the four commands: where the registry lives.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct Projects {
    data_dir: Option<PathBuf>,
}

impl Projects {
    /// `data_dir` is the app data directory from Tauri's path resolver; `None` if it could not be resolved.
    pub fn new(data_dir: Option<PathBuf>) -> Self {
        Projects { data_dir }
    }
}

/// Managed state for add_project: how a folder is chosen.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct Picker<R: Runtime>(Box<dyn FolderPicker<R>>);

impl<R: Runtime> Picker<R> {
    pub fn native() -> Self {
        Picker(Box::new(NativePicker))
    }

    #[cfg(test)]
    pub(crate) fn new(picker: Box<dyn FolderPicker<R>>) -> Self {
        Picker(picker)
    }
}

static SELECTING: AtomicBool = AtomicBool::new(false);

/// One panel at a time. It lives as long as the panel itself, not as long as the call that opened it (see add).
struct SelectionGuard;

impl SelectionGuard {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn acquire() -> Result<Self, ErrorCode> {
        SELECTING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| SelectionGuard)
            .map_err(|_| ErrorCode::SelectionInProgress)
    }
}

impl Drop for SelectionGuard {
    fn drop(&mut self) {
        SELECTING.store(false, Ordering::Release);
    }
}

// Tests that can reach the one-panel guard take this lock, so a parallel test never sees another's panel.
#[cfg(test)]
pub(crate) static GUARD_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn selecting() -> bool {
    SELECTING.load(Ordering::Acquire)
}

#[cfg(target_os = "macos")]
impl Projects {
    fn data_dir(&self) -> Result<PathBuf, ErrorCode> {
        self.data_dir.clone().ok_or(ErrorCode::RegistryUnavailable)
    }

    pub async fn list(&self) -> Result<ProjectList, ProjectError> {
        let dir = self.data_dir()?;
        blocking(move || registry::list(&dir)).await
    }

    /// The registry lock is taken only after the panel has closed.
    pub async fn add<R: Runtime>(&self, window: &WebviewWindow<R>, picker: &Picker<R>) -> Result<AddOutcome, ProjectError> {
        if window.label() != MAIN_WINDOW {
            return Err(ErrorCode::NotAllowed.into());
        }
        let dir = self.data_dir()?;
        let selecting = SelectionGuard::acquire()?;
        // rfd begins the sheet when the panel future is created and offers no way to dismiss it; dropping the
        // future leaves the sheet on screen until the user closes it. The guard therefore moves into a task that
        // owns the panel and ends only when AppKit's completion handler fires, so it outlives a dropped call.
        // A call dropped meanwhile registers nothing.
        let panel = picker.0.pick(window);
        let picked = tauri::async_runtime::spawn(async move {
            let picked = panel.await;
            drop(selecting);
            picked
        })
        .await
        .map_err(|_| ErrorCode::RegistryUnavailable)?;
        let Some(picked) = picked else {
            return Ok(AddOutcome::Cancelled);
        };
        blocking(move || registry::add(&dir, &picked)).await
    }

    pub async fn remove(&self, id: String) -> Result<RemoveOutcome, ProjectError> {
        let dir = self.data_dir()?;
        blocking(move || registry::remove(&dir, &id)).await
    }

    pub async fn reset(&self) -> Result<ResetOutcome, ProjectError> {
        let dir = self.data_dir()?;
        blocking(move || registry::reset(&dir)).await
    }
}

/// Registry work touches the filesystem, so it runs on a blocking worker, never on the UI thread.
#[cfg(target_os = "macos")]
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, ErrorCode> + Send + 'static,
) -> Result<T, ProjectError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .unwrap_or(Err(ErrorCode::RegistryUnavailable))
        .map_err(ProjectError::from)
}

#[cfg(not(target_os = "macos"))]
impl Projects {
    pub async fn list(&self) -> Result<ProjectList, ProjectError> {
        Err(ErrorCode::UnsupportedPlatform.into())
    }

    pub async fn add<R: Runtime>(&self, _window: &WebviewWindow<R>, _picker: &Picker<R>) -> Result<AddOutcome, ProjectError> {
        Err(ErrorCode::UnsupportedPlatform.into())
    }

    pub async fn remove(&self, _id: String) -> Result<RemoveOutcome, ProjectError> {
        Err(ErrorCode::UnsupportedPlatform.into())
    }

    pub async fn reset(&self) -> Result<ResetOutcome, ProjectError> {
        Err(ErrorCode::UnsupportedPlatform.into())
    }
}
