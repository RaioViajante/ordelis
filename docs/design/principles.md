# Design principles

**Status:** guidance. It records the approved brand direction and the principles the interface and website are built to. The existing desktop prototype predates this direction and has not yet been restyled to it.
**Scope:** identity, color, typography, responsive behavior, accessibility, motion, and which surface is responsible for what. It is not a component library, and it publishes no design files.
**Related:** [Vision](../product/vision.md) · [Roadmap: desktop UI redesign](../product/roadmap.md#3-desktop-ui-redesign) · [Migration record: design decisions](../migration/ordelis.md#accepted-decisions) · [Desktop UI design inventory](../implementation/desktop-ui-design-inventory.md)

## Identity

- **Name.** Ordelis in prose. The visual wordmark is lowercase: `ordelis`.
- **Tagline.** Your agents. In order.
- **Character.** Calm, precise and spacious. Editorial rather than promotional, and technical without being cold. The product's job is to make complex work legible, so the design should feel like order, not like busyness.
- **Voice.** Plain and specific. Say what something does and what state it is in. Avoid superlatives, urgency, and generic software-marketing phrases.
- **The symbol and logo.** An approved vector master of the Ordelis symbol does not exist yet. Until it does, the wordmark is set in type, and no reconstruction is used as an official logo. Final application icons, installer icons, the favicon and social images wait for the approved master.

## Color

The approved palette is a restrained green-gray family with warm neutral surfaces.

| Name | Hex | Role |
| --- | --- | --- |
| Forest | `#2E3A33` | Primary text and dark surfaces; the strongest accent |
| Sage | `#6B7F73` | Secondary text, lines, and quiet emphasis |
| Stone | `#A7AFA9` | Hairlines, disabled states, and quiet fills |
| Warm neutrals | (per system) | Page and panel surfaces in the light theme |

Contrast guidance, computed from these values against pure white and pure black. Surfaces that are not pure white or black must be checked again.

| Color | On white | On black | Consequence |
| --- | --- | --- | --- |
| Forest | 11.9 : 1 | 1.8 : 1 | Body text on light surfaces. Not for text on dark surfaces. |
| Sage | 4.3 : 1 | 4.9 : 1 | Large text and interface elements on light surfaces, below the 4.5 : 1 needed for small text. Usable for small text on dark surfaces. |
| Stone | 2.3 : 1 | 9.4 : 1 | Decorative on light surfaces, never for text or for a meaningful boundary. Good for text on dark surfaces. |

Rules that follow:

- **Color is never the only signal.** State, severity and approval are also carried by text, shape or position.
- **Light and dark themes are both first class,** with tokens defined once and remapped, not inverted by filter. Each theme has its own contrast check.
- **Status colors are held back.** Success, warning and danger appear only when they carry meaning, and each has an accessible text alternative.
- **The README and documentation do not depend on the palette.** The wordmark there is plain text, so it follows GitHub's light and dark themes.

## Typography

- **Desktop interface:** a system-native font stack with fallbacks for each platform, so the application looks at home on macOS, Windows and Linux. Proprietary system font files are never bundled or redistributed.
- **Code, terminals and identifiers:** JetBrains Mono.
- **Bundled fonts are self-hosted.** The interface loads no remote fonts, which the content security policy enforces. The current prototype's fonts predate this decision and are reconciled when the design system lands.
- **Hierarchy comes from size, weight and space,** not from many typefaces or from decoration.
- **Reading comfort.** Moderate line lengths, generous leading, and no walls of text. Lowercase wordmark and sentence-case headings.
- **The website** sets its own type for storytelling, within the same principles, and is not bound by the desktop's system-native requirement.

## Responsive behavior

- **The desktop** is a window, not a page. It has a documented minimum size and must remain fully usable down to it. Today that minimum is 760 by 560 pixels, with multi-column layouts collapsing as the window narrows and wide tables scrolling inside their own container instead of the whole page.
- **The website** is designed for phone, tablet and desktop widths from the start, and no layout depends on hover.
- **Text scales.** Layouts hold at larger text sizes and zoom without losing content.
- **Density is a choice, not an accident.** Spaciousness is the default; dense views such as logs and diffs are deliberately dense, and are the exception.

## Accessibility

- **Keyboard first.** Every function is reachable and operable by keyboard, with a visible focus indicator and a sensible order. Page navigation has shortcuts, and focus moves to the content after a navigation.
- **Semantics.** Native elements and correct roles, names and states. Tabs follow the tab pattern. Navigation announces the current page.
- **Live changes** are announced politely and once, not repeated or noisy.
- **Contrast** meets WCAG 2.2 AA in both themes, checked per surface.
- **Targets and text are large enough** to use, and nothing depends on precise pointing.
- **No information is available only on hover or only in color.**

## Motion

- **Motion explains, it does not decorate.** It shows where something came from or went, or that a state changed.
- **Short and quiet.** No looping, parallax or attention-seeking animation in the product.
- **Reduced motion is honored.** With `prefers-reduced-motion`, transitions are removed or reduced to simple fades, and nothing is lost: every state is still clear without animation.
- **Progress is truthful.** An indicator never implies progress that is not happening, and a spinner is never used to dress up sample data.

## Honest interface

These follow from the product's security principles and are design rules:

- Sample data is labelled as sample data wherever it appears, and is never presented as live execution.
- Controls for actions that do not exist are visibly inert and say why. They do not look available.
- Security-sensitive screens, such as approve, run, merge, discard and terminal, show only inert, labelled fixtures until the capability behind them exists.
- A screenshot or mockup is never presented as a capture of working functionality.

## Desktop and website responsibilities

| | Desktop application | Website |
| --- | --- | --- |
| Purpose | Do the work: plan, supervise, review, decide | Explain the product and, when releases exist, distribute it |
| Network | Works fully offline; loads no remote content | Normal web page |
| Design source | The desktop design export is the visual reference | The website design export is the reference |
| Implementation | Rebuilt in the application's own stack; prototype HTML is never shipped | Rebuilt in the website's own stack |
| Download or support claims | None | Disabled or marked unavailable until real release metadata exists |

Design exports are private working references. They are not production code and are not committed to this repository.

## Platform neutrality

- No macOS-only wording or iconography in shared copy. Prefer neutral phrasing over "this Mac".
- Shortcuts are meant to be labelled per platform: `⌘` on macOS, Ctrl elsewhere. The current prototype accepts either key everywhere and shows both labels, because it does not detect the operating system.
- Window controls, menus and dialogs use the native ones.
- Behavior that differs by platform is documented, and nothing is drawn to suggest support that has not been verified. See [Cross-platform](../architecture/cross-platform.md).
