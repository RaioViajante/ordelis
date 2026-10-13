fn main() {
    // Declaring the app commands makes Tauri ACL-check every app command from the local origin too.
    // Without a manifest a registered command is callable whatever capabilities/default.json says.
    // Keep this list equal to the invoke_handler in src/lib.rs and the permissions in capabilities/default.json.
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["ipc_boundary_status"])),
    )
    .expect("failed to run tauri-build");
}
