// The only place that starts Python. Everything about the child is fixed here or by build.rs; no value
// from the frontend, the environment or PATH reaches the interpreter path, argv, environment or cwd.
use super::{BridgeError, ErrorCode, GatePolicy, Provenance, RuntimeIdentity, RuntimeStatus};
use serde::Deserialize;
use std::ffi::OsString;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg_attr(not(feature = "dev-runtime"), allow(dead_code))]
const MAX_STDOUT: usize = 16 * 1024;
#[cfg_attr(not(feature = "dev-runtime"), allow(dead_code))]
const DEADLINE: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const SCHEMA_VERSION: u64 = 1;
// Exit status of desktop/runtime-bridge/status.py for an interpreter below its minimum version.
const EXIT_UNSUPPORTED_VERSION: i32 = 78;
const MAX_FIELD_LEN: usize = 32;

#[cfg(feature = "dev-runtime")]
const DEV_PYTHON: Option<&str> = option_env!("SYNAPSE_DEV_PYTHON");
#[cfg(feature = "dev-runtime")]
const DEV_CHECKOUT: Option<&str> = option_env!("SYNAPSE_DEV_CHECKOUT");

pub(super) struct Launch {
    pub interpreter: PathBuf,
    pub args: Vec<OsString>,
    pub deadline: Duration,
    pub max_stdout: usize,
}

// Strict shape of the child's report. Provenance is deliberately absent: the child may not claim it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    #[allow(dead_code)] // checked by the Peek pass in parse_report; kept so unknown-field rejection sees it
    schema_version: u64,
    runtime: ReportRuntime,
    gate: ReportGate,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportRuntime {
    version: String,
    python: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportGate {
    policy_version: u32,
}

static ACTIVE: AtomicBool = AtomicBool::new(false);

struct ActiveGuard;

impl ActiveGuard {
    fn acquire() -> Result<Self, ErrorCode> {
        ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ActiveGuard)
            .map_err(|_| ErrorCode::RuntimeBusy)
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
    }
}

#[cfg(feature = "dev-runtime")]
pub(super) fn query_dev() -> Result<RuntimeStatus, BridgeError> {
    query_with(&dev_launch()?)
}

/// One request at a time: a second concurrent call is refused rather than queued.
pub(super) fn query_with(launch: &Launch) -> Result<RuntimeStatus, BridgeError> {
    let _active = ActiveGuard::acquire()?;
    Ok(run(launch)?)
}

#[cfg(feature = "dev-runtime")]
fn dev_launch() -> Result<Launch, ErrorCode> {
    let interpreter = validate_interpreter(Path::new(DEV_PYTHON.ok_or(ErrorCode::RuntimeNotInstalled)?))?;
    let checkout = DEV_CHECKOUT.ok_or(ErrorCode::RuntimeNotInstalled)?;
    Ok(Launch {
        interpreter,
        args: dev_argv(Path::new(checkout)),
        deadline: DEADLINE,
        max_stdout: MAX_STDOUT,
    })
}

// -I isolated, -S no site, -B no bytecode. These narrow what Python loads; they are not a sandbox.
#[cfg(feature = "dev-runtime")]
fn dev_argv(checkout: &Path) -> Vec<OsString> {
    vec!["-I".into(), "-S".into(), "-B".into(), checkout.join("desktop/runtime-bridge/status.py").into()]
}

/// The configured interpreter is trusted development infrastructure. This checks that it is not obviously
/// replaceable by another account; it cannot defend against a same-user attacker (a TOCTOU window remains).
/// Every failure is reported identically so no path or reason leaks.
fn validate_interpreter(configured: &Path) -> Result<PathBuf, ErrorCode> {
    let unavailable = ErrorCode::RuntimeNotInstalled;
    if !configured.is_absolute() {
        return Err(unavailable);
    }
    // Symlinks (Homebrew, venvs) are resolved once here and the resolved file is what runs. A venv's
    // symlink therefore runs the base interpreter, which -I -S makes irrelevant.
    let resolved = configured.canonicalize().map_err(|_| unavailable)?;
    let file = std::fs::metadata(&resolved).map_err(|_| unavailable)?;
    let parent = std::fs::metadata(resolved.parent().ok_or(unavailable)?).map_err(|_| unavailable)?;
    // SAFETY: geteuid has no preconditions.
    let me = unsafe { libc::geteuid() };
    let trusted = |meta: &std::fs::Metadata| (meta.uid() == me || meta.uid() == 0) && meta.mode() & 0o022 == 0;
    if file.is_file() && parent.is_dir() && file.mode() & 0o111 != 0 && trusted(&file) && trusted(&parent) {
        Ok(resolved)
    } else {
        Err(unavailable)
    }
}

