// Deliberately minimal: no plugins, no sidecar, one in-process command that only proves the IPC boundary.
// Authorization is enforced by the app manifest in build.rs plus capabilities/default.json.
// See docs/implementation/desktop-architecture.md.
use serde::Serialize;

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

// generate_context! may expand only once per crate; the tests build the app from this same context.
fn context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ipc_boundary_status])
        .run(context())
        .expect("error while running the Synapse desktop shell");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
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

    // Uses the real generated context, so the manifest and capability come from build.rs and
    // capabilities/*.json exactly as in the shipped app.
    fn app() -> tauri::App<tauri::test::MockRuntime> {
        mock_builder()
            .invoke_handler(tauri::generate_handler![ipc_boundary_status, unlisted_probe])
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

    fn quoted_names(text: &str, open: &str) -> Vec<String> {
        let start = text.find(open).unwrap_or_else(|| panic!("{open} not found")) + open.len();
        let list = &text[start..text[start..].find([']', ')']).unwrap() + start];
        list.split(',').map(|s| s.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty()).collect()
    }

    #[test]
    fn manifest_handler_and_capability_name_the_same_single_command() {
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

        assert_eq!(manifest, ["ipc_boundary_status"]);
        assert_eq!(handler, manifest);
        assert_eq!(permissions, ["allow-ipc-boundary-status"]);
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
        assert!(capability.get("remote").is_none());
    }
}
