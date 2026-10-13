use super::*;
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;

// Tests that go through query_with share the one-request-at-a-time guard, so they must not overlap.
static SERIAL: Mutex<()> = Mutex::new(());

const GOOD: &str = r#"{"schema_version":1,"runtime":{"version":"0.12.1","python":"3.12.4"},"gate":{"policy_version":1}}"#;

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("synapse-bridge-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        Scratch(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap_or_default()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sh(script: &str, deadline: Duration) -> Launch {
    Launch { interpreter: "/bin/sh".into(), args: vec!["-c".into(), script.into()], deadline, max_stdout: MAX_STDOUT }
}

fn fast(script: &str) -> Launch {
    sh(script, Duration::from_secs(5))
}

fn emit(json: &str) -> Launch {
    fast(&format!("printf '%s' '{json}'"))
}

fn outcome(launch: &Launch) -> ErrorCode {
    run(launch).expect_err("expected a bridge error")
}

fn pid_is_gone(pid: libc::pid_t) -> bool {
    // A killed process can linger as a zombie until its adopter reaps it; allow a short grace period.
    (0..100).any(|_| {
        // SAFETY: signal 0 only checks for existence.
        let gone = unsafe { libc::kill(pid, 0) } == -1;
        if !gone {
            std::thread::sleep(Duration::from_millis(20));
        }
        gone
    })
}

#[test]
fn valid_report_becomes_a_status_with_rust_set_provenance() {
    let before = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    let status = run(&emit(GOOD)).unwrap();
    assert_eq!(status.schema_version, 1);
    assert_eq!(status.runtime, RuntimeIdentity { version: "0.12.1".into(), python: "3.12.4".into() });
    assert_eq!(status.gate, GatePolicy { policy_version: 1 });
    assert_eq!(status.provenance.source, "python_runtime");
    assert_eq!(status.provenance.interpreter_kind, "dev_checkout");
    assert!(status.provenance.observed_at_ms >= before);
}

#[test]
fn child_cannot_claim_provenance() {
    let json = r#"{"schema_version":1,"provenance":{"source":"x"},"runtime":{"version":"1","python":"3"},"gate":{"policy_version":1}}"#;
    assert_eq!(outcome(&emit(json)), ErrorCode::RuntimeOutputMalformed);
}