fn run(launch: &Launch) -> Result<RuntimeStatus, ErrorCode> {
    let start = Instant::now();
    let deadline = start + launch.deadline;
    // SAFETY: sysconf has no preconditions.
    let fd_limit = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) }.clamp(3, 1 << 16) as libc::c_int;

    let mut command = Command::new(&launch.interpreter);
    command
        .args(&launch.args)
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    // SAFETY: only async-signal-safe fcntl calls run between fork and exec. Descriptors are marked
    // close-on-exec rather than closed, so std's own exec-failure pipe keeps working.
    unsafe {
        command.pre_exec(move || {
            for fd in 3..fd_limit {
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            }
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => ErrorCode::RuntimeNotInstalled,
        _ => ErrorCode::RuntimeFailed,
    })?;

    let collected = collect(&mut child, launch.max_stdout, deadline);
    let (status, stdout) = match collected {
        Ok(done) => done,
        Err(code) => {
            terminate(&mut child);
            return Err(code);
        }
    };
    if status.code() == Some(EXIT_UNSUPPORTED_VERSION) {
        return Err(ErrorCode::RuntimeUnsupportedVersion);
    }
    if !status.success() {
        return Err(ErrorCode::RuntimeFailed);
    }
    let report = parse_report(&stdout)?;
    let observed_at_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| ErrorCode::RuntimeFailed)?.as_millis();
    Ok(RuntimeStatus {
        schema_version: 1,
        provenance: Provenance {
            source: "python_runtime",
            interpreter_kind: "dev_checkout",
            observed_at_ms: observed_at_ms as u64,
            duration_ms: start.elapsed().as_millis() as u64,
        },
        runtime: RuntimeIdentity { version: report.runtime.version, python: report.runtime.python },
        gate: GatePolicy { policy_version: report.gate.policy_version },
    })
}

// Reads stdout with poll(2) on this thread (no reader thread to abandon), never holding more than
// max_stdout + 1 bytes, then waits for the direct child, all against one deadline. stderr goes to /dev/null,
// so an undrained pipe cannot stall the child.
fn collect(child: &mut Child, max_stdout: usize, deadline: Instant) -> Result<(ExitStatus, Vec<u8>), ErrorCode> {
    let mut stdout = child.stdout.take().ok_or(ErrorCode::RuntimeFailed)?;
    let mut out = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let remaining = deadline.checked_duration_since(Instant::now()).ok_or(ErrorCode::RuntimeTimeout)?;
        if !readable(stdout.as_raw_fd(), remaining)? {
            continue;
        }
        let room = (max_stdout + 1 - out.len()).min(chunk.len());
        match stdout.read(&mut chunk[..room]) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&chunk[..n]);
                if out.len() > max_stdout {
                    return Err(ErrorCode::RuntimeOutputTooLarge);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(ErrorCode::RuntimeFailed),
        }
    }
    drop(stdout);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok((status, out)),
            Ok(None) => {}
            Err(_) => return Err(ErrorCode::RuntimeFailed),
        }
        let remaining = deadline.checked_duration_since(Instant::now()).ok_or(ErrorCode::RuntimeTimeout)?;
        std::thread::sleep(POLL_INTERVAL.min(remaining));
    }
}

fn readable(fd: libc::c_int, wait: Duration) -> Result<bool, ErrorCode> {
    let mut pollfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    let millis = wait.as_millis().clamp(1, i32::MAX as u128) as libc::c_int;
    // SAFETY: pollfd points to one valid pollfd for the duration of the call.
    match unsafe { libc::poll(&mut pollfd, 1, millis) } {
        0 => Ok(false),
        n if n > 0 => Ok(true),
        _ if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted => Ok(false),
        _ => Err(ErrorCode::RuntimeFailed),
    }
}

// SIGKILL the whole process group before reaping, while the zombie still reserves the pid, so the group
// id cannot have been reused. A process group is cleanup, not containment: a descendant that called
// setsid/setpgid escapes it.
fn terminate(child: &mut Child) {
    // SAFETY: killpg only signals; the group id is our direct child's pid (process_group(0)).
    unsafe { libc::killpg(child.id() as libc::pid_t, libc::SIGKILL) };
    let _ = child.kill();
    let _ = child.wait();
}

fn parse_report(stdout: &[u8]) -> Result<Report, ErrorCode> {
    #[derive(Deserialize)]
    struct Peek {
        schema_version: u64,
    }
    let malformed = ErrorCode::RuntimeOutputMalformed;
    // Typed parsing straight from the bytes, so duplicate keys are errors rather than last-one-wins.
    if serde_json::from_slice::<Peek>(stdout).map_err(|_| malformed)?.schema_version != SCHEMA_VERSION {
        return Err(ErrorCode::RuntimeUnsupportedVersion);
    }
    let report: Report = serde_json::from_slice(stdout).map_err(|_| malformed)?;
    let plain = |s: &str| (1..=MAX_FIELD_LEN).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b));
    if plain(&report.runtime.version) && plain(&report.runtime.python) {
        Ok(report)
    } else {
        Err(malformed)
    }
}

#[cfg(test)]
mod tests;
