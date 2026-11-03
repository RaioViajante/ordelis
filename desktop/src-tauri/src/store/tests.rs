// Compile-fail tests for the store's lock guard. Each case compiles the real store.rs, as a module of a small crate,
// with the same rustc and libc that built this test binary, and checks the compiler's own error code. A correct use
// must compile, so a failure cannot come from the harness itself.
use std::path::{Path, PathBuf};
use std::process::Command;

/// Two stores, as an owner module declares them, and a writer that accepts only the second store's guard.
const PRELUDE: &str = r#"
#![allow(dead_code, unused)]
mod store;
use std::path::Path;
use std::sync::Mutex;
use store::{Files, Locked, Store};

static A_WRITER: Mutex<()> = Mutex::new(());
static A_FILES: Files = Files { file: "a.json", lock: "a.lock", temp: "a.json.tmp", writer: &A_WRITER };
struct A;
impl Store for A {
    fn files() -> &'static Files {
        &A_FILES
    }
}

static B_WRITER: Mutex<()> = Mutex::new(());
static B_FILES: Files = Files { file: "b.json", lock: "b.lock", temp: "b.json.tmp", writer: &B_WRITER };
struct B;
impl Store for B {
    fn files() -> &'static Files {
        &B_FILES
    }
}

fn write_b(locked: &Locked<B>) {
    let _ = store::replace(locked, b"{}");
}
"#;

/// The correct use. If this does not compile, the harness is broken and every other case would prove nothing.
const ALLOWED: &str = r#"
fn allowed(dir: &Path) {
    if let Ok(Some(locked)) = store::lock::<B>(dir, true) {
        write_b(&locked);
        let _ = store::replace(&locked, b"{}");
    }
}
"#;

/// (what is attempted, code, the rustc error code it must fail with)
const REJECTED: [(&str, &str, &str); 7] = [
    (
        "a guard for one store authorizes a write to another",
        "fn f(dir: &Path) { let locked = store::lock::<A>(dir, true).unwrap().unwrap(); write_b(&locked); }",
        "E0308",
    ),
    (
        "a guard for one store, passed as another store's write",
        "fn f(dir: &Path) { let locked = store::lock::<A>(dir, true).unwrap().unwrap(); let _ = store::replace::<B>(&locked, b\"{}\"); }",
        "E0308",
    ),
    (
        "a write without a guard, given a directory instead",
        "fn f(dir: &Path) { let _ = store::replace::<B>(dir, b\"{}\"); }",
        "E0308",
    ),
    (
        "a guard forged outside the store",
        "fn f(file: std::fs::File, writer: std::sync::MutexGuard<'static, ()>) { let _forged: Locked<B> = Locked { dir: std::path::PathBuf::new(), _file: file, _writer: writer, _store: std::marker::PhantomData }; }",
        "E0451",
    ),
    // Named in full: `locked.clone()` on a reference would compile by copying the reference, not the guard.
    ("a guard copied", "fn f(locked: Locked<B>) { let _copy = <Locked<B> as Clone>::clone(&locked); }", "E0277"),
    (
        "a guard sent to another thread",
        "fn f(dir: &Path) { let locked = store::lock::<B>(dir, true).unwrap().unwrap(); std::thread::spawn(move || drop(locked)); }",
        "E0277",
    ),
    ("a guard retyped as another store's", "fn f(locked: Locked<A>) -> Locked<B> { locked }", "E0308"),
];

/// rustc beside the cargo that built this binary, and the libc builds in the same target directory.
fn toolchain() -> (PathBuf, PathBuf, Vec<PathBuf>) {
    let rustc = Path::new(env!("CARGO")).with_file_name("rustc");
    let rustc = if rustc.exists() { rustc } else { PathBuf::from("rustc") };
    let deps = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    let mut libc: Vec<PathBuf> = std::fs::read_dir(&deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("liblibc-") && name.ends_with(".rlib")
        })
        .collect();
    libc.sort();
    assert!(!libc.is_empty(), "no libc rlib in {deps:?}");
    (rustc, deps, libc)
}

struct Case(PathBuf);

impl Drop for Case {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Compiles PRELUDE + `code` as a library whose `store` module is a copy of store.rs. `Err` holds rustc's error
/// codes, in order.
fn compile(rustc: &Path, deps: &Path, libc: &Path, tag: &str, code: &str) -> Result<(), Vec<String>> {
    let dir = std::env::temp_dir().join(format!("ordelis-store-guard-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    let case = Case(dir);
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/store.rs"), case.0.join("store.rs")).unwrap();
    std::fs::write(case.0.join("lib.rs"), format!("{PRELUDE}\n{code}\n")).unwrap();
    // A cleared environment: another test sets hostile process-wide variables, which must not reach this child.
    let mut command = Command::new(rustc);
    command.env_clear();
    for key in ["HOME", "CARGO_HOME", "RUSTUP_HOME", "RUSTUP_TOOLCHAIN"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let output = command
        .args(["--edition", "2021", "--crate-type", "lib", "--crate-name", "guard_case", "--emit=metadata", "--error-format=json"])
        .arg("--out-dir")
        .arg(&case.0)
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--extern")
        .arg(format!("libc={}", libc.display()))
        .arg(case.0.join("lib.rs"))
        .output()
        .unwrap();
    if output.status.success() {
        return Ok(());
    }
    let codes = String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|m| m["level"] == "error")
        .map(|m| m["code"]["code"].as_str().unwrap_or("none").to_string())
        .collect();
    Err(codes)
}

#[test]
fn the_lock_guard_is_required_store_specific_unforgeable_and_stays_on_its_thread() {
    let (rustc, deps, candidates) = toolchain();
    // Several libc builds may share the directory; use one this compiler accepts for the correct use.
    let libc = candidates
        .iter()
        .find(|libc| compile(&rustc, &deps, libc, "allowed", ALLOWED).is_ok())
        .unwrap_or_else(|| panic!("the correct use must compile with one of {candidates:?}"));
    for (index, (what, code, expected)) in REJECTED.iter().enumerate() {
        match compile(&rustc, &deps, libc, &format!("rejected-{index}"), code) {
            Ok(()) => panic!("compiled, but must not: {what}"),
            Err(codes) => assert_eq!(codes.first().map(String::as_str), Some(*expected), "{what}: {codes:?}"),
        }
    }
}
