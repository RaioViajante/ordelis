# Desktop design reference

**Status:** canonical visual reference for the desktop app, approved by the maintainer. It replaces the legacy Synapse styling as the foundation for new interface work ([issue #72](https://github.com/RaioViajante/ordelis/issues/72)).
**Scope:** where the approved design lives, the tokens and shell structure taken from it, and how far the app follows it. It publishes no design files.
**Related:** [Design principles](principles.md) (identity, accessibility, motion) · [Desktop architecture](../implementation/desktop-architecture.md)

## Source

The reference is the 18-screen export "Ordelis — macOS app design" (Claude Design canvas, desktop). It is kept outside the repository, on the maintainer's machine:

- `ordelis-desktop-design/source/`: the canvas files (`*.dc.html` and `canvas.json`). They need the Claude Design runtime to open.
- `ordelis-desktop-design/html/`: the same screens as standalone pages; `index.html` lists them.

Rules:

- The exports are not committed and not published. Local copies belong in `reference/`, which `.gitignore` excludes. This document is the tracked record of what they contain.
- Use the desktop export. The website export and its `Explore*`/`Refined*` boards are not a source for the app.
- The export's people, repositories, hashes and counts (`lumen-web`, "Add passkey sign-in", `a41f9c2`, and so on) are invented sample content. Nothing in the app may present them as real.
- The export draws a reconstructed vector symbol next to the wordmark. `principles.md` forbids using a reconstruction as the logo, so the app shows the lowercase text wordmark only, until an approved vector master exists.

The boards, in canvas order: Foundations, Components, Home (light), Home (dark), First launch, Projects, New task (prepare), New task (review plan and approve), Task detail, Execution detail, Integrated terminal, Sessions, Changes and review, Context, Agents, Settings (execution policies), Product states, Sheets, dialogs, command bar and notifications.

## Tokens

Light is the reference appearance. Dark is designed, not inverted. The app follows the system appearance (`prefers-color-scheme`). Values are in `desktop/src/styles/index.css`.

| Token | Light | Dark | Role |
| --- | --- | --- | --- |
| `--win` / `--side` / `--surf` / `--surf2` | `#F5F5F2` / `#ECEDE9` / `#FFFFFF` / `#F9F9F7` | `#1C1E1D` / `#171918` / `#242725` / `#202321` | Window, sidebar, cards, secondary surface |
| `--line` / `--line2` | `#E4E5E1` / `#D3D6D1` | `#2F3330` / `#3D423E` | Hairline, stronger border |
| `--sel` / `--hover` | `#E0E6E1` / 5.5% ink | `#2E3932` / 6% white | Selected row, hover |
| `--ink1` / `--ink2` / `--ink3` | `#1D1F1E` / `#4C534E` / `#626A64` | `#ECEEEB` / `#B7BDB8` / `#959C96` | Text: 15.2, 7.2, 5.1 : 1 (light) |
| `--brand` | `#6B7F73` (Sage) | `#93A99B` | Focus ring, dots, quiet emphasis |
| `--pri` / `--pri-ink` | `#56695E` / `#FFFFFF` | `#506559` / `#FFFFFF` | The one filled action per view |
| `--acc` | `#46584D` | `#AFC4B6` | Icon on a selected row, avatar |
| `--run`, `--att`, `--ok`, `--bad` | `#2C61A0`, `#935605`, `#2F6E46`, `#A6382A` | `#93BBEA`, `#E8B86F`, `#93C8A4`, `#F29483` | State text: running, needs you, success, danger. Each has a `-w` wash |
| `--cc` `--cx` `--km` `--oc` | `#F3E1BC` `#D5E5F4` `#E5DDF8` `#D7EBDD` | `#4B4029` `#24394B` `#382F4F` `#273C2F` | Provider tints (Claude Code, Codex, Kimi, OpenCode) |
| `--term`, `--add`, `--del` | `#1A1C1B`, green and red tints | `#121413`, tints | Terminal and diff |

### Brand identity versus interface tokens

The documented brand colors in [principles.md](principles.md) are Forest `#2E3A33`, Sage `#6B7F73` and Stone `#A7AFA9`. They identify the product in the wordmark, the icon and marketing surfaces. They stay as documented.

The desktop interface does not derive its colors from them one to one. It uses the approved export's tokens, which are tuned for contrast on interface surfaces: ink `#1D1F1E`, the darker Sage 600 `#56695E` for filled actions (raw Sage carries white text at only 4.3 : 1), and `#46584D` for icons on a selected row. Only Sage appears unchanged (`--brand`). Forest is not forced onto interface elements, and no other palette is introduced. When the two disagree, the export's tokens decide the interface and `principles.md` decides the brand.

Brand colors (Sage, Forest, Sand, Stone, Charcoal) and state colors are separate sets. Sage appears as a seasoning: the one primary action per view, the selected sidebar row and the focus ring. Provider tints identify a provider and never a status.

- **Type:** system font (SF Pro) and SF Mono. Display 30/36, Title 1 22/28, Title 2 17/22, headline 13/18, body 13/20, callout 12/16, caption 11/14, mono 12/18. Weights 400, 500 (controls), 600 (titles). Sentence case, no all-caps labels.
- **Spacing:** 4-point grid. Sidebar rows 30 px, list rows 44 px, panel padding 20, section gap 32. Sidebar 236 px, content up to 920 px.
- **Radius:** 6 (chips), 8 (controls), 12 (cards, window), 16 (sheets, composer).
- **Elevation:** a 1 px line first, a surface step second, a shadow last. Shadows only on floating layers.
- **Controls:** 30 px high, one filled action per view. Focus is a 2 px Sage ring with a 2 px offset. Disabled controls are faded; the reason is shown on hover and keyboard focus.
- **Status:** a glyph plus a word, never color alone.

## Shell

A fixed 236 px sidebar and a scrolling page. From the top: the lowercase `ordelis` wordmark, **New Task**, **Search**, then Home, Tasks, Sessions, Projects, Context and Agents, a Projects section, Settings, and the workspace footer. Narrower than 900 px the sidebar keeps its structure and shows icons only (the window minimum is 760 px).

Navigation shortcuts follow the sidebar order: `⌘1` Home, `⌘2` Sessions, `⌘3` Projects, `⌘4` Context, `⌘5` Agents, `⌘6` Settings (Ctrl on other platforms). Tasks, New Task and Search are disabled and have no shortcut; the export's `⌘N`, `⌘K` and `⌘,` are not implemented. Focus moves to the page after a shortcut or a link. The order lives in `ROUTES` in `desktop/src/app/routes.ts`, which the Settings list also reads.

The visible product name is Ordelis (window title, page title, sidebar wordmark, interface copy). The bundle name `Synapse.app`, the bundle identifier, crate and package names, the data directory, and the native folder-panel and error messages in `src-tauri` still say Synapse. They belong to the identity migration, not to this design work.

The export also draws macOS window controls and a top bar with an agent summary and a notification bell. The app uses the native title bar, and the top bar has no real data behind it yet, so neither is drawn.

## What the app implements

| Screen | State |
| --- | --- |
| Foundations, Components | Tokens, sidebar, buttons, fields, switch, focus ring, count badge and disabled-with-reason controls are implemented. Status pills, chips, tabs, avatars and the diff and terminal styles are not |
| Home (light and dark) | Appearance follows the system. The layout is still the legacy sample page, not the export's composer |
| First launch, New task (both), Task detail, Execution, Terminal, Review, Product states, Dialogs | Not implemented. Where the app has an equivalent sample page it only picks up the new tokens |
| Projects, Sessions, Context, Agents, Settings | Legacy sample pages with the new tokens. Their layouts have not been rebuilt to the export |

New Task, Search and Tasks appear in the sidebar but are disabled with a stated reason, because task planning, search and a task list do not exist. Sample data stays labelled as sample data.

## Legacy aliases

`index.css` keeps the Synapse prototype's token names (`--bg-000` to `--bg-400`, `--line-100`, `--line-200`, `--ink-100` to `--ink-300`) as aliases of the tokens above, so screens that have not been rebuilt keep working. New code uses the tokens in the table. Remove an alias when the last screen using it is rebuilt.
