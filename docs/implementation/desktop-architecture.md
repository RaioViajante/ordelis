# Desktop application architecture

Foundation for [issue #24](https://github.com/RaioViajante/synapse/issues/24). Synapse stays CLI-first and the Python runtime is unchanged. This increment adds a **disconnected** desktop UI: it shows layout, navigation and honest empty states, and does nothing a user could mistake for execution. See also the [design inventory](desktop-ui-design-inventory.md) and the [visual verification](desktop-visual-verification.md).

## Where it lives

`desktop/` at the repository root, a self-contained pnpm project with its own lockfile and CI workflow (`.github/workflows/desktop.yml`).

- The root `package.json`, its `files` allow-list and `package-lock.json` are untouched, so the inherited `@tt-a1i/mco` npm package neither builds nor publishes any desktop file. The root `.gitignore` is untouched too; `desktop/.gitignore` covers its own build output.
- Existing `gate.yml` still runs the Python, Node, packaging and skill checks unmodified.

```text
desktop/
  package.json  pnpm-lock.yaml  vite.config.ts  tsconfig.json  eslint.config.js  index.html
  src/
    main.tsx                  entry; mounts SampleDataProvider + Shell
    app/                      routes.ts, useRoute.ts (hash router), Shell.tsx, SampleData.tsx, Workspace.tsx, *.test.tsx
    components/               ui.tsx (Button with reason, Evidence, Empty, ...), icons.tsx, SampleBar.tsx
    pages/                    Home, Projects, Sessions, Context, Agents, Settings, Preview (task + draft)
    fixtures/mock.ts          all sample data, in one labelled file
    styles/index.css          tokens and component rules ported from the reference
    assets/fonts/             bundled WOFF2 + provenance README
  src-tauri/
    Cargo.toml  build.rs  tauri.conf.json  Cargo.lock
    src/{main.rs,lib.rs}      builder with no plugins and no commands
    capabilities/default.json no permissions
    icons/                    master (app-icon.png) + generated platform icons
```

## Decisions

- **Tailwind CSS 4** supplies the reset and layout utilities. The reference's own tokens and component rules live in `styles/index.css` under their original names so later reference updates can be diffed against it.
- **shadcn/ui is not used.** The reference is a bespoke editorial system (pills, ruled rows, three type voices) and shadcn's Radix primitives would add dependencies without a component this increment needs. Revisit for dialogs, menus or comboboxes in a later increment.
- **Lucide** is used for glyphs the reference does not define (currently the select chevron). The rail glyphs and the three-node mark come from the reference export itself, which is what the design shows.
- **No router library.** Six top-level pages and no nested routes: `#/home` … `#/settings` with a small hook. Hash routing needs no server rewrite under Tauri's asset protocol.
- **Window.** The native macOS title bar is used instead of the reference's frameless look. An overlay title bar would put the traffic lights over the rail's mark and needs a custom drag region; that is a deliberate later refinement. The window opens at 1280×860, minimum 760×560, with a dark theme and the page background colour to avoid a white flash.

## Navigation and keyboard

The rail is a `<nav>` of links with accessible names and `aria-current`. `⌘/Ctrl + 1…6` switch pages. After a navigation focus moves to the page (`<main tabindex="-1">`). Settings tabs follow the ARIA tab pattern (arrow keys). Layout holds down to the 760 px minimum: multi-column grids collapse at 1100, 1000 and 900 px, and the wide tables scroll horizontally inside their own container.

## Interaction state (issue #26)

`app/Workspace.tsx` is one React context, mounted inside `Shell`, holding the UI state that Home and Projects share: the selected sample project, the task draft, the selected sample context notes and the text for a single polite live region (`role="status"`). It is in memory only (no storage, no persistence, no I/O), so it survives in-app navigation but not a restart. Hiding sample data invalidates its project and context selections, so stale fixture choices do not return when sample data is shown again; the user's draft remains. Changing project also clears the context selection, because notes belong to a project. Repeated live-region messages replace their text node so assistive technology receives each occurrence.

Routes stay hash based and gain parameters: `#/projects/<name>` (opens that project, or shows *Project not found*), `#/task/<id>` (sample task preview, or *Task not found*) and `#/preview` (preview of the current draft). The two sub-pages are not rail destinations; the rail keeps its six, `#/preview` highlights Home and `#/task/…` highlights nothing.

The composer offers **Preview task**, never Start or Run, and the page says so. Preview pages render only the request text, project, agents and a sample status, with a disabled *Open terminals* / *Plan with agents* that states why. There are no logs or progress. Sample project paths such as `~/code/atlas-api` are plain strings and are never resolved. `Workspace.test.tsx` also scans the app sources for Tauri, network, storage and process APIs.

## Context experience (issue #28)

Context reuses the same `WorkspaceProvider`: selection of synthetic notes and history lives there and is shared by Home, Context and the draft Preview. Rules, budget semantics and limits are in [desktop-context-experience.md](desktop-context-experience.md).

## Mock data policy

All sample data is in `src/fixtures/mock.ts`, is on by default for layout review, and is always accompanied by a **Sample data** notice that states nothing was read from the machine and nothing is running. It can be hidden from the notice or from Settings; hidden, every page shows a truthful empty state. The Agents page uses no sample data at all: every capability is "unknown", and the Security tab lists what this build cannot do. Every control either performs a local UI action (navigation, selecting a row, switches, tabs, typing) or is disabled with a reason shown on hover and keyboard focus and exposed through `aria-describedby`.

## Tauri and WebView trust boundaries

```text
 macOS user ── Synapse.app (Rust, tauri 2, no plugins, one boundary-check command, one development-only status command)
                  │  custom protocol serves bundled dist/ only
                  ▼
              WKWebView: React UI ── no IPC call is made ── (no network, no files, no process)
```

- **Capabilities.** `capabilities/default.json` grants the `main` window exactly two permissions, `allow-ipc-boundary-status` and `allow-get-runtime-status`, and nothing else: no shell, filesystem, network, opener or core plugin permission, and no `remote` origin. The frontend does not depend on `@tauri-apps/api` and `withGlobalTauri` is off, so the UI does not call it. The command exists to prove the boundary, not to be used; see [IPC authorization](#ipc-authorization).
- **CSP.** `default-src 'self'` with `connect-src ipc: http://ipc.localhost` (Tauri's own IPC alias, unused), `img-src 'self' data:`, `font-src 'self'`, `style-src 'self'`, `object-src 'none'`, `frame-src 'none'`, `base-uri 'none'`, `form-action 'none'`. No remote host, no `unsafe-eval` and no `unsafe-inline` in production. The UI therefore sets no inline `style` attributes. Only the development CSP allows the Vite websocket and inline styles.
- **No remote content.** Fonts are bundled; there are no CDN links, analytics or remote images. A test fails if a remote URL appears in the sources or if the Tauri configuration gains a permission, plugin or command beyond the two reviewed commands.
- **Untrusted data.** Nothing from disk, a provider or the vault reaches the UI in this increment. When it does, it must be treated as untrusted text (the vault excerpts rule in the [MVP plan](synapse-mvp-plan.md) applies) and rendered as text, never as HTML.

## IPC authorization

Tauri 2.12.3 checks the capability ACL for a **local-origin app command** only when the app declares an ACL manifest (`tauri-2.12.3/src/webview/mod.rs`: the check runs when the call is a plugin command, the app has a manifest, or the origin is not local). Without a manifest, any registered `#[tauri::command]` is callable from the bundled webview whatever `capabilities/default.json` says. An earlier version of this document implied the capability file alone was enough; it is not. This increment therefore declares the manifest before any native bridge exists.

| Piece | Where | Value |
| --- | --- | --- |
| Command | `src-tauri/src/lib.rs` | `ipc_boundary_status`, no arguments, returns `{"schema_version":1,"scope":"ipc_boundary_only"}` |
| Command | `src-tauri/src/lib.rs` | `get_runtime_status`, no arguments, development-only; see [the runtime bridge](desktop-runtime-bridge.md) |
| Manifest | `src-tauri/build.rs` | `AppManifest::new().commands(&["ipc_boundary_status", "get_runtime_status"])` |
| Generated permissions | `src-tauri/permissions/autogenerated/` | `allow-`/`deny-` pairs for each command, written by `tauri-build` and tracked so the grant is reviewable |
| Grant | `src-tauri/capabilities/default.json` | `allow-ipc-boundary-status` and `allow-get-runtime-status` for window `main`, local origin only |

`ipc_boundary_status` reads no file or environment variable, uses no network, spawns nothing, touches no Python and keeps no state. Its response says nothing about runtime health, providers or sessions; it only shows the call crossed the boundary. Adding a command means changing the manifest, the handler, the capability file and `config.test.ts` together; the Rust test `manifest_handler_and_capability_name_the_same_two_commands` fails if they drift.

Rust tests in `src-tauri/src/lib.rs` run against the real generated context and the installed Tauri version with its mock runtime (`cargo test`). They verify that the command is callable from `tauri://localhost` in window `main`; that a registered but unlisted command is rejected; that a window without the grant is rejected; that a remote origin is rejected; that representative core plugin commands (event, window, webview, app, path, resources) are rejected; and that an unknown command is rejected. With the manifest and the grant removed (a mutation check run once, not part of the suite), the unlisted-command, unknown-command, other-window and name-agreement tests fail, so they depend on the enforcement and not on configuration text. The remote-origin and core plugin tests still pass in that mutation because Tauri rejects those without a manifest; they pin behavior that must not regress, not the manifest.

Limitations:

- The tests use Tauri's mock runtime. They exercise the real ACL and the real generated manifest, but not a live WKWebView, so the origin string is supplied by the test rather than by WebKit.
- **Development origin is a trusted boundary, not authentication.** Tauri treats any URL under `devUrl` (`http://localhost:1420`) as the local origin, so in `tauri dev` whatever answers on that port can call `ipc_boundary_status` and `get_runtime_status`, with no credential or process check. That is acceptable only because the first command is fixed and inert, the second is fixed, argument-free, rate-limited to one request and unavailable outside `dev-runtime` builds, and because a developer's loopback is already trusted during development. Release builds serve the bundled protocol instead. No remote origin is configured, and the capability has no `remote` section.
- The "grant removed, manifest present" case is tested by constructing Tauri's runtime authority with an app ACL and no resolved grants (`registered_command_is_denied_when_its_grant_is_removed_and_the_manifest_remains`), not by building a second app without the capability. A companion test pins that without a manifest the same missing grant does not deny the call, which is the Tauri 2.12.3 behavior this design depends on.
- `tauri-build` loads every file under `permissions/` and never deletes stale ones. A stray file only matters if a capability references it, but then it could grant a registered command outside the manifest (reproduced: it makes the unlisted-command test fail). A Rust test therefore requires `permissions/` to hold exactly the generated pair for each declared command, and CI fails if regenerating changes or adds a tracked permission file.
- A compromised Rust process, a malicious dependency or a compromised dev machine is out of scope.

## Future Rust-to-Python integration boundaries (not implemented)

The Python runtime is connected only through the development-only identity query in [the runtime bridge](desktop-runtime-bridge.md). When a later increment connects more, the proposed boundary is:

1. React never reaches Python or the OS directly. It calls a short, closed list of typed Tauri commands, each declared in the app manifest and the capability file individually and individually tested (see [IPC authorization](#ipc-authorization); the capability file alone does not protect an app command).
2. Rust owns validation, path checks and process lifetime. It talks to the runtime over a narrow, versioned contract (for example JSON over a local-only channel), with no general shell, file or process API exposed to the webview.
3. Every command that could cause execution is gated by the existing Execution Safety Gate. Its decisions stay `dispatch_authorized: false` until the separate, reviewed coordinator increment; the UI must display that state rather than infer authorization.
4. State written by the runtime stays in the explicit external private state root, never in the checkout or in browser storage.

Open decision for that increment: a Python sidecar versus invoking an installed `mco`. Neither is chosen here.

## Platform support

macOS is the target and the only platform this increment was built and launched on. The Tauri crate, Vite bundle and test suite are platform-neutral, and icons for `.ico` and Linux are generated, but Windows and Linux are untested. `⌘`/Ctrl shortcuts already handle both; window chrome, fonts and the title bar need a pass on each platform. Mobile targets are not generated. Distribution needs code signing and notarization, which are not configured.

## Security capabilities and permissions

| Capability | State |
| --- | --- |
| Shell, process spawning, sidecars | None in default and release builds. No plugin. With the development-only `dev-runtime` feature, `get_runtime_status` starts one fixed Python identity query ([details](desktop-runtime-bridge.md)) |
| Filesystem access from React | None |
| Network and remote content | None; CSP forbids it |
| Opening links or paths | None; the opener plugin is not installed |
| Provider authentication, credentials, vault | None |
| Execution Safety Gate | Unchanged; the desktop code does not import or call it |

## Asset provenance and branding

| Asset | Source | Handling |
| --- | --- | --- |
| `src-tauri/icons/app-icon.png` (1024²) | `reference/icon.png`, the supplied desktop icon | Kept only the largest connected opaque region (the squircle; the faint stray pixels around it have alpha ≤ 8), cropped to its bounds and centred on a transparent 1024 canvas with its long side at 832 px (81%), preserving its 1.04:1 aspect. No recolouring, redrawing or resampling beyond the fit. Reproduce it with `desktop/scripts/prepare-app-icon.swift`. |
| Platform icons (`icon.icns`, `icon.ico`, `icon.png`, `32×32`, `64×64`, `128×128`, `128×128@2x`) | Generated from the master with `tauri icon` | Android, iOS and Windows Store variants were deleted because they are not targeted. |
| Rail mark and rail glyphs | SVG paths in `Synapse — telas.html` | Reproduced as inline SVG in `components/icons.tsx`. The rail uses the reference's ivory three-node mark rather than the amber raster, because that is what the design shows at 22 px. |
| `icon-remove-gb.png` | supplied | Not used (visible cut-out noise). |
| Fonts | extracted from the HTML export | The three tracked WOFF2 files match the export resources by SHA-256. Copyright notices and the complete SIL OFL 1.1 text are tracked in `src/assets/fonts/FONT-LICENSES.txt` and included as a Tauri bundle resource; see `src/assets/fonts/README.md`. |

The reference exports (PDF, HTML, PNGs, handoff) are not copied into the repository.
