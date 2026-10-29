# Ordelis migration: decision record

**Status:** accepted direction, nothing implemented. This document is documentation only. It records what was decided, what was verified, and what must happen before each later change. It does not rename anything.

**Scope of the migration:** the product currently named **Synapse** (a fork of [MCO](https://github.com/mco-org/mco)) becomes **Ordelis** (`ordelis.app`). Brand styling: `ordelis` as the visual wordmark, `Ordelis` in prose. This record covers the repository, desktop app, build metadata, ownership and release hygiene, and the planned website. It deliberately does not rename inherited MCO interfaces.

Contents: [Accepted decisions](#accepted-decisions) · [Safety invariants](#safety-invariants) · [Provenance](#provenance-and-attribution) · [Identifier matrix](#identifier-migration-matrix) · [Registry migration](#registry-migration-semantics-i2) · [Platform assumptions](#platform-assumptions) · [Visibility](#repository-visibility-evaluation) · [Rename runbook](#repository-rename-runbook-prepared-not-executed) · [Prerequisites](#outstanding-prerequisites) · [Risk register](#risk-register) · [PR roadmap](#pr-roadmap)

## Accepted decisions

| # | Decision |
| --- | --- |
| D1 | **Structure.** Keep `desktop/` and `runtime/` in place and add `web/` beside them. No `apps/`, no Turborepo or Nx, no root pnpm workspace for structural symmetry. Lockfiles stay independent: the root is an npm package, `desktop/` has its own `pnpm-lock.yaml`, and `web/` will have its own. |
| D2 | **Single monorepo.** Temporarily making the repository private during development is being evaluated (see [Repository visibility](#repository-visibility-evaluation)). Until a visibility change is separately authorized and executed, private website exports and other private design assets are not committed to this repository, including on pull-request branches. Any strategy that departs from a single monorepo (for example, keeping private work in a separate repository) is **not** pre-approved: it requires an explicit, recorded amendment to this decision first. |
| D3 | **Rename.** Destination `RaioViajante/ordelis`, under the existing personal account. No new organization. Full history, issues, pull requests and attribution are preserved. The procedure is [prepared](#repository-rename-runbook-prepared-not-executed) and is not executed without explicit authorization. |
| D4 | **Bundle identifier.** Proposed `app.ordelis.desktop`, subject to the verification below. The existing local project registry is preserved through a separately tested migration or a documented safe fallback. Trust-origin values and other security-sensitive identifiers are not renamed as part of branding. |
| D5 | **Design sources.** The newer web export directories (`projeto/`, `preview/`, `imagens/`, `fonte/`) are the provisional canonical website sources. The desktop export is the visual reference for the Tauri application. The two exports are not merged, and prototype HTML is not production code. A final consistency check against the approved design precedes implementation. |
| D6 | **Design system and fonts.** Preserve the approved palette, symbol, typography and identity. The desktop app uses a system-native font stack with cross-platform fallbacks; JetBrains Mono for code and terminals. No Apple-proprietary font files are distributed. An approved vector master of the symbol is required before final application and installer icons are generated. |
| D7 | **MCO compatibility.** Inherited MCO package names, CLI compatibility, attribution and copyright notices stay unless a separately reviewed change requires otherwise. Original MCO foundations are clearly distinguished from Ordelis-specific work. |
| D8 | **Release workflow and ownership.** `publish-npm.yml` is reviewed and gated or disabled before any Ordelis release. `CODEOWNERS` is updated to the actual Ordelis maintainer without removing upstream attribution. Each is its own reviewable change. |
| D9 | **Cross-platform.** Targets are macOS, Windows and Linux. No platform is declared supported until verified. macOS-only dependencies and Unix assumptions are [documented](#platform-assumptions). Platform migration is not combined with cosmetic redesign. |
| D10 | **Quality.** Every increment has explicit acceptance criteria, local verification, CI evidence and an independent review before merge. No force pushes, history rewriting, unauthorized merges, deployments or release publication. |

### Verification of D4 against Tauri

Checked against the [Tauri v2 configuration reference](https://v2.tauri.app/reference/config/) and the [Tauri CLI 2.6.0 release notes](https://v2.tauri.app/release/tauri-cli/v2.6.0/):

- `app.ordelis.desktop` meets the documented character rule (alphanumerics, hyphens and periods only) and the reverse-domain form.
- Tauri CLI 2.6.0 added a warning that an identifier should not **end** in `.app`, because it collides with the macOS bundle extension. `app.ordelis.desktop` begins with `app` and does not end with it.
- The identifier must be unique and determines the webview data directory. This repository's own [local projects design](../implementation/desktop-local-projects.md) documents the macOS registry location, `~/Library/Application Support/<identifier>/`. The per-OS paths for Windows and Linux come from Tauri's path resolver and have **not** been checked in this repository.
- Tauri documents that the bundle `publisher` defaults to the second element of the identifier. For `app.ordelis.desktop` that is `ordelis`; for the current `dev.raioviajante.synapse` it is `raioviajante`.
- **Correction to an earlier audit statement:** the Windows WiX upgrade code is derived from the **product name**, not the identifier. Changing `productName` therefore changes the default upgrade code. This has no effect today because no Windows installer exists, but the product name must be final before the first Windows installer ships.
- Not yet verified: how NSIS derives its install identity, and whether `enableGTKAppId` should be set on Linux. Both belong to the packaging phase.

## Safety invariants

These hold for every change in this migration.

- The execution safety gate is not bypassed, weakened or removed. It currently always answers no and stays that way until execution is separately designed, reviewed and approved.
- The IPC authorization model (the Tauri app command manifest and capability file), the strict CSP (`default-src 'self'`; no remote fonts or scripts) and the dev-runtime release isolation are preserved. A change to any of them is its own reviewed change.
- Demo or sample data is never presented as live execution.
- No cloud service, mandatory account or telemetry is introduced by branding or redesign work.
- Trust-origin values (`synapse_observed`, `synapse_enforced` in `runtime/execution_safety_gate.py`) are not renamed by any branding change.

## Provenance and attribution

### Requirements

1. `LICENSE` remains verbatim, including `Copyright (c) 2026 mco maintainers`. Any additional copyright line for Ordelis work is a separate, explicit maintainer decision and is added beside the original, never in place of it.
2. The README keeps its "Origins and license" statement, updated to say Ordelis was formerly named Synapse and began from MCO `v0.12.1`.
3. Attribution appears in **two places**, introduced by N1 (not by this record):
   - A standalone `NOTICE` file at the repository root is the authoritative statement. It names Ordelis (formerly Synapse), the upstream project (MCO, `https://github.com/mco-org/mco`, `v0.12.1`), the upstream copyright line and license (`Copyright (c) 2026 mco maintainers`, MIT, with `LICENSE` as the full text), the fork base commit recorded below, a pointer to this document's [origin table](#original-mco-foundations-versus-ordelis-specific-work), a statement that upstream commit history is retained, and a pointer to `desktop/src/assets/fonts/FONT-LICENSES.txt` for bundled fonts.
   - The README keeps a short "Origins and license" section that links to `NOTICE` and does not duplicate it.
   - `NOTICE` is not in the root npm package's `files` allowlist today, and the app bundle currently ships only `FONT-LICENSES.txt` as a resource. If the npm package is ever published from this repository, or a desktop bundle ever ships MCO-derived code (for example a bundled runtime), `NOTICE` and `LICENSE` must ship with it. That is a P-series acceptance criterion.
4. Git history is never rewritten, so the upstream authors' commits remain intact.
5. Historical documents (`CHANGELOG.md`, `docs/releases/`, `docs/archive/`, `docs/implementation/` decision records, `README.zh-CN.md`) are not blindly renamed. Where the old name would mislead, add a short "formerly Synapse" note and keep the text.
6. GitHub does not show this repository as a fork (`isFork: false`), so the fork relationship cannot be preserved on GitHub itself. Provenance is carried by the history, `LICENSE` and the attribution in requirements 1 to 3.

### Original MCO foundations versus Ordelis-specific work

Fork base: commit `3cfce13` (`fix(ci): repair npm publishing and prepare v0.12.1 (#131)`, 2026-10-07, upstream). The first commit after it, `40313f0`, is the first non-upstream commit. Determined from `git diff` against that base.

| Area | Origin |
| --- | --- |
| `runtime/` MCO modules, including the `runtime/session/` daemon and client, `runtime/adapters/` and the `mco` CLI entry | **MCO**. `runtime/cli.py` is the only inherited runtime file modified since the fork. |
| `runtime/codex_capabilities.py`, `codex_probe_runner.py`, `execution_safety_gate.py`, `probe_authorization_ledger.py`, `probe_confirmation.py`, `runtime_validation.py`, `synapse_plan.py`, `vault_context.py`, `vault_mock_plan.py` | **Ordelis-specific** (added after the fork) |
| `tests/` | MCO, plus 13 added test files. 5 inherited test files were modified. |
| `bin/mco.js`, `mco`, `skills/mco-cli/`, root `package.json`, `pyproject.toml`, `scripts/` | **MCO**. Only `scripts/run_capability_probes.sh` was modified and `scripts/sanitize_capability_probe_artifacts.py` added. |
| `desktop/` | **Ordelis-specific**: the entire tree |
| `docs/contracts/`, `docs/reference/`, `docs/releases/`, `docs/archive/`, `README.zh-CN.md` | **MCO** |
| `docs/implementation/`, the 7 guides added under `docs/guides/` after the fork (capability inspection, zero-model probes, interactive probe authorization, private execution plan, probe artifact hygiene, runtime validation, vault context preview), `docs/assets/desktop/` and the Synapse brand image | **Ordelis-specific** |
| `.github/workflows/desktop.yml` | **Ordelis-specific**. `gate.yml` is inherited and modified. |
| `web/` (future) | **Ordelis-specific** |

## Identifier migration matrix

Each identifier is **Rename**, **Preserve**, **Migrate** (rename with backward compatibility) or **Audit first**. "PR" refers to the [roadmap](#pr-roadmap).

### User-facing

| Identifier | Where | Decision | PR |
| --- | --- | --- | --- |
| `productName` and window `title` `Synapse`, which also name the macOS bundle `Synapse.app` | `desktop/src-tauri/tauri.conf.json` | **Rename** to `Ordelis`. This changes the bundle path to `Ordelis.app`; see the next row. | N1 |
| **CI bundle paths** `bundle/macos/Synapse.app` | `.github/workflows/desktop.yml:120` and `:128` | **Must change in the same PR as `productName`**, or the Desktop job fails. This is a workflow edit inside N1's scope and is called out in N1's acceptance criteria. | N1 |
| Bundle path in usage and help text `<Synapse.app>` | `desktop/scripts/verify-release-isolation.sh:3` and `:15` | **Rename the text only.** `EXPECTED_EXECUTABLE` (line 9) is an I1 change, not N1. | N1 |
| `<title>Synapse</title>` and the package `description` | `desktop/index.html:6`, `desktop/package.json:6` | **Rename.** The package `name` is an I1 change. | N1 |
| UI copy: `aria-label="Synapse"` on the rail mark, "Set up Synapse", Agents lede and note, Continuity note | `desktop/src/app/Shell.tsx:62`, `desktop/src/pages/Home.tsx:161`, `desktop/src/pages/Agents.tsx:43` and `:157`, `desktop/src/components/Continuity.tsx:46` | **Rename.** | N1 |
| Frontend assertions on that copy | `desktop/src/app/Shell.test.tsx:80`, `desktop/src/app/Workspace.test.tsx:149` (`"Set up Synapse"`) and `:259` (`aria-label` `"Synapse"`) | **Update with the copy** in the same PR. | N1 |
| Native Rust user-facing strings | `desktop/src-tauri/src/projects.rs:21` (the folder-panel message), `projects.rs:64` (registry "newer version of Synapse" error), `desktop/src-tauri/src/lib.rs:86` (startup failure message) | **Rename.** A search of the Rust sources found no test asserting these strings; `cargo test --locked` in CI is the check. | N1 |
| Sample project named `synapse` at `~/code/synapse`, and the test that selects it | `desktop/src/fixtures/mock.ts:111-112`; `desktop/src/app/Context.test.tsx:82-83` | **Not branding: decide explicitly.** It is sample data and a test input. Rename it together with the test, or choose a neutral sample name. Never change it by blind find-and-replace. | N1 |
| `SynapseMark` component | `desktop/src/components/icons.tsx:6`, `desktop/src/app/Shell.tsx:2` and `:63` | **Preserve until the approved vector master exists.** It is replaced, not renamed in isolation, by the design-system and icon work. | U1, U-icons |
| Comments citing the former design reference (`reference/Synapse — telas.*`) and the old theme name | `icons.tsx:2`, `fixtures/mock.ts:5`, `fixtures/sessions.ts:5`, `assets/fonts/README.md:4`, `styles/index.css:4` and `:6`, `pages/Agents.tsx:5` | **Preserve** as provenance of the former design reference. The `index.css` header goes away when the theme is rewritten. | U1 |
| Living how-to commands that embed the bundle path | `docs/implementation/desktop-visual-verification.md` (the build, open and verify steps around lines 160-181) | **Update** with the new bundle path. | N1 |
| Verification *records* that mention `Synapse.app` | `docs/implementation/desktop-visual-verification.md:5,54,62,74,82`, `desktop-architecture.md:61`, `desktop-runtime-bridge.md:97` | **Preserve** as historical records. Add a "formerly Synapse" note where the old name would mislead. | N1 |
| README and `desktop/README.md` | `README.md`, `desktop/README.md` | **Rename**, keeping the provenance statement. Add `NOTICE` (see Provenance). | N1 |
| CLI help text `Absolute external Synapse JSON config`; banner `=== Synapse: authorize ONE controlled Codex probe ===` | `runtime/cli.py:1305`, `runtime/probe_confirmation.py:89` | **Rename the text only.** A search of `tests/` found no test asserting either string, so the full unittest suite is the check. The banner is a human authorization prompt, so it gets careful review. | N1 |
| GitHub repo description, topics, social preview | Repository settings | **Update** after the rename | R2 |

### Compatibility-sensitive

| Identifier | Where | Decision | PR |
| --- | --- | --- | --- |
| Bundle identifier `dev.raioviajante.synapse` | `tauri.conf.json`; determines the app data directory holding `projects.json` | **Migrate** to `app.ordelis.desktop`, with a native-side, validated, copy-only import of the registry under the [semantics below](#registry-migration-semantics-i2). Fallback if the import is not approved: document that projects are re-added through the native folder picker. | I2 |
| Cargo package `synapse-desktop`, library `synapse_desktop_lib`, npm package `synapse-desktop`, executable `Contents/MacOS/synapse-desktop` | `desktop/src-tauri/Cargo.toml`, `main.rs`, `desktop/package.json`, `Cargo.lock` | **Rename** to `ordelis-desktop`, atomically with the scripts that hard-code the old names: `verify-release-isolation.sh` (`EXPECTED_EXECUTABLE`, line 9), `test-verify-release-isolation.sh`, `verify-native-boundary.mjs` (the build-directory pattern at line 95), and the `pgrep -x synapse-desktop` command in `docs/implementation/desktop-visual-verification.md:164`. These are security checks and must still pass, including their negative tests. The executable check stays **exact**: it expects `ordelis-desktop` only and must not accept both names, because accepting the old name would weaken the check. | I1 |
| `SYNAPSE_DEV_PYTHON`, `SYNAPSE_DEV_CHECKOUT` | `desktop/src-tauri/build.rs:19-20`, `desktop/src-tauri/src/runtime_bridge/process.rs:26` and `:28`, release-isolation needles at `verify-release-isolation.sh:40` | **Rename** the variables the build emits and the bridge reads to `ORDELIS_DEV_PYTHON` and `ORDELIS_DEV_CHECKOUT`. Compile-time and dev-only. **During the transition both the old and the new names stay in the release-isolation needle list**, so a bundle that embeds either spelling is rejected. The environment-scrub tests (`runtime_bridge/process/tests.rs:203` and `:213`) assert that **both** `SYNAPSE_` and `ORDELIS_` prefixes, and a canary for each, do not reach the child process. The needle list in `docs/implementation/desktop-runtime-bridge.md:99` is updated to match. The old needles are removed only by a separately reviewed change; the recommendation is to keep them through the first Ordelis-named release. | I1 |
| `SYNAPSE_PROBE_AUTHORIZED`, `SYNAPSE_PROBE_ID`, `SYNAPSE_CONFIRM`, `SYNAPSE_TOKEN`, `SYNAPSE_UNLISTED`, `SYNAPSE_CANARY`, `SYNAPSE_PROJECTS_TEST_CHILD` | `tests/`, `desktop/src-tauri/src/projects/registry/tests.rs` | **Audit first.** They appear in tests only and, from their names, act as environment canaries or negative cases. Their exact purpose has not been read in detail. Add `ORDELIS_` twins; do not simply replace. | I1 |
| `synapse_observed`, `synapse_enforced` | `runtime/execution_safety_gate.py` (`ORIGINS`, `_TRUSTED_ORIGINS`) | **Preserve.** Trust-origin vocabulary of the safety gate. Renaming silently changes trust classification and may break stored evidence. If a rename is ever wanted, it is a dedicated security-reviewed change that accepts both spellings on read. | none |
| `runtime/synapse_plan.py` and its imports (`cli.py`, `codex_probe_runner.py`, `execution_safety_gate.py`, `probe_authorization_ledger.py`, `runtime_validation.py`; six test files) | `runtime/` | **Preserve.** Internal module, named in the import-boundary allowlist in `tests/test_runtime_status_bridge.py`. A later rename could ship a re-export shim. | none |
| Probe scratch prefix `synapse-codex-probe-` | `runtime/codex_probe_runner.py` | **Audit first.** Check for any cleanup glob keyed on the prefix before renaming, so old scratch directories are not orphaned. | I1 |
| Internal temp-directory prefixes and test canaries: `synapse-ipc-`, `synapse-projects-`, `synapse-bridge-`, `synapse-stderr-canary-`, `synapse-child-locked`, `synapse-child-ready`, `/nonexistent/synapse-test…`, and `synapse-dep-info-` in the boundary script; the CI scratch name `synapse-capability-probe-work-` | `desktop/src-tauri/src/lib.rs:500`, `projects/registry/tests.rs:51,596-597,798,1051,1123,1256,1264`, `runtime_bridge/process/tests.rs:11,171`, `desktop/scripts/verify-native-boundary.mjs:122`, `.github/workflows/gate.yml:89` | **Preserve.** Internal and cosmetic; renaming adds diff without changing behavior. Rename opportunistically only when a file is touched for another reason. `SYNAPSE_PROJECTS_TEST_CHILD` (`registry/tests.rs:593`) and the `synapse-child-*` markers look like one parent-child test protocol (not read in detail), so rename them together or not at all. | none |
| `/desktop/src-tauri/.dev-python` in `.gitignore` | `.gitignore:33` | **Preserve.** Contains no product name. | none |
| `Cargo.lock`, `pnpm-lock.yaml` entries | `desktop/` | Follow the package renames. Regenerate with `--locked` builds only. | I1 |
| Window `theme: "Dark"` and `backgroundColor: "#100e0c"` | `tauri.conf.json` | **Change** with the design system. The new design has light and dark themes. | U1 |

### Inherited MCO, preserved

| Identifier | Decision |
| --- | --- |
| Python package `runtime`, project name `mco` in `pyproject.toml`, console script `mco`, `bin/mco.js`, the `mco` launcher, npm package `@tt-a1i/mco`, `skills/mco-cli/`, `.mco/state.json` | **Preserve.** Any renaming needs its own tested package-migration plan with compatibility aliases. |
| `pyproject.toml` URLs to `mco-org/mco` | **Preserve.** They are correct attribution. |
| `LICENSE`, `README.zh-CN.md`, `CHANGELOG.md`, `docs/releases/`, `docs/archive/`, `docs/contracts/`, `docs/reference/`, the `mco-*` brand assets | **Preserve.** Append to the changelog; never rewrite. |
| `docs/implementation/synapse-mvp-plan.md` and similar historical filenames | **Preserve** the filename (it is referenced from code comments and other documents). Add a "formerly Synapse" note. |
| `RaioViajante/synapse` links in historical docs (10 files) | **Leave.** GitHub redirects renamed repositories. Update only living documents. |
| `docs/assets/brand/synapse-icon.png` and the current amber icon set | **Replace** only once the approved vector master exists (D6). Reuse `desktop/scripts/prepare-app-icon.swift`. |

### Release, ownership and reporting hygiene

| Item | Finding | Decision | PR |
| --- | --- | --- | --- |
| `.github/workflows/publish-npm.yml` | Publishes `@tt-a1i/mco` when a GitHub Release is published. Today it refuses to publish because no `NPM_TOKEN` secret exists (the repository has none), so safety rests on a missing secret alone. | **Gate or disable**; do not delete. Guard it so it can never run for this repository, and document why. | H1 |
| `.github/workflows/preview-package.yml` | Builds a `tt-a1i-mco-*.tgz` preview. It does not publish. | **Review** within H1. | H1 |
| `.github/CODEOWNERS` | Every path is owned by `@tt-a1i`, the upstream author. GitHub's documentation says a code owner needs explicit write access, and an owner without it is not assigned. The only collaborator on this repository is its owner, so the file assigns no reviewer today. | **Update** to the Ordelis maintainer (`@RaioViajante`). Keep the upstream author credited in `NOTICE`, not as review owner. Be aware that CODEOWNERS only *requests* review; GitHub documents enforcement as a separate branch-protection or ruleset setting ("Require review from Code Owners"). Whether that setting is available on this repository and plan has **not** been verified, and the repository has no branch protection or rulesets today. | H2 |
| `.github/ISSUE_TEMPLATE/config.yml` | The security-disclosure link points at `tt-a1i/mco/security/advisories/new`. Vulnerability reports about this project would reach upstream. No `SECURITY.md` exists. | **Repoint** it at this repository (H2, using today's URL), and **re-validate** it once visibility and name are final (H3). See the [security-reporting facts](#security-reporting-and-ownership-facts-checked-for-this-record) below. | H2, H3 |

### Security-reporting and ownership facts checked for this record

Checked against GitHub's documentation and read-only API queries. "Not established" means the pages read do not say, so no definite claim is made.

| Fact | Evidence | Status |
| --- | --- | --- |
| Private vulnerability reporting is a feature of **public** repositories | [GitHub docs](https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/configuring-private-vulnerability-reporting-for-a-repository): "Owners and administrators of public repositories can allow security researchers to report vulnerabilities". Private repositories are not mentioned. | Verified for public; private **not established** |
| It is currently off for this repository | API read of the repository's private-vulnerability-reporting setting returned `enabled: false` | Verified |
| Repository security advisories on a **private** repository | [GitHub docs](https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/about-repository-security-advisories) describe them for "maintainers of public repositories" and do not mention private ones | **Not established** |
| Whether the `…/security/advisories/new` URL redirects after a rename, or works after the repository goes private | Not covered by the pages read | **Not established**; check after R2 |
| A code owner needs explicit write access, otherwise no owner is assigned | [GitHub docs](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners) | Verified |
| Code-owner review is *enforced* only by a branch-protection or ruleset setting | Same page; [protected-branches docs](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches) | Verified |
| Branch protection and rulesets on a Free personal account, public or private | The protected-branches page read does not state plan or visibility availability for personal accounts | **Not established** |
| Current state: `main` unprotected, no rulesets, one collaborator (the owner), no `SECURITY.md` | API reads and the working tree | Verified |
| Whether a repository owner can approve their own pull request when they are the only collaborator | Not checked | **Not established**. It determines whether "required review" can be enforced with a single maintainer. |

**Ordering consequences.**

- While the repository is public, enabling private vulnerability reporting and repointing the issue-template link at this repository are both possible, so H2 does them with today's URL. This has no dependency on the rename.
- If the repository is later made private (R1), the documentation does not support private vulnerability reporting for it. The reporting channel must then be restated rather than assumed. H3 does this after R1 and R2 are decided, and may add a `SECURITY.md` that names the channel in a way that does not depend on visibility.
- The runbook's post-rename checks (below) include re-testing the advisory URL, so a broken redirect is caught rather than assumed away.
- Nothing in this list is executed by this PR.

### Registry migration semantics (I2)

The bundle identifier determines the app data directory, which holds the local project registry. These are the facts from source and the semantics I2 must meet.

**Facts (verified in source).**

- Registry files live in `<app_data_dir>`: `projects.json`, `projects.lock`, `projects.json.tmp`, and `projects.json.corrupt-<ms>[-<n>]` (`docs/implementation/desktop-local-projects.md`, Storage; constants in `registry.rs:17-20`).
- The registry and the folder panel are compiled **only on macOS** (`projects.rs:12-15`). Other targets return `UnsupportedPlatform` for every project command (`projects.rs`, non-macOS impl). The import is therefore macOS-only.
- Registry functions take the directory as a parameter. Only startup resolves it: `lib.rs:73` builds `Projects::new(app.path().app_data_dir().ok())`.
- The read protocol never creates, writes, renames, truncates or re-permissions anything, and rejects a directory that is a symlink, foreign-owned or group- or world-writable, and a file that is a symlink, a FIFO, not regular, not owned by the effective user or over 256 KiB.
- `remove` rewrites `projects.json` and keeps the file even when the list becomes empty (`registry.rs:111-123`). `reset` acts only on a corrupt registry, keeps it as `projects.json.corrupt-*`, and then removes `projects.json` (`registry.rs:127-149`). `projects.lock` is created by the first locked operation and never removed. `commit` always renames a freshly written `projects.json` into place, including for an empty list (`registry.rs:359-383`). No existing test pins the "last project removed" case.
- Each record stores the project folder's `dev` and `ino` and its path, not anything tied to the data directory, so a record stays meaningful in a different data directory on the same volume.
- The identifier is the same for debug and release builds (`desktop-local-projects.md:28`), so both would import from the same legacy directory.
- The frontend uses no `localStorage`, `sessionStorage` or IndexedDB (searched `desktop/src`; one unrelated comment matched), so there is no browser-storage migration.

**Required semantics.**

1. **Native-side only, no new IPC surface.** The import runs in Rust at startup or inside an existing code path. It adds no command, no permission and no frontend input. The command list in `build.rs`, `capabilities/default.json`, the autogenerated permission files and the invoke handler stay as they are, and the existing test that keeps them equal continues to pass unchanged. The legacy directory is derived in Rust, as a sibling of the new data directory, from a fixed constant for the legacy identifier. It is never taken from the webview.
2. **Existing new registry takes precedence.** The import is skipped if the new data directory already contains any registry artifact: `projects.json`, `projects.lock` or any `projects.json.corrupt-*`. It is whole-or-nothing into an empty registry and never merges.
3. **No restoration of intentionally removed projects.** Because of rule 2, none of these re-triggers an import: removing one project, removing the last project (the file stays with an empty list), or resetting a corrupt registry (the backup stays). The first import itself leaves `projects.json` and `projects.lock` behind. This rests on the behavior in the facts above, and I2 adds a test for each sequence. If the existing artifacts turn out not to guarantee it, a durable marker is added as an explicit, reviewed change to the file table, not silently.
4. **Validation through the existing loader.** The legacy registry is read only through the existing read protocol and strict parser. Only a valid version-1 result is imported. Empty, corrupt, newer-version, unavailable, symlinked, FIFO, foreign-owned or over-permissive input is skipped. The user then sees an empty list and the documented fallback. No legacy path or content is shown in an error, and no repair or `reset` is ever applied to the legacy directory.
5. **Copy, never move.** The legacy directory and its files are never modified, renamed, locked for writing or deleted. The parent of the new data directory is never created (an existing invariant).
6. **Same write guarantees.** The import commits through the existing locked, synced, atomic write path. Records are carried as validated, with `id` and `next_id` preserved. Per-record folder checks (missing or changed folder) continue to apply at the moment of use.
7. **Safe under concurrency.** The import runs under the new directory's lock, so two instances racing produce one import and one skip.
8. **Scope.** macOS builds only. Other platforms keep `UnsupportedPlatform`.
9. **Fallback.** If the import is not approved or a legacy registry fails validation, the documented fallback is to re-add projects with the folder picker. Registry contents are paths and folder identities only, so nothing else is lost. Release notes say so.

**Tests I2 must add** (none exists today): import from a valid legacy fixture; skip when the new registry exists; the three removal and reset sequences above followed by a relaunch; a hostile legacy directory (symlink, foreign owner, group- or world-writable, FIFO, oversize, corrupt, newer version); the legacy directory unchanged byte for byte and in listing before and after; no new command in the manifest.

## Platform assumptions

Documented here so new development stays portable. **No platform other than macOS has been built or verified for the desktop app.** CI builds it on `macos-latest` only (`desktop.yml`). Statements below are labelled as either *source* (read from the code) or *evidence* (a CI job or a run). Source-based expectations are not build evidence.

### macOS-only

- `rfd` (native folder panel) is a dependency only under `cfg(target_os = "macos")` in `desktop/src-tauri/Cargo.toml`.
- Desktop bundle targets are `app` and `dmg` (`tauri.conf.json`). `desktop/scripts/verify-release-isolation.sh` uses `PlistBuddy`, `otool`, `lipo` and `nm` and inspects `Info.plist` and `Contents/MacOS/`, so release isolation is verified on macOS bundles only. `desktop/scripts/prepare-app-icon.swift` is a Swift script (not read in detail here).
- The current UI copy and shortcuts assume macOS conventions in places. The design exports show macOS window chrome and `⌘` shortcuts. Windows and Linux need `Ctrl` mappings, native window controls and neutral copy such as "Stored on this Mac".

### Conditional-compilation boundaries in the desktop Rust code

These are read from the source (*source*), and they are deliberate. The unconditional `std::os::unix` and `libc` use is **inside modules that are compiled only on macOS** or inside test code.

| Boundary | Where | Effect on other targets |
| --- | --- | --- |
| Registry and folder modules | `desktop/src-tauri/src/projects.rs:12-15` (`#[cfg(target_os = "macos")] mod folder;` and `mod registry;`). Their `std::os::unix` and `libc` use (`getpwuid_r`, `geteuid`, `flock`, `O_NOFOLLOW`, `O_DIRECTORY`, modes `0o600` and `0o700`) lives here. | Not compiled. Every project command (`list`, `add`, `remove`, `reset`) returns `UnsupportedPlatform` ("Local projects are not supported on this platform."). |
| Native folder panel | `projects.rs`, `NativePicker::pick`: the `rfd` branch is `#[cfg(target_os = "macos")]`, with an explicit `#[cfg(not(target_os = "macos"))]` branch that returns no selection | A stub that cannot add a project. |
| Development runtime bridge | `desktop/src-tauri/src/runtime_bridge.rs:9`: `#[cfg(any(test, all(feature = "dev-runtime", target_os = "macos")))]` gates the process code in `runtime_bridge/process.rs`, which uses `std::os::unix` and `libc` (`geteuid`, `sysconf`, `fcntl`, `process_group`, `pre_exec`) | Not compiled in a default build, and the feature is development-only. It **is** compiled under `cfg(test)` on every target. |
| Test code | `desktop/src-tauri/src/lib.rs:613` (`std::os::unix::fs::PermissionsExt` inside `#[cfg(test)]`); registry tests use `mkfifo` and `PermissionsExt` | Unix-only. |
| `libc` dependency | `desktop/src-tauri/Cargo.toml:19` | Declared unconditionally. Whether it matters on Windows has not been checked. |

**What the source supports, and what it does not.**

- *Source expectation:* a **default, non-test** build is designed to compile on non-macOS targets, with project registration and the development bridge compiled out. This is an expectation from reading the code. It has **not** been confirmed by building.
- *Source expectation:* `cargo test` on a non-Unix target is expected not to compile, because test builds include `process.rs` and the unix test code above.
- *Not established:* whether the rest of the app (Tauri, the frontend, `build.rs`) builds, runs or behaves correctly on Windows or Linux. No Windows or Linux desktop job exists and none was run for this record.
- A Windows or Linux implementation of local project registration is a design task, not a stub: the registry's safety properties (refusing untrusted or foreign-owned directories, no symlink following, advisory locking) are implemented with the Unix primitives above, and equivalent protections would need to be designed and reviewed.

### Python runtime

- Already cross-platform in part: `runtime/platform.py` shims `os.getuid`, `os.killpg` and `.cmd` spawning for Windows, and `gate.yml` runs a Windows installer smoke job (`tests.test_platform_helpers`, `tests.test_acp_transport`, packaging and CLI smoke).
- Not portable: the session daemon and client (`runtime/session/daemon.py`, `client.py`) use `socket.AF_UNIX`. `runtime/probe_authorization_ledger.py` calls `os.geteuid()`, opens files relative to a directory descriptor (`dir_fd`) and enforces `0o700`/`0o600` modes, none of which has the same meaning on Windows (`os.geteuid` does not exist there). `runtime/vault_context.py` opens files with `os.O_NOFOLLOW`, which Windows does not define, and `runtime/codex_probe_runner.py` checks `0o700` scratch directories. Their Windows behavior is unverified.
- Process-group termination falls back to `terminate`/`kill` on Windows, with weaker guarantees than `killpg`.

### Evidence ledger

*Evidence* means a CI job or a run that exercised the thing. *Source* means read from the code only.

| Component | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Desktop app build and tests | **Evidence:** `Desktop` job on `macos-latest`, passing on `e0d2778` | None. Source: default build designed to compile (see above), `cargo test` expected not to. | None. Source: same as Windows for the build; Unix primitives exist, but the registry stays compiled out. |
| Desktop frontend (lint, typecheck, tests, build) | **Evidence:** the same `Desktop` job | None | None |
| Release-isolation and boundary verification | **Evidence:** `Desktop` job on a real bundle | Not applicable: macOS-only tooling | Not applicable |
| Python runtime, full unittest suite | **Evidence:** a full local run on this working tree (813 passed, 4 skipped) | None | The `Runtime G/H gate` job runs on `ubuntu-latest`. Its coverage was not audited for this record. |
| Python CLI packaging and install | **Evidence:** `Packaging smoke (macOS)` | **Evidence, narrow:** `Installer smoke (Windows)` runs `tests.test_platform_helpers`, `tests.test_acp_transport`, `npm pack` and install, and `mco --help`, `--version`, `skills read` and `install --dry-run`. It does not exercise the session daemon, the authorization ledger, vault context or probe modules. | None beyond the gate job |
| Installers | `app` and `dmg` are configured. CI builds `--bundles app` only (`desktop.yml:102,117,125`), so there is no CI evidence for the `dmg`. | None configured | None configured |

No platform is declared supported on the strength of any row above (D9).

## Repository visibility evaluation

**Nothing is changed by this document. Visibility changes need a separate, explicit authorization.**

### Current state (read-only queries)

Public; `isFork: false`; 0 stars, 0 watchers, 0 forks; one collaborator (the owner); 5 open issues; no tags or releases; no other open pull request than this record's own while it is under review; no GitHub Pages, wiki, discussions, webhooks, Actions secrets or environments; no branch protection or rulesets; license MIT; no topics. The account plan was not readable with the available token scope.

### Consequences of public to private

Sources: [GitHub: setting repository visibility](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/managing-repository-settings/setting-repository-visibility) and [GitHub billing for Actions](https://docs.github.com/en/billing/managing-billing-for-your-products/managing-billing-for-github-actions/about-billing-for-github-actions).

| Area | Consequence | Impact here |
| --- | --- | --- |
| Public access | The page read describes the opposite direction (private to public: code visible to anyone). The expected inverse is that non-collaborators lose access to code, issues and pull requests and that existing links stop working for them. That is an inference, not a quoted statement. Anyone with an existing clone keeps that copy. | Low. No known downstream consumers: nothing is published to npm or any registry from this repository (no tags, releases or packages were found). |
| Stars and watchers | Erased, per the page. | None (0 and 0). |
| Forks | Public forks are detached into a separate network and stay public. | None (0 forks). |
| Contributors | Only invited collaborators can read or push. | None today (owner only). |
| GitHub Pages | A published site is unpublished on Free plans. | None (no Pages). |
| Actions cost | Standard runners are free for public repositories. For private ones usage is billed against the plan's included allowance. The billing page lists GitHub Free as "$12 USD" of included compute per month, expressed as money and not minutes, with per-minute rates of $0.006 (Linux, 2-core), $0.010 (Windows, 2-core) and $0.062 (macOS, 3- or 4-core). Per-job rounding, and whether `macos-latest` matches the listed runner size, were not checked. | **Material, and an estimate.** The account plan could not be read, so the allowance is an assumption.<br>Measured on recent `main` runs: `Desktop` (macOS) ≈14 min, ≈$0.87 at the macOS rate; `Gate` ≈0.5 min macOS + 1.3 min Windows + 3 min Linux, ≈$0.06.<br>**Both workflows fire on both `push` and `pull_request`.** `gate.yml` has no path filter and `desktop.yml` filters on `desktop/**`, `runtime/**` and a few other paths on both events, so a pushed branch with an open PR runs each twice (this PR shows two `Gate` runs).<br>Estimated cost per push to a PR branch: ≈$0.13 without changes under `desktop/**` or `runtime/**`, and ≈$1.9 with them. At $12 that is on the order of 6 pushes per month with desktop-affecting changes, or about 95 docs-only pushes. Treat these as order-of-magnitude figures. |
| Branch protection, rulesets, required reviews | Availability on a private repository under a Free personal account is **not established**. The plans page lists some such tools among paid plans' private-repository features, but the branch-protection documentation read does not state plan or visibility availability for personal accounts. | Nothing is configured today (verified: `main` is unprotected, 0 rulesets), so nothing is lost now. If they are unavailable, D10's review requirement is a process rule only. If they are available, a single-collaborator repository may still be unable to satisfy a required review (not established). |
| Security features and reporting | The page read says custom Dependabot alert rules are disabled and code scanning is unavailable on a private repository unless GitHub Code Security is enabled, and that dependency graph and alerts stay on. Private vulnerability reporting is documented for public repositories only. | Low for Dependabot and scanning: nothing relies on them today (Dependabot security updates are disabled and secret scanning with push protection is enabled, per the API). For reporting, see the [security-reporting facts](#security-reporting-and-ownership-facts-checked-for-this-record). |
| Webhooks, integrations | None configured. | None. |
| Upstream attribution | Unaffected: the history and license are unchanged. | None. |
| Going public later | Code and Actions history and logs become visible to everyone; anyone can fork; stars and watchers reset; push rulesets are disabled. | **Pre-publication review is required**, including history, issue text, Actions logs and the contents of any private material committed while private. Nothing is committed that the maintainer would not want public. |

### Recommendation

The audience-related costs are small because the repository has no audience yet (0 stars, watchers and forks). The real trade-off is CI cost, possible loss of review enforcement and a changed security-reporting channel, against the benefit of developing the private website and desktop design in one repository. The CI cost and the protection availability are unverified for this account.

Before authorizing, the maintainer should confirm: the GitHub plan; the Actions allowance and the cost estimate above; whether private-repository branch protection is available and needed; how security reports will be received while private; and the plan for a pre-publication history audit. If the change is authorized, make the repository private **before** the first private asset or private-derived implementation is committed, and perform the rename in the same maintenance window or immediately after.

Until then, the roadmap blocks `web/` and the desktop redesign (anything derived from the private designs), and allows only changes that contain no private material.

## Repository rename runbook (prepared, not executed)

Destination `RaioViajante/ordelis`. Reference: [GitHub: renaming a repository](https://docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository).

**What GitHub documents.** The web URL, issues, stars and followers are redirected, and `git clone`, `fetch` and `push` against the old location keep working "as if made on the new location". GitHub Pages project URLs and actions hosted in the renamed repository are **not** redirected (this repository has no Pages site and hosts no action, and no workflow references `RaioViajante/synapse`). Creating a new repository with the old name breaks the redirect. **What the page does not say:** whether the REST API, `gh`, webhooks or the security-advisory URL follow the redirect, and it does not list pull requests explicitly. The checks below verify these instead of assuming them.

**Preconditions**

1. Explicit maintainer authorization for the rename.
2. H1 and H2 merged. H2 has no dependency on the rename (see the roadmap), so there is no circularity. H3 and the post-rename security-link check come after.
3. **Open pull requests are allowed and are not a blocker.** A rename preserves pull requests (D3), and their head branches live in the same repository. They are recorded in the baseline below and re-checked afterwards.
4. No workflow run is in progress at the moment of the rename (`gh run list --status in_progress` is empty), so no run straddles the rename.
5. The rename does not depend on the visibility decision. Visibility is a separate authorization (R1), scheduled first in the roadmap. Either order works technically.

**Baseline to capture before the rename** (read-only, saved outside the repository):

- `gh pr list --state all` with number, state, head ref, head SHA and base ref.
- The full ref listing from `git ls-remote origin`, including `refs/heads/*`, `refs/tags/*` and `refs/pull/*`, with SHAs.
- Issue counts by state, the commit count on `main`, and `gh repo view` output for name, visibility, fork flag, default branch, description and topics.
- The latest check status for `main` and for each open pull request head.
- Every local clone and worktree, with its `origin` URL.

**Steps (maintainer or authorized agent)**

1. Settings, General, repository name: `ordelis`.
2. In **each** local clone and worktree: `git remote set-url origin github-personal:RaioViajante/ordelis.git`, then `git fetch --all`. The `github-personal` host alias is a local SSH configuration; keep whichever alias that clone already uses.
3. Update the repository description and topics. Keep "forked from MCO" or equivalent attribution in the description.
4. Update links in **living** documents only (`README.md`, `desktop/README.md`, `docs/README.md`).
5. **Never create a repository named `synapse`** under this account.

**Post-rename checks.** Each must pass or be recorded with its result.

| Check | How | Expected |
| --- | --- | --- |
| Repository state | `gh repo view RaioViajante/ordelis` | New name; visibility, fork flag, default branch and counts match the baseline |
| Web redirect | `curl -sI` on the old repository URL | A redirect to the new URL |
| Git through the old URL | `git ls-remote` on the old HTTPS and SSH URLs | Works and lists the same SHAs |
| API through the old name | `gh api repos/RaioViajante/synapse` | **Record the result**; not documented on the page read |
| Refs | `git ls-remote` on the new URL compared with the baseline | Every head, tag and `refs/pull/*` entry present with an identical SHA |
| Pull requests | `gh pr list --state all` compared with the baseline | Same numbers, states, head and base refs, head SHAs, comments and review requests |
| Pushing to a PR branch | An ordinary push (no force) to an open PR branch, only when there is real work to push | Accepted, and the PR updates |
| CI | The next ordinary push, or a re-run on an open PR head | `Gate` passes; `Desktop` passes when its paths change; no workflow still references the old name (`git grep RaioViajante/synapse` over `.github`) |
| Remotes | `git remote -v` and `git fetch --all` in every clone and worktree | New URL; no redirect warnings |
| Security link | Request the advisory URL under the new name | **Record the result**; H3 acts on it |
| Counts | Issues, pull requests, commits on `main` | Equal to the baseline |

Text that earlier bot comments wrote with the old name stays as history and needs no action.

**Rollback:** rename back in Settings. Redirects follow the rename in either direction, but any links created under the new name need updating, and the post-rename checks are repeated. No data is deleted by a rename. If a repository named `synapse` is created by accident, treat it as an incident: it breaks the redirect.

## Outstanding prerequisites

These are recorded, **not** assumed complete.

| Prerequisite | Needed by | Status |
| --- | --- | --- |
| **Private-content gate:** either R1 (the visibility change) is executed, **or** an explicit, recorded amendment to D2 approves a named alternative strategy | W1, U1 and later (anything derived from the private designs) | **Open.** R1 needs its own authorization. Any alternative that departs from a single monorepo needs the D2 amendment first. Neither is assumed. |
| GitHub plan, the Actions allowance and cost estimate, availability of private-repository branch protection, and how security reports will be received while private | R1 | **Open** |
| Approved vector master of the Ordelis symbol | Icons, favicon, installers, social images (U-series, P-series) | **Open.** The provisional reconstruction is not used for final assets. |
| Final consistency check of the canonical design against the approved design: cross-platform language, shared layout grid, responsive layouts, latest storytelling | W1 | **Open** |
| Confirm which foundations and components board is authoritative for the desktop app (the desktop export's or the web export's system board) | U1 | **Open** |
| Updates and telemetry statement for the website privacy page | W-series | **Open.** The app currently sends nothing and its CSP allows only IPC. |
| Decision on the sample project named `synapse` in the fixtures and the test that selects it | N1 | **Open** (see the identifier matrix) |
| Maintainer contact or channel to name in `SECURITY.md` | H3 | **Open** |
| Final product-name check before the first Windows installer (the WiX upgrade code follows `productName`) | P-series | **Open** |
| Plan for a pre-publication audit if the repository is later made public again | Open-source publication | **Open** |

## Risk register

| # | Risk | Likelihood | Impact | Mitigation | Owner PR |
| --- | --- | --- | --- | --- | --- |
| 1 | Private design or website material lands in a public repository, including on a PR branch | Medium | High | Do not commit exports; the private-content gate must be satisfied first; review each diff for private material | W1, U-series |
| 2 | App data orphaned by an identifier change | Medium | Medium | Native-side, validated, copy-only import with the semantics above; documented re-add fallback | I2 |
| 3 | Safety-gate vocabulary or verification scripts renamed carelessly | Low | High | Trust-origin values preserved; verification scripts updated atomically with their negative tests; old and new development-environment names both kept in the isolation needles | I1 |
| 4 | Accidental npm publish under the upstream scope | Low today (no `NPM_TOKEN` exists), grows if one is added | High | Hard guard on `publish-npm.yml` | H1 |
| 5 | Security reports reach upstream, or no channel exists after the repository goes private | Medium | Medium | H2 repoints the link now while the repository is public; H3 re-validates the channel after R1 and R2; private vulnerability reporting is documented for public repositories only | H2, H3 |
| 6 | Attribution erosion | Low | High | Provenance requirements; `LICENSE` untouched; standalone `NOTICE` plus a README link | B1, N1 |
| 7 | Private-repository CI cost and possible loss of review enforcement | Medium | Medium | Both unverified for this account; confirm the plan first; the estimate counts `push` and `pull_request` triggers; keep path filters; review may be a process rule only | R1 |
| 8 | Windows or Linux claimed before verification | Medium | High | No support claims; download surfaces stay "not yet available"; the evidence ledger separates source expectations from evidence | P-series |
| 9 | A Windows or Linux project-registry port weakens the Unix-hardened guarantees | Medium | High | Local projects return `UnsupportedPlatform` off macOS by design; a port needs a separate reviewed design, not a stub | P-series |
| 10 | Redesign copies prototype HTML that violates the CSP (remote fonts, inline styles) | Medium | Medium | Rebuild components in React and Tailwind; keep `font-src 'self'`; CSP checks stay in CI | U-series |
| 11 | Security-sensitive screens (approve, run, merge, discard, terminal) look functional | Medium | High | Inert, labelled fixtures only; no new IPC commands; execution stays disabled | U-series |
| 12 | Flaky tests (issue #44) mask rename regressions | Low | Medium | Triage reruns explicitly; do not hide failures | all |
| 13 | A rename PR is large and unreviewable | Medium | Medium | One concern per PR as listed below | all |
| 14 | A new repository is created under the old name | Low | Medium | Runbook step 5 | R2 |
| 15 | Provisional icon art leaks into releases | Low | Medium | Block icon and installer work on the approved vector master | U-series, P-series |
| 16 | Renaming `productName` breaks the Desktop CI job, which hard-codes `bundle/macos/Synapse.app` | High if missed | Medium | N1 includes the `desktop.yml` paths and its acceptance requires the Desktop job to pass | N1 |
| 17 | The registry import restores projects the user deliberately removed | Medium | Medium | New-registry-precedence rule plus a test for each remove and reset sequence | I2 |
| 18 | A blind find-and-replace changes sample data, test inputs or security vocabulary | Medium | High | The identifier matrix classifies each site; N1 ends with a grep audit against an explicit allowlist of preserved items | N1, I1 |
| 19 | A source-based platform expectation is mistaken for build evidence (this record's first revision did exactly that for Windows) | Medium | Medium | The evidence ledger; the first P1 step adds Windows and Linux compile jobs and replaces expectations with results | P1 |

## PR roadmap

One concern per pull request. Every PR carries: acceptance criteria, local verification, CI evidence, an independent review before merge, and a rollback note. None changes the safety gate's behavior. Order follows the maintainer's sequence (record, release and ownership hygiene, visibility and rename, branding, compatibility-sensitive identifiers and data, website, desktop design, cross-platform). Adjustments: H3 follows R1 and R2 so the security channel is validated against the final visibility and name; W1 and U1 wait for the private-content gate; N1 contains a workflow edit, as a separate commit.

| ID | Scope | Acceptance criteria | Verification | Rollback | Depends on |
| --- | --- | --- | --- | --- | --- |
| **B1** | This record. Docs only. | Adds `docs/migration/ordelis.md` and one index link. No code, workflow or asset changes. No private material. | Diff touches only the two docs files; links resolve; tables well-formed; `Gate` green; review of the amended head by someone other than its author | Revert the commit | none |
| **H1** | Gate or disable `publish-npm.yml`; review `preview-package.yml` (its PR comment names the upstream package) | The workflow cannot run for this repository, with a comment explaining why. The upstream package name is untouched. `Gate` still green. | Inspect with a dry trigger; no publish step reachable; `Gate` green | Revert | B1 |
| **H2** | `CODEOWNERS` to `@RaioViajante`, keeping a header comment that credits the upstream MCO maintainers; repoint the issue-template security link at this repository using today's URL; **maintainer action:** optionally enable private vulnerability reporting while the repository is public | No path is owned by the upstream author. The link targets this repository. No claim is made about enforcement. | Review the file diffs; read the PVR setting back through the API | Revert; disable the setting | B1. **No dependency on R2.** |
| **R1** | **Maintainer action.** Visibility change, if authorized | Consequences and cost estimate re-confirmed at execution time; plan known | `gh repo view` shows visibility; CI still runs | Make public again (note the consequences above) | Separate authorization |
| **R2** | **Maintainer action.** Rename to `RaioViajante/ordelis` per the runbook | Every post-rename check passes or is recorded | The runbook's baseline and post-rename checks | Rename back; repeat the checks | H1, H2, authorization. Open PRs do not block. |
| **H3** | Re-validate the security-reporting channel after R1 and R2; add `SECURITY.md` naming it; fix the issue-template link if the redirect failed | The channel is stated truthfully for the final visibility (private vulnerability reporting is public-only per GitHub's documentation) | The advisory-URL result from the runbook; read-back of settings | Revert | H2, R1 and R2 decided |
| **N1** | User-facing branding exactly as in the identifier matrix's user-facing rows: `productName` and title, **`desktop.yml` bundle paths** (own commit), usage text, `index.html` and package `description`, UI copy, the frontend assertions on that copy, native Rust strings, living how-to commands, READMEs, CLI help text and probe banner; add `NOTICE` and link it from the README; resolve the sample-project fixture decision | **The `Desktop` job passes with the `Ordelis.app` path.** A grep audit finds no `Synapse` on user-facing surfaces except the matrix's preserved items. Compatibility-sensitive identifiers (`synapse_observed`, `synapse_enforced`, `synapse_plan`, the executable and crate names, the environment names) are untouched. `NOTICE` exists and the README links to it. | `pnpm lint && pnpm typecheck && pnpm test`; `cargo test --locked`; the Python `unittest` suite; `Gate` and `Desktop` green; the grep audit output attached | Revert | R2 |
| **I1** | Atomic rename of the Cargo and npm packages, library, executable, `ORDELIS_DEV_*` build variables and the scripts that hard-code them | `verify-release-isolation` and `verify-native-boundary` pass on a real build, and their negative tests pass. The executable check is exact. **The isolation needle list contains both old and new development-environment names**, and the scrub tests cover both prefixes. | `test-verify-*.sh`; `pnpm tauri build --bundles app` followed by `verify-release-isolation.sh`; `cargo test --locked` | Revert | N1 |
| **I2** | Bundle identifier `app.ordelis.desktop` plus the registry import under the [semantics above](#registry-migration-semantics-i2), or the documented fallback | Every listed test passes, including a relaunch after each remove and reset sequence. The legacy directory is unchanged. The command manifest, capabilities and permissions are unchanged. | New registry tests; `cargo test --locked`; a manual launch with a pre-existing legacy registry on macOS | Revert; the legacy data directory is untouched | I1 |
| **W1** | `web/` scaffold: Next.js with static export, TypeScript, own pnpm lockfile and path-filtered CI | Builds independently of `desktop/`; no private exports committed; no deployment | Build in CI; `Desktop` unaffected | Remove `web/` | Private-content gate; design consistency check |
| **W2 onward** | Website increments: layout grid, hero, motion, supporting pages, accessibility and reduced-motion checks | Responsive at the documented breakpoints; reduced-motion honoured; placeholders left visibly unfilled; no platform or download claims; no deploy | Ported browser checks; manual keyboard pass | Revert | W1 |
| **U1** | Desktop design system: tokens, light and dark themes, system-native font stack, JetBrains Mono for code | CSP unchanged; no remote fonts; contrast checks pass in both themes; no Apple font files; unused bundled fonts and their license text reconciled | Tests; contrast check; `Desktop` green | Revert | Private-content gate; N1 |
| **U2 onward** | Incremental desktop UI: shell and navigation, then screens as labelled fixtures | Existing tests pass; no new IPC commands; sample-data labelling intact; execution stays disabled | Tests; visual verification per `desktop-visual-verification.md` | Revert per PR | U1 |
| **U-icons** | Application and installer icons | Generated only from the approved vector master | Icon pipeline output inspected | Revert | Approved master |
| **P1 onward** | Cross-platform. **First step: add Windows and Linux compile jobs (default build, and tests separately) and record the real results, replacing this record's source-based expectations with evidence.** Then: a reviewed design for Windows and Linux project registration (until then they keep `UnsupportedPlatform`), CI jobs, packaging, signing, release metadata, and shipping `NOTICE` and `LICENSE` with any bundle that contains MCO-derived code | A platform is documented as verified only after its build, packaging and security behavior pass in CI and by hand. Download surfaces stay unavailable until real metadata exists. | Per-platform CI and manual checks | Revert per PR | I2; relates to issue #38 |

Existing open issues that interact with this plan: #38 (release packaging assurance, anchors P1), #44 (flaky ledger consumers), #39 (runtime import hygiene), #23 (session readiness race).

## Explicitly out of scope

Application changes; renaming any inherited MCO interface; enabling agent execution; deploying the website or connecting `ordelis.app`; publishing any release; telemetry or accounts; committing private design exports; creating a new organization.

## Revisions

- **First revision.** The initial record.
- **Second revision.** Corrections made after a review of the first revision: the Windows compilation statement was replaced by the actual conditional-compilation boundaries and an evidence ledger; the identifier matrix and N1 scope were completed with the CI, test, Rust-string and fixture couplings; the H2 and R2 ordering was resolved by splitting out H3; the rename runbook no longer treats open pull requests as a blocker and gained baseline and post-rename checks; registry migration semantics were specified; visibility, billing and protection claims were qualified, with both CI triggers counted; the privacy prerequisite was reconciled with D2; and attribution was specified as a standalone `NOTICE` plus a README link.
