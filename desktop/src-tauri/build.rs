use std::path::Path;

// The development interpreter is baked in only when the dev-runtime feature is on, so default and release
// builds carry no interpreter or checkout path. Validation of the interpreter happens at run time.
fn dev_runtime() {
    // Cargo watches the path even while it does not exist, so creating or editing the file re-runs this script.
    println!("cargo:rerun-if-changed=.dev-python");
    if std::env::var("PROFILE").as_deref() != Ok("debug") {
        panic!("the dev-runtime feature is development-only and cannot be built with a release profile");
    }
    let Ok(configured) = std::fs::read_to_string(".dev-python") else { return };
    let path = configured.trim();
    // A control character would let the file inject cargo: directives.
    if path.is_empty() || path.chars().any(char::is_control) || !Path::new(path).is_absolute() {
        println!("cargo:warning=.dev-python must hold one absolute interpreter path; the bridge stays unavailable");
        return;
    }
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("checkout root");
    println!("cargo:rustc-env=SYNAPSE_DEV_PYTHON={path}");
    println!("cargo:rustc-env=SYNAPSE_DEV_CHECKOUT={}", checkout.display());
}

fn main() {
    if std::env::var_os("CARGO_FEATURE_DEV_RUNTIME").is_some() {
        dev_runtime();
    }
    // Declaring the app commands makes Tauri ACL-check every app command from the local origin too.
    // Without a manifest a registered command is callable whatever capabilities/default.json says.
    // Keep this list equal to the invoke_handler in src/lib.rs and the permissions in capabilities/default.json.
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["ipc_boundary_status", "get_runtime_status"])),
    )
    .expect("failed to run tauri-build");
}
