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
