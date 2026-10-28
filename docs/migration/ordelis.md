# Ordelis migration: decision record

**Status:** accepted direction, nothing implemented. This document is documentation only. It records what was decided, what was verified, and what must happen before each later change. It does not rename anything.

**Scope of the migration:** the product currently named **Synapse** (a fork of [MCO](https://github.com/mco-org/mco)) becomes **Ordelis** (`ordelis.app`). Brand styling: `ordelis` as the visual wordmark, `Ordelis` in prose. This record covers the repository, desktop app, build metadata, ownership and release hygiene, and the planned website. It deliberately does not rename inherited MCO interfaces.

Contents: [Accepted decisions](#accepted-decisions) · [Safety invariants](#safety-invariants) · [Provenance](#provenance-and-attribution) · [Identifier matrix](#identifier-migration-matrix) · [Platform assumptions](#platform-assumptions) · [Visibility](#repository-visibility-evaluation) · [Rename runbook](#repository-rename-runbook-prepared-not-executed) · [Prerequisites](#outstanding-prerequisites) · [Risk register](#risk-register) · [PR roadmap](#pr-roadmap)

## Accepted decisions

| # | Decision |
| --- | --- |
| D1 | **Structure.** Keep `desktop/` and `runtime/` in place and add `web/` beside them. No `apps/`, no Turborepo or Nx, no root pnpm workspace for structural symmetry. Lockfiles stay independent: the root is an npm package, `desktop/` has its own `pnpm-lock.yaml`, and `web/` will have its own. |
| D2 | **Single monorepo.** Temporarily making the repository private during development is being evaluated (see [Repository visibility](#repository-visibility-evaluation)). Until a visibility change is separately authorized and executed, private website exports and other private design assets are not committed to this repository, including on pull-request branches. |
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
3. A `NOTICE`-style attribution section lists the upstream project, its license and the fork base commit below.
4. Git history is never rewritten, so the upstream authors' commits remain intact.
5. Historical documents (`CHANGELOG.md`, `docs/releases/`, `docs/archive/`, `docs/implementation/` decision records, `README.zh-CN.md`) are not blindly renamed. Where the old name would mislead, add a short "formerly Synapse" note and keep the text.
6. GitHub does not show this repository as a fork (`isFork: false`), so the fork relationship cannot be preserved on GitHub itself. Provenance is carried by the history and by requirements 1 to 3.

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
| "Synapse" in UI copy, window title, `desktop/index.html`, READMEs, `productName` | `desktop/src`, `desktop/src-tauri/tauri.conf.json`, `README.md`, `desktop/README.md` | **Rename** | N1 |
| CLI help text `Absolute external Synapse JSON config`; banner `=== Synapse: authorize ONE controlled Codex probe ===` | `runtime/cli.py`, `runtime/probe_confirmation.py` | **Rename the text only**; update the tests that assert it. The banner is a human authorization prompt, so review it carefully. | N1 |
| GitHub repo description, topics, social preview | Repository settings | **Update** after the rename | R2 |

### Compatibility-sensitive

| Identifier | Where | Decision | PR |
| --- | --- | --- | --- |
| Bundle identifier `dev.raioviajante.synapse` | `tauri.conf.json`; determines the app data directory holding `projects.json` | **Migrate** to `app.ordelis.desktop`, with a validated one-shot import of the registry. Fallback if the import is not approved: document that projects are re-added through the native folder picker. | I2 |
| Cargo package `synapse-desktop`, library `synapse_desktop_lib`, npm package `synapse-desktop`, executable `Contents/MacOS/synapse-desktop` | `desktop/src-tauri/Cargo.toml`, `main.rs`, `desktop/package.json`, `Cargo.lock` | **Rename** to `ordelis-desktop`, atomically with the scripts that hard-code the old names: `verify-release-isolation.sh`, `test-verify-release-isolation.sh`, `verify-native-boundary.mjs`, and the `pgrep -x synapse-desktop` lines in `docs/implementation/desktop-visual-verification.md`. These are security checks and must still pass, including their negative tests. | I1 |
| `SYNAPSE_DEV_PYTHON`, `SYNAPSE_DEV_CHECKOUT` | `desktop/src-tauri/build.rs`, `runtime_bridge/process.rs`, release-isolation "needles" | **Rename** to `ORDELIS_*`. Compile-time and dev-only. The environment-scrub tests keep asserting that **both** prefixes do not leak. | I1 |
| `SYNAPSE_PROBE_AUTHORIZED`, `SYNAPSE_PROBE_ID`, `SYNAPSE_CONFIRM`, `SYNAPSE_TOKEN`, `SYNAPSE_UNLISTED`, `SYNAPSE_CANARY`, `SYNAPSE_PROJECTS_TEST_CHILD` | `tests/`, `desktop/src-tauri/src/projects/registry/tests.rs` | **Audit first.** They appear in tests only and, from their names, act as environment canaries or negative cases. Their exact purpose has not been read in detail. Add `ORDELIS_` twins; do not simply replace. | I1 |
| `synapse_observed`, `synapse_enforced` | `runtime/execution_safety_gate.py` (`ORIGINS`, `_TRUSTED_ORIGINS`) | **Preserve.** Trust-origin vocabulary of the safety gate. Renaming silently changes trust classification and may break stored evidence. If a rename is ever wanted, it is a dedicated security-reviewed change that accepts both spellings on read. | none |
| `runtime/synapse_plan.py` and its imports (`cli.py`, `codex_probe_runner.py`, `execution_safety_gate.py`, `probe_authorization_ledger.py`, `runtime_validation.py`; six test files) | `runtime/` | **Preserve.** Internal module, named in the import-boundary allowlist in `tests/test_runtime_status_bridge.py`. A later rename could ship a re-export shim. | none |
| Probe scratch prefix `synapse-codex-probe-` | `runtime/codex_probe_runner.py` | **Audit first.** Check for any cleanup glob keyed on the prefix before renaming, so old scratch directories are not orphaned. | I1 |
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
| `.github/CODEOWNERS` | Every path is owned by `@tt-a1i`, the upstream author. | **Update** to the Ordelis maintainer. Keep the upstream author credited in attribution, not as review owner. | H2 |
| `.github/ISSUE_TEMPLATE/config.yml` | The security-disclosure link points at `tt-a1i/mco/security/advisories/new`. Vulnerability reports about this project would reach upstream. | **Update** to this repository's private advisory form (after the rename), and enable private vulnerability reporting. | H2 |

## Platform assumptions

Documented here so new development stays portable. **No platform other than macOS has been built or verified.** CI builds the desktop app on `macos-latest` only.

### macOS-only

- `rfd` (native folder panel) is a dependency only under `cfg(target_os = "macos")` in `desktop/src-tauri/Cargo.toml`. The `add_project` folder panel code in `desktop/src-tauri/src/projects.rs` is likewise gated to macOS, so no other platform has a folder picker.
- Desktop bundle targets are `app` and `dmg`. Release-isolation checks inspect `Info.plist` and `Contents/MacOS/`.
- The current UI copy and shortcuts assume macOS conventions in places. The design exports show macOS window chrome and `⌘` shortcuts. Windows and Linux need `Ctrl` mappings, native window controls and neutral copy such as "Stored on this Mac".

### Unix assumptions in the desktop Rust code

- `desktop/src-tauri/src/projects/folder.rs` and `registry.rs` import `std::os::unix` without `cfg` gating. They use `libc` (`getpwuid_r`, `geteuid`, `flock`, `O_NOFOLLOW`, `O_DIRECTORY`), Unix file modes (`0o600`, `0o700`) and ownership checks. `libc` is an unconditional dependency.
- The registry's safety properties (refusing untrusted or foreign-owned directories, no symlink following, advisory locking) are implemented with these Unix primitives. A Windows port needs equivalent protections designed and reviewed, not a stub.
- Consequence: the desktop crate is **expected not to compile on Windows**. This has not been tried.
- The registry's Rust tests (`desktop/src-tauri/src/projects/registry/tests.rs`, `lib.rs`) use `mkfifo` and `PermissionsExt`, which are Unix-only.

### Python runtime

- Already cross-platform in part: `runtime/platform.py` shims `os.getuid`, `os.killpg` and `.cmd` spawning for Windows, and `gate.yml` runs a Windows installer smoke job (`tests.test_platform_helpers`, `tests.test_acp_transport`, packaging and CLI smoke).
- Not portable: the session daemon and client (`runtime/session/daemon.py`, `client.py`) use `socket.AF_UNIX`. `runtime/probe_authorization_ledger.py` calls `os.geteuid()`, opens files relative to a directory descriptor (`dir_fd`) and enforces `0o700`/`0o600` modes, none of which has the same meaning on Windows (`os.geteuid` does not exist there). `runtime/vault_context.py` opens files with `os.O_NOFOLLOW`, which Windows does not define, and `runtime/codex_probe_runner.py` checks `0o700` scratch directories. Their Windows behavior is unverified.
- Process-group termination falls back to `terminate`/`kill` on Windows, with weaker guarantees than `killpg`.

## Repository visibility evaluation

**Nothing is changed by this document. Visibility changes need a separate, explicit authorization.**

### Current state (read-only queries)

Public; `isFork: false`; 0 stars, 0 watchers, 0 forks; one collaborator (the owner); 5 open issues, no open pull requests; no tags or releases; no GitHub Pages, wiki, discussions, webhooks, Actions secrets or environments; no branch protection or rulesets; license MIT; no topics. The account plan was not readable with the available token scope.

### Consequences of public to private

Sources: [GitHub: setting repository visibility](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/managing-repository-settings/setting-repository-visibility) and [GitHub billing for Actions](https://docs.github.com/en/billing/managing-billing-for-your-products/managing-billing-for-github-actions/about-billing-for-github-actions).

| Area | Consequence | Impact here |
| --- | --- | --- |
| Public access | Code, issues and pull requests are no longer visible to non-collaborators. Existing links return not-found for them. Anyone with an existing clone keeps that copy. | Low. No known downstream consumers: nothing is published to npm or any registry from this repository. |
| Stars and watchers | Permanently erased. | None (0 and 0). |
| Forks | Public forks are detached into a separate network and stay public. | None (0 forks). |
| Contributors | Only invited collaborators can read or push. | None today (owner only). |
| GitHub Pages | A published site is unpublished on Free plans. | None (no Pages). |
| Actions cost | Standard runners are free for public repositories. For private ones usage is billed against the plan's included allowance (GitHub Free: $12 per month of included compute, per the billing page). Per-minute rates are $0.006 Linux, $0.010 Windows, $0.062 macOS, so macOS minutes cost about 10 times Linux. | **Material.** The macOS `Desktop` job took about 14 minutes in the latest run on `main`, roughly $0.87 per run, so about 13 runs would use the Free allowance. Docs-only changes do not trigger `Desktop` (path filter on `desktop/**`) but do trigger `Gate`. The plan is unverified, so this is an estimate. |
| Protection features | Protected branches, required reviews and code-owner enforcement are listed under paid plans for private repositories. On a Free personal plan they appear unavailable. This is inferred from the plans page, not confirmed. | Nothing is configured today, so nothing is lost, but the "independent review before merge" rule (D10) could not be technically enforced on a private Free repository. It would be a process rule only. |
| Security features | Dependabot alert rules and code scanning need GitHub Code Security on private repositories. Dependency graph and alerts stay on. | Low. Nothing relies on them today. |
| Webhooks, integrations | None configured. | None. |
| Upstream attribution | Unaffected: the history and license are unchanged. | None. |
| Going public later | Code and Actions history and logs become visible to everyone; anyone can fork; stars and watchers reset; push rulesets are disabled. | **Pre-publication review is required**, including history, issue text, Actions logs and the contents of any private material committed while private. Nothing is committed that the maintainer would not want public. |

### Recommendation

The costs are small because the repository has no audience yet. The real trade-off is CI cost and loss of branch protection against the benefit of developing the private website and desktop design in one repository.

Before authorizing, the maintainer should confirm: the GitHub plan; whether private-repository branch protection is needed; and the plan for a pre-publication history audit. If the change is authorized, make the repository private **before** the first private asset or private-derived implementation is committed, and perform the rename in the same maintenance window or immediately after.

Until then, the roadmap blocks `web/` and the desktop redesign (anything derived from the private designs), and allows only changes that contain no private material.

## Repository rename runbook (prepared, not executed)

Destination `RaioViajante/ordelis`. Reference: [GitHub: renaming a repository](https://docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository).

**Verified facts:** GitHub redirects the web URL, issues, stars and followers and keeps `git clone`, `fetch` and `push` working against the old location. It does **not** redirect GitHub Pages project URLs (none here) or actions hosted in the renamed repository (this repository hosts none, and no workflow references `RaioViajante/synapse`). Creating a new repository with the old name breaks the redirect.

**Preconditions**

1. Explicit maintainer authorization for the rename.
2. H1 and H2 merged, so release workflow, ownership and security-advisory routing are already correct.
3. No open pull request, so branch references are not in flight. The repository has none today.
4. The visibility decision is executed or deliberately deferred.

**Steps (maintainer or authorized agent)**

1. Settings, General, repository name: `ordelis`.
2. In each local clone: `git remote set-url origin github-personal:RaioViajante/ordelis.git`, then `git fetch`.
3. Update the repository description and topics. Keep "forked from MCO" or equivalent attribution in the description.
4. Update links in **living** documents only (`README.md`, `desktop/README.md`, `docs/README.md`).
5. **Never create a repository named `synapse`** under this account.

**Verification:** `gh repo view` reports the new name; the old URL redirects; `git ls-remote` works through both URLs; the issue and pull-request counts and the commit count (269 at the time of this record) match before and after; `Gate` runs and passes on the next push.

**Rollback:** rename back in Settings. Redirects follow the rename in either direction, but any links created under the new name need updating. No data is deleted by a rename.

## Outstanding prerequisites

These are recorded, **not** assumed complete.

| Prerequisite | Needed by | Status |
| --- | --- | --- |
| Visibility decision executed, or an alternative for private work agreed | W1, U1 and later (anything derived from private designs) | **Open.** Requires separate authorization. |
| Approved vector master of the Ordelis symbol | Icons, favicon, installers, social images (U-series, P-series) | **Open.** The provisional reconstruction is not used for final assets. |
| GitHub plan and whether private-repository branch protection is required | Visibility decision | **Open** |
| Final consistency check of the canonical design against the approved design: cross-platform language, shared layout grid, responsive layouts, latest storytelling | W1 | **Open** |
| Confirm which foundations and components board is authoritative for the desktop app (the desktop export's or the web export's system board) | U1 | **Open** |
| Updates and telemetry statement for the website privacy page | W-series | **Open.** The app currently sends nothing and its CSP allows only IPC. |
| Final product-name check before the first Windows installer (the WiX upgrade code follows `productName`) | P-series | **Open** |
| Plan for a pre-publication audit if the repository is later made public again | Open-source publication | **Open** |

## Risk register

| # | Risk | Likelihood | Impact | Mitigation | Owner PR |
| --- | --- | --- | --- | --- | --- |
| 1 | Private design or website material lands in a public repository, including on a PR branch | Medium | High | Do not commit exports; execute the visibility decision first; review each diff for private material | W1, U-series |
| 2 | App data orphaned by an identifier change | Medium | Medium | Validated one-shot import through the existing registry loader; documented re-add fallback; copy, never move | I2 |
| 3 | Safety-gate vocabulary or verification scripts renamed carelessly | Low | High | Trust-origin values preserved; verification scripts updated atomically with their negative tests | I1 |
| 4 | Accidental npm publish under the upstream scope | Low today, grows if any `NPM_TOKEN` is added | High | Hard guard on `publish-npm.yml` | H1 |
| 5 | Security reports reach upstream | Medium | Medium | Update the advisory link and enable private reporting | H2 |
| 6 | Attribution erosion | Low | High | Provenance requirements above; `LICENSE` untouched | B1, N1 |
| 7 | Private repository CI costs and loss of branch protection | Medium | Medium | Confirm the plan first; keep path filters; review is a process rule | R1 |
| 8 | Windows or Linux claimed before verification | Medium | High | No support claims; download surfaces stay "not yet available" | P-series |
| 9 | Windows registry port weakens the Unix-hardened guarantees | Medium | High | Separate reviewed design; no stub implementation | P-series |
| 10 | Redesign copies prototype HTML that violates the CSP (remote fonts, inline styles) | Medium | Medium | Rebuild components in React and Tailwind; keep `font-src 'self'`; CSP checks stay in CI | U-series |
| 11 | Security-sensitive screens (approve, run, merge, discard, terminal) look functional | Medium | High | Inert, labelled fixtures only; no new IPC commands; execution stays disabled | U-series |
| 12 | Flaky tests (issue #44) mask rename regressions | Low | Medium | Triage reruns explicitly; do not hide failures | all |
| 13 | A rename PR is large and unreviewable | Medium | Medium | One concern per PR as listed below | all |
| 14 | A new repository is created under the old name | Low | Medium | Runbook rule 5 | R2 |
| 15 | Provisional icon art leaks into releases | Low | Medium | Block icon and installer work on the approved vector master | U-series, P-series |

## PR roadmap

One concern per pull request. Every PR carries: acceptance criteria, local verification, CI evidence, an independent review before merge, and a rollback note. None changes the safety gate's behavior. Order follows the maintainer's priorities; the only dependency adjustment is that web and desktop-design work wait on the visibility decision.

| ID | Scope | Acceptance criteria | Verification | Rollback | Depends on |
| --- | --- | --- | --- | --- | --- |
| **B1** | This record. Docs only. | Adds `docs/migration/ordelis.md` and one index link. No code, workflow or asset changes. No private material. | Diff touches only `docs/`; links resolve; `Gate` green | Revert the commit | none |
| **H1** | Gate or disable `publish-npm.yml`; review `preview-package.yml` | The workflow cannot run for this repository, with a comment explaining why. The upstream package name is untouched. `Gate` still green. | Inspect with a dry trigger; no publish step reachable; `Gate` green | Revert | B1 |
| **H2** | `CODEOWNERS` to the Ordelis maintainer; security-advisory link; private vulnerability reporting | Upstream author credited in attribution, not as owner. Advisory link targets this repository. | Review file diffs; confirm the advisory URL after R2 | Revert | B1; advisory link finalized after R2 |
| **R1** | **Maintainer action.** Visibility change, if authorized | Consequences re-confirmed at execution time; plan known | `gh repo view` shows visibility; CI still runs | Make public again (note the consequences above) | Separate authorization |
| **R2** | **Maintainer action.** Rename to `RaioViajante/ordelis` per the runbook | Runbook verification passes | Runbook verification | Rename back | H1, H2, authorization |
| **N1** | User-facing branding: UI strings, window title, `productName`, READMEs, CLI help text, probe banner, attribution section | No "Synapse" on user-facing surfaces except historical text. Identifiers in the matrix untouched. Provenance statement present. | `pnpm lint && pnpm typecheck && pnpm test`; `cargo test --locked`; Python `unittest discover`; `Gate` and `Desktop` green; grep audit | Revert | R2 |
| **I1** | Atomic rename of the Cargo and npm packages, executable, `ORDELIS_*` build variables and the scripts that hard-code them | `verify-release-isolation` and `verify-native-boundary` pass on a real build, and their negative tests pass. Environment-scrub tests cover both prefixes. | `test-verify-*.sh`; `pnpm tauri build --bundles app` followed by `verify-release-isolation.sh` | Revert | N1 |
| **I2** | Bundle identifier `app.ordelis.desktop` plus validated registry import, or the documented fallback | An old-directory fixture imports; a hostile or foreign-owned directory is rejected; the old directory is left untouched; identity checks still pass | New registry tests; `cargo test --locked`; manual launch with a pre-existing registry | Revert; the old data directory is untouched | I1 |
| **W1** | `web/` scaffold: Next.js with static export, TypeScript, own pnpm lockfile and path-filtered CI | Builds independently of `desktop/`; no private exports committed; no deployment | Build in CI; `Desktop` unaffected | Remove `web/` | Visibility decision; design consistency check |
| **W2 onward** | Website increments: layout grid, hero, motion, supporting pages, accessibility and reduced-motion checks | Responsive at the documented breakpoints; reduced-motion honoured; placeholders left visibly unfilled; no platform or download claims; no deploy | Ported browser checks; manual keyboard pass | Revert | W1 |
| **U1** | Desktop design system: tokens, light and dark themes, system-native font stack, JetBrains Mono for code | CSP unchanged; no remote fonts; contrast checks pass in both themes; no Apple font files; unused bundled fonts and their license text reconciled | Tests; contrast check; `Desktop` green | Revert | Visibility decision; N1 |
| **U2 onward** | Incremental desktop UI: shell and navigation, then screens as labelled fixtures | Existing tests pass; no new IPC commands; sample-data labelling intact; execution stays disabled | Tests; visual verification per `desktop-visual-verification.md` | Revert per PR | U1 |
| **U-icons** | Application and installer icons | Generated only from the approved vector master | Icon pipeline output inspected | Revert | Approved master |
| **P1 onward** | Cross-platform: gate Unix-only modules, a reviewed Windows registry design, Linux and Windows CI jobs, packaging, signing and release metadata | A platform is documented as verified only after its build, packaging and security behavior pass in CI and by hand. Download surfaces stay unavailable until real metadata exists. | Per-platform CI and manual checks | Revert per PR | I2; relates to issue #38 |

Existing open issues that interact with this plan: #38 (release packaging assurance, anchors P1), #44 (flaky ledger consumers), #39 (runtime import hygiene), #23 (session readiness race).

## Explicitly out of scope

Application changes; renaming any inherited MCO interface; enabling agent execution; deploying the website or connecting `ordelis.app`; publishing any release; telemetry or accounts; committing private design exports; creating a new organization.
