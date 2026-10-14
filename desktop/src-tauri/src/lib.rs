// Deliberately minimal: no plugins, no sidecar, one inert command that proves the IPC boundary, one
// development-only runtime status command (see runtime_bridge.rs) and four local project registration
// commands (see projects.rs), none of which takes a path.
// Authorization is enforced by the app manifest in build.rs plus capabilities/default.json.
// See docs/implementation/desktop-architecture.md.
mod projects;
mod runtime_bridge;

use projects::{AddOutcome, Picker, ProjectError, ProjectList, Projects, RemoveOutcome, ResetOutcome};
use runtime_bridge::{BridgeError, ErrorCode, RuntimeStatus};
use serde::Serialize;
use tauri::{Manager, Runtime, State, WebviewWindow};

#[derive(Serialize)]
struct IpcBoundaryStatus {
    schema_version: u8,
    scope: &'static str,
}

/// Fixed response used to verify that authorization is enforced. It reports nothing about the
/// runtime, providers, sessions or the machine, and takes no input.
#[tauri::command]
fn ipc_boundary_status() -> IpcBoundaryStatus {
    IpcBoundaryStatus { schema_version: 1, scope: "ipc_boundary_only" }
}

/// Takes no input: the interpreter, argv and environment are fixed by the build, never by the frontend.
/// Runs on a blocking worker so the UI thread never waits for Python.
#[tauri::command]
async fn get_runtime_status() -> Result<RuntimeStatus, BridgeError> {
    tauri::async_runtime::spawn_blocking(runtime_bridge::query)
        .await
        .unwrap_or_else(|_| Err(ErrorCode::RuntimeFailed.into()))
}

/// Lists registrations with a status revalidated now. Never writes the registry.
#[tauri::command]
async fn list_projects(projects: State<'_, Projects>) -> Result<ProjectList, ProjectError> {
    projects.list().await
}

/// Takes no input: Rust opens the folder panel on the calling window, which must be "main". The user's choice
/// in the panel is the consent; the frontend never supplies a path.
#[tauri::command]
async fn add_project<R: Runtime>(
    window: WebviewWindow<R>,
    projects: State<'_, Projects>,
    picker: State<'_, Picker<R>>,
) -> Result<AddOutcome, ProjectError> {
    projects.add(&window, &picker).await
}

/// `id` is looked up, never resolved; the registered folder is not touched.
#[tauri::command]
async fn remove_project(id: String, projects: State<'_, Projects>) -> Result<RemoveOutcome, ProjectError> {
    projects.remove(id).await
}

/// Acts only on a registry that is corrupt under the lock, and keeps the corrupt file.
#[tauri::command]
async fn reset_projects(projects: State<'_, Projects>) -> Result<ResetOutcome, ProjectError> {
    projects.reset().await
}

// generate_context! may expand only once per crate; the tests build the app from this same context.
fn context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(Projects::new(app.path().app_data_dir().ok()));
            app.manage(Picker::<tauri::Wry>::native());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc_boundary_status,
            get_runtime_status,
            list_projects,
            add_project,
            remove_project,
            reset_projects
        ])
        .run(context())
        .expect("error while running the Synapse desktop shell");
}

#[cfg(test)]
mod tests {
    use super::*;
    use projects::{FolderPicker, PickedFolder};
    use std::path::{Path, PathBuf};
    use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    use tauri::{ipc::{CallbackFn, InvokeBody}, Manager, WebviewWindowBuilder};

