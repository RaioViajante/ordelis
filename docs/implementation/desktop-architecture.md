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
    app/                      routes.ts, useRoute.ts (hash router), Shell.tsx, SampleData.tsx, *.test.tsx
    components/               ui.tsx (Button with reason, Evidence, Empty, ...), icons.tsx, SampleBar.tsx
    pages/                    Home, Projects, Sessions, Context, Agents, Settings
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

## Mock data policy

All sample data is in `src/fixtures/mock.ts`, is on by default for layout review, and is always accompanied by a **Sample data** notice that states nothing was read from the machine and nothing is running. It can be hidden from the notice or from Settings; hidden, every page shows a truthful empty state. The Agents page uses no sample data at all: every capability is "unknown", and the Security tab lists what this build cannot do. Every control either performs a local UI action (navigation, selecting a row, switches, tabs, typing) or is disabled with a reason shown on hover and keyboard focus and exposed through `aria-describedby`.

## Tauri and WebView trust boundaries

```text
 macOS user ── Synapse.app (Rust, tauri 2, no plugins, no commands)
                  │  custom protocol serves bundled dist/ only
                  ▼
              WKWebView: React UI ── no IPC call exists ── (no network, no files, no process)
```

- **Capabilities.** `capabilities/default.json` grants the `main` window an empty permission list. The frontend does not depend on `@tauri-apps/api`, `withGlobalTauri` is off, and there are no `#[tauri::command]` functions or `.plugin()` calls, so the webview has no way to invoke native code.
- **CSP.** `default-src 'self'` with `connect-src ipc: http://ipc.localhost` (Tauri's own IPC alias, unused), `img-src 'self' data:`, `font-src 'self'`, `style-src 'self'`, `object-src 'none'`, `frame-src 'none'`, `base-uri 'none'`, `form-action 'none'`. No remote host, no `unsafe-eval` and no `unsafe-inline` in production. The UI therefore sets no inline `style` attributes. Only the development CSP allows the Vite websocket and inline styles.
- **No remote content.** Fonts are bundled; there are no CDN links, analytics or remote images. A test fails if a remote URL appears in the sources or if the Tauri configuration gains a permission, plugin or command.
- **Untrusted data.** Nothing from disk, a provider or the vault reaches the UI in this increment. When it does, it must be treated as untrusted text (the vault excerpts rule in the [MVP plan](synapse-mvp-plan.md) applies) and rendered as text, never as HTML.

## Future Rust-to-Python integration boundaries (not implemented)

The Python runtime stays disconnected. When a later increment connects it, the proposed boundary is:

1. React never reaches Python or the OS directly. It calls a short, closed list of typed Tauri commands, each added to the capability file individually and individually tested.
2. Rust owns validation, path checks and process lifetime. It talks to the runtime over a narrow, versioned contract (for example JSON over a local-only channel), with no general shell, file or process API exposed to the webview.
3. Every command that could cause execution is gated by the existing Execution Safety Gate. Its decisions stay `dispatch_authorized: false` until the separate, reviewed coordinator increment; the UI must display that state rather than infer authorization.
4. State written by the runtime stays in the explicit external private state root, never in the checkout or in browser storage.

Open decision for that increment: a Python sidecar versus invoking an installed `mco`. Neither is chosen here.

## Platform support

macOS is the target and the only platform this increment was built and launched on. The Tauri crate, Vite bundle and test suite are platform-neutral, and icons for `.ico` and Linux are generated, but Windows and Linux are untested. `⌘`/Ctrl shortcuts already handle both; window chrome, fonts and the title bar need a pass on each platform. Mobile targets are not generated. Distribution needs code signing and notarization, which are not configured.

## Security capabilities and permissions

| Capability | State |
| --- | --- |
| Shell, process spawning, sidecars | None. No plugin or command |
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
