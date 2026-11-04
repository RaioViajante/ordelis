// Saved tasks (see docs/implementation/desktop-tasks.md). A task is a description of work the user wants done, tied
// to one registered project. Saving one starts nothing: a task holds no plan, no run and no authority, and its stored
// status is only draft or archived. The store is macOS-only, like the project registry; other targets answer every
// command with unsupported_platform.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod records;

// Most codes are only produced by the macOS store, which other targets do not compile.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskErrorCode {
    InvalidInput,
    ProjectNotFound,
    ProjectUnavailable,
    TasksFull,
    TasksCorrupt,
    TasksNewerVersion,
    TasksUnavailable,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    UnsupportedPlatform,
}

/// Every message is fixed: no task text, path, errno or OS text reaches the UI.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct TaskError {
    code: TaskErrorCode,
    message: &'static str,
}

impl From<TaskErrorCode> for TaskError {
    fn from(code: TaskErrorCode) -> Self {
        let message = match code {
            TaskErrorCode::InvalidInput => "The task details are not valid.",
            TaskErrorCode::ProjectNotFound => "The project is not registered, or its registration changed.",
            TaskErrorCode::ProjectUnavailable => "The project list is unavailable.",
            TaskErrorCode::TasksFull => "The task list is full.",
            TaskErrorCode::TasksCorrupt => "The task list could not be read. The file was kept unchanged.",
            TaskErrorCode::TasksNewerVersion => "The task list was saved by a newer version of Ordelis.",
            TaskErrorCode::TasksUnavailable => "The task list is unavailable.",
            TaskErrorCode::UnsupportedPlatform => "Tasks are not supported on this platform.",
        };
        TaskError { code, message }
    }
}

/// The only stored statuses. Planning and running states belong to future records, not to a saved task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Draft,
    Archived,
}

/// How a task's saved project identity relates to the registry now. Computed on every read, never stored.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    /// A registration with the same id and registration time exists.
    Linked,
    /// No registration has that id.
    Removed,
    /// The id now names a different registration (only possible after the project list was reset).
    Replaced,
    /// The project list could not be read.
    Unknown,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProjectLink {
    id: String,
    /// The saved registration time, so the interface can match a task to a registration by both halves of its
    /// identity (issue #98). It is a number the project list already shows, never a path.
    registered_at_ms: u64,
    link: Link,
}

/// What a list shows of a task: everything but its description.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskSummary {
    id: String,
    title: String,
    status: Status,
    project: ProjectLink,
    created_at_ms: u64,
    updated_at_ms: u64,
    revision: u64,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Task {
    #[serde(flatten)]
    summary: TaskSummary,
    description: String,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct TaskList {
    schema_version: u8,
    tasks: Vec<TaskSummary>,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum GetOutcome {
    Found { task: Task },
    NotFound,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CreateOutcome {
    Created { task: Task },
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum UpdateOutcome {
    Updated { task: Task },
    /// The given revision is not the task's current one; nothing was written. Carries the current task.
    Conflict { task: Task },
    NotFound,
}

/// Managed state for the four commands: where the store lives (the app data directory, shared with the registry).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct Tasks {
    data_dir: Option<PathBuf>,
}

impl Tasks {
    /// `data_dir` is the app data directory from Tauri's path resolver; `None` if it could not be resolved.
    pub fn new(data_dir: Option<PathBuf>) -> Self {
        Tasks { data_dir }
    }
}

#[cfg(target_os = "macos")]
impl Tasks {
    fn data_dir(&self) -> Result<PathBuf, TaskErrorCode> {
        self.data_dir.clone().ok_or(TaskErrorCode::TasksUnavailable)
    }

    pub async fn list(&self) -> Result<TaskList, TaskError> {
        let dir = self.data_dir()?;
        blocking(move || records::list(&dir)).await
    }

    pub async fn get(&self, id: String) -> Result<GetOutcome, TaskError> {
        let dir = self.data_dir()?;
        blocking(move || records::get(&dir, &id)).await
    }

    pub async fn create(
        &self,
        project_id: String,
        expected_registered_at_ms: u64,
        title: String,
        description: String,
    ) -> Result<CreateOutcome, TaskError> {
        let dir = self.data_dir()?;
        blocking(move || records::create(&dir, &project_id, expected_registered_at_ms, &title, &description)).await
    }

    pub async fn update(
        &self,
        id: String,
        revision: u64,
        title: String,
        description: String,
        status: String,
    ) -> Result<UpdateOutcome, TaskError> {
        let dir = self.data_dir()?;
        blocking(move || records::update(&dir, &id, revision, &title, &description, &status)).await
    }
}

/// Store work touches the filesystem, so it runs on a blocking worker, never on the UI thread.
#[cfg(target_os = "macos")]
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, TaskErrorCode> + Send + 'static,
) -> Result<T, TaskError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .unwrap_or(Err(TaskErrorCode::TasksUnavailable))
        .map_err(TaskError::from)
}

#[cfg(not(target_os = "macos"))]
impl Tasks {
    pub async fn list(&self) -> Result<TaskList, TaskError> {
        Err(TaskErrorCode::UnsupportedPlatform.into())
    }

    pub async fn get(&self, _id: String) -> Result<GetOutcome, TaskError> {
        Err(TaskErrorCode::UnsupportedPlatform.into())
    }

    pub async fn create(
        &self,
        _project_id: String,
        _expected_registered_at_ms: u64,
        _title: String,
        _description: String,
    ) -> Result<CreateOutcome, TaskError> {
        Err(TaskErrorCode::UnsupportedPlatform.into())
    }

    pub async fn update(
        &self,
        _id: String,
        _revision: u64,
        _title: String,
        _description: String,
        _status: String,
    ) -> Result<UpdateOutcome, TaskError> {
        Err(TaskErrorCode::UnsupportedPlatform.into())
    }
}