#[test]
fn malformed_unknown_and_unsupported_reports_are_rejected() {
    for bad in [
        "{",
        "",
        "[]",
        "null",
        r#"{"schema_version":1}"#,
        r#"{"schema_version":"1","runtime":{"version":"1","python":"3"},"gate":{"policy_version":1}}"#,
        r#"{"schema_version":1,"runtime":{"version":"1","python":"3","extra":1},"gate":{"policy_version":1}}"#,
        r#"{"schema_version":1,"runtime":{"version":"1","python":"3"},"gate":{"policy_version":1},"extra":1}"#,
        r#"{"schema_version":1,"schema_version":1,"runtime":{"version":"1","python":"3"},"gate":{"policy_version":1}}"#,
        r#"{"schema_version":1,"runtime":{"version":"/Users/x/py","python":"3"},"gate":{"policy_version":1}}"#,
        r#"{"schema_version":1,"runtime":{"version":"","python":"3"},"gate":{"policy_version":1}}"#,
        r#"{"schema_version":1,"runtime":{"version":"1","python":"3"},"gate":{"policy_version":-1}}"#,
    ] {
        assert_eq!(outcome(&emit(bad)), ErrorCode::RuntimeOutputMalformed, "{bad}");
    }
    assert_eq!(outcome(&fast(&format!("printf '%s trailing' '{GOOD}'"))), ErrorCode::RuntimeOutputMalformed);
    for unsupported in [2, 0, 99] {
        let json = GOOD.replace(r#""schema_version":1"#, &format!(r#""schema_version":{unsupported}"#));
        assert_eq!(outcome(&emit(&json)), ErrorCode::RuntimeUnsupportedVersion, "{json}");
    }
}

#[test]
fn bootstrap_unsupported_version_exit_status_is_mapped() {
    assert_eq!(outcome(&fast("exit 78")), ErrorCode::RuntimeUnsupportedVersion);
}

#[test]
fn other_failures_are_runtime_failed_and_never_echo_output() {
    assert_eq!(outcome(&fast("echo secret-from-stdout; echo secret-from-stderr >&2; exit 1")), ErrorCode::RuntimeFailed);
    assert_eq!(outcome(&fast(&format!("printf '%s' '{GOOD}'; exit 3"))), ErrorCode::RuntimeFailed);
    assert_eq!(outcome(&fast("kill -9 $$")), ErrorCode::RuntimeFailed);
}

#[test]
fn missing_interpreter_is_not_installed() {
    let launch = Launch { interpreter: "/nonexistent/python".into(), args: vec![], deadline: DEADLINE, max_stdout: MAX_STDOUT };
    assert_eq!(outcome(&launch), ErrorCode::RuntimeNotInstalled);
}

#[test]
fn oversized_output_is_stopped_and_the_child_is_reaped() {
    let scratch = Scratch::new("big");
    let pidfile = scratch.path("pid");
    let launch = fast(&format!("echo $$ > '{}'; exec yes", pidfile.display()));
    assert_eq!(outcome(&launch), ErrorCode::RuntimeOutputTooLarge);
    assert!(pid_is_gone(scratch.read("pid").trim().parse().unwrap()));
}

#[test]
fn output_exactly_at_the_limit_is_read_but_one_more_byte_is_not() {
    let at_limit = Launch { max_stdout: GOOD.len(), ..emit(GOOD) };
    assert!(run(&at_limit).is_ok());
    let over = Launch { max_stdout: GOOD.len() - 1, ..emit(GOOD) };
    assert_eq!(outcome(&over), ErrorCode::RuntimeOutputTooLarge);
}

#[test]
fn timeout_kills_and_reaps_the_child_and_its_process_group() {
    let scratch = Scratch::new("timeout");
    let script = format!("echo $$ > '{0}/pid'; sleep 60 & echo $! > '{0}/grandchild'; wait", scratch.0.display());
    let started = Instant::now();
    // Long enough that a loaded machine has started the shell and written the pid files first.
    assert_eq!(outcome(&sh(&script, Duration::from_millis(1500))), ErrorCode::RuntimeTimeout);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(pid_is_gone(scratch.read("pid").trim().parse().unwrap()));
    assert!(pid_is_gone(scratch.read("grandchild").trim().parse().unwrap()));
}

#[test]
fn a_child_that_closes_stdout_but_keeps_running_still_times_out() {
    let started = Instant::now();
    assert_eq!(outcome(&sh("exec >&-; sleep 60", Duration::from_millis(1000))), ErrorCode::RuntimeTimeout);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_flooded_stderr_cannot_block_the_child() {
    let script = format!("head -c 4000000 /dev/zero >&2; printf '%s' '{GOOD}'");
    assert!(run(&fast(&script)).is_ok());
}

#[test]
fn child_gets_a_cleared_environment_neutral_cwd_and_null_stdin() {
    let scratch = Scratch::new("env");
    for (key, value) in [
        ("PATH", "/tmp/hostile-path"),
        ("PYTHONPATH", "/tmp/hostile"),
        ("PYTHONHOME", "/tmp/hostile"),
        ("DYLD_INSERT_LIBRARIES", "/tmp/hostile.dylib"),
        ("DYLD_LIBRARY_PATH", "/tmp/hostile"),
        ("PYTHONSTARTUP", "/tmp/hostile.py"),
        ("SYNAPSE_TEST_SECRET", "canary"),
    ] {
        std::env::set_var(key, value);
    }
    let dir = scratch.0.display();
    // cat would block until the deadline if stdin were inherited rather than null.
    let script = format!("/usr/bin/env > '{dir}/env'; /bin/pwd > '{dir}/cwd'; /bin/cat > '{dir}/stdin'; printf '%s' '{GOOD}'");
    let launch = Launch { interpreter: "/bin/sh".into(), args: vec!["-c".into(), script.into()], deadline: Duration::from_secs(5), max_stdout: MAX_STDOUT };
    assert!(run(&launch).is_ok());
    let env = scratch.read("env");
    for leaked in ["hostile", "canary", "PYTHON", "DYLD_", "SYNAPSE_"] {
        assert!(!env.contains(leaked), "{leaked} leaked into the child: {env}");
    }
    assert_eq!(scratch.read("cwd").trim(), "/");
    assert_eq!(scratch.read("stdin"), "");
}

#[test]
fn descriptors_without_close_on_exec_are_not_inherited() {
    let scratch = Scratch::new("fd");
    let file = std::fs::File::open("/dev/null").unwrap();
    // F_DUPFD yields a descriptor without close-on-exec (unlike File's own). Number 200 is high enough that
    // the child shell will not reuse it for its own redirections, which would make the probe lie.
    // SAFETY: duplicating a valid descriptor.
    let leaked = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD, 200) };
    assert_eq!(leaked, 200);
    // /dev/fd/N exists only while N is open in the probing process; /dev/fd/1 (stdout) is the positive control.
    let script = format!(
        "[ -e /dev/fd/1 ] && echo control-open > '{0}/control'; if [ -e /dev/fd/{leaked} ]; then echo open; else echo closed; fi > '{0}/fd'; printf '%s' '{GOOD}'",
        scratch.0.display()
    );
    assert!(run(&fast(&script)).is_ok());
    // SAFETY: closing the descriptor created above.
    unsafe { libc::close(leaked) };
    assert_eq!(scratch.read("control").trim(), "control-open", "the probe must be able to see an open descriptor");
    assert_eq!(scratch.read("fd").trim(), "closed");
}

#[test]
fn a_second_concurrent_request_is_busy_and_the_slot_is_released_afterwards() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let slow = fast(&format!("sleep 2; printf '%s' '{GOOD}'"));
    let first = std::thread::spawn(move || query_with(&slow));
    // Wait until the first request really holds the slot instead of guessing how long a thread takes to start.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ACTIVE.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "first request never started");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(query_with(&emit(GOOD)).unwrap_err(), ErrorCode::RuntimeBusy.into());
    assert!(first.join().unwrap().is_ok());
    assert!(query_with(&emit(GOOD)).is_ok());
}

