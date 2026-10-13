// Development-only bridge to the Python runtime identity query (see docs/implementation/desktop-runtime-bridge.md).
// The process code below is compiled only with the dev-runtime feature (and in tests); a default or release
// build contains no way to start Python and answers with a fixed "unavailable" error.
use serde::Serialize;

#[cfg(all(feature = "dev-runtime", not(debug_assertions)))]
compile_error!("the dev-runtime feature is development-only and must not be combined with release assertions");

#[cfg(any(test, all(feature = "dev-runtime", target_os = "macos")))]
mod process;

// Several codes are only produced by the dev-runtime process code, which default builds do not compile.
#[cfg_attr(not(any(test, feature = "dev-runtime")), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    RuntimeNotInstalled,
    RuntimeUnsupportedVersion,
    RuntimeTimeout,
    RuntimeOutputTooLarge,
    RuntimeOutputMalformed,
    RuntimeFailed,
    RuntimeBusy,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    UnsupportedPlatform,
}

/// Every message is fixed: nothing from the child, the filesystem or the environment can reach the UI.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct BridgeError {
    code: ErrorCode,
    message: &'static str,
}

impl From<ErrorCode> for BridgeError {
    fn from(code: ErrorCode) -> Self {
        let message = match code {
            ErrorCode::RuntimeNotInstalled => "Development runtime is unavailable.",
            ErrorCode::RuntimeUnsupportedVersion => "Development runtime version is not supported.",
            ErrorCode::RuntimeTimeout => "Development runtime did not respond in time.",
            ErrorCode::RuntimeOutputTooLarge => "Development runtime response was too large.",
            ErrorCode::RuntimeOutputMalformed => "Development runtime response was not understood.",
            ErrorCode::RuntimeFailed => "Development runtime failed.",
            ErrorCode::RuntimeBusy => "A runtime status request is already in progress.",
            ErrorCode::UnsupportedPlatform => "The development runtime bridge is not supported on this platform.",
        };
        BridgeError { code, message }
    }
}

// Provenance is set by Rust from its own observation; the child is never asked to describe how it ran.
#[cfg_attr(not(any(test, feature = "dev-runtime")), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Provenance {
    source: &'static str,
    interpreter_kind: &'static str,
    observed_at_ms: u64,
    duration_ms: u64,
}

#[cfg_attr(not(any(test, feature = "dev-runtime")), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeIdentity {
    version: String,
    python: String,
}

#[cfg_attr(not(any(test, feature = "dev-runtime")), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct GatePolicy {
    policy_version: u32,
}

#[cfg_attr(not(any(test, feature = "dev-runtime")), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeStatus {
    schema_version: u8,
    provenance: Provenance,
    runtime: RuntimeIdentity,
    gate: GatePolicy,
}

// Tests that can reach the one-request-at-a-time guard (directly or through the command) take this lock,
// so a parallel test never sees another test's request as "busy".
#[cfg(test)]
pub(crate) static GUARD_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Blocking; callers run it on a worker thread.
pub fn query() -> Result<RuntimeStatus, BridgeError> {
    #[cfg(not(target_os = "macos"))]
    return Err(ErrorCode::UnsupportedPlatform.into());
    #[cfg(all(target_os = "macos", not(feature = "dev-runtime")))]
    return Err(ErrorCode::RuntimeNotInstalled.into());
    #[cfg(all(target_os = "macos", feature = "dev-runtime"))]
    return process::query_dev();
}
