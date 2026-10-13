// Deliberately minimal: no plugins, no commands, no sidecar. The webview can
// reach nothing outside its own bundled assets. See docs/implementation/desktop-architecture.md.
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running the Synapse desktop shell");
}