#[test]
fn errors_carry_only_fixed_messages() {
    let all = [
        ErrorCode::RuntimeNotInstalled,
        ErrorCode::RuntimeUnsupportedVersion,
        ErrorCode::RuntimeTimeout,
        ErrorCode::RuntimeOutputTooLarge,
        ErrorCode::RuntimeOutputMalformed,
        ErrorCode::RuntimeFailed,
        ErrorCode::RuntimeBusy,
        ErrorCode::UnsupportedPlatform,
    ];
    let codes: Vec<String> = all
        .iter()
        .map(|code| {
            let value = serde_json::to_value(BridgeError::from(*code)).unwrap();
            assert_eq!(value.as_object().unwrap().len(), 2);
            let message = value["message"].as_str().unwrap();
            assert!(!message.contains('/') && message.ends_with('.'), "{message}");
            value["code"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(
        codes,
        [
            "runtime_not_installed",
            "runtime_unsupported_version",
            "runtime_timeout",
            "runtime_output_too_large",
            "runtime_output_malformed",
            "runtime_failed",
            "runtime_busy",
            "unsupported_platform"
        ]
    );
}

#[test]
fn status_serializes_to_the_documented_shape() {
    let value = serde_json::to_value(run(&emit(GOOD)).unwrap()).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["runtime"], serde_json::json!({ "version": "0.12.1", "python": "3.12.4" }));
    assert_eq!(value["gate"], serde_json::json!({ "policy_version": 1 }));
    let provenance = value["provenance"].as_object().unwrap();
    let mut keys: Vec<&str> = provenance.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["duration_ms", "interpreter_kind", "observed_at_ms", "source"]);
    assert_eq!(value.as_object().unwrap().len(), 4);
}

mod interpreter {
    use super::*;

    fn executable(scratch: &Scratch, name: &str, mode: u32) -> PathBuf {
        let path = scratch.path(name);
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    #[test]
    fn accepts_an_owned_executable_in_a_private_directory() {
        let scratch = Scratch::new("ok");
        let path = executable(&scratch, "python", 0o755);
        assert_eq!(validate_interpreter(&path), Ok(path.canonicalize().unwrap()));
        assert!(validate_interpreter(Path::new("/bin/sh")).is_ok());
    }

    #[test]
    fn rejects_relative_missing_directory_and_non_executable_paths() {
        let scratch = Scratch::new("bad");
        let plain = executable(&scratch, "plain", 0o644);
        for path in [Path::new("python3"), Path::new("./python"), Path::new("/nonexistent/python"), scratch.0.as_path(), plain.as_path()] {
            assert_eq!(validate_interpreter(path), Err(ErrorCode::RuntimeNotInstalled), "{path:?}");
        }
    }

    #[test]
    fn rejects_group_or_world_writable_files_and_directories() {
        let scratch = Scratch::new("writable");
        for mode in [0o775, 0o757, 0o777] {
            let path = executable(&scratch, "python", mode);
            assert_eq!(validate_interpreter(&path), Err(ErrorCode::RuntimeNotInstalled), "{mode:o}");
        }
        let shared = Scratch::new("shared-dir");
        let path = executable(&shared, "python", 0o755);
        std::fs::set_permissions(&shared.0, std::fs::Permissions::from_mode(0o775)).unwrap();
        assert_eq!(validate_interpreter(&path), Err(ErrorCode::RuntimeNotInstalled));
    }

    #[test]
    fn symlinks_are_resolved_and_judged_by_their_target() {
        let scratch = Scratch::new("link");
        let target = executable(&scratch, "real", 0o755);
        let link = scratch.path("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(validate_interpreter(&link), Ok(target.canonicalize().unwrap()));

        let bad = executable(&scratch, "writable", 0o777);
        let bad_link = scratch.path("bad-link");
        std::os::unix::fs::symlink(&bad, &bad_link).unwrap();
        assert_eq!(validate_interpreter(&bad_link), Err(ErrorCode::RuntimeNotInstalled));
    }
}

#[cfg(feature = "dev-runtime")]
mod dev {
    use super::*;

    #[test]
    fn argv_is_fixed_and_contains_only_the_isolation_flags_and_the_script() {
        let argv = dev_argv(Path::new("/checkout"));
        assert_eq!(argv, ["-I", "-S", "-B", "/checkout/desktop/runtime-bridge/status.py"]);
    }

    // Needs desktop/src-tauri/.dev-python (see docs/implementation/desktop-runtime-bridge.md); failing
    // without it is intended, because this feature is only built where the interpreter is configured.
    #[test]
    fn configured_development_runtime_reports_its_identity() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let status = query_dev().expect("configure .dev-python with a Python >= 3.10");
        assert_eq!(status.gate.policy_version, 1);
        assert_eq!(status.provenance.interpreter_kind, "dev_checkout");
        assert!(status.provenance.duration_ms < 5000);
        let toml = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pyproject.toml")).unwrap();
        assert!(toml.contains(&format!("version = \"{}\"", status.runtime.version)));
    }
}