    // A registered command that is deliberately absent from the app manifest and the capability file.
    #[tauri::command]
    fn unlisted_probe() -> &'static str {
        "reached"
    }

    const LOCAL: &str = "tauri://localhost";

    fn request(cmd: &str, url: &str) -> InvokeRequest {
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: url.parse().unwrap(),
            body: InvokeBody::default(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        }
    }

    // Stands in for the native folder panel; the registry directory is always a test directory (or none),
    // never the real app data directory.
    struct FakePicker(Box<dyn Fn() -> PickedFolder + Send + Sync>);

    impl<R: Runtime> FolderPicker<R> for FakePicker {
        fn pick(&self, _parent: &WebviewWindow<R>) -> PickedFolder {
            (self.0)()
        }
    }

    fn cancels() -> FakePicker {
        FakePicker(Box::new(|| Box::pin(std::future::ready(None))))
    }

    fn picks(folder: &Path) -> FakePicker {
        let folder = folder.to_path_buf();
        FakePicker(Box::new(move || Box::pin(std::future::ready(Some(folder.clone())))))
    }

    // Uses the real generated context, so the manifest and capability come from build.rs and
    // capabilities/*.json exactly as in the shipped app.
    fn app() -> tauri::App<MockRuntime> {
        app_with(None, cancels())
    }

    fn app_with(data_dir: Option<PathBuf>, picker: FakePicker) -> tauri::App<MockRuntime> {
        mock_builder()
            .manage(Projects::new(data_dir))
            .manage(Picker::<MockRuntime>::new(Box::new(picker)))
            .invoke_handler(tauri::generate_handler![
                ipc_boundary_status,
                get_runtime_status,
                list_projects,
                add_project,
                remove_project,
                reset_projects,
                unlisted_probe
            ])
            .build(context())
            .unwrap()
    }

    fn webview(app: &tauri::App<tauri::test::MockRuntime>, label: &str) -> tauri::WebviewWindow<tauri::test::MockRuntime> {
        app.get_webview_window(label)
            .unwrap_or_else(|| WebviewWindowBuilder::new(app, label, Default::default()).build().unwrap())
    }

    fn call(label: &str, cmd: &str, url: &str) -> Result<serde_json::Value, serde_json::Value> {
        let app = app();
        let window = webview(&app, label);
        get_ipc_response(&window, request(cmd, url))
            .map(|body| body.deserialize::<serde_json::Value>().unwrap())
    }

    fn rejected(result: Result<serde_json::Value, serde_json::Value>) -> String {
        result.expect_err("command must be rejected").to_string()
    }

    #[test]
    fn allowlisted_command_is_callable_from_the_local_origin() {
        let value = call("main", "ipc_boundary_status", LOCAL).unwrap();
        assert_eq!(value, serde_json::json!({ "schema_version": 1, "scope": "ipc_boundary_only" }));
    }

    // An allowed call may still fail inside the command; what matters here is that the ACL let it through.
    #[test]
    fn runtime_status_command_is_callable_from_the_local_origin() {
        let _guard = runtime_bridge::GUARD_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let result = call("main", "get_runtime_status", LOCAL);
        #[cfg(not(feature = "dev-runtime"))]
        assert_eq!(
            result.unwrap_err(),
            serde_json::json!({ "code": "runtime_not_installed", "message": "Development runtime is unavailable." })
        );
        #[cfg(feature = "dev-runtime")]
        assert!(!rejected_by_acl(&result), "{result:?}");
    }

    #[cfg(feature = "dev-runtime")]
    fn rejected_by_acl(result: &Result<serde_json::Value, serde_json::Value>) -> bool {
        matches!(result, Err(e) if e.to_string().contains("not allowed"))
    }

    #[test]
    fn runtime_status_command_is_denied_to_other_windows_and_remote_origins() {
        let other_window = rejected(call("other", "get_runtime_status", LOCAL));
        assert!(other_window.contains("not allowed"), "{other_window}");
        let remote = rejected(call("main", "get_runtime_status", "https://example.com/"));
        assert!(remote.contains("not allowed"), "{remote}");
    }

    // The command has no parameters, so a hostile payload has nothing to bind to.
    #[test]
    fn runtime_status_ignores_any_frontend_supplied_arguments() {
        let _guard = runtime_bridge::GUARD_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let app = app();
        let window = webview(&app, "main");
        let mut hostile = request("get_runtime_status", LOCAL);
        hostile.body = InvokeBody::Json(serde_json::json!({
            "python": "/bin/sh", "path": "/tmp", "args": ["-c", "id"], "env": { "PATH": "/tmp" }, "code": "print(1)"
        }));
        let plain = get_ipc_response(&window, request("get_runtime_status", LOCAL));
        let with_payload = get_ipc_response(&window, hostile);
        #[cfg(not(feature = "dev-runtime"))]
        assert_eq!(
            plain.map(|b| b.deserialize::<serde_json::Value>().unwrap()),
            with_payload.map(|b| b.deserialize::<serde_json::Value>().unwrap())
        );
        #[cfg(feature = "dev-runtime")]
        assert_eq!(plain.is_ok(), with_payload.is_ok());
    }

    #[test]
    fn registered_but_unlisted_command_is_rejected() {
        let message = rejected(call("main", "unlisted_probe", LOCAL));
        assert!(message.contains("not allowed"), "{message}");
    }

    #[test]
    fn command_is_rejected_for_a_window_without_the_permission() {
        let message = rejected(call("other", "ipc_boundary_status", LOCAL));
        assert!(message.contains("not allowed"), "{message}");
    }

    #[test]
    fn remote_origin_cannot_invoke_the_command() {
        let message = rejected(call("main", "ipc_boundary_status", "https://example.com/"));
        assert!(message.contains("not allowed"), "{message}");
    }

    #[test]
    fn core_and_plugin_commands_stay_restricted() {
        for cmd in [
            "plugin:event|listen",
            "plugin:event|emit",
            "plugin:window|close",
            "plugin:webview|create_webview_window",
            "plugin:app|version",
            "plugin:path|resolve_directory",
            "plugin:resources|close",
            // No dialog, filesystem, shell or opener plugin is registered or granted.
            "plugin:dialog|open",
            "plugin:fs|read_dir",
            "plugin:fs|read_file",
            "plugin:shell|execute",
            "plugin:opener|open_path",
        ] {
            let message = rejected(call("main", cmd, LOCAL));
            assert!(message.contains("not allowed"), "{cmd}: {message}");
        }
    }

    #[test]
    fn unknown_application_commands_are_not_implicitly_authorized() {
        let message = rejected(call("main", "does_not_exist", LOCAL));
        assert!(message.contains("not allowed"), "{message}");
    }

    // Models the state "AppManifest present, capability grant removed": the authority has an app ACL
    // but resolves no allowed command. Built from Tauri's public (doc-hidden) RuntimeAuthority
    // constructor, so it is the same lookup the real context uses, but the grant is absent by
    // construction rather than by editing capabilities/default.json.
    fn call_without_grant(has_app_acl: bool, cmd: &str) -> Result<serde_json::Value, serde_json::Value> {
        use tauri::ipc::RuntimeAuthority;
        use tauri::utils::acl::resolved::Resolved;
        let mut ctx = context();
        let resolved = Resolved { has_app_acl, ..Default::default() };
        let authority = RuntimeAuthority::new(Default::default(), resolved);
        *ctx.runtime_authority_mut() = authority;
        let app = mock_builder()
            .manage(Projects::new(None))
            .manage(Picker::<MockRuntime>::new(Box::new(cancels())))
            .invoke_handler(tauri::generate_handler![
                ipc_boundary_status,
                get_runtime_status,
                list_projects,
                add_project,
                remove_project,
                reset_projects,
                unlisted_probe
            ])
            .build(ctx)
            .unwrap();
        let window = webview(&app, "main");
        get_ipc_response(&window, request(cmd, LOCAL)).map(|body| body.deserialize::<serde_json::Value>().unwrap())
    }

    #[test]
    fn registered_command_is_denied_when_its_grant_is_removed_and_the_manifest_remains() {
        let message = rejected(call_without_grant(true, "ipc_boundary_status"));
        assert!(message.contains("not allowed"), "{message}");
    }

    #[test]
    fn runtime_status_is_denied_when_its_grant_is_removed_and_the_manifest_remains() {
        let message = rejected(call_without_grant(true, "get_runtime_status"));
        assert!(message.contains("not allowed"), "{message}");
    }

    // Pins the Tauri 2.12.3 behavior this design depends on: with no app ACL the same missing grant
    // does not stop a local-origin app command. If an upgrade changes this, revisit docs and tests.
    #[test]
    fn without_a_manifest_a_missing_grant_does_not_deny_a_local_app_command() {
        assert!(call_without_grant(false, "ipc_boundary_status").is_ok());
    }

    // tauri-build loads every file under permissions/ and never deletes stale ones, so an extra file
    // could be referenced by a later capability edit to grant a command outside the manifest.
    #[test]
    fn permissions_directory_holds_only_the_generated_pairs_for_the_declared_commands() {
        fn files(dir: &std::path::Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    files(&path, out);
                } else {
                    out.push(path.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap().to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let mut found = Vec::new();
        files(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("permissions"), &mut found);
        found.sort();
        let mut commands = COMMANDS.to_vec();
        commands.sort();
        let expected: Vec<String> = commands.iter().map(|c| format!("permissions/autogenerated/{c}.toml")).collect();
        assert_eq!(found, expected);
        for (file, command) in found.iter().zip(commands) {
            let slug = command.replace('_', "-");
            let toml = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file)).unwrap();
            let identifiers: Vec<&str> = toml.lines().filter(|l| l.starts_with("identifier")).collect();
            assert_eq!(identifiers, [format!("identifier = \"allow-{slug}\""), format!("identifier = \"deny-{slug}\"")]);
            let commands: Vec<&str> = toml.lines().filter(|l| l.starts_with("commands.")).collect();
            assert_eq!(commands, [format!("commands.allow = [\"{command}\"]"), format!("commands.deny = [\"{command}\"]")]);
        }
    }

    fn quoted_names(text: &str, open: &str) -> Vec<String> {
        let start = text.find(open).unwrap_or_else(|| panic!("{open} not found")) + open.len();
        let list = &text[start..text[start..].find([']', ')']).unwrap() + start];
        list.split(',').map(|s| s.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty()).collect()
    }

    const COMMANDS: [&str; 6] =
        ["ipc_boundary_status", "get_runtime_status", "list_projects", "add_project", "remove_project", "reset_projects"];

    #[test]
    fn manifest_handler_and_capability_name_the_same_six_commands() {
        let build = include_str!("../build.rs");
        let manifest = quoted_names(build, ".commands(&[");
        let source = include_str!("lib.rs");
        let production = &source[..source.find("#[cfg(test)]").unwrap()];
        let handler = quoted_names(production, "generate_handler![");
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let permissions: Vec<String> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_string())
            .collect();

        assert_eq!(manifest, COMMANDS);
        assert_eq!(handler, manifest);
        let grants: Vec<String> = COMMANDS.iter().map(|c| format!("allow-{}", c.replace('_', "-"))).collect();
        assert_eq!(permissions, grants);
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
        assert!(capability.get("remote").is_none());
    }

    // Local project registration commands (projects.rs). Registry behavior itself is covered by
    // projects/registry/tests.rs; these prove the IPC boundary and the panel guard.

    const PROJECT_COMMANDS: [&str; 4] = ["list_projects", "add_project", "remove_project", "reset_projects"];

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("synapse-ipc-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir(&dir).unwrap();
            std::fs::create_dir(dir.join("folder")).unwrap();
            Scratch(dir.canonicalize().unwrap())
        }

        fn data(&self) -> PathBuf {
            self.0.join("data")
        }

        fn folder(&self) -> PathBuf {
            self.0.join("folder")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn invoke(
        window: &WebviewWindow<MockRuntime>,
        cmd: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, serde_json::Value> {
        let mut request = request(cmd, LOCAL);
        request.body = InvokeBody::Json(body);
        get_ipc_response(window, request).map(|b| b.deserialize::<serde_json::Value>().unwrap())
    }

    fn error(code: projects::ErrorCode) -> serde_json::Value {
        serde_json::to_value(ProjectError::from(code)).unwrap()
    }

    #[test]
    fn project_commands_are_callable_from_main_at_the_local_origin() {
        let _guard = projects::GUARD_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let scratch = Scratch::new("allowed");
        let app = app_with(Some(scratch.data()), picks(&scratch.folder()));
        let main = webview(&app, "main");
        let none = serde_json::json!({});
        assert_eq!(invoke(&main, "list_projects", none.clone()), Ok(serde_json::json!({ "schema_version": 1, "projects": [] })));

        let added = invoke(&main, "add_project", none.clone()).unwrap();
        assert_eq!(added["outcome"], "added");
        let project = &added["project"];
        assert_eq!((project["id"].as_str(), project["name"].as_str(), project["status"].as_str()), (Some("p1"), Some("folder"), Some("available")));
        let mut keys: Vec<&str> = project.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["id", "name", "path_display", "registered_at_ms", "status"], "no device or inode numbers");

        let again = invoke(&main, "add_project", none.clone()).unwrap();
        assert_eq!((again["outcome"].as_str(), &again["project"]), (Some("already_registered"), project));
        assert_eq!(invoke(&main, "list_projects", none.clone()).unwrap()["projects"], serde_json::json!([project]));
        assert_eq!(invoke(&main, "remove_project", serde_json::json!({ "id": "p1" })), Ok(serde_json::json!({ "outcome": "removed" })));
        assert_eq!(invoke(&main, "remove_project", serde_json::json!({ "id": "p1" })), Ok(serde_json::json!({ "outcome": "not_found" })));
        assert_eq!(invoke(&main, "reset_projects", none), Ok(serde_json::json!({ "outcome": "not_needed" })));
        assert!(scratch.folder().is_dir() && std::fs::read_dir(scratch.folder()).unwrap().next().is_none());
    }

    #[test]
    fn project_commands_are_denied_to_other_windows_remote_origins_and_without_their_grant() {
        for cmd in PROJECT_COMMANDS {
            let other_window = rejected(call("other", cmd, LOCAL));
            assert!(other_window.contains("not allowed"), "{cmd}: {other_window}");
            let remote = rejected(call("main", cmd, "https://example.com/"));
            assert!(remote.contains("not allowed"), "{cmd}: {remote}");
            let ungranted = rejected(call_without_grant(true, cmd));
            assert!(ungranted.contains("not allowed"), "{cmd}: {ungranted}");
        }
    }

    // A hostile payload has nothing to bind to: add_project and reset_projects take no argument, and a path is
    // never read from the body. With the panel cancelled, nothing is created.
    #[test]
    fn argument_free_project_commands_ignore_any_payload() {
        let _guard = projects::GUARD_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let scratch = Scratch::new("hostile");
        let app = app_with(Some(scratch.data()), cancels());
        let main = webview(&app, "main");
        let hostile = serde_json::json!({ "path": "/etc", "folder": "/", "dir": "..", "id": "p1", "data_dir": "/tmp" });
        for cmd in ["add_project", "reset_projects", "list_projects"] {
            assert_eq!(invoke(&main, cmd, hostile.clone()), invoke(&main, cmd, serde_json::json!({})), "{cmd}");
        }
        assert_eq!(invoke(&main, "add_project", hostile), Ok(serde_json::json!({ "outcome": "cancelled" })));
        assert!(!scratch.data().exists(), "a cancelled panel creates no directory, lock or registry");
    }

    #[test]
    fn remove_project_rejects_malformed_ids_and_ignores_extra_keys() {
        let scratch = Scratch::new("remove-input");
        let app = app_with(Some(scratch.data()), cancels());
        let main = webview(&app, "main");
        for bad in ["", "p0", "p01", "P1", "../projects.json", "/etc/hosts", "p12345678901", "p1 "] {
            assert_eq!(invoke(&main, "remove_project", serde_json::json!({ "id": bad })), Err(error(projects::ErrorCode::InvalidInput)), "{bad:?}");
        }
        // Missing or non-string ids never reach the command; Tauri's message names only the command and key.
        for body in [serde_json::json!({}), serde_json::json!({ "id": 1 }), serde_json::json!({ "id": null }), serde_json::json!({ "path": "/etc" })] {
            let message = invoke(&main, "remove_project", body.clone()).unwrap_err().to_string();
            assert!(message.contains("remove_project") && message.contains("id") && !message.contains("/etc"), "{body}: {message}");
        }
        assert!(!scratch.data().exists());
        let extra = serde_json::json!({ "id": "p7", "path": "/etc", "projects": [] });
        assert_eq!(invoke(&main, "remove_project", extra), Ok(serde_json::json!({ "outcome": "not_found" })));
    }

    #[test]
    fn a_corrupt_registry_is_reported_over_ipc_and_reset_preserves_it() {
        let _guard = projects::GUARD_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let scratch = Scratch::new("corrupt");
        std::fs::create_dir(scratch.data()).unwrap();
        std::fs::set_permissions(scratch.data(), std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        let corrupt = b"{\"schema_version\":1,\"next_id\":\"oops\"}";
        std::fs::write(scratch.data().join("projects.json"), corrupt).unwrap();
        let app = app_with(Some(scratch.data()), picks(&scratch.folder()));
        let main = webview(&app, "main");
        let none = serde_json::json!({});
        assert_eq!(invoke(&main, "list_projects", none.clone()), Err(error(projects::ErrorCode::RegistryCorrupt)));
        assert_eq!(invoke(&main, "add_project", none.clone()), Err(error(projects::ErrorCode::RegistryCorrupt)));
        assert_eq!(invoke(&main, "remove_project", serde_json::json!({ "id": "p1" })), Err(error(projects::ErrorCode::RegistryCorrupt)));
        assert_eq!(std::fs::read(scratch.data().join("projects.json")).unwrap(), corrupt);

        assert_eq!(invoke(&main, "reset_projects", none.clone()), Ok(serde_json::json!({ "outcome": "reset" })));
        let backups: Vec<PathBuf> = std::fs::read_dir(scratch.data())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.file_name().unwrap().to_str().unwrap().starts_with("projects.json.corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(std::fs::read(&backups[0]).unwrap(), corrupt);
        assert_eq!(invoke(&main, "list_projects", none.clone()), Ok(serde_json::json!({ "schema_version": 1, "projects": [] })));
        assert_eq!(invoke(&main, "reset_projects", none), Ok(serde_json::json!({ "outcome": "not_needed" })));
    }

    #[test]
    fn an_unresolved_data_directory_is_unavailable() {
        let app = app_with(None, cancels());
        let main = webview(&app, "main");
        for (cmd, body) in [
            ("list_projects", serde_json::json!({})),
            ("remove_project", serde_json::json!({ "id": "p1" })),
            ("reset_projects", serde_json::json!({})),
        ] {
            assert_eq!(invoke(&main, cmd, body), Err(error(projects::ErrorCode::RegistryUnavailable)), "{cmd}");
        }
        let _guard = projects::GUARD_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(invoke(&main, "add_project", serde_json::json!({})), Err(error(projects::ErrorCode::RegistryUnavailable)));
    }

    // A panel that stays open until the test releases it with the given outcome.
    fn held() -> (FakePicker, std::sync::mpsc::Sender<Option<PathBuf>>) {
        let (release, outcome) = std::sync::mpsc::channel::<Option<PathBuf>>();
        let outcome = std::sync::Arc::new(std::sync::Mutex::new(outcome));
        let picker = FakePicker(Box::new(move || {
            let outcome = outcome.clone();
            Box::pin(async move {
                tauri::async_runtime::spawn_blocking(move || outcome.lock().unwrap().recv().unwrap()).await.unwrap()
            })
        }));
        (picker, release)
    }

    fn wait_until(condition: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !condition() {
            assert!(std::time::Instant::now() < deadline, "timed out");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn only_one_folder_panel_is_open_at_a_time() {
        let _guard = projects::GUARD_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let scratch = Scratch::new("busy");
        let (picker, release) = held();
        let app = app_with(Some(scratch.data()), picker);
        let main = webview(&app, "main");
        let first = {
            let main = main.clone();
            std::thread::spawn(move || invoke(&main, "add_project", serde_json::json!({})))
        };
        wait_until(projects::selecting);
        assert_eq!(invoke(&main, "add_project", serde_json::json!({})), Err(error(projects::ErrorCode::SelectionInProgress)));
        // The registry is not locked while the panel is open.
        assert_eq!(invoke(&main, "list_projects", serde_json::json!({})), Ok(serde_json::json!({ "schema_version": 1, "projects": [] })));
        release.send(Some(scratch.folder())).unwrap();
        assert_eq!(first.join().unwrap().unwrap()["outcome"], "added");
        assert!(!projects::selecting());

        let second = {
            let main = main.clone();
            std::thread::spawn(move || invoke(&main, "add_project", serde_json::json!({})))
        };
        wait_until(projects::selecting);
        release.send(None).unwrap();
        assert_eq!(second.join().unwrap(), Ok(serde_json::json!({ "outcome": "cancelled" })));
        assert!(!projects::selecting());
    }

    // rfd cannot dismiss a sheet, and dropping its future leaves the sheet up, so a dropped call must not free
    // the guard while the panel is still open: another call would put a second sheet on the window.
    #[test]
    fn a_dropped_call_keeps_the_guard_until_its_panel_closes_and_registers_nothing() {
        let _guard = projects::GUARD_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let scratch = Scratch::new("dropped");
        let (panel, release) = held();
        let app = app_with(Some(scratch.data()), cancels());
        let main = webview(&app, "main");
        let dropped = {
            let (main, data) = (main.clone(), scratch.data());
            tauri::async_runtime::spawn(async move {
                Projects::new(Some(data)).add(&main, &Picker::new(Box::new(panel))).await
            })
        };
        wait_until(projects::selecting);
        dropped.abort();
        wait_until(|| dropped.inner().is_finished());
        assert!(projects::selecting(), "the panel is still open, so the guard must be held");
        assert_eq!(invoke(&main, "add_project", serde_json::json!({})), Err(error(projects::ErrorCode::SelectionInProgress)));

        // The user picks a folder in the orphaned panel: the guard is released and nothing is registered.
        release.send(Some(scratch.folder())).unwrap();
        wait_until(|| !projects::selecting());
        assert!(!scratch.data().exists());
        assert_eq!(invoke(&main, "add_project", serde_json::json!({})), Ok(serde_json::json!({ "outcome": "cancelled" })));
    }

    #[test]
    fn a_cancelled_panel_releases_the_guard_for_the_next_call() {
        let _guard = projects::GUARD_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let scratch = Scratch::new("cancel-release");
        let (panel, release) = held();
        let app = app_with(Some(scratch.data()), panel);
        let main = webview(&app, "main");
        for _ in 0..3 {
            let call = {
                let main = main.clone();
                std::thread::spawn(move || invoke(&main, "add_project", serde_json::json!({})))
            };
            wait_until(projects::selecting);
            release.send(None).unwrap();
            assert_eq!(call.join().unwrap(), Ok(serde_json::json!({ "outcome": "cancelled" })));
            assert!(!projects::selecting());
        }
        assert!(!scratch.data().exists());
    }

    // The capability already confines the command to "main"; the label check is a second, independent refusal
    // that also keeps the panel off any other window.
    #[test]
    fn add_project_refuses_any_window_but_main_before_opening_the_panel() {
        let _guard = projects::GUARD_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let app = app();
        let other = webview(&app, "other");
        let opened = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = opened.clone();
        let picker = Picker::new(Box::new(FakePicker(Box::new(move || {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            Box::pin(std::future::ready(None))
        }))));
        let result = tauri::async_runtime::block_on(Projects::new(None).add(&other, &picker));
        assert_eq!(result, Err(projects::ErrorCode::NotAllowed.into()));
        assert!(!opened.load(std::sync::atomic::Ordering::SeqCst));
    }
}
