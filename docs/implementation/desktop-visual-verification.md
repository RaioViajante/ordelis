# Desktop visual verification

Evidence for [issue #24](https://github.com/RaioViajante/synapse/issues/24). A successful build proves nothing about fidelity, so this compares the running app against the supplied references.

**Method.** The debug `Synapse.app` (`pnpm tauri build --debug --bundles app`) was launched on macOS and its real window captured with `screencapture`. For Home and Projects the window was sized to a 1440×1000 viewport (the export's preview size), cropped below the native title bar, and laid side by side with the reference PNG (`telas-png/Home.png`, `Projects.png`) at equal width. Context, Agents, Sessions and Settings were compared by eye against PDF pages 3–6. The side-by-side images were produced locally and are not committed because they contain reference pixels.

| Screenshot | What it shows |
| --- | --- |
| [`home-1440.png`](../assets/desktop/home-1440.png) | Home with sample data, 1440 wide |
| [`projects-1440.png`](../assets/desktop/projects-1440.png) | Projects with sample data |
| [`sessions-empty-1440.png`](../assets/desktop/sessions-empty-1440.png) | An empty state: Sessions with sample data hidden |
| [`home-900.png`](../assets/desktop/home-900.png) | A narrower window (900×700) |

The window also launched and was checked at its 760×560 minimum.

## Home against reference page 1

Matches within a few pixels: window, rail and page surfaces and radii; rail icon positions and active plate; serif title size and weight; composer width (840), radius and textarea; amber primary button; section headings; row rules and spacing; provenance and kind chips; agent squares and the status colours; mono agent names and project labels; column positions of the task table.

Differences, in order of visibility:

1. **Native title bar.** The reference is frameless. This is deliberate (see the architecture document).
2. **Sample data notice.** A persistent footer at the bottom of the sample pages is not in the reference. It exists so that no fixture reads as real or live; it has its own space and does not cover the scrolling content.
3. **Composer toolbar.** About 7 px taller than the reference after adding its hairline divider, so everything below sits slightly lower.
4. **"Open inbox →"** is not underlined: it is disabled (the inbox is not built), and a disabled control should not look like a live link.
5. **Select chevron.** A 14 px Lucide chevron stands in for the reference's native-style chevron; position and weight differ slightly.
6. **Text rendering.** The reference was rendered by Chrome; the app uses the system WKWebView, so glyph hinting and weight look marginally different at the same sizes.
7. **Scrollbar.** The page scrolls inside the window (thin dark scrollbar). The reference pages are static.

## Other pages

- **Projects** follows reference page 2 closely. The Context "Access" row reads "○ read-only · by design, not checked" instead of the reference's verified "● read-only", because nothing is checked.
- **Sessions** follows page 3, with "Requires" cells changed from verified or advertised claims to "not checked", and the action buttons disabled with reasons.
- **Context** follows page 4. Search and re-index are disabled; the toggle and limit selects are local-only and live inside the sample data.
- **Agents is intentionally different from page 5.** It keeps the evidence legend and table structure but shows every fact as unknown, because the reference's installed versions, sign-in state and verified capabilities would be fabricated. Kimi is shown without an adapter, as the MVP plan records.
- **Settings is intentionally different from page 6.** The Security tab shows what this build cannot do instead of "Policy v3". Preferences shows a theme chip, the sample-data switch and keyboard shortcuts instead of agent and workspace selects, which would imply settings that do nothing.
- **Empty states** use the wording and structure of reference page 19 ("Set up Synapse" on Home, "No vault connected" on Context) with disabled actions that state their reason.

Not compared: reference pages 7–18 and 20 (task flow, inbox, workflows) are outside this increment.

## Resizing

At 900 px the sessions table drops its Project and Worktree columns, the task table drops project and agents, stage strips go two-up, and grids become single column. The pick-up comparison table keeps its 900 px width and scrolls horizontally inside its own container. At the 760 px minimum the sample notice takes about 14% of the window height, while the page content scrolls in a separate region above it. Compact vertical spacing keeps the composer and the next section heading visible without overlap. No layout was checked below 760 px because the window cannot be smaller.

## Missing fonts, spacing and elements

- Only the Latin subsets in the export are bundled. Non-Latin text and Newsreader italics fall back to system fonts (no implemented screen uses italics).
- The disabled-control reason tooltips were not captured in a screenshot. They are covered by component tests: unavailable buttons remain in the keyboard order, expose their reason through `aria-describedby`, and suppress pointer and keyboard activation. Hover and focus CSS reveal the same reason visually.
- The reference's rail has no separate inbox tooltip or disabled state; the app adds both.
- Windows and Linux were not built or launched.

## Interactive Home and Projects ([issue #26](https://github.com/RaioViajante/synapse/issues/26))

**What was captured, and how.** The rebuilt debug `Synapse.app` was launched and its real 1280×860 WKWebView window captured with `screencapture -l` (window only): [`home-interactive-app.png`](../assets/desktop/home-interactive-app.png). macOS denied UI-automation access to the Synapse app, so the native window was not clicked or resized. The other captures come from the same production `dist/` bundle served by `vite preview` on localhost and rendered by headless Firefox 159.0b1, not WKWebView: [`projects-interactive-1440.png`](../assets/desktop/projects-interactive-1440.png), [`task-preview-1440.png`](../assets/desktop/task-preview-1440.png) and [`home-interactive-760.png`](../assets/desktop/home-interactive-760.png) (the 760×560 minimum). Interaction states (typing, context selection, clearing and browser history) were verified by the tests, not by screenshots.

**Against the references.** Projects still matches reference page 2 (list, header, stage strip, two-column Workspace/Context). Intentional differences: the primary button reads "Draft task here" and works, the Access row says "not checked", an extra "Speed up note search" task keeps the "4 tasks" line true, and "ensaios" reads "no tasks yet" instead of the reference's "2 tasks" because the fixture lists none. Home keeps the composer, headings and tables; differences: a "Preview task" button next to "Select context" (the reference's amber button is now the draft-gated Preview action, shown muted until there is a draft), a one-line preview-only hint under the composer, and task rows that are links. At 760 px the two action buttons wrap together onto a second row. The task preview page has no reference page; its content now uses the same 50 px effective left gutter as Projects at 1440 px and the standard 24 px narrow gutter below 1000 px.

**Independent-review corrections.** The 760×560 capture originally showed the sticky sample bar covering the Waiting-on-you heading, and Preview started about 18 px left of the other wide pages. The sample bar is now a separate footer below an independently scrolling content region, Preview aligns with the standard wide gutter, and the composer action buttons wrap as one group. The refreshed captures above show those corrections. WKWebView renders the bundled fonts slightly lighter and tighter than Firefox, but the geometry and hierarchy remain consistent.

## Context experience ([issue #28](https://github.com/RaioViajante/synapse/issues/28))

**What was captured, and how.** The debug `Synapse.app` was rebuilt and launched on macOS and its real WKWebView window captured with `screencapture -l`: [`home-sample-context-native.png`](../assets/desktop/home-sample-context-native.png). macOS again denied UI automation, so the native window could not be navigated to Context or resized; **no native capture of the Context page exists**. The Context captures are **WebKit rendered by Playwright** (not the Tauri WKWebView) from the same production `dist/` served by `vite preview`: [populated 1440](../assets/desktop/context-populated-1440-webkit.png), [selected with a cut excerpt, 1440](../assets/desktop/context-selected-1440-webkit.png), [no results, 1440](../assets/desktop/context-no-results-1440-webkit.png) and [selected at the 760×560 minimum](../assets/desktop/context-selected-760-webkit.png). At 760×560 a script found no horizontal page scroll and no element past the right edge. Interaction behaviour is covered by `Context.test.tsx`, not by the screenshots.

**Against the references.** Context keeps page 4's structure: three ruled source cells with the `your notes` / `private history` / `pending suggestion` tags, a pill search field, ruled result rows with mono token meta, and a 340 px aside. The inline "Excerpt / Close" row with a visible "cut here" marker follows the Task · 1 · Context screen. Intentional differences, all for honesty: the vault card, path, index time and "1,284 notes" are gone because no vault exists; results are grouped by source; the search filters as you type instead of using a Search button; the Limits selects became a static, labelled example budget; "Review suggestions" stays disabled; selection is a checkbox, not the reference's pill switch.

**Known limitations.** Native Context and native narrow-window states are unverified. The aside sits below the results under 1100 px, so the selection summary is a scroll away at 760 px (the "N selected" count stays beside the search heading). Screenshots are not pixel-diffed against the reference.

**Projects vault disclosure.** The Projects Context section now labels the vault "sample, not connected", renames "Linked notes" to "Sample notes", shows Access as "none · no vault is connected" instead of "read-only", and adds a one-line note; the stage strip reads "sample vault · 4 sample notes". The layout is unchanged. The reference's "read-only" evidence dot is replaced because nothing is connected or checked.

## Sessions (issue #30)

Captures are **WebKit through Playwright against the production build, not the Tauri WKWebView**: [list](../assets/desktop/sessions-list-1440-webkit.png), [selected row with continuity comparison](../assets/desktop/sessions-continuity-1440-webkit.png), [detail](../assets/desktop/session-detail-1440-webkit.png), [filtered-empty](../assets/desktop/sessions-empty-filtered-1440-webkit.png), and [760×560 list](../assets/desktop/sessions-760x560-webkit.png) and [detail](../assets/desktop/session-detail-760x560-webkit.png). No page-level horizontal overflow at 760×560 in any of them.

The debug `Synapse.app` was built and launched, but no native Sessions screenshot exists: the window could not be driven or captured reliably (the keystroke went to another app), so no native claim is made. The sample-data-off empty state is covered by tests rather than a new capture; `sessions-empty-1440.png` still shows it.

Differences from reference page 3: a search and two filters above the table, a "Details" link per row, a fifth (blocked) sample session, "Worktree" fed from the sample project's HEAD (`main @ 9b04e7d`) rather than an invented hash, and the "Requires" row reading "advertised, not verified" or "not checked" instead of a verified claim. The timeline is a recorded-sample list, not the reference's terminal block, and carries offsets instead of wall-clock times so it implies no real time. Not compared: the PDF itself (no PDF renderer was available), so pages 3, 11, 15 and 16 were read from their PNG exports.

## Runtime status (issue #40)

Captures are **WebKit through Playwright against the Vite dev server, not the Tauri WKWebView**, so they show the real browser-preview state (no IPC, no mocks): [1280 wide](../assets/desktop/runtime-browser-preview-1280-webkit.png) and [the 760 minimum](../assets/desktop/runtime-browser-preview-760-webkit.png). No page-level horizontal overflow at either width.

No native screenshot exists. The release `Synapse.app` was rebuilt and launched, and `⌘6` reached Settings, but macOS would not bring the window forward for the Runtime tab to be selected, so neither the release "Not available in this build" state nor a `dev-runtime` success was captured. Nothing was mocked into a screenshot to stand in for them.

What each kind of evidence covers:

| Evidence | Covers | Does not cover |
| --- | --- | --- |
| The two screenshots above | The browser-preview state, rendered by WebKit | Any native state, the Tauri WKWebView |
| `Runtime.test.tsx`, `runtimeStatus.test.ts` (jsdom, `mockIPC`) | How the UI validates and shows each native answer shape, error code and lifecycle case, using mocked IPC responses | A real IPC call or real rendering |
| `cargo test` (default and `--release`) | The real `runtime_not_installed` answer and the IPC authorization rules | The webview |
| `cargo test --features dev-runtime` | A real call from Rust to the configured Python runtime, passing locally and in CI | The webview and the frontend |

**Not verified in #40:** the end-to-end native path, from the WKWebView through IPC to the rendered Runtime tab. Issue #42 below verifies it in the real window.

## Native end-to-end runtime status (issue #42)

Verifies the whole path in the real Tauri window: React → WKWebView → authorized IPC → Rust bridge → Python identity query → validated answer → rendered Settings → Runtime. Run on 2026-10-13, macOS 27.0.1 (arm64), from `f5f1360`. Nothing in the app, its capabilities or the bridge was changed to do it.

**Method.** Each build's `.app` was opened with `open -n`. The window was driven through the macOS accessibility API (AppleScript `System Events`, `AXPress` on the rail link, the Settings tab and Refresh, found by role and name), its text read back from the WebView's accessibility tree, and captured window-only with `screencapture -o -l <window id>`. Requests were counted outside the app with a kqueue `NOTE_FORK` watch on the app's PID (same user, no root; see the procedure below), and a `ps` poll named each child. The app forks for nothing else: across all runs every fork was a `status.py` child, and idle time and navigation produced none.

### Development build (`pnpm tauri build --debug --features dev-runtime --bundles app`)

Ground truth from the checkout: `pyproject.toml` version `0.12.1`; the configured interpreter's `platform.python_version()` `3.14.7`; `POLICY_VERSION = 1`.

| Step | Native observation | Forks |
| --- | --- | --- |
| Launch, Home, then Settings (opens on Preferences) | No query | 0 |
| Press Runtime | *Checking the runtime…* captured, then *Answered by the runtime*: Runtime version `0.12.1`, Python `3.14.7`, Gate policy `version 1` with the metadata-only note, Source *Python runtime · development checkout*, Observed `17:32:47 · answered in 108 ms` | 1 (child `Python -I -S -B status.py`) |
| Stay on the tab, no input, 70 s (30 s timed) | Nothing changes | 0 |
| Press Refresh | Observed `17:33:59 · answered in 81 ms` | 1, new child PID |
| Security policies, 10 s, back to Runtime | Observed `17:34:30 · answered in 83 ms` | 1 |
| Home, then Settings again | Settings opens on Preferences; no query | 0 |
| Resize to 760×560, press Runtime | Same fields, `17:35:10 · 79 ms`; no horizontal overflow | 1 |
| Quit | Watcher total **4**, one per action that should query | — |

The rendered text was searched for `/Users`, the account name, `python3.` and the native error messages: none present.

Captures: [loading](../assets/desktop/runtime-native-dev-loading-1280.png), [answered, 1280](../assets/desktop/runtime-native-dev-answered-1280.png), [answered, 760×560](../assets/desktop/runtime-native-dev-answered-760.png).

### Controlled failure: interpreter unavailable

`.dev-python` (Git-ignored) was backed up, pointed at the nonexistent absolute path `/nonexistent/synapse-verification/python3`, the `dev-runtime` bundle rebuilt (the binary contains that path) and launched. Settings → Runtime showed *Not available in this build* with the generic detail, before and after Refresh, with **0 forks**: the run-time interpreter check fails before any spawn. Its capture was byte-identical (same MD5) to the release capture below, as the bridge intends (`runtime_not_installed` looks the same whatever the cause). `.dev-python` was then restored, its SHA-256 compared equal, and touched so Cargo would rebuild (see Troubleshooting).

Not exercised natively, because they need a different interpreter, a modified `status.py` or a test-only hook: unsupported version, timeout, oversize, malformed output, non-zero exit and `runtime_busy` (the frontend shares an in-flight request, so the UI cannot produce it). Each remains covered by the Rust process tests with fixture scripts and by `Runtime.test.tsx` with mocked IPC.

### Release build (`pnpm tauri build --bundles app`, the CI release command)

- `scripts/verify-release-isolation.sh` accepted the bundle: `sha256 d7df64fb…7c25a15, 14 libraries, 290 imported symbols: no development runtime evidence`. The running process was that binary (same SHA-256).
- Settings → Runtime: *Not available in this build* and its fixed detail; no identity fields. That copy is mapped only from `runtime_not_installed`, so the answer came from the Rust command (an IPC denial would show *Status couldn't be read*; no IPC would show *Browser preview*).
- Refresh pressed three times: the same state, no identity text anywhere in the tree, **0 forks** for the whole session.
- Capture: [release, unavailable](../assets/desktop/runtime-native-release-unavailable-1280.png).
- `pnpm tauri build` with the configured default targets (`app`, `dmg`) built the `.app` but `bundle_dmg.sh` failed in this environment. DMG packaging is not part of this verification (see #38).

### Verification matrix

| Claim | Build | Method | Expected | Observed | Evidence | Limits |
| --- | --- | --- | --- | --- | --- | --- |
| Real Python answer is shown | debug + `dev-runtime`, macOS arm64 | Native window, AX read, capture | Checkout's versions, dev-checkout source | Matches | Table and captures above | One interpreter (3.14.7) |
| Loading state renders | same | Native capture | *Checking the runtime…* | Seen | Loading capture | Caught in one frame; ~100 ms |
| Refresh makes a new native request | same | kqueue fork count + new child PID + new `observed_at`/`duration` | +1 fork | +1 | Table | Fast repeated clicks not tested natively (AX presses take ~1.5 s each) |
| Tab return re-queries once, no polling | same | Fork count over idle and navigation | 1 per open, 0 idle | As expected | Table | 70 s idle window |
| Errors sanitized | same and release | AX text search | No path, username or native message | None | Above | Text search, not exhaustive pixel review |
| Missing interpreter fails closed | debug + `dev-runtime`, bogus path | Native window, fork count | *Not available*, no spawn | As expected | MD5 match with release capture | Only this failure natively |
| Release cannot start Python | release | Verifier + native window + fork count | Unavailable, 0 forks | As expected | Above | Refresh's IPC calls not counted directly (no fork to count); state unchanged |
| Narrow window | debug + `dev-runtime` | Native capture at 760×560 | No overflow | None | 760 capture | Visual check |
| Answer shapes, every error code, Strict Mode, unmount | — | `Runtime.test.tsx`, `runtimeStatus.test.ts` | — | Pass | jsdom, **mocked IPC** | Not native |
| Bridge process rules, ACL | default, `--release`, `dev-runtime` | `cargo test` | — | Pass | Rust tests | No webview |
| Browser preview | Vite dev server | Playwright WebKit | *Browser preview*, no call | As before | #40 captures | Browser only |
| Windows, Linux, Python 3.10 natively, signed builds | — | — | — | Not run | — | Untested |

### Reproducing it

Prerequisites: macOS; `desktop/src-tauri/.dev-python` holding one absolute path to a Python ≥ 3.10 that passes the [interpreter checks](desktop-runtime-bridge.md#development-interpreter); `pnpm install` in `desktop/`. Window automation is optional: everything below can be done by hand.

1. In `desktop/`: `pnpm tauri build --debug --features dev-runtime --bundles app`, then `open -n src-tauri/target/debug/bundle/macos/Synapse.app`. This bundle contains your interpreter and checkout paths: never distribute it.
2. Optionally, in another terminal, count the app's forks. Each one should be a runtime query:

   ```sh
   python3 -I - "$(pgrep -x synapse-desktop)" <<'PY'
   import select, sys, time
   kq = select.kqueue()
   kq.control([select.kevent(int(sys.argv[1]), select.KQ_FILTER_PROC, select.KQ_EV_ADD,
                             select.KQ_NOTE_FORK | select.KQ_NOTE_EXIT)], 0)
   n = 0
   while True:
       for ev in kq.control(None, 1):
           if ev.fflags & select.KQ_NOTE_EXIT:
               sys.exit(f"app exited after {n} forks")
           n += 1
           print(time.strftime("%H:%M:%S"), "fork", n, flush=True)
   PY
   ```
3. Click Settings in the rail (or ⌘6), then the Runtime tab. Expect *Checking the runtime…* briefly, then *Answered by the runtime* with your checkout's `pyproject.toml` version, your interpreter's version (`<interpreter> -c 'import platform; print(platform.python_version())'`), `Gate policy version 1` and *Python runtime · development checkout*. One fork.
4. Wait 30 s: no fork. Press Refresh: one fork, and Observed shows a new time and duration.
5. Open another Settings tab and return to Runtime: one fork. Go to another page and back to Settings: it opens on Preferences, no fork.
6. Quit. In `desktop/`: `pnpm tauri build --bundles app`, `scripts/verify-release-isolation.sh src-tauri/target/release/bundle/macos/Synapse.app` (expect `OK`), then open that `.app`. Settings → Runtime shows *Not available in this build*; Refresh leaves it unchanged; the fork counter stays at 0.

**Troubleshooting.** *Not available in this build* in step 3: the bundle was built without `--features dev-runtime`, `.dev-python` is missing, relative or fails the ownership and permission checks (`cargo test --features dev-runtime` in `src-tauri` runs the same call and fails the same way), or the interpreter moved since the build. *Unsupported runtime version*: the interpreter is older than 3.10, for example Apple's `/usr/bin/python3`. *Browser preview*: you opened the Vite page in a browser, not the app. *Status couldn't be read*: the answer or rejection didn't match the contract, for example an ACL denial. A change to `.dev-python` takes effect only after a rebuild, and Cargo notices it by modification time: restoring the file with its old time (`cp -p`, a backup tool) leaves the previous path baked in. `touch` the file and rebuild; `strings src-tauri/target/debug/synapse-desktop | grep <path>` shows which path the binary holds. `screencapture -l` needs the window number (from `CGWindowListCopyWindowInfo`) and Screen Recording permission; AX driving needs Accessibility permission for the terminal.

**Why no automated native test.** Driving the window needs Accessibility (and capturing it Screen Recording) permission granted to the invoking terminal, which headless CI runners don't reliably have. It was also not robust here: AX enumeration of the WebView takes about 1.5 s per action, and a first JavaScript-for-Automation attempt pressed the wrong rail link (it landed on Sessions) before the AppleScript name match was used. A flaky GUI test in CI would cost more than the manual procedure above, which takes a few minutes.

**Limitations.** The captures show a faint, illegible shape low in the window where the inactive macOS window material lets content behind it through; pixels were not edited. One machine, one interpreter version, Apple Silicon only. Request counts rely on the app forking only for runtime queries, which held throughout but is an observation, not a guarantee. Refresh in the release build was not counted at the IPC level. Concurrent Refresh clicks were not exercised natively.
