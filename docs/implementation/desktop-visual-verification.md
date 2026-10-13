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
2. **Sample data notice.** A sticky bar at the bottom of the sample pages is not in the reference. It exists so that no fixture reads as real or live.
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

At 900 px the sessions table drops its Project and Worktree columns, the task table drops project and agents, stage strips go two-up, and grids become single column. The pick-up comparison table keeps its 900 px width and scrolls horizontally inside its own container. At the 760 px minimum the sticky sample notice takes about 15% of the window height. No layout was checked below 760 px because the window cannot be smaller.

## Missing fonts, spacing and elements

- Only the Latin subsets in the export are bundled. Non-Latin text and Newsreader italics fall back to system fonts (no implemented screen uses italics).
- The disabled-control reason tooltips were not captured in a screenshot. They are covered by component tests (every disabled button must expose a description) and by hover and focus CSS, but the visual tooltip was not inspected in the running window.
- The reference's rail has no separate inbox tooltip or disabled state; the app adds both.
- Windows and Linux were not built or launched.
