# Desktop UI design inventory

Inventory of the supplied design references for [issue #24](https://github.com/RaioViajante/synapse/issues/24). The references are the visual source of truth; this document records what they contain and which parts are implemented, mocked or not yet real. The `reference/` directory itself is not committed (about 20 MB of exports); only the assets listed under [Provenance](desktop-architecture.md#asset-provenance-and-branding) are copied.

## Reference files

| File | What it is | How it is used |
| --- | --- | --- |
| `Synapse — telas.pdf` | 20-page UI export (1080×750 pt screens, plus several letter-size pages). Page 1 Home, 2 Projects, 3 Sessions, 4 Context, 5 Agents, 6 Settings, 7–18 task flow and inbox, 19 empty states and failures, 20 Workflows (not in MVP). | Visual source of truth. |
| `Synapse — telas.html` | A bundled export, **not** a buildable project. A JSON manifest holds 19 page documents, each itself a bundle holding its fonts and a template. Unpacked, every page is static HTML and one shared CSS block in a custom `<x-dc>` component format with hash-link navigation to sibling files. There is no React, no state management and no reusable component source. | Token and CSS source: the design tokens, component rules and rail SVG paths were read from it and ported. The markup structure was used as a guide. |
| `Synapse — telas-png/` | 19 page PNGs at 2880×2000 (2×). The PDF has 20 pages because the task page with terminals spans two. | Pixel comparison. |
| `icon.png` | 1312×1199 RGBA app icon (amber node graph on a warm-dark squircle). The squircle is not square (about 929×894 px) and the transparent area carries faint stray pixels. | Master for the application icon. |
| `icon-remove-gb.png` | 1254×1254 RGBA glyph with the background removed. It contains visible red and yellow noise inside the cut-out areas and a different tone from `icon.png`. | **Not used.** It is not clean enough to ship. |
| `Synapse Technical Handoff.pdf` | 14-page technical handoff. Describes branch `feat/5-codex-cli-capability-inspection` at `da21c24` and says no UI exists. | Historical only; its repository state is outdated. Its security invariants still apply. |

## Colour and surface hierarchy

Warm near-black, never neutral grey or blue-black. Depth comes from stepping up one surface, not from shadows or gradients.

| Token | Value | Role |
| --- | --- | --- |
| `--bg-000` | `#100e0c` | Window background, terminals |
| `--bg-100` | `#171411` | Rail and page |
| `--bg-200` | `#1f1b17` | Cards, selected rows, composer |
| `--bg-300` | `#29241f` | Buttons, fields, active rail item |
| `--bg-400` | `#342e28` | Hover, switch track |
| `--line-100` / `--line-200` | `#2c2621` / `#6f665b` | Hairline dividers / outlined chips |
| `--ink-100/200/300` | `#f1e9dd` / `#c8bdac` / `#a89b89` | Ivory text, secondary, tertiary |
| `--accent` | `#dc9550` | The one amber: primary action, "needs you" |
| `--ok` / `--danger` | `#93bab4` / `#e57a62` | Verified/done (muted teal) and failed/blocked (clay) |
| `--cc` `--cx` `--oc` `--km` | `#e2a68a` `#a2c8b0` `#a9bbda` `#cdaed2` | Agent identity squares: claude-code, codex, opencode, kimi |

Radii: 10 / 14 / 20 px and pill. No gradients, blur or glass anywhere in the references.

## Typography

Three voices, each with one job (the reference names them editorial, controls, data):

- **Newsreader** (serif): page titles 32 px (Home 36 px), section headings 18 px, empty-state titles, lede text. Reads as editorial.
- **IBM Plex Sans**: body 15/23, controls 14.5 medium, labels 13 medium in the tertiary ink, metadata 13.5.
- **JetBrains Mono**: agent names, branch and path values, counters, state chips, token counts. Tabular numerals are on globally.

## Navigation

A 68 px rail inset 12 px from the window edge, floating as its own rounded panel. Top: the three-node mark. Then Home, Projects, Sessions, Context, Agents, Settings as 44 px icon buttons (active = `--bg-300` plate, no label). Bottom: an inbox button with an amber count badge for "Waiting on you". There are no text labels, so every button needs an accessible name. The inbox target (reference page 14) is outside this increment.

## Layout proportions

Window 1440×1000 in the export. Rail 68 px, 12 px gaps, page panel radius 20. Home content column max 1180 with a 840 px composer; other pages use 1100–1300 px columns with 32 px gutters and 28 px vertical rhythm (Home 44 px). Master/detail pages use a 280 px list and a fluid detail pane; Context and Agents use a fluid main column plus a 340 px aside. Rows are ruled, not carded: 1 px dividers, 13–15 px vertical padding.

## Components and states

Pill buttons (default, amber primary, ghost, small; disabled is an outline, not a fade), select and text fields on `--bg-300`, a 40×24 switch, tabs with a 2 px underline, ruled rows, key/value lists, a four-up stage strip, comparison tables, status dots (`wait` hollow, `work` ring, `done` teal, `fail` clay, `you` amber), evidence glyphs (● verified, ○ advertised, ? unknown, ✕ not supported), provenance chips (`your notes`, `private history`, `pending suggestion`), dashed "vision" pills for things that are not in this version, notices, and empty-state panels. Focus is a 2 px ivory ring.

## Interaction patterns

Master/detail selection by list button; tab panels in Settings; toggles for local preferences; plain text search. Anything not built is shown as a dashed "not in this version / later / planned" pill rather than hidden. The reference's own empty-state page states the rule this app follows: *each says what happened, why, and what you can do; none pretends something works.*

## Branding and window

The reference rail uses a small ivory three-node mark (SVG paths copied from the export). The application icon is the amber squircle from `icon.png`. The export is frameless; the shipped window uses the native macOS title bar (see the architecture document for why).

## Mock data versus verified data

Everything in the references is invented illustration (`atlas-api`, `Cérebro`, version numbers, policy v3, task and session ids). It maps to this build as follows.

| Reference content | In this build |
| --- | --- |
| Projects, tasks, sessions, worktrees, vault, notes, token counts, waiting items | Local fixtures in `src/fixtures/mock.ts`, shown behind a "Sample data" notice, switchable off |
| Running or paused agents, "1 running" | Only inside sample data; no live indicator exists outside it, and the sample notice says nothing is running |
| Agent installed versions, sign-in state, models, 9 verified capabilities | **Not reproduced.** Every fact is "unknown"; a note says nothing was checked |
| Security policy v3 and authorized runs | **Not reproduced.** Replaced by the real restrictions of this build (every sensitive action is simply unavailable) |
| Vault read-only access | Shown as "by design", not as verified |

## Functionality the references show that does not exist yet

Task planning and execution, terminals, plan review and approval, budget limits, results comparison, Obsidian vault reading and indexing, memory suggestions, worktrees, notifications, agent capability checks, editable security policy, Workflows, and the inbox. Each appears here only as a page shell, an empty state, or a disabled control that states its reason. Kimi has no MCO adapter today (see the [MVP plan](synapse-mvp-plan.md)), which the Agents page reflects.
